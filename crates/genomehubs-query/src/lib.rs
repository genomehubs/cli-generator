//! Pure URL builder for genomehubs search API queries.
//!
//! This crate has no I/O dependencies and compiles to WebAssembly via wasm-pack.
//!
//! ## WebAssembly usage (JavaScript)
//! ```bash
//! wasm-pack build --target nodejs --features wasm
//! ```
//! ```javascript
//! const wasm = require('./pkg/genomehubs_query');
//! const url = wasm.build_url(queryYaml, paramsYaml, apiBase, apiVersion);
//! ```
//!
//! ## Rust usage
//! ```rust
//! use genomehubs_query::query::{SearchQuery, QueryParams, build_query_url};
//! ```

pub mod parse;
pub mod query;

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

/// Build a fully-encoded genomehubs API URL from YAML inputs.
///
/// This is the WASM-exported entry point that JavaScript calls via the generated bindings.
/// It parses YAML representations of [`query::SearchQuery`] and [`query::QueryParams`]
/// and delegates to the pure Rust [`query::build_query_url`] function.
///
/// # Arguments
/// - `query_yaml`: YAML for `SearchQuery` (index, taxa, attributes, fields, …)
/// - `params_yaml`: YAML for `QueryParams` (size, page, sort, taxonomy, …)
/// - `api_base`: e.g. `"https://goat.genomehubs.org/api"` (no trailing slash)
/// - `api_version`: e.g. `"v2"`
///
/// # Returns
/// The fully percent-encoded API URL, or an empty string on parse error.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn build_url(query_yaml: &str, params_yaml: &str, api_base: &str, api_version: &str) -> String {
    let query = match query::SearchQuery::from_yaml(query_yaml) {
        Ok(q) => q,
        Err(_) => return String::new(),
    };
    let params = match query::QueryParams::from_yaml(params_yaml) {
        Ok(p) => p,
        Err(_) => return String::new(),
    };
    query::build_query_url(&query, &params, api_base, api_version, "search")
}

/// Build an API URL for an arbitrary endpoint (e.g. `"search"`, `"searchPaginated"`, `"count"`).
///
/// Identical to [`build_url`] but accepts an explicit `endpoint` string.  Use
/// this when you need an endpoint other than the default `"search"`.
///
/// Returns the fully percent-encoded URL, or an empty string on parse error.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn build_url_for_endpoint(
    query_yaml: &str,
    params_yaml: &str,
    api_base: &str,
    api_version: &str,
    endpoint: &str,
) -> String {
    let query = match query::SearchQuery::from_yaml(query_yaml) {
        Ok(q) => q,
        Err(_) => return String::new(),
    };
    let params = match query::QueryParams::from_yaml(params_yaml) {
        Ok(p) => p,
        Err(_) => return String::new(),
    };
    query::build_query_url(&query, &params, api_base, api_version, endpoint)
}

/// Parse the `status` block from a raw genomehubs API JSON response.
///
/// Returns a compact JSON string: `{"hits":N,"ok":true|false,"error":null|"msg"}`.
/// This is the canonical way all SDK `count()` methods should read the hit count.
///
/// On completely invalid JSON, returns `{"hits":0,"ok":false,"error":"<message>"}`.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn parse_response_status(raw: &str) -> String {
    match parse::parse_response_status(raw) {
        Ok(status) => parse::response_status_to_json(&status),
        Err(e) => format!(r#"{{"hits":0,"ok":false,"error":{e:?}}}"#),
    }
}

/// Parse a raw genomehubs `/search` JSON response into a flat record array.
///
/// Returns a compact JSON array string.  Each element is one flat record with:
/// - Identity columns (`taxon_id`, `scientific_name`, `taxon_rank`, …)
/// - `{field}` — representative value (`null` for stub fields with no value)
/// - `{field}_source` — `"direct"`, `"ancestor"`, or `"descendant"` (taxon only)
/// - Stat sub-keys present on the raw object: `{field}_min`, `{field}_max`,
///   `{field}_median`, `{field}_mode`, `{field}_mean`, `{field}_count`,
///   `{field}_sp_count`, `{field}_from`, `{field}_to`, `{field}_length`
///
/// On error returns a JSON string `{"error":"..."}`.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn parse_search_json(raw: &str) -> String {
    match parse::parse_search_json(raw) {
        Ok(records) => records,
        Err(e) => format!(r#"{{"error":{e:?}}}"#),
    }
}

/// Add `{field}_label` columns to already-flat parsed records.
///
/// `records_json` is the output of [`parse_search_json`].
/// `mode` is one of `"all"`, `"non_direct"` (default), or `"ancestral_only"`.
///
/// Returns the annotated records JSON string, or `{"error":"..."}` on failure.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn annotate_source_labels(records_json: &str, mode: &str) -> String {
    match parse::annotate_source_labels(records_json, mode) {
        Ok(records) => records,
        Err(e) => format!(r#"{{"error":{e:?}}}"#),
    }
}

/// Reshape flat parsed records into split-source columns.
///
/// `records_json` is the output of [`parse_search_json`].  Each `{field}` /
/// `{field}__source` pair is replaced by `{field}__direct`, `{field}__descendant`,
/// and `{field}__ancestral`.
///
/// Returns the reshaped records JSON string, or `{"error":"..."}` on failure.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn split_source_columns(records_json: &str) -> String {
    match parse::split_source_columns(records_json) {
        Ok(records) => records,
        Err(e) => format!(r#"{{"error":{e:?}}}"#),
    }
}

/// Strip all `__*` sub-key columns from flat records, keeping only identity
/// columns and bare field values.
///
/// `records_json` is the output of [`parse_search_json`].  Columns like
/// `{field}__source`, `{field}__min`, `{field}__label`, `{field}__direct` are
/// removed; bare `{field}` values and identity columns are preserved.
///
/// `keep_columns_json` is a JSON array of column names to preserve despite
/// containing `__`, e.g. `'["assembly_span__min"]'`.  Pass `""` or `"[]"`
/// to strip all `__*` columns (the usual case).
///
/// Returns the stripped records JSON string, or `{"error":"..."}` on failure.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn values_only(records_json: &str, keep_columns_json: &str) -> String {
    match parse::values_only(records_json, keep_columns_json) {
        Ok(records) => records,
        Err(e) => format!(r#"{{"error":{e:?}}}"#),
    }
}

/// Return records with non-direct field values replaced by their annotated label.
///
/// Chains [`annotate_source_labels`] then for each `{field}__label` moves the
/// label string into `{field}`, then strips all remaining `__*` columns.
/// `mode` is one of `"all"`, `"non_direct"` (default), or `"ancestral_only"`.
///
/// `keep_columns_json` is a JSON array of column names to preserve after label
/// promotion, e.g. `'["assembly_span__min"]'`.  Pass `""` to strip all.
///
/// Returns the annotated records JSON string, or `{"error":"..."}` on failure.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn annotated_values(records_json: &str, mode: &str, keep_columns_json: &str) -> String {
    match parse::annotated_values(records_json, mode, keep_columns_json) {
        Ok(records) => records,
        Err(e) => format!(r#"{{"error":{e:?}}}"#),
    }
}

/// Reshape flat records produced by [`parse_search_json`] into long/tidy format.
///
/// Returns a JSON array with one row per field per source record.  Each row
/// contains the identity columns (`taxon_id`, `scientific_name`, …), plus
/// `"field"`, `"value"`, and `"source"`.  Explicitly-requested modifier columns
/// (from `field:modifier` requests) are emitted as separate rows whose `"field"`
/// is  `"{bare}:{modifier}"`.
///
/// Returns `{"error":"..."}` if `records_json` is not valid JSON.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn to_tidy_records(records_json: &str) -> String {
    match parse::to_tidy_records(records_json) {
        Ok(records) => records,
        Err(e) => format!(r#"{{"error":{e:?}}}"#),
    }
}

/// Parse one page from a `/searchPaginated` API response.
///
/// Returns a JSON object:
/// ```json
/// {
///   "records": [...],
///   "hasMore": true,
///   "searchAfter": [...],
///   "totalHits": 5000
/// }
/// ```
///
/// `records` contains flat records in the same format as [`parse_search_json`].
/// Use `searchAfter` as the `search_after` cursor for the next request.
///
/// Returns `{"error":"..."}` if the input is not valid JSON.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn parse_paginated_json(raw: &str) -> String {
    match parse::parse_paginated_json(raw) {
        Ok(page) => parse::paginated_page_to_json(&page),
        Err(e) => format!(r#"{{"error":{e:?}}}"#),
    }
}
