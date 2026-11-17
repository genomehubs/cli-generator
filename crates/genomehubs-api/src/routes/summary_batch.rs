use axum::{extract::Json, Extension};
use serde::{Deserialize, Deserializer, Serialize};
use std::sync::Arc;

use crate::{fetch_records, index_name, routes::ApiStatus, AppState};

fn deserialize_summary_fields<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;

    match value {
        serde_json::Value::String(s) => Ok(s
            .split(',')
            .map(str::trim)
            .filter(|field| !field.is_empty())
            .map(str::to_string)
            .collect()),
        serde_json::Value::Array(items) => {
            let mut fields = Vec::new();
            for item in items {
                let raw = item.as_str().ok_or_else(|| {
                    serde::de::Error::custom("expected a field name or a list of field names")
                })?;
                fields.extend(
                    raw.split(',')
                        .map(str::trim)
                        .filter(|field| !field.is_empty())
                        .map(str::to_string),
                );
            }
            Ok(fields)
        }
        _ => Err(serde::de::Error::custom(
            "expected a field value as a string or array of strings",
        )),
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SummaryBatchItem {
    #[serde(rename = "recordId")]
    pub record_id: String,
    pub result: Option<String>,
    #[serde(default, deserialize_with = "deserialize_summary_fields")]
    pub fields: Vec<String>,
    #[allow(dead_code)]
    pub summary: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SummaryBatchRequest {
    /// Array of summary requests to run in parallel (max 100).
    pub queries: Vec<SummaryBatchItem>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SummaryBatchResultItem {
    pub status: ApiStatus,
    pub summaries: Vec<super::summary::SummaryItem>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SummaryBatchResponse {
    pub status: ApiStatus,
    /// Per-query summary results.
    pub results: Vec<SummaryBatchResultItem>,
}

#[utoipa::path(
    post,
    path = "/api/v3/summary/batch",
    tag = "Data",
    summary = "Fetch aggregated summary statistics for multiple records in parallel",
    description = "Runs multiple `/summary` queries in a single request.  Each item in `queries` is processed independently and results are returned in the same order.",
    request_body(
        content = SummaryBatchRequest,
        examples(
            ("Two records" = (
                summary = "Fetch genome_size summary for two taxa",
                value = json!({
                    "queries": [
                        {"recordId": "9606", "result": "taxon", "fields": ["genome_size"]},
                        {"recordId": "7227", "result": "taxon", "fields": ["genome_size", "c_value"]}
                    ]
                })
            ))
        )
    ),
    responses(
        (status = 200, description = "Batch summary results", body = SummaryBatchResponse)
    )
)]
#[axum::debug_handler]
pub async fn post_summary_batch(
    Extension(state): Extension<Arc<AppState>>,
    Json(req): Json<SummaryBatchRequest>,
) -> Json<SummaryBatchResponse> {
    if req.queries.is_empty() {
        return Json(SummaryBatchResponse {
            status: ApiStatus::error("queries array must not be empty"),
            results: vec![],
        });
    }
    if req.queries.len() > 100 {
        return Json(SummaryBatchResponse {
            status: ApiStatus::error("maximum 100 queries per batch"),
            results: vec![],
        });
    }

    let mut results: Vec<SummaryBatchResultItem> = Vec::with_capacity(req.queries.len());

    for item in &req.queries {
        let result_type = item.result.as_deref().unwrap_or(&state.default_result);
        let idx = index_name::resolve_index_str(result_type, &state);

        let docs = match fetch_records::fetch_records_by_id(
            &state.client,
            &state.es_base,
            &idx,
            &[&item.record_id],
        )
        .await
        {
            Ok(d) => d,
            Err(e) => {
                results.push(SummaryBatchResultItem {
                    status: ApiStatus::error(e),
                    summaries: vec![],
                });
                continue;
            }
        };

        if docs.is_empty() {
            results.push(SummaryBatchResultItem {
                status: ApiStatus::error("record not found"),
                summaries: vec![],
            });
            continue;
        }

        let fields = super::summary::normalize_summary_fields(&item.fields);
        let summaries: Vec<super::summary::SummaryItem> = fields
            .iter()
            .map(|field| super::summary::SummaryItem {
                name: result_type.to_string(),
                field: field.to_string(),
                lineage: item.record_id.clone(),
                taxonomy: state.default_taxonomy.clone(),
                summary: serde_json::json!({}),
            })
            .collect();

        let count = summaries.len() as u64;
        results.push(SummaryBatchResultItem {
            status: ApiStatus::query_ok(count, 0),
            summaries,
        });
    }

    Json(SummaryBatchResponse {
        status: ApiStatus::query_ok(results.len() as u64, 0),
        results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_batch_accepts_list_fields() {
        let req: SummaryBatchRequest = serde_json::from_value(serde_json::json!({
            "queries": [
                {
                    "recordId": "9606",
                    "result": "taxon",
                    "fields": ["genome_size", "c_value"]
                }
            ]
        }))
        .unwrap();

        assert_eq!(req.queries[0].fields, vec!["genome_size", "c_value"]);
    }

    #[test]
    fn summary_batch_accepts_csv_fields_for_compatibility() {
        let req: SummaryBatchRequest = serde_json::from_value(serde_json::json!({
            "queries": [
                {
                    "recordId": "9606",
                    "result": "taxon",
                    "fields": "genome_size,c_value"
                }
            ]
        }))
        .unwrap();

        assert_eq!(req.queries[0].fields, vec!["genome_size", "c_value"]);
    }
}
