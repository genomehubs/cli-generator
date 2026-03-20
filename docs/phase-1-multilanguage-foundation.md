# Phase 1: Multi-Language Foundation & Query Description

**Status:** Planned
**Date:** 20 March 2026
**Duration Estimate:** 2 weeks
**Target:** Establish language-agnostic codegen infrastructure and query description system

---

## Overview

Phase 1 prepares `cli-generator` for multi-language SDK support (R, JavaScript) by:

1. **Refactoring codegen** to handle multiple output languages
2. **Adding snippet generation** infrastructure (code examples in any language)
3. **Adding query description** system (human-readable explanations of queries)
4. **Maintaining backward compatibility** with Python SDK generation

After Phase 1 succeeds, Phase 2 (R SDK) becomes straightforward templating work.

---

## Architecture Changes

### High-Level Flow

```
CLI Input (query.yaml, site config)
    ↓
SearchQuery (core/query) + SiteConfig (core/config)
    ↓
┌─────────────────────────────────────────┐
│ CodeGenerator (core/codegen)            │
│ render_for_language() → per-language    │
│                         HashMap output  │
└─────────────────────────────────────────┘
    ↓
    ├─→ Python SDK templates → python/{sdk_name}/
    ├─→ R SDK templates (Phase 2) → r/{sdk_name}/
    ├─→ Shared templates → root level
    └─→ Snippet templates (SnippetGenerator) → n/a (rendered on demand)

┌─────────────────────────────────────────┐
│ QueryDescriber (core/describe)          │
│ Takes: SearchQuery + FieldMetadata      │
│ Outputs: prose or structured parts      │
└─────────────────────────────────────────┘
    ↓
    ├─→ CLI: --describe flag
    ├─→ SDK: .describe() method
    └─→ API: POST /api/describe endpoint
```

---

## Implementation Steps

### Step 1: Reorganize Templates by Language

**Affected File:** `templates/` directory

**Current structure:**

```
templates/
  autoupdate.yml.tera
  ci.yml.tera
  cli_flags.rs.tera
  ... (mixed Rust, Python, shared)
```

**New structure:**

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
  snippets/
    python_snippet.tera
    # r_snippet.tera (Phase 2)
    # javascript_snippet.tera (Phase 3)
  r/
    # (empty for now, created in Phase 2)
```

**Action:** Move files into subdirectories. This is a pure refactor with no behavior change yet.

---

### Step 2: Update `src/core/config.rs` — Add `enabled_sdks`

**Change:** Add optional field to `SiteConfig`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteConfig {
    pub name: String,
    pub display_name: String,
    // ... existing fields ...

    /// Which SDK languages to generate (defaults to ["python"]).
    #[serde(default = "default_enabled_sdks")]
    pub enabled_sdks: Vec<String>,
}

fn default_enabled_sdks() -> Vec<String> {
    vec!["python".to_string()]
}
```

**Update example config** (`sites/goat.yaml`):

```yaml
name: goat
display_name: Genomes on a Tree
api_base: https://goat.genomehubs.org/api
api_version: v2

enabled_sdks:
  - python
  # - r (enable in Phase 2)

indexes:
  - name: taxon
    # ... rest unchanged
```

**Note:** Field display names come from the API metadata (via `resultFields`), not from config. The `FieldDef` struct should already include a `display_name` field populated from the API response.

```rust
use crate::core::config::FieldDef;
use crate::core::query::{Attribute, AttributeSet, Identifiers, QueryParams, SearchIndex, SearchQuery};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Generates human-readable descriptions of genomehubs queries.
pub struct QueryDescriber {
    /// Field metadata from API (includes canonical name and display name).
    field_metadata: HashMap<String, FieldDef>,
}

impl QueryDescriber {
    /// Create a new describer with field metadata from the API.
    ///
    /// The `field_metadata` HashMap should be populated from the API's `resultFields`
    /// endpoint, where each `FieldDef` contains the canonical name and display name.
    pub fn new(field_metadata: HashMap<String, FieldDef>) -> Self {
        Self { field_metadata }
    }

    /// Get the display name for a field (prefers metadata, falls back to canon name).
    fn display_name(&self, canonical_name: &str) -> String {
        self.field_metadata
            .get(canonical_name)
            .and_then(|field| field.display_name.clone())
            .unwrap_or_else(|| canonical_name.replace('_', " "))
    }

    /// Describe a query as structured components.
    pub fn describe_parts(&self, query: &SearchQuery) -> DescribedQuery {
        DescribedQuery {
            index: self.describe_index(&query.identifiers),
            taxa_filter: self.describe_taxa_filter(&query.identifiers),
            filters: self.describe_filters(&query.attributes),
            sorts: self.describe_sorts(&query.parameters),
            selections: self.describe_selections(&query.attributes),
        }
    }

    /// Describe a query in concise prose form.
    /// Example: "Search for taxa in Mammalia, filtered to genome size >= 1GB, returning organism_name."
    pub fn describe_concise(&self, query: &SearchQuery) -> String {
        let parts = self.describe_parts(query);
        self.assemble_prose(&parts, false)
    }

    /// Describe a query in verbose prose form.
    /// Example: "Search for taxa in the Mammalia taxonomy branch (including all descendants).
    /// Filtered to: genome size >= 1 gigabyte and assembly level is chromosome.
    /// Sorted by organism name (ascending). Returning fields: organism_name, genome_size."
    pub fn describe_verbose(&self, query: &SearchQuery) -> String {
        let parts = self.describe_parts(query);
        self.assemble_prose(&parts, true)
    }

    fn describe_index(&self, identifiers: &Identifiers) -> String {
        match &identifiers.search_index {
            SearchIndex::Taxon => "taxa".to_string(),
            SearchIndex::Assembly => "assemblies".to_string(),
            SearchIndex::Sample => "samples".to_string(),
        }
    }

    fn describe_taxa_filter(&self, identifiers: &Identifiers) -> Option<String> {
        identifiers.taxa.as_ref().map(|taxa_filter| {
            let names = taxa_filter.names.join(", ");
            let mode = match taxa_filter.filter_type {
                crate::core::query::TaxonFilterType::Direct => {
                    "direct matches only"
                }
                crate::core::query::TaxonFilterType::Tree => {
                    "including all descendants in the taxonomy tree"
                }
                crate::core::query::TaxonFilterType::Ancestral => {
                    "including all ancestors in the taxonomy tree"
                }
            };
            format!("{} ({})", names, mode)
        })
    }

    fn describe_filters(&self, attributes: &AttributeSet) -> Vec<FilterDescription> {
        attributes
            .attributes
            .iter()
            .filter_map(|attr| {
                let field_display = self.display_name(&attr.name);

                match (&attr.operator, &attr.value) {
                    (Some(op), Some(value)) => {
                        let op_symbol = op.as_str();
                        let values_str = value.as_strs().join(", ");
                        Some(FilterDescription {
                            field: field_display,
                            operator: op_symbol.to_string(),
                            value: values_str,
                            concise: format!("{} {} {}", field_display, op_symbol, values_str),
                        })
                    }
                    (Some(op), None) => {
                        // Exists / Missing operator without value
                        let verb = if op.as_str().is_empty() {
                            "exists"
                        } else {
                            "is missing"
                        };
                        Some(FilterDescription {
                            field: field_display.clone(),
                            operator: verb.to_string(),
                            value: String::new(),
                            concise: format!("{} {}", field_display, verb),
                        })
                    }
                    _ => None,
                }
            })
            .collect()
    }

    fn describe_sorts(&self, params: &QueryParams) -> Vec<SortDescription> {
        params
            .sort_by
            .iter()
            .map(|field| {
                let field_display = self.display_name(field);
                let direction = match params.sort_order {
                    crate::core::query::SortOrder::Ascending => "ascending",
                    crate::core::query::SortOrder::Descending => "descending",
                };
                SortDescription {
                    field: field_display.clone(),
                    direction: direction.to_string(),
                    concise: format!("{} ({})", field_display, direction),
                }
            })
            .collect()
    }

    fn describe_selections(&self, attributes: &AttributeSet) -> Vec<String> {
        attributes
            .fields
            .iter()
            .map(|field| self.display_name(&field.name))
            .collect()
    }

    fn assemble_prose(&self, parts: &DescribedQuery, verbose: bool) -> String {
        if verbose {
            self.assemble_verbose(parts)
        } else {
            self.assemble_concise(parts)
        }
    }

    fn assemble_concise(&self, parts: &DescribedQuery) -> String {
        let mut prose = format!("Search for {} in {}", parts.index, parts.index);

        if let Some(ref taxa) = parts.taxa_filter {
            prose.push_str(&format!(" ({})", taxa));
        }

        if !parts.filters.is_empty() {
            let filter_strs: Vec<_> = parts.filters.iter().map(|f| f.concise.clone()).collect();
            prose.push_str(&format!(", filtered to {}", filter_strs.join(" and ")));
        }

        if !parts.sorts.is_empty() {
            let sort_strs: Vec<_> = parts.sorts.iter().map(|s| s.concise.clone()).collect();
            prose.push_str(&format!(", {}", sort_strs.join(", ")));
        }

        if !parts.selections.is_empty() {
            prose.push_str(&format!(", returning {}", parts.selections.join(", ")));
        }

        prose.push('.');
        prose
    }

    fn assemble_verbose(&self, parts: &DescribedQuery) -> String {
        let mut prose = format!("Search for {} in the database", parts.index);

        if let Some(ref taxa) = parts.taxa_filter {
            prose.push_str(&format!(" in the {} taxonomy branch", taxa));
        }
        prose.push('.');

        if !parts.filters.is_empty() {
            prose.push_str("\n\nFilters applied:\n");
            for filter in &parts.filters {
                prose.push_str(&format!("  • {} {} {}\n", filter.field, filter.operator, filter.value));
            }
        }

        if !parts.sorts.is_empty() {
            prose.push_str("\nSorted by:\n");
            for sort in &parts.sorts {
                prose.push_str(&format!("  • {} ({})\n", sort.field, sort.direction));
            }
        }

        if !parts.selections.is_empty() {
            prose.push_str("\nReturning fields:\n");
            for field in &parts.selections {
                prose.push_str(&format!("  • {}\n", field));
            }
        }

        prose
    }

}

/// Structured description of a query (can be formatted multiple ways). #[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescribedQuery {
pub index: String,
pub taxa_filter: Option<String>,
pub filters: Vec<FilterDescription>,
pub sorts: Vec<SortDescription>,
pub selections: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterDescription {
pub field: String,
pub operator: String,
pub value: String,
pub concise: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortDescription {
pub field: String,
pub direction: String,
pub concise: String,
}

```

**Update `src/core/mod.rs`:**

```rust
pub mod codegen;
pub mod config;
pub mod describe;  // NEW
pub mod fetch;
pub mod query;
pub mod snippet;
```

---

### Step 4: Create `src/core/snippet.rs` — Snippet Generation

**New file:** `src/core/snippet.rs`

```rust
//! Code snippet generation for all languages.
//!
//! Generates runnable code examples in Python, R, JavaScript, etc. suitable for
//! embedding in UIs or documentation.

use crate::core::config::SiteConfig;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tera::{Context as TeraContext, Tera};

/// Represents a single query as built by an SDK or UI.
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
    /// Traversal context: (field_name, direction)
    pub traversal: Option<(String, String)>,
    /// Summaries: (field_name, modifier)
    pub summaries: Vec<(String, String)>,
}

/// Generates runnable code snippets in multiple languages.
pub struct SnippetGenerator {
    tera: Tera,
}

impl SnippetGenerator {
    /// Create a new snippet generator with bundled templates.
    pub fn new() -> Result<Self> {
        let mut tera = Tera::default();

        tera.add_raw_template(
            "python_snippet",
            include_str!("../../templates/snippets/python_snippet.tera"),
        )
        .context("loading python_snippet template")?;

        // R snippet added in Phase 2
        // JavaScript snippet added in Phase 3

        Ok(Self { tera })
    }

    /// Render a single-language snippet.
    pub fn render_snippet(
        &self,
        query: &QuerySnapshot,
        language: &str,
        site: &SiteConfig,
    ) -> Result<String> {
        let template_name = format!("{}_snippet", language);
        let ctx = self.build_context(query, site);

        self.tera
            .render(&template_name, &ctx)
            .with_context(|| format!("rendering {} snippet", language))
    }

    /// Render snippets in multiple languages.
    pub fn render_all_snippets(
        &self,
        query: &QuerySnapshot,
        site: &SiteConfig,
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
        site: &SiteConfig,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_generator_renders_python() {
        let gen = SnippetGenerator::new().unwrap();
        let site = SiteConfig {
            name: "testsite".to_string(),
            display_name: "Test Site".to_string(),
            ..Default::default()
        };
        let query = QuerySnapshot {
            filters: vec![("genome_size".to_string(), ">=".to_string(), "1000000000".to_string())],
            sorts: vec![],
            flags: vec![],
            selections: vec![],
            traversal: None,
            summaries: vec![],
        };

        let snippet = gen.render_snippet(&query, "python", &site).unwrap();

        assert!(snippet.contains("QueryBuilder"));
        assert!(snippet.contains("genome_size"));
        assert!(snippet.contains(">="));
    }
}
```

---

### Step 5: Create Snippet Template

**New file:** `templates/snippets/python_snippet.tera`

```python
import {{ sdk_name }} as sdk

# Create a query builder for {{ site_name }}
qb = sdk.QueryBuilder("taxon")

{% if traversal -%}
# Set traversal: {{ traversal[0] }} {{ traversal[1] }}
qb.set_traversal("{{ traversal[0] }}", "{{ traversal[1] }}")
{% endif %}

{% for filter in filters -%}
# Add filter: {{ filter[0] }} {{ filter[1] }} {{ filter[2] }}
qb.add_attribute("{{ filter[0] }}", operator="{{ filter[1] }}", value="{{ filter[2] }}")
{% endfor %}

{% for sort in sorts -%}
# Sort by {{ sort[0] }} {{ sort[1] }}
qb.add_sort("{{ sort[0] }}", "{{ sort[1] }}")
{% endfor %}

{% if selections -%}
# Select specific fields
qb.set_fields([
{%- for field in selections %}
    "{{ field }}",
{%- endfor %}
])
{% endif %}

# Build the query and fetch results
url = qb.build()
print(f"Query URL: {url}")

# Optionally fetch data:
# import requests
# response = requests.get(url)
# data = response.json()
```

---

### Step 6: Update `src/core/codegen.rs` — Refactor for Multi-Language

**Key changes:**

1. **New function signature:**

```rust
/// Render templates for all enabled languages, returning nested map.
pub fn render_all(
    &self,
    site: &SiteConfig,
    options: &CliOptionsConfig,
    fields_by_index: &HashMap<String, Vec<FieldDef>>,
) -> Result<HashMap<String, HashMap<String, String>>> {
    let mut all_langs: HashMap<String, HashMap<String, String>> = HashMap::new();

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
    let template_names = match language {
        "rust" => vec![
            "cli_meta.rs", "indexes.rs", "fields.rs", "groups.rs", "cli_flags.rs",
            "client.rs", "output.rs", "field_meta.rs", "sdk.rs", "lib.rs", "generated_mod.rs",
            "main.rs",
        ],
        "python" => vec!["query.py", "site_cli.pyi"],
        "r" => vec![], // Empty for Phase 2
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

2. **Update `make_tera()` to load from new subdirectories:**

```rust
fn make_tera() -> Result<Tera> {
    let mut tera = Tera::default();

    // Rust templates
    tera.add_raw_template(
        "cli_meta.rs",
        include_str!("../../templates/rust/cli_meta.rs.tera"),
    )?;
    // ... repeat for all Rust templates ...

    // Python templates
    tera.add_raw_template(
        "query.py",
        include_str!("../../templates/python/query.py.tera"),
    )?;
    // ... repeat for all Python templates ...

    // Shared templates
    tera.add_raw_template(
        "GETTING_STARTED.md",
        include_str!("../../templates/shared/GETTING_STARTED.md.tera"),
    )?;
    // ... repeat for all shared templates ...

    Ok(tera)
}
```

3. **Update `template_name_to_dest()`:**

```rust
fn template_name_to_dest(template_name: &str, language: &str, sdk_name: &str) -> String {
    match language {
        "rust" => {
            match template_name {
                "cli_meta.rs" => "src/cli_meta.rs".to_string(),
                "indexes.rs" => "src/generated/indexes.rs".to_string(),
                // ... rest of Rust routing ...
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
            // Phase 2
            format!("r/{sdk_name}/{template_name}")
        }
        _ => format!("generated/{language}/{sdk_name}/{template_name}"),
    }
}
```

---

### Step 7: Update `src/commands/new.rs` and `update.rs`

**Change:** Handle nested language map from `render_all()`:

```rust
// Old (single language):
// let rendered = generator.render_all(site, options, fields)?;
// write_files(&rendered, output_dir)?;

// New (multi-language):
let rendered_by_lang = generator.render_all(site, options, fields)?;

for (language, rendered) in rendered_by_lang {
    let lang_output_dir = output_dir.join(&language);  // Optional: organize by language
    write_files(&rendered, &lang_output_dir)?;

    // Run language-specific post-processing
    postprocess_language(&lang_output_dir, &language)?;
}

fn postprocess_language(dir: &Path, language: &str) -> Result<()> {
    match language {
        "python" => {
            run_command("black", &["--line-length", "120", dir.to_str().unwrap()])?;
            run_command("isort", &["--profile", "black", "--line-length", "120", dir.to_str().unwrap()])?;
        }
        "r" => {
            // Phase 2: R styler
        }
        // ... other languages
        _ => {}
    }
    Ok(())
}
```

---

### Step 8: Update Python SDK — Add `.describe()` Method

**File:** `python/cli_generator/query.py`

```python
from typing import Literal

class QueryBuilder:
    # ... existing methods ...

    def describe(self, mode: Literal["concise", "verbose"] = "concise") -> str:
        """Get a human-readable description of this query.

        Args:
            mode: "concise" for one-line summary, "verbose" for detailed breakdown.

        Returns:
            English prose description of the query.

        Example:
            qb = QueryBuilder("taxon").add_attribute("genome_size", ">=", "1G")
            print(qb.describe())
            # Output: "Search for taxa, filtered to genome size >= 1G, returning all fields."
        """
        import json
        from .cli_generator import describe_query  # FFI call to Rust

        query_yaml = self.to_query_yaml()
        params_yaml = self.to_params_yaml()

        # Get field metadata from the API (resultFields endpoint)
        # This should be fetched once and cached at QueryBuilder initialization
        field_metadata_json = json.dumps(self.field_metadata)  # self.field_metadata set during init

        return describe_query(query_yaml, params_yaml, field_metadata_json, mode)
```

**Update `src/lib.rs`** to export Rust function as PyO3:

```rust
#[pyfunction]
#[pyo3(signature = (query_yaml, params_yaml, field_metadata_json, mode = "concise"))]
fn describe_query(query_yaml: &str, params_yaml: &str, field_metadata_json: &str, mode: &str) -> PyResult<String> {
    let query: SearchQuery = serde_yaml::from_str(query_yaml)
        .map_err(|e| PyValueError::new_err(format!("Invalid query YAML: {}", e)))?;

    // Parse field metadata from JSON (populated from API's resultFields endpoint)
    let field_metadata: HashMap<String, FieldDef> = serde_json::from_str(field_metadata_json)
        .map_err(|e| PyValueError::new_err(format!("Invalid field metadata JSON: {}", e)))?;

    let describer = QueryDescriber::new(field_metadata);

    let result = match mode {
        "verbose" => describer.describe_verbose(&query),
        _ => describer.describe_concise(&query),
    };

    Ok(result)
}
```

---

### Step 9: Add CLI Flag — `--describe`

**File:** `src/main.rs` (or new `src/commands/describe.rs`)

```rust
#[derive(Parser)]
struct SearchArgs {
    #[arg(long, help = "Get a human-readable description instead of building URL")]
    describe: bool,

    #[arg(long, help = "Verbose description (detailed breakdown)")]
    verbose: bool,
}

fn search_command(args: SearchArgs) -> Result<()> {
    let query = load_query(&args.query_file)?;
    let site = load_site_config(&args.site)?;

    if args.describe {
        let display_names = site.field_display_names.clone();
        let describer = QueryDescriber::new(display_names);

        let description = if args.verbose {
            describer.describe_verbose(&query)
        } else {
            describer.describe_concise(&query)
        };

        println!("{}", description);
        return Ok(());
    }

    // ... rest of search logic ...
}
```

**Usage:**

```bash
cli-generator search query.yaml --site goat --describe
# Output: "Search for taxa in Mammalia, filtered to genome size >= 1GB."

cli-generator search query.yaml --site goat --describe --verbose
# Output: "Search for taxa in the Mammalia taxonomy branch...
#          Filters applied:
#            • genome size >= 1 gigabyte
#          ..."
```

---

### Step 10: Comprehensive Tests

**File:** `src/core/describe.rs` (in existing module)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn sample_describer() -> QueryDescriber {
        // Build field metadata from API response (simulate resultFields endpoint)
        let mut metadata = HashMap::new();

        metadata.insert("genome_size".to_string(), FieldDef {
            name: "genome_size".to_string(),
            display_name: Some("genome size".to_string()),
            ..Default::default()
        });

        metadata.insert("organism_name".to_string(), FieldDef {
            name: "organism_name".to_string(),
            display_name: Some("organism name".to_string()),
            ..Default::default()
        });

        QueryDescriber::new(metadata)
    }

    #[test]
    fn describe_concise_simple_filter() {
        let describer = sample_describer();
        let mut query = SearchQuery::default();
        query.attributes.attributes.push(Attribute {
            name: "genome_size".to_string(),
            operator: Some(AttributeOperator::Ge),
            value: Some(AttributeValue::Single("1000000000".to_string())),
            modifier: vec![],
        });

        let desc = describer.describe_concise(&query);

        assert!(desc.contains("genome size"));
        assert!(desc.contains(">="));
        assert!(desc.contains("1000000000"));
    }

    #[test]
    fn describe_verbose_formats_as_bullet_list() {
        let describer = sample_describer();
        let mut query = SearchQuery::default();
        query.attributes.attributes.push(Attribute {
            name: "genome_size".to_string(),
            operator: Some(AttributeOperator::Ge),
            value: Some(AttributeValue::Single("1000000000".to_string())),
            modifier: vec![],
        });

        let desc = describer.describe_verbose(&query);

        assert!(desc.contains("Filters applied:"));
        assert!(desc.contains("•"));
    }

    #[test]
    fn describe_parts_returns_structured_form() {
        let describer = sample_describer();
        let query = SearchQuery::default();
        let parts = describer.describe_parts(&query);

        assert_eq!(parts.index, "taxa");
        assert!(parts.filters.is_empty());
    }

    #[test]
    fn describe_with_multiple_filters() {
        let describer = sample_describer();
        let mut query = SearchQuery::default();

        // Add genome_size filter
        query.attributes.attributes.push(Attribute {
            name: "genome_size".to_string(),
            operator: Some(AttributeOperator::Ge),
            value: Some(AttributeValue::Single("1000000000".to_string())),
            modifier: vec![],
        });

        // Add organism_name filter
        query.attributes.attributes.push(Attribute {
            name: "organism_name".to_string(),
            operator: Some(AttributeOperator::Eq),
            value: Some(AttributeValue::Single("Homo sapiens".to_string())),
            modifier: vec![],
        });

        let desc = describer.describe_concise(&query);

        assert!(desc.contains("genome size"));
        assert!(desc.contains("organism name"));
        assert!(desc.contains("and"));
    }

    #[test]
    fn describe_fallback_to_canonical_names() {
        let describer = QueryDescriber::new(HashMap::new());  // No field metadata
        let mut query = SearchQuery::default();
        query.attributes.attributes.push(Attribute {
            name: "assembly_level".to_string(),
            operator: Some(AttributeOperator::Eq),
            value: Some(AttributeValue::Single("chromosome".to_string())),
            modifier: vec![],
        });

        let desc = describer.describe_concise(&query);

        // Should convert "assembly_level" to "assembly level"
        assert!(desc.contains("assembly level"));
    }

    #[test]
    fn describe_with_sort() {
        let describer = sample_describer();
        let mut query = SearchQuery::default();
        query.parameters.sort_by = vec!["genome_size".to_string()];
        query.parameters.sort_order = SortOrder::Descending;

        let desc = describer.describe_concise(&query);

        assert!(desc.contains("genome size"));
        assert!(desc.contains("descending"));
    }

    #[test]
    fn describe_with_field_selections() {
        let describer = sample_describer();
        let mut query = SearchQuery::default();
        query.attributes.fields.push(Field {
            name: "organism_name".to_string(),
            modifier: vec![],
        });
        query.attributes.fields.push(Field {
            name: "genome_size".to_string(),
            modifier: vec![],
        });

        let desc = describer.describe_concise(&query);

        assert!(desc.contains("returning"));
        assert!(desc.contains("organism name"));
        assert!(desc.contains("genome size"));
    }
}
```

**File:** `src/core/codegen.rs` (in existing module)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_all_returns_nested_map_per_language() {
        let generator = CodeGenerator::new().unwrap();
        let mut site = sample_site();
        site.enabled_sdks = vec!["python".to_string()];
        let options = sample_options();
        let mut fields_by_index = HashMap::new();
        fields_by_index.insert("taxon".to_string(), sample_fields());

        let all_langs = generator.render_all(&site, &options, &fields_by_index).unwrap();

        assert!(all_langs.contains_key("python"));
        assert!(all_langs.get("python").unwrap().contains_key("python/testsite_sdk/query.py"));
    }

    #[test]
    fn template_name_to_dest_routes_python() {
        assert_eq!(
            template_name_to_dest("query.py", "python", "goat_sdk"),
            "python/goat_sdk/query.py"
        );
    }

    #[test]
    fn config_parses_enabled_sdks() {
        let yaml = r#"
name: testsite
display_name: Test Site
api_base: https://example.com/api
api_version: v2
enabled_sdks:
  - python
"#;
        let config: SiteConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.enabled_sdks, vec!["python"]);
    }

    #[test]
    fn backward_compat_regenerate_python_unmodified() {
        // Regenerate GOAT CLI and verify files match (diffs == 0)
        // This is a manual step, but can be automated in CI
    }
}
```

**File:** `tests/python/test_core.py`

```python
def test_query_builder_describe_concise() -> None:
    """QueryBuilder.describe() returns concise prose."""
    qb = (
        QueryBuilder("taxon")
        .add_attribute("genome_size", operator=">=", value="1000000000")
    )
    desc = qb.describe(mode="concise")

    assert "genome size" in desc or "genome_size" in desc
    assert ">=" in desc
    assert "1000000000" in desc or "1 gigabyte" in desc


def test_query_builder_describe_verbose() -> None:
    """QueryBuilder.describe(verbose=True) formats as bullet list."""
    qb = QueryBuilder("taxon").add_attribute("genome_size", operator=">=", value="1000000000")
    desc = qb.describe(mode="verbose")

    assert "Filters" in desc
    assert "•" in desc or "-" in desc


def test_snippet_generator_python() -> None:
    """SnippetGenerator renders Python code snippets."""
    from cli_generator import QuerySnapshot, SnippetGenerator

    gen = SnippetGenerator()
    query = QuerySnapshot(
        filters=[("genome_size", ">=", "1000000000")],
        sorts=[],
        flags=[],
        selections=[],
        traversal=None,
        summaries=[],
    )
    site_config = {...}  # minimal config

    snippet = gen.render_snippet(query, "python", site_config)

    assert "QueryBuilder" in snippet
    assert "add_attribute" in snippet
    assert "genome_size" in snippet
```

---

### Step 11: Update CI/Tests

**File:** `.github/workflows/ci.yml`

Add Phase 1 test steps:

```yaml
- name: Test Phase 1 infrastructure (multi-lang codegen)
  run: |
    cargo test codegen::tests::render_all_returns_nested_map_per_language
    cargo test describe::tests::describe_concise_simple_filter
    cargo test snippet::tests::snippet_generator_renders_python

- name: Test Python SDK integration
  run: |
    maturin develop --features extension-module
    python -m pytest tests/python/test_core.py::test_query_builder_describe_concise -v
```

---

### Step 12: Verification & Backward Compatibility

**Checklist:**

- [ ] Run all Rust tests (should be 100% pass)
- [ ] Run all Python tests (should be 100% pass)
- [ ] Regenerate GOAT CLI: `cli-generator new goat --output ../goat-test`
- [ ] Verify file diffs vs main branch are **zero** (structure unchanged)
- [ ] Run `black`, `isort`, `pyright` on generated Python code
- [ ] Verify Python SDK still imports and builds URLs correctly
- [ ] Test `--describe` CLI flag: `cli-generator search --describe`
- [ ] Test `.describe()` Python method: `qb.describe()` returns prose
- [ ] Verify snippet generation works: `SnippetGenerator::new().render_snippet()`

---

## Implementation Timeline

| Week      | Task                                     | Effort       |
| --------- | ---------------------------------------- | ------------ |
| 1a        | Reorganize templates, update config.rs   | 2 days       |
| 1b        | Implement describe.rs + tests            | 2 days       |
| 1c        | Implement snippet.rs + templates         | 1.5 days     |
| 2a        | Refactor codegen.rs for multi-lang       | 2.5 days     |
| 2b        | Update commands, CLI flag, Python SDK    | 1.5 days     |
| 2c        | Integration tests, backward compat check | 1 day        |
| 2d        | **Buffer for refinement & edge cases**   | 1 day        |
| **Total** | **Phase 1 Complete**                     | **~12 days** |

---

## Deliverables

After Phase 1:

1. ✅ **Codegen infrastructure** handles multiple languages (Python now, R in Phase 2)
2. ✅ **QueryDescriber API** in Rust (with prose + structured output)
3. ✅ **Python SDK** enhanced with `.describe()` method
4. ✅ **CLI** supports `--describe` flag
5. ✅ **SnippetGenerator** framework ready (Python snippet included)
6. ✅ **Full test coverage** (codegen, describe, snippet, backward compat)
7. ✅ **Documentation** in PREVIEW.md showing new features
8. ✅ **Zero behavioral changes** to existing Python SDK generation

---

## Risks & Mitigation

| Risk                                               | Mitigation                                                                  |
| -------------------------------------------------- | --------------------------------------------------------------------------- |
| Refactoring codegen breaks Python output           | Automated backward-compat test: regenerate GOAT/BOAT, diffs must be empty   |
| Config parsing fails with new `enabled_sdks` field | Test with YAML missing field (should default to `["python"]`)               |
| QueryDescriber doesn't handle all query types      | Extend tests incrementally; start with filters, add sorts, selections, etc. |
| Python FFI to Rust describe() fails                | Test with concrete examples; verify PyO3 serialization                      |
| Template reorganization misses file                | Comprehensive checklist; verify all 18 templates relocated                  |

---

## Next Steps (Post-Phase 1)

1. **Phase 2 planning:** R SDK templates, validation.R structure
2. **Snippet endpoint:** Express backend `POST /api/snippet` integration
3. **UI update:** Display descriptions + snippets side-by-side
4. **Performance:** Measure describe + snippet generation time (should be < 100ms each)

---

## References

- [multi-language-sdk-plan.md](multi-language-sdk-plan.md) — Full roadmap
- [docs/python-sdk-design.md](python-sdk-design.md) — Current SDK architecture
- `.github/workflows/ci.yml` — CI structure
- `sites/goat.yaml` — Example config (will get `enabled_sdks` + `field_display_names`)
