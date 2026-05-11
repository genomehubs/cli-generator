use axum::{extract::Json, Extension};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::sync::Arc;

use super::deserialize_helpers;
use crate::{es_client, index_name, routes::ApiStatus, AppState};

/// Combine multiple ES query bodies using bool.should (OR) or bool.must (AND).
fn combine_es_bodies(
    bodies: Vec<Value>,
    combine_with: &genomehubs_query::query::CombineStrategy,
) -> Value {
    if bodies.is_empty() {
        return serde_json::json!({ "query": { "match_all": {} } });
    }
    if bodies.len() == 1 {
        return bodies.into_iter().next().unwrap();
    }

    // Extract the "query" clause from each body; combine with bool.should/must
    let queries: Vec<Value> = bodies
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

    // Preserve the size/from from the first body, apply combined query
    let mut result = bodies.into_iter().next().unwrap();
    if let Some(obj) = result.as_object_mut() {
        obj.insert("query".to_string(), combined_query);
    }
    result
}

#[derive(utoipa::ToSchema)]
pub struct SearchRequest {
    pub query_yaml: String,
    pub params_yaml: String,
}

impl<'de> Deserialize<'de> for SearchRequest {
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

        Ok(SearchRequest {
            query_yaml,
            params_yaml,
        })
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SearchResponse {
    pub status: ApiStatus,
    /// URL that was built for this query (for debugging/reproduction).
    pub url: String,
    /// Flat result records.
    pub results: Vec<Value>,
    /// Cursor for the next page, if more results exist.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_after: Option<Value>,
    /// Per-rank ancestor aggregation results.
    ///
    /// Shape: `{rank: {ancestor_taxon_id: {field: distribution}}}`.
    /// Only present when `lineage_rank_summary` was requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lineage_summary: Option<Value>,
}

#[utoipa::path(
    post,
    path = "/api/v3/search",
    request_body = SearchRequest,
    responses(
        (status = 200, description = "Search results", body = SearchResponse)
    )
)]
#[axum::debug_handler]
pub async fn post_search(
    Extension(state): Extension<Arc<AppState>>,
    Json(req): Json<SearchRequest>,
) -> Json<SearchResponse> {
    macro_rules! bail {
        ($msg:expr) => {
            return Json(SearchResponse {
                status: ApiStatus::error($msg),
                url: String::new(),
                results: vec![],
                search_after: None,
                lineage_summary: None,
            })
        };
    }

    let query = match genomehubs_query::query::SearchQuery::from_yaml(&req.query_yaml) {
        Ok(q) => q,
        Err(e) => bail!(format!("failed to parse query_yaml: {e}")),
    };

    let params = match genomehubs_query::query::QueryParams::from_yaml(&req.params_yaml) {
        Ok(p) => p,
        Err(e) => bail!(format!("failed to parse params_yaml: {e}")),
    };

    // Derive a TypesMap from the startup metadata cache so build_search_body can pick
    // the single correct typed-value docvalue field (e.g. half_float_value) per attribute
    // rather than requesting all possible typed-value fields.
    let types_map: Option<cli_generator::core::attr_types::TypesMap> =
        if let Some(ref arc) = state.cache {
            let guard = arc.read().await;
            Some(guard.as_types_map())
        } else {
            None
        };

    // Check if this is a multi-query request (top-level OR/AND)
    if let Some(nested_queries) = &query.queries {
        if nested_queries.is_empty() {
            bail!("multi-query mode requires at least one query in the queries array");
        }
        if nested_queries.len() > 10 {
            bail!("maximum 10 queries for multi-query combining");
        }

        // Validate all queries use the same index
        let first_index = &nested_queries[0].index;
        if !nested_queries.iter().all(|q| &q.index == first_index) {
            bail!("all queries in multi-query mode must use the same index");
        }

        // Build a body for each nested query
        let mut bodies: Vec<Value> = vec![];
        for nested_query in nested_queries {
            let group = match nested_query.index {
                genomehubs_query::query::SearchIndex::Taxon => "taxon",
                genomehubs_query::query::SearchIndex::Assembly => "assembly",
                genomehubs_query::query::SearchIndex::Sample => "sample",
            };

            let offset = (params.page.saturating_sub(1)) * params.size;

            // Create temporary vectors for field/name/rank references
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

            // Build taxa query fragment from identifiers
            let taxa_query = nested_query
                .identifiers
                .taxa
                .as_ref()
                .map(|t| format!("{}({})", t.filter_type.api_function(), t.names.join(",")));

            let body = match cli_generator::core::query_builder::build_search_body(
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
                offset,
                types_map.as_ref(),
                Some(group),
            ) {
                Ok(b) => b,
                Err(e) => bail!(format!("failed to build ES body for nested query: {}", e)),
            };
            bodies.push(body);
        }

        // Combine the bodies with bool.should or bool.must
        let combined_body = combine_es_bodies(bodies, &query.combine_with);

        // Get the index name for the first query (we validated they're all the same)
        let idx = index_name::resolve_index(first_index, &state);

        // For v3 API POST endpoints, return the endpoint path
        // (actual query is in the JSON request body, not URL-reproducible)
        let built_url = "/api/v3/search".to_string();

        let raw =
            match es_client::execute_search(&state.client, &state.es_base, &idx, &combined_body)
                .await
            {
                Ok(v) => v,
                Err(e) => bail!(e),
            };

        // Extract status block from ES response
        let hits_count = raw
            .get("hits")
            .and_then(|h| h.get("total"))
            .and_then(|t| t.get("value"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let took_ms = raw.get("took").and_then(|v| v.as_u64()).unwrap_or(0);

        // Transform raw ES response to API response format expected by parse_search_json
        let group_name = match first_index {
            genomehubs_query::query::SearchIndex::Taxon => "taxon",
            genomehubs_query::query::SearchIndex::Assembly => "assembly",
            genomehubs_query::query::SearchIndex::Sample => "sample",
        };

        let es_hits = raw
            .get("hits")
            .and_then(|h| h.get("hits"))
            .and_then(|hits| hits.as_array());

        let results: Vec<Value> = es_hits
            .map(|hits| {
                hits.iter()
                    .map(|hit| {
                        deserialize_helpers::transform_es_hit(
                            hit,
                            group_name,
                            params.include_lineage,
                            params.include_taxon_names,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Extract search_after cursor for pagination
        let search_after = raw
            .get("hits")
            .and_then(|h| h.get("hits"))
            .and_then(|hits| hits.as_array())
            .and_then(|arr| arr.last())
            .and_then(|last| last.get("sort"))
            .cloned();

        return Json(SearchResponse {
            status: ApiStatus::query_ok(hits_count, took_ms),
            url: built_url,
            results,
            search_after,
            lineage_summary: None,
        });
    }

    // Single-query mode (existing behavior)
    let idx = index_name::resolve_index(&query.index, &state);

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

    let offset = (params.page.saturating_sub(1)) * params.size;

    // Build taxa query fragment from identifiers
    let taxa_query = query
        .identifiers
        .taxa
        .as_ref()
        .map(|t| format!("{}({})", t.filter_type.api_function(), t.names.join(",")));

    // `build_search_body` is in cli_generator::core::query_builder
    let mut body = match cli_generator::core::query_builder::build_search_body(
        taxa_query.as_deref(),
        fields_slice.as_deref(),
        None,
        Some(&query.attributes.attributes),
        query.identifiers.rank.as_deref(),
        names_slice.as_deref(),
        ranks_slice.as_deref(),
        sort_by,
        sort_order,
        params.size,
        offset,
        types_map.as_ref(),
        Some(group),
    ) {
        Ok(b) => b,
        Err(e) => bail!(format!("failed to build ES body: {}", e)),
    };

    // Inject lineage_rank_summary aggregations when requested
    if let Some(specs) = &query.lineage_rank_summary {
        if specs.len() > 5 {
            bail!("lineage_rank_summary: maximum 5 rank specs per request".to_string());
        }
        if let Err(e) =
            super::lineage_agg::validate_lineage_rank_summary_fields(specs, &state.cache)
        {
            bail!(e);
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
                Err(e) => bail!(format!("lineage_rank_summary: {e}")),
            }
        }
    }

    // For v3 API POST endpoints, return the endpoint path
    // (actual query is in the JSON request body, not URL-reproducible)
    let built_url = "/api/v3/search".to_string();

    let raw = match es_client::execute_search(&state.client, &state.es_base, &idx, &body).await {
        Ok(v) => v,
        Err(e) => bail!(e),
    };

    // Extract hits and took directly from raw ES response (not wrapped in API status)
    let hits_count = raw
        .get("hits")
        .and_then(|h| h.get("total"))
        .and_then(|t| t.get("value"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let took_ms = raw.get("took").and_then(|v| v.as_u64()).unwrap_or(0);

    // Lineage is needed for ancestor lookup when lineage_rank_summary is requested
    let include_lineage = params.include_lineage
        || query
            .lineage_rank_summary
            .as_ref()
            .map_or(false, |s| !s.is_empty());

    // Transform raw ES response to API response format expected by parse_search_json
    let es_hits = raw
        .get("hits")
        .and_then(|h| h.get("hits"))
        .and_then(|hits| hits.as_array());

    let results: Vec<Value> = es_hits
        .map(|hits| {
            hits.iter()
                .map(|hit| {
                    deserialize_helpers::transform_es_hit(
                        hit,
                        group,
                        include_lineage,
                        params.include_taxon_names,
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    // Extract search_after cursor for pagination
    let search_after = raw
        .get("hits")
        .and_then(|h| h.get("hits"))
        .and_then(|hits| hits.as_array())
        .and_then(|arr| arr.last())
        .and_then(|last| last.get("sort"))
        .cloned();

    // Extract lineage summary if it was requested
    let lineage_summary = query
        .lineage_rank_summary
        .as_deref()
        .filter(|specs| !specs.is_empty())
        .map(|specs| super::lineage_agg::extract_lineage_summary(&raw, specs));

    Json(SearchResponse {
        status: ApiStatus::query_ok(hits_count, took_ms),
        url: built_url,
        results,
        search_after,
        lineage_summary,
    })
}
