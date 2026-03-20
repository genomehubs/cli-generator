//! Python extension module entry point.
//!
//! This file wires Rust functions to their Python-callable counterparts using PyO3.
//! All actual logic lives in `core`; this file only handles the FFI boundary and
//! any necessary type conversions.
//!
//! The PyO3 bindings are only compiled when the `extension-module` feature is
//! enabled (i.e. when maturin is building a Python wheel).  Plain `cargo build`
//! and `cargo run` therefore do not link against libpython.
//!
//! # Exposing a new function to Python
//! 1. Implement the logic in `src/core/`.
//! 2. Add a thin `#[pyfunction]` wrapper here that calls into `core`.
//! 3. Register the wrapper with `m.add_function(...)` inside the module init.
//! 4. Add a typed signature to `python/cli_generator/cli_generator.pyi`.
//! 5. Re-export from `python/cli_generator/__init__.py`.

pub mod cli_meta;
pub mod commands;
pub mod core;

// Generated code lives in src/generated/. Hand-written code never goes there.
pub mod generated {}

#[cfg(feature = "extension-module")]
use pyo3::prelude::*;

/// Return the cli-generator version string.
/// Exposed to Python as `cli_generator.version()`.
#[cfg(feature = "extension-module")]
#[pyfunction]
fn version() -> &'static str {
    cli_meta::VERSION
}

/// Build a fully-encoded genomehubs API query URL from YAML inputs.
///
/// Both `query_yaml` and `params_yaml` are serialised [`core::query::SearchQuery`] /
/// [`core::query::QueryParams`] strings respectively.  Returns the complete URL
/// including all query parameters ready to pass to an HTTP client.
///
/// Raises `ValueError` when either YAML string cannot be parsed.
#[cfg(feature = "extension-module")]
#[pyfunction]
fn build_url(
    query_yaml: &str,
    params_yaml: &str,
    api_base: &str,
    api_version: &str,
    endpoint: &str,
) -> PyResult<String> {
    use crate::core::query::{build_query_url, QueryParams, SearchQuery};
    let query = SearchQuery::from_yaml(query_yaml)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let params = QueryParams::from_yaml(params_yaml)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    Ok(build_query_url(
        &query,
        &params,
        api_base,
        api_version,
        endpoint,
    ))
}

/// Describe a query in human-readable form, returning a string suitable for CLI help messages.
#[cfg(feature = "extension-module")]
#[pyfunction]
#[pyo3(signature = (query_yaml, params_yaml, field_metadata_json, mode = "concise"))]
fn describe_query(
    query_yaml: &str,
    params_yaml: &str,
    field_metadata_json: &str,
    mode: &str,
) -> PyResult<String> {
    let query: SearchQuery = serde_yaml::from_str(query_yaml)
        .map_err(|e| PyValueError::new_err(format!("Invalid query YAML: {}", e)))?;

    // Parse field metadata from JSON (populated from API's resultFields endpoint)
    let field_metadata: HashMap<String, FieldDef> = serde_json::from_str(field_metadata_json)
        .map_err(|e| PyValueError::new_err(format!("Invalid field metadata JSON: {}", e)))?;

    let describer = QueryDescriber::new(field_metadata);

    let result = match mode {
        "verbose" => describer.describe_verbose(&query),
        _ => describer.describe_concise(&query),
    };

    Ok(result)
}

/// Python module definition for `cli_generator`.
#[cfg(feature = "extension-module")]
#[pymodule]
fn cli_generator(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(build_url, m)?)?;
    m.add_function(wrap_pyfunction!(describe_query, m)?)?;
    Ok(())
}
