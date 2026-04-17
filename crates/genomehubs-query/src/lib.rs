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
