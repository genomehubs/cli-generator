//! Server-side report infrastructure.
//!
//! This module provides three layers:
//!
//! 1. `bounds` — probe ES for a field's actual domain and cardinality
//! 2. `agg` — build ES aggregation bodies
//! 3. `pipeline` — transform raw ES bucket responses into plot-ready data
//!
//! Report route handlers (Phase 6) wire these together into complete report workflows.

pub mod agg;
pub mod arc;
pub mod bounds;
pub mod filter_expr;
pub mod pipeline;
pub mod positional;
pub mod report_types;
