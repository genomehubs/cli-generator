use genomehubs_query::plot_spec_to_vega_lite_json;
use serde_json::json;

#[test]
fn scratch_print_output() {
    let spec = json!({
        "report_type": "scatter",
        "x": {"field":"assembly_span", "label":"assembly_span", "scale":"linear", "value_type":"float"},
        "y": {"field":"assembly_level","label":"assembly_level","scale":"linear","value_type":"keyword"},
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
    println!("OUT={}", out);
}
