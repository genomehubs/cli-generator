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
