//! Painting-mode output shaping.
//!
//! For `report: painting`, a single assembly is shown as a set of
//! chromosomal segments each carrying a category colour.  Segments come
//! directly from windowed points (or individual positions when
//! `window_size` is null).

use serde_json::{json, Value};

use super::window::{RawPoint, WindowedPoint};

/// Build the painting `segments` array from pre-windowed points.
pub fn build_painting_segments(windowed: &[WindowedPoint], cat_field: Option<&str>) -> Value {
    let segments: Vec<Value> = windowed
        .iter()
        .flat_map(|w| {
            if w.cats.is_empty() {
                vec![json!({
                    "sequence_id": w.sequence_id,
                    "start": w.window_start,
                    "end": w.window_end,
                    "count": w.count
                })]
            } else {
                w.cats
                    .iter()
                    .map(|(cat_val, count)| {
                        json!({
                            "sequence_id": w.sequence_id,
                            "start": w.window_start,
                            "end": w.window_end,
                            "cat": cat_val,
                            "count": count
                        })
                    })
                    .collect::<Vec<_>>()
            }
        })
        .collect();

    json!({
        "cat": cat_field,
        "segments": segments
    })
}

/// Build painting segments from raw (non-windowed) individual positions.
pub fn build_painting_segments_raw(points: &[RawPoint], cat_field: Option<&str>) -> Value {
    let segments: Vec<Value> = points
        .iter()
        .map(|p| {
            let mut seg = json!({
                "sequence_id": p.sequence_id,
                "start": p.start
            });
            if let Some(cat) = &p.cat_value {
                seg["cat"] = json!(cat);
            }
            seg
        })
        .collect();

    json!({
        "cat": cat_field,
        "segments": segments
    })
}
