# Phase 9: URL Query String Support

**Depends on:** Phases 1–2 (all query routes must exist first)
**Blocks:** nothing downstream
**Estimated scope:** 1 new file, ~1 modification per query route, no SDK changes

This phase is server-side only. The SDK always uses JSON body. URL query string
support is for:

- Browser bookmarkability of API queries
- UI link generation (GoaT front-end → API)
- Backward-compatible URL alias support

---

## Goal

Allow all query routes (`/search`, `/count`, `/record`, `/lookup`, `/summary`)
to accept request parameters as URL query strings in addition to (or instead of)
JSON request body.

**Priority:** JSON body always takes precedence. If both are present, JSON body wins.

URL query string support also handles UI key aliases (e.g. `tax_rank(X)` → `rank: X` in YAML),
enabling the GoaT UI to generate direct API links.

---

## Files to Create

| File                                      | Purpose                           |
| ----------------------------------------- | --------------------------------- |
| `crates/genomehubs-api/src/qs_adapter.rs` | Parse URL query strings into YAML |

## Files to Modify

| File                                          | Change                                             |
| --------------------------------------------- | -------------------------------------------------- |
| `crates/genomehubs-api/src/main.rs`           | `mod qs_adapter;`                                  |
| `crates/genomehubs-api/src/routes/search.rs`  | Accept `Query<HashMap<String,String>>` as fallback |
| `crates/genomehubs-api/src/routes/count.rs`   | Same                                               |
| `crates/genomehubs-api/src/routes/record.rs`  | Already GET; no body; all params from QS           |
| `crates/genomehubs-api/src/routes/lookup.rs`  | Already GET; no body; all params from QS           |
| `crates/genomehubs-api/src/routes/summary.rs` | Already GET; no body; all params from QS           |

---

## `crates/genomehubs-api/src/qs_adapter.rs`

```rust
//! Adapter to convert URL query string parameters into YAML for query routes.
//!
//! The JSON body format is the canonical input to all query routes. This module
//! provides a fallback: when no JSON body is present, URL query parameters are
//! parsed and converted to `query_yaml` / `params_yaml` strings.
//!
//! ## UI key aliases
//!
//! The GoaT UI uses compact key names that differ from the internal YAML keys.
//! This module normalises them:
//!
//! | UI key | YAML key |
//! |---|---|
//! | `tax_rank(X)` | `rank: X` |
//! | `result` | index type (passed as top-level context) |
//! | `query` | combined into `attributes` |
//! | `fields` | `fields` (comma → list) |
//! | `names` | `names` (comma → list) |
//! | `ranks` | `ranks` (comma → list) |
//! | `size` | `size` |
//! | `offset` | → `page` (offset / size + 1) |
//! | `sortBy` | `sort_by` |
//! | `sortOrder` | `sort_order` |
//! | `taxonomy` | `taxonomy` |
//! | `includeEstimates` | `include_estimates` |

use std::collections::HashMap;

/// Convert URL query parameters into `(query_yaml, params_yaml)` strings.
///
/// This is a best-effort conversion. Parameters that cannot be mapped are silently
/// dropped. The returned YAML strings are valid inputs to `SearchQuery::from_yaml`
/// and `QueryParams::from_yaml`.
///
/// Returns an error if the `result` (index type) is missing and no default is provided.
pub fn qs_to_yaml(
    params: &HashMap<String, String>,
    default_result: &str,
) -> Result<(String, String), String> {
    let query_yaml = qs_to_query_yaml(params, default_result)?;
    let params_yaml = qs_to_params_yaml(params);
    Ok((query_yaml, params_yaml))
}

/// Convert URL query parameters to a `SearchQuery` YAML string.
pub fn qs_to_query_yaml(
    params: &HashMap<String, String>,
    default_result: &str,
) -> Result<String, String> {
    let index = params.get("result").map(|s| s.as_str()).unwrap_or(default_result);

    let mut doc = serde_yaml::Mapping::new();
    doc.insert("index".into(), index.into());

    // Taxa filter
    if let Some(taxa_str) = params.get("taxa").or_else(|| params.get("tax_name(eq)")) {
        let taxa: Vec<String> = taxa_str.split(',').map(|t| t.trim().to_string()).collect();
        doc.insert("taxa".into(), serde_yaml::Value::Sequence(
            taxa.into_iter().map(serde_yaml::Value::String).collect()
        ));
    }

    // Taxon filter type
    if let Some(filter_type) = params.get("taxon_filter_type") {
        doc.insert("taxon_filter_type".into(), filter_type.clone().into());
    }

    // Rank filter: UI uses "tax_rank(X)" or plain "rank"
    let rank_val = params.get("rank")
        .or_else(|| params.get("tax_rank(eq)"));
    if let Some(rank) = rank_val {
        doc.insert("rank".into(), rank.clone().into());
    }

    // Attributes: comma-separated "field[op]value" syntax
    if let Some(query_str) = params.get("query") {
        let attributes = parse_query_string_attributes(query_str);
        if !attributes.is_empty() {
            doc.insert("attributes".into(), serde_yaml::to_value(attributes)
                .map_err(|e| format!("attribute serialisation error: {e}"))?);
        }
    }

    // Fields
    if let Some(fields_str) = params.get("fields") {
        let fields: Vec<String> = fields_str.split(',').map(|f| f.trim().to_string()).collect();
        doc.insert("fields".into(), serde_yaml::Value::Sequence(
            fields.into_iter().map(serde_yaml::Value::String).collect()
        ));
    }

    // Names
    if let Some(names_str) = params.get("names") {
        let names: Vec<String> = names_str.split(',').map(|n| n.trim().to_string()).collect();
        doc.insert("names".into(), serde_yaml::Value::Sequence(
            names.into_iter().map(serde_yaml::Value::String).collect()
        ));
    }

    // Ranks (lineage columns)
    if let Some(ranks_str) = params.get("ranks") {
        let ranks: Vec<String> = ranks_str.split(',').map(|r| r.trim().to_string()).collect();
        doc.insert("ranks".into(), serde_yaml::Value::Sequence(
            ranks.into_iter().map(serde_yaml::Value::String).collect()
        ));
    }

    serde_yaml::to_string(&serde_yaml::Value::Mapping(doc))
        .map_err(|e| format!("YAML serialisation error: {e}"))
}

/// Convert URL query parameters to a `QueryParams` YAML string.
pub fn qs_to_params_yaml(params: &HashMap<String, String>) -> String {
    let mut doc = serde_yaml::Mapping::new();

    if let Some(size) = params.get("size").and_then(|s| s.parse::<usize>().ok()) {
        doc.insert("size".into(), size.into());
    }

    // Convert offset to page number
    if let Some(offset) = params.get("offset").and_then(|s| s.parse::<usize>().ok()) {
        let size = params.get("size").and_then(|s| s.parse::<usize>().ok()).unwrap_or(10);
        let page = offset / size + 1;
        doc.insert("page".into(), page.into());
    } else if let Some(page) = params.get("page").and_then(|s| s.parse::<usize>().ok()) {
        doc.insert("page".into(), page.into());
    }

    if let Some(sort_by) = params.get("sortBy").or_else(|| params.get("sort_by")) {
        doc.insert("sort_by".into(), sort_by.clone().into());
    }

    if let Some(sort_order) = params.get("sortOrder").or_else(|| params.get("sort_order")) {
        doc.insert("sort_order".into(), sort_order.clone().into());
    }

    if let Some(taxonomy) = params.get("taxonomy") {
        doc.insert("taxonomy".into(), taxonomy.clone().into());
    }

    if let Some(inc) = params.get("includeEstimates").or_else(|| params.get("include_estimates")) {
        let flag = !matches!(inc.as_str(), "false" | "0" | "no");
        doc.insert("include_estimates".into(), flag.into());
    }

    serde_yaml::to_string(&serde_yaml::Value::Mapping(doc)).unwrap_or_default()
}

/// Parse a compact query string attribute expression into YAML-compatible attribute dicts.
///
/// The query string uses the format `"field[op]value"` with comma-separated terms:
/// `"genome_size[gte]=1000000,assembly_level[eq]=chromosome"`
///
/// Supported operators: `eq`, `ne`, `lt`, `lte`, `gt`, `gte`, `exists`, `missing`.
fn parse_query_string_attributes(query_str: &str) -> Vec<serde_yaml::Value> {
    query_str
        .split(';')  // some UIs use semicolons between filters
        .chain(query_str.split(',').filter(|s| s.contains('[') || s.contains('=')))
        .filter(|s| !s.is_empty())
        .filter_map(|term| parse_single_attribute(term.trim()))
        .collect()
}

fn parse_single_attribute(term: &str) -> Option<serde_yaml::Value> {
    // Format: "field[op]=value" or "field[op]" for exists/missing
    if let Some(bracket_pos) = term.find('[') {
        let field = term[..bracket_pos].trim().to_string();
        let rest = &term[bracket_pos + 1..];
        let close = rest.find(']')?;
        let op = &rest[..close];
        let value_part = rest[close + 1..].trim_start_matches('=');

        let yaml_op = match op {
            "eq" => "eq",
            "ne" => "ne",
            "lt" => "lt",
            "lte" | "le" => "le",
            "gt" => "gt",
            "gte" | "ge" => "ge",
            "exists" => "exists",
            "missing" => "missing",
            _ => return None,
        };

        let mut attr = serde_yaml::Mapping::new();
        attr.insert("name".into(), field.into());
        attr.insert("operator".into(), yaml_op.into());
        if !value_part.is_empty() {
            attr.insert("value".into(), value_part.to_string().into());
        }
        return Some(serde_yaml::Value::Mapping(attr));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn basic_result_sets_index() {
        let p = params(&[("result", "taxon")]);
        let yaml = qs_to_query_yaml(&p, "assembly").unwrap();
        assert!(yaml.contains("index: taxon"), "yaml was: {yaml}");
    }

    #[test]
    fn default_result_used_when_absent() {
        let p = params(&[]);
        let yaml = qs_to_query_yaml(&p, "assembly").unwrap();
        assert!(yaml.contains("index: assembly"), "yaml was: {yaml}");
    }

    #[test]
    fn taxa_parsed_from_comma_list() {
        let p = params(&[("taxa", "Mammalia,Insecta"), ("result", "taxon")]);
        let yaml = qs_to_query_yaml(&p, "taxon").unwrap();
        assert!(yaml.contains("Mammalia"), "yaml was: {yaml}");
        assert!(yaml.contains("Insecta"), "yaml was: {yaml}");
    }

    #[test]
    fn offset_converted_to_page() {
        let p = params(&[("offset", "20"), ("size", "10")]);
        let yaml = qs_to_params_yaml(&p);
        assert!(yaml.contains("page: 3"), "yaml was: {yaml}");
    }

    #[test]
    fn size_passed_through() {
        let p = params(&[("size", "25")]);
        let yaml = qs_to_params_yaml(&p);
        assert!(yaml.contains("size: 25"), "yaml was: {yaml}");
    }

    #[test]
    fn attribute_bracket_syntax_parsed() {
        let attrs = parse_query_string_attributes("genome_size[gte]=1000000");
        assert_eq!(attrs.len(), 1);
        let a = attrs[0].as_mapping().unwrap();
        assert_eq!(a["name"], serde_yaml::Value::String("genome_size".into()));
        assert_eq!(a["operator"], serde_yaml::Value::String("ge".into()));
        assert_eq!(a["value"], serde_yaml::Value::String("1000000".into()));
    }
}
```

---

## Route Changes

For POST routes (`/search`, `/count`), add an `Option<Json<...>>` body parameter and
fall back to QS parsing when no body is present:

```rust
// In routes/search.rs:
pub async fn post_search(
    Query(qs_params): Query<HashMap<String, String>>,
    Extension(state): Extension<Arc<AppState>>,
    body: Option<Json<SearchRequest>>,
) -> Json<SearchResponse> {
    let (query_yaml, params_yaml) = if let Some(Json(req)) = body {
        // JSON body takes priority
        (req.query_yaml, req.params_yaml)
    } else {
        // Fall back to URL query string
        match crate::qs_adapter::qs_to_yaml(&qs_params, &state.default_result) {
            Ok(pair) => pair,
            Err(e) => return Json(SearchResponse {
                status: ApiStatus::error(e),
                results: vec![],
                total: 0,
            }),
        }
    };
    // ... rest of handler unchanged
}
```

For GET routes (`/record`, `/lookup`, `/summary`), the existing `Query<...>` extractor
already handles query strings. The `qs_adapter` is only needed for these if the existing
param names differ from the UI aliases — check during implementation.

---

## Handling `tax_rank(X)` UI alias

The GoaT UI uses `tax_rank(eq)=species` instead of `rank=species`. The adapter handles
this in `qs_to_query_yaml`:

```rust
let rank_val = params.get("rank")
    .or_else(|| params.get("tax_rank(eq)"))
    .or_else(|| {
        // Also check for other rank operators: tax_rank(ne), tax_rank(gt), etc.
        params.keys()
            .find(|k| k.starts_with("tax_rank("))
            .and_then(|k| params.get(k))
    });
```

The full list of UI aliases to support should be extracted from the GoaT front-end
source code. At minimum, support:

| UI key                 | YAML mapping                                 |
| ---------------------- | -------------------------------------------- |
| `tax_rank(eq)`         | `rank: <value>`                              |
| `tax_name(eq)`         | `taxa: [<value>]`, `taxon_filter_type: name` |
| `tax_tree(eq)`         | `taxa: [<value>]`, `taxon_filter_type: tree` |
| `fields`               | `fields: [...]`                              |
| `names`                | `names: [...]`                               |
| `ranks`                | `ranks: [...]`                               |
| `sortBy` / `sortOrder` | `sort_by`, `sort_order`                      |
| `includeEstimates`     | `include_estimates`                          |
| `size` / `offset`      | `size`, `page` (via offset ÷ size + 1)       |

---

## Verification

```bash
cargo test -p genomehubs-api qs_adapter

# QS fallback for /search
curl -s "http://localhost:3000/api/v3/search?result=taxon&taxa=Mammalia&size=5&taxonomy=ncbi" \
  | jq '{hits: .status.hits, first: .results[0].scientific_name}'

# QS with UI alias tax_rank
curl -s "http://localhost:3000/api/v3/search?result=taxon&tax_rank(eq)=species&size=3" \
  | jq '.results[0].taxon_rank'

# JSON body still works (regression)
curl -s -X POST http://localhost:3000/api/v3/search \
  -H 'Content-Type: application/json' \
  -d '{"query_yaml":"index: taxon\ntaxa: [Mammalia]\ntaxon_filter_type: tree\n","params_yaml":"size: 5\ntaxonomy: ncbi\n"}' \
  | jq '.status.hits'
```

---

## Completion Checklist

- [ ] `qs_adapter.rs` created in `crates/genomehubs-api/src/`
- [ ] `qs_to_yaml()` round-trips correctly for basic search params
- [ ] `qs_to_params_yaml()` converts `offset` → `page` correctly
- [ ] `parse_query_string_attributes()` handles `field[op]=value` syntax
- [ ] UI alias `tax_rank(eq)` mapped to `rank` in YAML
- [ ] UI alias `tax_name(eq)` / `tax_tree(eq)` mapped to `taxa` + `taxon_filter_type`
- [ ] `routes/search.rs` accepts optional JSON body with QS fallback
- [ ] `routes/count.rs` same
- [ ] GET routes (`/record`, `/lookup`, `/summary`) checked for alias compatibility
- [ ] `mod qs_adapter` declared in `main.rs`
- [ ] Unit tests pass: `cargo test -p genomehubs-api qs_adapter`
- [ ] QS-only curl smoke tests pass
- [ ] JSON body smoke tests still pass (regression)
