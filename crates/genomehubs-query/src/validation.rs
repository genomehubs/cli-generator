/// Portable validation module for use in WASM and extendr.
/// Uses HashMap instead of phf::Map for JSON round-trip compatibility.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Field metadata for validation (portable version without static lifetime constraints).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMeta {
    /// Processed type: "long", "double", "keyword", "date", etc.
    /// Keyword fields do not support </> operators.
    pub processed_type: String,

    /// Direction of value inheritance in the tree: "up", "down", "both", or null.
    pub traverse_direction: Option<String>,

    /// Valid summary modifiers for this field.
    #[serde(default)]
    pub summary: Vec<String>,

    /// Valid enum values for keyword fields; null for open-ended fields.
    pub constraint_enum: Option<Vec<String>>,
}

/// Validation configuration (prefixes, classes, values).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConfig {
    /// Valid assembly accession prefixes (case-insensitive).
    #[serde(default = "default_assembly_prefixes")]
    pub assembly_accession_prefixes: Vec<String>,

    /// Valid sample accession prefixes (case-insensitive).
    #[serde(default = "default_sample_prefixes")]
    pub sample_accession_prefixes: Vec<String>,

    /// Valid taxon name classes.
    #[serde(default = "default_name_classes")]
    pub taxon_name_classes: Vec<String>,

    /// Valid taxon filter types.
    #[serde(default = "default_taxon_filter_types")]
    pub taxon_filter_types: Vec<String>,
}

fn default_assembly_prefixes() -> Vec<String> {
    vec![
        "gca_".to_string(),
        "gcf_".to_string(),
        "gcs_".to_string(),
        "gcn_".to_string(),
        "gcp_".to_string(),
        "gcr_".to_string(),
        "wgs".to_string(),
        "asm".to_string(),
    ]
}

fn default_sample_prefixes() -> Vec<String> {
    vec![
        "srs".to_string(),
        "srr".to_string(),
        "srx".to_string(),
        "sam".to_string(),
        "ers".to_string(),
        "erp".to_string(),
        "erx".to_string(),
        "drr".to_string(),
        "drx".to_string(),
        "samea".to_string(),
        "sameg".to_string(),
    ]
}

fn default_name_classes() -> Vec<String> {
    vec![
        "scientific_name".to_string(),
        "common_name".to_string(),
        "synonym".to_string(),
        "tolid_prefix".to_string(),
        "authority".to_string(),
    ]
}

fn default_taxon_filter_types() -> Vec<String> {
    vec![
        "name".to_string(),
        "tree".to_string(),
        "lineage".to_string(),
    ]
}

/// Portable SearchQuery type for JSON deserialization.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub index: String,

    #[serde(default)]
    pub taxa: Vec<String>,

    #[serde(default)]
    pub taxon_filter: Option<String>,

    #[serde(default)]
    pub rank: Option<String>,

    #[serde(default)]
    pub assembly_ids: Vec<String>,

    #[serde(default)]
    pub sample_ids: Vec<String>,

    #[serde(default)]
    pub attributes: Vec<AttributeFilter>,

    #[serde(default)]
    pub fields: Vec<FieldSelection>,

    #[serde(default)]
    pub names: Option<Vec<String>>,

    #[serde(default)]
    pub ranks: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AttributeFilter {
    pub name: String,
    #[serde(default)]
    pub operator: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub modifiers: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldSelection {
    pub name: String,
    #[serde(default)]
    pub modifiers: Option<Vec<String>>,
}

/// Validate a query using JSON field metadata and config.
///
/// Returns a JSON array of error strings. Empty array means valid.
pub fn validate_query_json(
    query_yaml: &str,
    field_meta_json: &str,
    config_json: &str,
    synonyms_str: &str,
) -> String {
    let errors = match validate_query_impl(query_yaml, field_meta_json, config_json, synonyms_str) {
        Ok(errs) => errs,
        Err(e) => vec![format!("validation failed: {}", e)],
    };

    serde_json::to_string(&errors).unwrap_or_else(|_| r#"["serialization error"]"#.to_string())
}

fn validate_query_impl(
    query_yaml: &str,
    field_meta_json: &str,
    config_json: &str,
    synonyms_str: &str,
) -> Result<Vec<String>, String> {
    // Parse query from YAML
    let query: SearchQuery = serde_yaml::from_str(query_yaml)
        .map_err(|e| format!("failed to parse query YAML: {}", e))?;

    // Parse field metadata from JSON
    let field_meta: HashMap<String, FieldMeta> = serde_json::from_str(field_meta_json)
        .map_err(|e| format!("failed to parse field_meta JSON: {}", e))?;

    // Parse validation config from JSON
    let config: ValidationConfig = serde_json::from_str(config_json)
        .map_err(|e| format!("failed to parse validation config JSON: {}", e))?;

    // Parse synonyms from JSON
    let synonyms: HashMap<String, String> = serde_json::from_str(synonyms_str).unwrap_or_default();

    let mut errors = Vec::new();

    // Validate index exists
    if query.index.is_empty() {
        errors.push("index not specified".to_string());
    } else if !matches!(query.index.as_str(), "taxon" | "assembly" | "sample") {
        errors.push(format!("unknown search index '{}'", query.index));
    }

    // Validate attributes
    for attr in &query.attributes {
        let attr_name = synonyms
            .get(&attr.name)
            .map(|s| s.as_str())
            .unwrap_or(&attr.name);

        if !field_meta.contains_key(attr_name) {
            errors.push(format!(
                "unknown attribute '{}' for index '{}'",
                attr.name, query.index
            ));
            continue;
        }

        let meta = &field_meta[attr_name];

        // Check operator validity for keyword fields
        if meta.processed_type == "keyword" {
            if let Some(op) = &attr.operator {
                if !matches!(op.as_str(), "=" | "!=") {
                    errors.push(format!(
                        "operator '{}' is not valid for keyword attribute '{}' (only =, != are allowed)",
                        op, attr.name
                    ));
                }
            }
        }

        // Validate enum values
        if let Some(allowed_vals) = &meta.constraint_enum {
            if let Some(value) = &attr.value {
                // Strip negation prefix if present
                let check_val = value.trim_start_matches('!');
                if !allowed_vals.iter().any(|av| av == check_val) {
                    errors.push(format!(
                        "value '{}' is not valid for attribute '{}'; allowed: {:?}",
                        check_val, attr.name, allowed_vals
                    ));
                }
            }
        }

        // Validate modifiers
        if let Some(modifiers) = &attr.modifiers {
            for modifier in modifiers {
                match modifier.as_str() {
                    "ancestral" => {
                        if meta.traverse_direction.as_deref() != Some("down")
                            && meta.traverse_direction.as_deref() != Some("both")
                        {
                            errors.push(format!(
                                "modifier 'ancestral' not valid for '{}': traverse_direction is {:?}, needs 'down' or 'both'",
                                attr.name, meta.traverse_direction
                            ));
                        }
                    }
                    "descendant" => {
                        if meta.traverse_direction.as_deref() != Some("up")
                            && meta.traverse_direction.as_deref() != Some("both")
                        {
                            errors.push(format!(
                                "modifier 'descendant' not valid for '{}': traverse_direction is {:?}, needs 'up' or 'both'",
                                attr.name, meta.traverse_direction
                            ));
                        }
                    }
                    _ => {
                        if !meta.summary.contains(modifier) {
                            errors.push(format!(
                                "modifier '{}' is not valid for attribute '{}'; allowed: {:?}",
                                modifier, attr.name, meta.summary
                            ));
                        }
                    }
                }
            }
        }
    }

    // Validate assembly accession prefixes
    for asm_id in &query.assembly_ids {
        let check_id = asm_id.trim_start_matches('!');
        let prefix_lower = check_id
            .split('_')
            .next()
            .unwrap_or(check_id)
            .to_lowercase();
        let found = config
            .assembly_accession_prefixes
            .iter()
            .any(|p| p.to_lowercase() == prefix_lower);
        if !found {
            errors.push(format!(
                "invalid assembly accession prefix '{}'; expected one of {:?}",
                check_id, config.assembly_accession_prefixes
            ));
        }
    }

    // Validate sample accession prefixes
    for sample_id in &query.sample_ids {
        let check_id = sample_id.trim_start_matches('!');
        let prefix = check_id.split('_').next().unwrap_or(check_id);
        let found = config
            .sample_accession_prefixes
            .iter()
            .any(|p| p.to_lowercase() == prefix.to_lowercase());
        if !found {
            errors.push(format!(
                "invalid sample accession prefix '{}'; expected one of {:?}",
                check_id, config.sample_accession_prefixes
            ));
        }
    }

    // Validate taxon_filter_type
    if let Some(filter_type) = &query.taxon_filter {
        if !config.taxon_filter_types.contains(filter_type) {
            errors.push(format!(
                "invalid taxon_filter_type '{}'; allowed: {:?}",
                filter_type, config.taxon_filter_types
            ));
        }
    }

    // Validate name classes
    if let Some(names) = &query.names {
        for name in names {
            // Strip field suffix (e.g., "scientific_name:filter" -> "scientific_name")
            let class = name.split(':').next().unwrap_or(name);
            if !config.taxon_name_classes.contains(&class.to_string()) {
                errors.push(format!(
                    "invalid taxon name class '{}'; allowed: {:?}",
                    class, config.taxon_name_classes
                ));
            }
        }
    }

    // Validate fields
    for field in &query.fields {
        if !field_meta.contains_key(&field.name) {
            errors.push(format!(
                "unknown field '{}' for index '{}'",
                field.name, query.index
            ));
        }
    }

    Ok(errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_query() {
        let query = r#"
          index: taxon
          taxa:
            - Mammalia
        "#;
        let field_meta = r#"
          {
            "genome_size": {
              "processed_type": "double",
              "traverse_direction": null,
              "summary": ["min", "max", "median"],
              "constraint_enum": null
            }
          }
        "#;
        let config = "{}"; // Uses defaults
        let synonyms = "{}";

        let result = validate_query_json(query, field_meta, config, synonyms);
        assert_eq!(result, "[]");
    }

    #[test]
    fn test_unknown_index() {
        let query = r#"
          index: invalid
        "#;
        let field_meta = "{}";
        let config = "{}";
        let synonyms = "{}";

        let result = validate_query_json(query, field_meta, config, synonyms);
        assert!(result.contains("unknown search index"));
    }

    #[test]
    fn test_unknown_attribute() {
        let query = r#"
          index: taxon
          attributes:
            - name: invalid_field
              operator: "="
              value: "test"
        "#;
        let field_meta = r#"
          {
            "valid_field": {
              "processed_type": "keyword",
              "traverse_direction": null,
              "summary": [],
              "constraint_enum": null
            }
          }
        "#;
        let config = "{}";
        let synonyms = "{}";

        let result = validate_query_json(query, field_meta, config, synonyms);
        assert!(result.contains("unknown attribute"));
    }
}
