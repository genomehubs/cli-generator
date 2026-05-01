use axum::{Extension, Json};
use serde::Serialize;
use std::sync::Arc;

use crate::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct TaxonomiesResponse {
    pub status: super::ApiStatus,
    pub taxonomies: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}

pub async fn get_taxonomies(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<TaxonomiesResponse> {
    let mut taxonomies = Vec::new();
    let mut last = None;
    if let Some(lock) = &state.cache {
        let r = lock.read().await;
        taxonomies = r.taxonomies.clone();
        last = r.last_updated.clone();
    }
    Json(TaxonomiesResponse {
        status: super::ApiStatus::ok(),
        taxonomies,
        last_updated: last,
    })
}

#[utoipa::path(
    get,
    path = "/api/v3/taxonomies",
    responses(
        (status = 200, description = "Cached taxonomies", body = TaxonomiesResponse)
    )
)]
#[allow(dead_code)]
pub async fn get_taxonomies_openapi(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<TaxonomiesResponse> {
    // This function exists to provide an explicit symbol for utoipa OpenAPI generation.
    // The real request handler is `get_taxonomies`.
    get_taxonomies(Extension(state)).await
}
