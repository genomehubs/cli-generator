//! POST /api/v3/record/batch — fetch up to 1,000 records by ID in one request.
//!
//! The existing `GET /record` already batches via ES `_mget` when given a
//! comma-separated `recordId`, but has a URL-length limit and poor discoverability.
//! This endpoint provides an explicit JSON body interface for the same operation.

use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::record::RecordItem;
use crate::{fetch_records, index_name, routes::ApiStatus, AppState};

const MAX_BATCH_SIZE: usize = 1_000;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RecordBatchRequest {
    /// Record IDs to fetch (max 1,000).
    ///
    /// IDs may be fully-qualified (e.g. `"taxon-9606"`) or bare numeric
    /// strings; bare IDs are auto-prefixed with `"<result>-"`.
    pub record_ids: Vec<String>,
    /// Result type: `"taxon"`, `"assembly"`, or `"sample"`. Defaults to `"taxon"`.
    pub result: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct RecordBatchResponse {
    pub status: ApiStatus,
    pub records: Vec<RecordItem>,
}

#[utoipa::path(
    post,
    path = "/api/v3/record/batch",
    tag = "Data",
    summary = "Fetch up to 1,000 records by ID in one request",
    description = "Accepts an explicit array of record IDs and returns all found records in a single `_mget` call. Prefer this over repeated `GET /record` calls.",
    request_body = RecordBatchRequest,
    responses(
        (status = 200, description = "Batch record results", body = RecordBatchResponse)
    )
)]
pub async fn post_record_batch(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<RecordBatchRequest>,
) -> Json<RecordBatchResponse> {
    if body.record_ids.is_empty() {
        return Json(RecordBatchResponse {
            status: ApiStatus::error("record_ids must not be empty"),
            records: vec![],
        });
    }
    if body.record_ids.len() > MAX_BATCH_SIZE {
        return Json(RecordBatchResponse {
            status: ApiStatus::error(format!(
                "record_ids must contain at most {MAX_BATCH_SIZE} entries"
            )),
            records: vec![],
        });
    }

    let result_type = body
        .result
        .as_deref()
        .unwrap_or(&state.default_result)
        .to_string();
    let idx = index_name::resolve_index_str(&result_type, &state);

    // Auto-prefix bare IDs (matches GET /record behaviour)
    let prefix = format!("{}-", result_type);
    let ids: Vec<String> = body
        .record_ids
        .iter()
        .map(|id| {
            if id.starts_with(&prefix) || id.contains('-') {
                id.clone()
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
                return Json(RecordBatchResponse {
                    status: ApiStatus::error(e),
                    records: vec![],
                })
            }
        };

    // Pair found docs with their IDs in input order.
    // fetch_records_by_id preserves input order for _mget hits.
    let records: Vec<RecordItem> = id_refs
        .iter()
        .zip(docs.iter())
        .map(|(id, doc)| RecordItem {
            record: doc.clone(),
            record_id: id.to_string(),
            result: result_type.clone(),
        })
        .collect();

    Json(RecordBatchResponse {
        status: ApiStatus::query_ok(records.len() as u64, 0),
        records,
    })
}
