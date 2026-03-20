//! Human-readable descriptions of genomehubs queries.
//! This module provides functionality to generate concise and verbose descriptions of
//! `SearchQuery` objects, using field metadata from the API to produce user-friendly output. The descriptions can
//! be structured as components (index, filters, sorts, selections) or assembled into prose form for display in CLI help messages

use crate::core::fetch::FieldDef;
use crate::core::query::{AttributeSet, Identifiers, SearchIndex, SearchQuery};
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
            index: self.describe_index(query),
            taxa_filter: self.describe_taxa_filter(&query.identifiers),
            filters: self.describe_filters(&query.attributes),
            sorts: vec![], // Sorts not yet implemented in SearchQuery, so leaving empty for now
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

    fn describe_index(&self, query: &SearchQuery) -> String {
        match &query.index {
            SearchIndex::Taxon => "taxa".to_string(),
            SearchIndex::Assembly => "assemblies".to_string(),
            SearchIndex::Sample => "samples".to_string(),
        }
    }

    fn describe_taxa_filter(&self, identifiers: &Identifiers) -> Option<String> {
        identifiers.taxa.as_ref().map(|taxa_filter| {
            let names = taxa_filter.names.join(", ");
            let mode = match taxa_filter.filter_type {
                crate::core::query::TaxonFilterType::Name => "direct matches only",
                crate::core::query::TaxonFilterType::Tree => {
                    "including all descendants in the taxonomy tree"
                }
                crate::core::query::TaxonFilterType::Lineage => {
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
                            field: field_display.clone(),
                            operator: op_symbol.to_string(),
                            value: values_str.clone(),
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

    // TODO: Enable once SearchQuery has parameters field with sort_by/sort_order
    // fn describe_sorts(&self, params: &QueryParams) -> Vec<SortDescription> {
    //     params
    //         .sort_by
    //         .iter()
    //         .map(|field| {
    //             let field_display = self.display_name(field);
    //             let direction = match params.sort_order {
    //                 crate::core::query::SortOrder::Asc => "ascending",
    //                 crate::core::query::SortOrder::Desc => "descending",
    //             };
    //             SortDescription {
    //                 field: field_display.clone(),
    //                 direction: direction.to_string(),
    //                 concise: format!("{} ({})", field_display, direction),
    //             }
    //         })
    //         .collect()
    // }

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
                prose.push_str(&format!(
                    "  • {} {} {}\n",
                    filter.field, filter.operator, filter.value
                ));
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
