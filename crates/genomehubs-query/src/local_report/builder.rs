//! Build a [`PlotSpec`] from a local TSV/CSV data file.
//!
//! This module provides [`local_plot_spec`], which reads delimited text data
//! and constructs a [`PlotSpec`] entirely client-side — no API call required.
//!
//! # Supported report types
//!
//! | `PlotReportType` | Required columns           |
//! |------------------|----------------------------|
//! | `Histogram`      | One numeric `x` column     |
//! | `Scatter`        | Numeric `x` and `y` columns |
//! | `CountPerRank`   | Keyword `x`, numeric `y`   |
//!
//! Column names are resolved through `column_map`:
//! `{"x": "genome_size", "y": "c_value"}` — axis role → file column header.
//! If `column_map` is empty the first column is `x` and the second is `y`.

use std::collections::HashMap;

use serde_json::Value;
use thiserror::Error;

use super::super::report::display::DisplaySpec;
use super::super::report::plot_spec::{AxisMeta, PlotReportType, PlotSpec};
use super::super::report::spec_builder::resolve_axis_display;
use super::tsv::TsvError;

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors that can occur while building a local plot spec.
#[derive(Debug, Error)]
pub enum LocalReportError {
    /// The delimited file could not be parsed.
    #[error("failed to read data file: {0}")]
    Tsv(#[from] TsvError),

    /// A required column was not found in the data.
    #[error("column '{column}' (mapped from axis '{axis}') not found in data")]
    MissingColumn { axis: String, column: String },

    /// The report type is not supported for local data.
    #[error("report type '{0}' is not supported for local data files")]
    UnsupportedReportType(String),

    /// A column expected to be numeric contains non-numeric values.
    #[error("column '{column}' must be numeric for axis '{axis}'")]
    NonNumericColumn { axis: String, column: String },
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Build a [`PlotSpec`] from delimited text content.
///
/// `content` is the full text of the TSV/CSV file.
/// `report_type` determines which axes are extracted.
/// `column_map` maps axis roles (`"x"`, `"y"`) to column names in the file;
///   pass an empty map to use positional defaults (first column → `x`,
///   second column → `y`).
/// `display` is applied as-is to the `PlotSpec`; axis hints are resolved.
/// `delimiter` is the field separator (use [`super::tsv::detect_delimiter`] to
///   infer it from the file extension before calling this function).
///
/// # Errors
///
/// Returns a [`LocalReportError`] when the file is unreadable, a required
/// column is missing, or the report type is not supported for local data.
pub fn local_plot_spec(
    content: &str,
    report_type: PlotReportType,
    column_map: &HashMap<String, String>,
    display: Option<DisplaySpec>,
    delimiter: char,
) -> Result<PlotSpec, LocalReportError> {
    let (headers, rows) = super::tsv::read_delimited_with_headers(content, delimiter)?;

    // Determine effective column names (explicit map or positional defaults).
    let x_col = resolve_column("x", column_map, &headers, 0)?;
    let y_col = match report_type {
        PlotReportType::Histogram => None,
        PlotReportType::Scatter | PlotReportType::CountPerRank => {
            Some(resolve_column("y", column_map, &headers, 1)?)
        }
        other => {
            return Err(LocalReportError::UnsupportedReportType(
                format!("{other:?}").to_lowercase(),
            ))
        }
    };

    let display = display.unwrap_or_default();

    let x_values = extract_numeric_column(&rows, &x_col)?;
    let mut x_meta = build_axis_meta(&x_col, &x_values, "float");

    let x_axis_opts = display
        .histogram
        .as_ref()
        .and_then(|h| h.x_axis.as_ref())
        .or_else(|| display.scatter.as_ref().and_then(|s| s.x_axis.as_ref()));
    resolve_axis_display(&mut x_meta, x_axis_opts);

    let y_meta = match y_col {
        None => None,
        Some(ref col) => {
            let y_values = extract_numeric_column(&rows, col)?;
            let mut meta = build_axis_meta(col, &y_values, "float");
            let y_axis_opts = display
                .histogram
                .as_ref()
                .and_then(|h| h.y_axis.as_ref())
                .or_else(|| display.scatter.as_ref().and_then(|s| s.y_axis.as_ref()));
            resolve_axis_display(&mut meta, y_axis_opts);
            Some(meta)
        }
    };

    let data = rows_to_data_value(&rows, &x_col, y_col.as_deref());

    Ok(PlotSpec {
        report_type,
        x: Some(x_meta),
        y: y_meta,
        z: None,
        cat: None,
        series: vec![],
        display,
        data,
    })
}

// ── WASM / PyO3-facing JSON entry point ──────────────────────────────────────

/// Build a [`PlotSpec`] from local delimited data and return it as JSON.
///
/// All arguments are strings so this function is directly usable as a
/// WASM export or PyO3 binding without any complex types crossing the FFI.
///
/// `report_type_str` — one of `"histogram"`, `"scatter"`, `"bar"`.
/// `column_map_json` — JSON object mapping axis roles to column names, e.g.
///   `{"x":"genome_size","y":"c_value"}`. Pass `"{}"` for positional defaults.
/// `display_json` — serialised [`DisplaySpec`]; pass `"{}"` for defaults.
/// `delimiter_str` — field separator: `"\t"` for TSV, `","` for CSV.
///   Pass `""` to default to `"\t"`.
///
/// Returns the serialised [`PlotSpec`] on success, or
/// `{"error":"..."}` on failure.
pub fn local_plot_spec_json(
    content: &str,
    report_type_str: &str,
    column_map_json: &str,
    display_json: &str,
    delimiter_str: &str,
) -> String {
    let report_type = match PlotReportType::parse(report_type_str) {
        Some(t) => t,
        None => {
            return serde_json::json!({"error": format!("unknown report type '{report_type_str}'")})
                .to_string()
        }
    };

    let column_map: HashMap<String, String> =
        match serde_json::from_str(if column_map_json.is_empty() {
            "{}"
        } else {
            column_map_json
        }) {
            Ok(m) => m,
            Err(e) => {
                return serde_json::json!({"error": format!("invalid column_map JSON: {e}")})
                    .to_string()
            }
        };

    let display: Option<DisplaySpec> = if display_json.is_empty() || display_json == "{}" {
        None
    } else {
        match serde_json::from_str(display_json) {
            Ok(d) => Some(d),
            Err(e) => {
                return serde_json::json!({"error": format!("invalid display JSON: {e}")})
                    .to_string()
            }
        }
    };

    let delimiter = match delimiter_str {
        "," => ',',
        _ => '\t',
    };

    match local_plot_spec(content, report_type, &column_map, display, delimiter) {
        Ok(spec) => serde_json::to_string(&spec).unwrap_or_else(|e| {
            serde_json::json!({"error": format!("serialisation error: {e}")}).to_string()
        }),
        Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Resolve an axis column name from the column map, falling back to the
/// positional default header index.
fn resolve_column(
    axis: &str,
    column_map: &HashMap<String, String>,
    headers: &[String],
    position: usize,
) -> Result<String, LocalReportError> {
    if let Some(col) = column_map.get(axis) {
        // Validate the explicit name is present in the headers.
        if !headers.contains(col) {
            return Err(LocalReportError::MissingColumn {
                axis: axis.to_string(),
                column: col.clone(),
            });
        }
        return Ok(col.clone());
    }
    headers
        .get(position)
        .cloned()
        .ok_or_else(|| LocalReportError::MissingColumn {
            axis: axis.to_string(),
            column: format!("position {position}"),
        })
}

/// Extract all finite `f64` values from a column.
///
/// Returns an error when any non-null cell fails to parse as a number.
fn extract_numeric_column(
    rows: &[HashMap<String, Value>],
    col: &str,
) -> Result<Vec<f64>, LocalReportError> {
    let mut values = Vec::with_capacity(rows.len());
    for row in rows {
        match row.get(col) {
            None | Some(Value::Null) => {}
            Some(Value::Number(n)) => {
                if let Some(f) = n.as_f64() {
                    values.push(f);
                }
            }
            Some(_) => {
                return Err(LocalReportError::NonNumericColumn {
                    axis: col.to_string(),
                    column: col.to_string(),
                })
            }
        }
    }
    Ok(values)
}

/// Build an [`AxisMeta`] with computed domain from a slice of values.
fn build_axis_meta(col: &str, values: &[f64], value_type: &str) -> AxisMeta {
    use crate::report::display::TickLabelPlacement;

    let (min, max) = if values.is_empty() {
        (0.0, 1.0)
    } else {
        let min = values
            .iter()
            .copied()
            .filter(|v| v.is_finite())
            .fold(f64::INFINITY, f64::min);
        let max = values
            .iter()
            .copied()
            .filter(|v| v.is_finite())
            .fold(f64::NEG_INFINITY, f64::max);
        (min, if max > min { max } else { min + 1.0 })
    };

    AxisMeta {
        field: col.to_string(),
        label: None,
        scale: "linear".to_string(),
        domain: [min, max],
        tick_values: vec![],
        tick_labels: vec![],
        value_type: value_type.to_string(),
        tick_label_placement: TickLabelPlacement::OnTick,
        tick_label_stride: 1,
        tick_label_max_length: None,
    }
}

/// Serialise row data into a JSON `Value` suitable for [`PlotSpec::data`].
///
/// Produces `{"rows": [...]}` with each row containing the x (and optionally
/// y) column values.
fn rows_to_data_value(rows: &[HashMap<String, Value>], x_col: &str, y_col: Option<&str>) -> Value {
    let data_rows: Vec<Value> = rows
        .iter()
        .map(|row| {
            let mut entry = serde_json::Map::new();
            if let Some(v) = row.get(x_col) {
                entry.insert(x_col.to_string(), v.clone());
            }
            if let Some(yc) = y_col {
                if let Some(v) = row.get(yc) {
                    entry.insert(yc.to_string(), v.clone());
                }
            }
            Value::Object(entry)
        })
        .collect();
    serde_json::json!({"rows": data_rows})
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const HISTOGRAM_TSV: &str = "genome_size\n1000000\n5000000\n10000000\n";
    const SCATTER_TSV: &str = "genome_size\tc_value\n1000000\t0.5\n5000000\t1.2\n10000000\t3.0\n";

    #[test]
    fn histogram_from_tsv_returns_plot_spec() {
        let spec = local_plot_spec(
            HISTOGRAM_TSV,
            PlotReportType::Histogram,
            &HashMap::new(),
            None,
            '\t',
        )
        .unwrap();
        assert_eq!(spec.report_type, PlotReportType::Histogram);
        let x = spec.x.as_ref().unwrap();
        assert_eq!(x.field, "genome_size");
        assert!(x.domain[0] <= 1_000_000.0);
        assert!(x.domain[1] >= 10_000_000.0);
        assert!(spec.y.is_none());
    }

    #[test]
    fn scatter_from_tsv_returns_x_and_y() {
        let spec = local_plot_spec(
            SCATTER_TSV,
            PlotReportType::Scatter,
            &HashMap::new(),
            None,
            '\t',
        )
        .unwrap();
        assert_eq!(spec.report_type, PlotReportType::Scatter);
        assert!(spec.x.is_some());
        assert_eq!(spec.y.as_ref().unwrap().field, "c_value");
    }

    #[test]
    fn column_map_overrides_positional_defaults() {
        let mut map = HashMap::new();
        map.insert("x".to_string(), "c_value".to_string());
        map.insert("y".to_string(), "genome_size".to_string());
        let spec = local_plot_spec(SCATTER_TSV, PlotReportType::Scatter, &map, None, '\t').unwrap();
        assert_eq!(spec.x.as_ref().unwrap().field, "c_value");
        assert_eq!(spec.y.as_ref().unwrap().field, "genome_size");
    }

    #[test]
    fn missing_column_returns_error() {
        let mut map = HashMap::new();
        map.insert("x".to_string(), "nonexistent".to_string());
        let err = local_plot_spec(HISTOGRAM_TSV, PlotReportType::Histogram, &map, None, '\t')
            .unwrap_err();
        assert!(matches!(err, LocalReportError::MissingColumn { .. }));
    }

    #[test]
    fn unsupported_report_type_returns_error() {
        let err = local_plot_spec(
            HISTOGRAM_TSV,
            PlotReportType::Tree,
            &HashMap::new(),
            None,
            '\t',
        )
        .unwrap_err();
        assert!(matches!(err, LocalReportError::UnsupportedReportType(_)));
    }

    #[test]
    fn local_plot_spec_json_returns_valid_json() {
        let json = local_plot_spec_json(HISTOGRAM_TSV, "histogram", "{}", "{}", "\t");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("error").is_none(), "unexpected error: {v}");
        assert_eq!(v["report_type"], "histogram");
    }

    #[test]
    fn local_plot_spec_json_invalid_report_type() {
        let json = local_plot_spec_json(HISTOGRAM_TSV, "unknown_type", "{}", "{}", "\t");
        let v: Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("error").is_some());
    }
}
