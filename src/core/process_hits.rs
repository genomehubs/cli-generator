use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Minimal port of the API's `processHits` that transforms an ES `/search`
/// response `body` into an array of result objects. This intentionally
/// implements a subset of the JS original: attribute -> `fields` mapping,
/// `inner_hits` name/identifier parsing, basic `ranks`/`lca` handling, and
/// `reason` extraction. Buckets/aggregation binning are omitted for now.
#[allow(
    clippy::cognitive_complexity,
    clippy::too_many_arguments,
    clippy::collapsible_match,
    clippy::collapsible_if,
    clippy::get_first
)]
pub fn process_hits(
    body: &Value,
    names: bool,
    ranks: bool,
    fields: &[String],
    reason: bool,
    lca: Option<&Value>,
    inner_hits_option: bool,
    process_as_doc: bool,
) -> Result<Vec<Value>> {
    let mut results: Vec<Value> = Vec::new();

    // parse `fields` into target_fields similar to the JS implementation
    // each entry may be `attr:suffix` where suffix defaults to "value"
    let mut target_fields: HashMap<String, Vec<String>> = HashMap::new();
    for f in fields.iter() {
        let mut parts = f.splitn(2, ':');
        let attr = parts.next().unwrap_or("");
        let suffix = parts.next().unwrap_or("value");
        target_fields
            .entry(attr.to_string())
            .or_default()
            .push(suffix.to_string());
    }

    let hits = match body.get("hits").and_then(|h| h.get("hits")) {
        Some(Value::Array(a)) => a,
        _ => return Ok(results),
    };

    for hit in hits {
        let index = hit.get("_index").cloned().unwrap_or(Value::Null);
        let id = hit.get("_id").cloned().unwrap_or(Value::Null);
        let score = hit.get("_score").cloned().unwrap_or(Value::Null);

        let mut result_obj = json!({
            "index": index,
            "id": id,
            "score": score,
            "result": hit.get("_source").cloned().unwrap_or(Value::Null)
        });

        // processAsDoc: passthrough for now
        if process_as_doc {
            // no-op: keep _source as result
        } else {
            // parse names/identifiers from inner_hits into `result.names` when requested
            if names {
                if let Some(inner_hits) = hit.get("inner_hits") {
                    // taxon_names
                    if let Some(tn) = inner_hits.get("taxon_names") {
                        if let Some(array) = tn.get("hits").and_then(|h| h.get("hits")) {
                            let mut parsed = serde_json::Map::new();
                            if let Value::Array(arr) = array {
                                for obj in arr {
                                    if let Some(fields_map) = obj.get("fields") {
                                        let class = fields_map
                                            .get("taxon_names.class")
                                            .and_then(|v| v.get(0))
                                            .cloned()
                                            .unwrap_or(Value::String("".into()));
                                        let class_key = class.as_str().unwrap_or("");
                                        let mut hit_names = serde_json::Map::new();
                                        if let Value::Object(fmap) = fields_map {
                                            for (k, v) in fmap.iter() {
                                                let key = k
                                                    .replace("taxon_names.", "")
                                                    .replace(".raw", "");
                                                hit_names.insert(key, v.clone());
                                            }
                                        }
                                        parsed.insert(
                                            class_key.to_string(),
                                            Value::Object(hit_names),
                                        );
                                    }
                                }
                            }
                            if let Value::Object(ref mut base) = result_obj["result"] {
                                base.insert("names".to_string(), Value::Object(parsed));
                            }
                        }
                    }
                    // identifiers
                    if let Some(ids) = inner_hits.get("identifiers") {
                        if let Some(array) = ids.get("hits").and_then(|h| h.get("hits")) {
                            let mut parsed = serde_json::Map::new();
                            if let Value::Array(arr) = array {
                                for obj in arr {
                                    if let Some(fields_map) = obj.get("fields") {
                                        let class = fields_map
                                            .get("identifiers.class")
                                            .and_then(|v| v.get(0))
                                            .cloned()
                                            .unwrap_or(Value::String("".into()));
                                        let class_key = class.as_str().unwrap_or("");
                                        let mut hit_ids = serde_json::Map::new();
                                        if let Value::Object(fmap) = fields_map {
                                            for (k, v) in fmap.iter() {
                                                let key = k
                                                    .replace("identifiers.", "")
                                                    .replace(".raw", "");
                                                hit_ids.insert(key, v.clone());
                                            }
                                        }
                                        parsed
                                            .insert(class_key.to_string(), Value::Object(hit_ids));
                                    }
                                }
                            }
                            if let Value::Object(ref mut base) = result_obj["result"] {
                                // merge or set `names` key with identifier info under `identifiers`
                                let mut names_obj = match base.get("names") {
                                    Some(Value::Object(o)) => o.clone(),
                                    _ => serde_json::Map::new(),
                                };
                                names_obj.insert("identifiers".to_string(), Value::Object(parsed));
                                base.insert("names".to_string(), Value::Object(names_obj));
                            }
                        }
                    }
                }
            }

            // lineage ranks / lca handling
            if let Some(inner_hits) = hit.get("inner_hits") {
                if let Some(lineage) = inner_hits.get("lineage") {
                    if ranks {
                        if let Some(arr) = lineage.get("hits").and_then(|h| h.get("hits")) {
                            let mut taxon_ranks = serde_json::Map::new();
                            if let Value::Array(a) = arr {
                                for obj in a {
                                    if let Some(fields_map) = obj.get("fields") {
                                        if let Some(rank_v) = fields_map.get("lineage.taxon_rank") {
                                            let rank = rank_v
                                                .get(0)
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("");
                                            let mut hit_ranks = serde_json::Map::new();
                                            if let Value::Object(fmap) = fields_map {
                                                for (k, v) in fmap.iter() {
                                                    let key = k
                                                        .replace("lineage.", "")
                                                        .replace(".raw", "");
                                                    hit_ranks.insert(key, v.clone());
                                                }
                                            }
                                            taxon_ranks
                                                .insert(rank.to_string(), Value::Object(hit_ranks));
                                        }
                                    }
                                }
                            }
                            if let Value::Object(ref mut base) = result_obj["result"] {
                                base.insert("ranks".to_string(), Value::Object(taxon_ranks));
                            }
                        }
                    }

                    if lca.is_some() {
                        if let Some(arr) = lineage.get("hits").and_then(|h| h.get("hits")) {
                            let mut lineage_vec = Vec::new();
                            if let Value::Array(a) = arr {
                                for obj in a {
                                    if let Some(fields_map) = obj.get("fields") {
                                        let taxon_id = fields_map
                                            .get("lineage.taxon_id")
                                            .and_then(|v| v.get(0))
                                            .cloned();
                                        let scientific_name = fields_map
                                            .get("lineage.scientific_name.raw")
                                            .and_then(|v| v.get(0))
                                            .cloned();
                                        let node_depth = fields_map
                                            .get("lineage.node_depth")
                                            .and_then(|v| v.get(0))
                                            .cloned();
                                        lineage_vec.push(json!({
                                            "taxon_id": taxon_id,
                                            "scientific_name": scientific_name,
                                            "node_depth": node_depth
                                        }));
                                    }
                                }
                            }
                            if let Value::Object(ref mut base) = result_obj["result"] {
                                base.insert("lineage".to_string(), Value::Array(lineage_vec));
                            }
                        }
                    }
                }
            }

            // small helper: normalize date-like strings (strip trailing midnight UTC)
            let normalize_value = |val: &Value| -> Value {
                if let Value::String(s) = val {
                    if s.contains("T00:00:00") {
                        if let Some(pos) = s.find('T') {
                            return Value::String(s[..pos].to_string());
                        }
                    }
                }
                val.clone()
            };

            // attributes -> fields conversion
            if let Some(attrs) = result_obj.get("result").and_then(|r| r.get("attributes")) {
                if let Value::Array(arr) = attrs {
                    let mut fields_map = serde_json::Map::new();
                    for attribute in arr {
                        if let Value::Object(attr_obj) = attribute {
                            let mut name_opt: Option<String> = None;
                            let mut field = serde_json::Map::new();
                            for (k, v) in attr_obj.iter() {
                                if k == "key" {
                                    if let Some(s) = v.as_str() {
                                        name_opt = Some(s.to_string());
                                    }
                                } else if k.ends_with("_value") {
                                    if k == "is_primary_value" {
                                        field.insert(
                                            "is_primary".to_string(),
                                            Value::Bool(v.as_bool().unwrap_or(false)),
                                        );
                                    } else {
                                        field.insert("value".to_string(), normalize_value(v));
                                    }
                                } else if k == "values" {
                                    field.insert("rawValues".to_string(), v.clone());
                                } else {
                                    field.insert(k.clone(), v.clone());
                                }
                            }
                            if let Some(name) = name_opt {
                                fields_map.insert(name, Value::Object(field));
                            }
                        }
                    }
                    if !fields_map.is_empty() {
                        if let Value::Object(ref mut base) = result_obj["result"] {
                            base.insert("fields".to_string(), Value::Object(fields_map));
                        }
                    }
                    // remove raw attributes
                    if let Value::Object(ref mut base) = result_obj["result"] {
                        base.remove("attributes");
                    }
                }
            }

            // merge per-attribute inner_hits into `result.result.fields` when available
            if inner_hits_option {
                if let Some(inner_hits) = hit.get("inner_hits") {
                    if let Some(attrs_hits) = inner_hits.get("attributes") {
                        if let Some(arr) = attrs_hits.get("hits").and_then(|h| h.get("hits")) {
                            if let Value::Array(a) = arr {
                                for obj in a {
                                    if let Some(fields_map) = obj.get("fields") {
                                        // extract attribute key
                                        if let Some(key_val) = fields_map.get("attributes.key") {
                                            if let Some(key_arr) = key_val.get(0) {
                                                if let Some(key_str) = key_arr.as_str() {
                                                    // build simplified inner hit map
                                                    let mut inner_map = serde_json::Map::new();
                                                    if let Value::Object(fmap) = fields_map {
                                                        for (k, v) in fmap.iter() {
                                                            let newk = k
                                                                .replace("attributes.", "")
                                                                .replace(".raw", "");
                                                            inner_map
                                                                .insert(newk, normalize_value(v));
                                                        }
                                                    }

                                                    // post-process aggregation_source if present (match JS behavior)
                                                    if let Some(agg_src_val) =
                                                        inner_map.get("aggregation_source")
                                                    {
                                                        if let Value::Array(arr) = agg_src_val {
                                                            let mut has_direct = false;
                                                            let mut has_descendant = false;
                                                            for v in arr.iter() {
                                                                if let Some(s) = v.as_str() {
                                                                    if s == "direct" {
                                                                        has_direct = true;
                                                                    }
                                                                    if s == "descendant" {
                                                                        has_descendant = true;
                                                                    }
                                                                }
                                                            }
                                                            // placeholder removed: we don't need to borrow `result_obj` here
                                                            // replace aggregation_source raw array with canonical form in inner_map
                                                            if has_direct {
                                                                inner_map.insert(
                                                                    "aggregation_source"
                                                                        .to_string(),
                                                                    Value::String(
                                                                        "direct".to_string(),
                                                                    ),
                                                                );
                                                                if has_descendant {
                                                                    inner_map.insert(
                                                                        "has_descendants"
                                                                            .to_string(),
                                                                        Value::Bool(true),
                                                                    );
                                                                }
                                                            } else {
                                                                inner_map.insert(
                                                                    "aggregation_source"
                                                                        .to_string(),
                                                                    Value::Array(arr.clone()),
                                                                );
                                                            }
                                                        }

                                                        // insert into result.result.fields[<key>].inner_hits array
                                                        if let Value::Object(ref mut base) =
                                                            result_obj["result"]
                                                        {
                                                            // ensure fields object exists
                                                            if !base.contains_key("fields") {
                                                                base.insert(
                                                                    "fields".to_string(),
                                                                    Value::Object(
                                                                        serde_json::Map::new(),
                                                                    ),
                                                                );
                                                            }
                                                            if let Some(Value::Object(ref mut fm)) =
                                                                base.get_mut("fields")
                                                            {
                                                                // ensure field entry exists
                                                                if !fm.contains_key(key_str) {
                                                                    fm.insert(
                                                                        key_str.to_string(),
                                                                        Value::Object(
                                                                            serde_json::Map::new(),
                                                                        ),
                                                                    );
                                                                }
                                                                // take ownership of the existing field object to avoid simultaneous borrows
                                                                let existing = fm.remove(key_str);
                                                                let mut owned_field = match existing
                                                                {
                                                                    Some(Value::Object(m)) => m,
                                                                    Some(other) => {
                                                                        // put it back unchanged if unexpected shape
                                                                        fm.insert(
                                                                            key_str.to_string(),
                                                                            other,
                                                                        );
                                                                        continue;
                                                                    }
                                                                    None => serde_json::Map::new(),
                                                                };

                                                                // merge aggregation_source/has_descendants into owned_field
                                                                if let Some(agg_val) = inner_map
                                                                    .get("aggregation_source")
                                                                {
                                                                    owned_field.insert(
                                                                        "aggregation_source"
                                                                            .to_string(),
                                                                        agg_val.clone(),
                                                                    );
                                                                }
                                                                if let Some(has_desc) =
                                                                    inner_map.get("has_descendants")
                                                                {
                                                                    owned_field.insert(
                                                                        "has_descendants"
                                                                            .to_string(),
                                                                        has_desc.clone(),
                                                                    );
                                                                }

                                                                // append inner_hits array
                                                                match owned_field
                                                                    .get_mut("inner_hits")
                                                                {
                                                                    Some(Value::Array(arr)) => arr
                                                                        .push(Value::Object(
                                                                            inner_map.clone(),
                                                                        )),
                                                                    _ => {
                                                                        owned_field.insert(
                                                                            "inner_hits"
                                                                                .to_string(),
                                                                            Value::Array(vec![
                                                                                Value::Object(
                                                                                    inner_map
                                                                                        .clone(),
                                                                                ),
                                                                            ]),
                                                                        );
                                                                    }
                                                                }

                                                                // insert the modified field back
                                                                fm.insert(
                                                                    key_str.to_string(),
                                                                    Value::Object(
                                                                        owned_field.clone(),
                                                                    ),
                                                                );

                                                                // if target_fields requests subset variants, duplicate into name:subset entries
                                                                if let Some(subsets) =
                                                                    target_fields.get(key_str)
                                                                {
                                                                    for subset in subsets.iter() {
                                                                        if subset != "value" {
                                                                            let new_name = format!(
                                                                                "{}:{}",
                                                                                key_str, subset
                                                                            );
                                                                            if !fm.contains_key(
                                                                                &new_name,
                                                                            ) {
                                                                                fm.insert(
                                                                                    new_name
                                                                                        .clone(),
                                                                                    Value::Object(
                                                                                        owned_field
                                                                                            .clone(
                                                                                            ),
                                                                                    ),
                                                                                );
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // reason: collect inner_hit scores/fields
            if reason {
                if let Some(inner_hits) = hit.get("inner_hits") {
                    let mut reason_vec = Vec::new();
                    if let Value::Object(map) = inner_hits {
                        for (_k, v) in map.iter() {
                            if let Some(hits_arr) = v.get("hits").and_then(|h| h.get("hits")) {
                                if let Value::Array(arr) = hits_arr {
                                    for inner in arr {
                                        let score =
                                            inner.get("_score").cloned().unwrap_or(Value::Null);
                                        let fields =
                                            inner.get("fields").cloned().unwrap_or(Value::Null);
                                        reason_vec.push(json!({"score": score, "fields": fields}));
                                    }
                                }
                            }
                        }
                    }
                    if !reason_vec.is_empty() {
                        if let Value::Object(ref mut base) = result_obj {
                            base.insert("reason".to_string(), Value::Array(reason_vec));
                        }
                    }
                }
            }
        }

        // push final result
        results.push(result_obj);
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_process_hits_debug_response() {
        let text = fs::read_to_string("docs/planning/debug_search_response_mammalia.json")
            .expect("read sample");
        let processed: Value = serde_json::from_str(&text).expect("parse json");
        // The saved file is a processed response (status/results). Convert to a
        // faux ES response with hits.hits so process_hits can parse it.
        let mut hits_array = Vec::new();
        if let Some(results) = processed.get("results").and_then(|r| r.as_array()) {
            for r in results {
                let idx = r.get("index").cloned().unwrap_or(Value::Null);
                let id = r.get("id").cloned().unwrap_or(Value::Null);
                let score = r.get("score").cloned().unwrap_or(Value::Null);
                let source = r.get("result").cloned().unwrap_or(Value::Null);
                hits_array
                    .push(json!({"_index": idx, "_id": id, "_score": score, "_source": source}));
            }
        }
        let body = json!({"hits": {"hits": hits_array}});
        let out = process_hits(&body, true, false, &[], true, None, true, false).expect("process");
        assert!(!out.is_empty());
    }

    #[test]
    fn test_process_hits_subset_aggregation() {
        // Construct a faux ES response with an inner_hit for an attribute
        let body = json!({
            "hits": { "hits": [
                {
                    "_index": "idx",
                    "_id": "1",
                    "_score": 1.0,
                    "_source": json!({}),
                    "inner_hits": {
                        "attributes": {
                            "hits": {
                                "hits": [
                                    {
                                        "_score": 1.0,
                                        "fields": {
                                            "attributes.key": ["genome_size"],
                                            "attributes.value": ["42"],
                                            "attributes.aggregation_source": ["direct", "descendant"]
                                        }
                                    }
                                ]
                            }
                        }
                    }
                }
            ] }
        });

        let f = vec!["genome_size:min".to_string()];
        let out = process_hits(&body, false, false, &f, false, None, true, false).expect("process");
        assert!(!out.is_empty());
        let res = &out[0];
        let fields = res
            .get("result")
            .and_then(|r| r.get("fields"))
            .and_then(|f| f.as_object())
            .expect("fields object");
        // base field exists
        assert!(fields.contains_key("genome_size"));
        // subset-named duplicate was created
        assert!(fields.contains_key("genome_size:min"));
        // aggregation_source canonicalized to "direct"
        let base_field = fields
            .get("genome_size")
            .and_then(|v| v.as_object())
            .unwrap();
        assert_eq!(
            base_field.get("aggregation_source").unwrap(),
            &Value::String("direct".into())
        );
    }
}
