//! ES feature queries for the `/api/v3/positional` endpoint.
//!
//! Builds and executes the two queries needed for a positional report:
//! 1. `toplevel` features to obtain sequence lengths.
//! 2. Group-by features (e.g. BUSCO genes) to obtain marker positions.
//!
//! Both queries use the **feature index v2** flat-field structure where
//! `sequence_id`, `start`, `end`, `strand`, `length`, and `sequence_length`
//! are promoted to top-level ES source fields.  The endpoint is hard-gated
//! on index version detection, so no v1 nested-attribute paths are present
//! here.
//!
//! ## Feature index v2 data model (relevant fields)
//!
//! - Top-level ES fields: `assembly_id`, `feature_id`, `taxon_id`,
//!   `primary_type`, `sequence_id`, `start`, `end`, `strand`, `length`,
//!   `sequence_length`, `container_ids`.
//! - Attribute-only fields (still in nested `attributes` array): `name`,
//!   `gc`, `score`, `status`, `merian_unit`, custom attributes.
//! - `feature_type` values use **lowercase kebab-case** in ES, so the
//!   caller-supplied `group_by` value (which may be snake_case, e.g.
//!   `busco_gene`) is normalised with [`resolve_feature_type`] before
//!   building the `primary_type` filter.

use serde_json::{json, Value};

use crate::es_client::execute_search;

/// A single parsed feature record.
#[derive(Debug, Clone)]
pub struct FeatureRecord {
    pub assembly_id: String,
    pub feature_id: String,
    pub sequence_id: String,
    pub start: u64,
    pub end: u64,
    pub strand: i8,
    pub group_value: String,
    pub cat_value: Option<String>,
}

/// A single top-level sequence (chromosome / scaffold) with its length.
#[derive(Debug, Clone)]
pub struct SequenceRecord {
    pub assembly_id: String,
    pub sequence_id: String,
    pub length: u64,
}

/// Resolve a user-supplied feature-type string to the actual ES attribute value
/// by looking it up in the known `feature_types` list from the metadata cache.
///
/// Resolution order (first match wins):
/// 1. Exact match against a known type.
/// 2. Case-insensitive exact match.
/// 3. Normalised form: lowercase + `_` → `-` substitution (handles `busco_gene`
///    → `busco-gene` and `topLevel` → `toplevel`).
/// 4. If no known types are cached, falls back to the normalised form.
///
/// Returns `Err` when known types are present but nothing matches, so the caller
/// can surface a helpful error listing valid options.
pub fn resolve_feature_type(input: &str, known_types: &[String]) -> Result<String, String> {
    if known_types.is_empty() {
        // Cache not yet populated — fall back to simple normalisation.
        return Ok(input.to_lowercase().replace('_', "-"));
    }

    // 1. Exact match
    if known_types.iter().any(|t| t == input) {
        return Ok(input.to_string());
    }

    // 2. Case-insensitive match
    let lower = input.to_lowercase();
    if let Some(m) = known_types.iter().find(|t| t.to_lowercase() == lower) {
        return Ok(m.clone());
    }

    // 3. Normalised match: lowercase + underscore → hyphen
    let normalised = lower.replace('_', "-");
    if let Some(m) = known_types
        .iter()
        .find(|t| t.to_lowercase().replace('_', "-") == normalised)
    {
        return Ok(m.clone());
    }

    // Nothing matched — return a helpful error
    Err(format!(
        "unknown feature_type '{}'. known types: {}",
        input,
        known_types.join(", ")
    ))
}

/// Fetch top-level sequence records for the given assemblies.
///
/// Uses the feature index v2 flat-field structure: reads `sequence_id` and
/// `length` from promoted top-level ES source fields and filters by
/// `primary_type = "toplevel"`.
pub async fn fetch_sequence_lengths_flat(
    client: &reqwest::Client,
    es_base: &str,
    index: &str,
    assembly_ids: &[String],
) -> Result<Vec<SequenceRecord>, String> {
    let body = json!({
        "query": {
            "bool": {
                "filter": [
                    {"terms": {"assembly_id": assembly_ids}},
                    {"term": {"primary_type": "toplevel"}}
                ]
            }
        },
        "_source": ["assembly_id", "sequence_id", "length"],
        "size": 10_000
    });

    let raw = execute_search(client, es_base, index, &body).await?;
    let total = raw
        .pointer("/hits/total/value")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let hits = raw
        .pointer("/hits/hits")
        .and_then(|h| h.as_array())
        .ok_or_else(|| {
            format!("toplevel flat query: missing hits (index={index}, total={total})")
        })?;

    let mut records = Vec::new();
    for hit in hits {
        let source = match hit.get("_source") {
            Some(s) => s,
            None => continue,
        };

        let assembly_id = source
            .get("assembly_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let sequence_id = source
            .get("sequence_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let length = source.get("length").and_then(|v| v.as_u64()).unwrap_or(0);

        if !assembly_id.is_empty() && !sequence_id.is_empty() && length > 0 {
            records.push(SequenceRecord {
                assembly_id,
                sequence_id,
                length,
            });
        }
    }
    if records.is_empty() && total > 0 {
        return Err(format!(
            "toplevel flat query matched {total} ES docs in index '{index}' \
             but no valid SequenceRecords were extracted. \
             Check that assembly_id, sequence_id, and length are present as \
             top-level source fields (feature index v2 required)."
        ));
    }
    Ok(records)
}

/// Parse a single feature index v2 ES hit into a [`FeatureRecord`].
///
/// Reads `sequence_id`, `start`, `end`, and `strand` from promoted top-level
/// source fields.  `group_value` (the shared marker identifier) and the
/// optional `cat_value` remain in the nested `attributes` array and are read
/// via [`extract_attributes`].
///
/// Returns `None` when any of `assembly_id`, `sequence_id`, or `group_value`
/// are absent or empty.
pub fn parse_flat_hit(
    hit: &Value,
    group_by: &str,
    cat_field: Option<&str>,
) -> Option<FeatureRecord> {
    let source = hit.get("_source")?;

    let assembly_id = source
        .get("assembly_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    let feature_id = source
        .get("feature_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let sequence_id = source
        .get("sequence_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    let start = source.get("start").and_then(|v| v.as_u64()).unwrap_or(0);
    let end = source.get("end").and_then(|v| v.as_u64()).unwrap_or(start);
    let strand = source
        .get("strand")
        .and_then(|v| v.as_i64())
        .map(|n| n as i8)
        .unwrap_or(1);

    // group_value and cat_value remain in nested attributes in feature index v2.
    let attrs = extract_attributes(source);
    let group_value = attrs
        .get(group_by)
        .cloned()
        .filter(|v| !v.is_empty())
        .or_else(|| attrs.get("name").cloned())
        .filter(|v| !v.is_empty())?;
    let cat_value = cat_field.and_then(|f| attrs.get(f).cloned());

    Some(FeatureRecord {
        assembly_id,
        feature_id,
        sequence_id,
        start,
        end,
        strand,
        group_value,
        cat_value,
    })
}

/// Parameters for [`fetch_features_flat`].
///
/// Bundles optional filter arguments to keep the function signature lean.
pub struct FeatureQueryFlat<'a> {
    /// Attribute key used as the shared marker identifier (e.g. `"busco_gene"`).
    pub group_by: &'a str,
    /// `primary_type` value to filter on.  Resolved against `known_feature_types`
    /// at query time.  Defaults to `group_by` at the call site when `None`.
    pub feature_type: Option<&'a str>,
    /// Optional category attribute key for colour grouping (e.g. `"merian_unit"`).
    pub cat_field: Option<&'a str>,
    /// Maximum number of features to return from ES.
    pub max_features: usize,
    /// Pre-built taxon filter clause (from `build_feature_taxon_filter`).
    pub taxon_filter: Option<&'a Value>,
    /// Pre-built nested attribute filter clauses for fields that remain in
    /// `attributes` (e.g. `status = "Complete"`).
    pub attribute_filters: &'a [Value],
    /// Known feature types from the metadata cache.  Used for smart resolution
    /// of user-supplied feature type names.  Pass an empty slice when the cache
    /// is unavailable (falls back to simple normalisation).
    pub known_feature_types: &'a [String],
}

/// Fetch positional marker features for the given assemblies.
///
/// Uses the feature index v2 flat-field structure: filters by the top-level
/// `primary_type` field and reads `sequence_id`, `start`, `end`, and `strand`
/// from top-level source fields.  `group_value` and `cat_value` are still read
/// from the nested `attributes` array.
pub async fn fetch_features_flat(
    client: &reqwest::Client,
    es_base: &str,
    index: &str,
    assembly_ids: &[String],
    query: &FeatureQueryFlat<'_>,
) -> Result<Vec<FeatureRecord>, String> {
    let FeatureQueryFlat {
        group_by,
        feature_type,
        cat_field,
        max_features,
        taxon_filter,
        attribute_filters,
        known_feature_types,
    } = query;
    let group_by: &str = group_by;
    let cat_field: Option<&str> = *cat_field;

    let mut filters = vec![json!({"terms": {"assembly_id": assembly_ids}})];

    if let Some(ftype) = feature_type {
        let resolved = resolve_feature_type(ftype, known_feature_types)?;
        filters.push(json!({"term": {"primary_type": resolved}}));
    }

    if let Some(tf) = *taxon_filter {
        filters.push(tf.clone());
    }

    for af in *attribute_filters {
        filters.push(af.clone());
    }

    let body = json!({
        "query": {
            "bool": {
                "filter": filters
            }
        },
        "_source": ["assembly_id", "feature_id", "sequence_id", "start", "end", "strand", "attributes"],
        "size": max_features
    });

    let raw = execute_search(client, es_base, index, &body).await?;
    let hits = raw
        .pointer("/hits/hits")
        .and_then(|h| h.as_array())
        .ok_or_else(|| "feature flat query: missing hits".to_string())?;

    let records = hits
        .iter()
        .filter_map(|hit| parse_flat_hit(hit, group_by, cat_field))
        .collect();

    Ok(records)
}

/// Extract a flat `key → value` map from the nested ES `attributes` array.
///
/// For each attribute entry, the first non-empty typed value field is used.
/// Numeric values are converted to their string representation.
pub(crate) fn extract_attributes(source: &Value) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let Some(attrs) = source.get("attributes").and_then(|v| v.as_array()) else {
        return map;
    };
    for entry in attrs {
        let Some(key) = entry
            .get("key")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
        else {
            continue;
        };
        // `keyword_value` may be a scalar string or an array; take the first
        // element when it is an array (e.g. `feature_type`).
        let value = entry
            .get("keyword_value")
            .and_then(|v| {
                if let Some(s) = v.as_str() {
                    Some(s.to_string())
                } else if let Some(arr) = v.as_array() {
                    arr.first().and_then(|e| e.as_str()).map(|s| s.to_string())
                } else {
                    None
                }
            })
            .or_else(|| {
                entry
                    .get("long_value")
                    .and_then(|v| v.as_i64())
                    .map(|n| n.to_string())
            })
            .or_else(|| {
                entry
                    .get("byte_value")
                    .and_then(|v| v.as_i64())
                    .map(|n| n.to_string())
            })
            .or_else(|| {
                entry
                    .get("integer_value")
                    .and_then(|v| v.as_i64())
                    .map(|n| n.to_string())
            })
            .or_else(|| {
                entry
                    .get("half_float_value")
                    .and_then(|v| v.as_f64())
                    .map(|n| n.to_string())
            })
            .or_else(|| {
                entry
                    .get("double_value")
                    .and_then(|v| v.as_f64())
                    .map(|n| n.to_string())
            })
            .or_else(|| {
                entry
                    .get("float_value")
                    .and_then(|v| v.as_f64())
                    .map(|n| n.to_string())
            })
            .or_else(|| {
                entry
                    .get("3dp_value")
                    .and_then(|v| v.as_f64())
                    .map(|n| n.to_string())
            });

        if let Some(v) = value {
            map.insert(key, v);
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct FlatHitParams<'a> {
        assembly_id: &'a str,
        feature_id: &'a str,
        sequence_id: &'a str,
        start: u64,
        end: u64,
        strand: i64,
        name: &'a str,
        cat: Option<(&'a str, &'a str)>,
    }

    fn make_flat_hit(p: FlatHitParams<'_>) -> Value {
        let mut attrs = vec![json!({"key": "name", "keyword_value": p.name})];
        if let Some((cat_key, cat_val)) = p.cat {
            attrs.push(json!({"key": cat_key, "keyword_value": cat_val}));
        }
        json!({
            "_source": {
                "assembly_id":  p.assembly_id,
                "feature_id":   p.feature_id,
                "sequence_id":  p.sequence_id,
                "start":        p.start,
                "end":          p.end,
                "strand":       p.strand,
                "attributes":   attrs
            }
        })
    }

    #[test]
    fn test_parse_flat_hit_complete() {
        let hit = make_flat_hit(FlatHitParams {
            assembly_id: "GCA_001",
            feature_id: "feat_001",
            sequence_id: "LR000001.1",
            start: 1_000_000,
            end: 1_001_500,
            strand: 1,
            name: "OG0001234",
            cat: Some(("merian_unit", "MZ-12")),
        });
        let rec = parse_flat_hit(&hit, "busco_gene", Some("merian_unit")).expect("should parse");
        assert_eq!(rec.assembly_id, "GCA_001");
        assert_eq!(rec.sequence_id, "LR000001.1");
        assert_eq!(rec.start, 1_000_000);
        assert_eq!(rec.end, 1_001_500);
        assert_eq!(rec.strand, 1);
        assert_eq!(rec.group_value, "OG0001234");
        assert_eq!(rec.cat_value.as_deref(), Some("MZ-12"));
    }

    #[test]
    fn test_parse_flat_hit_reverse_strand() {
        let hit = make_flat_hit(FlatHitParams {
            assembly_id: "GCA_001",
            feature_id: "feat_002",
            sequence_id: "LR000002.1",
            start: 5000,
            end: 6000,
            strand: -1,
            name: "OG9",
            cat: None,
        });
        let rec = parse_flat_hit(&hit, "busco_gene", None).expect("should parse");
        assert_eq!(rec.strand, -1);
        assert!(rec.cat_value.is_none());
    }

    #[test]
    fn test_parse_flat_hit_missing_sequence_id_returns_none() {
        let hit = json!({
            "_source": {
                "assembly_id": "GCA_001",
                "feature_id":  "feat_003",
                "start":       100,
                "end":         200,
                "strand":      1,
                "attributes":  [{"key": "name", "keyword_value": "OG1"}]
            }
        });
        assert!(parse_flat_hit(&hit, "busco_gene", None).is_none());
    }

    #[test]
    fn test_parse_flat_hit_missing_group_value_returns_none() {
        let hit = json!({
            "_source": {
                "assembly_id":  "GCA_001",
                "feature_id":   "feat_004",
                "sequence_id":  "LR000001.1",
                "start":        100,
                "end":          200,
                "strand":       1,
                "attributes":   []
            }
        });
        assert!(parse_flat_hit(&hit, "busco_gene", None).is_none());
    }

    #[test]
    fn test_resolve_feature_type_normalises_underscore() {
        let known = vec!["busco-gene".to_string(), "toplevel".to_string()];
        assert_eq!(
            resolve_feature_type("busco_gene", &known).unwrap(),
            "busco-gene"
        );
    }

    #[test]
    fn test_resolve_feature_type_unknown_with_cache_returns_err() {
        let known = vec!["busco-gene".to_string()];
        assert!(resolve_feature_type("nonexistent_type", &known).is_err());
    }

    #[test]
    fn test_resolve_feature_type_empty_cache_falls_back() {
        assert_eq!(
            resolve_feature_type("busco_gene", &[]).unwrap(),
            "busco-gene"
        );
    }
}
