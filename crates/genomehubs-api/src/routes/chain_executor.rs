//! Execution of named sub-queries for chain substitution.
//!
//! This module is the I/O half of the chain-query system.  Pure logic
//! (type definitions, reference parsing, value substitution) lives in
//! `genomehubs_query::query::chain`.
//!
//! # How it fits in a request
//!
//! ```text
//! Deserialise SearchQuery
//!   → execute_named_queries()   ← this module
//!   → resolve_chain_refs()      ← genomehubs_query::query::chain
//!   → build ES query body
//!   → execute main ES query
//! ```
//!
//! Sub-queries that share the same target index are batched into a single
//! `_msearch` request to minimise round-trips.

use std::collections::HashMap;

use reqwest::Client;
use serde_json::{json, Value};

use genomehubs_query::query::chain::{ChainError, ChainRef, NamedQuerySpec};
use genomehubs_query::query::SearchIndex;

use crate::report::filter_expr::filter_expr_to_es_query;
use crate::AppState;

// ── execute_named_queries ─────────────────────────────────────────────────────

/// Execute all named sub-queries in a query's `named_queries` map and return
/// resolved field values, ready for
/// [`resolve_chain_refs`](genomehubs_query::query::chain::resolve_chain_refs).
///
/// Sub-queries targeting the same index are batched into a single `_msearch`
/// request.  Only the fields actually referenced in `chain_refs` are
/// requested from ES.
///
/// # Arguments
///
/// * `named_queries` — the `named_queries` map from the `SearchQuery`
/// * `chain_refs` — collected chain references from the main query attributes
///   (used to determine which fields to fetch)
/// * `parent_index` — the ES index name of the parent query (used when a
///   named query has no explicit index)
/// * `base_query` — the parent query's ES clause, used to scope same-index
///   sub-queries when `inherit_scope` is `true`
pub async fn execute_named_queries(
    named_queries: &HashMap<String, NamedQuerySpec>,
    chain_refs: &[ChainRef],
    parent_index: &str,
    base_query: &Value,
    state: &AppState,
) -> Result<HashMap<String, Vec<String>>, ChainError> {
    if named_queries.is_empty() {
        return Ok(HashMap::new());
    }

    // Group named query keys by their resolved ES index name.
    let mut by_index: HashMap<String, Vec<&str>> = HashMap::new();
    for (key, spec) in named_queries {
        let target_index = resolve_index(spec, parent_index, state);
        by_index.entry(target_index).or_default().push(key.as_str());
    }

    // For each index group, batch the queries via _msearch.
    let mut resolved: HashMap<String, Vec<String>> = HashMap::new();

    for (target_index, keys) in &by_index {
        let batch_results = execute_batch(
            &state.client,
            &state.es_base,
            target_index,
            parent_index,
            base_query,
            &keys
                .iter()
                .map(|k| (*k, named_queries.get(*k).unwrap()))
                .collect::<Vec<_>>(),
            chain_refs,
        )
        .await?;

        resolved.extend(batch_results);
    }

    Ok(resolved)
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Resolve the ES index name for a named query spec.
fn resolve_index(spec: &NamedQuerySpec, parent_index: &str, state: &AppState) -> String {
    match &spec.index {
        None => parent_index.to_string(),
        Some(idx) => index_for_search_index(idx, parent_index, state),
    }
}

/// Convert a [`SearchIndex`] enum to an ES index name.
fn index_for_search_index(idx: &SearchIndex, parent_index: &str, state: &AppState) -> String {
    let prefix = match idx {
        SearchIndex::Taxon => "taxon",
        SearchIndex::Assembly => "assembly",
        SearchIndex::Sample => "sample",
    };
    // Use the same index-name derivation as the parent index, replacing the
    // result-type prefix while keeping the suffix (taxonomy, date, etc.).
    // If we can't determine the suffix, fall back to the prefix alone.
    let sep = &state.index_separator;
    if let Some(suffix_start) = parent_index.find(sep.as_str()) {
        format!("{prefix}{sep}{}", &parent_index[suffix_start + sep.len()..])
    } else {
        prefix.to_string()
    }
}

/// Execute a batch of named sub-queries against a single ES index via `_msearch`.
///
/// Returns a map from query key → list of extracted field values.
async fn execute_batch(
    client: &Client,
    es_base: &str,
    target_index: &str,
    parent_index: &str,
    base_query: &Value,
    key_specs: &[(&str, &NamedQuerySpec)],
    chain_refs: &[ChainRef],
) -> Result<HashMap<String, Vec<String>>, ChainError> {
    // Build ndjson body.
    let mut ndjson = String::new();
    let header = json!({ "index": target_index });

    // For each key, determine the fields we need and build the ES query.
    let mut key_field_pairs: Vec<(&str, String)> = Vec::new();

    for (key, spec) in key_specs {
        // Find the field requested for this key (first chain ref wins).
        let field = chain_refs
            .iter()
            .find(|r| r.key == *key)
            .map(|r| r.field.clone())
            .unwrap_or_default();

        // Decide whether to inherit parent scope.
        let same_index = target_index == parent_index;
        let inherit = spec.inherit_scope.unwrap_or(same_index);

        let es_query = if spec.filter_expr.is_empty() {
            if inherit {
                base_query.clone()
            } else {
                json!({ "match_all": {} })
            }
        } else {
            let scope = if inherit {
                base_query
            } else {
                &json!({ "match_all": {} })
            };
            filter_expr_to_es_query(&spec.filter_expr, scope).map_err(|e| {
                ChainError::SubQueryFailed {
                    key: key.to_string(),
                    message: e,
                }
            })?
        };

        let size = spec.effective_limit();
        let body = json!({
            "query": es_query,
            "size": size,
            "_source": [field],
        });

        ndjson.push_str(&serde_json::to_string(&header).unwrap());
        ndjson.push('\n');
        ndjson.push_str(&serde_json::to_string(&body).unwrap());
        ndjson.push('\n');

        key_field_pairs.push((key, field));
    }

    let url = format!("{}/{target_index}/_msearch", es_base.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .header("Content-Type", "application/x-ndjson")
        .body(ndjson)
        .send()
        .await
        .map_err(|e| ChainError::SubQueryFailed {
            key: key_specs
                .first()
                .map(|(k, _)| (*k).to_string())
                .unwrap_or_default(),
            message: format!("msearch request failed: {e}"),
        })?;

    let data: Value = resp.json().await.map_err(|e| ChainError::SubQueryFailed {
        key: key_specs
            .first()
            .map(|(k, _)| (*k).to_string())
            .unwrap_or_default(),
        message: format!("msearch response parse error: {e}"),
    })?;

    let responses = data["responses"]
        .as_array()
        .ok_or_else(|| ChainError::SubQueryFailed {
            key: key_specs
                .first()
                .map(|(k, _)| (*k).to_string())
                .unwrap_or_default(),
            message: "msearch response missing 'responses' array".to_string(),
        })?;

    // Extract field values from each response.
    let mut result: HashMap<String, Vec<String>> = HashMap::new();
    for ((key, field), response) in key_field_pairs.iter().zip(responses.iter()) {
        if let Some(error) = response.get("error") {
            return Err(ChainError::SubQueryFailed {
                key: key.to_string(),
                message: format!("ES error: {error}"),
            });
        }
        let hits = response["hits"]["hits"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let values: Vec<String> = hits
            .iter()
            .filter_map(|hit| hit["_source"].get(field.as_str()))
            .filter_map(|v| match v {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .collect();
        result.insert(key.to_string(), values);
    }

    Ok(result)
}
