//! Report axis type system.
//!
//! All types in this module are serialisable and WASM-compatible.
//! They express the full configuration space for a single report axis —
//! what field to aggregate, how to bin it, and how to present the result.

pub mod axis;
pub mod bounds;
pub mod display;
pub mod hybrid;
pub mod layout;
pub mod plot_spec;
pub mod positional;
pub mod spec_builder;
pub mod tree_category;
pub mod tree_layout;
pub mod tree_polar;
pub mod tree_rect;
pub mod tree_ring;

pub use axis::{
    AxisOpts, AxisRole, AxisSpec, AxisSummary, DateInterval, Scale, SortMode, ValueType,
};
pub use bounds::BoundsResult;
pub use display::DisplaySpec;
pub use plot_spec::PlotSpec;
pub use positional::{
    AttributeFilter, FilterOperator, FilterTarget, FilterValue, PositionalReportType,
    PositionalSpec, RegionBounds, RegionsSpec,
};
use serde::{Deserialize, Serialize};
pub use spec_builder::resolve_axis_display;

/// Supported v3 report types.
#[derive(Debug, Clone, PartialEq)]
pub enum ReportType {
    Histogram,
    Scatter,
    Map,
    Tree,
    CountPerRank,
    Sources,
    Arc,
}

impl ReportType {
    /// Parse a report type string into a `ReportType` variant.
    ///
    /// Returns `None` for unknown strings.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "histogram" => Some(Self::Histogram),
            "scatter" => Some(Self::Scatter),
            "map" => Some(Self::Map),
            "tree" => Some(Self::Tree),
            "countPerRank" => Some(Self::CountPerRank),
            "sources" => Some(Self::Sources),
            "arc" => Some(Self::Arc),
            _ => None,
        }
    }

    /// Return the canonical string name for this report type.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Histogram => "histogram",
            Self::Scatter => "scatter",
            Self::Map => "map",
            Self::Tree => "tree",
            Self::CountPerRank => "countPerRank",
            Self::Sources => "sources",
            Self::Arc => "arc",
        }
    }

    /// Fields that must be present in the report YAML for this type.
    pub fn required_axes(&self) -> &'static [&'static str] {
        match self {
            Self::Histogram => &["x"],
            Self::Scatter => &["x", "y"],
            Self::Map => &[],
            Self::Tree => &["rank"],
            Self::CountPerRank => &["query"],
            Self::Sources => &[],
            Self::Arc => &["x"],
        }
    }
}

// ── URL parsing ───────────────────────────────────────────────────────────────

/// Parse a v2 report URL into `(query_yaml, params_yaml, report_yaml)`.
///
/// Handles both API URLs (`/api/v2/report?…`) and UI URLs (`/report?…`).
/// The search-related params (`tax_name=`, `fields=`, `result=`, `size=`, …)
/// are forwarded to [`crate::query::query_yaml_from_url_params`].  Report-specific
/// params (`report=`, `x=`, `y=`, `cat=`, …) are extracted into a YAML map.
///
/// Returns `Err` when a required `report=` param is absent or YAML serialisation
/// fails.
pub fn report_yaml_from_url_params(url: &str) -> Result<(String, String, String), String> {
    let (query_yaml, params_yaml) = crate::query::query_yaml_from_url_params(url)?;
    let params = crate::query::parse_url_query_string(url);

    let report_type = params
        .get("report")
        .and_then(|v| v.first())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "URL does not contain a 'report=' parameter".to_string())?
        .clone();

    let mut doc = std::collections::BTreeMap::<String, serde_json::Value>::new();
    doc.insert("report".to_string(), serde_json::Value::String(report_type));

    for (key, yaml_key) in [
        ("x", "x"),
        ("y", "y"),
        ("cat", "cat"),
        ("rank", "rank"),
        ("xOpts", "x_opts"),
        ("yOpts", "y_opts"),
        ("catOpts", "cat_opts"),
    ] {
        if let Some(val) = params.get(key).and_then(|v| v.first()) {
            if !val.is_empty() {
                doc.insert(yaml_key.to_string(), serde_json::Value::String(val.clone()));
            }
        }
    }

    // Boolean / numeric fields
    if let Some(val) = params.get("collapseMonotypic").and_then(|v| v.first()) {
        let b = matches!(val.to_lowercase().as_str(), "true" | "1" | "yes");
        doc.insert("collapse_monotypic".to_string(), serde_json::Value::Bool(b));
    }
    for (key, yaml_key) in [
        ("hexResolution", "hex_resolution"),
        ("countThreshold", "scatter_threshold"),
        ("mapThreshold", "map_threshold"),
    ] {
        if let Some(val) = params.get(key).and_then(|v| v.first()) {
            if let Ok(n) = val.parse::<i64>() {
                doc.insert(yaml_key.to_string(), serde_json::Value::Number(n.into()));
            }
        }
    }

    // `ranks` — comma-separated list
    if let Some(ranks_str) = params.get("ranks").and_then(|v| v.first()) {
        if !ranks_str.is_empty() {
            let ranks: Vec<serde_json::Value> = ranks_str
                .split(',')
                .map(|r| serde_json::Value::String(r.trim().to_string()))
                .collect();
            doc.insert("ranks".to_string(), serde_json::Value::Array(ranks));
        }
    }

    let report_yaml = serde_yaml::to_string(&doc).map_err(|e| e.to_string())?;
    Ok((query_yaml, params_yaml, report_yaml))
}

// ── Vega-Lite conversion ──────────────────────────────────────────────────────

/// Convert a `PlotSpec` JSON string into a render specification.
///
/// Tree reports produce Vega v6 via [`plot_spec_to_vega_json`]; all other reports
/// produce Vega-Lite v6 via [`plot_spec_to_vega_lite_json`].
pub fn plot_spec_to_render_json(input: &str) -> String {
    match report_type_of(input).as_deref() {
        Some("tree") => plot_spec_to_vega_json(input),
        _ => plot_spec_to_vega_lite_json(input),
    }
}

/// Read the tree display options from `display.tree`; missing values take their defaults.
///
/// `font_size` and `font_scale` are resolved against the canvas. Setting both is an error.
/// A `category_display` mode that does not suit `style` is also an error.
fn tree_render_options(
    tree: &serde_json::Value,
    style: &str,
    width: f64,
    height: f64,
) -> Result<tree_layout::TreeRenderOptions, String> {
    let alignment =
        tree_layout::TipAlignment::parse(tree.get("tip_alignment").and_then(|v| v.as_str()))?;
    let label_orientation = tree_layout::LabelOrientation::parse(
        tree.get("label_orientation").and_then(|v| v.as_str()),
    )?;
    let font_size = tree_layout::resolve_font_size(
        tree.get("font_size").and_then(|v| v.as_f64()),
        tree.get("font_scale").and_then(|v| v.as_f64()),
        width,
        height,
    )?;
    let category_display = tree_layout::CategoryDisplay::resolve(
        tree.get("category_display").and_then(|v| v.as_str()),
        style == "ring",
    )?;
    Ok(tree_layout::TreeRenderOptions {
        alignment,
        label_orientation,
        font_size,
        category_display,
    })
}

/// Convert a tree `PlotSpec` JSON string into a Vega v6 specification JSON string.
///
/// Chooses the layout from `display.tree`: `tree_style` is `"rect"` (default), `"circ"`, `"radial"`, or `"ring"`.
/// `total_angle` (default `360.0`) applies to `"circ"` and `"ring"`. `start_angle` (default `-90.0`) applies to `"circ"`, `"radial"`, and `"ring"`.
/// `tip_alignment` is `"none"` (default), `"tips"`, `"ultrametric"`, or `"labels"`. It applies to `"rect"`, `"circ"`, and `"radial"`.
/// `label_orientation` is `"flip"` (default) or `"continuous"`. It applies to `"circ"`, `"radial"`, and `"ring"`.
/// `font_size` sets label size in pixels, and `font_scale` sets it as a share of the shorter canvas side. Set at most one.
/// A spec without `treeNodes` yields an empty tree sized from `display`.
/// Returns an error JSON on failure, including an unknown `tree_style`.
pub fn plot_spec_to_vega_json(input: &str) -> String {
    let parsed: serde_json::Value = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(e) => return format!("{{\"error\":\"invalid JSON: {e}\"}}"),
    };
    let Some(spec_val) = plot_spec_value(&parsed) else {
        return "{\"error\":\"no plot_spec found in input\"}".to_string();
    };

    let display = spec_val.get("display").unwrap_or(&serde_json::Value::Null);
    let width = display.get("width").and_then(|v| v.as_u64()).unwrap_or(600);
    let height = display
        .get("height")
        .and_then(|v| v.as_u64())
        .unwrap_or(400);
    let title = display.get("title").and_then(|v| v.as_str());
    let tree = display.get("tree").unwrap_or(&serde_json::Value::Null);
    let style = tree
        .get("tree_style")
        .and_then(|v| v.as_str())
        .unwrap_or("rect");
    let total_angle = tree
        .get("total_angle")
        .and_then(|v| v.as_f64())
        .unwrap_or(360.0);
    let start_angle = tree
        .get("start_angle")
        .and_then(|v| v.as_f64())
        .unwrap_or(-90.0);
    let mut tree_nodes = spec_val
        .pointer("/data/treeNodes")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let options = match tree_render_options(tree, style, width as f64, height as f64) {
        Ok(options) => options,
        Err(e) => return serde_json::json!({"error": e}).to_string(),
    };
    let legend = tree_category::annotate_category_colours(
        &mut tree_nodes,
        spec_val.pointer("/data/catBounds"),
    );
    let legend_title = spec_val
        .pointer("/data/catBounds/field")
        .and_then(|v| v.as_str())
        .unwrap_or("category");
    let rendered = match style {
        "rect" => tree_rect::rect_tree_vega(&tree_nodes, width, height, title, options),
        "circ" => tree_polar::circ_tree_vega(
            &tree_nodes,
            width,
            height,
            title,
            total_angle,
            start_angle,
            options,
        ),
        "radial" => {
            tree_polar::radial_tree_vega(&tree_nodes, width, height, title, start_angle, options)
        }
        "ring" => tree_ring::ring_tree_vega(
            &tree_nodes,
            width,
            height,
            title,
            total_angle,
            start_angle,
            options,
        ),
        other => Err(format!("unknown tree_style: {other}")),
    };
    match rendered {
        Ok(mut spec) => {
            tree_category::add_category_legend(&mut spec, &legend, legend_title);
            spec.to_string()
        }
        Err(e) => serde_json::json!({"error": e}).to_string(),
    }
}

/// Return the `PlotSpec` object from a bare spec or a full `/report` response.
fn plot_spec_value(parsed: &serde_json::Value) -> Option<&serde_json::Value> {
    if parsed.get("report_type").is_some() {
        Some(parsed)
    } else {
        parsed.get("plot_spec")
    }
}

/// Read `report_type` from a spec or report envelope, or `None` if absent or invalid.
fn report_type_of(input: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(input).ok()?;
    plot_spec_value(&parsed)?
        .get("report_type")?
        .as_str()
        .map(str::to_string)
}

/// Convert a `PlotSpec` JSON string (or a full `/report` response) to a Vega-Lite v6 specification JSON string.
///
/// Accepts the full `/report` response envelope (extracts `plot_spec` automatically)
/// or a bare `PlotSpec` object.  Returns an error JSON on failure.
pub fn plot_spec_to_vega_lite_json(input: &str) -> String {
    let parsed: serde_json::Value = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(e) => return format!("{{\"error\":\"invalid JSON: {e}\"}}"),
    };

    let Some(spec_val) = plot_spec_value(&parsed) else {
        return "{\"error\":\"no plot_spec found in input\"}".to_string();
    };

    let report_type = spec_val
        .get("report_type")
        .and_then(|v| v.as_str())
        .unwrap_or("histogram");

    let display = spec_val.get("display").unwrap_or(&serde_json::Value::Null);
    let title = display.get("title").and_then(|v| v.as_str());
    let width = display.get("width").and_then(|v| v.as_u64()).unwrap_or(600);
    let height = display
        .get("height")
        .and_then(|v| v.as_u64())
        .unwrap_or(400);
    let font_size = display
        .get("font_size")
        .and_then(|v| v.as_f64())
        .unwrap_or(12.0);

    let mut base = serde_json::json!({
        "$schema": "https://vega.github.io/schema/vega-lite/v6.json",
        "width": width,
        "height": height,
        "config": {
            "axis": {"labelFontSize": font_size, "titleFontSize": font_size},
            "legend": {"labelFontSize": font_size}
        }
    });
    if let Some(t) = title {
        base["title"] = serde_json::Value::String(t.to_string());
    }

    let vl = match report_type {
        "histogram" => vl_histogram(spec_val, base),
        "scatter" => vl_scatter(spec_val, base),
        "oxford" | "ribbon" => vl_scatter(spec_val, base),
        "count_per_rank" | "countPerRank" | "sources" => vl_bar(spec_val, base),
        "painting" => vl_painting(spec_val, base),
        "map" => {
            let projection = display
                .get("map")
                .and_then(|m| m.get("projection"))
                .and_then(|p| p.as_str())
                .unwrap_or("mercator");
            base["projection"] = serde_json::json!({"type": projection});
            base
        }
        "arc" => {
            // Special-case batch arc payloads produced by the API: these use
            // data.type == "arc_batch" and contain `entries` with per-ring
            // `scaled` values (0..1) and `report_index`/`report_label`.
            if spec_val
                .get("data")
                .and_then(|d| d.get("type"))
                .and_then(|t| t.as_str())
                == Some("arc_batch")
            {
                vl_arc_batch(spec_val, base)
            } else {
                vl_arc(spec_val, base)
            }
        }
        _ => base,
    };

    match serde_json::to_string(&vl) {
        Ok(s) => s,
        Err(e) => format!("{{\"error\":\"serialisation failed: {e}\"}}"),
    }
}

/// Build a Vega-Lite encoding object for an axis given optional server-side
/// axis metadata and optional numeric boundaries (bin edges).
fn make_vl_axis_encoding(
    axis_meta_opt: Option<&serde_json::Value>,
    data_field: &str,
    label_hint: Option<&str>,
    boundaries_opt: Option<&[f64]>,
    prefer_nominal: bool,
    z_index: Option<i64>,
) -> Result<serde_json::Value, String> {
    // Require server-provided axis metadata including `value_type`.
    let meta = axis_meta_opt.ok_or_else(|| {
        format!(
            "missing axis metadata for field '{}' — server must provide axis.value_type",
            data_field
        )
    })?;

    let value_type = meta
        .get("value_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("axis metadata for '{}' lacks 'value_type'", data_field))?;

    let label = meta
        .get("label")
        .and_then(|v| v.as_str())
        .or(label_hint)
        .unwrap_or(data_field);

    let scale_hint = meta
        .get("scale")
        .and_then(|v| v.as_str())
        .unwrap_or("linear");

    // Map canonical server `value_type` strings to Vega-Lite types deterministically.
    let vl_type = if prefer_nominal || value_type == "keyword" {
        "nominal"
    } else if value_type == "date" {
        "temporal"
    } else if value_type == "float"
        || value_type == "integer"
        || value_type == "number"
        || value_type == "coordinate"
    {
        "quantitative"
    } else {
        return Err(format!(
            "unknown axis value_type '{}' for field '{}'",
            value_type, data_field
        ));
    };

    let scale_type = if vl_type == "nominal" {
        "band"
    } else if vl_type == "temporal" {
        "time"
    } else if scale_hint == "log10" {
        "log"
    } else {
        "linear"
    };

    // Helper to convert JSON values (number or numeric string) to f64
    fn json_to_f64(v: &serde_json::Value) -> Option<f64> {
        if let Some(n) = v.as_f64() {
            Some(n)
        } else if let Some(s) = v.as_str() {
            s.parse::<f64>().ok()
        } else {
            None
        }
    }

    // Tick values: explicit computed boundaries (if provided) because these
    // are derived from bin edges and are usually the best ticks for histograms;
    // otherwise use server-provided tick values if present and non-empty.
    let tick_values_json = if let Some(b) = boundaries_opt {
        if vl_type == "temporal" {
            Some(serde_json::Value::Array(
                b.iter()
                    .map(|v| serde_json::Value::Number((*v as i64).into()))
                    .collect(),
            ))
        } else {
            Some(serde_json::Value::Array(
                b.iter().map(|v| serde_json::Value::from(*v)).collect(),
            ))
        }
    } else {
        meta.get("tick_values")
            .and_then(|tv| tv.as_array())
            .and_then(|arr| {
                if arr.is_empty() {
                    None
                } else if vl_type == "temporal" {
                    // Convert numeric tick values to datetime signals; leave strings alone
                    Some(serde_json::Value::Array(
                        arr.iter()
                            .map(|v| {
                                if let Some(n) = v.as_f64() {
                                    serde_json::json!({"signal": format!("datetime({})", n as i64)})
                                } else if let Some(s) = v.as_str() {
                                    serde_json::Value::String(s.to_string())
                                } else {
                                    v.clone()
                                }
                            })
                            .collect(),
                    ))
                } else {
                    Some(serde_json::Value::Array(arr.clone()))
                }
            })
    };

    // Domain: prefer explicit computed boundaries (if provided) because these
    // are derived from bin edges and are usually the correct visual domain;
    // otherwise fall back to a server-provided domain (robustly parsed).
    let domain_opt = boundaries_opt
        .and_then(|b| {
            if !b.is_empty() {
                Some((b[0], *b.last().unwrap()))
            } else {
                None
            }
        })
        .or_else(|| {
            meta.get("domain")
                .and_then(|d| d.as_array())
                .and_then(|arr| {
                    if arr.len() >= 2 {
                        let lo = json_to_f64(&arr[0]).unwrap_or(0.0);
                        let hi = json_to_f64(&arr[1]).unwrap_or(lo + 1.0);
                        Some((lo, hi))
                    } else {
                        None
                    }
                })
        });

    // Build scale object
    let mut scale_obj = serde_json::Map::new();
    scale_obj.insert(
        "type".to_string(),
        serde_json::Value::String(scale_type.to_string()),
    );
    if let Some((lo, hi)) = domain_opt {
        if vl_type == "temporal" {
            scale_obj.insert(
                "domain".to_string(),
                serde_json::Value::Array(vec![
                    serde_json::Value::Number((lo as i64).into()),
                    serde_json::Value::Number((hi as i64).into()),
                ]),
            );
        } else {
            scale_obj.insert("domain".to_string(), serde_json::json!([lo, hi]));
        }
    }
    if vl_type == "nominal" {
        scale_obj.insert(
            "paddingOuter".to_string(),
            serde_json::Value::Number((0).into()),
        );
        // Remove inner padding so adjacent categorical bars fill the full
        // width between ticks (useful for histogram-style categorical axes).
        scale_obj.insert(
            "paddingInner".to_string(),
            serde_json::Value::Number((0).into()),
        );
        // If the server provided explicit tick values for a nominal axis,
        // use them as the scale domain to preserve ordering (e.g. taxon id
        // list or human-readable bucket labels).
        #[allow(clippy::collapsible_match)]
        if let Some(tv) = &tick_values_json {
            if let serde_json::Value::Array(arr) = tv {
                if !arr.is_empty() {
                    scale_obj.insert("domain".to_string(), serde_json::Value::Array(arr.clone()));
                }
            }
        }
    }

    // Build axis object
    let mut axis_obj = serde_json::Map::new();
    axis_obj.insert(
        "title".to_string(),
        serde_json::Value::String(label.to_string()),
    );
    if let Some(tv) = tick_values_json {
        axis_obj.insert("values".to_string(), tv);
    }
    if vl_type == "temporal" {
        // Choose a sensible date format. Prefer server-declared interval when
        // present (e.g. "year" -> show year only). Otherwise heuristically
        // infer from computed boundaries if available.
        let mut date_fmt = "%Y-%m-%d".to_string();
        if let Some(interval_str) = meta.get("interval").and_then(|v| v.as_str()) {
            match interval_str {
                "year" | "decade" => date_fmt = "%Y".to_string(),
                "month" | "quarter" => date_fmt = "%Y-%m".to_string(),
                _ => date_fmt = "%Y-%m-%d".to_string(),
            }
        } else if let Some(b) = boundaries_opt {
            if b.len() >= 2 {
                let width = (b[1] - b[0]).abs();
                let day_ms = 86400.0 * 1000.0;
                let year_ms = 365.0 * day_ms;
                if width >= year_ms {
                    date_fmt = "%Y".to_string();
                } else if width >= 28.0 * day_ms {
                    date_fmt = "%Y-%m".to_string();
                } else {
                    date_fmt = "%Y-%m-%d".to_string();
                }
            }
        }
        axis_obj.insert("format".to_string(), serde_json::Value::String(date_fmt));
    } else if vl_type == "quantitative" {
        axis_obj.insert(
            "format".to_string(),
            serde_json::Value::String(".3s".to_string()),
        );
    } else if vl_type == "nominal" {
        axis_obj.insert("grid".to_string(), serde_json::Value::Bool(true));
        axis_obj.insert(
            "tickBand".to_string(),
            serde_json::Value::String("extent".to_string()),
        );
    }
    if let Some(z) = z_index {
        axis_obj.insert("zindex".to_string(), serde_json::Value::Number(z.into()));
    }

    Ok(serde_json::json!({
        "field": data_field,
        "type": vl_type,
        "scale": serde_json::Value::Object(scale_obj),
        "axis": serde_json::Value::Object(axis_obj)
    }))
}

/// Compute the pixel width for a single bar in a grouped histogram or scatter bar chart.
///
/// Divides the available `plot_width_px` evenly across `n_bins` bins, reserves 90 %
/// of each bin for bar content, then splits that space evenly among `n_cats` categories.
/// The result is clamped to a minimum of 2 px so bars remain visible for large datasets.
fn grouped_bar_size_px(n_bins: f64, n_cats: f64, plot_width_px: f64) -> f64 {
    let bin_pixel = if n_bins > 0.0 {
        plot_width_px / n_bins
    } else {
        10.0
    };
    ((bin_pixel * 0.9) / n_cats.max(1.0)).max(2.0)
}

fn vl_histogram(spec: &serde_json::Value, mut base: serde_json::Value) -> serde_json::Value {
    let x_meta = spec.get("x").unwrap_or(&serde_json::Value::Null);
    let x_field = x_meta
        .get("field")
        .and_then(|v| v.as_str())
        .unwrap_or("key");
    let x_label = x_meta
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or(x_field);

    let display = spec.get("display").unwrap_or(&serde_json::Value::Null);
    let hist = display.get("histogram").unwrap_or(&serde_json::Value::Null);
    let y_scale_str = hist
        .get("y_scale")
        .and_then(|v| v.as_str())
        .unwrap_or("linear");
    let y_vl_scale = if y_scale_str == "log10" {
        "log"
    } else {
        "linear"
    };
    let y_min = match y_vl_scale {
        "log" => 1.0, // avoid log(0) issues
        _ => 0.0,
    };
    let y_label = display
        .get("y_label")
        .and_then(|v| v.as_str())
        .unwrap_or("Count");

    // Transform ES-style buckets (key, doc_count) into left/right bar values
    // with explicit `x` (left) and `x2` (right) so Vega-Lite draws bars with
    // bin boundaries. Also compute axis `values` ticks at each boundary.
    let raw_buckets = spec
        .get("data")
        .and_then(|d| d.get("buckets"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));

    // If the server provided per-category breakdowns (`by_cat`) produce
    // a long-form dataset so Vega-Lite can render stacked/grouped/faceted
    // histograms. Preserve axis metadata for numeric/date axes by keeping
    // `x` as numeric/temporal values when possible so tick formatting is
    // delegated to `make_vl_axis_encoding` (server-side metadata remains
    // authoritative).
    if let Some(by_cat_val) = spec.get("data").and_then(|d| d.get("by_cat")).cloned() {
        if by_cat_val.is_object() {
            let hist_mode = hist
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("grouped");
            let hist_cumulative = hist
                .get("cumulative")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            // Category order preference: explicit `data.cats` else object keys order
            let cats: Vec<String> = spec
                .get("data")
                .and_then(|d| d.get("cats"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|s| s.as_str().unwrap_or("").to_string())
                        .collect()
                })
                .unwrap_or_else(|| by_cat_val.as_object().unwrap().keys().cloned().collect());

            // Optional mapping from raw cat key -> human-readable label. If
            // the server provided `report_data.cat.tick_labels` and a
            // parallel `report_data.cats` ordering, use that to map keys to
            // friendly labels for display (legend, color). This preserves
            // raw keys for lookups while presenting readable names.
            let mut cat_label_map: Option<std::collections::HashMap<String, String>> = None;
            if let Some(lbls) = spec
                .get("data")
                .and_then(|d| d.get("cat"))
                .and_then(|c| c.get("tick_labels"))
                .and_then(|v| v.as_array())
            {
                if lbls.len() == cats.len() {
                    let mut m = std::collections::HashMap::new();
                    for (i, k) in cats.iter().enumerate() {
                        if let Some(lbl) = lbls.get(i).and_then(|v| v.as_str()) {
                            m.insert(k.clone(), lbl.to_string());
                        }
                    }
                    if !m.is_empty() {
                        cat_label_map = Some(m);
                    }
                }
            }

            // Decide how to treat the x axis: preserve numeric/temporal type
            // when the server indicates it (so axis ticks/formatting are kept).
            let x_value_type = x_meta
                .get("value_type")
                .and_then(|v| v.as_str())
                .unwrap_or("keyword");

            // Helper: extract bucket count for (cat, idx)
            let get_count = |cat: &str, idx: usize| -> f64 {
                by_cat_val
                    .get(cat)
                    .and_then(|arr| arr.as_array())
                    .and_then(|a| a.get(idx))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
            };

            if x_value_type == "keyword" {
                // Categorical: previous behaviour — string `x` values with server
                // ordering preserved; support stacked/grouped/facet via color/xOffset/facet.
                let bucket_labels: Vec<String> = raw_buckets
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .map(|b| {
                                b.get("label")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string())
                                    .or_else(|| {
                                        b.get("key").and_then(|k| k.as_str().map(|s| s.to_string()))
                                    })
                                    .or_else(|| {
                                        b.get("id").and_then(|k| k.as_str().map(|s| s.to_string()))
                                    })
                                    .unwrap_or_else(|| b.to_string())
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                // Long-form values and compute per-bucket sums. Support
                // cumulative mode by maintaining running totals per category.
                let mut values: Vec<serde_json::Value> = Vec::new();
                let mut max_sum: f64 = 0.0;
                let mut running: Vec<f64> = vec![0.0; cats.len()];
                let mut cat_max: Vec<f64> = vec![0.0; cats.len()];
                for (i, bl) in bucket_labels.iter().enumerate() {
                    let mut bucket_sum = 0.0_f64;
                    for (ci, cat) in cats.iter().enumerate() {
                        let count = get_count(cat, i);
                        let display_count = if hist_cumulative {
                            running[ci] += count;
                            running[ci]
                        } else {
                            count
                        };
                        // track per-category maximum for grouped/facet domains
                        if display_count > cat_max[ci] {
                            cat_max[ci] = display_count;
                        }
                        bucket_sum += display_count;
                        let mut obj = serde_json::Map::new();
                        obj.insert("x".to_string(), serde_json::Value::String(bl.clone()));
                        let display_cat = cat_label_map
                            .as_ref()
                            .and_then(|m| m.get(cat))
                            .cloned()
                            .unwrap_or_else(|| cat.clone());
                        obj.insert("cat".to_string(), serde_json::Value::String(display_cat));
                        obj.insert(
                            "doc_count".to_string(),
                            serde_json::Value::from(display_count),
                        );
                        values.push(serde_json::Value::Object(obj));
                    }
                    if bucket_sum > max_sum {
                        max_sum = bucket_sum;
                    }
                }
                let max_cat_max = cat_max.iter().cloned().fold(0.0_f64, f64::max);

                // X encoding uses nominal domain derived from bucket labels
                let x_meta_override =
                    serde_json::json!({"tick_values": bucket_labels, "value_type": "keyword"});
                let mut x_encoding = match make_vl_axis_encoding(
                    Some(&x_meta_override),
                    "x",
                    Some(x_label),
                    None,
                    true,
                    Some(1),
                ) {
                    Ok(v) => v,
                    Err(e) => return serde_json::json!({"error": e}),
                };

                // When grouped mode is requested on a nominal x axis, allow
                // some inner padding so `xOffset` can place multiple bars
                // side-by-side inside each band. Default `paddingInner=0`
                // would make bars occupy the full band and overlap.
                if hist_mode == "grouped" {
                    if let Some(x_enc_obj) = x_encoding.as_object() {
                        if let Some(scale_val) = x_enc_obj.get("scale").cloned() {
                            if scale_val.is_object() {
                                let mut scale_map =
                                    scale_val.as_object().cloned().unwrap_or_default();
                                scale_map.insert("paddingInner".to_string(), serde_json::json!(0));
                                scale_map.insert("paddingOuter".to_string(), serde_json::json!(0));
                                // replace the scale in x_encoding
                                if let Some(x_enc_obj_mut) = x_encoding.as_object_mut() {
                                    x_enc_obj_mut.insert(
                                        "scale".to_string(),
                                        serde_json::Value::Object(scale_map),
                                    );
                                }
                            }
                        }
                    }
                }

                // Y axis encoding: doc_count; prefer quantitative with sensible domain
                let y_axis_meta = spec.get("y");
                let mut y_encoding = match make_vl_axis_encoding(
                    y_axis_meta,
                    "doc_count",
                    Some(y_label),
                    None,
                    false,
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return serde_json::json!({"error": e}),
                };
                // Determine y domain depending on histogram mode:
                // - stacked: domain = max total per bin (max_sum)
                // - grouped/facet: domain = max per-category bar height (max_cat_max)
                let desired_y_max = match hist_mode {
                    "grouped" | "facet" => {
                        if max_cat_max > 0.0 {
                            max_cat_max
                        } else {
                            1.0
                        }
                    }
                    _ => {
                        if max_sum > 0.0 {
                            max_sum
                        } else {
                            1.0
                        }
                    }
                };

                if let Some(obj) = y_encoding.as_object_mut() {
                    obj.insert(
                        "aggregate".to_string(),
                        serde_json::Value::String("sum".to_string()),
                    );
                    if let Some(scale_val) = obj.get_mut("scale") {
                        if scale_val.is_object() {
                            scale_val.as_object_mut().unwrap().insert(
                                "domain".to_string(),
                                serde_json::json!([0.0, desired_y_max]),
                            );
                        }
                    }
                }

                base["data"] = serde_json::json!({"values": values});

                // Compute pixel-based grouped bar size for categorical bins
                let plot_width_px =
                    base.get("width").and_then(|v| v.as_u64()).unwrap_or(600) as f64;
                let grouped_bar_px = grouped_bar_size_px(
                    bucket_labels.len() as f64,
                    cats.len() as f64,
                    plot_width_px,
                );

                match hist_mode {
                    "grouped" => {
                        // Use xOffset to separate categories within each bucket.
                        // Also explicitly disable stacking so viewers that auto-stack
                        // aggregated colour channels will render grouped bars.
                        if let Some(y_obj) = y_encoding.as_object_mut() {
                            y_obj.insert("stack".to_string(), serde_json::Value::Null);
                        }
                        base["mark"] = serde_json::json!({"type": "bar", "size": grouped_bar_px});
                        base["encoding"] = serde_json::json!({
                            "x": x_encoding,
                            "y": y_encoding,
                            "color": {"field": "cat", "type": "nominal"},
                            "xOffset": {"field": "cat", "type": "nominal"}
                        });
                        return base;
                    }
                    "facet" => {
                        // Use facet: row by category (small multiples). Place the
                        // `mark` and `encoding` inside `spec` only; do not set a
                        // top-level `mark` (invalid with `facet`).
                        let spec_obj = serde_json::json!({
                            "mark": {"type": "bar"},
                            "encoding": {"x": x_encoding, "y": y_encoding, "y2": {"datum": y_min}}
                        });
                        base["facet"] =
                            serde_json::json!({"row": {"field": "cat", "type": "nominal"}});
                        base["spec"] = spec_obj;
                        // Keep colour/legend off for facet default; caller can style separately
                        return base;
                    }
                    _ => {
                        // default: stacked — let Vega-Lite perform stacking via
                        // the `color` encoding and aggregated `y`.
                        base["mark"] = serde_json::json!({"type": "bar", "size": grouped_bar_px});
                        base["encoding"] = serde_json::json!({
                            "x": x_encoding,
                            "y": y_encoding,
                            "color": {"field": "cat", "type": "nominal"}
                        });
                        return base;
                    }
                }
            } else {
                // Numeric / temporal buckets: preserve numeric x values so axis
                // formatting and tick values computed by the server are retained.
                // Compute numeric keys and boundaries similar to the non-cat path.
                let mut keys_num: Vec<f64> = Vec::new();
                if let Some(arr) = raw_buckets.as_array() {
                    for b in arr {
                        if let Some(k) = b.get("key").and_then(|v| v.as_f64()) {
                            keys_num.push(k);
                        }
                    }
                }

                // Determine bin width
                let width = if keys_num.len() >= 2 {
                    keys_num[1] - keys_num[0]
                } else if let Some(domain_arr) = x_meta.get("domain").and_then(|d| d.as_array()) {
                    if domain_arr.len() >= 2 {
                        let lo = domain_arr[0].as_f64().unwrap_or(0.0);
                        let hi = domain_arr[1].as_f64().unwrap_or(lo + 1.0);
                        let tick_count = x_meta
                            .get("tickCount")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(10) as f64;
                        (hi - lo) / tick_count.max(1.0)
                    } else {
                        1.0
                    }
                } else {
                    1.0
                };

                // Build numeric boundaries
                let mut boundaries_f64: Vec<f64> = Vec::new();
                if !keys_num.is_empty() {
                    for k in &keys_num {
                        boundaries_f64.push(*k);
                    }
                    let last_right = if keys_num.len() >= 2 {
                        keys_num[keys_num.len() - 1] + (keys_num[1] - keys_num[0])
                    } else {
                        keys_num[0] + width
                    };
                    boundaries_f64.push(last_right);
                }

                // Long-form numeric values with either (x,x2) for stacked mode
                // or center-based `x` for grouped mode so bars can be narrower
                // and offset with `xOffset`.
                let mut values: Vec<serde_json::Value> = Vec::new();
                let mut max_sum: f64 = 0.0;

                // Precompute bin centers
                let mut centers: Vec<f64> = Vec::new();
                for (i, left) in keys_num.iter().enumerate() {
                    let right = if i + 1 < keys_num.len() {
                        keys_num[i + 1]
                    } else {
                        left + width
                    };
                    centers.push(left + (right - left) / 2.0);
                }

                // Compute pixel-based bar size for grouped mode so bars fit side-by-side
                let plot_width_px =
                    base.get("width").and_then(|v| v.as_u64()).unwrap_or(600) as f64;
                let n_cats = cats.len() as f64;
                let grouped_bar_px =
                    grouped_bar_size_px(keys_num.len() as f64, n_cats, plot_width_px);

                // Precompute domain span for converting pixel offsets into data units
                let domain_min = boundaries_f64.first().cloned().unwrap_or(0.0);
                let domain_max = boundaries_f64.last().cloned().unwrap_or(domain_min + 1.0);
                let domain_span = if domain_max > domain_min {
                    domain_max - domain_min
                } else {
                    1.0
                };

                let mut running: Vec<f64> = vec![0.0; cats.len()];
                let mut cat_max: Vec<f64> = vec![0.0; cats.len()];
                for (i, left) in keys_num.iter().enumerate() {
                    let right = if i + 1 < keys_num.len() {
                        keys_num[i + 1]
                    } else {
                        left + width
                    };
                    let mut bucket_sum = 0.0_f64;
                    for (ci, cat) in cats.iter().enumerate() {
                        let count = get_count(cat, i);
                        let display_count = if hist_cumulative {
                            running[ci] += count;
                            running[ci]
                        } else {
                            count
                        };
                        // track per-category maximum for grouped/facet scaling
                        if display_count > cat_max[ci] {
                            cat_max[ci] = display_count;
                        }
                        bucket_sum += display_count;
                        let mut obj = serde_json::Map::new();
                        if hist_mode == "grouped" {
                            // Compute a small data-space shift for this category so
                            // bars are placed side-by-side without relying on
                            // viewer support for `xOffset`.
                            let ci_f = ci as f64;
                            let center_index = (n_cats - 1.0) / 2.0;
                            let data_per_pixel = domain_span / plot_width_px.max(1.0);
                            let bar_data_width = grouped_bar_px * data_per_pixel;
                            let shift = (ci_f - center_index) * bar_data_width;
                            let x_val = centers[i] + shift;
                            obj.insert("x".to_string(), serde_json::Value::from(x_val));
                        } else {
                            // Stacked / default: use range [x,x2]
                            obj.insert("x".to_string(), serde_json::Value::from(*left));
                            obj.insert("x2".to_string(), serde_json::Value::from(right));
                        }
                        let display_cat = cat_label_map
                            .as_ref()
                            .and_then(|m| m.get(cat))
                            .cloned()
                            .unwrap_or_else(|| cat.clone());
                        obj.insert("cat".to_string(), serde_json::Value::String(display_cat));
                        obj.insert(
                            "doc_count".to_string(),
                            serde_json::Value::from(display_count),
                        );
                        values.push(serde_json::Value::Object(obj));
                    }
                    if bucket_sum > max_sum {
                        max_sum = bucket_sum;
                    }
                }

                // Build x encoding using numeric boundaries so axis formatting is correct
                let x_encoding = match make_vl_axis_encoding(
                    spec.get("x"),
                    "x",
                    Some(x_label),
                    Some(&boundaries_f64),
                    false,
                    Some(1),
                ) {
                    Ok(v) => v,
                    Err(e) => return serde_json::json!({"error": e}),
                };

                // Y axis encoding: doc_count; aggregate per x and ensure domain starts at 0
                let y_axis_meta = spec.get("y");
                let mut y_encoding = match make_vl_axis_encoding(
                    y_axis_meta,
                    "doc_count",
                    Some(y_label),
                    None,
                    false,
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => return serde_json::json!({"error": e}),
                };
                let max_cat_max = cat_max.iter().cloned().fold(0.0_f64, f64::max);
                let desired_y_max = match hist_mode {
                    "grouped" | "facet" => {
                        if max_cat_max > 0.0 {
                            max_cat_max
                        } else {
                            1.0
                        }
                    }
                    _ => {
                        if max_sum > 0.0 {
                            max_sum
                        } else {
                            1.0
                        }
                    }
                };
                if let Some(obj) = y_encoding.as_object_mut() {
                    obj.insert(
                        "aggregate".to_string(),
                        serde_json::Value::String("sum".to_string()),
                    );
                    if let Some(scale_val) = obj.get_mut("scale") {
                        if scale_val.is_object() {
                            scale_val.as_object_mut().unwrap().insert(
                                "domain".to_string(),
                                serde_json::json!([0.0, desired_y_max]),
                            );
                        }
                    }
                }

                base["data"] = serde_json::json!({"values": values});

                match hist_mode {
                    "facet" => {
                        // Facet: small multiples by category; keep shared x scale
                        let spec_obj = serde_json::json!({
                            "mark": {"type": "bar"},
                            "encoding": {"x": x_encoding, "x2": {"field": "x2"}, "y": y_encoding, "y2": {"datum": y_min}}
                        });
                        base["facet"] =
                            serde_json::json!({"row": {"field": "cat", "type": "nominal"}});
                        base["spec"] = spec_obj;
                        return base;
                    }
                    "grouped" => {
                        // Grouped: use xOffset (Vega-Lite v5+) to offset categories within numeric bins
                        // and explicitly disable stacking on the y encoding so viewers
                        // do not aggregate into stacked bars.
                        if let Some(y_obj) = y_encoding.as_object_mut() {
                            y_obj.insert("stack".to_string(), serde_json::Value::Null);
                        }
                        base["mark"] = serde_json::json!({"type": "bar", "size": grouped_bar_px});
                        base["encoding"] = serde_json::json!({
                            "x": x_encoding,
                            "y": y_encoding,
                            "color": {"field": "cat", "type": "nominal"},
                            "xOffset": {"field": "cat", "type": "nominal"}
                        });
                        return base;
                    }
                    _ => {
                        // Default: attempt stacked. Vega-Lite stacking across numeric
                        // continuous axes is not universally supported; for numeric
                        // axes we fallback to grouped behaviour to preserve axis
                        // formatting. If the server prefers true stacked nominal
                        // bins it can provide `x.tick_labels` and the client can
                        // request `value_type: keyword` instead.
                        base["mark"] = serde_json::json!({"type": "bar"});
                        base["encoding"] = serde_json::json!({
                            "x": x_encoding,
                            "x2": {"field": "x2"},
                            "y": y_encoding,
                            "color": {"field": "cat", "type": "nominal"},
                            "xOffset": {"field": "cat", "type": "nominal"}
                        });
                        return base;
                    }
                }
            }
        }
    }

    let mut values: Vec<serde_json::Value> = Vec::new();
    let mut keys: Vec<f64> = Vec::new();
    let x_value_type = x_meta
        .get("value_type")
        .and_then(|v| v.as_str())
        .unwrap_or("keyword");

    if let Some(arr) = raw_buckets.as_array() {
        if x_value_type == "keyword" {
            // Categorical histogram: emit one value per category with
            // a string `x` field and numeric `doc_count` so Vega-Lite can
            // render nominal bars with the server-provided tick order.
            for b in arr {
                let label = b
                    .get("label")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| b.get("key").and_then(|k| k.as_str().map(|s| s.to_string())))
                    .or_else(|| b.get("id").and_then(|k| k.as_str().map(|s| s.to_string())))
                    .unwrap_or_else(|| b.to_string());
                let count = b.get("doc_count").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let mut obj = serde_json::Map::new();
                obj.insert("x".to_string(), serde_json::Value::String(label.clone()));
                obj.insert("doc_count".to_string(), serde_json::Value::from(count));
                if let Some(kv) = b.get("key") {
                    obj.insert("key".to_string(), kv.clone());
                } else if let Some(idv) = b.get("id") {
                    obj.insert("id".to_string(), idv.clone());
                }
                values.push(serde_json::Value::Object(obj));
            }
        } else {
            for b in arr {
                if let Some(k) = b.get("key").and_then(|v| v.as_f64()) {
                    keys.push(k);
                }
            }
            // Determine bin width
            let width = if keys.len() >= 2 {
                keys[1] - keys[0]
            } else if let Some(domain_arr) = x_meta.get("domain").and_then(|d| d.as_array()) {
                if domain_arr.len() >= 2 {
                    let lo = domain_arr[0].as_f64().unwrap_or(0.0);
                    let hi = domain_arr[1].as_f64().unwrap_or(lo + 1.0);
                    let tick_count = x_meta
                        .get("tickCount")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(10) as f64;
                    (hi - lo) / tick_count.max(1.0)
                } else {
                    1.0
                }
            } else {
                1.0
            };

            for (i, b) in arr.iter().enumerate() {
                let key = b.get("key").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let right = if i + 1 < keys.len() {
                    keys[i + 1]
                } else {
                    key + width
                };
                let count = b.get("doc_count").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let mut obj = serde_json::Map::new();
                obj.insert("x".to_string(), serde_json::Value::from(key));
                obj.insert("x2".to_string(), serde_json::Value::from(right));
                obj.insert("doc_count".to_string(), serde_json::Value::from(count));
                // Preserve original key for backwards compatibility
                obj.insert("key".to_string(), serde_json::Value::from(key));
                values.push(serde_json::Value::Object(obj));
            }
        }
    }

    if x_value_type == "keyword" {
        // Extract category order from the buckets (labels) to use as tick values
        let mut cats: Vec<String> = Vec::new();
        if let Some(arr) = raw_buckets.as_array() {
            for b in arr {
                let label = b
                    .get("label")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| b.get("key").and_then(|k| k.as_str().map(|s| s.to_string())))
                    .or_else(|| b.get("id").and_then(|k| k.as_str().map(|s| s.to_string())))
                    .unwrap_or_default();
                cats.push(label);
            }
        }

        let x_meta_override = serde_json::json!({"tick_values": cats, "value_type": "keyword"});
        let x_encoding = match make_vl_axis_encoding(
            Some(&x_meta_override),
            "x",
            Some(x_label),
            None,
            true,
            Some(1),
        ) {
            Ok(v) => v,
            Err(e) => return serde_json::json!({"error": e}),
        };

        // Y axis encoding: doc_count; prefer quantitative with sensible domain
        let y_axis_meta = spec.get("y");
        let mut y_encoding =
            match make_vl_axis_encoding(y_axis_meta, "doc_count", Some(y_label), None, false, None)
            {
                Ok(v) => v,
                Err(e) => return serde_json::json!({"error": e}),
            };
        // Ensure y domain starts at zero for histograms
        if let Some(scale_obj) = y_encoding.get_mut("scale") {
            if scale_obj.is_object() {
                let max_val = values
                    .iter()
                    .filter_map(|o| o.get("doc_count").and_then(|v| v.as_f64()))
                    .fold(0.0_f64, |a, b| a.max(b));
                scale_obj.as_object_mut().unwrap().insert(
                    "domain".to_string(),
                    serde_json::json!([0.0, if max_val > 0.0 { max_val } else { 1.0 }]),
                );
            }
        }

        base["data"] = serde_json::json!({"values": values});
        base["mark"] = serde_json::json!({"type": "bar"});
        base["encoding"] = serde_json::json!({
            "x": x_encoding,
            "y": y_encoding,
            "y2": {"datum": y_min}
        });
        let _ = x_field;
        return base;
    }

    // Compute numeric boundaries (left edges + final right edge)
    let mut boundaries_f64: Vec<f64> = Vec::new();
    if !keys.is_empty() {
        for k in &keys {
            boundaries_f64.push(*k);
        }
        // final right
        let last_right = if keys.len() >= 2 {
            keys[keys.len() - 1] + (keys[1] - keys[0])
        } else {
            keys[0] + 1.0
        };
        boundaries_f64.push(last_right);
    }

    // X axis encoding: use server axis meta + computed boundaries
    let x_encoding = match make_vl_axis_encoding(
        spec.get("x"),
        "x",
        Some(x_label),
        Some(&boundaries_f64),
        false,
        Some(1),
    ) {
        Ok(v) => v,
        Err(e) => return serde_json::json!({"error": e}),
    };

    // Y axis encoding: doc_count; prefer quantitative with sensible domain
    let y_axis_meta = spec.get("y");
    let y_encoding =
        match make_vl_axis_encoding(y_axis_meta, "doc_count", Some(y_label), None, false, None) {
            Ok(v) => v,
            Err(e) => return serde_json::json!({"error": e}),
        };

    base["data"] = serde_json::json!({"values": values});
    base["mark"] = serde_json::json!({"type": "bar"});
    base["encoding"] = serde_json::json!({
        "x": x_encoding,
        "x2": {"field": "x2"},
        "y": y_encoding,
        "y2": {"datum": y_min}
    });
    let _ = x_field;
    base
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ArcDataEntry {
    report_label: String,
    value: f64,
    portion: f64,
    label: String,
    index: usize,
    initial_offset: f64,
    final_offset: f64,
    feature_count: usize,
    reference_count: usize,
    context_count: usize,
}

impl ArcDataEntry {
    fn from_report_data(
        report_data: &serde_json::Value,
        segment: usize,
        initial_offset: f64,
    ) -> Option<Self> {
        let report_label = report_data.get("report_label")?.as_str()?.to_string();
        let arc = report_data.get("arc")?.as_f64()?;
        let arc2 = 1.0 - arc;
        let mut offset = initial_offset;
        let (value, portion, label) = match segment {
            1 => {
                offset -= arc * std::f64::consts::PI; // shift back by arc portion
                (
                    report_data.get("feature_count")?.as_f64()?,
                    arc,
                    "feature".to_string(),
                )
            }
            2 => {
                offset -= arc2 * std::f64::consts::PI; // shift back by arc2 portion
                (
                    report_data.get("reference_count")?.as_f64()?,
                    arc2,
                    "reference".to_string(),
                )
            }
            // 3 => {
            //     offset -= std::f64::consts::PI; // shift back by full circle
            //     (
            //     report_data.get("context_count")?.as_f64()?,
            //     1.0, // context is the whole circle, so portion is 1.0
            //     "context".to_string(),
            // )},
            _ => return None,
        };
        let feature_count = report_data
            .get("feature_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let reference_count = report_data
            .get("reference_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        // let context_count = report_data.get("context_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        Some(Self {
            report_label,
            value,
            portion,
            label,
            index: segment - 1, // convert to 0-based index
            initial_offset,
            final_offset: offset,
            feature_count,
            reference_count,
            context_count: 0, // context_count,
        })
    }
}

/// Vega-Lite renderer for `arc_batch` PlotSpec data.
/// Produces a layered semicircular concentric ring chart coloured by series.
fn vl_arc_batch(spec: &serde_json::Value, mut base: serde_json::Value) -> serde_json::Value {
    let data_entries = spec
        .get("data")
        .and_then(|d| d.get("entries"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let n = data_entries.len() as f64;

    // Top-level display options (unused for now but kept for future tuning)
    let _display = spec.get("display").unwrap_or(&serde_json::Value::Null);
    let width = base.get("width").and_then(|v| v.as_u64()).unwrap_or(600) as f64;
    let height = base.get("height").and_then(|v| v.as_u64()).unwrap_or(400) as f64;

    // Unique series (report labels) in encountered order
    let mut labels = Vec::new();
    let mut entries = Vec::new();
    // loop through positions 1,2,3 for each entry to build ArcDataEntry structs
    for segment in 1..=3 {
        let mut offset = -std::f64::consts::PI / 2.0; // start at top of circle
        let mut segment_entries = Vec::new();
        for (i, e) in data_entries.iter().enumerate() {
            if segment > 1 {
                offset -= entries
                    .last()
                    .and_then(|seg_entries: &Vec<Option<ArcDataEntry>>| {
                        seg_entries[i].as_ref().map(|e| e.final_offset)
                    })
                    .unwrap_or(offset);
            }
            if let Some(lbl) = e.get("report_label").and_then(|v| v.as_str()) {
                if !labels.contains(&lbl.to_string()) {
                    labels.push(lbl.to_string());
                } else {
                    // already have this label, append a suffix to make it unique
                    let mut suffix = 2;
                    let mut new_label = format!("{} ({})", lbl, suffix);
                    while labels.contains(&new_label) {
                        suffix += 1;
                        new_label = format!("{} ({})", lbl, suffix);
                    }
                    labels.push(new_label);
                }
                let entry = ArcDataEntry::from_report_data(e, segment, offset);
                segment_entries.push(entry);
                if let Some(Some(e)) = segment_entries.last() {
                    offset = e.final_offset;
                }
            }
        }
        entries.push(segment_entries);
    }

    // radius allocation: leave small inner padding and outer padding
    let max_radius = (height.min(width) / 2.0) * 0.9;
    let inner_padding = 8.0;
    let slot = ((max_radius - inner_padding) / n).max(8.0);

    // Shared color encoding using a rainbow scheme
    let color_encoding = serde_json::json!({
        "field": "label",
        "type": "nominal",
        "scale": {"scheme": "rainbow"}
    });

    // Build layers: one arc per series
    let mut layers: Vec<serde_json::Value> = Vec::new();
    for (i, _lbl) in labels.iter().enumerate() {
        // collect entry belonging to this report index, filter out Nones
        let data: Vec<_> = entries
            .iter()
            .flat_map(|segment_entries| segment_entries.iter().filter_map(|e| e.as_ref()))
            .filter(|e| e.index == i) // segment index starts at 0
            .map(|e| {
                serde_json::json!({
                    "report_label": e.report_label,
                    "value": e.value,
                    "portion": e.portion,
                    "label": e.label,
                    "offset": e.initial_offset,
                    "index": e.index,
                    "feature_count": e.feature_count,
                    "reference_count": e.reference_count,
                    "context_count": e.context_count
                })
            })
            .collect();

        let inner = (inner_padding + (i as f64) * slot).round();
        let outer = (inner + slot * 0.8).round();

        let layer = serde_json::json!({
            "data": {"values": data},
            "mark": {
                "type": "arc",
                "innerRadius": inner,
                "outerRadius": outer,
                "thetaOffset": {"expr": "datum.offset"},
                "theta2Offset": {"expr": "datum.offset"}
            },
            "encoding": {
                "theta": {
                    "field": "portion",
                    "type": "quantitative",
                    "scale": {"domain": [0.0, 2.0]}
                },
                "theta2": {"value": 0},
                "color": color_encoding.clone()
            }
        });

        layers.push(layer);
    }

    base["layer"] = serde_json::Value::Array(layers);
    base
}

fn arc_values_from_report_data(report_data: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut entries = vec![];

    let feature_count = count_field(report_data, "feature_count");
    let reference_count = count_field(report_data, "reference_count");
    let context_count = count_field(report_data, "context_count");

    // add feature segment
    let feature_portion = if feature_count > 0.0 {
        let portion = if context_count > 0.0 {
            feature_count / context_count
        } else if reference_count > 0.0 {
            feature_count / reference_count
        } else {
            1.0
        };
        entries.push(serde_json::json!({
            "label": "feature",
            "value": feature_count,
            "portion": portion,
            "query_term": report_data.get("featureTerm").and_then(|v| v.as_str()).unwrap_or("")
        }));
        portion
    } else {
        0.0
    };

    // add reference segment
    let reference_portion = if reference_count > 0.0 {
        let portion = if context_count > 0.0 {
            (reference_count - feature_count) / context_count
        } else {
            1.0 - feature_portion
        };
        entries.push(serde_json::json!({
            "label": "reference",
            "value": reference_count,
            "portion": portion,
            "query_term": report_data.get("referenceTerm").and_then(|v| v.as_str()).unwrap_or("")
        }));
        portion
    } else {
        0.0
    };

    // add context segment if context_count is present and greater than feature+reference
    let context_portion = 1.0 - feature_portion - reference_portion;
    if context_portion > 0.0 {
        entries.push(serde_json::json!({
            "label": "context",
            "value": context_count,
            "portion": context_portion,
            "query_term": report_data.get("contextTerm").and_then(|v| v.as_str()).unwrap_or("")
        }));
    }
    entries
}

/// Donut for a single arc; a list under `arc` renders one ring per entry.
fn vl_arc(spec: &serde_json::Value, mut base: serde_json::Value) -> serde_json::Value {
    let Some(report_data) = spec.get("data") else {
        return serde_json::json!({"error": "No data found"});
    };
    if let Some(rings) = report_data.get("arc").and_then(|v| v.as_array()) {
        return vl_arc_rings(rings, base);
    }
    let width = base["width"].as_f64().unwrap_or(600.0);
    let height = base["height"].as_f64().unwrap_or(400.0);
    let outer_radius = width.min(height) / 2.0;
    let inner_radius = outer_radius * 0.5;
    let (headline, detail) = arc_centre_labels(report_data);
    let segments = arc_segment_angles(arc_values_from_report_data(report_data));
    let mid_radius = (outer_radius + inner_radius) / 2.0;
    let segment_font_size = outer_radius * 0.12;
    let headline_font_size = outer_radius * 0.2;
    let detail_font_size = outer_radius * 0.1;
    let segment_labels = arc_segment_label_values(&segments, mid_radius, segment_font_size);

    let donut_layer = serde_json::json!({
        "data": {"values": segments},
        "mark": {
            "type": "arc",
            "innerRadius": inner_radius,
            "outerRadius": outer_radius,
            "stroke": "#ffffff",
            "strokeWidth": 2
        },
        "encoding": {
            "theta": {"field": "start", "type": "quantitative", "scale": null},
            "theta2": {"field": "end", "type": "quantitative", "scale": null},
            "color": {
                "field": "label",
                "type": "nominal",
                "scale": {"scheme": "category10", "domain": ["feature", "reference", "context"]},
                "legend": null
            }
        }
    });
    let mut layers = vec![donut_layer];
    if let Some(cumulative) = arc_cumulative_fraction(report_data) {
        layers.push(cumulative_arc_layer(cumulative, inner_radius));
    }
    layers.push(segment_label_layer(
        segment_labels,
        mid_radius,
        segment_font_size,
    ));
    layers.push(centre_text_layer(
        &headline,
        width,
        height,
        -headline_font_size / 3.0,
        headline_font_size,
        "normal",
    ));
    layers.push(centre_text_layer(
        &detail,
        width,
        height,
        detail_font_size * 1.2,
        detail_font_size,
        "normal",
    ));

    base["view"] = serde_json::json!({"stroke": null});
    base["layer"] = serde_json::Value::Array(layers);
    base
}

/// Colour scheme for rank rings; change it to recolour the rings.
const RING_COLOUR_SCHEME: &str = "category10";

/// Neutral fill for the context span of each ring.
const RING_TRACK_FILL: &str = "#cccccc";

/// Inner radius of the innermost ring as a fraction of the outer radius.
const RING_HOLE_RATIO: f64 = 0.11;

/// Gap between adjacent ring bands as a fraction of the ring pitch.
const RING_GAP_RATIO: f64 = 0.15;

/// Gap between the arcs and the legend, as a fraction of the outer radius.
const RING_LEGEND_GAP: f64 = 0.08;

/// Height of one legend row as a fraction of the outer radius.
const RING_LEGEND_ROW_RATIO: f64 = 0.17;

/// Legend name text size as a fraction of the outer radius.
const RING_LEGEND_NAME_RATIO: f64 = 0.055;

/// Legend counts text size as a fraction of the outer radius.
const RING_LEGEND_COUNTS_RATIO: f64 = 0.045;

/// Legend bar width as a fraction of the outer radius.
const RING_LEGEND_BAR_RATIO: f64 = 0.024;

/// Legend text padding as a fraction of the outer radius.
const RING_LEGEND_PAD_RATIO: f64 = 0.02;

/// Average glyph width of legend text in em.
const RING_LEGEND_CHAR_EM: f64 = 0.55;

/// Cap height of legend text in em, used to centre the name line on its bar.
const RING_LEGEND_CAP_EM: f64 = 0.72;

/// Fill opacity of the reference-only band drawn over the track.
const RING_REFERENCE_OPACITY: f64 = 0.4;

/// Outline width shared by every ring arc so segment borders line up.
const RING_OUTLINE_WIDTH: f64 = 2.0;

/// Centre and radii of one ring band.
#[derive(Clone, Copy)]
struct RingBand {
    cx: f64,
    cy: f64,
    inner: f64,
    outer: f64,
}

/// Semicircular rainbow for an `arc` report list, first entry outermost.
fn vl_arc_rings(rings: &[serde_json::Value], mut base: serde_json::Value) -> serde_json::Value {
    let width = base["width"].as_f64().unwrap_or(600.0);
    let height = base["height"].as_f64().unwrap_or(400.0);
    let (outer_radius, columns) = ring_layout(width, height, rings);
    let pitch = ring_pitch(outer_radius, rings.len());
    let band = pitch * (1.0 - RING_GAP_RATIO);
    let domain: Vec<String> = rings.iter().map(ring_label).collect();

    let mut layers = Vec::new();
    let mut label_layers = Vec::new();
    for (index, ring) in rings.iter().enumerate() {
        let outer = outer_radius - index as f64 * pitch;
        let geometry = RingBand {
            cx: width / 2.0,
            cy: outer_radius,
            inner: outer - band,
            outer,
        };
        let (feature, reference) = ring_fractions(ring);
        layers.extend(ring_band_layers(
            ring, geometry, feature, reference, &domain,
        ));
        let mid = outer - band / 2.0;
        let percent = format!("{:.1}%", reference * 100.0);
        label_layers.push(positioned_text_layer(
            &percent,
            geometry.cx,
            geometry.cy - mid,
            band * 0.4,
        ));
    }
    layers.extend(label_layers);
    let items = ring_legend_items(rings, columns, width, outer_radius);
    layers.extend(ring_legend_layers(items, outer_radius, &domain));

    base["view"] = serde_json::json!({"stroke": null});
    base["layer"] = serde_json::Value::Array(layers);
    base
}

/// Distance between ring outer edges, sized so the innermost band ends at the hole.
fn ring_pitch(outer_radius: f64, ring_count: usize) -> f64 {
    outer_radius * (1.0 - RING_HOLE_RATIO) / (ring_count.max(1) as f64 - RING_GAP_RATIO)
}

/// Outer radius and legend columns, preferring the most columns that fit left of the arc's right edge.
fn ring_layout(width: f64, height: f64, rings: &[serde_json::Value]) -> (f64, usize) {
    let count = rings.len().max(1);
    let height_radius = |columns: usize| {
        let rows = count.div_ceil(columns) as f64;
        (width / 2.0).min(height / (1.0 + RING_LEGEND_GAP + rows * RING_LEGEND_ROW_RATIO))
    };
    let width_radius = |columns: usize| {
        let total_em: f64 = ring_legend_column_em(rings, columns).iter().sum();
        (width / 2.0) / (total_em - 1.0).max(f64::EPSILON)
    };
    let columns = (1..=count)
        .rev()
        .find(|&columns| height_radius(columns) <= width_radius(columns))
        .unwrap_or(1);
    (height_radius(columns).min(width_radius(columns)), columns)
}

/// Widest legend item in each grid column when items fill `columns` columns.
fn ring_legend_column_em(rings: &[serde_json::Value], columns: usize) -> Vec<f64> {
    let mut column_em = vec![0.0_f64; columns];
    for (index, ring) in rings.iter().enumerate() {
        let column = ring_legend_column(index, rings.len(), columns);
        column_em[column] = column_em[column].max(ring_legend_item_em(ring));
    }
    column_em
}

/// Grid column of legend item `index`, with the last row right-aligned.
fn ring_legend_column(index: usize, count: usize, columns: usize) -> usize {
    let row_start = index / columns * columns;
    let row_len = (count - row_start).min(columns);
    columns - row_len + index - row_start
}

/// Width of one legend item as a multiple of the outer radius, from its own text.
fn ring_legend_item_em(ring: &serde_json::Value) -> f64 {
    let name = ring_label(ring).chars().count() as f64 * RING_LEGEND_NAME_RATIO;
    let counts = ring_count_summary(ring).chars().count() as f64 * RING_LEGEND_COUNTS_RATIO;
    name.max(counts) * RING_LEGEND_CHAR_EM + 2.0 * RING_LEGEND_PAD_RATIO + RING_LEGEND_BAR_RATIO
}

/// Series label of a ring entry.
fn ring_label(ring: &serde_json::Value) -> String {
    ring.get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Feature share of a ring entry, clamped to the full circle.
fn ring_feature_share(ring: &serde_json::Value) -> f64 {
    ring.get("arc")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
        .clamp(0.0, 1.0)
}

/// Feature and reference fractions of a ring's semicircle, measured against context when present.
fn ring_fractions(ring: &serde_json::Value) -> (f64, f64) {
    let context = count_field(ring, "context_count");
    if context > 0.0 {
        return (
            count_field(ring, "feature_count") / context,
            count_field(ring, "reference_count") / context,
        );
    }
    let share = ring_feature_share(ring);
    (share, share)
}

/// Track, reference-only band, and feature arc for one ring, in draw order.
fn ring_band_layers(
    ring: &serde_json::Value,
    band: RingBand,
    feature: f64,
    reference: f64,
    domain: &[String],
) -> Vec<serde_json::Value> {
    let label = ring_label(ring);
    let colour = ring_colour_encoding(domain);
    let track = ring_arc_layer(
        serde_json::json!([{"start": semicircle_angle(0.0), "end": semicircle_angle(1.0)}]),
        band,
        serde_json::json!({}),
        serde_json::json!({"color": {"value": RING_TRACK_FILL}, "stroke": {"value": RING_TRACK_FILL}}),
    );
    let mut layers = vec![track];
    if reference > feature {
        layers.push(ring_arc_layer(
            serde_json::json!([{
                "label": label,
                "start": semicircle_angle(feature),
                "end": semicircle_angle(reference)
            }]),
            band,
            serde_json::json!({"fillOpacity": RING_REFERENCE_OPACITY}),
            serde_json::json!({"color": colour.clone(), "stroke": colour.clone()}),
        ));
    }
    if feature > 0.0 {
        layers.push(ring_arc_layer(
            serde_json::json!([{
                "label": label,
                "start": semicircle_angle(0.0),
                "end": semicircle_angle(feature)
            }]),
            band,
            serde_json::json!({}),
            serde_json::json!({"color": colour.clone(), "stroke": colour}),
        ));
    }
    layers
}

/// Canvas angle in radians for a fraction of the top semicircle, starting at 9 o'clock.
fn semicircle_angle(fraction: f64) -> f64 {
    -std::f64::consts::FRAC_PI_2 + fraction * std::f64::consts::PI
}

/// Colour channel that maps ring labels through the ring colour scheme.
fn ring_colour_encoding(domain: &[String]) -> serde_json::Value {
    serde_json::json!({
        "field": "label",
        "type": "nominal",
        "legend": null,
        "scale": {"scheme": RING_COLOUR_SCHEME, "domain": domain}
    })
}

/// Arc layer across a semicircular band; `style` sets mark fields and `encodings` adds channels.
fn ring_arc_layer(
    values: serde_json::Value,
    band: RingBand,
    style: serde_json::Value,
    encodings: serde_json::Value,
) -> serde_json::Value {
    let mut mark = serde_json::json!({
        "type": "arc",
        "innerRadius": band.inner,
        "outerRadius": band.outer,
        "strokeWidth": RING_OUTLINE_WIDTH
    });
    merge_fields(&mut mark, style);
    let mut encoding = serde_json::json!({
        "x": {"value": band.cx},
        "y": {"value": band.cy},
        "theta": {"field": "start", "type": "quantitative", "scale": null},
        "theta2": {"field": "end", "type": "quantitative", "scale": null}
    });
    merge_fields(&mut encoding, encodings);
    serde_json::json!({"data": {"values": values}, "mark": mark, "encoding": encoding})
}

/// Copies the object fields of `extra` into `target`.
fn merge_fields(target: &mut serde_json::Value, extra: serde_json::Value) {
    if let (Some(target), serde_json::Value::Object(extra)) = (target.as_object_mut(), extra) {
        target.extend(extra);
    }
}

/// Text layer at fixed pixel coordinates, vertically centred on the anchor.
fn positioned_text_layer(text: &str, x: f64, y: f64, font_size: f64) -> serde_json::Value {
    serde_json::json!({
        "data": {"values": [{}]},
        "mark": {
            "type": "text",
            "fontSize": font_size,
            "fontWeight": "bold",
            "color": "#ffffff",
            "baseline": "middle"
        },
        "encoding": {
            "text": {"value": text},
            "x": {"value": x},
            "y": {"value": y}
        }
    })
}

/// Legend items on a column-aligned grid whose right edge meets the outer arc's right edge.
fn ring_legend_items(
    rings: &[serde_json::Value],
    columns: usize,
    width: f64,
    outer_radius: f64,
) -> Vec<serde_json::Value> {
    let pad = outer_radius * RING_LEGEND_PAD_RATIO;
    let bar = outer_radius * RING_LEGEND_BAR_RATIO;
    let name_size = outer_radius * RING_LEGEND_NAME_RATIO;
    let counts_size = outer_radius * RING_LEGEND_COUNTS_RATIO;
    let row_height = outer_radius * RING_LEGEND_ROW_RATIO;
    let top = outer_radius * (1.0 + RING_LEGEND_GAP);
    let bar_height = name_size + counts_size * 1.7;
    let text_height = name_size * RING_LEGEND_CAP_EM + counts_size * 1.4;
    let text_inset = (bar_height - text_height) / 2.0;
    let column_em = ring_legend_column_em(rings, columns);
    let grid_right = width / 2.0 + outer_radius + pad;
    let mut column_left = Vec::with_capacity(columns);
    let mut left = grid_right - column_em.iter().sum::<f64>() * outer_radius;
    for em in &column_em {
        column_left.push(left);
        left += em * outer_radius;
    }
    rings
        .iter()
        .enumerate()
        .map(|(index, ring)| {
            let column = ring_legend_column(index, rings.len(), columns);
            let row_top = top + (index / columns) as f64 * row_height;
            let bar_x1 = column_left[column] + column_em[column] * outer_radius - pad;
            let bar_x0 = bar_x1 - bar;
            let name_y = row_top + text_inset + name_size * RING_LEGEND_CAP_EM;
            let counts_y = name_y + counts_size * 1.4;
            serde_json::json!({
                "label": ring_label(ring),
                "counts": ring_count_summary(ring),
                "bar_x0": bar_x0,
                "bar_x1": bar_x1,
                "bar_y0": row_top,
                "bar_y1": row_top + bar_height,
                "text_x": bar_x0 - pad,
                "name_y": name_y,
                "counts_y": counts_y
            })
        })
        .collect()
}

/// Legend under the arcs: rank name and counts right-aligned to a colour bar per item.
fn ring_legend_layers(
    items: Vec<serde_json::Value>,
    outer_radius: f64,
    domain: &[String],
) -> Vec<serde_json::Value> {
    let name_size = outer_radius * RING_LEGEND_NAME_RATIO;
    let counts_size = outer_radius * RING_LEGEND_COUNTS_RATIO;
    let colour = ring_colour_encoding(domain);
    let text_x = serde_json::json!({"field": "text_x", "type": "quantitative", "scale": null});
    vec![
        serde_json::json!({
            "data": {"values": items.clone()},
            "mark": {"type": "rect"},
            "encoding": {
                "x": {"field": "bar_x0", "type": "quantitative", "scale": null},
                "x2": {"field": "bar_x1", "type": "quantitative", "scale": null},
                "y": {"field": "bar_y0", "type": "quantitative", "scale": null},
                "y2": {"field": "bar_y1", "type": "quantitative", "scale": null},
                "color": colour.clone()
            }
        }),
        serde_json::json!({
            "data": {"values": items.clone()},
            "mark": {"type": "text", "align": "right", "baseline": "alphabetic", "fontSize": name_size, "fontWeight": "bold"},
            "encoding": {
                "x": text_x.clone(),
                "y": {"field": "name_y", "type": "quantitative", "scale": null},
                "text": {"field": "label", "type": "nominal"},
                "color": colour
            }
        }),
        serde_json::json!({
            "data": {"values": items},
            "mark": {"type": "text", "align": "right", "baseline": "alphabetic", "fontSize": counts_size, "color": "#666666"},
            "encoding": {
                "x": text_x,
                "y": {"field": "counts_y", "type": "quantitative", "scale": null},
                "text": {"field": "counts", "type": "nominal"}
            }
        }),
    ]
}

/// Feature, reference, and context counts of a ring as compact text.
fn ring_count_summary(ring: &serde_json::Value) -> String {
    let mut counts = vec![
        count_field(ring, "feature_count"),
        count_field(ring, "reference_count"),
    ];
    let context = count_field(ring, "context_count");
    if context > 0.0 {
        counts.push(context);
    }
    counts
        .into_iter()
        .map(format_count_compact)
        .collect::<Vec<_>>()
        .join(" / ")
}

/// Count with at most three significant figures and a k or M suffix, e.g. `12583` as `12.6k`.
fn format_count_compact(value: f64) -> String {
    let (scaled, suffix) = if value >= 1_000_000.0 {
        (value / 1_000_000.0, "M")
    } else if value >= 1_000.0 {
        (value / 1_000.0, "k")
    } else {
        return format!("{value:.0}");
    };
    let decimals = if scaled >= 100.0 {
        0
    } else if scaled >= 10.0 {
        1
    } else {
        2
    };
    format!("{scaled:.decimals$}{suffix}")
}

/// Clockwise start, end and mid angles in radians from 12 o'clock for each segment.
fn arc_segment_angles(mut segments: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    let mut start = 0.0;
    for segment in &mut segments {
        let end = start + segment["portion"].as_f64().unwrap_or(0.0) * std::f64::consts::TAU;
        segment["start"] = serde_json::json!(start);
        segment["end"] = serde_json::json!(end);
        segment["mid"] = serde_json::json!((start + end) / 2.0);
        start = end;
    }
    segments
}

/// Approximate glyph width in em, used to test whether a segment label fits.
const ARC_LABEL_CHAR_EM: f64 = 0.6;

/// Percentage labels for segments whose arc length at `mid_radius` fits the label text.
fn arc_segment_label_values(
    segments: &[serde_json::Value],
    mid_radius: f64,
    font_size: f64,
) -> Vec<serde_json::Value> {
    segments
        .iter()
        .filter_map(|segment| {
            let portion = segment["portion"].as_f64()?;
            let text = format!("{:.1}%", portion * 100.0);
            let arc_length = portion * std::f64::consts::TAU * mid_radius;
            (arc_length >= text.len() as f64 * font_size * ARC_LABEL_CHAR_EM)
                .then(|| serde_json::json!({"text": text, "mid": segment["mid"]}))
        })
        .collect()
}

/// Text layer placing segment labels on `mid_radius` at each segment's mid angle.
fn segment_label_layer(
    labels: Vec<serde_json::Value>,
    mid_radius: f64,
    font_size: f64,
) -> serde_json::Value {
    serde_json::json!({
        "data": {"values": labels},
        "mark": {"type": "text", "fontSize": font_size, "fontWeight": "normal", "color": "#ffffff"},
        "encoding": {
            "theta": {"field": "mid", "type": "quantitative", "scale": null},
            "radius": {"value": mid_radius},
            "text": {"field": "text", "type": "nominal"}
        }
    })
}

/// Share of context covered by reference (feature and reference-only together); `None` without context.
fn arc_cumulative_fraction(report_data: &serde_json::Value) -> Option<f64> {
    let context = count_field(report_data, "context_count");
    (context > 0.0).then(|| count_field(report_data, "reference_count") / context)
}

/// Thin arc sweeping `fraction` of the circle from 12 o'clock, just inside the donut hole.
fn cumulative_arc_layer(fraction: f64, inner_radius: f64) -> serde_json::Value {
    let end_angle = fraction * std::f64::consts::TAU;
    serde_json::json!({
        "data": {"values": [{"start": 0.0, "end": end_angle}]},
        "mark": {
            "type": "arc",
            "innerRadius": inner_radius - 7.0,
            "outerRadius": inner_radius - 4.0,
            "fill": "#2f3140"
        },
        "encoding": {
            "theta": {"field": "start", "type": "quantitative", "scale": null},
            "theta2": {"field": "end", "type": "quantitative", "scale": null}
        }
    })
}

/// Headline percentage and count label shown in the donut centre.
fn arc_centre_labels(report_data: &serde_json::Value) -> (String, String) {
    if let Some(cumulative) = arc_cumulative_fraction(report_data) {
        let reference = count_field(report_data, "reference_count");
        let context = count_field(report_data, "context_count");
        return (
            format!("{:.1}%", cumulative * 100.0),
            format!("{} / {}", format_count(reference), format_count(context)),
        );
    }
    let feature = count_field(report_data, "feature_count");
    let reference = count_field(report_data, "reference_count");
    let fraction = if reference > 0.0 {
        feature / reference
    } else {
        0.0
    };
    (
        format!("{:.1}%", fraction * 100.0),
        format!("{} / {}", format_count(feature), format_count(reference)),
    )
}

fn count_field(report_data: &serde_json::Value, key: &str) -> f64 {
    report_data.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)
}

/// Formats a count with comma thousands separators, e.g. `10384` as `10,384`.
fn format_count(value: f64) -> String {
    let digits = format!("{:.0}", value);
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// A single-line text layer centred in the view, shifted vertically by `dy` pixels.
fn centre_text_layer(
    text: &str,
    width: f64,
    height: f64,
    dy: f64,
    font_size: f64,
    font_weight: &str,
) -> serde_json::Value {
    serde_json::json!({
        "data": {"values": [{"text": text}]},
        "mark": {"type": "text", "dy": dy, "fontSize": font_size, "fontWeight": font_weight},
        "encoding": {
            "text": {"field": "text", "type": "nominal"},
            "x": {"value": width / 2.0},
            "y": {"value": height / 2.0}
        }
    })
}

fn vl_scatter(spec: &serde_json::Value, mut base: serde_json::Value) -> serde_json::Value {
    let x_meta = spec.get("x").unwrap_or(&serde_json::Value::Null);
    let x_field = x_meta.get("field").and_then(|v| v.as_str()).unwrap_or("x");
    let x_label = x_meta
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or(x_field);

    let y_meta = spec.get("y").unwrap_or(&serde_json::Value::Null);
    let y_field = y_meta.get("field").and_then(|v| v.as_str()).unwrap_or("y");
    let y_label = y_meta
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or(y_field);

    // Extract optional server-provided buckets. Support two shapes:
    // 1) legacy: `buckets` is an array of primitive ids (strings/numbers) and
    //    `bucketLabels` may be present as a parallel array of labels.
    // 2) structured: `buckets` is an array of objects `{id,label,count}`.
    let bucket_labels_opt: Option<Vec<String>>;
    let mut bucket_ids_opt: Option<Vec<String>> = None;

    if let Some(buckets_arr) = spec
        .get("data")
        .and_then(|d| d.get("buckets"))
        .and_then(|v| v.as_array())
        .cloned()
    {
        if !buckets_arr.is_empty() && buckets_arr[0].is_object() {
            // structured array of objects
            let mut ids: Vec<String> = Vec::new();
            let mut labels: Vec<String> = Vec::new();
            for obj in &buckets_arr {
                if let Some(idv) = obj.get("id").or_else(|| obj.get("key")) {
                    if let Some(s) = idv.as_str() {
                        ids.push(s.to_string());
                    } else {
                        ids.push(idv.to_string());
                    }
                } else {
                    ids.push(obj.to_string());
                }
                if let Some(lv) = obj.get("label").or_else(|| obj.get("name")) {
                    if let Some(s) = lv.as_str() {
                        labels.push(s.to_string());
                    } else {
                        labels.push(lv.to_string());
                    }
                } else {
                    labels.push(String::new());
                }
            }
            bucket_ids_opt = Some(ids);
            bucket_labels_opt = Some(labels);
        } else {
            // legacy primitive array
            bucket_ids_opt = Some(
                buckets_arr
                    .iter()
                    .map(|k| {
                        if let Some(s) = k.as_str() {
                            s.to_string()
                        } else {
                            k.to_string()
                        }
                    })
                    .collect(),
            );
            // try separate `bucketLabels` field as fallback but treat empty
            // arrays or arrays of empty strings as absent.
            bucket_labels_opt = spec
                .get("data")
                .and_then(|d| d.get("bucketLabels"))
                .and_then(|v| v.as_array())
                .and_then(|arr| {
                    let vec: Vec<String> = arr
                        .iter()
                        .map(|s| s.as_str().unwrap_or("").to_string())
                        .collect();
                    if vec.iter().all(|s| s.is_empty()) {
                        None
                    } else {
                        Some(vec)
                    }
                });
        }
    } else {
        // no buckets array at all; attempt to read `bucketLabels` only
        bucket_labels_opt = spec
            .get("data")
            .and_then(|d| d.get("bucketLabels"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|s| s.as_str().unwrap_or("").to_string())
                    .collect()
            });
    }

    // Build id->label map when both arrays are present and aligned.
    let id_to_label: Option<std::collections::HashMap<String, String>> =
        if let (Some(ids), Some(labels)) = (&bucket_ids_opt, &bucket_labels_opt) {
            if ids.len() == labels.len() {
                let mut m = std::collections::HashMap::new();
                for (i, id) in ids.iter().enumerate() {
                    m.insert(id.clone(), labels[i].clone());
                }
                Some(m)
            } else {
                None
            }
        } else {
            None
        };

    // Build y id->label map from `yBuckets` + `yBucketLabels` when available.
    let y_id_to_label: Option<std::collections::HashMap<String, String>> = {
        let y_ids_opt: Option<Vec<String>> = spec
            .get("data")
            .and_then(|d| d.get("yBuckets"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|k| k.as_str().unwrap_or(&k.to_string()).to_string())
                    .collect()
            });
        let y_labels_opt: Option<Vec<String>> = spec
            .get("data")
            .and_then(|d| d.get("yBucketLabels"))
            .and_then(|v| v.as_array())
            .and_then(|arr| {
                let vec: Vec<String> = arr
                    .iter()
                    .map(|s| s.as_str().unwrap_or("").to_string())
                    .collect();
                if vec.iter().all(|s| s.is_empty()) {
                    None
                } else {
                    Some(vec)
                }
            });

        if let (Some(ids), Some(labels)) = (y_ids_opt, y_labels_opt) {
            if ids.len() == labels.len() {
                let mut m = std::collections::HashMap::new();
                for (i, id) in ids.iter().enumerate() {
                    m.insert(id.clone(), labels[i].clone());
                }
                Some(m)
            } else {
                None
            }
        } else {
            None
        }
    };

    let cells = if let Some(existing_cells) = spec.get("data").and_then(|d| d.get("cells")) {
        existing_cells.clone()
    } else {
        // API scatter responses provide raw points grouped by category under
        // data.rawData.{cat}[]; flatten them into a single values array.
        let mut flattened: Vec<serde_json::Value> = Vec::new();
        if let Some(raw_data_obj) = spec
            .get("data")
            .and_then(|d| d.get("rawData"))
            .and_then(|v| v.as_object())
        {
            for (cat_key, points) in raw_data_obj {
                if let Some(point_arr) = points.as_array() {
                    for point in point_arr {
                        let mut point_obj = point.as_object().cloned().unwrap_or_default();
                        if !point_obj.contains_key("cat") {
                            point_obj.insert(
                                "cat".to_string(),
                                serde_json::Value::String(cat_key.clone()),
                            );
                        }
                        // If we have an id->label map, attach an `x_label` field
                        // for this point so categorical axes can display
                        // human-readable labels while preserving ids.
                        if let Some(map) = id_to_label.as_ref() {
                            // find a candidate id on the point
                            let mut key_opt: Option<String> = None;
                            if let Some(s) = point_obj.get("x").and_then(|v| v.as_str()) {
                                key_opt = Some(s.to_string());
                            } else if let Some(s) = point_obj.get("cat").and_then(|v| v.as_str()) {
                                key_opt = Some(s.to_string());
                            } else if let Some(n) =
                                point_obj.get("taxonId").and_then(|v| v.as_i64())
                            {
                                key_opt = Some(n.to_string());
                            } else if let Some(s) =
                                point_obj.get("taxonId").and_then(|v| v.as_str())
                            {
                                key_opt = Some(s.to_string());
                            }
                            if let Some(k) = key_opt {
                                if let Some(lbl) = map.get(&k) {
                                    point_obj.insert(
                                        "x_label".to_string(),
                                        serde_json::Value::String(lbl.clone()),
                                    );
                                }
                            }

                            // Populate `y_label` when possible so categorical Y
                            // encodings that expect `y_label` find a value.
                            if !point_obj.contains_key("y_label") {
                                // If we have a y id->label map prefer that.
                                if let Some(y_map) = y_id_to_label.as_ref() {
                                    let mut y_key_opt: Option<String> = None;
                                    if let Some(s) = point_obj.get("y").and_then(|v| v.as_str()) {
                                        y_key_opt = Some(s.to_string());
                                    } else if let Some(n) =
                                        point_obj.get("y").and_then(|v| v.as_i64())
                                    {
                                        y_key_opt = Some(n.to_string());
                                    }
                                    if let Some(yk) = y_key_opt {
                                        if let Some(y_lbl) = y_map.get(&yk) {
                                            point_obj.insert(
                                                "y_label".to_string(),
                                                serde_json::Value::String(y_lbl.clone()),
                                            );
                                        } else {
                                            // Fall back to copying the existing `y` string
                                            point_obj.insert(
                                                "y_label".to_string(),
                                                serde_json::Value::String(yk),
                                            );
                                        }
                                    }
                                } else {
                                    // No mapping available: if `y` is already a string
                                    // copy it to `y_label` so encoders using that
                                    // field render correctly.
                                    if let Some(s) = point_obj.get("y").and_then(|v| v.as_str()) {
                                        point_obj.insert(
                                            "y_label".to_string(),
                                            serde_json::Value::String(s.to_string()),
                                        );
                                    } else if let Some(n) =
                                        point_obj.get("y").and_then(|v| v.as_i64())
                                    {
                                        point_obj.insert(
                                            "y_label".to_string(),
                                            serde_json::Value::String(n.to_string()),
                                        );
                                    }
                                }
                            }
                        }
                        flattened.push(serde_json::Value::Object(point_obj));
                    }
                }
            }
        }
        serde_json::Value::Array(flattened)
    };

    // Pre-compute boundaries or category labels from buckets so tick marks can
    // be applied even when raw point `cells` are present. We handle numeric
    // and string buckets differently: numeric buckets yield numeric
    // boundaries; string buckets yield categorical tick values.
    let mut x_boundaries_f64: Vec<f64> = Vec::new();
    let mut y_boundaries_f64: Vec<f64> = Vec::new();
    let mut x_categories: Option<Vec<String>> = None;
    let mut y_categories: Option<Vec<String>> = None;

    if let Some(x_keys_arr) = spec
        .get("data")
        .and_then(|d| d.get("buckets"))
        .and_then(|v| v.as_array())
        .cloned()
    {
        // If the buckets are structured objects, prefer the extracted ids
        // from `bucket_ids_opt`. Otherwise fall back to primitive handling.
        if !x_keys_arr.is_empty() && x_keys_arr[0].is_object() {
            // Structured buckets (objects) are typed by server-provided
            // axis metadata. Require `value_type` to decide how to treat ids.
            if let Some(ids) = bucket_ids_opt.clone() {
                match x_meta.get("value_type").and_then(|v| v.as_str()) {
                    Some("float") | Some("integer") | Some("date") | Some("coordinate") => {
                        let x_keys_num: Vec<f64> = ids
                            .iter()
                            .map(|s| s.parse::<f64>().unwrap_or(0.0))
                            .collect();
                        // Determine bin width
                        let width = if x_keys_num.len() >= 2 {
                            x_keys_num[1] - x_keys_num[0]
                        } else if let Some(domain_arr) =
                            x_meta.get("domain").and_then(|d| d.as_array())
                        {
                            if domain_arr.len() >= 2 {
                                let lo = domain_arr[0].as_f64().unwrap_or(0.0);
                                let hi = domain_arr[1].as_f64().unwrap_or(lo + 1.0);
                                let tick_count = x_meta
                                    .get("tickCount")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(10)
                                    as f64;
                                (hi - lo) / tick_count.max(1.0)
                            } else {
                                1.0
                            }
                        } else {
                            1.0
                        };

                        for k in &x_keys_num {
                            x_boundaries_f64.push(*k);
                        }
                        let last_right = if x_keys_num.len() >= 2 {
                            x_keys_num[x_keys_num.len() - 1] + (x_keys_num[1] - x_keys_num[0])
                        } else {
                            x_keys_num[0] + width
                        };
                        x_boundaries_f64.push(last_right);
                    }
                    Some("keyword") => {
                        x_categories = Some(ids);
                    }
                    Some(other) => {
                        return serde_json::json!({"error": format!("unsupported axis value_type '{}' for x buckets", other)});
                    }
                    None => {
                        return serde_json::json!({"error": "missing axis value_type for x buckets; server must provide axis.value_type"});
                    }
                }
            } else {
                // No extracted ids available; stringify structured objects into labels
                x_categories = Some(
                    x_keys_arr
                        .iter()
                        .map(|o| match o.get("id").or_else(|| o.get("key")) {
                            Some(idv) => {
                                if let Some(s) = idv.as_str() {
                                    s.to_string()
                                } else {
                                    idv.to_string()
                                }
                            }
                            None => match o.get("label").or_else(|| o.get("name")) {
                                Some(lv) => {
                                    if let Some(s) = lv.as_str() {
                                        s.to_string()
                                    } else {
                                        lv.to_string()
                                    }
                                }
                                None => o.to_string(),
                            },
                        })
                        .collect(),
                );
            }
        } else {
            // For primitive arrays, require server-provided type information.
            match x_meta.get("value_type").and_then(|v| v.as_str()) {
                Some("keyword") => {
                    x_categories = Some(
                        x_keys_arr
                            .iter()
                            .map(|k| k.as_str().unwrap_or("").to_string())
                            .collect(),
                    );
                }
                Some("float") | Some("integer") | Some("number") | Some("date")
                | Some("coordinate") => {
                    // parse values to f64 as needed
                    let to_f64 = |v: &serde_json::Value| -> f64 {
                        v.as_f64()
                            .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
                            .unwrap_or(0.0)
                    };
                    let x_keys: Vec<f64> = x_keys_arr.iter().map(to_f64).collect();
                    if !x_keys.is_empty() {
                        let width = if x_keys.len() >= 2 {
                            x_keys[1] - x_keys[0]
                        } else if let Some(domain_arr) = spec
                            .get("x")
                            .and_then(|x| x.get("domain"))
                            .and_then(|d| d.as_array())
                        {
                            if domain_arr.len() >= 2 {
                                let lo = domain_arr[0].as_f64().unwrap_or(0.0);
                                let hi = domain_arr[1].as_f64().unwrap_or(lo + 1.0);
                                let tick_count =
                                    spec.get("x")
                                        .and_then(|x| x.get("tickCount"))
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(10) as f64;
                                (hi - lo) / tick_count.max(1.0)
                            } else {
                                1.0
                            }
                        } else {
                            1.0
                        };

                        for k in &x_keys {
                            x_boundaries_f64.push(*k);
                        }
                        let last_right = if x_keys.len() >= 2 {
                            x_keys[x_keys.len() - 1] + (x_keys[1] - x_keys[0])
                        } else {
                            x_keys[0] + width
                        };
                        x_boundaries_f64.push(last_right);
                    }
                }
                Some(other) => {
                    return serde_json::json!({"error": format!("unsupported axis value_type '{}' for x primitive buckets", other)});
                }
                None => {
                    return serde_json::json!({"error": "missing axis value_type for x primitive buckets; server must provide axis.value_type"});
                }
            }
        }
    }

    if let Some(y_keys_arr) = spec
        .get("data")
        .and_then(|d| d.get("yBuckets"))
        .and_then(|v| v.as_array())
        .cloned()
    {
        // Prefer explicit label array when provided by the server. This
        // keeps `yBuckets` as the canonical ids used for bin alignment and
        // uses `yBucketLabels` for human-readable axis categories.
        let y_labels_opt: Option<Vec<String>> = spec
            .get("data")
            .and_then(|d| d.get("yBucketLabels"))
            .and_then(|v| v.as_array())
            .and_then(|arr| {
                let vec: Vec<String> = arr
                    .iter()
                    .map(|s| s.as_str().unwrap_or("").to_string())
                    .collect();
                if vec.iter().all(|s| s.is_empty()) {
                    None
                } else {
                    Some(vec)
                }
            });

        // Require server-provided `value_type` for the Y axis as well.
        match y_meta.get("value_type").and_then(|v| v.as_str()) {
            Some("keyword") => {
                if let Some(lbls) = y_labels_opt {
                    y_categories = Some(lbls);
                } else {
                    y_categories = Some(
                        y_keys_arr
                            .iter()
                            .map(|k| k.as_str().unwrap_or("").to_string())
                            .collect(),
                    );
                }
            }
            Some("float") | Some("integer") | Some("number") | Some("date")
            | Some("coordinate") => {
                let to_f64 = |v: &serde_json::Value| -> f64 {
                    v.as_f64()
                        .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
                        .unwrap_or(0.0)
                };
                let y_keys: Vec<f64> = y_keys_arr.iter().map(to_f64).collect();
                if !y_keys.is_empty() {
                    let height = if y_keys.len() >= 2 {
                        y_keys[1] - y_keys[0]
                    } else if let Some(domain_arr) = spec
                        .get("y")
                        .and_then(|y| y.get("domain"))
                        .and_then(|d| d.as_array())
                    {
                        if domain_arr.len() >= 2 {
                            let lo = domain_arr[0].as_f64().unwrap_or(0.0);
                            let hi = domain_arr[1].as_f64().unwrap_or(lo + 1.0);
                            let tick_count = spec
                                .get("y")
                                .and_then(|y| y.get("tickCount"))
                                .and_then(|v| v.as_u64())
                                .unwrap_or(10) as f64;
                            (hi - lo) / tick_count.max(1.0)
                        } else {
                            1.0
                        }
                    } else {
                        1.0
                    };

                    for k in &y_keys {
                        y_boundaries_f64.push(*k);
                    }
                    let last_top = if y_keys.len() >= 2 {
                        y_keys[y_keys.len() - 1] + (y_keys[1] - y_keys[0])
                    } else {
                        y_keys[0] + height
                    };
                    y_boundaries_f64.push(last_top);
                }
            }
            Some(other) => {
                return serde_json::json!({"error": format!("unsupported axis value_type '{}' for y buckets", other)});
            }
            None => {
                return serde_json::json!({"error": "missing axis value_type for y buckets; server must provide axis.value_type"});
            }
        }
    }

    // If we have point data (cells/rawData) render points, otherwise check for
    // binned 2D data (`allYValues` + `yBuckets`) and render as a heatmap rect grid.
    let mut is_cells_empty = true;
    if let serde_json::Value::Array(arr) = &cells {
        is_cells_empty = arr.is_empty();
    }

    if !is_cells_empty {
        // Use shared axis encoding that respects server-provided axis meta.
        // Prefer computed bin boundaries (if we extracted them above) so tick
        // marks align with histogram/scatter bin edges even when raw points
        // (`cells`) are present.
        // If we have categorical bucket labels, prefer them for axis ticks.
        let mut x_meta_override_value: Option<serde_json::Value> = None;
        if let Some(ref cats) = x_categories {
            // Prefer server-provided human-readable bucket labels when present.
            if let Some(ref labels) = bucket_labels_opt {
                x_meta_override_value =
                    Some(serde_json::json!({"tick_values": labels, "value_type": "keyword"}));
            } else {
                x_meta_override_value =
                    Some(serde_json::json!({"tick_values": cats, "value_type": "keyword"}));
            }
        }
        let x_enc = if let Some(ref meta) = x_meta_override_value {
            // When using human-readable labels, the data objects will include
            // an `x_label` field; use that field for axis encoding so labels
            // render in the intended order.
            let enc_res = if bucket_labels_opt.is_some() {
                make_vl_axis_encoding(Some(meta), "x_label", Some(x_label), None, true, None)
            } else {
                make_vl_axis_encoding(Some(meta), "x", Some(x_label), None, true, None)
            };
            match enc_res {
                Ok(v) => v,
                Err(e) => return serde_json::json!({"error": e}),
            }
        } else {
            let x_bound_opt: Option<&[f64]> = if x_boundaries_f64.is_empty() {
                None
            } else {
                Some(x_boundaries_f64.as_slice())
            };
            match make_vl_axis_encoding(spec.get("x"), "x", Some(x_label), x_bound_opt, false, None)
            {
                Ok(v) => v,
                Err(e) => return serde_json::json!({"error": e}),
            }
        };

        let mut y_meta_override_value: Option<serde_json::Value> = None;
        if let Some(ref cats) = y_categories {
            // Use the y bucket categories for y-axis tick values. Do NOT reuse
            // `bucket_labels_opt` which holds labels for the x-axis buckets.
            y_meta_override_value =
                Some(serde_json::json!({"tick_values": cats, "value_type": "keyword"}));
        }
        let y_enc = if let Some(ref meta) = y_meta_override_value {
            match make_vl_axis_encoding(Some(meta), "y_label", Some(y_label), None, true, None) {
                Ok(v) => v,
                Err(e) => return serde_json::json!({"error": e}),
            }
        } else {
            let y_bound_opt: Option<&[f64]> = if y_boundaries_f64.is_empty() {
                None
            } else {
                Some(y_boundaries_f64.as_slice())
            };
            match make_vl_axis_encoding(spec.get("y"), "y", Some(y_label), y_bound_opt, false, None)
            {
                Ok(v) => v,
                Err(e) => return serde_json::json!({"error": e}),
            }
        };

        base["data"] = serde_json::json!({"values": cells});
        base["mark"] = serde_json::Value::String("point".to_string());

        // Build encoding map and add jitter offsets when axes are categorical.
        let mut encoding_map = serde_json::Map::new();
        encoding_map.insert("x".to_string(), x_enc);
        encoding_map.insert("y".to_string(), y_enc);

        let mut transforms: Vec<serde_json::Value> = Vec::new();
        // Add a small pixel-offset jitter for categorical axes using Vega's
        // `random()` expression. Offsets are in pixels and encoded via
        // `xOffset`/`yOffset` which Vega-Lite supports for point marks.
        if x_categories.is_some() {
            transforms.push(serde_json::json!({"calculate": "(random()-0.5) * (random()-0.5)", "as": "_xOffset"}));
            encoding_map.insert(
                "xOffset".to_string(),
                serde_json::json!({"field": "_xOffset", "scale":{"domain":[-1,1]}, "type": "quantitative"}),
            );
        }
        if y_categories.is_some() {
            transforms.push(serde_json::json!({"calculate": "(random()-0.5) * (random()-0.5)", "as": "_yOffset"}));
            encoding_map.insert(
                "yOffset".to_string(),
                serde_json::json!({"field": "_yOffset", "scale":{"domain":[-1,1]}, "type": "quantitative"}),
            );
        }

        if spec.get("data").and_then(|d| d.get("cats")).is_some() {
            // If the spec includes a top-level `cats` array, add a color encoding
            // that maps the category field to a color scheme.
            // This is a common pattern for scatter plots with categorical grouping.
            encoding_map.insert(
                "color".to_string(),
                serde_json::json!({"field": "cat", "type": "nominal"}),
            );
        }

        if !transforms.is_empty() {
            base["transform"] = serde_json::Value::Array(transforms);
        }

        base["encoding"] = serde_json::Value::Object(encoding_map);
    } else {
        // Attempt binned heatmap: x buckets + yBuckets + allYValues
        let maybe_x_keys = spec
            .get("data")
            .and_then(|d| d.get("buckets"))
            .and_then(|v| v.as_array())
            .cloned();
        let maybe_y_keys = spec
            .get("data")
            .and_then(|d| d.get("yBuckets"))
            .and_then(|v| v.as_array())
            .cloned();
        let maybe_all_y = spec
            .get("data")
            .and_then(|d| d.get("allYValues"))
            .and_then(|v| v.as_array())
            .cloned();

        if let (Some(x_keys_arr), Some(y_keys_arr), Some(all_y_arr)) =
            (maybe_x_keys, maybe_y_keys, maybe_all_y)
        {
            // Decide whether x/y buckets are categorical (strings) or numeric.
            let x_is_categorical = x_categories.is_some();
            let y_is_categorical = y_categories.is_some();

            // Prepare numeric vectors if needed. Support primitive numeric
            // arrays as well as structured object buckets where ids were
            // extracted into `bucket_ids_opt`.
            let x_keys: Vec<f64> = if !x_is_categorical {
                if !x_keys_arr.is_empty() && x_keys_arr[0].is_object() {
                    if let Some(ids) = bucket_ids_opt.clone() {
                        ids.iter()
                            .map(|s| s.parse::<f64>().unwrap_or(0.0))
                            .collect()
                    } else {
                        Vec::new()
                    }
                } else {
                    x_keys_arr
                        .iter()
                        .map(|k| k.as_f64().unwrap_or(0.0))
                        .collect()
                }
            } else {
                Vec::new()
            };
            let y_keys: Vec<f64> = if !y_is_categorical {
                y_keys_arr
                    .iter()
                    .map(|k| k.as_f64().unwrap_or(0.0))
                    .collect()
            } else {
                Vec::new()
            };

            let x_width = if !x_is_categorical && x_keys.len() >= 2 {
                x_keys[1] - x_keys[0]
            } else if !x_is_categorical {
                if let Some(domain_arr) = spec
                    .get("x")
                    .and_then(|x| x.get("domain"))
                    .and_then(|d| d.as_array())
                {
                    if domain_arr.len() >= 2 {
                        let lo = domain_arr[0].as_f64().unwrap_or(0.0);
                        let hi = domain_arr[1].as_f64().unwrap_or(lo + 1.0);
                        (hi - lo) / (x_keys.len() as f64).max(1.0)
                    } else {
                        1.0
                    }
                } else {
                    1.0
                }
            } else {
                1.0
            };

            let y_height = if !y_is_categorical && y_keys.len() >= 2 {
                y_keys[1] - y_keys[0]
            } else if !y_is_categorical {
                if let Some(domain_arr) = spec
                    .get("y")
                    .and_then(|y| y.get("domain"))
                    .and_then(|d| d.as_array())
                {
                    if domain_arr.len() >= 2 {
                        let lo = domain_arr[0].as_f64().unwrap_or(0.0);
                        let hi = domain_arr[1].as_f64().unwrap_or(lo + 1.0);
                        (hi - lo) / (y_keys.len() as f64).max(1.0)
                    } else {
                        1.0
                    }
                } else {
                    1.0
                }
            } else {
                1.0
            };

            // Build rects from allYValues: outer array per x-bucket, inner per y-bucket
            let mut rects: Vec<serde_json::Value> = Vec::new();
            for (xi, x_bucket) in all_y_arr.iter().enumerate() {
                if let Some(y_counts) = x_bucket.as_array() {
                    for (yi, count_val) in y_counts.iter().enumerate() {
                        let count_opt = count_val
                            .as_u64()
                            .or_else(|| count_val.as_i64().map(|n| n as u64))
                            .filter(|&n| n != 0);

                        // Only emit rects for buckets with a non-zero count.
                        if let Some(count) = count_opt {
                            let mut obj = serde_json::Map::new();
                            if x_is_categorical {
                                let x_cat = x_categories
                                    .as_ref()
                                    .and_then(|v| v.get(xi))
                                    .cloned()
                                    .unwrap_or_default();
                                obj.insert(
                                    "x".to_string(),
                                    serde_json::Value::String(x_cat.clone()),
                                );
                                if let Some(ref labels) = bucket_labels_opt {
                                    if let Some(lbl) = labels.get(xi) {
                                        obj.insert(
                                            "x_label".to_string(),
                                            serde_json::Value::String(lbl.clone()),
                                        );
                                    }
                                }
                            } else {
                                let left = *x_keys.get(xi).unwrap_or(&0.0);
                                let right = if xi + 1 < x_keys.len() {
                                    x_keys[xi + 1]
                                } else {
                                    left + x_width
                                };
                                obj.insert("x".to_string(), serde_json::Value::from(left));
                                obj.insert("x2".to_string(), serde_json::Value::from(right));
                            }

                            if y_is_categorical {
                                let y_cat = y_categories
                                    .as_ref()
                                    .and_then(|v| v.get(yi))
                                    .cloned()
                                    .unwrap_or_default();
                                obj.insert("y".to_string(), serde_json::Value::String(y_cat));
                            } else {
                                let bottom = *y_keys.get(yi).unwrap_or(&0.0);
                                let top = if yi + 1 < y_keys.len() {
                                    y_keys[yi + 1]
                                } else {
                                    bottom + y_height
                                };
                                obj.insert("y".to_string(), serde_json::Value::from(bottom));
                                obj.insert("y2".to_string(), serde_json::Value::from(top));
                            }

                            obj.insert("count".to_string(), serde_json::Value::from(count));
                            rects.push(serde_json::Value::Object(obj));
                        }
                    }
                }
            }

            // Colour domain from zDomain if provided (as Value)
            let color_domain_value = spec
                .get("data")
                .and_then(|d| d.get("zDomain"))
                .and_then(|v| v.as_array())
                .and_then(|arr| {
                    if arr.len() >= 2 {
                        let a = arr[0].as_f64().unwrap_or(0.0);
                        let b = arr[1].as_f64().unwrap_or(a + 1.0);
                        Some(serde_json::Value::Array(vec![
                            serde_json::Value::from(a),
                            serde_json::Value::from(b),
                        ]))
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| serde_json::Value::Array(vec![]));

            // Build axis encodings: use categorical tick_values when available,
            // otherwise use numeric boundaries computed above.
            let mut encoding_map = serde_json::Map::new();

            if let Some(ref cats) = x_categories {
                let x_meta = serde_json::json!({"tick_values": cats, "value_type": "keyword"});
                let x_enc_res =
                    make_vl_axis_encoding(Some(&x_meta), "x", Some(x_label), None, true, Some(1));
                let x_enc = match x_enc_res {
                    Ok(v) => v,
                    Err(e) => return serde_json::json!({"error": e}),
                };
                encoding_map.insert("x".to_string(), x_enc);
            } else {
                let mut x_boundaries_num: Vec<f64> = x_keys.clone();
                if !x_boundaries_num.is_empty() {
                    let last_right = if x_boundaries_num.len() >= 2 {
                        x_boundaries_num[x_boundaries_num.len() - 1]
                            + (x_boundaries_num[1] - x_boundaries_num[0])
                    } else {
                        x_boundaries_num[0] + x_width
                    };
                    x_boundaries_num.push(last_right);
                }
                let x_enc = match make_vl_axis_encoding(
                    spec.get("x"),
                    "x",
                    Some(x_label),
                    Some(&x_boundaries_num),
                    false,
                    Some(1),
                ) {
                    Ok(v) => v,
                    Err(e) => return serde_json::json!({"error": e}),
                };
                encoding_map.insert("x".to_string(), x_enc);
                encoding_map.insert("x2".to_string(), serde_json::json!({"field": "x2"}));
            }

            if let Some(ref cats) = y_categories {
                let y_meta = serde_json::json!({"tick_values": cats, "value_type": "keyword"});
                let y_enc_res =
                    make_vl_axis_encoding(Some(&y_meta), "y", Some(y_label), None, true, Some(1));
                let y_enc = match y_enc_res {
                    Ok(v) => v,
                    Err(e) => return serde_json::json!({"error": e}),
                };
                encoding_map.insert("y".to_string(), y_enc);
            } else {
                let mut y_boundaries_num: Vec<f64> = y_keys.clone();
                if !y_boundaries_num.is_empty() {
                    let last_top = if y_boundaries_num.len() >= 2 {
                        y_boundaries_num[y_boundaries_num.len() - 1]
                            + (y_boundaries_num[1] - y_boundaries_num[0])
                    } else {
                        y_boundaries_num[0] + y_height
                    };
                    y_boundaries_num.push(last_top);
                }
                let y_enc = match make_vl_axis_encoding(
                    spec.get("y"),
                    "y",
                    Some(y_label),
                    Some(&y_boundaries_num),
                    false,
                    Some(1),
                ) {
                    Ok(v) => v,
                    Err(e) => return serde_json::json!({"error": e}),
                };
                encoding_map.insert("y".to_string(), y_enc);
                encoding_map.insert("y2".to_string(), serde_json::json!({"field": "y2"}));
            }

            encoding_map.insert(
                "color".to_string(),
                serde_json::json!({
                    "field": "count",
                    "type": "quantitative",
                    "scale": {"type": "linear", "domain": color_domain_value}
                }),
            );

            base["data"] = serde_json::json!({"values": rects});
            base["mark"] = serde_json::json!({"type": "rect"});
            base["encoding"] = serde_json::Value::Object(encoding_map);
        } else {
            // Fallback to empty points if nothing useful present
            base["data"] = serde_json::json!({"values": serde_json::json!([])});
            base["mark"] = serde_json::Value::String("point".to_string());
            base["encoding"] = serde_json::json!({});
        }
    }
    let _ = (x_field, y_field);
    base
}

fn vl_bar(spec: &serde_json::Value, mut base: serde_json::Value) -> serde_json::Value {
    let x_meta = spec.get("x").unwrap_or(&serde_json::Value::Null);
    let x_field = x_meta
        .get("field")
        .and_then(|v| v.as_str())
        .unwrap_or("rank");
    let x_label = x_meta
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or(x_field);

    let buckets = spec
        .get("data")
        .and_then(|d| d.get("buckets"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));

    base["data"] = serde_json::json!({"values": buckets});
    base["mark"] = serde_json::Value::String("bar".to_string());
    base["encoding"] = serde_json::json!({
        "y": {
            "field": x_field,
            "type": "nominal",
            "axis": {"title": x_label}
        },
        "x": {"field": "count", "type": "quantitative"}
    });
    base
}

fn vl_painting(spec: &serde_json::Value, mut base: serde_json::Value) -> serde_json::Value {
    let segments = spec
        .get("data")
        .and_then(|d| d.get("segments"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));

    base["data"] = serde_json::json!({"values": segments});
    base["mark"] = serde_json::json!({"type": "bar"});
    base["encoding"] = serde_json::json!({
        "x": {"field": "start", "type": "quantitative", "axis": {"title": "Start"}},
        "x2": {"field": "end"},
        "y": {"field": "sequenceId", "type": "nominal", "axis": {"title": "Sequence"}},
        "color": {"field": "cat", "type": "nominal"}
    });
    base
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_dispatch_sends_tree_to_vega_v6() {
        let spec = serde_json::json!({
            "report_type": "tree",
            "display": {"title": "Tree", "width": 800}
        });

        let out: serde_json::Value =
            serde_json::from_str(&plot_spec_to_render_json(&spec.to_string())).unwrap();

        assert_eq!(out["$schema"], "https://vega.github.io/schema/vega/v6.json");
        assert_eq!(out["width"], 800);
        assert_eq!(out["title"]["text"], "Tree");
    }

    #[test]
    fn render_dispatch_defaults_tree_to_rect_layout() {
        let out = tree_vega_for(serde_json::json!({}));
        let links = out["data"][0]["values"].as_array().unwrap();
        assert!(links
            .iter()
            .all(|link| !link["d"].as_str().unwrap().contains('A')));
        assert!(links
            .iter()
            .any(|link| link["d"].as_str().unwrap().contains('H')));
    }

    #[test]
    fn render_dispatch_routes_circ_style_to_arcs() {
        let out = tree_vega_for(serde_json::json!({"tree_style": "circ"}));
        let links = out["data"][0]["values"].as_array().unwrap();
        assert!(links
            .iter()
            .any(|link| link["d"].as_str().unwrap().contains('A')));
    }

    #[test]
    fn render_dispatch_routes_radial_style_to_rays() {
        let out = tree_vega_for(serde_json::json!({"tree_style": "radial"}));
        let links = out["data"][0]["values"].as_array().unwrap();
        assert!(!links.is_empty());
        assert!(links
            .iter()
            .all(|link| !link["d"].as_str().unwrap().contains('A')));
    }

    #[test]
    fn render_dispatch_routes_ring_style_to_sector_arcs() {
        let out = tree_vega_for(serde_json::json!({"tree_style": "ring"}));
        assert!(out.get("error").is_none(), "{out}");
        assert_eq!(out["data"][0]["name"], "arcs");
        let arcs = out["data"][0]["values"].as_array().unwrap();
        assert_eq!(arcs.len(), 4);
        assert!(arcs
            .iter()
            .all(|arc| arc["d"].as_str().unwrap().contains('A')));
    }

    #[test]
    fn render_dispatch_ring_style_has_no_leaders_dataset() {
        let out = tree_vega_for(serde_json::json!({"tree_style": "ring"}));
        let names: Vec<&str> = out["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|data| data["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["arcs", "labels"]);
    }

    #[test]
    fn render_dispatch_rejects_unknown_tree_style() {
        let out = tree_vega_for(serde_json::json!({"tree_style": "spiral"}));
        assert!(out["error"]
            .as_str()
            .unwrap()
            .contains("unknown tree_style"));
    }

    #[test]
    fn render_dispatch_rejects_unknown_tip_alignment() {
        let out = tree_vega_for(serde_json::json!({"tip_alignment": "sideways"}));
        assert!(out["error"]
            .as_str()
            .unwrap()
            .contains("unknown tip_alignment"));
    }

    #[test]
    fn render_dispatch_accepts_tip_alignment_for_every_style() {
        for style in ["rect", "circ", "radial"] {
            let out =
                tree_vega_for(serde_json::json!({"tree_style": style, "tip_alignment": "labels"}));
            assert!(out.get("error").is_none(), "{style}: {out}");
            assert_eq!(out["data"][2]["name"], "leaders");
        }
    }

    #[test]
    fn render_dispatch_applies_font_size_to_every_style() {
        for style in ["rect", "circ", "radial"] {
            let out = tree_vega_for(serde_json::json!({"tree_style": style, "font_size": 8}));
            assert!(out.get("error").is_none(), "{style}: {out}");
            assert!(
                out.to_string().contains("\"fontSize\":{\"value\":8.0}"),
                "{style}: {out}"
            );
        }
    }

    #[test]
    fn render_dispatch_accepts_font_scale_for_every_style() {
        for style in ["rect", "circ", "radial", "ring"] {
            let out = tree_vega_for(serde_json::json!({"tree_style": style, "font_scale": 0.02}));
            assert!(out.get("error").is_none(), "{style}: {out}");
        }
    }

    #[test]
    fn render_dispatch_rejects_font_size_with_font_scale() {
        let out = tree_vega_for(serde_json::json!({"font_size": 8, "font_scale": 0.02}));
        assert!(out["error"].as_str().unwrap().contains("not both"));
    }

    #[test]
    fn render_dispatch_rejects_unknown_label_orientation() {
        let out =
            tree_vega_for(serde_json::json!({"tree_style": "circ", "label_orientation": "upside"}));
        assert!(out["error"]
            .as_str()
            .unwrap()
            .contains("unknown label_orientation"));
    }

    #[test]
    fn render_dispatch_accepts_label_orientation_for_circular_styles() {
        for style in ["circ", "radial", "ring"] {
            let out = tree_vega_for(
                serde_json::json!({"tree_style": style, "label_orientation": "continuous"}),
            );
            assert!(out.get("error").is_none(), "{style}: {out}");
        }
    }

    #[test]
    fn render_dispatch_routes_ring_total_angle_to_sweep() {
        let full = tree_vega_for(serde_json::json!({"tree_style": "ring"}));
        let half = tree_vega_for(serde_json::json!({"tree_style": "ring", "total_angle": 180.0}));

        assert!(half.get("error").is_none(), "{half}");
        assert_ne!(full["data"][0]["values"], half["data"][0]["values"]);
    }

    /// Render a nested tree whose `display.tree` object is `tree`.
    fn tree_vega_for(tree: serde_json::Value) -> serde_json::Value {
        let spec = serde_json::json!({
            "report_type": "tree",
            "display": {"width": 800, "height": 600, "tree": tree},
            "data": {"treeNodes": {
                "r": {"children": {"a": {}, "b": {}}},
                "a": {"children": {"a1": {}, "a2": {}}, "branch_length": 1.0},
                "a1": {"scientific_name": "A1", "branch_length": 1.0},
                "a2": {"scientific_name": "A2", "branch_length": 1.0},
                "b": {"scientific_name": "B", "branch_length": 1.0}
            }}
        });
        serde_json::from_str(&plot_spec_to_render_json(&spec.to_string())).unwrap()
    }

    #[test]
    fn render_dispatch_unwraps_report_envelope_for_tree() {
        let envelope = serde_json::json!({"plot_spec": {"report_type": "tree"}});

        let out: serde_json::Value =
            serde_json::from_str(&plot_spec_to_render_json(&envelope.to_string())).unwrap();

        assert_eq!(out["$schema"], "https://vega.github.io/schema/vega/v6.json");
    }

    #[test]
    fn render_dispatch_keeps_other_reports_on_vega_lite() {
        let spec = serde_json::json!({"report_type": "map"});

        let out: serde_json::Value =
            serde_json::from_str(&plot_spec_to_render_json(&spec.to_string())).unwrap();

        assert_eq!(
            out["$schema"],
            "https://vega.github.io/schema/vega-lite/v6.json"
        );
    }

    #[test]
    fn arc_vega_lite_donut_has_two_segments_and_centre_labels() {
        let spec = serde_json::json!({
            "report_type": "arc",
            "data": {
                "type": "arc",
                "feature_count": 1538,
                "reference_count": 10384,
                "featureTerm": "genome_size>1000000000",
                "referenceTerm": "assembly_level=Chromosome"
            }
        });

        let out = plot_spec_to_vega_lite_json(&spec.to_string());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let layers = parsed.get("layer").and_then(|l| l.as_array()).unwrap();

        assert_eq!(
            layers[0].pointer("/mark/type").and_then(|t| t.as_str()),
            Some("arc")
        );
        let segments = layers[0]
            .pointer("/data/values")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(
            layers[1]
                .pointer("/data/values/1/text")
                .and_then(|t| t.as_str()),
            Some("85.2%")
        );
        assert_eq!(
            layers[2]
                .pointer("/data/values/0/text")
                .and_then(|t| t.as_str()),
            Some("14.8%")
        );
        assert_eq!(
            layers[3]
                .pointer("/data/values/0/text")
                .and_then(|t| t.as_str()),
            Some("1,538 / 10,384")
        );
    }

    #[test]
    fn arc_vega_lite_three_value_donut_centres_reference_share_of_context() {
        let spec = serde_json::json!({
            "report_type": "arc",
            "data": {
                "type": "arc",
                "feature_count": 1454,
                "reference_count": 8211,
                "context_count": 10384,
                "featureTerm": "genome_size>1000000000",
                "referenceTerm": "assembly_level=Chromosome",
                "contextTerm": ""
            }
        });

        let out = plot_spec_to_vega_lite_json(&spec.to_string());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let layers = parsed.get("layer").and_then(|l| l.as_array()).unwrap();

        assert_eq!(layers.len(), 5);
        assert_eq!(
            layers[3]
                .pointer("/data/values/0/text")
                .and_then(|t| t.as_str()),
            Some("79.1%")
        );
        assert_eq!(
            layers[4]
                .pointer("/data/values/0/text")
                .and_then(|t| t.as_str()),
            Some("8,211 / 10,384")
        );
        assert_eq!(
            layers[1].pointer("/mark/type").and_then(|t| t.as_str()),
            Some("arc")
        );
        let end_angle = layers[1]
            .pointer("/data/values/0/end")
            .and_then(|v| v.as_f64())
            .unwrap();
        assert!((end_angle - 0.7907 * std::f64::consts::TAU).abs() < 1e-3);
    }

    #[test]
    fn arc_vega_lite_rainbow_draws_semicircle_rings_outermost_first() {
        let spec = serde_json::json!({
            "report_type": "arc",
            "data": {
                "type": "arc",
                "featureTerm": "assembly_span>300000000",
                "referenceTerm": "assembly_level",
                "arc": [
                    {"label": "species", "feature_count": 1008, "reference_count": 1155, "context_count": 12583},
                    {"label": "genus", "feature_count": 515, "reference_count": 570, "context_count": 1447},
                    {"label": "family", "feature_count": 139, "reference_count": 150, "context_count": 164}
                ]
            }
        });

        let out = plot_spec_to_vega_lite_json(&spec.to_string());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let layers = parsed.get("layer").and_then(|l| l.as_array()).unwrap();

        assert_eq!(layers.len(), 15);
        let track_outer_radii: Vec<f64> = [0, 3, 6]
            .iter()
            .map(|&index| {
                layers[index]
                    .pointer("/mark/outerRadius")
                    .and_then(|v| v.as_f64())
                    .unwrap()
            })
            .collect();
        assert!((track_outer_radii[0] - 300.0).abs() < 1e-9);
        assert!(
            track_outer_radii[0] > track_outer_radii[1]
                && track_outer_radii[1] > track_outer_radii[2]
        );
        assert_eq!(
            layers[0]
                .pointer("/encoding/color/value")
                .and_then(|v| v.as_str()),
            Some(RING_TRACK_FILL)
        );
        assert_eq!(
            layers[2]
                .pointer("/encoding/color/scale/scheme")
                .and_then(|v| v.as_str()),
            Some(RING_COLOUR_SCHEME)
        );
        assert_eq!(
            layers[2].pointer("/encoding/color/scale/domain"),
            Some(&serde_json::json!(["species", "genus", "family"]))
        );
        let species_feature_end = layers[2]
            .pointer("/data/values/0/end")
            .and_then(|v| v.as_f64())
            .unwrap();
        let expected_end = semicircle_angle(1008.0 / 12583.0);
        assert!((species_feature_end - expected_end).abs() < 1e-9);
        let percent_labels: Vec<&str> = (9..12)
            .map(|index| {
                layers[index]
                    .pointer("/encoding/text/value")
                    .and_then(|v| v.as_str())
                    .unwrap()
            })
            .collect();
        assert_eq!(percent_labels, vec!["9.2%", "39.4%", "91.5%"]);
        assert_eq!(
            layers[14]
                .pointer("/data/values/1/counts")
                .and_then(|v| v.as_str()),
            Some("515 / 570 / 1.45k")
        );
    }

    #[test]
    fn format_count_compact_uses_three_figures() {
        assert_eq!(format_count_compact(570.0), "570");
        assert_eq!(format_count_compact(1160.0), "1.16k");
        assert_eq!(format_count_compact(12583.0), "12.6k");
        assert_eq!(format_count_compact(2_500_000.0), "2.50M");
    }

    #[test]
    fn report_yaml_from_url_histogram() {
        let url = "https://goat.genomehubs.org/api/v2/report?report=histogram&x=genome_size&result=taxon&tax_name=Mammalia&rank=species";
        let (qy, _py, ry) = report_yaml_from_url_params(url).unwrap();
        let q: crate::query::SearchQuery = serde_yaml::from_str(&qy).unwrap();
        assert_eq!(q.index, crate::query::SearchIndex::Taxon);
        let taxa = q.identifiers.taxa.unwrap();
        assert_eq!(taxa.names, vec!["Mammalia"]);
        let r: std::collections::BTreeMap<String, serde_json::Value> =
            serde_yaml::from_str(&ry).unwrap();
        assert_eq!(
            r["report"],
            serde_json::Value::String("histogram".to_string())
        );
        assert_eq!(r["x"], serde_json::Value::String("genome_size".to_string()));
    }

    #[test]
    fn report_yaml_from_url_scatter_with_thresholds() {
        let url = "https://example.org/report?report=scatter&x=genome_size&y=chromosome_number&result=taxon&countThreshold=50&collapseMonotypic=true";
        let (_qy, _py, ry) = report_yaml_from_url_params(url).unwrap();
        let r: std::collections::BTreeMap<String, serde_json::Value> =
            serde_yaml::from_str(&ry).unwrap();
        assert_eq!(r["scatter_threshold"], serde_json::Value::Number(50.into()));
        assert_eq!(r["collapse_monotypic"], serde_json::Value::Bool(true));
    }

    #[test]
    fn report_yaml_from_url_missing_report_param() {
        let url = "https://goat.genomehubs.org/api/v2/search?result=taxon";
        assert!(report_yaml_from_url_params(url).is_err());
    }

    #[test]
    fn scatter_vega_lite_uses_raw_data_when_cells_missing() {
        let spec = serde_json::json!({
            "report_type": "scatter",
            "x": {"field": "genome_size", "scale": "linear", "value_type": "float"},
            "y": {"field": "busco_total", "scale": "linear", "value_type": "float"},
            "data": {
                "rawData": {
                    "all": [
                        {"x": 10.0, "y": 20.0}
                    ]
                }
            }
        });

        let out = plot_spec_to_vega_lite_json(&spec.to_string());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let values = parsed
            .pointer("/data/values")
            .and_then(|v| v.as_array())
            .unwrap();

        assert_eq!(values.len(), 1);
        assert_eq!(values[0].get("x").and_then(|v| v.as_f64()), Some(10.0));
        assert_eq!(values[0].get("y").and_then(|v| v.as_f64()), Some(20.0));
        assert_eq!(values[0].get("cat").and_then(|v| v.as_str()), Some("all"));
    }

    #[test]
    fn scatter_vega_lite_renders_heatmap_from_binned_values() {
        let spec = serde_json::json!({
            "report_type": "scatter",
            "x": {"field": "x", "scale": "linear", "value_type": "float"},
            "y": {"field": "y", "scale": "linear", "value_type": "float"},
            "data": {
                "buckets": [0.0, 10.0],
                "yBuckets": [0.0, 5.0],
                "allYValues": [[1,2],[3,4]],
                "zDomain": [1,4]
            }
        });

        let out = plot_spec_to_vega_lite_json(&spec.to_string());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            parsed
                .get("mark")
                .and_then(|m| m.get("type"))
                .and_then(|t| t.as_str()),
            Some("rect")
        );
        let values = parsed
            .pointer("/data/values")
            .and_then(|v| v.as_array())
            .unwrap();
        // 2 x-buckets * 2 y-buckets -> 4 rects
        assert_eq!(values.len(), 4);
        // check a sample rect has expected keys
        let sample = &values[0];
        assert!(sample.get("x").is_some());
        assert!(sample.get("x2").is_some());
        assert!(sample.get("y").is_some());
        assert!(sample.get("y2").is_some());
        assert!(sample.get("count").is_some());
    }
}
