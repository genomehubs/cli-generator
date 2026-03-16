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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

fn default_api_version() -> String {
    "v2".to_string()
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

    /// Return the `resultFields` endpoint URL for a given index.
    ///
    /// Uses the index's `result_fields_endpoint` override when present;
    /// otherwise falls back to `{api_base}/{api_version}/resultFields`.
    pub fn result_fields_url(&self, index: &IndexDef) -> String {
        if let Some(ref ep) = index.result_fields_endpoint {
            return ep.clone();
        }
        format!("{}/{}/resultFields", self.api_base, self.api_version)
    }
}

/// Definition of a single index within a site (e.g. `taxon`, `assembly`, `feature`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDef {
    /// Index name recognised by the API, e.g. `"taxon"`.
    pub name: String,
    /// Optional full URL override for the `resultFields` endpoint.
    ///
    /// When absent the standard `{api_base}/{api_version}/resultFields` URL
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

/// A single CLI flag that selects one or more fields.
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
    /// Flag name without leading dashes, e.g. `"genome-size"`.
    pub flag: String,
    /// Short help text shown in `--help` output.
    pub description: String,
    /// API `display_group` values enabled by this flag.
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
        assert_eq!(url, "https://example.com/api/v2/resultFields");
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
