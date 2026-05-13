//! POST /api/v3/lookup/batch — resolve up to 100 search terms via 3-round msearch.
//!
//! Instead of spawning one task per item (up to 3 ES requests each), this
//! implementation batches requests into at most 3 `_msearch` calls:
//!
//! 1. **SAYT round** — prefix queries for all `taxon` items when `has_sayt`.
//! 2. **Wildcard round** — exact/wildcard queries for all items with no SAYT hit.
//! 3. **Suggest round** — phrase-suggest for remaining `taxon` items when `has_trigram`.

use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::lookup::{
    build_lookup_query, build_sayt_query, build_suggest_query, extract_lookup_results,
    extract_suggest_results, LookupResult,
};
use crate::{es_client, index_name, routes::ApiStatus, AppState};

const MAX_BATCH_SIZE: usize = 100;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct LookupBatchItem {
    /// Search term (required).
    pub search_term: String,
    /// Result type: `"taxon"`, `"assembly"`, or `"sample"`. Defaults to `"taxon"`.
    pub result: Option<String>,
    /// Maximum results per item (default 10).
    pub size: Option<usize>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct LookupBatchRequest {
    /// Array of lookup items (max 100).
    pub lookups: Vec<LookupBatchItem>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct LookupBatchResultItem {
    pub status: ApiStatus,
    pub results: Vec<LookupResult>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct LookupBatchResponse {
    pub status: ApiStatus,
    /// Per-item lookup results, in input order.
    pub results: Vec<LookupBatchResultItem>,
}

struct PendingItem {
    /// Original position in the input array (preserves output order).
    pos: usize,
    search_term: String,
    result_type: String,
    index: String,
    size: usize,
}

#[utoipa::path(
    post,
    path = "/api/v3/lookup/batch",
    tag = "Data",
    summary = "Resolve up to 100 search terms to record IDs in one request",
    description = "Runs each item through the same three-stage waterfall (SAYT → wildcard → suggest) as `GET /lookup`, using at most 3 `_msearch` calls for the entire batch. Results are returned in input order.",
    request_body = LookupBatchRequest,
    responses(
        (status = 200, description = "Batch lookup results", body = LookupBatchResponse)
    )
)]
pub async fn post_lookup_batch(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<LookupBatchRequest>,
) -> Json<LookupBatchResponse> {
    if body.lookups.is_empty() {
        return Json(LookupBatchResponse {
            status: ApiStatus::error("lookups must not be empty"),
            results: vec![],
        });
    }
    if body.lookups.len() > MAX_BATCH_SIZE {
        return Json(LookupBatchResponse {
            status: ApiStatus::error(format!(
                "lookups must contain at most {MAX_BATCH_SIZE} items"
            )),
            results: vec![],
        });
    }

    let (has_sayt, has_trigram) = if let Some(cache_lock) = &state.cache {
        let cache = cache_lock.read().await;
        (cache.has_sayt_field, cache.has_trigram_field)
    } else {
        (false, false)
    };

    let default_result = state.default_result.clone();

    let mut pending: Vec<PendingItem> = body
        .lookups
        .into_iter()
        .enumerate()
        .map(|(pos, item)| {
            let result_type = item
                .result
                .as_deref()
                .unwrap_or(&default_result)
                .to_string();
            let index = index_name::resolve_index_str(&result_type, &state);
            PendingItem {
                pos,
                search_term: item.search_term,
                result_type,
                index,
                size: item.size.unwrap_or(10),
            }
        })
        .collect();

    let capacity = pending.len();
    let mut results: Vec<Option<LookupBatchResultItem>> = (0..capacity).map(|_| None).collect();

    // ── Round 1: SAYT ──────────────────────────────────────────────────────────
    if has_sayt {
        let sayt_items: Vec<&PendingItem> = pending
            .iter()
            .filter(|it| it.result_type == "taxon")
            .collect();

        if !sayt_items.is_empty() {
            let searches: Vec<(String, serde_json::Value)> = sayt_items
                .iter()
                .map(|it| (it.index.clone(), build_sayt_query(&it.search_term, it.size)))
                .collect();
            let ndjson = es_client::build_msearch_body(&searches);
            if let Ok(resp) =
                es_client::execute_msearch(&state.client, &state.es_base, &ndjson).await
            {
                let responses = resp
                    .get("responses")
                    .and_then(|r| r.as_array())
                    .cloned()
                    .unwrap_or_default();
                for (item, raw_resp) in sayt_items.iter().zip(responses.iter()) {
                    let hits = extract_lookup_results(raw_resp, "sayt");
                    if !hits.is_empty() {
                        results[item.pos] = Some(LookupBatchResultItem {
                            status: ApiStatus::query_ok(hits.len() as u64, 0),
                            results: hits,
                        });
                    }
                }
            }
        }
        pending.retain(|it| results[it.pos].is_none());
    }

    // ── Round 2: Wildcard ──────────────────────────────────────────────────────
    if !pending.is_empty() {
        let searches: Vec<(String, serde_json::Value)> = pending
            .iter()
            .map(|it| {
                (
                    it.index.clone(),
                    build_lookup_query(&it.search_term, it.size),
                )
            })
            .collect();
        let ndjson = es_client::build_msearch_body(&searches);
        if let Ok(resp) = es_client::execute_msearch(&state.client, &state.es_base, &ndjson).await {
            let responses = resp
                .get("responses")
                .and_then(|r| r.as_array())
                .cloned()
                .unwrap_or_default();
            for (item, raw_resp) in pending.iter().zip(responses.iter()) {
                let hits = extract_lookup_results(raw_resp, "wildcard");
                if !hits.is_empty() {
                    results[item.pos] = Some(LookupBatchResultItem {
                        status: ApiStatus::query_ok(hits.len() as u64, 0),
                        results: hits,
                    });
                }
            }
        }
        pending.retain(|it| results[it.pos].is_none());
    }

    // ── Round 3: Suggest ───────────────────────────────────────────────────────
    if has_trigram && !pending.is_empty() {
        let suggest_items: Vec<&PendingItem> = pending
            .iter()
            .filter(|it| it.result_type == "taxon")
            .collect();

        if !suggest_items.is_empty() {
            let searches: Vec<(String, serde_json::Value)> = suggest_items
                .iter()
                .map(|it| (it.index.clone(), build_suggest_query(&it.search_term)))
                .collect();
            let ndjson = es_client::build_msearch_body(&searches);
            if let Ok(resp) =
                es_client::execute_msearch(&state.client, &state.es_base, &ndjson).await
            {
                let responses = resp
                    .get("responses")
                    .and_then(|r| r.as_array())
                    .cloned()
                    .unwrap_or_default();
                for (item, raw_resp) in suggest_items.iter().zip(responses.iter()) {
                    let hits = extract_suggest_results(raw_resp);
                    if !hits.is_empty() {
                        results[item.pos] = Some(LookupBatchResultItem {
                            status: ApiStatus::query_ok(hits.len() as u64, 0),
                            results: hits,
                        });
                    }
                }
            }
        }
    }

    // Fill remaining positions with empty "no results" items
    let results: Vec<LookupBatchResultItem> = results
        .into_iter()
        .map(|r| {
            r.unwrap_or_else(|| LookupBatchResultItem {
                status: ApiStatus::query_ok(0, 0),
                results: vec![],
            })
        })
        .collect();

    let total_hits: u64 = results.iter().map(|r| r.results.len() as u64).sum();

    Json(LookupBatchResponse {
        status: ApiStatus::query_ok(total_hits, 0),
        results,
    })
}
