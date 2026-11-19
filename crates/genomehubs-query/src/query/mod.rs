//! Query builder for genomehubs search APIs.
//!
//! Provides a serde-serialisable [`SearchQuery`] / [`QueryParams`] pair and a
//! pure [`build_query_url`] function that converts them into a fully-encoded API URL.
//!
//! This crate has no I/O dependencies and compiles to WebAssembly.

pub mod attributes;
pub mod chain;
pub mod identifiers;
pub mod url;

pub use attributes::{Attribute, AttributeOperator, AttributeSet, AttributeValue, Field, Modifier};
pub use chain::{ChainError, ChainRef, NamedQuerySpec};
pub use identifiers::{Identifiers, TaxaIdentifier, TaxonFilterType};
pub use url::{build_query_url, build_ui_url};

use serde::{Deserialize, Serialize};

// ── SearchQuery ───────────────────────────────────────────────────────────────

/// Top-level query describing *what* to search for.
///
/// Combines the `process_identifiers` and `process_attributes` artifacts from
/// the GoaT MCP server into a single serde-serialisable struct.
///
/// Supports both single queries and multi-query OR/AND combinations.
///
/// Load from YAML with [`SearchQuery::from_yaml`]; build a URL with
/// [`build_query_url`].
///
/// # Single Query Example
/// ```yaml
/// index: taxon
/// taxa: [Mammalia, "!Felis"]
/// rank: species
/// taxon_filter_type: tree
/// attributes:
///   - name: genome_size
///     operator: lt
///     value: "3G"
///     modifier: [min, direct]
/// fields:
///   - name: genome_size
///     modifier: [min]
/// names: [scientific_name]
/// ranks: [genus]
/// ```
///
/// # Multi-Query OR Example
/// ```yaml
/// combine_with: OR
/// queries:
///   - index: taxon
///     taxa: [Mammalia]
///   - index: taxon
///     taxa: [Aves]
/// ```
/// Specification for a single lineage-rank aggregation requested alongside
/// the main search results.
///
/// `rank` names a taxonomic rank (e.g. `"genus"`, `"family"`, `"order"`).
/// `fields` names one or more attributes to aggregate per ancestor bucket.
/// Multiple fields at the same rank share one outer nested-lineage agg pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageRankSummarySpec {
    /// Taxonomic rank to group by (e.g. `"genus"`, `"family"`).
    pub rank: String,
    /// Attribute fields whose value distributions to aggregate per ancestor.
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SearchQuery {
    /// Which index to search (for single-query mode; ignored in multi-query).
    #[serde(default)]
    pub index: SearchIndex,
    /// Taxon, assembly, and sample identifiers with rank and filter type.
    #[serde(flatten, default)]
    pub identifiers: Identifiers,
    /// Attribute filters, return fields, name classes, and rank columns.
    #[serde(flatten, default)]
    pub attributes: AttributeSet,
    /// Multiple queries to combine (enables multi-query mode).
    #[serde(default)]
    pub queries: Option<Vec<SearchQuery>>,
    /// How to combine multiple queries: AND or OR (default: AND).
    #[serde(default)]
    pub combine_with: CombineStrategy,
    /// Named sub-queries for chain substitution.
    ///
    /// Values in `attributes` may reference these using dot notation:
    /// `value: queryA.field` or `value: queryA.summary(field)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub named_queries: Option<std::collections::HashMap<String, chain::NamedQuerySpec>>,
    /// Per-rank ancestor aggregations to compute alongside search results.
    ///
    /// Produces a `lineage_summary` map in the response envelope, keyed by
    /// `rank → ancestor_taxon_id → field → distribution`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage_rank_summary: Option<Vec<LineageRankSummarySpec>>,
}

/// Strategy for combining multiple queries.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum CombineStrategy {
    /// Combine with boolean AND (default).
    #[default]
    AND,
    /// Combine with boolean OR.
    OR,
}

impl SearchQuery {
    /// Parse a [`SearchQuery`] from a YAML string.
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    /// Serialise this [`SearchQuery`] to a YAML string.
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(self)
    }

    /// Check if this query is in multi-query mode (has nested queries).
    pub fn is_multi_query(&self) -> bool {
        self.queries.is_some()
    }

    /// Check if this is a single query mode (no nested queries).
    pub fn is_single_query(&self) -> bool {
        !self.is_multi_query()
    }
}

/// Which API search index to query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SearchIndex {
    #[default]
    Taxon,
    Assembly,
    Sample,
    /// Feature index — positional features (BUSCO genes, scaffolds, etc.).
    /// Used by the `/api/v3/positional` endpoint.
    Feature,
}

impl SearchIndex {
    /// Index name as used in API metadata requests, e.g. `"taxon"`.
    pub fn as_str(&self) -> &'static str {
        match self {
            SearchIndex::Taxon => "taxon",
            SearchIndex::Assembly => "assembly",
            SearchIndex::Sample => "sample",
            SearchIndex::Feature => "feature",
        }
    }
}

// ── SortEntry ───────────────────────────────────────────────────────────────

/// Sort configuration entry for search results.
/// entries have field and order keys, e.g. `{"by": "genome_size", "order": "desc"}` or `{"by": "taxon_id", "order": "asc"}`.
/// Corresponds to the `--sort` CLI flag.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SortConfig {
    /// Field to sort results by.
    pub by: String,
    /// Sort direction (default ascending).
    pub order: Option<SortOrder>,
}

// ── QueryParams ───────────────────────────────────────────────────────────────

/// Execution parameters describing *how* to fetch and present results.
///
/// Separate from [`SearchQuery`] so the same query can be issued as
/// `count` / `search` / `report` with different pagination and formatting.
/// Corresponds to the `submit_query` parameters in the GoaT MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryParams {
    /// Maximum records per page; maps to `&size=` (default 10).
    #[serde(default = "default_size")]
    pub size: usize,
    /// 1-based page number; `offset = (page - 1) * size`.
    #[serde(default = "default_page")]
    pub page: usize,
    /// Field to sort results by.
    #[serde(default)]
    pub sort_by: Option<String>,
    /// Sort direction (default ascending).
    #[serde(default)]
    pub sort_order: SortOrder,
    /// Sort: list of field and order pairs for sorting results, e.g. `[{"by": "genome_size", "order": "desc"}, {"by": "taxon_id", "order": "asc"}]`.
    /// Note: `sort_by` and `sort_order` are ignored if `sort` is present.
    /// Corresponds to `--sort` CLI flag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<Vec<SortConfig>>,
    /// Include ancestrally derived estimated values (`&includeEstimates=true`).
    ///
    /// Defaults to `false` to match the API default and MCP server behaviour.
    /// Corresponds to `--include-estimates` CLI flag.
    #[serde(default = "default_false")]
    pub include_estimates: bool,
    /// Request tidy (long) format via `&summaryValues=false`.
    ///
    /// Prefers the API's native tidy format over any client-side pivot
    /// (gap-analysis item 11).
    #[serde(default)]
    pub tidy: bool,
    /// Taxonomy backbone; defaults to `"ncbi"`.
    ///
    /// Site-level override is held in `SiteConfig`; only surfaced as a
    /// user-facing flag when a site uses a different taxonomy backbone.
    #[serde(default = "default_taxonomy")]
    pub taxonomy: String,
    /// Cursor for `/searchPaginated` continuation.
    ///
    /// Passed as `&searchAfter=<json-array>` on every page after the first.
    /// Set from the `pagination.searchAfter` value returned by the previous
    /// page response.  `None` means "start from the first page".
    #[serde(default)]
    pub search_after: Option<Vec<serde_json::Value>>,
    /// Include full lineage array in each result (default false — heavyweight).
    #[serde(default)]
    pub include_lineage: bool,
    /// Include taxon_names array in each result (default false — heavyweight).
    #[serde(default)]
    pub include_taxon_names: bool,
    /// Which lineage summary mode to return: background (default) computes
    /// per-ancestor distributions across all descendant taxa; matched returns
    /// distributions restricted to the matched query results.
    #[serde(default)]
    pub lineage_summary_mode: LineageSummaryMode,
    /// Filter results to exactly this set of IDs.
    ///
    /// Injected as an ES `terms` clause ANDed with the main query.
    /// Maximum 65,536 entries (ES hard limit for `terms` filters).
    /// Which field is used depends on `id_type` (defaults to current index).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_set: Option<Vec<String>>,
    /// Which ID field to filter on when `id_set` is provided.
    ///
    /// One of `"taxon"`, `"assembly"`, `"sample"`, `"feature"`.
    /// Defaults to the current index type if not specified.
    /// - `taxon` index → `taxon_id` field
    /// - `assembly` index → `assembly_id` field (or `taxon_id` if id_type=taxon)
    /// - `sample` index → `sample_id` field (or `taxon_id`, `assembly_id` if specified)
    /// - `feature` index → `feature_id` field (or `taxon_id`, `assembly_id` if specified)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_type: Option<String>,
}

impl Default for QueryParams {
    fn default() -> Self {
        Self {
            size: default_size(),
            page: default_page(),
            sort_by: None,
            sort_order: SortOrder::default(),
            include_estimates: false,
            tidy: false,
            taxonomy: default_taxonomy(),
            search_after: None,
            include_lineage: false,
            include_taxon_names: false,
            lineage_summary_mode: LineageSummaryMode::default(),
            id_set: None,
            id_type: None,
            sort: None,
        }
    }
}

/// Mode for lineage summary computation.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LineageSummaryMode {
    #[default]
    Background,
    Matched,
}

/// Sort direction for search results.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

impl QueryParams {
    /// Parse a [`QueryParams`] from a YAML string.
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    /// Validate that `id_set` is within acceptable bounds.
    ///
    /// Returns `Err` if the set exceeds the ES hard limit of 65,536.
    /// This is the hard limit for ES `terms` filter clauses.
    pub fn validate_id_set(&self) -> Result<(), String> {
        const ES_TERMS_LIMIT: usize = 65_536;

        if let Some(ids) = &self.id_set {
            if ids.len() > ES_TERMS_LIMIT {
                return Err(format!(
                    "id_set contains {} IDs, which exceeds the ES terms clause limit of {}",
                    ids.len(),
                    ES_TERMS_LIMIT
                ));
            }
        }
        Ok(())
    }

    /// Resolve the ID field name based on the current index and id_type.
    ///
    /// If `id_set` is not present, returns `None`.
    /// Otherwise, returns the ES field name to filter on:
    /// - If `id_type` is specified, uses the field for that type
    /// - Otherwise, defaults to the index type (taxon → taxon_id, assembly → assembly_id, etc.)
    pub fn resolve_id_field(&self, index: &str) -> Option<String> {
        self.id_set.as_ref().map(|_| {
            let id_type = self.id_type.as_deref().unwrap_or(match index {
                "assembly" => "assembly",
                "sample" => "sample",
                "feature" => "feature",
                _ => "taxon",
            });

            match id_type {
                "assembly" => "assembly_id".to_string(),
                "sample" => "sample_id".to_string(),
                "feature" => "feature_id".to_string(),
                _ => "taxon_id".to_string(),
            }
        })
    }
}

fn default_size() -> usize {
    10
}
fn default_page() -> usize {
    1
}
fn default_false() -> bool {
    false
}
fn default_taxonomy() -> String {
    "ncbi".to_string()
}

// ── URL parsing ───────────────────────────────────────────────────────────────

/// Parse a URL query string into a multi-map of decoded `key → Vec<value>`.
///
/// Handles both full URLs (`https://…?key=val`) and bare query strings.
/// `+` signs are treated as spaces; all values are percent-decoded.
pub(crate) fn parse_url_query_string(url: &str) -> std::collections::HashMap<String, Vec<String>> {
    let qs = if let Some(pos) = url.find('?') {
        &url[pos + 1..]
    } else {
        url
    };
    let qs = qs.split('#').next().unwrap_or(qs);

    let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for part in qs.split('&') {
        if part.is_empty() {
            continue;
        }
        let (raw_key, raw_val) = match part.find('=') {
            Some(pos) => (&part[..pos], &part[pos + 1..]),
            None => (part, ""),
        };
        let key = decode_url_component(raw_key);
        let val = decode_url_component(raw_val);
        map.entry(key).or_default().push(val);
    }
    map
}

/// Percent-decode a URL component and replace `+` with space.
fn decode_url_component(s: &str) -> String {
    let replaced = s.replace('+', " ");
    percent_encoding::percent_decode_str(&replaced)
        .decode_utf8_lossy()
        .into_owned()
}

/// Return `true` if the URL refers to a report endpoint.
///
/// Checks whether the path ends with `/report` or the query string contains
/// a `report=` parameter.
pub fn url_is_report(url: &str) -> bool {
    let path = url.split('?').next().unwrap_or(url);
    let path = path.split('#').next().unwrap_or(path);
    if path.ends_with("/report") || path.ends_with("/report/") {
        return true;
    }
    parse_url_query_string(url).contains_key("report")
}

/// Top-level keys that belong to [`QueryParams`]; every other key belongs to [`SearchQuery`].
pub const QUERY_PARAMS_KEYS: &[&str] = &[
    "size",
    "page",
    "sort_by",
    "sort_order",
    "sort",
    "include_estimates",
    "tidy",
    "taxonomy",
    "search_after",
    "include_lineage",
    "include_taxon_names",
    "lineage_summary_mode",
    "id_set",
    "id_type",
];

/// Separate query and params YAML documents, as sent to `/v3/search` and `/v3/count`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitQueryDocument {
    /// YAML for [`SearchQuery`].
    pub query_yaml: String,
    /// YAML for [`QueryParams`].
    pub params_yaml: String,
}

/// Split a combined YAML document into query and params documents.
///
/// The document is either flat, with top-level keys in [`QUERY_PARAMS_KEYS`] moving to the
/// params document, or nested under `query:` and `params:` sections. An optional
/// `explicit_params` document follows the same two forms and may only hold params keys; a key
/// present in both documents is an error.
pub fn split_query_document(
    document: &str,
    explicit_params: Option<&str>,
) -> Result<SplitQueryDocument, String> {
    let (query_map, mut params_map) = split_sections(parse_yaml_mapping(document)?)?;
    if let Some(explicit) = explicit_params {
        let (explicit_query, explicit_map) = split_sections(parse_yaml_mapping(explicit)?)?;
        if let Some(key) = explicit_query.keys().next() {
            return Err(format!(
                "params document contains a non-params key: {}",
                key_name(key)
            ));
        }
        for (key, value) in explicit_map {
            if params_map.contains_key(&key) {
                return Err(format!(
                    "params key `{}` is set in both the query and params documents",
                    key_name(&key)
                ));
            }
            params_map.insert(key, value);
        }
    }
    Ok(SplitQueryDocument {
        query_yaml: mapping_to_yaml(&query_map)?,
        params_yaml: mapping_to_yaml(&params_map)?,
    })
}

/// Parse a YAML document into a mapping; an empty document is an empty mapping.
fn parse_yaml_mapping(document: &str) -> Result<serde_yaml::Mapping, String> {
    match serde_yaml::from_str::<serde_yaml::Value>(document).map_err(|error| error.to_string())? {
        serde_yaml::Value::Mapping(mapping) => Ok(mapping),
        serde_yaml::Value::Null => Ok(serde_yaml::Mapping::new()),
        _ => Err("YAML document must be a mapping of keys to values".to_string()),
    }
}

/// Split a mapping into `(non-params, params)` by top-level key.
fn partition_params_keys(
    mapping: serde_yaml::Mapping,
) -> (serde_yaml::Mapping, serde_yaml::Mapping) {
    let (params, others): (Vec<_>, Vec<_>) = mapping.into_iter().partition(|(key, _)| {
        key.as_str()
            .is_some_and(|name| QUERY_PARAMS_KEYS.contains(&name))
    });
    (others.into_iter().collect(), params.into_iter().collect())
}

/// Split a mapping into `(query, params)`, flat or nested under `query:` and `params:`.
fn split_sections(
    mapping: serde_yaml::Mapping,
) -> Result<(serde_yaml::Mapping, serde_yaml::Mapping), String> {
    if !mapping.keys().any(is_section_key) {
        return Ok(partition_params_keys(mapping));
    }
    if let Some(key) = mapping.keys().find(|key| !is_section_key(key)) {
        return Err(format!(
            "nested document mixes `query`/`params` with key: {}",
            key_name(key)
        ));
    }
    let (query, query_params) = partition_params_keys(section_mapping(&mapping, "query")?);
    if let Some(key) = query_params.keys().next() {
        return Err(format!(
            "query section contains a params key: {}",
            key_name(key)
        ));
    }
    let (params_others, params) = partition_params_keys(section_mapping(&mapping, "params")?);
    if let Some(key) = params_others.keys().next() {
        return Err(format!(
            "params section contains a non-params key: {}",
            key_name(key)
        ));
    }
    Ok((query, params))
}

/// Whether a top-level key opens a nested `query:` or `params:` section.
fn is_section_key(key: &serde_yaml::Value) -> bool {
    key.as_str()
        .is_some_and(|name| matches!(name, "query" | "params"))
}

/// Read a nested section as a mapping; an empty section is an empty mapping.
fn section_mapping(
    mapping: &serde_yaml::Mapping,
    name: &str,
) -> Result<serde_yaml::Mapping, String> {
    match mapping.get(name) {
        None | Some(serde_yaml::Value::Null) => Ok(serde_yaml::Mapping::new()),
        Some(serde_yaml::Value::Mapping(section)) => Ok(section.clone()),
        Some(_) => Err(format!("`{name}` section must be a mapping")),
    }
}

/// Top-level key name for error messages.
fn key_name(key: &serde_yaml::Value) -> String {
    key.as_str()
        .map_or_else(|| format!("{key:?}"), str::to_string)
}

/// Serialise a mapping as YAML text.
fn mapping_to_yaml(mapping: &serde_yaml::Mapping) -> Result<String, String> {
    serde_yaml::to_string(mapping).map_err(|error| error.to_string())
}

/// Parse a v2 API or UI URL into `(query_yaml, params_yaml)`.
///
/// Reconstructs a [`SearchQuery`] and [`QueryParams`] from the URL query
/// string.  Handles structured params (`tax_name=`, `fields=`, `result=`, …)
/// and the composite `query=` fragment form produced by the GoaT API.
///
/// Returns `Err` only when YAML serialisation fails (extremely unlikely).
pub fn query_yaml_from_url_params(url: &str) -> Result<(String, String), String> {
    let params = parse_url_query_string(url);

    let result_str = params
        .get("result")
        .and_then(|v| v.first())
        .map(|s| s.as_str())
        .unwrap_or("taxon");

    let mut query = SearchQuery {
        index: parse_result_index(result_str),
        ..Default::default()
    };
    let mut qparams = QueryParams::default();

    apply_taxa_params(&params, &mut query);

    if let Some(rank) = params.get("rank").and_then(|v| v.first()) {
        if !rank.is_empty() {
            query.identifiers.rank = Some(rank.clone());
        }
    }

    if let Some(fields_str) = params.get("fields").and_then(|v| v.first()) {
        for raw in fields_str.split(',') {
            let name = raw.trim().to_string();
            if !name.is_empty() {
                query.attributes.fields.push(attributes::Field {
                    name,
                    modifier: Vec::new(),
                });
            }
        }
    }

    if let Some(fragment) = params.get("query").and_then(|v| v.first()) {
        apply_query_fragment(fragment, &mut query);
    }

    if let Some(size_str) = params.get("size").and_then(|v| v.first()) {
        if let Ok(n) = size_str.parse::<usize>() {
            if n > 0 {
                qparams.size = n;
            }
        }
    }
    if let Some(offset_str) = params.get("offset").and_then(|v| v.first()) {
        if let Ok(offset) = offset_str.parse::<usize>() {
            qparams.page = offset / qparams.size + 1;
        }
    }
    if let Some(sort) = params.get("sortBy").and_then(|v| v.first()) {
        if !sort.is_empty() {
            qparams.sort_by = Some(sort.clone());
        }
    }
    if let Some(order) = params
        .get("sortOrder")
        .or_else(|| params.get("sortorder"))
        .and_then(|v| v.first())
    {
        qparams.sort_order = match order.to_lowercase().as_str() {
            "desc" => SortOrder::Desc,
            _ => SortOrder::Asc,
        };
    }
    if let Some(ie) = params.get("includeEstimates").and_then(|v| v.first()) {
        qparams.include_estimates = ie.to_lowercase() != "false" && ie != "0";
    }
    if let Some(tax) = params.get("taxonomy").and_then(|v| v.first()) {
        if !tax.is_empty() {
            qparams.taxonomy = tax.clone();
        }
    }

    let query_yaml = query.to_yaml().map_err(|e| e.to_string())?;
    let params_yaml = serde_yaml::to_string(&qparams).map_err(|e| e.to_string())?;
    Ok((query_yaml, params_yaml))
}

/// Parse the `result=` URL param into a [`SearchIndex`].
fn parse_result_index(result: &str) -> SearchIndex {
    match result {
        "assembly" => SearchIndex::Assembly,
        "sample" => SearchIndex::Sample,
        _ => SearchIndex::Taxon,
    }
}

/// Apply `tax_name=`, `tax_tree=`, and `tax_lineage=` params to a query.
///
/// Only the first matching key is applied; structured params take priority
/// over the `query=` fragment for taxon identity.
fn apply_taxa_params(
    params: &std::collections::HashMap<String, Vec<String>>,
    query: &mut SearchQuery,
) {
    for (key, filter_type) in [
        ("tax_name", identifiers::TaxonFilterType::Name),
        ("tax_tree", identifiers::TaxonFilterType::Tree),
        ("tax_lineage", identifiers::TaxonFilterType::Lineage),
    ] {
        if let Some(vals) = params.get(key) {
            let names: Vec<String> = vals
                .iter()
                .flat_map(|v| v.split(','))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if !names.is_empty() {
                query.identifiers.taxa = Some(identifiers::TaxaIdentifier { names, filter_type });
                return;
            }
        }
    }
}

/// Parse the `query=` fragment and merge taxa / attributes into `query`.
///
/// Only sets taxa from the fragment when the query does not already have taxa
/// from structured params (`tax_name=`, etc.).
fn apply_query_fragment(fragment: &str, query: &mut SearchQuery) {
    let mut taxa_names: Vec<String> = Vec::new();
    let mut taxa_filter_type = identifiers::TaxonFilterType::Name;
    let mut found_taxa = false;

    for part in split_on_and(fragment) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(inner) = extract_fn_arg(part, "tax_name") {
            taxa_names.extend(
                inner
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
            );
            taxa_filter_type = identifiers::TaxonFilterType::Name;
            found_taxa = true;
        } else if let Some(inner) = extract_fn_arg(part, "tax_tree") {
            taxa_names.extend(
                inner
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
            );
            taxa_filter_type = identifiers::TaxonFilterType::Tree;
            found_taxa = true;
        } else if let Some(inner) = extract_fn_arg(part, "tax_lineage") {
            taxa_names.extend(
                inner
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
            );
            taxa_filter_type = identifiers::TaxonFilterType::Lineage;
            found_taxa = true;
        } else if let Some(rank) = extract_fn_arg(part, "tax_rank") {
            let rank = rank.trim().to_string();
            if !rank.is_empty() && query.identifiers.rank.is_none() {
                query.identifiers.rank = Some(rank);
            }
        } else if let Some(ids_str) = part.strip_prefix("assembly_id=") {
            query.identifiers.assemblies.extend(
                ids_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
            );
        } else if let Some(ids_str) = part.strip_prefix("sample_id=") {
            query.identifiers.samples.extend(
                ids_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
            );
        } else if let Some(attr) = parse_attribute_fragment(part) {
            query.attributes.attributes.push(attr);
        }
    }

    if found_taxa && query.identifiers.taxa.is_none() && !taxa_names.is_empty() {
        query.identifiers.taxa = Some(identifiers::TaxaIdentifier {
            names: taxa_names,
            filter_type: taxa_filter_type,
        });
    }
}

/// Split a query fragment on ` AND ` (case-insensitive).
fn split_on_and(fragment: &str) -> Vec<&str> {
    let upper = fragment.to_uppercase();
    let sep = " AND ";
    let mut result: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i + sep.len() <= upper.len() {
        if upper[i..].starts_with(sep) {
            result.push(&fragment[start..i]);
            start = i + sep.len();
            i = start;
        } else {
            i += 1;
        }
    }
    result.push(&fragment[start..]);
    result
}

/// Extract the argument from `func_name(arg)`.  Returns `None` otherwise.
fn extract_fn_arg<'a>(s: &'a str, func_name: &str) -> Option<&'a str> {
    let prefix = format!("{func_name}(");
    if s.starts_with(prefix.as_str()) && s.ends_with(')') {
        Some(&s[prefix.len()..s.len() - 1])
    } else {
        None
    }
}

/// Parse a single attribute fragment such as `genome_size>=1000000000` or
/// `min(genome_size)>=1G` into an [`Attribute`].
///
/// Operators are tried longest-first to avoid prefix collisions.
/// A bare field name with no operator is treated as an `exists` test.
pub fn parse_attribute_fragment(s: &str) -> Option<attributes::Attribute> {
    const OPERATORS: &[(&str, attributes::AttributeOperator)] = &[
        (">=", attributes::AttributeOperator::Ge),
        ("<=", attributes::AttributeOperator::Le),
        ("!=", attributes::AttributeOperator::Ne),
        (">", attributes::AttributeOperator::Gt),
        ("<", attributes::AttributeOperator::Lt),
        ("=", attributes::AttributeOperator::Eq),
    ];

    for (op_str, op) in OPERATORS {
        if let Some(pos) = s.find(op_str) {
            let lhs = s[..pos].trim();
            let rhs = s[pos + op_str.len()..].trim().to_string();
            if rhs.is_empty() {
                continue;
            }
            let (field_name, summary_modifier) = parse_attribute_lhs(lhs);
            if field_name.is_empty() {
                continue;
            }
            let modifier = summary_modifier.into_iter().collect();
            return Some(attributes::Attribute {
                name: field_name,
                operator: Some(op.clone()),
                value: Some(attributes::AttributeValue::Single(rhs)),
                modifier,
            });
        }
    }

    // No operator — treat as `exists` (bare field name only, no parens)
    let s = s.trim();
    if !s.is_empty() && !s.contains('(') && !s.contains(')') {
        return Some(attributes::Attribute {
            name: s.to_string(),
            operator: Some(attributes::AttributeOperator::Exists),
            value: None,
            modifier: Vec::new(),
        });
    }
    None
}

/// Parse a LHS like `min(genome_size)` into `(field_name, optional_modifier)`.
fn parse_attribute_lhs(lhs: &str) -> (String, Option<attributes::Modifier>) {
    const SUMMARY_FNS: &[(&str, attributes::Modifier)] = &[
        ("min", attributes::Modifier::Min),
        ("max", attributes::Modifier::Max),
        ("mean", attributes::Modifier::Mean),
        ("median", attributes::Modifier::Median),
        ("sum", attributes::Modifier::Sum),
        ("list", attributes::Modifier::List),
        ("length", attributes::Modifier::Length),
    ];
    for (fn_name, modifier) in SUMMARY_FNS {
        let prefix = format!("{fn_name}(");
        if lhs.starts_with(prefix.as_str()) && lhs.ends_with(')') {
            let field = lhs[prefix.len()..lhs.len() - 1].trim().to_string();
            return (field, Some(modifier.clone()));
        }
    }
    (lhs.to_string(), None)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_moves_params_keys_out_of_the_query() {
        let document = "index: taxon\ntaxa:\n  - Mammalia\nsize: 5\ninclude_estimates: true\n";

        let split = split_query_document(document, None).unwrap();

        let query = SearchQuery::from_yaml(&split.query_yaml).unwrap();
        assert_eq!(
            query.identifiers.taxa.map(|taxa| taxa.names),
            Some(vec!["Mammalia".to_string()])
        );
        let params = QueryParams::from_yaml(&split.params_yaml).unwrap();
        assert_eq!(params.size, 5);
        assert!(params.include_estimates);
    }

    #[test]
    fn split_merges_explicit_params_document() {
        let split =
            split_query_document("index: taxon\nsize: 5\n", Some("taxonomy: ncbi\n")).unwrap();

        let params = QueryParams::from_yaml(&split.params_yaml).unwrap();
        assert_eq!(params.size, 5);
        assert_eq!(params.taxonomy, "ncbi");
    }

    #[test]
    fn split_rejects_key_set_in_both_documents() {
        let error = split_query_document("size: 5\n", Some("size: 10\n")).unwrap_err();

        assert!(error.contains("`size`"), "unexpected error: {error}");
    }

    #[test]
    fn split_rejects_query_key_in_params_document() {
        let error =
            split_query_document("index: taxon\n", Some("taxa:\n  - Mammalia\n")).unwrap_err();

        assert!(error.contains("taxa"), "unexpected error: {error}");
    }

    #[test]
    fn split_rejects_non_mapping_document() {
        assert!(split_query_document("- a\n- b\n", None).is_err());
    }

    #[test]
    fn split_leaves_misspelt_keys_for_the_query_parser() {
        let split = split_query_document("index: taxon\nsise: 5\n", None).unwrap();

        assert!(SearchQuery::from_yaml(&split.query_yaml).is_err());
    }

    #[test]
    fn split_empty_document_gives_empty_mappings() {
        let split = split_query_document("", None).unwrap();

        assert!(SearchQuery::from_yaml(&split.query_yaml).is_ok());
        assert_eq!(QueryParams::from_yaml(&split.params_yaml).unwrap().size, 10);
    }

    #[test]
    fn split_reads_nested_query_and_params_sections() {
        let document = "query:\n  index: taxon\n  taxa:\n    - Mammalia\nparams:\n  size: 5\n";

        let split = split_query_document(document, None).unwrap();

        let query = SearchQuery::from_yaml(&split.query_yaml).unwrap();
        assert_eq!(
            query.identifiers.taxa.map(|taxa| taxa.names),
            Some(vec!["Mammalia".to_string()])
        );
        assert_eq!(QueryParams::from_yaml(&split.params_yaml).unwrap().size, 5);
    }

    #[test]
    fn split_accepts_empty_nested_sections() {
        let split = split_query_document("query:\nparams:\n", None).unwrap();

        assert!(SearchQuery::from_yaml(&split.query_yaml).is_ok());
        assert!(QueryParams::from_yaml(&split.params_yaml).is_ok());
    }

    #[test]
    fn split_rejects_nested_document_mixing_sections_and_flat_keys() {
        let error = split_query_document("query:\n  index: taxon\nsize: 5\n", None).unwrap_err();

        assert!(error.contains("mixes"), "unexpected error: {error}");
    }

    #[test]
    fn split_rejects_params_key_inside_query_section() {
        let error = split_query_document("query:\n  size: 5\n", None).unwrap_err();

        assert!(error.contains("query section"), "unexpected error: {error}");
    }

    #[test]
    fn split_rejects_non_params_key_inside_params_section() {
        let error = split_query_document("params:\n  taxa:\n    - Mammalia\n", None).unwrap_err();

        assert!(
            error.contains("params section"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn split_merges_explicit_nested_params_section() {
        let split = split_query_document(
            "index: taxon\nsize: 5\n",
            Some("params:\n  taxonomy: ncbi\n"),
        )
        .unwrap();

        let params = QueryParams::from_yaml(&split.params_yaml).unwrap();
        assert_eq!(params.size, 5);
        assert_eq!(params.taxonomy, "ncbi");
    }

    #[test]
    fn split_rejects_query_section_in_params_document() {
        let error =
            split_query_document("index: taxon\n", Some("query:\n  index: taxon\n")).unwrap_err();

        assert!(
            error.contains("non-params key"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn query_params_keys_match_struct_fields() {
        let params = QueryParams {
            sort: Some(Vec::new()),
            id_set: Some(Vec::new()),
            id_type: Some("taxon".to_string()),
            search_after: Some(Vec::new()),
            ..Default::default()
        };
        let serialised = serde_yaml::to_value(&params).unwrap();
        let mut field_names: Vec<String> = serialised
            .as_mapping()
            .unwrap()
            .keys()
            .map(|key| key.as_str().unwrap().to_string())
            .collect();
        let mut listed: Vec<String> = QUERY_PARAMS_KEYS
            .iter()
            .map(|key| key.to_string())
            .collect();
        field_names.sort();
        listed.sort();

        assert_eq!(listed, field_names);
    }

    #[test]
    fn search_query_round_trips_yaml() {
        let yaml = r#"
index: taxon
taxa:
  - Mammalia
  - "!Felis"
rank: species
taxon_filter_type: tree
attributes:
  - name: genome_size
    operator: lt
    value: "3000000000"
    modifier: [min, direct]
fields:
  - name: genome_size
    modifier: [min]
names: [scientific_name]
ranks: [genus]
"#;
        let query: SearchQuery = serde_yaml::from_str(yaml).expect("parse YAML");
        assert_eq!(query.index, SearchIndex::Taxon);
        let taxa = query
            .identifiers
            .taxa
            .as_ref()
            .expect("taxa should be Some");
        assert_eq!(taxa.names, vec!["Mammalia", "!Felis"]);
        assert_eq!(query.identifiers.rank, Some("species".to_string()));
        assert_eq!(taxa.filter_type, TaxonFilterType::Tree);
        assert_eq!(query.attributes.attributes.len(), 1);
        assert_eq!(query.attributes.fields.len(), 1);
        assert_eq!(query.attributes.names, vec!["scientific_name"]);
        assert_eq!(query.attributes.ranks, vec!["genus"]);
    }

    #[test]
    fn query_params_defaults_match_mcp_server() {
        let params = QueryParams::default();
        assert_eq!(params.size, 10);
        assert_eq!(params.page, 1);
        assert!(!params.include_estimates);
        assert!(!params.tidy);
        assert_eq!(params.taxonomy, "ncbi");
        assert_eq!(params.sort_order, SortOrder::Asc);
    }

    #[test]
    fn search_index_taxon() {
        assert_eq!(SearchIndex::Taxon, SearchIndex::Taxon);
    }

    #[test]
    fn search_index_assembly() {
        assert_eq!(SearchIndex::Assembly, SearchIndex::Assembly);
    }

    #[test]
    fn search_index_sample() {
        assert_eq!(SearchIndex::Sample, SearchIndex::Sample);
    }

    #[test]
    fn search_query_from_yaml_single_index() {
        let yaml = r#"
index: assembly
taxa: []
"#;
        let query = SearchQuery::from_yaml(yaml).expect("parse");
        assert_eq!(query.index, SearchIndex::Assembly);
    }

    #[test]
    fn search_query_from_yaml_with_error() {
        let yaml = "invalid: {yaml: [";
        let result = SearchQuery::from_yaml(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn search_query_to_yaml() {
        let query = SearchQuery {
            index: SearchIndex::Taxon,
            identifiers: Identifiers::default(),
            attributes: AttributeSet::default(),
            queries: None,
            named_queries: None,
            combine_with: Default::default(),
            lineage_rank_summary: None,
        };
        let yaml = query.to_yaml().expect("serialize");
        assert!(yaml.contains("taxon"));
    }

    #[test]
    fn search_query_assembly_index() {
        let query = SearchQuery {
            index: SearchIndex::Assembly,
            identifiers: Identifiers::default(),
            attributes: AttributeSet::default(),
            queries: None,
            named_queries: None,
            combine_with: Default::default(),
            lineage_rank_summary: None,
        };
        assert_eq!(query.index, SearchIndex::Assembly);
    }

    #[test]
    fn sort_order_ascending_is_default() {
        let order = SortOrder::default();
        assert_eq!(order, SortOrder::Asc);
    }

    #[test]
    fn sort_order_descending_exists() {
        let order = SortOrder::Desc;
        assert_eq!(order, SortOrder::Desc);
    }

    #[test]
    fn query_params_with_custom_size() {
        let params = QueryParams {
            size: 100,
            ..Default::default()
        };
        assert_eq!(params.size, 100);
        assert_eq!(params.page, 1);
    }

    #[test]
    fn query_params_with_custom_page() {
        let params = QueryParams {
            page: 5,
            ..Default::default()
        };
        assert_eq!(params.page, 5);
        assert_eq!(params.size, 10);
    }

    #[test]
    fn query_params_with_tidy_true() {
        let params = QueryParams {
            tidy: true,
            ..Default::default()
        };
        assert!(params.tidy);
    }

    #[test]
    fn query_params_with_custom_taxonomy() {
        let params = QueryParams {
            taxonomy: "ott".to_string(),
            ..Default::default()
        };
        assert_eq!(params.taxonomy, "ott");
    }

    #[test]
    fn query_params_with_sort_by() {
        let params = QueryParams {
            sort_by: Some("genome_size".to_string()),
            ..Default::default()
        };
        assert_eq!(params.sort_by, Some("genome_size".to_string()));
    }

    #[test]
    fn query_params_with_sort_order_desc() {
        let params = QueryParams {
            sort_order: SortOrder::Desc,
            ..Default::default()
        };
        assert_eq!(params.sort_order, SortOrder::Desc);
    }

    #[test]
    fn query_params_include_estimates_true() {
        let params = QueryParams {
            include_estimates: true,
            ..Default::default()
        };
        assert!(params.include_estimates);
    }

    #[test]
    fn url_is_report_path() {
        assert!(url_is_report(
            "https://goat.genomehubs.org/api/v2/report?report=histogram&x=genome_size&result=taxon"
        ));
        assert!(url_is_report(
            "https://goat.genomehubs.org/report?report=histogram"
        ));
        assert!(!url_is_report(
            "https://goat.genomehubs.org/api/v2/search?tax_name=Primates"
        ));
    }

    #[test]
    fn url_is_report_param() {
        assert!(url_is_report(
            "https://example.org/api?report=scatter&result=taxon"
        ));
    }

    #[test]
    fn query_yaml_from_url_params_structured() {
        let url = "https://goat.genomehubs.org/api/v2/search?tax_name=Primates&fields=genome_size&result=taxon&size=20";
        let (qy, py) = query_yaml_from_url_params(url).unwrap();
        let q: SearchQuery = serde_yaml::from_str(&qy).unwrap();
        assert_eq!(q.index, SearchIndex::Taxon);
        let taxa = q.identifiers.taxa.unwrap();
        assert_eq!(taxa.names, vec!["Primates"]);
        assert_eq!(taxa.filter_type, identifiers::TaxonFilterType::Name);
        assert_eq!(q.attributes.fields[0].name, "genome_size");
        let p: QueryParams = serde_yaml::from_str(&py).unwrap();
        assert_eq!(p.size, 20);
    }

    #[test]
    fn query_yaml_from_url_params_query_fragment() {
        let url = "https://goat.genomehubs.org/api/v2/search?query=tax_name(Mammalia)%20AND%20genome_size%3E%3D1000000000&result=taxon";
        let (qy, _py) = query_yaml_from_url_params(url).unwrap();
        let q: SearchQuery = serde_yaml::from_str(&qy).unwrap();
        let taxa = q.identifiers.taxa.unwrap();
        assert_eq!(taxa.names, vec!["Mammalia"]);
        assert_eq!(q.attributes.attributes[0].name, "genome_size");
    }

    #[test]
    fn query_yaml_from_url_params_assembly_index() {
        let url = "https://example.org/search?result=assembly&tax_tree=Mammalia";
        let (qy, _) = query_yaml_from_url_params(url).unwrap();
        let q: SearchQuery = serde_yaml::from_str(&qy).unwrap();
        assert_eq!(q.index, SearchIndex::Assembly);
        let taxa = q.identifiers.taxa.unwrap();
        assert_eq!(taxa.filter_type, identifiers::TaxonFilterType::Tree);
    }

    #[test]
    fn query_yaml_from_url_params_sort_and_taxonomy() {
        let url = "https://example.org/search?result=taxon&sortBy=genome_size&sortOrder=desc&taxonomy=ott&includeEstimates=false";
        let (_qy, py) = query_yaml_from_url_params(url).unwrap();
        let p: QueryParams = serde_yaml::from_str(&py).unwrap();
        assert_eq!(p.sort_by, Some("genome_size".to_string()));
        assert_eq!(p.sort_order, SortOrder::Desc);
        assert_eq!(p.taxonomy, "ott");
        assert!(!p.include_estimates);
    }

    #[test]
    fn validate_id_set_under_limit() {
        let params = QueryParams {
            id_set: Some(vec![
                "1".into(),
                "2".into(),
                "3".into(),
                "4".into(),
                "5".into(),
            ]),
            ..Default::default()
        };
        assert!(params.validate_id_set().is_ok());
    }

    #[test]
    fn validate_id_set_at_limit() {
        let params = QueryParams {
            id_set: Some(vec!["1".to_string(); 65_536]),
            ..Default::default()
        };
        assert!(params.validate_id_set().is_ok());
    }

    #[test]
    fn validate_id_set_over_limit() {
        let params = QueryParams {
            id_set: Some(vec!["1".to_string(); 65_537]),
            ..Default::default()
        };
        let err = params.validate_id_set();
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("65536"));
    }

    #[test]
    fn validate_id_set_none() {
        let params = QueryParams::default();
        assert!(params.validate_id_set().is_ok());
    }

    #[test]
    fn resolve_id_field_taxon_default() {
        let params = QueryParams {
            id_set: Some(vec!["1".into(), "2".into(), "3".into()]),
            id_type: None,
            ..Default::default()
        };
        assert_eq!(
            params.resolve_id_field("taxon"),
            Some("taxon_id".to_string())
        );
    }

    #[test]
    fn resolve_id_field_assembly_default() {
        let params = QueryParams {
            id_set: Some(vec!["1".into(), "2".into(), "3".into()]),
            id_type: None,
            ..Default::default()
        };
        assert_eq!(
            params.resolve_id_field("assembly"),
            Some("assembly_id".to_string())
        );
    }

    #[test]
    fn resolve_id_field_sample_default() {
        let params = QueryParams {
            id_set: Some(vec!["1".into(), "2".into(), "3".into()]),
            id_type: None,
            ..Default::default()
        };
        assert_eq!(
            params.resolve_id_field("sample"),
            Some("sample_id".to_string())
        );
    }

    #[test]
    fn resolve_id_field_explicit_type() {
        let params = QueryParams {
            id_set: Some(vec!["1".into(), "2".into(), "3".into()]),
            id_type: Some("taxon".to_string()),
            ..Default::default()
        };
        assert_eq!(
            params.resolve_id_field("assembly"),
            Some("taxon_id".to_string())
        );
    }

    #[test]
    fn resolve_id_field_no_id_set() {
        let params = QueryParams::default();
        assert_eq!(params.resolve_id_field("taxon"), None);
    }

    #[test]
    fn query_params_id_set_serde_roundtrip() {
        let yaml =
            "size: 10\npage: 1\nid_set:\n  - '10090'\n  - '10116'\n  - '9606'\nid_type: taxon\n";
        let params: QueryParams = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(
            params.id_set,
            Some(vec![
                "10090".to_string(),
                "10116".to_string(),
                "9606".to_string()
            ])
        );
        assert_eq!(params.id_type, Some("taxon".to_string()));
    }
}
