// Integration tests for genomehubs-api v3 endpoints: /record, /lookup, /batchSearch
//
// These tests run against a live Elasticsearch instance when `config/es_integration.toml`
// exists and contains ES connection details. The tests use the live ES data to verify
// endpoint functionality.
//
// Skipped if config is not found.

use std::fs;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct EsConfig {
    base_url: String,
    #[allow(dead_code)]
    default_result: Option<String>,
    default_index: Option<String>,
    index_suffix: Option<String>,
    #[allow(dead_code)]
    hub_name: Option<String>,
}

fn load_es_config() -> Option<EsConfig> {
    let cfg_path = std::env::var("ES_INTEGRATION_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("config/es_integration.toml"));

    if !cfg_path.exists() {
        eprintln!(
            "skipping API endpoint tests; config not found at {:?}",
            cfg_path
        );
        return None;
    }

    let raw = fs::read_to_string(&cfg_path).expect("reading es_integration.toml");
    let cfg: EsConfig = toml::from_str(&raw).expect("parsing es_integration.toml");
    Some(cfg)
}

/// Get the API server base URL (defaults to localhost:3000)
fn get_api_base_url() -> String {
    std::env::var("API_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
}

/// Fetch first taxon_id from the ES index for use in subsequent tests
fn fetch_first_taxon_id(base_url: &str, index: &str) -> Option<String> {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let client = reqwest::Client::new();
        let url = format!("{}/{}/_search", base_url.trim_end_matches('/'), index);
        let body = json!({
            "size": 1,
            "query": { "match_all": {} },
            "_source": ["taxon_id"]
        });

        if let Ok(resp) = client.post(&url).json(&body).send().await {
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                if let Some(hits) = data["hits"]["hits"].as_array() {
                    if let Some(first_hit) = hits.first() {
                        if let Some(taxon_id) = first_hit["_source"]["taxon_id"].as_str() {
                            return Some(taxon_id.to_string());
                        }
                    }
                }
            }
        }
        None
    })
}

#[test]
fn api_lookup_get_with_search_term() {
    let cfg = match load_es_config() {
        Some(c) => c,
        None => return,
    };

    let api_base = get_api_base_url();
    let _index = cfg.default_index.unwrap_or_else(|| "taxon".to_string());
    let _suffix = cfg.index_suffix.unwrap_or_default();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let client = reqwest::Client::new();

        // Test 1: Basic lookup with search term
        let url = format!(
            "{}/api/v3/lookup?searchTerm=Homo&result=taxon&size=5",
            api_base
        );
        let resp = client.get(&url).send().await.expect("lookup request");
        assert_eq!(resp.status(), 200, "lookup status should be 200");

        let body: serde_json::Value = resp.json().await.expect("parsing lookup response");
        assert!(
            body["status"]["success"].as_bool().unwrap_or(false),
            "status.success should be true"
        );
        assert!(body["results"].is_array(), "results should be an array");

        // Verify result structure
        if let Some(results) = body["results"].as_array() {
            for result in results {
                assert!(result["id"].is_string(), "result.id should be a string");
                assert!(result["name"].is_string(), "result.name should be a string");
                assert!(
                    result["reason"].is_string(),
                    "result.reason should be a string"
                );
            }
        }
    });
}

#[test]
fn api_lookup_empty_search_term() {
    let _cfg = match load_es_config() {
        Some(c) => c,
        None => return,
    };

    let api_base = get_api_base_url();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let client = reqwest::Client::new();

        // Empty search term should return empty results gracefully
        let url = format!("{}/api/v3/lookup?searchTerm=&result=taxon", api_base);
        let resp = client.get(&url).send().await.expect("lookup request");
        assert_eq!(resp.status(), 200, "lookup status should be 200");

        let body: serde_json::Value = resp.json().await.expect("parsing lookup response");
        assert!(
            body["status"]["success"].as_bool().unwrap_or(false),
            "status should be successful"
        );
    });
}

#[test]
fn api_record_get_with_id() {
    let cfg = match load_es_config() {
        Some(c) => c,
        None => return,
    };

    let api_base = get_api_base_url();
    let index = cfg.default_index.unwrap_or_else(|| "taxon".to_string());
    let suffix = cfg.index_suffix.unwrap_or_default();
    let full_index = format!("{}{}", index, suffix);

    // Fetch a real taxon_id to use in the test
    let taxon_id = match fetch_first_taxon_id(&cfg.base_url, &full_index) {
        Some(id) => id,
        None => {
            eprintln!("skipping record test; could not fetch taxon_id from ES");
            return;
        }
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let client = reqwest::Client::new();

        // Test 1: Fetch single record by ID
        let url = format!(
            "{}/api/v3/record?recordId={}&result=taxon",
            api_base, taxon_id
        );
        let resp = client.get(&url).send().await.expect("record request");
        assert_eq!(resp.status(), 200, "record status should be 200");

        let body: serde_json::Value = resp.json().await.expect("parsing record response");
        assert!(
            body["status"]["success"].as_bool().unwrap_or(false),
            "status.success should be true"
        );
        assert!(body["records"].is_array(), "records should be an array");

        // Verify record structure
        if let Some(records) = body["records"].as_array() {
            for record in records {
                assert!(
                    record["recordId"].is_string(),
                    "record.recordId should be a string"
                );
                assert!(
                    record["result"].is_string(),
                    "record.result should be a string"
                );
                assert!(
                    record["record"].is_object(),
                    "record.record should be an object"
                );
            }
        }
    });
}

#[test]
fn api_record_multiple_ids() {
    let cfg = match load_es_config() {
        Some(c) => c,
        None => return,
    };

    let api_base = get_api_base_url();
    let index = cfg.default_index.unwrap_or_else(|| "taxon".to_string());
    let suffix = cfg.index_suffix.unwrap_or_default();
    let full_index = format!("{}{}", index, suffix);

    // Fetch multiple taxon_ids from ES
    let rt = tokio::runtime::Runtime::new().unwrap();
    let prefixed_ids = rt.block_on(async {
        let client = reqwest::Client::new();
        let url = format!(
            "{}/{}/_search",
            cfg.base_url.trim_end_matches('/'),
            full_index
        );
        let body = json!({
            "size": 3,
            "query": { "match_all": {} },
            "_source": ["taxon_id"]
        });

        if let Ok(resp) = client.post(&url).json(&body).send().await {
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                if let Some(hits) = data["hits"]["hits"].as_array() {
                    hits.iter()
                        .filter_map(|h| h["_source"]["taxon_id"].as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    });

    if prefixed_ids.is_empty() {
        eprintln!("skipping multiple records test; could not fetch taxon_ids from ES");
        return;
    }

    // Test 1: With prefixed IDs (taxon-9612)
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let client = reqwest::Client::new();

        let ids_param = prefixed_ids.join(",");
        let url = format!(
            "{}/api/v3/record?recordId={}&result=taxon",
            api_base, ids_param
        );
        let resp = client.get(&url).send().await.expect("record request");
        assert_eq!(resp.status(), 200, "record status should be 200");

        let body: serde_json::Value = resp.json().await.expect("parsing record response");
        assert!(
            body["status"]["success"].as_bool().unwrap_or(false),
            "status.success should be true"
        );

        if let Some(records) = body["records"].as_array() {
            assert!(
                !records.is_empty(),
                "should have returned at least one record"
            );
            // Verify that we got records for the requested IDs
            assert_eq!(
                records.len(),
                prefixed_ids.len(),
                "should return same number of records as requested"
            );
        }
    });

    // Test 2: With unprefixed IDs (9612) — tests auto-prefixing behavior
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let client = reqwest::Client::new();

        // Strip the "taxon-" prefix from the IDs to test auto-prefixing
        let unprefixed_ids: Vec<String> = prefixed_ids
            .iter()
            .filter_map(|id| id.strip_prefix("taxon-").map(|s| s.to_string()))
            .collect();

        if !unprefixed_ids.is_empty() {
            let ids_param = unprefixed_ids.join(",");
            let url = format!(
                "{}/api/v3/record?recordId={}&result=taxon",
                api_base, ids_param
            );
            let resp = client.get(&url).send().await.expect("record request");
            assert_eq!(resp.status(), 200, "record status should be 200");

            let body: serde_json::Value = resp.json().await.expect("parsing record response");
            assert!(
                body["status"]["success"].as_bool().unwrap_or(false),
                "status.success should be true with unprefixed IDs"
            );

            if let Some(records) = body["records"].as_array() {
                assert!(
                    !records.is_empty(),
                    "should have returned records even with unprefixed IDs"
                );
                assert_eq!(
                    records.len(),
                    unprefixed_ids.len(),
                    "should return same number of records as unprefixed IDs requested"
                );
            }
        }
    });
}

#[test]
fn api_batchsearch_multiple_queries() {
    let _cfg = match load_es_config() {
        Some(c) => c,
        None => return,
    };

    let api_base = get_api_base_url();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let client = reqwest::Client::new();

        // Test: Batch search with multiple queries
        let url = format!("{}/api/v3/batchSearch", api_base);
        let body = json!({
            "searches": [
                {
                    "query_yaml": "---\nindex: taxon\n",
                    "params_yaml": "---\npage: 1\nsize: 10\n"
                },
                {
                    "query_yaml": "---\nindex: taxon\n",
                    "params_yaml": "---\npage: 1\nsize: 5\n"
                }
            ]
        });

        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .expect("batchSearch request");
        assert_eq!(resp.status(), 200, "batchSearch status should be 200");

        let response: serde_json::Value = resp.json().await.expect("parsing batchSearch response");
        assert!(
            response["status"]["success"].as_bool().unwrap_or(false),
            "status.success should be true"
        );
        assert!(response["results"].is_array(), "results should be an array");

        // Verify result structure
        if let Some(results) = response["results"].as_array() {
            assert_eq!(results.len(), 2, "should have 2 result entries");
            for result in results {
                assert!(
                    result["status"]["success"].as_bool().unwrap_or(false),
                    "each result status should be successful"
                );
                assert!(
                    result["count"].is_number(),
                    "result.count should be a number"
                );
                assert!(result["hits"].is_array(), "result.hits should be an array");
            }
        }
    });
}

#[test]
fn api_batchsearch_max_queries_limit() {
    let _cfg = match load_es_config() {
        Some(c) => c,
        None => return,
    };

    let api_base = get_api_base_url();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let client = reqwest::Client::new();

        // Test: Exceed max queries limit (100+)
        let mut searches = Vec::new();
        for i in 0..101 {
            searches.push(json!({
                "query_yaml": "---\nindex: taxon\n",
                "params_yaml": format!("---\npage: {}\nsize: 1\n", i)
            }));
        }

        let url = format!("{}/api/v3/batchSearch", api_base);
        let body = json!({ "searches": searches });

        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .expect("batchSearch request");
        assert_eq!(resp.status(), 200, "batchSearch status should be 200");

        let response: serde_json::Value = resp.json().await.expect("parsing batchSearch response");
        // Should fail or return error status due to exceeding max
        assert!(
            !response["status"]["success"].as_bool().unwrap_or(true),
            "should fail when exceeding max queries"
        );
    });
}

#[test]
fn api_batchsearch_invalid_yaml() {
    let _cfg = match load_es_config() {
        Some(c) => c,
        None => return,
    };

    let api_base = get_api_base_url();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let client = reqwest::Client::new();

        // Test: Invalid YAML should be handled gracefully
        let url = format!("{}/api/v3/batchSearch", api_base);
        let body = json!({
            "searches": [
                {
                    "query_yaml": "invalid: [yaml: content",
                    "params_yaml": "---\npage: 1\nsize: 10\n"
                }
            ]
        });

        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .expect("batchSearch request");
        assert_eq!(resp.status(), 200, "batchSearch status should be 200");

        let response: serde_json::Value = resp.json().await.expect("parsing batchSearch response");
        // Should fail due to invalid query_yaml
        assert!(
            !response["status"]["success"].as_bool().unwrap_or(true),
            "should fail with invalid YAML"
        );
    });
}
