use axum::{Extension, Json};
use serde::Serialize;
use std::sync::Arc;

use crate::AppState;

/// Aggregated metadata response combining indices, taxonomies, ranks, and versions in a
/// single round-trip. Fields are excluded because they require a `?result=`
/// qualifier and cannot be returned without a parameter.
#[derive(Serialize, utoipa::ToSchema)]
pub struct MetadataResponse {
    pub status: super::ApiStatus,
    pub indices: Vec<String>,
    pub taxonomies: Vec<String>,
    pub ranks: Vec<String>,
    /// Known data release versions. Currently a single-element list; multi-version
    /// support will extend this when the API serves more than one release.
    pub versions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v3/metadata",
    tag = "Metadata",
    summary = "Aggregated hub metadata",
    description = "Returns available indices, taxonomies, taxonomic ranks, and API versions in a single response.",
    responses(
        (status = 200, description = "Aggregated metadata: indices, taxonomies, ranks, and versions", body = MetadataResponse)
    )
)]
pub async fn get_metadata(Extension(state): Extension<Arc<AppState>>) -> Json<MetadataResponse> {
    let mut indices = Vec::new();
    let mut taxonomies = Vec::new();
    let mut ranks = Vec::new();
    let mut last = None;
    if let Some(lock) = &state.cache {
        let r = lock.read().await;
        indices = r.indices.clone();
        taxonomies = r.taxonomies.clone();
        ranks = r.taxonomic_ranks.clone();
        last = r.last_updated.clone();
    }
    Json(MetadataResponse {
        status: super::ApiStatus::ok(),
        indices,
        taxonomies,
        ranks,
        versions: vec![state.default_version.clone()],
        last_updated: last,
    })
}
