use axum::{extract::Json, Extension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::{es_client, index_name, routes::ApiStatus, AppState};

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SearchRequest {
    /// YAML string describing the SearchQuery.
    pub query_yaml: String,
    /// YAML string describing the QueryParams.
    pub params_yaml: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SearchResponse {
    pub status: ApiStatus,
    /// URL that was built for this query (for debugging/reproduction).
    pub url: String,
    /// Flat result records.
    pub results: Vec<Value>,
    /// Cursor for the next page, if more results exist.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_after: Option<Value>,
}

#[utoipa::path(
    post,
    path = "/api/v3/search",
    request_body = SearchRequest,
    responses(
        (status = 200, description = "Search results", body = SearchResponse)
    )
)]
#[axum::debug_handler]
pub async fn post_search(
    Extension(state): Extension<Arc<AppState>>,
    Json(req): Json<SearchRequest>,
) -> Json<SearchResponse> {
    macro_rules! bail {
        ($msg:expr) => {
            return Json(SearchResponse {
                status: ApiStatus::error($msg),
                url: String::new(),
                results: vec![],
                search_after: None,
            })
        };
    }

    let query = match genomehubs_query::query::SearchQuery::from_yaml(&req.query_yaml) {
        Ok(q) => q,
        Err(e) => bail!(format!("failed to parse query_yaml: {e}")),
    };

    let params = match genomehubs_query::query::QueryParams::from_yaml(&req.params_yaml) {
        Ok(p) => p,
        Err(e) => bail!(format!("failed to parse params_yaml: {e}")),
    };

    let idx = index_name::resolve_index(&query.index, &state);

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

    let offset = (params.page.saturating_sub(1)) * params.size;

    // `build_search_body` is in cli_generator::core::query_builder
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
        params.size,
        offset,
        None,
        Some(group),
    ) {
        Ok(b) => b,
        Err(e) => bail!(format!("failed to build ES body: {}", e)),
    };

    // Build a URL for the response (no network call — for debugging)
    let built_url =
        genomehubs_query::query::build_query_url(&query, &params, &state.es_base, "v3", "search");

    let raw = match es_client::execute_search(&state.client, &state.es_base, &idx, &body).await {
        Ok(v) => v,
        Err(e) => bail!(e),
    };

    // Extract status block from ES response
    let status_block = genomehubs_query::parse::parse_response_status(&raw.to_string())
        .unwrap_or_else(|_| genomehubs_query::parse::ResponseStatus {
            hits: 0,
            ok: false,
            error: Some("failed to parse response".to_string()),
            took: 0,
        });

    // Flatten records via the existing parse pipeline
    let results_json = match genomehubs_query::parse::parse_search_json(&raw.to_string()) {
        Ok(s) => s,
        Err(e) => bail!(format!("failed to parse search results: {e}")),
    };
    let results: Vec<Value> = serde_json::from_str(&results_json).unwrap_or_default();

    // Extract search_after cursor for pagination
    let search_after = raw
        .get("hits")
        .and_then(|h| h.get("hits"))
        .and_then(|hits| hits.as_array())
        .and_then(|arr| arr.last())
        .and_then(|last| last.get("sort"))
        .cloned();

    Json(SearchResponse {
        status: ApiStatus::query_ok(status_block.hits, status_block.took),
        url: built_url,
        results,
        search_after,
    })
}
