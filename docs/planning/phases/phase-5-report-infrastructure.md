# Phase 5: Report Infrastructure

**Depends on:** Phase 4 (axis types), Phase 1 (es_client, AppState.client)
**Blocks:** Phase 6 (report routes use bounds, agg, pipeline), Phase 7 (arc uses agg)
**Estimated scope:** ~5 new files in `crates/genomehubs-api/src/report/` + /summary endpoint

**Note:** Phase 5 also includes the `GET /api/v3/summary` endpoint (deferred from Phase 2).
The summary aggregation logic reuses the `AggBuilder` infrastructure to avoid duplication
with histogram aggregations. Once report infrastructure is in place, adding /summary becomes
a thin wrapper around existing aggregation builders.

---

## Goal

Build the server-side report infrastructure:

1. **`bounds.rs`** — probe ES for a field's range/terms; produce `BoundsResult`
2. **`agg.rs`** — `AggBuilder` trait + concrete implementations for every histogram shape
3. **`pipeline.rs`** — `PipelineStep` trait + transformation steps from raw ES buckets to plot data

These three modules are purely server-side (I/O or data-transform). They contain no
HTTP route handlers and no report-type-specific logic; those come in Phase 6.

---

## Files to Create

```
crates/genomehubs-api/src/report/
    mod.rs        — re-exports; declares sub-modules
    bounds.rs     — compute_bounds()
    agg.rs        — AggBuilder trait + concrete builders
    pipeline.rs   — PipelineStep trait + transformation steps
```

## Files to Modify

| File                                | Change                                                   |
| ----------------------------------- | -------------------------------------------------------- |
| `crates/genomehubs-api/src/main.rs` | Add `mod report;`                                        |
| `crates/genomehubs-api/Cargo.toml`  | No new deps needed (reqwest, serde_json already present) |

---

## Implementation

### `crates/genomehubs-api/src/report/mod.rs`

```rust
//! Server-side report infrastructure.
//!
//! This module provides three layers:
//!
//! 1. `bounds` — probe ES for a field's actual domain and cardinality
//! 2. `agg` — build ES aggregation bodies
//! 3. `pipeline` — transform raw ES bucket responses into plot-ready data
//!
//! Report route handlers (Phase 6) wire these together.

pub mod agg;
pub mod bounds;
pub mod pipeline;

pub use agg::AggBuilder;
pub use bounds::compute_bounds;
pub use pipeline::Pipeline;
```

---

### `crates/genomehubs-api/src/report/bounds.rs`

`compute_bounds` runs one ES query per axis to determine the actual domain
(min/max for numerics, unique term count for keywords) before building the
main aggregation. This ensures histogram buckets cover the real data range.

```rust
use genomehubs_query::report::{AxisSpec, BoundsResult};
use genomehubs_query::report::axis::{DateInterval, Scale, ValueType};
use reqwest::Client;
use serde_json::{json, Value};

use crate::es_client;

/// Probe ES for the domain of a single axis field.
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
        ValueType::Numeric => compute_numeric_bounds(client, es_base, index, spec, base_query).await,
        ValueType::Date => compute_date_bounds(client, es_base, index, spec, base_query).await,
        ValueType::Keyword | ValueType::TaxonRank => {
            compute_keyword_bounds(client, es_base, index, spec, base_query).await
        }
        ValueType::GeoPoint => {
            // Geo bounds use a geo_bounds aggregation
            compute_geo_bounds(client, es_base, index, spec, base_query).await
        }
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
    let (domain_min, domain_max) = if matches!(
        spec.opts.scale,
        Scale::Log | Scale::Log2 | Scale::Log10
    ) {
        let floor = if raw_min > 0.0 { raw_min } else { 1.0 };
        (floor, raw_max.max(floor))
    } else {
        (raw_min, raw_max)
    };

    // Override with user-specified domain if provided
    let (final_min, final_max) = spec
        .opts
        .domain
        .map(|[lo, hi]| (lo, hi))
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
    let stats = resp.pointer("/aggregations/date_range").cloned().unwrap_or_default();

    let min_ms = stats.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let max_ms = stats.get("max").and_then(|v| v.as_f64()).unwrap_or(0.0);

    // Use user-specified interval if provided, otherwise let the route handler
    // choose based on the date range span
    let interval = spec.opts.interval;

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
    // Use fixed_values if provided; skip ES round-trip
    if !spec.opts.fixed_values.is_empty() {
        let labels = spec.opts.fixed_values.clone();
        return Ok(BoundsResult {
            domain: None,
            tick_count: labels.len(),
            interval: None,
            scale: Scale::Ordinal,
            value_type: spec.value_type,
            fixed_terms: labels.clone(),
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
                    "size": spec.opts.size
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

    // Encode geo viewport as [lon_min, lat_min, lon_max, lat_max] packed into domain[0..1]
    let bounds = resp.pointer("/aggregations/viewport/bounds").cloned().unwrap_or_default();
    let tl_lon = bounds.pointer("/top_left/lon").and_then(|v| v.as_f64()).unwrap_or(-180.0);
    let br_lon = bounds.pointer("/bottom_right/lon").and_then(|v| v.as_f64()).unwrap_or(180.0);

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
```

---

### `crates/genomehubs-api/src/report/agg.rs`

```rust
use serde_json::{json, Value};

use genomehubs_query::report::{AxisSpec, BoundsResult};
use genomehubs_query::report::axis::{Scale, ValueType};

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

/// Build a numeric `histogram` or `auto_date_histogram` aggregation.
pub struct HistogramAggBuilder {
    pub field: String,
    pub interval: f64,
    pub min: f64,
    pub max: f64,
    pub scale: Scale,
    pub missing: Option<Value>,
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
    pub calendar_interval: String,   // "1d", "1w", "1M", "3M", "1y", "10y"
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
            agg[agg_name]["date_histogram"]["time_zone"] = Value::String(tz.clone());
        }
        json!({ agg_name: agg[agg_name].clone() })
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
    pub include: Option<Vec<String>>,   // fixed term list
    pub missing_bucket: bool,
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
        if self.missing_bucket {
            terms["missing"] = json!("(missing)");
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
        if let Some(outer_agg) = outer.get_mut(agg_name) {
            outer_agg["aggs"] = inner_agg;
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
                missing: None,
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
            missing_bucket: false,
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
/// For log scales, the interval is computed in log-space.
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
```

---

### `crates/genomehubs-api/src/report/pipeline.rs`

```rust
use serde_json::Value;

use super::agg::RawBuckets;
use genomehubs_query::report::axis::Scale;

/// Context passed to each pipeline step.
pub struct ReportContext {
    pub scale: Scale,
    pub cat_labels: Vec<String>,
    pub show_other: bool,
}

/// A single transformation step applied to raw ES buckets.
///
/// Steps are composable: `Pipeline::run` applies them in sequence.
pub trait PipelineStep: Send + Sync {
    fn apply(&self, input: RawBuckets, ctx: &ReportContext) -> RawBuckets;
}

/// Apply a log/sqrt/ordinal scale transformation to bucket keys.
pub struct ScaleStep;

impl PipelineStep for ScaleStep {
    fn apply(&self, input: RawBuckets, ctx: &ReportContext) -> RawBuckets {
        input
            .into_iter()
            .map(|mut bucket| {
                if let Some(key) = bucket.get("key").and_then(|k| k.as_f64()) {
                    let scaled = match ctx.scale {
                        Scale::Log | Scale::Log10 => key.max(1.0).log10(),
                        Scale::Log2 => key.max(1.0).log2(),
                        Scale::Sqrt => key.max(0.0).sqrt(),
                        _ => key,
                    };
                    bucket["key_scaled"] = Value::from(scaled);
                }
                bucket
            })
            .collect()
    }
}

/// Pass buckets through unchanged.
pub struct NullStep;

impl PipelineStep for NullStep {
    fn apply(&self, input: RawBuckets, _ctx: &ReportContext) -> RawBuckets {
        input
    }
}

/// Replace raw keyword bucket keys with display labels.
///
/// When a cat axis has `fixed_values` in `AxisOpts`, the bucket keys
/// are canonical terms; `cat_labels` from `BoundsResult` may provide
/// friendlier display names (e.g. after aliasing assembly levels).
pub struct CatLabelStep;

impl PipelineStep for CatLabelStep {
    fn apply(&self, mut input: RawBuckets, ctx: &ReportContext) -> RawBuckets {
        if ctx.cat_labels.is_empty() {
            return input;
        }
        for bucket in &mut input {
            if let Some(key) = bucket.get("key").and_then(|k| k.as_str()) {
                if let Some(label) = ctx.cat_labels.iter().find(|l| l.as_str() == key) {
                    bucket["label"] = Value::String(label.clone());
                }
            }
        }
        input
    }
}

/// Retain raw `_source` documents instead of buckets (for scatter raw mode).
///
/// Passed through unchanged; used as a sentinel that tells the scatter
/// route to attach raw hit documents rather than aggregation buckets.
pub struct RawDataStep;

impl PipelineStep for RawDataStep {
    fn apply(&self, input: RawBuckets, _ctx: &ReportContext) -> RawBuckets {
        input
    }
}

/// Ordered sequence of pipeline steps applied left-to-right to raw buckets.
pub struct Pipeline {
    steps: Vec<Box<dyn PipelineStep>>,
}

impl Pipeline {
    /// Create an empty pipeline (pass-through).
    pub fn new() -> Self {
        Self { steps: vec![] }
    }

    /// Add a step to the end of the pipeline.
    pub fn add(mut self, step: impl PipelineStep + 'static) -> Self {
        self.steps.push(Box::new(step));
        self
    }

    /// Run all steps in order, threading `RawBuckets` through each.
    pub fn run(self, input: RawBuckets, ctx: &ReportContext) -> RawBuckets {
        self.steps.into_iter().fold(input, |buckets, step| step.apply(buckets, ctx))
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## Interval Auto-selection for Date Axes

When `spec.opts.interval` is `None`, the route handler should auto-select based on
the date range span (in milliseconds from `BoundsResult.domain`):

```rust
pub fn auto_date_interval(range_ms: f64) -> DateInterval {
    const DAY_MS: f64 = 86_400_000.0;
    const YEAR_MS: f64 = DAY_MS * 365.25;
    if range_ms < 30.0 * DAY_MS       { DateInterval::Day }
    else if range_ms < 6.0 * YEAR_MS  { DateInterval::Month }
    else if range_ms < 50.0 * YEAR_MS { DateInterval::Year }
    else                               { DateInterval::Decade }
}
```

Add this function to `bounds.rs` (it is a pure computation, no I/O).

---

## Verification

```bash
cargo build -p genomehubs-api
cargo test -p genomehubs-api report

# Bounds test (requires running ES + API)
curl -s -X POST http://localhost:3000/api/v3/report \
  -H 'Content-Type: application/json' \
  -d '{"query_yaml":"index: taxon\n","params_yaml":"taxonomy: ncbi\n",
       "report_yaml":"report: histogram\nx: genome_size\nx_opts: \";;20;log10\"\n"}' \
  | jq '.status'
```

---

## Completion Checklist

- [ ] `crates/genomehubs-api/src/report/mod.rs` created
- [ ] `compute_bounds()` implemented for all 4 value types
- [ ] `auto_date_interval()` added to `bounds.rs`
- [ ] `AggBuilder` trait + 6 concrete implementations in `agg.rs`
- [ ] `agg_builder_for()` factory function in `agg.rs`
- [ ] `PipelineStep` trait + 4 concrete steps in `pipeline.rs`
- [ ] `Pipeline::run()` threads steps correctly
- [ ] `mod report` declared in `crates/genomehubs-api/src/main.rs`
- [ ] `cargo build -p genomehubs-api` passes
- [ ] `cargo test -p genomehubs-api` passes
