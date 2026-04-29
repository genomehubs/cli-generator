use axum::{Extension, Json};
use serde::Serialize;
use std::sync::Arc;

use crate::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct StatusResponse {
    pub ready: bool,
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
    Json(StatusResponse {
        ready,
        last_updated: last,
    })
}
