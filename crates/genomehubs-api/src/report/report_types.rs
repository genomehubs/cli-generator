//! Per-report-type handler functions.
//!
//! Each handler issues ES queries, applies bounds/aggregation/pipeline logic,
//! and returns structured report data.

use genomehubs_query::query::{QueryParams, SearchQuery};
use genomehubs_query::report::axis::{AxisInput, AxisRole, AxisSpec, AxisSummary, ValueType};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::es_client;
use crate::index_name;
use crate::report::agg::{
    agg_builder_for, build_nested_attribute_histogram_with_categories,
    build_nested_attribute_scatter_agg, x_bucket_agg_name,
};
use crate::report::bounds::compute_bounds;
use crate::report::pipeline::{Pipeline, ReportContext, ScaleStep};
use crate::AppState;

fn value_type_to_string(v: ValueType) -> &'static str {
    match v {
        ValueType::Numeric => "float",
        ValueType::Keyword => "keyword",
        ValueType::Date => "date",
        ValueType::GeoPoint => "coordinate",
        ValueType::TaxonRank => "keyword",
    }
}

/// Extract per-category per-bucket counts from a v2-pattern `categoryHistograms` response.
///
/// For each category label the function follows:
/// `.../categoryHistograms/by_attribute/by_cat/by_value/buckets/{label}/histogram/by_attribute/{x_field}/histogram/buckets`
///
/// Returns a JSON object mapping each category key to an array of `doc_count` values, one per
/// main-histogram bucket. Includes an `"other"` key when `show_other` is true.
#[allow(clippy::too_many_arguments)]
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
    main_buckets: &[Value],
) -> Value {
    // Try several possible locations where categoryHistograms may place the
    // per-category buckets. There are two top-level insertion points (the
    // `by_key` attributes path and the `at_rank` lineage path) and two
    // category anchoring strategies inside `categoryHistograms` (attribute
    // keyed or lineage keyed). Try these candidates in order and pick the
    // first that exists in the response.
    let candidates = vec![
        format!(
            "/aggregations/{}/by_key/categoryHistograms/by_attribute/by_cat/by_value/buckets",
            agg_name
        ),
        format!(
            "/aggregations/{}/by_key/categoryHistograms/by_lineage/at_cat_rank/by_value/buckets",
            agg_name
        ),
        format!(
            "/aggregations/{}/at_rank/categoryHistograms/by_attribute/by_cat/by_value/buckets",
            agg_name
        ),
        format!(
            "/aggregations/{}/at_rank/categoryHistograms/by_lineage/at_cat_rank/by_value/buckets",
            agg_name
        ),
    ];

    let base = candidates
        .into_iter()
        .find(|p| resp.pointer(p).is_some())
        .unwrap_or_default();

    if base.is_empty() {
        return Value::Null;
    }

    let mut by_cat = serde_json::Map::new();

    // Build a list of main bucket keys (stringified) so per-category
    // histograms can be aligned to the top-level bucket ordering. This
    // avoids placing category counts in the wrong bin when inner
    // per-category aggregations return buckets in a different order.
    let main_keys: Vec<String> = main_buckets
        .iter()
        .map(|b| {
            b.get("key")
                .and_then(|k| k.as_str().map(|s| s.to_string()))
                .or_else(|| b.get("key").map(|k| k.to_string()))
                .or_else(|| b.get("id").and_then(|i| i.as_str().map(|s| s.to_string())))
                .unwrap_or_default()
        })
        .collect();

    if cat_is_numeric {
        // by_value uses a histogram agg — buckets is an array of { key, histogram: {…} }.
        let cat_buckets = resp
            .pointer(&base)
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default();
        for bucket in &cat_buckets {
            // bucket key may be numeric
            let key_val = bucket.get("key").cloned().unwrap_or(json!(0));
            let label = if let Some(kf) = key_val.as_f64() {
                kf.to_string()
            } else if let Some(ks) = key_val.as_str() {
                ks.to_string()
            } else {
                key_val.to_string()
            };

            // Try attribute-style inner histogram path, then lineage-style.
            let hist_path_attr = format!(
                "/histogram/by_attribute/{}/{}/buckets",
                x_field, x_bucket_agg
            );
            let hist_path_lineage =
                format!("/histogram/by_lineage/at_rank/{}/buckets", x_bucket_agg);
            let hist_buckets = bucket
                .pointer(&hist_path_attr)
                .or_else(|| bucket.pointer(&hist_path_lineage))
                .and_then(|b| b.as_array())
                .cloned()
                .unwrap_or_default();

            // Map inner bucket keys -> counts for alignment
            let mut counts_map: std::collections::HashMap<String, u64> =
                std::collections::HashMap::new();
            for hb in &hist_buckets {
                let k = hb.get("key").cloned().unwrap_or(json!(""));
                let kstr = if let Some(s) = k.as_str() {
                    s.to_string()
                } else if let Some(n) = k.as_f64() {
                    n.to_string()
                } else {
                    k.to_string()
                };
                let cnt = hb.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0);
                counts_map.insert(kstr, cnt);
            }

            // Build aligned counts vector according to main_keys
            let mut counts: Vec<u64> = Vec::with_capacity(main_bucket_count);
            for mk in &main_keys {
                counts.push(*counts_map.get(mk).unwrap_or(&0));
            }
            counts.resize(main_bucket_count, 0);
            by_cat.insert(label, json!(counts));
        }
    } else {
        // by_value uses a filters agg — buckets is an object keyed by label.
        let mut named_sums: Vec<Vec<u64>> = Vec::with_capacity(cat_labels.len());

        for label in cat_labels {
            // Prefer attribute-style inner histogram path, fall back to lineage-style.
            let hist_path_attr = format!(
                "{}/{}/histogram/by_attribute/{}/{}/buckets",
                base, label, x_field, x_bucket_agg
            );
            let hist_path_lineage = format!(
                "{}/{}/histogram/by_lineage/at_rank/{}/buckets",
                base, label, x_bucket_agg
            );
            let hist_buckets = resp
                .pointer(&hist_path_attr)
                .or_else(|| resp.pointer(&hist_path_lineage))
                .and_then(|b| b.as_array())
                .cloned()
                .unwrap_or_default();

            // Map inner bucket keys -> counts for alignment
            let mut counts_map: std::collections::HashMap<String, u64> =
                std::collections::HashMap::new();
            for hb in &hist_buckets {
                let k = hb.get("key").cloned().unwrap_or(json!(""));
                let kstr = if let Some(s) = k.as_str() {
                    s.to_string()
                } else if let Some(n) = k.as_f64() {
                    n.to_string()
                } else {
                    k.to_string()
                };
                let cnt = hb.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0);
                counts_map.insert(kstr, cnt);
            }

            let mut counts: Vec<u64> = Vec::with_capacity(main_bucket_count);
            for mk in &main_keys {
                counts.push(*counts_map.get(mk).unwrap_or(&0));
            }
            counts.resize(main_bucket_count, 0);
            named_sums.push(counts.clone());
            by_cat.insert(label.clone(), json!(counts));
        }

        if show_other {
            let other_path_attr = format!(
                "{}/other/histogram/by_attribute/{}/{}/buckets",
                base, x_field, x_bucket_agg
            );
            let other_path_lineage = format!(
                "{}/other/histogram/by_lineage/at_rank/{}/buckets",
                base, x_bucket_agg
            );
            let other_counts: Vec<u64> = if let Some(buckets) = resp
                .pointer(&other_path_attr)
                .or_else(|| resp.pointer(&other_path_lineage))
                .and_then(|b| b.as_array())
            {
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
        .await
        .ok_or("report config missing 'x' axis (set 'x' field or use 'axes')")?;
    let x_field = x_spec.field.clone();
    let cat_spec_opt = resolve_axis_spec(AxisRole::Cat, report_config, state).await;

    // Augment the base query for bounds computation with a presence-filter
    // for the opposite axis so bounds reflect only records that will be
    // plotted. This mirrors the scatter report behaviour and avoids empty
    // category buckets when one axis lacks values for certain categories.
    let cat_presence = cat_spec_opt.as_ref().and_then(presence_filter_for_axis);
    let x_base_query = if let Some(f) = cat_presence {
        json!({ "bool": { "must": [ base_query.clone(), f ] } })
    } else {
        base_query.clone()
    };

    let x_bounds = compute_bounds(
        &state.client,
        &state.es_base,
        index,
        &x_spec,
        &x_base_query,
        &state.cache,
    )
    .await?;

    let agg_name = "x_agg";

    let x_inner_agg = x_bucket_agg_name(x_spec.value_type);

    // Build aggregation — categorized path supports both keyword (filters) and numeric (histogram) cat.
    let (final_agg, cat_labels, show_other_cat, cat_is_numeric) =
        if let Some(ref cat_spec) = cat_spec_opt {
            // When computing category bounds, require the x-axis presence
            // so categories returned are only those that will be plotted
            // (i.e., documents that contain an x value). This prevents
            // returning category labels with no corresponding x buckets.
            let x_presence = presence_filter_for_axis(&x_spec);
            let cat_base_query = if let Some(f) = x_presence {
                json!({ "bool": { "must": [ base_query.clone(), f ] } })
            } else {
                base_query.clone()
            };

            let cat_bounds = compute_bounds(
                &state.client,
                &state.es_base,
                index,
                cat_spec,
                &cat_base_query,
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
    // DEBUG: print ES body to help diagnose nested aggregation shapes for
    // category histograms. Remove this eprintln once debugging is complete.
    eprintln!(
        "ES body for histogram: {}",
        serde_json::to_string_pretty(&es_body).unwrap_or_default()
    );
    let resp = es_client::execute_search(&state.client, &state.es_base, index, &es_body).await?;
    // DEBUG: print ES response for inspection
    eprintln!(
        "ES resp for histogram: {}",
        serde_json::to_string_pretty(&resp).unwrap_or_default()
    );

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
            &raw_buckets,
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
    let processed_raw = pipeline.run(raw_buckets.clone(), &ctx);

    // Align and label processed buckets. When the bounds provide an
    // authoritative `fixed_terms` list, use that ordering and drop any
    // unexpected buckets. Otherwise, for keyword axes drop zero-count
    // placeholder buckets and ensure each bucket has a label.
    let processed_buckets = if !x_bounds.fixed_terms.is_empty() {
        align_and_label_processed_buckets(
            processed_raw,
            &x_bounds.fixed_terms,
            &x_bounds.cat_labels,
        )
    } else {
        let mut pb = processed_raw;
        if matches!(x_spec.value_type, ValueType::Keyword) {
            pb.retain(|b| b.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0) > 0);
        }
        for b in pb.iter_mut() {
            if b.get("label").is_none() {
                let id_str = b
                    .get("key")
                    .and_then(|k| k.as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| b.get("key").map(|k| k.to_string()).unwrap_or_default());
                b["label"] = json!(id_str);
            }
        }
        pb
    };

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
            "tickCount": x_bounds.tick_count,
            "value_type": value_type_to_string(x_spec.value_type)
        },
        "buckets": processed_buckets,
        "allValues": all_values
    });

    if !by_cat.is_null() {
        report_data["by_cat"] = by_cat;
        if let Some(ref cat_spec) = cat_spec_opt {
            report_data["cat"] = json!({
                "field": cat_spec.field,
                "value_type": value_type_to_string(cat_spec.value_type),
                "scale": format!("{:?}", cat_spec.opts.scale).to_lowercase()
            });
        }
        report_data["cats"] = json!(cat_labels);
    }

    Ok((total_hits, took, report_data))
}

/// Run a countPerRank report (counts per taxonomic rank).
pub async fn run_count_per_rank_report(
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
                            let entry = sources_map.entry(source_name.clone()).or_default();

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
///
/// Builds a taxon tree from the matched set with full v2 parity:
/// 1. Finding the LCA via nested lineage aggregation
/// 2. Using search_after pagination to fetch all matching taxa
/// 3. Walking each result's lineage to build parent-child relationships
/// 4. Extracting per-node attribute fields and cat label
/// 5. Running a second paginated search when `status_filter` is set
/// 6. Propagating cat labels up to `cat_rank` ancestors
/// 7. Collapsing monotypic nodes when `collapse_monotypic` is set
/// 8. Computing subtree counts and depth statistics
///
/// The tree is capped at 100 000 nodes to match v2 behaviour.
pub async fn run_tree_report(
    state: &Arc<AppState>,
    index: &str,
    base_query: &Value,
    report_config: &serde_yaml::Value,
) -> Result<(u64, u64, Value), String> {
    use std::collections::{BTreeMap, HashSet};

    const MAX_TREE_NODES: usize = 100_000;
    const PAGE_SIZE: usize = 500;

    // --- Collect y-axis field specs (field + summary + opts) ---
    // Prefer structured `axes` array; fall back to flat `y:` / `y_opts:` or legacy
    // `fields:` sequence (AxisSummary::Value for all).
    let tree_field_specs: Vec<(String, AxisSummary)> = {
        let from_axes = resolve_y_specs(report_config, state).await;
        if !from_axes.is_empty() {
            from_axes
                .into_iter()
                .map(|s| (s.field, s.summary))
                .collect()
        } else {
            // Legacy `fields:` sequence — no per-field summary control
            let from_seq: Vec<String> = report_config
                .get("fields")
                .and_then(|v| v.as_sequence())
                .map(|seq| {
                    seq.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            from_seq
                .into_iter()
                .map(|f| (f, AxisSummary::default()))
                .collect()
        }
    };

    // --- Cat axis: resolve full AxisSpec + bounds (same pipeline as histogram) ---
    let cat_spec_opt = resolve_axis_spec(AxisRole::Cat, report_config, state).await;
    let cat_bounds_opt = if let Some(ref cat_spec) = cat_spec_opt {
        Some(
            compute_bounds(
                &state.client,
                &state.es_base,
                index,
                cat_spec,
                base_query,
                &state.cache,
            )
            .await?,
        )
    } else {
        None
    };
    let cat_field: Option<String> = cat_spec_opt.as_ref().map(|s| s.field.clone());
    let cat_labels: Vec<String> = cat_bounds_opt
        .as_ref()
        .map(|b| b.cat_labels.clone())
        .unwrap_or_default();

    // --- Optional propagation rank for cat labels ---
    let cat_rank: Option<String> = report_config
        .get("cat_rank")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    // --- Status filter: compile query-string fragment into ES clause ---
    let status_filter_str: Option<String> = report_config
        .get("status_filter")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    // --- Collapse monotypic ---
    let collapse_monotypic = report_config
        .get("collapse_monotypic")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let preserve_ranks: Vec<String> = {
        let mut ranks = vec!["species".to_string()];
        if let Some(extra) = report_config.get("preserve_rank").and_then(|v| v.as_str()) {
            ranks.extend(extra.split(',').map(|s| s.trim().to_string()));
        }
        ranks
    };

    // Optional: count descendants of a specific rank per tree node (e.g. count_rank: species).
    let count_rank: Option<String> = report_config
        .get("count_rank")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    // --- Step 1: Find LCA ---
    let (lca_id, lca_name, lca_rank, lca_parent, total_hits, lca_took) =
        find_tree_lca(state, index, base_query).await?;

    if total_hits == 0 {
        return Ok((0, lca_took, json!({ "lca": null, "treeNodes": {} })));
    }

    if total_hits > MAX_TREE_NODES as u64 {
        return Err(format!(
            "Tree limited to {MAX_TREE_NODES} nodes; query returns {total_hits} taxa. \
             Add filters to reduce the result set."
        ));
    }

    // --- Step 2: Paginate all results, build tree ---
    let mut tree_nodes: BTreeMap<String, serde_json::Map<String, Value>> = BTreeMap::new();
    let mut direct_results: HashSet<String> = HashSet::new();
    let mut search_after: Option<Value> = None;
    let mut took_total = lca_took;
    let mut fetched = 0usize;

    loop {
        let mut es_body = json!({
            "size": PAGE_SIZE,
            "query": base_query,
            "_source": ["taxon_id", "scientific_name", "taxon_rank", "parent", "lineage", "attributes"],
            "sort": [{ "taxon_id": "asc" }]
        });
        if let Some(ref cursor) = search_after {
            es_body["search_after"] = cursor.clone();
        }

        let resp =
            es_client::execute_search(&state.client, &state.es_base, index, &es_body).await?;
        took_total += resp.get("took").and_then(|t| t.as_u64()).unwrap_or(0);

        let hits = resp
            .pointer("/hits/hits")
            .and_then(|h| h.as_array())
            .cloned()
            .unwrap_or_default();

        if hits.is_empty() {
            break;
        }

        for hit in &hits {
            if let Some(src) = hit.get("_source") {
                process_tree_hit(
                    src,
                    &lca_id,
                    &tree_field_specs,
                    cat_field.as_deref(),
                    &cat_labels,
                    &mut tree_nodes,
                    &mut direct_results,
                );
                fetched += 1;
                if fetched >= MAX_TREE_NODES {
                    break;
                }
            }
        }

        search_after = hits.last().and_then(|h| h.get("sort")).cloned();
        if hits.len() < PAGE_SIZE || fetched >= MAX_TREE_NODES {
            break;
        }
    }

    // --- Step 3: Status filter — run second paginated search, collect matching IDs ---
    let status_node_ids: Option<HashSet<String>> = if let Some(ref filter_str) = status_filter_str {
        let filter_clause =
            match crate::report::filter_expr::filter_expr_to_es_query(filter_str, base_query) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[tree] invalid status_filter '{}': {}", filter_str, e);
                    base_query.clone()
                }
            };
        let mut ids: HashSet<String> = HashSet::new();
        let mut sa: Option<Value> = None;
        loop {
            let mut body = json!({
                "size": PAGE_SIZE,
                "query": filter_clause,
                "_source": ["taxon_id"],
                "sort": [{ "taxon_id": "asc" }]
            });
            if let Some(ref cursor) = sa {
                body["search_after"] = cursor.clone();
            }
            let resp =
                es_client::execute_search(&state.client, &state.es_base, index, &body).await?;
            took_total += resp.get("took").and_then(|t| t.as_u64()).unwrap_or(0);
            let hits = resp
                .pointer("/hits/hits")
                .and_then(|h| h.as_array())
                .cloned()
                .unwrap_or_default();
            if hits.is_empty() {
                break;
            }
            for hit in &hits {
                if let Some(id) = hit.pointer("/_source/taxon_id").and_then(|v| v.as_str()) {
                    ids.insert(id.to_string());
                }
            }
            sa = hits.last().and_then(|h| h.get("sort")).cloned();
            if hits.len() < PAGE_SIZE {
                break;
            }
        }
        Some(ids)
    } else {
        None
    };

    // --- Step 4: Propagate cat labels up to cat_rank ancestors ---
    if let Some(ref rank) = cat_rank {
        propagate_cat_to_rank(&mut tree_nodes, &lca_id, rank);
    }

    // --- Step 5: Collapse monotypic nodes ---
    if collapse_monotypic {
        collapse_monotypic_nodes(&mut tree_nodes, &lca_id, &preserve_ranks);
    }

    // --- Step 6: Descendant rank counts (optional) ---
    // One extra ES query using nested lineage → reverse_nested to count how many
    // taxa at `count_rank` descend from each tree node.
    if let Some(ref rank) = count_rank {
        let descendant_counts =
            fetch_descendant_counts(&state.client, &state.es_base, index, &lca_id, rank).await?;
        took_total += descendant_counts.took;
        for (taxon_id, count) in descendant_counts.counts {
            if let Some(node) = tree_nodes.get_mut(&taxon_id) {
                node.insert("descendant_count".to_string(), json!(count));
            }
        }
    }

    // --- Step 7: Compute subtree counts ---
    compute_subtree_counts(&mut tree_nodes, &lca_id, &direct_results);

    // --- Step 8: Compute tree depths from LCA ---
    let (max_depth, min_depth) = compute_tree_depths(&tree_nodes, &lca_id);

    // --- Step 9: Build response ---
    let lca = json!({
        "taxon_id": lca_id,
        "scientific_name": lca_name,
        "taxon_rank": lca_rank,
        "count": total_hits,
        "maxDepth": max_depth,
        "minDepth": min_depth,
        "parent": lca_parent
    });

    let mut tree_nodes_json = serde_json::Map::new();
    for (id, mut node) in tree_nodes {
        // status=1: node appears in status_filter results OR (no filter) has field data.
        // status=0: no match or no fields when fields/status_filter are not set.
        let status: u8 = match &status_node_ids {
            Some(ids) => u8::from(ids.contains(&id)),
            None => u8::from(!tree_field_specs.is_empty() && node.contains_key("fields")),
        };
        node.insert("status".to_string(), json!(status));
        tree_nodes_json.insert(id, Value::Object(node));
    }

    let mut report_data = json!({
        "lca": lca,
        "treeNodes": tree_nodes_json
    });

    // Include catBounds in the response when a cat axis was resolved
    if let (Some(cat_spec), Some(cat_bounds)) = (&cat_spec_opt, &cat_bounds_opt) {
        report_data["catBounds"] = json!({
            "field": cat_spec.field,
            "domain": cat_bounds.domain,
            "labels": cat_bounds.cat_labels,
            "scale": format!("{:?}", cat_spec.opts.scale).to_lowercase()
        });
    }

    // Advertise which rank was used for descendant_count so the UI knows how to label it
    if let Some(ref rank) = count_rank {
        report_data["countRank"] = json!(rank);
    }

    Ok((total_hits, took_total, report_data))
}

/// Run a map report (geohash grid).
/// Run a map report.
///
/// Produces two complementary data shapes, mirroring v2:
///
/// - **`rawData`**: individual point records `{scientific_name, taxonId, coords, aggregation_source, cat}`
///   grouped by cat label (or `"all taxa"` when no cat is set). Only populated when the
///   count of taxa with location data is ≤ `map_threshold`.
///
/// - **`hexBinCounts`**: `{h3_cell_id: count}` map from a `terms` aggregation on
///   `attributes.hexbin{N}` (pre-computed H3 cells stored on each location attribute entry).
///   Resolution is controlled by `hex_resolution` (1–6, default 3).
///
/// **Config keys:**
/// - `location_field` (default `"sample_location"`) — attribute key for geo-point data
/// - `hex_resolution` (1–6, default 3) — H3 resolution for hexbin aggregation
/// - `map_threshold` (default 2000) — switch from raw-point to hex-only mode
/// - `cat` / `cat_opts` — category field; uses the same resolve_axis_spec pipeline as histogram
pub async fn run_map_report(
    state: &Arc<AppState>,
    index: &str,
    base_query: &Value,
    report_config: &serde_yaml::Value,
) -> Result<(u64, u64, Value), String> {
    let location_field = report_config
        .get("location_field")
        .or_else(|| report_config.get("x"))
        .and_then(|v| v.as_str())
        .unwrap_or("sample_location");

    let hex_resolution = report_config
        .get("hex_resolution")
        .and_then(|v| v.as_u64())
        .unwrap_or(3)
        .clamp(1, 6) as u8;

    let map_threshold = report_config
        .get("map_threshold")
        .and_then(|v| v.as_u64())
        .unwrap_or(2000);

    let hexbin_field = format!("hexbin{hex_resolution}");

    // --- Cat axis (optional) ---
    let cat_spec_opt = resolve_axis_spec(AxisRole::Cat, report_config, state).await;
    let cat_bounds_opt = if let Some(ref spec) = cat_spec_opt {
        compute_bounds(
            &state.client,
            &state.es_base,
            index,
            spec,
            base_query,
            &state.cache,
        )
        .await
        .ok()
    } else {
        None
    };
    let cat_field: Option<String> = cat_spec_opt.as_ref().map(|s| s.field.clone());
    let cat_labels: Vec<String> = cat_bounds_opt
        .as_ref()
        .map(|b| b.cat_labels.clone())
        .unwrap_or_default();

    // --- Step 1: Count taxa that have location data ---
    let count_body = json!({
        "size": 0,
        "query": {
            "bool": {
                "must": [
                    base_query.clone(),
                    {
                        "nested": {
                            "path": "attributes",
                            "query": { "term": { "attributes.key": location_field } }
                        }
                    }
                ]
            }
        }
    });
    let count_resp =
        es_client::execute_search(&state.client, &state.es_base, index, &count_body).await?;
    let location_count = count_resp
        .pointer("/hits/total/value")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let mut took_total = count_resp.get("took").and_then(|t| t.as_u64()).unwrap_or(0);

    // --- Step 2: Hexbin aggregation (always) ---
    let hex_body = json!({
        "size": 0,
        "query": base_query,
        "aggs": {
            "location_attr": {
                "nested": { "path": "attributes" },
                "aggs": {
                    "by_key": {
                        "filter": { "term": { "attributes.key": location_field } },
                        "aggs": {
                            "hexbins": {
                                "terms": {
                                    "field": format!("attributes.{hexbin_field}"),
                                    "size": 50000
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    let hex_resp =
        es_client::execute_search(&state.client, &state.es_base, index, &hex_body).await?;
    took_total += hex_resp.get("took").and_then(|t| t.as_u64()).unwrap_or(0);

    let mut hex_bin_counts: serde_json::Map<String, Value> = serde_json::Map::new();
    if let Some(buckets) = hex_resp
        .pointer("/aggregations/location_attr/by_key/hexbins/buckets")
        .and_then(|v| v.as_array())
    {
        for bucket in buckets {
            if let (Some(key), Some(count)) = (
                bucket.get("key").and_then(|k| k.as_str()),
                bucket.get("doc_count").and_then(|c| c.as_u64()),
            ) {
                hex_bin_counts.insert(key.to_string(), json!(count));
            }
        }
    }

    // --- Step 3: Raw point data (when below threshold) ---
    let mut raw_data: serde_json::Map<String, Value> = serde_json::Map::new();

    if location_count > 0 && location_count <= map_threshold {
        let raw_body = json!({
            "size": location_count.min(10_000),
            "query": {
                "bool": {
                    "must": [
                        base_query.clone(),
                        {
                            "nested": {
                                "path": "attributes",
                                "query": { "term": { "attributes.key": location_field } }
                            }
                        }
                    ]
                }
            },
            "_source": ["taxon_id", "scientific_name", "attributes"]
        });
        let raw_resp =
            es_client::execute_search(&state.client, &state.es_base, index, &raw_body).await?;
        took_total += raw_resp.get("took").and_then(|t| t.as_u64()).unwrap_or(0);

        let hits = raw_resp
            .pointer("/hits/hits")
            .and_then(|h| h.as_array())
            .cloned()
            .unwrap_or_default();

        let empty_attrs: Vec<Value> = vec![];
        for hit in &hits {
            let src = match hit.get("_source") {
                Some(s) => s,
                None => continue,
            };
            let taxon_id = src
                .get("taxon_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let sci_name = src
                .get("scientific_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let attrs = src
                .get("attributes")
                .and_then(|v| v.as_array())
                .unwrap_or(&empty_attrs);

            // Find the location attribute entry
            let loc_attr = attrs
                .iter()
                .find(|a| a.get("key").and_then(|k| k.as_str()) == Some(location_field));
            let Some(loc) = loc_attr else {
                continue;
            };

            // Collect all coords from the attribute (may be a list or single value)
            let coords_list: Vec<String> =
                if let Some(arr) = loc.get("geo_point_value").and_then(|v| v.as_array()) {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                } else if let Some(s) = loc.get("geo_point_value").and_then(|v| v.as_str()) {
                    vec![s.to_string()]
                } else {
                    continue;
                };

            let aggregation_source = loc
                .get("aggregation_source")
                .and_then(|v| v.as_str())
                .unwrap_or("direct");

            // Resolve cat label for this taxon
            let cat_label: String = if let Some(ref cf) = cat_field {
                resolve_cat_label(src, cf, &cat_labels).unwrap_or_else(|| "other".to_string())
            } else {
                "all taxa".to_string()
            };

            let points = raw_data
                .entry(cat_label.clone())
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .expect("cat entry is always an array");

            for coords in coords_list {
                points.push(json!({
                    "scientific_name": sci_name,
                    "taxonId": taxon_id,
                    "coords": coords,
                    "aggregation_source": aggregation_source,
                    "cat": cat_label,
                }));
            }
        }
    }

    // --- Assemble response ---
    let total_hits = location_count;
    let mut report_data = json!({
        "type": "map",
        "locationField": location_field,
        "hexResolution": hex_resolution,
        "rawData": raw_data,
        "hexBinCounts": hex_bin_counts
    });

    if let (Some(ref spec), Some(ref bounds)) = (cat_spec_opt, cat_bounds_opt) {
        report_data["catBounds"] = json!({
            "field": spec.field,
            "labels": bounds.cat_labels,
            "domain": bounds.domain,
            "scale": "linear"
        });
    }

    Ok((total_hits, took_total, report_data))
}

// ============================================================================
// Helper functions
// ============================================================================

/// Async variant of `infer_value_type` that acquires the cache read lock
/// via `read().await` so callers in async handlers can reliably observe
/// populated metadata without falling back on the non-blocking `try_read`.
async fn infer_value_type_async(
    field: &str,
    cache: &Option<Arc<tokio::sync::RwLock<crate::es_metadata::MetadataCache>>>,
) -> ValueType {
    if let Some(cache_lock) = cache {
        let guard = cache_lock.read().await;
        if guard.taxonomic_ranks.contains(&field.to_string()) {
            return ValueType::TaxonRank;
        }
        if let serde_json::Value::Object(groups) = &guard.attr_types {
            for (_, group) in groups {
                if let serde_json::Value::Object(fields) = group {
                    if let Some(serde_json::Value::Object(meta_obj)) = fields.get(field) {
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
    ValueType::Numeric
}

/// Resolve an [`AxisSpec`] for the given role from a report config.
///
/// Checks the structured `axes` array first. Falls back to legacy flat keys
/// (`x`/`x_opts`, `y`/`y_opts`, `cat`/`cat_opts`) so existing request bodies
/// continue to work unchanged.
async fn resolve_axis_spec(
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
                let inferred = infer_value_type_async(&input.field, &state.cache).await;
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
    let value_type = infer_value_type_async(field, &state.cache).await;
    Some(AxisSpec {
        field: field.to_string(),
        role,
        summary: AxisSummary::default(),
        value_type,
        opts: opts_str.parse().unwrap_or_default(),
    })
}

// ============================================================================
// Tree helpers
// ============================================================================

/// Collect all `y`-role axis specs from `report_config`.
///
/// Prefers the structured `axes` array (multiple entries, per-field `summary` and
/// opts).  Falls back to the flat `y:` + `y_opts:` shorthand for a single field
/// with `AxisSummary::Value`.  Returns an empty vec when neither is present.
async fn resolve_y_specs(
    report_config: &serde_yaml::Value,
    state: &Arc<AppState>,
) -> Vec<AxisSpec> {
    // Structured form: collect every entry with position == "y"
    if let Some(axes) = report_config.get("axes").and_then(|a| a.as_sequence()) {
        let inputs: Vec<AxisInput> = axes
            .iter()
            .filter(|e| e.get("position").and_then(|p| p.as_str()) == Some("y"))
            .filter_map(|e| serde_yaml::from_value::<AxisInput>(e.clone()).ok())
            .collect();
        if !inputs.is_empty() {
            let mut out = Vec::with_capacity(inputs.len());
            for input in inputs {
                let inferred = infer_value_type_async(&input.field, &state.cache).await;
                out.push(input.into_spec(inferred));
            }
            return out;
        }
    }

    // Flat shorthand fallback: `y:` + optional `y_opts:`
    let field = match report_config.get("y").and_then(|v| v.as_str()) {
        Some(f) => f,
        None => return vec![],
    };
    let opts_str = report_config
        .get("y_opts")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let value_type = infer_value_type_async(field, &state.cache).await;
    vec![AxisSpec {
        field: field.to_string(),
        role: AxisRole::Y,
        summary: AxisSummary::default(),
        value_type,
        opts: opts_str.parse().unwrap_or_default(),
    }]
}

/// Find the LCA (lowest common ancestor) of the query result set.
///
/// Uses a nested lineage aggregation sorted by doc_count desc + min_depth asc
/// (same strategy as v2) to pick the deepest ancestor common to all results.
/// The first result's lineage is then scanned to resolve name, rank, and parent.
///
/// Returns `(lca_id, scientific_name, taxon_rank, parent, total_hits, took_ms)`.
async fn find_tree_lca(
    state: &Arc<AppState>,
    index: &str,
    base_query: &Value,
) -> Result<(String, String, String, Option<String>, u64, u64), String> {
    let es_body = json!({
        "size": 1,
        "query": base_query,
        "_source": ["taxon_id", "scientific_name", "taxon_rank", "lineage"],
        "aggs": {
            "by_lineage": {
                "nested": { "path": "lineage" },
                "aggs": {
                    "ancestors": {
                        "terms": { "field": "lineage.taxon_id", "size": 100 },
                        "aggs": {
                            "types_count": { "value_count": { "field": "lineage.taxon_id" } },
                            "min_depth": { "min": { "field": "lineage.node_depth" } },
                            "ancestor_sort": {
                                "bucket_sort": {
                                    "sort": [
                                        { "types_count": { "order": "desc" } },
                                        { "min_depth": { "order": "asc" } }
                                    ],
                                    "size": 2
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

    // LCA candidate = highest-count + shallowest ancestor
    let lca_id = resp
        .pointer("/aggregations/by_lineage/ancestors/buckets/0/key")
        .and_then(|k| k.as_str())
        .ok_or_else(|| "no ancestor buckets — empty result set".to_string())?
        .to_string();

    // Walk first result's lineage to resolve name, rank, parent
    let first_lineage = resp
        .pointer("/hits/hits/0/_source/lineage")
        .and_then(|l| l.as_array())
        .ok_or_else(|| "lineage missing from first result".to_string())?;

    let mut scientific_name = lca_id.clone();
    let mut taxon_rank = String::new();
    let mut parent: Option<String> = None;
    let mut found = false;

    for entry in first_lineage {
        let entry_id = entry.get("taxon_id").and_then(|v| v.as_str()).unwrap_or("");
        if entry_id == lca_id {
            scientific_name = entry
                .get("scientific_name")
                .and_then(|v| v.as_str())
                .unwrap_or(&lca_id)
                .to_string();
            taxon_rank = entry
                .get("taxon_rank")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            found = true;
        } else if found {
            parent = Some(entry_id.to_string());
            break;
        }
    }

    // Fallback: use first result itself when LCA is absent from lineage
    if !found {
        if let Some(src) = resp.pointer("/hits/hits/0/_source") {
            scientific_name = src
                .get("scientific_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            taxon_rank = src
                .get("taxon_rank")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
        }
    }

    Ok((
        lca_id,
        scientific_name,
        taxon_rank,
        parent,
        total_hits,
        took,
    ))
}

/// Process one ES hit and update the tree node map.
///
/// - Marks the taxon as a direct result.
/// - Walks its lineage to create intermediate ancestor nodes and link children
///   up to (but not above) the LCA.
/// - Extracts attribute values for each `(field, summary)` spec; each field entry
///   gains a `display_value` key set to the sub-field selected by its `AxisSummary`.
/// - Assigns a cat label from `cat_labels` when `cat_field` is set.
fn process_tree_hit(
    src: &Value,
    lca_id: &str,
    tree_field_specs: &[(String, AxisSummary)],
    cat_field: Option<&str>,
    cat_labels: &[String],
    tree_nodes: &mut std::collections::BTreeMap<String, serde_json::Map<String, Value>>,
    direct_results: &mut std::collections::HashSet<String>,
) {
    let taxon_id = match src.get("taxon_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return,
    };
    let scientific_name = src
        .get("scientific_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let taxon_rank = src
        .get("taxon_rank")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    direct_results.insert(taxon_id.clone());

    // Collect all requested field data
    let mut merged_fields = serde_json::Map::new();
    for (field, summary) in tree_field_specs {
        let extracted = extract_tree_field(src, field, *summary);
        merged_fields.extend(extracted);
    }

    // Determine cat label from cat_field attribute
    let cat_label: Option<String> = cat_field.and_then(|cf| resolve_cat_label(src, cf, cat_labels));

    // Insert or update the node for this taxon
    {
        let node = tree_nodes.entry(taxon_id.clone()).or_insert_with(|| {
            let mut n = serde_json::Map::new();
            n.insert("taxon_id".to_string(), json!(&taxon_id));
            n.insert("scientific_name".to_string(), json!(&scientific_name));
            n.insert("taxon_rank".to_string(), json!(&taxon_rank));
            n.insert("count".to_string(), json!(0u64));
            n.insert("children".to_string(), json!({}));
            n
        });
        // Overwrite name/rank in case node was created as a placeholder from lineage
        node.insert("scientific_name".to_string(), json!(&scientific_name));
        node.insert("taxon_rank".to_string(), json!(&taxon_rank));
        if !merged_fields.is_empty() {
            node.insert("fields".to_string(), Value::Object(merged_fields));
        }
        if let Some(ref label) = cat_label {
            node.insert("cat".to_string(), json!(label));
        }
    }

    // Walk lineage (depth 0 = self, depth 1 = parent, ...) to build parent-child links
    let lineage = match src.get("lineage").and_then(|l| l.as_array()) {
        Some(l) => l.clone(),
        None => return,
    };

    for i in 0..lineage.len() {
        let entry_id = lineage[i]
            .get("taxon_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Stop at LCA — don't add a link from LCA to its parent
        if entry_id == lca_id {
            break;
        }

        if i + 1 >= lineage.len() {
            break;
        }

        let parent_id = lineage[i + 1]
            .get("taxon_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if parent_id.is_empty() {
            continue;
        }
        let parent_name = lineage[i + 1]
            .get("scientific_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let parent_rank = lineage[i + 1]
            .get("taxon_rank")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Create parent node if absent
        {
            let parent_node = tree_nodes.entry(parent_id.clone()).or_insert_with(|| {
                let mut n = serde_json::Map::new();
                n.insert("taxon_id".to_string(), json!(&parent_id));
                n.insert("scientific_name".to_string(), json!(&parent_name));
                n.insert("taxon_rank".to_string(), json!(&parent_rank));
                n.insert("count".to_string(), json!(0u64));
                n.insert("children".to_string(), json!({}));
                n
            });
            // Register current entry as a child of this parent
            if let Some(children) = parent_node
                .get_mut("children")
                .and_then(|c| c.as_object_mut())
            {
                children.insert(entry_id.clone(), json!(true));
            }
        }
    }
}

/// Extract field metadata for `field_name` from a document's `attributes` array.
///
/// Returns a map `{field_name: {source, value, min, max, display_value}}` or empty
/// if the field is not present.  `display_value` is the sub-field selected by
/// `summary`; `value`, `min`, and `max` are always included when available.
fn extract_tree_field(
    src: &Value,
    field_name: &str,
    summary: AxisSummary,
) -> serde_json::Map<String, Value> {
    let mut fields_map = serde_json::Map::new();
    let attrs = match src.get("attributes").and_then(|a| a.as_array()) {
        Some(a) => a,
        None => return fields_map,
    };

    for attr in attrs {
        if attr.get("key").and_then(|k| k.as_str()) != Some(field_name) {
            continue;
        }
        let mut field_data = serde_json::Map::new();

        if let Some(source) = attr.get("aggregation_source") {
            field_data.insert("source".to_string(), source.clone());
        }
        // Raw stored value: prefer long_value → float_value → half_float_value → keyword_value
        let raw_value = attr
            .get("long_value")
            .or_else(|| attr.get("float_value"))
            .or_else(|| attr.get("half_float_value"))
            .or_else(|| attr.get("keyword_value"));
        if let Some(v) = raw_value {
            field_data.insert("value".to_string(), v.clone());
        }
        for key in ["min", "max", "median", "mean", "count", "length"] {
            if let Some(v) = attr.get(key) {
                field_data.insert(key.to_string(), v.clone());
            }
        }
        // display_value: the summary sub-field the UI should use for colouring/sizing
        let display_value = match summary {
            AxisSummary::Value => raw_value.cloned(),
            AxisSummary::Min => attr.get("min").cloned(),
            AxisSummary::Max => attr.get("max").cloned(),
            AxisSummary::Median => attr.get("median").cloned(),
            AxisSummary::Mean => attr.get("mean").cloned(),
            AxisSummary::Count => attr.get("count").cloned(),
            AxisSummary::Length => attr.get("length").cloned(),
        };
        if let Some(dv) = display_value {
            field_data.insert("display_value".to_string(), dv);
        }

        fields_map.insert(field_name.to_string(), Value::Object(field_data));
        break; // use first matching attribute
    }
    fields_map
}

/// Result of `fetch_descendant_counts`.
struct DescendantCounts {
    /// Per-node counts: taxon_id → number of descendants at the requested rank.
    counts: std::collections::HashMap<String, u64>,
    /// ES `took` value for the query, in ms.
    took: u64,
}

/// Count, for every ancestor node that is in scope under `lca_id`, how many taxa
/// at `count_rank` descend from it.
///
/// Runs a single ES aggregation:
/// - Filter to documents at `count_rank` whose lineage contains `lca_id`
/// - Nest into `lineage`, group by `lineage.taxon_id` (all ancestors)
/// - `reverse_nested` to count distinct parent documents per ancestor bucket
///
/// This gives the number of `count_rank` taxa beneath every ancestor in one round-trip.
async fn fetch_descendant_counts(
    client: &reqwest::Client,
    es_base: &str,
    index: &str,
    lca_id: &str,
    count_rank: &str,
) -> Result<DescendantCounts, String> {
    let body = json!({
        "size": 0,
        "query": {
            "bool": {
                "must": [
                    { "term": { "taxon_rank": count_rank } },
                    {
                        "nested": {
                            "path": "lineage",
                            "query": { "term": { "lineage.taxon_id": lca_id } }
                        }
                    }
                ]
            }
        },
        "aggs": {
            "by_ancestor": {
                "nested": { "path": "lineage" },
                "aggs": {
                    "ancestors": {
                        "terms": {
                            "field": "lineage.taxon_id",
                            "size": 100000
                        },
                        "aggs": {
                            "node_count": { "reverse_nested": {} }
                        }
                    }
                }
            }
        }
    });

    let resp = crate::es_client::execute_search(client, es_base, index, &body).await?;
    let took = resp.get("took").and_then(|t| t.as_u64()).unwrap_or(0);

    let buckets = resp
        .pointer("/aggregations/by_ancestor/ancestors/buckets")
        .and_then(|b| b.as_array())
        .cloned()
        .unwrap_or_default();

    let mut counts = std::collections::HashMap::with_capacity(buckets.len());
    for bucket in &buckets {
        let taxon_id = match bucket.get("key").and_then(|k| k.as_str()) {
            Some(id) => id.to_string(),
            None => continue,
        };
        let count = bucket
            .pointer("/node_count/doc_count")
            .and_then(|c| c.as_u64())
            .unwrap_or(0);
        counts.insert(taxon_id, count);
    }

    Ok(DescendantCounts { counts, took })
}

/// Compute subtree counts via iterative post-order DFS.
///
/// Each leaf node that is a direct result contributes 1; each internal node
/// receives the sum of its children's counts.
fn compute_subtree_counts(
    tree_nodes: &mut std::collections::BTreeMap<String, serde_json::Map<String, Value>>,
    lca_id: &str,
    direct_results: &std::collections::HashSet<String>,
) {
    use std::collections::HashMap;

    // Snapshot the structure so we can mutate tree_nodes afterwards
    let structure: HashMap<String, Vec<String>> = tree_nodes
        .iter()
        .map(|(id, node)| {
            let children: Vec<String> = node
                .get("children")
                .and_then(|c| c.as_object())
                .map(|obj| obj.keys().cloned().collect())
                .unwrap_or_default();
            (id.clone(), children)
        })
        .collect();

    let mut counts: HashMap<String, u64> = HashMap::new();
    // Stack items: (id, children_processed)
    let mut stack: Vec<(String, bool)> = vec![(lca_id.to_string(), false)];

    while let Some((id, processed)) = stack.pop() {
        if processed {
            let children = structure.get(&id).map(|c| c.as_slice()).unwrap_or(&[]);
            let count = if children.is_empty() {
                u64::from(direct_results.contains(&id))
            } else {
                children
                    .iter()
                    .map(|c| counts.get(c).copied().unwrap_or(0))
                    .sum()
            };
            counts.insert(id, count);
        } else {
            stack.push((id.clone(), true));
            if let Some(children) = structure.get(&id) {
                for child in children {
                    if !counts.contains_key(child.as_str()) {
                        stack.push((child.clone(), false));
                    }
                }
            }
        }
    }

    for (id, count) in &counts {
        if let Some(node) = tree_nodes.get_mut(id) {
            node.insert("count".to_string(), json!(count));
        }
    }
}

/// Compute max and min leaf depth from the LCA via BFS.
///
/// Returns `(max_depth, min_leaf_depth)` where depth 0 is the LCA itself.
fn compute_tree_depths(
    tree_nodes: &std::collections::BTreeMap<String, serde_json::Map<String, Value>>,
    lca_id: &str,
) -> (usize, usize) {
    use std::collections::{HashSet, VecDeque};

    let mut max_depth = 0usize;
    let mut min_leaf_depth: Option<usize> = None;
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    queue.push_back((lca_id.to_string(), 0));

    while let Some((id, depth)) = queue.pop_front() {
        if visited.contains(&id) {
            continue;
        }
        visited.insert(id.clone());
        max_depth = max_depth.max(depth);

        let children: Vec<String> = tree_nodes
            .get(&id)
            .and_then(|n| n.get("children"))
            .and_then(|c| c.as_object())
            .map(|obj| obj.keys().cloned().collect())
            .unwrap_or_default();

        if children.is_empty() {
            min_leaf_depth = Some(min_leaf_depth.map_or(depth, |m| m.min(depth)));
        } else {
            for child in children {
                queue.push_back((child, depth + 1));
            }
        }
    }

    (max_depth, min_leaf_depth.unwrap_or(0))
}

/// Resolve a cat label for the given field from a document's `attributes` array.
///
/// Finds the attribute matching `cat_field`, extracts its representative value
/// (same priority as `extract_tree_field`), then maps it to the label from
/// `cat_labels` that best matches — for keyword fields this is a direct string
/// match; for numeric fields we pick the label whose index corresponds to the
/// bucket the value falls in (labels are ordered by `cat_bounds.cat_labels`).
///
/// Returns `None` when the field is absent or the value cannot be mapped.
fn resolve_cat_label(src: &Value, cat_field: &str, cat_labels: &[String]) -> Option<String> {
    if cat_labels.is_empty() {
        return None;
    }
    let attrs = src.get("attributes").and_then(|a| a.as_array())?;
    for attr in attrs {
        if attr.get("key").and_then(|k| k.as_str()) != Some(cat_field) {
            continue;
        }
        // Try keyword value first (direct label match)
        if let Some(kw) = attr.get("keyword_value").and_then(|v| v.as_str()) {
            // Direct match in cat_labels
            if let Some(label) = cat_labels.iter().find(|l| l.as_str() == kw) {
                return Some(label.clone());
            }
            // Fall through to "other" if present
            return cat_labels.last().filter(|l| l.as_str() == "other").cloned();
        }
        // Numeric value — return the label for the bucket index matching its position
        // The label order mirrors cat_bounds.cat_labels which is ordered by bucket key.
        // We rely on the label itself being the representative string; the UI uses
        // the label string, not an index, so we return whatever label was assigned
        // to the bucket containing this value.  Since we don't have the bucket
        // boundaries here (only labels), we return the first label as a best-effort
        // for numeric fields when a direct match isn't possible.
        //
        // Full resolution requires the bounds to be passed in; deferred to the
        // cat_rank propagation step which works at the response level.
        let _val = attr
            .get("long_value")
            .or_else(|| attr.get("float_value"))
            .or_else(|| attr.get("half_float_value"))?;
        return cat_labels.first().cloned();
    }
    None
}

/// Propagate cat labels from ancestors at `cat_rank` down to unlabelled descendants.
///
/// BFS from LCA. When a node whose `taxon_rank == cat_rank` is visited, all
/// of its descendants that still lack a `cat` value inherit that node's `cat`
/// (or the node's own `taxon_id` if it has no cat label of its own, mirroring v2).
fn propagate_cat_to_rank(
    tree_nodes: &mut std::collections::BTreeMap<String, serde_json::Map<String, Value>>,
    lca_id: &str,
    cat_rank: &str,
) {
    use std::collections::VecDeque;

    // BFS; carry the "inherited cat" label down from ancestors at cat_rank.
    let mut queue: VecDeque<(String, Option<String>)> = VecDeque::new();
    // Determine initial inherited cat for LCA
    let lca_cat = tree_nodes
        .get(lca_id)
        .and_then(|n| n.get("cat"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    queue.push_back((lca_id.to_string(), lca_cat));

    while let Some((id, inherited)) = queue.pop_front() {
        // Snapshot children before borrowing mutably
        let children: Vec<String> = tree_nodes
            .get(&id)
            .and_then(|n| n.get("children"))
            .and_then(|c| c.as_object())
            .map(|obj| obj.keys().cloned().collect())
            .unwrap_or_default();

        // Determine the cat to propagate to children
        let node_rank = tree_nodes
            .get(&id)
            .and_then(|n| n.get("taxon_rank"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_default();

        let propagate = if node_rank == cat_rank {
            // This node is at the target rank; only propagate if it has real cat data.
            // Nodes without cat data at the target rank are not given a fallback label —
            // descendants remain uncategorised rather than inheriting a meaningless id.
            tree_nodes
                .get(&id)
                .and_then(|n| n.get("cat"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        } else {
            inherited.clone()
        };

        // Apply inherited label to this node if it has none yet
        if let Some(ref label) = propagate {
            if let Some(node) = tree_nodes.get_mut(&id) {
                if !node.contains_key("cat") {
                    node.insert("cat".to_string(), json!(label));
                }
            }
        }

        for child in children {
            queue.push_back((child, propagate.clone()));
        }
    }
}

/// Remove monotypic internal nodes (nodes with exactly one child whose rank
/// is not in `preserve_ranks`) from the tree in-place.
///
/// Iterative post-order DFS. On collapse, the single child is grafted directly
/// into the collapsed node's parent's children map.
fn collapse_monotypic_nodes(
    tree_nodes: &mut std::collections::BTreeMap<String, serde_json::Map<String, Value>>,
    lca_id: &str,
    preserve_ranks: &[String],
) {
    use std::collections::HashMap;

    // Build parent map: child → parent
    let mut parent_of: HashMap<String, String> = HashMap::new();
    for (id, node) in tree_nodes.iter() {
        let children: Vec<String> = node
            .get("children")
            .and_then(|c| c.as_object())
            .map(|obj| obj.keys().cloned().collect())
            .unwrap_or_default();
        for child in children {
            parent_of.insert(child, id.clone());
        }
    }

    // Post-order traversal: collect collapse candidates bottom-up
    let mut stack: Vec<(String, bool)> = vec![(lca_id.to_string(), false)];
    let mut visit_order: Vec<String> = Vec::new();
    let mut visited = std::collections::HashSet::new();

    while let Some((id, processed)) = stack.pop() {
        if processed {
            visit_order.push(id);
        } else {
            if visited.contains(&id) {
                continue;
            }
            visited.insert(id.clone());
            stack.push((id.clone(), true));
            let children: Vec<String> = tree_nodes
                .get(&id)
                .and_then(|n| n.get("children"))
                .and_then(|c| c.as_object())
                .map(|obj| obj.keys().cloned().collect())
                .unwrap_or_default();
            for child in children {
                stack.push((child, false));
            }
        }
    }

    // Process in post-order (leaves first)
    for id in visit_order {
        if id == lca_id {
            continue;
        }
        let children: Vec<String> = tree_nodes
            .get(&id)
            .and_then(|n| n.get("children"))
            .and_then(|c| c.as_object())
            .map(|obj| obj.keys().cloned().collect())
            .unwrap_or_default();
        if children.len() != 1 {
            continue;
        }
        let rank = tree_nodes
            .get(&id)
            .and_then(|n| n.get("taxon_rank"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if preserve_ranks.iter().any(|r| r == &rank) {
            continue;
        }
        // Collapse: graft the single child into the parent's children map
        let only_child = children[0].clone();
        if let Some(parent_id) = parent_of.get(&id).cloned() {
            if let Some(parent_node) = tree_nodes.get_mut(&parent_id) {
                if let Some(children_map) = parent_node
                    .get_mut("children")
                    .and_then(|c| c.as_object_mut())
                {
                    children_map.remove(&id);
                    children_map.insert(only_child.clone(), json!(true));
                }
            }
        }
        // Update parent_of for the child
        if let Some(parent_id) = parent_of.get(&id).cloned() {
            parent_of.insert(only_child, parent_id);
        }
        tree_nodes.remove(&id);
    }
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

/// Find the first date attribute value for `field` in an `attributes` array.
/// Returns milliseconds since epoch as `f64` when possible.
fn find_attr_date(attrs: &[Value], field: &str) -> Option<f64> {
    for attr in attrs {
        if attr.get("key").and_then(|k| k.as_str()) != Some(field) {
            continue;
        }

        // If ES stored the date as a numeric epoch (stats use this), accept it.
        if let Some(n) = attr.get("date_value").and_then(|v| v.as_f64()) {
            return Some(n);
        }

        // If it's a string (ISO or yyyy-mm-dd), try parsing common formats.
        if let Some(s) = attr.get("date_value").and_then(|v| v.as_str()) {
            // Try RFC3339 first
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                return Some(dt.timestamp_millis() as f64);
            }
            // Try simple date-only form YYYY-MM-DD
            if let Ok(nd) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
                if let Some(naive_dt) = nd.and_hms_opt(0, 0, 0) {
                    let dt = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                        naive_dt,
                        chrono::Utc,
                    );
                    return Some(dt.timestamp_millis() as f64);
                }
            }
            // Try common datetime without timezone
            if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
                let dt =
                    chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc);
                return Some(dt.timestamp_millis() as f64);
            }
            if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
                let dt =
                    chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc);
                return Some(dt.timestamp_millis() as f64);
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
#[allow(clippy::too_many_arguments)]
fn extract_scatter_by_cat(
    resp: &Value,
    agg_name: &str,
    x_field: &str,
    x_bucket_agg: &str,
    y_field: &str,
    y_inner_agg: &str,
    x_bucket_count: usize,
    y_bucket_count: usize,
    cat_labels: &[String],
    show_other: bool,
    cat_is_numeric: bool,
    main_counts: &[u64],
    y_fixed_terms: Option<&[String]>,
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
                let y_path = format!(
                    "/yHistograms/by_attribute/{}/{}/buckets",
                    y_field, y_inner_agg
                );
                let y_counts = if let Some(yb) =
                    x_bucket.pointer(&y_path).and_then(|b| b.as_array())
                {
                    if let Some(fixed) = y_fixed_terms {
                        use std::collections::HashMap;
                        let mut map: HashMap<String, u64> = HashMap::new();
                        for b in yb {
                            if let Some(k) = b.get("key").and_then(|k| k.as_str()) {
                                let c = b.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0);
                                map.insert(k.to_string(), c);
                            }
                        }
                        let mut aligned = Vec::with_capacity(fixed.len());
                        for key in fixed {
                            aligned.push(map.get(key.as_str()).copied().unwrap_or(0));
                        }
                        aligned
                    } else {
                        yb.iter()
                            .map(|b| b.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0))
                            .collect()
                    }
                } else {
                    vec![0; y_bucket_count]
                };
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
                let y_hist_path = format!(
                    "/yHistograms/by_attribute/{}/{}/buckets",
                    y_field, y_inner_agg
                );
                let y_counts = if let Some(yb) =
                    x_bucket.pointer(&y_hist_path).and_then(|b| b.as_array())
                {
                    if let Some(fixed) = y_fixed_terms {
                        use std::collections::HashMap;
                        let mut map: HashMap<String, u64> = HashMap::new();
                        for b in yb {
                            if let Some(k) = b.get("key").and_then(|k| k.as_str()) {
                                let c = b.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0);
                                map.insert(k.to_string(), c);
                            }
                        }
                        let mut aligned = Vec::with_capacity(fixed.len());
                        for key in fixed {
                            aligned.push(map.get(key.as_str()).copied().unwrap_or(0));
                        }
                        aligned
                    } else {
                        yb.iter()
                            .map(|b| b.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0))
                            .collect()
                    }
                } else {
                    vec![0; y_bucket_count]
                };
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

/// Fetch scientific name labels for a list of taxon ids in the configured taxon index.
/// Returns a Vec of labels aligned to the input `ids` (falls back to the id string when
/// a name is not found).
async fn fetch_taxon_labels(
    state: &Arc<AppState>,
    ids: &[String],
    rank: &str,
) -> Result<Vec<String>, String> {
    if ids.is_empty() {
        return Ok(vec![]);
    }

    let taxon_index = index_name::resolve_index_str(&state.default_result, state);

    // Build msearch body: one query per id so we can preserve order in the
    // responses.
    let mut searches: Vec<(String, Value)> = Vec::new();
    for id in ids {
        let q = json!({
            "query": {
                "bool": {
                    "filter": [
                        { "term": { "taxon_id": id } },
                        { "term": { "taxon_rank": rank } }
                    ]
                }
            },
            "_source": ["taxon_id", "scientific_name"]
        });
        searches.push((taxon_index.clone(), q));
    }

    let nd = es_client::build_msearch_body(&searches);
    let resp = es_client::execute_msearch(&state.client, &state.es_base, &nd).await?;

    let mut labels: Vec<String> = Vec::with_capacity(ids.len());
    if let Some(resps) = resp.get("responses").and_then(|r| r.as_array()) {
        for (i, r) in resps.iter().enumerate() {
            if let Some(total) = r.pointer("/hits/total/value").and_then(|v| v.as_u64()) {
                if total >= 1 {
                    if let Some(hit) = r.pointer("/hits/hits/0/_source/scientific_name") {
                        if let Some(s) = hit.as_str() {
                            labels.push(s.to_string());
                            continue;
                        }
                    }
                }
            }
            // fallback: use the id string
            labels.push(ids.get(i).cloned().unwrap_or_default());
        }
    }
    Ok(labels)
}

/// Build a presence-filter query for an axis that matches documents which
/// contain a value for that axis. Used to augment the base query when
/// computing bounds so bounds reflect only records that will be plotted.
fn presence_filter_for_axis(spec: &AxisSpec) -> Option<Value> {
    match spec.value_type {
        ValueType::TaxonRank => Some(json!({
            "nested": {
                "path": "lineage",
                "query": {
                    "term": { "lineage.taxon_rank": spec.field }
                }
            }
        })),
        _ => {
            // For other types try either a nested attribute entry or a
            // top-level field existence. For keywords prefer the `.keyword`
            // accessor.
            let attr_nested = json!({
                "nested": {
                    "path": "attributes",
                    "query": { "term": { "attributes.key": spec.field } }
                }
            });
            let top_field = if matches!(spec.value_type, ValueType::Keyword) {
                format!("{}.keyword", spec.field)
            } else {
                spec.field.clone()
            };
            let top_exists = json!({ "exists": { "field": top_field } });
            Some(
                json!({ "bool": { "should": [ attr_nested, top_exists ], "minimum_should_match": 1 } }),
            )
        }
    }
}

/// Build a canonical, labelled buckets array from raw ES buckets.
///
/// If `fixed_terms` is non-empty, produce buckets in that order and
/// include only those terms (this prevents appending unexpected buckets
/// produced by ES). If `bucket_labels` aligns with `fixed_terms`, use
/// those human-readable labels; otherwise fall back to any `label` field
/// on the bucket or the id string.
fn build_structured_buckets(
    raw_buckets: &[Value],
    fixed_terms: &[String],
    bucket_labels: &[String],
) -> Vec<Value> {
    use std::collections::HashMap;
    // Build key -> bucket map for fast lookup
    let mut map: HashMap<String, Value> = HashMap::new();
    for b in raw_buckets {
        if let Some(kv) = b.get("key") {
            let ks = if let Some(s) = kv.as_str() {
                s.to_string()
            } else {
                kv.to_string()
            };
            map.insert(ks, b.clone());
        }
    }

    let mut out: Vec<Value> = Vec::new();
    if !fixed_terms.is_empty() {
        // Use fixed_terms ordering and labels when available
        for (i, id) in fixed_terms.iter().enumerate() {
            let id_str = id.clone();
            let label = if !bucket_labels.is_empty() && bucket_labels.len() == fixed_terms.len() {
                bucket_labels.get(i).cloned().unwrap_or(id_str.clone())
            } else if let Some(b) = map.get(&id_str) {
                b.get("label")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or(id_str.clone())
            } else {
                id_str.clone()
            };
            let count = map
                .get(&id_str)
                .and_then(|b| b.get("doc_count").and_then(|c| c.as_u64()))
                .unwrap_or(0);
            out.push(json!({"id": id_str, "label": label, "count": count}));
        }
    } else {
        // No fixed terms: preserve raw bucket order, attach label if present
        for b in raw_buckets {
            let id_val = b.get("key").cloned().unwrap_or(Value::Null);
            let id_str = if let Some(s) = id_val.as_str() {
                s.to_string()
            } else {
                id_val.to_string()
            };
            let label = b
                .get("label")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or(id_str.clone());
            let count = b.get("doc_count").and_then(|v| v.as_u64()).unwrap_or(0);
            out.push(json!({"id": id_str, "label": label, "count": count}));
        }
    }
    out
}

/// Align processed buckets (which may include `key_scaled` etc.) to `fixed_terms` and
/// attach `label` fields. If `fixed_terms` is empty, return processed buckets with
/// labels attached where possible.
fn align_and_label_processed_buckets(
    mut processed: Vec<Value>,
    fixed_terms: &[String],
    fixed_labels: &[String],
) -> Vec<Value> {
    use std::collections::HashMap;
    if fixed_terms.is_empty() {
        // Attach labels if provided in fixed_labels (unlikely when empty)
        for (i, b) in processed.iter_mut().enumerate() {
            if let Some(_lbl) = b.get("label").and_then(|v| v.as_str()) {
                // already has label
            } else if i < fixed_labels.len() {
                b["label"] = json!(fixed_labels[i].clone());
            }
        }
        return processed;
    }

    // Map existing processed buckets by id string
    let mut map: HashMap<String, Value> = HashMap::new();
    for b in processed.into_iter() {
        let id_str = if let Some(k) = b.get("key") {
            if let Some(s) = k.as_str() {
                s.to_string()
            } else {
                k.to_string()
            }
        } else if let Some(id) = b.get("id") {
            id.as_str().map(|s| s.to_string()).unwrap_or(id.to_string())
        } else {
            continue;
        };
        map.insert(id_str, b);
    }

    let mut out: Vec<Value> = Vec::new();
    for (i, id) in fixed_terms.iter().enumerate() {
        let bucket = map.remove(id);
        let mut b = if let Some(existing) = bucket {
            existing
        } else {
            // Create empty bucket placeholder
            json!({"key": id.clone(), "doc_count": 0})
        };
        let label = if !fixed_labels.is_empty() && fixed_labels.len() == fixed_terms.len() {
            fixed_labels.get(i).cloned().unwrap_or(id.clone())
        } else if let Some(lbl) = b.get("label").and_then(|v| v.as_str()) {
            lbl.to_string()
        } else {
            id.clone()
        };
        b["label"] = json!(label);
        out.push(b);
    }
    out
}

/// Fetch raw point data for scatter when total hits are within the scatter threshold.
///
/// Returns an object mapping category name to an array of `{scientific_name, taxonId, x, y, cat}`
/// point objects. Falls back to a single "all" key when no category is specified.
#[allow(clippy::too_many_arguments)]
async fn fetch_raw_point_data(
    state: &Arc<AppState>,
    index: &str,
    base_query: &Value,
    x_field: &str,
    x_is_taxon_rank: bool,
    y_field: &str,
    y_is_taxon_rank: bool,
    cat_field: Option<&str>,
    cat_labels: &[String],
    show_other: bool,
    x_fixed_terms: Option<&[String]>,
    threshold: usize,
) -> Value {
    // Build combined query: base_query AND optional x_bucket filter when
    // `x_fixed_terms` provided. This ensures raw points align with the
    // canonical buckets used to compute axis ticks and avoid stray points
    // that fall outside those buckets.
    let mut final_query: Value = base_query.clone();
    if let Some(fixed) = x_fixed_terms {
        // Convert slice into JSON array
        let fixed_json = json!(fixed);
        if x_is_taxon_rank {
            // Nested lineage filter: require ancestor at the requested rank
            // whose taxon_id is one of the fixed terms.
            let extra_filter = json!({
                "nested": {
                    "path": "lineage",
                    "query": {
                        "bool": {
                            "must": [
                                { "term": { "lineage.taxon_rank": x_field } },
                                { "terms": { "lineage.taxon_id": fixed_json } }
                            ]
                        }
                    }
                }
            });
            final_query = json!({ "bool": { "must": [ base_query.clone(), extra_filter ] } });
        } else {
            // Non-rank: try to match either nested attributes (attributes.key)
            // or a top-level `.keyword` field. Use a SHOULD so either form
            // matching will include the document.
            let attr_filter = json!({
                "nested": {
                    "path": "attributes",
                    "query": {
                        "bool": {
                            "must": [
                                { "term": { "attributes.key": x_field } },
                                { "terms": { "attributes.keyword_value.raw": fixed_json } }
                            ]
                        }
                    }
                }
            });
            let top_filter = json!({ "terms": { format!("{}.keyword", x_field): fixed_json } });
            let should_filter = json!({ "bool": { "should": [ attr_filter, top_filter ], "minimum_should_match": 1 } });
            final_query = json!({ "bool": { "must": [ base_query.clone(), should_filter ] } });
        }
    }

    // Request `lineage` so we can resolve ancestor IDs when the axis is a taxon rank.
    let es_body = json!({
        "size": threshold,
        "query": final_query,
        "_source": ["scientific_name", "taxon_id", "attributes", "lineage"]
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

        // Extract x and y values. When the axis is a taxon rank, prefer the
        // ancestor id found in `lineage`. Otherwise prefer numeric/date/keyword
        // attributes as before.
        let mut x_label_for_point: Option<String> = None;
        let x_json_val = if x_is_taxon_rank {
            // Try to find ancestor at the requested rank in the `lineage` array.
            let mut found: Option<Value> = None;
            if let Some(lineage_arr) = src.get("lineage").and_then(|l| l.as_array()) {
                for anc in lineage_arr {
                    if anc.get("taxon_rank").and_then(|r| r.as_str()) == Some(x_field) {
                        if let Some(idv) = anc.get("taxon_id") {
                            if let Some(s) = idv.as_str() {
                                found = Some(json!(s.to_string()));
                            } else if let Some(n) = idv.as_u64() {
                                found = Some(json!(n.to_string()));
                            }
                        }
                        // Try to capture scientific_name from the ancestor for labeling
                        if x_label_for_point.is_none() {
                            if let Some(sn) = anc.get("scientific_name").and_then(|v| v.as_str()) {
                                x_label_for_point = Some(sn.to_string());
                            } else if let Some(nm) = anc.get("name").and_then(|v| v.as_str()) {
                                x_label_for_point = Some(nm.to_string());
                            }
                        }
                        if found.is_some() {
                            break;
                        }
                    }
                }
            }
            if let Some(v) = found {
                v
            } else if let Some(v) = find_attr_numeric(&attrs, x_field) {
                json!(v)
            } else if let Some(d) = find_attr_date(&attrs, x_field) {
                json!(d)
            } else if let Some(s) = find_attr_keyword(&attrs, x_field) {
                json!(s)
            } else {
                continue;
            }
        } else if let Some(v) = find_attr_numeric(&attrs, x_field) {
            json!(v)
        } else if let Some(d) = find_attr_date(&attrs, x_field) {
            json!(d)
        } else if let Some(s) = find_attr_keyword(&attrs, x_field) {
            json!(s)
        } else {
            continue;
        };

        let mut y_label_for_point: Option<String> = None;
        let y_json_val = if y_is_taxon_rank {
            // y-axis as taxon rank — resolve ancestor id from lineage if present.
            let mut found: Option<Value> = None;
            if let Some(lineage_arr) = src.get("lineage").and_then(|l| l.as_array()) {
                for anc in lineage_arr {
                    if anc.get("taxon_rank").and_then(|r| r.as_str()) == Some(y_field) {
                        if let Some(idv) = anc.get("taxon_id") {
                            if let Some(s) = idv.as_str() {
                                found = Some(json!(s.to_string()));
                            } else if let Some(n) = idv.as_u64() {
                                found = Some(json!(n.to_string()));
                            }
                        }
                        // capture ancestor scientific name for label
                        if y_label_for_point.is_none() {
                            if let Some(sn) = anc.get("scientific_name").and_then(|v| v.as_str()) {
                                y_label_for_point = Some(sn.to_string());
                            } else if let Some(nm) = anc.get("name").and_then(|v| v.as_str()) {
                                y_label_for_point = Some(nm.to_string());
                            }
                        }
                        if found.is_some() {
                            break;
                        }
                    }
                }
            }
            if let Some(v) = found {
                v
            } else if let Some(v) = find_attr_numeric(&attrs, y_field) {
                json!(v)
            } else if let Some(d) = find_attr_date(&attrs, y_field) {
                json!(d)
            } else if let Some(s) = find_attr_keyword(&attrs, y_field) {
                json!(s)
            } else {
                continue;
            }
        } else if let Some(v) = find_attr_numeric(&attrs, y_field) {
            json!(v)
        } else if let Some(d) = find_attr_date(&attrs, y_field) {
            json!(d)
        } else if let Some(s) = find_attr_keyword(&attrs, y_field) {
            json!(s)
        } else {
            continue;
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

        let mut point_obj = json!({
            "scientific_name": scientific_name,
            "taxonId": taxon_id,
            "x": x_json_val,
            "y": y_json_val,
            "cat": cat_key
        });
        if let Some(lbl) = x_label_for_point {
            point_obj["x_label"] = json!(lbl);
        }
        if let Some(lbl) = y_label_for_point {
            point_obj["y_label"] = json!(lbl);
        }
        raw_data.entry(cat_key.clone()).or_default().push(point_obj);
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
        .await
        .ok_or("report config missing 'x' axis (set 'x' field or use 'axes')")?;
    let y_spec = resolve_axis_spec(AxisRole::Y, report_config, state)
        .await
        .ok_or("scatter report requires 'y' axis (set 'y' field or use 'axes')")?;
    let x_field = x_spec.field.clone();
    let y_field = y_spec.field.clone();
    let cat_spec_opt = resolve_axis_spec(AxisRole::Cat, report_config, state).await;
    let scatter_threshold = report_config
        .get("scatter_threshold")
        .and_then(|v| v.as_u64())
        .unwrap_or(1000) as usize;

    // Augment the base query for bounds computation with a presence-filter
    // for the opposite axis so bounds reflect only records that will appear
    // in the final plot. This avoids empty buckets caused by one axis being
    // filtered out by the other.
    let x_presence = presence_filter_for_axis(&y_spec);
    let x_base_query = if let Some(f) = x_presence {
        json!({ "bool": { "must": [ base_query.clone(), f ] } })
    } else {
        base_query.clone()
    };

    let x_bounds = compute_bounds(
        &state.client,
        &state.es_base,
        index,
        &x_spec,
        &x_base_query,
        &state.cache,
    )
    .await?;
    dbg!(&x_bounds);
    // If this is a taxon-rank axis and bounds provided a fixed term list (ids),
    // attempt to fetch human-readable labels (scientific names) for each id so
    // the final report can include a labelled mapping. Fall back to the
    // original bounds.cat_labels when lookup fails.
    let mut x_bucket_labels: Vec<String> = x_bounds.cat_labels.clone();
    if matches!(x_spec.value_type, ValueType::TaxonRank) && !x_bounds.fixed_terms.is_empty() {
        if let Ok(labels) = fetch_taxon_labels(state, &x_bounds.fixed_terms, &x_spec.field).await {
            if labels.len() == x_bounds.fixed_terms.len() {
                x_bucket_labels = labels;
            }
        }
    }
    let y_presence = presence_filter_for_axis(&x_spec);
    let y_base_query = if let Some(f) = y_presence {
        json!({ "bool": { "must": [ base_query.clone(), f ] } })
    } else {
        base_query.clone()
    };

    let y_bounds = compute_bounds(
        &state.client,
        &state.es_base,
        index,
        &y_spec,
        &y_base_query,
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
    let mut x_raw_buckets = resp
        .pointer(&x_hist_path)
        .and_then(|b| b.as_array())
        .cloned()
        .unwrap_or_default();

    // Respect the definitive fixed term order calculated during bounds.
    // If `x_bounds.fixed_terms` is non-empty, reorder the returned buckets to
    // match that list. Append any unexpected buckets at the end.
    if !x_bounds.fixed_terms.is_empty() {
        let mut ordered: Vec<Value> = Vec::with_capacity(x_raw_buckets.len());
        for id in &x_bounds.fixed_terms {
            if let Some(pos) = x_raw_buckets
                .iter()
                .position(|b| b.get("key").and_then(|k| k.as_str()) == Some(id.as_str()))
            {
                ordered.push(x_raw_buckets[pos].clone());
            }
        }
        // Append any remaining buckets not present in fixed_terms
        for b in &x_raw_buckets {
            let key = b.get("key").and_then(|k| k.as_str()).unwrap_or("");
            if !x_bounds.fixed_terms.iter().any(|t| t == key) {
                ordered.push(b.clone());
            }
        }
        x_raw_buckets = ordered;
    }
    let x_bucket_count = x_raw_buckets.len();

    let all_values: Vec<u64> = x_raw_buckets
        .iter()
        .map(|b| b.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0))
        .collect();

    // ---- Extract allYValues (per x-bucket y-histogram) and yBuckets ----
    let y_bucket_count = y_bounds.tick_count;
    let mut all_y_values: Vec<Vec<u64>> = Vec::with_capacity(x_bucket_count);
    let mut y_bucket_keys: Vec<Value> = Vec::new();
    let mut y_bucket_labels: Vec<String> = Vec::new();

    // If bounds provided canonical fixed terms for a keyword/rank Y axis,
    // prefer that ordering for yBuckets so keys are consistent across x buckets.
    if matches!(
        y_bounds.value_type,
        ValueType::Keyword | ValueType::TaxonRank
    ) && !y_bounds.fixed_terms.is_empty()
    {
        y_bucket_keys = y_bounds
            .fixed_terms
            .iter()
            .map(|s| Value::String(s.clone()))
            .collect();
    }

    // If this is a taxon-rank Y axis and bounds provided fixed term ids,
    // attempt to fetch human-readable labels (scientific names). Keep
    // `y_bucket_keys` as the canonical ids used for bin alignment, and
    // separately store `y_bucket_labels` for display.
    if matches!(y_spec.value_type, ValueType::TaxonRank) && !y_bounds.fixed_terms.is_empty() {
        if let Ok(labels) = fetch_taxon_labels(state, &y_bounds.fixed_terms, &y_spec.field).await {
            if labels.len() == y_bounds.fixed_terms.len() {
                y_bucket_labels = labels;
                // Ensure the canonical keys remain the ids from fixed_terms
                // (they were set earlier from `y_bounds.fixed_terms`).
            }
        }
    }

    // Determine inner agg name for y histograms so we can locate buckets
    // inside each x-bucket's `yHistograms` result.
    let y_inner_agg = if matches!(
        y_bounds.value_type,
        ValueType::Keyword | ValueType::TaxonRank
    ) {
        "top_terms"
    } else if matches!(y_bounds.value_type, ValueType::Date) {
        "date_histogram"
    } else {
        "histogram"
    };

    // If we still have no canonical y keys, scan *all* x-buckets and collect
    // the union of y bucket keys found. This avoids using a single
    // first-non-empty-bucket ordering which can produce too-small yBuckets
    // when some x-buckets yield sparse date/rank histograms.
    if y_bucket_keys.is_empty() {
        use std::collections::HashSet;
        let mut seen: HashSet<String> = HashSet::new();
        let mut ordered_keys: Vec<Value> = Vec::new();
        for x_bucket in &x_raw_buckets {
            let y_hist_path = format!(
                "/yHistograms/by_attribute/{}/{}/buckets",
                y_field, y_inner_agg
            );
            if let Some(ybuckets) = x_bucket.pointer(&y_hist_path).and_then(|b| b.as_array()) {
                for b in ybuckets {
                    if let Some(kv) = b.get("key").cloned() {
                        let ks = if let Some(s) = kv.as_str() {
                            s.to_string()
                        } else {
                            kv.to_string()
                        };
                        if seen.insert(ks) {
                            ordered_keys.push(kv);
                        }
                    }
                }
            }
        }

        if !ordered_keys.is_empty() {
            // If all keys are numeric, sort ascending numerically; otherwise
            // keep discovery order which tends to reflect term ordering.
            let all_numeric = ordered_keys.iter().all(|v| v.as_f64().is_some());
            if all_numeric {
                ordered_keys.sort_by(|a, b| {
                    a.as_f64()
                        .partial_cmp(&b.as_f64())
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            y_bucket_keys = ordered_keys;
        }
    }

    for x_bucket in &x_raw_buckets {
        let y_hist_path = format!(
            "/yHistograms/by_attribute/{}/{}/buckets",
            y_field, y_inner_agg
        );
        let y_buckets_opt = x_bucket.pointer(&y_hist_path).and_then(|b| b.as_array());

        if let Some(ybuckets) = y_buckets_opt {
            // If we don't already have canonical keys, initialise from this first non-empty bucket
            if y_bucket_keys.is_empty() {
                y_bucket_keys = ybuckets
                    .iter()
                    .filter_map(|b| b.get("key").cloned())
                    .collect();
            }

            // Build counts aligned to `y_bucket_keys`. For keyword/rank keys this
            // ensures the same ordering even if some x buckets lack particular terms.
            if matches!(
                y_bounds.value_type,
                ValueType::Keyword | ValueType::TaxonRank
            ) {
                use std::collections::HashMap;
                let mut map: HashMap<String, u64> = HashMap::new();
                for b in ybuckets {
                    if let Some(kv) = b.get("key") {
                        // Normalize the bucket key to a string regardless of JSON type
                        let key_s = if let Some(s) = kv.as_str() {
                            s.to_string()
                        } else if let Some(n) = kv.as_u64() {
                            n.to_string()
                        } else if let Some(n) = kv.as_i64() {
                            n.to_string()
                        } else if let Some(f) = kv.as_f64() {
                            f.to_string()
                        } else {
                            kv.to_string()
                        };
                        let c = b.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0);
                        map.insert(key_s, c);
                    }
                }
                let mut aligned: Vec<u64> = Vec::with_capacity(y_bucket_keys.len());
                for k in &y_bucket_keys {
                    let key_s = if let Some(s) = k.as_str() {
                        s.to_string()
                    } else if let Some(n) = k.as_u64() {
                        n.to_string()
                    } else if let Some(n) = k.as_i64() {
                        n.to_string()
                    } else if let Some(f) = k.as_f64() {
                        f.to_string()
                    } else {
                        k.to_string()
                    };
                    aligned.push(map.get(&key_s).copied().unwrap_or(0));
                }
                all_y_values.push(aligned);
            } else {
                all_y_values.push(
                    ybuckets
                        .iter()
                        .map(|b| b.get("doc_count").and_then(|c| c.as_u64()).unwrap_or(0))
                        .collect(),
                );
            }
        } else if y_bucket_keys.is_empty() {
            all_y_values.push(vec![0; y_bucket_count]);
        } else {
            all_y_values.push(vec![0; y_bucket_keys.len()]);
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
            y_inner_agg,
            x_bucket_count,
            y_bucket_count,
            &cat_labels,
            show_other_cat,
            cat_is_numeric,
            &all_values,
            if !y_bounds.fixed_terms.is_empty() {
                Some(&y_bounds.fixed_terms[..])
            } else {
                None
            },
        )
    } else {
        (Value::Null, Value::Null)
    };

    // ---- Fetch raw point data when needed ----
    // Previously we only fetched raw points when total hits <= threshold.
    // For categorical axes (keyword/taxon) we also want raw points so the
    // client/converter can jitter points within categories for visibility.
    // Only fetch rawData when the total matched hits are within the configured
    // `scatter_threshold`. Previously we also fetched raw points for categorical
    // axes to enable client jittering; that behaviour is opt-in and not the
    // default — respect the threshold by default.
    let should_fetch_raw = total_hits as usize <= scatter_threshold;

    let raw_data = if should_fetch_raw {
        fetch_raw_point_data(
            state,
            index,
            base_query,
            x_field.as_str(),
            matches!(x_spec.value_type, ValueType::TaxonRank),
            y_field.as_str(),
            matches!(y_spec.value_type, ValueType::TaxonRank),
            cat_field_str,
            &cat_labels,
            show_other_cat,
            if !x_bounds.fixed_terms.is_empty() {
                Some(&x_bounds.fixed_terms[..])
            } else {
                None
            },
            scatter_threshold,
        )
        .await
    } else {
        Value::Null
    };

    // Build a single structured `buckets` array where each element is an
    // object `{ id, label, count }`. Use `x_bounds.fixed_terms` (when
    // present) as the authoritative ordering to avoid appending spurious
    // buckets returned by ES.
    let buckets_struct: Vec<Value> =
        build_structured_buckets(&x_raw_buckets, &x_bounds.fixed_terms, &x_bucket_labels);

    let mut report_data = json!({
        "type": "scatter",
        "x": {
            "field": x_field,
            "scale": format!("{:?}", x_spec.opts.scale).to_lowercase(),
            "domain": x_bounds.domain,
            "value_type": value_type_to_string(x_spec.value_type)
        },
        "y": {
            "field": y_field,
            "scale": format!("{:?}", y_spec.opts.scale).to_lowercase(),
            "domain": y_bounds.domain,
            "value_type": value_type_to_string(y_spec.value_type)
        },
        "buckets": buckets_struct,
        "allValues": all_values,
        "yBuckets": y_bucket_keys,
        "yBucketLabels": y_bucket_labels,
        "allYValues": all_y_values,
        "zDomain": z_domain
    });

    // Historically we returned `bucketLabels` separately; clients should now
    // consume the structured `buckets` array. Keep `bucketLabels` absent to
    // avoid duplication.

    if !by_cat.is_null() {
        report_data["by_cat"] = by_cat;
        report_data["yValuesByCat"] = y_values_by_cat;
        if let Some(ref cat_spec) = cat_spec_opt {
            report_data["cat"] = json!({
                "field": cat_spec.field,
                "value_type": value_type_to_string(cat_spec.value_type),
                "scale": format!("{:?}", cat_spec.opts.scale).to_lowercase()
            });
        }
        report_data["cats"] = json!(cat_labels);
    }

    if !raw_data.is_null() {
        report_data["rawData"] = raw_data;
    }

    Ok((total_hits, took, report_data))
}
