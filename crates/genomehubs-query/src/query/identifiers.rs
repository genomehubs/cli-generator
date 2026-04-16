//! Identifier types for [`SearchQuery`](super::SearchQuery).
//!
//! Covers taxa, assemblies, and samples with rank and filter-type selection.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ── Identifiers ───────────────────────────────────────────────────────────────

/// Taxon, assembly, and sample identifiers for a query.
///
/// Corresponds to the `process_identifiers` artifact in the GoaT MCP server.
///
/// Taxa support a `"!"` prefix for NOT filters, e.g. `"!Felis"` excludes that
/// taxon from the query.  Wildcards (`*`) are supported at the start/end of
/// identifiers.
#[derive(Debug, Clone, Default)]
pub struct Identifiers {
    /// Scientific taxon names or IDs.  `"!"` prefix = NOT filter.
    pub taxa: Option<TaxaIdentifier>,
    /// Assembly accession IDs (e.g. `"GCF_000002305.6"`).
    pub assemblies: Vec<String>,
    /// Sample accession IDs (e.g. `"SRR1234567"`).
    pub samples: Vec<String>,
    /// Taxonomic rank for filtering results (maps to `tax_rank(X)` in the query).
    ///
    /// Use `--rank` on the CLI (gap-analysis item 4 — distinct from `--ranks`
    /// which selects rank columns to return).
    pub rank: Option<String>,
}

impl<'de> Deserialize<'de> for Identifiers {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default)]
            taxa: Vec<String>,
            #[serde(default)]
            taxon_filter_type: Option<TaxonFilterType>,
            #[serde(default)]
            assemblies: Vec<String>,
            #[serde(default)]
            samples: Vec<String>,
            #[serde(default)]
            rank: Option<String>,
        }

        let raw = Raw::deserialize(deserializer)?;
        Ok(Identifiers {
            taxa: if raw.taxa.is_empty() {
                None
            } else {
                Some(TaxaIdentifier {
                    names: raw.taxa,
                    filter_type: raw.taxon_filter_type.unwrap_or_default(),
                })
            },
            assemblies: raw.assemblies,
            samples: raw.samples,
            rank: raw.rank,
        })
    }
}

impl Serialize for Identifiers {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;

        if let Some(taxa) = &self.taxa {
            map.serialize_entry("taxa", &taxa.names)?;
            map.serialize_entry("taxon_filter_type", &taxa.filter_type)?;
        }

        if !self.assemblies.is_empty() {
            map.serialize_entry("assemblies", &self.assemblies)?;
        }
        if !self.samples.is_empty() {
            map.serialize_entry("samples", &self.samples)?;
        }
        if let Some(rank) = &self.rank {
            map.serialize_entry("rank", rank)?;
        }

        map.end()
    }
}

/// Taxon names with their filter strategy.
/// This struct is used within `Identifiers` to specify how taxon names should be filtered in the query (e.g., direct matches, including descendants, including ancestors).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxaIdentifier {
    pub names: Vec<String>,
    pub filter_type: TaxonFilterType,
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
        let taxa = identifiers.taxa.expect("taxa should be Some");
        assert_eq!(taxa.names, vec!["Mammalia"]);
        assert_eq!(taxa.filter_type, TaxonFilterType::Tree);
    }

    #[test]
    fn identifiers_default_is_empty() {
        let id = Identifiers::default();
        assert!(id.taxa.is_none());
        assert!(id.assemblies.is_empty());
        assert!(id.samples.is_empty());
        assert!(id.rank.is_none());
    }

    #[test]
    fn taxa_identifier_default_filter_type_is_name() {
        let taxa = TaxaIdentifier {
            names: vec!["Felis".to_string()],
            filter_type: TaxonFilterType::default(),
        };
        assert_eq!(taxa.filter_type, TaxonFilterType::Name);
    }
}
