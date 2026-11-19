/// Portable validation module for use in WASM and extendr.
/// Uses HashMap instead of phf::Map for JSON round-trip compatibility.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::report::ReportType;
use crate::types::FieldDef;

/// Field metadata for validation (portable version without static lifetime constraints).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMeta {
    /// Processed type: "long", "double", "keyword", "date", etc.
    /// Keyword fields do not support </> operators.
    pub processed_type: String,

    /// Direction of value inheritance: "up", "down", "both", or null when the field is not inherited.
    /// Inheritance modifiers (`ancestral`, `descendant`) need a matching direction on the taxon index.
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

impl Default for ValidationConfig {
    /// Default prefixes, name classes, and filter types, matching the serde defaults.
    fn default() -> Self {
        Self {
            assembly_accession_prefixes: default_assembly_prefixes(),
            sample_accession_prefixes: default_sample_prefixes(),
            taxon_name_classes: default_name_classes(),
            taxon_filter_types: default_taxon_filter_types(),
        }
    }
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

fn default_index() -> String {
    "taxon".to_string()
}

/// Portable SearchQuery type for JSON deserialization.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchQuery {
    /// Index to search; defaults to `taxon`, matching the core query parser.
    #[serde(default = "default_index")]
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

/// Attribute filter as written in query YAML.
///
/// `value` may be a scalar (string or number) or a list, matching the core parser.
#[derive(Debug, Clone, Deserialize)]
pub struct AttributeFilter {
    pub name: String,
    #[serde(default)]
    pub operator: Option<String>,
    #[serde(default)]
    pub value: Option<serde_yaml::Value>,
    #[serde(default, alias = "modifier")]
    pub modifiers: Option<Vec<String>>,
}

/// Field selection as written in query YAML: a bare name or an object with optional modifiers.
#[derive(Debug, Clone)]
pub struct FieldSelection {
    pub name: String,
    pub modifiers: Option<Vec<String>>,
}

impl<'de> Deserialize<'de> for FieldSelection {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum FieldSelectionInput {
            Name(String),
            Full {
                name: String,
                #[serde(default, alias = "modifier")]
                modifiers: Option<Vec<String>>,
            },
        }

        Ok(match FieldSelectionInput::deserialize(deserializer)? {
            FieldSelectionInput::Name(name) => FieldSelection {
                name,
                modifiers: None,
            },
            FieldSelectionInput::Full { name, modifiers } => FieldSelection { name, modifiers },
        })
    }
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

/// Validate a query YAML against field definitions, using the default validation config.
///
/// Returns error messages; an empty list means valid. An empty `fields` slice skips name checks.
pub fn validate_query_for_fields(query_yaml: &str, fields: &[FieldDef]) -> Vec<String> {
    let query: SearchQuery = match serde_yaml::from_str(query_yaml) {
        Ok(query) => query,
        Err(error) => return vec![format!("failed to parse query YAML: {error}")],
    };
    check_query(
        &query,
        &field_meta_map(fields),
        &ValidationConfig::default(),
        &synonym_map(fields),
    )
}

/// Build validator field metadata keyed by canonical field name.
///
/// Produces the same shape as the `field_meta.json` consumed by the SDK validators.
pub fn field_meta_map(fields: &[FieldDef]) -> HashMap<String, FieldMeta> {
    fields
        .iter()
        .map(|field| (field.name.clone(), field_meta_of(field)))
        .collect()
}

/// Convert one field definition into validator metadata.
fn field_meta_of(field: &FieldDef) -> FieldMeta {
    let processed_type = field
        .processed_type
        .clone()
        .or_else(|| field.field_type.clone())
        .unwrap_or_else(|| "keyword".to_string());
    let constraint_enum = field
        .constraint
        .as_ref()
        .map(|constraint| constraint.enum_values.clone())
        .filter(|values| !values.is_empty());
    FieldMeta {
        processed_type,
        traverse_direction: field.traverse_direction.clone(),
        summary: field.summary.clone(),
        constraint_enum,
    }
}

/// Map each field synonym to its canonical field name.
pub fn synonym_map(fields: &[FieldDef]) -> HashMap<String, String> {
    fields
        .iter()
        .flat_map(|field| {
            field
                .synonyms
                .iter()
                .map(move |synonym| (synonym.clone(), field.name.clone()))
        })
        .collect()
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

    Ok(check_query(&query, &field_meta, &config, &synonyms))
}

/// Run the structural and name checks on a parsed query.
fn check_query(
    query: &SearchQuery,
    field_meta: &HashMap<String, FieldMeta>,
    config: &ValidationConfig,
    synonyms: &HashMap<String, String>,
) -> Vec<String> {
    let mut errors = Vec::new();

    // Validate index (defaults to taxon, matching the core query parser)
    if !matches!(
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
                if !KEYWORD_OPERATORS.contains(&op.as_str()) {
                    errors.push(format!(
                        "operator '{}' is not valid for keyword attribute '{}' (allowed: =, ==, eq, !=, ne, exists, missing)",
                        op, attr.name
                    ));
                }
            }
        }

        // Validate enum values
        if let Some(allowed_vals) = &meta.constraint_enum {
            for value in attr.value.as_ref().map(value_texts).unwrap_or_default() {
                // Strip negation prefix if present
                let check_val = value.trim_start_matches('!').to_lowercase();
                if !allowed_vals.iter().any(|av| av.to_lowercase() == check_val) {
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
                if let Some(message) = modifier_error(modifier, &attr.name, &query.index, meta) {
                    errors.push(message);
                }
            }
        }
    }

    // Validate assembly accession prefixes
    for asm_id in &query.assembly_ids {
        let check_id = asm_id.trim_start_matches('!');
        if !has_accession_prefix(check_id, &config.assembly_accession_prefixes) {
            errors.push(format!(
                "invalid assembly accession prefix '{}'; expected one of {:?}",
                check_id, config.assembly_accession_prefixes
            ));
        }
    }

    // Validate sample accession prefixes
    for sample_id in &query.sample_ids {
        let check_id = sample_id.trim_start_matches('!');
        if !has_accession_prefix(check_id, &config.sample_accession_prefixes) {
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
            let field_name = synonyms.get(&field.name).unwrap_or(&field.name);
            if !field_meta.contains_key(field_name) {
                errors.push(format!(
                    "unknown field '{}' for index '{}'",
                    field.name, query.index
                ));
            }
        }
    }

    errors
}

/// Operators valid for keyword attributes: equality, inequality, and presence checks.
const KEYWORD_OPERATORS: [&str; 7] = ["=", "==", "eq", "!=", "ne", "exists", "missing"];

/// Status modifiers that apply to any attribute and are never rejected.
const STATUS_MODIFIERS: [&str; 3] = ["direct", "estimated", "missing"];

/// Returns true when `accession` starts with one of `prefixes`, ignoring case.
fn has_accession_prefix(accession: &str, prefixes: &[String]) -> bool {
    let lower = accession.to_lowercase();
    prefixes
        .iter()
        .any(|prefix| lower.starts_with(&prefix.to_lowercase()))
}

/// Text values of an attribute `value`, which may be a scalar or a list.
fn value_texts(value: &serde_yaml::Value) -> Vec<String> {
    match value {
        serde_yaml::Value::Sequence(items) => items.iter().flat_map(value_texts).collect(),
        serde_yaml::Value::String(text) => vec![text.clone()],
        serde_yaml::Value::Number(number) => vec![number.to_string()],
        serde_yaml::Value::Bool(flag) => vec![flag.to_string()],
        _ => Vec::new(),
    }
}

/// Returns an error message when `modifier` is not valid for the attribute.
fn modifier_error(
    modifier: &str,
    attr_name: &str,
    index: &str,
    meta: &FieldMeta,
) -> Option<String> {
    if STATUS_MODIFIERS.contains(&modifier) {
        return None;
    }
    match modifier {
        "ancestral" => inheritance_error(modifier, attr_name, index, meta, "down"),
        "descendant" => inheritance_error(modifier, attr_name, index, meta, "up"),
        _ if meta.summary.iter().any(|allowed| allowed == modifier) => None,
        _ => Some(format!(
            "modifier '{}' is not valid for attribute '{}'; allowed: {:?}",
            modifier, attr_name, meta.summary
        )),
    }
}

/// Returns an error when an inheritance modifier cannot apply to the attribute.
///
/// Inheritance applies only to the `taxon` index. Within taxon, the field needs a
/// traverse direction that matches `required_direction`, or `both`. A missing direction
/// means the field is not inherited, so only direct or missing values are available.
fn inheritance_error(
    modifier: &str,
    attr_name: &str,
    index: &str,
    meta: &FieldMeta,
    required_direction: &str,
) -> Option<String> {
    if index != "taxon" {
        return Some(format!(
            "modifier '{}' not valid for '{}' on index '{}': inheritance applies to taxon only",
            modifier, attr_name, index
        ));
    }
    let Some(direction) = meta.traverse_direction.as_deref() else {
        return Some(format!(
            "modifier '{}' not valid for '{}': field has no traverse_direction, so only direct or missing values are available",
            modifier, attr_name
        ));
    };
    if direction == required_direction || direction == "both" {
        return None;
    }
    Some(format!(
        "modifier '{}' not valid for '{}': traverse_direction is '{}', needs '{}' or 'both'",
        modifier, attr_name, direction, required_direction
    ))
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
/// 5. `axes` entries have a known position and a field (known when metadata is given).
/// 6. Tree reports reject a flat `y` and `fields`; metrics use `axes` entries with `position: y`.
pub fn validate_report_yaml(report_yaml: &str, field_meta_json: &str) -> String {
    let errors = match validate_report_impl(report_yaml, field_meta_json) {
        Ok(errs) => errs,
        Err(e) => vec![format!("validation failed: {}", e)],
    };
    serde_json::to_string(&errors).unwrap_or_else(|_| r#"["serialization error"]"#.to_string())
}

/// Errors for each `axes` entry: a known position, a field name, and a known field when metadata is loaded.
fn validate_axes_entries(
    doc: &serde_yaml::Value,
    field_meta: &HashMap<String, FieldMeta>,
) -> Vec<String> {
    let Some(entries) = doc.get("axes").and_then(|a| a.as_sequence()) else {
        return vec![];
    };
    let mut errors = Vec::new();
    for entry in entries {
        let position = entry.get("position").and_then(|p| p.as_str()).unwrap_or("");
        if !(["x", "y", "z", "cat"].contains(&position)) {
            errors.push(format!(
                "invalid position '{}' in 'axes' entry; expected one of: x, y, z, cat",
                position
            ));
        }
        match entry.get("field").and_then(|f| f.as_str()) {
            None => errors.push("'axes' entry missing 'field'".to_string()),
            Some(field) if !field_meta.is_empty() && !field_meta.contains_key(field) => {
                errors.push(format!("unknown field '{}' in 'axes' entry", field));
            }
            Some(_) => {}
        }
    }
    errors
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

    if matches!(report_type, ReportType::Tree) {
        for legacy_key in ["y", "fields"] {
            if doc.get(legacy_key).is_some() {
                errors.push(format!(
                    "report type 'tree' does not accept '{legacy_key}'; use an 'axes' entry with position 'y'"
                ));
            }
        }
    }
    errors.extend(validate_axes_entries(&doc, &field_meta));

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

    use crate::types::FieldConstraint;

    fn genome_size_field() -> FieldDef {
        FieldDef {
            name: "genome_size".to_string(),
            processed_type: Some("long".to_string()),
            summary: vec!["min".to_string()],
            synonyms: vec!["gsize".to_string()],
            ..FieldDef::default()
        }
    }

    #[test]
    fn validate_query_for_fields_accepts_known_field() {
        let query = "index: taxon\ntaxa:\n  - Mammalia\nfields:\n  - genome_size\n";
        assert!(validate_query_for_fields(query, &[genome_size_field()]).is_empty());
    }

    #[test]
    fn validate_query_for_fields_reports_unknown_field() {
        let query = "index: taxon\nfields:\n  - genome_sise\n";
        let errors = validate_query_for_fields(query, &[genome_size_field()]);
        assert_eq!(
            errors,
            vec!["unknown field 'genome_sise' for index 'taxon'"]
        );
    }

    #[test]
    fn validate_query_for_fields_resolves_synonyms() {
        let query = "index: taxon\nfields:\n  - gsize\n";
        assert!(validate_query_for_fields(query, &[genome_size_field()]).is_empty());
    }

    #[test]
    fn validate_query_for_fields_skips_name_checks_without_fields() {
        let query = "index: taxon\nfields:\n  - anything\n";
        assert!(validate_query_for_fields(query, &[]).is_empty());
    }

    #[test]
    fn validate_query_for_fields_reports_unparseable_yaml() {
        let errors = validate_query_for_fields("index: [unclosed", &[]);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("failed to parse query YAML"));
    }

    #[test]
    fn field_meta_map_defaults_type_and_drops_empty_enums() {
        let field = FieldDef {
            name: "assembly_level".to_string(),
            constraint: Some(FieldConstraint {
                enum_values: Vec::new(),
            }),
            ..FieldDef::default()
        };
        let meta_map = field_meta_map(&[field]);
        let meta = &meta_map["assembly_level"];
        assert_eq!(meta.processed_type, "keyword");
        assert!(meta.constraint_enum.is_none());
    }

    #[test]
    fn synonym_map_maps_synonym_to_canonical_name() {
        let synonyms = synonym_map(&[genome_size_field()]);
        assert_eq!(
            synonyms.get("gsize").map(String::as_str),
            Some("genome_size")
        );
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
    fn test_numeric_and_list_values_are_accepted() {
        let query = r#"
index: assembly
attributes:
  - name: assembly_span
    operator: gte
    value: 1000000000
  - name: assembly_level
    operator: "="
    value: [chromosome, scaffold]
"#;
        let field_meta = r#"{
            "assembly_span": {"processed_type": "long", "traverse_direction": null, "summary": [], "constraint_enum": null},
            "assembly_level": {"processed_type": "keyword", "traverse_direction": null, "summary": [], "constraint_enum": ["chromosome", "scaffold", "contig"]}
        }"#;
        assert_eq!(validate_query_json(query, field_meta, "{}", "{}"), "[]");
    }

    #[test]
    fn test_enum_check_flags_invalid_list_member() {
        let query = "index: assembly\nattributes:\n  - name: assembly_level\n    value: [chromosome, bogus]\n";
        let field_meta = r#"{"assembly_level": {"processed_type": "keyword", "traverse_direction": null, "summary": [], "constraint_enum": ["chromosome", "scaffold"]}}"#;
        let result = validate_query_json(query, field_meta, "{}", "{}");
        assert!(result.contains("value 'bogus' is not valid"), "{result}");
        assert!(!result.contains("'chromosome'"), "{result}");
    }

    #[test]
    fn test_enum_check_is_case_insensitive() {
        let query = "index: assembly\nattributes:\n  - name: assembly_level\n    value: [Chromosome, SCAFFOLD]\n";
        let field_meta = r#"{"assembly_level": {"processed_type": "keyword", "traverse_direction": null, "summary": [], "constraint_enum": ["chromosome", "scaffold"]}}"#;
        assert_eq!(validate_query_json(query, field_meta, "{}", "{}"), "[]");
    }

    #[test]
    fn test_missing_index_defaults_to_taxon() {
        let query = "taxa:\n  - Mammalia\n";
        assert_eq!(validate_query_json(query, "{}", "{}", "{}"), "[]");
    }

    #[test]
    fn test_keyword_accepts_core_parser_operators() {
        let query = "index: taxon\nattributes:\n  - name: assembly_level\n    operator: eq\n    value: scaffold\n  - name: assembly_level\n    operator: ne\n    value: contig\n  - name: assembly_level\n    operator: exists\n  - name: assembly_level\n    operator: missing\n";
        let field_meta = r#"{"assembly_level": {"processed_type": "keyword", "traverse_direction": null, "summary": [], "constraint_enum": null}}"#;
        assert_eq!(validate_query_json(query, field_meta, "{}", "{}"), "[]");
    }

    #[test]
    fn test_keyword_rejects_range_operator() {
        let query = "index: taxon\nattributes:\n  - name: assembly_level\n    operator: gt\n    value: scaffold\n";
        let field_meta = r#"{"assembly_level": {"processed_type": "keyword", "traverse_direction": null, "summary": [], "constraint_enum": null}}"#;
        let result = validate_query_json(query, field_meta, "{}", "{}");
        assert!(result.contains("operator 'gt' is not valid"), "{result}");
    }

    #[test]
    fn test_status_modifiers_are_always_valid() {
        let query = "index: taxon\nattributes:\n  - name: genome_size\n    modifiers: [min, direct, estimated]\n";
        let field_meta = r#"{"genome_size": {"processed_type": "long", "traverse_direction": null, "summary": ["min"], "constraint_enum": null}}"#;
        assert_eq!(validate_query_json(query, field_meta, "{}", "{}"), "[]");
    }

    #[test]
    fn test_modifier_alias_is_checked() {
        let query = "index: taxon\nattributes:\n  - name: genome_size\n    modifier: [median]\n";
        let field_meta = r#"{"genome_size": {"processed_type": "long", "traverse_direction": null, "summary": ["min"], "constraint_enum": null}}"#;
        let result = validate_query_json(query, field_meta, "{}", "{}");
        assert!(
            result.contains("modifier 'median' is not valid"),
            "{result}"
        );
    }

    #[test]
    fn test_direct_modifier_accepted_when_direction_unknown() {
        let query = "index: taxon\nattributes:\n  - name: genome_size\n    modifiers: [direct]\n";
        let field_meta = r#"{"genome_size": {"processed_type": "long", "traverse_direction": null, "summary": [], "constraint_enum": null}}"#;
        assert_eq!(validate_query_json(query, field_meta, "{}", "{}"), "[]");
    }

    #[test]
    fn test_assembly_accession_prefix_is_case_insensitive() {
        let query = "index: assembly\nassembly_ids:\n  - GCA_000001405.29\n  - gcf_000001405.40\n  - ASM12345v1\n";
        assert_eq!(validate_query_json(query, "{}", "{}", "{}"), "[]");
    }

    #[test]
    fn test_sample_accession_prefix_accepts_long_form() {
        let query = "index: sample\nsample_ids:\n  - SAMEA123\n  - ERS000001\n";
        assert_eq!(validate_query_json(query, "{}", "{}", "{}"), "[]");
    }

    #[test]
    fn test_unknown_assembly_accession_prefix_rejected() {
        let query = "index: assembly\nassembly_ids:\n  - XYZ_000001\n";
        let result = validate_query_json(query, "{}", "{}", "{}");
        assert!(
            result.contains("invalid assembly accession prefix"),
            "{result}"
        );
    }

    #[test]
    fn test_inheritance_modifier_rejected_when_direction_unknown() {
        let query =
            "index: taxon\nattributes:\n  - name: genome_size\n    modifiers: [ancestral]\n";
        let field_meta = r#"{"genome_size": {"processed_type": "long", "traverse_direction": null, "summary": [], "constraint_enum": null}}"#;
        let result = validate_query_json(query, field_meta, "{}", "{}");
        assert!(result.contains("no traverse_direction"), "{result}");
    }

    #[test]
    fn test_inheritance_modifier_rejected_on_non_taxon_index() {
        let query =
            "index: assembly\nattributes:\n  - name: genome_size\n    modifiers: [descendant]\n";
        let field_meta = r#"{"genome_size": {"processed_type": "long", "traverse_direction": "up", "summary": [], "constraint_enum": null}}"#;
        let result = validate_query_json(query, field_meta, "{}", "{}");
        assert!(result.contains("taxon only"), "{result}");
    }

    #[test]
    fn test_inheritance_modifier_accepted_when_direction_matches() {
        let query =
            "index: taxon\nattributes:\n  - name: genome_size\n    modifiers: [descendant]\n";
        let field_meta = r#"{"genome_size": {"processed_type": "long", "traverse_direction": "up", "summary": [], "constraint_enum": null}}"#;
        assert_eq!(validate_query_json(query, field_meta, "{}", "{}"), "[]");
    }

    #[test]
    fn test_inheritance_modifier_accepted_when_direction_both() {
        let query =
            "index: taxon\nattributes:\n  - name: genome_size\n    modifiers: [ancestral]\n";
        let field_meta = r#"{"genome_size": {"processed_type": "long", "traverse_direction": "both", "summary": [], "constraint_enum": null}}"#;
        assert_eq!(validate_query_json(query, field_meta, "{}", "{}"), "[]");
    }

    #[test]
    fn test_inheritance_modifier_rejected_when_direction_conflicts() {
        let query =
            "index: taxon\nattributes:\n  - name: genome_size\n    modifiers: [ancestral]\n";
        let field_meta = r#"{"genome_size": {"processed_type": "long", "traverse_direction": "up", "summary": [], "constraint_enum": null}}"#;
        let result = validate_query_json(query, field_meta, "{}", "{}");
        assert!(
            result.contains("modifier 'ancestral' not valid"),
            "{result}"
        );
    }

    #[test]
    fn test_field_synonym_is_resolved() {
        let query = "index: taxon\nfields:\n  - name: genome_size_alias\n";
        let field_meta = r#"{"genome_size": {"processed_type": "long", "traverse_direction": null, "summary": [], "constraint_enum": null}}"#;
        let synonyms = r#"{"genome_size_alias": "genome_size"}"#;
        assert_eq!(validate_query_json(query, field_meta, "{}", synonyms), "[]");
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
    fn test_validate_report_yaml_tree_rejects_flat_y() {
        let result = validate_report_yaml("report: tree\nrank: genus\ny: genome_size\n", "{}");
        assert!(result.contains("does not accept 'y'"));
    }

    #[test]
    fn test_validate_report_yaml_tree_rejects_fields() {
        let result =
            validate_report_yaml("report: tree\nrank: genus\nfields: [genome_size]\n", "{}");
        assert!(result.contains("does not accept 'fields'"));
    }

    #[test]
    fn test_validate_report_yaml_tree_axes_y_valid() {
        let report_yaml =
            "report: tree\nrank: genus\naxes:\n  - position: y\n    field: genome_size\n";
        assert_eq!(validate_report_yaml(report_yaml, "{}"), "[]");
    }

    #[test]
    fn test_validate_report_yaml_axes_invalid_position() {
        let report_yaml =
            "report: histogram\nx: genome_size\naxes:\n  - position: w\n    field: genome_size\n";
        let result = validate_report_yaml(report_yaml, "{}");
        assert!(result.contains("invalid position 'w'"));
    }

    #[test]
    fn test_validate_report_yaml_axes_unknown_field() {
        let field_meta = r#"{"genome_size": {"processed_type": "double", "traverse_direction": null, "summary": ["min"], "constraint_enum": null}}"#;
        let report_yaml =
            "report: tree\nrank: genus\naxes:\n  - position: y\n    field: gc_percent\n";
        let result = validate_report_yaml(report_yaml, field_meta);
        assert!(result.contains("unknown field 'gc_percent' in 'axes' entry"));
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
