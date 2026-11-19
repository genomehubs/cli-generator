use reqwest::Client;
use serde_json::{json, Map, Value as JsonValue};
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

use crate::AppState;

#[derive(Default, Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct MetadataCache {
    pub taxonomies: Vec<String>,
    pub indices: Vec<String>,
    pub attr_types: JsonValue,
    pub taxonomic_ranks: Vec<String>,
    /// All distinct `feature_type` attribute values found in the feature index.
    /// Populated at startup; used for user-supplied feature-type normalisation.
    pub feature_types: Vec<String>,
    pub last_updated: Option<String>,
    pub has_sayt_field: bool,
    pub has_trigram_field: bool,
    /// Top-level `keyword` mapping fields per index group (`taxon`, `assembly`, `sample`, `feature`).
    pub keyword_fields: HashMap<String, Vec<String>>,
    /// Top-most taxonomic ancestor of the taxonomy, read from the lineage of a taxon document.
    /// Scopes queries that carry no taxon filter.
    pub root_taxon_id: Option<String>,
}

impl AppState {
    /// Returns the root taxon ID discovered at startup, or `None` if the cache is not populated.
    pub async fn root_taxon_id(&self) -> Option<String> {
        let lock = self.cache.as_ref()?;
        lock.read().await.root_taxon_id.clone()
    }
}

impl MetadataCache {
    /// Returns the top-level keyword field names of the index group `group`, or an empty slice.
    pub fn keyword_fields_of(&self, group: &str) -> &[String] {
        self.keyword_fields
            .get(group)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Convert the cached `attr_types` JSON into a `TypesMap` for use with `build_search_body`.
    ///
    /// This is a pure deserialization — no network I/O. Call it per request when
    /// a `TypesMap` is needed to select the correct typed value docvalue field.
    pub fn as_types_map(&self) -> genomehubs_query::attr_types::TypesMap {
        serde_json::from_value(self.attr_types.clone()).unwrap_or_default()
    }
}

fn processed_type(meta: &JsonValue) -> Option<String> {
    let t = meta.get("type").and_then(|v| v.as_str()).unwrap_or("");
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
        return Some("float".to_string());
    }
    if ["long", "integer", "short", "byte"].contains(&t) {
        return Some("integer".to_string());
    }
    if t == "keyword" {
        // examine summary
        if let Some(summary) = meta.get("summary") {
            if summary.is_array() {
                if let Some(first) = summary.get(0).and_then(|v| v.as_str()) {
                    if first == "enum" {
                        return Some("ordered_keyword".to_string());
                    }
                    if first == "primary" {
                        if let Some(second) = summary.get(1).and_then(|v| v.as_str()) {
                            if second == "enum" {
                                return Some("ordered_keyword".to_string());
                            }
                        }
                    }
                }
            } else if summary == "enum" {
                return Some("ordered_keyword".to_string());
            }
        }
        return Some("keyword".to_string());
    }
    Some(t.to_string())
}

fn processed_summary_and_simple(meta: &JsonValue) -> (String, String) {
    if let Some(default_summary) = meta.get("default_summary").and_then(|v| v.as_str()) {
        let simple = meta
            .get("return_type")
            .and_then(|v| v.as_str())
            .unwrap_or("value")
            .to_string();
        return (default_summary.to_string(), simple);
    }

    let simple = meta
        .get("return_type")
        .and_then(|v| v.as_str())
        .unwrap_or("value")
        .to_string();

    let summary = if meta.get("type").and_then(|v| v.as_str()) == Some("keyword") {
        "keyword_value.raw".to_string()
    } else {
        format!(
            "{}_value",
            meta.get("type").and_then(|v| v.as_str()).unwrap_or("")
        )
    };

    (summary, simple)
}

/// Fetch all distinct `feature_type` attribute values from the feature index.
///
/// Uses a nested `terms` aggregation on `attributes.key = feature_type` to
/// collect the `attributes.keyword_value` bucket keys.
pub async fn fetch_feature_types(
    client: &Client,
    es_base: &str,
    feature_index: &str,
) -> Result<Vec<String>, String> {
    let url = format!(
        "{}/{}/_search",
        es_base.trim_end_matches('/'),
        feature_index
    );
    let body = json!({
        "size": 0,
        "aggs": {
            "attrs": {
                "nested": {"path": "attributes"},
                "aggs": {
                    "by_type": {
                        "filter": {"term": {"attributes.key": "feature_type"}},
                        "aggs": {
                            "vals": {
                                "terms": {"field": "attributes.keyword_value", "size": 200}
                            }
                        }
                    }
                }
            }
        }
    });

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("feature_types request failed: {e}"))?;

    if !resp.status().is_success() {
        // Non-fatal: feature index may not exist yet
        return Ok(Vec::new());
    }

    let body_json: JsonValue = resp
        .json()
        .await
        .map_err(|e| format!("feature_types json parse: {e}"))?;

    let buckets = body_json
        .pointer("/aggregations/attrs/by_type/vals/buckets")
        .and_then(|b| b.as_array())
        .cloned()
        .unwrap_or_default();

    let types = buckets
        .iter()
        .filter_map(|b| b.get("key").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect();

    Ok(types)
}

/// Fetch `_cat/indices?format=json` and derive taxonomies + indices similar to the JS logic.
pub async fn fetch_cat_indices_json(
    client: &Client,
    es_base: &str,
    taxonomy: &str,
    release: &str,
) -> Result<(Vec<String>, Vec<String>), String> {
    let url = format!(
        "{}/_cat/indices?format=json&expand_wildcards=all",
        es_base.trim_end_matches('/')
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("cat.indices request failed: {}", e))?
        .error_for_status()
        .map_err(|e| format!("cat.indices status error: {}", e))?;

    let arr: JsonValue = resp
        .json()
        .await
        .map_err(|e| format!("json parse error: {}", e))?;

    let mut taxonomies = Vec::new();
    let mut indices = Vec::new();

    if let Some(items) = arr.as_array() {
        for item in items.iter() {
            let index_name = item.get("index").and_then(|v| v.as_str()).unwrap_or("");
            // docs count can be string or number
            let docs_count = item
                .get("docs.count")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i64>().ok())
                .or_else(|| item.get("docs.count").and_then(|v| v.as_i64()));

            let docs_ok = docs_count.map(|c| c > 0).unwrap_or(true);

            if index_name.starts_with("taxon--") && index_name.contains(release) {
                // taxon--<taxonomy>--... -> extract between separators
                let parts: Vec<&str> = index_name.split("--").collect();
                if parts.len() > 1 {
                    if let Some(t) = parts.get(1) {
                        let t = t.to_string();
                        if !taxonomies.contains(&t) {
                            taxonomies.push(t);
                        }
                    }
                }
            }

            if index_name.contains(&format!("--{}--", taxonomy))
                && index_name.contains(&format!("--{}", release))
                && docs_ok
            {
                let parts: Vec<&str> = index_name.split("--").collect();
                if let Some(prefix) = parts.first() {
                    let p = prefix.to_string();
                    if !indices.contains(&p) {
                        indices.push(p);
                    }
                }
            }
        }
    }

    // ensure configured taxonomy is first
    if !taxonomies.contains(&taxonomy.to_string()) {
        taxonomies.insert(0, taxonomy.to_string());
    } else {
        // move to front
        taxonomies.retain(|t| t != taxonomy);
        taxonomies.insert(0, taxonomy.to_string());
    }

    Ok((taxonomies, indices))
}

/// Attribute document fields copied into the startup cache.
///
/// `traverse_direction` and `constraint` drive the inheritance and enum checks in
/// the query validator, so they must be listed here or the cache drops them.
const ATTRIBUTE_CACHE_FIELDS: [&str; 9] = [
    "group",
    "name",
    "type",
    "summary",
    "default_summary",
    "return_type",
    "synonyms",
    "traverse_direction",
    "constraint",
];

/// Builds the Elasticsearch search body for the attribute documents of `result`.
///
/// `"multi"` fetches every group. Any other value restricts the search to that group.
pub fn attr_search_body(result: &str) -> JsonValue {
    if result == "multi" {
        json!({ "size": 10000, "_source": ATTRIBUTE_CACHE_FIELDS })
    } else {
        json!({
            "size": 10000,
            "query": { "match": { "group": { "query": result } } },
            "_source": ATTRIBUTE_CACHE_FIELDS
        })
    }
}

/// Fetch attribute documents from the attributes index and build a grouped map like the JS code.
pub async fn fetch_attr_types(
    client: &Client,
    es_base: &str,
    attributes_index: &str,
    result: &str,
) -> Result<JsonValue, String> {
    let url = format!(
        "{}/{}/_search",
        es_base.trim_end_matches('/'),
        attributes_index
    );
    let body = attr_search_body(result);

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("attr types request failed: {}", e))?
        .error_for_status()
        .map_err(|e| format!("attr types status error: {}", e))?;

    let body_json: JsonValue = resp
        .json()
        .await
        .map_err(|e| format!("json parse error: {}", e))?;

    let hits = body_json
        .get("hits")
        .and_then(|h| h.get("hits"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| "missing hits.hits array".to_string())?;

    let mut types_map: Map<String, JsonValue> = Map::new();

    for hit in hits.iter() {
        if let Some(source) = hit.get("_source") {
            if let (Some(group), Some(name)) = (
                source.get("group").and_then(|v| v.as_str()),
                source.get("name").and_then(|v| v.as_str()),
            ) {
                let entry = types_map
                    .entry(group.to_string())
                    .or_insert_with(|| JsonValue::Object(Map::new()));
                if let JsonValue::Object(map) = entry {
                    let mut doc = source.clone();
                    if let Some(pt) = processed_type(&doc) {
                        doc.as_object_mut()
                            .map(|m| m.insert("processed_type".to_string(), JsonValue::String(pt)));
                    }
                    let (ps, simple) = processed_summary_and_simple(&doc);
                    doc.as_object_mut()
                        .map(|m| m.insert("processed_summary".to_string(), JsonValue::String(ps)));
                    doc.as_object_mut().map(|m| {
                        m.insert("processed_simple".to_string(), JsonValue::String(simple))
                    });
                    map.insert(name.to_string(), doc);
                }
            }
        }
    }

    Ok(JsonValue::Object(types_map))
}

/// Check if taxon_names.name.live field exists in the index mapping.
/// Used to enable Stage 1 (SAYT) lookups across synonyms and common names.
async fn check_sayt_field(client: &Client, es_base: &str, index: &str) -> Result<bool, String> {
    let url = format!("{}/{}/_mapping", es_base.trim_end_matches('/'), index);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("mapping request failed: {}", e))?;

    if !resp.status().is_success() {
        return Ok(false);
    }

    let mapping_json: JsonValue = resp
        .json()
        .await
        .map_err(|e| format!("json parse error: {}", e))?;

    // Check if taxon_names.name.live exists
    if let Some(indices) = mapping_json.as_object() {
        for (_, index_mapping) in indices.iter() {
            if let Some(properties) = index_mapping
                .get("mappings")
                .and_then(|m| m.get("properties"))
            {
                if let Some(taxon_names) = properties.get("taxon_names") {
                    if let Some(nested_props) = taxon_names.get("properties") {
                        if let Some(name_field) = nested_props.get("name") {
                            if let Some(fields) = name_field.get("fields") {
                                if fields.get("live").is_some() {
                                    return Ok(true);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(false)
}

/// Check if taxon_names.name.trigram field exists in the index mapping.
async fn check_trigram_field(client: &Client, es_base: &str, index: &str) -> Result<bool, String> {
    let url = format!("{}/{}/_mapping", es_base.trim_end_matches('/'), index);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("mapping request failed: {}", e))?;

    if !resp.status().is_success() {
        return Ok(false);
    }

    let mapping_json: JsonValue = resp
        .json()
        .await
        .map_err(|e| format!("json parse error: {}", e))?;

    // Check if taxon_names.name.trigram exists
    if let Some(indices) = mapping_json.as_object() {
        for (_, index_mapping) in indices.iter() {
            if let Some(properties) = index_mapping
                .get("mappings")
                .and_then(|m| m.get("properties"))
            {
                if let Some(taxon_names) = properties.get("taxon_names") {
                    if let Some(properties) = taxon_names.get("properties") {
                        if let Some(name_field) = properties.get("name") {
                            if let Some(fields) = name_field.get("fields") {
                                if fields.get("trigram").is_some() {
                                    return Ok(true);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(false)
}

/// Fetches the top-level keyword field names of one index.
///
/// Returns an empty list when the index does not exist, so hubs without a sample or feature index still start.
pub async fn fetch_keyword_fields(
    client: &Client,
    es_base: &str,
    index: &str,
) -> Result<Vec<String>, String> {
    let url = format!("{}/{}/_mapping", es_base.trim_end_matches('/'), index);
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("keyword mapping request failed: {}", e))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    let mapping_json: JsonValue = resp
        .error_for_status()
        .map_err(|e| format!("keyword mapping status error: {}", e))?
        .json()
        .await
        .map_err(|e| format!("json parse error: {}", e))?;
    Ok(keyword_field_names(&mapping_json))
}

/// Returns the sorted, de-duplicated top-level `keyword` property names in a `_mapping` response.
fn keyword_field_names(mapping_json: &JsonValue) -> Vec<String> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    for index_mapping in mapping_json
        .as_object()
        .into_iter()
        .flat_map(|o| o.values())
    {
        let properties = index_mapping
            .pointer("/mappings/properties")
            .and_then(JsonValue::as_object);
        for (name, property) in properties.into_iter().flatten() {
            if property.get("type").and_then(JsonValue::as_str) == Some("keyword") {
                names.insert(name.clone());
            }
        }
    }
    names.into_iter().collect()
}

/// Fetches the top-level keyword fields of the `taxon`, `assembly`, `sample`, and `feature` indices.
async fn fetch_all_keyword_fields(
    client: &Client,
    es_base: &str,
    suffix: Option<&str>,
) -> Result<HashMap<String, Vec<String>>, String> {
    let mut fields = HashMap::new();
    for group in ["taxon", "assembly", "sample", "feature"] {
        let index = match suffix {
            Some(suffix) => format!("{group}{suffix}"),
            None => group.to_string(),
        };
        let names = fetch_keyword_fields(client, es_base, &index).await?;
        fields.insert(group.to_string(), names);
    }
    Ok(fields)
}

pub async fn fetch_taxonomic_ranks(
    client: &Client,
    es_base: &str,
    index: &str,
) -> Result<Vec<String>, String> {
    let url = format!("{}/{}/_search", es_base.trim_end_matches('/'), index);
    let body = json!({
        "size": 0,
        "query": { "bool": { "must_not": { "term": { "taxon_rank": "no rank" } } } },
        "aggs": { "unique_ranks": { "terms": { "field": "taxon_rank", "size": 100 } } }
    });

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("ranks request failed: {}", e))?
        .error_for_status()
        .map_err(|e| format!("ranks status error: {}", e))?;

    let body_json: JsonValue = resp
        .json()
        .await
        .map_err(|e| format!("json parse error: {}", e))?;

    if let Some(buckets) = body_json
        .get("aggregations")
        .and_then(|a| a.get("unique_ranks"))
        .and_then(|u| u.get("buckets"))
        .and_then(|b| b.as_array())
    {
        let mut ranks = Vec::new();
        for bucket in buckets.iter() {
            if let Some(k) = bucket.get("key").and_then(|v| v.as_str()) {
                ranks.push(k.to_string());
            }
        }
        return Ok(ranks);
    }

    Ok(Vec::new())
}

/// Returns the root taxon ID of the taxonomy, read from the lineage of one taxon document.
///
/// Lineage is ordered self first and top-most ancestor last, so the root is the final entry.
pub async fn fetch_root_taxon_id(
    client: &Client,
    es_base: &str,
    index: &str,
) -> Result<String, String> {
    let url = format!("{}/{}/_search", es_base.trim_end_matches('/'), index);
    let body = json!({
        "size": 1,
        "_source": ["lineage.taxon_id"],
        "query": { "match_all": {} }
    });
    let body_json: JsonValue = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("root taxon request failed: {}", e))?
        .error_for_status()
        .map_err(|e| format!("root taxon status error: {}", e))?
        .json()
        .await
        .map_err(|e| format!("json parse error: {}", e))?;
    root_taxon_from_search_response(&body_json)
        .ok_or_else(|| format!("no lineage root found in index '{}'", index))
}

/// Extracts the root taxon ID from the first hit of a `_search` response with `lineage.taxon_id` in `_source`.
fn root_taxon_from_search_response(body_json: &JsonValue) -> Option<String> {
    body_json
        .pointer("/hits/hits/0/_source/lineage")
        .and_then(JsonValue::as_array)
        .and_then(|lineage| lineage.last())
        .and_then(|entry| entry.get("taxon_id"))
        .and_then(JsonValue::as_str)
        .map(str::to_string)
}

/// Populate the shared cache by calling the above helpers. Returns Ok(()) on success.
pub async fn populate_cache(state: Arc<AppState>, client: &Client) -> Result<(), String> {
    let es_base = &state.es_base;
    let taxonomy = &state.default_taxonomy;
    let release = &state.default_version;

    // normalise suffix
    let suffix_norm = state.index_suffix.as_ref().map(|s| {
        if s.starts_with("--") {
            s.clone()
        } else {
            format!("--{}", s)
        }
    });

    let attributes_index = suffix_norm
        .as_ref()
        .map(|s| format!("attributes{}", s))
        .unwrap_or_else(|| "attributes".to_string());

    // fetch cat indices
    let (taxonomies, indices) = fetch_cat_indices_json(client, es_base, taxonomy, release).await?;

    // fetch attribute types
    let attr_types = fetch_attr_types(client, es_base, &attributes_index, "multi").await?;

    // fetch ranks from taxon index
    let taxon_index = suffix_norm
        .as_ref()
        .map(|s| format!("taxon{}", s))
        .unwrap_or_else(|| "taxon".to_string());
    let ranks = fetch_taxonomic_ranks(client, es_base, &taxon_index)
        .await
        .unwrap_or_default();

    let root_taxon_id = fetch_root_taxon_id(client, es_base, &taxon_index).await?;

    let has_sayt_field = check_sayt_field(client, es_base, &taxon_index)
        .await
        .unwrap_or(false);

    let has_trigram_field = check_trigram_field(client, es_base, &taxon_index)
        .await
        .unwrap_or(false);

    // fetch feature types (non-fatal: feature index may be absent on some hubs)
    let feature_index = suffix_norm
        .as_ref()
        .map(|s| format!("feature{}", s))
        .unwrap_or_else(|| "feature".to_string());
    let feature_types = fetch_feature_types(client, es_base, &feature_index)
        .await
        .unwrap_or_default();

    let keyword_fields = fetch_all_keyword_fields(client, es_base, suffix_norm.as_deref()).await?;

    let now = chrono::Utc::now().to_rfc3339();

    let cache = MetadataCache {
        taxonomies,
        indices,
        attr_types,
        taxonomic_ranks: ranks,
        feature_types,
        last_updated: Some(now),
        has_sayt_field,
        has_trigram_field,
        keyword_fields,
        root_taxon_id: Some(root_taxon_id.clone()),
    };
    tracing::info!(root_taxon_id = %root_taxon_id, "discovered root taxon");

    // write into AppState cache (tokio RwLock inside AppState)
    if let Some(lock) = &state.cache {
        let mut w = lock.write().await;
        *w = cache;
    }

    Ok(())
}

/// Retry-populate with exponential backoff. If max_attempts is None, retry indefinitely.
pub async fn populate_with_retry(
    state: Arc<AppState>,
    client: &Client,
    max_attempts: Option<usize>,
) -> Result<(), String> {
    let mut attempt: usize = 0;
    let mut wait = Duration::from_secs(1);
    loop {
        attempt += 1;
        match populate_cache(state.clone(), client).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                tracing::error!(attempt, error = %e, "populate_cache failed");
                if let Some(max) = max_attempts {
                    if attempt >= max {
                        return Err(format!("failed after {} attempts: {}", attempt, e));
                    }
                }
                tracing::info!(wait = ?wait, "retrying populate_cache after backoff");
                sleep(wait).await;
                wait = std::cmp::min(wait * 2, Duration::from_secs(30));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_taxon_is_last_lineage_entry_of_first_hit() {
        let response = json!({
            "hits": { "hits": [ { "_source": { "lineage": [
                { "taxon_id": "131567" },
                { "taxon_id": "1" }
            ] } } ] }
        });
        assert_eq!(
            root_taxon_from_search_response(&response),
            Some("1".to_string())
        );
    }

    #[test]
    fn root_taxon_is_none_without_hits() {
        let response = json!({ "hits": { "hits": [] } });
        assert_eq!(root_taxon_from_search_response(&response), None);
    }

    #[test]
    fn keyword_field_names_keeps_only_top_level_keywords() {
        let mapping = json!({
            "taxon--ncbi--goat--2026.11.16": {
                "mappings": {
                    "properties": {
                        "taxon_id": { "type": "keyword" },
                        "scientific_name": { "type": "keyword" },
                        "genome_size": { "type": "long" },
                        "lineage": { "type": "nested" }
                    }
                }
            }
        });
        assert_eq!(
            keyword_field_names(&mapping),
            vec!["scientific_name".to_string(), "taxon_id".to_string()]
        );
    }

    #[test]
    fn keyword_field_names_is_empty_without_mappings() {
        assert!(keyword_field_names(&json!({})).is_empty());
    }

    #[test]
    fn keyword_fields_of_returns_empty_for_unknown_group() {
        let cache = MetadataCache::default();
        assert!(cache.keyword_fields_of("sample").is_empty());
    }

    #[test]
    fn keyword_fields_of_returns_fields_for_known_group() {
        let cache = MetadataCache {
            keyword_fields: HashMap::from([(
                "assembly".to_string(),
                vec!["assembly_id".to_string()],
            )]),
            ..Default::default()
        };
        assert_eq!(cache.keyword_fields_of("assembly"), ["assembly_id"]);
    }

    #[test]
    fn attr_search_body_requests_traverse_and_constraint() {
        let body = attr_search_body("multi");
        let source: Vec<&str> = body["_source"]
            .as_array()
            .expect("_source is an array")
            .iter()
            .filter_map(JsonValue::as_str)
            .collect();
        assert!(source.contains(&"traverse_direction"));
        assert!(source.contains(&"constraint"));
        assert!(body.get("query").is_none());
    }

    #[test]
    fn attr_search_body_restricts_single_group() {
        let body = attr_search_body("taxon");
        assert_eq!(body["query"]["match"]["group"]["query"], "taxon");
    }
}
