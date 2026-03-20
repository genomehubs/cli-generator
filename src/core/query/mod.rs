//! Query builder for genomehubs search APIs.
//!
//! Provides a serde-serialisable [`SearchQuery`] / [`QueryParams`] pair that
//! maps 1-to-1 with the `process_identifiers` + `process_attributes` +
//! `submit_query` pipeline in the GoaT MCP server, and a pure [`build_query_url`]
//! function that converts them into a fully-encoded API URL.
//!
//! Submodules:
//! - [`identifiers`] — [`Identifiers`] and [`TaxonFilterType`]
//! - [`attributes`]  — [`Attribute`], [`AttributeSet`], [`Field`], operators, modifiers
//! - [`url`]         — the [`build_query_url`] function and encoding helpers
//! - [`validation`]  — static validation using generated `phf::Map` tables

pub mod attributes;
pub mod identifiers;
pub mod url;
pub mod validation;

pub use attributes::{Attribute, AttributeOperator, AttributeSet, AttributeValue, Field, Modifier};
pub use identifiers::{Identifiers, TaxonFilterType};
pub use url::build_query_url;

use serde::{Deserialize, Serialize};

// ── SearchQuery ───────────────────────────────────────────────────────────────

/// Top-level query describing *what* to search for.
///
/// Combines the `process_identifiers` and `process_attributes` artifacts from
/// the GoaT MCP server into a single serde-serialisable struct.
///
/// Load from YAML with [`SearchQuery::from_yaml`]; build a URL with
/// [`build_query_url`].
///
/// # Example YAML
/// ```yaml
/// index: taxon
/// taxa: [Mammalia, "!Felis"]
/// rank: species
/// taxon_filter_type: tree
/// attributes:
///   - name: genome_size
///     operator: lt
///     value: "3G"
///     modifier: [min, direct]
/// fields:
///   - name: genome_size
///     modifier: [min]
/// names: [scientific_name]
/// ranks: [genus]
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    /// Which index to search.
    pub index: SearchIndex,
    /// Taxon, assembly, and sample identifiers with rank and filter type.
    #[serde(flatten)]
    pub identifiers: Identifiers,
    /// Attribute filters, return fields, name classes, and rank columns.
    #[serde(flatten)]
    pub attributes: AttributeSet,
}

impl SearchQuery {
    /// Parse a [`SearchQuery`] from a YAML string.
    pub fn from_yaml(yaml: &str) -> anyhow::Result<Self> {
        serde_yaml::from_str(yaml).map_err(|e| anyhow::anyhow!("parsing SearchQuery YAML: {e}"))
    }

    /// Serialise this [`SearchQuery`] to a YAML string.
    pub fn to_yaml(&self) -> anyhow::Result<String> {
        serde_yaml::to_string(self).map_err(|e| anyhow::anyhow!("serialising SearchQuery: {e}"))
    }
}

/// Which API search index to query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SearchIndex {
    Taxon,
    Assembly,
    Sample,
}

// ── QueryParams ───────────────────────────────────────────────────────────────

/// Execution parameters describing *how* to fetch and present results.
///
/// Separate from [`SearchQuery`] so the same query can be issued as
/// `count` / `search` / `report` with different pagination and formatting.
/// Corresponds to the `submit_query` parameters in the GoaT MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryParams {
    /// Maximum records per page; maps to `&size=` (default 10).
    #[serde(default = "default_size")]
    pub size: usize,
    /// 1-based page number; `offset = (page - 1) * size`.
    #[serde(default = "default_page")]
    pub page: usize,
    /// Field to sort results by.
    #[serde(default)]
    pub sort_by: Option<String>,
    /// Sort direction (default ascending).
    #[serde(default)]
    pub sort_order: SortOrder,
    /// Include ancestrally estimated values (`&includeEstimates=true`).
    ///
    /// Defaults to `true` to match the API default and MCP server behaviour.
    /// Corresponds to `--include-estimates` CLI flag (gap-analysis item 5).
    #[serde(default = "default_true")]
    pub include_estimates: bool,
    /// Request tidy (long) format via `&summaryValues=false`.
    ///
    /// Prefers the API's native tidy format over any client-side pivot
    /// (gap-analysis item 11).
    #[serde(default)]
    pub tidy: bool,
    /// Taxonomy backbone; defaults to `"ncbi"`.
    ///
    /// Site-level override is held in `SiteConfig`; only surfaced as a
    /// user-facing flag when a site uses a different taxonomy backbone.
    #[serde(default = "default_taxonomy")]
    pub taxonomy: String,
}

impl Default for QueryParams {
    fn default() -> Self {
        Self {
            size: default_size(),
            page: default_page(),
            sort_by: None,
            sort_order: SortOrder::default(),
            include_estimates: true,
            tidy: false,
            taxonomy: default_taxonomy(),
        }
    }
}

/// Sort direction for search results.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

impl QueryParams {
    /// Parse a [`QueryParams`] from a YAML string.
    pub fn from_yaml(yaml: &str) -> anyhow::Result<Self> {
        serde_yaml::from_str(yaml).map_err(|e| anyhow::anyhow!("parsing QueryParams YAML: {e}"))
    }
}

fn default_size() -> usize {
    10
}
fn default_page() -> usize {
    1
}
fn default_true() -> bool {
    true
}
fn default_taxonomy() -> String {
    "ncbi".to_string()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_query_round_trips_yaml() {
        let yaml = r#"
index: taxon
taxa:
  - Mammalia
  - "!Felis"
rank: species
taxon_filter_type: tree
attributes:
  - name: genome_size
    operator: lt
    value: "3000000000"
    modifier: [min, direct]
fields:
  - name: genome_size
    modifier: [min]
names: [scientific_name]
ranks: [genus]
"#;
        let query: SearchQuery = serde_yaml::from_str(yaml).expect("parse YAML");
        assert_eq!(query.index, SearchIndex::Taxon);
        let taxa = query
            .identifiers
            .taxa
            .as_ref()
            .expect("taxa should be Some");
        assert_eq!(taxa.names, vec!["Mammalia", "!Felis"]);
        assert_eq!(query.identifiers.rank, Some("species".to_string()));
        assert_eq!(taxa.filter_type, TaxonFilterType::Tree);
        assert_eq!(query.attributes.attributes.len(), 1);
        assert_eq!(query.attributes.fields.len(), 1);
        assert_eq!(query.attributes.names, vec!["scientific_name"]);
        assert_eq!(query.attributes.ranks, vec!["genus"]);
    }

    #[test]
    fn query_params_defaults_match_mcp_server() {
        let params = QueryParams::default();
        assert_eq!(params.size, 10);
        assert_eq!(params.page, 1);
        assert!(params.include_estimates);
        assert!(!params.tidy);
        assert_eq!(params.taxonomy, "ncbi");
        assert_eq!(params.sort_order, SortOrder::Asc);
    }

    #[test]
    fn search_index_taxon() {
        assert_eq!(SearchIndex::Taxon, SearchIndex::Taxon);
    }

    #[test]
    fn search_index_assembly() {
        assert_eq!(SearchIndex::Assembly, SearchIndex::Assembly);
    }

    #[test]
    fn search_index_sample() {
        assert_eq!(SearchIndex::Sample, SearchIndex::Sample);
    }

    #[test]
    fn search_query_from_yaml_single_index() {
        let yaml = r#"
index: assembly
taxa: []
"#;
        let query = SearchQuery::from_yaml(yaml).expect("parse");
        assert_eq!(query.index, SearchIndex::Assembly);
    }

    #[test]
    fn search_query_from_yaml_with_error() {
        let yaml = "invalid: {yaml: [";
        let result = SearchQuery::from_yaml(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn search_query_to_yaml() {
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers::default(),
            attributes: AttributeSet::default(),
        };
        let yaml = query.to_yaml().expect("serialize");
        assert!(yaml.contains("taxon"));
    }

    #[test]
    fn search_query_assembly_index() {
        let query = SearchQuery {
            index: SearchIndex::Assembly,
            identifiers: Identifiers::default(),
            attributes: AttributeSet::default(),
        };
        assert_eq!(query.index, SearchIndex::Assembly);
    }

    #[test]
    fn sort_order_ascending_is_default() {
        let order = SortOrder::default();
        assert_eq!(order, SortOrder::Asc);
    }

    #[test]
    fn sort_order_descending_exists() {
        let order = SortOrder::Desc;
        assert_eq!(order, SortOrder::Desc);
    }

    #[test]
    fn query_params_with_custom_size() {
        let params = QueryParams {
            size: 100,
            ..Default::default()
        };
        assert_eq!(params.size, 100);
        assert_eq!(params.page, 1);
    }

    #[test]
    fn query_params_with_custom_page() {
        let params = QueryParams {
            page: 5,
            ..Default::default()
        };
        assert_eq!(params.page, 5);
        assert_eq!(params.size, 10);
    }

    #[test]
    fn query_params_with_tidy_true() {
        let params = QueryParams {
            tidy: true,
            ..Default::default()
        };
        assert!(params.tidy);
    }

    #[test]
    fn query_params_with_custom_taxonomy() {
        let params = QueryParams {
            taxonomy: "ott".to_string(),
            ..Default::default()
        };
        assert_eq!(params.taxonomy, "ott");
    }

    #[test]
    fn query_params_with_sort_by() {
        let params = QueryParams {
            sort_by: Some("genome_size".to_string()),
            ..Default::default()
        };
        assert_eq!(params.sort_by, Some("genome_size".to_string()));
    }

    #[test]
    fn query_params_with_sort_order_desc() {
        let params = QueryParams {
            sort_order: SortOrder::Desc,
            ..Default::default()
        };
        assert_eq!(params.sort_order, SortOrder::Desc);
    }

    #[test]
    fn query_params_include_estimates_false() {
        let params = QueryParams {
            include_estimates: false,
            ..Default::default()
        };
        assert!(!params.include_estimates);
    }
}
