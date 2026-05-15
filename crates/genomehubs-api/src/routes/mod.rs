use serde::Serialize;

/// Uniform status block present in every v3 API response.
///
/// Metadata-only endpoints (taxonomies, ranks, indices) omit `hits` and `took`.
/// Query endpoints always populate all four fields.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ApiStatus {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hits: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub took: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ApiStatus {
    /// Status for a successful metadata-only endpoint (no hits/took).
    pub fn ok() -> Self {
        Self {
            success: true,
            hits: None,
            took: None,
            error: None,
        }
    }

    /// Status for a successful query endpoint.
    pub fn query_ok(hits: u64, took: u64) -> Self {
        Self {
            success: true,
            hits: Some(hits),
            took: Some(took),
            error: None,
        }
    }

    /// Status for a failed request.
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            hits: None,
            took: None,
            error: Some(msg.into()),
        }
    }
}

pub mod chain_executor;
pub mod count;
pub mod count_batch;
pub mod deserialize_helpers;
pub mod indices;
pub mod lineage_agg;
pub mod lookup;
pub mod lookup_batch;
pub mod metadata;
pub mod phylopic;
pub mod positional;
pub mod record;
pub mod record_batch;
pub mod report;
pub mod result_fields;
pub mod search;
pub mod search_batch;
pub mod status;
pub mod summary;
pub mod summary_batch;
pub mod taxonomic_ranks;
pub mod taxonomies;

/// Inject an `id_set` terms filter into an ES query body.
///
/// Wraps the existing `query` clause in a `bool.must` alongside a `terms`
/// filter, restricting results to exactly the supplied IDs.
///
/// If the body has no `query` clause, `match_all` is used as the base.
pub fn inject_id_set_filter(body: &mut serde_json::Value, field: &str, ids: &[String]) {
    let existing_query = body
        .get("query")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({"match_all": {}}));

    let new_query = serde_json::json!({
        "bool": {
            "must": [
                existing_query,
                { "terms": { field: ids } }
            ]
        }
    });

    if let Some(obj) = body.as_object_mut() {
        obj.insert("query".to_string(), new_query);
    }
}
