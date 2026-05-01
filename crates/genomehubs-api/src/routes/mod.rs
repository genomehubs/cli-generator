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

pub mod count;
pub mod indices;
pub mod result_fields;
pub mod search;
pub mod status;
pub mod taxonomic_ranks;
pub mod taxonomies;
