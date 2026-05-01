# Phase 6: Report Types

**Depends on:** Phase 5 (AggBuilder, bounds, pipeline)
**Blocks:** Phase 7 (arc is a distinct report sub-type; uses the route pattern from here)
**Estimated scope:** ~4 new files, 3 new parse functions

---

## Goal

Implement `POST /api/v3/report` — the single report endpoint that handles all
non-arc report types:

| Report               | Key                 | ES technique                                      | When                      |
| -------------------- | ------------------- | ------------------------------------------------- | ------------------------- |
| Histogram (1D)       | `histogram`         | histogram/date_histogram/terms agg                | always                    |
| Histogram + cat      | `histogram` + `cat` | 2-level terms + histogram                         | when `cat:` set           |
| 2D histogram/heatmap | `histogram` + `y`   | composite terms/histogram                         | when `y:` set             |
| Scatter (raw)        | `scatter`           | top-N `_search` hits                              | count < scatter_threshold |
| Scatter (grid)       | `scatter`           | 2D histogram                                      | count ≥ scatter_threshold |
| xPerRank             | `xPerRank`          | terms agg on `taxon_rank`, nested stats           | always                    |
| Sources              | `sources`           | terms on `source`                                 | always                    |
| Tree                 | `tree`              | reverse_nested lineage agg → Newick serialisation | always                    |
| Map                  | `map`               | geohash_grid agg                                  | always                    |

Add `ReportBuilder` to `crates/genomehubs-query` and expose via PyO3/WASM/extendr.
Add `parse_histogram_json`, `parse_tree_json`, `to_plot_dataframe` to `parse.rs`.

---

## Files to Create

```
crates/genomehubs-api/src/routes/report.rs         — POST /api/v3/report
crates/genomehubs-api/src/report/report_types.rs   — per-type handler functions
crates/genomehubs-query/src/report/builder.rs      — ReportBuilder SDK type
```

## Files to Modify

| File                                        | Change                                                |
| ------------------------------------------- | ----------------------------------------------------- |
| `crates/genomehubs-api/src/routes/mod.rs`   | `pub mod report;`                                     |
| `crates/genomehubs-api/src/report/mod.rs`   | `pub mod report_types;`                               |
| `crates/genomehubs-api/src/main.rs`         | Register route + OpenAPI                              |
| `crates/genomehubs-query/src/report/mod.rs` | `pub mod builder; pub use builder::ReportBuilder;`    |
| `crates/genomehubs-query/src/parse.rs`      | Add 3 new parse functions                             |
| `src/lib.rs`                                | PyO3 exports for ReportBuilder + parse functions      |
| `crates/genomehubs-query/src/lib.rs`        | WASM exports                                          |
| `templates/r/lib.rs.tera`                   | extendr exports                                       |
| `templates/r/extendr-wrappers.R.tera`       | R wrappers                                            |
| `python/cli_generator/query.py`             | `histogram()`, `scatter()`, `tree()`, `map()` methods |
| `templates/python/query.py.tera`            | Mirror same methods                                   |
| `templates/js/query.js`                     | Same methods                                          |
| `templates/r/query.R`                       | Same methods                                          |

---

## API Request / Response

### Request body

```json
{
  "query_yaml": "index: taxon\ntaxa: [Mammalia]\ntaxon_filter_type: tree\n",
  "params_yaml": "taxonomy: ncbi\n",
  "report_yaml": "report: histogram\nx: genome_size\nx_opts: \";;20;log10\"\ncat: assembly_level\ncat_opts: \";;5+\"\nscatter_threshold: 100\n"
}
```

`report_yaml` fields:

| Key                 | Type   | Description                                                  |
| ------------------- | ------ | ------------------------------------------------------------ |
| `report`            | string | `histogram`, `scatter`, `xPerRank`, `sources`, `tree`, `map` |
| `x`                 | string | X-axis field name                                            |
| `x_opts`            | string | AxisOpts string for X                                        |
| `y`                 | string | Optional Y-axis field (2D histogram / scatter Y)             |
| `y_opts`            | string | AxisOpts string for Y                                        |
| `cat`               | string | Category breakdown field                                     |
| `cat_opts`          | string | AxisOpts string for cat                                      |
| `scatter_threshold` | usize  | Switch to raw mode below this count (default 100)            |

### Response envelope

```json
{
  "status": { "success": true, "hits": 12345, "took": 42 },
  "report": {
    "type": "histogram",
    "x": { "field": "genome_size", "scale": "log10", "domain": [1e6, 1e12] },
    "buckets": [
      {
        "key": 1000000,
        "count": 42,
        "cat_counts": { "Chromosome": 20, "Scaffold": 22 }
      }
    ]
  }
}
```

---

## Implementation

### `crates/genomehubs-api/src/routes/report.rs`

```rust
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use genomehubs_query::{query, report::axis::AxisOpts};
use serde_json::Value;

use crate::{index_name, report::report_types, routes::ApiStatus, AppState};

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ReportRequest {
    pub query_yaml: String,
    pub params_yaml: String,
    pub report_yaml: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ReportResponse {
    pub status: ApiStatus,
    pub report: Value,
}

#[utoipa::path(
    post,
    path = "/api/v3/report",
    request_body = ReportRequest,
    responses((status = 200, description = "Report data", body = ReportResponse))
)]
#[axum::debug_handler]
pub async fn post_report(
    Extension(state): Extension<Arc<AppState>>,
    Json(req): Json<ReportRequest>,
) -> Json<ReportResponse> {
    let search_query = match query::SearchQuery::from_yaml(&req.query_yaml) {
        Ok(q) => q,
        Err(e) => return Json(ReportResponse {
            status: ApiStatus::error(format!("invalid query_yaml: {e}")),
            report: Value::Null,
        }),
    };
    let params = match query::QueryParams::from_yaml(&req.params_yaml) {
        Ok(p) => p,
        Err(e) => return Json(ReportResponse {
            status: ApiStatus::error(format!("invalid params_yaml: {e}")),
            report: Value::Null,
        }),
    };
    let report_config: serde_yaml::Value = match serde_yaml::from_str(&req.report_yaml) {
        Ok(v) => v,
        Err(e) => return Json(ReportResponse {
            status: ApiStatus::error(format!("invalid report_yaml: {e}")),
            report: Value::Null,
        }),
    };

    let idx = index_name::resolve_index(&search_query.index, &state);
    let report_type = report_config
        .get("report")
        .and_then(|v| v.as_str())
        .unwrap_or("histogram");

    let base_query = crate::core::query_builder::build_filter_query(&search_query, &params, &state.default_taxonomy);

    let result = match report_type {
        "histogram" | "scatter" => {
            report_types::run_histogram_report(&state, &idx, &search_query, &params, &report_config, &base_query).await
        }
        "xPerRank" => report_types::run_x_per_rank_report(&state, &idx, &base_query, &report_config).await,
        "sources" => report_types::run_sources_report(&state, &idx, &base_query).await,
        "tree" => report_types::run_tree_report(&state, &idx, &base_query, &report_config).await,
        "map" => report_types::run_map_report(&state, &idx, &base_query, &report_config).await,
        unknown => Err(format!("unknown report type: {unknown}")),
    };

    match result {
        Ok((hits, took, report_data)) => Json(ReportResponse {
            status: ApiStatus::query_ok(hits, took),
            report: report_data,
        }),
        Err(e) => Json(ReportResponse {
            status: ApiStatus::error(e),
            report: Value::Null,
        }),
    }
}
```

---

### `crates/genomehubs-api/src/report/report_types.rs`

All per-type handler functions return `Result<(u64 hits, u64 took, Value report_data), String>`.

#### Histogram report

```rust
pub async fn run_histogram_report(
    state: &Arc<AppState>,
    index: &str,
    search_query: &SearchQuery,
    params: &QueryParams,
    report_config: &serde_yaml::Value,
    base_query: &Value,
) -> Result<(u64, u64, Value), String> {
    let x_field = report_config.get("x").and_then(|v| v.as_str())
        .ok_or("report_yaml missing 'x' field")?;
    let x_opts_str = report_config.get("x_opts").and_then(|v| v.as_str()).unwrap_or("");
    let cat_field = report_config.get("cat").and_then(|v| v.as_str());
    let scatter_threshold = report_config
        .get("scatter_threshold").and_then(|v| v.as_u64()).unwrap_or(100) as u64;

    // Infer value type from field metadata (MetadataCache.attr_types)
    let x_value_type = infer_value_type(x_field, &state.cache);
    let x_spec = AxisSpec {
        field: x_field.to_string(),
        role: AxisRole::X,
        summary: AxisSummary::default(),
        value_type: x_value_type,
        opts: AxisOpts::from_str(x_opts_str),
    };

    // Bounds probe
    let x_bounds = compute_bounds(&state.client, &state.es_base, index, &x_spec, base_query).await?;

    // Check if scatter mode (count < threshold)
    let doc_count = count_docs(&state.client, &state.es_base, index, base_query).await?;
    if doc_count < scatter_threshold && report_config.get("report").and_then(|v| v.as_str()) == Some("scatter") {
        return run_scatter_raw(state, index, search_query, params, &x_spec, doc_count).await;
    }

    // Build aggregation
    let x_agg = agg_builder_for(&x_spec, &x_bounds);
    let agg_name = "x_agg";

    // Optionally nest cat terms inside x buckets
    let agg_body = if let Some(cat) = cat_field {
        let cat_spec = build_cat_spec(cat, report_config, state);
        let cat_bounds = compute_bounds(&state.client, &state.es_base, index, &cat_spec, base_query).await?;
        let cat_agg = agg_builder_for(&cat_spec, &cat_bounds);
        CompositeAggBuilder {
            outer: x_agg,
            inner: cat_agg,
            inner_name: "cat_agg".to_string(),
        }.build(agg_name)
    } else {
        x_agg.build(agg_name)
    };

    let es_body = json!({
        "size": 0,
        "query": base_query,
        "aggs": agg_body
    });

    let resp = es_client::execute_search(&state.client, &state.es_base, index, &es_body).await?;
    let took = resp.get("took").and_then(|t| t.as_u64()).unwrap_or(0);
    let total_hits = resp.pointer("/hits/total/value").and_then(|v| v.as_u64()).unwrap_or(0);

    // Extract and transform buckets
    let raw_buckets = x_agg.extract(&resp, agg_name);
    let pipeline = Pipeline::new().add(ScaleStep);
    let ctx = ReportContext { scale: x_spec.opts.scale, cat_labels: x_bounds.cat_labels.clone(), show_other: x_spec.opts.show_other };
    let buckets = pipeline.run(raw_buckets, &ctx);

    let report_data = json!({
        "type": "histogram",
        "x": { "field": x_field, "scale": format!("{:?}", x_spec.opts.scale).to_lowercase(), "domain": x_bounds.domain },
        "buckets": buckets
    });

    Ok((total_hits, took, report_data))
}
```

#### Scatter (raw mode)

```rust
async fn run_scatter_raw(
    state: &Arc<AppState>,
    index: &str,
    search_query: &SearchQuery,
    params: &QueryParams,
    x_spec: &AxisSpec,
    count: u64,
) -> Result<(u64, u64, Value), String> {
    // Use build_search_body with a large size to get raw documents
    let mut scatter_params = params.clone();
    scatter_params.size = count.min(10_000) as usize;

    let body = cli_generator::core::query_builder::build_search_body(
        search_query, &scatter_params, "taxon", None, &state.default_taxonomy,
    );

    let resp = es_client::execute_search(&state.client, &state.es_base, index, &body).await?;
    let took = resp.get("took").and_then(|t| t.as_u64()).unwrap_or(0);

    let hits: Vec<Value> = resp
        .pointer("/hits/hits")
        .and_then(|h| h.as_array())
        .map(|arr| arr.iter().filter_map(|h| h.get("_source").cloned()).collect())
        .unwrap_or_default();

    let report_data = json!({
        "type": "scatter",
        "mode": "raw",
        "x": { "field": &x_spec.field },
        "hits": hits
    });

    Ok((count, took, report_data))
}
```

#### xPerRank report

```rust
pub async fn run_x_per_rank_report(
    state: &Arc<AppState>,
    index: &str,
    base_query: &Value,
    report_config: &serde_yaml::Value,
) -> Result<(u64, u64, Value), String> {
    let x_field = report_config.get("x").and_then(|v| v.as_str())
        .ok_or("report_yaml missing 'x' field")?;

    let es_body = json!({
        "size": 0,
        "query": base_query,
        "aggs": {
            "by_rank": {
                "terms": { "field": "taxon_rank", "size": 20 },
                "aggs": {
                    "field_stats": { "stats": { "field": x_field } },
                    "doc_count": { "value_count": { "field": x_field } }
                }
            }
        }
    });

    let resp = es_client::execute_search(&state.client, &state.es_base, index, &es_body).await?;
    let took = resp.get("took").and_then(|t| t.as_u64()).unwrap_or(0);
    let total = resp.pointer("/hits/total/value").and_then(|v| v.as_u64()).unwrap_or(0);
    let buckets = resp.pointer("/aggregations/by_rank/buckets").cloned().unwrap_or_default();

    Ok((total, took, json!({ "type": "xPerRank", "x": x_field, "buckets": buckets })))
}
```

#### Sources report

```rust
pub async fn run_sources_report(
    state: &Arc<AppState>,
    index: &str,
    base_query: &Value,
) -> Result<(u64, u64, Value), String> {
    let es_body = json!({
        "size": 0,
        "query": base_query,
        "aggs": {
            "sources": {
                "terms": { "field": "sources.keyword", "size": 50 }
            }
        }
    });

    let resp = es_client::execute_search(&state.client, &state.es_base, index, &es_body).await?;
    let took = resp.get("took").and_then(|t| t.as_u64()).unwrap_or(0);
    let total = resp.pointer("/hits/total/value").and_then(|v| v.as_u64()).unwrap_or(0);
    let buckets = resp.pointer("/aggregations/sources/buckets").cloned().unwrap_or_default();

    Ok((total, took, json!({ "type": "sources", "buckets": buckets })))
}
```

#### Tree report

Uses `reverse_nested` aggregation on the lineage path to build a hierarchy, then
serialises to Newick format.

```rust
pub async fn run_tree_report(
    state: &Arc<AppState>,
    index: &str,
    base_query: &Value,
    report_config: &serde_yaml::Value,
) -> Result<(u64, u64, Value), String> {
    let rank_field = report_config.get("rank").and_then(|v| v.as_str()).unwrap_or("phylum");
    let depth = report_config.get("depth").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

    let es_body = json!({
        "size": 0,
        "query": base_query,
        "aggs": {
            "lineage": {
                "nested": { "path": "lineage" },
                "aggs": {
                    "by_rank": {
                        "filter": { "term": { "lineage.taxon_rank": rank_field } },
                        "aggs": {
                            "names": {
                                "terms": { "field": "lineage.scientific_name.keyword", "size": depth * 10 },
                                "aggs": {
                                    "count": { "reverse_nested": {} }
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    let resp = es_client::execute_search(&state.client, &state.es_base, index, &es_body).await?;
    let took = resp.get("took").and_then(|t| t.as_u64()).unwrap_or(0);
    let total = resp.pointer("/hits/total/value").and_then(|v| v.as_u64()).unwrap_or(0);

    let buckets = resp
        .pointer("/aggregations/lineage/by_rank/names/buckets")
        .cloned()
        .unwrap_or_default();

    // Serialise to Newick string
    let newick = buckets_to_newick(&buckets);

    Ok((total, took, json!({ "type": "tree", "newick": newick, "buckets": buckets })))
}

fn buckets_to_newick(buckets: &Value) -> String {
    let arr = match buckets.as_array() {
        Some(a) => a,
        None => return "();".to_string(),
    };

    let nodes: Vec<String> = arr
        .iter()
        .map(|b| {
            let name = b.get("key").and_then(|k| k.as_str()).unwrap_or("?");
            let count = b.pointer("/count/doc_count").and_then(|v| v.as_u64()).unwrap_or(0);
            format!("{name}:{count}")
        })
        .collect();

    format!("({});", nodes.join(","))
}
```

#### Map report

```rust
pub async fn run_map_report(
    state: &Arc<AppState>,
    index: &str,
    base_query: &Value,
    report_config: &serde_yaml::Value,
) -> Result<(u64, u64, Value), String> {
    let geo_field = report_config.get("x").and_then(|v| v.as_str()).unwrap_or("location");
    let size = report_config.get("size").and_then(|v| v.as_u64()).unwrap_or(500) as usize;
    let precision = report_config.get("precision").and_then(|v| v.as_u64()).unwrap_or(4) as u8;

    let es_body = json!({
        "size": 0,
        "query": base_query,
        "aggs": {
            "geo_grid": {
                "geohash_grid": {
                    "field": geo_field,
                    "precision": precision,
                    "size": size
                }
            }
        }
    });

    let resp = es_client::execute_search(&state.client, &state.es_base, index, &es_body).await?;
    let took = resp.get("took").and_then(|t| t.as_u64()).unwrap_or(0);
    let total = resp.pointer("/hits/total/value").and_then(|v| v.as_u64()).unwrap_or(0);
    let buckets = resp.pointer("/aggregations/geo_grid/buckets").cloned().unwrap_or_default();

    Ok((total, took, json!({ "type": "map", "field": geo_field, "buckets": buckets })))
}
```

---

## `ReportBuilder` in `crates/genomehubs-query/src/report/builder.rs`

```rust
use serde::Serialize;

use super::axis::{AxisOpts, AxisRole, AxisSpec, AxisSummary, ValueType};

/// Build a report request from incremental SDK calls.
///
/// Serialises to `report_yaml` for use with the `/report` endpoint.
#[derive(Debug, Default, Serialize)]
pub struct ReportBuilder {
    report_type: String,
    x: Option<String>,
    x_opts: Option<String>,
    y: Option<String>,
    y_opts: Option<String>,
    cat: Option<String>,
    cat_opts: Option<String>,
    scatter_threshold: Option<usize>,
    rank: Option<String>,
    depth: Option<usize>,
}

impl ReportBuilder {
    pub fn new(report_type: impl Into<String>) -> Self {
        Self { report_type: report_type.into(), ..Default::default() }
    }

    pub fn set_x(mut self, field: impl Into<String>) -> Self { self.x = Some(field.into()); self }
    pub fn set_x_opts(mut self, opts: impl Into<String>) -> Self { self.x_opts = Some(opts.into()); self }
    pub fn set_y(mut self, field: impl Into<String>) -> Self { self.y = Some(field.into()); self }
    pub fn set_y_opts(mut self, opts: impl Into<String>) -> Self { self.y_opts = Some(opts.into()); self }
    pub fn set_cat(mut self, field: impl Into<String>) -> Self { self.cat = Some(field.into()); self }
    pub fn set_cat_opts(mut self, opts: impl Into<String>) -> Self { self.cat_opts = Some(opts.into()); self }
    pub fn set_scatter_threshold(mut self, n: usize) -> Self { self.scatter_threshold = Some(n); self }
    pub fn set_rank(mut self, rank: impl Into<String>) -> Self { self.rank = Some(rank.into()); self }
    pub fn set_depth(mut self, depth: usize) -> Self { self.depth = Some(depth); self }

    /// Serialise to YAML for use in the `report_yaml` request field.
    pub fn to_report_yaml(&self) -> String {
        let mut doc = serde_yaml::Mapping::new();
        doc.insert("report".into(), self.report_type.clone().into());
        if let Some(x) = &self.x { doc.insert("x".into(), x.clone().into()); }
        if let Some(xo) = &self.x_opts { doc.insert("x_opts".into(), xo.clone().into()); }
        if let Some(y) = &self.y { doc.insert("y".into(), y.clone().into()); }
        if let Some(yo) = &self.y_opts { doc.insert("y_opts".into(), yo.clone().into()); }
        if let Some(c) = &self.cat { doc.insert("cat".into(), c.clone().into()); }
        if let Some(co) = &self.cat_opts { doc.insert("cat_opts".into(), co.clone().into()); }
        if let Some(st) = self.scatter_threshold { doc.insert("scatter_threshold".into(), st.into()); }
        if let Some(r) = &self.rank { doc.insert("rank".into(), r.clone().into()); }
        if let Some(d) = self.depth { doc.insert("depth".into(), d.into()); }
        serde_yaml::to_string(&serde_yaml::Value::Mapping(doc)).unwrap_or_default()
    }
}
```

---

## Parse functions in `crates/genomehubs-query/src/parse.rs`

```rust
/// Parse histogram report buckets from a raw `/report` response.
///
/// Returns a JSON array of bucket objects with normalised `key`, `count`, and
/// optional `cat_counts` (for categorised histograms).
pub fn parse_histogram_json(raw: &str) -> Result<String, String> {
    let envelope: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| format!("invalid JSON: {e}"))?;
    let buckets = envelope
        .pointer("/report/buckets")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    serde_json::to_string(&buckets).map_err(|e| format!("serialisation error: {e}"))
}

/// Parse tree report Newick string from a raw `/report` response.
///
/// Returns just the Newick string, or an error if the report field is absent.
pub fn parse_tree_json(raw: &str) -> Result<String, String> {
    let envelope: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| format!("invalid JSON: {e}"))?;
    envelope
        .pointer("/report/newick")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "missing /report/newick in response".to_string())
}

/// Convert a parsed histogram bucket list to a long-format (tidy) data frame.
///
/// Input is a JSON array of histogram bucket objects. Output is a JSON array of
/// flat dicts suitable for `pandas.DataFrame` or R `data.frame`:
/// `[{ "key": 1e6, "count": 42, "cat": "Chromosome" }, ...]`
///
/// For plain histograms (no `cat_counts`), the `"cat"` column is omitted.
pub fn to_plot_dataframe(buckets_json: &str) -> Result<String, String> {
    let buckets: Vec<serde_json::Value> = serde_json::from_str(buckets_json)
        .map_err(|e| format!("invalid JSON: {e}"))?;

    let mut rows: Vec<serde_json::Value> = vec![];

    for bucket in &buckets {
        let key = bucket.get("key").cloned().unwrap_or(serde_json::Value::Null);
        let key_scaled = bucket.get("key_scaled").cloned();

        if let Some(cat_counts) = bucket.get("cat_counts").and_then(|v| v.as_object()) {
            for (cat, count) in cat_counts {
                let mut row = serde_json::json!({ "key": key, "count": count, "cat": cat });
                if let Some(ks) = &key_scaled { row["key_scaled"] = ks.clone(); }
                rows.push(row);
            }
        } else {
            let count = bucket.get("doc_count").cloned().unwrap_or(serde_json::Value::from(0));
            let mut row = serde_json::json!({ "key": key, "count": count });
            if let Some(ks) = &key_scaled { row["key_scaled"] = ks.clone(); }
            rows.push(row);
        }
    }

    serde_json::to_string(&rows).map_err(|e| format!("serialisation error: {e}"))
}
```

---

## SDK Methods

Add to `python/cli_generator/query.py` and `templates/python/query.py.tera`:

```python
def histogram(
    self,
    x: str,
    x_opts: str = "",
    cat: str | None = None,
    cat_opts: str = "",
    *,
    api_base: str = "https://goat.genomehubs.org/api",
    api_version: str = "v3",
) -> list[dict[str, Any]]:
    """Run a histogram report and return the bucket list."""
    from . import parse_histogram_json as _parse
    report_yaml = f"report: histogram\nx: {x}\n"
    if x_opts: report_yaml += f"x_opts: \"{x_opts}\"\n"
    if cat: report_yaml += f"cat: {cat}\n"
    if cat_opts: report_yaml += f"cat_opts: \"{cat_opts}\"\n"
    return self._run_report(report_yaml, api_base, api_version, _parse)

def scatter(self, x: str, y: str | None = None, *, threshold: int = 100,
            api_base: str = "https://goat.genomehubs.org/api",
            api_version: str = "v3") -> list[dict[str, Any]]:
    """Run a scatter report."""
    from . import parse_histogram_json as _parse
    report_yaml = f"report: scatter\nx: {x}\nscatter_threshold: {threshold}\n"
    if y: report_yaml += f"y: {y}\n"
    return self._run_report(report_yaml, api_base, api_version, _parse)

def tree(self, rank: str = "phylum", depth: int = 5,
         *, api_base: str = "https://goat.genomehubs.org/api",
         api_version: str = "v3") -> str:
    """Run a tree report and return Newick string."""
    from . import parse_tree_json as _parse
    report_yaml = f"report: tree\nrank: {rank}\ndepth: {depth}\n"
    return self._run_report(report_yaml, api_base, api_version, _parse)

def map(self, geo_field: str = "location", precision: int = 4,
        *, api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3") -> list[dict[str, Any]]:
    """Run a map report and return geohash bucket list."""
    from . import parse_histogram_json as _parse
    report_yaml = f"report: map\nx: {geo_field}\nprecision: {precision}\n"
    return self._run_report(report_yaml, api_base, api_version, _parse)

def _run_report(
    self,
    report_yaml: str,
    api_base: str,
    api_version: str,
    parse_fn: Any,
) -> Any:
    """POST to /report and apply parse_fn to the raw response."""
    import json, urllib.request
    url = f"{api_base}/{api_version}/report"
    payload = json.dumps({
        "query_yaml": self.to_query_yaml(),
        "params_yaml": self.to_params_yaml(),
        "report_yaml": report_yaml,
    }).encode()
    req = urllib.request.Request(url, data=payload,
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req) as resp:
        raw = resp.read().decode()
    return json.loads(parse_fn(raw))
```

---

## Verification

```bash
cargo build -p genomehubs-api
cargo test -p genomehubs-query report
maturin develop --features extension-module
pytest tests/python/ -v -k report

# Smoke test
curl -s -X POST http://localhost:3000/api/v3/report \
  -H 'Content-Type: application/json' \
  -d '{"query_yaml":"index: taxon\n","params_yaml":"taxonomy: ncbi\n",
       "report_yaml":"report: histogram\nx: genome_size\nx_opts: \";;20;log10\"\n"}' \
  | jq '{success: .status.success, type: .report.type, bucket_count: (.report.buckets | length)}'
```

---

## Completion Checklist

- [ ] `routes/report.rs` created; dispatches to `report_types`
- [ ] All 6 report type functions in `report_types.rs`
- [ ] `ReportBuilder` in `crates/genomehubs-query/src/report/builder.rs`
- [ ] `parse_histogram_json`, `parse_tree_json`, `to_plot_dataframe` in `parse.rs`
- [ ] PyO3 / WASM / extendr exports for all three parse functions
- [ ] R wrapper stubs in `extendr-wrappers.R.tera`
- [ ] SDK methods `histogram()`, `scatter()`, `tree()`, `map()` in all three languages
- [ ] Route registered in `main.rs` + OpenAPI components updated
- [ ] `cargo test -p genomehubs-api` passes
- [ ] `pytest tests/python/ -v` passes
- [ ] Smoke test for each report type passes
