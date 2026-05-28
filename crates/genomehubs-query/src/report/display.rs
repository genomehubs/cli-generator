//! Presentation and visual style options for a report.
//!
//! [`DisplaySpec`] is parsed from the `display` field in the API request and
//! returned unchanged in the response. Rendering is always client-side.
//!
//! Per-report-type options live in the typed sub-structs (`histogram`, `scatter`,
//! `tree`, etc.) embedded in [`DisplaySpec`]. Universal options (title, size,
//! legend, colour) are top-level fields.

use serde::{Deserialize, Serialize};

// ── Universal helpers ─────────────────────────────────────────────────────────

/// Legend position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LegendPosition {
    /// No legend.
    None,
    /// Legend above the plot.
    Top,
    /// Legend to the right of the plot (default).
    #[default]
    Right,
    /// Legend below the plot.
    Bottom,
    /// Legend to the left of the plot.
    Left,
}

/// Controls whether tick labels sit directly on tick marks or between them.
///
/// Tick marks always fall on bin boundaries regardless of this setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TickLabelPlacement {
    /// Label is centred on the tick mark.
    ///
    /// Natural for numeric/range bins where the tick value is the boundary
    /// (e.g. a genome-size histogram showing `1Mb`, `10Mb`, `100Mb` at each
    /// boundary tick).
    OnTick,
    /// Label is centred between two consecutive tick marks.
    ///
    /// Natural for keyword/category bins where the label names the bar
    /// occupying that interval (e.g. assembly-level bins: `"chromosome"`,
    /// `"scaffold"`, …).
    BetweenTicks,
}

/// Per-axis tick label and overflow options.
///
/// Embedded in per-report sub-structs for any configurable axis.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AxisOptions {
    /// Override label text for this axis.
    pub label: Option<String>,
    /// Rotate tick labels by this many degrees (positive = counter-clockwise).
    ///
    /// Default: `0` (horizontal). Use `-90` for vertical labels when tick
    /// values are long strings.
    pub tick_label_angle: Option<i16>,
    /// Show every Nth tick label when there are too many to fit without
    /// overlapping.
    ///
    /// `1` = show every label (default). `2` = every other label. `3` = one
    /// in three. `None` = auto: the renderer picks the largest stride that
    /// fits cleanly.
    pub tick_label_stride: Option<u32>,
    /// Maximum characters per keyword tick label before truncation with `…`.
    ///
    /// Applied to string labels only; numeric labels are not truncated.
    /// `None` means no truncation.
    pub tick_label_max_length: Option<usize>,
    /// Position of tick labels relative to tick marks.
    ///
    /// `None` = auto: `OnTick` for numeric/range axes, `BetweenTicks` for
    /// keyword/category axes.
    pub tick_label_placement: Option<TickLabelPlacement>,
    /// Show or hide tick labels entirely.
    /// `None` = auto: show when plot width ≥ `compact_width`.
    pub show_tick_labels: Option<bool>,
    /// d3 format string for tick values on this axis (e.g. `".2s"`).
    /// Overrides `DisplaySpec.number_format` for this axis only.
    pub number_format: Option<String>,
}

// ── Per-report-type sub-structs ───────────────────────────────────────────────

/// Histogram-specific display options.
///
/// Set via `display.histogram` in the API request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HistogramOptions {
    /// Stack category series instead of overlaying them.
    pub stacked: Option<bool>,
    /// Display mode for categorized histograms: "stacked", "grouped", or "facet".
    /// When present, overrides `stacked` where applicable.
    pub mode: Option<String>,
    /// Cumulative sum mode: each bar shows the sum of all preceding bars.
    pub cumulative: Option<bool>,
    /// Y-axis scale: `"linear"` (default), `"log10"`, or `"proportion"`.
    ///
    /// `"proportion"` normalises each bar to 1.0.
    pub y_scale: Option<String>,
    /// X-axis tick label and overflow options.
    pub x_axis: Option<AxisOptions>,
    /// Y-axis tick label and overflow options.
    pub y_axis: Option<AxisOptions>,
}

/// Scatter-specific display options.
///
/// Set via `display.scatter` in the API request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScatterOptions {
    /// Z-axis (heatmap density cell colour) scale.
    /// One of `"linear"` (default), `"log10"`, `"proportion"`, `"sqrt"`.
    pub z_scale: Option<String>,
    /// Stack mode — colour heatmap cells by category proportion.
    pub stacked: Option<bool>,
    /// Reverse the Y axis (high values at bottom).
    pub reversed: Option<bool>,
    /// Highlight a rectangular region of the plot: `"x_min,x_max,y_min,y_max"`
    /// in axis-space coordinates.
    ///
    /// All four bounds are required; use the axis min/max to highlight a full
    /// row or column. Values are in the native axis scale (e.g. log10 values
    /// when the axis is log-scaled).
    pub highlight_area: Option<String>,
    /// Render a reference line from a mathematical equation.
    ///
    /// Accepts a simple two-variable expression in terms of `x` and `y`
    /// (e.g. `"y = x"`, `"y = 2*x + 1"`, `"y = x^2"`). The renderer
    /// evaluates the expression over the visible axis range and draws the
    /// resulting curve. Only one equation line is supported per plot.
    pub equation_line: Option<String>,
    /// Maximum raw data points to overlay before switching to heatmap-only.
    pub scatter_threshold: Option<u32>,
    /// X-axis options.
    pub x_axis: Option<AxisOptions>,
    /// Y-axis options.
    pub y_axis: Option<AxisOptions>,
}

/// Count-per-rank bar chart display options.
///
/// Set via `display.count_per_rank` in the API request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CountPerRankOptions {
    /// Rank-axis (horizontal bar label) options.
    pub rank_axis: Option<AxisOptions>,
    /// Value-axis options.
    pub value_axis: Option<AxisOptions>,
}

/// Map report display options.
///
/// Set via `display.map` in the API request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MapOptions {
    /// Map projection.
    /// One of `"cylindricalEqualArea"` (default), `"mercator"`,
    /// `"naturalEarth1"`, `"orthographic"`.
    pub projection: Option<String>,
    /// Basemap visual theme: `"light"` or `"dark"`.
    pub theme: Option<String>,
    /// Display mode: `"hex"` (hexbin aggregation) or `"point"`.
    /// Auto-selected from point count vs `map_threshold` when `None`.
    pub map_type: Option<String>,
    /// Base layer type: `"map"` (flat 2-D projection, default) or `"globe"`
    /// (interactive 3-D orthographic sphere).
    ///
    /// When `"globe"`, the `projection` field is ignored and the renderer uses
    /// a drag-rotatable orthographic projection.
    pub base_type: Option<String>,
    /// Geographic bounding box: `"lon_min,lat_min,lon_max,lat_max"`.
    pub geo_bounds: Option<String>,
}

/// Tree report display options.
///
/// Set via `display.tree` in the API request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TreeOptions {
    /// Tree layout style: `"rect"` (default), `"circ"` (circular), `"radial"`.
    pub tree_style: Option<String>,
    /// Hide source colour bars on tree nodes.
    pub hide_source_colors: Option<bool>,
    /// Hide error bars on tree node summary boxes.
    pub hide_error_bars: Option<bool>,
    /// Hide ancestral value bars (show only direct/descendant values).
    pub hide_ancestral_bars: Option<bool>,
    /// Overlay PhyloPic silhouettes on tree nodes.
    pub show_phylopics: Option<bool>,
    /// Taxonomic rank at which to show PhyloPic images.
    pub phylopic_rank: Option<String>,
    /// Scale factor for PhyloPic silhouette size (default: `1.0`).
    pub phylopic_size: Option<f32>,
}

/// Arc (Venn-style) report display options.
///
/// Set via `display.arc` in the API request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArcOptions {
    /// Show percentage labels inside arc segments (default: `true`).
    pub show_labels: Option<bool>,
}

/// Sources data-attribution bar chart display options.
///
/// Set via `display.sources` in the API request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourcesOptions {
    /// Source-name axis (horizontal bar labels) options.
    pub source_axis: Option<AxisOptions>,
    /// Value-axis options.
    pub value_axis: Option<AxisOptions>,
}

// ── DisplaySpec ───────────────────────────────────────────────────────────────

/// Presentation and visual style options for a report.
///
/// All fields are optional; missing fields use renderer defaults.
/// Parsed from the `display` field in the API request (which accepts either a
/// YAML string or a JSON object) and returned in the response unchanged.
///
/// Universal options are top-level. Per-report-type options live in the
/// `histogram`, `scatter`, `tree`, `map`, `arc`, `sources`, and
/// `count_per_rank` sub-structs. At most one sub-struct will be populated per
/// response.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DisplaySpec {
    // ── Presentation / layout ─────────────────────────────────────────────────
    /// Main plot title.
    pub title: Option<String>,
    /// Secondary title shown below the main title.
    pub subtitle: Option<String>,
    /// Figure caption shown below the plot.
    pub caption: Option<String>,
    /// Plot width in pixels.
    pub width: Option<u32>,
    /// Plot height in pixels.
    pub height: Option<u32>,
    /// Legend position (default: `right`).
    #[serde(default)]
    pub legend: LegendPosition,
    /// Compact (small coloured squares) legend mode.
    ///
    /// `None` = auto-select: compact when plot width < `compact_width`.
    pub compact_legend: Option<bool>,
    /// Width threshold (pixels) below which compact legend is auto-selected.
    pub compact_width: Option<u32>,
    /// Override label for the X axis.
    pub x_label: Option<String>,
    /// Override label for the Y axis.
    pub y_label: Option<String>,
    /// Override label for the category (series) axis / legend header.
    pub cat_label: Option<String>,
    /// Additional fields to include in hover tooltips.
    #[serde(default)]
    pub tooltip_fields: Vec<String>,

    // ── Visual style ──────────────────────────────────────────────────────────
    /// Base font size in points for axis labels, legend text, and tick labels.
    pub font_size: Option<f32>,
    /// Base marker size in pixels for scatter plot points and legend swatches.
    pub marker_size: Option<f32>,
    /// d3 number format string (e.g. `".2s"`). Defaults to automatic.
    pub number_format: Option<String>,
    /// Named colour scheme (e.g. `"tableau10"`) or array of hex colours.
    pub color_scheme: Option<serde_json::Value>,
    /// Named colour palette (e.g. `"plasma"`, `"viridis"`).
    /// Overrides `color_scheme` for continuous scales.
    pub color_palette: Option<String>,
    /// Line width in pixels.
    pub line_width: Option<f32>,
    /// Series opacity (0.0–1.0).
    pub opacity: Option<f32>,

    // ── Per-report-type sub-structs ────────────────────────────────────────────
    /// Histogram-specific options. Populated when `report = "histogram"`.
    pub histogram: Option<HistogramOptions>,
    /// Scatter-specific options. Populated when `report = "scatter"`.
    pub scatter: Option<ScatterOptions>,
    /// Count-per-rank options. Populated when `report = "countPerRank"`.
    pub count_per_rank: Option<CountPerRankOptions>,
    /// Map-specific options. Populated when `report = "map"`.
    pub map: Option<MapOptions>,
    /// Tree-specific options. Populated when `report = "tree"`.
    pub tree: Option<TreeOptions>,
    /// Arc-specific options. Populated when `report = "arc"`.
    pub arc: Option<ArcOptions>,
    /// Sources bar chart options. Populated when `report = "sources"`.
    pub sources: Option<SourcesOptions>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_display_spec_is_all_none() {
        let spec = DisplaySpec::default();
        assert!(spec.title.is_none());
        assert!(spec.width.is_none());
        assert!(spec.height.is_none());
        assert_eq!(spec.legend, LegendPosition::Right);
        assert!(spec.tooltip_fields.is_empty());
        assert!(spec.histogram.is_none());
        assert!(spec.scatter.is_none());
        assert!(spec.cat_label.is_none());
        assert!(spec.compact_legend.is_none());
        assert!(spec.font_size.is_none());
        assert!(spec.marker_size.is_none());
    }

    #[test]
    fn round_trips_through_yaml_and_json() {
        let yaml = "title: Genome size\nwidth: 800\nheight: 500\n";
        let spec: DisplaySpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.title.as_deref(), Some("Genome size"));
        assert_eq!(spec.width, Some(800));
        assert_eq!(spec.height, Some(500));

        let json = serde_json::to_string(&spec).unwrap();
        let roundtrip: DisplaySpec = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.title, spec.title);
        assert_eq!(roundtrip.width, spec.width);
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let yaml = "title: Test\nunknown_key: ignored\n";
        let result: Result<DisplaySpec, _> = serde_yaml::from_str(yaml);
        assert!(result.is_ok());
    }

    #[test]
    fn legend_position_deserialises_lowercase() {
        let yaml = "legend: left\n";
        let spec: DisplaySpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.legend, LegendPosition::Left);
    }

    #[test]
    fn histogram_sub_struct_round_trips() {
        let yaml = "title: Histogram\nhistogram:\n  stacked: true\n  y_scale: log10\n  cumulative: false\n";
        let spec: DisplaySpec = serde_yaml::from_str(yaml).unwrap();
        let hist = spec.histogram.as_ref().unwrap();
        assert_eq!(hist.stacked, Some(true));
        assert_eq!(hist.y_scale.as_deref(), Some("log10"));
        assert_eq!(hist.cumulative, Some(false));

        let json = serde_json::to_string(&spec).unwrap();
        let rt: DisplaySpec = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.histogram.as_ref().unwrap().stacked, Some(true));
    }

    #[test]
    fn scatter_highlight_area_round_trips() {
        let yaml = "scatter:\n  z_scale: log10\n  highlight_area: \"1e8,1e10,1e8,1e10\"\n";
        let spec: DisplaySpec = serde_yaml::from_str(yaml).unwrap();
        let sc = spec.scatter.as_ref().unwrap();
        assert_eq!(sc.z_scale.as_deref(), Some("log10"));
        assert!(sc.highlight_area.is_some());
    }

    #[test]
    fn cat_label_and_compact_legend_round_trip() {
        let yaml = "cat_label: Assembly level\ncompact_legend: false\ncompact_width: 600\n";
        let spec: DisplaySpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.cat_label.as_deref(), Some("Assembly level"));
        assert_eq!(spec.compact_legend, Some(false));
        assert_eq!(spec.compact_width, Some(600));
    }

    #[test]
    fn tree_options_bool_flags_round_trip() {
        let yaml = "tree:\n  tree_style: rect\n  hide_ancestral_bars: true\n  show_phylopics: true\n  phylopic_rank: class\n";
        let spec: DisplaySpec = serde_yaml::from_str(yaml).unwrap();
        let tree = spec.tree.as_ref().unwrap();
        assert_eq!(tree.tree_style.as_deref(), Some("rect"));
        assert_eq!(tree.hide_ancestral_bars, Some(true));
        assert_eq!(tree.show_phylopics, Some(true));
        assert_eq!(tree.phylopic_rank.as_deref(), Some("class"));
    }

    #[test]
    fn axis_options_nested_round_trip() {
        let yaml = "histogram:\n  x_axis:\n    tick_label_angle: -90\n    show_tick_labels: true\n    label: \"Genome size (Gb)\"\n";
        let spec: DisplaySpec = serde_yaml::from_str(yaml).unwrap();
        let ax = spec.histogram.as_ref().unwrap().x_axis.as_ref().unwrap();
        assert_eq!(ax.tick_label_angle, Some(-90));
        assert_eq!(ax.show_tick_labels, Some(true));
        assert_eq!(ax.label.as_deref(), Some("Genome size (Gb)"));
    }

    #[test]
    fn tick_label_stride_and_placement_round_trip() {
        let yaml = "histogram:\n  x_axis:\n    tick_label_stride: 3\n    tick_label_max_length: 12\n    tick_label_placement: between_ticks\n";
        let spec: DisplaySpec = serde_yaml::from_str(yaml).unwrap();
        let ax = spec.histogram.as_ref().unwrap().x_axis.as_ref().unwrap();
        assert_eq!(ax.tick_label_stride, Some(3));
        assert_eq!(ax.tick_label_max_length, Some(12));
        assert_eq!(
            ax.tick_label_placement,
            Some(TickLabelPlacement::BetweenTicks)
        );

        let json = serde_json::to_string(&spec).unwrap();
        let rt: DisplaySpec = serde_json::from_str(&json).unwrap();
        let ax_rt = rt.histogram.as_ref().unwrap().x_axis.as_ref().unwrap();
        assert_eq!(
            ax_rt.tick_label_placement,
            Some(TickLabelPlacement::BetweenTicks)
        );
    }

    #[test]
    fn scatter_equation_line_separate_from_highlight_area() {
        let yaml =
            "scatter:\n  highlight_area: \"1e8,1e10,1e8,1e10\"\n  equation_line: \"y = x\"\n";
        let spec: DisplaySpec = serde_yaml::from_str(yaml).unwrap();
        let sc = spec.scatter.as_ref().unwrap();
        assert!(sc.highlight_area.is_some());
        assert_eq!(sc.equation_line.as_deref(), Some("y = x"));
    }

    #[test]
    fn map_base_type_globe_round_trips() {
        let yaml = "map:\n  base_type: globe\n  projection: orthographic\n";
        let spec: DisplaySpec = serde_yaml::from_str(yaml).unwrap();
        let m = spec.map.as_ref().unwrap();
        assert_eq!(m.base_type.as_deref(), Some("globe"));
        assert_eq!(m.projection.as_deref(), Some("orthographic"));
    }

    #[test]
    fn font_and_marker_size_independent() {
        let yaml = "font_size: 14.0\nmarker_size: 8.0\n";
        let spec: DisplaySpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.font_size, Some(14.0));
        assert_eq!(spec.marker_size, Some(8.0));
    }
}
