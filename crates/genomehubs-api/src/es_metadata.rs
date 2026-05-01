use reqwest::Client;
use serde_json::{json, Map, Value as JsonValue};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

use crate::AppState;

#[derive(Default, Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct MetadataCache {
    pub taxonomies: Vec<String>,
    pub indices: Vec<String>,
    pub attr_types: JsonValue,
    pub taxonomic_ranks: Vec<String>,
    pub last_updated: Option<String>,
    pub has_sayt_field: bool,
    pub has_trigram_field: bool,
}

fn processed_type(meta: &JsonValue) -> Option<String> {
    let t = meta.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if [
        "double",
        "float",
        "half_float",
        "scaled_float",
        "unsigned_long",
    ]
    .contains(&t)
        || t.ends_with("dp")
    {
        return Some("float".to_string());
    }
    if ["long", "integer", "short", "byte"].contains(&t) {
        return Some("integer".to_string());
    }
    if t == "keyword" {
        // examine summary
        if let Some(summary) = meta.get("summary") {
            if summary.is_array() {
                if let Some(first) = summary.get(0).and_then(|v| v.as_str()) {
                    if first == "enum" {
                        return Some("ordered_keyword".to_string());
                    }
                    if first == "primary" {
                        if let Some(second) = summary.get(1).and_then(|v| v.as_str()) {
                            if second == "enum" {
                                return Some("ordered_keyword".to_string());
                            }
                        }
                    }
                }
            } else if summary == "enum" {
                return Some("ordered_keyword".to_string());
            }
        }
        return Some("keyword".to_string());
    }
    Some(t.to_string())
}

fn processed_summary_and_simple(meta: &JsonValue) -> (String, String) {
    if let Some(default_summary) = meta.get("default_summary").and_then(|v| v.as_str()) {
        let simple = meta
            .get("return_type")
            .and_then(|v| v.as_str())
            .unwrap_or("value")
            .to_string();
        return (default_summary.to_string(), simple);
    }

    let simple = meta
        .get("return_type")
        .and_then(|v| v.as_str())
        .unwrap_or("value")
        .to_string();

    let summary = if meta.get("type").and_then(|v| v.as_str()) == Some("keyword") {
        "keyword_value.raw".to_string()
    } else {
        format!(
            "{}_value",
            meta.get("type").and_then(|v| v.as_str()).unwrap_or("")
        )
    };

    (summary, simple)
}

/// Fetch `_cat/indices?format=json` and derive taxonomies + indices similar to the JS logic.
pub async fn fetch_cat_indices_json(
    client: &Client,
    es_base: &str,
    taxonomy: &str,
    release: &str,
) -> Result<(Vec<String>, Vec<String>), String> {
    let url = format!(
        "{}/_cat/indices?format=json&expand_wildcards=all",
        es_base.trim_end_matches('/')
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("cat.indices request failed: {}", e))?
        .error_for_status()
        .map_err(|e| format!("cat.indices status error: {}", e))?;

    let arr: JsonValue = resp
        .json()
        .await
        .map_err(|e| format!("json parse error: {}", e))?;

    let mut taxonomies = Vec::new();
    let mut indices = Vec::new();

    if let Some(items) = arr.as_array() {
        for item in items.iter() {
            let index_name = item.get("index").and_then(|v| v.as_str()).unwrap_or("");
            // docs count can be string or number
            let docs_count = item
                .get("docs.count")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i64>().ok())
                .or_else(|| item.get("docs.count").and_then(|v| v.as_i64()));

            let docs_ok = docs_count.map(|c| c > 0).unwrap_or(true);

            if index_name.starts_with("taxon--") && index_name.contains(release) {
                // taxon--<taxonomy>--... -> extract between separators
                let parts: Vec<&str> = index_name.split("--").collect();
                if parts.len() > 1 {
                    if let Some(t) = parts.get(1) {
                        let t = t.to_string();
                        if !taxonomies.contains(&t) {
                            taxonomies.push(t);
                        }
                    }
                }
            }

            if index_name.contains(&format!("--{}--", taxonomy))
                && index_name.contains(&format!("--{}", release))
                && docs_ok
            {
                let parts: Vec<&str> = index_name.split("--").collect();
                if let Some(prefix) = parts.first() {
                    let p = prefix.to_string();
                    if !indices.contains(&p) {
                        indices.push(p);
                    }
                }
            }
        }
    }

    // ensure configured taxonomy is first
    if !taxonomies.contains(&taxonomy.to_string()) {
        taxonomies.insert(0, taxonomy.to_string());
    } else {
        // move to front
        taxonomies.retain(|t| t != taxonomy);
        taxonomies.insert(0, taxonomy.to_string());
    }

    Ok((taxonomies, indices))
}

/// Fetch attribute documents from the attributes index and build a grouped map like the JS code.
pub async fn fetch_attr_types(
    client: &Client,
    es_base: &str,
    attributes_index: &str,
    result: &str,
) -> Result<JsonValue, String> {
    let url = format!(
        "{}/{}/_search",
        es_base.trim_end_matches('/'),
        attributes_index
    );
    let body = if result == "multi" {
        json!({ "size": 10000, "_source": ["group","name","type","summary","default_summary","return_type","synonyms"] })
    } else {
        json!({ "size": 10000, "query": { "match": { "group": { "query": result } } }, "_source": ["group","name","type","summary","default_summary","return_type","synonyms"] })
    };

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("attr types request failed: {}", e))?
        .error_for_status()
        .map_err(|e| format!("attr types status error: {}", e))?;

    let body_json: JsonValue = resp
        .json()
        .await
        .map_err(|e| format!("json parse error: {}", e))?;

    let hits = body_json
        .get("hits")
        .and_then(|h| h.get("hits"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| "missing hits.hits array".to_string())?;

    let mut types_map: Map<String, JsonValue> = Map::new();

    for hit in hits.iter() {
        if let Some(source) = hit.get("_source") {
            if let (Some(group), Some(name)) = (
                source.get("group").and_then(|v| v.as_str()),
                source.get("name").and_then(|v| v.as_str()),
            ) {
                let entry = types_map
                    .entry(group.to_string())
                    .or_insert_with(|| JsonValue::Object(Map::new()));
                if let JsonValue::Object(map) = entry {
                    let mut doc = source.clone();
                    if let Some(pt) = processed_type(&doc) {
                        doc.as_object_mut()
                            .map(|m| m.insert("processed_type".to_string(), JsonValue::String(pt)));
                    }
                    let (ps, simple) = processed_summary_and_simple(&doc);
                    doc.as_object_mut()
                        .map(|m| m.insert("processed_summary".to_string(), JsonValue::String(ps)));
                    doc.as_object_mut().map(|m| {
                        m.insert("processed_simple".to_string(), JsonValue::String(simple))
                    });
                    map.insert(name.to_string(), doc);
                }
            }
        }
    }

    Ok(JsonValue::Object(types_map))
}

/// Check if taxon_names.name.live field exists in the index mapping.
/// Used to enable Stage 1 (SAYT) lookups across synonyms and common names.
async fn check_sayt_field(client: &Client, es_base: &str, index: &str) -> Result<bool, String> {
    let url = format!("{}/{}/_mapping", es_base.trim_end_matches('/'), index);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("mapping request failed: {}", e))?;

    if !resp.status().is_success() {
        return Ok(false);
    }

    let mapping_json: JsonValue = resp
        .json()
        .await
        .map_err(|e| format!("json parse error: {}", e))?;

    // Check if taxon_names.name.live exists
    if let Some(indices) = mapping_json.as_object() {
        for (_, index_mapping) in indices.iter() {
            if let Some(properties) = index_mapping
                .get("mappings")
                .and_then(|m| m.get("properties"))
            {
                if let Some(taxon_names) = properties.get("taxon_names") {
                    if let Some(nested_props) = taxon_names.get("properties") {
                        if let Some(name_field) = nested_props.get("name") {
                            if let Some(fields) = name_field.get("fields") {
                                if fields.get("live").is_some() {
                                    return Ok(true);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(false)
}

/// Check if taxon_names.name.trigram field exists in the index mapping.
async fn check_trigram_field(client: &Client, es_base: &str, index: &str) -> Result<bool, String> {
    let url = format!("{}/{}/_mapping", es_base.trim_end_matches('/'), index);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("mapping request failed: {}", e))?;

    if !resp.status().is_success() {
        return Ok(false);
    }

    let mapping_json: JsonValue = resp
        .json()
        .await
        .map_err(|e| format!("json parse error: {}", e))?;

    // Check if taxon_names.name.trigram exists
    if let Some(indices) = mapping_json.as_object() {
        for (_, index_mapping) in indices.iter() {
            if let Some(properties) = index_mapping
                .get("mappings")
                .and_then(|m| m.get("properties"))
            {
                if let Some(taxon_names) = properties.get("taxon_names") {
                    if let Some(properties) = taxon_names.get("properties") {
                        if let Some(name_field) = properties.get("name") {
                            if let Some(fields) = name_field.get("fields") {
                                if fields.get("trigram").is_some() {
                                    return Ok(true);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(false)
}

pub async fn fetch_taxonomic_ranks(
    client: &Client,
    es_base: &str,
    index: &str,
) -> Result<Vec<String>, String> {
    let url = format!("{}/{}/_search", es_base.trim_end_matches('/'), index);
    let body = json!({
        "size": 0,
        "query": { "bool": { "must_not": { "term": { "taxon_rank": "no rank" } } } },
        "aggs": { "unique_ranks": { "terms": { "field": "taxon_rank", "size": 100 } } }
    });

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("ranks request failed: {}", e))?
        .error_for_status()
        .map_err(|e| format!("ranks status error: {}", e))?;

    let body_json: JsonValue = resp
        .json()
        .await
        .map_err(|e| format!("json parse error: {}", e))?;

    if let Some(buckets) = body_json
        .get("aggregations")
        .and_then(|a| a.get("unique_ranks"))
        .and_then(|u| u.get("buckets"))
        .and_then(|b| b.as_array())
    {
        let mut ranks = Vec::new();
        for bucket in buckets.iter() {
            if let Some(k) = bucket.get("key").and_then(|v| v.as_str()) {
                ranks.push(k.to_string());
            }
        }
        return Ok(ranks);
    }

    Ok(Vec::new())
}

/// Populate the shared cache by calling the above helpers. Returns Ok(()) on success.
pub async fn populate_cache(state: Arc<AppState>, client: &Client) -> Result<(), String> {
    let es_base = &state.es_base;
    let taxonomy = &state.default_taxonomy;
    let release = &state.default_version;

    // normalise suffix
    let suffix_norm = state.index_suffix.as_ref().map(|s| {
        if s.starts_with("--") {
            s.clone()
        } else {
            format!("--{}", s)
        }
    });

    let attributes_index = suffix_norm
        .as_ref()
        .map(|s| format!("attributes{}", s))
        .unwrap_or_else(|| "attributes".to_string());

    // fetch cat indices
    let (taxonomies, indices) = fetch_cat_indices_json(client, es_base, taxonomy, release).await?;

    // fetch attribute types
    let attr_types = fetch_attr_types(client, es_base, &attributes_index, "multi").await?;

    // fetch ranks from taxon index
    let taxon_index = suffix_norm
        .as_ref()
        .map(|s| format!("taxon{}", s))
        .unwrap_or_else(|| "taxon".to_string());
    let ranks = fetch_taxonomic_ranks(client, es_base, &taxon_index)
        .await
        .unwrap_or_default();

    let has_sayt_field = check_sayt_field(client, es_base, &taxon_index)
        .await
        .unwrap_or(false);

    let has_trigram_field = check_trigram_field(client, es_base, &taxon_index)
        .await
        .unwrap_or(false);

    let now = chrono::Utc::now().to_rfc3339();

    let cache = MetadataCache {
        taxonomies,
        indices,
        attr_types,
        taxonomic_ranks: ranks,
        last_updated: Some(now),
        has_sayt_field,
        has_trigram_field,
    };

    // write into AppState cache (tokio RwLock inside AppState)
    if let Some(lock) = &state.cache {
        let mut w = lock.write().await;
        *w = cache;
    }

    Ok(())
}

/// Retry-populate with exponential backoff. If max_attempts is None, retry indefinitely.
pub async fn populate_with_retry(
    state: Arc<AppState>,
    client: &Client,
    max_attempts: Option<usize>,
) -> Result<(), String> {
    let mut attempt: usize = 0;
    let mut wait = Duration::from_secs(1);
    loop {
        attempt += 1;
        match populate_cache(state.clone(), client).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                tracing::error!(attempt, error = %e, "populate_cache failed");
                if let Some(max) = max_attempts {
                    if attempt >= max {
                        return Err(format!("failed after {} attempts: {}", attempt, e));
                    }
                }
                tracing::info!(wait = ?wait, "retrying populate_cache after backoff");
                sleep(wait).await;
                wait = std::cmp::min(wait * 2, Duration::from_secs(30));
            }
        }
    }
}
