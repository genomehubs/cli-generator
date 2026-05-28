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

    /// Fields that may be present for this type (used by validator to warn on unknowns).
    pub fn valid_axes(&self) -> &'static [&'static str] {
        match self {
            Self::Histogram => &[
                "x",
                "y",
                "cat",
                "rank",
                "fields",
                "status_filter",
                "cat_rank",
                "cat_opts",
                "x_opts",
                "y_opts",
            ],
            Self::Scatter => &[
                "x",
                "y",
                "cat",
                "rank",
                "fields",
                "status_filter",
                "scatter_threshold",
                "cat_opts",
                "x_opts",
                "y_opts",
            ],
            Self::Map => &[
                "location_field",
                "hex_resolution",
                "map_threshold",
                "rank",
                "status_filter",
            ],
            Self::Tree => &[
                "rank",
                "collapse_monotypic",
                "preserve_rank",
                "count_rank",
                "status_filter",
                "cat",
                "cat_rank",
            ],
            Self::CountPerRank => &["query", "ranks", "cat", "cat_opts"],
            Self::Sources => &["rank", "fields", "status_filter"],
            Self::Arc => &["x", "y", "cat", "x_opts", "y_opts", "cat_opts"],
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
    for (key, yaml_key) in [("collapseMonotypic", "collapse_monotypic")] {
        if let Some(val) = params.get(key).and_then(|v| v.first()) {
            let b = matches!(val.to_lowercase().as_str(), "true" | "1" | "yes");
            doc.insert(yaml_key.to_string(), serde_json::Value::Bool(b));
        }
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

/// Convert a `PlotSpec` JSON string into a Vega-Lite v6 specification JSON string.
///
/// Accepts the full `/report` response envelope (extracts `plot_spec` automatically)
/// or a bare `PlotSpec` object.  Returns an error JSON on failure.
pub fn plot_spec_to_vega_lite_json(input: &str) -> String {
    let parsed: serde_json::Value = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(e) => return format!("{{\"error\":\"invalid JSON: {e}\"}}"),
    };

    // Accept either a full report response or a bare PlotSpec.
    let spec_val = if parsed.get("report_type").is_some() {
        &parsed
    } else if let Some(ps) = parsed.get("plot_spec") {
        ps
    } else {
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
        "tree" => {
            base["mark"] = serde_json::Value::String("point".to_string());
            base["data"] = serde_json::json!({"values": []});
            base
        }
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
            base["mark"] = serde_json::Value::String("arc".to_string());
            base
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
                        obj.insert("cat".to_string(), serde_json::Value::String(cat.clone()));
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
                let n_bins = bucket_labels.len() as f64;
                let bin_pixel = if n_bins > 0.0 {
                    plot_width_px / n_bins
                } else {
                    10.0
                };
                let n_cats = cats.len() as f64;
                let grouped_bar_px = ((bin_pixel * 0.9) / n_cats).max(2.0);

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
                let n_bins = keys_num.len() as f64;
                let bin_pixel = if n_bins > 0.0 {
                    plot_width_px / n_bins
                } else {
                    10.0
                };
                let n_cats = cats.len() as f64;
                let grouped_bar_px = ((bin_pixel * 0.9) / n_cats).max(2.0);

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
                        obj.insert("cat".to_string(), serde_json::Value::String(cat.clone()));
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
                            .and_then(|n| if n == 0 { None } else { Some(n) });

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
