use serde_json::{json, Value};

use reqwest::Client;

use genomehubs_query::report::plot_spec_to_vega_lite_json;

// This is an end-to-end test that posts to a running API at localhost:3000.
// It iterates a small set of axis-type combinations in both raw and binned
// modes and asserts the server-provided `plot_spec` includes axis `value_type`
// and that the converter produces a valid Vega-Lite spec (no error payload).

#[tokio::test]
async fn e2e_scatter_axis_type_permutations() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let base_url =
        std::env::var("GH_API_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let url = format!("{}/api/v3/report", base_url);

    // Axis candidates: rank-like (genus), numeric, keyword, date
    let axes = vec!["genus", "assembly_span", "assembly_level", "assembly_date"];

    for x in &axes {
        for y in &axes {
            for &threshold in &[1000_i64, 10_i64] {
                let req_body = json!({
                    "query": {"index":"taxon", "taxa": ["canidae"], "taxon_filter_type": "tree"},
                    "params": {},
                    "report": {"report":"scatter", "x": x, "y": y, "scatter_threshold": threshold},
                    "include_plot_spec": true,
                    "display": {"title": format!("scatter {} vs {} thresh {}", x, y, threshold)}
                });

                let resp = client
                    .post(&url)
                    .header("accept", "application/json")
                    .json(&req_body)
                    .send()
                    .await?;

                let status = resp.status();
                if !status.is_success() {
                    let body = resp.text().await.unwrap_or_default();
                    panic!(
                        "API returned non-success for x={} y={} threshold={}: status={} body={}",
                        x, y, threshold, status, body
                    );
                }

                let resp_json: Value = resp.json().await?;

                dbg!(&resp_json);

                let plot_spec = resp_json
                    .get("plot_spec")
                    .cloned()
                    .ok_or_else(|| format!("no plot_spec in response for x={} y={}", x, y))?;

                // Server must provide authoritative axis value types
                let x_vt = plot_spec
                    .get("x")
                    .and_then(|v| v.get("value_type"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| format!("plot_spec.x.value_type missing for x={} y={}", x, y))?;
                let y_vt = plot_spec
                    .get("y")
                    .and_then(|v| v.get("value_type"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| format!("plot_spec.y.value_type missing for x={} y={}", x, y))?;

                eprintln!(
                    "Testing x={} ({}) y={} ({}) threshold={}",
                    x, x_vt, y, y_vt, threshold
                );

                // Convert plot_spec to Vega-Lite using the workspace converter
                let ps_str = serde_json::to_string(&plot_spec)?;
                let vl_json_str = plot_spec_to_vega_lite_json(&ps_str);
                let vl_val: Value = serde_json::from_str(&vl_json_str).map_err(|e| {
                    format!(
                        "converter returned invalid JSON: {} -- payload: {}",
                        e, vl_json_str
                    )
                })?;

                if vl_val.get("error").is_some() {
                    panic!(
                        "converter returned error for x={} y={} threshold={}: {}",
                        x, y, threshold, vl_json_str
                    );
                }

                // Determine mark type (supports either string or object `mark` forms)
                let mark_type = match vl_val.get("mark") {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Object(obj)) => obj
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string(),
                    _ => "".to_string(),
                };

                if threshold >= 1000 {
                    assert!(mark_type == "point" || mark_type == "circle" || mark_type == "symbol", "expected point-like mark for raw mode but got {:?} for x={} y={} threshold={}", mark_type, x, y, threshold);
                } else {
                    assert!(mark_type == "rect" || mark_type == "bar", "expected rect/bar mark for binned mode but got {:?} for x={} y={} threshold={}", mark_type, x, y, threshold);
                }
            }
        }
    }

    Ok(())
}
