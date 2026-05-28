//! Elasticsearch aggregation builders for report axes.
//!
//! All field-type detection and path logic is centralised in [`super::field`];
//! this module is responsible only for composing valid ES aggregation JSON and
//! extracting buckets from responses.
//!
//! ## Key types
//! - [`AggBuilder`] — trait for all bucket aggregations
//! - [`GenericBucketAgg`] — single implementation that handles attribute/lineage/root fields
//! - [`build_nested_attribute_histogram_with_categories`] — type-agnostic 2-level agg
//! - [`build_nested_attribute_scatter_agg`] — scatter 2-level agg with optional categories

use serde_json::{json, Value};

use crate::es_metadata::MetadataCache;
use crate::report::field::{
    build_inner_x_agg_block, resolve_field_storage, wrap_cat_in_nested, wrap_in_nested,
    FieldStorage,
};
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

// ── GenericBucketAgg ─────────────────────────────────────────────────────────

/// A single `AggBuilder` implementation that covers attribute, lineage and
/// root-level fields by deriving the nested path from [`FieldStorage`].
///
/// Replaces the previous `NestedAttributeAggBuilder`, `NestedRankAggBuilder`,
/// `HistogramAggBuilder` and `TermsAggBuilder` specialisations.  Call
/// [`agg_builder_for`] to obtain a boxed instance.
pub struct GenericBucketAgg {
    /// Where the field lives in the ES document.
    pub storage: FieldStorage,
    /// ES aggregation type: `"terms"`, `"histogram"`, `"date_histogram"`, etc.
    pub bucket_type: String,
    /// Parameters object placed inside `{ bucket_type: params }`.
    pub bucket_params: Value,
}

impl AggBuilder for GenericBucketAgg {
    fn build(&self, agg_name: &str) -> Value {
        // ES requires {agg_name: {agg_type: params}}.  Wrap params in the type first,
        // then wrap the whole named agg in the nested envelope.
        let named_agg =
            json!({ &self.bucket_type: { &self.bucket_type: self.bucket_params.clone() } });
        let container = self.storage.x_container_name();
        let wrapped = wrap_in_nested(&self.storage, container, named_agg);
        json!({ agg_name: wrapped })
    }

    fn extract(&self, resp: &Value, agg_name: &str) -> RawBuckets {
        let path = self.storage.main_bucket_path(agg_name, &self.bucket_type);
        resp.pointer(&path)
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default()
    }
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
    let x_storage = resolve_field_storage(&x_spec.field, x_spec.value_type, cache)?;
    let y_storage = resolve_field_storage(y_field, y_bounds.value_type, cache)?;
    let x_value_field = x_storage.bucket_field().to_string();
    let y_value_field = y_storage.bucket_field().to_string();
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
        let cat_vt = cat_value_type.unwrap_or(ValueType::Keyword);
        let cat_storage = resolve_field_storage(cat, cat_vt, cache)?;
        let is_numeric_cat = !matches!(cat_vt, ValueType::Keyword | ValueType::TaxonRank);

        // Skip only when keyword cat has no known labels.
        if cat_labels.is_empty() && !is_numeric_cat {
            None
        } else {
            let default_bounds = cat_bounds.unwrap_or(x_bounds);
            let (by_value_agg_type, by_value_def) = build_by_value_agg(
                cat_vt,
                cat_storage.bucket_field(),
                default_bounds,
                cat_labels,
                show_other,
            );

            // Per-cat x agg: same type as main x, with y-histograms nested inside.
            // Uses build_inner_x_agg_block so extraction paths remain deterministic.
            // Pass raw x_agg_params; build_inner_x_agg_block wraps them in the
            // required {name: {type: params}} nesting.  yHistograms is provided
            // as sub_aggs so it sits inside the x bucket agg, not alongside it.
            let per_cat_x_with_y = build_inner_x_agg_block(
                &x_storage,
                x_agg_type,
                x_agg_params.clone(),
                Some(json!({ "yHistograms": y_histogram_sub_agg.clone() })),
            );

            let by_value_with_inner = json!({
                "by_value": {
                    by_value_agg_type: by_value_def,
                    "aggs": per_cat_x_with_y
                }
            });
            let cat_aggs = wrap_cat_in_nested(&cat_storage, by_value_with_inner);

            Some(json!({
                "reverse_nested": {},
                "aggs": cat_aggs
            }))
        }
    } else {
        None
    };

    // Build main x agg via generic factory and inject category histograms.
    let x_agg_builder = agg_builder_for(x_spec, x_bounds, cache)?;
    let mut final_agg = x_agg_builder.build(agg_name);

    // Inject yHistograms into the inner agg.
    inject_y_histograms_into_agg(&mut final_agg, agg_name, &x_storage, x_agg_type, x_with_y);

    if let Some(cat_hist) = category_histograms_opt {
        inject_category_histograms(&mut final_agg, agg_name, &x_storage, cat_hist);
    }

    Ok(final_agg)
}

/// Inject `x_with_y` (the x bucket agg including yHistograms sub-agg) into the built x agg.
fn inject_y_histograms_into_agg(
    final_agg: &mut Value,
    agg_name: &str,
    x_storage: &FieldStorage,
    x_agg_type: &str,
    x_with_y: Value,
) {
    let container = x_storage.x_container_name();
    let root = match final_agg.get_mut(agg_name) {
        Some(v) => v,
        None => return,
    };
    let aggs_obj = match root.get_mut("aggs") {
        Some(v) => v,
        None => return,
    };
    if container.is_empty() {
        aggs_obj[x_agg_type] = x_with_y;
        return;
    }
    if let Some(container_obj) = aggs_obj.get_mut(container) {
        if let Some(inner_aggs) = container_obj.get_mut("aggs") {
            inner_aggs[x_agg_type] = x_with_y;
        }
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
/// Build a complete histogram aggregation with per-category sub-histograms.
///
/// Fully type-agnostic: any combination of `(x_storage, cat_storage)` —
/// attribute × attribute, attribute × lineage, lineage × attribute, lineage × lineage —
/// is handled by composing [`FieldStorage`] values from [`field`][crate::report::field]
/// rather than by hand-writing separate cases.
///
/// # Aggregation structure (generalised)
/// ```text
/// {agg_name}:
///   [x nested envelope]
///     x_container: filter(x)
///       {x_bucket_type}: …           ← main x counts
///       categoryHistograms:
///         reverse_nested: {}
///         [cat nested envelope]
///           cat_container: filter(cat)
///             by_value: filters/histogram  ← per-cat buckets
///               [per-cat inner x agg — same x nested envelope]
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
    let x_storage = resolve_field_storage(&x_spec.field, x_spec.value_type, cache)?;
    let cat_storage = resolve_field_storage(cat_field, cat_value_type, cache)?;

    let (x_bucket_type, x_bucket_params) =
        build_x_agg_params(x_spec, x_storage.bucket_field(), x_bounds);
    let (by_value_agg_type, by_value_def) = build_by_value_agg(
        cat_value_type,
        cat_storage.bucket_field(),
        cat_bounds,
        cat_labels,
        show_other,
    );

    // Build the inner x agg block for each category bucket.
    // Uses canonical container names ("by_key"/"at_rank") so extraction paths
    // are deterministic via FieldStorage::inner_x_path().
    let per_cat_x_block =
        build_inner_x_agg_block(&x_storage, x_bucket_type, x_bucket_params.clone(), None);

    // Assemble: a named "by_value" agg → per_cat_x_block sub-aggs.
    // ES requires a name for every agg; "by_value" matches the extraction
    // path in FieldStorage::cat_histograms_base().
    let by_value_with_inner_x = json!({
        "by_value": {
            by_value_agg_type: by_value_def,
            "aggs": per_cat_x_block
        }
    });

    // Wrap in the cat nested envelope (by_attribute/at_cat_rank/etc.)
    let cat_aggs = wrap_cat_in_nested(&cat_storage, by_value_with_inner_x);

    let category_histograms = json!({
        "reverse_nested": {},
        "aggs": cat_aggs
    });

    // Build the main x agg via the generic factory and inject categoryHistograms.
    let x_agg_builder = agg_builder_for(x_spec, x_bounds, cache)?;
    let mut final_agg = x_agg_builder.build(agg_name);

    inject_category_histograms(&mut final_agg, agg_name, &x_storage, category_histograms);

    Ok(final_agg)
}

/// Inject `category_histograms` into the correct inner `aggs` map of a pre-built x agg.
fn inject_category_histograms(
    final_agg: &mut Value,
    agg_name: &str,
    x_storage: &FieldStorage,
    category_histograms: Value,
) {
    let container = x_storage.x_container_name();
    let root = match final_agg.get_mut(agg_name) {
        Some(v) => v,
        None => return,
    };
    let aggs_obj = match root.get_mut("aggs") {
        Some(v) => v,
        None => return,
    };
    if container.is_empty() {
        // Root-level x: inject directly into the top-level aggs.
        aggs_obj["categoryHistograms"] = category_histograms;
        return;
    }
    if let Some(container_obj) = aggs_obj.get_mut(container) {
        if let Some(inner_aggs) = container_obj.get_mut("aggs") {
            inner_aggs["categoryHistograms"] = category_histograms;
        }
    }
}

/// Select the appropriate `AggBuilder` for an axis spec.
///
/// Delegates all field-type detection to [`resolve_field_storage`] and returns a
/// [`GenericBucketAgg`] — a single type that handles attributes, lineage ranks and
/// root-level fields uniformly.
pub fn agg_builder_for(
    spec: &AxisSpec,
    bounds: &BoundsResult,
    cache: &Option<Arc<tokio::sync::RwLock<MetadataCache>>>,
) -> Result<Box<dyn AggBuilder>, String> {
    let storage = resolve_field_storage(&spec.field, spec.value_type, cache)?;
    let (bucket_type, bucket_params) = build_x_agg_params(spec, storage.bucket_field(), bounds);
    Ok(Box::new(GenericBucketAgg {
        storage,
        bucket_type: bucket_type.to_string(),
        bucket_params,
    }))
}

/// Map a requested geohash count to an ES geohash precision level (1–12).
#[allow(dead_code)]
fn geohash_precision_for_size(size: usize) -> u8 {
    match size {
        0..=50 => 3,
        51..=200 => 4,
        201..=1000 => 5,
        _ => 6,
    }
}
