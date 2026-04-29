use axum::{Extension, Json};
use serde::Serialize;
use std::sync::Arc;

use crate::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct IndicesResponse {
    pub indices: Vec<String>,
    pub last_updated: Option<String>,
}

pub async fn get_indices(Extension(state): Extension<Arc<AppState>>) -> Json<IndicesResponse> {
    let mut indices = Vec::new();
    let mut last = None;
    if let Some(lock) = &state.cache {
        let r = lock.read().await;
        indices = r.indices.clone();
        last = r.last_updated.clone();
    }
    Json(IndicesResponse {
        indices,
        last_updated: last,
    })
}

#[utoipa::path(
    get,
    path = "/api/v3/indices",
    responses(
        (status = 200, description = "Cached indices", body = IndicesResponse)
    )
)]
#[allow(dead_code)]
pub async fn get_indices_openapi(Extension(state): Extension<Arc<AppState>>) -> Json<IndicesResponse> {
    // Wrapper for OpenAPI generation; actual handler is `get_indices`.
    get_indices(Extension(state)).await
}
