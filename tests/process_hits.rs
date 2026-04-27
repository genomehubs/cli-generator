use cli_generator::core::process_hits::process_hits;
use serde_json::Value;
use std::fs;

#[test]
fn integration_process_hits() {
    let text = fs::read_to_string("docs/planning/debug_search_response_mammalia.json")
        .expect("read sample");
    let body: Value = serde_json::from_str(&text).expect("parse json");
    // Convert processed saved response into faux ES hits.hits structure
    let mut hits_array = Vec::new();
    if let Some(results) = body.get("results").and_then(|r| r.as_array()) {
        for r in results {
            let idx = r.get("index").cloned().unwrap_or(Value::Null);
            let id = r.get("id").cloned().unwrap_or(Value::Null);
            let score = r.get("score").cloned().unwrap_or(Value::Null);
            let source = r.get("result").cloned().unwrap_or(Value::Null);
            hits_array.push(
                serde_json::json!({"_index": idx, "_id": id, "_score": score, "_source": source}),
            );
        }
    }
    let faux = serde_json::json!({"hits": {"hits": hits_array}});
    let out = process_hits(&faux, true, false, &[], true, None, true, false).expect("process");
    assert!(!out.is_empty());
}
