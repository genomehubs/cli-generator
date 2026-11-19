//! Minimal Elasticsearch `count` helper for the Rust core.
//!
//! Provides a small, well-tested wrapper that posts an ES query to
//! `/{index}/_count` and returns the numeric hit count. This is intentionally
//! minimal so it can be used as a building block for higher-level API functions
//! (reports, aggregations) implemented later.

use crate::core::query_builder::build_count_body;
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde_json::Value;

/// Post `query` to `base_url/{index}/_count` and return the resulting count.
///
/// `base_url` should be the Elasticsearch base (e.g. `http://localhost:9200`).
pub fn count_docs(base_url: &str, index: &str, query: &Value) -> Result<u64> {
    let client = Client::new();
    let url = format!("{}/{}/_count", base_url.trim_end_matches('/'), index);

    let resp = client
        .post(&url)
        .json(query)
        .send()
        .with_context(|| format!("HTTP POST {url}"))?
        .error_for_status()
        .with_context(|| format!("non-2xx response from {url}"))?;

    let body: Value = resp.json().context("deserialising JSON response")?;
    let count = body
        .get("count")
        .and_then(|v| v.as_u64())
        .with_context(|| format!("missing numeric 'count' in response from {url}"))?;

    Ok(count)
}

/// Post `query` to `base_url/{index}/_count` and return the raw JSON response.
pub fn count_docs_raw(base_url: &str, index: &str, query: &Value) -> Result<Value> {
    let client = Client::new();
    let url = format!("{}/{}/_count", base_url.trim_end_matches('/'), index);

    let resp = client
        .post(&url)
        .json(query)
        .send()
        .with_context(|| format!("HTTP POST {url}"))?
        .error_for_status()
        .with_context(|| format!("non-2xx response from {url}"))?;

    let body: Value = resp.json().context("deserialising JSON response")?;
    Ok(body)
}

/// Convenience: build a minimal query body from a simple string and call `_count`.
pub fn count_docs_from_query_str(
    base_url: &str,
    index: &str,
    query_str: Option<&str>,
) -> Result<u64> {
    let body = build_count_body(query_str, false).context("building count body")?;
    count_docs(base_url, index, &body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    #[test]
    fn count_docs_returns_count_on_success() {
        let mut server = Server::new();
        let mock = server
            .mock("POST", "/taxon/_count")
            .with_status(200)
            .with_body(r#"{"count": 42}"#)
            .create();

        let base = server.url();
        let q = serde_json::json!({"query": {"match_all": {}}});
        let result = count_docs(&base, "taxon", &q).unwrap();
        assert_eq!(result, 42);
        mock.assert();
    }

    #[test]
    fn count_docs_errors_on_non_2xx() {
        let mut server = Server::new();
        let mock = server
            .mock("POST", "/taxon/_count")
            .with_status(500)
            .with_body("Internal Server Error")
            .create();

        let base = server.url();
        let q = serde_json::json!({"query": {"match_all": {}}});
        let err = count_docs(&base, "taxon", &q).unwrap_err();
        assert!(err.to_string().contains("non-2xx"));
        mock.assert();
    }

    #[test]
    fn count_docs_errors_on_missing_count_field() {
        let mut server = Server::new();
        let mock = server
            .mock("POST", "/taxon/_count")
            .with_status(200)
            .with_body(r#"{"hits": {}}"#)
            .create();

        let base = server.url();
        let q = serde_json::json!({"query": {"match_all": {}}});
        let err = count_docs(&base, "taxon", &q).unwrap_err();
        assert!(err.to_string().contains("missing numeric 'count'"));
        mock.assert();
    }

}
