//! Configuration types for the positional report family.
//!
//! Covers oxford (2-assembly dot-plot), ribbon (N-assembly synteny), and
//! painting (single-assembly chromosome colour map).

use serde::{Deserialize, Serialize};

/// Which positional report sub-type to produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PositionalReportType {
    /// Two-assembly Oxford dot-plot.
    Oxford,
    /// N-assembly ribbon / synteny diagram.
    Ribbon,
    /// Single-assembly chromosome painting map.
    Painting,
}

/// Configuration for a `POST /api/v3/positional` request.
///
/// Supplied as the `positional_yaml` field in the request body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionalSpec {
    /// Which sub-type to produce.
    pub report: PositionalReportType,
    /// Field used as shared marker identifier (e.g. `"busco_gene"`).
    pub group_by: String,
    /// Explicit assembly IDs.  When empty, assemblies are resolved from the
    /// query (assembly index) or the request fails with a clear error.
    #[serde(default)]
    pub assemblies: Vec<String>,
    /// `feature_type` value to filter features on (e.g. `"busco_gene"`).
    /// When absent, defaults to `group_by` so only the relevant feature type
    /// is returned.
    #[serde(default)]
    pub feature_type: Option<String>,
    /// Window size in bp for regional grouping.  `None` returns individual positions.
    #[serde(default)]
    pub window_size: Option<u64>,
    /// Auto-orient comparison sequences relative to the reference assembly.
    #[serde(default = "default_true")]
    pub reorient: bool,
    /// Maximum features to fetch from ES.  Hard cap; default 10 000.
    #[serde(default = "default_max_features")]
    pub max_features: usize,
    /// Optional category field for colour (e.g. `"busco_status"`).
    #[serde(default)]
    pub cat: Option<String>,
    /// Category axis options in the standard axis DSL string.
    ///
    /// Specify category values explicitly, e.g.:
    /// `"complete,fragmented,missing;;5"` — three fixed categories, size 5.
    ///
    /// Do **not** use the shorthand-only form `";;5+"` without listing category
    /// values; the positional endpoint uses explicit categories for colour mapping.
    #[serde(default)]
    pub cat_opts: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_max_features() -> usize {
    10_000
}
