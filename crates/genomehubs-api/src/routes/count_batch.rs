use axum::{extract::Json, Extension};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::sync::Arc;

use super::deserialize_helpers;
use crate::{index_name, routes::ApiStatus, AppState};

/// Combine multiple ES query bodies using bool.should (OR) or bool.must (AND).
fn combine_es_bodies(
    bodies: Vec<serde_json::Value>,
    combine_with: &genomehubs_query::query::CombineStrategy,
) -> serde_json::Value {
    if bodies.is_empty() {
        return serde_json::json!({ "query": { "match_all": {} }, "size": 0 });
    }
    if bodies.len() == 1 {
        return bodies.into_iter().next().unwrap();
    }

    // Extract the "query" clause from each body; combine with bool.should/must
    let queries: Vec<serde_json::Value> = bodies
        .iter()
        .filter_map(|b| b.get("query").cloned())
        .collect();

    let combined_query = match combine_with {
        genomehubs_query::query::CombineStrategy::OR => {
            serde_json::json!({
                "bool": {
                    "should": queries,
                    "minimum_should_match": 1
                }
            })
        }
        genomehubs_query::query::CombineStrategy::AND => {
            serde_json::json!({
                "bool": {
                    "must": queries
                }
            })
        }
    };

    // Preserve size from the first body, apply combined query
    let mut result = bodies.into_iter().next().unwrap();
    if let Some(obj) = result.as_object_mut() {
        obj.insert("query".to_string(), combined_query);
    }
    result
}

#[derive(utoipa::ToSchema)]
pub struct CountBatchItem {
    pub query_yaml: String,
    pub params_yaml: String,
}

impl<'de> Deserialize<'de> for CountBatchItem {
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

        Ok(CountBatchItem {
            query_yaml,
            params_yaml,
        })
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CountBatchRequest {
    /// Array of queries to count in batch (max 100).
    pub searches: Vec<CountBatchItem>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CountBatchResultItem {
    pub status: ApiStatus,
    pub count: u64,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CountBatchResponse {
    pub status: ApiStatus,
    /// Sum of all individual result counts.
    pub total: u64,
    /// Count of unique results across all searches (only set if all searches use same index).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique: Option<u64>,
    /// Per-query count results.
    pub results: Vec<CountBatchResultItem>,
}

/// Build ES _msearch body (NDJSON format — alternating header + body lines).
fn build_msearch_body(searches: &[(String, serde_json::Value)]) -> String {
    searches
        .iter()
        .flat_map(|(index, body)| {
            let header = serde_json::json!({ "index": index });
            vec![
                serde_json::to_string(&header).unwrap(),
                serde_json::to_string(body).unwrap(),
            ]
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Execute batch count on ES using _msearch API.
async fn execute_msearch(
    client: &reqwest::Client,
    es_base: &str,
    ndjson_body: &str,
) -> Result<serde_json::Value, String> {
    let url = format!("{es_base}/_msearch");
    let resp = client
        .post(&url)
        .header("Content-Type", "application/x-ndjson")
        .body(ndjson_body.to_string())
        .send()
        .await
        .map_err(|e| format!("msearch request failed: {e}"))?;
    resp.json().await.map_err(|e| format!("parse error: {e}"))
}

/// For lineage(X), extract all ancestor taxon_ids by querying lineage of X.
/// Returns comma-separated list of ancestor taxon_ids.
async fn resolve_lineage_taxon_ids(
    client: &reqwest::Client,
    es_base: &str,
    index: &str,
    taxon_name: &str,
) -> Result<String, String> {
    // Build a query to find the taxon and get its lineage
    let query_body = serde_json::json!({
        "query": {
            "bool": {
                "should": [
                    { "match": { "taxon_id": taxon_name.to_lowercase() } },
                    {
                        "nested": {
                            "path": "taxon_names",
                            "query": {
                                "bool": {
                                    "filter": [
                                        { "match": { "taxon_names.name": taxon_name.to_lowercase() } }
                                    ]
                                }
                            }
                        }
                    }
                ]
            }
        },
        "size": 100,
        "_source": ["lineage"]
    });

    let url = format!("{es_base}/{index}/_search");
    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&query_body)
        .send()
        .await
        .map_err(|e| format!("lineage query failed: {e}"))?;

    let result: serde_json::Value = resp.json().await.map_err(|e| format!("parse error: {e}"))?;

    // Extract all taxon_ids from lineage arrays
    let mut taxon_ids = std::collections::HashSet::new();

    if let Some(hits) = result
        .get("hits")
        .and_then(|h| h.get("hits"))
        .and_then(|h| h.as_array())
    {
        for hit in hits {
            if let Some(source) = hit.get("_source") {
                if let Some(lineage) = source.get("lineage").and_then(|l| l.as_array()) {
                    for ancestor in lineage {
                        if let Some(taxon_id) = ancestor.get("taxon_id").and_then(|t| t.as_str()) {
                            taxon_ids.insert(taxon_id.to_string());
                        }
                    }
                }
            }
        }
    }

    if taxon_ids.is_empty() {
        return Err(format!("no lineage found for taxon: {}", taxon_name));
    }

    Ok(taxon_ids.into_iter().collect::<Vec<_>>().join(","))
}

#[utoipa::path(
    post,
    path = "/api/v3/count/batch",
    tag = "Data",
    summary = "Count records for multiple queries in a single request",
    description = "Execute up to 100 count queries simultaneously using a single POST. Reduces network round-trips for parallel counts.",
    request_body(
        content = CountBatchRequest,
        examples(
            ("Two counts" = (
                summary = "Count Mammalia and Insecta taxa in parallel",
                value = json!({"queries": [{"query_yaml": "index: taxon\nquery: tax_tree(Mammalia)\n", "params_yaml": "taxonomy: ncbi\n"}, {"query_yaml": "index: taxon\nquery: tax_tree(Insecta)\n", "params_yaml": "taxonomy: ncbi\n"}]})
            ))
        )
    ),
    responses(
        (status = 200, description = "Batch count results", body = CountBatchResponse)
    )
)]
pub async fn post_count_batch(
    Extension(state): Extension<Arc<AppState>>,
    Json(req): Json<CountBatchRequest>,
) -> Json<CountBatchResponse> {
    if req.searches.len() > 100 {
        return Json(CountBatchResponse {
            status: ApiStatus::error("maximum 100 searches per request".to_string()),
            total: 0,
            unique: None,
            results: vec![],
        });
    }

    // Parse and build all searches (with size: 0 for counts only)
    let mut index_bodies: Vec<(String, serde_json::Value)> = vec![];
    for item in &req.searches {
        let query = match genomehubs_query::query::SearchQuery::from_yaml(&item.query_yaml) {
            Ok(q) => q,
            Err(e) => {
                return Json(CountBatchResponse {
                    status: ApiStatus::error(format!("failed to parse query_yaml: {e}")),
                    total: 0,
                    unique: None,
                    results: vec![],
                })
            }
        };
        let params = match genomehubs_query::query::QueryParams::from_yaml(&item.params_yaml) {
            Ok(p) => p,
            Err(e) => {
                return Json(CountBatchResponse {
                    status: ApiStatus::error(format!("failed to parse params_yaml: {e}")),
                    total: 0,
                    unique: None,
                    results: vec![],
                })
            }
        };

        // Check if this is multi-query mode (nested queries)
        let body = if let Some(nested_queries) = &query.queries {
            if nested_queries.is_empty() {
                return Json(CountBatchResponse {
                    status: ApiStatus::error(
                        "multi-query mode requires at least one query".to_string(),
                    ),
                    total: 0,
                    unique: None,
                    results: vec![],
                });
            }
            if nested_queries.len() > 10 {
                return Json(CountBatchResponse {
                    status: ApiStatus::error(
                        "maximum 10 queries for multi-query combining".to_string(),
                    ),
                    total: 0,
                    unique: None,
                    results: vec![],
                });
            }

            // Validate all queries use the same index
            let first_index = &nested_queries[0].index;
            if !nested_queries.iter().all(|q| &q.index == first_index) {
                return Json(CountBatchResponse {
                    status: ApiStatus::error(
                        "all queries in multi-query mode must use the same index".to_string(),
                    ),
                    total: 0,
                    unique: None,
                    results: vec![],
                });
            }

            // Build bodies for each nested query (size: 0 for counts only)
            let mut bodies: Vec<serde_json::Value> = vec![];
            for nested_query in nested_queries {
                let group = match nested_query.index {
                    genomehubs_query::query::SearchIndex::Taxon => "taxon",
                    genomehubs_query::query::SearchIndex::Assembly => "assembly",
                    genomehubs_query::query::SearchIndex::Sample => "sample",
                };

                let field_names: Vec<&str> = nested_query
                    .attributes
                    .fields
                    .iter()
                    .map(|f| f.name.as_str())
                    .collect();

                let name_strs: Vec<&str> = nested_query
                    .attributes
                    .names
                    .iter()
                    .map(|s| s.as_str())
                    .collect();

                let rank_strs: Vec<&str> = nested_query
                    .attributes
                    .ranks
                    .iter()
                    .map(|s| s.as_str())
                    .collect();

                // Handle lineage filter: resolve ancestor taxa_ids first
                let mut resolved_taxa = nested_query.identifiers.taxa.clone();
                if let Some(taxa) = &resolved_taxa {
                    if matches!(
                        taxa.filter_type,
                        genomehubs_query::query::TaxonFilterType::Lineage
                    ) {
                        let idx = index_name::resolve_index(&nested_query.index, &state);
                        let lineage_ids = match resolve_lineage_taxon_ids(
                            &state.client,
                            &state.es_base,
                            &idx,
                            &taxa.names.join(","),
                        )
                        .await
                        {
                            Ok(ids) => ids,
                            Err(e) => {
                                return Json(CountBatchResponse {
                                    status: ApiStatus::error(format!(
                                        "lineage resolution failed: {e}"
                                    )),
                                    total: 0,
                                    unique: None,
                                    results: vec![],
                                })
                            }
                        };
                        // Replace with resolved IDs, use Name filter to match direct taxon_id
                        resolved_taxa = Some(genomehubs_query::query::TaxaIdentifier {
                            filter_type: genomehubs_query::query::TaxonFilterType::Name,
                            names: lineage_ids.split(',').map(|s| s.to_string()).collect(),
                        });
                    }
                }

                // Build taxa query fragment from identifiers
                let taxa_query = resolved_taxa
                    .as_ref()
                    .map(|t| format!("{}({})", t.filter_type.api_function(), t.names.join(",")));

                let b = match cli_generator::core::query_builder::build_search_body(
                    taxa_query.as_deref(),
                    if field_names.is_empty() {
                        None
                    } else {
                        Some(field_names.as_slice())
                    },
                    None,
                    Some(&nested_query.attributes.attributes),
                    nested_query.identifiers.rank.as_deref(),
                    if name_strs.is_empty() {
                        None
                    } else {
                        Some(name_strs.as_slice())
                    },
                    if rank_strs.is_empty() {
                        None
                    } else {
                        Some(rank_strs.as_slice())
                    },
                    params.sort_by.as_deref(),
                    Some(match params.sort_order {
                        genomehubs_query::query::SortOrder::Asc => "asc",
                        genomehubs_query::query::SortOrder::Desc => "desc",
                    }),
                    0, // size: 0 for counts only
                    0,
                    None,
                    Some(group),
                ) {
                    Ok(b) => b,
                    Err(e) => {
                        return Json(CountBatchResponse {
                            status: ApiStatus::error(format!(
                                "failed to build ES body for nested query: {e}"
                            )),
                            total: 0,
                            unique: None,
                            results: vec![],
                        })
                    }
                };
                bodies.push(b);
            }

            // Combine the bodies
            combine_es_bodies(bodies, &query.combine_with)
        } else {
            // Single-query mode (existing behavior)
            let _idx = index_name::resolve_index(&query.index, &state);
            let group = match query.index {
                genomehubs_query::query::SearchIndex::Taxon => "taxon",
                genomehubs_query::query::SearchIndex::Assembly => "assembly",
                genomehubs_query::query::SearchIndex::Sample => "sample",
            };

            let field_names: Vec<&str> = query
                .attributes
                .fields
                .iter()
                .map(|f| f.name.as_str())
                .collect();

            let name_strs: Vec<&str> = query.attributes.names.iter().map(|s| s.as_str()).collect();

            let rank_strs: Vec<&str> = query.attributes.ranks.iter().map(|s| s.as_str()).collect();

            // Handle lineage filter: resolve ancestor taxa_ids first
            let mut resolved_taxa = query.identifiers.taxa.clone();
            if let Some(taxa) = &resolved_taxa {
                if matches!(
                    taxa.filter_type,
                    genomehubs_query::query::TaxonFilterType::Lineage
                ) {
                    let idx = index_name::resolve_index(&query.index, &state);
                    let lineage_ids = match resolve_lineage_taxon_ids(
                        &state.client,
                        &state.es_base,
                        &idx,
                        &taxa.names.join(","),
                    )
                    .await
                    {
                        Ok(ids) => ids,
                        Err(e) => {
                            return Json(CountBatchResponse {
                                status: ApiStatus::error(format!("lineage resolution failed: {e}")),
                                total: 0,
                                unique: None,
                                results: vec![],
                            })
                        }
                    };
                    // Replace with resolved IDs, use Name filter to match direct taxon_id
                    resolved_taxa = Some(genomehubs_query::query::TaxaIdentifier {
                        filter_type: genomehubs_query::query::TaxonFilterType::Name,
                        names: lineage_ids.split(',').map(|s| s.to_string()).collect(),
                    });
                }
            }

            // Build taxa query fragment from identifiers
            let taxa_query = resolved_taxa
                .as_ref()
                .map(|t| format!("{}({})", t.filter_type.api_function(), t.names.join(",")));

            match cli_generator::core::query_builder::build_search_body(
                taxa_query.as_deref(),
                if field_names.is_empty() {
                    None
                } else {
                    Some(field_names.as_slice())
                },
                None,
                Some(&query.attributes.attributes),
                query.identifiers.rank.as_deref(),
                if name_strs.is_empty() {
                    None
                } else {
                    Some(name_strs.as_slice())
                },
                if rank_strs.is_empty() {
                    None
                } else {
                    Some(rank_strs.as_slice())
                },
                params.sort_by.as_deref(),
                Some(match params.sort_order {
                    genomehubs_query::query::SortOrder::Asc => "asc",
                    genomehubs_query::query::SortOrder::Desc => "desc",
                }),
                0, // size: 0 for counts only
                0,
                None,
                Some(group),
            ) {
                Ok(b) => b,
                Err(e) => {
                    return Json(CountBatchResponse {
                        status: ApiStatus::error(format!("failed to build ES body: {e}")),
                        total: 0,
                        unique: None,
                        results: vec![],
                    })
                }
            }
        };

        let idx = index_name::resolve_index(
            if let Some(nested) = &query.queries {
                &nested[0].index
            } else {
                &query.index
            },
            &state,
        );
        index_bodies.push((idx, body));
    }

    let ndjson_body = build_msearch_body(&index_bodies);

    // Execute the batch count
    let raw = match execute_msearch(&state.client, &state.es_base, &ndjson_body).await {
        Ok(v) => v,
        Err(e) => {
            return Json(CountBatchResponse {
                status: ApiStatus::error(e),
                total: 0,
                unique: None,
                results: vec![],
            })
        }
    };

    // Parse the ES _msearch response and extract counts
    let empty_arr = vec![];
    let responses = match raw.get("responses") {
        Some(r) => r.as_array().unwrap_or(&empty_arr),
        None => {
            return Json(CountBatchResponse {
                status: ApiStatus::error("malformed _msearch response".to_string()),
                total: 0,
                unique: None,
                results: vec![],
            })
        }
    };

    let mut results: Vec<CountBatchResultItem> = vec![];

    for response in responses {
        let status_result = match response.get("error") {
            Some(error_obj) => {
                // Individual query error
                let error_msg = error_obj
                    .get("reason")
                    .and_then(|r| r.as_str())
                    .unwrap_or("unknown error");
                ApiStatus::error(error_msg.to_string())
            }
            None => {
                // Successful query — extract hit count
                let hits = response
                    .get("hits")
                    .and_then(|h| h.get("total"))
                    .and_then(|t| {
                        // ES 7.0+ returns {"value": N, "relation": "..."}
                        // Earlier versions return just N
                        t.get("value").or(Some(t)).and_then(|v| v.as_u64())
                    })
                    .unwrap_or(0);

                let took = response.get("took").and_then(|t| t.as_u64()).unwrap_or(0);

                ApiStatus::query_ok(hits, took)
            }
        };

        // Extract count from the status
        let count = status_result.hits.unwrap_or(0);

        results.push(CountBatchResultItem {
            status: status_result,
            count,
        });
    }

    // Aggregate status — success if all queries succeeded, error otherwise
    let all_ok = results.iter().all(|r| r.status.success);
    let total_took: u64 = results.iter().filter_map(|r| r.status.took).sum();
    let total_hits: u64 = results.iter().filter_map(|r| r.status.hits).sum();

    let aggregate_status = if all_ok {
        ApiStatus::query_ok(total_hits, total_took)
    } else {
        ApiStatus::error("one or more queries failed".to_string())
    };

    // Compute unique count if all searches use the same index
    let unique_count = if all_ok && !index_bodies.is_empty() {
        // Check if all searches use the same index
        let first_index = &index_bodies[0].0;
        if index_bodies.iter().all(|(idx, _)| idx == first_index) {
            // Combine all bodies with OR and run unified query
            let bodies: Vec<serde_json::Value> =
                index_bodies.iter().map(|(_, b)| b.clone()).collect();
            let combined_body =
                combine_es_bodies(bodies, &genomehubs_query::query::CombineStrategy::OR);

            // Build NDJSON for single unified query
            let header = serde_json::json!({ "index": first_index });
            let ndjson = format!(
                "{}
{}
",
                serde_json::to_string(&header).unwrap(),
                serde_json::to_string(&combined_body).unwrap()
            );

            // Execute the unified query
            match execute_msearch(&state.client, &state.es_base, &ndjson).await {
                Ok(raw) => {
                    let empty_arr = vec![];
                    let responses = raw
                        .get("responses")
                        .and_then(|r| r.as_array())
                        .unwrap_or(&empty_arr);
                    if let Some(response) = responses.first() {
                        if response.get("error").is_none() {
                            response
                                .get("hits")
                                .and_then(|h| h.get("total"))
                                .and_then(|t| t.get("value").or(Some(t)).and_then(|v| v.as_u64()))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                Err(_) => None,
            }
        } else {
            None
        }
    } else {
        None
    };

    Json(CountBatchResponse {
        status: aggregate_status,
        total: total_hits,
        unique: unique_count,
        results,
    })
}
