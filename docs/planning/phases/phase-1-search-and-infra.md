# Phase 1: Shared ES Infrastructure + `/search`

**Depends on:** Phase 0 (ApiStatus must exist)
**Blocks:** Phase 2 (reuses es_client and index_name helpers)
**Estimated scope:** ~4 new files, 2 modified files

---

## Goal

1. Extract reusable helpers (`es_client.rs`, `index_name.rs`) from the inline code in `count.rs`
2. Implement `POST /api/v3/search` returning paginated results
3. Extend `GET /api/v3/status` to include `supported: [...]` so the SDK can probe at build time

---

## Background

### Why extract helpers now?

`count.rs` contains an inline `index_name_for()` function and direct `reqwest` calls.
`search.rs` (and every subsequent route) needs the same logic. Extracting them first prevents
copy-paste drift and keeps each route ~50 lines.

### `/search` ES body

The `build_search_body()` function already exists in `src/core/query_builder.rs` (in the
`cli-generator` crate, already a dependency of `genomehubs-api`). The route just needs to call
it, POST the result to ES, and pass the response through `parse_search_json()`.

### API version detection

`GET /api/v3/status` will return `supported: ["/count", "/search", ...]`. The SDK will GET this
once at build time:

- `404` → no v3 API, use all v2 URL-param paths
- Success + partial list → use v3 for listed endpoints, v2 for the rest
- If mixed-mode logic is too complex → use a build-time config variable instead

---

## Files to Create

| File                                            | Purpose                                                          |
| ----------------------------------------------- | ---------------------------------------------------------------- |
| `crates/genomehubs-api/src/es_client.rs`        | `execute_es_request()` — POST to ES, return parsed `Value`       |
| `crates/genomehubs-api/src/index_name.rs`       | `resolve_index()` — map `SearchIndex` + state to ES index string |
| `crates/genomehubs-api/src/routes/search.rs`    | `POST /api/v3/search` handler                                    |
| `crates/genomehubs-api/tests/search_builder.rs` | Integration tests for search query building                      |

## Files to Modify

| File                                         | Change                                                          |
| -------------------------------------------- | --------------------------------------------------------------- |
| `crates/genomehubs-api/src/routes/count.rs`  | Replace inline helpers with calls to `es_client` / `index_name` |
| `crates/genomehubs-api/src/routes/mod.rs`    | Add `pub mod search;`                                           |
| `crates/genomehubs-api/src/routes/status.rs` | Populate `supported` from known route list                      |
| `crates/genomehubs-api/src/main.rs`          | Register `/search` route; add `SearchResponse` to OpenAPI       |

---

## Implementation

### 1. `crates/genomehubs-api/src/index_name.rs`

```rust
use crate::AppState;
use genomehubs_query::query::SearchIndex;

/// Resolve an ES index name from a `SearchIndex` variant and the server's
/// configured index suffix.
///
/// Example: `SearchIndex::Taxon` + suffix `"--ncbi--goat--2021.10.15"`
/// → `"taxon--ncbi--goat--2021.10.15"`
pub fn resolve_index(index: &SearchIndex, state: &AppState) -> String {
    let base = match index {
        SearchIndex::Taxon => "taxon",
        SearchIndex::Assembly => "assembly",
        SearchIndex::Sample => "sample",
    };
    match &state.index_suffix {
        Some(suf) => format!("{base}{suf}"),
        None => base.to_string(),
    }
}

/// Resolve an explicit index type name (string) instead of a `SearchIndex`.
/// Used by endpoints that accept `result` as a query param.
pub fn resolve_index_str(result: &str, state: &AppState) -> String {
    let base = match result {
        "assembly" => "assembly",
        "sample" => "sample",
        _ => "taxon",
    };
    match &state.index_suffix {
        Some(suf) => format!("{base}{suf}"),
        None => base.to_string(),
    }
}
```

Register in `main.rs`:

```rust
mod es_client;
mod index_name;
```

### 2. `crates/genomehubs-api/src/es_client.rs`

```rust
use reqwest::Client;
use serde_json::Value;

/// Send a JSON body to `{es_base}/{index}/_search` and return the parsed response.
///
/// Returns `Err(String)` for HTTP errors or unparseable JSON.
pub async fn execute_search(
    client: &Client,
    es_base: &str,
    index: &str,
    body: &Value,
) -> Result<Value, String> {
    let url = format!("{es_base}/{index}/_search");
    let resp = client
        .post(&url)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("ES request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("ES returned {status}: {text}"));
    }

    resp.json::<Value>()
        .await
        .map_err(|e| format!("failed to parse ES response: {e}"))
}

/// Send a JSON body to `{es_base}/{index}/_count`.
pub async fn execute_count(
    client: &Client,
    es_base: &str,
    index: &str,
    body: &Value,
) -> Result<Value, String> {
    let url = format!("{es_base}/{index}/_count");
    let resp = client
        .post(&url)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("ES count request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("ES count returned {status}: {text}"));
    }

    resp.json::<Value>()
        .await
        .map_err(|e| format!("failed to parse ES count response: {e}"))
}
```

**Note:** `reqwest::Client` should be stored in `AppState` rather than created per-request.
Add to `AppState`:

```rust
pub client: reqwest::Client,
```

And populate in `main.rs`:

```rust
AppState {
    ...
    client: reqwest::Client::new(),
}
```

Update `count.rs` to use `state.client` instead of creating a local client.

### 3. Refactor `routes/count.rs`

Replace the inline `index_name_for()` and the direct `reqwest::Client::new()` calls with:

```rust
use crate::{es_client, index_name};

// replace inline function:
let idx = index_name::resolve_index(&query.index, &state);

// replace local client creation:
let raw = es_client::execute_count(&state.client, &state.es_base, &idx, &body)
    .await
    .map_err(|e| Json(CountResponse {
        status: super::ApiStatus::error(e),
        url: built_url.clone(),
    }))?;
```

### 4. `routes/search.rs`

```rust
use axum::{extract::Json, Extension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::{es_client, index_name, routes::ApiStatus, AppState};

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SearchRequest {
    /// YAML string describing the SearchQuery.
    pub query_yaml: String,
    /// YAML string describing the QueryParams.
    pub params_yaml: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SearchResponse {
    pub status: ApiStatus,
    /// URL that was built for this query (for debugging/reproduction).
    pub url: String,
    /// Flat result records.
    pub results: Vec<Value>,
    /// Cursor for the next page, if more results exist.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_after: Option<Value>,
}

#[utoipa::path(
    post,
    path = "/api/v3/search",
    request_body = SearchRequest,
    responses(
        (status = 200, description = "Search results", body = SearchResponse)
    )
)]
#[axum::debug_handler]
pub async fn post_search(
    Extension(state): Extension<Arc<AppState>>,
    Json(req): Json<SearchRequest>,
) -> Json<SearchResponse> {
    macro_rules! bail {
        ($msg:expr) => {
            return Json(SearchResponse {
                status: ApiStatus::error($msg),
                url: String::new(),
                results: vec![],
                search_after: None,
            })
        };
    }

    let query = match genomehubs_query::query::SearchQuery::from_yaml(&req.query_yaml) {
        Ok(q) => q,
        Err(e) => bail!(format!("failed to parse query_yaml: {e}")),
    };

    let params = match genomehubs_query::query::QueryParams::from_yaml(&req.params_yaml) {
        Ok(p) => p,
        Err(e) => bail!(format!("failed to parse params_yaml: {e}")),
    };

    let idx = index_name::resolve_index(&query.index, &state);

    // Build ES request body using the shared query_builder from cli-generator.
    let group = match query.index {
        genomehubs_query::query::SearchIndex::Taxon => "taxon",
        genomehubs_query::query::SearchIndex::Assembly => "assembly",
        genomehubs_query::query::SearchIndex::Sample => "sample",
    };
    let fields_slice: Option<Vec<&str>> = if query.attributes.fields.is_empty() {
        None
    } else {
        Some(query.attributes.fields.iter().map(|f| f.name.as_str()).collect())
    };

    // `build_search_body` is in cli_generator::core::query_builder
    let body = cli_generator::core::query_builder::build_search_body(
        &query,
        &params,
        group,
        fields_slice.as_deref(),
        &state.default_taxonomy,
    );

    // Build a URL for the response (no network call — for debugging)
    let built_url = genomehubs_query::query::build_query_url(
        &query,
        &params,
        &state.es_base,
        "v3",
        "search",
    );

    let raw = match es_client::execute_search(&state.client, &state.es_base, &idx, &body).await {
        Ok(v) => v,
        Err(e) => bail!(e),
    };

    // Extract status block from ES response
    let status_block = genomehubs_query::parse::parse_response_status(
        &raw.to_string()
    ).unwrap_or_else(|_| genomehubs_query::parse::ResponseStatus {
        hits: 0, ok: false, error: Some("failed to parse response".to_string()), took: 0,
    });

    // Flatten records via the existing parse pipeline
    let results_json = match genomehubs_query::parse::parse_search_json(&raw.to_string()) {
        Ok(s) => s,
        Err(e) => bail!(format!("failed to parse search results: {e}")),
    };
    let results: Vec<Value> = serde_json::from_str(&results_json)
        .unwrap_or_default();

    // Extract search_after cursor for pagination
    let search_after = raw
        .get("hits")
        .and_then(|h| h.get("hits"))
        .and_then(|hits| hits.as_array())
        .and_then(|arr| arr.last())
        .and_then(|last| last.get("sort"))
        .cloned();

    Json(SearchResponse {
        status: ApiStatus::query_ok(status_block.hits, status_block.took),
        url: built_url,
        results,
        search_after,
    })
}
```

### 5. Update `routes/status.rs`

The `supported` list should be a static slice that grows as phases are completed.
Define it as a `const` in `main.rs` or `routes/mod.rs` and pass it to the handler.

Simplest approach: hardcode in `status.rs` and update as each route is added.

```rust
const SUPPORTED_ENDPOINTS: &[&str] = &[
    "/status",
    "/resultFields",
    "/taxonomies",
    "/taxonomicRanks",
    "/indices",
    "/count",
    "/search",           // added in Phase 1
];

pub async fn get_status(Extension(state): Extension<Arc<AppState>>) -> Json<StatusResponse> {
    ...
    Json(StatusResponse {
        status: super::ApiStatus::ok(),
        ready,
        supported: SUPPORTED_ENDPOINTS.iter().map(|s| s.to_string()).collect(),
        last_updated: last,
    })
}
```

### 6. `main.rs` — register route

```rust
.route("/api/v3/search", axum::routing::post(routes::search::post_search))
```

Add to `#[openapi]`:

```rust
paths(
    ...
    routes::search::post_search,
),
components(schemas(
    ...
    routes::search::SearchRequest,
    routes::search::SearchResponse,
))
```

---

## Integration Tests: `crates/genomehubs-api/tests/search_builder.rs`

These tests verify query construction (not HTTP). They call `build_search_body()` directly
and assert on the produced ES JSON:

```rust
use cli_generator::core::query_builder::build_search_body;
use genomehubs_query::query::{SearchQuery, QueryParams};

fn make_query(yaml: &str) -> SearchQuery {
    SearchQuery::from_yaml(yaml).expect("parse failed")
}
fn default_params() -> QueryParams {
    QueryParams::from_yaml("taxonomy: ncbi\n").unwrap()
}

#[test]
fn equality_operator() {
    let q = make_query("index: taxon\nattributes:\n  - name: assembly_level\n    operator: eq\n    value: chromosome\n");
    let body = build_search_body(&q, &default_params(), "taxon", None, "ncbi");
    let body_str = serde_json::to_string(&body).unwrap();
    assert!(body_str.contains("\"term\""), "should produce term query");
}

#[test]
fn inequality_not_equal() {
    let q = make_query("index: taxon\nattributes:\n  - name: assembly_level\n    operator: ne\n    value: contig\n");
    let body = build_search_body(&q, &default_params(), "taxon", None, "ncbi");
    let body_str = serde_json::to_string(&body).unwrap();
    assert!(body_str.contains("\"must_not\""));
}

#[test]
fn range_gte() {
    let q = make_query("index: taxon\nattributes:\n  - name: genome_size\n    operator: gte\n    value: \"1000000000\"\n");
    let body = build_search_body(&q, &default_params(), "taxon", None, "ncbi");
    let body_str = serde_json::to_string(&body).unwrap();
    assert!(body_str.contains("\"range\""));
    assert!(body_str.contains("\"gte\""));
}

#[test]
fn taxon_tree_filter() {
    let q = make_query("index: taxon\ntaxa: [Mammalia]\ntaxon_filter_type: tree\n");
    let body = build_search_body(&q, &default_params(), "taxon", None, "ncbi");
    let body_str = serde_json::to_string(&body).unwrap();
    assert!(body_str.contains("tax_tree") || body_str.contains("lineage"));
}

#[test]
fn field_projection() {
    let q = make_query("index: taxon\nfields:\n  - name: genome_size\n");
    let body = build_search_body(&q, &default_params(), "taxon", Some(&["genome_size"]), "ncbi");
    let body_str = serde_json::to_string(&body).unwrap();
    assert!(body_str.contains("genome_size"));
}

#[test]
fn pagination_offset() {
    let params = QueryParams::from_yaml("size: 50\npage: 3\ntaxonomy: ncbi\n").unwrap();
    let q = make_query("index: taxon\n");
    let body = build_search_body(&q, &params, "taxon", None, "ncbi");
    // page 3, size 50 → from = 100
    let body_str = serde_json::to_string(&body).unwrap();
    assert!(body_str.contains("\"from\":100") || body_str.contains("\"from\": 100"));
}
```

---

## Verification

```bash
cargo build -p genomehubs-api
cargo test -p genomehubs-api

# smoke test against live ES
curl -s -X POST http://localhost:3000/api/v3/search \
  -H 'Content-Type: application/json' \
  -d '{
    "query_yaml": "index: taxon\ntaxa: [Mammalia]\ntaxon_filter_type: tree\nattributes:\n  - name: genome_size\n    operator: exists\n",
    "params_yaml": "size: 10\ntaxonomy: ncbi\n"
  }' | jq '{hits: .status.hits, count: (.results | length)}'
# expected: {"hits": N, "count": 10}

# verify /status now includes /search
curl -s http://localhost:3000/api/v3/status | jq '.supported'
# expected: [..., "/search"]
```

---

## Completion Checklist

- [ ] `es_client.rs` created: `execute_search()`, `execute_count()`
- [ ] `index_name.rs` created: `resolve_index()`, `resolve_index_str()`
- [ ] `AppState` extended with `client: reqwest::Client`
- [ ] `count.rs` refactored to use shared helpers
- [ ] `routes/search.rs` created and registered
- [ ] `/status` returns `supported: [...]` including `"/search"`
- [ ] OpenAPI updated with `SearchRequest` + `SearchResponse` schemas
- [ ] `tests/search_builder.rs` passes (equality, ne, gte, tree filter, projection, pagination)
- [ ] `cargo test -p genomehubs-api` passes
- [ ] Manual smoke test against live ES returns expected shape
