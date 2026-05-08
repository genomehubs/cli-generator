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

#![allow(clippy::useless_conversion)]

pub mod cli_meta;
pub mod commands;
pub mod core;

// Generated code lives in src/generated/. Hand-written code never goes there.
pub mod generated {}

#[cfg(feature = "extension-module")]
use pyo3::prelude::*;

#[cfg(feature = "extension-module")]
use pyo3::exceptions::{PyRuntimeError, PyValueError};

#[cfg(feature = "extension-module")]
use std::collections::HashMap;

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

/// Build a fully-encoded genomehubs UI URL from YAML inputs.
///
/// Produces the same query parameters as `build_url` but targets the web
/// interface rather than the REST API — no version component is inserted,
/// so the result is `{ui_base}/{endpoint}?result=…&query=…`.
///
/// Raises `ValueError` when either YAML string cannot be parsed.
#[cfg(feature = "extension-module")]
#[pyfunction]
fn build_ui_url(
    query_yaml: &str,
    params_yaml: &str,
    ui_base: &str,
    endpoint: &str,
) -> PyResult<String> {
    use crate::core::query::{build_ui_url as _build_ui_url, QueryParams, SearchQuery};
    let query = SearchQuery::from_yaml(query_yaml)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let params = QueryParams::from_yaml(params_yaml)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    Ok(_build_ui_url(&query, &params, ui_base, endpoint))
}

/// Describe a query in human-readable form, returning a string suitable for CLI help messages.
#[cfg(feature = "extension-module")]
#[pyfunction]
#[allow(unused_variables)] // params_yaml reserved for future use; kept for API stability
#[pyo3(signature = (query_yaml, params_yaml, field_metadata_json, mode = "concise"))]
fn describe_query(
    query_yaml: &str,
    params_yaml: &str,
    field_metadata_json: &str,
    mode: &str,
) -> PyResult<String> {
    use crate::core::describe::QueryDescriber;
    use crate::core::fetch::FieldDef;
    use crate::core::query::SearchQuery;

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

/// Render code snippets for a query in one or more languages.
///
/// Accepts a JSON-serialised [`core::snippet::QuerySnapshot`] and minimal site
/// parameters, and returns a JSON object mapping each requested language name
/// to its rendered code snippet string.
///
/// `languages` is a comma-separated list of language keys, e.g. `"python"` or
/// `"python,r"`.  Each key must match a loaded snippet template.
///
/// Raises `ValueError` when the snapshot JSON cannot be parsed.
/// Raises `RuntimeError` when template rendering fails.
#[cfg(feature = "extension-module")]
#[pyfunction]
#[pyo3(signature = (snapshot_json, site_name, api_base, sdk_name, languages = "python"))]
fn render_snippet(
    snapshot_json: &str,
    site_name: &str,
    api_base: &str,
    sdk_name: &str,
    languages: &str,
) -> PyResult<String> {
    use crate::core::config::SiteConfig;
    use crate::core::snippet::{QuerySnapshot, SnippetGenerator};

    let snapshot: QuerySnapshot = serde_json::from_str(snapshot_json)
        .map_err(|e| PyValueError::new_err(format!("Invalid snapshot JSON: {}", e)))?;

    let site = SiteConfig {
        name: site_name.to_string(),
        api_base: api_base.to_string(),
        sdk_name: Some(sdk_name.to_string()),
        ..Default::default()
    };

    let lang_list: Vec<&str> = languages.split(',').map(str::trim).collect();

    let generator = SnippetGenerator::new().map_err(|e| {
        PyRuntimeError::new_err(format!("Failed to initialise snippet generator: {}", e))
    })?;

    let snippets = generator
        .render_all_snippets(&snapshot, &site, &lang_list)
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to render snippet: {}", e)))?;

    serde_json::to_string(&snippets)
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to serialise snippets: {}", e)))
}

/// Parse the `status` block from a raw genomehubs API JSON response.
///
/// Returns a compact JSON string: `{"hits":N,"ok":true|false,"error":null|"msg"}`.
/// On completely invalid JSON, returns an error-flagged JSON object rather than raising.
#[cfg(feature = "extension-module")]
#[pyfunction]
fn parse_response_status(raw: &str) -> String {
    genomehubs_query::parse_response_status(raw)
}

/// Parse a raw genomehubs `/search` JSON response into a flat record array.
///
/// Returns a compact JSON array string where each element is one flat record.
/// See [`genomehubs_query::parse_search_json`] for the full column specification.
#[cfg(feature = "extension-module")]
#[pyfunction]
fn parse_search_json(raw: &str) -> String {
    genomehubs_query::parse_search_json(raw)
}

/// Add `{field}_label` columns to already-flat parsed records.
///
/// `records_json` must be the output of [`parse_search_json`].
/// `mode` is `"all"`, `"non_direct"` (default), or `"ancestral_only"`.
#[cfg(feature = "extension-module")]
#[pyfunction]
#[pyo3(signature = (records_json, mode = "non_direct"))]
fn annotate_source_labels(records_json: &str, mode: &str) -> String {
    genomehubs_query::annotate_source_labels(records_json, mode)
}

/// Reshape flat parsed records into split-source columns.
///
/// `records_json` must be the output of [`parse_search_json`].  Each
/// `{field}` / `{field}__source` pair becomes `{field}__direct`,
/// `{field}__descendant`, and `{field}__ancestral`.
#[cfg(feature = "extension-module")]
#[pyfunction]
fn split_source_columns(records_json: &str) -> String {
    genomehubs_query::split_source_columns(records_json)
}

/// Strip all `__*` sub-key columns from flat records.
///
/// `records_json` must be the output of [`parse_search_json`].  Columns like
/// `{field}__source`, `{field}__min`, `{field}__label`, and `{field}__direct`
/// are removed; bare `{field}` values and identity columns are preserved.
///
/// `keep_columns_json` is a JSON array of column names to preserve despite
/// containing `__`, e.g. `'["assembly_span__min"]'`.  Default: `""`
/// (strip all).
#[cfg(feature = "extension-module")]
#[pyfunction]
#[pyo3(signature = (records_json, keep_columns_json = ""))]
fn values_only(records_json: &str, keep_columns_json: &str) -> String {
    genomehubs_query::values_only(records_json, keep_columns_json)
}

/// Return records with non-direct values replaced by their annotated label.
///
/// Chains `annotate_source_labels` then promotes each `{field}__label` into
/// `{field}`, then strips all remaining `__*` metadata columns.
/// `mode` is `"all"`, `"non_direct"` (default), or `"ancestral_only"`.
///
/// `keep_columns_json` is a JSON array of column names to preserve after
/// stripping, e.g. `'["assembly_span__min"]'`.  Default: `""` (strip all).
#[cfg(feature = "extension-module")]
#[pyfunction]
#[pyo3(signature = (records_json, mode = "non_direct", keep_columns_json = ""))]
fn annotated_values(records_json: &str, mode: &str, keep_columns_json: &str) -> String {
    genomehubs_query::annotated_values(records_json, mode, keep_columns_json)
}

/// Reshape flat records into long/tidy format — one row per field per record.
///
/// Accepts the JSON array produced by `parse_search_json`.  Each output row
/// contains identity columns, `"field"`, `"value"`, and `"source"`.
/// Explicitly-requested modifier columns are emitted with `"field"` as
/// `"{bare}:{modifier}"`.
#[cfg(feature = "extension-module")]
#[pyfunction]
fn to_tidy_records(records_json: &str) -> String {
    genomehubs_query::to_tidy_records(records_json)
}

/// Parse one page from a `/searchPaginated` response.
///
/// Returns a JSON object with `"records"` (flat, same format as
/// `parse_search_json`), `"hasMore"` (bool), `"searchAfter"` (array or null),
/// and `"totalHits"` (int).
#[cfg(feature = "extension-module")]
#[pyfunction]
fn parse_paginated_json(raw: &str) -> String {
    genomehubs_query::parse_paginated_json(raw)
}

/// Parse a raw batch search (`/msearch`) response into per-query flat record lists.
///
/// The genomehubs `/msearch` endpoint accepts multiple queries in a single POST
/// and returns results grouped by query.  Returns a JSON object with `"results"`
/// (array of per-query objects each containing `"records"`, `"total"`, and
/// `"error"`) and `"totalHits"`.
#[cfg(feature = "extension-module")]
#[pyfunction]
fn parse_batch_json(raw: &str) -> String {
    genomehubs_query::parse_batch_json(raw)
}

/// Parse the `records` array from a raw `/record` API response.
///
/// Returns a JSON array string of flat record dicts with all `_source` fields
/// merged with envelope fields (`recordId`, `result`).
///
/// Raises `ValueError` on parse failure.
#[cfg(feature = "extension-module")]
#[pyfunction]
fn parse_record_json(raw: &str) -> String {
    genomehubs_query::parse_record_json(raw)
}

/// Parse the `results` array from a raw `/lookup` API response.
///
/// Returns a JSON array string of candidate dicts with id, name, rank, and reason.
///
/// Raises `ValueError` on parse failure.
#[cfg(feature = "extension-module")]
#[pyfunction]
fn parse_lookup_json(raw: &str) -> String {
    genomehubs_query::parse_lookup_json(raw)
}

/// Extract histogram buckets from a raw `/report` JSON response.
///
/// Returns a compact JSON array of bucket objects.
/// Each bucket retains its `by_cat` entries when categorised data is present.
///
/// Returns `{"error":"..."}` if the input is not valid JSON or `report.buckets` is absent.
#[cfg(feature = "extension-module")]
#[pyfunction]
fn parse_histogram_json(raw: &str) -> String {
    genomehubs_query::parse_histogram_json(raw)
}

/// Flatten a tree report's `treeNodes` map into a JSON array.
///
/// Each element contains `taxon_id`, `scientific_name`, `taxon_rank`, `count`,
/// `descendant_count` (null when absent), `status`, `cat`, `children` (sorted
/// taxon_id array), and `fields`.
///
/// Returns `{"error":"..."}` if the input is not valid JSON or `report.treeNodes` is absent.
#[cfg(feature = "extension-module")]
#[pyfunction]
fn parse_tree_json(raw: &str) -> String {
    genomehubs_query::parse_tree_json(raw)
}

/// Validate a query against field metadata and configuration.
///
/// Accepts YAML representations of the query and field metadata as JSON, and
/// returns a JSON array of error strings. An empty array means the query is
/// valid.
///
/// Arguments:
/// - `query_yaml`: YAML for SearchQuery
/// - `field_metadata_json`: JSON mapping field names to metadata
/// - `validation_config_json`: JSON for ValidationConfig
/// - `synonyms_json`: JSON mapping attribute synonyms (or `{}` for none)
///
/// Returns: JSON array of error strings, or `["error: ..."]` if parsing fails.
#[cfg(feature = "extension-module")]
#[pyfunction]
fn validate_query_json(
    query_yaml: &str,
    field_metadata_json: &str,
    validation_config_json: &str,
    synonyms_json: &str,
) -> String {
    genomehubs_query::validate_query_json(
        query_yaml,
        field_metadata_json,
        validation_config_json,
        synonyms_json,
    )
}

/// Validate a report YAML string against known report type rules.
///
/// Returns a JSON array of error strings (empty array `[]` if valid).
/// Pass `"{}"` for `field_meta_json` to skip field-name validation.
#[cfg(feature = "extension-module")]
#[pyfunction]
fn validate_report_yaml(report_yaml: &str, field_meta_json: &str) -> String {
    genomehubs_query::validation::validate_report_yaml(report_yaml, field_meta_json)
}

/// Parse a v2 API or UI URL into `(query_yaml, params_yaml)`.
///
/// Handles both structured params (`tax_name=`, `fields=`, `result=`, …) and
/// the composite `query=` fragment form produced by the GoaT API.
///
/// Returns a `(query_yaml, params_yaml)` tuple, both as YAML strings.
///
/// Raises `ValueError` on parse or serialisation failure.
#[cfg(feature = "extension-module")]
#[pyfunction]
fn query_yaml_from_url_params(url: &str) -> PyResult<(String, String)> {
    genomehubs_query::query::query_yaml_from_url_params(url).map_err(|e| PyValueError::new_err(e))
}

/// Parse a v2 report URL into `(query_yaml, params_yaml, report_yaml)`.
///
/// The URL must contain a `report=` parameter or have a `/report` path suffix.
///
/// Returns a `(query_yaml, params_yaml, report_yaml)` triple, all as YAML strings.
///
/// Raises `ValueError` when the `report=` parameter is absent or serialisation fails.
#[cfg(feature = "extension-module")]
#[pyfunction]
fn report_yaml_from_url_params(url: &str) -> PyResult<(String, String, String)> {
    genomehubs_query::report::report_yaml_from_url_params(url).map_err(|e| PyValueError::new_err(e))
}

/// Python module definition for `cli_generator`.

#[cfg(feature = "extension-module")]
#[pymodule]
fn cli_generator(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(build_url, m)?)?;
    m.add_function(wrap_pyfunction!(build_ui_url, m)?)?;
    m.add_function(wrap_pyfunction!(describe_query, m)?)?;
    m.add_function(wrap_pyfunction!(render_snippet, m)?)?;
    m.add_function(wrap_pyfunction!(validate_query_json, m)?)?;
    m.add_function(wrap_pyfunction!(validate_report_yaml, m)?)?;
    m.add_function(wrap_pyfunction!(parse_response_status, m)?)?;
    m.add_function(wrap_pyfunction!(parse_search_json, m)?)?;
    m.add_function(wrap_pyfunction!(annotate_source_labels, m)?)?;
    m.add_function(wrap_pyfunction!(split_source_columns, m)?)?;
    m.add_function(wrap_pyfunction!(values_only, m)?)?;
    m.add_function(wrap_pyfunction!(annotated_values, m)?)?;
    m.add_function(wrap_pyfunction!(to_tidy_records, m)?)?;
    m.add_function(wrap_pyfunction!(parse_paginated_json, m)?)?;
    m.add_function(wrap_pyfunction!(parse_batch_json, m)?)?;
    m.add_function(wrap_pyfunction!(parse_record_json, m)?)?;
    m.add_function(wrap_pyfunction!(parse_lookup_json, m)?)?;
    m.add_function(wrap_pyfunction!(parse_histogram_json, m)?)?;
    m.add_function(wrap_pyfunction!(parse_tree_json, m)?)?;
    m.add_function(wrap_pyfunction!(query_yaml_from_url_params, m)?)?;
    m.add_function(wrap_pyfunction!(report_yaml_from_url_params, m)?)?;
    Ok(())
}
