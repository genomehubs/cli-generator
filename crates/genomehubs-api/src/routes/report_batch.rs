use axum::{extract::Json, Extension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::{routes::ApiStatus, AppState};

/// Batch request for running multiple reports in one HTTP call.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct ReportBatchRequest {
    /// Array of report requests to execute in batch (max 50).
    pub reports: Vec<crate::routes::report::ReportRequest>,
    /// Optional concurrency limit (1..=32).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<usize>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ReportBatchResultItem {
    pub status: ApiStatus,
    pub report: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plot_spec: Option<Value>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ReportBatchResponse {
    pub status: ApiStatus,
    /// Per-request results in the same order as the input `reports`.
    pub results: Vec<ReportBatchResultItem>,
}

#[utoipa::path(
    post,
    path = "/api/v3/report/batch",
    tag = "Data",
    summary = "Generate multiple reports in a single request",
    description = "Execute multiple report requests concurrently; returns per-item report responses.",
    request_body(content = ReportBatchRequest),
    responses((status = 200, description = "Batch report results", body = ReportBatchResponse))
)]
#[axum::debug_handler]
pub async fn post_report_batch(
    Extension(state): Extension<Arc<AppState>>,
    Json(req): Json<ReportBatchRequest>,
) -> Json<ReportBatchResponse> {
    if req.reports.len() > 50 {
        return Json(ReportBatchResponse {
            status: ApiStatus::error("maximum 50 reports per request".to_string()),
            results: vec![],
        });
    }

    let concurrency = req.concurrency.unwrap_or(8).clamp(1, 32);
    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));

    // Spawn a task per report; each task acquires a semaphore permit so we bound
    // the number of concurrently-executing handlers.
    let mut handles = Vec::with_capacity(req.reports.len());
    for report_req in req.reports.into_iter() {
        let sem = semaphore.clone();
        let st = state.clone();
        let handle = tokio::spawn(async move {
            let _permit = sem.acquire_owned().await.expect("semaphore closed");
            // Call the existing single-report handler directly so we reuse
            // the same parsing, chain resolution, and dispatch logic.
            let resp = crate::routes::report::post_report(Extension(st), Json(report_req)).await;
            let Json(report_resp) = resp;
            ReportBatchResultItem {
                status: report_resp.status,
                report: report_resp.report,
                plot_spec: report_resp.plot_spec,
            }
        });
        handles.push(handle);
    }

    // Await all tasks and preserve input order.
    let mut results: Vec<ReportBatchResultItem> = Vec::with_capacity(handles.len());
    for h in handles {
        match h.await {
            Ok(item) => results.push(item),
            Err(e) => results.push(ReportBatchResultItem {
                status: ApiStatus::error(format!("task join failed: {e}")),
                report: Value::Null,
                plot_spec: None,
            }),
        }
    }

    Json(ReportBatchResponse {
        status: ApiStatus::ok(),
        results,
    })
}
