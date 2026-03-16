//! CLI metadata constants.
//!
//! Single source of truth for the CLI name and description.
//!
//! # CLI generator note
//! When using the CLI generator tool, **only this file** is regenerated on config
//! update. Hand-written logic in `core/` and `lib.rs` is never overwritten by
//! the generator. Keep all generator-controlled strings here and reference them
//! from `main.rs` via the constants below.

/// Application name as displayed in `--help` output.
pub const NAME: &str = "cli-generator";

/// Short description shown in `--help`.
pub const ABOUT: &str = "Generic CLI generator for genomehubs instances";

/// Crate version from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
