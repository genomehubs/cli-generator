//! Per-report-type handler functions.
//!
//! Each handler issues ES queries, applies bounds/aggregation/pipeline logic,
//! and returns structured report data.

use genomehubs_query::query::{QueryParams, SearchQuery};
use genomehubs_query::report::axis::{AxisRole, AxisSpec, AxisSummary, ValueType};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::es_client;
use crate::report::bounds::compute_bounds;
use crate::report::agg::{agg_builder_for, build_nested_attribute_histogram_with_categories};
use crate::report::pipeline::{Pipeline, ReportContext, ScaleStep};
use crate::AppState;

/// Extract per-category per-bucket counts from a v2-pattern `categoryHistograms` response.
///
/// For each category label the function follows:
/// `.../categoryHistograms/by_attribute/by_cat/by_value/buckets/{label}/histogram/by_attribute/{x_field}/histogram/buckets`
///
/// Returns a JSON object mapping each category key to an array of `doc_count` values, one per
/// main-histogram bucket. Includes an `"other"` key when `show_other` is true.
fn extract_cat_histograms(
    resp: &Value,
    agg_name: &str,
    x_field: &str,
    main_bucket_count: usize,
    cat_labels: &[String],
    show_other: bool,
    main_counts: &[u64],
) -> Value {
    let base = format!(
        "/aggregations/{}/by_key/categoryHistograms/by_attribute/by_cat/by_value/buckets",
        agg_name
    );

    if resp.pointer(&base).is_none() {
        return Value::Null;
    }

    let mut by_cat = serde_json::Map::new();
    let mut named_sums: Vec<Vec<u64>> = Vec::with_capacity(cat_labels.len());

    for label in cat_labels {
        let hist_path = format!("{}/{}/histogram/by_attribute/{}/histogram/buckets", base, label, x_field);
        let mut counts: Vec<u64> = resp
            .pointer(&hist_path)
            .and_then(|b| b.as_array())
            .map(|buckets| {
                buckets
                    .iter()
                    .map(|b| b.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0))
                    .collect()
            })
            .unwrap_or_default();
        counts.resize(main_bucket_count, 0);
        named_sums.push(counts.clone());
        by_cat.insert(label.clone(), json!(counts));
    }

    if show_other {
        let other_path = format!("{}/other/histogram/by_attribute/{}/histogram/buckets", base, x_field);
        let other_counts: Vec<u64> = if let Some(buckets) = resp.pointer(&other_path).and_then(|b| b.as_array()) {
            let mut v: Vec<u64> = buckets
                .iter()
                .map(|b| b.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0))
                .collect();
            v.resize(main_bucket_count, 0);
            v
        } else {
            (0..main_bucket_count)
                .map(|i| {
                    let cat_sum: u64 = named_sums.iter().map(|c| c.get(i).copied().unwrap_or(0)).sum();
                    main_counts.get(i).copied().unwrap_or(0).saturating_sub(cat_sum)
                })
                .collect()
        };
        by_cat.insert("other".to_string(), json!(other_counts));
    }

    if by_cat.is_empty() {
        Value::Null
    } else {
        Value::Object(by_cat)
    }
}

/// Run a histogram (or categorised histogram) report.
///
/// Returns `(doc_count, took_ms, report_json)` or error.
pub async fn run_histogram_report(
    state: &Arc<AppState>,
    index: &str,
    _search_query: &SearchQuery,
    _params: &QueryParams,
    report_config: &serde_yaml::Value,
    base_query: &Value,
) -> Result<(u64, u64, Value), String> {
    let x_field = report_config
        .get("x")
        .and_then(|v| v.as_str())
        .ok_or("report_yaml missing 'x' field")?;
    let x_opts_str = report_config
        .get("x_opts")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let cat_field = report_config.get("cat").and_then(|v| v.as_str());

    let x_value_type = infer_value_type(x_field, &state.cache);
    let x_spec = AxisSpec {
        field: x_field.to_string(),
        role: AxisRole::X,
        summary: AxisSummary::default(),
        value_type: x_value_type,
        opts: x_opts_str.parse().unwrap_or_default(),
    };

    let x_bounds = compute_bounds(&state.client, &state.es_base, index, &x_spec, base_query, &state.cache).await?;

    let agg_name = "x_agg";

    // Build aggregation — categorized path uses v2-pattern categoryHistograms.
    let (final_agg, cat_labels, show_other_cat) = if let Some(cat) = cat_field {
        let cat_spec = build_cat_spec(cat, report_config, state);
        let cat_bounds =
            compute_bounds(&state.client, &state.es_base, index, &cat_spec, base_query, &state.cache).await?;
        let labels = cat_bounds.cat_labels.clone();
        let show_other = cat_spec.opts.show_other;
        let agg = build_nested_attribute_histogram_with_categories(
            agg_name,
            x_field,
            &x_bounds,
            cat,
            &labels,
            show_other,
            &state.cache,
        )?;
        (agg, labels, show_other)
    } else {
        let x_agg = agg_builder_for(&x_spec, &x_bounds, &state.cache)?;
        (x_agg.build(agg_name), vec![], false)
    };

    let es_body = json!({ "size": 0, "query": base_query, "aggs": final_agg });
    let resp = es_client::execute_search(&state.client, &state.es_base, index, &es_body).await?;

    let took = resp.get("took").and_then(|t| t.as_u64()).unwrap_or(0);
    let total_hits = resp
        .pointer("/hits/total/value")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    // Extract main histogram buckets (path is the same whether categorized or not).
    let x_agg = agg_builder_for(&x_spec, &x_bounds, &state.cache)?;
    let raw_buckets = x_agg.extract(&resp, agg_name);

    let main_counts: Vec<u64> = raw_buckets
        .iter()
        .map(|b| b.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0))
        .collect();

    let by_cat = if cat_field.is_some() && !cat_labels.is_empty() {
        extract_cat_histograms(
            &resp,
            agg_name,
            x_field,
            raw_buckets.len(),
            &cat_labels,
            show_other_cat,
            &main_counts,
        )
    } else {
        Value::Null
    };

    let pipeline = Pipeline::new().add(ScaleStep);
    let ctx = ReportContext {
        scale: x_spec.opts.scale,
        cat_labels: x_bounds.cat_labels.clone(),
        show_other: x_spec.opts.show_other,
    };
    let buckets = pipeline.run(raw_buckets, &ctx);

    let mut report_data = json!({
        "type": "histogram",
        "x": {
            "field": x_field,
            "scale": format!("{:?}", x_spec.opts.scale).to_lowercase(),
            "domain": x_bounds.domain
        },
        "buckets": buckets
    });

    if !by_cat.is_null() {
        report_data["by_cat"] = by_cat;
        report_data["cat"] = json!(cat_field);
        report_data["cats"] = json!(cat_labels);
    }

    Ok((total_hits, took, report_data))
}

/// Run an xPerRank report (values per taxonomic rank).
pub async fn run_x_per_rank_report(
    state: &Arc<AppState>,
    index: &str,
    base_query: &Value,
    report_config: &serde_yaml::Value,
) -> Result<(u64, u64, Value), String> {
    let x_field = report_config
        .get("x")
        .and_then(|v| v.as_str())
        .ok_or("report_yaml missing 'x' field")?;

    let es_body = json!({
        "size": 0,
        "query": base_query,
        "aggs": {
            "by_rank": {
                "terms": { "field": "taxon_rank", "size": 20 },
                "aggs": {
                    "field_stats": { "stats": { "field": x_field } },
                    "value_count": { "value_count": { "field": x_field } }
                }
            }
        }
    });

    let resp = es_client::execute_search(&state.client, &state.es_base, index, &es_body).await?;
    let took = resp.get("took").and_then(|t| t.as_u64()).unwrap_or(0);
    let total_hits = resp
        .pointer("/hits/total/value")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let buckets = resp
        .pointer("/aggregations/by_rank/buckets")
        .cloned()
        .unwrap_or_default();

    let report_data = json!({
        "type": "xPerRank",
        "x": x_field,
        "buckets": buckets
    });

    Ok((total_hits, took, report_data))
}

/// Run a sources report (top sources by count).
pub async fn run_sources_report(
    state: &Arc<AppState>,
    index: &str,
    base_query: &Value,
) -> Result<(u64, u64, Value), String> {
    let es_body = json!({
        "size": 0,
        "query": base_query,
        "aggs": {
            "sources": {
                "terms": { "field": "sources.keyword", "size": 50 }
            }
        }
    });

    let resp = es_client::execute_search(&state.client, &state.es_base, index, &es_body).await?;
    let took = resp.get("took").and_then(|t| t.as_u64()).unwrap_or(0);
    let total_hits = resp
        .pointer("/hits/total/value")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let buckets = resp
        .pointer("/aggregations/sources/buckets")
        .cloned()
        .unwrap_or_default();

    let report_data = json!({
        "type": "sources",
        "buckets": buckets
    });

    Ok((total_hits, took, report_data))
}

/// Run a tree report (hierarchical taxonomy).
pub async fn run_tree_report(
    state: &Arc<AppState>,
    index: &str,
    base_query: &Value,
    report_config: &serde_yaml::Value,
) -> Result<(u64, u64, Value), String> {
    let rank_field = report_config
        .get("rank")
        .and_then(|v| v.as_str())
        .unwrap_or("phylum");
    let depth = report_config
        .get("depth")
        .and_then(|v| v.as_u64())
        .unwrap_or(5) as usize;

    let es_body = json!({
        "size": 0,
        "query": base_query,
        "aggs": {
            "lineage": {
                "nested": { "path": "lineage" },
                "aggs": {
                    "by_rank": {
                        "filter": { "term": { "lineage.taxon_rank": rank_field } },
                        "aggs": {
                            "names": {
                                "terms": { "field": "lineage.scientific_name.keyword", "size": depth * 10 },
                                "aggs": {
                                    "count": { "reverse_nested": {} }
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    let resp = es_client::execute_search(&state.client, &state.es_base, index, &es_body).await?;
    let took = resp.get("took").and_then(|t| t.as_u64()).unwrap_or(0);
    let total_hits = resp
        .pointer("/hits/total/value")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let buckets = resp
        .pointer("/aggregations/lineage/by_rank/names/buckets")
        .cloned()
        .unwrap_or_default();

    let newick = buckets_to_newick(&buckets);

    let report_data = json!({
        "type": "tree",
        "newick": newick,
        "buckets": buckets
    });

    Ok((total_hits, took, report_data))
}

/// Run a map report (geohash grid).
pub async fn run_map_report(
    state: &Arc<AppState>,
    index: &str,
    base_query: &Value,
    report_config: &serde_yaml::Value,
) -> Result<(u64, u64, Value), String> {
    let geo_field = report_config
        .get("x")
        .and_then(|v| v.as_str())
        .unwrap_or("location");
    let size = report_config
        .get("size")
        .and_then(|v| v.as_u64())
        .unwrap_or(500) as usize;
    let precision = report_config
        .get("precision")
        .and_then(|v| v.as_u64())
        .unwrap_or(4) as u8;

    let es_body = json!({
        "size": 0,
        "query": base_query,
        "aggs": {
            "geo_grid": {
                "geohash_grid": {
                    "field": geo_field,
                    "precision": precision,
                    "size": size
                }
            }
        }
    });

    let resp = es_client::execute_search(&state.client, &state.es_base, index, &es_body).await?;
    let took = resp.get("took").and_then(|t| t.as_u64()).unwrap_or(0);
    let total_hits = resp
        .pointer("/hits/total/value")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let buckets = resp
        .pointer("/aggregations/geo_grid/buckets")
        .cloned()
        .unwrap_or_default();

    let report_data = json!({
        "type": "map",
        "field": geo_field,
        "buckets": buckets
    });

    Ok((total_hits, took, report_data))
}

// ============================================================================
// Helper functions
// ============================================================================

/// Infer the ValueType of a field from metadata cache.
/// Defaults to Numeric if not found or cache unavailable.
fn infer_value_type(field: &str, cache: &Option<Arc<tokio::sync::RwLock<crate::es_metadata::MetadataCache>>>) -> ValueType {
    // Check if it's a rank in the metadata
    if let Some(cache_lock) = cache {
        if let Ok(c) = cache_lock.try_read() {
            if c.taxonomic_ranks.contains(&field.to_string()) {
                return ValueType::TaxonRank;
            }

            // Check if it's an attribute in the metadata
            if let serde_json::Value::Object(groups) = &c.attr_types {
                for (_, group) in groups {
                    if let serde_json::Value::Object(fields) = group {
                        if let Some(field_meta) = fields.get(field) {
                            if let serde_json::Value::Object(meta_obj) = field_meta {
                                if let Some(type_str) = meta_obj.get("type").and_then(|v| v.as_str()) {
                                    return match type_str {
                                        "date" => ValueType::Date,
                                        "keyword" => ValueType::Keyword,
                                        "long" | "integer" | "float" | "double" => ValueType::Numeric,
                                        "geo_point" => ValueType::GeoPoint,
                                        _ => ValueType::Keyword,
                                    };
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    // Default to Numeric if not found in metadata
    ValueType::Numeric
}

/// Build an AxisSpec for a category field.
fn build_cat_spec(cat: &str, report_config: &serde_yaml::Value, state: &Arc<AppState>) -> AxisSpec {
    let cat_opts_str = report_config
        .get("cat_opts")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    AxisSpec {
        field: cat.to_string(),
        role: AxisRole::Cat,
        summary: AxisSummary::default(),
        value_type: infer_value_type(cat, &state.cache),
        opts: cat_opts_str.parse().unwrap_or_default(),
    }
}

/// Serialize taxonomy hierarchy to Newick format.
fn buckets_to_newick(buckets: &Value) -> String {
    let arr = match buckets.as_array() {
        Some(a) => a,
        None => return "();".to_string(),
    };

    let nodes: Vec<String> = arr
        .iter()
        .map(|b| {
            let name = b.get("key").and_then(|k| k.as_str()).unwrap_or("?");
            let count = b
                .pointer("/count/doc_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            format!("{name}:{count}")
        })
        .collect();

    format!("({});", nodes.join(","))
}
