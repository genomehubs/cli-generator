# Phase 6: Report Types

**Depends on:** Phase 5 (AggBuilder, bounds, pipeline)
**Blocks:** Phase 6b (SDK / CLI integration)
**Ordering note:** Phase 7 (shared filter-expression parser + arc + tree field axes) and Phase 15 (cross-query reports) both move ahead of Phase 6b, so SDK method signatures are finalised against a complete API surface before generated code is produced.
**Estimated scope:** ~4 new files, 3 new parse functions

---

## Goal

Implement `POST /api/v3/report` — the single report endpoint that handles all
non-arc report types:

| Report               | Key                           | ES technique                                                          | When                            |
| -------------------- | ----------------------------- | --------------------------------------------------------------------- | ------------------------------- |
| Histogram (1D)       | `histogram`                   | histogram/date_histogram/terms agg                                    | always                          |
| Histogram + cat      | `histogram` + `cat`           | 2-level terms + histogram                                             | when `cat:` set                 |
| 2D histogram/heatmap | `histogram` + `y`             | composite terms/histogram                                             | when `y:` set                   |
| Scatter (raw)        | `scatter`                     | top-N `_search` hits                                                  | count < scatter_threshold       |
| Scatter (grid)       | `scatter`                     | 2D histogram                                                          | count ≥ scatter_threshold       |
| xPerRank             | `xPerRank`                    | terms agg on `taxon_rank`, nested stats                               | always                          |
| Sources              | `sources`                     | terms on `source`                                                     | always                          |
| Tree                 | `tree`                        | lineage nested agg (LCA) + search_after pagination + lineage walk     | always                          |
| Tree + cat           | `tree` + `cat_rank`           | as above + per-node `cat` label from ancestor at named rank           | when `cat_rank:` set            |
| Tree (collapsed)     | `tree` + `collapse_monotypic` | as above + post-processing pass removes single-child nodes            | when `collapse_monotypic: true` |
| Map                  | `map`                         | nested terms agg on `attributes.hexbin{N}` + optional raw point fetch | always                          |

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

| Key                  | Type          | Description                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| -------------------- | ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `report`             | string        | `histogram`, `scatter`, `xPerRank`, `sources`, `tree`, `map`                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `x`                  | string        | X-axis field name                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `x_opts`             | string        | AxisOpts string for X                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `y`                  | string        | Optional Y-axis field (2D histogram / scatter Y)                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `y_opts`             | string        | AxisOpts string for Y                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `cat`                | string        | Category breakdown field                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `cat_opts`           | string        | AxisOpts string for cat                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `scatter_threshold`  | usize         | Switch to raw mode below this count (default 100)                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `y` / `fields`       | string / list | Tree only: attribute field(s) to extract per node. `y: genome_size` is the single-field shorthand; `fields: [genome_size, c_value]` extracts multiple. **Phase 6 only shows the `median` summary value.** Controlling which summary (min / max / median) is shown per field via `y_opts`-style opts is deferred to Phase 7, where the tree field config aligns fully with the histogram `y`/`y_opts` axis format.                                                                                                     |
| `status_filter`      | string        | Tree only: filter expression ANDed with `base_query`. Nodes whose taxon_id appears in the result set get `status=1`. When absent and `fields` is set, `status=1` if the node has any field data. **Phase 6 handles simple `field op value` expressions only** (`genome_size>3000000000`, `assembly_level=Chromosome`). Compound expressions (`min(field)<val`, `expr AND expr`) and agg-function prefixes are deferred to Phase 7 where a shared `parse_filter_string()` helper is added (used by both tree and arc). |
| `cat_rank`           | string        | Tree only: propagation rank for cat labels. After per-node labelling from attribute data, any node with no `cat` value inherits the label of its nearest ancestor whose `taxon_rank` matches `cat_rank`. Nodes at `cat_rank` that themselves have no cat data do **not** get a fallback label — their descendants remain uncategorised.                                                                                                                                                                               |
| `collapse_monotypic` | bool          | Tree only: when `true`, remove nodes that have exactly one child and whose `taxon_rank` is not in `preserve_rank` (species is always preserved). Default `false`.                                                                                                                                                                                                                                                                                                                                                     |
| `preserve_rank`      | string        | Tree only: comma-separated list of ranks to keep even when they are monotypic (only meaningful when `collapse_monotypic: true`). Example: `"family,order"`.                                                                                                                                                                                                                                                                                                                                                           |

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

Histogram `cat` is **fully implemented** in `run_histogram_report`. It uses the same
`resolve_axis_spec` + `compute_bounds` pipeline as the x-axis: `cat_spec` is built via
`resolve_axis_spec(AxisRole::Cat, ...)`, bounds are probed via `compute_bounds`, and the
resulting `cat_bounds.cat_labels` drive the nested aggregation structure. Both keyword
and numeric cat axes are supported (keyword → named filter buckets; numeric → histogram
sub-agg). The response carries `by_cat`, `cat`, and `cats` keys alongside `buckets`.

The outdated pseudo-code below predates the actual implementation; see
`crates/genomehubs-api/src/report/report_types.rs::run_histogram_report` for the
current code.

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

Builds a hierarchical taxon tree from the matched result set. Three main steps:

1. **LCA detection** — nested lineage aggregation sorted by `_count desc, min_depth asc`; the deepest bucket shared by all matching taxa is the LCA.
2. **Paginated fetch** — `search_after` on `taxon_id` (500 hits/page, capped at 100 000). Each hit's `lineage[]` array is walked to create parent-child links up to the LCA.
3. **Field extraction + status** — see below.

**Multiple fields (v2 parity):**

`fields` is a YAML sequence of attribute names to extract per node:

```yaml
report: tree
fields:
  - genome_size
  - c_value
```

The single-key `y` is still accepted as shorthand for `fields: [value]`. Both are read at the start of `run_tree_report`:

```rust
// Collect fields to extract, with `y` as a backwards-compat alias.
let tree_fields: Vec<String> = {
    let from_fields = report_config
        .get("fields")
        .and_then(|v| v.as_sequence())
        .map(|seq| seq.iter().filter_map(|v| v.as_str().map(str::to_string)).collect::<Vec<_>>())
        .unwrap_or_default();
    if from_fields.is_empty() {
        report_config
            .get("y")
            .and_then(|v| v.as_str())
            .map(|s| vec![s.to_string()])
            .unwrap_or_default()
    } else {
        from_fields
    }
};
```

`extract_tree_field` is called once per entry in `tree_fields`; all results are merged into a single `fields` map on the node.

**Status filter (v2 parity):**

`status_filter` is a query-string fragment in the same syntax as arc report's `x`/`y`/`z` params (`genome_size>3000000000`, `assembly_level=Chromosome`, etc.). It is compiled to an ES clause and ANDed with `base_query` to run a second paginated search. Taxon IDs that appear in those results get `status=1`; all others get `status=0`.

```yaml
report: tree
fields:
  - genome_size
status_filter: "genome_size>3000000000"
```

When `status_filter` is absent:

- If `fields` is set: `status=1` for nodes that have any field data in `fields` (field was present in their `attributes[]`).
- If neither is set: all nodes get `status=0`.

**Phase 6 stub / Phase 7 full parser:** Phase 6 compiles `status_filter` with a minimal dispatcher (`field op value`, nested attributes query). Phase 7 (moved ahead of Phase 6b) replaces the stub with a shared `parse_filter_string()` helper that also handles `agg(field) op value` and compound `AND` expressions. The same helper is used by arc report `x`/`y`/`z`. This keeps query-string parsing in one place.

**Cat axis for tree (v2 parity + v3 extension):**

Tree `cat` uses the **same `resolve_axis_spec` + `compute_bounds` pipeline as histogram** `cat`. The bounds result (`cat_bounds`) is returned in the response as `catBounds` so the UI can render a colour legend.

```yaml
report: tree
fields:
  - genome_size
cat: assembly_level
cat_opts: ";;5+"
cat_rank: order
```

Each node's `cat` value is set from its own attribute data for the cat field (same extraction path as `extract_tree_field`). The cat label is the string label from `cat_bounds.cat_labels` — the same bucket label the histogram would use.

`cat_rank` is an optional propagation control: after all nodes have been labelled from their own data, any node that still has no `cat` value inherits the label from its nearest _ancestor_ whose `taxon_rank` matches `cat_rank`. This ensures internal nodes at the specified rank and above are always labelled, making the colour visible throughout the tree.

```rust
// After per-node cat extraction:
// For each node (post-order), if cat is None and parent has a cat,
// inherit parent's cat when parent.taxon_rank == cat_rank OR
// when an ancestor at cat_rank was encountered walking up the lineage.
// This mirrors the v2 logic: ancestor.taxon_rank == catRank sets
// cat on all descendants seen so far in that lineage walk.
if let Some(ref rank) = cat_rank {
    // Second pass: propagate cat down from ancestor at cat_rank.
    // Walk BFS from LCA; when a node at cat_rank is seen, stamp all
    // descendants that still lack a cat value with its label.
}
```

Note: In v2, `catRank` was both the source of the label (the ancestor's `taxon_id`) and the propagation level. In v3 those roles are separated: `cat`/`cat_opts` define the field and its axis (the label comes from attribute data); `cat_rank` only controls propagation depth.

`catBounds` is always included in the response when `cat` is set:

```json
"catBounds": {
  "domain": [0, 5],
  "labels": ["Chromosome", "Scaffold", "Contig", "other"],
  "scale": "linear"
}
```

**Collapse monotypic (v2 parity):**

Applied as a post-processing step after the tree is fully built, before the response is assembled. Algorithm mirrors v2 `collapseNodes`:

```rust
fn collapse_monotypic_nodes(
    tree_nodes: &mut BTreeMap<String, serde_json::Map<String, Value>>,
    lca_id: &str,
    preserve_ranks: &[&str],  // always includes "species"
) -> u64 /* new max_depth */ {
    // Iterative post-order DFS (same snapshot trick as compute_subtree_counts).
    // A node is collapsed when:
    //   - it has exactly 1 child
    //   - its taxon_rank is not in preserve_ranks
    // On collapse: remove the node from tree_nodes; add its single child
    // directly into the parent's children map.
    // Returns updated max depth.
}
```

Parameters:

- `collapse_monotypic: true` enables collapsing.
- `preserve_rank: "family,order"` — comma-separated ranks to keep even if monotypic. `species` is always kept regardless.

**Response shape** — `cat`, `catBounds` added when `cat` is set:

````json
{
  "lca": { "taxon_id": "9608", "scientific_name": "Canidae", "taxon_rank": "family",
            "count": 126, "maxDepth": 3, "minDepth": 2, "parent": "379584" },
  "catBounds": {
    "domain": [0, 4],
    "labels": ["Chromosome", "Scaffold", "Contig", "other"],
    "scale": "linear"
  },
  "treeNodes": {
    "9608": {
      "taxon_id": "9608", "scientific_name": "Canidae", "taxon_rank": "family",
      "count": 96, "children": { "9611": true },
      "cat": "Chromosome",
      "fields": {
        "genome_size": { "source": "descendant", "value": 2.5e9, "min": 1.8e9, "max": 3.2e9 }
      },
      "status": 0
    }
  }
}

#### Map report

Geo-point data in GoaT is stored in nested `attributes[]` entries, not as top-level
fields. Location attribute entries carry:
- `geo_point_value` — raw lat/lon string (or array of strings for multi-value taxa)
- `hexbin1`–`hexbin6` — pre-computed H3 cell IDs at each resolution

The map report produces two complementary data shapes, mirroring v2:

**`rawData`** — per-taxon point records grouped by cat label (or `"all taxa"` when no
`cat` is set). Only populated when the count of taxa with location data is ≤
`map_threshold`. Each entry: `{scientific_name, taxonId, coords, aggregation_source, cat}`.

**`hexBinCounts`** — `{h3_cell_id: count}` from a `terms` aggregation on
`attributes.hexbin{N}`. Always returned regardless of threshold. Resolution 1–6,
default 3.

**Config keys:**

| Key | Type | Description |
| --- | ---- | ----------- |
| `location_field` | string | Attribute key for geo-point data (default `"sample_location"`). Also accepted as `x` for backwards compat. |
| `hex_resolution` | integer | H3 resolution 1–6 (default 3) |
| `map_threshold` | integer | Max point count for raw-data mode (default 2000) |
| `cat` / `cat_opts` | string | Category field; same `resolve_axis_spec` + `compute_bounds` pipeline as histogram. `catBounds` returned in response when set. |

**Response shape:**

```json
{
  "type": "map",
  "locationField": "sample_location",
  "hexResolution": 3,
  "rawData": {
    "Chromosome": [
      { "scientific_name": "Canis lupus", "taxonId": "9612",
        "coords": "77.785278,-70.631389", "aggregation_source": "direct", "cat": "Chromosome" }
    ],
    "all taxa": [ ... ]
  },
  "hexBinCounts": {
    "830264fffffffff": 1,
    "831958fffffffff": 2
  },
  "catBounds": {
    "field": "assembly_level",
    "labels": ["Scaffold", "Chromosome"],
    "domain": null,
    "scale": "linear"
  }
}
```

ES approach: three queries issued sequentially.
1. Nested count query (`must: [base_query, nested{key=location_field}]`) to get location count.
2. Nested `terms` agg on `attributes.hexbin{N}` filtered by `attributes.key = location_field`.
3. Top-N `_search` (only when count ≤ threshold) to retrieve raw `_source` for point extraction.

```rust
// Hexbin aggregation structure (step 2):
{
  "location_attr": { "nested": {"path": "attributes"},
    "aggs": { "by_key": { "filter": {"term": {"attributes.key": location_field}},
      "aggs": { "hexbins": { "terms": { "field": "attributes.hexbin3", "size": 50000 } } } } } }
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
    y: Option<String>,         // shorthand for tree fields: [y]; kept for backwards compat
    y_opts: Option<String>,
    cat: Option<String>,
    cat_opts: Option<String>,
    scatter_threshold: Option<usize>,
    rank: Option<String>,
    depth: Option<usize>,
    fields: Option<Vec<String>>,        // tree: attribute fields to extract per node
    status_filter: Option<String>,      // tree: query string determining status=1
    cat_rank: Option<String>,           // tree: propagation rank for cat labels
    collapse_monotypic: bool,           // tree: remove monotypic internal nodes
    preserve_rank: Option<String>,      // tree: comma-sep ranks to keep when collapsing
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
    pub fn set_fields(mut self, fields: Vec<impl Into<String>>) -> Self {
        self.fields = Some(fields.into_iter().map(Into::into).collect());
        self
    }
    pub fn set_status_filter(mut self, filter: impl Into<String>) -> Self {
        self.status_filter = Some(filter.into());
        self
    }
    pub fn set_cat_rank(mut self, rank: impl Into<String>) -> Self {
        self.cat_rank = Some(rank.into());
        self
    }
    pub fn set_collapse_monotypic(mut self, preserve_rank: Option<impl Into<String>>) -> Self {
        self.collapse_monotypic = true;
        self.preserve_rank = preserve_rank.map(Into::into);
        self
    }

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
        if let Some(fields) = &self.fields {
            let seq: serde_yaml::Value = serde_yaml::Value::Sequence(
                fields.iter().map(|f| serde_yaml::Value::String(f.clone())).collect(),
            );
            doc.insert("fields".into(), seq);
        }
        if let Some(sf) = &self.status_filter { doc.insert("status_filter".into(), sf.clone().into()); }
        if let Some(cr) = &self.cat_rank { doc.insert("cat_rank".into(), cr.clone().into()); }
        if self.collapse_monotypic { doc.insert("collapse_monotypic".into(), true.into()); }
        if let Some(pr) = &self.preserve_rank { doc.insert("preserve_rank".into(), pr.clone().into()); }
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

/// Parse a tree report response into a flat node list.
///
/// Returns a JSON array of node objects, one per entry in `treeNodes`, with
/// `taxon_id`, `scientific_name`, `taxon_rank`, `count`, `status`,
/// `parent_id` (derived from the node's position in the tree), and a flattened
/// `fields` map. Useful for building data frames in Python/R SDK code.
pub fn parse_tree_json(raw: &str) -> Result<String, String> {
    let envelope: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| format!("invalid JSON: {e}"))?;
    let nodes = envelope
        .pointer("/report/treeNodes")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "missing /report/treeNodes in response".to_string())?;
    let rows: Vec<serde_json::Value> = nodes
        .iter()
        .map(|(id, node)| {
            let mut row = serde_json::json!({
                "taxon_id": id,
                "scientific_name": node.get("scientific_name"),
                "taxon_rank": node.get("taxon_rank"),
                "count": node.get("count"),
                "status": node.get("status"),
            });
            // Flatten fields map into top-level keys prefixed with the field name
            if let Some(fields) = node.get("fields").and_then(|f| f.as_object()) {
                for (field, data) in fields {
                    row[field] = data.get("value").cloned().unwrap_or(serde_json::Value::Null);
                }
            }
            row
        })
        .collect();
    serde_json::to_string(&rows).map_err(|e| format!("serialisation error: {e}"))
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

def tree(
    self,
    fields: list[str] | None = None,
    status_filter: str | None = None,
    cat: str | None = None,
    cat_opts: str = "",
    cat_rank: str | None = None,
    collapse_monotypic: bool = False,
    preserve_rank: str | None = None,
    rank: str = "phylum",
    *,
    api_base: str = "https://goat.genomehubs.org/api",
    api_version: str = "v3",
) -> list[dict[str, Any]]:
    """Run a tree report and return a flat node list.

    Each element has taxon_id, scientific_name, taxon_rank, count, status,
    and one key per entry in `fields` containing that field's value.
    If cat is set, each element also has a `cat` key with the label for that
    field (same bucket labels as histogram cat). The response also includes
    a `catBounds` object for building a colour legend.
    cat_rank controls how far up the tree cat labels are propagated.
    """
    from . import parse_tree_json as _parse
    report_yaml = "report: tree\n"
    if rank: report_yaml += f"rank: {rank}\n"
    if fields:
        report_yaml += "fields:\n" + "".join(f"  - {f}\n" for f in fields)
    if status_filter: report_yaml += f"status_filter: \"{status_filter}\"\n"
    if cat: report_yaml += f"cat: {cat}\n"
    if cat_opts: report_yaml += f"cat_opts: \"{cat_opts}\"\n"
    if cat_rank: report_yaml += f"cat_rank: {cat_rank}\n"
    if collapse_monotypic:
        report_yaml += "collapse_monotypic: true\n"
        if preserve_rank: report_yaml += f"preserve_rank: \"{preserve_rank}\"\n"
    return self._run_report(report_yaml, api_base, api_version, _parse)

def map(self, location_field: str = "sample_location", hex_resolution: int = 3,
        map_threshold: int = 2000, cat: str | None = None, cat_opts: str = "",
        *, api_base: str = "https://goat.genomehubs.org/api",
        api_version: str = "v3") -> dict[str, Any]:
    """Run a map report. Returns rawData (per-point records) and hexBinCounts (H3 cells)."""
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

## Deferred to post-Phase 15

The following are related to tree decoration but require design work beyond Phase 6
and are captured here to avoid losing the intent.

| Item                                                        | Notes                                                                                                                                                                                                                                                                                                                                                                   |
| ----------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Node-based reports** (histogram per tree node)            | E.g. histogram of species genome sizes per family. Requires a new report shape where each tree node carries a mini-report payload rather than a scalar field value. Rough idea: `node_report: histogram` + `node_report_field: genome_size` in `report_yaml`; response adds `nodeReports: { id: {buckets:[...]} }`. Overlaps with Phase 15 custom histogram boundaries. |
| **Tree rank collapsing** (`rank` param, v2 `collapseNodes`) | v2 also supported collapsing nodes below a specific rank (not the same as monotypic collapse). Not yet needed; revisit when UI tree view is ported.                                                                                                                                                                                                                     |
| **`status_filter` compound expressions**                    | `min(field)<val`, `expr AND expr` — deferred to Phase 7 shared `parse_filter_string()` helper.                                                                                                                                                                                                                                                                          |
| **`fields` summary-value control**                          | Selecting min/max/median per field via `y_opts`-style opts — deferred to Phase 7 where tree field config aligns fully with histogram `y`/`y_opts` axis format.                                                                                                                                                                                                          |
| **`status_filter` combined query optimisation**             | Current plan runs a second paginated search for `status_filter`. A more efficient alternative: combine into a single aggregation (e.g. `filter` sub-agg per node). Defer until query volume warrants it.                                                                                                                                                                |
| **Newick serialisation**                                    | v2 could return Newick for export. Omitted in v3 until a use case arises.                                                                                                                                                                                                                                                                                               |
| **`fields` on non-tree reports**                            | Concept of per-result field extraction is currently tree-only. Could generalise to scatter raw mode (attach extra fields per point).                                                                                                                                                                                                                                    |

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
````
