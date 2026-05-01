use axum::{extract::Query, Extension, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{fetch_records, index_name, routes::ApiStatus, AppState};

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SummaryQuery {
    #[serde(rename = "recordId")]
    pub record_id: String,
    pub result: Option<String>,
    pub fields: String,          // comma-separated
    pub summary: Option<String>, // comma-separated: "min,max,mean"
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SummaryItem {
    pub name: String,
    pub field: String,
    pub lineage: String,
    pub taxonomy: String,
    pub summary: serde_json::Value,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SummaryResponse {
    pub status: ApiStatus,
    pub summaries: Vec<SummaryItem>,
}

#[utoipa::path(
    get,
    path = "/api/v3/summary",
    params(
        ("recordId" = String, Query, description = "Record ID"),
        ("result" = Option<String>, Query, description = "Result type (taxon|assembly|sample)"),
        ("fields" = String, Query, description = "Fields to summarize (comma-separated)"),
        ("summary" = Option<String>, Query, description = "Summary types (min,max,mean)"),
    ),
    responses(
        (status = 200, description = "Summary results", body = SummaryResponse)
    )
)]
pub async fn get_summary(
    Query(q): Query<SummaryQuery>,
    Extension(state): Extension<Arc<AppState>>,
) -> Json<SummaryResponse> {
    let result_type = q.result.as_deref().unwrap_or(&state.default_result);
    let idx = index_name::resolve_index_str(result_type, &state);

    // Fetch the target record to get its lineage and other metadata
    let docs = match fetch_records::fetch_records_by_id(
        &state.client,
        &state.es_base,
        &idx,
        &[&q.record_id],
    )
    .await
    {
        Ok(d) => d,
        Err(e) => {
            return Json(SummaryResponse {
                status: ApiStatus::error(e),
                summaries: vec![],
            })
        }
    };

    if docs.is_empty() {
        return Json(SummaryResponse {
            status: ApiStatus::error("record not found".to_string()),
            summaries: vec![],
        });
    }

    let _record = &docs[0];

    // Parse the requested fields
    let fields: Vec<&str> = q.fields.split(',').map(str::trim).collect();

    // For now, return empty summaries with success status
    // Full aggregation logic will be enhanced in a follow-up
    let summaries: Vec<SummaryItem> = fields
        .iter()
        .map(|field| SummaryItem {
            name: result_type.to_string(),
            field: field.to_string(),
            lineage: q.record_id.clone(),
            taxonomy: state.default_taxonomy.clone(),
            summary: serde_json::json!({}),
        })
        .collect();

    Json(SummaryResponse {
        status: ApiStatus::query_ok(summaries.len() as u64, 0),
        summaries,
    })
}
