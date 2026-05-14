//! Configuration types for the positional report family.
//!
//! Covers oxford (2-assembly dot-plot), ribbon (N-assembly synteny), painting
//! (single-assembly chromosome colour map), and circos.

use std::collections::HashMap;

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
    /// Circos arc diagram — supports within-assembly and cross-assembly connections.
    Circos,
}

// ── AttributeFilter ───────────────────────────────────────────────────────────

/// Comparison operator for an attribute filter predicate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOperator {
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
    In,
    /// Aggregate count — used for Type C cross-feature-type chains.
    GteCount,
}

/// Value side of an attribute filter predicate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FilterValue {
    /// Numeric scalar.
    Scalar(f64),
    /// String scalar.
    Text(String),
    /// Multi-value list (used with `In`).
    List(Vec<String>),
}

/// Which entity a filter targets, driving the chain query strategy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "target")]
pub enum FilterTarget {
    /// Direct predicate on feature attributes — no chain query needed.
    Feature,
    /// Type A chain — resolves `sequence_id` values from toplevel features first.
    Sequence,
    /// Type B chain — resolves `container_id` values from window features first.
    Window {
        /// Which window resolution to query (`1_000_000` → `window_1m`, etc.).
        /// `None` = auto-detect coarsest available.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window_size: Option<u64>,
        /// Minimum fraction of feature length inside the window.
        /// `None` = ANY overlap (current default).  Reserved for future phases.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        overlap_threshold: Option<f64>,
    },
    /// Type C chain — resolves a name/id set from a different feature type first.
    FeatureType {
        feature_type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        assembly_id: Option<String>,
    },
}

/// A single attribute filter predicate, optionally driving a chain query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttributeFilter {
    /// Attribute key or top-level field name.
    pub field: String,
    /// Comparison operator.
    pub operator: FilterOperator,
    /// Value to compare against.
    pub value: FilterValue,
    /// Which entity to apply the filter to (drives chain query type).
    #[serde(flatten)]
    pub target: FilterTarget,
}

// ── RegionsSpec ───────────────────────────────────────────────────────────────

/// How region boundaries are placed between adjacent feature runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RegionBounds {
    /// `[first_feat.start, last_feat.end]` — exact feature extent.
    #[default]
    FeatureEnds,
    /// Boundaries at midpoints between adjacent feature ends and starts.
    Midpoints,
}

/// Configuration for server-side region computation.
///
/// Supplied as the `regions` key inside `positional_yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionsSpec {
    /// Attribute key whose value groups features into runs (e.g. `"merian_unit"`).
    /// Mutually exclusive with `name_to_cat`; one must be set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cat: Option<String>,
    /// Explicit feature-name → category map.  When set, `cat` is ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_to_cat: Option<HashMap<String, String>>,
    /// Boundary placement algorithm.
    #[serde(default)]
    pub bounds: RegionBounds,
    /// Minimum features per region (regions with fewer features are merged into neighbours).
    #[serde(default = "default_min_features")]
    pub min_features: usize,
    /// Hard cap on how far a midpoint boundary may expand beyond the nearest feature edge,
    /// in base-pairs.  `None` = unlimited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_expansion: Option<u64>,
}

fn default_min_features() -> usize {
    1
}

// ── PositionalSpec ────────────────────────────────────────────────────────────

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
    #[serde(default)]
    pub cat_opts: Option<String>,
    /// Attribute filter predicates.  Each entry may drive a direct filter or a
    /// chain query depending on its `target`.
    #[serde(default)]
    pub filter: Vec<AttributeFilter>,
    /// Region computation configuration.  When present, the response includes
    /// a `regions` key containing computed region intervals.
    #[serde(default)]
    pub regions: Option<RegionsSpec>,
    /// Hard cap on connections emitted per group for M:N feature mappings.
    /// `None` uses the server default (25).
    #[serde(default)]
    pub max_connections_per_group: Option<usize>,
}

fn default_true() -> bool {
    true
}

fn default_max_features() -> usize {
    10_000
}
