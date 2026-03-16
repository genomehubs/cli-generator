//! Core library logic for the cli-generator tool.
//!
//! Submodules:
//!
//! - [`config`]  — YAML configuration types for site and CLI options.
//! - [`fetch`]   — API field fetching and local disk caching.
//! - [`codegen`] — Code generation via Tera templates.
//!
//! No PyO3 or clap dependencies live here.

pub mod codegen;
pub mod config;
pub mod fetch;
