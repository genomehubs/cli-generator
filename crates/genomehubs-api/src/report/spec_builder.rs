//! Build [`PlotSpec`] from report pipeline output.
//!
//! This module bridges the report pipeline's raw JSON output to the typed
//! [`PlotSpec`] wire format. It is the only place that knows how to:
//!
//! 1. Extract axis metadata from the pipeline's `report_data` Value.
//! 2. Resolve `AxisOptions` display hints (from `DisplaySpec`) into the
//!    concrete resolved fields on [`AxisMeta`].
//!
//! The resulting `PlotSpec` is fully self-contained: renderers on any platform
//! do not need to re-query the API or re-interpret `DisplaySpec` hints.

use genomehubs_query::report::display::{AxisOptions, DisplaySpec, TickLabelPlacement};
use genomehubs_query::report::plot_spec::{AxisMeta, PlotReportType, PlotSpec, SeriesMeta};
use serde_json::Value;

// ── Public API ────────────────────────────────────────────────────────────────

/// Build a [`PlotSpec`] from a completed report response.
///
/// `report_type_str` is the raw report type string from the request (e.g.
/// `"histogram"`). `report_data` is the `Value` returned by the report
/// pipeline handler (the value at `response["report"]`). `display` is the
/// parsed [`DisplaySpec`] from the request; use `DisplaySpec::default()` when
/// no `display` was supplied.
///
/// Axis metadata and series metadata are extracted from `report_data` via
/// [`axis_meta_from_report_data`].  Display hints in `display` are resolved
/// into concrete fields on each [`AxisMeta`] via [`resolve_axis_display`].
pub fn build_plot_spec(
    report_type_str: &str,
    report_data: &Value,
    display: DisplaySpec,
) -> PlotSpec {
    let report_type = PlotReportType::parse(report_type_str).unwrap_or(PlotReportType::Histogram);

    let mut x = axis_meta_from_report_data(report_data, "x");
    let mut y = axis_meta_from_report_data(report_data, "y");

    // Resolve display hints onto axis metas.
    let x_axis_opts = display
        .histogram
        .as_ref()
        .and_then(|h| h.x_axis.as_ref())
        .or_else(|| display.scatter.as_ref().and_then(|s| s.x_axis.as_ref()));
    let y_axis_opts = display
        .histogram
        .as_ref()
        .and_then(|h| h.y_axis.as_ref())
        .or_else(|| display.scatter.as_ref().and_then(|s| s.y_axis.as_ref()));

    if let Some(ref mut ax) = x {
        resolve_axis_display(ax, x_axis_opts);
    }
    if let Some(ref mut ax) = y {
        resolve_axis_display(ax, y_axis_opts);
    }

    let series = extract_series(report_data);

    PlotSpec {
        report_type,
        x,
        y,
        z: None,
        series,
        display,
        data: report_data.clone(),
    }
}

/// Set the resolved display fields on an [`AxisMeta`] from an optional
/// [`AxisOptions`] hint.
///
/// This is the single place where the server-side resolution rules live:
///
/// - `tick_label_placement`: user hint → auto (`between_ticks` for keywords,
///   `on_tick` for everything else).
/// - `tick_label_stride`: user hint → `1` (auto-fitting is the renderer's job).
/// - `tick_label_max_length`: passed through from the hint.
pub fn resolve_axis_display(meta: &mut AxisMeta, opts: Option<&AxisOptions>) {
    meta.tick_label_placement =
        opts.and_then(|o| o.tick_label_placement)
            .unwrap_or(match meta.value_type.as_str() {
                "keyword" => TickLabelPlacement::BetweenTicks,
                _ => TickLabelPlacement::OnTick,
            });
    meta.tick_label_stride = opts.and_then(|o| o.tick_label_stride).unwrap_or(1);
    meta.tick_label_max_length = opts.and_then(|o| o.tick_label_max_length);

    // Apply label override from AxisOptions if present (takes precedence over
    // the field name default set by axis_meta_from_report_data).
    if let Some(ref label) = opts.and_then(|o| o.label.clone()) {
        meta.label = Some(label.clone());
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Extract an [`AxisMeta`] for the given axis role (`"x"` or `"y"`) from the
/// report pipeline's JSON output.
///
/// Looks for `report_data[role]` which the histogram and scatter handlers
/// write as:
/// ```json
/// {
///   "field": "genome_size",
///   "scale": "log10",
///   "domain": [1e6, 1e12],
///   "tickCount": 10
/// }
/// ```
///
/// Returns `None` when the role key is absent (e.g. `"y"` for a histogram).
fn axis_meta_from_report_data(report_data: &Value, role: &str) -> Option<AxisMeta> {
    let ax = report_data.get(role)?;
    let field = ax.get("field").and_then(|f| f.as_str())?.to_string();
    let scale = ax
        .get("scale")
        .and_then(|s| s.as_str())
        .unwrap_or("linear")
        .to_string();
    let domain = extract_domain(ax);
    let value_type = infer_value_type(&scale, &field);

    Some(AxisMeta {
        field,
        label: None,
        scale,
        domain,
        tick_values: vec![],
        tick_labels: vec![],
        value_type,
        // Defaults; overwritten by resolve_axis_display.
        tick_label_placement: TickLabelPlacement::OnTick,
        tick_label_stride: 1,
        tick_label_max_length: None,
    })
}

/// Extract `[min, max]` from an axis JSON object.
///
/// Accepts both a `"domain": [min, max]` array (scatter reports) and
/// separate `"domain": {"min": ..., "max": ...}` objects, as well as the
/// top-level `"bounds"` object used by some pipeline handlers.
fn extract_domain(ax: &Value) -> [f64; 2] {
    if let Some(arr) = ax.get("domain").and_then(|d| d.as_array()) {
        let min = arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
        let max = arr.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0);
        return [min, max];
    }
    if let Some(obj) = ax.get("domain").and_then(|d| d.as_object()) {
        let min = obj.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let max = obj.get("max").and_then(|v| v.as_f64()).unwrap_or(1.0);
        return [min, max];
    }
    // Fallback: look for "bounds" with "min"/"max" at the axis level.
    let min = ax.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let max = ax.get("max").and_then(|v| v.as_f64()).unwrap_or(1.0);
    [min, max]
}

/// Infer `value_type` from scale and field name heuristics.
///
/// This is a lightweight fallback; the pipeline handlers do not currently
/// write `value_type` into the report data. A future refactor can add it
/// directly to the pipeline output and remove this heuristic.
fn infer_value_type(scale: &str, field: &str) -> String {
    // Coordinate fields are always "coordinate".
    if field.ends_with("_location") || field.ends_with("_geo") || field == "location" {
        return "coordinate".to_string();
    }
    // Ordinal scale → keyword.
    if scale == "ordinal" {
        return "keyword".to_string();
    }
    "float".to_string()
}

/// Extract `SeriesMeta` from the `cats` / `by_cat` fields in report data.
///
/// For categorised histogram/scatter reports the pipeline writes:
/// ```json
/// { "cats": ["chromosome", "scaffold", ...], "by_cat": { ... } }
/// ```
///
/// Returns an empty vec for non-categorised reports.
fn extract_series(report_data: &Value) -> Vec<SeriesMeta> {
    let cats = match report_data.get("cats").and_then(|c| c.as_array()) {
        Some(c) => c,
        None => return vec![],
    };
    cats.iter()
        .filter_map(|cat| cat.as_str())
        .map(|key| SeriesMeta {
            key: key.to_string(),
            label: key.to_string(),
            color: None,
        })
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn build_plot_spec_histogram_extracts_x_axis() {
        let report_data = json!({
            "type": "histogram",
            "x": {
                "field": "genome_size",
                "scale": "log10",
                "domain": [1e6, 1e12]
            },
            "buckets": []
        });
        let spec = build_plot_spec("histogram", &report_data, DisplaySpec::default());
        assert_eq!(spec.report_type, PlotReportType::Histogram);
        let x = spec.x.unwrap();
        assert_eq!(x.field, "genome_size");
        assert_eq!(x.scale, "log10");
        assert_eq!(x.domain, [1e6, 1e12]);
        assert_eq!(x.tick_label_placement, TickLabelPlacement::OnTick);
        assert_eq!(x.tick_label_stride, 1);
        assert!(spec.y.is_none());
    }

    #[test]
    fn resolve_axis_display_applies_user_hint_placement() {
        let mut meta = AxisMeta {
            field: "assembly_level".to_string(),
            label: None,
            scale: "ordinal".to_string(),
            domain: [0.0, 1.0],
            tick_values: vec![],
            tick_labels: vec![],
            value_type: "keyword".to_string(),
            tick_label_placement: TickLabelPlacement::OnTick,
            tick_label_stride: 1,
            tick_label_max_length: None,
        };
        let opts = AxisOptions {
            label: Some("Assembly level".to_string()),
            tick_label_angle: None,
            tick_label_stride: Some(2),
            tick_label_max_length: Some(10),
            tick_label_placement: Some(TickLabelPlacement::BetweenTicks),
            show_tick_labels: None,
            number_format: None,
        };
        resolve_axis_display(&mut meta, Some(&opts));
        assert_eq!(meta.tick_label_placement, TickLabelPlacement::BetweenTicks);
        assert_eq!(meta.tick_label_stride, 2);
        assert_eq!(meta.tick_label_max_length, Some(10));
        assert_eq!(meta.label.as_deref(), Some("Assembly level"));
    }

    #[test]
    fn resolve_axis_display_auto_between_ticks_for_keyword() {
        let mut meta = AxisMeta {
            field: "assembly_level".to_string(),
            label: None,
            scale: "linear".to_string(),
            domain: [0.0, 5.0],
            tick_values: vec![],
            tick_labels: vec![],
            value_type: "keyword".to_string(),
            tick_label_placement: TickLabelPlacement::OnTick,
            tick_label_stride: 1,
            tick_label_max_length: None,
        };
        resolve_axis_display(&mut meta, None);
        assert_eq!(meta.tick_label_placement, TickLabelPlacement::BetweenTicks);
    }

    #[test]
    fn extract_series_returns_empty_for_uncategorised_report() {
        let report_data = json!({ "type": "histogram", "buckets": [] });
        let series = extract_series(&report_data);
        assert!(series.is_empty());
    }

    #[test]
    fn extract_series_returns_series_for_categorised_report() {
        let report_data = json!({
            "type": "histogram",
            "cats": ["chromosome", "scaffold", "contig"],
            "by_cat": {}
        });
        let series = extract_series(&report_data);
        assert_eq!(series.len(), 3);
        assert_eq!(series[0].key, "chromosome");
    }

    #[test]
    fn build_plot_spec_unknown_type_defaults_to_histogram() {
        let report_data = json!({ "buckets": [] });
        let spec = build_plot_spec("unknown_type", &report_data, DisplaySpec::default());
        assert_eq!(spec.report_type, PlotReportType::Histogram);
    }
}
