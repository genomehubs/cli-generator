//! Report axis type system.
//!
//! All types in this module are serialisable and WASM-compatible.
//! They express the full configuration space for a single report axis —
//! what field to aggregate, how to bin it, and how to present the result.

pub mod axis;
pub mod bounds;
pub mod display;
pub mod plot_spec;

pub use axis::{
    AxisOpts, AxisRole, AxisSpec, AxisSummary, DateInterval, Scale, SortMode, ValueType,
};
pub use bounds::BoundsResult;
pub use display::DisplaySpec;
pub use plot_spec::PlotSpec;

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
