use axum::{extract::Json, Extension};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

use super::deserialize_helpers;
use crate::{es_client, index_name, AppState};

#[derive(utoipa::ToSchema)]
pub struct CountRequest {
    pub query_yaml: String,
    pub params_yaml: String,
}

impl<'de> Deserialize<'de> for CountRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de;
        let map = Value::deserialize(deserializer)?;

        // Get query from either "query" or "query_yaml" field
        let query_yaml = if let Some(query_val) = map.get("query").or_else(|| map.get("query_yaml"))
        {
            let normalized = deserialize_helpers::normalize_query(query_val.clone());
            deserialize_helpers::to_yaml(&normalized)?
        } else {
            return Err(de::Error::missing_field("query or query_yaml"));
        };

        // Get params from either "params" or "params_yaml" field
        let params_yaml =
            if let Some(params_val) = map.get("params").or_else(|| map.get("params_yaml")) {
                deserialize_helpers::to_yaml(params_val)?
            } else {
                return Err(de::Error::missing_field("params or params_yaml"));
            };

        Ok(CountRequest {
            query_yaml,
            params_yaml,
        })
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CountResponse {
    pub status: super::ApiStatus,
    /// The executed ES URL
    pub url: String,
}

#[utoipa::path(
    post,
    path = "/api/v3/count",
    tag = "Data",
    summary = "Count records matching a search query",
    description = "Fetch the record count for a search query without including any of the results.\n\n`/count` supports all relevant `/search` parameters to allow a record count to be obtained for a query prior to a full search.",
    request_body(
        content = CountRequest,
        examples(
            ("Mammalia species count" = (
                summary = "Count species in Mammalia with a genome size estimate",
                value = json!({"query_yaml": "index: taxon\nquery: tax_tree(Mammalia) AND genome_size\n", "params_yaml": "size: 0\ninclude_estimates: true\ntaxonomy: ncbi\n"})
            )),
            ("Assembly count" = (
                summary = "Count assemblies for Mammalia",
                value = json!({"query_yaml": "index: assembly\nquery: tax_tree(Mammalia)\n", "params_yaml": "size: 0\ntaxonomy: ncbi\n"})
            ))
        )
    ),
    responses(
        (status = 200, description = "Count result", body = CountResponse)
    )
)]
#[axum::debug_handler]
pub async fn post_count(
    Extension(state): Extension<Arc<AppState>>,
    Json(req): Json<CountRequest>,
) -> Json<CountResponse> {
    // Parse YAML inputs
    let query = match genomehubs_query::query::SearchQuery::from_yaml(&req.query_yaml) {
        Ok(q) => q,
        Err(e) => {
            return Json(CountResponse {
                status: super::ApiStatus::error(format!("failed to parse query_yaml: {}", e)),
                url: "".to_string(),
            })
        }
    };

    let params = match genomehubs_query::query::QueryParams::from_yaml(&req.params_yaml) {
        Ok(p) => p,
        Err(e) => {
            return Json(CountResponse {
                status: super::ApiStatus::error(format!("failed to parse params_yaml: {}", e)),
                url: "".to_string(),
            })
        }
    };

    if let Err(e) = params.validate_id_set() {
        return Json(CountResponse {
            status: super::ApiStatus::error(e),
            url: "".to_string(),
        });
    }

    // Resolve index name using shared helper
    let idx = index_name::resolve_index(&query.index, &state);

    // Build ES request body using the shared query_builder
    let group = match query.index {
        genomehubs_query::query::SearchIndex::Taxon => "taxon",
        genomehubs_query::query::SearchIndex::Assembly => "assembly",
        genomehubs_query::query::SearchIndex::Sample => "sample",
    };

    let fields_slice: Option<Vec<&str>> = if query.attributes.fields.is_empty() {
        None
    } else {
        Some(
            query
                .attributes
                .fields
                .iter()
                .map(|f| f.name.as_str())
                .collect(),
        )
    };
    let names_slice: Option<Vec<&str>> = if query.attributes.names.is_empty() {
        None
    } else {
        Some(query.attributes.names.iter().map(|s| s.as_str()).collect())
    };
    let ranks_slice: Option<Vec<&str>> = if query.attributes.ranks.is_empty() {
        None
    } else {
        Some(query.attributes.ranks.iter().map(|s| s.as_str()).collect())
    };

    let sort_by = params.sort_by.as_deref();
    let sort_order = Some(match params.sort_order {
        genomehubs_query::query::SortOrder::Asc => "asc",
        genomehubs_query::query::SortOrder::Desc => "desc",
    });

    let size = 0usize;
    let offset = (params.page.saturating_sub(1)) * params.size;

    // Build taxa query fragment from identifiers
    let taxa_query = query
        .identifiers
        .taxa
        .as_ref()
        .map(|t| format!("{}({})", t.filter_type.api_function(), t.names.join(",")));

    // Build a URL for the response (for debugging/reproduction)
    let built_url =
        genomehubs_query::query::build_query_url(&query, &params, &state.es_base, "v3", "count");

    let body = match cli_generator::core::query_builder::build_search_body(
        taxa_query.as_deref(),
        fields_slice.as_deref(),
        None,
        Some(&query.attributes.attributes),
        query.identifiers.rank.as_deref(),
        names_slice.as_deref(),
        ranks_slice.as_deref(),
        sort_by,
        sort_order,
        size,
        offset,
        None,
        Some(group),
    ) {
        Ok(b) => b,
        Err(e) => {
            return Json(CountResponse {
                status: super::ApiStatus::error(format!("failed to build ES body: {}", e)),
                url: built_url,
            })
        }
    };

    // Inject id_set filter if provided
    let mut body = body;
    if let Some(id_field) = params.resolve_id_field(group) {
        if let Some(ids) = &params.id_set {
            super::inject_id_set_filter(&mut body, &id_field, ids);
        }
    }

    // Extract only the query clause for the count endpoint (which expects {"query": {...}})
    let count_body = json!({
        "query": body
            .get("query")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({"match_all": {}}))
    });

    // Execute count query against ES using shared helper
    let raw = match es_client::execute_count(&state.client, &state.es_base, &idx, &count_body).await
    {
        Ok(v) => v,
        Err(e) => {
            return Json(CountResponse {
                status: super::ApiStatus::error(e),
                url: built_url,
            })
        }
    };

    // Extract hits and took from ES response
    if let Some(total) = raw.get("count").and_then(|c| c.as_u64()) {
        let took = raw.get("took").and_then(|t| t.as_u64()).unwrap_or(0);
        return Json(CountResponse {
            status: super::ApiStatus::query_ok(total, took),
            url: built_url,
        });
    }

    Json(CountResponse {
        status: super::ApiStatus::error(format!(
            "unexpected ES count response: {}",
            raw.to_string().chars().take(512).collect::<String>()
        )),
        url: built_url,
    })
}
