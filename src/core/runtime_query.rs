//! Runtime search and count requests against any GenomeHubs API.
//!
//! The API's `search` and `count` GET endpoints take a single `url` parameter
//! holding a GoaT-style query string (`result=…&query=…&fields=…&size=…`).
//! This module builds that inner string and sends it.

use std::collections::HashMap;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::core::config::{RuntimeConfig, SitePresetRegistry};
use crate::core::fetch::FieldDef;
use crate::core::metadata::{load_result_fields, validate_fields, MetadataOptions};
use crate::core::query::SearchQuery;
use genomehubs_query::describe::QueryDescriber;

/// Index used when neither `--result` nor the preset provides one.
const FALLBACK_RESULT_INDEX: &str = "taxon";

/// Base used only to percent-encode query parameters into a query string.
const ENCODING_BASE: &str = "http://localhost/";

/// Parameters for a search or count request.
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

/// Send a GET to `{api_base}/v3/{endpoint}` with the given query parameters.
fn get_json(api_base: &str, endpoint: &str, params: &[(&str, String)]) -> Result<Value> {
    let url = format!("{}/v3/{endpoint}", api_base.trim_end_matches('/'));
    let response = reqwest::blocking::Client::new()
        .get(&url)
        .query(params)
        .send()
        .with_context(|| format!("HTTP GET {url}"))?
        .error_for_status()
        .with_context(|| format!("non-2xx response from {url}"))?;
    response.json().context("deserialising JSON response")
}

/// Send a GET to `{api_base}/v3/{endpoint}` with the inner query in `url`.
pub fn api_get(api_base: &str, endpoint: &str, inner_query: &str) -> Result<Value> {
    get_json(api_base, endpoint, &[("url", inner_query.to_string())])
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

/// Validate fields against the index metadata, then run the request.
///
/// `endpoint` is `search` or `count`.
pub fn run_query(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    endpoint: &str,
    request: &QueryRequest,
    metadata: &MetadataOptions,
) -> Result<Value> {
    let api_base = runtime.require_api_base(registry)?;
    validate_fields(
        api_base,
        &runtime.site_name,
        &request.result,
        &request.fields,
        metadata,
    )?;
    let inner_query = build_query_string(request)?;
    api_get(api_base, endpoint, &inner_query)
}

/// Convert a request's parameters into `SearchQuery` YAML via the canonical v2 URL converter.
///
/// The same converter backs `QueryBuilder.from_v2_url()` in the SDKs.
pub fn query_yaml_from_request(request: &QueryRequest) -> Result<String> {
    let inner_query = build_query_string(request)?;
    let url = format!("{ENCODING_BASE}search?{inner_query}");
    let (query_yaml, _params_yaml) =
        genomehubs_query::query::query_yaml_from_url_params(&url).map_err(anyhow::Error::msg)?;
    Ok(query_yaml)
}

/// Convert a request's parameters into a `SearchQuery` via the canonical v2 URL converter.
pub fn search_query_from_request(request: &QueryRequest) -> Result<SearchQuery> {
    let query_yaml = query_yaml_from_request(request)?;
    serde_yaml::from_str(&query_yaml).context("parsing SearchQuery YAML from v2 URL converter")
}

/// Input for `describe`: legacy v2 parameters or a v3 `SearchQuery` YAML body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryInput {
    /// Legacy v2 parameters (`--query`), converted by the canonical URL converter.
    V2(QueryRequest),
    /// v3 `SearchQuery` YAML body (`--query-yaml`).
    V3Yaml(String),
}

/// Parse a v3 `SearchQuery` YAML body.
pub fn search_query_from_yaml(query_yaml: &str) -> Result<SearchQuery> {
    SearchQuery::from_yaml(query_yaml).context("parsing v3 query YAML")
}

/// Return the metadata index and `SearchQuery` YAML for a query input.
///
/// v2 input uses the request's `result`; v3 input uses the YAML's `index`.
fn query_yaml_target(input: &QueryInput) -> Result<(String, String)> {
    match input {
        QueryInput::V2(request) => Ok((request.result.clone(), query_yaml_from_request(request)?)),
        QueryInput::V3Yaml(query_yaml) => {
            let query = search_query_from_yaml(query_yaml)?;
            Ok((query.index.as_str().to_string(), query_yaml.clone()))
        }
    }
}

/// Describe a query in prose, naming fields from the index metadata.
pub fn describe_input(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    input: &QueryInput,
    metadata: &MetadataOptions,
    verbose: bool,
) -> Result<String> {
    let (result, query_yaml) = query_yaml_target(input)?;
    let query = search_query_from_yaml(&query_yaml)?;
    let api_base = runtime.require_api_base(registry)?;
    let fields = load_result_fields(api_base, &runtime.site_name, &result, metadata)?;
    let describer = QueryDescriber::new(field_map(fields));
    Ok(if verbose {
        describer.describe_verbose(&query)
    } else {
        describer.describe_concise(&query)
    })
}

/// Validate a query input against the index metadata and return the error messages.
///
/// An empty list means the query is valid. Uses the same field metadata as `describe`.
pub fn validate_input(
    runtime: &RuntimeConfig,
    registry: &SitePresetRegistry,
    input: &QueryInput,
    metadata: &MetadataOptions,
) -> Result<Vec<String>> {
    let (result, query_yaml) = query_yaml_target(input)?;
    let api_base = runtime.require_api_base(registry)?;
    let fields = load_result_fields(api_base, &runtime.site_name, &result, metadata)?;
    Ok(genomehubs_query::validation::validate_query_for_fields(
        &query_yaml,
        &fields,
    ))
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
    fn api_get_sends_inner_query_in_url_parameter() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/v3/search")
            .match_query(mockito::Matcher::UrlEncoded(
                "url".into(),
                "result=taxon".into(),
            ))
            .with_status(200)
            .with_body(r#"{"status":{"success":true}}"#)
            .create();

        let value = api_get(&server.url(), "search", "result=taxon").unwrap();

        assert_eq!(value["status"]["success"], true);
        mock.assert();
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
    fn search_query_from_request_converts_v2_query_fragment() {
        let request = QueryRequest {
            result: "taxon".to_string(),
            query: Some("tax_tree(Mammalia)".to_string()),
            fields: vec!["genome_size".to_string()],
            size: None,
        };

        let query = search_query_from_request(&request).unwrap();

        let taxa = query.identifiers.taxa.expect("taxa filter");
        assert_eq!(taxa.names, vec!["Mammalia".to_string()]);
        assert!(matches!(
            taxa.filter_type,
            crate::core::query::TaxonFilterType::Tree
        ));
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
        let request = QueryRequest {
            result: "taxon".to_string(),
            query: Some("tax_tree(Mammalia)".to_string()),
            fields: vec!["genome_size".to_string()],
            size: None,
        };
        let input = QueryInput::V2(request);

        let text = describe_input(&runtime, &registry, &input, &metadata, false).unwrap();

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
    fn search_query_from_yaml_rejects_malformed_yaml() {
        let err = search_query_from_yaml("index: [").unwrap_err();

        assert!(err.to_string().contains("parsing v3 query YAML"), "{err}");
    }

    #[test]
    fn query_yaml_target_uses_v3_index_for_metadata() {
        let query_yaml = "index: assembly\n";
        let input = QueryInput::V3Yaml(query_yaml.to_string());

        let (result, yaml) = query_yaml_target(&input).unwrap();

        assert_eq!(result, "assembly");
        assert_eq!(yaml, query_yaml);
    }

    #[test]
    fn query_yaml_target_uses_v2_result_for_metadata() {
        let input = QueryInput::V2(QueryRequest {
            result: "taxon".to_string(),
            query: Some("tax_tree(Mammalia)".to_string()),
            fields: vec![],
            size: None,
        });

        let (result, _) = query_yaml_target(&input).unwrap();

        assert_eq!(result, "taxon");
    }

    #[test]
    fn api_get_reports_http_errors() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/v3/count")
            .match_query(mockito::Matcher::Any)
            .with_status(500)
            .with_body("Internal Server Error")
            .create();

        let err = api_get(&server.url(), "count", "result=taxon")
            .unwrap_err()
            .to_string();

        assert!(err.contains("non-2xx response"));
        mock.assert();
    }
}
