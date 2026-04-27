use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Lightweight representation of an attribute metadata document from ES `_search` `_source`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeMeta {
    pub group: String,
    pub name: String,
    #[serde(rename = "type", default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub summary: Option<Value>,
    #[serde(default)]
    pub default_summary: Option<String>,
    #[serde(default)]
    pub return_type: Option<String>,
    #[serde(default)]
    pub synonyms: Vec<String>,
    #[serde(default)]
    pub processed_type: Option<String>,
    #[serde(default)]
    pub processed_summary: Option<String>,
    #[serde(default)]
    pub processed_simple: Option<String>,
}

pub type TypesMap = HashMap<String, HashMap<String, TypeMeta>>;
pub type SynonymsMap = HashMap<String, HashMap<String, String>>;

fn set_processed_type(meta: &mut TypeMeta) {
    if let Some(ref t) = meta.r#type {
        let t = t.as_str();
        if [
            "double",
            "float",
            "half_float",
            "scaled_float",
            "unsigned_long",
        ]
        .contains(&t)
            || t.ends_with("dp")
        {
            meta.processed_type = Some("float".to_string());
            return;
        }
        if ["long", "integer", "short", "byte"].contains(&t) {
            meta.processed_type = Some("integer".to_string());
            return;
        }
        if t == "keyword" {
            if let Some(ref summary) = meta.summary {
                if summary.is_array() {
                    if let Some(first) = summary.as_array().and_then(|a| a.first()) {
                        if first == "enum"
                            || (first == "primary"
                                && summary.as_array().and_then(|a| a.get(1))
                                    == Some(&Value::String("enum".to_string())))
                        {
                            meta.processed_type = Some("ordered_keyword".to_string());
                            return;
                        }
                    }
                } else if summary.is_string() && summary.as_str() == Some("enum") {
                    meta.processed_type = Some("ordered_keyword".to_string());
                    return;
                }
            }
            meta.processed_type = Some("keyword".to_string());
            return;
        }
        meta.processed_type = Some(t.to_string());
    }
}

fn set_processed_summary(meta: &mut TypeMeta) {
    let mut summary = meta.default_summary.clone();
    let simple = meta
        .return_type
        .clone()
        .unwrap_or_else(|| "value".to_string());
    if summary.is_none() {
        if meta.r#type.as_deref() == Some("keyword") {
            summary = Some("keyword_value.raw".to_string());
        } else if let Some(ref t) = meta.r#type {
            summary = Some(format!("{}_value", t));
        }
    }
    meta.processed_summary = summary;
    meta.processed_simple = Some(simple);
}

/// Query ES `/{index}/_search` for attribute docs and return maps of types and synonyms.
///
/// `es_base` example: "http://localhost:9200". `index` is the attributes index name.
/// `result` is the group to match (e.g. "taxon") or "multi" to fetch all groups.
pub fn attr_types(es_base: &str, index: &str, result: &str) -> Result<(TypesMap, SynonymsMap)> {
    let client = Client::new();
    let url = format!("{}/{}/_search", es_base.trim_end_matches('/'), index);

    let query_body = if result == "multi" {
        serde_json::json!({ "query": { "match_all": {} }, "size": 10000 })
    } else {
        serde_json::json!({
            "query": { "match": { "group": { "query": result } } },
            "size": 10000
        })
    };

    let resp = client
        .post(&url)
        .json(&query_body)
        .send()
        .with_context(|| format!("HTTP POST {url}"))?
        .error_for_status()
        .with_context(|| format!("non-2xx response from {url}"))?;

    let body: Value = resp.json().context("deserialising JSON response")?;

    let hits = body
        .get("hits")
        .and_then(|h| h.get("hits"))
        .and_then(|v| v.as_array())
        .with_context(|| format!("expected hits.hits array in response from {url}"))?;

    let mut types_map: TypesMap = HashMap::new();
    let mut synonyms: SynonymsMap = HashMap::new();

    for hit in hits.iter() {
        if let Some(source) = hit.get("_source") {
            let mut meta: TypeMeta =
                serde_json::from_value(source.clone()).context("parsing _source into TypeMeta")?;
            // Ensure name/group present
            if meta.group.is_empty() || meta.name.is_empty() {
                continue;
            }
            set_processed_type(&mut meta);
            set_processed_summary(&mut meta);

            types_map
                .entry(meta.group.clone())
                .or_default()
                .insert(meta.name.clone(), meta.clone());

            if !meta.synonyms.is_empty() {
                let group_map = synonyms.entry(meta.group.clone()).or_default();
                for syn in meta.synonyms.iter() {
                    group_map.insert(syn.clone(), meta.name.clone());
                }
            } else if meta.name.contains('_') {
                let group_map = synonyms.entry(meta.group.clone()).or_default();
                group_map.insert(meta.name.replace('_', "-"), meta.name.clone());
            }
        }
    }

    Ok((types_map, synonyms))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    fn sample_hit(group: &str, name: &str, typ: &str, synonyms: Option<Vec<&str>>) -> Value {
        let syn_json = synonyms.map(|s| s.iter().map(|x| Value::String(x.to_string())).collect());
        let mut obj = serde_json::json!({
            "group": group,
            "name": name,
            "type": typ
        });
        if let Some(s) = syn_json {
            obj["synonyms"] = Value::Array(s);
        }
        obj
    }

    #[test]
    fn attr_types_parses_hits_and_synonyms() {
        let mut server = Server::new();
        let body = serde_json::json!({
            "hits": { "hits": [ {"_source": sample_hit("taxon","genome_size","long", None) }, {"_source": sample_hit("taxon","assembly_level","keyword", Some(vec!["assembly-level"])) } ] }
        });
        let mock = server
            .mock("POST", "/attributes/_search")
            .with_status(200)
            .with_body(serde_json::to_string(&body).unwrap())
            .create();

        let base = server.url();
        let (types_map, synonyms) = attr_types(&base, "attributes", "taxon").unwrap();
        assert!(types_map.contains_key("taxon"));
        let taxon = types_map.get("taxon").unwrap();
        assert!(taxon.contains_key("genome_size"));
        assert!(taxon.contains_key("assembly_level"));
        let syn = synonyms.get("taxon").unwrap();
        assert_eq!(
            syn.get("assembly-level").map(|s| s.as_str()),
            Some("assembly_level")
        );
        mock.assert();
    }

    #[test]
    fn processed_type_mapping() {
        let mut server = Server::new();
        let body = serde_json::json!({
            "hits": { "hits": [ {"_source": sample_hit("taxon","a","double", None) }, {"_source": sample_hit("taxon","b","integer", None) }, {"_source": sample_hit("taxon","c","keyword", None) } ] }
        });
        let mock = server
            .mock("POST", "/attributes/_search")
            .with_status(200)
            .with_body(serde_json::to_string(&body).unwrap())
            .create();
        let base = server.url();
        let (types_map, _syn) = attr_types(&base, "attributes", "taxon").unwrap();
        let taxon = types_map.get("taxon").unwrap();
        assert_eq!(
            taxon.get("a").unwrap().processed_type.as_deref(),
            Some("float")
        );
        assert_eq!(
            taxon.get("b").unwrap().processed_type.as_deref(),
            Some("integer")
        );
        assert_eq!(
            taxon.get("c").unwrap().processed_type.as_deref(),
            Some("keyword")
        );
        mock.assert();
    }
}
