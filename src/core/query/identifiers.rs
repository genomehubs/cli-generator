//! Identifier types for [`SearchQuery`](super::SearchQuery).
//!
//! Covers taxa, assemblies, and samples with rank and filter-type selection.

use serde::{Deserialize, Serialize};

// ── Identifiers ───────────────────────────────────────────────────────────────

/// Taxon, assembly, and sample identifiers for a query.
///
/// Corresponds to the `process_identifiers` artifact in the GoaT MCP server.
///
/// Taxa support a `"!"` prefix for NOT filters, e.g. `"!Felis"` excludes that
/// taxon from the query.  Wildcards (`*`) are supported at the start/end of
/// identifiers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Identifiers {
    /// Scientific taxon names or IDs.  `"!"` prefix = NOT filter.
    #[serde(default)]
    pub taxa: Vec<String>,
    /// Assembly accession IDs (e.g. `"GCF_000002305.6"`).
    #[serde(default)]
    pub assemblies: Vec<String>,
    /// Sample accession IDs (e.g. `"SRR1234567"`).
    #[serde(default)]
    pub samples: Vec<String>,
    /// Taxonomic rank for filtering results (maps to `tax_rank(X)` in the query).
    ///
    /// Use `--rank` on the CLI (gap-analysis item 4 — distinct from `--ranks`
    /// which selects rank columns to return).
    #[serde(default)]
    pub rank: Option<String>,
    /// Controls which API taxon wrapper function is used.
    #[serde(default)]
    pub taxon_filter_type: TaxonFilterType,
}

// ── TaxonFilterType ───────────────────────────────────────────────────────────

/// Controls which API taxon wrapper function wraps each taxon name.
///
/// | Variant   | API function     | CLI `--taxon-type` | mcp-server value |
/// |-----------|------------------|-------------------|------------------|
/// | `Name`    | `tax_name(X)`    | `name` (default)  | `matching`       |
/// | `Tree`    | `tax_tree(X)`    | `tree`            | `children`       |
/// | `Lineage` | `tax_lineage(X)` | `lineage`         | `lineage`        |
///
/// The old goat-cli `--descendants` and `--lineage` flags are deprecated in
/// favour of `--taxon-type tree` and `--taxon-type lineage` (gap-analysis item 1).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaxonFilterType {
    /// `tax_name(X)` — exact name match (default).
    #[default]
    Name,
    /// `tax_tree(X)` — all descendants.
    Tree,
    /// `tax_lineage(X)` — all ancestors.
    Lineage,
}

impl TaxonFilterType {
    /// Return the raw API function name for use in query fragment building.
    pub fn api_function(&self) -> &'static str {
        match self {
            TaxonFilterType::Name => "tax_name",
            TaxonFilterType::Tree => "tax_tree",
            TaxonFilterType::Lineage => "tax_lineage",
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taxon_filter_type_api_functions() {
        assert_eq!(TaxonFilterType::Name.api_function(), "tax_name");
        assert_eq!(TaxonFilterType::Tree.api_function(), "tax_tree");
        assert_eq!(TaxonFilterType::Lineage.api_function(), "tax_lineage");
    }

    #[test]
    fn taxon_filter_type_deserialises_from_yaml() {
        let identifiers: Identifiers =
            serde_yaml::from_str("taxa: [Mammalia]\ntaxon_filter_type: tree").unwrap();
        assert_eq!(identifiers.taxon_filter_type, TaxonFilterType::Tree);
    }

    #[test]
    fn identifiers_default_is_empty() {
        let id = Identifiers::default();
        assert!(id.taxa.is_empty());
        assert!(id.assemblies.is_empty());
        assert!(id.samples.is_empty());
        assert!(id.rank.is_none());
        assert_eq!(id.taxon_filter_type, TaxonFilterType::Name);
    }
}
