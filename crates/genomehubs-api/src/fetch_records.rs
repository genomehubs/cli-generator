use reqwest::Client;
use serde_json::{json, Value};

/// Fetch one or more ES documents by ID.
///
/// Uses `_mget` for multiple IDs; falls back to `_doc/{id}` for single IDs.
/// Returns the `_source` of each found document; missing IDs are silently dropped.
pub async fn fetch_records_by_id(
    client: &Client,
    es_base: &str,
    index: &str,
    ids: &[&str],
) -> Result<Vec<Value>, String> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    if ids.len() == 1 {
        let url = format!("{es_base}/{index}/_doc/{}", ids[0]);
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("ES _doc request failed: {e}"))?;
        if resp.status().as_u16() == 404 {
            return Ok(vec![]);
        }
        let doc: Value = resp.json().await.map_err(|e| format!("parse error: {e}"))?;
        return Ok(doc.get("_source").cloned().into_iter().collect());
    }

    // Multiple IDs: use _mget
    let url = format!("{es_base}/{index}/_mget");
    let body = json!({ "ids": ids });
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("ES _mget request failed: {e}"))?;
    let result: Value = resp.json().await.map_err(|e| format!("parse error: {e}"))?;

    let docs = result
        .get("docs")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|doc| doc.get("found").and_then(|f| f.as_bool()).unwrap_or(false))
                .filter_map(|doc| doc.get("_source").cloned())
                .collect()
        })
        .unwrap_or_default();

    Ok(docs)
}
