use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

// Integration test that runs against a live Elasticsearch instance when
// `config/es_integration.toml` (or path set in `ES_INTEGRATION_CONFIG`) exists.
// If no config is present the test is skipped.

#[derive(Deserialize)]
struct EsConfig {
    base_url: String,
    default_result: Option<String>,
    index_suffix: Option<String>,
}

#[test]
fn live_elasticsearch_count_integration() {
    // Locate config: env override or default path
    let cfg_path = std::env::var("ES_INTEGRATION_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("config/es_integration.toml"));

    if !cfg_path.exists() {
        eprintln!("skipping live ES integration test; config not found at {:?}", cfg_path);
        return;
    }

    let raw = fs::read_to_string(&cfg_path).expect("reading es_integration.toml");
    let cfg: EsConfig = toml::from_str(&raw).expect("parsing es_integration.toml");

    let mut params = HashMap::new();
    // Use result param so the adapter determines the base index
    let result = cfg.default_result.unwrap_or_else(|| "taxon".to_string());
    params.insert("result".to_string(), result.clone());
    // Basic smoke query — rely on adapter to convert taxa/fields into a minimal body
    params.insert("taxa".to_string(), "Mammalia".to_string());

    // Parse params to obtain the SearchQuery index and then build the final
    // index name applying `index_suffix` from config when provided.
    let (search_query, _qp) = crate::core::query::adapter::parse_url_params(&params)
        .expect("parsing URL params");

    let base_index = match search_query.index {
        crate::core::query::SearchIndex::Taxon => "taxon",
        crate::core::query::SearchIndex::Assembly => "assembly",
        crate::core::query::SearchIndex::Sample => "sample",
    };

    let suffix = cfg.index_suffix.unwrap_or_default();
    let full_index = format!("{}{}", base_index, suffix);

    // Call the library function under test
    let count = cli_generator::core::count::count_docs_from_url_params(&cfg.base_url, &full_index, &params)
        .expect("count_docs_from_url_params against live ES");

    eprintln!("live ES count for {:?}: {}", cfg.index, count);
    // We can't assert an exact value; assert it returns a non-negative number.
    assert!(count >= 0);
}
