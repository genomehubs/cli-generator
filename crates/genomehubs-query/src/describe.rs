//! Human-readable descriptions of genomehubs queries.
//!
//! Generates concise and verbose descriptions of [`SearchQuery`] objects,
//! using field metadata from the API to produce user-friendly output.

use crate::query::{SearchIndex, SearchQuery};
use crate::types::FieldDef;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Generates human-readable descriptions of genomehubs queries.
pub struct QueryDescriber {
    /// Field metadata from API (includes canonical name and display name).
    field_metadata: HashMap<String, FieldDef>,
}

impl QueryDescriber {
    /// Create a new describer with field metadata from the API.
    pub fn new(field_metadata: HashMap<String, FieldDef>) -> Self {
        Self { field_metadata }
    }

    /// Get the display name for a field (prefers metadata, falls back to canonical name).
    fn display_name(&self, canonical_name: &str) -> String {
        self.field_metadata
            .get(canonical_name)
            .and_then(|field| field.display_name.clone())
            .unwrap_or_else(|| canonical_name.replace('_', " "))
    }

    /// Describe a query as structured components.
    pub fn describe_parts(&self, query: &SearchQuery) -> DescribedQuery {
        DescribedQuery {
            index: self.describe_index(query),
            taxa_filter: self.describe_taxa_filter(query),
            filters: self.describe_filters(query),
            sorts: vec![],
            selections: self.describe_selections(query),
        }
    }

    /// Describe a query in concise prose form.
    pub fn describe_concise(&self, query: &SearchQuery) -> String {
        let parts = self.describe_parts(query);
        self.assemble_prose(&parts, false)
    }

    /// Describe a query in verbose prose form.
    pub fn describe_verbose(&self, query: &SearchQuery) -> String {
        let parts = self.describe_parts(query);
        self.assemble_prose(&parts, true)
    }

    fn describe_index(&self, query: &SearchQuery) -> String {
        match &query.index {
            SearchIndex::Taxon => "taxa".to_string(),
            SearchIndex::Assembly => "assemblies".to_string(),
            SearchIndex::Sample => "samples".to_string(),
        }
    }

    fn describe_taxa_filter(&self, query: &SearchQuery) -> Option<String> {
        query.identifiers.taxa.as_ref().map(|taxa_filter| {
            let names = taxa_filter.names.join(", ");
            let mode = match taxa_filter.filter_type {
                crate::query::TaxonFilterType::Name => "direct matches only",
                crate::query::TaxonFilterType::Tree => "including all descendants",
                crate::query::TaxonFilterType::Lineage => "including all ancestors",
            };
            format!("{} ({})", names, mode)
        })
    }

    fn describe_filters(&self, query: &SearchQuery) -> Vec<FilterDescription> {
        query
            .attributes
            .attributes
            .iter()
            .filter_map(|attr| {
                let field_display = self.display_name(&attr.name);
                match (&attr.operator, &attr.value) {
                    (Some(op), Some(value)) => {
                        let op_symbol = format!("{:?}", op).to_lowercase();
                        let values_str = match value {
                            crate::query::AttributeValue::Single(s) => s.clone(),
                            crate::query::AttributeValue::List(list) => list.join(", "),
                        };
                        Some(FilterDescription {
                            field: field_display.clone(),
                            operator: op_symbol.clone(),
                            value: values_str.clone(),
                            concise: format!("{} {} {}", field_display, op_symbol, values_str),
                        })
                    }
                    _ => None,
                }
            })
            .collect()
    }

    fn describe_selections(&self, query: &SearchQuery) -> Vec<String> {
        query
            .attributes
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
        let mut prose = format!("Search for {} in the database", parts.index);

        if let Some(ref taxa) = parts.taxa_filter {
            prose.push_str(&format!(" in {} ({})", parts.index, taxa));
        }

        if !parts.filters.is_empty() {
            let filter_strs: Vec<_> = parts.filters.iter().map(|f| f.concise.clone()).collect();
            prose.push_str(&format!(", filtered to {}", filter_strs.join(" and ")));
        }

        if !parts.selections.is_empty() {
            prose.push_str(&format!(", returning {}", parts.selections.join(", ")));
        }

        prose.push('.');
        prose
    }

    fn assemble_verbose(&self, parts: &DescribedQuery) -> String {
        let mut prose = format!("Search for {}", parts.index);

        if let Some(ref taxa) = parts.taxa_filter {
            prose.push_str(&format!(" in {} ({})", parts.index, taxa));
        }
        prose.push('.');

        if !parts.filters.is_empty() {
            prose.push_str("\n\nFilters applied:\n");
            for filter in &parts.filters {
                prose.push_str(&format!(
                    "  • {} {} {}\n",
                    filter.field, filter.operator, filter.value
                ));
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

/// Structured description of a query (can be formatted multiple ways).
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_name() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "genome_size".to_string(),
            FieldDef {
                name: "genome_size".to_string(),
                display_name: Some("Genome size".to_string()),
                ..Default::default()
            },
        );
        let describer = QueryDescriber::new(metadata);
        assert_eq!(describer.display_name("genome_size"), "Genome size");
        assert_eq!(describer.display_name("unknown_field"), "unknown field");
    }
}
