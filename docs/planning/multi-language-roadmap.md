# Multi-Language SDK Support Plan

**Status:** Planned (not yet implemented)
**Date:** 18 March 2026
**Target Languages:** R (Phase 2), JavaScript (Phase 3), Go (Phase 3 optional)

---

## Overview

This document outlines a phased approach to extend the `cli-generator` beyond Rust/Python to support R, JavaScript, Go, and other languages. The architecture is language-agnostic at the core level, making expansion feasible within a single monorepo.

**New:** This plan also integrates **code snippet generation**, enabling any UI (JavaScript or otherwise) to display runnable code examples in multiple languages for a given query structure. SDKs and snippets share the same query representation but serve different purposes: SDKs are for end-user consumption, snippets are for UI exploration.

### Key Principles

- **Core is reusable:** Query building, config system, field fetching, validation rules all live in pure Rust with no language-specific dependencies.
- **Templates + paths vary:** Only Tera templates and output directory structures differ per language.
- **Query structure is universal:** Built by SDKs and consumed by both snippet generation and HTTP APIs.
- **Single version:** All SDKs version-lock to the generator (e.g., goat-cli-generator 0.1.0 generates goat_sdk 0.1.0 for all languages).
- **Monorepo:** Keep everything together. Build complexity is manageable with parallel CI jobs per language.
- **Centrally maintained:** Generated packages are version-controlled and published from this repo.

---

## Snippet Generation Architecture

### Overview

Code **snippets** are read-only, single-language code examples suitable for embedding in a UI (e.g., "here's how to run this query in R"). They differ from SDK-generated packages:

| Aspect      | SDK                                           | Snippet                              |
| ----------- | --------------------------------------------- | ------------------------------------ |
| Purpose     | Full SDK for users to build & execute queries | UI example code: "copy & paste this" |
| Form        | Installable package (PyPI, CRAN, npm)         | String (single file or function)     |
| Deployment  | Published to package managers                 | Served by API endpoint               |
| Built by    | Code generator (Rust)                         | Snippet generator (Rust)             |
| Consumed by | End users in their preferred language         | JavaScript UI, documentation         |

### Query Structure (Universal)

All queries are represented as a **`QuerySnapshot`** struct:

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct QuerySnapshot {
    pub filters: Vec<(String, String, String)>,  // (field, operator, value)
    pub sorts: Vec<(String, String)>,             // (field, direction)
    pub flags: Vec<String>,                        // CLI flags, e.g., ["genome-size"]
    pub selections: Vec<String>,                   // Selected output fields
    pub traversal: Option<(String, String)>,      // (field, direction)
    pub summaries: Vec<(String, String)>,         // (field, modifier)
}
```

This is:

- **Built by:** JavaScript UI (or any SDK) when constructing a query
- **Consumed by:** `SnippetGenerator` to render code snippets

### Hybrid Approach: Backend Snippet Endpoint

The JavaScript UI never instantiates an SDK. Instead:

1. **UI builds query structure** (filters, sorts, etc.) via form interactions
2. **UI sends JSON POST to Express backend:** `POST /api/snippet { site, query, languages }`
3. **Backend calls `SnippetGenerator::render_all_snippets()`**
4. **Backend returns rendered snippets** in all requested languages
5. **UI displays snippets** in tabs, modals, or sidebars

Example flow:

```
User selects filters → UI builds QuerySnapshot → POST /api/snippet
  ↓
Express backend receives JSON
  ↓
cli-generator SnippetGenerator renders Python/R/JS snippets
  ↓
Response: { "python": "import goat_sdk\n...", "r": "library(goat_sdk)\n...", ...}
  ↓
UI displays snippets in syntax-highlighted tabs
```

---

## Architecture Assessment

### Language-Agnostic Core

These components require **no changes** and can serve any language:

- **Query building** (`src/core/query/`): `SearchQuery`, `QueryParams`, URL encoding
- **Field fetching** (`src/core/fetch.rs`): `FieldDef` structs, API caching (24h)
- **Config system** (`src/core/config.rs`): `SiteConfig`, `CliOptionsConfig` (pure data, uses only serde/anyhow/chrono)
- **Validation rules** (emitted into `field_meta.rs.tera`): Operator validation, enum constraints, summary modifiers
- **Template context design** (`CodeGenerator::build_context()`): Generic structure (site metadata, field defs, CLI flags)
- **Query snapshots** (`src/core/snippet.rs`): Universal `QuerySnapshot` struct used by both SDKs and snippet generation

### Language-Specific Components

Only these parts vary per language:

- **SDK template set:** Currently 18 templates split between Rust (11), Python (2), CI (3), shared (2)
- **Snippet templates:** Short, example-code snippets in each language (Python, R, JS). Rendered by `SnippetGenerator`.
- **Output file paths:** E.g., Python → `python/{sdk_name}/`, R → `r/{sdk_name}/`
- **Post-processing:** Python uses `black`/`isort`, R uses `styler`, JS uses `prettier`
- **Compatibility flags:** Rust-only (`goat_cli_compat` for clap aliases)

### Tera Templating Capability

- **Features used:** Loops, filters, conditionals, filters
- **Strengths:** Unidirectional data flow, compile-time safety, outputs any text syntax
- **Limitation:** No runtime variation beyond what's in config
- **Dual use:** Both SDK generation (full packages) and snippet generation (single-language examples) use the same templating system

---

## Phased Implementation Plan

### Phase 1: Infrastructure Foundation

**Goal:** Establish language-agnostic template loading and rendering pipeline.

#### 1.1 Reorganize Templates by Language Family

Create subdirectories in `templates/`:

```
templates/
  rust/
    cli_meta.rs.tera
    main.rs.tera
    generated_mod.rs.tera
    fields.rs.tera
    groups.rs.tera
    cli_flags.rs.tera
    client.rs.tera
    output.rs.tera
    sdk.rs.tera
    lib.rs.tera
    field_meta.rs.tera
    indexes.rs.tera
  python/
    query.py.tera
    site_cli.pyi.tera
  shared/
    GETTING_STARTED.md.tera
    PREVIEW.md.tera
    autoupdate.yml.tera
    ci.yml.tera
  snippets/          # NEW: Code example templates
    python_snippet.tera
    r_snippet.tera              # Phase 2
    javascript_snippet.tera     # Phase 3
  r/                 # Created in Phase 2
    (new templates)
  js/                # Created in Phase 3
    (new templates)
```

#### 1.2 Update `make_tera()` in [src/core/codegen.rs](src/core/codegen.rs#L21)

- Group template registration by language family
- Maintain same `add_raw_template()` calls but organize with comments
- Add language constants for future CLI/config options:
  ```rust
  const SUPPORTED_LANGUAGES: &[&str] = &["rust", "python"];  // Extend later
  ```

#### 1.3 Extend Config Schema — Add `enabled_sdks` to `SiteConfig`

**File:** [src/core/config.rs](src/core/config.rs)

Add optional field to `SiteConfig`:

```rust
/// Which SDK languages to generate (defaults to ["python"] for backward compat).
#[serde(default = "default_enabled_sdks")]
pub enabled_sdks: Vec<String>,

fn default_enabled_sdks() -> Vec<String> {
    vec!["python".to_string()]
}
```

Update YAML config example (e.g., `sites/goat.yaml`):

```yaml
enabled_sdks:
  - python
  - r # Enable when Phase 2 complete
```

#### 1.4 Make `template_name_to_dest()` Language-Aware

**File:** [src/core/codegen.rs](src/core/codegen.rs#L436)

Refactor from hardcoded matching to language-parameterized:

```rust
/// Map a template name to its destination path, per language.
fn template_name_to_dest(
    template_name: &str,
    language: &str,
    sdk_name: &str,
) -> String {
    match language {
        "rust" => {
            match template_name {
                "cli_meta.rs" => "src/cli_meta.rs".to_string(),
                // ... rest of rust routing
                _ => format!("src/generated/{template_name}"),
            }
        }
        "python" => {
            match template_name {
                "query.py" => format!("python/{sdk_name}/query.py"),
                "site_cli.pyi" => format!("python/{sdk_name}/{sdk_name}.pyi"),
                _ => format!("python/{sdk_name}/{template_name}"),
            }
        }
        "r" => {
            match template_name {
                "query_builder.R" => format!("r/{sdk_name}/R/query.R"),
                "validation.R" => format!("r/{sdk_name}/R/validation.R"),
                "DESCRIPTION" => format!("r/{sdk_name}/DESCRIPTION"),
                "NAMESPACE" => format!("r/{sdk_name}/NAMESPACE"),
                _ => format!("r/{sdk_name}/{template_name}"),
            }
        }
        _ => format!("generated/{language}/{sdk_name}/{template_name}"),
    }
}
```

#### 1.5 Refactor `CodeGenerator::render_all()` to Loop Over Languages

**File:** [src/core/codegen.rs](src/core/codegen.rs#L217)

Change return type and rendering loop:

```rust
/// Render all templates for all enabled languages, returning nested HashMap.
pub fn render_all(
    &self,
    site: &SiteConfig,
    options: &CliOptionsConfig,
    fields_by_index: &HashMap<String, Vec<FieldDef>>,
) -> Result<HashMap<String, HashMap<String, String>>> {
    let mut all_langs = HashMap::new();

    for language in &site.enabled_sdks {
        let rendered = self.render_for_language(language, site, options, fields_by_index)?;
        all_langs.insert(language.clone(), rendered);
    }

    Ok(all_langs)
}

/// Render templates for a single language.
fn render_for_language(
    &self,
    language: &str,
    site: &SiteConfig,
    options: &CliOptionsConfig,
    fields_by_index: &HashMap<String, Vec<FieldDef>>,
) -> Result<HashMap<String, String>> {
    // Determine which templates to render per language
    let template_names = match language {
        "rust" => vec![
            "cli_meta.rs", "indexes.rs", "fields.rs", "groups.rs", "cli_flags.rs",
            "client.rs", "output.rs", "field_meta.rs", "sdk.rs", "lib.rs", "generated_mod.rs",
            "main.rs",
        ],
        "python" => vec!["query.py", "site_cli.pyi"],
        "r" => vec![ /* Phase 2 */ ],
        _ => vec![],
    };

    let ctx = self.build_context(site, options, fields_by_index);
    let mut out = HashMap::new();

    for template_name in template_names {
        let rendered = self.tera.render(template_name, &ctx)?;
        let dest_path = template_name_to_dest(template_name, language, &site.resolved_sdk_name());
        out.insert(dest_path, rendered);
    }

    Ok(out)
}
```

**Note:** For backward compatibility, update callers in `commands/new.rs` and `commands/update.rs` to iterate over returned languages.

#### 1.6 Add Shared Template Files

Move shared templates (not language-specific) to `templates/shared/`:

- `GETTING_STARTED.md.tera`
- `PREVIEW.md.tera`
- `autoupdate.yml.tera`
- `ci.yml.tera`

These are rendered **once per site** (not per language). Update `render_all()` to include them in all language maps, or render them separately.

#### 1.7 Create Snippet Infrastructure (New)

Create `src/core/snippet.rs` with the `QuerySnapshot` and `SnippetGenerator` types:

```rust
use serde::{Deserialize, Serialize};
use tera::{Context as TeraContext, Tera};
use anyhow::{Context, Result};
use std::collections::HashMap;

/// Represents a single query as submitted to an API or built by an SDK.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QuerySnapshot {
    /// Filters: (field_name, operator, value)
    pub filters: Vec<(String, String, String)>,
    /// Sorts: (field_name, direction)
    pub sorts: Vec<(String, String)>,
    /// CLI flags, e.g., ["genome-size", "assembly"]
    pub flags: Vec<String>,
    /// Selected output fields
    pub selections: Vec<String>,
    /// Traversal: (field_name, direction)
    pub traversal: Option<(String, String)>,
    /// Summaries: (field_name, modifier)
    pub summaries: Vec<(String, String)>,
}

/// Generates code snippets in multiple languages for a given query.
pub struct SnippetGenerator {
    tera: Tera,
}

impl SnippetGenerator {
    /// Create a new snippet generator by loading bundled snippet templates.
    pub fn new() -> Result<Self> {
        let mut tera = Tera::default();

        tera.add_raw_template(
            "python_snippet",
            include_str!("../../templates/snippets/python_snippet.tera"),
        )
        .context("loading python_snippet template")?;

        // R snippet added in Phase 2
        // javascript snippet added in Phase 3

        Ok(Self { tera })
    }

    /// Render a code snippet for a given query and language.
    pub fn render_snippet(
        &self,
        query: &QuerySnapshot,
        language: &str,
        site: &crate::core::config::SiteConfig,
    ) -> Result<String> {
        let ctx = self.build_context(query, site);
        self.tera
            .render(&format!("{language}_snippet"), &ctx)
            .with_context(|| format!("rendering {language} snippet"))
    }

    /// Render snippets in all specified languages.
    pub fn render_all_snippets(
        &self,
        query: &QuerySnapshot,
        site: &crate::core::config::SiteConfig,
        languages: &[&str],
    ) -> Result<HashMap<String, String>> {
        let mut snippets = HashMap::new();
        for lang in languages {
            let snippet = self.render_snippet(query, lang, site)?;
            snippets.insert(lang.to_string(), snippet);
        }
        Ok(snippets)
    }

    fn build_context(
        &self,
        query: &QuerySnapshot,
        site: &crate::core::config::SiteConfig,
    ) -> TeraContext {
        let mut ctx = TeraContext::new();
        ctx.insert("filters", &query.filters);
        ctx.insert("sorts", &query.sorts);
        ctx.insert("flags", &query.flags);
        ctx.insert("selections", &query.selections);
        ctx.insert("traversal", &query.traversal);
        ctx.insert("summaries", &query.summaries);
        ctx.insert("site_name", &site.name);
        ctx.insert("sdk_name", &site.resolved_sdk_name());
        ctx.insert("api_base", &site.api_base);
        ctx
    }
}
```

#### 1.8 Create Initial Snippet Templates

Create `templates/snippets/python_snippet.tera`:

```python
import {{ sdk_name }} as sdk

qb = sdk.QueryBuilder()
{% for filter in filters -%}
qb.add_filter("{{ filter[0] }}", "{{ filter[1] }}", "{{ filter[2] }}")
{% endfor %}
{% for sort in sorts -%}
qb.add_sort("{{ sort[0] }}", "{{ sort[1] }}")
{% endfor %}
{% if flags -%}
qb.set_field_groups([{% for flag in flags %}"{{ flag }}"{{ "," if not loop.last }}{% endfor %}])
{% endif %}
{% if selections -%}
qb.select_fields([{% for field in selections %}"{{ field }}"{{ "," if not loop.last }}{% endfor %}])
{% endif %}

url = qb.build()
# Fetch data:
# import requests
# response = requests.get(url)
# data = response.json()
```

#### 1.9 Create Snippet HTTP Endpoint (Optional, in Express Backend)

In your Express backend, add endpoint `/api/snippet`:

```javascript
// backend/routes/snippet.ts (Express example)
import express from 'express';

app.post('/api/snippet', async (req, res) => {
  const { site, query, languages } = req.body;

  try {
    // Call cli-generator (via subprocess or FFI)
    const snippets = await generateSnippets(site, query, languages);
    res.json(snippets);
  } catch (error) {
    res.status(400).json({ error: error.message });
  }
});

// Helper: call into cli-generator (pseudocode)
async function generateSnippets(site: string, query: QuerySnapshot, languages: string[]) {
  // Option A: subprocess call
  // const { exec } = require('child_process');
  // const result = await exec(`cli-generator snippet ${site} '${JSON.stringify(query)}' ${languages.join(' ')}`);

  // Option B: FFI / WASM module (if cli-generator exports snippet generation)
  // const { SnippetGenerator } = require('cli-generator');
  // const gen = new SnippetGenerator();
  // return gen.renderAllSnippets(query, site, languages);
}
```

**Request:**

```json
POST /api/snippet
{
  "site": "goat",
  "query": {
    "filters": [["genome_size", ">=", "1000000000"]],
    "sorts": [["genome_size", "desc"]],
    "flags": ["genome-size"],
    "selections": ["organism_name", "genome_size"],
    "traversal": null,
    "summaries": []
  },
  "languages": ["python", "r"]
}
```

**Response:**

```json
{
  "python": "import goat_sdk as sdk\n\nqb = sdk.QueryBuilder()\n...",
  "r": "library(goat_sdk)\n\nqb <- QueryBuilder$new()\n..."
}
```

**UI Integration** (JavaScript):

```typescript
// ui/src/api/snippets.ts
export async function fetchSnippets(
  siteConfig: SiteConfig,
  query: QuerySnapshot,
  languages: string[],
): Promise<Record<string, string>> {
  const response = await fetch("/api/snippet", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      site: siteConfig.name,
      query,
      languages,
    }),
  });

  if (!response.ok) throw new Error("Failed to generate snippets");
  return response.json();
}

// Usage in component:
const snippets = await fetchSnippets(siteConfig, currentQuery, ["python", "r"]);
// Display snippets in tabs or modal
```

#### 1.10 Unit Tests for Phase 1

**File:** [src/core/codegen.rs](src/core/codegen.rs#L600)

Add tests:

```rust
#[test]
fn render_all_returns_nested_map_per_language() {
    let gen = CodeGenerator::new().unwrap();
    let site = sample_site();  // with enabled_sdks: ["python"]
    let options = sample_options();
    let mut fields_by_index = HashMap::new();
    fields_by_index.insert("taxon".to_string(), sample_fields());

    let all_langs = gen.render_all(&site, &options, &fields_by_index).unwrap();

    assert!(all_langs.contains_key("python"));
    assert!(all_langs.get("python").unwrap().contains_key("python/testsite_sdk/query.py"));
}

#[test]
fn template_name_to_dest_routes_per_language() {
    assert_eq!(
        template_name_to_dest("query.py", "python", "goat_sdk"),
        "python/goat_sdk/query.py"
    );

    // Placeholder for R (Phase 2):
    // assert_eq!(
    //     template_name_to_dest("query_builder.R", "r", "goat_sdk"),
    //     "r/goat_sdk/R/query.R"
    // );
}

#[test]
fn config_parses_enabled_sdks() {
    let yaml = r#"
name: testsite
display_name: Test Site
api_base: https://example.com/api
api_version: v2
indexes:
  - name: taxon
enabled_sdks:
  - python
"#;
    let config: SiteConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.enabled_sdks, vec!["python"]);
}

#[test]
fn snippet_generator_renders_python_snippet() {
    let gen = SnippetGenerator::new().unwrap();
    let query = QuerySnapshot {
        filters: vec![("genome_size".to_string(), ">=".to_string(), "1000000000".to_string())],
        sorts: vec![],
        flags: vec!["genome-size".to_string()],
        selections: vec![],
        traversal: None,
        summaries: vec![],
    };
    let site = sample_site();

    let snippet = gen.render_snippet(&query, "python", &site).unwrap();

    assert!(snippet.contains("import testsite_sdk"));
    assert!(snippet.contains("add_filter"));
    assert!(snippet.contains("genome_size"));
    assert!(snippet.contains(">="));
}

#[test]
fn snippet_generator_renders_multiple_languages() {
    let gen = SnippetGenerator::new().unwrap();
    let query = QuerySnapshot {
        filters: vec![],
        sorts: vec![],
        flags: vec![],
        selections: vec![],
        traversal: None,
        summaries: vec![],
    };
    let site = sample_site();

    let snippets = gen.render_all_snippets(&query, &site, &["python"]).unwrap();

    assert!(snippets.contains_key("python"));
}
```

#### 1.11 Update Command Callers

**Files:** [src/commands/new.rs](src/commands/new.rs), [src/commands/update.rs](src/commands/update.rs)

Update `write_generated_files()` to iterate over languages:

```rust
fn write_generated_files(
    rendered_by_lang: &HashMap<String, HashMap<String, String>>,
    output_dir: &Path,
) -> Result<()> {
    for (language, rendered) in rendered_by_lang {
        // Apply language-specific post-processing
        let processed = postprocess(rendered, language)?;
        for (file_path, content) in processed {
            let full_path = output_dir.join(&file_path);
            fs::create_dir_all(full_path.parent().unwrap())?;
            fs::write(&full_path, content)?;
        }
    }
    Ok(())
}
```

#### 1.12 Extend CI to Report Language-Specific Status

**File:** `.github/workflows/ci.yml`

Update Rust tests to verify Phase 1 refactoring:

```yaml
- name: Test Phase 1 infrastructure (multi-lang rendering)
  run: cargo test codegen::tests::render_all_returns_nested_map_per_language
```

---

### Phase 2: R SDK Implementation

**Goal:** Prove the multi-language pattern works. Deliver R SDK with full feature parity to Python.

#### 2.1 Design R Template Set

Create templates in `templates/r/`:

1. **query_builder.R.tera** — S4 class or R6 reference class matching Python QueryBuilder
   - Methods: `add_filter()`, `add_sort()`, `restrict()`, `add_summary()`, `build()`
   - Properties: filters, sorts, flags, selections, traversal
   - Docstrings via roxygen2 comments

2. **validation.R.tera** — Field metadata and validation logic
   - Data frame of canonical field names, types, synonyms, enum values
   - Functions: `validate_field()`, `get_enum_values()`, `get_summary_modifiers()`
   - Synonym resolution (deprecated aliases → canonical names)

3. **DESCRIPTION.tera** — R package metadata
   - Package name: `{sdk_name}` (e.g., `goat_sdk`)
   - Version: match generator version (e.g., `0.1.0`)
   - Title: "QueryBuilder for {site_display_name}"
   - Authors: auto-populate from config or defaults
   - Imports: `httr2` (HTTP), `jsonlite` (JSON)

4. **NAMESPACE.tera** — R package exports
   - Export S4 class `QueryBuilder`
   - Export helper functions (if any)
   - Import namespace declarations

5. **README.md.tera** — Basic usage guide (similar to Python's query.py docstring)

6. **Create R Snippet Template** — `templates/snippets/r_snippet.tera`
   - Short, readable example: load SDK, build query, execute
   - Format: clean R idioms ($ accessor, <- assignment)
   - Matches Python snippet structure for consistency

   Example `r_snippet.tera`:

   ```r
   library({{ sdk_name }})

   qb <- QueryBuilder$new()
   {% for filter in filters -%}
   qb$add_filter("{{ filter[0] }}", "{{ filter[1] }}", "{{ filter[2] }}")
   {% endfor %}
   {% for sort in sorts -%}
   qb$add_sort("{{ sort[0] }}", "{{ sort[1] }}")
   {% endfor %}
   {% if flags -%}
   qb$set_field_groups(c({% for flag in flags %}"{{ flag }}"{{ "," if not loop.last }}{% endfor %}))
   {% endif %}

   url <- qb$build()
   # Fetch data:
   # response <- httr2::request(url) %>% httr2::req_perform()
   # data <- response %>% httr2::resp_body_json()
   ```

#### 2.2 Implement R Query Builder Class

**Design notes:**

- Use **R6 reference class** over S4 for simplicity and Python parity
- Methods return `self` for method chaining: `qb$add_filter()$add_sort()$build()`
- `build()` method returns URL string (via `paste0()`) matching Rust `build_url()`
- Field validation at runtime (prevent typos, unsupported operators)
- Support field synonyms (deprecated aliases resolve to canonical)

#### 2.3 Add R to `make_tera()` and `render_for_language()`

**File:** [src/core/codegen.rs](src/core/codegen.rs#L21)

```rust
fn make_tera() -> Result<Tera> {
    // ... existing Rust/Python templates ...

    // Phase 2: R templates
    tera.add_raw_template(
        "query_builder.R",
        include_str!("../../templates/r/query_builder.R.tera"),
    )
    .context("loading query_builder.R template")?;
    tera.add_raw_template(
        "validation.R",
        include_str!("../../templates/r/validation.R.tera"),
    )
    .context("loading validation.R template")?;
    tera.add_raw_template(
        "DESCRIPTION",
        include_str!("../../templates/r/DESCRIPTION.tera"),
    )
    .context("loading DESCRIPTION template")?;
    // ... etc
    Ok(tera)
}
```

Update `render_for_language()`:

```rust
"r" => vec![
    "query_builder.R", "validation.R", "DESCRIPTION", "NAMESPACE", "README.md",
],
```

#### 2.4 Implement R Code Generation Tests

**File:** [tests/python/test_core.py](tests/python/test_core.py) (or `tests/r/test_codegen.r`)

1. **Template rendering test:** Render R templates for test site, verify output files exist
2. **Syntax validation:** Parse generated R code (language-agnostic: check for balanced parens/quotes, roxygen markers)
3. **QueryBuilder interface:** Verify generated R class has required methods (`add_filter`, `build`, etc.)
4. **Field metadata:** Verify synonym resolution works in generated validation.R
5. **Version parity:** Assert DESCRIPTION version matches config metadata

Example (Python-based):

```python
def test_render_r_templates():
    gen = CodeGenerator()
    site = sample_site()
    site.enabled_sdks = ["r"]
    rendered = gen.render_all(site, sample_options(), sample_fields_by_index())

    assert "r" in rendered
    assert "r/testsite_sdk/R/query.R" in rendered["r"]
    assert "r/testsite_sdk/DESCRIPTION" in rendered["r"]

    r_code = rendered["r"]["r/testsite_sdk/R/query.R"]
    assert "setRefClass" in r_code or "R6Class" in r_code
    assert "add_filter" in r_code
    assert "build" in r_code
```

#### 2.5 Extend CI to Test R

**File:** `.github/workflows/ci.yml`

Add parallel job:

```yaml
test-r:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: r-lib/actions/setup-r@v2
      with:
        r-version: "latest"
    - name: Install dependencies
      run: |
        install.packages(c("devtools", "lintr"))
        devtools::install_dev_deps()
      shell: Rscript {0}
    - name: Lint R code
      run: lintr::lint_package()
      shell: Rscript {0}
    - name: Run R tests
      run: devtools::test()
      shell: Rscript {0}
```

#### 2.6 Document R SDK in PREVIEW

Update [templates/PREVIEW.md.tera](templates/PREVIEW.md.tera) to include:

- Quick-start code snippet for R QueryBuilder
- Feature parity matrix vs Python
- Link to generated R package README

#### 2.7 Verify Backward Compatibility

Run full test suite:

```bash
cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test
pytest tests/python/ -v
```

Ensure Python SDK generation unaffected. Regenerate GOAT/BOAT CLIs; output should match pre-Phase-1 commits.

---

### Phase 3: Additional Languages (Future)

Once R is complete and tested, the pattern is established. Subsequent languages follow the same steps:

#### 3.1 JavaScript/TypeScript SDK

**SDK Templates:** `query_builder.ts`, `validation.ts`, `package.json`
**Snippet Template:** `templates/snippets/javascript_snippet.tera` (example code snippet)
**Formatter:** `prettier`
**Test framework:** Jest
**Effort:** ~80% of Phase 2 (templates + tests)

Example `javascript_snippet.tera`:

```javascript
import { QueryBuilder } from '{{ sdk_name }}';

const qb = new QueryBuilder();
{% for filter in filters -%}
qb.addFilter('{{ filter[0] }}', '{{ filter[1] }}', '{{ filter[2] }}');
{% endfor %}
{% for sort in sorts -%}
qb.addSort('{{ sort[0] }}', '{{ sort[1] }}');
{% endfor %}
{% if flags -%}
qb.setFieldGroups([{% for flag in flags %}'{{ flag }}'{{ "," if not loop.last }}{% endfor %}]);
{% endif %}

const url = qb.build();
// Fetch data:
// const response = await fetch(url);
// const data = await response.json();
```

#### 3.2 Go SDK

**SDK Templates:** `query_builder.go`, `validation.go`, `go.mod`
**Snippet Template:** `templates/snippets/go_snippet.tera` (optional; Go is less interactive, so snippets may be less relevant)
**Formatter:** `gofmt`
**Test framework:** `go test`

---

## Implementation Checklist

### Phase 1: Infrastructure

- [ ] Reorganize templates into language subdirectories (Rust, Python, shared, snippets)
- [ ] Update `make_tera()` with new template paths
- [ ] Add `enabled_sdks` field to `SiteConfig`
- [ ] Refactor `template_name_to_dest()` to be language-aware
- [ ] Refactor `CodeGenerator::render_all()` to loop over languages
- [ ] Handle shared templates (GETTING_STARTED, etc.)
- [ ] Update `commands/new.rs` and `commands/update.rs` to consume nested HashMap
- [ ] Create `src/core/snippet.rs` with `QuerySnapshot` and `SnippetGenerator`
- [ ] Create initial snippet template: `templates/snippets/python_snippet.tera`
- [ ] (Optional) Create `src/commands/snippet.rs` for CLI snippet generation
- [ ] (Optional) Document Express endpoint structure for `/api/snippet`
- [ ] Add Phase 1 unit tests (CodeGenerator + SnippetGenerator)
- [ ] Run full test suite; verify backward compat with Python

### Phase 2: R SDK

- [ ] Write R SDK template files (5 templates: query_builder.R, validation.R, DESCRIPTION, NAMESPACE, README.md)
- [ ] Add R snippet template: `templates/snippets/r_snippet.tera`
- [ ] Add R templates to `make_tera()`
- [ ] Add R branch to `render_for_language()` for both SDK and snippet rendering
- [ ] Add R tests (template rendering, syntax, interface, snippet generation)
- [ ] Extend CI with R test job
- [ ] Update PREVIEW.md with R quick-start
- [ ] Manually test: `cli-generator new goat --output-dir ../goat-r --enabled-sdks r`
- [ ] Verify generated R package installs and QueryBuilder works
- [ ] Test snippet generation for R with sample query

### Phase 3: JavaScript (whenever)

- [ ] Write JS SDK template files (3 templates: query_builder.ts, validation.ts, package.json)
- [ ] Add JS snippet template: `templates/snippets/javascript_snippet.tera`
- [ ] Add JS templates to `make_tera()`
- [ ] Add JS branch to `render_for_language()` for both SDK and snippet rendering
- [ ] Add JS tests (template rendering, syntax, interface, snippet generation)
- [ ] Extend CI with JS test job (Jest)
- [ ] Integrate JS SDK generation into Express backend
- [ ] Implement `/api/snippet` endpoint that calls `SnippetGenerator`
- [ ] Test UI integration: fetch snippets, display in tabs

### Phase 3: Go (whenever)

- [ ] Repeat SDK steps for Go (templates, CI, tests)
- [ ] Add Go snippet template (optional; Go is less interactive)

---

## Key Decisions

1. **Versioning:** All SDKs locked to generator version. Single changelog.
2. **Post-processing:** Mandatory in both `new` and `update` commands (Python: `black`/`isort`, R: `styler`, JS: `prettier`).
3. **Field metadata:** Serialized to JSON alongside generated files (Option A). R and JS templates read same JSON.
4. **Monorepo:** Expand `cli-generator` to include R, JS, Go. CI runs language tests in parallel.
5. **Backward compatibility:** Phase 1 refactoring maintains Python SDK generation without changes to `sites/` configs.
6. **Snippet generation:** Hybrid approach. UI submits `QuerySnapshot` JSON to backend `/api/snippet` endpoint. Backend uses `SnippetGenerator` to render snippets in all languages.

---

## Risk Mitigation

1. **Phase 1 refactoring is breaking:** Mitigate with extensive backward-compat tests. Regenerate GOAT/BOAT CLIs; diffs should be empty.
2. **R test infrastructure unfamiliar:** Evaluate R CI action (`r-lib/actions/setup-r`) early. Fallback: run `R CMD check` if devtools is overkill.
3. **Field metadata drift:** Enforce schema via tests. Python, R, JS must all read and validate same `TemplateFieldMeta` structure.
4. **Build time bloat:** Monitor CI time. If >10min for all languages, investigate parallelization.
5. **Query structure versioning:** If `QuerySnapshot` schema changes, ensure backward compatibility (default values for new fields, graceful degradation in old clients).

---

## Future Considerations

- **Runtime metadata API:** Expose field definitions at SDK runtime (R `list()`, JS object) for advanced use cases?
- **SDK package managers:** CRAN (R), PyPI (Python), npm (JS) registration and publishing automation?
- **Constraint solver:** Advanced query validation (e.g., "field X cannot be combined with field Y")?
- **Performance:** Benchmark URL building across languages; optimize if needed.

---

## Files Modified (Phase 1 & 2)

**Core:**

- `src/core/codegen.rs` — Template loading, rendering loop, destination routing
- `src/core/config.rs` — Add `enabled_sdks` to `SiteConfig`
- `src/core/snippet.rs` — **NEW** — `QuerySnapshot` struct, `SnippetGenerator` type

**Commands:**

- `src/commands/new.rs` — Update to handle language-nested rendered output
- `src/commands/update.rs` — Same
- `src/commands/snippet.rs` — **NEW (Optional)** — CLI command for snippet generation

**Templates:**

- Move existing to `templates/rust/` and `templates/python/`
- Add `templates/snippets/` — Snippet templates for all languages
  - `templates/snippets/python_snippet.tera` (Phase 1)
  - `templates/snippets/r_snippet.tera` (Phase 2)
  - `templates/snippets/javascript_snippet.tera` (Phase 3)
- Add `templates/r/` (Phase 2)
- Add `templates/shared/` for GETTING_STARTED, PREVIEW, etc.

**Tests:**

- `src/core/codegen.rs` — Add Phase 1 tests (including `SnippetGenerator` tests)
- `tests/python/test_core.py` — Add R rendering and snippet tests (Phase 2)
- `.github/workflows/ci.yml` — Add R test job (Phase 2)

**Documentation:**

- `docs/multi-language-sdk-plan.md` — This file
- `templates/PREVIEW.md.tera` — R quick-start (Phase 2)
- Backend docs — Document `/api/snippet` endpoint (Phase 3, in Express backend)

**Express Backend (Not in cli-generator repo):**

- `backend/routes/snippet.ts` — **NEW (Phase 3)** — Endpoint to call `SnippetGenerator`
- `backend/types/query.ts` — **NEW (Phase 3)** — Export `QuerySnapshot` type
- UI integration via `POST /api/snippet` → render snippets in tabs/modal

---

## References

- [src/core/codegen.rs](src/core/codegen.rs) — Current template system
- [src/core/config.rs](src/core/config.rs) — Config schema
- [src/commands/new.rs](src/commands/new.rs) — File writing logic
- [tests/python/test_core.py](tests/python/test_core.py) — Test patterns
- [.github/workflows/ci.yml](.github/workflows/ci.yml) — CI structure
