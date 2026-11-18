//! Human-readable descriptions of genomehubs queries.
//!
//! This is the single describer for the workspace. The CLI, the PyO3 and WASM
//! bindings, and the copy embedded in generated SDKs all use it. It turns a
//! [`SearchQuery`] into concise or verbose English, naming fields from the
//! API field metadata.

use crate::query::{AttributeOperator, AttributeValue, SearchIndex, SearchQuery, TaxonFilterType};
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
            index: index_noun(&query.index).to_string(),
            taxa_filter: describe_taxa_filter(query),
            filters: self.describe_filters(query),
            selections: self.describe_selections(query),
        }
    }

    /// Describe a query in concise prose form.
    ///
    /// Example: `Search for taxa in Mammalia (including all descendants), filtered to Genome size >= 1G, returning Organism name.`
    pub fn describe_concise(&self, query: &SearchQuery) -> String {
        assemble_concise(&self.describe_parts(query))
    }

    /// Describe a query in verbose prose form, with one bullet per filter and field.
    pub fn describe_verbose(&self, query: &SearchQuery) -> String {
        assemble_verbose(&self.describe_parts(query))
    }

    /// Describe each attribute filter as a field, operator, and value.
    fn describe_filters(&self, query: &SearchQuery) -> Vec<FilterDescription> {
        query
            .attributes
            .attributes
            .iter()
            .filter_map(|attribute| {
                let (operator, value) =
                    filter_condition(attribute.operator.as_ref(), attribute.value.as_ref())?;
                Some(FilterDescription::new(
                    self.display_name(&attribute.name),
                    operator,
                    value,
                ))
            })
            .collect()
    }

    /// Display names for the fields requested as result columns.
    fn describe_selections(&self, query: &SearchQuery) -> Vec<String> {
        query
            .attributes
            .fields
            .iter()
            .map(|field| self.display_name(&field.name))
            .collect()
    }
}

/// Plural noun for a search index, used as the subject of the prose.
fn index_noun(index: &SearchIndex) -> &'static str {
    match index {
        SearchIndex::Taxon => "taxa",
        SearchIndex::Assembly => "assemblies",
        SearchIndex::Sample => "samples",
        SearchIndex::Feature => "features",
    }
}

/// Describe the taxa filter, e.g. `Mammalia (including all descendants)`.
fn describe_taxa_filter(query: &SearchQuery) -> Option<String> {
    let taxa = query.identifiers.taxa.as_ref()?;
    let mode = match taxa.filter_type {
        TaxonFilterType::Name => "direct matches only",
        TaxonFilterType::Tree => "including all descendants",
        TaxonFilterType::Lineage => "including all ancestors",
    };
    Some(format!("{} ({})", taxa.names.join(", "), mode))
}

/// Map an attribute's operator and value to display text.
///
/// A missing operator is an existence test, matching the v3 YAML semantics.
/// A comparison without a value has no condition to describe, so it yields `None`.
fn filter_condition(
    operator: Option<&AttributeOperator>,
    value: Option<&AttributeValue>,
) -> Option<(String, String)> {
    match operator {
        None | Some(AttributeOperator::Exists) => Some(("exists".to_string(), String::new())),
        Some(AttributeOperator::Missing) => Some(("is missing".to_string(), String::new())),
        Some(comparison) => value.map(|attribute_value| {
            (
                comparison.as_str().to_string(),
                attribute_value.as_strs().join(", "),
            )
        }),
    }
}

/// Structured description of a query (can be formatted multiple ways).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescribedQuery {
    /// Plural noun for the searched index, e.g. `taxa`.
    pub index: String,
    /// Taxa named by the query with their match mode, if any.
    pub taxa_filter: Option<String>,
    /// One entry per attribute condition.
    pub filters: Vec<FilterDescription>,
    /// Display names of the returned fields.
    pub selections: Vec<String>,
}

/// One attribute condition, e.g. `Genome size >= 1G`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterDescription {
    /// Display name of the filtered field.
    pub field: String,
    /// Operator text, e.g. `>=`, `exists`, or `is missing`.
    pub operator: String,
    /// Compared value; empty for existence tests.
    pub value: String,
    /// Complete phrase, e.g. `Genome size >= 1G`.
    pub concise: String,
}

impl FilterDescription {
    /// Build a filter description and its concise phrase.
    fn new(field: String, operator: String, value: String) -> Self {
        let concise = if value.is_empty() {
            format!("{field} {operator}")
        } else {
            format!("{field} {operator} {value}")
        };
        Self {
            field,
            operator,
            value,
            concise,
        }
    }
}

/// Assemble the single-sentence concise form.
fn assemble_concise(parts: &DescribedQuery) -> String {
    let mut prose = format!("Search for {}", parts.index);
    if let Some(taxa) = &parts.taxa_filter {
        prose.push_str(&format!(" in {taxa}"));
    }
    if !parts.filters.is_empty() {
        let conditions: Vec<&str> = parts.filters.iter().map(|f| f.concise.as_str()).collect();
        prose.push_str(&format!(", filtered to {}", conditions.join(" and ")));
    }
    if !parts.selections.is_empty() {
        prose.push_str(&format!(", returning {}", parts.selections.join(", ")));
    }
    prose.push('.');
    prose
}

/// Assemble the multi-line verbose form with bulleted filters and fields.
fn assemble_verbose(parts: &DescribedQuery) -> String {
    let mut prose = format!("Search for {}", parts.index);
    if let Some(taxa) = &parts.taxa_filter {
        prose.push_str(&format!(" in {taxa}"));
    }
    prose.push('.');
    if !parts.filters.is_empty() {
        prose.push_str("\n\nFilters applied:\n");
        for filter in &parts.filters {
            prose.push_str(&format!("  • {}\n", filter.concise));
        }
    }
    if !parts.selections.is_empty() {
        prose.push_str("\nReturning fields:\n");
        for field in &parts.selections {
            prose.push_str(&format!("  • {field}\n"));
        }
    }
    prose
}

/// Parse a v3 `query_yaml` body and describe it with the given field metadata.
///
/// `verbose` selects the detailed form. Shared by the CLI and the PyO3 and WASM bindings.
///
/// # Errors
///
/// Returns the YAML error when `query_yaml` is not a valid `SearchQuery`.
pub fn describe_yaml(
    query_yaml: &str,
    field_metadata: HashMap<String, FieldDef>,
    verbose: bool,
) -> Result<String, serde_yaml::Error> {
    let query = SearchQuery::from_yaml(query_yaml)?;
    let describer = QueryDescriber::new(field_metadata);
    Ok(if verbose {
        describer.describe_verbose(&query)
    } else {
        describer.describe_concise(&query)
    })
}

/// Describe a report configuration as a short English phrase.
///
/// Parses a YAML string representing a report document (as produced by
/// `ReportBuilder.to_report_yaml()`) and returns a phrase suitable for
/// embedding in a combined query+report description, e.g.:
///
/// - `"a histogram of genome size by species rank"`
/// - `"a scatter plot of genome size vs c value"`
/// - `"a geographic distribution map"`
/// - `"a taxonomic tree at genus rank"`
///
/// Returns an empty string on YAML parse failure so callers can degrade
/// gracefully.
pub fn describe_report_yaml(report_yaml: &str) -> String {
    let doc: serde_yaml::Value = match serde_yaml::from_str(report_yaml) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };

    let report_type = doc
        .get("report")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let x = doc
        .get("x")
        .and_then(|v| v.as_str())
        .map(|s| s.replace('_', " "));
    let y = doc
        .get("y")
        .and_then(|v| v.as_str())
        .map(|s| s.replace('_', " "));
    let cat = doc
        .get("cat")
        .and_then(|v| v.as_str())
        .map(|s| s.replace('_', " "));
    let rank = doc
        .get("rank")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    match report_type.as_str() {
        "histogram" => {
            let x_label = x.as_deref().unwrap_or("values");
            let mut phrase = format!("a histogram of {}", x_label);
            if let Some(r) = &rank {
                phrase.push_str(&format!(" by {} rank", r));
            } else if let Some(c) = &cat {
                phrase.push_str(&format!(" by category {}", c));
            }
            phrase
        }
        "scatter" => {
            let x_label = x.as_deref().unwrap_or("x");
            let y_label = y.as_deref().unwrap_or("y");
            let mut phrase = format!("a scatter plot of {} vs {}", x_label, y_label);
            if let Some(c) = &cat {
                phrase.push_str(&format!(", coloured by {}", c));
            }
            phrase
        }
        "map" => {
            let location = doc
                .get("location_field")
                .and_then(|v| v.as_str())
                .map(|s| s.replace('_', " "));
            match location {
                Some(loc) if loc != "sample location" => {
                    format!("a geographic distribution map using {}", loc)
                }
                _ => "a geographic distribution map".to_string(),
            }
        }
        "tree" => {
            let mut phrase = "a taxonomic tree".to_string();
            if let Some(r) = &rank {
                phrase.push_str(&format!(" at {} rank", r));
            }
            phrase
        }
        "countPerRank" | "xPerRank" => {
            let x_label = x
                .as_deref()
                .or(doc.get("query").and_then(|v| v.as_str()))
                .unwrap_or("values");
            let mut phrase = format!("{} per rank", x_label.replace('_', " "));
            if !rank.as_deref().unwrap_or("").is_empty() {
                phrase.push_str(&format!(" (at {})", rank.as_deref().unwrap_or("")));
            }
            phrase
        }
        "sources" => "a data source summary".to_string(),
        "arc" | "arc2" => {
            let x_label = x.as_deref().unwrap_or("features");
            format!("an arc diagram of {}", x_label)
        }
        other if !other.is_empty() => format!("a {} report", other.replace('_', " ")),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata_with_display_names(entries: &[(&str, &str)]) -> HashMap<String, FieldDef> {
        entries
            .iter()
            .map(|(name, display)| {
                let field = FieldDef {
                    name: name.to_string(),
                    display_name: Some(display.to_string()),
                    ..Default::default()
                };
                (name.to_string(), field)
            })
            .collect()
    }

    fn sample_metadata() -> HashMap<String, FieldDef> {
        metadata_with_display_names(&[
            ("genome_size", "Genome size"),
            ("organism_name", "Organism name"),
            ("assembly_level", "Assembly level"),
        ])
    }

    #[test]
    fn test_display_name() {
        let describer = QueryDescriber::new(sample_metadata());
        assert_eq!(describer.display_name("genome_size"), "Genome size");
        assert_eq!(describer.display_name("unknown_field"), "unknown field");
    }

    #[test]
    fn concise_describes_taxa_comparison_and_fields() {
        let yaml = "index: taxon\n\
            taxa: [Mammalia]\n\
            taxon_filter_type: tree\n\
            attributes:\n  - name: genome_size\n    operator: ge\n    value: 1G\n\
            fields:\n  - organism_name\n";
        let prose = describe_yaml(yaml, sample_metadata(), false).unwrap();
        assert_eq!(
            prose,
            "Search for taxa in Mammalia (including all descendants), filtered to Genome size >= 1000000000, returning Organism name."
        );
    }

    #[test]
    fn exists_and_missing_have_no_value_text() {
        let yaml = "index: taxon\n\
            attributes:\n  - name: assembly_level\n    operator: exists\n  - name: genome_size\n    operator: missing\n";
        let prose = describe_yaml(yaml, sample_metadata(), false).unwrap();
        assert_eq!(
            prose,
            "Search for taxa, filtered to Assembly level exists and Genome size is missing."
        );
    }

    #[test]
    fn taxa_filter_modes_are_named() {
        let lineage = "index: taxon\ntaxa: [Mammalia]\ntaxon_filter_type: lineage\n";
        let direct = "index: taxon\ntaxa: [Mammalia]\n";
        let lineage_prose = describe_yaml(lineage, HashMap::new(), false).unwrap();
        let direct_prose = describe_yaml(direct, HashMap::new(), false).unwrap();
        assert!(lineage_prose.contains("(including all ancestors)"));
        assert!(direct_prose.contains("(direct matches only)"));
    }

    #[test]
    fn verbose_lists_filters_and_fields_as_bullets() {
        let yaml = "index: assembly\n\
            attributes:\n  - name: genome_size\n    operator: lt\n    value: 2G\n\
            fields:\n  - assembly_level\n";
        let prose = describe_yaml(yaml, sample_metadata(), true).unwrap();
        assert_eq!(
            prose,
            "Search for assemblies.\n\nFilters applied:\n  • Genome size < 2000000000\n\nReturning fields:\n  • Assembly level\n"
        );
    }

    #[test]
    fn invalid_yaml_is_an_error() {
        assert!(describe_yaml("index: [", HashMap::new(), false).is_err());
    }
}
