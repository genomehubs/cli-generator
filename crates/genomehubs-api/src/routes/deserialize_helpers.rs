//! Shared helpers for deserializing JSON request bodies.
//!
//! Provides utilities to normalize and convert JSON input to YAML format
//! expected by the query parser.

use serde_json::Value;

/// Normalize fields and attributes arrays for consistent parsing.
///
/// Converts string arrays to struct arrays:
/// - `"fields": ["genome_size"]` → `"fields": [{"name": "genome_size"}]`
/// - `"attributes": ["taxon_rank"]` → `"attributes": [{"name": "taxon_rank"}]`
pub fn normalize_query(mut query: Value) -> Value {
    if let Some(obj) = query.as_object_mut() {
        // Normalize "fields" array: convert strings to {name: string} objects
        if let Some(fields_val) = obj.get_mut("fields") {
            if let Some(arr) = fields_val.as_array_mut() {
                let normalized: Vec<Value> = arr
                    .iter()
                    .map(|f| {
                        if f.is_string() {
                            serde_json::json!({ "name": f })
                        } else {
                            f.clone()
                        }
                    })
                    .collect();
                *fields_val = Value::Array(normalized);
            }
        }

        // Normalize "attributes" array: convert strings to {name: string} objects
        if let Some(attrs_val) = obj.get_mut("attributes") {
            if let Some(arr) = attrs_val.as_array_mut() {
                let normalized: Vec<Value> = arr
                    .iter()
                    .map(|a| {
                        if a.is_string() {
                            serde_json::json!({ "name": a })
                        } else {
                            a.clone()
                        }
                    })
                    .collect();
                *attrs_val = Value::Array(normalized);
            }
        }
    }
    query
}

/// Convert a JSON value to YAML string, preserving strings as-is.
///
/// Handles both string and structured inputs for flexible deserialization.
pub fn to_yaml<D: serde::de::Error>(val: &Value) -> Result<String, D> {
    match val {
        Value::String(s) => Ok(s.clone()),
        _ => serde_yaml::to_string(val).map_err(D::custom),
    }
}

/// Detect and extract v2-style `queryA=...` named-query params from a JSON
/// request body and inject them as `named_queries` into the YAML string.
///
/// Keys matching the pattern `query` + one-or-more uppercase ASCII letters
/// (e.g. `queryA`, `queryB`, `queryAB`) are converted via
/// [`NamedQuerySpec::from_legacy_string`] and appended to the `named_queries`
/// map in the YAML.  Unrecognised index prefixes are silently skipped.
///
/// Returns the (possibly unchanged) YAML string.
pub fn inject_legacy_named_queries(query_yaml: &str, body: &Value) -> String {
    use genomehubs_query::query::chain::NamedQuerySpec;

    let Some(obj) = body.as_object() else {
        return query_yaml.to_string();
    };

    let legacy_entries: Vec<(String, NamedQuerySpec)> = obj
        .iter()
        .filter_map(|(key, val)| {
            // Key must start with "query" and have only uppercase letters after.
            let suffix = key.strip_prefix("query")?;
            if suffix.is_empty() || !suffix.chars().all(|c| c.is_ascii_uppercase()) {
                return None;
            }
            let raw = val.as_str()?;
            let spec = NamedQuerySpec::from_legacy_string(raw)?;
            Some((key.clone(), spec))
        })
        .collect();

    if legacy_entries.is_empty() {
        return query_yaml.to_string();
    }

    // Parse the existing YAML, inject named_queries, re-serialise.
    let mut doc: serde_yaml::Value = match serde_yaml::from_str(query_yaml) {
        Ok(v) => v,
        Err(_) => return query_yaml.to_string(),
    };

    if let serde_yaml::Value::Mapping(ref mut map) = doc {
        let nq_key = serde_yaml::Value::String("named_queries".to_string());
        let nq_map = map
            .entry(nq_key)
            .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));

        if let serde_yaml::Value::Mapping(ref mut nq) = nq_map {
            for (key, spec) in legacy_entries {
                if let Ok(spec_val) = serde_yaml::to_value(&spec) {
                    nq.insert(serde_yaml::Value::String(key), spec_val);
                }
            }
        }
    }

    serde_yaml::to_string(&doc).unwrap_or_else(|_| query_yaml.to_string())
}

/// Transform a raw Elasticsearch hit into a V3 result envelope.
///
/// Mirrors V2 processHits.js logic:
/// - Identity fields (`taxon_id`, `scientific_name`, `taxon_rank`, `parent`) come from `_source`
/// - Attribute field data comes from `inner_hits.attributes.hits.hits[*].fields`
///   (doc_values format, same as V2's `docvalue_fields` inner_hits)
/// - `lineage` and `taxon_names` are stripped from `_source` unless opted in
pub fn transform_es_hit(
    hit: &Value,
    index: &str,
    include_lineage: bool,
    include_taxon_names: bool,
) -> Value {
    let hit_id = hit.get("_id").and_then(|v| v.as_str()).unwrap_or("");
    let score = hit.get("_score").cloned().unwrap_or(Value::Null);

    // Build result from _source identity fields only
    let mut result = serde_json::Map::new();
    if let Some(src) = hit.get("_source").and_then(|v| v.as_object()) {
        for key in &[
            "taxon_id",
            "assembly_id",
            "sample_id",
            "scientific_name",
            "taxon_rank",
            "parent",
        ] {
            if let Some(v) = src.get(*key) {
                result.insert(key.to_string(), v.clone());
            }
        }
        if include_lineage {
            if let Some(v) = src.get("lineage") {
                result.insert("lineage".to_string(), v.clone());
            }
        }
        if include_taxon_names {
            if let Some(v) = src.get("taxon_names") {
                result.insert("taxon_names".to_string(), v.clone());
            }
        }
    }

    // Extract attribute fields from inner_hits (set by build_search_body's
    // docvalue_fields inner_hits — same pattern as V2 matchAttributes)
    if let Some(inner_hits) = hit.get("inner_hits").and_then(|v| v.as_object()) {
        for inner_name in &["attributes", "optionalAttributes"] {
            if let Some(attr_hits) = inner_hits
                .get(*inner_name)
                .and_then(|v| v.get("hits"))
                .and_then(|v| v.get("hits"))
                .and_then(|v| v.as_array())
            {
                let mut fields = serde_json::Map::new();
                for attr_hit in attr_hits {
                    let dv = match attr_hit.get("fields").and_then(|v| v.as_object()) {
                        Some(f) => f,
                        None => continue,
                    };
                    // key is always a single-element array from docvalue_fields
                    let name = match dv
                        .get("attributes.key")
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.first())
                        .and_then(|v| v.as_str())
                    {
                        Some(k) => k.to_string(),
                        None => continue,
                    };
                    let mut field = serde_json::Map::new();
                    for (dv_key, dv_val) in dv {
                        // Strip "attributes." prefix
                        let short = dv_key
                            .strip_prefix("attributes.")
                            .unwrap_or(dv_key.as_str());
                        if short == "key" {
                            continue;
                        }
                        // Unwrap single-element arrays (ES docvalue_fields format)
                        let scalar = if let Some(arr) = dv_val.as_array() {
                            if arr.len() == 1 {
                                arr[0].clone()
                            } else {
                                dv_val.clone()
                            }
                        } else {
                            dv_val.clone()
                        };
                        // Map typed value fields (e.g. long_value, half_float_value) to "value"
                        let out_key = if short.ends_with("_value") && short != "is_primary_value" {
                            "value".to_string()
                        } else if short == "is_primary_value" {
                            "is_primary".to_string()
                        } else {
                            // strip ".raw" suffix (e.g. keyword_value.raw → keyword_value already mapped)
                            short.trim_end_matches(".raw").to_string()
                        };
                        // Don't overwrite "value" if already set to a non-null value;
                        // but do replace a null placeholder with a real value
                        // (ES docvalue_fields returns null for absent typed fields,
                        // e.g. long_value is null for a half_float attribute).
                        if out_key == "value" {
                            match field.entry(out_key) {
                                serde_json::map::Entry::Vacant(e) => {
                                    e.insert(scalar);
                                }
                                serde_json::map::Entry::Occupied(mut e) => {
                                    if e.get().is_null() && !scalar.is_null() {
                                        *e.get_mut() = scalar;
                                    }
                                }
                            }
                        } else {
                            field.entry(out_key).or_insert(scalar);
                        }
                    }
                    if !field.is_empty() {
                        // Merge into existing field entry for this name if present
                        match fields.entry(name) {
                            serde_json::map::Entry::Occupied(mut e) => {
                                if let Some(existing) = e.get_mut().as_object_mut() {
                                    for (k, v) in field {
                                        existing.entry(k).or_insert(v);
                                    }
                                }
                            }
                            serde_json::map::Entry::Vacant(e) => {
                                e.insert(Value::Object(field));
                            }
                        }
                    }
                }
                if !fields.is_empty() {
                    // Merge into result.fields
                    match result.entry("fields".to_string()) {
                        serde_json::map::Entry::Occupied(mut e) => {
                            if let Some(existing) = e.get_mut().as_object_mut() {
                                for (k, v) in fields {
                                    existing.entry(k).or_insert(v);
                                }
                            }
                        }
                        serde_json::map::Entry::Vacant(e) => {
                            e.insert(Value::Object(fields));
                        }
                    }
                }
            }
        }
    }

    serde_json::json!({
        "index": index,
        "id": hit_id,
        "score": score,
        "result": Value::Object(result),
    })
}
