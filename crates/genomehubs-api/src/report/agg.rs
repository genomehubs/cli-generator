//! Elasticsearch aggregation builders for report axes.
//!
//! Each `AggBuilder` produces the JSON fragment for one ES aggregation, and extracts
//! the bucket list from the response. Builders are composable: use `CompositeAggBuilder`
//! to nest them (e.g., histogram containing stats).

use serde_json::{json, Value};

use crate::es_metadata::MetadataCache;
use genomehubs_query::report::axis::{DateInterval, Scale, ValueType};
use genomehubs_query::report::{AxisSpec, BoundsResult};
use std::sync::Arc;

/// Raw bucket list extracted from an ES aggregation response.
pub type RawBuckets = Vec<Value>;

/// Build an ES aggregation body for a given axis configuration.
///
/// Implementations are responsible for:
/// - Returning the JSON fragment to insert under `"aggs"` in the ES request body
/// - Extracting `RawBuckets` from the ES response
pub trait AggBuilder: Send + Sync {
    /// Return the ES aggregation JSON for this axis, keyed by `agg_name`.
    fn build(&self, agg_name: &str) -> Value;

    /// Extract the bucket list from the ES response under the given aggregation path.
    fn extract(&self, resp: &Value, agg_name: &str) -> RawBuckets;
}

/// Build a numeric `histogram` aggregation.
pub struct HistogramAggBuilder {
    pub field: String,
    pub interval: f64,
    pub min: f64,
    pub max: f64,
    pub script: Option<String>,
}

impl AggBuilder for HistogramAggBuilder {
    fn build(&self, agg_name: &str) -> Value {
        let mut hist = json!({
            "field": &self.field,
            "interval": self.interval,
            "extended_bounds": { "min": self.min, "max": self.max },
            "min_doc_count": 0
        });

        if let Some(script) = &self.script {
            hist["script"] = Value::String(script.clone());
        }

        json!({
            agg_name: {
                "histogram": hist
            }
        })
    }

    fn extract(&self, resp: &Value, agg_name: &str) -> RawBuckets {
        resp.pointer(&format!("/aggregations/{agg_name}/buckets"))
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default()
    }
}

/// Build a `date_histogram` aggregation with `calendar_interval`.
pub struct DateHistogramAggBuilder {
    pub field: String,
    pub calendar_interval: String, // "1d", "1w", "1M", "3M", "1y", "10y"
    pub time_zone: Option<String>,
}

impl AggBuilder for DateHistogramAggBuilder {
    fn build(&self, agg_name: &str) -> Value {
        let mut agg = json!({
            "date_histogram": {
                "field": &self.field,
                "calendar_interval": &self.calendar_interval,
                "min_doc_count": 0
            }
        });
        if let Some(tz) = &self.time_zone {
            agg["date_histogram"]["time_zone"] = Value::String(tz.clone());
        }
        json!({ agg_name: agg })
    }

    fn extract(&self, resp: &Value, agg_name: &str) -> RawBuckets {
        resp.pointer(&format!("/aggregations/{agg_name}/buckets"))
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default()
    }
}

/// Build a `terms` aggregation for categorical axes.
pub struct TermsAggBuilder {
    pub field: String,
    pub size: usize,
    pub include: Option<Vec<String>>, // fixed term list
}

impl AggBuilder for TermsAggBuilder {
    fn build(&self, agg_name: &str) -> Value {
        let mut terms = json!({
            "field": format!("{}.keyword", &self.field),
            "size": self.size,
            "min_doc_count": 0
        });
        if let Some(include) = &self.include {
            terms["include"] = json!(include);
        }
        json!({ agg_name: { "terms": terms } })
    }

    fn extract(&self, resp: &Value, agg_name: &str) -> RawBuckets {
        resp.pointer(&format!("/aggregations/{agg_name}/buckets"))
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default()
    }
}

/// Build a `stats` sub-aggregation (used for Y-axis values within X buckets).
#[allow(dead_code)]
pub struct StatsAggBuilder {
    pub field: String,
}

impl AggBuilder for StatsAggBuilder {
    fn build(&self, agg_name: &str) -> Value {
        json!({ agg_name: { "stats": { "field": &self.field } } })
    }

    fn extract(&self, resp: &Value, agg_name: &str) -> RawBuckets {
        // Stats returns a single object, not a bucket list; return wrapped
        resp.pointer(&format!("/aggregations/{agg_name}"))
            .cloned()
            .into_iter()
            .collect()
    }
}

/// Build a `geohash_grid` aggregation for map reports.
pub struct GeoHashAggBuilder {
    pub field: String,
    pub precision: u8,
    pub size: usize,
}

impl AggBuilder for GeoHashAggBuilder {
    fn build(&self, agg_name: &str) -> Value {
        json!({
            agg_name: {
                "geohash_grid": {
                    "field": &self.field,
                    "precision": self.precision,
                    "size": self.size
                }
            }
        })
    }

    fn extract(&self, resp: &Value, agg_name: &str) -> RawBuckets {
        resp.pointer(&format!("/aggregations/{agg_name}/buckets"))
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default()
    }
}

/// Build a `reverse_nested` aggregation (used for tree node counts).
#[allow(dead_code)]
pub struct ReverseNestedAggBuilder;

impl AggBuilder for ReverseNestedAggBuilder {
    fn build(&self, agg_name: &str) -> Value {
        json!({ agg_name: { "reverse_nested": {} } })
    }

    fn extract(&self, resp: &Value, agg_name: &str) -> RawBuckets {
        resp.pointer(&format!("/aggregations/{agg_name}"))
            .cloned()
            .into_iter()
            .collect()
    }
}

/// Compose two `AggBuilder`s: parent builds outer agg; inner is nested within each bucket.
///
/// Used for patterns like: x-axis histogram → y-axis stats within each x bucket.
#[allow(dead_code)]
pub struct CompositeAggBuilder<'a> {
    pub outer: &'a dyn AggBuilder,
    pub inner: &'a dyn AggBuilder,
    pub inner_name: String,
}

impl<'a> AggBuilder for CompositeAggBuilder<'a> {
    fn build(&self, agg_name: &str) -> Value {
        let mut outer = self.outer.build(agg_name);
        let inner_agg = self.inner.build(&self.inner_name);

        // Recursively inject inner agg into outer's nested structure
        self.inject_inner_agg(&mut outer, agg_name, &inner_agg);
        outer
    }

    fn extract(&self, resp: &Value, agg_name: &str) -> RawBuckets {
        self.outer.extract(resp, agg_name)
    }
}

impl<'a> CompositeAggBuilder<'a> {
    /// Recursively inject inner aggregation into nested structures.
    /// Handles both direct aggregations and nested attribute aggregations.
    #[allow(dead_code)]
    fn inject_inner_agg(&self, outer: &mut Value, agg_name: &str, inner_agg: &Value) {
        if let Some(outer_obj) = outer.get_mut(agg_name) {
            // First try direct injection (for simple histogram, terms, etc.)
            for key in &["histogram", "date_histogram", "terms", "geohash_grid"] {
                if outer_obj.get(key).is_some() {
                    outer_obj["aggs"] = inner_agg.clone();
                    return;
                }
            }

            // If not found directly, look inside nested aggregations
            if outer_obj.get("nested").is_some() {
                if let Some(nested_aggs) = outer_obj.get_mut("aggs") {
                    let filter_agg_opt = if nested_aggs.get("by_key").is_some() {
                        nested_aggs.get_mut("by_key")
                    } else {
                        nested_aggs.get_mut("by_value")
                    };

                    if let Some(filter_agg) = filter_agg_opt {
                        if let Some(inner_aggs) = filter_agg.get_mut("aggs") {
                            for key in &["histogram", "date_histogram", "terms", "geohash_grid"] {
                                if inner_aggs.get(key).is_some() {
                                    if self.inner_name == "cat_agg" {
                                        if let Some(agg_def) = inner_aggs.get_mut(key) {
                                            if agg_def.get("aggs").is_none() {
                                                agg_def["aggs"] = json!({});
                                            }
                                            if let Some(cat_agg_inner) = inner_agg.get("cat_agg") {
                                                agg_def["aggs"]["cat_agg"] = cat_agg_inner.clone();
                                            }
                                        }
                                    } else if let Some(inner_value) =
                                        inner_agg.get(&self.inner_name)
                                    {
                                        inner_aggs[&self.inner_name.clone()] = inner_value.clone();
                                    } else {
                                        inner_aggs[&self.inner_name.clone()] = inner_agg.clone();
                                    }
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Determine if a field is a taxonomic rank.
fn is_rank(field: &str, cache: &Option<Arc<tokio::sync::RwLock<MetadataCache>>>) -> bool {
    if let Some(cache_lock) = cache {
        if let Ok(c) = cache_lock.try_read() {
            return c.taxonomic_ranks.contains(&field.to_string());
        }
    }
    false
}

/// Determine if a field is an attribute.
fn is_attribute(field: &str, cache: &Option<Arc<tokio::sync::RwLock<MetadataCache>>>) -> bool {
    if let Some(cache_lock) = cache {
        if let Ok(c) = cache_lock.try_read() {
            // If the field is a recognised taxonomic rank, prefer that
            // interpretation and do not treat it as a nested attribute.
            if c.taxonomic_ranks.contains(&field.to_string()) {
                return false;
            }
            if let Value::Object(groups) = &c.attr_types {
                for (_, group) in groups {
                    if let Value::Object(fields) = group {
                        if fields.contains_key(field) {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Get the exact value field for an attribute from metadata.
/// Returns the processed_summary field (e.g., "attributes.long_value" for type=long).
/// This MUST come from metadata, not guessed.
fn get_attribute_value_field(
    field: &str,
    cache: &Option<Arc<tokio::sync::RwLock<MetadataCache>>>,
) -> Result<String, String> {
    dbg!(&field);
    if let Some(cache_lock) = cache {
        if let Ok(c) = cache_lock.try_read() {
            if let Value::Object(groups) = &c.attr_types {
                // Search all groups for this field
                for (_, group) in groups {
                    if let Value::Object(fields) = group {
                        if let Some(Value::Object(meta_obj)) = fields.get(field) {
                            // Get processed_summary which is the exact ES field name
                            if let Some(ps) =
                                meta_obj.get("processed_summary").and_then(|v| v.as_str())
                            {
                                return Ok(format!("attributes.{}", ps));
                            }
                        }
                    }
                }
            }
        }
    }
    Err(format!("field '{}' not found in metadata", field))
}

/// Return the ES aggregation type name (and matching agg name) for an x-axis value type.
///
/// The agg is always named the same as its type so extraction paths are predictable:
/// `…/by_key/{x_bucket_agg_name}/buckets`.
pub fn x_bucket_agg_name(value_type: ValueType) -> &'static str {
    match value_type {
        ValueType::Keyword | ValueType::TaxonRank => "terms",
        ValueType::Date => "date_histogram",
        _ => "histogram",
    }
}

/// Build the ES aggregation params object for an x-axis field.
///
/// Returns `(agg_type, params)` where `agg_type` matches [`x_bucket_agg_name`] and
/// `params` is the body placed inside `{ agg_type: params }` in the ES query.
fn build_x_agg_params(
    x_spec: &AxisSpec,
    x_value_field: &str,
    x_bounds: &BoundsResult,
) -> (&'static str, Value) {
    let agg_type = x_bucket_agg_name(x_spec.value_type);
    let params = match x_spec.value_type {
        ValueType::Keyword | ValueType::TaxonRank => {
            let mut t = json!({
                "field": x_value_field,
                "size": x_spec.opts.size,
                "min_doc_count": 0
            });
            if !x_bounds.fixed_terms.is_empty() {
                t["include"] = json!(x_bounds.fixed_terms);
            }
            t
        }
        ValueType::Date => {
            // For date histograms use calendar_interval.
            let calendar_interval = x_bounds
                .interval
                .map(|i| i.to_es_interval().to_string())
                .unwrap_or_else(|| "1y".to_string());
            let mut params = json!({
                "field": x_value_field,
                "calendar_interval": calendar_interval,
                "min_doc_count": 0
            });
            if let Some(domain_arr) = x_bounds.domain {
                params["extended_bounds"] = json!({ "min": domain_arr[0], "max": domain_arr[1] });
            }
            params
        }
        _ => {
            let [domain_min, domain_max] = x_bounds.domain.unwrap_or([0.0, 1.0]);
            let (hist_min, hist_max, script_opt) = match x_spec.opts.scale {
                Scale::Log | Scale::Log10 => {
                    let mn = domain_min.max(1.0).log10();
                    let mx = domain_max.max(1.0).log10();
                    (mn, mx, Some("Math.log10(_value)".to_string()))
                }
                Scale::Log2 => {
                    let mn = domain_min.max(1.0).log2();
                    let mx = domain_max.max(1.0).log2();
                    (
                        mn,
                        mx,
                        Some("Math.max(Math.log(_value)/Math.log(2), 0)".to_string()),
                    )
                }
                Scale::Sqrt => (domain_min.max(0.0).sqrt(), domain_max.sqrt(), None),
                _ => (domain_min, domain_max, None),
            };
            let interval = (hist_max - hist_min) / x_bounds.tick_count.max(1) as f64;
            let mut h = json!({
                "field": x_value_field,
                "interval": interval,
                "extended_bounds": { "min": hist_min, "max": hist_max },
                "offset": hist_min,
                "min_doc_count": 0
            });
            if let Some(script) = script_opt {
                h["script"] = Value::String(script);
            }
            h
        }
    };
    (agg_type, params)
}

#[allow(clippy::too_many_arguments)]
/// Build a Y-axis sub-aggregation that adapts to the Y value type.
/// For numeric values this produces a histogram, for date a date_histogram,
/// and for keyword/taxon-rank a `terms` (named `top_terms`) aggregation.
fn build_y_sub_agg(
    y_field: &str,
    y_value_field: &str,
    y_value_type: ValueType,
    y_scale: Scale,
    y_bounds_min: f64,
    y_bounds_max: f64,
    y_ticks: usize,
    y_interval: Option<DateInterval>,
) -> Value {
    match y_value_type {
        ValueType::TaxonRank => {
            // For taxon ranks, aggregate within the `lineage` nested path and
            // filter ancestors by the requested rank (e.g., "genus"), then
            // terms-aggregate on `lineage.taxon_id` (or configured y_value_field).
            let mut y_field_agg = serde_json::Map::new();
            y_field_agg.insert(
                y_field.to_string(),
                json!({
                    "filter": { "term": { "lineage.taxon_rank": y_field } },
                    "aggs": {
                        "top_terms": {
                            "terms": {
                                "field": y_value_field,
                                "size": y_ticks,
                                "min_doc_count": 0
                            }
                        }
                    }
                }),
            );
            json!({
                "reverse_nested": {},
                "aggs": {
                    "by_attribute": {
                        "nested": { "path": "lineage" },
                        "aggs": Value::Object(y_field_agg)
                    }
                }
            })
        }
        ValueType::Keyword => {
            // terms agg named `top_terms` inside the `attributes` nested path
            let mut y_field_agg = serde_json::Map::new();
            y_field_agg.insert(
                y_field.to_string(),
                json!({
                    "filter": { "term": { "attributes.key": y_field } },
                    "aggs": {
                        "top_terms": {
                            "terms": {
                                "field": y_value_field,
                                "size": y_ticks,
                                "min_doc_count": 0
                            }
                        }
                    }
                }),
            );
            json!({
                "reverse_nested": {},
                "aggs": {
                    "by_attribute": {
                        "nested": { "path": "attributes" },
                        "aggs": Value::Object(y_field_agg)
                    }
                }
            })
        }
        ValueType::Date => {
            // date_histogram using calendar_interval derived from bounds tick_count or provided interval
            let calendar_interval = y_interval
                .map(|i| i.to_es_interval().to_string())
                .unwrap_or_else(|| "1y".to_string());

            let mut date_hist_params = json!({
                "field": y_value_field,
                "calendar_interval": calendar_interval,
                "min_doc_count": 0
            });
            // Ensure buckets for empty intervals cover the full domain
            date_hist_params["extended_bounds"] =
                json!({ "min": y_bounds_min, "max": y_bounds_max });

            let mut y_field_agg = serde_json::Map::new();
            y_field_agg.insert(
                y_field.to_string(),
                json!({
                    "filter": { "term": { "attributes.key": y_field } },
                    "aggs": {
                        "date_histogram": { "date_histogram": date_hist_params }
                    }
                }),
            );
            json!({
                "reverse_nested": {},
                "aggs": {
                    "by_attribute": {
                        "nested": { "path": "attributes" },
                        "aggs": Value::Object(y_field_agg)
                    }
                }
            })
        }
        _ => {
            // Numeric histogram path (existing behaviour)
            let (hist_min, hist_max, script_opt) = match y_scale {
                Scale::Log | Scale::Log10 => {
                    let mn = y_bounds_min.max(1.0).log10();
                    let mx = y_bounds_max.max(1.0).log10();
                    (mn, mx, Some("Math.log10(_value)".to_string()))
                }
                Scale::Log2 => {
                    let mn = y_bounds_min.max(1.0).log2();
                    let mx = y_bounds_max.max(1.0).log2();
                    (
                        mn,
                        mx,
                        Some("Math.max(Math.log(_value)/Math.log(2), 0)".to_string()),
                    )
                }
                Scale::Sqrt => {
                    let mn = y_bounds_min.max(0.0).sqrt();
                    let mx = y_bounds_max.sqrt();
                    (mn, mx, None)
                }
                _ => (y_bounds_min, y_bounds_max, None),
            };

            let interval = (hist_max - hist_min) / y_ticks.max(1) as f64;

            let mut hist_params = json!({
                "field": y_value_field,
                "interval": interval,
                "extended_bounds": { "min": hist_min, "max": hist_max },
                "offset": hist_min,
                "min_doc_count": 0
            });
            if let Some(script) = script_opt {
                hist_params["script"] = Value::String(script);
            }

            let mut y_field_agg = serde_json::Map::new();
            y_field_agg.insert(
                y_field.to_string(),
                json!({
                    "filter": { "term": { "attributes.key": y_field } },
                    "aggs": { "histogram": { "histogram": hist_params } }
                }),
            );

            json!({
                "reverse_nested": {},
                "aggs": {
                    "by_attribute": {
                        "nested": { "path": "attributes" },
                        "aggs": Value::Object(y_field_agg)
                    }
                }
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// Build a complete scatter (2-axis) aggregation for nested attributes, optionally with categories.
///
/// Supports any x-axis value type: numeric fields use `histogram`, keyword/rank fields use
/// `terms`. The agg type name is used consistently as both the agg name and type so that
/// extraction paths remain predictable (`…/by_key/{x_bucket_agg_name}/buckets`).
///
/// # Aggregation structure (numeric x)
/// ```text
/// {agg_name}: nested(attributes)
///   by_key: filter(x_field)
///     histogram (or terms for keyword x)
///       aggs:
///         yHistograms: reverse_nested → nested → y_field filter → y histogram
///     categoryHistograms (when cat_labels non-empty): reverse_nested
///       … (same x-agg type used per-category)
/// ```
pub fn build_nested_attribute_scatter_agg(
    agg_name: &str,
    x_spec: &AxisSpec,
    x_bounds: &BoundsResult,
    y_field: &str,
    y_bounds: &BoundsResult,
    y_scale: Scale,
    cat_field: Option<&str>,
    cat_value_type: Option<ValueType>,
    cat_bounds: Option<&BoundsResult>,
    cat_labels: &[String],
    show_other: bool,
    cache: &Option<Arc<tokio::sync::RwLock<MetadataCache>>>,
) -> Result<Value, String> {
    let x_field = x_spec.field.as_str();
    let is_x_rank = is_rank(x_field, cache);
    let x_value_field = if is_x_rank {
        "lineage.taxon_id".to_string()
    } else {
        get_attribute_value_field(x_field, cache)?
    };
    let is_y_rank = is_rank(y_field, cache);
    let y_value_field = if is_y_rank {
        "lineage.taxon_id".to_string()
    } else {
        get_attribute_value_field(y_field, cache)?
    };
    let (x_agg_type, x_agg_params) = build_x_agg_params(x_spec, &x_value_field, x_bounds);
    let [y_domain_min, y_domain_max] = y_bounds.domain.unwrap_or([0.0, 1.0]);
    let y_histogram_sub_agg = build_y_sub_agg(
        y_field,
        &y_value_field,
        y_bounds.value_type,
        y_scale,
        y_domain_min,
        y_domain_max,
        y_bounds.tick_count,
        y_bounds.interval,
    );

    // Main x agg with nested y-histograms.
    // Structure: { x_agg_type: x_agg_params, "aggs": { yHistograms: … } }
    let x_with_y = json!({
        x_agg_type: x_agg_params.clone(),
        "aggs": { "yHistograms": y_histogram_sub_agg.clone() }
    });

    // Category histograms (optional).
    let category_histograms_opt = if let Some(cat) = cat_field {
        let cat_value_field = get_attribute_value_field(cat, cache)?;
        let cat_vt = cat_value_type.unwrap_or(ValueType::Keyword);
        let is_numeric_cat = !matches!(cat_vt, ValueType::Keyword | ValueType::TaxonRank);

        // Skip only when keyword cat has no known labels.
        if cat_labels.is_empty() && !is_numeric_cat {
            None
        } else {
            let default_bounds = cat_bounds.unwrap_or(x_bounds);
            let (by_value_agg_type, by_value_def) = build_by_value_agg(
                cat_vt,
                &cat_value_field,
                default_bounds,
                cat_labels,
                show_other,
            );

            // Per-cat x agg: same type as main x, with y-histograms nested inside.
            let mut cat_x_field_agg = serde_json::Map::new();
            cat_x_field_agg.insert(
                x_field.to_string(),
                json!({
                    "filter": { "term": { "attributes.key": x_field } },
                    "aggs": { x_agg_type: x_with_y.clone() }
                }),
            );

            Some(json!({
                "reverse_nested": {},
                "aggs": {
                    "by_attribute": {
                        "nested": { "path": "attributes" },
                        "aggs": {
                            "by_cat": {
                                "filter": { "term": { "attributes.key": cat } },
                                "aggs": {
                                    "by_value": {
                                        by_value_agg_type: by_value_def,
                                        "aggs": {
                                            "histogram": {
                                                "reverse_nested": {},
                                                "aggs": {
                                                    "by_attribute": {
                                                        "nested": { "path": "attributes" },
                                                        "aggs": Value::Object(cat_x_field_agg)
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
            }))
        }
    } else {
        None
    };

    let mut by_key_aggs = json!({ x_agg_type: x_with_y });
    if let Some(cat_hist) = category_histograms_opt {
        by_key_aggs["categoryHistograms"] = cat_hist;
    }

    if is_x_rank {
        Ok(json!({
            agg_name: {
                "nested": { "path": "lineage" },
                "aggs": {
                    "by_key": {
                        "filter": { "term": { "lineage.taxon_rank": x_field } },
                        "aggs": by_key_aggs
                    }
                }
            }
        }))
    } else {
        Ok(json!({
            agg_name: {
                "nested": { "path": "attributes" },
                "aggs": {
                    "by_key": {
                        "filter": { "term": { "attributes.key": x_field } },
                        "aggs": by_key_aggs
                    }
                }
            }
        }))
    }
}

/// Build the `by_value` aggregation used for per-category sub-histograms.
///
/// Returns `(agg_type_key, agg_params)` for embedding in the `by_value` ES object:
/// - Keyword/rank fields use `filters` (named buckets, one per known label).
/// - Numeric fields use `histogram` (array buckets from bounds domain/interval).
fn build_by_value_agg(
    cat_value_type: ValueType,
    cat_value_field: &str,
    cat_bounds: &BoundsResult,
    cat_labels: &[String],
    show_other: bool,
) -> (&'static str, Value) {
    match cat_value_type {
        ValueType::Keyword | ValueType::TaxonRank => {
            let mut filters = serde_json::Map::new();
            for label in cat_labels {
                filters.insert(label.clone(), json!({ "term": { cat_value_field: label } }));
            }
            let mut def = json!({ "filters": Value::Object(filters) });
            if show_other {
                def["other_bucket_key"] = json!("other");
            }
            ("filters", def)
        }
        ValueType::Date => {
            let calendar_interval = cat_bounds
                .interval
                .map(|i| i.to_es_interval().to_string())
                .unwrap_or_else(|| "1y".to_string());
            (
                "date_histogram",
                json!({
                    "field": cat_value_field,
                    "calendar_interval": calendar_interval,
                    "min_doc_count": 0
                }),
            )
        }
        _ => {
            let [domain_min, domain_max] = cat_bounds.domain.unwrap_or([0.0, 1.0]);
            let ticks = cat_bounds.tick_count.max(1) as f64;
            let interval = ((domain_max - domain_min) / ticks).max(f64::EPSILON);
            (
                "histogram",
                json!({
                    "field": cat_value_field,
                    "interval": interval,
                    "extended_bounds": { "min": domain_min, "max": domain_max },
                    "offset": domain_min,
                    "min_doc_count": 0
                }),
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// Build a complete nested-attribute histogram aggregation with per-category sub-histograms.
///
/// Supports any x-axis value type: numeric fields use `histogram`, keyword/rank fields use
/// `terms`. The cat axis is always filtered by term (keyword/rank); pass a keyword or rank
/// field for `cat_field`.
///
/// Supports any cat-axis value type: keyword/rank fields use named `filters` (one per label),
/// numeric fields use a `histogram` agg bucketed by the cat domain.
///
/// # Aggregation structure
/// ```text
/// {agg_name}: nested(attributes)
///   by_key: filter(x_field)
///     {x_agg_type}: histogram or terms (main x-axis counts)
///     categoryHistograms: reverse_nested
///       by_attribute: nested(attributes)
///         by_cat: filter(cat_field)
///           by_value: filters (keyword) or histogram (numeric)
///             histogram: reverse_nested
///               by_attribute: nested(attributes)
///                 {x_field}: filter(x_field)
///                   {x_agg_type}: histogram or terms (per-category counts)
/// ```
pub fn build_nested_attribute_histogram_with_categories(
    agg_name: &str,
    x_spec: &AxisSpec,
    x_bounds: &BoundsResult,
    cat_field: &str,
    cat_value_type: ValueType,
    cat_bounds: &BoundsResult,
    cat_labels: &[String],
    show_other: bool,
    cache: &Option<Arc<tokio::sync::RwLock<MetadataCache>>>,
) -> Result<Value, String> {
    let x_field = x_spec.field.as_str();
    let is_x_rank = is_rank(x_field, cache);
    let x_value_field = if is_x_rank {
        "lineage.taxon_id".to_string()
    } else {
        get_attribute_value_field(x_field, cache)?
    };
    let is_cat_rank = is_rank(cat_field, cache);
    let cat_value_field = if is_cat_rank {
        "lineage.taxon_id".to_string()
    } else {
        get_attribute_value_field(cat_field, cache)?
    };

    let (x_agg_type, x_agg_params) = build_x_agg_params(x_spec, &x_value_field, x_bounds);
    let (by_value_agg_type, by_value_def) = build_by_value_agg(
        cat_value_type,
        &cat_value_field,
        cat_bounds,
        cat_labels,
        show_other,
    );

    // Build the per-category histogram payload. There are two axes of variation:
    // - whether the category axis is an attribute (attributes path) or a
    //   taxonomic rank (lineage path);
    // - whether the x-axis itself is a rank (lineage) or an attribute/root
    //   field. We generate a `categoryHistograms` block that uses the correct
    // nested path for the category values and, inside each category bucket,
    // computes the per-category x-aggregation using the appropriate nested
    // path for `x`.
    let category_histograms = if is_cat_rank {
        // Category values live under `lineage` (taxon ids). Build a lineage-anchored
        // category histogram, then for each category value compute the x histogram
        // by reversing to the document root and nesting into the appropriate path
        // for `x`.
        let x_hist_inner = if is_x_rank {
            // x is a taxon rank: compute x agg in a lineage nested path at the rank
            json!({
                "by_lineage": {
                    "nested": { "path": "lineage" },
                    "aggs": {
                        "at_rank": {
                            "filter": { "term": { "lineage.taxon_rank": x_field } },
                            "aggs": { x_agg_type: { x_agg_type: x_agg_params.clone() } }
                        }
                    }
                }
            })
        } else {
            // x is an attribute (or other non-lineage field): compute x agg under attributes
            let mut x_field_agg = serde_json::Map::new();
            x_field_agg.insert(
                x_field.to_string(),
                json!({
                    "filter": { "term": { "attributes.key": x_field } },
                    "aggs": { x_agg_type: { x_agg_type: x_agg_params.clone() } }
                }),
            );
            json!({ "by_attribute": { "nested": { "path": "attributes" }, "aggs": Value::Object(x_field_agg) } })
        };

        json!({
            "reverse_nested": {},
            "aggs": {
                "by_lineage": {
                    "nested": { "path": "lineage" },
                    "aggs": {
                        "at_cat_rank": {
                            "filter": { "term": { "lineage.taxon_rank": cat_field } },
                            "aggs": {
                                "by_value": {
                                    by_value_agg_type: by_value_def,
                                    "aggs": {
                                        "histogram": {
                                            "reverse_nested": {},
                                            "aggs": x_hist_inner
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
    } else if is_x_rank {
        // Category values are attributes; x is a rank. Keep the existing
        // attribute->by_cat arrangement but compute x inside a lineage nested
        // path per-category.
        json!({
            "reverse_nested": {},
            "aggs": {
                "by_attribute": {
                    "nested": { "path": "attributes" },
                    "aggs": {
                        "by_cat": {
                            "filter": { "term": { "attributes.key": cat_field } },
                            "aggs": {
                                "by_value": {
                                    by_value_agg_type: by_value_def,
                                    "aggs": {
                                        "histogram": {
                                            "reverse_nested": {},
                                            "aggs": {
                                                "by_lineage": {
                                                    "nested": { "path": "lineage" },
                                                    "aggs": {
                                                        "at_rank": {
                                                            "filter": { "term": { "lineage.taxon_rank": x_field } },
                                                            "aggs": { x_agg_type: { x_agg_type: x_agg_params.clone() } }
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
        })
    } else {
        // Both x and cat are attributes (legacy path): attribute->by_cat->by_value,
        // then compute x under attributes inside each category.
        let mut x_field_agg = serde_json::Map::new();
        x_field_agg.insert(
            x_field.to_string(),
            json!({
                "filter": { "term": { "attributes.key": x_field } },
                "aggs": { x_agg_type: { x_agg_type: x_agg_params.clone() } }
            }),
        );

        json!({
            "reverse_nested": {},
            "aggs": {
                "by_attribute": {
                    "nested": { "path": "attributes" },
                    "aggs": {
                        "by_cat": {
                            "filter": { "term": { "attributes.key": cat_field } },
                            "aggs": {
                                "by_value": {
                                    by_value_agg_type: by_value_def,
                                    "aggs": {
                                        "histogram": {
                                            "reverse_nested": {},
                                            "aggs": {
                                                "by_attribute": {
                                                    "nested": { "path": "attributes" },
                                                    "aggs": Value::Object(x_field_agg)
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
        })
    };

    // Start from the canonical x-aggregation built by `agg_builder_for()` so
    // extraction paths and naming conventions remain consistent.
    let x_agg_builder = agg_builder_for(x_spec, x_bounds, cache)?;
    let mut final_agg = x_agg_builder.build(agg_name);

    // Inject `categoryHistograms` into the appropriate inner `aggs` map.
    if let Some(root) = final_agg.get_mut(agg_name) {
        if let Some(aggs_obj) = root.get_mut("aggs") {
            // Try `by_key` (attributes path) then `at_rank` (lineage path)
            if let Some(by_key) = aggs_obj.get_mut("by_key") {
                if let Some(inner_aggs) = by_key.get_mut("aggs") {
                    inner_aggs["categoryHistograms"] = category_histograms;
                }
            } else if let Some(at_rank) = aggs_obj.get_mut("at_rank") {
                if let Some(inner_aggs) = at_rank.get_mut("aggs") {
                    inner_aggs["categoryHistograms"] = category_histograms;
                }
            }
        }
    }

    Ok(final_agg)
}

/// Select the appropriate `AggBuilder` for an axis spec.
///
/// This is the main factory function; report handlers call it rather than
/// constructing builders directly.
pub fn agg_builder_for(
    spec: &AxisSpec,
    bounds: &BoundsResult,
    cache: &Option<Arc<tokio::sync::RwLock<MetadataCache>>>,
) -> Result<Box<dyn AggBuilder>, String> {
    let is_attr = is_attribute(&spec.field, cache);
    let is_rk = is_rank(&spec.field, cache);
    dbg!(&is_attr);
    dbg!(&is_rk);

    match spec.value_type {
        ValueType::Numeric => {
            let [domain_min, domain_max] = bounds.domain.unwrap_or([0.0, 1.0]);

            // For log scales, transform bounds to log space for histogram interval calculation
            let (hist_min, hist_max) = match spec.opts.scale {
                Scale::Log | Scale::Log10 => {
                    let min_val = domain_min.max(1.0).log10();
                    let max_val = domain_max.max(1.0).log10();
                    (min_val, max_val)
                }
                Scale::Log2 => {
                    let min_val = domain_min.max(1.0).log2();
                    let max_val = domain_max.max(1.0).log2();
                    (min_val, max_val)
                }
                Scale::Sqrt => {
                    let min_val = domain_min.max(0.0).sqrt();
                    let max_val = domain_max.sqrt();
                    (min_val, max_val)
                }
                _ => (domain_min, domain_max),
            };

            // Compute interval in transformed space
            let ticks = bounds.tick_count.max(1) as f64;
            let interval = (hist_max - hist_min) / ticks;

            if is_attr {
                let value_field = get_attribute_value_field(&spec.field, cache)?;

                // Build script transform for log scales
                let script_opt = match spec.opts.scale {
                    Scale::Log10 => Some("Math.log10(_value)".to_string()),
                    Scale::Log => Some("Math.log(_value)".to_string()),
                    Scale::Log2 => Some("Math.max(Math.log(_value)/Math.log(2), 0)".to_string()),
                    Scale::Sqrt => Some("Math.sqrt(_value)".to_string()),
                    _ => None,
                };

                let mut inner_agg = json!({
                    "histogram": {
                        "field": &value_field,
                        "interval": interval,
                        "extended_bounds": { "min": hist_min, "max": hist_max },
                        "min_doc_count": 0
                    }
                });

                if let Some(script) = script_opt {
                    inner_agg["histogram"]["script"] = Value::String(script);
                }

                Ok(Box::new(NestedAttributeAggBuilder {
                    field: spec.field.clone(),
                    inner_agg_body: inner_agg,
                    inner_agg_name: "histogram".to_string(),
                }))
            } else {
                let script_opt = match spec.opts.scale {
                    Scale::Log10 => Some("Math.log10(_value)".to_string()),
                    Scale::Log => Some("Math.log(_value)".to_string()),
                    Scale::Log2 => Some("Math.max(Math.log(_value)/Math.log(2), 0)".to_string()),
                    Scale::Sqrt => Some("Math.sqrt(_value)".to_string()),
                    _ => None,
                };

                Ok(Box::new(HistogramAggBuilder {
                    field: spec.field.clone(),
                    interval,
                    min: hist_min,
                    max: hist_max,
                    script: script_opt,
                }))
            }
        }
        ValueType::Date => {
            let calendar_interval = bounds
                .interval
                .map(|i| i.to_es_interval().to_string())
                .unwrap_or_else(|| "1y".to_string());

            if is_attr {
                let value_field = get_attribute_value_field(&spec.field, cache)?;
                let inner_agg = json!({
                    "date_histogram": {
                        "field": &value_field,
                        "calendar_interval": &calendar_interval,
                        "min_doc_count": 0
                    }
                });
                Ok(Box::new(NestedAttributeAggBuilder {
                    field: spec.field.clone(),
                    inner_agg_body: inner_agg,
                    inner_agg_name: "date_histogram".to_string(),
                }))
            } else {
                Ok(Box::new(DateHistogramAggBuilder {
                    field: spec.field.clone(),
                    calendar_interval,
                    time_zone: None,
                }))
            }
        }
        ValueType::Keyword | ValueType::TaxonRank => {
            // Prefer taxon-rank handling when the field is recognised as a rank.
            // This avoids attempting attribute lookups on rank names that are
            // present in `taxonomic_ranks` but not in `attr_types`.

            dbg!(&is_attr);
            dbg!(&is_rk);
            if is_rk {
                let inner_agg = json!({
                    "terms": {
                        "field": "lineage.taxon_id",
                        "size": spec.opts.size,
                        "min_doc_count": 0
                    }
                });
                Ok(Box::new(NestedRankAggBuilder {
                    field: spec.field.clone(),
                    inner_agg_body: inner_agg,
                    inner_agg_name: "terms".to_string(),
                }))
            } else if is_attr {
                let value_field = get_attribute_value_field(&spec.field, cache)?;
                let inner_agg = json!({
                    "terms": {
                        "field": &value_field,
                        "size": spec.opts.size,
                        "min_doc_count": 0
                    }
                });
                Ok(Box::new(NestedAttributeAggBuilder {
                    field: spec.field.clone(),
                    inner_agg_body: inner_agg,
                    inner_agg_name: "terms".to_string(),
                }))
            } else {
                Ok(Box::new(TermsAggBuilder {
                    field: spec.field.clone(),
                    size: spec.opts.size,
                    include: if bounds.fixed_terms.is_empty() {
                        None
                    } else {
                        Some(bounds.fixed_terms.clone())
                    },
                }))
            }
        }
        ValueType::GeoPoint => Ok(Box::new(GeoHashAggBuilder {
            field: spec.field.clone(),
            precision: geohash_precision_for_size(spec.opts.size),
            size: spec.opts.size,
        })),
    }
}

/// Wrapper that adds nested query logic around a base aggregation for nested attributes.
pub struct NestedAttributeAggBuilder {
    pub field: String,
    pub inner_agg_body: Value,
    pub inner_agg_name: String,
}

impl AggBuilder for NestedAttributeAggBuilder {
    fn build(&self, agg_name: &str) -> Value {
        json!({
            agg_name: {
                "nested": { "path": "attributes" },
                "aggs": {
                    "by_key": {
                        "filter": { "term": { "attributes.key": &self.field } },
                        "aggs": {
                            &self.inner_agg_name: self.inner_agg_body.clone()
                        }
                    }
                }
            }
        })
    }

    fn extract(&self, resp: &Value, agg_name: &str) -> RawBuckets {
        resp.pointer(&format!(
            "/aggregations/{}/by_key/{}/buckets",
            agg_name, self.inner_agg_name
        ))
        .and_then(|b| b.as_array())
        .cloned()
        .unwrap_or_default()
    }
}

/// Wrapper that adds nested query logic around a base aggregation for nested rank (lineage) fields.
pub struct NestedRankAggBuilder {
    pub field: String,
    pub inner_agg_body: Value,
    pub inner_agg_name: String,
}

impl AggBuilder for NestedRankAggBuilder {
    fn build(&self, agg_name: &str) -> Value {
        json!({
            agg_name: {
                "nested": { "path": "lineage" },
                "aggs": {
                    "at_rank": {
                        "filter": { "term": { "lineage.taxon_rank": &self.field } },
                        "aggs": {
                            &self.inner_agg_name: self.inner_agg_body.clone()
                        }
                    }
                }
            }
        })
    }

    fn extract(&self, resp: &Value, agg_name: &str) -> RawBuckets {
        resp.pointer(&format!(
            "/aggregations/{}/at_rank/{}/buckets",
            agg_name, self.inner_agg_name
        ))
        .and_then(|b| b.as_array())
        .cloned()
        .unwrap_or_default()
    }
}

/// Map a requested geohash count to an ES geohash precision level (1–12).
fn geohash_precision_for_size(size: usize) -> u8 {
    match size {
        0..=50 => 3,
        51..=200 => 4,
        201..=1000 => 5,
        _ => 6,
    }
}
