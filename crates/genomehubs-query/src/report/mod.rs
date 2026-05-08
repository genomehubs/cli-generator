//! Report axis type system.
//!
//! All types in this module are serialisable and WASM-compatible.
//! They express the full configuration space for a single report axis —
//! what field to aggregate, how to bin it, and how to present the result.

pub mod axis;
pub mod bounds;

pub use axis::{
    AxisOpts, AxisRole, AxisSpec, AxisSummary, DateInterval, Scale, SortMode, ValueType,
};
pub use bounds::BoundsResult;

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
    pub fn from_str(s: &str) -> Option<Self> {
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
