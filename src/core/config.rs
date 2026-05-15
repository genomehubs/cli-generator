//! YAML configuration types for site and CLI options.
//!
//! Two top-level config files drive generation:
//!
//! - **`site.yaml`** — describes a site: API base URL, index list, compat flags.
//! - **`cli-options.yaml`** — maps CLI flag names to `display_group` values so
//!   users can type `--genome-size` instead of enumerating field names.
//!
//! Both are bundled in the generator's `sites/` directory as canonical defaults
//! and are copied into each generated repo's `config/` directory where they can
//! be overridden without rebuilding the generator.

use std::{collections::HashMap, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

// ── SiteConfig ────────────────────────────────────────────────────────────────

/// Top-level site configuration loaded from `site.yaml`.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct SiteConfig {
    /// Short identifier used in file names and cache paths, e.g. `"goat"`.
    pub name: String,
    /// Human-readable name, e.g. `"Genomes on a Tree (GoaT)"`.
    pub display_name: String,
    /// Base URL of the API *without* a trailing slash,
    /// e.g. `"https://goat.genomehubs.org/api"`.
    pub api_base: String,
    /// API version path component, e.g. `"v2"`.
    #[serde(default = "default_api_version")]
    pub api_version: String,
    /// Ordered list of index definitions available on this site.
    pub indexes: Vec<IndexDef>,
    /// Optional compatibility flags for this site.
    #[serde(default)]
    pub compat: CompatConfig,
    /// When `true` this site points to a frozen archive API (e.g.
    /// `api_version: "2025.04.21"`).  Field definitions are cached
    /// indefinitely — the schema of an archived API never changes.
    #[serde(default)]
    pub archive: bool,
    /// Validation rules for query parameters.  Centralised here so custom
    /// GoaT instances (and BoaT) can override without touching generated code.
    #[serde(default)]
    pub validation: ValidationConfig,
    /// Python package name for the generated SDK, e.g. `"goat_sdk"`.
    /// Defaults to `"{name}_sdk"` when absent from the YAML.
    #[serde(default)]
    pub sdk_name: Option<String>,
    /// Which SDK languages to generate (defaults to ["python"]).
    #[serde(default = "default_enabled_sdks")]
    pub enabled_sdks: Vec<String>,
    /// Base URL of the web UI without a trailing slash,
    /// e.g. `"https://goat.genomehubs.org"`.
    /// When absent, derived by stripping any trailing `/api` segment from `api_base`.
    #[serde(default)]
    pub ui_base: Option<String>,
    /// Curated recipes for the generated documentation site.
    ///
    /// When present, `cli-generator new` renders `docs/recipes/*.qmd` alongside
    /// the other Quarto pages and adds a *Recipes* menu to `_quarto.yml`.
    #[serde(default)]
    pub recipes: Option<RecipesConfig>,
    /// Optional notice shown at the top of the docs landing page.
    ///
    /// Renders as a Quarto `:::{.callout-note}` block.  Supports markdown.
    /// Typical use: `"This is a **v3 preview** — the API is subject to change."`
    #[serde(default)]
    pub notice_text: Option<String>,
}

fn default_api_version() -> String {
    "v2".to_string()
}

fn default_enabled_sdks() -> Vec<String> {
    vec!["python".to_string()]
}

impl SiteConfig {
    /// Load a [`SiteConfig`] from a YAML file on disk.
    pub fn from_file(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading site config: {}", path.display()))?;
        Self::parse_yaml(&text)
    }

    /// Parse a [`SiteConfig`] from a YAML string.
    pub fn parse_yaml(yaml: &str) -> Result<Self> {
        serde_yaml::from_str(yaml).context("parsing site config YAML")
    }

    /// Return the Python package name for the generated SDK.
    ///
    /// Uses `sdk_name` from the YAML when present; otherwise derives
    /// `"{name}_sdk"` (with hyphens replaced by underscores).
    pub fn resolved_sdk_name(&self) -> String {
        self.sdk_name
            .clone()
            .unwrap_or_else(|| format!("{}_sdk", self.name.replace('-', "_")))
    }

    /// Return the UI base URL for this site.
    ///
    /// Uses `ui_base` from the YAML when present; otherwise strips any trailing
    /// `/api` path segment from `api_base` as a sensible default.
    pub fn resolved_ui_base(&self) -> String {
        self.ui_base.clone().unwrap_or_else(|| {
            self.api_base
                .strip_suffix("/api")
                .unwrap_or(&self.api_base)
                .to_string()
        })
    }

    /// Return the `metadata/fields` endpoint URL for a given index.
    ///
    /// Uses the index's `result_fields_endpoint` override when present;
    /// otherwise falls back to `{api_base}/{api_version}/metadata/fields`.
    pub fn result_fields_url(&self, index: &IndexDef) -> String {
        if let Some(ref ep) = index.result_fields_endpoint {
            return ep.clone();
        }
        format!("{}/{}/metadata/fields", self.api_base, self.api_version)
    }
}

/// Definition of a single index within a site (e.g. `taxon`, `assembly`, `feature`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDef {
    /// Index name recognised by the API, e.g. `"taxon"`.
    pub name: String,
    /// Optional full URL override for the `metadata/fields` endpoint.
    ///
    /// When absent the standard `{api_base}/{api_version}/metadata/fields` URL
    /// is constructed by [`SiteConfig::result_fields_url`].
    #[serde(default)]
    pub result_fields_endpoint: Option<String>,
}

/// Compatibility flags that alter generated code.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompatConfig {
    /// Emit `#[clap(alias = "...")]` attributes so legacy `goat-cli` flag
    /// names are still accepted by the generated CLI.
    #[serde(default)]
    pub goat_cli: bool,
}

/// Validation rules for query parameters.
///
/// Centralised in `site.yaml` so custom GoaT instances (e.g. BoaT) can
/// override any list without touching generated Rust code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConfig {
    /// Valid accession ID prefixes for assembly queries (case-insensitive).
    #[serde(default = "default_assembly_prefixes")]
    pub assembly_accession_prefixes: Vec<String>,
    /// Valid accession ID prefixes for sample queries (case-insensitive).
    #[serde(default = "default_sample_prefixes")]
    pub sample_accession_prefixes: Vec<String>,
    /// Valid taxon name classes accepted by the `names` parameter.
    #[serde(default = "default_name_classes")]
    pub taxon_name_classes: Vec<String>,
    /// Valid values for the `taxon_filter_type` field.
    #[serde(default = "default_taxon_filter_types")]
    pub taxon_filter_types: Vec<String>,
}

fn default_assembly_prefixes() -> Vec<String> {
    ["GCA_", "GCF_", "GCS_", "GCN_", "GCP_", "GCR_", "WGS", "ASM"]
        .iter()
        .map(|s| s.to_lowercase())
        .collect()
}

fn default_sample_prefixes() -> Vec<String> {
    [
        "SRS", "SRR", "SRX", "SAM", "ERS", "ERP", "ERX", "DRR", "DRX", "SAMEA", "SAMEG",
    ]
    .iter()
    .map(|s| s.to_lowercase())
    .collect()
}

fn default_name_classes() -> Vec<String> {
    [
        "scientific_name",
        "common_name",
        "synonym",
        "tolid_prefix",
        "authority",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn default_taxon_filter_types() -> Vec<String> {
    ["name", "tree", "lineage"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            assembly_accession_prefixes: default_assembly_prefixes(),
            sample_accession_prefixes: default_sample_prefixes(),
            taxon_name_classes: default_name_classes(),
            taxon_filter_types: default_taxon_filter_types(),
        }
    }
}

// ── Recipe configuration ──────────────────────────────────────────────────────

/// Container for all recipe categories defined in a site YAML.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct RecipesConfig {
    /// Short, single-concept recipes rendered into `docs/recipes/simple.qmd`.
    #[serde(default)]
    pub simple: Vec<SimpleRecipe>,
    /// Multi-step recipes rendered into `docs/recipes/intermediate.qmd`.
    #[serde(default)]
    pub intermediate: Vec<IntermediateRecipe>,
    /// Report gallery entries rendered into `docs/recipes/reports.qmd`.
    #[serde(default)]
    pub reports: Vec<ReportRecipe>,
}

/// A single-concept recipe that maps directly to one SDK call.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct SimpleRecipe {
    /// Short title used as the section heading.
    pub title: String,
    /// URL-safe identifier, e.g. `missing_genome_size`.
    pub slug: String,
    /// One or two sentences describing what the recipe demonstrates.
    #[serde(default)]
    pub description: String,
    /// API index name, e.g. `"taxon"` or `"assembly"`.
    #[serde(default = "default_recipe_index")]
    pub index: String,
    /// Taxa to filter by, e.g. `["Mammalia"]`.
    #[serde(default)]
    pub taxa: Vec<String>,
    /// How the taxon list is applied: `"name"`, `"tree"`, or `"lineage"`.
    #[serde(default = "default_recipe_taxon_filter")]
    pub taxon_filter: String,
    /// Restrict results to this rank, e.g. `"species"`.
    #[serde(default)]
    pub rank: Option<String>,
    /// Attribute filters as `[field, operator, value]` triples.
    #[serde(default)]
    pub filters: Vec<Vec<String>>,
    /// Output fields passed to `add_field()`.
    #[serde(default)]
    pub fields: Vec<String>,
    /// Sort specification as `[field, direction]`, e.g. `[genome_size, desc]`.
    #[serde(default)]
    pub sort: Vec<String>,
    /// API call type: `"search"` (default), `"count"`, `"report"`, or `"positional"`.
    #[serde(default = "default_call_type")]
    pub call_type: String,
}

fn default_recipe_index() -> String {
    "taxon".to_string()
}

fn default_recipe_taxon_filter() -> String {
    "name".to_string()
}

fn default_call_type() -> String {
    "search".to_string()
}

/// A free-form, per-language code block for use inside recipe steps.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct LanguageCodeBlock {
    #[serde(default)]
    pub python: Option<String>,
    #[serde(default)]
    pub r: Option<String>,
    #[serde(default)]
    pub javascript: Option<String>,
    #[serde(default)]
    pub cli: Option<String>,
}

/// One step inside a multi-step intermediate recipe.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct IntermediateStep {
    /// Short title rendered as a sub-heading.
    pub title: String,
    /// Optional narrative paragraph shown before the code block.
    #[serde(default)]
    pub prose: Option<String>,
    /// When present, snippets are auto-generated from this query spec.
    #[serde(default)]
    pub query: Option<SimpleRecipe>,
    /// When present, these verbatim code blocks are shown instead of (or
    /// alongside) generated snippets.  Useful for DataFrame operations, plots,
    /// and other post-processing steps that cannot be expressed as a query.
    #[serde(default)]
    pub code: Option<LanguageCodeBlock>,
}

/// A multi-step recipe with narrative prose and mixed query/code steps.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct IntermediateRecipe {
    /// Short title used as the section heading.
    pub title: String,
    /// URL-safe identifier, e.g. `vertebrate_comparison`.
    pub slug: String,
    /// One or two sentences describing the overall goal.
    #[serde(default)]
    pub description: String,
    /// Ordered list of steps.
    #[serde(default)]
    pub steps: Vec<IntermediateStep>,
}

/// A report gallery entry with a single report configuration.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ReportRecipe {
    /// Short title used as the section heading.
    pub title: String,
    /// URL-safe identifier, e.g. `mammalia_genome_size_histogram`.
    pub slug: String,
    /// One or two sentences describing what the report shows.
    #[serde(default)]
    pub description: String,
    /// The report configuration to render.
    pub report: ReportSpec,
}

/// Inline report specification used inside [`ReportRecipe`].
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ReportSpec {
    /// Report type: `"histogram"`, `"scatter"`, `"tree"`, `"map"`,
    /// `"xPerRank"`, `"sources"`, `"arc"`, etc.
    pub report_type: String,
    /// API index, e.g. `"taxon"`.
    #[serde(default = "default_recipe_index")]
    pub index: String,
    /// Taxa to filter by.
    #[serde(default)]
    pub taxa: Vec<String>,
    /// How the taxon list is applied.
    #[serde(default = "default_recipe_taxon_filter")]
    pub taxon_filter: String,
    /// Restrict to this rank.
    #[serde(default)]
    pub rank: Option<String>,
    /// Attribute filters as `[field, operator, value]` triples.
    #[serde(default)]
    pub filters: Vec<Vec<String>>,
    /// Primary x-axis field.
    #[serde(default)]
    pub x: Option<String>,
    /// Secondary y-axis field (scatter, tree).
    #[serde(default)]
    pub y: Option<String>,
    /// Category/colour field.
    #[serde(default)]
    pub cat: Option<String>,
}

// ── CliOptionsConfig ──────────────────────────────────────────────────────────

/// Top-level CLI options configuration loaded from `cli-options.yaml`.
///
/// Maps index names to their ordered flag group definitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliOptionsConfig {
    /// Per-index flag group definitions, keyed by index name.
    pub indexes: HashMap<String, IndexOptions>,
}

impl CliOptionsConfig {
    /// Load a [`CliOptionsConfig`] from a YAML file on disk.
    pub fn from_file(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading CLI options config: {}", path.display()))?;
        Self::parse_yaml(&text)
    }

    /// Parse a [`CliOptionsConfig`] from a YAML string.
    pub fn parse_yaml(yaml: &str) -> Result<Self> {
        serde_yaml::from_str(yaml).context("parsing CLI options config YAML")
    }
}

/// Ordered flag group definitions for a single index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexOptions {
    /// Each entry becomes one boolean `--flag` on the generated CLI.
    pub field_groups: Vec<FieldGroup>,
}

/// A single field group entry that selects one or more fields.
///
/// Fields are resolved at code-generation time from three sources (all
/// optional, all additive, duplicates are dropped):
///
/// * `display_groups` — all fields whose API `display_group` matches.
/// * `fields` — explicit field names included unconditionally.
/// * `patterns` — glob-style patterns matched against field names:
///   `"busco_*"` matches any name starting with `busco_`;
///   `"*_date"` matches any name ending with `_date`;
///   `"*busco*"` matches any name containing `busco`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldGroup {
    /// Group name used in `--field-groups`, e.g. `"genome-size"`.
    pub flag: String,
    /// Short help text shown by `--list-field-groups`.
    pub description: String,
    /// Optional single-character short code accepted within `--field-groups`,
    /// e.g. `G` for `genome-size`.  Must be unique within an index.
    #[serde(default)]
    pub short: Option<String>,
    /// API `display_group` values whose fields are included by this group.
    #[serde(default)]
    pub display_groups: Vec<String>,
    /// Explicit field names to include regardless of display group.
    #[serde(default)]
    pub fields: Vec<String>,
    /// Glob patterns matched against field names at code-generation time.
    /// Supports `prefix*`, `*suffix`, and `*contains*` forms.
    #[serde(default)]
    pub patterns: Vec<String>,
    /// Legacy flag aliases emitted when `compat.goat_cli` is enabled.
    #[serde(default)]
    pub compat_aliases: Vec<String>,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_SITE_YAML: &str = r#"
name: test
display_name: Test Site
api_base: "https://example.com/api"
api_version: "v2"
indexes:
  - name: taxon
  - name: assembly
"#;

    const MINIMAL_OPTS_YAML: &str = r#"
indexes:
  taxon:
    field_groups:
      - flag: genome-size
        description: "Genome size fields"
        display_groups: [genome_size]
        compat_aliases: [gs]
"#;

    #[test]
    fn site_config_parses_minimal_yaml() {
        let cfg = SiteConfig::parse_yaml(MINIMAL_SITE_YAML).unwrap();
        assert_eq!(cfg.name, "test");
        assert_eq!(cfg.indexes.len(), 2);
        assert_eq!(cfg.indexes[0].name, "taxon");
        assert!(!cfg.compat.goat_cli);
    }

    #[test]
    fn site_config_default_api_version_is_v2() {
        let yaml = "name: x\ndisplay_name: X\napi_base: http://x\nindexes: []";
        let cfg = SiteConfig::parse_yaml(yaml).unwrap();
        assert_eq!(cfg.api_version, "v2");
    }

    #[test]
    fn result_fields_url_uses_standard_pattern_by_default() {
        let cfg = SiteConfig::parse_yaml(MINIMAL_SITE_YAML).unwrap();
        let url = cfg.result_fields_url(&cfg.indexes[0]);
        assert_eq!(url, "https://example.com/api/v2/metadata/fields");
    }

    #[test]
    fn result_fields_url_respects_override() {
        let yaml = r#"
name: x
display_name: X
api_base: "https://example.com/api"
indexes:
  - name: feature
    result_fields_endpoint: "https://other.example.com/fields"
"#;
        let cfg = SiteConfig::parse_yaml(yaml).unwrap();
        let url = cfg.result_fields_url(&cfg.indexes[0]);
        assert_eq!(url, "https://other.example.com/fields");
    }

    #[test]
    fn cli_options_parses_field_group() {
        let opts = CliOptionsConfig::parse_yaml(MINIMAL_OPTS_YAML).unwrap();
        let taxon = opts.indexes.get("taxon").unwrap();
        assert_eq!(taxon.field_groups.len(), 1);
        let fg = &taxon.field_groups[0];
        assert_eq!(fg.flag, "genome-size");
        assert_eq!(fg.display_groups, ["genome_size"]);
        assert_eq!(fg.compat_aliases, ["gs"]);
    }
}
