use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

use cli_generator::core::attr_types;
use cli_generator::core::count::count_docs_from_url_params;
use cli_generator::core::query::adapter;
use cli_generator::core::query::SearchIndex;
use cli_generator::core::query::{Attribute, AttributeOperator, AttributeValue, SortOrder};
use cli_generator::core::query_builder;
use serde::Deserialize;

fn split_args_into_map() -> HashMap<String, String> {
    // Very small arg parser: --key value pairs
    let mut params = HashMap::new();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--debug" {
            params.insert("debug".to_string(), "1".to_string());
            continue;
        }
        if arg.starts_with("--") {
            let key = arg.trim_start_matches("--").to_string();
            if let Some(val) = args.next() {
                params.insert(key, val);
            }
        }
    }
    params
}

#[derive(Deserialize)]
struct EsConfig {
    base_url: String,
    default_result: Option<String>,
    index_suffix: Option<String>,
}

#[allow(clippy::cognitive_complexity)]
fn main() {
    // Find config
    let cfg_path = env::var("ES_INTEGRATION_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let mut p = PathBuf::from("config/es_integration.toml");
            if !p.exists() {
                p = PathBuf::from("config/es_integration.toml.example");
            }
            p
        });

    let raw = match fs::read_to_string(&cfg_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to read config at {:?}: {}", cfg_path, e);
            std::process::exit(2);
        }
    };

    let cfg: EsConfig = match toml::from_str(&raw) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to parse TOML config: {}", e);
            std::process::exit(2);
        }
    };

    let mut params = split_args_into_map();

    // If no params provided, add a sensible default
    if params.is_empty() {
        params.insert(
            "result".to_string(),
            cfg.default_result
                .clone()
                .unwrap_or_else(|| "taxon".to_string()),
        );
        params.insert("taxa".to_string(), "Mammalia".to_string());
    }

    // Determine index from parsed `result`
    let (mut search_query, qp) = match adapter::parse_url_params(&params) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to parse params: {}", e);
            std::process::exit(2);
        }
    };

    // Ensure assembly index queries include a default `data_freeze` attribute
    // when not already present — matches legacy server behaviour where the
    // attributes metadata declares a `data_freeze` field for assembly results.
    if let SearchIndex::Assembly = search_query.index {
        let has_data_freeze = search_query
            .attributes
            .attributes
            .iter()
            .any(|a| a.name == "data_freeze");
        if !has_data_freeze {
            search_query.attributes.attributes.push(Attribute {
                name: "data_freeze".to_string(),
                operator: Some(AttributeOperator::Eq),
                value: Some(AttributeValue::Single("latest".to_string())),
                modifier: Vec::new(),
            });
        }
    }

    let base_index = match search_query.index {
        SearchIndex::Taxon => "taxon",
        SearchIndex::Assembly => "assembly",
        SearchIndex::Sample => "sample",
    };

    let suffix = cfg.index_suffix.unwrap_or_default();
    let full_index = format!("{}{}", base_index, suffix);

    println!(
        "Running live query against {} at {}",
        full_index, cfg.base_url
    );

    let debug = params
        .get("debug")
        .map(|v| v != "0" && v != "false")
        .unwrap_or(false);

    if debug {
        // Build an ES-style search body using the query builder and POST to /_search
        let field_names: Vec<&str> = search_query
            .attributes
            .fields
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        // Build a simple query string from identifiers (taxa) if present
        let query_str = if let Some(taxa_ident) = &search_query.identifiers.taxa {
            let joined = taxa_ident.names.join(",");
            if joined.is_empty() {
                None
            } else {
                Some(format!("tax_name({})", joined))
            }
        } else {
            None
        };

        let rank_arg = search_query.identifiers.rank.as_deref();

        let attributes_slice = if search_query.attributes.attributes.is_empty() {
            None
        } else {
            Some(search_query.attributes.attributes.as_slice())
        };

        // names/ranks params (for taxon_names/class and lineage rank filtering)
        let names_vec: Vec<&str> = search_query
            .attributes
            .names
            .iter()
            .map(|s| s.as_str())
            .collect();
        let ranks_vec: Vec<&str> = search_query
            .attributes
            .ranks
            .iter()
            .map(|s| s.as_str())
            .collect();

        let sort_by_arg = qp.sort_by.as_deref();
        let sort_order_arg = match qp.sort_order {
            SortOrder::Desc => Some("desc"),
            _ => Some("asc"),
        };

        // Try to fetch attribute metadata to build attribute-aware inner_hits.
        let mut fetched_types =
            match attr_types::attr_types(&cfg.base_url, "attributes", base_index) {
                Ok((types_map, _syn)) => Some(types_map),
                Err(e) => {
                    eprintln!("Warning: failed to fetch attribute metadata from ES: {}", e);
                    None
                }
            };

        // If fetching from ES failed, try a local metadata stub for offline
        // development placed at `workdir/fixture_diffs/attribute_metadata.json`.
        if fetched_types.is_none() {
            let p = PathBuf::from("workdir/fixture_diffs/attribute_metadata.json");
            if p.exists() {
                match fs::read_to_string(&p) {
                    Ok(s) => match serde_json::from_str::<attr_types::TypesMap>(&s) {
                        Ok(tm) => {
                            eprintln!("Loaded attribute metadata from {:?}", p);
                            fetched_types = Some(tm);
                        }
                        Err(e) => {
                            eprintln!("Failed to parse attribute metadata file {:?}: {}", p, e)
                        }
                    },
                    Err(e) => eprintln!("Failed to read attribute metadata file {:?}: {}", p, e),
                }
            }
        }

        eprintln!(
            "DEBUG: names={:?} ranks={:?}",
            search_query.attributes.names, search_query.attributes.ranks
        );

        let body = match query_builder::build_search_body(
            query_str.as_deref(),
            if field_names.is_empty() {
                None
            } else {
                Some(field_names.as_slice())
            },
            None,
            attributes_slice,
            rank_arg,
            if names_vec.is_empty() {
                None
            } else {
                Some(names_vec.as_slice())
            },
            if ranks_vec.is_empty() {
                None
            } else {
                Some(ranks_vec.as_slice())
            },
            sort_by_arg,
            sort_order_arg,
            50,
            0,
            fetched_types.as_ref(),
            Some(base_index),
        ) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Failed to build search body: {}", e);
                std::process::exit(2);
            }
        };

        let url = format!(
            "{}/{}/_search",
            cfg.base_url.trim_end_matches('/'),
            full_index
        );
        println!("POST URL: {}", url);
        println!(
            "Request body:\n{}",
            serde_json::to_string_pretty(&body).unwrap()
        );

        let client = reqwest::blocking::Client::new();
        match client.post(&url).json(&body).send() {
            Ok(resp) => match resp.text() {
                Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(j) => println!(
                        "Full response:\n{}",
                        serde_json::to_string_pretty(&j).unwrap()
                    ),
                    Err(_) => println!("Full response (raw):\n{}", text),
                },
                Err(e) => {
                    eprintln!("Failed to read response body: {}", e);
                    std::process::exit(2);
                }
            },
            Err(e) => {
                eprintln!("HTTP request failed: {}", e);
                std::process::exit(2);
            }
        }
    } else {
        match count_docs_from_url_params(&cfg.base_url, &full_index, &params) {
            Ok(count) => println!("Count: {}", count),
            Err(e) => {
                eprintln!("Query failed: {}", e);
                std::process::exit(2);
            }
        }
    }
}
