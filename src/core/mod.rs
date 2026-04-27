//! Core library logic for the cli-generator tool.
//!
//! Submodules:
//!
//! - [`config`]  — YAML configuration types for site and CLI options.
//! - [`codegen`] — Code generation via Tera templates.
//! - [`describe`] — Human-readable descriptions of queries.
//! - [`fetch`]   — API field fetching and local disk caching.
//! - [`query`]   — `SearchQuery` / `QueryParams` structs and URL builder (via genomehubs-query).
//! - [`snippet`]   — Code snippet generation for all languages.
//!
//! No PyO3 or clap dependencies live here.

pub mod attr_types;
pub mod codegen;
pub mod config;
pub mod count;
pub mod describe;
pub mod fetch;
pub mod process_hits;
pub mod query;
pub mod query_builder;
pub mod snippet;
