use axum::{Extension, Json};
use serde::Serialize;
use std::sync::Arc;

use crate::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct StatusResponse {
    pub status: super::ApiStatus,
    pub ready: bool,
    pub supported: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v3/status",
    responses(
        (status = 200, description = "Health status", body = StatusResponse)
    )
)]
pub async fn get_status(Extension(state): Extension<Arc<AppState>>) -> Json<StatusResponse> {
    let mut ready = false;
    let mut last = None;
    if let Some(lock) = &state.cache {
        let r = lock.read().await;
        if r.last_updated.is_some() {
            ready = true;
            last = r.last_updated.clone();
        }
    }
    let supported = vec![
        "/status".to_string(),
        "/resultFields".to_string(),
        "/taxonomies".to_string(),
        "/taxonomicRanks".to_string(),
        "/indices".to_string(),
        "/count".to_string(),
    ];
    Json(StatusResponse {
        status: super::ApiStatus::ok(),
        ready,
        supported,
        last_updated: last,
    })
}
