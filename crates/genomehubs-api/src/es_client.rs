use reqwest::Client;
use serde_json::Value;

/// Send a JSON body to `{es_base}/{index}/_search` and return the parsed response.
///
/// Returns `Err(String)` for HTTP errors or unparseable JSON.
pub async fn execute_search(
    client: &Client,
    es_base: &str,
    index: &str,
    body: &Value,
) -> Result<Value, String> {
    let url = format!("{es_base}/{index}/_search");
    let resp = client
        .post(&url)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("ES request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("ES returned {status}: {text}"));
    }

    resp.json::<Value>()
        .await
        .map_err(|e| format!("failed to parse ES response: {e}"))
}

/// Send a JSON body to `{es_base}/{index}/_count`.
pub async fn execute_count(
    client: &Client,
    es_base: &str,
    index: &str,
    body: &Value,
) -> Result<Value, String> {
    let url = format!("{es_base}/{index}/_count");
    let resp = client
        .post(&url)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("ES count request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("ES count returned {status}: {text}"));
    }

    resp.json::<Value>()
        .await
        .map_err(|e| format!("failed to parse ES count response: {e}"))
}

/// Build ES `_msearch` body in NDJSON format (alternating header + query lines).
///
/// Each element of `searches` is `(index_name, query_body)`.
pub fn build_msearch_body(searches: &[(String, Value)]) -> String {
    searches
        .iter()
        .flat_map(|(index, body)| {
            let header = serde_json::json!({ "index": index });
            vec![
                serde_json::to_string(&header).unwrap(),
                serde_json::to_string(body).unwrap(),
            ]
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Execute a batch search against `{es_base}/_msearch`.
///
/// `ndjson_body` must already be formatted as alternating header+query lines
/// as produced by [`build_msearch_body`].
pub async fn execute_msearch(
    client: &Client,
    es_base: &str,
    ndjson_body: &str,
) -> Result<Value, String> {
    let url = format!("{es_base}/_msearch");
    let resp = client
        .post(&url)
        .header("Content-Type", "application/x-ndjson")
        .body(ndjson_body.to_string())
        .send()
        .await
        .map_err(|e| format!("msearch request failed: {e}"))?;

    resp.json().await.map_err(|e| format!("parse error: {e}"))
}
