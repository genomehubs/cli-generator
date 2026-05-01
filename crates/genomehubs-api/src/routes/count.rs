use axum::{extract::Json, Extension};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{es_client, index_name, AppState};

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

    // Resolve index name using shared helper
    let idx = index_name::resolve_index(&query.index, &state);

    // Build ES request body using the shared query_builder
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
                .map(|f| f.name.as_str())
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

    // Build a URL for the response (for debugging/reproduction)
    let built_url =
        genomehubs_query::query::build_query_url(&query, &params, &state.es_base, "v3", "count");

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
                url: built_url,
            })
        }
    };

    // Execute count query against ES using shared helper
    let raw = match es_client::execute_count(&state.client, &state.es_base, &idx, &body).await {
        Ok(v) => v,
        Err(e) => {
            return Json(CountResponse {
                status: super::ApiStatus::error(e),
                url: built_url,
            })
        }
    };

    // Extract hits and took from ES response
    if let Some(total) = raw.get("count").and_then(|c| c.as_u64()) {
        let took = raw.get("took").and_then(|t| t.as_u64()).unwrap_or(0);
        return Json(CountResponse {
            status: super::ApiStatus::query_ok(total, took),
            url: built_url,
        });
    }

    Json(CountResponse {
        status: super::ApiStatus::error(format!(
            "unexpected ES count response: {}",
            raw.to_string().chars().take(512).collect::<String>()
        )),
        url: built_url,
    })
}
