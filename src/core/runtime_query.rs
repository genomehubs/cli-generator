//! Runtime search and count requests against any GenomeHubs API.
//!
//! v2 `--query` expressions are translated to v3 documents by the canonical URL
//! converter. Search and count requests send a POST body holding a `query_yaml`
//! and `params_yaml` document, split from a single combined YAML where needed.
//! Describe and validate read the same split documents locally.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::Value;

use crate::core::config::{RuntimeConfig, SitePresetRegistry};
use crate::core::fetch::FieldDef;
use crate::core::metadata::{load_result_fields, MetadataOptions};
use crate::core::query::{SearchQuery, TaxaIdentifier, TaxonFilterType};
use genomehubs_query::describe::QueryDescriber;
use genomehubs_query::query::{
    parse_attribute_fragment, split_query_document, Attribute, Field, QueryParams,
    SplitQueryDocument,
};

/// Index used when neither `--result` nor the preset provides one.
const FALLBACK_RESULT_INDEX: &str = "taxon";

/// Base used only to percent-encode query parameters into a query string.
const ENCODING_BASE: &str = "http://localhost/";

/// Parameters of a v2 `--query` expression, translated to v3 by the canonical URL converter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueryRequest {
    /// Result index to query, e.g. `taxon`.
    pub result: String,
    /// Optional query expression, e.g. `tax_tree(Mammalia)`.
    pub query: Option<String>,
    /// Field names to return; empty means none requested.
    pub fields: Vec<String>,
    /// Number of hits to return (search only).
    pub size: Option<usize>,
}

/// Choose the result index: explicit flag, then the preset default, then `taxon`.
pub fn resolve_result_index(requested: Option<&str>, runtime: &RuntimeConfig) -> String {
    requested
        .map(str::to_owned)
        .or_else(|| runtime.default_index.clone())
        .unwrap_or_else(|| FALLBACK_RESULT_INDEX.to_string())
}

/// Build the inner query string (without a leading `?`) for a request.
pub fn build_query_string(request: &QueryRequest) -> Result<String> {
    let mut params: Vec<(&str, String)> = vec![("result", request.result.clone())];
    if let Some(query) = &request.query {
        params.push(("query", query.clone()));
    }
    if !request.fields.is_empty() {
        params.push(("fields", request.fields.join(",")));
    }
    if let Some(size) = request.size {
        params.push(("size", size.to_string()));
    }
    let encoded = reqwest::Url::parse_with_params(ENCODING_BASE, &params)?;
    Ok(encoded.query().unwrap_or_default().to_string())
}

/// Parameters for a type-ahead lookup request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LookupRequest {
    /// Free-text term to resolve, e.g. `Homo sapiens`.
    pub search_term: String,
    /// Result index to search, e.g. `taxon`.
    pub result: String,
    /// Number of candidates to return.
    pub size: Option<usize>,
}

/// Parameters for fetching one or more records by ID.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordRequest {
    /// Record IDs to fetch, e.g. `9606` or `assembly-GCA_000001405.29`.
    pub record_ids: Vec<String>,
    /// Result index of the records, e.g. `taxon`.
    pub result: String,
}

/// Build the `/v3/record` query parameters; IDs are sent comma-separated.
fn record_params(request: &RecordRequest) -> Vec<(&'static str, String)> {
    vec![
        ("recordId", request.record_ids.join(",")),
        ("result", request.result.clone()),
    ]
}

/// Build the `{api_base}/v3/{endpoint}` URL.
fn v3_url(api_base: &str, endpoint: &str) -> String {
    format!("{}/v3/{endpoint}", api_base.trim_end_matches('/'))
}

/// Send a prepared request and decode the JSON body; non-2xx responses are errors.
fn send_json(method: &str, url: &str, request: reqwest::blocking::RequestBuilder) -> Result<Value> {
    let response = request
        .send()
        .with_context(|| format!("HTTP {method} {url}"))?
        .error_for_status()
        .with_context(|| format!("non-2xx response from {url}"))?;
    response.json().context("deserialising JSON response")
}

/// Send a GET to `{api_base}/v3/{endpoint}` with the given query parameters.
fn get_json(api_base: &str, endpoint: &str, params: &[(&str, String)]) -> Result<Value> {
    let url = v3_url(api_base, endpoint);
    let request = reqwest::blocking::Client::new().get(&url).query(params);
    send_json("GET", &url, request)
}

/// Send a POST to `{api_base}/v3/{endpoint}` with the query and params documents as JSON.
fn post_v3(api_base: &str, endpoint: &str, documents: &SplitQueryDocument) -> Result<Value> {
    let body = serde_json::json!({
        "query_yaml": documents.query_yaml,
        "params_yaml": documents.params_yaml,
    });
    post_json(api_base, endpoint, &body)
}

/// Send a POST with a JSON body to `{api_base}/v3/{endpoint}`.
fn post_json(api_base: &str, endpoint: &str, body: &Value) -> Result<Value> {
    let url = v3_url(api_base, endpoint);
    let request = reqwest::blocking::Client::new().post(&url).json(body);
    send_json("POST", &url, request)
}

/// Plot options shared by the `report` and `positional` commands.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlotOptions {
    /// Ask the API to include the PlotSpec in the response.
    pub include_plot_spec: bool,
    /// Display options applied to the returned PlotSpec.
    pub display: Option<Value>,
}

impl PlotOptions {
    /// Build plot options from the `--include-plot-spec` flag and the optional `--display` JSON.
    pub fn from_flags(include_plot_spec: bool, display_json: Option<&str>) -> Result<Self> {
        let display: Option<Value> = display_json
            .map(|json| serde_json::from_str(json).context("parsing --display as JSON"))
            .transpose()?;
        Ok(Self {
            include_plot_spec,
            display,
        })
    }
}

/// Run a v3 `report` or `positional` request and return the JSON response.
///
/// `endpoint` is `report` or `positional`; the config YAML is sent as `{endpoint}_yaml`.
pub fn run_v3_plot(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    endpoint: &str,
    documents: &SplitQueryDocument,
    config_yaml: &str,
    options: &PlotOptions,
) -> Result<Value> {
    let config = read_yaml_argument(config_yaml)?;
    let body = plot_request_body(endpoint, documents, &config, options);
    let api_base = runtime.require_api_base(registry)?;
    post_json(api_base, endpoint, &body)
}

/// Build the JSON body for a v3 `report` or `positional` request.
fn plot_request_body(
    endpoint: &str,
    documents: &SplitQueryDocument,
    config_yaml: &str,
    options: &PlotOptions,
) -> Value {
    let mut body = serde_json::json!({
        "query_yaml": documents.query_yaml,
        "params_yaml": documents.params_yaml,
    });
    body[format!("{endpoint}_yaml")] = Value::String(config_yaml.to_string());
    if options.include_plot_spec {
        body["include_plot_spec"] = Value::Bool(true);
    }
    if let Some(display) = &options.display {
        body["display"] = display.clone();
    }
    body
}

/// Run a v3 search or count with separate query and params documents.
pub fn run_v3_query(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    endpoint: &str,
    documents: &SplitQueryDocument,
) -> Result<Value> {
    let api_base = runtime.require_api_base(registry)?;
    post_v3(api_base, endpoint, documents)
}

/// Read a YAML argument: the contents of an existing file, or the text itself.
///
/// A value shaped like a `.yaml` or `.yml` path that does not exist is an error.
pub fn read_yaml_argument(value: &str) -> Result<String> {
    let path = Path::new(value);
    if path.is_file() {
        return std::fs::read_to_string(path).with_context(|| format!("reading YAML file {value}"));
    }
    if looks_like_yaml_path(value) {
        bail!("YAML file not found: {value}");
    }
    Ok(value.to_string())
}

/// Whether a value is shaped like a YAML file path rather than YAML text.
fn looks_like_yaml_path(value: &str) -> bool {
    let has_yaml_extension = matches!(
        Path::new(value).extension().and_then(|ext| ext.to_str()),
        Some("yaml" | "yml")
    );
    has_yaml_extension && !value.contains(['\n', ':'])
}

/// Build v3 query documents from the optional `--query-yaml` and `--params-yaml` arguments.
///
/// With no query, the search matches all records on `default_index`. Both arguments accept a
/// file path or inline YAML.
pub fn v3_query_input(
    query_yaml: Option<&str>,
    params_yaml: Option<&str>,
    default_index: &str,
) -> Result<SplitQueryDocument> {
    let query_document = match query_yaml {
        Some(value) => read_yaml_argument(value)?,
        None => format!("index: {default_index}\n"),
    };
    let params_document = params_yaml.map(read_yaml_argument).transpose()?;
    let documents = split_query_document(&query_document, params_document.as_deref())
        .map_err(anyhow::Error::msg)?;
    Ok(documents)
}

/// Run a lookup request against `/v3/lookup` and return the JSON response.
pub fn run_lookup(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    request: &LookupRequest,
) -> Result<Value> {
    let api_base = runtime.require_api_base(registry)?;
    let mut params = vec![
        ("searchTerm", request.search_term.clone()),
        ("result", request.result.clone()),
    ];
    if let Some(size) = request.size {
        params.push(("size", size.to_string()));
    }
    get_json(api_base, "lookup", &params)
}

/// Fetch records by ID from `/v3/record` and return the JSON response.
pub fn run_record(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    request: &RecordRequest,
) -> Result<Value> {
    let api_base = runtime.require_api_base(registry)?;
    get_json(api_base, "record", &record_params(request))
}

/// Licence restrictions sent to PhyloPic as API params; empty or `false` means no restriction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PhylopicFilter {
    /// SPDX licence IDs the image must carry; empty accepts any licence.
    pub licenses: Vec<String>,
    /// Accept only public-domain images.
    pub public_domain: bool,
}

/// A PhyloPic request for one taxon or a batch of taxa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhylopicRequest {
    /// Resolve one taxon with `GET /v3/phylopic`.
    Single {
        /// NCBI taxon ID, e.g. `9606`.
        taxon_id: String,
        /// Taxonomy the ID belongs to, e.g. `ncbi`.
        taxonomy: String,
        /// Licence restrictions.
        filter: PhylopicFilter,
    },
    /// Resolve many taxa with `POST /v3/phylopic/batch`.
    Batch {
        /// NCBI taxon IDs; the API accepts up to 200.
        taxon_ids: Vec<String>,
        /// Taxonomy the IDs belong to, e.g. `ncbi`.
        taxonomy: String,
        /// Licence restrictions.
        filter: PhylopicFilter,
    },
}

/// Build a PhyloPic request from `--taxon-id` values or a `--batch` file.
///
/// One ID uses the single route; two or more use the batch route.
pub fn phylopic_request(
    taxon_ids: Vec<String>,
    batch_file: Option<&Path>,
    taxonomy: String,
    filter: PhylopicFilter,
) -> Result<PhylopicRequest> {
    let mut flag_ids: Vec<String> = taxon_ids
        .into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();
    if let Some(path) = batch_file {
        if !flag_ids.is_empty() {
            bail!("provide either --taxon-id or --batch, not both");
        }
        return Ok(PhylopicRequest::Batch {
            taxon_ids: read_taxon_ids_file(path)?,
            taxonomy,
            filter,
        });
    }
    match flag_ids.len() {
        0 => bail!("provide either --taxon-id or --batch"),
        1 => Ok(PhylopicRequest::Single {
            taxon_id: flag_ids.swap_remove(0),
            taxonomy,
            filter,
        }),
        _ => Ok(PhylopicRequest::Batch {
            taxon_ids: flag_ids,
            taxonomy,
            filter,
        }),
    }
}

/// Read a batch file of taxon IDs, as a JSON array or one numeric ID per line.
pub fn read_taxon_ids_file(path: &Path) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading batch file {}", path.display()))?;
    parse_taxon_ids(&text)
}

/// Parse taxon IDs from a JSON array, or from a bare list with one numeric ID per line.
pub fn parse_taxon_ids(text: &str) -> Result<Vec<String>> {
    if text.trim_start().starts_with('[') {
        return parse_json_taxon_ids(text);
    }
    parse_bare_taxon_ids(text)
}

/// Parse one numeric taxon ID per line, skipping blank lines.
fn parse_bare_taxon_ids(text: &str) -> Result<Vec<String>> {
    text.lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.trim()))
        .filter(|(_, line)| !line.is_empty())
        .map(|(number, line)| {
            if line.bytes().all(|byte| byte.is_ascii_digit()) {
                Ok(line.to_string())
            } else {
                bail!("line {number} of the batch file is not a numeric taxon ID: {line}")
            }
        })
        .collect()
}

/// Parse a JSON array of taxon IDs; integers are accepted and converted to strings.
fn parse_json_taxon_ids(text: &str) -> Result<Vec<String>> {
    let value: Value = serde_json::from_str(text).context("parsing batch file as JSON")?;
    let items = value
        .as_array()
        .context("batch file must contain a JSON array of taxon IDs")?;
    items
        .iter()
        .map(|item| -> Result<String> {
            match item {
                Value::String(id) => Ok(id.clone()),
                Value::Number(id) => Ok(id.to_string()),
                _ => bail!("batch file entries must be taxon IDs (strings or integers)"),
            }
        })
        .collect()
}

/// Build the `/v3/phylopic` query parameters; licence params are sent only when set.
fn phylopic_params(
    taxon_id: &str,
    taxonomy: &str,
    filter: &PhylopicFilter,
) -> Vec<(&'static str, String)> {
    let mut params = vec![
        ("taxon_id", taxon_id.to_string()),
        ("taxonomy", taxonomy.to_string()),
    ];
    if !filter.licenses.is_empty() {
        params.push(("license", filter.licenses.join(",")));
    }
    if filter.public_domain {
        params.push(("public_domain", "true".to_string()));
    }
    params
}

/// Build the `/v3/phylopic/batch` JSON body; an unset licence is sent as `null`.
fn phylopic_batch_body(taxon_ids: &[String], taxonomy: &str, filter: &PhylopicFilter) -> Value {
    let license = (!filter.licenses.is_empty()).then(|| filter.licenses.join(","));
    serde_json::json!({
        "taxon_ids": taxon_ids,
        "taxonomy": taxonomy,
        "license": license,
        "public_domain": filter.public_domain,
    })
}

/// Resolve PhyloPic images via `/v3/phylopic` or `/v3/phylopic/batch` and return the JSON response.
pub fn run_phylopic(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    request: &PhylopicRequest,
) -> Result<Value> {
    let api_base = runtime.require_api_base(registry)?;
    match request {
        PhylopicRequest::Single {
            taxon_id,
            taxonomy,
            filter,
        } => get_json(
            api_base,
            "phylopic",
            &phylopic_params(taxon_id, taxonomy, filter),
        ),
        PhylopicRequest::Batch {
            taxon_ids,
            taxonomy,
            filter,
        } => post_json(
            api_base,
            "phylopic/batch",
            &phylopic_batch_body(taxon_ids, taxonomy, filter),
        ),
    }
}

/// A metadata request, one per `/v3/metadata` route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataRequest {
    /// Aggregated hub metadata from `/v3/metadata`.
    Aggregate,
    /// Available indices from `/v3/metadata/indices`.
    Indices,
    /// Taxonomies from `/v3/metadata/taxonomies`.
    Taxonomies,
    /// Taxonomic ranks from `/v3/metadata/ranks`.
    Ranks,
    /// Field definitions for one result index from `/v3/metadata/fields`.
    Fields {
        /// Result index to describe, e.g. `taxon`.
        index: String,
    },
}

/// Map a metadata request to its endpoint and query params; `index` is sent as `result`.
fn metadata_endpoint(request: &MetadataRequest) -> (&'static str, Vec<(&'static str, String)>) {
    match request {
        MetadataRequest::Aggregate => ("metadata", Vec::new()),
        MetadataRequest::Indices => ("metadata/indices", Vec::new()),
        MetadataRequest::Taxonomies => ("metadata/taxonomies", Vec::new()),
        MetadataRequest::Ranks => ("metadata/ranks", Vec::new()),
        MetadataRequest::Fields { index } => ("metadata/fields", vec![("result", index.clone())]),
    }
}

/// Fetch hub metadata from the `/v3/metadata` routes and return the JSON response.
pub fn run_metadata(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    request: &MetadataRequest,
) -> Result<Value> {
    let api_base = runtime.require_api_base(registry)?;
    let (endpoint, params) = metadata_endpoint(request);
    get_json(api_base, endpoint, &params)
}

/// Fetch API health and readiness from `/v3/status`.
pub fn run_status(runtime: &RuntimeConfig, registry: &SitePresetRegistry) -> Result<Value> {
    let api_base = runtime.require_api_base(registry)?;
    get_json(api_base, "status", &[])
}

/// Translate a v2 `--query` request into v3 documents via the canonical URL converter.
///
/// The same converter backs `QueryBuilder.from_v2_url()` in the SDKs.
pub fn v3_query_input_from_v2(request: &QueryRequest) -> Result<SplitQueryDocument> {
    let inner_query = build_query_string(request)?;
    let url = format!("{ENCODING_BASE}search?{inner_query}");
    let (query_yaml, params_yaml) =
        genomehubs_query::query::query_yaml_from_url_params(&url).map_err(anyhow::Error::msg)?;
    Ok(SplitQueryDocument {
        query_yaml,
        params_yaml,
    })
}

/// Overrides from command-line flags; each set field replaces the matching YAML value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueryOverrides {
    /// Hit count: the params `size`.
    pub size: Option<usize>,
    /// v3 params `page`, 1-based.
    pub page: Option<usize>,
    /// v3 params `sort_by`, e.g. `genome_size`.
    pub sort_by: Option<String>,
    /// v3 params `sort_order`: `asc` or `desc`.
    pub sort_order: Option<String>,
    /// v3 params `tidy`.
    pub tidy: Option<bool>,
    /// v3 params `include_estimates`.
    pub include_estimates: Option<bool>,
    /// v3 params `taxonomy`, e.g. `ncbi`.
    pub taxonomy: Option<String>,
    /// v3 query `identifiers.taxa` names; replaces the YAML names and keeps the filter type.
    pub taxa: Vec<String>,
    /// v3 query `identifiers.rank`, e.g. `species`.
    pub taxon_rank: Option<String>,
    /// v3 query `identifiers.taxa.filter_type`; requires taxa from `taxa` or the query YAML.
    pub taxon_filter: Option<TaxonFilterType>,
    /// v3 query `identifiers.assemblies`; replaces the YAML list.
    pub assemblies: Vec<String>,
    /// v3 query `identifiers.samples`; replaces the YAML list.
    pub samples: Vec<String>,
    /// v3 query `attributes.fields`, the return columns; replaces the YAML list.
    pub fields: Vec<String>,
    /// v3 query `attributes.ranks`, the rank columns; replaces the YAML list.
    pub ranks: Vec<String>,
    /// v3 query `attributes.names`, the name classes; replaces the YAML list.
    pub names: Vec<String>,
    /// v3 query `attributes.attributes` filter expressions, e.g. `genome_size<3G`; replaces the YAML list.
    pub filters: Vec<String>,
}

impl QueryOverrides {
    /// The `(key, value)` pairs for every flag that was set, keyed as in the params YAML.
    fn entries(&self) -> Vec<(&'static str, serde_yaml::Value)> {
        [
            (
                "size",
                self.size.map(|value| serde_yaml::Value::from(value as u64)),
            ),
            (
                "page",
                self.page.map(|value| serde_yaml::Value::from(value as u64)),
            ),
            (
                "sort_by",
                self.sort_by.as_deref().map(serde_yaml::Value::from),
            ),
            (
                "sort_order",
                self.sort_order.as_deref().map(serde_yaml::Value::from),
            ),
            ("tidy", self.tidy.map(serde_yaml::Value::from)),
            (
                "include_estimates",
                self.include_estimates.map(serde_yaml::Value::from),
            ),
            (
                "taxonomy",
                self.taxonomy.as_deref().map(serde_yaml::Value::from),
            ),
        ]
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key, value)))
        .collect()
    }

    /// True when a flag sets a value stored in the v3 query YAML.
    fn touches_query(&self) -> bool {
        !self.taxa.is_empty()
            || self.taxon_rank.is_some()
            || self.taxon_filter.is_some()
            || !self.assemblies.is_empty()
            || !self.samples.is_empty()
            || !self.fields.is_empty()
            || !self.ranks.is_empty()
            || !self.names.is_empty()
            || !self.filters.is_empty()
    }
}

/// Parse the `--taxon-filter` value: `name`, `tree`, or `lineage`.
pub fn parse_taxon_filter(value: &str) -> Result<TaxonFilterType> {
    match value {
        "name" => Ok(TaxonFilterType::Name),
        "tree" => Ok(TaxonFilterType::Tree),
        "lineage" => Ok(TaxonFilterType::Lineage),
        other => bail!("unknown taxon filter '{other}'; expected name, tree, or lineage"),
    }
}

/// Replace the query YAML values set by flags, then check the result parses.
fn apply_query_overrides(
    documents: SplitQueryDocument,
    overrides: &QueryOverrides,
) -> Result<SplitQueryDocument> {
    if !overrides.touches_query() {
        return Ok(documents);
    }
    let mut query = search_query_from_yaml(&documents.query_yaml)?;
    apply_identifier_overrides(&mut query, overrides)?;
    apply_attribute_overrides(&mut query, overrides)?;
    let query_yaml = query.to_yaml().context("serialising query YAML")?;
    Ok(SplitQueryDocument {
        query_yaml,
        ..documents
    })
}

/// Replace the `identifiers` values set by flags.
fn apply_identifier_overrides(query: &mut SearchQuery, overrides: &QueryOverrides) -> Result<()> {
    if !overrides.taxa.is_empty() {
        let filter_type = query
            .identifiers
            .taxa
            .take()
            .map(|taxa| taxa.filter_type)
            .unwrap_or_default();
        query.identifiers.taxa = Some(TaxaIdentifier {
            names: overrides.taxa.clone(),
            filter_type,
        });
    }
    if let Some(filter_type) = &overrides.taxon_filter {
        let Some(taxa) = query.identifiers.taxa.as_mut() else {
            bail!("--taxon-filter needs taxa, from --taxon or the query YAML");
        };
        taxa.filter_type = filter_type.clone();
    }
    if let Some(rank) = &overrides.taxon_rank {
        query.identifiers.rank = Some(rank.clone());
    }
    if !overrides.assemblies.is_empty() {
        query.identifiers.assemblies = overrides.assemblies.clone();
    }
    if !overrides.samples.is_empty() {
        query.identifiers.samples = overrides.samples.clone();
    }
    Ok(())
}

/// Replace the `attributes` values set by flags; each filter expression must parse.
fn apply_attribute_overrides(query: &mut SearchQuery, overrides: &QueryOverrides) -> Result<()> {
    if !overrides.fields.is_empty() {
        query.attributes.fields = overrides
            .fields
            .iter()
            .map(|name| Field {
                name: name.clone(),
                modifier: Vec::new(),
            })
            .collect();
    }
    if !overrides.ranks.is_empty() {
        query.attributes.ranks = overrides.ranks.clone();
    }
    if !overrides.names.is_empty() {
        query.attributes.names = overrides.names.clone();
    }
    if !overrides.filters.is_empty() {
        query.attributes.attributes = overrides
            .filters
            .iter()
            .map(|expression| parse_filter(expression))
            .collect::<Result<Vec<Attribute>>>()?;
    }
    Ok(())
}

/// Parse one `--filter` expression, e.g. `genome_size<3G`.
fn parse_filter(expression: &str) -> Result<Attribute> {
    parse_attribute_fragment(expression).with_context(|| format!("invalid --filter '{expression}'"))
}

/// Replace the params keys set by flags, then check the result parses as [`QueryParams`].
fn apply_v3_overrides(
    documents: SplitQueryDocument,
    overrides: &QueryOverrides,
) -> Result<SplitQueryDocument> {
    let entries = overrides.entries();
    if entries.is_empty() {
        return Ok(documents);
    }
    let mut params: serde_yaml::Mapping =
        serde_yaml::from_str(&documents.params_yaml).context("parsing v3 params YAML")?;
    for (key, value) in entries {
        params.insert(serde_yaml::Value::from(key), value);
    }
    let params_yaml = serde_yaml::to_string(&params).context("serialising params YAML")?;
    QueryParams::from_yaml(&params_yaml).context("applying overrides to params YAML")?;
    Ok(SplitQueryDocument {
        params_yaml,
        ..documents
    })
}

/// Apply flag overrides: query flags replace the query YAML values, then params flags replace the params keys.
pub fn apply_overrides(
    documents: SplitQueryDocument,
    overrides: &QueryOverrides,
) -> Result<SplitQueryDocument> {
    let documents = apply_query_overrides(documents, overrides)?;
    apply_v3_overrides(documents, overrides)
}

/// Parse a v3 `SearchQuery` YAML body.
pub fn search_query_from_yaml(query_yaml: &str) -> Result<SearchQuery> {
    SearchQuery::from_yaml(query_yaml).context("parsing v3 query YAML")
}

/// Describe a query in prose, naming fields from the index metadata.
pub fn describe_input(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    documents: &SplitQueryDocument,
    metadata: &MetadataOptions,
    verbose: bool,
) -> Result<String> {
    let query = search_query_from_yaml(&documents.query_yaml)?;
    let api_base = runtime.require_api_base(registry)?;
    let fields = load_result_fields(api_base, &runtime.site_name, query.index.as_str(), metadata)?;
    let describer = QueryDescriber::new(field_map(fields));
    Ok(if verbose {
        describer.describe_verbose(&query)
    } else {
        describer.describe_concise(&query)
    })
}

/// Validate a query against the index metadata and return the error messages.
///
/// An empty list means the query is valid. Uses the same field metadata as `describe`.
pub fn validate_input(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    documents: &SplitQueryDocument,
    metadata: &MetadataOptions,
) -> Result<Vec<String>> {
    let query = search_query_from_yaml(&documents.query_yaml)?;
    let api_base = runtime.require_api_base(registry)?;
    let fields = load_result_fields(api_base, &runtime.site_name, query.index.as_str(), metadata)?;
    let mut errors =
        genomehubs_query::validation::validate_query_for_fields(&documents.query_yaml, &fields);
    if let Err(error) = QueryParams::from_yaml(&documents.params_yaml) {
        errors.push(format!("params: {error}"));
    }
    Ok(errors)
}

/// Key field definitions by canonical name for the describer.
fn field_map(fields: Vec<FieldDef>) -> HashMap<String, FieldDef> {
    fields
        .into_iter()
        .map(|field| (field.name.clone(), field))
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime_with_default_index(default_index: Option<&str>) -> RuntimeConfig {
        RuntimeConfig {
            default_index: default_index.map(str::to_owned),
            ..RuntimeConfig::default()
        }
    }

    #[test]
    fn resolve_result_index_prefers_explicit_request() {
        let runtime = runtime_with_default_index(Some("assembly"));
        assert_eq!(resolve_result_index(Some("taxon"), &runtime), "taxon");
    }

    #[test]
    fn resolve_result_index_falls_back_to_preset_then_taxon() {
        let with_preset = runtime_with_default_index(Some("assembly"));
        assert_eq!(resolve_result_index(None, &with_preset), "assembly");

        let without_preset = runtime_with_default_index(None);
        assert_eq!(resolve_result_index(None, &without_preset), "taxon");
    }

    #[test]
    fn build_query_string_encodes_all_parameters() {
        let request = QueryRequest {
            result: "taxon".to_string(),
            query: Some("tax_tree(Mammalia)".to_string()),
            fields: vec!["genome_size".to_string(), "assembly_level".to_string()],
            size: Some(10),
        };
        let inner = build_query_string(&request).unwrap();
        assert!(inner.starts_with("result=taxon&"));
        assert!(inner.contains("fields=genome_size%2Cassembly_level"));
        assert!(inner.contains("size=10"));
        assert!(inner.contains("query=tax_tree"));
    }

    #[test]
    fn build_query_string_omits_empty_optional_parameters() {
        let request = QueryRequest {
            result: "taxon".to_string(),
            ..QueryRequest::default()
        };
        assert_eq!(build_query_string(&request).unwrap(), "result=taxon");
    }

    #[test]
    fn run_lookup_sends_term_result_and_size() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/v3/lookup")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("searchTerm".into(), "Homo sapiens".into()),
                mockito::Matcher::UrlEncoded("result".into(), "taxon".into()),
                mockito::Matcher::UrlEncoded("size".into(), "5".into()),
            ]))
            .with_status(200)
            .with_body(r#"{"status":{"success":true},"results":[]}"#)
            .create();
        let runtime = RuntimeConfig {
            api_base: server.url(),
            ..RuntimeConfig::default()
        };
        let request = LookupRequest {
            search_term: "Homo sapiens".to_string(),
            result: "taxon".to_string(),
            size: Some(5),
        };
        let registry = SitePresetRegistry::builtin().unwrap();

        let value = run_lookup(&runtime, &registry, &request).unwrap();

        assert_eq!(value["status"]["success"], true);
        mock.assert();
    }

    #[test]
    fn record_params_joins_ids_and_sets_result() {
        let request = RecordRequest {
            record_ids: vec!["9606".to_string(), "9605".to_string()],
            result: "taxon".to_string(),
        };
        assert_eq!(
            record_params(&request),
            vec![
                ("recordId", "9606,9605".to_string()),
                ("result", "taxon".to_string()),
            ]
        );
    }

    #[test]
    fn run_record_sends_ids_and_result() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/v3/record")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("recordId".into(), "9606,9605".into()),
                mockito::Matcher::UrlEncoded("result".into(), "taxon".into()),
            ]))
            .with_status(200)
            .with_body(r#"{"status":{"success":true},"records":[]}"#)
            .create();
        let runtime = RuntimeConfig {
            api_base: server.url(),
            ..RuntimeConfig::default()
        };
        let request = RecordRequest {
            record_ids: vec!["9606".to_string(), "9605".to_string()],
            result: "taxon".to_string(),
        };
        let registry = SitePresetRegistry::builtin().unwrap();

        let value = run_record(&runtime, &registry, &request).unwrap();

        assert_eq!(value["status"]["success"], true);
        mock.assert();
    }

    #[test]
    fn phylopic_params_omits_unset_licence_params() {
        assert_eq!(
            phylopic_params("9606", "ncbi", &PhylopicFilter::default()),
            vec![
                ("taxon_id", "9606".to_string()),
                ("taxonomy", "ncbi".to_string()),
            ]
        );
    }

    #[test]
    fn phylopic_params_sends_licence_and_public_domain() {
        let filter = PhylopicFilter {
            licenses: vec!["CC0-1.0".to_string(), "CC-BY-4.0".to_string()],
            public_domain: true,
        };
        assert_eq!(
            phylopic_params("9606", "ncbi", &filter),
            vec![
                ("taxon_id", "9606".to_string()),
                ("taxonomy", "ncbi".to_string()),
                ("license", "CC0-1.0,CC-BY-4.0".to_string()),
                ("public_domain", "true".to_string()),
            ]
        );
    }

    #[test]
    fn phylopic_batch_body_sends_ids_taxonomy_and_filter() {
        let ids = vec!["9606".to_string(), "9605".to_string()];
        let filter = PhylopicFilter {
            licenses: vec!["CC0-1.0".to_string()],
            public_domain: false,
        };
        assert_eq!(
            phylopic_batch_body(&ids, "ncbi", &filter),
            serde_json::json!({
                "taxon_ids": ["9606", "9605"],
                "taxonomy": "ncbi",
                "license": "CC0-1.0",
                "public_domain": false,
            })
        );
    }

    #[test]
    fn parse_taxon_ids_accepts_strings_and_integers() {
        assert_eq!(
            parse_taxon_ids(r#"["9606", 9605]"#).unwrap(),
            vec!["9606", "9605"]
        );
    }

    #[test]
    fn parse_taxon_ids_rejects_object_and_nested_entries() {
        assert!(parse_taxon_ids(r#"{"taxon_ids": []}"#).is_err());
        assert!(parse_taxon_ids(r#"[{"id": 9606}]"#).is_err());
    }

    #[test]
    fn parse_taxon_ids_accepts_bare_list_skipping_blank_lines() {
        assert_eq!(
            parse_taxon_ids("9606\n\n  9605 \n").unwrap(),
            vec!["9606", "9605"]
        );
    }

    #[test]
    fn parse_taxon_ids_rejects_non_numeric_bare_line() {
        assert!(parse_taxon_ids("9606\nHomo sapiens\n").is_err());
        assert!(parse_taxon_ids("{\"taxon_ids\": []}").is_err());
    }

    #[test]
    fn metadata_endpoint_maps_each_route() {
        assert_eq!(
            metadata_endpoint(&MetadataRequest::Aggregate),
            ("metadata", vec![])
        );
        assert_eq!(
            metadata_endpoint(&MetadataRequest::Indices),
            ("metadata/indices", vec![])
        );
        assert_eq!(
            metadata_endpoint(&MetadataRequest::Taxonomies),
            ("metadata/taxonomies", vec![])
        );
        assert_eq!(
            metadata_endpoint(&MetadataRequest::Ranks),
            ("metadata/ranks", vec![])
        );
        assert_eq!(
            metadata_endpoint(&MetadataRequest::Fields {
                index: "assembly".to_string()
            }),
            ("metadata/fields", vec![("result", "assembly".to_string())])
        );
    }

    #[test]
    fn run_metadata_fields_sends_index_as_result() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/v3/metadata/fields")
            .match_query(mockito::Matcher::UrlEncoded(
                "result".into(),
                "assembly".into(),
            ))
            .with_status(200)
            .with_body(r#"{"status":{"success":true},"fields":{}}"#)
            .create();
        let runtime = RuntimeConfig {
            api_base: server.url(),
            ..RuntimeConfig::default()
        };
        let request = MetadataRequest::Fields {
            index: "assembly".to_string(),
        };
        let registry = SitePresetRegistry::builtin().unwrap();

        let value = run_metadata(&runtime, &registry, &request).unwrap();

        assert_eq!(value["status"]["success"], true);
        mock.assert();
    }

    #[test]
    fn run_status_gets_v3_status_route() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/v3/status")
            .with_status(200)
            .with_body(r#"{"status":{"success":true},"ready":true}"#)
            .create();
        let runtime = RuntimeConfig {
            api_base: server.url(),
            ..RuntimeConfig::default()
        };
        let registry = SitePresetRegistry::builtin().unwrap();

        let value = run_status(&runtime, &registry).unwrap();

        assert_eq!(value["ready"], true);
        mock.assert();
    }

    #[test]
    fn phylopic_request_requires_taxon_id_or_batch() {
        let filter = PhylopicFilter::default();
        assert!(phylopic_request(Vec::new(), None, "ncbi".to_string(), filter).is_err());
    }

    #[test]
    fn phylopic_request_rejects_taxon_id_with_batch_file() {
        let request = phylopic_request(
            vec!["9606".to_string()],
            Some(Path::new("ids.txt")),
            "ncbi".to_string(),
            PhylopicFilter::default(),
        );
        assert!(request.is_err());
    }

    #[test]
    fn phylopic_request_single_id_is_single() {
        let request = phylopic_request(
            vec!["9606".to_string()],
            None,
            "ncbi".to_string(),
            PhylopicFilter::default(),
        )
        .unwrap();
        assert_eq!(
            request,
            PhylopicRequest::Single {
                taxon_id: "9606".to_string(),
                taxonomy: "ncbi".to_string(),
                filter: PhylopicFilter::default(),
            }
        );
    }

    #[test]
    fn phylopic_request_many_ids_is_batch_without_blank_entries() {
        let request = phylopic_request(
            vec![" 9606".to_string(), "10090 ".to_string(), " ".to_string()],
            None,
            "ncbi".to_string(),
            PhylopicFilter::default(),
        )
        .unwrap();
        assert_eq!(
            request,
            PhylopicRequest::Batch {
                taxon_ids: vec!["9606".to_string(), "10090".to_string()],
                taxonomy: "ncbi".to_string(),
                filter: PhylopicFilter::default(),
            }
        );
    }

    #[test]
    fn run_phylopic_single_sends_licence_params() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/v3/phylopic")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("taxon_id".into(), "9606".into()),
                mockito::Matcher::UrlEncoded("taxonomy".into(), "ncbi".into()),
                mockito::Matcher::UrlEncoded("license".into(), "CC0-1.0".into()),
                mockito::Matcher::UrlEncoded("public_domain".into(), "true".into()),
            ]))
            .with_status(200)
            .with_body(r#"{"status":{"success":true},"phylopic":null}"#)
            .create();
        let runtime = RuntimeConfig {
            api_base: server.url(),
            ..RuntimeConfig::default()
        };
        let request = PhylopicRequest::Single {
            taxon_id: "9606".to_string(),
            taxonomy: "ncbi".to_string(),
            filter: PhylopicFilter {
                licenses: vec!["CC0-1.0".to_string()],
                public_domain: true,
            },
        };
        let registry = SitePresetRegistry::builtin().unwrap();

        let value = run_phylopic(&runtime, &registry, &request).unwrap();

        assert_eq!(value["status"]["success"], true);
        mock.assert();
    }

    #[test]
    fn run_phylopic_batch_posts_ids_and_filter() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v3/phylopic/batch")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "taxon_ids": ["9606", "9605"],
                "taxonomy": "ncbi",
                "license": null,
                "public_domain": true,
            })))
            .with_status(200)
            .with_body(r#"{"status":{"success":true},"results":{}}"#)
            .create();
        let runtime = RuntimeConfig {
            api_base: server.url(),
            ..RuntimeConfig::default()
        };
        let request = PhylopicRequest::Batch {
            taxon_ids: vec!["9606".to_string(), "9605".to_string()],
            taxonomy: "ncbi".to_string(),
            filter: PhylopicFilter {
                licenses: Vec::new(),
                public_domain: true,
            },
        };
        let registry = SitePresetRegistry::builtin().unwrap();

        let value = run_phylopic(&runtime, &registry, &request).unwrap();

        assert_eq!(value["status"]["success"], true);
        mock.assert();
    }

    #[test]
    fn v3_query_input_from_v2_translates_expression_and_fields() {
        let request = QueryRequest {
            result: "taxon".to_string(),
            query: Some("tax_tree(Mammalia)".to_string()),
            fields: vec!["genome_size".to_string()],
            size: None,
        };

        let documents = v3_query_input_from_v2(&request).unwrap();

        let query = search_query_from_yaml(&documents.query_yaml).unwrap();
        let taxa = query.identifiers.taxa.expect("taxa filter");
        assert_eq!(taxa.names, vec!["Mammalia".to_string()]);
        assert_eq!(taxa.filter_type, TaxonFilterType::Tree);
        assert_eq!(query.attributes.fields[0].name, "genome_size");
    }

    #[test]
    fn describe_input_names_taxa_and_fields() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/api/v3/metadata/fields")
            .match_query(mockito::Matcher::UrlEncoded(
                "result".into(),
                "taxon".into(),
            ))
            .with_status(200)
            .with_body(r#"{"fields":{"genome_size":{"type":"long"}}}"#)
            .create();
        let cache_dir = tempfile::tempdir().unwrap();
        let metadata = MetadataOptions {
            cache_root: cache_dir.path().to_path_buf(),
            refresh: false,
        };
        let runtime = RuntimeConfig {
            api_base: format!("{}/api", server.url()),
            ..RuntimeConfig::default()
        };
        let registry = SitePresetRegistry::builtin().unwrap();
        let documents = SplitQueryDocument {
            query_yaml: "index: taxon\ntaxa: [Mammalia]\ntaxon_filter_type: tree\nfields:\n  - name: genome_size\n".to_string(),
            params_yaml: "{}\n".to_string(),
        };

        let text = describe_input(&runtime, &registry, &documents, &metadata, false).unwrap();

        assert!(text.contains("Mammalia"), "{text}");
        assert!(text.contains("genome size"), "{text}");
        mock.assert();
    }

    #[test]
    fn search_query_from_yaml_parses_v3_body() {
        let query_yaml = "index: taxon\ntaxa: [Mammalia]\ntaxon_filter_type: tree\n";

        let query = search_query_from_yaml(query_yaml).unwrap();

        assert_eq!(query.index.as_str(), "taxon");
        let taxa = query.identifiers.taxa.expect("taxa filter");
        assert_eq!(taxa.names, vec!["Mammalia".to_string()]);
    }

    #[test]
    fn read_yaml_argument_returns_inline_text() {
        let text = "index: taxon\n";

        assert_eq!(read_yaml_argument(text).unwrap(), text);
    }

    #[test]
    fn read_yaml_argument_reads_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("query.yaml");
        std::fs::write(&path, "index: taxon\n").unwrap();

        let text = read_yaml_argument(path.to_str().unwrap()).unwrap();

        assert_eq!(text, "index: taxon\n");
    }

    #[test]
    fn read_yaml_argument_rejects_a_missing_yaml_path() {
        let err = read_yaml_argument("no-such-query.yaml").unwrap_err();

        assert!(err.to_string().contains("YAML file not found"), "{err}");
    }

    #[test]
    fn v3_query_input_splits_a_combined_document() {
        let documents = v3_query_input(Some("index: taxon\nsize: 5\n"), None, "taxon").unwrap();
        assert!(SearchQuery::from_yaml(&documents.query_yaml).is_ok());
        assert_eq!(
            QueryParams::from_yaml(&documents.params_yaml).unwrap().size,
            5
        );
    }

    #[test]
    fn v3_query_input_merges_a_separate_params_document() {
        let documents = v3_query_input(Some("index: taxon\n"), Some("size: 7\n"), "taxon").unwrap();
        assert_eq!(
            QueryParams::from_yaml(&documents.params_yaml).unwrap().size,
            7
        );
    }

    #[test]
    fn v3_query_input_defaults_to_search_all_on_the_index() {
        let documents = v3_query_input(None, None, "assembly").unwrap();
        let query = search_query_from_yaml(&documents.query_yaml).unwrap();
        assert_eq!(query.index.as_str(), "assembly");
        assert!(query.identifiers.taxa.is_none());
    }

    #[test]
    fn v3_query_input_accepts_params_without_a_query() {
        let documents = v3_query_input(None, Some("size: 3\n"), "taxon").unwrap();
        assert_eq!(
            QueryParams::from_yaml(&documents.params_yaml).unwrap().size,
            3
        );
    }

    #[test]
    fn post_v3_sends_query_and_params_documents() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v3/search")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "query_yaml": "index: taxon\n",
                "params_yaml": "size: 5\n",
            })))
            .with_status(200)
            .with_body(r#"{"status":{"success":true}}"#)
            .create();
        let documents = SplitQueryDocument {
            query_yaml: "index: taxon\n".to_string(),
            params_yaml: "size: 5\n".to_string(),
        };

        let value = post_v3(&server.url(), "search", &documents).unwrap();

        assert_eq!(value["status"]["success"], true);
        mock.assert();
    }

    #[test]
    fn with_overrides_replaces_yaml_params_and_keeps_query() {
        let input =
            v3_query_input(Some("index: taxon\nsize: 5\npage: 1\n"), None, "taxon").unwrap();
        let overrides = QueryOverrides {
            size: Some(10),
            page: Some(2),
            sort_order: Some("desc".to_string()),
            tidy: Some(true),
            ..QueryOverrides::default()
        };

        let documents = apply_overrides(input, &overrides).unwrap();
        let params = QueryParams::from_yaml(&documents.params_yaml).unwrap();
        assert_eq!((params.size, params.page, params.tidy), (10, 2, true));
        assert!(SearchQuery::from_yaml(&documents.query_yaml).is_ok());
    }

    #[test]
    fn with_overrides_rejects_an_invalid_params_value() {
        let input = v3_query_input(Some("index: taxon\n"), None, "taxon").unwrap();
        let overrides = QueryOverrides {
            sort_order: Some("sideways".to_string()),
            ..QueryOverrides::default()
        };

        assert!(apply_overrides(input, &overrides).is_err());
    }

    #[test]
    fn with_overrides_without_flags_leaves_input_unchanged() {
        let input = v3_query_input(Some("index: taxon\nsize: 5\n"), None, "taxon").unwrap();

        let unchanged = apply_overrides(input.clone(), &QueryOverrides::default()).unwrap();

        assert_eq!(unchanged, input);
    }

    #[test]
    fn with_overrides_sets_taxa_and_rank_on_the_query() {
        let input = v3_query_input(None, None, "taxon").unwrap();
        let overrides = QueryOverrides {
            taxa: vec!["Mammalia".to_string(), "Aves".to_string()],
            taxon_rank: Some("species".to_string()),
            ..QueryOverrides::default()
        };

        let documents = apply_overrides(input, &overrides).unwrap();
        let query = search_query_from_yaml(&documents.query_yaml).unwrap();
        let taxa = query.identifiers.taxa.unwrap();
        assert_eq!(taxa.names, vec!["Mammalia", "Aves"]);
        assert_eq!(query.identifiers.rank.as_deref(), Some("species"));
    }

    #[test]
    fn with_overrides_replaces_taxa_and_keeps_the_filter_type() {
        let input = v3_query_input(
            Some("index: taxon\ntaxa: [Lepidoptera]\ntaxon_filter_type: tree\n"),
            None,
            "taxon",
        )
        .unwrap();
        let overrides = QueryOverrides {
            taxa: vec!["Mammalia".to_string()],
            ..QueryOverrides::default()
        };

        let documents = apply_overrides(input, &overrides).unwrap();
        let taxa = search_query_from_yaml(&documents.query_yaml)
            .unwrap()
            .identifiers
            .taxa
            .unwrap();
        assert_eq!(taxa.names, vec!["Mammalia"]);
        assert_eq!(taxa.filter_type, TaxonFilterType::Tree);
    }

    #[test]
    fn with_overrides_sets_taxonomy_in_params() {
        let input = v3_query_input(None, None, "taxon").unwrap();
        let overrides = QueryOverrides {
            taxonomy: Some("ott".to_string()),
            ..QueryOverrides::default()
        };

        let documents = apply_overrides(input, &overrides).unwrap();
        let params = QueryParams::from_yaml(&documents.params_yaml).unwrap();
        assert_eq!(params.taxonomy, "ott");
    }

    #[test]
    fn with_overrides_sets_assemblies_samples_and_taxon_filter() {
        let input = v3_query_input(None, None, "taxon").unwrap();
        let overrides = QueryOverrides {
            taxa: vec!["Mammalia".to_string()],
            taxon_filter: Some(TaxonFilterType::Lineage),
            assemblies: vec!["GCF_000002305.6".to_string()],
            samples: vec!["SAMN1".to_string(), "SAMN2".to_string()],
            ..QueryOverrides::default()
        };

        let documents = apply_overrides(input, &overrides).unwrap();
        let query = search_query_from_yaml(&documents.query_yaml).unwrap();
        let taxa = query.identifiers.taxa.unwrap();
        assert_eq!(taxa.filter_type, TaxonFilterType::Lineage);
        assert_eq!(query.identifiers.assemblies, vec!["GCF_000002305.6"]);
        assert_eq!(query.identifiers.samples, vec!["SAMN1", "SAMN2"]);
    }

    #[test]
    fn with_overrides_rejects_taxon_filter_without_taxa() {
        let input = v3_query_input(None, None, "taxon").unwrap();
        let overrides = QueryOverrides {
            taxon_filter: Some(TaxonFilterType::Tree),
            ..QueryOverrides::default()
        };

        let result = apply_overrides(input, &overrides);

        assert!(result.unwrap_err().to_string().contains("--taxon-filter"));
    }

    #[test]
    fn with_overrides_sets_return_fields_ranks_and_names() {
        let input = v3_query_input(
            Some("index: taxon\nfields:\n  - name: genome_size\n"),
            None,
            "taxon",
        )
        .unwrap();
        let overrides = QueryOverrides {
            fields: vec!["assembly_span".to_string(), "gc_percent".to_string()],
            ranks: vec!["phylum".to_string()],
            names: vec!["scientific_name".to_string()],
            ..QueryOverrides::default()
        };

        let documents = apply_overrides(input, &overrides).unwrap();
        let query = search_query_from_yaml(&documents.query_yaml).unwrap();
        let field_names: Vec<&str> = query
            .attributes
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect();
        assert_eq!(field_names, vec!["assembly_span", "gc_percent"]);
        assert_eq!(query.attributes.ranks, vec!["phylum"]);
        assert_eq!(query.attributes.names, vec!["scientific_name"]);
    }

    #[test]
    fn with_overrides_replaces_attribute_filters() {
        let input = v3_query_input(
            Some("index: taxon\nattributes:\n  - name: busco_completeness\n"),
            None,
            "taxon",
        )
        .unwrap();
        let overrides = QueryOverrides {
            filters: vec!["genome_size<3G".to_string(), "assembly_level".to_string()],
            ..QueryOverrides::default()
        };

        let documents = apply_overrides(input, &overrides).unwrap();
        let query = search_query_from_yaml(&documents.query_yaml).unwrap();
        let names: Vec<&str> = query
            .attributes
            .attributes
            .iter()
            .map(|attribute| attribute.name.as_str())
            .collect();
        assert_eq!(names, vec!["genome_size", "assembly_level"]);
    }

    #[test]
    fn with_overrides_rejects_an_unparseable_filter() {
        let input = v3_query_input(None, None, "taxon").unwrap();
        let overrides = QueryOverrides {
            filters: vec!["min(genome_size".to_string()],
            ..QueryOverrides::default()
        };

        let result = apply_overrides(input, &overrides);

        assert!(result.unwrap_err().to_string().contains("--filter"));
    }

    #[test]
    fn parse_taxon_filter_accepts_the_three_types() {
        assert_eq!(parse_taxon_filter("name").unwrap(), TaxonFilterType::Name);
        assert_eq!(parse_taxon_filter("tree").unwrap(), TaxonFilterType::Tree);
        assert_eq!(
            parse_taxon_filter("lineage").unwrap(),
            TaxonFilterType::Lineage
        );
        assert!(parse_taxon_filter("other").is_err());
    }

    #[test]
    fn plot_request_body_sends_config_under_endpoint_key() {
        let input = v3_query_input(None, None, "taxon").unwrap();
        let options = PlotOptions {
            include_plot_spec: true,
            display: Some(serde_json::json!({"title": "My plot"})),
        };

        let body = plot_request_body("positional", &input, "report: oxford\n", &options);

        assert_eq!(body["positional_yaml"], "report: oxford\n");
        assert!(body.get("report_yaml").is_none());
        assert_eq!(body["include_plot_spec"], true);
        assert_eq!(body["display"]["title"], "My plot");
        assert!(body["query_yaml"]
            .as_str()
            .unwrap()
            .contains("index: taxon"));
    }

    #[test]
    fn plot_request_body_omits_unset_plot_options() {
        let input = v3_query_input(None, None, "taxon").unwrap();

        let body = plot_request_body(
            "report",
            &input,
            "report: histogram\n",
            &PlotOptions::default(),
        );

        assert!(body.get("include_plot_spec").is_none());
        assert!(body.get("display").is_none());
    }

    #[test]
    fn plot_options_from_flags_parses_display_json() {
        let options = PlotOptions::from_flags(true, Some(r#"{"title":"My plot"}"#)).unwrap();

        assert!(options.include_plot_spec);
        assert_eq!(
            options.display,
            Some(serde_json::json!({"title": "My plot"}))
        );
        assert!(PlotOptions::from_flags(false, Some("{bad")).is_err());
    }

    #[test]
    fn search_query_from_yaml_rejects_malformed_yaml() {
        let err = search_query_from_yaml("index: [").unwrap_err();

        assert!(err.to_string().contains("parsing v3 query YAML"), "{err}");
    }
}
