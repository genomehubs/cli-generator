use axum::{extract::Query, Extension, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::{fetch_records, index_name, routes::ApiStatus, AppState};

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RecordQuery {
    #[serde(rename = "recordId")]
    pub record_id: String, // comma-separated or single
    pub result: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct RecordItem {
    pub record: Value,
    #[serde(rename = "recordId")]
    pub record_id: String,
    pub result: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct RecordResponse {
    pub status: ApiStatus,
    pub records: Vec<RecordItem>,
}

#[utoipa::path(
    get,
    path = "/api/v3/record",
    params(
        ("recordId" = String, Query, description = "Record ID (comma-separated for multiple)"),
        ("result" = Option<String>, Query, description = "Result type (taxon|assembly|sample)"),
    ),
    responses(
        (status = 200, description = "Record(s)", body = RecordResponse)
    )
)]
pub async fn get_record(
    Query(q): Query<RecordQuery>,
    Extension(state): Extension<Arc<AppState>>,
) -> Json<RecordResponse> {
    let result_type = q.result.as_deref().unwrap_or(&state.default_result);
    let idx = index_name::resolve_index_str(result_type, &state);

    // Parse record IDs and auto-prefix if needed (preserving v2 API behavior)
    let prefix = format!("{}-", result_type);
    let ids: Vec<String> = q
        .record_id
        .split(',')
        .map(str::trim)
        .map(|id| {
            if id.starts_with(&prefix) || id.contains('-') {
                id.to_string()
            } else {
                format!("{}{}", prefix, id)
            }
        })
        .collect();

    let id_refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();

    let docs =
        match fetch_records::fetch_records_by_id(&state.client, &state.es_base, &idx, &id_refs)
            .await
        {
            Ok(d) => d,
            Err(e) => {
                return Json(RecordResponse {
                    status: ApiStatus::error(e),
                    records: vec![],
                })
            }
        };

    let records: Vec<RecordItem> = id_refs
        .iter()
        .zip(docs.iter())
        .map(|(id, doc)| RecordItem {
            record: doc.clone(),
            record_id: id.to_string(),
            result: result_type.to_string(),
        })
        .collect();

    Json(RecordResponse {
        status: ApiStatus::query_ok(records.len() as u64, 0),
        records,
    })
}
