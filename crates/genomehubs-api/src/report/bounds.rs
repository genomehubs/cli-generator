//! Probe Elasticsearch for axis field domains and cardinality.
//!
//! Each `compute_*_bounds()` function issues one ES aggregation query to determine
//! the actual data range for a field, then wraps it in a `BoundsResult`.

use genomehubs_query::report::axis::{DateInterval, Scale, ValueType};
use genomehubs_query::report::{AxisSpec, BoundsResult};
use reqwest::Client;
use serde_json::{json, Value};

use crate::es_client;

/// Probe Elasticsearch for the domain of a single axis field.
///
/// Issues one stats/terms aggregation against `index` to determine:
/// - For numeric/date fields: `[min, max]` domain, suggested tick count
/// - For keyword/taxon_rank fields: the top `spec.opts.size` terms
///
/// The `base_query` is ANDed with the existing query so bounds reflect
/// only the data that will appear in the report (not the whole index).
pub async fn compute_bounds(
    client: &Client,
    es_base: &str,
    index: &str,
    spec: &AxisSpec,
    base_query: &Value,
) -> Result<BoundsResult, String> {
    match spec.value_type {
        ValueType::Numeric => {
            compute_numeric_bounds(client, es_base, index, spec, base_query).await
        }
        ValueType::Date => compute_date_bounds(client, es_base, index, spec, base_query).await,
        ValueType::Keyword | ValueType::TaxonRank => {
            compute_keyword_bounds(client, es_base, index, spec, base_query).await
        }
        ValueType::GeoPoint => compute_geo_bounds(client, es_base, index, spec, base_query).await,
    }
}

async fn compute_numeric_bounds(
    client: &Client,
    es_base: &str,
    index: &str,
    spec: &AxisSpec,
    base_query: &Value,
) -> Result<BoundsResult, String> {
    let agg_body = json!({
        "size": 0,
        "query": base_query,
        "aggs": {
            "field_stats": {
                "stats": { "field": &spec.field }
            }
        }
    });

    let resp = es_client::execute_search(client, es_base, index, &agg_body).await?;

    let stats = resp
        .pointer("/aggregations/field_stats")
        .ok_or_else(|| "missing field_stats aggregation".to_string())?;

    let raw_min = stats.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let raw_max = stats.get("max").and_then(|v| v.as_f64()).unwrap_or(1.0);

    // Apply log scale adjustment: domain must be > 0 for log scales
    let (domain_min, domain_max) =
        if matches!(spec.opts.scale, Scale::Log | Scale::Log2 | Scale::Log10) {
            let floor = if raw_min > 0.0 { raw_min } else { 1.0 };
            (floor, raw_max.max(floor))
        } else {
            (raw_min, raw_max)
        };

    // Override with user-specified domain if provided
    let (final_min, final_max) = spec
        .opts
        .min
        .as_deref()
        .and_then(|s| s.parse::<f64>().ok())
        .and_then(|min_val| {
            spec.opts
                .max
                .as_deref()
                .and_then(|s| s.parse::<f64>().ok())
                .map(|max_val| (min_val, max_val))
        })
        .unwrap_or((domain_min, domain_max));

    Ok(BoundsResult {
        domain: Some([final_min, final_max]),
        tick_count: spec.opts.size,
        interval: None,
        scale: spec.opts.scale,
        value_type: spec.value_type,
        fixed_terms: vec![],
        cat_labels: vec![],
    })
}

async fn compute_date_bounds(
    client: &Client,
    es_base: &str,
    index: &str,
    spec: &AxisSpec,
    base_query: &Value,
) -> Result<BoundsResult, String> {
    let agg_body = json!({
        "size": 0,
        "query": base_query,
        "aggs": {
            "date_range": {
                "stats": { "field": &spec.field }
            }
        }
    });

    let resp = es_client::execute_search(client, es_base, index, &agg_body).await?;
    let stats = resp
        .pointer("/aggregations/date_range")
        .cloned()
        .unwrap_or_default();

    let min_ms = stats.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let max_ms = stats.get("max").and_then(|v| v.as_f64()).unwrap_or(0.0);

    // Use user-specified interval if provided, otherwise auto-select based on range
    let interval = spec
        .opts
        .interval
        .or_else(|| auto_date_interval(max_ms - min_ms));

    Ok(BoundsResult {
        domain: Some([min_ms, max_ms]),
        tick_count: spec.opts.size,
        interval,
        scale: Scale::Date,
        value_type: ValueType::Date,
        fixed_terms: vec![],
        cat_labels: vec![],
    })
}

async fn compute_keyword_bounds(
    client: &Client,
    es_base: &str,
    index: &str,
    spec: &AxisSpec,
    base_query: &Value,
) -> Result<BoundsResult, String> {
    // Use fixed_values if provided in opts; skip ES round-trip
    if !spec.opts.fixed_values.is_empty() {
        let labels: Vec<String> = spec
            .opts
            .fixed_values
            .iter()
            .map(|(_, label)| label.clone())
            .collect();
        let values: Vec<String> = spec
            .opts
            .fixed_values
            .iter()
            .map(|(val, _)| val.clone())
            .collect();
        return Ok(BoundsResult {
            domain: None,
            tick_count: labels.len(),
            interval: None,
            scale: Scale::Ordinal,
            value_type: spec.value_type,
            fixed_terms: values,
            cat_labels: labels,
        });
    }

    let agg_body = json!({
        "size": 0,
        "query": base_query,
        "aggs": {
            "top_terms": {
                "terms": {
                    "field": format!("{}.keyword", &spec.field),
                    "size": spec.opts.size,
                    "min_doc_count": 0
                }
            }
        }
    });

    let resp = es_client::execute_search(client, es_base, index, &agg_body).await?;

    let buckets = resp
        .pointer("/aggregations/top_terms/buckets")
        .and_then(|b| b.as_array())
        .cloned()
        .unwrap_or_default();

    let terms: Vec<String> = buckets
        .iter()
        .filter_map(|b| b.get("key").and_then(|k| k.as_str()).map(|s| s.to_string()))
        .collect();

    Ok(BoundsResult {
        domain: None,
        tick_count: terms.len(),
        interval: None,
        scale: Scale::Ordinal,
        value_type: spec.value_type,
        fixed_terms: terms.clone(),
        cat_labels: terms,
    })
}

async fn compute_geo_bounds(
    client: &Client,
    es_base: &str,
    index: &str,
    spec: &AxisSpec,
    base_query: &Value,
) -> Result<BoundsResult, String> {
    let agg_body = json!({
        "size": 0,
        "query": base_query,
        "aggs": {
            "viewport": {
                "geo_bounds": {
                    "field": &spec.field,
                    "wrap_longitude": true
                }
            }
        }
    });

    let resp = es_client::execute_search(client, es_base, index, &agg_body).await?;

    // Encode geo viewport as [lon_min, lon_max] for geohash precision selection
    let bounds = resp
        .pointer("/aggregations/viewport/bounds")
        .cloned()
        .unwrap_or_default();
    let tl_lon = bounds
        .pointer("/top_left/lon")
        .and_then(|v| v.as_f64())
        .unwrap_or(-180.0);
    let br_lon = bounds
        .pointer("/bottom_right/lon")
        .and_then(|v| v.as_f64())
        .unwrap_or(180.0);

    // Simplified: just capture the longitude span for geohash grid precision selection
    Ok(BoundsResult {
        domain: Some([tl_lon, br_lon]),
        tick_count: spec.opts.size,
        interval: None,
        scale: Scale::Linear,
        value_type: ValueType::GeoPoint,
        fixed_terms: vec![],
        cat_labels: vec![],
    })
}

/// Auto-select a date interval based on the time range span in milliseconds.
///
/// Selects the most appropriate calendar interval for rendering:
/// - < 30 days → Day
/// - < 6 years → Month
/// - < 50 years → Year
/// - >= 50 years → Decade
pub fn auto_date_interval(range_ms: f64) -> Option<DateInterval> {
    const DAY_MS: f64 = 86_400_000.0;
    const YEAR_MS: f64 = DAY_MS * 365.25;

    if range_ms <= 0.0 {
        return None;
    }

    Some(if range_ms < 30.0 * DAY_MS {
        DateInterval::Day
    } else if range_ms < 6.0 * YEAR_MS {
        DateInterval::Month
    } else if range_ms < 50.0 * YEAR_MS {
        DateInterval::Year
    } else {
        DateInterval::Decade
    })
}
