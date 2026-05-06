//! Elasticsearch aggregation builders for report axes.
//!
//! Each `AggBuilder` produces the JSON fragment for one ES aggregation, and extracts
//! the bucket list from the response. Builders are composable: use `CompositeAggBuilder`
//! to nest them (e.g., histogram containing stats).

use serde_json::{json, Value};

use genomehubs_query::report::axis::{Scale, ValueType};
use genomehubs_query::report::{AxisSpec, BoundsResult};

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
}

impl AggBuilder for HistogramAggBuilder {
    fn build(&self, agg_name: &str) -> Value {
        json!({
            agg_name: {
                "histogram": {
                    "field": &self.field,
                    "interval": self.interval,
                    "extended_bounds": { "min": self.min, "max": self.max },
                    "min_doc_count": 0
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
pub struct CompositeAggBuilder {
    pub outer: Box<dyn AggBuilder>,
    pub inner: Box<dyn AggBuilder>,
    pub inner_name: String,
}

impl AggBuilder for CompositeAggBuilder {
    fn build(&self, agg_name: &str) -> Value {
        let mut outer = self.outer.build(agg_name);
        let inner_agg = self.inner.build(&self.inner_name);

        // Inject inner agg into outer's aggs key
        if let Some(outer_obj) = outer.get_mut(agg_name) {
            // Find the outer aggregation spec (histogram, date_histogram, or terms)
            for key in &["histogram", "date_histogram", "terms", "geohash_grid"] {
                if outer_obj.get(key).is_some() {
                    outer_obj["aggs"] = inner_agg;
                    break;
                }
            }
        }
        outer
    }

    fn extract(&self, resp: &Value, agg_name: &str) -> RawBuckets {
        self.outer.extract(resp, agg_name)
    }
}

/// Select the appropriate `AggBuilder` for an axis spec.
///
/// This is the main factory function; report handlers call it rather than
/// constructing builders directly.
pub fn agg_builder_for(spec: &AxisSpec, bounds: &BoundsResult) -> Box<dyn AggBuilder> {
    match spec.value_type {
        ValueType::Numeric => {
            let [min, max] = bounds.domain.unwrap_or([0.0, 1.0]);
            let interval = compute_histogram_interval(min, max, bounds.tick_count, spec.opts.scale);
            Box::new(HistogramAggBuilder {
                field: spec.field.clone(),
                interval,
                min,
                max,
                scale: spec.opts.scale,
            })
        }
        ValueType::Date => {
            let calendar_interval = bounds
                .interval
                .map(|i| i.to_es_interval().to_string())
                .unwrap_or_else(|| "1y".to_string());
            Box::new(DateHistogramAggBuilder {
                field: spec.field.clone(),
                calendar_interval,
                time_zone: None,
            })
        }
        ValueType::Keyword | ValueType::TaxonRank => Box::new(TermsAggBuilder {
            field: spec.field.clone(),
            size: spec.opts.size,
            include: if bounds.fixed_terms.is_empty() {
                None
            } else {
                Some(bounds.fixed_terms.clone())
            },
        }),
        ValueType::GeoPoint => Box::new(GeoHashAggBuilder {
            field: spec.field.clone(),
            precision: geohash_precision_for_size(spec.opts.size),
            size: spec.opts.size,
        }),
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

/// Map a requested geohash count to an ES geohash precision level (1–12).
fn geohash_precision_for_size(size: usize) -> u8 {
    match size {
        0..=50 => 3,
        51..=200 => 4,
        201..=1000 => 5,
        _ => 6,
    }
}
