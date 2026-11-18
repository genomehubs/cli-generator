//! Core library logic for the cli-generator tool.
//!
//! Submodules:
//!
//! - [`config`]  — YAML configuration types for site and CLI options.
//! - [`codegen`] — Code generation via Tera templates.
//! - [`fetch`]   — API field fetching and local disk caching.
//! - [`query`]   — `SearchQuery` / `QueryParams` structs and URL builder (via genomehubs-query).
//! - [`snippet`]   — Code snippet generation for all languages.
//!
//! No PyO3 or clap dependencies live here.

pub mod codegen;
pub mod config;
pub mod fetch;
pub mod metadata;
pub mod query;
pub mod runtime_query;
pub mod snippet;
