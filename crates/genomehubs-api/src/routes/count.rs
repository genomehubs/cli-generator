use axum::{extract::Json, Extension};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::AppState;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CountRequest {
    /// YAML string describing the SearchQuery
    pub query_yaml: String,
    /// YAML string describing the QueryParams
    pub params_yaml: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CountResponse {
    pub status: super::ApiStatus,
    /// The executed ES URL
    pub url: String,
}

#[utoipa::path(
    post,
    path = "/api/v3/count",
    request_body = CountRequest,
    responses(
        (status = 200, description = "Count result", body = CountResponse)
    )
)]
#[axum::debug_handler]
pub async fn post_count(
    Extension(state): Extension<Arc<AppState>>,
    Json(req): Json<CountRequest>,
) -> Json<CountResponse> {
    // Parse YAML inputs
    let query = match genomehubs_query::query::SearchQuery::from_yaml(&req.query_yaml) {
        Ok(q) => q,
        Err(e) => {
            return Json(CountResponse {
                status: super::ApiStatus::error(format!("failed to parse query_yaml: {}", e)),
                url: "".to_string(),
            })
        }
    };

    let params = match genomehubs_query::query::QueryParams::from_yaml(&req.params_yaml) {
        Ok(p) => p,
        Err(e) => {
            return Json(CountResponse {
                status: super::ApiStatus::error(format!("failed to parse params_yaml: {}", e)),
                url: "".to_string(),
            })
        }
    };

    // Build index name
    fn index_name_for(
        query_index: &genomehubs_query::query::SearchIndex,
        state: &AppState,
    ) -> String {
        let base = match query_index {
            genomehubs_query::query::SearchIndex::Taxon => "taxon",
            genomehubs_query::query::SearchIndex::Assembly => "assembly",
            genomehubs_query::query::SearchIndex::Sample => "sample",
        };
        let mut idx = base.to_string();
        if let Some(suf) = &state.index_suffix {
            idx.push_str(suf);
        }
        idx
    }

    let idx = index_name_for(&query.index, &state);

    // Use the project's canonical builder to create a proper ES search body
    let group = match query.index {
        genomehubs_query::query::SearchIndex::Taxon => "taxon",
        genomehubs_query::query::SearchIndex::Assembly => "assembly",
        genomehubs_query::query::SearchIndex::Sample => "sample",
    };

    let fields_slice: Option<Vec<&str>> = if query.attributes.fields.is_empty() {
        None
    } else {
        Some(
            query
                .attributes
                .fields
                .iter()
                .map(|s| s.name.as_str())
                .collect(),
        )
    };
    let names_slice: Option<Vec<&str>> = if query.attributes.names.is_empty() {
        None
    } else {
        Some(query.attributes.names.iter().map(|s| s.as_str()).collect())
    };
    let ranks_slice: Option<Vec<&str>> = if query.attributes.ranks.is_empty() {
        None
    } else {
        Some(query.attributes.ranks.iter().map(|s| s.as_str()).collect())
    };

    let sort_by = params.sort_by.as_deref();
    let sort_order = Some(match params.sort_order {
        genomehubs_query::query::SortOrder::Asc => "asc",
        genomehubs_query::query::SortOrder::Desc => "desc",
    });

    let size = 0usize;
    let offset = (params.page.saturating_sub(1)) * params.size;

    let body = match cli_generator::core::query_builder::build_search_body(
        None,
        fields_slice.as_deref(),
        None,
        Some(&query.attributes.attributes),
        query.identifiers.rank.as_deref(),
        names_slice.as_deref(),
        ranks_slice.as_deref(),
        sort_by,
        sort_order,
        size,
        offset,
        None,
        Some(group),
    ) {
        Ok(b) => b,
        Err(e) => {
            return Json(CountResponse {
                status: super::ApiStatus::error(format!("failed to build ES body: {}", e)),
                url: "".to_string(),
            })
        }
    };

    // No-op: `build_search_body` now omits empty `aggs` itself.

    // POST to _search (size=0) and read hits.total.value
    let es_base = state.es_base.trim_end_matches('/').to_string();
    let url = format!("{}/{}/_search", es_base, idx);
    let client = reqwest::Client::new();

    match client.post(&url).json(&body).send().await {
        Ok(resp) => match resp.text().await {
            Ok(raw) => {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if let Some(total) = v
                        .get("hits")
                        .and_then(|h| h.get("total"))
                        .and_then(|t| t.get("value"))
                        .and_then(|n| n.as_u64())
                    {
                        let took = v.get("took").and_then(|t| t.as_u64()).unwrap_or(0);
                        return Json(CountResponse {
                            status: super::ApiStatus::query_ok(total, took),
                            url,
                        });
                    }
                    return Json(CountResponse {
                        status: super::ApiStatus::error(format!(
                            "unexpected ES response: {}",
                            &raw.chars().take(512).collect::<String>()
                        )),
                        url,
                    });
                }
                Json(CountResponse {
                    status: super::ApiStatus::error(format!(
                        "non-JSON ES response: {}",
                        &raw.chars().take(512).collect::<String>()
                    )),
                    url,
                })
            }
            Err(e) => Json(CountResponse {
                status: super::ApiStatus::error(format!("failed to read ES response: {}", e)),
                url,
            }),
        },
        Err(e) => Json(CountResponse {
            status: super::ApiStatus::error(format!("request error: {}", e)),
            url,
        }),
    }
}
