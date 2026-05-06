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
