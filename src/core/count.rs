//! Minimal Elasticsearch `count` helper for the Rust core.
//!
//! Provides a small, well-tested wrapper that posts an ES query to
//! `/{index}/_count` and returns the numeric hit count. This is intentionally
//! minimal so it can be used as a building block for higher-level API functions
//! (reports, aggregations) implemented later.

use crate::core::query::SearchQuery;
use crate::core::query_builder::build_count_body;
use crate::core::runtime_query::{search_query_from_request, QueryRequest};
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde_json::json;
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

/// Count documents matching a v2 request, converted by the canonical URL converter.
///
/// Taxa and fields become `match_phrase` filters on `_all`, so the count is best-effort.
pub fn count_docs_from_request(base_url: &str, index: &str, request: &QueryRequest) -> Result<u64> {
    let query = search_query_from_request(request).context("converting request to SearchQuery")?;
    count_docs(base_url, index, &match_body(&query))
}

/// Build an ES body from a `SearchQuery`'s taxa and fields.
///
/// Returns `match_all` when neither taxa nor fields are present.
fn match_body(query: &SearchQuery) -> Value {
    let mut filters: Vec<Value> = Vec::new();

    if let Some(taxa_ident) = &query.identifiers.taxa {
        let joined = taxa_ident.names.join(" ");
        if !joined.is_empty() {
            filters.push(json!({ "match_phrase": { "_all": joined } }));
        }
    }

    for field in &query.attributes.fields {
        if !field.name.is_empty() {
            filters.push(json!({ "match_phrase": { "_all": field.name } }));
        }
    }

    if filters.is_empty() {
        json!({ "query": { "match_all": {} } })
    } else {
        json!({ "query": { "bool": { "filter": filters } } })
    }
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

    #[test]
    fn count_docs_from_request_posts_match_filters() {
        let mut server = Server::new();
        let mock = server
            .mock("POST", "/taxon/_count")
            .with_status(200)
            .with_body(r#"{"count": 7}"#)
            .create();

        let base = server.url();
        let request = QueryRequest {
            result: "taxon".to_string(),
            query: Some("tax_name(Mammalia)".to_string()),
            fields: vec!["genome_size".to_string()],
            size: None,
        };

        let result = count_docs_from_request(&base, "taxon", &request).unwrap();
        assert_eq!(result, 7);
        mock.assert();
    }

    #[test]
    fn match_body_is_match_all_without_taxa_or_fields() {
        let body = match_body(&SearchQuery::default());
        assert_eq!(body, json!({ "query": { "match_all": {} } }));
    }
}
