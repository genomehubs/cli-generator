//! Transform a v1 ES feature fixture into a v2 flat-field fixture.
//!
//! This binary is the canonical reference for how `genomehubs-index` must
//! populate the v2 feature index mapping.  Every field promotion described
//! in phase-18 is implemented here.
//!
//! ## Usage
//!
//! ```
//! cargo run -p genomehubs-api --bin transform_v1_fixture -- \
//!     tests/fixtures/feature_v1_GCA_905147045.json \
//!     tests/fixtures/feature_v2_GCA_905147045.json
//! ```
//!
//! ## V1 → V2 field promotion rules
//!
//! | Source (v1 `attributes` entry)           | v2 top-level field   |
//! |------------------------------------------|----------------------|
//! | `key=sequence_id`, `keyword_value`        | `sequence_id`        |
//! | `key=start`,       `long_value`           | `start`              |
//! | `key=end`,         `long_value`           | `end`                |
//! | `key=strand`,      `long_value`           | `strand`             |
//! | `key=length`,      `long_value`           | `length`             |
//! | *(derived from toplevel doc `length`)*    | `sequence_length`    |
//! | *(computed from start/end vs window grid)*| `container_ids`      |
//!
//! All promoted fields are also retained in `attributes` for v2 API compat.
//! Synthetic window docs (`primary_type = window_1m`) are generated at 1 Mbp
//! resolution from the toplevel sequence lengths.

use std::{collections::HashMap, env, fs, path::Path};

use serde_json::{json, Value};

const WINDOW_SIZE_1M: u64 = 1_000_000;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: transform_v1_fixture <input_v1.json> <output_v2.json>");
        std::process::exit(1);
    }
    let input_path = Path::new(&args[1]);
    let output_path = Path::new(&args[2]);

    let raw = fs::read_to_string(input_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", input_path.display()));
    let fixture: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("cannot parse JSON from {}: {e}", input_path.display()));

    let hits = fixture["hits"]
        .as_array()
        .unwrap_or_else(|| panic!("fixture must have a 'hits' array"));

    // Build a map from sequence_id → sequence_length from toplevel docs.
    let seq_lengths = build_sequence_length_map(hits);
    eprintln!("sequence length map: {} entries", seq_lengths.len());

    let mut v2_hits: Vec<Value> = Vec::with_capacity(hits.len());

    for hit in hits {
        v2_hits.push(promote_hit(hit, &seq_lengths));
    }

    // Generate synthetic window_1m docs from sequence lengths.
    let assembly_id = fixture["meta"]["assembly_id"].as_str().unwrap_or("unknown");
    let window_docs = generate_window_docs(assembly_id, &seq_lengths);
    eprintln!("generated {} synthetic window_1m docs", window_docs.len());
    v2_hits.extend(window_docs);

    let output = json!({
        "meta": fixture["meta"],
        "hits": v2_hits,
        "notes": {
            "v2_fields": ["sequence_id", "start", "end", "strand", "length",
                          "sequence_length", "container_ids"],
            "window_resolution": "1m",
            "window_size_bp": WINDOW_SIZE_1M,
            "overlap_policy": "any_overlap"
        }
    });

    let out_str = serde_json::to_string_pretty(&output).expect("failed to serialise output");
    fs::write(output_path, &out_str)
        .unwrap_or_else(|e| panic!("cannot write {}: {e}", output_path.display()));

    eprintln!(
        "Written {} v2 docs to {}",
        v2_hits.len(),
        output_path.display()
    );
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract a single attribute value by key from a v1 `_source.attributes` array.
fn attr_str<'a>(attributes: &'a [Value], key: &str) -> Option<&'a str> {
    attributes.iter().find_map(|a| {
        if a.get("key").and_then(|v| v.as_str()) == Some(key) {
            a.get("keyword_value").and_then(|v| v.as_str())
        } else {
            None
        }
    })
}

fn attr_i64(attributes: &[Value], key: &str) -> Option<i64> {
    attributes.iter().find_map(|a| {
        if a.get("key").and_then(|v| v.as_str()) == Some(key) {
            a.get("long_value").and_then(|v| v.as_i64())
        } else {
            None
        }
    })
}

/// Determine whether a v1 hit is a toplevel sequence document.
fn is_toplevel(attributes: &[Value]) -> bool {
    attributes.iter().any(|a| {
        a.get("key").and_then(|v| v.as_str()) == Some("feature_type")
            && a.get("keyword_value").and_then(|v| v.as_str()) == Some("toplevel")
    })
}

/// Build `sequence_id → length` from all toplevel docs in the fixture.
fn build_sequence_length_map(hits: &[Value]) -> HashMap<String, u64> {
    let mut map = HashMap::new();
    for hit in hits {
        let source = &hit["_source"];
        let attrs = source["attributes"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[]);
        if !is_toplevel(attrs) {
            continue;
        }
        // For toplevel docs the sequence_id is the feature_id
        let seq_id = source
            .get("feature_id")
            .and_then(|v| v.as_str())
            .or_else(|| attr_str(attrs, "sequence_id"))
            .unwrap_or("")
            .to_string();
        let length = attr_i64(attrs, "length").unwrap_or(0) as u64;
        if !seq_id.is_empty() && length > 0 {
            map.insert(seq_id, length);
        }
    }
    map
}

/// Compute the resolution-prefixed `container_ids` for a feature.
///
/// Uses ANY-overlap policy: a feature is assigned to every window whose interval
/// `[window_start, window_end)` overlaps `[start, end]`.
fn compute_container_ids(
    assembly_id: &str,
    sequence_id: &str,
    start: u64,
    end: u64,
    window_size: u64,
    prefix: &str,
) -> Vec<String> {
    if start > end {
        return vec![];
    }
    let first_bin = start / window_size;
    let last_bin = end / window_size;
    (first_bin..=last_bin)
        .map(|bin| format!("{}:{}:{}:{}", prefix, assembly_id, sequence_id, bin))
        .collect()
}

/// Promote v1 hit fields to v2 and add `sequence_length` + `container_ids`.
fn promote_hit(hit: &Value, seq_lengths: &HashMap<String, u64>) -> Value {
    let mut source = hit["_source"].clone();
    let attrs = source["attributes"].as_array().cloned().unwrap_or_default();

    let assembly_id = source
        .get("assembly_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Resolve feature_type — use the attributes array value for canonical type
    let primary_type_from_attr = attr_str(&attrs, "feature_type").map(|s| s.to_string());
    let existing_primary_type = source
        .get("primary_type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let primary_type = primary_type_from_attr
        .or(existing_primary_type)
        .unwrap_or_else(|| "unknown".to_string());

    // Promote positional fields
    let sequence_id = attr_str(&attrs, "sequence_id")
        .map(|s| s.to_string())
        .unwrap_or_default();
    let start = attr_i64(&attrs, "start").unwrap_or(0) as u64;
    let end = attr_i64(&attrs, "end").unwrap_or(0) as u64;
    let strand = attr_i64(&attrs, "strand").unwrap_or(1) as i64;
    let length = attr_i64(&attrs, "length").unwrap_or(0) as u64;

    // Enrich sequence_length from the sequence length map
    let sequence_length = if primary_type == "toplevel" {
        // For toplevel docs, sequence_length == their own length
        length
    } else {
        seq_lengths.get(&sequence_id).copied().unwrap_or(0)
    };

    // Compute container_ids at 1 Mbp resolution
    let container_ids = if !sequence_id.is_empty() && end > 0 && primary_type != "toplevel" {
        compute_container_ids(
            &assembly_id,
            &sequence_id,
            start,
            end,
            WINDOW_SIZE_1M,
            "win_1m",
        )
    } else {
        vec![]
    };

    // Merge promoted fields into source
    let source_obj = source.as_object_mut().expect("_source must be an object");
    source_obj.insert("primary_type".to_string(), json!(primary_type));
    source_obj.insert("sequence_id".to_string(), json!(sequence_id));
    source_obj.insert("start".to_string(), json!(start));
    source_obj.insert("end".to_string(), json!(end));
    source_obj.insert("strand".to_string(), json!(strand));
    source_obj.insert("length".to_string(), json!(length));
    source_obj.insert("sequence_length".to_string(), json!(sequence_length));
    source_obj.insert("container_ids".to_string(), json!(container_ids));

    json!({
        "_index": hit["_index"],
        "_id":    hit["_id"],
        "_score": hit["_score"],
        "_source": source,
    })
}

/// Generate synthetic `window_1m` feature documents from sequence lengths.
///
/// One document per (assembly, sequence, bin). Stats fields (`gc`, `coverage`,
/// `repeat_density`) are omitted — they must be populated by the real indexer.
/// This provides the structural template only.
fn generate_window_docs(assembly_id: &str, seq_lengths: &HashMap<String, u64>) -> Vec<Value> {
    let mut docs = Vec::new();
    for (seq_id, &seq_len) in seq_lengths {
        let num_windows = seq_len.div_ceil(WINDOW_SIZE_1M);
        for bin in 0..num_windows {
            let win_start = bin * WINDOW_SIZE_1M;
            let win_end = (win_start + WINDOW_SIZE_1M).min(seq_len);
            let win_len = win_end - win_start;
            let feature_id = format!("win_1m:{}:{}:{}", assembly_id, seq_id, bin);
            docs.push(json!({
                "_index": "feature--synthetic",
                "_id":    feature_id,
                "_score": null,
                "_source": {
                    "assembly_id":     assembly_id,
                    "feature_id":      feature_id,
                    "primary_type":    "window_1m",
                    "sequence_id":     seq_id,
                    "start":           win_start,
                    "end":             win_end,
                    "length":          win_len,
                    "sequence_length": seq_len,
                    "container_ids":   [],
                    // Placeholder attribute entries — real values from indexer
                    "attributes": [
                        {"key": "feature_type", "keyword_value": "window_1m"},
                        {"key": "gc",            "3dp_value": null},
                        {"key": "coverage",      "3dp_value": null},
                        {"key": "repeat_density","3dp_value": null}
                    ]
                }
            }));
        }
    }
    docs
}
