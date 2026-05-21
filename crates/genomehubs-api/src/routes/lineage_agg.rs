//! Builder and extractor for `lineage_rank_summary` aggregations.
//!
//! Each [`LineageRankSummarySpec`] maps to one outer `nested(lineage)` aggregation
//! in the ES request body.  All fields in a spec share a single pass over
//! `by_ancestor` buckets, so requesting `["assembly_level", "ebp_standard_date"]`
//! at genus rank costs one outer agg — not two.
//!
//! The aggregation structure is:
//! ```text
//! lineage_{rank}:
//!   nested(lineage)
//!     by_rank: filter(lineage.taxon_rank = rank)
//!       by_ancestor: terms(lineage.taxon_id, size=N)
//!         back_to_root: reverse_nested
//!           by_attribute: nested(attributes)
//!             {field}: filter(attributes.key = field)
//!               by_value: terms(keyword_value.raw) | stats(long/float_value)
//! ```

use serde_json::{json, Value};
use std::sync::Arc;

use crate::es_metadata::MetadataCache;
use genomehubs_query::query::LineageRankSummarySpec;

// ── Agg builder ───────────────────────────────────────────────────────────────

/// Build one outer `nested(lineage)` aggregation for a [`LineageRankSummarySpec`].
///
/// Returns `(agg_name, agg_body)` where `agg_name` is `lineage_{rank}`.
// New: build lineage agg with optional ancestor include list. When
// `ancestor_include` is Some(list) the `by_ancestor` terms agg will be
// restricted to those ancestor IDs — useful for background summaries
// computed only for the ancestors observed in the matched results.
pub fn build_lineage_rank_summary_agg_with_include(
    spec: &LineageRankSummarySpec,
    ancestor_bucket_size: usize,
    cache: &Option<Arc<tokio::sync::RwLock<MetadataCache>>>,
    ancestor_include: Option<&[String]>,
) -> Result<(String, Value), String> {
    if spec.fields.is_empty() {
        return Err(format!(
            "lineage_rank_summary spec for rank '{}' has no fields",
            spec.rank
        ));
    }

    let agg_name = format!("lineage_{}", spec.rank);

    let mut field_aggs = serde_json::Map::new();
    for field in &spec.fields {
        let sub = build_field_sub_agg(field, cache);
        field_aggs.insert(field.clone(), sub);
    }

    // Build terms object for by_ancestor, optionally adding `include` list.
    let mut terms_obj = serde_json::Map::new();
    terms_obj.insert("field".to_string(), json!("lineage.taxon_id"));
    terms_obj.insert("size".to_string(), json!(ancestor_bucket_size));
    if let Some(ids) = ancestor_include {
        let arr = ids.iter().map(|s| Value::String(s.clone())).collect();
        terms_obj.insert("include".to_string(), Value::Array(arr));
    }

    let agg_body = json!({
        "nested": { "path": "lineage" },
        "aggs": {
            "by_rank": {
                "filter": { "term": { "lineage.taxon_rank": spec.rank } },
                "aggs": {
                    "by_ancestor": {
                        "terms": Value::Object(terms_obj),
                        "aggs": {
                            "back_to_root": {
                                "reverse_nested": {},
                                "aggs": {
                                    "by_attribute": {
                                        "nested": { "path": "attributes" },
                                        "aggs": Value::Object(field_aggs)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    Ok((agg_name, agg_body))
}

/// Backward-compatible wrapper kept for existing call sites.
pub fn build_lineage_rank_summary_agg(
    spec: &LineageRankSummarySpec,
    ancestor_bucket_size: usize,
    cache: &Option<Arc<tokio::sync::RwLock<MetadataCache>>>,
) -> Result<(String, Value), String> {
    build_lineage_rank_summary_agg_with_include(spec, ancestor_bucket_size, cache, None)
}

/// Select the appropriate ancestor bucket size for a given taxonomic rank.
///
/// Calibrated to maximum realistic clade sizes (all insect genera ≈ 40,000;
/// all insect families ≈ 660; all plant genera ≈ 16,000).
pub fn ancestor_bucket_size_for_rank(rank: &str) -> usize {
    match rank {
        "genus" => 50_000,
        "family" => 10_000,
        "order" => 2_000,
        "class" => 500,
        _ => 10_000,
    }
}

// ── Inner field aggregation ───────────────────────────────────────────────────

/// Validate that every field in each spec exists in the metadata cache.
///
/// Returns an error listing all unknown field names if any are not found.
/// When the cache is unavailable the check is skipped (no error).
pub fn validate_lineage_rank_summary_fields(
    specs: &[LineageRankSummarySpec],
    cache: &Option<Arc<tokio::sync::RwLock<MetadataCache>>>,
) -> Result<(), String> {
    let Some(cache_arc) = cache else {
        return Ok(());
    };
    let Ok(guard) = cache_arc.try_read() else {
        return Ok(());
    };
    let Value::Object(groups) = &guard.attr_types else {
        return Ok(());
    };

    let known: std::collections::HashSet<&str> = groups
        .values()
        .filter_map(|g| g.as_object())
        .flat_map(|m| m.keys().map(String::as_str))
        .collect();

    let unknown: Vec<String> = specs
        .iter()
        .flat_map(|s| s.fields.iter())
        .filter(|f| !known.contains(f.as_str()))
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    if unknown.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "lineage_rank_summary: unknown field(s): {}",
            unknown.join(", ")
        ))
    }
}

///
/// Dispatches on processed type from the metadata cache:
/// - `integer` → `stats` on `attributes.long_value`
/// - `float`   → `stats` on `attributes.half_float_value`
/// - `date`    → `stats` on `attributes.date_value`
/// - anything else → `terms` on `attributes.keyword_value.raw`
fn build_field_sub_agg(
    field: &str,
    cache: &Option<Arc<tokio::sync::RwLock<MetadataCache>>>,
) -> Value {
    let value_subfield = resolve_value_subfield(field, cache);

    let inner_agg = if value_subfield.ends_with("long_value")
        || value_subfield.ends_with("half_float_value")
        || value_subfield.ends_with("date_value")
    {
        json!({ "stats": { "field": value_subfield } })
    } else {
        json!({
            "terms": {
                "field": "attributes.keyword_value.raw",
                "size": 20
            }
        })
    };

    json!({
        "filter": { "term": { "attributes.key": field } },
        "aggs": { "by_value": inner_agg }
    })
}

/// Resolve the correct ES value subfield for an attribute from the metadata cache.
///
/// Returns `attributes.keyword_value.raw` as a safe default when the field is
/// absent from the cache or has an unrecognised processed type.
fn resolve_value_subfield(
    field: &str,
    cache: &Option<Arc<tokio::sync::RwLock<MetadataCache>>>,
) -> String {
    let Some(cache_arc) = cache else {
        return "attributes.keyword_value.raw".to_string();
    };
    let Ok(guard) = cache_arc.try_read() else {
        return "attributes.keyword_value.raw".to_string();
    };
    let Value::Object(groups) = &guard.attr_types else {
        return "attributes.keyword_value.raw".to_string();
    };

    for group in groups.values() {
        let Value::Object(fields) = group else {
            continue;
        };
        let Some(Value::Object(meta)) = fields.get(field) else {
            continue;
        };
        let processed_type = meta
            .get("processed_type")
            .and_then(|v| v.as_str())
            .unwrap_or("keyword");

        return match processed_type {
            "integer" => "attributes.long_value".to_string(),
            "float" => "attributes.half_float_value".to_string(),
            "date" => "attributes.date_value".to_string(),
            _ => "attributes.keyword_value.raw".to_string(),
        };
    }

    "attributes.keyword_value.raw".to_string()
}

// ── Response extractor ────────────────────────────────────────────────────────

/// Extract per-rank lineage summary from an ES aggregation response.
///
/// Returns a nested map structured as:
/// `rank → ancestor_taxon_id → field → distribution`
///
/// Distribution shape depends on field type:
/// - keyword: `{"chromosome": 3, "scaffold": 2, …}`
/// - numeric (stats): `{"min": 0.5, "max": 99.1, "avg": 72.3, "count": 40}`
/// - no data: `{}`
///
/// A genus absent from the map means it had zero matching species in the main
/// query.  A genus present with `"field": {}` means it had matching species but
/// none had a value for that field.
pub fn extract_lineage_summary(es_resp: &Value, specs: &[LineageRankSummarySpec]) -> Value {
    let mut summary = serde_json::Map::new();

    for spec in specs {
        let agg_name = format!("lineage_{}", spec.rank);
        let mut rank_map = serde_json::Map::new();

        let ancestor_buckets = es_resp
            .pointer(&format!(
                "/aggregations/{agg_name}/by_rank/by_ancestor/buckets"
            ))
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default();

        for bucket in &ancestor_buckets {
            let Some(ancestor_id) = extract_bucket_key(bucket) else {
                continue;
            };

            let mut field_map = serde_json::Map::new();
            for field in &spec.fields {
                let path = format!("/back_to_root/by_attribute/{field}/by_value");
                let distribution = extract_field_distribution(bucket.pointer(&path));
                field_map.insert(field.clone(), distribution);
            }

            rank_map.insert(ancestor_id, Value::Object(field_map));
        }

        summary.insert(spec.rank.clone(), Value::Object(rank_map));
    }

    Value::Object(summary)
}

/// Extract a string key from an ES bucket, accepting both string and integer keys.
fn extract_bucket_key(bucket: &Value) -> Option<String> {
    bucket.get("key").and_then(|k| {
        k.as_str()
            .map(str::to_string)
            .or_else(|| k.as_u64().map(|n| n.to_string()))
    })
}

/// Convert the `by_value` aggregation result for one field into its distribution.
fn extract_field_distribution(by_value: Option<&Value>) -> Value {
    match by_value {
        // Keyword terms agg: `by_value` has a `buckets` array
        Some(v) if v.get("buckets").is_some() => {
            let mut counts = serde_json::Map::new();
            for vb in v["buckets"].as_array().cloned().unwrap_or_default() {
                let key = vb
                    .get("key_as_string")
                    .or_else(|| vb.get("key"))
                    .and_then(|k| k.as_str())
                    .unwrap_or("unknown");
                let count = vb["doc_count"].as_u64().unwrap_or(0);
                counts.insert(key.to_string(), json!(count));
            }
            Value::Object(counts)
        }
        // Numeric stats agg: `by_value` has a `count` field
        Some(v) if v.get("count").is_some() => v.clone(),
        // No data
        _ => json!({}),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use genomehubs_query::query::LineageRankSummarySpec;

    fn make_spec(rank: &str, fields: &[&str]) -> LineageRankSummarySpec {
        LineageRankSummarySpec {
            rank: rank.to_string(),
            fields: fields.iter().map(|s| s.to_string()).collect(),
        }
    }

    // ── Builder tests ─────────────────────────────────────────────────────────

    #[test]
    fn test_build_lineage_rank_summary_agg_keyword_field() {
        let spec = make_spec("genus", &["assembly_level"]);
        let (name, body) = build_lineage_rank_summary_agg(&spec, 50_000, &None).unwrap();

        assert_eq!(name, "lineage_genus");
        assert_eq!(body["nested"]["path"], "lineage");
        assert_eq!(
            body["aggs"]["by_rank"]["filter"]["term"]["lineage.taxon_rank"],
            "genus"
        );
        assert_eq!(
            body["aggs"]["by_rank"]["aggs"]["by_ancestor"]["terms"]["size"],
            50_000
        );

        // Field sub-agg uses keyword_value.raw (no cache → default)
        let field_agg = &body["aggs"]["by_rank"]["aggs"]["by_ancestor"]["aggs"]["back_to_root"]
            ["aggs"]["by_attribute"]["aggs"]["assembly_level"];
        assert_eq!(
            field_agg["filter"]["term"]["attributes.key"],
            "assembly_level"
        );
        assert_eq!(
            field_agg["aggs"]["by_value"]["terms"]["field"],
            "attributes.keyword_value.raw"
        );
    }

    #[test]
    fn test_build_lineage_rank_summary_agg_multi_field() {
        let spec = make_spec("genus", &["assembly_level", "ebp_standard_date"]);
        let (name, body) = build_lineage_rank_summary_agg(&spec, 50_000, &None).unwrap();

        assert_eq!(name, "lineage_genus");
        let by_attr = &body["aggs"]["by_rank"]["aggs"]["by_ancestor"]["aggs"]["back_to_root"]
            ["aggs"]["by_attribute"]["aggs"];

        assert!(by_attr.get("assembly_level").is_some());
        assert!(by_attr.get("ebp_standard_date").is_some());
    }

    #[test]
    fn test_build_lineage_rank_summary_agg_empty_fields_errors() {
        let spec = make_spec("genus", &[]);
        let result = build_lineage_rank_summary_agg(&spec, 50_000, &None);
        assert!(result.is_err());
    }

    #[test]
    fn test_ancestor_bucket_size_defaults() {
        assert_eq!(ancestor_bucket_size_for_rank("genus"), 50_000);
        assert_eq!(ancestor_bucket_size_for_rank("family"), 10_000);
        assert_eq!(ancestor_bucket_size_for_rank("order"), 2_000);
        assert_eq!(ancestor_bucket_size_for_rank("class"), 500);
        assert_eq!(ancestor_bucket_size_for_rank("phylum"), 10_000); // default
    }

    // ── Extractor tests ───────────────────────────────────────────────────────

    fn mock_es_response_keyword() -> Value {
        json!({
            "aggregations": {
                "lineage_genus": {
                    "by_rank": {
                        "by_ancestor": {
                            "buckets": [
                                {
                                    "key": "10088",
                                    "back_to_root": {
                                        "by_attribute": {
                                            "assembly_level": {
                                                "by_value": {
                                                    "buckets": [
                                                        { "key": "chromosome", "doc_count": 3 },
                                                        { "key": "scaffold",   "doc_count": 2 }
                                                    ]
                                                }
                                            }
                                        }
                                    }
                                },
                                {
                                    "key": "10114",
                                    "back_to_root": {
                                        "by_attribute": {
                                            "assembly_level": {
                                                "by_value": {
                                                    "buckets": []
                                                }
                                            }
                                        }
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        })
    }

    #[test]
    fn test_extract_lineage_summary_keyword() {
        let resp = mock_es_response_keyword();
        let specs = vec![make_spec("genus", &["assembly_level"])];
        let summary = extract_lineage_summary(&resp, &specs);

        let genus = &summary["genus"];
        assert_eq!(genus["10088"]["assembly_level"]["chromosome"], 3);
        assert_eq!(genus["10088"]["assembly_level"]["scaffold"], 2);
    }

    #[test]
    fn test_extract_lineage_summary_empty_field() {
        let resp = mock_es_response_keyword();
        let specs = vec![make_spec("genus", &["assembly_level"])];
        let summary = extract_lineage_summary(&resp, &specs);

        // Genus 10114 has matching species but empty buckets → {}
        assert_eq!(summary["genus"]["10114"]["assembly_level"], json!({}));
    }

    #[test]
    fn test_extract_lineage_summary_absent_genus() {
        let resp = mock_es_response_keyword();
        let specs = vec![make_spec("genus", &["assembly_level"])];
        let summary = extract_lineage_summary(&resp, &specs);

        // Genus 99999 was not in the query result → completely absent
        assert!(summary["genus"].get("99999").is_none());
    }

    #[test]
    fn test_extract_lineage_summary_numeric_stats() {
        let resp = json!({
            "aggregations": {
                "lineage_genus": {
                    "by_rank": {
                        "by_ancestor": {
                            "buckets": [
                                {
                                    "key": "10088",
                                    "back_to_root": {
                                        "by_attribute": {
                                            "busco_completeness": {
                                                "by_value": {
                                                    "count": 12,
                                                    "min": 45.2,
                                                    "max": 98.7,
                                                    "avg": 72.1,
                                                    "sum": 865.2
                                                }
                                            }
                                        }
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        });

        let specs = vec![make_spec("genus", &["busco_completeness"])];
        let summary = extract_lineage_summary(&resp, &specs);

        let stats = &summary["genus"]["10088"]["busco_completeness"];
        assert_eq!(stats["count"], 12);
        assert_eq!(stats["min"], 45.2);
        assert_eq!(stats["avg"], 72.1);
    }

    #[test]
    fn test_extract_lineage_summary_empty_response() {
        let resp = json!({ "aggregations": {} });
        let specs = vec![make_spec("genus", &["assembly_level"])];
        let summary = extract_lineage_summary(&resp, &specs);

        // Graceful: produces empty rank map
        assert_eq!(summary["genus"], json!({}));
    }

    #[test]
    fn test_extract_lineage_summary_integer_key() {
        // ES sometimes returns taxon_id as integer rather than string
        let resp = json!({
            "aggregations": {
                "lineage_genus": {
                    "by_rank": {
                        "by_ancestor": {
                            "buckets": [
                                {
                                    "key": 10088,
                                    "back_to_root": {
                                        "by_attribute": {
                                            "assembly_level": {
                                                "by_value": {
                                                    "buckets": [
                                                        { "key": "chromosome", "doc_count": 1 }
                                                    ]
                                                }
                                            }
                                        }
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        });

        let specs = vec![make_spec("genus", &["assembly_level"])];
        let summary = extract_lineage_summary(&resp, &specs);

        // Key is stringified
        assert_eq!(summary["genus"]["10088"]["assembly_level"]["chromosome"], 1);
    }

    // ── Validation tests ──────────────────────────────────────────────────────

    fn make_cache_with_fields(fields: &[&str]) -> Option<Arc<tokio::sync::RwLock<MetadataCache>>> {
        let mut field_map = serde_json::Map::new();
        for f in fields {
            field_map.insert(f.to_string(), json!({ "processed_type": "keyword" }));
        }
        let attr_types = json!({ "default": Value::Object(field_map) });
        let cache = MetadataCache {
            attr_types,
            ..Default::default()
        };
        Some(Arc::new(tokio::sync::RwLock::new(cache)))
    }

    fn make_cache_with_typed(
        entries: &[(&str, &str)],
    ) -> Option<Arc<tokio::sync::RwLock<MetadataCache>>> {
        let mut field_map = serde_json::Map::new();
        for (f, t) in entries {
            field_map.insert(f.to_string(), json!({ "processed_type": t }));
        }
        let attr_types = json!({ "default": Value::Object(field_map) });
        let cache = MetadataCache {
            attr_types,
            ..Default::default()
        };
        Some(Arc::new(tokio::sync::RwLock::new(cache)))
    }

    #[test]
    fn test_build_lineage_rank_summary_agg_date_field() {
        let cache = make_cache_with_typed(&[("assembly_date", "date")]);
        let spec = make_spec("genus", &["assembly_date"]);
        let (_, body) = build_lineage_rank_summary_agg(&spec, 50_000, &cache).unwrap();

        let field_agg = &body["aggs"]["by_rank"]["aggs"]["by_ancestor"]["aggs"]["back_to_root"]
            ["aggs"]["by_attribute"]["aggs"]["assembly_date"];
        // Date fields use stats on date_value, not terms on keyword_value.raw
        assert_eq!(
            field_agg["aggs"]["by_value"]["stats"]["field"], "attributes.date_value",
            "date field should use stats on date_value"
        );
    }
    #[test]
    fn test_validate_fields_all_known() {
        let cache = make_cache_with_fields(&["assembly_level", "genome_size"]);
        let specs = vec![make_spec("genus", &["assembly_level", "genome_size"])];
        assert!(validate_lineage_rank_summary_fields(&specs, &cache).is_ok());
    }

    #[test]
    fn test_validate_fields_unknown_returns_error() {
        let cache = make_cache_with_fields(&["assembly_level"]);
        let specs = vec![make_spec("genus", &["assembly_level", "null_field"])];
        let err = validate_lineage_rank_summary_fields(&specs, &cache).unwrap_err();
        assert!(
            err.contains("null_field"),
            "error should name the bad field: {err}"
        );
    }

    #[test]
    fn test_validate_fields_multiple_specs_deduplicated() {
        let cache = make_cache_with_fields(&["assembly_level"]);
        let specs = vec![
            make_spec("genus", &["unknown_a", "assembly_level"]),
            make_spec("family", &["unknown_b", "unknown_a"]),
        ];
        let err = validate_lineage_rank_summary_fields(&specs, &cache).unwrap_err();
        assert!(err.contains("unknown_a"), "{err}");
        assert!(err.contains("unknown_b"), "{err}");
        // Listed only once each (BTreeSet dedup)
        assert_eq!(err.matches("unknown_a").count(), 1);
    }

    #[test]
    fn test_validate_fields_no_cache_passes() {
        let specs = vec![make_spec("genus", &["totally_nonexistent"])];
        assert!(validate_lineage_rank_summary_fields(&specs, &None).is_ok());
    }
}
