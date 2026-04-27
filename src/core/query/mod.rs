//! Query builder for genomehubs search APIs.
//!
//! All query types and URL building logic live in the `genomehubs-query` subcrate
//! (WASM-compatible). This module re-exports them so the rest of cli-generator
//! continues to use `crate::core::query::*`.
//!
//! Validation logic (which depends on generated `phf::Map` tables) lives here
//! rather than in the subcrate since it has additional compile-time dependencies.

pub use genomehubs_query::query::{
    attributes, build_query_url, build_ui_url, identifiers, url, Attribute, AttributeOperator,
    AttributeSet, AttributeValue, Field, Identifiers, Modifier, QueryParams, SearchIndex,
    SearchQuery, SortOrder, TaxaIdentifier, TaxonFilterType,
};

pub mod adapter;
pub mod validation;
