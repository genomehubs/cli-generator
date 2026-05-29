//! Self-contained plot specification type.
//!
//! [`PlotSpec`] wraps processed report data alongside all display metadata
//! needed to render a plot without re-querying the API. It is produced by
//! `crates/genomehubs-api/src/report/spec_builder.rs` and returned in the
//! API response `plot_spec` field when requested.
//!
//! Rust defines the data shape and resolved axis metadata; each platform
//! renders natively. The JS SDK converts `PlotSpec` to Vega-Lite via a thin
//! JS function. Python/R receive the dict/list directly.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::display::{DisplaySpec, TickLabelPlacement};

// ── Report type discriminant ──────────────────────────────────────────────────

/// The type of report this spec describes.
///
/// Oxford/ribbon/painting variants are added when Phase 11 (positional family
/// endpoint) lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlotReportType {
    Histogram,
    Scatter,
    CountPerRank,
    Sources,
    Tree,
    Map,
    Arc,
    Oxford,
    Ribbon,
    Painting,
}

impl PlotReportType {
    /// Parse a report type string into a `PlotReportType`.
    ///
    /// Accepts both the camelCase wire names used by the API
    /// (`"countPerRank"`) and the snake_case serde names (`"count_per_rank"`).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "histogram" => Some(Self::Histogram),
            "scatter" => Some(Self::Scatter),
            "countPerRank" | "count_per_rank" => Some(Self::CountPerRank),
            "sources" => Some(Self::Sources),
            "tree" => Some(Self::Tree),
            "map" => Some(Self::Map),
            "arc" => Some(Self::Arc),
            "oxford" => Some(Self::Oxford),
            "ribbon" => Some(Self::Ribbon),
            "painting" => Some(Self::Painting),
            _ => None,
        }
    }
}

// ── Axis metadata ─────────────────────────────────────────────────────────────

/// Fully resolved axis metadata for a single axis.
///
/// All display fields (`tick_label_placement`, `tick_label_stride`,
/// `tick_label_max_length`) are **resolved** by the server — renderers
/// consume them directly without any auto-detection logic. They are
/// derived from the matching [`crate::report::display::AxisOptions`] hint
/// in the request's `DisplaySpec` plus the axis `value_type`; see
/// `spec_builder::resolve_axis_display`.
///
/// `tick_values` and `tick_labels` are the computed tick positions and their
/// display strings (empty slices mean the renderer should auto-generate them
/// from `domain` + `scale`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisMeta {
    /// Field name from the ES index.
    pub field: String,
    /// Human-readable axis label (overrides field name in renderers).
    pub label: Option<String>,
    /// Scale applied to axis values.
    ///
    /// One of `"linear"` | `"log10"` | `"log2"` | `"sqrt"` | `"proportion"`.
    pub scale: String,
    /// `[min, max]` of the axis in native (pre-scale) data coordinates.
    pub domain: [f64; 2],
    /// Pre-computed tick positions in native data coordinates.
    ///
    /// Empty when the renderer should compute them from `domain` + `scale`.
    pub tick_values: Vec<f64>,
    /// Display strings for each tick in `tick_values`.
    ///
    /// Must have the same length as `tick_values` when non-empty.
    pub tick_labels: Vec<String>,
    /// Inferred data type of this axis's field values.
    ///
    /// One of `"integer"` | `"float"` | `"keyword"` | `"coordinate"`.
    pub value_type: String,
    /// Resolved label placement relative to tick marks.
    ///
    /// `on_tick` for numeric/range axes; `between_ticks` for keyword/category
    /// axes. Tick marks always fall on bin boundaries.
    pub tick_label_placement: TickLabelPlacement,
    /// Resolved label stride.
    ///
    /// `1` = show every label. `2` = every other label. `3` = one in three.
    /// Set from the `AxisOptions` hint; falls back to `1` (auto-fitting is the
    /// renderer's responsibility).
    pub tick_label_stride: u32,
    /// Maximum characters before truncation with `…`.
    ///
    /// `None` means no truncation. Applied to string labels only.
    pub tick_label_max_length: Option<usize>,
}

// ── Series metadata ───────────────────────────────────────────────────────────

/// A single data series (category) for multi-series plots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeriesMeta {
    /// Canonical series key as it appears in the data.
    pub key: String,
    /// Human-readable series label for legends.
    pub label: String,
    /// Hex colour string assigned by the theme (e.g. `"#4c78a8"`).
    pub color: Option<String>,
}

// ── PlotSpec ──────────────────────────────────────────────────────────────────

/// A fully self-contained plot specification.
///
/// Contains processed data and all metadata needed to render the plot on any
/// platform. Produced by
/// `crates/genomehubs-api/src/report/spec_builder::build_plot_spec` and
/// returned in the API response `plot_spec` field when the request includes
/// a `display` field (Phase 10) or sets `include_plot_spec: true` in `params`.
///
/// # Platform rendering
///
/// | Platform | Approach |
/// |----------|----------|
/// | JS SDK   | `plotSpecToVegaLite(spec)` → Vega-Lite JSON |
/// | Python SDK | Plain `dict`; users pass to altair / plotly |
/// | R SDK    | Plain `list`; users pass to ggplot2 / vegalite |
/// | CLI      | Rust `plotters` (feature-gated, not in WASM) — Phase 12b |
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlotSpec {
    /// Report type, determining which sub-struct in `display` is relevant.
    pub report_type: PlotReportType,
    /// Primary (X) axis metadata, if applicable.
    pub x: Option<AxisMeta>,
    /// Secondary (Y) axis metadata, if applicable.
    pub y: Option<AxisMeta>,
    /// Category axis metadata (for series / categorical axes), if applicable.
    pub cat: Option<AxisMeta>,
    /// Tertiary (Z / heatmap density) axis metadata, if applicable.
    pub z: Option<AxisMeta>,
    /// Series (category) metadata. Empty for non-categorised plots.
    pub series: Vec<SeriesMeta>,
    /// Display spec carried through from the request (with per-type sub-structs).
    pub display: DisplaySpec,
    /// Serialised plot data. Shape depends on `report_type`.
    ///
    /// For `histogram`: `{ "buckets": [...], "allValues": [...] }`.
    /// For `scatter`: `{ "cells": [...] }`.
    /// For `tree`: `{ "treeNodes": {...} }`.
    /// Matches the existing `report` field in the API response.
    pub data: Value,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::display::DisplaySpec;

    #[test]
    fn plot_report_type_from_str_camel_and_snake() {
        assert_eq!(
            PlotReportType::parse("countPerRank"),
            Some(PlotReportType::CountPerRank)
        );
        assert_eq!(
            PlotReportType::parse("count_per_rank"),
            Some(PlotReportType::CountPerRank)
        );
        assert_eq!(
            PlotReportType::parse("histogram"),
            Some(PlotReportType::Histogram)
        );
        assert_eq!(PlotReportType::parse("unknown"), None);
    }

    #[test]
    fn plot_spec_round_trips_through_json() {
        let spec = PlotSpec {
            report_type: PlotReportType::Histogram,
            x: Some(AxisMeta {
                field: "genome_size".to_string(),
                label: Some("Genome size".to_string()),
                scale: "log10".to_string(),
                domain: [1e6, 1e12],
                tick_values: vec![1e6, 1e9, 1e12],
                tick_labels: vec!["1Mb".to_string(), "1Gb".to_string(), "1Tb".to_string()],
                value_type: "float".to_string(),
                tick_label_placement: TickLabelPlacement::OnTick,
                tick_label_stride: 1,
                tick_label_max_length: None,
            }),
            y: None,
            cat: None,
            z: None,
            series: vec![SeriesMeta {
                key: "chromosome".to_string(),
                label: "Chromosome".to_string(),
                color: Some("#4c78a8".to_string()),
            }],
            display: DisplaySpec::default(),
            data: serde_json::json!({"buckets": []}),
        };

        let json = serde_json::to_string(&spec).unwrap();
        let rt: PlotSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.report_type, PlotReportType::Histogram);
        assert_eq!(rt.x.as_ref().unwrap().field, "genome_size");
        assert_eq!(rt.series[0].key, "chromosome");
        assert_eq!(
            rt.x.as_ref().unwrap().tick_label_placement,
            TickLabelPlacement::OnTick
        );
    }

    #[test]
    fn axis_meta_placement_serialises_as_snake_case() {
        let meta = AxisMeta {
            field: "assembly_level".to_string(),
            label: None,
            scale: "linear".to_string(),
            domain: [0.0, 1.0],
            tick_values: vec![],
            tick_labels: vec![],
            value_type: "keyword".to_string(),
            tick_label_placement: TickLabelPlacement::BetweenTicks,
            tick_label_stride: 1,
            tick_label_max_length: Some(12),
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("\"between_ticks\""));
        assert!(json.contains("\"tick_label_max_length\":12"));
    }
}
