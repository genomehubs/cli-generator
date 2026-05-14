/// Portable validation module for use in WASM and extendr.
/// Uses HashMap instead of phf::Map for JSON round-trip compatibility.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::report::ReportType;

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
    } else if !matches!(
        query.index.as_str(),
        "taxon" | "assembly" | "sample" | "feature"
    ) {
        errors.push(format!("unknown search index '{}'", query.index));
    }

    // Validate attributes — name checks are skipped when field_meta is empty
    // (no metadata available → structural-only validation, no false unknowns)
    for attr in &query.attributes {
        let attr_name = synonyms
            .get(&attr.name)
            .map(|s| s.as_str())
            .unwrap_or(&attr.name);

        if field_meta.is_empty() {
            // No field metadata provided: skip field-name checks entirely.
            continue;
        }

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

    // Validate fields — name checks are skipped when field_meta is empty
    if !field_meta.is_empty() {
        for field in &query.fields {
            if !field_meta.contains_key(&field.name) {
                errors.push(format!(
                    "unknown field '{}' for index '{}'",
                    field.name, query.index
                ));
            }
        }
    }

    Ok(errors)
}

/// Validate a report YAML string against known report type rules.
///
/// Returns a JSON array of error strings (empty array if valid).
///
/// Checks:
/// 1. `report` key is present and names a known report type.
/// 2. All fields required by that type are present.
/// 3. Axis field names (`x`, `y`, `cat`, `query`) are known fields when
///    `field_meta_json` is non-empty.
/// 4. Numeric range constraints: `hex_resolution` 1–12, `map_threshold` > 0,
///    `scatter_threshold` > 0.
pub fn validate_report_yaml(report_yaml: &str, field_meta_json: &str) -> String {
    let errors = match validate_report_impl(report_yaml, field_meta_json) {
        Ok(errs) => errs,
        Err(e) => vec![format!("validation failed: {}", e)],
    };
    serde_json::to_string(&errors).unwrap_or_else(|_| r#"["serialization error"]"#.to_string())
}

fn validate_report_impl(report_yaml: &str, field_meta_json: &str) -> Result<Vec<String>, String> {
    let doc: serde_yaml::Value = serde_yaml::from_str(report_yaml)
        .map_err(|e| format!("failed to parse report YAML: {}", e))?;

    let field_meta: HashMap<String, FieldMeta> =
        if field_meta_json.is_empty() || field_meta_json == "{}" {
            HashMap::new()
        } else {
            serde_json::from_str(field_meta_json)
                .map_err(|e| format!("failed to parse field_meta JSON: {}", e))?
        };

    let mut errors = Vec::new();

    let report_type_str = match doc.get("report").and_then(|v| v.as_str()) {
        Some(rt) => rt,
        None => {
            errors.push("report YAML missing required 'report' key".to_string());
            return Ok(errors);
        }
    };

    let report_type = match ReportType::parse(report_type_str) {
        Some(rt) => rt,
        None => {
            errors.push(format!(
                "unknown report type '{}'; expected one of: histogram, scatter, map, tree, countPerRank, sources, arc",
                report_type_str
            ));
            return Ok(errors);
        }
    };

    for required in report_type.required_axes() {
        if doc.get(required).is_none() {
            errors.push(format!(
                "report type '{}' requires '{}' field in report YAML",
                report_type_str, required
            ));
        }
    }

    if !field_meta.is_empty() {
        for axis in &["x", "y", "cat", "query"] {
            if let Some(field_val) = doc.get(axis) {
                let check_fields: Vec<&str> = match field_val {
                    serde_yaml::Value::Sequence(seq) => {
                        seq.iter().filter_map(|v| v.as_str()).collect()
                    }
                    serde_yaml::Value::String(s) => vec![s.as_str()],
                    _ => vec![],
                };
                for field_name in check_fields {
                    let bare = field_name.split(':').next().unwrap_or(field_name);
                    if !field_meta.contains_key(bare) {
                        errors.push(format!("unknown field '{}' in '{}' axis", bare, axis));
                    }
                }
            }
        }
    }

    if let Some(hex_res) = doc.get("hex_resolution").and_then(|v| v.as_u64()) {
        if !(1..=12).contains(&hex_res) {
            errors.push(format!(
                "hex_resolution must be between 1 and 12, got {}",
                hex_res
            ));
        }
    }
    if let Some(map_t) = doc.get("map_threshold").and_then(|v| v.as_u64()) {
        if map_t == 0 {
            errors.push("map_threshold must be greater than 0".to_string());
        }
    }
    if let Some(scatter_t) = doc.get("scatter_threshold").and_then(|v| v.as_u64()) {
        if scatter_t == 0 {
            errors.push("scatter_threshold must be greater than 0".to_string());
        }
    }

    Ok(errors)
}

/// Validate custom histogram boundaries in axis options.
///
/// Checks:
/// - Numeric boundaries are sorted in ascending order
/// - Date boundaries (explicit) are valid ISO 8601 and sorted
/// - Label count matches bucket count if provided
pub fn validate_axis_boundaries(
    axis_role: &str,
    axis_value: &serde_json::Value,
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    // Check if boundaries are specified
    let boundaries = match axis_value.get("boundaries") {
        Some(b) => b,
        None => return Ok(()),
    };

    // Numeric boundaries
    if let Some(numeric_vals) = boundaries.as_array() {
        // All numeric: check sorted
        if numeric_vals.iter().all(|v| v.is_number()) {
            let values: Vec<f64> = numeric_vals.iter().filter_map(|v| v.as_f64()).collect();

            if values.len() != numeric_vals.len() {
                errors.push(format!(
                    "axis {} boundaries: mixed numeric and non-numeric values",
                    axis_role
                ));
            } else {
                // Check sorted
                for i in 1..values.len() {
                    if values[i] <= values[i - 1] {
                        errors.push(format!(
                            "axis {} boundaries must be strictly increasing; got {} after {}",
                            axis_role,
                            values[i],
                            values[i - 1]
                        ));
                    }
                }
            }
        }
    } else if let Some(obj) = boundaries.as_object() {
        // Date boundaries with explicit timestamps
        if let Some(explicit) = obj.get("explicit").and_then(|e| e.as_array()) {
            let mut dates = Vec::new();
            for (i, date_str) in explicit.iter().enumerate() {
                if let Some(s) = date_str.as_str() {
                    // Validate ISO 8601 format (YYYY-MM-DD or YYYY-MM-DDTHH:MM:SS)
                    // Simple validation: check format is YYYY-MM-DD with numeric parts
                    if s.len() >= 10 {
                        let date_part = &s[..10];
                        if !is_valid_iso_date(date_part) {
                            errors.push(format!(
                                "axis {} date boundary [{}]: invalid date format (expected YYYY-MM-DD), got '{}'",
                                axis_role, i, s
                            ));
                        } else {
                            dates.push(date_part.to_string());
                        }
                    } else {
                        errors.push(format!(
                            "axis {} date boundary [{}]: invalid date format (expected YYYY-MM-DD), got '{}'",
                            axis_role, i, s
                        ));
                    }
                } else {
                    errors.push(format!(
                        "axis {} date boundary [{}]: expected string",
                        axis_role, i
                    ));
                }
            }

            // Check dates are sorted (lexicographic since YYYY-MM-DD format sorts correctly)
            for i in 1..dates.len() {
                if dates[i] <= dates[i - 1] {
                    errors.push(format!(
                        "axis {} dates must be strictly increasing; got {} after {}",
                        axis_role,
                        dates[i],
                        dates[i - 1]
                    ));
                }
            }
        }
    }

    // Check labels if provided
    if let Some(labels) = axis_value.get("labels").and_then(|l| l.as_array()) {
        // Calculate expected bucket count
        let expected_buckets = if let Some(numeric_vals) = boundaries.as_array() {
            if numeric_vals.iter().all(|v| v.is_number()) {
                numeric_vals.len().saturating_sub(1)
            } else {
                0
            }
        } else if let Some(obj) = boundaries.as_object() {
            if let Some(explicit) = obj.get("explicit").and_then(|e| e.as_array()) {
                explicit.len().saturating_sub(1)
            } else {
                0
            }
        } else {
            0
        };

        if expected_buckets > 0 && labels.len() != expected_buckets {
            errors.push(format!(
                "axis {} labels mismatch: provided {} labels but have {} buckets",
                axis_role,
                labels.len(),
                expected_buckets
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Simple validation of ISO 8601 date format (YYYY-MM-DD).
///
/// Does not perform full date validation (e.g., Feb 30), just format check.
fn is_valid_iso_date(date_str: &str) -> bool {
    if date_str.len() != 10 {
        return false;
    }

    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return false;
    }

    // Check format: digits-digits-digits (YYYY-MM-DD)
    if parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return false;
    }

    parts[0].chars().all(|c| c.is_numeric())
        && parts[1].chars().all(|c| c.is_numeric())
        && parts[2].chars().all(|c| c.is_numeric())
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

    #[test]
    fn test_validate_report_yaml_histogram_valid() {
        let report_yaml = "report: histogram\nx: genome_size\n";
        let result = validate_report_yaml(report_yaml, "{}");
        assert_eq!(result, "[]");
    }

    #[test]
    fn test_validate_report_yaml_histogram_missing_x() {
        let report_yaml = "report: histogram\nrank: species\n";
        let result = validate_report_yaml(report_yaml, "{}");
        assert!(result.contains("requires 'x'"));
    }

    #[test]
    fn test_validate_report_yaml_scatter_missing_y() {
        let report_yaml = "report: scatter\nx: genome_size\n";
        let result = validate_report_yaml(report_yaml, "{}");
        assert!(result.contains("requires 'y'"));
    }

    #[test]
    fn test_validate_report_yaml_map_valid() {
        let result = validate_report_yaml("report: map\n", "{}");
        assert_eq!(result, "[]");
    }

    #[test]
    fn test_validate_report_yaml_tree_missing_rank() {
        let result = validate_report_yaml("report: tree\nx: genome_size\n", "{}");
        assert!(result.contains("requires 'rank'"));
    }

    #[test]
    fn test_validate_report_yaml_count_per_rank_valid() {
        let result = validate_report_yaml("report: countPerRank\nquery: genome_size\n", "{}");
        assert_eq!(result, "[]");
    }

    #[test]
    fn test_validate_report_yaml_unknown_type() {
        let result = validate_report_yaml("report: xPerRank\nx: genome_size\n", "{}");
        assert!(result.contains("unknown report type"));
    }

    #[test]
    fn test_validate_report_yaml_missing_report_key() {
        let result = validate_report_yaml("x: genome_size\n", "{}");
        assert!(result.contains("missing required 'report' key"));
    }

    #[test]
    fn test_validate_report_yaml_hex_resolution_out_of_range() {
        let result = validate_report_yaml("report: map\nhex_resolution: 15\n", "{}");
        assert!(result.contains("hex_resolution"));
    }

    #[test]
    fn test_validate_report_yaml_map_threshold_zero() {
        let result = validate_report_yaml("report: map\nmap_threshold: 0\n", "{}");
        assert!(result.contains("map_threshold"));
    }

    #[test]
    fn test_validate_report_yaml_unknown_field_with_meta() {
        let field_meta = r#"{"genome_size": {"processed_type": "double", "traverse_direction": null, "summary": [], "constraint_enum": null}}"#;
        let result = validate_report_yaml("report: histogram\nx: unknown_field\n", field_meta);
        assert!(result.contains("unknown field"));
    }

    #[test]
    fn test_validate_report_yaml_known_field_with_meta() {
        let field_meta = r#"{"genome_size": {"processed_type": "double", "traverse_direction": null, "summary": [], "constraint_enum": null}}"#;
        let result = validate_report_yaml("report: histogram\nx: genome_size\n", field_meta);
        assert_eq!(result, "[]");
    }
}
