//! Positional report infrastructure (oxford / ribbon / painting).
//!
//! This module handles:
//! - Feature record parsing from ES `_source.attributes`
//! - Sequence layout: ordering, orientation, cumulative offset computation
//! - Regional windowing (grouping individual positions into intervals)
//! - Painting-mode segment shaping
//! - Region computation (collapsing features into categorical intervals)

pub mod feature_query;
pub mod layout;
pub mod painter;
pub mod region;
pub mod window;
