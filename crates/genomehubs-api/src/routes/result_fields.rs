use axum::{extract::Query, Extension, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::AppState;

#[derive(Deserialize)]
pub struct ResultFieldsQuery {
    pub result: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct FieldMeta {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: Option<String>,
    pub processed_type: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ResultFieldsResponse {
    pub status: super::ApiStatus,
    pub fields: serde_json::Value,
    pub identifiers: serde_json::Value,
    pub hub: String,
    pub release: String,
    pub source: String,
}

#[utoipa::path(
    get,
    path = "/api/v3/resultFields",
    params(
        ("result" = Option<String>, Query, description = "Result type (taxon|assembly|sample)"),

    ),
    responses(
        (status = 200, description = "Field metadata", body = ResultFieldsResponse)
    )
)]
pub async fn get_result_fields(
    axum::extract::Query(q): Query<ResultFieldsQuery>,
    Extension(state): Extension<Arc<AppState>>,
) -> Json<ResultFieldsResponse> {
    let result_type = q
        .result
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| state.default_result.clone());

    // Attempt to use cached attribute types populated on startup.
    let (fields_value, status) = if let Some(lock) = &state.cache {
        let r = lock.read().await;
        let attr_types = r.attr_types.clone();
        if !attr_types.is_object() {
            (json!({}), super::ApiStatus::error("no attr_types in cache"))
        } else {
            let fv = if result_type != "multi" {
                if let Some(group_val) = attr_types.get(&result_type) {
                    if group_val.is_object() {
                        serde_json::to_value(group_val).unwrap_or_else(|_| json!({}))
                    } else {
                        serde_json::to_value(&attr_types).unwrap_or_else(|_| json!({}))
                    }
                } else {
                    serde_json::to_value(&attr_types).unwrap_or_else(|_| json!({}))
                }
            } else {
                serde_json::to_value(&attr_types).unwrap_or_else(|_| json!({}))
            };
            (fv, super::ApiStatus::ok())
        }
    } else {
        (json!({}), super::ApiStatus::error("no cache configured"))
    };

    // identifiers currently not cached; return empty object for now.
    let identifiers_value = json!({});

    let resp = ResultFieldsResponse {
        status,
        fields: fields_value,
        identifiers: identifiers_value,
        hub: "genomehubs".to_string(),
        release: "2026-04-29".to_string(),
        source: "local".to_string(),
    };

    Json(resp)
}
