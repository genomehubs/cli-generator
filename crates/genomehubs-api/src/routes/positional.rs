//! `POST /api/v3/positional` — positional report family (oxford / ribbon / painting).
//!
//! **`query_yaml` is optional.**  When omitted, features are filtered only by
//! `group_by` (used as `feature_type`) and the explicit `assemblies` list.
//!
//! When provided, `query_yaml.index` controls the filter strategy:
//!
//! - `index: feature` — attribute filters (`attributes:`) are translated to
//!   nested ES clauses on the feature index.  Taxa are resolved via the taxon
//!   index just as in the `taxon` case below.
//!
//! - `index: taxon` — `taxa:` + `taxon_filter_type:` only; attribute filters
//!   are ignored (they would target the taxon index, not features).
//!   The feature index only stores `taxon_id` (keyword) and `ancestors`
//!   (array), so name resolution is performed via the taxon index.
//!
//! - `index: assembly` — resolve assembly IDs matching the query, then use them
//!   as the `assemblies` filter.  Assemblies must be omitted from
//!   `positional_yaml` when using this mode.
//!
//! **Feature attribute filter notes:**
//!
//! Feature attributes are stored in a nested `attributes` array with fields
//! `attributes.key`, `attributes.keyword_value`, and `attributes.long_value`.
//! Operators `eq`/`ne` on string values use `keyword_value`; numeric operators
//! (`lt`, `le`, `gt`, `ge`) use `long_value`.  `exists`/`missing` check both.

use axum::{extract::Json, Extension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use genomehubs_query::query::{
    attributes::AttributeOperator, SearchIndex, SearchQuery, TaxonFilterType,
};
use genomehubs_query::report::{FilterTarget, PositionalReportType, PositionalSpec};

use crate::{
    es_client::execute_search,
    es_metadata::FeatureIndexVersion,
    index_name,
    report::positional::{
        feature_query::{
            build_feature_direct_clauses, fetch_features_flat, fetch_sequence_lengths_flat,
            resolve_sequence_ids, resolve_window_ids, FeatureQueryFlat, FeatureRecord,
            SequenceRecord,
        },
        layout::{compute_offsets, order_sequences_by_median, orient_sequence, SequenceLayout},
        painter::{build_painting_segments, build_painting_segments_raw},
        region::compute_regions,
        window::{apply_window, RawPoint},
    },
    routes::ApiStatus,
    AppState,
};
// ── Request / Response types ──────────────────────────────────────────────────

#[derive(utoipa::ToSchema)]
pub struct PositionalRequest {
    /// Optional query filter.  When omitted features are filtered only by
    /// `group_by` (as `primary_type`) and the `assemblies` list.
    pub query_yaml: Option<String>,
    pub positional_yaml: String,
}

impl<'de> Deserialize<'de> for PositionalRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de;
        let map = Value::deserialize(deserializer)?;

        let to_yaml = |val: &Value| -> Result<String, D::Error> {
            match val {
                Value::String(s) => Ok(s.clone()),
                _ => serde_yaml::to_string(val).map_err(de::Error::custom),
            }
        };

        let query_yaml = map
            .get("query")
            .or_else(|| map.get("query_yaml"))
            .map(to_yaml)
            .transpose()?;

        let positional_yaml =
            if let Some(v) = map.get("positional").or_else(|| map.get("positional_yaml")) {
                to_yaml(v)?
            } else {
                return Err(de::Error::missing_field("positional or positional_yaml"));
            };

        Ok(PositionalRequest {
            query_yaml,
            positional_yaml,
        })
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct PositionalResponse {
    pub status: ApiStatus,
    pub report: Value,
}

// ── Handler ───────────────────────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/api/v3/positional",
    tag = "Data",
    summary = "Generate a positional report (oxford / ribbon / painting)",
    description = "Returns positional comparison data for one or more assemblies. \
        Supports oxford (2-assembly dot-plot), ribbon (N-assembly synteny), and \
        painting (single-assembly chromosome map).\n\n\
        `query_yaml` is **optional**. When omitted, features are filtered by \
        the `group_by` value (used as `primary_type`) automatically.\n\n\
        Set `index: feature` in `query_yaml` to apply attribute filters directly \
        to the feature index (e.g. `sequence_length >= 1000000`).\n\
        Set `index: taxon` + `taxa:` to restrict features to a given taxon subtree.",
    request_body(
        content = PositionalRequest,
        examples(
            ("Oxford — no query filter" = (
                summary = "Oxford dot-plot, features filtered by group_by only",
                value = json!({
                    "positional_yaml": "report: oxford\ngroup_by: busco_gene\nassemblies:\n  - GCA_905147045.1\n  - GCA_902806685.2\nwindow_size: 1000000\n"
                })
            )),
            ("Oxford — taxon filter" = (
                summary = "Oxford dot-plot with taxon subtree filter",
                value = json!({
                    "query_yaml": "index: taxon\ntaxa:\n  - Lepidoptera\ntaxon_filter_type: tree\n",
                    "positional_yaml": "report: oxford\ngroup_by: busco_gene\nassemblies:\n  - GCA_905147045.1\n  - GCA_902806685.2\nwindow_size: 1000000\n"
                })
            )),
            ("Oxford — feature attribute filter" = (
                summary = "Oxford dot-plot restricted to chromosomes ≥ 10 Mb",
                description = "When `index: feature` is set, attribute filters first resolve which \
                    sequences (chromosomes/scaffolds) match, then only features on those sequences \
                    are returned. This selects busco genes located on sequences with \
                    `length >= 10000000` (10 Mb).",
                value = json!({
                    "query_yaml": "index: feature\nattributes:\n  - name: length\n    operator: ge\n    value: \"10000000\"\n",
                    "positional_yaml": "report: oxford\ngroup_by: metazoa_odb10-busco-gene\nassemblies:\n  - GCA_905147045.1\n  - GCA_902806685.2\n"
                })
            )),
            ("Painting" = (
                summary = "Chromosome painting coloured by BUSCO status",
                value = json!({
                    "positional_yaml": "report: painting\ngroup_by: busco_gene\nassemblies:\n  - GCA_905147045.1\ncat: busco_status\nwindow_size: 500000\n"
                })
            ))
        )
    ),
    responses((status = 200, description = "Positional report data", body = PositionalResponse))
)]
#[axum::debug_handler]
pub async fn post_positional(
    Extension(state): Extension<Arc<AppState>>,
    Json(req): Json<PositionalRequest>,
) -> Json<PositionalResponse> {
    macro_rules! bail {
        ($msg:expr) => {
            return Json(PositionalResponse {
                status: ApiStatus::error($msg),
                report: Value::Null,
            })
        };
    }

    let started = Instant::now();

    // Parse optional query YAML
    let search_query: Option<SearchQuery> = match &req.query_yaml {
        Some(yaml) => match SearchQuery::from_yaml(yaml) {
            Ok(q) => Some(q),
            Err(e) => bail!(format!("invalid query_yaml: {e}")),
        },
        None => None,
    };

    // Parse positional YAML
    let spec: PositionalSpec = match serde_yaml::from_str(&req.positional_yaml) {
        Ok(s) => s,
        Err(e) => bail!(format!("invalid positional_yaml: {e}")),
    };

    // Gate: require a v2 feature index.  The two-stage sequence-ID workaround
    // used on v1 indices is removed; sites must rebuild the index.
    {
        let version = if let Some(lock) = &state.cache {
            lock.read().await.feature_index_version.clone()
        } else {
            FeatureIndexVersion::V1
        };
        if version != FeatureIndexVersion::V2 {
            let index_name = index_name::resolve_index_str("feature", &state);
            bail!(format!(
                "The /positional endpoint requires a v2 feature index. \
                 The current index '{}' uses v1 structure. \
                 Rebuild the index with the updated genomehubs-index pipeline.",
                index_name
            ));
        }
    }

    // Resolve assembly IDs: explicit list takes priority; otherwise resolve
    // from an assembly-index query if one is provided.
    let assembly_ids: Vec<String> = if !spec.assemblies.is_empty() {
        spec.assemblies.clone()
    } else {
        match &search_query {
            Some(q) if q.index == SearchIndex::Assembly => match resolve_assemblies_from_query(
                q,
                &state.client,
                &state.es_base,
                &index_name::resolve_index_str("assembly", &state),
                spec.report,
            )
            .await
            {
                Ok(ids) => ids,
                Err(e) => bail!(e),
            },
            _ => bail!(
                "assemblies list is empty and no assembly-index query was provided. \
                 either list assemblies in positional_yaml or set index: assembly in query_yaml"
                    .to_string()
            ),
        }
    };

    // Validate assembly counts per report type
    match spec.report {
        PositionalReportType::Oxford if assembly_ids.len() != 2 => {
            bail!(format!(
                "oxford report requires exactly 2 assemblies, got {}",
                assembly_ids.len()
            ))
        }
        PositionalReportType::Ribbon if assembly_ids.len() < 2 => {
            bail!(format!(
                "ribbon report requires at least 2 assemblies, got {}",
                assembly_ids.len()
            ))
        }
        PositionalReportType::Painting if assembly_ids.len() != 1 => {
            bail!(format!(
                "painting report requires exactly 1 assembly, got {}",
                assembly_ids.len()
            ))
        }
        PositionalReportType::Circos if assembly_ids.is_empty() => {
            bail!("circos report requires at least 1 assembly".to_string())
        }
        _ => {}
    }

    // Resolve feature index name
    let feature_idx = index_name::resolve_index_str("feature", &state);

    // Build taxon filter (resolved via taxon index, applicable for index:taxon and index:feature)
    let taxon_filter: Option<Value> = match &search_query {
        Some(q) => match build_feature_taxon_filter(
            q,
            &state.client,
            &state.es_base,
            &index_name::resolve_index_str("taxon", &state),
        )
        .await
        {
            Ok(f) => f,
            Err(e) => bail!(e),
        },
        None => None,
    };

    // Read known feature types from cache for smart feature-type resolution
    let known_feature_types: Vec<String> = if let Some(lock) = &state.cache {
        lock.read().await.feature_types.clone()
    } else {
        vec![]
    };

    // Build nested attribute filter clauses for index:feature queries.
    // These target fields that remain in the `attributes` array in the v2 index
    // (e.g. `status = "Complete"`).  The v1 two-stage sequence-ID workaround
    // has been removed: with the v2 index, `sequence_length` and `length` are
    // top-level fields and can be filtered directly via the `filter:` spec
    // (Step C).
    let attribute_filters: Vec<Value> = match &search_query {
        Some(q) if q.index == SearchIndex::Feature && !q.attributes.attributes.is_empty() => {
            build_feature_attribute_filters(&q.attributes.attributes)
        }
        _ => vec![],
    };

    // Dispatch spec.filter entries:
    //   Feature target  → build direct ES clauses (top-level or nested attribute)
    //   Sequence target → Type A chain: resolve sequence_ids, add terms clause
    //   Window target   → Type B chain: resolve container_ids, add terms clause
    //   FeatureType     → Type C chain: not yet implemented
    let mut spec_filter_clauses: Vec<Value> = build_feature_direct_clauses(&spec.filter);

    for af in &spec.filter {
        match &af.target {
            FilterTarget::Feature => {} // already handled above
            FilterTarget::Sequence => {
                match resolve_sequence_ids(
                    &state.client,
                    &state.es_base,
                    &feature_idx,
                    &assembly_ids,
                    af,
                )
                .await
                {
                    Ok(ids) if ids.is_empty() => bail!(format!(
                        "sequence chain filter on '{}': no sequences matched — \
                         no features can be returned",
                        af.field
                    )),
                    Ok(ids) => spec_filter_clauses.push(json!({ "terms": { "sequence_id": ids } })),
                    Err(e) => bail!(format!("sequence chain filter failed: {e}")),
                }
            }
            FilterTarget::Window { window_size, .. } => {
                match resolve_window_ids(
                    &state.client,
                    &state.es_base,
                    &feature_idx,
                    &assembly_ids,
                    af,
                    *window_size,
                )
                .await
                {
                    Ok(ids) if ids.is_empty() => bail!(format!(
                        "window chain filter on '{}': no windows matched — \
                         no features can be returned",
                        af.field
                    )),
                    Ok(ids) => {
                        spec_filter_clauses.push(json!({ "terms": { "container_ids": ids } }))
                    }
                    Err(e) => bail!(format!("window chain filter failed: {e}")),
                }
            }
            FilterTarget::FeatureType { .. } => {
                bail!("filter target 'feature_type' (Type C chain) is not yet implemented")
            }
        }
    }

    let combined_attribute_filters: Vec<Value> = attribute_filters
        .iter()
        .cloned()
        .chain(spec_filter_clauses)
        .collect();

    // Use spec.regions.cat as cat_field when spec.cat is absent, so region
    // colours are always populated even if the caller didn't set the top-level cat.
    let effective_cat_field: Option<&str> = spec
        .cat
        .as_deref()
        .or_else(|| spec.regions.as_ref().and_then(|r| r.cat.as_deref()));

    // Fetch sequence lengths (toplevel features, flat-field path).
    let seq_records = match fetch_sequence_lengths_flat(
        &state.client,
        &state.es_base,
        &feature_idx,
        &assembly_ids,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => bail!(format!("sequence length query failed: {e}")),
    };

    // Early validation: check if sequences were found
    if seq_records.is_empty() {
        bail!(format!(
            "no topLevel sequences found for assemblies: {} (feature index: '{feature_idx}'). \
             Check the feature index contains topLevel records for these assemblies. \
             Verify es_base and index_suffix are configured correctly.",
            assembly_ids.join(", ")
        ))
    }

    // Fetch positional features (group_by markers).
    // feature_type defaults to group_by so we always filter by the relevant
    // feature type — returning all types is almost never useful.
    let effective_feature_type = spec.feature_type.as_deref().unwrap_or(&spec.group_by);
    let feature_records = match fetch_features_flat(
        &state.client,
        &state.es_base,
        &feature_idx,
        &assembly_ids,
        &FeatureQueryFlat {
            group_by: &spec.group_by,
            feature_type: Some(effective_feature_type),
            cat_field: effective_cat_field,
            max_features: spec.max_features,
            taxon_filter: taxon_filter.as_ref(),
            attribute_filters: &combined_attribute_filters,
            known_feature_types: &known_feature_types,
        },
    )
    .await
    {
        Ok(r) => r,
        Err(e) => bail!(e),
    };

    // Early validation: check if features were found
    if feature_records.is_empty() {
        let hint = if req.query_yaml.is_some() {
            format!(
                "no features found with feature_type='{}'. \
                 check group_by matches a valid feature_type in the feature index and \
                 that any query_yaml filters match features in these assemblies",
                effective_feature_type
            )
        } else {
            format!(
                "no features found with feature_type='{}'. \
                 check group_by matches a valid feature_type in the feature index",
                effective_feature_type
            )
        };
        bail!(hint)
    }

    let hit_count = feature_records.len() as u64;

    // Build per-assembly layout
    let assembly_layouts =
        build_assembly_layouts(&assembly_ids, &seq_records, &feature_records, spec.reorient);

    // Build report payload
    let mut report_data = match spec.report {
        PositionalReportType::Oxford | PositionalReportType::Ribbon => {
            build_oxford_ribbon_report(&spec, &assembly_ids, &assembly_layouts, &feature_records)
        }
        PositionalReportType::Painting => {
            build_painting_report(&spec, &assembly_layouts, &feature_records)
        }
        PositionalReportType::Circos => {
            build_circos_report(&spec, &assembly_ids, &assembly_layouts, &feature_records)
        }
    };

    // Inject computed regions when the caller supplied a regions spec.
    if let Some(regions_spec) = &spec.regions {
        let region_records = compute_regions(&feature_records, regions_spec);
        let regions_json: Vec<Value> = region_records
            .iter()
            .map(|r| {
                let x_offset = assembly_layouts
                    .get(&r.assembly_id)
                    .and_then(|l| l.sequences.iter().find(|s| s.sequence_id == r.sequence_id))
                    .map(|s| s.offset)
                    .unwrap_or(0);
                json!({
                    "sequenceId":   r.sequence_id,
                    "assemblyId":   r.assembly_id,
                    "start":        r.start,
                    "end":          r.end,
                    "catValue":     r.cat_value,
                    "featureCount": r.feature_count,
                    "xOffset":      x_offset
                })
            })
            .collect();
        if let Some(obj) = report_data.as_object_mut() {
            obj.insert("regions".to_string(), json!(regions_json));
        }
    }

    let took = started.elapsed().as_millis() as u64;

    Json(PositionalResponse {
        status: ApiStatus::query_ok(hit_count, took),
        report: report_data,
    })
}

// ── Assembly resolution from query ───────────────────────────────────────────

/// Resolve assembly IDs from an assembly-index query when no explicit list is
/// given.  Runs a terms-aggregation (or small search) against the assembly
/// index and returns `assembly_id` values.
///
/// The number of assemblies returned is validated against the report type
/// after this function returns.
async fn resolve_assemblies_from_query(
    query: &SearchQuery,
    client: &reqwest::Client,
    es_base: &str,
    assembly_idx: &str,
    report_type: PositionalReportType,
) -> Result<Vec<String>, String> {
    // Build a size cap: oxford needs exactly 2, ribbon >= 2, painting = 1.
    // Use a small cap to avoid accidentally pulling hundreds of assemblies.
    let size = match report_type {
        PositionalReportType::Oxford => 2usize,
        PositionalReportType::Painting => 1,
        PositionalReportType::Ribbon | PositionalReportType::Circos => 50,
    };

    // Build a minimal taxa filter for the assembly query if taxa are specified
    let mut filters: Vec<Value> = vec![];
    if let Some(taxa) = &query.identifiers.taxa {
        if !taxa.names.is_empty() {
            let names_query = taxa
                .names
                .iter()
                .map(|n| json!({"multi_match": {"query": n, "fields": ["scientific_name", "taxon_names.name"]}}))
                .collect::<Vec<_>>();
            filters.push(json!({
                "bool": {
                    "should": names_query,
                    "minimum_should_match": 1
                }
            }));
        }
    }

    let body = if filters.is_empty() {
        json!({"query": {"match_all": {}}, "_source": ["assembly_id"], "size": size})
    } else {
        json!({"query": {"bool": {"filter": filters}}, "_source": ["assembly_id"], "size": size})
    };

    let raw = execute_search(client, es_base, assembly_idx, &body)
        .await
        .map_err(|e| format!("assembly query failed: {e}"))?;

    let hits = raw
        .pointer("/hits/hits")
        .and_then(|h| h.as_array())
        .ok_or_else(|| "assembly query: missing hits".to_string())?;

    if hits.is_empty() {
        return Err(
            "assembly query returned no results. check query_yaml taxa filters".to_string(),
        );
    }

    let ids: Vec<String> = hits
        .iter()
        .filter_map(|h| {
            h.get("_source")
                .and_then(|s| s.get("assembly_id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();

    if ids.is_empty() {
        return Err("assembly query returned hits but no assembly_id field. \
                    check the assembly index mapping"
            .to_string());
    }

    Ok(ids)
}

// ── Feature attribute filter builder ─────────────────────────────────────────

/// Convert `SearchQuery` attribute filters to nested ES filter clauses for the
/// feature index.
///
/// Each attribute becomes a `nested` clause on `path: "attributes"` with:
/// - a `match` on `attributes.key` for the field name
/// - a value constraint on `attributes.keyword_value` (string equality) or
///   `attributes.long_value` (numeric range / equality)
///
/// Without type metadata we infer the value type by trying to parse it as a
/// number: numeric → `long_value`, otherwise → `keyword_value`.
fn build_feature_attribute_filters(
    attrs: &[genomehubs_query::query::attributes::Attribute],
) -> Vec<Value> {
    let mut filters = Vec::new();

    for attr in attrs {
        let name = &attr.name;
        let op = match &attr.operator {
            Some(o) => o,
            None => {
                // No operator: existence test
                filters.push(json!({
                    "nested": {
                        "path": "attributes",
                        "query": {
                            "bool": {
                                "filter": [
                                    {"match": {"attributes.key": name}},
                                    {"bool": {
                                        "should": [
                                            {"exists": {"field": "attributes.long_value"}},
                                            {"exists": {"field": "attributes.keyword_value"}}
                                        ],
                                        "minimum_should_match": 1
                                    }}
                                ]
                            }
                        }
                    }
                }));
                continue;
            }
        };

        let value_str: Option<String> = attr.value.as_ref().map(|v| match v {
            genomehubs_query::query::attributes::AttributeValue::Single(s) => s.clone(),
            genomehubs_query::query::attributes::AttributeValue::List(list) => {
                list.first().cloned().unwrap_or_default()
            }
        });
        let values_list: Option<Vec<String>> = attr.value.as_ref().and_then(|v| match v {
            genomehubs_query::query::attributes::AttributeValue::List(list) => Some(list.clone()),
            _ => None,
        });

        let filter_clause: Value = match op {
            AttributeOperator::Exists => {
                json!({
                    "nested": {
                        "path": "attributes",
                        "query": {
                            "bool": {
                                "filter": [
                                    {"match": {"attributes.key": name}},
                                    {"bool": {
                                        "should": [
                                            {"exists": {"field": "attributes.long_value"}},
                                            {"exists": {"field": "attributes.keyword_value"}}
                                        ],
                                        "minimum_should_match": 1
                                    }}
                                ]
                            }
                        }
                    }
                })
            }
            AttributeOperator::Missing => {
                json!({
                    "bool": {
                        "must_not": [{
                            "nested": {
                                "path": "attributes",
                                "query": {"bool": {"filter": [{"match": {"attributes.key": name}}]}}
                            }
                        }]
                    }
                })
            }
            AttributeOperator::Eq => {
                if let Some(list) = values_list {
                    // Set membership
                    json!({
                        "nested": {
                            "path": "attributes",
                            "query": {
                                "bool": {
                                    "filter": [
                                        {"match": {"attributes.key": name}},
                                        {"terms": {"attributes.keyword_value": list}}
                                    ]
                                }
                            }
                        }
                    })
                } else if let Some(v) = value_str {
                    let is_numeric = v.parse::<f64>().is_ok();
                    let val_field = if is_numeric {
                        "attributes.long_value"
                    } else {
                        "attributes.keyword_value"
                    };
                    json!({
                        "nested": {
                            "path": "attributes",
                            "query": {
                                "bool": {
                                    "filter": [
                                        {"match": {"attributes.key": name}},
                                        {"term": {val_field: v}}
                                    ]
                                }
                            }
                        }
                    })
                } else {
                    continue;
                }
            }
            AttributeOperator::Ne => {
                if let Some(v) = value_str {
                    let is_numeric = v.parse::<f64>().is_ok();
                    let val_field = if is_numeric {
                        "attributes.long_value"
                    } else {
                        "attributes.keyword_value"
                    };
                    json!({
                        "nested": {
                            "path": "attributes",
                            "query": {
                                "bool": {
                                    "filter": [{"match": {"attributes.key": name}}],
                                    "must_not": [{"term": {val_field: v}}]
                                }
                            }
                        }
                    })
                } else {
                    continue;
                }
            }
            // Numeric range operators
            op @ (AttributeOperator::Lt
            | AttributeOperator::Le
            | AttributeOperator::Gt
            | AttributeOperator::Ge) => {
                if let Some(v) = value_str {
                    let range_key = match op {
                        AttributeOperator::Lt => "lt",
                        AttributeOperator::Le => "lte",
                        AttributeOperator::Gt => "gt",
                        AttributeOperator::Ge => "gte",
                        _ => unreachable!(),
                    };
                    json!({
                        "nested": {
                            "path": "attributes",
                            "query": {
                                "bool": {
                                    "filter": [
                                        {"match": {"attributes.key": name}},
                                        {"range": {"attributes.long_value": {range_key: v}}}
                                    ]
                                }
                            }
                        }
                    })
                } else {
                    continue;
                }
            }
        };

        filters.push(filter_clause);
    }

    filters
}

// ── Taxon filter resolution ───────────────────────────────────────────────────

/// Build an optional ES filter clause for the feature index based on `taxa`.
///
/// - `name` filter → resolves name to taxon_id via taxon index, produces
///   `terms: {taxon_id: [ids...]}`
/// - `tree` / `lineage` filter → same resolution, but filters on `ancestors`
///   (features where the resolved taxon_id is an ancestor) OR `taxon_id` equals
///   that id (to include the taxon's own features).
/// - No taxa specified → no filter (match all features).
async fn build_feature_taxon_filter(
    query: &SearchQuery,
    client: &reqwest::Client,
    es_base: &str,
    taxon_idx: &str,
) -> Result<Option<Value>, String> {
    let taxa = match &query.identifiers.taxa {
        Some(t) if !t.names.is_empty() => t,
        _ => return Ok(None),
    };

    let resolved_ids = resolve_taxon_ids(client, es_base, taxon_idx, &taxa.names).await?;
    if resolved_ids.is_empty() {
        return Err(format!(
            "could not resolve taxon(s) to IDs: {}",
            taxa.names.join(", ")
        ));
    }

    let filter = match taxa.filter_type {
        TaxonFilterType::Name => {
            // Exact match on taxon_id field
            json!({"terms": {"taxon_id": resolved_ids}})
        }
        TaxonFilterType::Tree | TaxonFilterType::Lineage => {
            // Features belonging to any descendant (ancestors contains the id)
            // OR the taxon itself (taxon_id == id)
            json!({
                "bool": {
                    "should": [
                        {"terms": {"taxon_id": resolved_ids}},
                        {"terms": {"ancestors": resolved_ids}}
                    ],
                    "minimum_should_match": 1
                }
            })
        }
    };

    Ok(Some(filter))
}

/// Resolve taxon names (or raw numeric IDs) to ES `taxon_id` strings.
///
/// Queries the taxon index by `scientific_name` or `taxon_id`. Returns the
/// unique set of matched `taxon_id` values.
async fn resolve_taxon_ids(
    client: &reqwest::Client,
    es_base: &str,
    taxon_idx: &str,
    names: &[String],
) -> Result<Vec<String>, String> {
    let query = json!({
        "query": {
            "bool": {
                "should": names.iter().map(|n| {
                    json!({
                        "bool": {
                            "should": [
                                {"term": {"taxon_id": n}},
                                {"match": {"scientific_name": n}},
                                {
                                    "nested": {
                                        "path": "taxon_names",
                                        "query": {
                                            "match": {"taxon_names.name": n}
                                        }
                                    }
                                }
                            ],
                            "minimum_should_match": 1
                        }
                    })
                }).collect::<Vec<_>>(),
                "minimum_should_match": 1
            }
        },
        "_source": ["taxon_id"],
        "size": 50
    });

    let raw = execute_search(client, es_base, taxon_idx, &query).await?;

    let hits = raw
        .pointer("/hits/hits")
        .and_then(|h| h.as_array())
        .ok_or_else(|| "taxon lookup: missing hits".to_string())?;

    let ids: Vec<String> = hits
        .iter()
        .filter_map(|h| {
            h.get("_source")
                .and_then(|s| s.get("taxon_id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();

    // Deduplicate
    let mut seen = std::collections::HashSet::new();
    Ok(ids
        .into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect())
}

// ── Layout computation ────────────────────────────────────────────────────────
/// Per-assembly computed layout (sorted sequences with offsets).
struct AssemblyLayout {
    sequences: Vec<SequenceLayout>,
    total_span: u64,
}

fn build_assembly_layouts(
    assembly_ids: &[String],
    seq_records: &[SequenceRecord],
    feature_records: &[FeatureRecord],
    reorient: bool,
) -> HashMap<String, AssemblyLayout> {
    // Index seq_records by assembly_id
    let mut seqs_by_assembly: HashMap<&str, Vec<&SequenceRecord>> = HashMap::new();
    for s in seq_records {
        seqs_by_assembly
            .entry(s.assembly_id.as_str())
            .or_default()
            .push(s);
    }

    // Reference assembly (first in assembly_ids)
    let ref_id = assembly_ids[0].as_str();

    // Build initial SequenceLayout entries for reference, sorted by length desc
    let ref_seqs: Vec<SequenceLayout> = {
        let mut v: Vec<_> = seqs_by_assembly
            .get(ref_id)
            .map(|ss| {
                ss.iter()
                    .map(|s| SequenceLayout {
                        sequence_id: s.sequence_id.clone(),
                        length: s.length,
                        offset: 0,
                        orientation: 1,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        v.sort_by(|a, b| b.length.cmp(&a.length));
        v
    };

    // Build group → ref genome-wide position map using the (pre-offset) reference layout
    // First compute offsets for reference
    let mut ref_seqs_with_offsets = ref_seqs.clone();
    compute_offsets(&mut ref_seqs_with_offsets);

    let ref_offset_map: HashMap<String, u64> = ref_seqs_with_offsets
        .iter()
        .map(|s| (s.sequence_id.clone(), s.offset))
        .collect();

    // group_value → genome-wide position in reference
    let group_to_ref_pos: HashMap<String, u64> = feature_records
        .iter()
        .filter(|f| f.assembly_id == ref_id)
        .filter_map(|f| {
            ref_offset_map
                .get(&f.sequence_id)
                .map(|&off| (f.group_value.clone(), off + f.start))
        })
        .collect();

    let mut result = HashMap::new();

    // Reference assembly layout
    let ref_layout = ref_seqs_with_offsets.clone();
    let ref_span = ref_layout.last().map(|s| s.offset + s.length).unwrap_or(0);
    result.insert(
        ref_id.to_string(),
        AssemblyLayout {
            sequences: ref_layout,
            total_span: ref_span,
        },
    );

    // For each comparison assembly, sort and optionally orient sequences
    for cmp_id in &assembly_ids[1..] {
        let cmp_seq_records: Vec<&&SequenceRecord> = seqs_by_assembly
            .get(cmp_id.as_str())
            .map(|v| v.iter().collect())
            .unwrap_or_default();
        let cmp_seqs_init: Vec<SequenceLayout> = cmp_seq_records
            .iter()
            .map(|s| SequenceLayout {
                sequence_id: s.sequence_id.clone(),
                length: s.length,
                offset: 0,
                orientation: 1,
            })
            .collect();

        // seq → groups in that sequence
        let seq_to_groups: HashMap<String, Vec<String>> = {
            let mut m: HashMap<String, Vec<String>> = HashMap::new();
            for f in feature_records.iter().filter(|f| &f.assembly_id == cmp_id) {
                m.entry(f.sequence_id.clone())
                    .or_default()
                    .push(f.group_value.clone());
            }
            m
        };

        let mut sorted = order_sequences_by_median(
            &cmp_seqs_init,
            &ref_offset_map,
            &group_to_ref_pos,
            &seq_to_groups,
        );

        if reorient {
            // Compute orientation for each comparison sequence
            for seq in &mut sorted {
                let pairs: Vec<(f64, f64)> = feature_records
                    .iter()
                    .filter(|f| &f.assembly_id == cmp_id && f.sequence_id == seq.sequence_id)
                    .filter_map(|f| {
                        group_to_ref_pos
                            .get(&f.group_value)
                            .map(|&ref_pos| (ref_pos as f64, f.start as f64))
                    })
                    .collect();
                seq.orientation = orient_sequence(&pairs);
            }
        }

        compute_offsets(&mut sorted);
        let span = sorted.last().map(|s| s.offset + s.length).unwrap_or(0);
        result.insert(
            cmp_id.clone(),
            AssemblyLayout {
                sequences: sorted,
                total_span: span,
            },
        );
    }

    result
}

// ── Oxford / Ribbon report output ─────────────────────────────────────────────

fn build_oxford_ribbon_report(
    spec: &PositionalSpec,
    assembly_ids: &[String],
    layouts: &HashMap<String, AssemblyLayout>,
    features: &[FeatureRecord],
) -> Value {
    let report_type = match spec.report {
        PositionalReportType::Oxford => "oxford",
        PositionalReportType::Ribbon => "ribbon",
        _ => "oxford",
    };

    let ref_id = &assembly_ids[0];
    let ref_layout = layouts.get(ref_id.as_str());

    let assemblies_json = serialise_assembly_metadata(assembly_ids, layouts);
    let ref_offsets = offset_map(ref_layout);

    // Group all reference features by group_value — collect Vec to detect M:N.
    let mut group_to_ref_all: HashMap<&str, Vec<&FeatureRecord>> = HashMap::new();
    for f in features.iter().filter(|f| &f.assembly_id == ref_id) {
        group_to_ref_all.entry(&f.group_value).or_default().push(f);
    }

    let max_conn = spec.max_connections_per_group.unwrap_or(25);

    let mut all_points: Vec<Value> = Vec::new();
    let mut all_connections: Vec<Value> = Vec::new();
    let mut cat_counts: HashMap<String, u64> = HashMap::new();

    for cmp_id in &assembly_ids[1..] {
        let cmp_layout = layouts.get(cmp_id.as_str());
        let cmp_offsets = offset_map(cmp_layout);
        let cmp_orientations: HashMap<String, i8> = cmp_layout
            .map(|l| {
                l.sequences
                    .iter()
                    .map(|s| (s.sequence_id.clone(), s.orientation))
                    .collect()
            })
            .unwrap_or_default();

        // Group comparison features by group_value.
        let mut cmp_by_group: HashMap<&str, Vec<&FeatureRecord>> = HashMap::new();
        for f in features.iter().filter(|f| &f.assembly_id == cmp_id) {
            cmp_by_group.entry(&f.group_value).or_default().push(f);
        }

        for (group, cmp_feats) in &cmp_by_group {
            let ref_feats = match group_to_ref_all.get(group) {
                Some(v) => v,
                None => continue,
            };

            let cat = cmp_feats
                .first()
                .and_then(|f| f.cat_value.as_deref())
                .unwrap_or("");
            if !cat.is_empty() {
                *cat_counts.entry(cat.to_string()).or_insert(0) += 1;
            }

            let is_mn = ref_feats.len() > 1 || cmp_feats.len() > 1;

            if !is_mn {
                // 1:1 path — same output as before
                let rf = ref_feats[0];
                let cf = cmp_feats[0];

                let x = ref_offsets
                    .get(&rf.sequence_id)
                    .map(|&off| off + rf.start)
                    .unwrap_or(rf.start);
                let x2 = ref_offsets
                    .get(&rf.sequence_id)
                    .map(|&off| off + rf.end)
                    .unwrap_or(rf.end);

                let y_orient = cmp_orientations.get(&cf.sequence_id).copied().unwrap_or(1);
                let (y, y2) = genome_wide_y(cf, cmp_layout, &cmp_offsets, y_orient);

                let mut point = json!({
                    "featureId":  rf.feature_id,
                    "yFeatureId": cf.feature_id,
                    "x": x, "x2": x2,
                    "y": y, "y2": y2,
                    "group": cf.group_value,
                    "strand": rf.strand,
                    "yStrand": y_orient * cf.strand
                });
                if !cat.is_empty() {
                    point["cat"] = json!(cat);
                }
                if assembly_ids.len() > 2 {
                    point["assemblyPair"] = json!([ref_id, cmp_id]);
                }
                all_points.push(point);
            } else {
                // M:N path — emit a connection record
                let mut x_coords: Vec<u64> = Vec::new();
                let mut x2_coords: Vec<u64> = Vec::new();
                let mut x_seq_ids: Vec<&str> = Vec::new();
                let mut x_strands: Vec<i8> = Vec::new();
                let mut y_coords: Vec<u64> = Vec::new();
                let mut y2_coords: Vec<u64> = Vec::new();
                let mut y_seq_ids: Vec<&str> = Vec::new();
                let mut y_strands: Vec<i8> = Vec::new();

                for rf in ref_feats.iter() {
                    let x = ref_offsets
                        .get(&rf.sequence_id)
                        .map(|&off| off + rf.start)
                        .unwrap_or(rf.start);
                    let x2 = ref_offsets
                        .get(&rf.sequence_id)
                        .map(|&off| off + rf.end)
                        .unwrap_or(rf.end);
                    x_coords.push(x);
                    x2_coords.push(x2);
                    x_seq_ids.push(&rf.sequence_id);
                    x_strands.push(rf.strand);
                }

                for cf in cmp_feats.iter() {
                    let y_orient = cmp_orientations.get(&cf.sequence_id).copied().unwrap_or(1);
                    let (y, y2) = genome_wide_y(cf, cmp_layout, &cmp_offsets, y_orient);
                    y_coords.push(y);
                    y2_coords.push(y2);
                    y_seq_ids.push(&cf.sequence_id);
                    y_strands.push(y_orient * cf.strand);
                }

                // Cap total connections (|x| × |y|)
                let total = x_coords.len() * y_coords.len();
                let truncated = total > max_conn;

                let mut conn = json!({
                    "group":     group,
                    "xCoords":   x_coords,
                    "x2Coords":  x2_coords,
                    "xSeqIds":   x_seq_ids,
                    "xStrands":  x_strands,
                    "yCoords":   y_coords,
                    "y2Coords":  y2_coords,
                    "ySeqIds":   y_seq_ids,
                    "yStrands":  y_strands,
                    "truncated": truncated
                });
                if !cat.is_empty() {
                    conn["catValue"] = json!(cat);
                }
                if assembly_ids.len() > 2 {
                    conn["assemblyPair"] = json!([ref_id, cmp_id]);
                }
                all_connections.push(conn);
            }
        }
    }

    // Build cats array sorted by count
    let mut cats_vec: Vec<_> = cat_counts.into_iter().collect();
    cats_vec.sort_by(|a, b| b.1.cmp(&a.1));
    let cats_json: Vec<Value> = cats_vec
        .iter()
        .map(|(k, _)| json!({"key": k, "label": k}))
        .collect();

    // Build 2D per-sequence histogram for oxford (2-assembly) reports.
    let (histograms, z_domain_max) = if assembly_ids.len() == 2 {
        let cmp_id = &assembly_ids[1];
        let cmp_layout = layouts.get(cmp_id.as_str());

        let ref_seq_order: Vec<String> = ref_layout
            .map(|l| l.sequences.iter().map(|s| s.sequence_id.clone()).collect())
            .unwrap_or_default();
        let cmp_seq_order: Vec<String> = cmp_layout
            .map(|l| l.sequences.iter().map(|s| s.sequence_id.clone()).collect())
            .unwrap_or_default();

        let ref_idx_map: HashMap<&str, usize> = ref_seq_order
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();
        let cmp_idx_map: HashMap<&str, usize> = cmp_seq_order
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();

        let n_x = ref_seq_order.len();
        let n_y = cmp_seq_order.len();
        let mut matrix: Vec<Vec<u32>> = vec![vec![0u32; n_y]; n_x];

        let cmp_id = &assembly_ids[1];
        for cmp_feat in features.iter().filter(|f| &f.assembly_id == cmp_id) {
            if let Some(ref_feats) = group_to_ref_all.get(cmp_feat.group_value.as_str()) {
                for rf in ref_feats {
                    if let (Some(&xi), Some(&yi)) = (
                        ref_idx_map.get(rf.sequence_id.as_str()),
                        cmp_idx_map.get(cmp_feat.sequence_id.as_str()),
                    ) {
                        matrix[xi][yi] += 1;
                    }
                }
            }
        }

        let all_values: Vec<u32> = matrix.iter().map(|row| row.iter().sum()).collect();
        let z_max = all_values.iter().copied().max().unwrap_or(0) as u64;

        let x_buckets: Vec<u64> = ref_layout
            .map(|l| l.sequences.iter().map(|s| s.offset).collect())
            .unwrap_or_default();
        let y_buckets: Vec<u64> = cmp_layout
            .map(|l| l.sequences.iter().map(|s| s.offset).collect())
            .unwrap_or_default();

        let hist = json!({
            "xBuckets": x_buckets,
            "yBuckets": y_buckets,
            "values": matrix,
            "allValues": all_values,
            "xLabels": ref_seq_order,
            "yLabels": cmp_seq_order
        });
        (hist, z_max)
    } else {
        (Value::Null, all_points.len() as u64)
    };

    // Apply windowing if requested
    if let Some(ws) = spec.window_size {
        let raw_points: Vec<RawPoint> = features
            .iter()
            .filter(|f| f.assembly_id == *ref_id)
            .map(|f| RawPoint {
                sequence_id: f.sequence_id.clone(),
                start: f.start,
                cat_value: f.cat_value.clone(),
            })
            .collect();
        let windowed = apply_window(&raw_points, ws);
        let windowed_json: Vec<Value> = windowed
            .iter()
            .map(|w| {
                json!({
                    "sequenceId": w.sequence_id,
                    "start": w.window_start,
                    "end": w.window_end,
                    "count": w.count,
                    "cats": w.cats
                })
            })
            .collect();

        json!({
            "type": report_type,
            "assemblies": assemblies_json,
            "points": Value::Null,
            "connections": all_connections,
            "windowedPoints": windowed_json,
            "histograms": histograms,
            "cat": spec.cat,
            "cats": cats_json,
            "zDomain": [0, z_domain_max]
        })
    } else {
        json!({
            "type": report_type,
            "assemblies": assemblies_json,
            "points": all_points,
            "connections": all_connections,
            "windowedPoints": Value::Null,
            "histograms": histograms,
            "cat": spec.cat,
            "cats": cats_json,
            "zDomain": [0, z_domain_max]
        })
    }
}

/// Compute genome-wide (y, y2) coordinates for a comparison feature,
/// applying orientation flip when the sequence is reversed.
fn genome_wide_y(
    feat: &FeatureRecord,
    cmp_layout: Option<&AssemblyLayout>,
    cmp_offsets: &HashMap<String, u64>,
    y_orient: i8,
) -> (u64, u64) {
    if let Some(&off) = cmp_offsets.get(&feat.sequence_id) {
        let seq_len = cmp_layout
            .and_then(|l| {
                l.sequences
                    .iter()
                    .find(|s| s.sequence_id == feat.sequence_id)
            })
            .map(|s| s.length)
            .unwrap_or(0);
        if y_orient == -1 {
            let flipped_start = seq_len.saturating_sub(feat.end);
            let flipped_end = seq_len.saturating_sub(feat.start);
            (off + flipped_start, off + flipped_end)
        } else {
            (off + feat.start, off + feat.end)
        }
    } else {
        (feat.start, feat.end)
    }
}

// ── Painting report output ────────────────────────────────────────────────────

fn build_painting_report(
    spec: &PositionalSpec,
    layouts: &HashMap<String, AssemblyLayout>,
    features: &[FeatureRecord],
) -> Value {
    let assembly_id = &spec.assemblies[0];
    let assemblies_json = serialise_assembly_metadata(&spec.assemblies, layouts);

    let painting = if let Some(ws) = spec.window_size {
        let raw_points: Vec<RawPoint> = features
            .iter()
            .filter(|f| &f.assembly_id == assembly_id)
            .map(|f| RawPoint {
                sequence_id: f.sequence_id.clone(),
                start: f.start,
                cat_value: f.cat_value.clone(),
            })
            .collect();
        let windowed = apply_window(&raw_points, ws);
        build_painting_segments(&windowed, spec.cat.as_deref())
    } else {
        let raw_points: Vec<RawPoint> = features
            .iter()
            .filter(|f| &f.assembly_id == assembly_id)
            .map(|f| RawPoint {
                sequence_id: f.sequence_id.clone(),
                start: f.start,
                cat_value: f.cat_value.clone(),
            })
            .collect();
        build_painting_segments_raw(&raw_points, spec.cat.as_deref())
    };

    json!({
        "type": "painting",
        "assemblies": assemblies_json,
        "segments": painting["segments"],
        "cat": spec.cat
    })
}

// ── Circos report output ──────────────────────────────────────────────────────

/// Build a circos arc diagram response.
///
/// All assemblies are arranged on a single circle (angle 0–360°).  Sequences
/// are sorted by length (longest first) within each assembly, then assemblies
/// are laid out in the order they appear in `assembly_ids`.  A small gap
/// (1% of total genome span, capped at 10 Mbp) separates each assembly.
///
/// Arcs connect feature positions; M:N groups produce one arc per (x, y) pair
/// up to `max_connections_per_group`.
fn build_circos_report(
    spec: &PositionalSpec,
    assembly_ids: &[String],
    layouts: &HashMap<String, AssemblyLayout>,
    features: &[FeatureRecord],
) -> Value {
    let assemblies_json = serialise_assembly_metadata(assembly_ids, layouts);
    let max_conn = spec.max_connections_per_group.unwrap_or(25);

    // Compute total genome span across all assemblies (used for angle normalisation).
    let total_span: u64 = assembly_ids
        .iter()
        .filter_map(|id| layouts.get(id.as_str()))
        .map(|l| l.total_span)
        .sum();

    if total_span == 0 {
        return json!({
            "type": "circos",
            "assemblies": assemblies_json,
            "sequences": [],
            "arcs": []
        });
    }

    // Gap between assemblies: 1% of total span, capped at 10 Mbp.
    let gap_bp: u64 = (total_span / 100).min(10_000_000);
    let total_with_gaps = total_span + gap_bp * (assembly_ids.len() as u64).saturating_sub(1);
    let scale = 360.0_f64 / total_with_gaps as f64;

    // Build circos sequence entries with angle ranges.
    let mut global_offset: u64 = 0;
    let mut seq_angle_start: HashMap<(String, String), f64> = HashMap::new(); // (assembly, seq) → angle_start
    let mut seq_angle_scale: HashMap<(String, String), f64> = HashMap::new(); // (assembly, seq) → bp_per_degree⁻¹

    let mut sequences_json: Vec<Value> = Vec::new();

    for (asm_idx, asm_id) in assembly_ids.iter().enumerate() {
        let layout = match layouts.get(asm_id.as_str()) {
            Some(l) => l,
            None => continue,
        };
        for seq in &layout.sequences {
            let angle_start = (global_offset + seq.offset) as f64 * scale;
            let angle_end = angle_start + seq.length as f64 * scale;
            let key = (asm_id.clone(), seq.sequence_id.clone());
            seq_angle_start.insert(key.clone(), angle_start);
            seq_angle_scale.insert(key, scale);

            sequences_json.push(json!({
                "sequenceId":  seq.sequence_id,
                "assemblyId":  asm_id,
                "length":      seq.length,
                "offset":      seq.offset,
                "angleStart":  (angle_start * 100.0).round() / 100.0,
                "angleEnd":    (angle_end   * 100.0).round() / 100.0
            }));
        }
        global_offset += layout.total_span;
        if asm_idx + 1 < assembly_ids.len() {
            global_offset += gap_bp;
        }
    }

    // Group all features by group_value for arc building.
    let mut group_by_value: HashMap<&str, Vec<&FeatureRecord>> = HashMap::new();
    for f in features {
        group_by_value.entry(&f.group_value).or_default().push(f);
    }

    /// Convert a genome-wide bp position (offset + start) to an angle.
    fn pos_to_angle(
        assembly_id: &str,
        sequence_id: &str,
        pos: u64,
        seq_angle_start: &HashMap<(String, String), f64>,
        seq_angle_scale: &HashMap<(String, String), f64>,
        layout: &HashMap<String, AssemblyLayout>,
    ) -> f64 {
        let offset = layout
            .get(assembly_id)
            .and_then(|l| l.sequences.iter().find(|s| s.sequence_id == sequence_id))
            .map(|s| s.offset)
            .unwrap_or(0);
        let local_pos = pos; // pos is already local (start/end from FeatureRecord)
        let key = (assembly_id.to_string(), sequence_id.to_string());
        let base = *seq_angle_start.get(&key).unwrap_or(&0.0);
        let sc = *seq_angle_scale.get(&key).unwrap_or(&0.0);
        let _ = offset; // offset already baked into seq_angle_start via global_offset
        (base + local_pos as f64 * sc * 100.0).round() / 100.0
    }

    let mut arcs_json: Vec<Value> = Vec::new();

    for (group, feats) in &group_by_value {
        // Group by assembly
        let mut by_asm: HashMap<&str, Vec<&FeatureRecord>> = HashMap::new();
        for f in feats {
            by_asm.entry(&f.assembly_id).or_default().push(f);
        }

        // Emit one arc per unique (from_feat, to_feat) pair across assembly pairs,
        // plus within-assembly pairs (same assembly, different features).
        let all_feats: Vec<&FeatureRecord> = feats.to_vec();
        let total = all_feats.len();
        let mut arc_count = 0;

        'outer: for i in 0..total {
            for j in (i + 1)..total {
                if arc_count >= max_conn {
                    break 'outer;
                }
                let from = all_feats[i];
                let to = all_feats[j];

                let from_angle = pos_to_angle(
                    &from.assembly_id,
                    &from.sequence_id,
                    from.start,
                    &seq_angle_start,
                    &seq_angle_scale,
                    layouts,
                );
                let to_angle = pos_to_angle(
                    &to.assembly_id,
                    &to.sequence_id,
                    to.start,
                    &seq_angle_start,
                    &seq_angle_scale,
                    layouts,
                );
                let cat = from
                    .cat_value
                    .as_deref()
                    .or(to.cat_value.as_deref())
                    .unwrap_or("");

                let mut arc = json!({
                    "group": group,
                    "from": {
                        "sequenceId": from.sequence_id,
                        "assemblyId": from.assembly_id,
                        "pos":        from.start,
                        "angle":      from_angle
                    },
                    "to": {
                        "sequenceId": to.sequence_id,
                        "assemblyId": to.assembly_id,
                        "pos":        to.start,
                        "angle":      to_angle
                    },
                    "weight": 1
                });
                if !cat.is_empty() {
                    arc["catValue"] = json!(cat);
                }
                arcs_json.push(arc);
                arc_count += 1;
            }
        }
    }

    json!({
        "type":       "circos",
        "assemblies": assemblies_json,
        "sequences":  sequences_json,
        "arcs":       arcs_json
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn offset_map(layout: Option<&AssemblyLayout>) -> HashMap<String, u64> {
    layout
        .map(|l| {
            l.sequences
                .iter()
                .map(|s| (s.sequence_id.clone(), s.offset))
                .collect()
        })
        .unwrap_or_default()
}

fn serialise_assembly_metadata(
    assembly_ids: &[String],
    layouts: &HashMap<String, AssemblyLayout>,
) -> Value {
    let mut map = serde_json::Map::new();
    for id in assembly_ids {
        let layout = match layouts.get(id.as_str()) {
            Some(l) => l,
            None => continue,
        };
        let sequences: Vec<Value> = layout
            .sequences
            .iter()
            .map(|s| {
                json!({
                    "id": s.sequence_id,
                    "length": s.length,
                    "offset": s.offset,
                    "orientation": s.orientation
                })
            })
            .collect();

        let buckets: Vec<u64> = layout.sequences.iter().map(|s| s.offset).collect();

        map.insert(
            id.clone(),
            json!({
                "sequences": sequences,
                "domain": [0, layout.total_span],
                "buckets": buckets
            }),
        );
    }
    Value::Object(map)
}
