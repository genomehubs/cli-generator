//! Routes for the PhyloPic proxy: `GET /api/v3/phylopic` and `POST /api/v3/phylopic/batch`.
//!
//! Both handlers consult the shared `PhylopicCache` before making external HTTP
//! requests, and write resolved records back to the cache so subsequent requests
//! are served without network round-trips (as long as the PhyloPic build number
//! has not advanced).

use axum::{extract::Query, Extension, Json};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};

use crate::{
    fetch_records, index_name,
    phylopic_client::{self, PhylopicRecord, TaxonInfo, TaxonName},
    routes::ApiStatus,
    AppState,
};

// ── Request / response types ──────────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PhylopicQuery {
    /// NCBI taxon ID.
    pub taxon_id: String,
    /// Taxonomy name (e.g. `"ncbi"`).
    pub taxonomy: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct PhylopicResponse {
    pub status: ApiStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phylopic: Option<PhylopicRecord>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PhylopicBatchRequest {
    /// Up to 200 NCBI taxon IDs.
    pub taxon_ids: Vec<String>,
    /// Taxonomy name (e.g. `"ncbi"`).
    pub taxonomy: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct PhylopicBatchResponse {
    pub status: ApiStatus,
    pub results: HashMap<String, PhylopicResponse>,
}

// ── GET /api/v3/phylopic ──────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/v3/phylopic",
    tag = "External",
    summary = "Get PhyloPic silhouette for a taxon",
    description = "Fetches a PhyloPic silhouette image record for a given NCBI taxon ID. Results are cached per API build cycle.",
    params(
        ("taxon_id" = String, Query, description = "NCBI taxon ID"),
        ("taxonomy" = String, Query, description = "Taxonomy name (e.g. ncbi)"),
    ),
    responses(
        (status = 200, description = "PhyloPic silhouette record", body = PhylopicResponse)
    )
)]
pub async fn get_phylopic(
    Query(q): Query<PhylopicQuery>,
    Extension(state): Extension<Arc<AppState>>,
) -> Json<PhylopicResponse> {
    // Consult cache first
    {
        let cache = state.phylopic_cache.read().await;
        if let Some(record) = cache.get(&q.taxon_id) {
            return Json(PhylopicResponse {
                status: ApiStatus::ok(),
                phylopic: Some(record.clone()),
            });
        }
    }

    let build = {
        let cache = state.phylopic_cache.read().await;
        cache.current_build
    };

    // Fetch taxon info from ES to populate the resolution pipeline
    let info = match fetch_taxon_info(&state, &q.taxon_id, &q.taxonomy).await {
        Ok(info) => info,
        Err(msg) => {
            return Json(PhylopicResponse {
                status: ApiStatus::error(msg),
                phylopic: None,
            })
        }
    };

    // Resolve via PhyloPic
    match phylopic_client::resolve(&state.client, &info, build).await {
        Ok(mut record) => {
            record.image_name = info.scientific_name.clone();
            let mut cache = state.phylopic_cache.write().await;
            cache.insert(q.taxon_id.clone(), record.clone());
            Json(PhylopicResponse {
                status: ApiStatus::ok(),
                phylopic: Some(record),
            })
        }
        Err(phylopic_client::PhylopicError::NotFound) => Json(PhylopicResponse {
            status: ApiStatus::error(format!("no image found for taxon_id {}", q.taxon_id)),
            phylopic: None,
        }),
        Err(e) => Json(PhylopicResponse {
            status: ApiStatus::error(e.to_string()),
            phylopic: None,
        }),
    }
}

// ── POST /api/v3/phylopic/batch ───────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/api/v3/phylopic/batch",
    tag = "External",
    summary = "Resolve PhyloPic silhouettes for up to 200 taxa",
    description = "Fetches PhyloPic silhouette image records for up to 200 NCBI taxon IDs in a single POST. Results are returned as a map keyed by taxon ID.",
    request_body = PhylopicBatchRequest,
    responses(
        (status = 200, description = "Per-taxon PhyloPic results", body = PhylopicBatchResponse)
    )
)]
pub async fn post_phylopic_batch(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<PhylopicBatchRequest>,
) -> Json<PhylopicBatchResponse> {
    if body.taxon_ids.is_empty() {
        return Json(PhylopicBatchResponse {
            status: ApiStatus::error("taxon_ids must not be empty"),
            results: HashMap::new(),
        });
    }
    if body.taxon_ids.len() > 200 {
        return Json(PhylopicBatchResponse {
            status: ApiStatus::error("taxon_ids must contain at most 200 entries"),
            results: HashMap::new(),
        });
    }

    let build = {
        let cache = state.phylopic_cache.read().await;
        cache.current_build
    };

    let mut results: HashMap<String, PhylopicResponse> = HashMap::new();
    let mut uncached_ids: Vec<String> = Vec::new();

    // Serve cache hits immediately
    {
        let cache = state.phylopic_cache.read().await;
        for id in &body.taxon_ids {
            if let Some(record) = cache.get(id) {
                results.insert(
                    id.clone(),
                    PhylopicResponse {
                        status: ApiStatus::ok(),
                        phylopic: Some(record.clone()),
                    },
                );
            } else {
                uncached_ids.push(id.clone());
            }
        }
    }

    // Fetch taxon info for cache misses and resolve
    for id in &uncached_ids {
        let response = match fetch_taxon_info(&state, id, &body.taxonomy).await {
            Err(msg) => PhylopicResponse {
                status: ApiStatus::error(msg),
                phylopic: None,
            },
            Ok(info) => match phylopic_client::resolve(&state.client, &info, build).await {
                Ok(mut record) => {
                    record.image_name = info.scientific_name.clone();
                    let mut cache = state.phylopic_cache.write().await;
                    cache.insert(id.clone(), record.clone());
                    PhylopicResponse {
                        status: ApiStatus::ok(),
                        phylopic: Some(record),
                    }
                }
                Err(phylopic_client::PhylopicError::NotFound) => PhylopicResponse {
                    status: ApiStatus::error(format!("no image found for taxon_id {id}")),
                    phylopic: None,
                },
                Err(e) => PhylopicResponse {
                    status: ApiStatus::error(e.to_string()),
                    phylopic: None,
                },
            },
        };
        results.insert(id.clone(), response);
    }

    Json(PhylopicBatchResponse {
        status: ApiStatus::ok(),
        results,
    })
}

// ── ES taxon info fetch ───────────────────────────────────────────────────────

/// Fetch lineage and name data for a taxon from Elasticsearch.
///
/// Queries the taxon index for the given `taxon_id` and extracts the fields
/// needed by the PhyloPic resolution pipeline.
async fn fetch_taxon_info(
    state: &AppState,
    taxon_id: &str,
    _taxonomy: &str,
) -> Result<TaxonInfo, String> {
    let idx = index_name::resolve_index_str("taxon", state);
    let doc_id = format!("taxon-{taxon_id}");

    let sources =
        fetch_records::fetch_records_by_id(&state.client, &state.es_base, &idx, &[doc_id.as_str()])
            .await
            .map_err(|e| format!("ES returned {e} for taxon_id {taxon_id}"))?;

    let source = sources
        .into_iter()
        .next()
        .ok_or_else(|| format!("taxon_id {taxon_id} not found in index {idx}"))?;

    let scientific_name = source["scientific_name"]
        .as_str()
        .unwrap_or(taxon_id)
        .to_string();

    let rank = source["taxon_rank"]
        .as_str()
        .unwrap_or("species")
        .to_string();

    let taxon_names: Vec<TaxonName> = source["taxon_names"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    v["name"].as_str().map(|s| TaxonName {
                        name: s.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let lineage_ids: Vec<String> = source["lineage"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v["taxon_id"].as_u64().map(|id| id.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let gbif_lineage_keys: Vec<String> = source["lineage"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    v["gbif_id"]
                        .as_u64()
                        .or_else(|| v["gbif_id"].as_str().and_then(|s| s.parse().ok()))
                        .map(|id| id.to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(TaxonInfo {
        taxon_id: taxon_id.to_string(),
        scientific_name,
        rank,
        taxon_names,
        lineage_ids,
        gbif_lineage_keys,
    })
}
