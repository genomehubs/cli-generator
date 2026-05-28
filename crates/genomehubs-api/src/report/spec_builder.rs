//! Server-side PlotSpec construction helpers.
//!
//! Build a fully-resolved `PlotSpec` from a report payload and optional
//! `display` hints. This lives in the API crate because it may consult
//! server-side knowledge (report JSON shapes) and is not intended for the
//! WASM-local build path.

use serde_json::Value;

use genomehubs_query::report::display::TickLabelPlacement;
use genomehubs_query::report::plot_spec::{AxisMeta, PlotReportType, SeriesMeta};
use genomehubs_query::report::DisplaySpec;
use genomehubs_query::report::PlotSpec;

fn parse_display(display: Option<&Value>) -> DisplaySpec {
    if let Some(dv) = display {
        if let Some(s) = dv.as_str() {
            serde_yaml::from_str(s).unwrap_or_default()
        } else {
            serde_json::from_value(dv.clone()).unwrap_or_default()
        }
    } else {
        DisplaySpec::default()
    }
}

fn domain_from_value(v: Option<&Value>) -> [f64; 2] {
    if let Some(Value::Array(arr)) = v {
        if arr.len() >= 2 {
            let a = arr[0].as_f64().unwrap_or(0.0);
            let b = arr[1].as_f64().unwrap_or(a + 1.0);
            return [a, b];
        }
    }
    [0.0, 1.0]
}

fn make_axis_meta(
    field: &str,
    scale: Option<&str>,
    domain_val: Option<&Value>,
    value_type_hint: Option<&str>,
) -> AxisMeta {
    let domain = domain_from_value(domain_val);
    let scale_s = scale
        .map(|s| s.to_string())
        .unwrap_or_else(|| "linear".to_string());
    let value_type = value_type_hint.map(|s| s.to_string()).unwrap_or_else(|| {
        if domain != [0.0, 1.0] {
            "float".to_string()
        } else {
            "keyword".to_string()
        }
    });

    let tick_label_placement = if value_type == "keyword" {
        TickLabelPlacement::BetweenTicks
    } else {
        TickLabelPlacement::OnTick
    };

    AxisMeta {
        field: field.to_string(),
        label: None,
        scale: scale_s,
        domain,
        tick_values: vec![],
        tick_labels: vec![],
        value_type,
        tick_label_placement,
        tick_label_stride: 1,
        tick_label_max_length: None,
    }
}

fn build_series_from_cats(cats: Option<&Value>) -> Vec<SeriesMeta> {
    if let Some(Value::Array(arr)) = cats {
        arr.iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .map(|key| SeriesMeta {
                key: key.clone(),
                label: key,
                color: None,
            })
            .collect()
    } else {
        Vec::new()
    }
}

/// Build a `PlotSpec` from a v3 report payload and optional `display` hints.
///
/// `report_type` is the canonical report string (e.g. "histogram", "scatter").
/// `report_data` is the JSON returned by the report handlers. `display` may be
/// a YAML string or JSON object and will be merged into the resulting spec.
pub fn build_plot_spec(
    report_type: &str,
    report_data: &Value,
    display: Option<&Value>,
) -> Result<PlotSpec, String> {
    let pr = PlotReportType::parse(report_type).unwrap_or(PlotReportType::Histogram);
    let display_spec = parse_display(display);

    // Normalise histogram display options: prefer explicit `mode` when present;
    // otherwise derive it from legacy boolean flags for compatibility.
    if let Some(hist) = display_spec.histogram.as_ref() {
        // nothing to do when mode already set
        if hist.mode.is_none() {
            // We'll fill in a sensible default later when serialising the
            // PlotSpec; clone and adjust the DisplaySpec to ensure the
            // resulting `plot_spec.display.histogram.mode` is always present
            // for clients and converters.
        }
    }

    // Ensure the returned DisplaySpec contains a canonical `histogram.mode`
    // when histogram options are present. This keeps downstream converters
    // simple: `mode` is authoritative and overrides `stacked`/`cumulative`.
    let mut display_spec = display_spec;
    if let Some(hist_opts) = display_spec.histogram.as_mut() {
        if hist_opts.mode.is_none() {
            if hist_opts.stacked.unwrap_or(false) {
                hist_opts.mode = Some("stacked".to_string());
            } else if hist_opts.cumulative.unwrap_or(false) {
                hist_opts.mode = Some("cumulative".to_string());
            } else {
                // default behaviour remains stacked for backward-compatibility
                hist_opts.mode = Some("stacked".to_string());
            }
        }
        // Keep boolean `stacked` consistent with `mode` for consumers
        match hist_opts.mode.as_deref() {
            Some("stacked") => hist_opts.stacked = Some(true),
            Some("grouped") | Some("facet") | Some("cumulative") => hist_opts.stacked = Some(false),
            _ => {}
        }
    }

    // Default empty values
    let mut x: Option<AxisMeta> = None;
    let mut y: Option<AxisMeta> = None;
    let z: Option<AxisMeta> = None;
    let mut series: Vec<SeriesMeta> = Vec::new();

    match pr {
        PlotReportType::Histogram => {
            if let Some(x_obj) = report_data.get("x") {
                if let Some(field) = x_obj.get("field").and_then(|v| v.as_str()) {
                    let scale = x_obj.get("scale").and_then(|v| v.as_str());
                    let domain = x_obj.get("domain");
                    let value_type_hint = x_obj.get("value_type").and_then(|v| v.as_str());
                    x = Some(make_axis_meta(field, scale, domain, value_type_hint));
                    if let Some(meta) = x.as_mut() {
                        let axis_opts = display_spec
                            .histogram
                            .as_ref()
                            .and_then(|h| h.x_axis.as_ref());
                        genomehubs_query::report::spec_builder::resolve_axis_display(
                            meta, axis_opts,
                        );

                        if meta.value_type == "keyword" {
                            if let Some(buckets) =
                                report_data.get("buckets").and_then(|v| v.as_array())
                            {
                                let labels: Vec<String> = buckets
                                    .iter()
                                    .map(|b| {
                                        b.get("label")
                                            .and_then(|l| l.as_str())
                                            .map(|s| s.to_string())
                                            .unwrap_or_else(|| {
                                                if let Some(kv) = b.get("key") {
                                                    if let Some(s) = kv.as_str() {
                                                        s.to_string()
                                                    } else {
                                                        kv.to_string()
                                                    }
                                                } else {
                                                    String::new()
                                                }
                                            })
                                    })
                                    .collect();
                                if !labels.is_empty() {
                                    meta.tick_labels = labels;
                                }
                            }
                        } else {
                            // Numeric: try to derive bin boundaries from buckets
                            if let Some(buckets) =
                                report_data.get("buckets").and_then(|v| v.as_array())
                            {
                                let mut keys_num: Vec<f64> = Vec::new();
                                for b in buckets.iter() {
                                    if let Some(k) = b.get("key").and_then(|v| v.as_f64()) {
                                        keys_num.push(k);
                                    } else if let Some(id_s) = b.get("id").and_then(|v| v.as_str())
                                    {
                                        if let Ok(n) = id_s.parse::<f64>() {
                                            keys_num.push(n);
                                        }
                                    }
                                }
                                if !keys_num.is_empty() {
                                    keys_num.sort_by(|a, b| {
                                        a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                                    });
                                    let width = if keys_num.len() >= 2 {
                                        keys_num[1] - keys_num[0]
                                    } else if let Some(domain_arr) =
                                        x_obj.get("domain").and_then(|d| d.as_array())
                                    {
                                        if domain_arr.len() >= 2 {
                                            let lo = domain_arr[0].as_f64().unwrap_or(0.0);
                                            let hi = domain_arr[1].as_f64().unwrap_or(lo + 1.0);
                                            let tick_count = x_obj
                                                .get("tickCount")
                                                .and_then(|v| v.as_u64())
                                                .unwrap_or(10)
                                                as f64;
                                            (hi - lo) / tick_count.max(1.0)
                                        } else {
                                            1.0
                                        }
                                    } else {
                                        1.0
                                    };
                                    let mut boundaries = keys_num.clone();
                                    let last_right = if keys_num.len() >= 2 {
                                        keys_num[keys_num.len() - 1] + (keys_num[1] - keys_num[0])
                                    } else {
                                        keys_num[0] + width
                                    };
                                    boundaries.push(last_right);
                                    meta.tick_values = boundaries;
                                }
                            }
                        }
                    }
                }
            }
            // Series from cats
            series = build_series_from_cats(report_data.get("cats"));
            // Y axis: histogram counts (doc_count) — ensure converter receives
            // authoritative axis metadata so it does not need to guess.
            if let Some(buckets) = report_data.get("buckets").and_then(|v| v.as_array()) {
                let counts: Vec<f64> = buckets
                    .iter()
                    .map(|b| b.get("doc_count").and_then(|c| c.as_f64()).unwrap_or(0.0))
                    .collect();
                let max = counts.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let domain = if max.is_finite() {
                    [0.0, if max > 0.0 { max } else { 1.0 }]
                } else {
                    [0.0, 1.0]
                };
                y = Some(AxisMeta {
                    field: "doc_count".to_string(),
                    label: Some("count".to_string()),
                    scale: "linear".to_string(),
                    domain,
                    tick_values: vec![],
                    tick_labels: vec![],
                    value_type: "integer".to_string(),
                    tick_label_placement: TickLabelPlacement::OnTick,
                    tick_label_stride: 1,
                    tick_label_max_length: None,
                });
            } else {
                y = Some(make_axis_meta(
                    "doc_count",
                    Some("linear"),
                    None,
                    Some("integer"),
                ));
            }
        }
        PlotReportType::Scatter => {
            if let Some(x_obj) = report_data.get("x") {
                if let Some(field) = x_obj.get("field").and_then(|v| v.as_str()) {
                    let scale = x_obj.get("scale").and_then(|v| v.as_str());
                    let domain = x_obj.get("domain");
                    let value_type_hint = x_obj.get("value_type").and_then(|v| v.as_str());
                    x = Some(make_axis_meta(field, scale, domain, value_type_hint));
                    if let Some(meta) = x.as_mut() {
                        let axis_opts = display_spec
                            .scatter
                            .as_ref()
                            .and_then(|s| s.x_axis.as_ref())
                            .or_else(|| {
                                display_spec
                                    .histogram
                                    .as_ref()
                                    .and_then(|h| h.x_axis.as_ref())
                            });
                        genomehubs_query::report::spec_builder::resolve_axis_display(
                            meta, axis_opts,
                        );

                        if meta.value_type == "keyword" {
                            if let Some(buckets) =
                                report_data.get("buckets").and_then(|v| v.as_array())
                            {
                                let labels: Vec<String> = buckets
                                    .iter()
                                    .map(|b| {
                                        b.get("label")
                                            .and_then(|l| l.as_str())
                                            .map(|s| s.to_string())
                                            .unwrap_or_else(|| {
                                                if let Some(kv) = b.get("id") {
                                                    if let Some(s) = kv.as_str() {
                                                        s.to_string()
                                                    } else {
                                                        kv.to_string()
                                                    }
                                                } else {
                                                    String::new()
                                                }
                                            })
                                    })
                                    .collect();
                                if !labels.is_empty() {
                                    meta.tick_labels = labels;
                                }
                            }
                        } else if let Some(buckets) =
                            report_data.get("buckets").and_then(|v| v.as_array())
                        {
                            let mut keys_num: Vec<f64> = Vec::new();
                            for b in buckets.iter() {
                                if let Some(k) = b.get("key").and_then(|v| v.as_f64()) {
                                    keys_num.push(k);
                                } else if let Some(id_s) = b.get("id").and_then(|v| v.as_str()) {
                                    if let Ok(n) = id_s.parse::<f64>() {
                                        keys_num.push(n);
                                    }
                                }
                            }
                            if !keys_num.is_empty() {
                                keys_num.sort_by(|a, b| {
                                    a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                                });
                                let width = if keys_num.len() >= 2 {
                                    keys_num[1] - keys_num[0]
                                } else if let Some(domain_arr) =
                                    x_obj.get("domain").and_then(|d| d.as_array())
                                {
                                    if domain_arr.len() >= 2 {
                                        let lo = domain_arr[0].as_f64().unwrap_or(0.0);
                                        let hi = domain_arr[1].as_f64().unwrap_or(lo + 1.0);
                                        let tick_count = x_obj
                                            .get("tickCount")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(10)
                                            as f64;
                                        (hi - lo) / tick_count.max(1.0)
                                    } else {
                                        1.0
                                    }
                                } else {
                                    1.0
                                };
                                let mut boundaries = keys_num.clone();
                                let last_right = if keys_num.len() >= 2 {
                                    keys_num[keys_num.len() - 1] + (keys_num[1] - keys_num[0])
                                } else {
                                    keys_num[0] + width
                                };
                                boundaries.push(last_right);
                                meta.tick_values = boundaries;
                            }
                        }
                    }
                }
            }
            if let Some(y_obj) = report_data.get("y") {
                if let Some(field) = y_obj.get("field").and_then(|v| v.as_str()) {
                    let scale = y_obj.get("scale").and_then(|v| v.as_str());
                    let domain = y_obj.get("domain");
                    let value_type_hint = y_obj.get("value_type").and_then(|v| v.as_str());
                    y = Some(make_axis_meta(field, scale, domain, value_type_hint));
                    if let Some(meta) = y.as_mut() {
                        let axis_opts = display_spec
                            .scatter
                            .as_ref()
                            .and_then(|s| s.y_axis.as_ref())
                            .or_else(|| {
                                display_spec
                                    .histogram
                                    .as_ref()
                                    .and_then(|h| h.y_axis.as_ref())
                            });
                        genomehubs_query::report::spec_builder::resolve_axis_display(
                            meta, axis_opts,
                        );

                        if meta.value_type == "keyword" {
                            // Prefer explicit `yBucketLabels` when the server has
                            // provided human-readable labels for taxon-rank buckets.
                            // If `yBucketLabels` exists but is empty, fall back to
                            // `yBuckets` so categorical axes still receive tick labels.
                            let mut labels: Vec<String> = Vec::new();
                            if let Some(lbls) =
                                report_data.get("yBucketLabels").and_then(|v| v.as_array())
                            {
                                labels = lbls
                                    .iter()
                                    .map(|v| v.as_str().unwrap_or("").to_string())
                                    .collect();
                            }
                            if labels.is_empty() {
                                if let Some(yb) =
                                    report_data.get("yBuckets").and_then(|v| v.as_array())
                                {
                                    labels = yb
                                        .iter()
                                        .map(|v| v.as_str().unwrap_or("").to_string())
                                        .collect();
                                }
                            }
                            if !labels.is_empty() {
                                meta.tick_labels = labels;
                            }
                        } else if let Some(yb) =
                            report_data.get("yBuckets").and_then(|v| v.as_array())
                        {
                            let mut y_keys: Vec<f64> = Vec::new();
                            for k in yb.iter() {
                                if let Some(n) = k.as_f64() {
                                    y_keys.push(n);
                                } else if let Some(s) = k.as_str() {
                                    if let Ok(n) = s.parse::<f64>() {
                                        y_keys.push(n);
                                    }
                                }
                            }
                            if !y_keys.is_empty() {
                                y_keys.sort_by(|a, b| {
                                    a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                                });
                                let height = if y_keys.len() >= 2 {
                                    y_keys[1] - y_keys[0]
                                } else if let Some(domain_arr) =
                                    y_obj.get("domain").and_then(|d| d.as_array())
                                {
                                    if domain_arr.len() >= 2 {
                                        let lo = domain_arr[0].as_f64().unwrap_or(0.0);
                                        let hi = domain_arr[1].as_f64().unwrap_or(lo + 1.0);
                                        let tick_count = y_obj
                                            .get("tickCount")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(10)
                                            as f64;
                                        (hi - lo) / tick_count.max(1.0)
                                    } else {
                                        1.0
                                    }
                                } else {
                                    1.0
                                };
                                let mut boundaries = y_keys.clone();
                                let last_top = if y_keys.len() >= 2 {
                                    y_keys[y_keys.len() - 1] + (y_keys[1] - y_keys[0])
                                } else {
                                    y_keys[0] + height
                                };
                                boundaries.push(last_top);
                                meta.tick_values = boundaries;
                            }
                        }
                    }
                }
            }
            series = build_series_from_cats(report_data.get("cats"));
        }
        PlotReportType::CountPerRank => {
            // Count per rank: x is rank labels (keyword), y is count
            if let Some(buckets) = report_data.get("buckets").and_then(|v| v.as_array()) {
                // pick first bucket's rank field name via keys
                // we'll construct a dummy x axis named "rank"
                x = Some(make_axis_meta(
                    "rank",
                    Some("ordinal"),
                    None,
                    Some("keyword"),
                ));
                // y domain from counts
                let counts: Vec<f64> = buckets
                    .iter()
                    .map(|b| b.get("count").and_then(|c| c.as_f64()).unwrap_or(0.0))
                    .collect();
                let min = counts.iter().cloned().fold(f64::INFINITY, f64::min);
                let max = counts.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let domain = if min.is_finite() && max.is_finite() {
                    [min, if max > min { max } else { min + 1.0 }]
                } else {
                    [0.0, 1.0]
                };
                y = Some(AxisMeta {
                    field: "count".to_string(),
                    label: Some("count".to_string()),
                    scale: "linear".to_string(),
                    domain,
                    tick_values: vec![],
                    tick_labels: vec![],
                    value_type: "integer".to_string(),
                    tick_label_placement: TickLabelPlacement::OnTick,
                    tick_label_stride: 1,
                    tick_label_max_length: None,
                });
            }
        }
        PlotReportType::Sources => {
            // Sources returns buckets; treat as categorical x + numeric y
            if let Some(buckets) = report_data.get("buckets").and_then(|v| v.as_array()) {
                x = Some(make_axis_meta(
                    "source",
                    Some("ordinal"),
                    None,
                    Some("keyword"),
                ));
                let counts: Vec<f64> = buckets
                    .iter()
                    .map(|b| b.get("count").and_then(|c| c.as_f64()).unwrap_or(0.0))
                    .collect();
                let min = counts.iter().cloned().fold(f64::INFINITY, f64::min);
                let max = counts.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let domain = if min.is_finite() && max.is_finite() {
                    [min, if max > min { max } else { min + 1.0 }]
                } else {
                    [0.0, 1.0]
                };
                y = Some(AxisMeta {
                    field: "count".to_string(),
                    label: Some("count".to_string()),
                    scale: "linear".to_string(),
                    domain,
                    tick_values: vec![],
                    tick_labels: vec![],
                    value_type: "integer".to_string(),
                    tick_label_placement: TickLabelPlacement::OnTick,
                    tick_label_stride: 1,
                    tick_label_max_length: None,
                });
            }
        }
        PlotReportType::Tree
        | PlotReportType::Map
        | PlotReportType::Arc
        | PlotReportType::Oxford
        | PlotReportType::Ribbon
        | PlotReportType::Painting => {
            // Positional / complex reports: rely on display/data only. Axis
            // metadata for these are highly report-specific and are handled by
            // the positional endpoint's own PlotSpec builder. Here we provide
            // a conservative default: embed the full report JSON as data and
            // leave axes empty.
        }
    }

    // Build `cat` AxisMeta from report_data["cat"] when present. This keeps
    // categorical metadata (field, value_type, scale, tick labels) in the
    // canonical PlotSpec so converters can deterministically render legends
    // and category axes.
    let mut cat_meta: Option<AxisMeta> = None;
    if let Some(cat_obj) = report_data.get("cat") {
        if let Some(field) = cat_obj.get("field").and_then(|v| v.as_str()) {
            let scale = cat_obj.get("scale").and_then(|v| v.as_str());
            let domain = cat_obj.get("domain");
            let value_type_hint = cat_obj.get("value_type").and_then(|v| v.as_str());
            let mut cm = make_axis_meta(field, scale, domain, value_type_hint);
            // Apply any top-level display label for categories if provided
            if let Some(label) = display_spec.cat_label.as_ref() {
                cm.label = Some(label.clone());
            }
            // Populate tick labels for categorical cat axes from report_data["cats"]
            if cm.value_type == "keyword" {
                if let Some(cats_arr) = report_data.get("cats").and_then(|v| v.as_array()) {
                    let labels: Vec<String> = cats_arr
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                    if !labels.is_empty() {
                        cm.tick_labels = labels;
                    }
                }
            } else {
                // numeric cat axes: try to populate numeric tick_values from `cats`
                if let Some(cats_arr) = report_data.get("cats").and_then(|v| v.as_array()) {
                    let mut nums: Vec<f64> = Vec::new();
                    for v in cats_arr.iter() {
                        if let Some(n) = v.as_f64() {
                            nums.push(n);
                        } else if let Some(s) = v.as_str() {
                            if let Ok(n) = s.parse::<f64>() {
                                nums.push(n);
                            }
                        }
                    }
                    if !nums.is_empty() {
                        nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                        let width = if nums.len() >= 2 {
                            nums[1] - nums[0]
                        } else {
                            1.0
                        };
                        let mut boundaries = nums.clone();
                        let last = nums[nums.len() - 1] + width;
                        boundaries.push(last);
                        cm.tick_values = boundaries;
                    }
                }
            }
            cat_meta = Some(cm);
        }
    }

    let plot_spec = PlotSpec {
        report_type: pr,
        x,
        y,
        cat: cat_meta,
        z,
        series,
        display: display_spec,
        data: report_data.clone(),
    };

    Ok(plot_spec)
}
