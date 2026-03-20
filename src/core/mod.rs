//! Core library logic for the cli-generator tool.
//!
//! Submodules:
//!
//! - [`config`]  — YAML configuration types for site and CLI options.
//! - [`codegen`] — Code generation via Tera templates.
//! - [`describe`] — Human-readable descriptions of queries.
//! - [`fetch`]   — API field fetching and local disk caching.
//! - [`query`]   — `SearchQuery` / `QueryParams` structs and URL builder.
//! - [`snippet`]   — Code snippet generation for all languages.
//!
//! No PyO3 or clap dependencies live here.

pub mod codegen;
pub mod config;
pub mod describe;
pub mod fetch;
pub mod query;
pub mod snippet;
