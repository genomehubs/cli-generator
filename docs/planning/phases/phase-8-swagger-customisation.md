# Phase 8: Per-Site Swagger Customisation

**Depends on:** Phase 0 (all endpoints exist and have OpenAPI specs)
**Blocks:** nothing downstream
**Estimated scope:** ~3 files modified, 1 new file per generated site, minimal new code

---

## Goal

Allow each generated site (GoaT, BoaT, etc.) to ship request/response examples
in the Swagger UI, so that the auto-generated docs are immediately useful for
exploration rather than showing only generic types.

The approach:

1. Generator reads a per-site `examples.yaml` (or auto-generates one from field metadata)
2. Generator embeds the examples into the generated project as `docs/api_examples.yaml`
3. The API server reads `docs/api_examples.yaml` at startup and merges examples
   into the utoipa OpenAPI spec under `components.examples` and per-path `examples` refs

---

## Files to Create

| Location                                     | File                                   | Purpose                            |
| -------------------------------------------- | -------------------------------------- | ---------------------------------- |
| `sites/{site}/`                              | `examples.yaml`                        | Hand-authored examples per site    |
| `docs/api_examples.yaml` (generated project) | Auto-generated or copied from site dir | Runtime input to server            |
| `crates/genomehubs-api/src/`                 | `api_examples.rs`                      | Load and merge examples at startup |

## Files to Modify

| File                                | Change                                                 |
| ----------------------------------- | ------------------------------------------------------ |
| `src/commands/new.rs`               | Add `generate_api_examples()` step in codegen pipeline |
| `crates/genomehubs-api/src/main.rs` | Load examples file and merge into OpenAPI spec         |

---

## `examples.yaml` Format

One entry per endpoint, with named examples matching the utoipa path operations.

```yaml
# sites/goat/examples.yaml
search:
  mammal_tree_search:
    summary: Search Mammalia (tree filter)
    description: Returns all taxa within Mammalia using a taxonomic tree filter.
    request:
      query_yaml: |
        index: taxon
        taxa:
          - Mammalia
        taxon_filter_type: tree
      params_yaml: |
        size: 10
        taxonomy: ncbi
    response:
      status:
        success: true
        hits: 6495
        took: 12
      results:
        - taxon_id: "9606"
          scientific_name: Homo sapiens
          taxon_rank: species

count:
  genome_size_count:
    summary: Count taxa with genome size data
    request:
      query_yaml: |
        index: taxon
        attributes:
          - name: genome_size
            operator: exists
      params_yaml: |
        taxonomy: ncbi

report:
  genome_size_histogram:
    summary: Genome size distribution histogram
    request:
      query_yaml: |
        index: taxon
        taxa:
          - Eukaryota
        taxon_filter_type: tree
      params_yaml: "taxonomy: ncbi\n"
      report_yaml: |
        report: histogram
        x: genome_size
        x_opts: ";;20;log10"
```

---

## `api_examples.rs`

```rust
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

/// A single named example for an endpoint.
#[derive(Debug, Deserialize, Serialize)]
pub struct EndpointExample {
    pub summary: Option<String>,
    pub description: Option<String>,
    pub request: Value,
    pub response: Option<Value>,
}

/// All examples for the API, keyed by endpoint name then example name.
pub type ApiExamples = HashMap<String, HashMap<String, EndpointExample>>;

/// Load API examples from a YAML file.
///
/// Returns an empty map if the file does not exist; logs a warning if present
/// but unparseable.
pub fn load_examples(path: &std::path::Path) -> ApiExamples {
    if !path.exists() {
        return HashMap::new();
    }
    match std::fs::read_to_string(path) {
        Ok(content) => serde_yaml::from_str(&content).unwrap_or_else(|e| {
            tracing::warn!("Failed to parse api_examples.yaml: {e}");
            HashMap::new()
        }),
        Err(e) => {
            tracing::warn!("Failed to read api_examples.yaml: {e}");
            HashMap::new()
        }
    }
}

/// Inject examples from `api_examples.yaml` into the serialised OpenAPI spec.
///
/// Modifies the OpenAPI JSON in-place:
/// - Adds each example under `components.examples.{endpoint}_{example_name}`
/// - Adds an `examples` ref map to the matching path's `requestBody.content`
///
/// The `openapi_json` argument is the string written by utoipa to `target/openapi.json`.
/// Returns the modified JSON string.
pub fn merge_examples_into_spec(openapi_json: &str, examples: &ApiExamples) -> Result<String, String> {
    let mut spec: Value = serde_json::from_str(openapi_json)
        .map_err(|e| format!("invalid OpenAPI JSON: {e}"))?;

    // Ensure components.examples exists
    let components_examples = spec
        .pointer_mut("/components/examples")
        .and_then(|v| v.as_object_mut());

    if components_examples.is_none() {
        // Create the path if absent
        spec["components"]["examples"] = json!({});
    }

    for (endpoint, named_examples) in examples {
        for (example_name, example) in named_examples {
            let component_key = format!("{endpoint}_{example_name}");

            // Build OpenAPI Example Object
            let mut example_obj = json!({
                "value": example.request
            });
            if let Some(summary) = &example.summary {
                example_obj["summary"] = Value::String(summary.clone());
            }
            if let Some(desc) = &example.description {
                example_obj["description"] = Value::String(desc.clone());
            }

            // Add to components.examples
            if let Some(obj) = spec["components"]["examples"].as_object_mut() {
                obj.insert(component_key.clone(), example_obj);
            }

            // Add ref to matching path requestBody
            let path_key = format!("/api/v3/{endpoint}");
            if let Some(path_item) = spec.pointer_mut(&format!("/paths/{}", path_key.replace('/', "~1"))) {
                if let Some(request_body) = path_item.pointer_mut("/post/requestBody/content/application~1json/examples") {
                    if let Some(obj) = request_body.as_object_mut() {
                        obj.insert(example_name.clone(), json!({
                            "$ref": format!("#/components/examples/{component_key}")
                        }));
                    }
                }
            }
        }
    }

    serde_json::to_string_pretty(&spec).map_err(|e| format!("serialisation error: {e}"))
}
```

---

## `main.rs` changes

After the server starts and the OpenAPI spec is written to `target/openapi.json`,
load and merge examples:

```rust
// In the startup sequence, after writing openapi.json:
let examples_path = std::path::Path::new("docs/api_examples.yaml");
let examples = api_examples::load_examples(examples_path);

if !examples.is_empty() {
    let spec_path = std::path::Path::new("target/openapi.json");
    if let Ok(spec_json) = std::fs::read_to_string(spec_path) {
        match api_examples::merge_examples_into_spec(&spec_json, &examples) {
            Ok(merged) => {
                if let Err(e) = std::fs::write(spec_path, &merged) {
                    tracing::warn!("Failed to write merged OpenAPI spec: {e}");
                }
            }
            Err(e) => tracing::warn!("Failed to merge API examples: {e}"),
        }
    }
}
```

Add `mod api_examples;` to `main.rs`.

---

## Generator changes in `src/commands/new.rs`

Add a `generate_api_examples()` step after the main codegen:

```rust
/// Copy or generate `docs/api_examples.yaml` for the new project.
///
/// If `sites/{site}/examples.yaml` exists, copy it verbatim.
/// Otherwise, generate a minimal stub from field metadata with one example per endpoint.
fn generate_api_examples(
    site_name: &str,
    workdir: &std::path::Path,
    field_metadata: &serde_json::Value,
) -> Result<(), String> {
    let site_examples = std::path::Path::new("sites").join(site_name).join("examples.yaml");

    let content = if site_examples.exists() {
        std::fs::read_to_string(&site_examples)
            .map_err(|e| format!("failed to read site examples: {e}"))?
    } else {
        generate_stub_examples(site_name, field_metadata)
    };

    let docs_dir = workdir.join("docs");
    std::fs::create_dir_all(&docs_dir)
        .map_err(|e| format!("failed to create docs dir: {e}"))?;
    std::fs::write(docs_dir.join("api_examples.yaml"), content)
        .map_err(|e| format!("failed to write api_examples.yaml: {e}"))?;

    Ok(())
}

/// Generate a minimal stub `examples.yaml` from field metadata.
///
/// Selects one numeric field and one keyword field for the histogram example.
/// Produces a valid but sparsely populated examples file as a starting point
/// for hand-editing.
fn generate_stub_examples(site_name: &str, field_metadata: &serde_json::Value) -> String {
    // Find first numeric field for histogram x
    let numeric_field = field_metadata
        .as_object()
        .and_then(|m| {
            m.iter()
                .find(|(_, v)| v.get("type").and_then(|t| t.as_str()) == Some("float"))
                .map(|(k, _)| k.clone())
        })
        .unwrap_or_else(|| "genome_size".to_string());

    format!(
        r#"# Auto-generated examples for {site_name}
# Edit this file to add meaningful examples for your site.

search:
  basic_search:
    summary: Basic taxon search
    request:
      query_yaml: "index: taxon\n"
      params_yaml: "size: 10\ntaxonomy: ncbi\n"

count:
  basic_count:
    summary: Count all taxa
    request:
      query_yaml: "index: taxon\n"
      params_yaml: "taxonomy: ncbi\n"

report:
  {numeric_field}_histogram:
    summary: "{numeric_field} distribution"
    request:
      query_yaml: "index: taxon\n"
      params_yaml: "taxonomy: ncbi\n"
      report_yaml: "report: histogram\nx: {numeric_field}\nx_opts: \";;20;log10\"\n"
"#,
        site_name = site_name,
        numeric_field = numeric_field
    )
}
```

---

## Verification

```bash
# Generator creates the examples file
cargo run -- new --site goat --out /tmp/goat-test
ls /tmp/goat-test/docs/api_examples.yaml

# API server merges examples into spec
cd /tmp/goat-test
cargo run &
sleep 2
curl -s http://localhost:3000/api-docs/openapi.json \
  | jq '.components.examples | keys'
# → should include "search_mammal_tree_search" etc.

# Examples appear in Swagger UI
open http://localhost:3000/swagger-ui/
```

---

## Completion Checklist

- [ ] `api_examples.rs` created in `crates/genomehubs-api/src/`
- [ ] `load_examples()` handles missing/malformed file gracefully
- [ ] `merge_examples_into_spec()` correctly updates `components.examples` and path refs
- [ ] `main.rs` calls load+merge after writing `target/openapi.json`
- [ ] `generate_api_examples()` added to `src/commands/new.rs` codegen pipeline
- [ ] `generate_stub_examples()` produces valid YAML with at least 3 endpoint stubs
- [ ] `sites/goat/examples.yaml` created with at least 3 named examples
- [ ] Generator test: `cargo run -- new --site goat` creates `docs/api_examples.yaml`
- [ ] API startup test: examples appear under `components.examples` in the served spec
- [ ] Swagger UI shows example request bodies in the "Try it out" panel
