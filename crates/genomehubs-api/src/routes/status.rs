use axum::{Extension, Json};
use serde::Serialize;
use std::sync::Arc;

use crate::{es_metadata::FeatureIndexVersion, AppState};

const SUPPORTED_ENDPOINTS: &[&str] = &[
    "/status",
    "/metadata",
    "/metadata/indices",
    "/metadata/fields",
    "/metadata/ranks",
    "/metadata/taxonomies",
    "/count",
    "/count/batch",
    "/search",
    "/search/batch",
    "/record",
    "/lookup",
    "/summary",
    "/report",
    "/phylopic",
    "/phylopic/batch",
];

#[derive(Serialize, utoipa::ToSchema)]
pub struct StatusResponse {
    pub status: super::ApiStatus,
    pub ready: bool,
    pub supported: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
    /// Feature index version detected at startup.
    /// `"v2"` means the `/positional` endpoint is available;
    /// `"v1"` means positional queries will return an error.
    pub feature_index_version: FeatureIndexVersion,
}

#[utoipa::path(
    get,
    path = "/api/v3/status",
    tag = "Status",
    summary = "API health and supported endpoints",
    description = "Returns health status of the API and lists all supported endpoint paths.",
    responses(
        (status = 200, description = "Health status", body = StatusResponse)
    )
)]
pub async fn get_status(Extension(state): Extension<Arc<AppState>>) -> Json<StatusResponse> {
    let mut ready = false;
    let mut last = None;
    let mut feature_index_version = FeatureIndexVersion::V1;
    if let Some(lock) = &state.cache {
        let r = lock.read().await;
        if r.last_updated.is_some() {
            ready = true;
            last = r.last_updated.clone();
        }
        feature_index_version = r.feature_index_version.clone();
    }
    Json(StatusResponse {
        status: super::ApiStatus::ok(),
        ready,
        supported: SUPPORTED_ENDPOINTS.iter().map(|s| s.to_string()).collect(),
        last_updated: last,
        feature_index_version,
    })
}
