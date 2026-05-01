use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{index_name, routes::ApiStatus, AppState};

#[derive(Deserialize, utoipa::ToSchema)]
pub struct BatchSearchItem {
    pub query_yaml: String,
    pub params_yaml: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct BatchSearchRequest {
    pub searches: Vec<BatchSearchItem>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct BatchSearchResultItem {
    pub status: ApiStatus,
    pub count: usize,
    pub hits: Vec<serde_json::Value>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct BatchSearchResponse {
    pub status: ApiStatus,
    pub results: Vec<BatchSearchResultItem>,
}

/// Build ES _msearch body (NDJSON format — alternating header + body lines).
fn build_msearch_body(searches: &[(String, serde_json::Value)]) -> String {
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

/// Execute batch search on ES using _msearch API.
async fn execute_msearch(
    client: &reqwest::Client,
    es_base: &str,
    ndjson_body: &str,
) -> Result<serde_json::Value, String> {
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

#[utoipa::path(
    post,
    path = "/api/v3/batchSearch",
    request_body = BatchSearchRequest,
    responses(
        (status = 200, description = "Batch search results", body = BatchSearchResponse)
    )
)]
pub async fn post_batchSearch(
    Extension(state): Extension<Arc<AppState>>,
    Json(req): Json<BatchSearchRequest>,
) -> Json<BatchSearchResponse> {
    if req.searches.len() > 100 {
        return Json(BatchSearchResponse {
            status: ApiStatus::error("maximum 100 searches per request".to_string()),
            results: vec![],
        });
    }

    // Parse and build all searches
    let mut index_bodies: Vec<(String, serde_json::Value)> = vec![];
    for item in &req.searches {
        let query = match genomehubs_query::query::SearchQuery::from_yaml(&item.query_yaml) {
            Ok(q) => q,
            Err(e) => {
                return Json(BatchSearchResponse {
                    status: ApiStatus::error(format!("failed to parse query_yaml: {e}")),
                    results: vec![],
                })
            }
        };
        let params = match genomehubs_query::query::QueryParams::from_yaml(&item.params_yaml) {
            Ok(p) => p,
            Err(e) => {
                return Json(BatchSearchResponse {
                    status: ApiStatus::error(format!("failed to parse params_yaml: {e}")),
                    results: vec![],
                })
            }
        };
        let idx = index_name::resolve_index(&query.index, &state);

        // Extract fields from attributes
        let fields_vec: Vec<String> = query
            .attributes
            .fields
            .iter()
            .map(|f| f.name.clone())
            .collect();
        let names_slice: &[String] = &query.attributes.names;
        let ranks_slice: &[String] = &query.attributes.ranks;

        // Build the query body
        let group = match query.index {
            genomehubs_query::query::SearchIndex::Taxon => "taxon",
            genomehubs_query::query::SearchIndex::Assembly => "assembly",
            genomehubs_query::query::SearchIndex::Sample => "sample",
        };

        let sort_by = params.sort_by.as_deref();
        let sort_order = match params.sort_order {
            genomehubs_query::query::SortOrder::Asc => "asc",
            genomehubs_query::query::SortOrder::Desc => "desc",
        };
        let offset = (params.page - 1) * params.size;

        // Convert string slices for build_search_body
        let fields_refs: Vec<&str> = fields_vec.iter().map(|s| s.as_str()).collect();
        let names_refs: Vec<&str> = names_slice.iter().map(|s| s.as_str()).collect();
        let ranks_refs: Vec<&str> = ranks_slice.iter().map(|s| s.as_str()).collect();

        let body = match cli_generator::core::query_builder::build_search_body(
            None,
            if fields_refs.is_empty() {
                None
            } else {
                Some(fields_refs.as_slice())
            },
            None,
            Some(&query.attributes.attributes),
            query.identifiers.rank.as_deref(),
            if names_refs.is_empty() {
                None
            } else {
                Some(names_refs.as_slice())
            },
            if ranks_refs.is_empty() {
                None
            } else {
                Some(ranks_refs.as_slice())
            },
            sort_by,
            Some(sort_order),
            params.size,
            offset,
            None,
            Some(group),
        ) {
            Ok(b) => b,
            Err(e) => {
                return Json(BatchSearchResponse {
                    status: ApiStatus::error(format!("failed to build query: {e}")),
                    results: vec![],
                })
            }
        };

        index_bodies.push((idx, body));
    }

    // Execute batch search
    let ndjson = build_msearch_body(&index_bodies);
    let raw = match execute_msearch(&state.client, &state.es_base, &ndjson).await {
        Ok(v) => v,
        Err(e) => {
            return Json(BatchSearchResponse {
                status: ApiStatus::error(e),
                results: vec![],
            })
        }
    };

    // Parse responses
    let responses = raw
        .get("responses")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();

    let mut total_hits = 0u64;
    let results: Vec<BatchSearchResultItem> = responses
        .iter()
        .map(|resp| {
            let raw_str = resp.to_string();
            let rs = genomehubs_query::parse::parse_response_status(&raw_str).unwrap_or(
                genomehubs_query::parse::ResponseStatus {
                    hits: 0,
                    ok: false,
                    error: None,
                    took: 0,
                },
            );
            let hits_json =
                genomehubs_query::parse::parse_search_json(&raw_str).unwrap_or_default();
            let hits: Vec<serde_json::Value> = serde_json::from_str(&hits_json).unwrap_or_default();
            total_hits += rs.hits;
            BatchSearchResultItem {
                status: ApiStatus::query_ok(rs.hits, rs.took),
                count: hits.len(),
                hits,
            }
        })
        .collect();

    Json(BatchSearchResponse {
        status: ApiStatus::query_ok(total_hits, 0),
        results,
    })
}
