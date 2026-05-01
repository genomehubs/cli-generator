use axum::{Extension, Json};
use serde::Serialize;
use std::sync::Arc;

use crate::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct RanksResponse {
    pub status: super::ApiStatus,
    pub ranks: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}

pub async fn get_taxonomic_ranks(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<RanksResponse> {
    let mut ranks = Vec::new();
    let mut last = None;
    if let Some(lock) = &state.cache {
        let r = lock.read().await;
        ranks = r.taxonomic_ranks.clone();
        last = r.last_updated.clone();
    }
    Json(RanksResponse {
        status: super::ApiStatus::ok(),
        ranks,
        last_updated: last,
    })
}

#[utoipa::path(
    get,
    path = "/api/v3/taxonomicRanks",
    responses(
        (status = 200, description = "Cached taxonomic ranks", body = RanksResponse)
    )
)]
#[allow(dead_code)]
pub async fn get_taxonomic_ranks_openapi(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<RanksResponse> {
    /*
    This wrapper function exists solely to provide a concrete symbol annotated
    with `#[utoipa::path]` so the `utoipa` derive macro can generate OpenAPI
    metadata. It is not invoked by the Axum router at runtime (the real
    handler registered with the router is `get_taxonomic_ranks`).

    The wrapper therefore appears as "dead code" to the compiler and
    rust-analyzer. We keep it intentionally and silence the dead-code
    warning with `#[allow(dead_code)]` so the intent is explicit and
    OpenAPI generation continues to work.
    */
    get_taxonomic_ranks(Extension(state)).await
}
