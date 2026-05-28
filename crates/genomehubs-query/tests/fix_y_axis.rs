use genomehubs_query::plot_spec_to_vega_lite_json;
use serde_json::json;

#[test]
fn y_axis_uses_yBuckets_for_raw_points() {
    let spec = json!({
        "report_type": "scatter",
        "x": {"field":"assembly_span", "label":"assembly_span", "scale":"linear"},
        "y": {"field":"assembly_level","label":"assembly_level","scale":"linear"},
        "data": {
            "buckets": [{"id":"1","label":"B1"},{"id":"2","label":"B2"}],
            "yBuckets": ["Scaffold","Chromosome"],
            "rawData": {
                "all": [
                    {"x":1.0,"y":"Scaffold","cat":"all"},
                    {"x":2.0,"y":"Chromosome","cat":"all"}
                ]
            }
        }
    });

    let out = plot_spec_to_vega_lite_json(&spec.to_string());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let y_values = parsed.pointer("/encoding/y/axis/values").unwrap();
    let arr = y_values.as_array().unwrap();
    assert_eq!(arr[0].as_str().unwrap(), "Scaffold");
    assert_eq!(arr[1].as_str().unwrap(), "Chromosome");
}
