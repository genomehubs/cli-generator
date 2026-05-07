//! Per-report-type handler functions.
//!
//! Each handler issues ES queries, applies bounds/aggregation/pipeline logic,
//! and returns structured report data.

use genomehubs_query::query::{QueryParams, SearchQuery};
use genomehubs_query::report::axis::{AxisInput, AxisRole, AxisSpec, AxisSummary, ValueType};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::es_client;
use crate::report::agg::{
    agg_builder_for, build_nested_attribute_histogram_with_categories,
    build_nested_attribute_scatter_agg, x_bucket_agg_name,
};
use crate::report::bounds::compute_bounds;
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
    x_bucket_agg: &str,
    main_bucket_count: usize,
    cat_labels: &[String],
    show_other: bool,
    cat_is_numeric: bool,
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

    if cat_is_numeric {
        // by_value uses a histogram agg — buckets is an array of { key, histogram: {…} }.
        let cat_buckets = resp
            .pointer(&base)
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default();
        for bucket in &cat_buckets {
            let key = bucket.get("key").and_then(|k| k.as_f64()).unwrap_or(0.0);
            let label = key.to_string();
            let hist_path = format!(
                "/histogram/by_attribute/{}/{}/buckets",
                x_field, x_bucket_agg
            );
            let mut counts: Vec<u64> = bucket
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
            by_cat.insert(label, json!(counts));
        }
    } else {
        // by_value uses a filters agg — buckets is an object keyed by label.
        let mut named_sums: Vec<Vec<u64>> = Vec::with_capacity(cat_labels.len());

        for label in cat_labels {
            let hist_path = format!(
                "{}/{}/histogram/by_attribute/{}/{}/buckets",
                base, label, x_field, x_bucket_agg
            );
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
            let other_path = format!(
                "{}/other/histogram/by_attribute/{}/{}/buckets",
                base, x_field, x_bucket_agg
            );
            let other_counts: Vec<u64> =
                if let Some(buckets) = resp.pointer(&other_path).and_then(|b| b.as_array()) {
                    let mut v: Vec<u64> = buckets
                        .iter()
                        .map(|b| b.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0))
                        .collect();
                    v.resize(main_bucket_count, 0);
                    v
                } else {
                    (0..main_bucket_count)
                        .map(|i| {
                            let cat_sum: u64 = named_sums
                                .iter()
                                .map(|c| c.get(i).copied().unwrap_or(0))
                                .sum();
                            main_counts
                                .get(i)
                                .copied()
                                .unwrap_or(0)
                                .saturating_sub(cat_sum)
                        })
                        .collect()
                };
            by_cat.insert("other".to_string(), json!(other_counts));
        }
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
    let x_spec = resolve_axis_spec(AxisRole::X, report_config, state)
        .ok_or("report config missing 'x' axis (set 'x' field or use 'axes')")?;
    let x_field = x_spec.field.clone();
    let cat_spec_opt = resolve_axis_spec(AxisRole::Cat, report_config, state);

    let x_bounds = compute_bounds(
        &state.client,
        &state.es_base,
        index,
        &x_spec,
        base_query,
        &state.cache,
    )
    .await?;

    let agg_name = "x_agg";

    let x_inner_agg = x_bucket_agg_name(x_spec.value_type);

    // Build aggregation — categorized path supports both keyword (filters) and numeric (histogram) cat.
    let (final_agg, cat_labels, show_other_cat, cat_is_numeric) =
        if let Some(ref cat_spec) = cat_spec_opt {
            let cat_bounds = compute_bounds(
                &state.client,
                &state.es_base,
                index,
                cat_spec,
                base_query,
                &state.cache,
            )
            .await?;
            let labels = cat_bounds.cat_labels.clone();
            let show_other = cat_spec.opts.show_other;
            let is_numeric = !matches!(
                cat_spec.value_type,
                ValueType::Keyword | ValueType::TaxonRank
            );
            let agg = build_nested_attribute_histogram_with_categories(
                agg_name,
                &x_spec,
                &x_bounds,
                cat_spec.field.as_str(),
                cat_spec.value_type,
                &cat_bounds,
                &labels,
                show_other,
                &state.cache,
            )?;
            (agg, labels, show_other, is_numeric)
        } else {
            let x_agg = agg_builder_for(&x_spec, &x_bounds, &state.cache)?;
            (x_agg.build(agg_name), vec![], false, false)
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

    let by_cat = if !cat_labels.is_empty() || cat_is_numeric {
        extract_cat_histograms(
            &resp,
            agg_name,
            x_field.as_str(),
            x_inner_agg,
            raw_buckets.len(),
            &cat_labels,
            show_other_cat,
            cat_is_numeric,
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
    let processed_buckets = pipeline.run(raw_buckets.clone(), &ctx);

    // allValues: flat array of doc_counts parallel to buckets.
    let all_values: Vec<u64> = raw_buckets
        .iter()
        .map(|b| b.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0))
        .collect();

    let mut report_data = json!({
        "type": "histogram",
        "x": {
            "field": &x_field,
            "scale": format!("{:?}", x_spec.opts.scale).to_lowercase(),
            "domain": x_bounds.domain,
            "tickCount": x_bounds.tick_count
        },
        "buckets": processed_buckets,
        "allValues": all_values
    });

    if !by_cat.is_null() {
        report_data["by_cat"] = by_cat;
        report_data["cat"] = json!(cat_spec_opt.as_ref().map(|s| s.field.as_str()));
        report_data["cats"] = json!(cat_labels);
    }

    Ok((total_hits, took, report_data))
}

/// Run an xPerRank report (values per taxonomic rank).
pub async fn run_x_per_rank_report(
    state: &Arc<AppState>,
    index: &str,
    base_query: &Value,
    _report_config: &serde_yaml::Value,
) -> Result<(u64, u64, Value), String> {
    // Group by taxon_rank and return counts at each rank.
    // Response format: simplified v3 style with just rank and count per bucket.
    let es_body = json!({
        "size": 0,
        "query": base_query,
        "aggs": {
            "by_rank": {
                "terms": { "field": "taxon_rank", "size": 100 }
            }
        }
    });

    let resp = es_client::execute_search(&state.client, &state.es_base, index, &es_body).await?;
    let took = resp.get("took").and_then(|t| t.as_u64()).unwrap_or(0);
    let total_hits = resp
        .pointer("/hits/total/value")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    // Transform ES buckets to v3 format: simplify to just rank + count.
    let es_buckets = resp
        .pointer("/aggregations/by_rank/buckets")
        .and_then(|b| b.as_array())
        .cloned()
        .unwrap_or_default();

    let buckets: Vec<Value> = es_buckets
        .iter()
        .map(|bucket| {
            let rank = bucket
                .get("key")
                .and_then(|k| k.as_str())
                .unwrap_or("unknown");
            let count = bucket
                .get("doc_count")
                .and_then(|c| c.as_u64())
                .unwrap_or(0);
            json!({
                "rank": rank,
                "count": count
            })
        })
        .collect();

    let report_data = json!({
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
    // Aggregate attributes (nested) -> fields -> sources, collecting metadata per source.
    // This mirrors the v2 implementation to extract field associations, URLs, and dates.
    let es_body = json!({
        "size": 0,
        "query": base_query,
        "aggs": {
            "attributes": {
                "nested": { "path": "attributes" },
                "aggs": {
                    "direct": {
                        "filter": { "match": { "attributes.aggregation_source": "direct" } },
                        "aggs": {
                            "fields": {
                                "terms": { "field": "attributes.key", "size": 200 },
                                "aggs": {
                                    "summary": {
                                        "nested": { "path": "attributes.values" },
                                        "aggs": {
                                            "terms": {
                                                "terms": { "field": "attributes.values.source.raw", "size": 200 },
                                                "aggs": {
                                                    "min_date": {
                                                        "min": { "field": "attributes.values.source_date", "format": "yyyy-MM-dd" }
                                                    },
                                                    "max_date": {
                                                        "max": { "field": "attributes.values.source_date", "format": "yyyy-MM-dd" }
                                                    },
                                                    "url": {
                                                        "terms": { "field": "attributes.values.source_url", "size": 1 }
                                                    }
                                                }
                                            }
                                        }
                                    }
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

    // Extract nested aggregation structure and build sources object.
    // For each field, iterate through sources and build: {source_name: {count, attributes, url, date}}
    let mut sources_map: std::collections::BTreeMap<String, serde_json::Map<String, Value>> =
        std::collections::BTreeMap::new();

    if let Some(fields_agg) = resp.pointer("/aggregations/attributes/direct/fields/buckets") {
        if let Some(field_buckets) = fields_agg.as_array() {
            for field_bucket in field_buckets {
                let field_name = field_bucket
                    .get("key")
                    .and_then(|k| k.as_str())
                    .unwrap_or("unknown");

                // Navigate into summary -> terms buckets
                if let Some(source_buckets) = field_bucket.pointer("/summary/terms/buckets") {
                    if let Some(sources) = source_buckets.as_array() {
                        for source_bucket in sources {
                            let source_name = source_bucket
                                .get("key")
                                .and_then(|k| k.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            let count = source_bucket
                                .get("doc_count")
                                .and_then(|c| c.as_u64())
                                .unwrap_or(0);

                            // Get or create source entry
                            let entry = sources_map
                                .entry(source_name.clone())
                                .or_insert_with(serde_json::Map::new);

                            // Update count
                            if let Some(existing_count) =
                                entry.get("count").and_then(|c| c.as_u64())
                            {
                                entry.insert("count".to_string(), json!(existing_count + count));
                            } else {
                                entry.insert("count".to_string(), json!(count));
                            }

                            // Add field to attributes list
                            let attrs = entry
                                .entry("attributes".to_string())
                                .or_insert_with(|| json!(Vec::<String>::new()));
                            if let Some(arr) = attrs.as_array_mut() {
                                if !arr.iter().any(|v| v.as_str() == Some(field_name)) {
                                    arr.push(json!(field_name));
                                }
                            }

                            // Extract and set URL if present
                            if let Some(url_agg) = source_bucket.pointer("/url/buckets") {
                                if let Some(url_buckets) = url_agg.as_array() {
                                    if let Some(first_url) = url_buckets.first() {
                                        if let Some(url) = first_url.get("key") {
                                            entry.insert("url".to_string(), url.clone());
                                        }
                                    }
                                }
                            }

                            // Extract date range (use min as date if available)
                            if let Some(min_date) = source_bucket
                                .pointer("/min_date/value_as_string")
                                .and_then(|v| v.as_str())
                            {
                                entry.insert("date".to_string(), json!(min_date));
                            }
                        }
                    }
                }
            }
        }
    }

    // Convert BTreeMap to regular JSON object
    let mut sources_obj = serde_json::Map::new();
    for (source_name, mut source_data) in sources_map {
        // Ensure attributes is sorted for consistency
        if let Some(attrs) = source_data.get_mut("attributes") {
            if let Some(arr) = attrs.as_array_mut() {
                arr.sort_by(|a, b| a.as_str().unwrap_or("").cmp(b.as_str().unwrap_or("")));
            }
        }
        sources_obj.insert(source_name, Value::Object(source_data));
    }

    let report_data = json!({
        "sources": sources_obj
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
fn infer_value_type(
    field: &str,
    cache: &Option<Arc<tokio::sync::RwLock<crate::es_metadata::MetadataCache>>>,
) -> ValueType {
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
                                if let Some(type_str) =
                                    meta_obj.get("type").and_then(|v| v.as_str())
                                {
                                    return match type_str {
                                        "date" => ValueType::Date,
                                        "keyword" => ValueType::Keyword,
                                        "long" | "integer" | "float" | "double" => {
                                            ValueType::Numeric
                                        }
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

/// Resolve an [`AxisSpec`] for the given role from a report config.
///
/// Checks the structured `axes` array first. Falls back to legacy flat keys
/// (`x`/`x_opts`, `y`/`y_opts`, `cat`/`cat_opts`) so existing request bodies
/// continue to work unchanged.
fn resolve_axis_spec(
    role: AxisRole,
    report_config: &serde_yaml::Value,
    state: &Arc<AppState>,
) -> Option<AxisSpec> {
    let role_str = match role {
        AxisRole::X => "x",
        AxisRole::Y => "y",
        AxisRole::Z => "z",
        AxisRole::Cat => "cat",
    };

    // Structured `axes` array (preferred v3 format)
    if let Some(axes) = report_config.get("axes").and_then(|a| a.as_sequence()) {
        for entry in axes {
            if entry.get("position").and_then(|p| p.as_str()) != Some(role_str) {
                continue;
            }
            if let Ok(input) = serde_yaml::from_value::<AxisInput>(entry.clone()) {
                let inferred = infer_value_type(&input.field, &state.cache);
                return Some(input.into_spec(inferred));
            }
        }
    }

    // Legacy flat keys fallback (`x`, `x_opts`, `cat`, `cat_opts`, …)
    let field = report_config.get(role_str).and_then(|v| v.as_str())?;
    let opts_key = if role == AxisRole::Cat {
        "cat_opts".to_string()
    } else {
        format!("{}_opts", role_str)
    };
    let opts_str = report_config
        .get(&opts_key)
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let value_type = infer_value_type(field, &state.cache);
    Some(AxisSpec {
        field: field.to_string(),
        role,
        summary: AxisSummary::default(),
        value_type,
        opts: opts_str.parse().unwrap_or_default(),
    })
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

// ============================================================================
// Scatter helpers
// ============================================================================

/// Find the first numeric attribute value for `field` in an `attributes` array.
fn find_attr_numeric(attrs: &[Value], field: &str) -> Option<f64> {
    for attr in attrs {
        if attr.get("key").and_then(|k| k.as_str()) != Some(field) {
            continue;
        }
        for value_key in &["long_value", "integer_value", "float_value", "double_value"] {
            if let Some(v) = attr.get(value_key).and_then(|v| v.as_f64()) {
                return Some(v);
            }
        }
    }
    None
}

/// Find the first keyword attribute value for `field` in an `attributes` array.
fn find_attr_keyword(attrs: &[Value], field: &str) -> Option<String> {
    attrs
        .iter()
        .find(|a| a.get("key").and_then(|k| k.as_str()) == Some(field))
        .and_then(|a| {
            a.get("keyword_value")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
}

/// Extract per-category per-x-bucket counts and per-x-bucket y-counts from a scatter response.
///
/// Returns `(by_cat, y_values_by_cat)`:
/// - `by_cat`: `{label: [count per x-bucket]}`
/// - `y_values_by_cat`: `{label: [[y-counts per x-bucket]]}`
fn extract_scatter_by_cat(
    resp: &Value,
    agg_name: &str,
    x_field: &str,
    x_bucket_agg: &str,
    y_field: &str,
    x_bucket_count: usize,
    y_bucket_count: usize,
    cat_labels: &[String],
    show_other: bool,
    cat_is_numeric: bool,
    main_counts: &[u64],
) -> (Value, Value) {
    let base = format!(
        "/aggregations/{}/by_key/categoryHistograms/by_attribute/by_cat/by_value/buckets",
        agg_name
    );

    if resp.pointer(&base).is_none() {
        return (Value::Null, Value::Null);
    }

    let mut by_cat = serde_json::Map::new();
    let mut y_values_by_cat = serde_json::Map::new();

    if cat_is_numeric {
        // by_value uses a histogram agg — buckets is an array of { key, histogram: {…} }.
        let cat_buckets = resp
            .pointer(&base)
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default();
        for bucket in &cat_buckets {
            let key = bucket.get("key").and_then(|k| k.as_f64()).unwrap_or(0.0);
            let label = key.to_string();
            let x_path = format!(
                "/histogram/by_attribute/{}/{}/buckets",
                x_field, x_bucket_agg
            );
            let x_buckets_inner = bucket
                .pointer(&x_path)
                .and_then(|b| b.as_array())
                .cloned()
                .unwrap_or_default();
            let mut x_counts: Vec<u64> = Vec::with_capacity(x_bucket_count);
            let mut y_counts_per_x: Vec<Vec<u64>> = Vec::with_capacity(x_bucket_count);
            for x_bucket in &x_buckets_inner {
                x_counts.push(
                    x_bucket
                        .get("doc_count")
                        .and_then(|c| c.as_u64())
                        .unwrap_or(0),
                );
                let y_path = format!("/yHistograms/by_attribute/{}/histogram/buckets", y_field);
                let y_counts = x_bucket
                    .pointer(&y_path)
                    .and_then(|b| b.as_array())
                    .map(|yb| {
                        yb.iter()
                            .map(|b| b.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0))
                            .collect()
                    })
                    .unwrap_or_else(|| vec![0; y_bucket_count]);
                y_counts_per_x.push(y_counts);
            }
            x_counts.resize(x_bucket_count, 0);
            y_counts_per_x.resize(x_bucket_count, vec![0; y_bucket_count]);
            by_cat.insert(label.clone(), json!(x_counts));
            y_values_by_cat.insert(label, json!(y_counts_per_x));
        }
    } else {
        let mut named_x_sums: Vec<Vec<u64>> = Vec::new();
        let all_labels: Vec<&str> = {
            let mut v: Vec<&str> = cat_labels.iter().map(String::as_str).collect();
            if show_other {
                v.push("other");
            }
            v
        };

        for label in &all_labels {
            let x_hist_path = format!(
                "{}/{}/histogram/by_attribute/{}/{}/buckets",
                base, label, x_field, x_bucket_agg
            );
            let x_buckets = resp
                .pointer(&x_hist_path)
                .and_then(|b| b.as_array())
                .cloned()
                .unwrap_or_default();
            let mut x_counts: Vec<u64> = Vec::with_capacity(x_bucket_count);
            let mut y_counts_per_x: Vec<Vec<u64>> = Vec::with_capacity(x_bucket_count);

            for x_bucket in &x_buckets {
                x_counts.push(
                    x_bucket
                        .get("doc_count")
                        .and_then(|c| c.as_u64())
                        .unwrap_or(0),
                );
                let y_hist_path =
                    format!("/yHistograms/by_attribute/{}/histogram/buckets", y_field);
                let y_counts = x_bucket
                    .pointer(&y_hist_path)
                    .and_then(|b| b.as_array())
                    .map(|yb| {
                        yb.iter()
                            .map(|b| b.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0))
                            .collect()
                    })
                    .unwrap_or_else(|| vec![0; y_bucket_count]);
                y_counts_per_x.push(y_counts);
            }
            x_counts.resize(x_bucket_count, 0);
            y_counts_per_x.resize(x_bucket_count, vec![0; y_bucket_count]);

            if *label == "other" && x_counts.iter().all(|&c| c == 0) {
                x_counts = (0..x_bucket_count)
                    .map(|i| {
                        let cat_sum: u64 = named_x_sums
                            .iter()
                            .map(|c| c.get(i).copied().unwrap_or(0))
                            .sum();
                        main_counts
                            .get(i)
                            .copied()
                            .unwrap_or(0)
                            .saturating_sub(cat_sum)
                    })
                    .collect();
            } else {
                named_x_sums.push(x_counts.clone());
            }
            by_cat.insert(label.to_string(), json!(x_counts));
            y_values_by_cat.insert(label.to_string(), json!(y_counts_per_x));
        }
    }

    (Value::Object(by_cat), Value::Object(y_values_by_cat))
}

/// Compute z-domain `[min_nonzero, max]` over all y-bucket counts across all x-buckets.
fn compute_z_domain(all_y_values: &[Vec<u64>]) -> [u64; 2] {
    let mut z_min = u64::MAX;
    let mut z_max = 0u64;
    for row in all_y_values {
        for &v in row {
            if v > 0 {
                z_min = z_min.min(v);
                z_max = z_max.max(v);
            }
        }
    }
    if z_max == 0 {
        [0, 0]
    } else {
        [z_min, z_max]
    }
}

/// Fetch raw point data for scatter when total hits are within the scatter threshold.
///
/// Returns an object mapping category name to an array of `{scientific_name, taxonId, x, y, cat}`
/// point objects. Falls back to a single "all" key when no category is specified.
async fn fetch_raw_point_data(
    state: &Arc<AppState>,
    index: &str,
    base_query: &Value,
    x_field: &str,
    y_field: &str,
    cat_field: Option<&str>,
    cat_labels: &[String],
    show_other: bool,
    threshold: usize,
) -> Value {
    let es_body = json!({
        "size": threshold,
        "query": base_query,
        "_source": ["scientific_name", "taxon_id", "attributes"]
    });

    let resp = match es_client::execute_search(&state.client, &state.es_base, index, &es_body).await
    {
        Ok(r) => r,
        Err(_) => return Value::Null,
    };

    let hits = resp
        .pointer("/hits/hits")
        .and_then(|h| h.as_array())
        .cloned()
        .unwrap_or_default();

    let cat_set: std::collections::HashSet<&str> = cat_labels.iter().map(String::as_str).collect();

    let mut raw_data: std::collections::BTreeMap<String, Vec<Value>> =
        std::collections::BTreeMap::new();

    for hit in &hits {
        let src = &hit["_source"];
        let scientific_name = src
            .get("scientific_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let taxon_id = src
            .get("taxon_id")
            .and_then(|v| {
                v.as_str()
                    .map(String::from)
                    .or_else(|| v.as_u64().map(|n| n.to_string()))
            })
            .unwrap_or_default();

        let attrs: Vec<Value> = src
            .get("attributes")
            .and_then(|a| a.as_array())
            .cloned()
            .unwrap_or_default();

        let x_val = match find_attr_numeric(&attrs, x_field) {
            Some(v) => v,
            None => continue,
        };
        let y_val = match find_attr_numeric(&attrs, y_field) {
            Some(v) => v,
            None => continue,
        };

        let cat_key = if let Some(cf) = cat_field {
            let stored = find_attr_keyword(&attrs, cf).unwrap_or_default();
            if cat_set.contains(stored.as_str()) {
                stored
            } else if show_other {
                "other".to_string()
            } else {
                stored
            }
        } else {
            "all".to_string()
        };

        raw_data.entry(cat_key.clone()).or_default().push(json!({
            "scientific_name": scientific_name,
            "taxonId": taxon_id,
            "x": x_val,
            "y": y_val,
            "cat": cat_key
        }));
    }

    let mut result = serde_json::Map::new();
    for (cat, points) in raw_data {
        result.insert(cat, json!(points));
    }
    Value::Object(result)
}

/// Run a scatter report.
///
/// Issues a single aggregation query that produces:
/// - `allValues`: counts per x-bucket
/// - `allYValues`: per-x-bucket arrays of y-bucket counts (2D binning)
/// - `by_cat` / `yValuesByCat`: per-category breakdowns of the above
/// - `rawData`: individual point records when total hits ≤ `scatter_threshold`
///
/// Returns `(total_hits, took_ms, report_json)` or error.
pub async fn run_scatter_report(
    state: &Arc<AppState>,
    index: &str,
    _search_query: &SearchQuery,
    _params: &QueryParams,
    report_config: &serde_yaml::Value,
    base_query: &Value,
) -> Result<(u64, u64, Value), String> {
    let x_spec = resolve_axis_spec(AxisRole::X, report_config, state)
        .ok_or("report config missing 'x' axis (set 'x' field or use 'axes')")?;
    let y_spec = resolve_axis_spec(AxisRole::Y, report_config, state)
        .ok_or("scatter report requires 'y' axis (set 'y' field or use 'axes')")?;
    let x_field = x_spec.field.clone();
    let y_field = y_spec.field.clone();
    let cat_spec_opt = resolve_axis_spec(AxisRole::Cat, report_config, state);
    let scatter_threshold = report_config
        .get("scatter_threshold")
        .and_then(|v| v.as_u64())
        .unwrap_or(1000) as usize;

    let x_bounds = compute_bounds(
        &state.client,
        &state.es_base,
        index,
        &x_spec,
        base_query,
        &state.cache,
    )
    .await?;
    let y_bounds = compute_bounds(
        &state.client,
        &state.es_base,
        index,
        &y_spec,
        base_query,
        &state.cache,
    )
    .await?;

    let (cat_labels, show_other_cat, cat_is_numeric, cat_bounds_opt) =
        if let Some(ref cat_spec) = cat_spec_opt {
            let cat_bounds = compute_bounds(
                &state.client,
                &state.es_base,
                index,
                cat_spec,
                base_query,
                &state.cache,
            )
            .await?;
            let labels = cat_bounds.cat_labels.clone();
            let show_other = cat_spec.opts.show_other;
            let is_numeric = !matches!(
                cat_spec.value_type,
                ValueType::Keyword | ValueType::TaxonRank
            );
            (labels, show_other, is_numeric, Some(cat_bounds))
        } else {
            (vec![], false, false, None)
        };

    let x_inner_agg = x_bucket_agg_name(x_spec.value_type);

    let agg_name = "x_agg";
    let cat_field_str = cat_spec_opt.as_ref().map(|s| s.field.as_str());
    let scatter_agg = build_nested_attribute_scatter_agg(
        agg_name,
        &x_spec,
        &x_bounds,
        y_field.as_str(),
        &y_bounds,
        y_spec.opts.scale,
        cat_field_str,
        cat_spec_opt.as_ref().map(|s| s.value_type),
        cat_bounds_opt.as_ref(),
        &cat_labels,
        show_other_cat,
        &state.cache,
    )?;

    let es_body = json!({ "size": 0, "query": base_query, "aggs": scatter_agg });
    let resp = es_client::execute_search(&state.client, &state.es_base, index, &es_body).await?;

    let took = resp.get("took").and_then(|t| t.as_u64()).unwrap_or(0);
    let total_hits = resp
        .pointer("/hits/total/value")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    // ---- Extract main x buckets (histogram or terms depending on x type) ----
    let x_hist_path = format!("/aggregations/{}/by_key/{}/buckets", agg_name, x_inner_agg);
    let x_raw_buckets = resp
        .pointer(&x_hist_path)
        .and_then(|b| b.as_array())
        .cloned()
        .unwrap_or_default();
    let x_bucket_count = x_raw_buckets.len();

    // Keys may be numeric (histogram) or string (terms) — collect as raw JSON Values.
    let x_bucket_keys: Vec<Value> = x_raw_buckets
        .iter()
        .filter_map(|b| b.get("key").cloned())
        .collect();

    let all_values: Vec<u64> = x_raw_buckets
        .iter()
        .map(|b| b.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0))
        .collect();

    // ---- Extract allYValues (per x-bucket y-histogram) and yBuckets ----
    let y_bucket_count = y_bounds.tick_count;
    let mut all_y_values: Vec<Vec<u64>> = Vec::with_capacity(x_bucket_count);
    let mut y_bucket_keys: Vec<f64> = Vec::new();

    for x_bucket in &x_raw_buckets {
        let y_hist_path = format!("/yHistograms/by_attribute/{}/histogram/buckets", y_field);
        let y_buckets_opt = x_bucket.pointer(&y_hist_path).and_then(|b| b.as_array());

        if let Some(ybuckets) = y_buckets_opt {
            if y_bucket_keys.is_empty() {
                y_bucket_keys = ybuckets
                    .iter()
                    .filter_map(|b| b.get("key").and_then(|k| k.as_f64()))
                    .collect();
            }
            all_y_values.push(
                ybuckets
                    .iter()
                    .map(|b| b.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0))
                    .collect(),
            );
        } else {
            all_y_values.push(vec![0; y_bucket_count]);
        }
    }

    let z_domain = compute_z_domain(&all_y_values);

    // ---- Extract per-category data ----
    let (by_cat, y_values_by_cat) = if !cat_labels.is_empty() || cat_is_numeric {
        extract_scatter_by_cat(
            &resp,
            agg_name,
            x_field.as_str(),
            x_inner_agg,
            y_field.as_str(),
            x_bucket_count,
            y_bucket_count,
            &cat_labels,
            show_other_cat,
            cat_is_numeric,
            &all_values,
        )
    } else {
        (Value::Null, Value::Null)
    };

    // ---- Fetch raw point data if below threshold ----
    let raw_data = if total_hits as usize <= scatter_threshold {
        fetch_raw_point_data(
            state,
            index,
            base_query,
            x_field.as_str(),
            y_field.as_str(),
            cat_field_str,
            &cat_labels,
            show_other_cat,
            scatter_threshold,
        )
        .await
    } else {
        Value::Null
    };

    let mut report_data = json!({
        "type": "scatter",
        "x": {
            "field": x_field,
            "scale": format!("{:?}", x_spec.opts.scale).to_lowercase(),
            "domain": x_bounds.domain
        },
        "y": {
            "field": y_field,
            "scale": format!("{:?}", y_spec.opts.scale).to_lowercase(),
            "domain": y_bounds.domain
        },
        "buckets": x_bucket_keys,
        "allValues": all_values,
        "yBuckets": y_bucket_keys,
        "allYValues": all_y_values,
        "zDomain": z_domain
    });

    if !by_cat.is_null() {
        report_data["by_cat"] = by_cat;
        report_data["yValuesByCat"] = y_values_by_cat;
        report_data["cat"] = json!(cat_spec_opt.as_ref().map(|s| s.field.as_str()));
        report_data["cats"] = json!(cat_labels);
    }

    if !raw_data.is_null() {
        report_data["rawData"] = raw_data;
    }

    Ok((total_hits, took, report_data))
}
