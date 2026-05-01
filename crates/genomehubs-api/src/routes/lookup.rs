use axum::{extract::Query, Extension, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::{es_client, index_name, routes::ApiStatus, AppState};

#[derive(Deserialize, utoipa::ToSchema)]
pub struct LookupQuery {
    #[serde(rename = "searchTerm")]
    pub search_term: String,
    pub result: Option<String>,
    pub size: Option<usize>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct LookupResult {
    pub id: String,
    pub name: String,
    pub rank: Option<String>,
    pub reason: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct LookupResponse {
    pub status: ApiStatus,
    pub results: Vec<LookupResult>,
}

/// Build ES query for SAYT prefix match (Stage 1).
/// Searches scientific_name (high boost) and nested taxon_names.name.live (lower boost).
fn build_sayt_query(term: &str, size: usize) -> serde_json::Value {
    json!({
        "size": size,
        "query": {
            "bool": {
                "should": [
                    // Direct scientific_name match — highest boost
                    {
                        "match": {
                            "scientific_name": {
                                "query": term,
                                "boost": 100
                            }
                        }
                    },
                    // Nested taxon_names search — lower boost (finds synonyms/common names)
                    {
                        "nested": {
                            "path": "taxon_names",
                            "query": {
                                "multi_match": {
                                    "query": term,
                                    "type": "phrase_prefix",
                                    "boost": 1.0,
                                    "fields": [
                                        "taxon_names.name.live",
                                        "taxon_names.name.live._2gram",
                                        "taxon_names.name.live._3gram"
                                    ]
                                }
                            }
                        }
                    }
                ]
            }
        },
        "_source": ["taxon_id", "scientific_name", "taxon_rank"]
    })
}

/// Build ES suggest query for phrase suggestion (Stage 3).
/// Requires taxon_names.name.trigram field for trigram-based matching.
fn build_suggest_query(term: &str) -> serde_json::Value {
    json!({
        "suggest": {
            "name_suggest": {
                "text": term,
                "simple_phrase": {
                    "phrase": {
                        "field": "taxon_names.name.trigram",
                        "size": 3,
                        "gram_size": 3,
                        "confidence": 1,
                        "direct_generator": [
                            {
                                "field": "taxon_names.name.trigram",
                                "suggest_mode": "always"
                            },
                            {
                                "field": "taxon_names.name.trigram",
                                "suggest_mode": "always",
                                "pre_filter": "reverse",
                                "post_filter": "reverse"
                            }
                        ],
                        "collate": {
                            "query": {
                                "source": {
                                    "match_phrase": {
                                        "taxon_names.name": "{{suggestion}}"
                                    }
                                }
                            },
                            "prune": true
                        }
                    }
                }
            }
        }
    })
}

/// Execute ES suggest request.
async fn execute_suggest(
    client: &reqwest::Client,
    es_base: &str,
    index: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let url = format!("{}/{}/_search", es_base.trim_end_matches('/'), index);

    let resp = client
        .post(&url)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("suggest request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp
            .text()
            .await
            .unwrap_or_else(|_| "unknown error".to_string());
        return Err(format!(
            "suggest error ({}): {}",
            status,
            text.chars().take(200).collect::<String>()
        ));
    }

    resp.json().await.map_err(|e| format!("parse error: {}", e))
}

/// Extract lookup results from ES suggest response.
fn extract_suggest_results(resp: &serde_json::Value) -> Vec<LookupResult> {
    let mut results = Vec::new();

    if let Some(suggestions) = resp
        .get("suggest")
        .and_then(|s| s.get("name_suggest"))
        .and_then(|ns| ns.as_array())
    {
        for suggestion in suggestions {
            if let Some(options) = suggestion.get("options").and_then(|o| o.as_array()) {
                for option in options {
                    if let Some(text) = option.get("text").and_then(|t| t.as_str()) {
                        if !text.is_empty() {
                            results.push(LookupResult {
                                id: String::new(), // Suggest doesn't provide taxon_id directly
                                name: text.to_string(),
                                rank: None,
                                reason: "suggest".to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    results
}

/// Build ES query for exact/wildcard match (Stage 2).
fn build_lookup_query(term: &str, size: usize) -> serde_json::Value {
    let wildcard_term = if term.contains('*') {
        term.to_string()
    } else {
        format!("{term}*")
    };
    json!({
        "size": size,
        "query": {
            "bool": {
                "should": [
                    { "match": { "scientific_name": { "query": term, "boost": 2 } } },
                    { "wildcard": { "scientific_name.keyword": { "value": wildcard_term } } },
                    {
                        "nested": {
                            "path": "taxon_names",
                            "query": {
                                "multi_match": {
                                    "query": term,
                                    "fields": ["taxon_names.name"]
                                }
                            }
                        }
                    }
                ]
            }
        },
        "_source": ["taxon_id", "scientific_name", "taxon_rank"]
    })
}

/// Extract lookup results from ES response.
fn extract_lookup_results(resp: &serde_json::Value, reason: &str) -> Vec<LookupResult> {
    let mut results = Vec::new();

    if let Some(hits) = resp
        .get("hits")
        .and_then(|h| h.get("hits"))
        .and_then(|h| h.as_array())
    {
        for hit in hits {
            if let Some(result) = hit.get("result") {
                let id = result
                    .get("taxon_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = result
                    .get("scientific_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let rank = result
                    .get("taxon_rank")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                if !id.is_empty() {
                    results.push(LookupResult {
                        id,
                        name,
                        rank,
                        reason: reason.to_string(),
                    });
                }
            }
        }
    }

    results
}

#[utoipa::path(
    get,
    path = "/api/v3/lookup",
    params(
        ("searchTerm" = String, Query, description = "Search term for lookup"),
        ("result" = Option<String>, Query, description = "Result type (taxon|assembly|sample)"),
        ("size" = Option<usize>, Query, description = "Maximum results to return (default 10)"),
    ),
    responses(
        (status = 200, description = "Lookup results", body = LookupResponse)
    )
)]
pub async fn get_lookup(
    Query(q): Query<LookupQuery>,
    Extension(state): Extension<Arc<AppState>>,
) -> Json<LookupResponse> {
    let result_type = q.result.as_deref().unwrap_or(&state.default_result);
    let idx = index_name::resolve_index_str(result_type, &state);
    let size = q.size.unwrap_or(10);

    // Get field availability flags from cache
    let (has_sayt, has_trigram) = if let Some(cache_lock) = &state.cache {
        let cache = cache_lock.read().await;
        (cache.has_sayt_field, cache.has_trigram_field)
    } else {
        (false, false)
    };

    // Stage 1: SAYT prefix match (if taxon_names.name.live exists)
    if has_sayt && result_type == "taxon" {
        let body = build_sayt_query(&q.search_term, size);
        match es_client::execute_search(&state.client, &state.es_base, &idx, &body).await {
            Ok(resp) => {
                let results = extract_lookup_results(&resp, "sayt");
                if !results.is_empty() {
                    return Json(LookupResponse {
                        status: ApiStatus::query_ok(results.len() as u64, 0),
                        results,
                    });
                }
            }
            Err(e) => {
                return Json(LookupResponse {
                    status: ApiStatus::error(e),
                    results: vec![],
                })
            }
        }
    }

    // Stage 2: Exact/wildcard match
    let body = build_lookup_query(&q.search_term, size);
    match es_client::execute_search(&state.client, &state.es_base, &idx, &body).await {
        Ok(resp) => {
            let results = extract_lookup_results(&resp, "wildcard");
            if !results.is_empty() {
                return Json(LookupResponse {
                    status: ApiStatus::query_ok(results.len() as u64, 0),
                    results,
                });
            }
        }
        Err(e) => {
            return Json(LookupResponse {
                status: ApiStatus::error(e),
                results: vec![],
            })
        }
    }

    // Stage 3: ES suggest (if taxon_names.name.trigram exists)
    if has_trigram && result_type == "taxon" {
        let body = build_suggest_query(&q.search_term);
        match execute_suggest(&state.client, &state.es_base, &idx, &body).await {
            Ok(resp) => {
                let results = extract_suggest_results(&resp);
                if !results.is_empty() {
                    return Json(LookupResponse {
                        status: ApiStatus::query_ok(results.len() as u64, 0),
                        results,
                    });
                }
            }
            Err(_e) => {
                // Suggest failed or returned no results, fall through to empty
            }
        }
    }

    // No results found
    Json(LookupResponse {
        status: ApiStatus::query_ok(0, 0),
        results: vec![],
    })
}
