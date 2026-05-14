//! Report axis type system.
//!
//! All types in this module are serialisable and WASM-compatible.
//! They express the full configuration space for a single report axis —
//! what field to aggregate, how to bin it, and how to present the result.

pub mod axis;
pub mod bounds;
pub mod display;
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

/// Convert a `PlotSpec` JSON string into a Vega-Lite v5 specification JSON string.
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
        "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
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
        "count_per_rank" | "countPerRank" | "sources" => vl_bar(spec_val, base),
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
    let x_scale_str = x_meta
        .get("scale")
        .and_then(|v| v.as_str())
        .unwrap_or("linear");
    let x_vl_scale = if x_scale_str == "log10" {
        "log"
    } else {
        "linear"
    };

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
    let y_label = display
        .get("y_label")
        .and_then(|v| v.as_str())
        .unwrap_or("Count");

    let buckets = spec
        .get("data")
        .and_then(|d| d.get("buckets"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));

    base["data"] = serde_json::json!({"values": buckets});
    base["mark"] = serde_json::json!({"type": "bar"});
    base["encoding"] = serde_json::json!({
        "x": {
            "field": "key",
            "type": "quantitative",
            "scale": {"type": x_vl_scale},
            "axis": {"title": x_label}
        },
        "y": {
            "field": "doc_count",
            "type": "quantitative",
            "scale": {"type": y_vl_scale},
            "axis": {"title": y_label}
        }
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
    let x_scale_str = x_meta
        .get("scale")
        .and_then(|v| v.as_str())
        .unwrap_or("linear");
    let x_vl_scale = if x_scale_str == "log10" {
        "log"
    } else {
        "linear"
    };

    let y_meta = spec.get("y").unwrap_or(&serde_json::Value::Null);
    let y_field = y_meta.get("field").and_then(|v| v.as_str()).unwrap_or("y");
    let y_label = y_meta
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or(y_field);
    let y_scale_str = y_meta
        .get("scale")
        .and_then(|v| v.as_str())
        .unwrap_or("linear");
    let y_vl_scale = if y_scale_str == "log10" {
        "log"
    } else {
        "linear"
    };

    let cells = spec
        .get("data")
        .and_then(|d| d.get("cells"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));

    base["data"] = serde_json::json!({"values": cells});
    base["mark"] = serde_json::Value::String("point".to_string());
    base["encoding"] = serde_json::json!({
        "x": {
            "field": "x",
            "type": "quantitative",
            "scale": {"type": x_vl_scale},
            "axis": {"title": x_label}
        },
        "y": {
            "field": "y",
            "type": "quantitative",
            "scale": {"type": y_vl_scale},
            "axis": {"title": y_label}
        }
    });
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
}
