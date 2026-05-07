//! Elasticsearch aggregation builders for report axes.
//!
//! Each `AggBuilder` produces the JSON fragment for one ES aggregation, and extracts
//! the bucket list from the response. Builders are composable: use `CompositeAggBuilder`
//! to nest them (e.g., histogram containing stats).

use serde_json::{json, Value};

use crate::es_metadata::MetadataCache;
use genomehubs_query::report::axis::{Scale, ValueType};
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
    pub scale: Scale,
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
    if let Some(cache_lock) = cache {
        if let Ok(c) = cache_lock.try_read() {
            if let Value::Object(groups) = &c.attr_types {
                // Search all groups for this field
                for (_, group) in groups {
                    if let Value::Object(fields) = group {
                        if let Some(field_meta) = fields.get(field) {
                            if let Value::Object(meta_obj) = field_meta {
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
    }
    Err(format!("field '{}' not found in metadata", field))
}

/// Build the `yHistograms` sub-aggregation used inside each x-histogram bucket.
///
/// Escapes nested context via `reverse_nested`, re-enters attributes, and runs a histogram
/// on the y-field value. Supports log/sqrt scale transforms via ES script.
///
/// ```text
/// yHistograms: reverse_nested
///   by_attribute: nested(attributes)
///     {y_field}: filter(y_field)
///       histogram: histogram(y_value_field)
/// ```
fn build_y_histogram_sub_agg(
    y_field: &str,
    y_value_field: &str,
    y_scale: Scale,
    y_domain_min: f64,
    y_domain_max: f64,
    y_ticks: usize,
) -> Value {
    let (hist_min, hist_max, script_opt) = match y_scale {
        Scale::Log | Scale::Log10 => {
            let mn = y_domain_min.max(1.0).log10();
            let mx = y_domain_max.max(1.0).log10();
            (mn, mx, Some("Math.log10(_value)".to_string()))
        }
        Scale::Log2 => {
            let mn = y_domain_min.max(1.0).log2();
            let mx = y_domain_max.max(1.0).log2();
            (
                mn,
                mx,
                Some("Math.max(Math.log(_value)/Math.log(2), 0)".to_string()),
            )
        }
        Scale::Sqrt => {
            let mn = y_domain_min.max(0.0).sqrt();
            let mx = y_domain_max.sqrt();
            (mn, mx, None)
        }
        _ => (y_domain_min, y_domain_max, None),
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

/// Build a complete scatter (2-axis) aggregation for nested attributes, optionally with categories.
///
/// Extends the `categoryHistograms` pattern from `build_nested_attribute_histogram_with_categories`
/// to add per-bucket y-histograms (`yHistograms`) nested inside both the main x-histogram and each
/// per-category x-histogram. This produces all the data needed for the scatter or 3-way binned plot.
///
/// # Aggregation structure
/// ```text
/// {agg_name}: nested(attributes)
///   by_key: filter(x_field)
///     histogram (x-axis)
///       aggs:
///         yHistograms: reverse_nested → nested → y_field filter → y histogram
///     categoryHistograms (when cat_labels non-empty): reverse_nested
///       by_attribute: nested(attributes)
///         by_cat: filter(cat_field)
///           by_value: filters(one per cat_label)
///             histogram: reverse_nested
///               by_attribute: nested(attributes)
///                 {x_field}: filter(x_field)
///                   histogram (per-cat x-axis)
///                     aggs:
///                       yHistograms: same as above
/// ```
pub fn build_nested_attribute_scatter_agg(
    agg_name: &str,
    x_field: &str,
    x_bounds: &BoundsResult,
    x_scale: Scale,
    y_field: &str,
    y_bounds: &BoundsResult,
    y_scale: Scale,
    cat_field: Option<&str>,
    cat_labels: &[String],
    show_other: bool,
    cache: &Option<Arc<tokio::sync::RwLock<MetadataCache>>>,
) -> Result<Value, String> {
    let x_value_field = get_attribute_value_field(x_field, cache)?;
    let y_value_field = get_attribute_value_field(y_field, cache)?;

    let [x_domain_min, x_domain_max] = x_bounds.domain.unwrap_or([0.0, 1.0]);
    let (x_hist_min, x_hist_max, x_script_opt) = match x_scale {
        Scale::Log | Scale::Log10 => {
            let mn = x_domain_min.max(1.0).log10();
            let mx = x_domain_max.max(1.0).log10();
            (mn, mx, Some("Math.log10(_value)".to_string()))
        }
        Scale::Log2 => {
            let mn = x_domain_min.max(1.0).log2();
            let mx = x_domain_max.max(1.0).log2();
            (
                mn,
                mx,
                Some("Math.max(Math.log(_value)/Math.log(2), 0)".to_string()),
            )
        }
        Scale::Sqrt => (x_domain_min.max(0.0).sqrt(), x_domain_max.sqrt(), None),
        _ => (x_domain_min, x_domain_max, None),
    };
    let x_interval = (x_hist_max - x_hist_min) / x_bounds.tick_count.max(1) as f64;

    let mut x_hist_params = json!({
        "field": &x_value_field,
        "interval": x_interval,
        "extended_bounds": { "min": x_hist_min, "max": x_hist_max },
        "offset": x_hist_min,
        "min_doc_count": 0
    });
    if let Some(script) = x_script_opt {
        x_hist_params["script"] = Value::String(script);
    }

    let [y_domain_min, y_domain_max] = y_bounds.domain.unwrap_or([0.0, 1.0]);
    let y_histogram_sub_agg = build_y_histogram_sub_agg(
        y_field,
        &y_value_field,
        y_scale,
        y_domain_min,
        y_domain_max,
        y_bounds.tick_count,
    );

    // Main x histogram with nested y-histograms.
    let x_histogram_with_y = json!({
        "histogram": x_hist_params.clone(),
        "aggs": { "yHistograms": y_histogram_sub_agg.clone() }
    });

    // Category histograms (optional).
    let category_histograms_opt = if let Some(cat) = cat_field {
        if cat_labels.is_empty() {
            None
        } else {
            let cat_value_field = get_attribute_value_field(cat, cache)?;

            let mut filters = serde_json::Map::new();
            for label in cat_labels {
                filters.insert(
                    label.clone(),
                    json!({ "term": { &cat_value_field: label } }),
                );
            }
            let mut filters_agg = json!({ "filters": Value::Object(filters) });
            if show_other {
                filters_agg["other_bucket_key"] = json!("other");
            }

            let mut cat_x_field_agg = serde_json::Map::new();
            cat_x_field_agg.insert(
                x_field.to_string(),
                json!({
                    "filter": { "term": { "attributes.key": x_field } },
                    "aggs": { "histogram": x_histogram_with_y.clone() }
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
                                        "filters": filters_agg,
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

    let mut by_key_aggs = json!({ "histogram": x_histogram_with_y });
    if let Some(cat_hist) = category_histograms_opt {
        by_key_aggs["categoryHistograms"] = cat_hist;
    }

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

/// Build a complete nested-attribute histogram aggregation with per-category sub-histograms.
///
/// Uses the v2 API's proven `categoryHistograms` pattern: a `filters` aggregation with one
/// explicitly named bucket per known category label. This completely eliminates fake placeholder
/// codes because only categories that exist in the bounds result are ever referenced.
///
/// # Aggregation structure
/// ```text
/// {agg_name}: nested(attributes)
///   by_key: filter(x_field)
///     histogram: histogram (main x-axis bucket counts)
///     categoryHistograms: reverse_nested
///       by_attribute: nested(attributes)
///         by_cat: filter(cat_field)
///           by_value: filters(one per cat_label)
///             histogram: reverse_nested
///               by_attribute: nested(attributes)
///                 {x_field}: filter(x_field)
///                   histogram: histogram (per-category bucket counts)
/// ```
pub fn build_nested_attribute_histogram_with_categories(
    agg_name: &str,
    x_field: &str,
    x_bounds: &BoundsResult,
    cat_field: &str,
    cat_labels: &[String],
    show_other: bool,
    cache: &Option<Arc<tokio::sync::RwLock<MetadataCache>>>,
) -> Result<Value, String> {
    let x_value_field = get_attribute_value_field(x_field, cache)?;
    let cat_value_field = get_attribute_value_field(cat_field, cache)?;

    let [domain_min, domain_max] = x_bounds.domain.unwrap_or([0.0, 1.0]);
    let ticks = x_bounds.tick_count.max(1) as f64;
    let interval = (domain_max - domain_min) / ticks;

    // Build one named filter per known category (no fake codes ever produced).
    let mut filters = serde_json::Map::new();
    for label in cat_labels {
        filters.insert(
            label.clone(),
            json!({ "term": { &cat_value_field: label } }),
        );
    }
    let mut filters_agg = json!({ "filters": Value::Object(filters) });
    if show_other {
        filters_agg["other_bucket_key"] = json!("other");
    }

    // Per-category inner histogram config — same bounds as the main histogram.
    let inner_histogram = json!({
        "histogram": {
            "field": &x_value_field,
            "interval": interval,
            "extended_bounds": { "min": domain_min, "max": domain_max },
            "offset": domain_min,
            "min_doc_count": 0
        }
    });

    // Dynamic key: {x_field} → filter → histogram inside the per-category reverse_nested path.
    let mut x_field_agg = serde_json::Map::new();
    x_field_agg.insert(
        x_field.to_string(),
        json!({
            "filter": { "term": { "attributes.key": x_field } },
            "aggs": { "histogram": inner_histogram }
        }),
    );

    let category_histograms = json!({
        "reverse_nested": {},
        "aggs": {
            "by_attribute": {
                "nested": { "path": "attributes" },
                "aggs": {
                    "by_cat": {
                        "filter": { "term": { "attributes.key": cat_field } },
                        "aggs": {
                            "by_value": {
                                "filters": filters_agg,
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
    });

    Ok(json!({
        agg_name: {
            "nested": { "path": "attributes" },
            "aggs": {
                "by_key": {
                    "filter": { "term": { "attributes.key": x_field } },
                    "aggs": {
                        "histogram": {
                            "histogram": {
                                "field": &x_value_field,
                                "interval": interval,
                                "extended_bounds": { "min": domain_min, "max": domain_max },
                                "offset": domain_min,
                                "min_doc_count": 0
                            }
                        },
                        "categoryHistograms": category_histograms
                    }
                }
            }
        }
    }))
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
                    value_field,
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
                    scale: spec.opts.scale,
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
                    value_field,
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
            if is_attr {
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
                    value_field,
                    inner_agg_body: inner_agg,
                    inner_agg_name: "terms".to_string(),
                }))
            } else if is_rk {
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

/// Compute a histogram bin interval from domain and desired tick count.
///
/// For log scales, the interval is computed in log-space. For other scales,
/// it is a linear division of the domain.
fn compute_histogram_interval(min: f64, max: f64, tick_count: usize, scale: Scale) -> f64 {
    let ticks = tick_count.max(1) as f64;
    match scale {
        Scale::Log | Scale::Log10 => {
            let log_min = min.max(1.0).log10();
            let log_max = max.max(1.0).log10();
            let log_interval = (log_max - log_min) / ticks;
            10_f64.powf(log_interval)
        }
        Scale::Log2 => {
            let log_min = min.max(1.0).log2();
            let log_max = max.max(1.0).log2();
            let log_interval = (log_max - log_min) / ticks;
            2_f64.powf(log_interval)
        }
        Scale::Sqrt => {
            let sqrt_min = min.max(0.0).sqrt();
            let sqrt_max = max.sqrt();
            let sqrt_interval = (sqrt_max - sqrt_min) / ticks;
            sqrt_interval * sqrt_interval
        }
        _ => (max - min) / ticks,
    }
}

/// Wrapper that adds nested query logic around a base aggregation for nested attributes.
pub struct NestedAttributeAggBuilder {
    pub field: String,
    pub value_field: String,
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
