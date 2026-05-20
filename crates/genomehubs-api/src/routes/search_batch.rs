use axum::{extract::Json, Extension};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::sync::Arc;

use super::deserialize_helpers;
use crate::{index_name, routes::ApiStatus, AppState};

#[derive(utoipa::ToSchema)]
pub struct SearchBatchItem {
    pub query_yaml: String,
    pub params_yaml: String,
}

impl<'de> Deserialize<'de> for SearchBatchItem {
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

        Ok(SearchBatchItem {
            query_yaml,
            params_yaml,
        })
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SearchBatchRequest {
    /// Array of queries to search in batch (max 100).
    pub searches: Vec<SearchBatchItem>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SearchBatchResultItem {
    /// Total number of matching documents in ES (may exceed `results` length).
    pub total: u64,
    /// Document results in the same `{index, id, score, result}` format as `/api/v3/search`.
    pub results: Vec<serde_json::Value>,
    /// Cursor for the next page, if more results exist.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_after: Option<serde_json::Value>,
    /// Per-rank ancestor aggregation results (only present when `lineage_rank_summary` was requested).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lineage_summary: Option<serde_json::Value>,
    /// Error message for this query if it failed independently.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Per-query metadata needed to transform `_msearch` ES responses.
struct BatchQueryMeta {
    group: String,
    include_lineage: bool,
    include_taxon_names: bool,
    lineage_specs: Option<Vec<genomehubs_query::query::LineageRankSummarySpec>>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SearchBatchResponse {
    pub status: ApiStatus,
    /// Per-query search results.
    pub results: Vec<SearchBatchResultItem>,
}

/// Build ES _msearch body (NDJSON format — alternating header + body lines).
fn build_msearch_body(searches: &[(String, serde_json::Value)]) -> String {
    crate::es_client::build_msearch_body(searches)
}

/// Execute batch search on ES using _msearch API.
async fn execute_msearch(
    client: &reqwest::Client,
    es_base: &str,
    ndjson_body: &str,
) -> Result<serde_json::Value, String> {
    crate::es_client::execute_msearch(client, es_base, ndjson_body).await
}

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
    path = "/api/v3/search/batch",
    tag = "Data",
    summary = "Fetch records for multiple search queries in a single request",
    description = "Execute up to 100 search queries simultaneously using a single POST. Reduces network round-trips for parallel searches.",
    request_body(
        content = SearchBatchRequest,
        examples(
            ("Two searches" = (
                summary = "Search Mammalia and Insecta in parallel",
                value = json!({"queries": [{"query_yaml": "index: taxon\nquery: tax_tree(Mammalia)\n", "params_yaml": "size: 5\nfields: genome_size\ntaxonomy: ncbi\n"}, {"query_yaml": "index: taxon\nquery: tax_tree(Insecta)\n", "params_yaml": "size: 5\nfields: genome_size\ntaxonomy: ncbi\n"}]})
            ))
        )
    ),
    responses(
        (status = 200, description = "Batch search results", body = SearchBatchResponse)
    )
)]
pub async fn post_search_batch(
    Extension(state): Extension<Arc<AppState>>,
    Json(req): Json<SearchBatchRequest>,
) -> Json<SearchBatchResponse> {
    if req.searches.len() > 100 {
        return Json(SearchBatchResponse {
            status: ApiStatus::error("maximum 100 searches per request".to_string()),
            results: vec![],
        });
    }

    // Derive a TypesMap from startup metadata so build_search_body can select the
    // correct typed-value docvalue field per attribute.
    let types_map: Option<cli_generator::core::attr_types::TypesMap> =
        if let Some(ref arc) = state.cache {
            let guard = arc.read().await;
            Some(guard.as_types_map())
        } else {
            None
        };

    // Parse and build all searches (with size from params for actual document results)
    let mut index_bodies: Vec<(String, serde_json::Value)> = vec![];
    let mut metas: Vec<BatchQueryMeta> = vec![];
    for item in &req.searches {
        let query = match genomehubs_query::query::SearchQuery::from_yaml(&item.query_yaml) {
            Ok(q) => q,
            Err(e) => {
                return Json(SearchBatchResponse {
                    status: ApiStatus::error(format!("failed to parse query_yaml: {e}")),
                    results: vec![],
                })
            }
        };
        let params = match genomehubs_query::query::QueryParams::from_yaml(&item.params_yaml) {
            Ok(p) => p,
            Err(e) => {
                return Json(SearchBatchResponse {
                    status: ApiStatus::error(format!("failed to parse params_yaml: {e}")),
                    results: vec![],
                })
            }
        };

        // Check if this is multi-query mode (nested queries with OR/AND combining)
        let body = if let Some(nested_queries) = &query.queries {
            if nested_queries.is_empty() {
                return Json(SearchBatchResponse {
                    status: ApiStatus::error(
                        "multi-query mode requires at least one query".to_string(),
                    ),
                    results: vec![],
                });
            }
            if nested_queries.len() > 10 {
                return Json(SearchBatchResponse {
                    status: ApiStatus::error(
                        "maximum 10 queries for multi-query combining".to_string(),
                    ),
                    results: vec![],
                });
            }

            // Validate all queries use the same index
            let first_index = &nested_queries[0].index;
            if !nested_queries.iter().all(|q| &q.index == first_index) {
                return Json(SearchBatchResponse {
                    status: ApiStatus::error(
                        "all queries in multi-query mode must use the same index".to_string(),
                    ),
                    results: vec![],
                });
            }

            // Build bodies for each nested query (with size from params for documents)
            let mut bodies: Vec<serde_json::Value> = vec![];
            for nested_query in nested_queries {
                let group = match nested_query.index {
                    genomehubs_query::query::SearchIndex::Taxon => "taxon",
                    genomehubs_query::query::SearchIndex::Assembly => "assembly",
                    genomehubs_query::query::SearchIndex::Sample => "sample",
                    genomehubs_query::query::SearchIndex::Feature => "feature",
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
                                return Json(SearchBatchResponse {
                                    status: ApiStatus::error(format!(
                                        "lineage resolution failed: {e}"
                                    )),
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
                    params.size,
                    (params.page - 1) * params.size,
                    types_map.as_ref(),
                    Some(group),
                ) {
                    Ok(b) => b,
                    Err(e) => {
                        return Json(SearchBatchResponse {
                            status: ApiStatus::error(format!(
                                "failed to build ES body for nested query: {e}"
                            )),
                            results: vec![],
                        })
                    }
                };
                bodies.push(b);
            }

            // Combine the bodies with OR or AND
            combine_es_bodies(bodies, &query.combine_with)
        } else {
            // Single-query mode (existing behavior)
            let group = match query.index {
                genomehubs_query::query::SearchIndex::Taxon => "taxon",
                genomehubs_query::query::SearchIndex::Assembly => "assembly",
                genomehubs_query::query::SearchIndex::Sample => "sample",
                genomehubs_query::query::SearchIndex::Feature => "feature",
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
                            return Json(SearchBatchResponse {
                                status: ApiStatus::error(format!("lineage resolution failed: {e}")),
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

            let sort_by = params.sort_by.as_deref();
            let sort_order = match params.sort_order {
                genomehubs_query::query::SortOrder::Asc => "asc",
                genomehubs_query::query::SortOrder::Desc => "desc",
            };
            let offset = (params.page - 1) * params.size;

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
                sort_by,
                Some(sort_order),
                params.size,
                offset,
                types_map.as_ref(),
                Some(group),
            ) {
                Ok(b) => b,
                Err(e) => {
                    return Json(SearchBatchResponse {
                        status: ApiStatus::error(format!("failed to build ES body: {e}")),
                        results: vec![],
                    })
                }
            }
        };

        let idx = index_name::resolve_index(&query.index, &state);

        // Inject id_set terms filter when caller supplied a set of IDs.
        let mut body = body;
        if let Some(ids) = &params.id_set {
            let index_str = match &query.index {
                genomehubs_query::query::SearchIndex::Taxon => "taxon",
                genomehubs_query::query::SearchIndex::Assembly => "assembly",
                genomehubs_query::query::SearchIndex::Sample => "sample",
                genomehubs_query::query::SearchIndex::Feature => "feature",
            };
            if let Some(field) = params.resolve_id_field(index_str) {
                super::inject_id_set_filter(&mut body, &field, ids);
            }
        }

        // Inject lineage_rank_summary aggregations when requested.
        let lineage_specs = query.lineage_rank_summary.clone();
        if let Some(specs) = &lineage_specs {
            if specs.len() > 5 {
                return Json(SearchBatchResponse {
                    status: ApiStatus::error(
                        "lineage_rank_summary: maximum 5 rank specs per request".to_string(),
                    ),
                    results: vec![],
                });
            }
            if let Err(e) =
                super::lineage_agg::validate_lineage_rank_summary_fields(specs, &state.cache)
            {
                return Json(SearchBatchResponse {
                    status: ApiStatus::error(e),
                    results: vec![],
                });
            }
            let aggs = body
                .as_object_mut()
                .unwrap()
                .entry("aggs")
                .or_insert_with(|| serde_json::json!({}));
            for spec in specs {
                let size = super::lineage_agg::ancestor_bucket_size_for_rank(&spec.rank);
                match super::lineage_agg::build_lineage_rank_summary_agg(spec, size, &state.cache) {
                    Ok((name, agg_body)) => {
                        aggs[name] = agg_body;
                    }
                    Err(e) => {
                        return Json(SearchBatchResponse {
                            status: ApiStatus::error(format!("lineage_rank_summary: {e}")),
                            results: vec![],
                        })
                    }
                }
            }
        }

        // Determine the group string for response transformation.
        let response_group = match query.index {
            genomehubs_query::query::SearchIndex::Taxon => "taxon",
            genomehubs_query::query::SearchIndex::Assembly => "assembly",
            genomehubs_query::query::SearchIndex::Sample => "sample",
            genomehubs_query::query::SearchIndex::Feature => "feature",
        };
        let include_lineage =
            params.include_lineage || lineage_specs.as_ref().is_some_and(|s| !s.is_empty());

        metas.push(BatchQueryMeta {
            group: response_group.to_string(),
            include_lineage,
            include_taxon_names: params.include_taxon_names,
            lineage_specs,
        });
        index_bodies.push((idx, body));
    }

    let ndjson_body = build_msearch_body(&index_bodies);

    // Execute the batch search
    let raw = match execute_msearch(&state.client, &state.es_base, &ndjson_body).await {
        Ok(v) => v,
        Err(e) => {
            return Json(SearchBatchResponse {
                status: ApiStatus::error(e),
                results: vec![],
            })
        }
    };

    // Parse the ES _msearch response and extract hits
    let empty_arr = vec![];
    let responses = match raw.get("responses") {
        Some(r) => r.as_array().unwrap_or(&empty_arr),
        None => {
            return Json(SearchBatchResponse {
                status: ApiStatus::error("malformed _msearch response".to_string()),
                results: vec![],
            })
        }
    };

    let mut results: Vec<SearchBatchResultItem> = vec![];
    let mut total_hits = 0u64;

    for (response, meta) in responses.iter().zip(metas.iter()) {
        match response.get("error") {
            Some(error_obj) => {
                // Individual query error — return empty results with error message.
                let error_msg = error_obj
                    .get("reason")
                    .and_then(|r| r.as_str())
                    .unwrap_or("unknown error");
                results.push(SearchBatchResultItem {
                    total: 0,
                    results: vec![],
                    search_after: None,
                    lineage_summary: None,
                    error: Some(error_msg.to_string()),
                });
            }
            None => {
                // Successful query — transform hits into {index, id, score, result} format.
                let hit_count = response
                    .get("hits")
                    .and_then(|h| h.get("total"))
                    .and_then(|t| {
                        // ES 7.0+ returns {"value": N, "relation": "..."}
                        // Earlier versions return just N
                        t.get("value").or(Some(t)).and_then(|v| v.as_u64())
                    })
                    .unwrap_or(0);

                let result_docs: Vec<Value> = response
                    .get("hits")
                    .and_then(|h| h.get("hits"))
                    .and_then(|h| h.as_array())
                    .map(|arr| {
                        arr.iter()
                            .map(|hit| {
                                deserialize_helpers::transform_es_hit(
                                    hit,
                                    &meta.group,
                                    meta.include_lineage,
                                    meta.include_taxon_names,
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                // Extract search_after cursor for pagination.
                let search_after = response
                    .get("hits")
                    .and_then(|h| h.get("hits"))
                    .and_then(|hits| hits.as_array())
                    .and_then(|arr| arr.last())
                    .and_then(|last| last.get("sort"))
                    .cloned();

                // Extract lineage summary if it was requested for this query.
                let lineage_summary = meta
                    .lineage_specs
                    .as_deref()
                    .filter(|specs| !specs.is_empty())
                    .map(|specs| super::lineage_agg::extract_lineage_summary(response, specs));

                total_hits += hit_count;
                results.push(SearchBatchResultItem {
                    total: hit_count,
                    results: result_docs,
                    search_after,
                    lineage_summary,
                    error: None,
                });
            }
        }
    }

    // Aggregate status — success if all queries succeeded, error otherwise.
    let all_ok = results.iter().all(|r| r.error.is_none());
    let aggregate_status = if all_ok {
        ApiStatus::query_ok(total_hits, 0)
    } else {
        ApiStatus::error("one or more queries failed".to_string())
    };

    Json(SearchBatchResponse {
        status: aggregate_status,
        results,
    })
}
