use axum::{extract::Json, Extension};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::sync::Arc;

use cli_generator::core::query_builder;
use genomehubs_query::query::chain::{collect_chain_refs, resolve_chain_refs};
use genomehubs_query::query::{QueryParams, SearchQuery};
use genomehubs_query::report::axis::AxisOpts;

use crate::{index_name, report::report_types, routes::ApiStatus, AppState};

#[derive(utoipa::ToSchema)]
pub struct ReportRequest {
    pub query_yaml: String,
    pub params_yaml: String,
    pub report_yaml: String,
}

impl<'de> Deserialize<'de> for ReportRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de;
        let map = Value::deserialize(deserializer)?;

        // Helper to convert value to YAML string
        let to_yaml = |val: &Value| -> Result<String, D::Error> {
            match val {
                Value::String(s) => Ok(s.clone()),
                _ => serde_yaml::to_string(val).map_err(de::Error::custom),
            }
        };

        // Get query from either "query" or "query_yaml" field
        let query_yaml = if let Some(query_val) = map.get("query").or_else(|| map.get("query_yaml"))
        {
            to_yaml(query_val)?
        } else {
            return Err(de::Error::missing_field("query or query_yaml"));
        };

        // Get params from either "params" or "params_yaml" field
        let params_yaml =
            if let Some(params_val) = map.get("params").or_else(|| map.get("params_yaml")) {
                to_yaml(params_val)?
            } else {
                return Err(de::Error::missing_field("params or params_yaml"));
            };

        // Get report from either "report" or "report_yaml" field
        let report_yaml =
            if let Some(report_val) = map.get("report").or_else(|| map.get("report_yaml")) {
                to_yaml(report_val)?
            } else {
                return Err(de::Error::missing_field("report or report_yaml"));
            };

        Ok(ReportRequest {
            query_yaml,
            params_yaml,
            report_yaml,
        })
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ReportResponse {
    pub status: ApiStatus,
    pub report: Value,
}

#[utoipa::path(
    post,
    path = "/api/v3/report",
    request_body = ReportRequest,
    responses((status = 200, description = "Report data", body = ReportResponse))
)]
#[axum::debug_handler]
pub async fn post_report(
    Extension(state): Extension<Arc<AppState>>,
    Json(req): Json<ReportRequest>,
) -> Json<ReportResponse> {
    macro_rules! bail {
        ($msg:expr) => {
            return Json(ReportResponse {
                status: ApiStatus::error($msg),
                report: Value::Null,
            })
        };
    }

    // Parse query YAML
    let mut search_query = match SearchQuery::from_yaml(&req.query_yaml) {
        Ok(q) => q,
        Err(e) => bail!(format!("invalid query_yaml: {e}")),
    };

    // Parse params YAML
    let params = match QueryParams::from_yaml(&req.params_yaml) {
        Ok(p) => p,
        Err(e) => bail!(format!("invalid params_yaml: {e}")),
    };

    // Parse report YAML
    let report_config: serde_yaml::Value = match serde_yaml::from_str(&req.report_yaml) {
        Ok(v) => v,
        Err(e) => bail!(format!("invalid report_yaml: {e}")),
    };

    // Resolve index name from search_query
    let idx = index_name::resolve_index(&search_query.index, &state);

    // Chain substitution: if the query has named_queries, execute them and
    // substitute values into attribute filters before building the ES query.
    if let Some(named_queries) = &search_query.named_queries.clone() {
        let chain_refs = collect_chain_refs(&search_query.attributes.attributes);
        if !chain_refs.is_empty() {
            let resolved = match crate::routes::chain_executor::execute_named_queries(
                named_queries,
                &chain_refs,
                &idx,
                &Value::Object(Default::default()),
                &state,
            )
            .await
            {
                Ok(r) => r,
                Err(e) => bail!(format!("chain query failed: {e}")),
            };
            if let Err(e) = resolve_chain_refs(
                &mut search_query.attributes.attributes,
                &resolved,
                named_queries,
            ) {
                bail!(format!("chain resolution failed: {e}"));
            }
        }
    }

    // Extract report type (default to "histogram")
    let report_type = report_config
        .get("report")
        .and_then(|v| v.as_str())
        .unwrap_or("histogram");

    // Derive a TypesMap from the startup metadata cache so build_search_body can pick
    // the single correct typed-value docvalue field per attribute.
    let types_map: Option<cli_generator::core::attr_types::TypesMap> =
        if let Some(ref arc) = state.cache {
            let guard = arc.read().await;
            Some(guard.as_types_map())
        } else {
            None
        };

    // Build base query from search parameters
    let base_query = match build_report_query(
        &search_query,
        &params,
        &state.default_taxonomy,
        types_map.as_ref(),
    ) {
        Ok(q) => q,
        Err(e) => bail!(e),
    };

    // Dispatch to appropriate report handler
    let result = match report_type {
        "histogram" => {
            report_types::run_histogram_report(
                &state,
                &idx,
                &search_query,
                &params,
                &report_config,
                &base_query,
            )
            .await
        }
        "scatter" => {
            report_types::run_scatter_report(
                &state,
                &idx,
                &search_query,
                &params,
                &report_config,
                &base_query,
            )
            .await
        }
        "xPerRank" => {
            report_types::run_x_per_rank_report(&state, &idx, &base_query, &report_config).await
        }
        "sources" => report_types::run_sources_report(&state, &idx, &base_query).await,
        "tree" => report_types::run_tree_report(&state, &idx, &base_query, &report_config).await,
        "map" => report_types::run_map_report(&state, &idx, &base_query, &report_config).await,
        "arc" => {
            use crate::report::arc::{run_arc_report, ArcConfig};
            match ArcConfig::from_yaml(&report_config) {
                Ok(cfg) => {
                    run_arc_report(&state.client, &state.es_base, &idx, &base_query, &cfg).await
                }
                Err(e) => Err(e),
            }
        }
        unknown => Err(format!("unknown report type: {unknown}")),
    };

    // Return response
    match result {
        Ok((hits, took, report_data)) => Json(ReportResponse {
            status: ApiStatus::query_ok(hits, took),
            report: report_data,
        }),
        Err(e) => Json(ReportResponse {
            status: ApiStatus::error(e),
            report: Value::Null,
        }),
    }
}

/// Build a base filter query from search parameters.
///
/// Uses the query module's query building logic to create a match-all-ish query
/// with the specified filters (taxa, filters, etc.).
fn build_report_query(
    query: &SearchQuery,
    _params: &QueryParams,
    _default_taxonomy: &str,
    types_map: Option<&cli_generator::core::attr_types::TypesMap>,
) -> Result<Value, String> {
    // Build taxa query expression from search query (None → match_all base)
    let taxa_query: Option<String> = query
        .identifiers
        .taxa
        .as_ref()
        .map(|t| format!("{}({})", t.filter_type.api_function(), t.names.join(",")));

    // Extract vectors for query builder
    let field_names: Vec<&str> = query
        .attributes
        .fields
        .iter()
        .map(|f| f.name.as_str())
        .collect();

    let name_strs: Vec<&str> = query.attributes.names.iter().map(|s| s.as_str()).collect();

    let rank_strs: Vec<&str> = query.attributes.ranks.iter().map(|s| s.as_str()).collect();

    // Determine group from index
    let group = match query.index {
        genomehubs_query::query::SearchIndex::Assembly => "assembly",
        genomehubs_query::query::SearchIndex::Sample => "sample",
        genomehubs_query::query::SearchIndex::Taxon => "taxon",
    };

    // Build full search body using query builder
    let body = cli_generator::core::query_builder::build_search_body(
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
        None,
        None,
        1, // size: only use for structuring query, not actual size
        0, // offset
        types_map,
        Some(group),
    )
    .map_err(|e| format!("query builder error: {e}"))?;

    // Extract just the query part from the full search body
    body.get("query")
        .cloned()
        .ok_or_else(|| "query builder produced no query clause".to_string())
}
