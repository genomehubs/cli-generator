//! Static attribute validation for [`SearchQuery`](super::SearchQuery).
//!
//! All validators are pure functions that borrow a `phf::Map` from the
//! generated `field_meta.rs` — no I/O, no async.
//!
//! ## Architecture
//!
//! Field metadata (operator legality, valid modifiers, enum constraints) is
//! fetched from the API once at `cli-generator update` time and baked into
//! `src/generated/field_meta.rs` as compile-time `phf::Map` instances.  At
//! runtime, the generated binary passes those maps here as borrows.
//!
//! For the Python SDK the validation step is optional: `build_query_url` builds
//! the URL regardless; callers that do have field metadata can call
//! [`validate_query`] separately before building.

use crate::core::config::ValidationConfig;
use crate::core::query::{
    attributes::{Attribute, AttributeOperator, AttributeSet, AttributeValue, Field, Modifier},
    identifiers::Identifiers,
    SearchQuery,
};

// ── FieldMeta ─────────────────────────────────────────────────────────────────

/// Metadata for a single API field, used for compile-time validation.
///
/// One instance per field; stored in the generated `*_FIELD_META` maps.
#[derive(Debug, Clone)]
pub struct FieldMeta {
    /// Processed type used for operator validation: `"long"`, `"double"`,
    /// `"keyword"`, `"date"`, etc.  Keyword fields do not support `<`/`>`.
    pub processed_type: &'static str,
    /// Direction of value inheritance across the tree: `"up"`, `"down"`,
    /// `"both"`, or `None` for non-inherited fields.
    pub traverse_direction: Option<&'static str>,
    /// Valid summary modifiers for this field (e.g. `["min", "max", "median"]`).
    pub summary: &'static [&'static str],
    /// Valid enum values for keyword fields; `None` for open-ended fields.
    pub constraint_enum: Option<&'static [&'static str]>,
}

// ── ValidationError ───────────────────────────────────────────────────────────

/// Structured error returned by validation functions.
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("unknown attribute '{name}' for index '{index}'")]
    UnknownAttribute { name: String, index: String },

    #[error(
        "operator '{op}' is not valid for keyword attribute '{name}' (only =, != are allowed)"
    )]
    InvalidOperatorForKeyword { name: String, op: String },

    #[error("value '{value}' is not valid for attribute '{name}'; allowed: {allowed:?}")]
    InvalidEnumValue {
        name: String,
        value: String,
        allowed: Vec<String>,
    },

    #[error("modifier '{modifier}' is not valid for attribute '{name}'; allowed: {allowed:?}")]
    InvalidModifier {
        name: String,
        modifier: String,
        allowed: Vec<String>,
    },

    #[error(
        "modifier 'ancestral' not valid for '{name}': traverse_direction is {direction:?}, \
         needs 'down' or 'both'"
    )]
    AncestralModifierNotSupported {
        name: String,
        direction: Option<String>,
    },

    #[error(
        "modifier 'descendant' not valid for '{name}': traverse_direction is {direction:?}, \
         needs 'up' or 'both'"
    )]
    DescendantModifierNotSupported {
        name: String,
        direction: Option<String>,
    },

    #[error("invalid assembly accession prefix '{value}'; expected one of {allowed:?}")]
    InvalidAssemblyPrefix { value: String, allowed: Vec<String> },

    #[error("invalid sample accession prefix '{value}'; expected one of {allowed:?}")]
    InvalidSamplePrefix { value: String, allowed: Vec<String> },

    #[error("invalid taxon name class '{value}'; allowed: {allowed:?}")]
    InvalidNameClass { value: String, allowed: Vec<String> },

    #[error("invalid taxon_filter_type '{value}'; allowed: {allowed:?}")]
    InvalidTaxonFilterType { value: String, allowed: Vec<String> },

    #[error("unknown search index '{name}'")]
    UnknownIndex { name: String },
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Validate a [`SearchQuery`] against generated field metadata and site config.
///
/// Returns a [`Vec`] of all validation errors found; an empty vec means the
/// query is valid.  Accumulates all errors rather than stopping at the first,
/// so callers see the complete problem set.
///
/// # Parameters
/// - `query`      — the query to validate
/// - `field_meta` — per-index map of canonical name → [`FieldMeta`]; keyed by index name
/// - `synonyms`   — per-index synonym → canonical name lookup; keyed by index name
/// - `valid_indexes` — list of valid index names from `SiteConfig`
/// - `config`     — site validation config (prefixes, name classes, filter types)
pub fn validate_query(
    query: &SearchQuery,
    field_meta: &phf::Map<&'static str, FieldMeta>,
    synonyms: &phf::Map<&'static str, &'static str>,
    valid_indexes: &[&str],
    config: &ValidationConfig,
) -> Vec<ValidationError> {
    let mut errors: Vec<ValidationError> = Vec::new();

    let index_str = query.index.to_api_str();
    if !valid_indexes.contains(&index_str) {
        errors.push(ValidationError::UnknownIndex {
            name: index_str.to_string(),
        });
        // Cannot validate attributes without a known index
        return errors;
    }

    errors.extend(validate_identifiers(&query.identifiers, config));
    errors.extend(validate_attribute_set(
        &query.attributes,
        field_meta,
        synonyms,
        index_str,
        config,
    ));

    errors
}

/// Resolve an attribute name through the synonym table.
///
/// Returns the canonical name if found in either the canonical map or the
/// synonym map; returns `None` if the name is unknown.
pub fn resolve_attribute_name<'a>(
    name: &'a str,
    field_meta: &phf::Map<&'static str, FieldMeta>,
    synonyms: &'a phf::Map<&'static str, &'static str>,
) -> Option<&'a str> {
    if field_meta.contains_key(name) {
        return Some(name);
    }
    synonyms.get(name).copied()
}

// ── Identifier validation ─────────────────────────────────────────────────────

fn validate_identifiers(
    identifiers: &Identifiers,
    config: &ValidationConfig,
) -> Vec<ValidationError> {
    let mut errors: Vec<ValidationError> = Vec::new();

    let filter_type_str = serde_json::to_value(&identifiers.taxon_filter_type)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();

    if !config.taxon_filter_types.contains(&filter_type_str) {
        errors.push(ValidationError::InvalidTaxonFilterType {
            value: filter_type_str,
            allowed: config.taxon_filter_types.clone(),
        });
    }

    for assembly in &identifiers.assemblies {
        let clean = assembly.trim_start_matches('!').to_lowercase();
        let valid = config
            .assembly_accession_prefixes
            .iter()
            .any(|p| clean.starts_with(p.as_str()));
        if !valid {
            errors.push(ValidationError::InvalidAssemblyPrefix {
                value: assembly.clone(),
                allowed: config.assembly_accession_prefixes.clone(),
            });
        }
    }

    for sample in &identifiers.samples {
        let clean = sample.trim_start_matches('!').to_lowercase();
        let valid = config
            .sample_accession_prefixes
            .iter()
            .any(|p| clean.starts_with(p.as_str()));
        if !valid {
            errors.push(ValidationError::InvalidSamplePrefix {
                value: sample.clone(),
                allowed: config.sample_accession_prefixes.clone(),
            });
        }
    }

    errors
}

// ── Attribute set validation ──────────────────────────────────────────────────

fn validate_attribute_set(
    attributes: &AttributeSet,
    field_meta: &phf::Map<&'static str, FieldMeta>,
    synonyms: &phf::Map<&'static str, &'static str>,
    index_str: &str,
    config: &ValidationConfig,
) -> Vec<ValidationError> {
    let mut errors: Vec<ValidationError> = Vec::new();

    for attr in &attributes.attributes {
        errors.extend(validate_attribute(attr, field_meta, synonyms, index_str));
    }

    for field in &attributes.fields {
        errors.extend(validate_field(field, field_meta, synonyms, index_str));
    }

    for name_class in &attributes.names {
        // Strip filter suffixes like "common_name:*bat*" before checking
        let base = name_class.split(':').next().unwrap_or(name_class);
        if !config.taxon_name_classes.contains(&base.to_string()) {
            errors.push(ValidationError::InvalidNameClass {
                value: name_class.clone(),
                allowed: config.taxon_name_classes.clone(),
            });
        }
    }

    errors
}

fn validate_attribute(
    attr: &Attribute,
    field_meta: &phf::Map<&'static str, FieldMeta>,
    synonyms: &phf::Map<&'static str, &'static str>,
    index_str: &str,
) -> Vec<ValidationError> {
    let mut errors: Vec<ValidationError> = Vec::new();

    let Some(canonical) = resolve_attribute_name(&attr.name, field_meta, synonyms) else {
        errors.push(ValidationError::UnknownAttribute {
            name: attr.name.clone(),
            index: index_str.to_string(),
        });
        return errors;
    };

    let meta = &field_meta[canonical];

    if let Some(op) = &attr.operator {
        errors.extend(validate_operator(canonical, op, meta));
    }

    if let Some(value) = &attr.value {
        errors.extend(validate_value(canonical, value, meta));
    }

    for modifier in &attr.modifier {
        errors.extend(validate_modifier(canonical, modifier, meta));
    }

    errors
}

fn validate_field(
    field: &Field,
    field_meta: &phf::Map<&'static str, FieldMeta>,
    synonyms: &phf::Map<&'static str, &'static str>,
    index_str: &str,
) -> Vec<ValidationError> {
    let mut errors: Vec<ValidationError> = Vec::new();

    let Some(canonical) = resolve_attribute_name(&field.name, field_meta, synonyms) else {
        errors.push(ValidationError::UnknownAttribute {
            name: field.name.clone(),
            index: index_str.to_string(),
        });
        return errors;
    };

    let meta = &field_meta[canonical];

    for modifier in &field.modifier {
        errors.extend(validate_modifier(canonical, modifier, meta));
    }

    errors
}

fn validate_operator(name: &str, op: &AttributeOperator, meta: &FieldMeta) -> Vec<ValidationError> {
    let is_keyword = meta.processed_type.contains("keyword");
    let is_range_op = matches!(
        op,
        AttributeOperator::Lt
            | AttributeOperator::Le
            | AttributeOperator::Gt
            | AttributeOperator::Ge
    );

    if is_keyword && is_range_op {
        vec![ValidationError::InvalidOperatorForKeyword {
            name: name.to_string(),
            op: op.as_str().to_string(),
        }]
    } else {
        vec![]
    }
}

fn validate_value(name: &str, value: &AttributeValue, meta: &FieldMeta) -> Vec<ValidationError> {
    let Some(allowed_enum) = meta.constraint_enum else {
        return vec![];
    };

    value
        .as_strs()
        .into_iter()
        .filter_map(|v| {
            let clean = v.trim_start_matches('!').to_lowercase();
            let valid = allowed_enum.iter().any(|e| e.to_lowercase() == clean);
            if valid {
                return None;
            }
            Some(ValidationError::InvalidEnumValue {
                name: name.to_string(),
                value: v.to_string(),
                allowed: allowed_enum.iter().map(|s| s.to_string()).collect(),
            })
        })
        .collect()
}

fn validate_modifier(name: &str, modifier: &Modifier, meta: &FieldMeta) -> Vec<ValidationError> {
    if modifier.is_status() {
        return validate_status_modifier(name, modifier, meta);
    }

    // Summary modifier: check it is in the field's summary list
    let mod_str = modifier.as_str();
    if !meta.summary.contains(&mod_str) {
        return vec![ValidationError::InvalidModifier {
            name: name.to_string(),
            modifier: mod_str.to_string(),
            allowed: meta.summary.iter().map(|s| s.to_string()).collect(),
        }];
    }

    vec![]
}

fn validate_status_modifier(
    name: &str,
    modifier: &Modifier,
    meta: &FieldMeta,
) -> Vec<ValidationError> {
    let direction = meta.traverse_direction;
    match modifier {
        Modifier::Ancestral => {
            let ok = matches!(direction, Some("down") | Some("both"));
            if ok {
                vec![]
            } else {
                vec![ValidationError::AncestralModifierNotSupported {
                    name: name.to_string(),
                    direction: direction.map(str::to_string),
                }]
            }
        }
        Modifier::Descendant => {
            let ok = matches!(direction, Some("up") | Some("both"));
            if ok {
                vec![]
            } else {
                vec![ValidationError::DescendantModifierNotSupported {
                    name: name.to_string(),
                    direction: direction.map(str::to_string),
                }]
            }
        }
        _ => vec![], // Direct, Estimated, Missing — always valid status modifiers
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::query::{
        attributes::{Attribute, AttributeSet, AttributeValue},
        identifiers::Identifiers,
        SearchIndex, SearchQuery,
    };

    // Minimal phf maps for testing — normally generated by field_meta.rs.tera.
    static TEST_FIELD_META: phf::Map<&'static str, FieldMeta> = phf::phf_map! {
        "genome_size" => FieldMeta {
            processed_type: "long",
            traverse_direction: Some("down"),
            summary: &["min", "max", "median"],
            constraint_enum: None,
        },
        "assembly_level" => FieldMeta {
            processed_type: "keyword",
            traverse_direction: None,
            summary: &[],
            constraint_enum: Some(&["contig", "scaffold", "chromosome", "complete genome"]),
        },
    };

    static TEST_SYNONYMS: phf::Map<&'static str, &'static str> = phf::phf_map! {
        "assembly_status" => "assembly_level",
    };

    fn test_config() -> ValidationConfig {
        ValidationConfig::default()
    }

    fn valid_indexes() -> Vec<&'static str> {
        vec!["taxon", "assembly", "sample"]
    }

    #[test]
    fn valid_query_returns_no_errors() {
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers {
                taxa: vec!["Mammalia".to_string()],
                ..Default::default()
            },
            attributes: AttributeSet {
                attributes: vec![Attribute {
                    name: "genome_size".to_string(),
                    operator: Some(AttributeOperator::Lt),
                    value: Some(AttributeValue::Single("3000000000".to_string())),
                    modifier: vec![Modifier::Min, Modifier::Direct],
                }],
                ..Default::default()
            },
        };
        let errors = validate_query(
            &query,
            &TEST_FIELD_META,
            &TEST_SYNONYMS,
            &valid_indexes(),
            &test_config(),
        );
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    #[test]
    fn unknown_attribute_name_is_an_error() {
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers::default(),
            attributes: AttributeSet {
                attributes: vec![Attribute {
                    name: "not_a_real_field".to_string(),
                    operator: None,
                    value: None,
                    modifier: vec![],
                }],
                ..Default::default()
            },
        };
        let errors = validate_query(
            &query,
            &TEST_FIELD_META,
            &TEST_SYNONYMS,
            &valid_indexes(),
            &test_config(),
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::UnknownAttribute { .. })));
    }

    #[test]
    fn synonym_resolves_successfully() {
        let resolved = resolve_attribute_name("assembly_status", &TEST_FIELD_META, &TEST_SYNONYMS);
        assert_eq!(resolved, Some("assembly_level"));
    }

    #[test]
    fn range_operator_on_keyword_is_invalid() {
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers::default(),
            attributes: AttributeSet {
                attributes: vec![Attribute {
                    name: "assembly_level".to_string(),
                    operator: Some(AttributeOperator::Gt),
                    value: Some(AttributeValue::Single("contig".to_string())),
                    modifier: vec![],
                }],
                ..Default::default()
            },
        };
        let errors = validate_query(
            &query,
            &TEST_FIELD_META,
            &TEST_SYNONYMS,
            &valid_indexes(),
            &test_config(),
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidOperatorForKeyword { .. })));
    }

    #[test]
    fn invalid_enum_value_is_reported() {
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers::default(),
            attributes: AttributeSet {
                attributes: vec![Attribute {
                    name: "assembly_level".to_string(),
                    operator: Some(AttributeOperator::Eq),
                    value: Some(AttributeValue::Single("supercontig".to_string())),
                    modifier: vec![],
                }],
                ..Default::default()
            },
        };
        let errors = validate_query(
            &query,
            &TEST_FIELD_META,
            &TEST_SYNONYMS,
            &valid_indexes(),
            &test_config(),
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidEnumValue { .. })));
    }

    #[test]
    fn ancestral_modifier_rejected_without_traverse_direction() {
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers::default(),
            attributes: AttributeSet {
                attributes: vec![Attribute {
                    name: "assembly_level".to_string(),
                    operator: None,
                    value: None,
                    modifier: vec![Modifier::Ancestral],
                }],
                ..Default::default()
            },
        };
        let errors = validate_query(
            &query,
            &TEST_FIELD_META,
            &TEST_SYNONYMS,
            &valid_indexes(),
            &test_config(),
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::AncestralModifierNotSupported { .. })));
    }

    #[test]
    fn invalid_assembly_prefix_reported() {
        let query = SearchQuery {
            index: SearchIndex::Assembly,
            identifiers: Identifiers {
                assemblies: vec!["NOTANACCESSION".to_string()],
                ..Default::default()
            },
            attributes: AttributeSet::default(),
        };
        let errors = validate_query(
            &query,
            &TEST_FIELD_META,
            &TEST_SYNONYMS,
            &valid_indexes(),
            &test_config(),
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidAssemblyPrefix { .. })));
    }

    #[test]
    fn unknown_index_reported() {
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers::default(),
            attributes: AttributeSet::default(),
        };
        let errors = validate_query(
            &query,
            &TEST_FIELD_META,
            &TEST_SYNONYMS,
            &["assembly"], // Only assembly is valid, not taxon
            &test_config(),
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::UnknownIndex { .. })));
    }

    #[test]
    fn invalid_name_class_reported() {
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers::default(),
            attributes: AttributeSet {
                names: vec!["not_a_name_class".to_string()],
                ..Default::default()
            },
        };
        let errors = validate_query(
            &query,
            &TEST_FIELD_META,
            &TEST_SYNONYMS,
            &valid_indexes(),
            &test_config(),
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidNameClass { .. })));
    }

    #[test]
    fn name_class_with_filter_suffix_validated() {
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers::default(),
            attributes: AttributeSet {
                names: vec!["scientific_name:*bat*".to_string()],
                ..Default::default()
            },
        };
        let errors = validate_query(
            &query,
            &TEST_FIELD_META,
            &TEST_SYNONYMS,
            &valid_indexes(),
            &test_config(),
        );
        // Should not error since scientific_name is valid (suffix is ignored)
        assert!(!errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidNameClass { .. })));
    }

    #[test]
    fn valid_query_produces_no_errors() {
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers::default(),
            attributes: AttributeSet {
                attributes: vec![Attribute {
                    name: "assembly_level".to_string(),
                    operator: Some(AttributeOperator::Eq),
                    value: Some(AttributeValue::Single("chromosome".to_string())),
                    modifier: vec![],
                }],
                names: vec!["scientific_name".to_string()],
                ..Default::default()
            },
        };
        let errors = validate_query(
            &query,
            &TEST_FIELD_META,
            &TEST_SYNONYMS,
            &valid_indexes(),
            &test_config(),
        );
        assert!(errors.is_empty(), "valid query should produce no errors");
    }

    #[test]
    fn invalid_sample_prefix_reported() {
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers {
                samples: vec!["BADSAMPLE".to_string()],
                ..Default::default()
            },
            attributes: AttributeSet::default(),
        };
        let errors = validate_query(
            &query,
            &TEST_FIELD_META,
            &TEST_SYNONYMS,
            &valid_indexes(),
            &test_config(),
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidSamplePrefix { .. })));
    }

    #[test]
    fn negated_assembly_accession_accepted() {
        // Negated accessions start with '!' which should be stripped before validation
        let query = SearchQuery {
            index: SearchIndex::Assembly,
            identifiers: Identifiers {
                assemblies: vec!["!GCA_000001405.40".to_string()],
                ..Default::default()
            },
            attributes: AttributeSet::default(),
        };
        let errors = validate_query(
            &query,
            &TEST_FIELD_META,
            &TEST_SYNONYMS,
            &valid_indexes(),
            &test_config(),
        );
        // Should not error; the '!' prefix should be stripped before validation
        assert!(!errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidAssemblyPrefix { .. })));
    }

    #[test]
    fn invalid_summary_modifier_on_non_summary_field() {
        // Test that an invalid modifier is rejected for a field that doesn't support it
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers {
                taxa: vec!["Mammalia".to_string()],
                ..Default::default()
            },
            attributes: AttributeSet {
                attributes: vec![Attribute {
                    name: "assembly_level".to_string(),
                    operator: None,
                    value: None,
                    // "Min" modifier is not valid for assembly_level (a non-summary field)
                    modifier: vec![Modifier::Min],
                }],
                ..Default::default()
            },
        };
        let errors = validate_query(
            &query,
            &TEST_FIELD_META,
            &TEST_SYNONYMS,
            &valid_indexes(),
            &test_config(),
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidModifier { .. })));
    }

    #[test]
    fn descendant_modifier_rejected_without_traverse_direction() {
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers {
                taxa: vec!["Mammalia".to_string()],
                ..Default::default()
            },
            attributes: AttributeSet {
                attributes: vec![Attribute {
                    name: "genome_size".to_string(),
                    operator: None,
                    value: None,
                    // Descendant modifier on genome_size (which doesn't support it)
                    modifier: vec![Modifier::Descendant],
                }],
                ..Default::default()
            },
        };
        let errors = validate_query(
            &query,
            &TEST_FIELD_META,
            &TEST_SYNONYMS,
            &valid_indexes(),
            &test_config(),
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::DescendantModifierNotSupported { .. })));
    }

    #[test]
    fn field_entry_with_invalid_modifier_reported() {
        // Test Field (not Attribute) validation with invalid modifier
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers::default(),
            attributes: AttributeSet {
                fields: vec![Field {
                    name: "assembly_level".to_string(),
                    modifier: vec![Modifier::Min], // Invalid on non-summary field
                }],
                ..Default::default()
            },
        };
        let errors = validate_query(
            &query,
            &TEST_FIELD_META,
            &TEST_SYNONYMS,
            &valid_indexes(),
            &test_config(),
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidModifier { .. })));
    }

    #[test]
    fn field_entry_with_unknown_field_reported() {
        // Test Field with unknown name
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers::default(),
            attributes: AttributeSet {
                fields: vec![Field {
                    name: "nonexistent_field".to_string(),
                    modifier: vec![],
                }],
                ..Default::default()
            },
        };
        let errors = validate_query(
            &query,
            &TEST_FIELD_META,
            &TEST_SYNONYMS,
            &valid_indexes(),
            &test_config(),
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::UnknownAttribute { .. })));
    }

    #[test]
    fn multiple_errors_accumulate() {
        // Test that multiple independent errors are all reported
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers {
                assemblies: vec!["BADBAD".to_string()],
                ..Default::default()
            },
            attributes: AttributeSet {
                attributes: vec![Attribute {
                    name: "nonexistent".to_string(),
                    operator: None,
                    value: None,
                    modifier: vec![],
                }],
                names: vec!["invalid_name_class".to_string()],
                ..Default::default()
            },
        };
        let errors = validate_query(
            &query,
            &TEST_FIELD_META,
            &TEST_SYNONYMS,
            &valid_indexes(),
            &test_config(),
        );
        // Should have at least 3 errors: invalid assembly, unknown attribute, invalid name class
        assert!(
            errors.len() >= 3,
            "expected at least 3 errors, got {}",
            errors.len()
        );
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidAssemblyPrefix { .. })));
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::UnknownAttribute { .. })));
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidNameClass { .. })));
    }

    #[test]
    fn invalid_taxon_filter_type_detected() {
        // Test detection of invalid taxon filter type (serialization path)
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers::default(),
            attributes: AttributeSet::default(),
        };
        // The TaxonFilterType is an enum that gets serialized; we test the validation
        let errors = validate_query(
            &query,
            &TEST_FIELD_META,
            &TEST_SYNONYMS,
            &valid_indexes(),
            &ValidationConfig {
                taxon_filter_types: vec![], // Empty list means all types invalid
                ..ValidationConfig::default()
            },
        );
        // Should report invalid filter type
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidTaxonFilterType { .. })));
    }

    #[test]
    fn estimated_status_modifier_always_valid() {
        // Test that Estimated (and other status modifiers) are always valid
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers::default(),
            attributes: AttributeSet {
                attributes: vec![Attribute {
                    name: "genome_size".to_string(),
                    operator: None,
                    value: None,
                    // Estimated is a status modifier that's always valid
                    modifier: vec![Modifier::Estimated],
                }],
                ..Default::default()
            },
        };
        let errors = validate_query(
            &query,
            &TEST_FIELD_META,
            &TEST_SYNONYMS,
            &valid_indexes(),
            &test_config(),
        );
        // Should not report any errors for Estimated modifier
        assert!(!errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidModifier { .. })));
    }
}
