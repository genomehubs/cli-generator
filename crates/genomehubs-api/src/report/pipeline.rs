//! Bucket transformation pipeline for report data.
//!
//! Pipeline steps are composable transformations applied left-to-right to raw
//! ES aggregation buckets. Each step is a reusable module that handles one aspect
//! of data shaping (scaling, labeling, filtering, etc.).

use serde_json::Value;

use genomehubs_query::report::axis::Scale;

use super::agg::RawBuckets;

/// Context passed to each pipeline step.
///
/// Contains axis-specific information needed by transformation steps.
pub struct ReportContext {
    pub scale: Scale,
    pub cat_labels: Vec<String>,
    pub show_other: bool,
}

/// A single transformation step applied to raw ES buckets.
///
/// Steps are composable: `Pipeline::run` applies them in sequence.
pub trait PipelineStep: Send + Sync {
    /// Transform the input bucket list and return the result.
    fn apply(&self, input: RawBuckets, ctx: &ReportContext) -> RawBuckets;
}

/// Apply a log/sqrt/ordinal scale transformation to bucket keys.
///
/// For numeric scales like log or sqrt, computes the scaled value and stores it
/// in a `key_scaled` field for rendering. Linear scale keys pass through unchanged.
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

/// Pass buckets through unchanged (identity transformation).
pub struct NullStep;

impl PipelineStep for NullStep {
    fn apply(&self, input: RawBuckets, _ctx: &ReportContext) -> RawBuckets {
        input
    }
}

/// Replace raw keyword bucket keys with display labels.
///
/// When a categorical axis has fixed values with translations (from AxisOpts),
/// the bucket keys are canonical terms; `cat_labels` from `BoundsResult` may
/// provide friendlier display names. This step stores the label in a `label` field.
pub struct CatLabelStep;

impl PipelineStep for CatLabelStep {
    fn apply(&self, mut input: RawBuckets, ctx: &ReportContext) -> RawBuckets {
        if ctx.cat_labels.is_empty() {
            return input;
        }

        // Build a key → label map from cat_labels
        // (assumes cat_labels parallel to bucket ordering)
        for (i, bucket) in input.iter_mut().enumerate() {
            if i < ctx.cat_labels.len() {
                bucket["label"] = Value::String(ctx.cat_labels[i].clone());
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

/// Filter buckets to retain only the top N by doc count.
///
/// Useful for limit-based reports where only the most significant buckets
/// should be displayed (e.g., top-10 assemblies by count).
pub struct TopBucketsStep {
    pub limit: usize,
}

impl PipelineStep for TopBucketsStep {
    fn apply(&self, mut input: RawBuckets, _ctx: &ReportContext) -> RawBuckets {
        input.sort_by(|a, b| {
            let a_count = a
                .get("doc_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let b_count = b
                .get("doc_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            b_count.cmp(&a_count)
        });
        input.truncate(self.limit);
        input
    }
}

/// Ordered sequence of pipeline steps applied left-to-right to raw buckets.
///
/// Builder pattern: construct with `Pipeline::new()`, add steps with `.add()`,
/// then run with `.run()`.
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
        self.steps
            .into_iter()
            .fold(input, |buckets, step| step.apply(buckets, ctx))
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}
