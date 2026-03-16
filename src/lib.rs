//! Python extension module entry point.
//!
//! This file wires Rust functions to their Python-callable counterparts using PyO3.
//! All actual logic lives in `core`; this file only handles the FFI boundary and
//! any necessary type conversions.
//!
//! # Exposing a new function to Python
//! 1. Implement the logic in `src/core/`.
//! 2. Add a thin `#[pyfunction]` wrapper here that calls into `core`.
//! 3. Register the wrapper with `m.add_function(...)` inside the module init.
//! 4. Add a typed signature to `python/cli_generator/cli_generator.pyi`.
//! 5. Re-export from `python/cli_generator/__init__.py`.

use pyo3::prelude::*;

pub mod cli_meta;
pub mod commands;
pub mod core;

// Generated code lives in src/generated/. Hand-written code never goes there.
pub mod generated {}

/// Return the cli-generator version string.
/// Exposed to Python as `cli_generator.version()`.
#[pyfunction]
fn version() -> &'static str {
    cli_meta::VERSION
}

/// Python module definition for `cli_generator`.
#[pymodule]
fn cli_generator(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    Ok(())
}
