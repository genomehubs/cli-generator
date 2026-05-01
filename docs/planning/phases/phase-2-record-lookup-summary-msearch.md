# Phase 2: `/record`, `/lookup`, `/batchSearch`

**Depends on:** Phase 0 (ApiStatus), Phase 1 (es_client, index_name, AppState.client)
**Blocks:** Phase 3 (SDK methods call these endpoints)
**Estimated scope:** ~4 new files, 1 shared helper extracted

Steps 1–3 (the three routes) are independent and can be implemented in parallel.

**Note:** `/summary` has been deferred to Phase 5 (after report aggregation infrastructure
is in place) so it can reuse histogram aggregation logic rather than implementing its own
nested aggregation query builder. This keeps aggregation logic centralized and avoids
maintenance drift across endpoints.

---

## Goal

Implement the three remaining "simple" (non-aggregation) endpoints:

| Endpoint       | Method | Purpose                                |
| -------------- | ------ | -------------------------------------- |
| `/record`      | GET    | Fetch one or more records by ID        |
| `/lookup`      | GET    | Autocomplete / identifier resolution   |
| `/batchSearch` | POST   | Batch multiple search queries into one |

`/summary` is deferred to Phase 5 to leverage report aggregation infrastructure.

---

## Files to Create

| File                                              | Route                                 |
| ------------------------------------------------- | ------------------------------------- |
| `crates/genomehubs-api/src/routes/record.rs`      | `GET /api/v3/record`                  |
| `crates/genomehubs-api/src/routes/lookup.rs`      | `GET /api/v3/lookup`                  |
| `crates/genomehubs-api/src/routes/batchSearch.rs` | `POST /api/v3/batchSearch`            |
| `crates/genomehubs-api/src/fetch_records.rs`      | Shared `fetch_records_by_id()` helper |

## Files to Modify

| File                                         | Change                                |
| -------------------------------------------- | ------------------------------------- |
| `crates/genomehubs-api/src/routes/mod.rs`    | Add four `pub mod` declarations       |
| `crates/genomehubs-api/src/routes/status.rs` | Extend `SUPPORTED_ENDPOINTS`          |
| `crates/genomehubs-api/src/main.rs`          | Register four routes + update OpenAPI |

---

## Implementation

### Shared helper: `fetch_records.rs`

Used by both `/record` and `/summary`. Fetches one or more full documents from ES.

```rust
use reqwest::Client;
use serde_json::{json, Value};

/// Fetch one or more ES documents by ID.
///
/// Uses `_mget` for multiple IDs; falls back to `_doc/{id}` for single IDs.
/// Returns the `_source` of each found document; missing IDs are silently dropped.
pub async fn fetch_records_by_id(
    client: &Client,
    es_base: &str,
    index: &str,
    ids: &[&str],
) -> Result<Vec<Value>, String> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    if ids.len() == 1 {
        let url = format!("{es_base}/{index}/_doc/{}", ids[0]);
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("ES _doc request failed: {e}"))?;
        if resp.status().as_u16() == 404 {
            return Ok(vec![]);
        }
        let doc: Value = resp.json().await.map_err(|e| format!("parse error: {e}"))?;
        return Ok(doc.get("_source").cloned().into_iter().collect());
    }

    // Multiple IDs: use _mget
    let url = format!("{es_base}/{index}/_mget");
    let body = json!({ "ids": ids });
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("ES _mget request failed: {e}"))?;
    let result: Value = resp.json().await.map_err(|e| format!("parse error: {e}"))?;

    let docs = result
        .get("docs")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|doc| doc.get("found").and_then(|f| f.as_bool()).unwrap_or(false))
                .filter_map(|doc| doc.get("_source").cloned())
                .collect()
        })
        .unwrap_or_default();

    Ok(docs)
}
```

Register in `main.rs`:

```rust
mod fetch_records;
```

---

### 1. `routes/record.rs`

**Request:** `GET /api/v3/record?recordId={id}&result={type}&taxonomy={name}`

Multiple `recordId` values can be passed as repeated params or comma-separated.

```rust
use axum::{extract::Query, Extension, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::{fetch_records, index_name, routes::ApiStatus, AppState};

#[derive(Deserialize)]
pub struct RecordQuery {
    #[serde(rename = "recordId")]
    pub record_id: String,          // comma-separated or single
    pub result: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct RecordItem {
    pub record: Value,
    #[serde(rename = "recordId")]
    pub record_id: String,
    pub result: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct RecordResponse {
    pub status: ApiStatus,
    pub records: Vec<RecordItem>,
}

#[utoipa::path(
    get,
    path = "/api/v3/record",
    params(
        ("recordId" = String, Query, description = "Record ID (comma-separated for multiple)"),
        ("result" = Option<String>, Query, description = "Result type (taxon|assembly|sample)"),
    ),
    responses(
        (status = 200, description = "Record(s)", body = RecordResponse)
    )
)]
pub async fn get_record(
    Query(q): Query<RecordQuery>,
    Extension(state): Extension<Arc<AppState>>,
) -> Json<RecordResponse> {
    let result_type = q.result.as_deref().unwrap_or(&state.default_result);
    let idx = index_name::resolve_index_str(result_type, &state);

    let ids: Vec<&str> = q.record_id.split(',').map(str::trim).collect();

    let docs = match fetch_records::fetch_records_by_id(
        &state.client, &state.es_base, &idx, &ids,
    ).await {
        Ok(d) => d,
        Err(e) => return Json(RecordResponse {
            status: ApiStatus::error(e),
            records: vec![],
        }),
    };

    let records: Vec<RecordItem> = ids.iter().zip(docs.iter()).map(|(id, doc)| RecordItem {
        record: doc.clone(),
        record_id: id.to_string(),
        result: result_type.to_string(),
    }).collect();

    Json(RecordResponse {
        status: ApiStatus::query_ok(records.len() as u64, 0),
        records,
    })
}
```

---

### 2. `routes/lookup.rs`

**Request:** `GET /api/v3/lookup?searchTerm={term}&result={type}`

Three-stage fallback:

1. Prefix match on `scientific_name` (SAYT) — requires `scientific_name.sayt` field mapping
   in the ES index; if that field doesn't exist, falls through to stage 2
2. Exact / wildcard match on primary identifier and names fields
3. ES suggest (phrase-suggest) — only if `scientific_name.trigram` or similar field exists;
   otherwise return empty results with a descriptive status

**Audit ES index mapping before implementing stage 1 and 3.** Use:

```bash
curl http://localhost:9200/{taxon_index}/_mapping | jq '.[] | .mappings.properties.scientific_name'
```

If `sayt` sub-field exists → implement stage 1.
If `trigram` sub-field exists → implement stage 3.

```rust
#[derive(Deserialize)]
pub struct LookupQuery {
    #[serde(rename = "searchTerm")]
    pub search_term: String,
    pub result: Option<String>,
    pub size: Option<usize>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct LookupResult {
    pub id: String,
    pub name: String,
    pub rank: Option<String>,
    pub reason: String,    // "sayt" | "exact" | "wildcard" | "suggest"
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct LookupResponse {
    pub status: ApiStatus,
    pub results: Vec<LookupResult>,
}
```

**Stage 1 — SAYT prefix match** (if `scientific_name.sayt` exists):

```rust
fn build_sayt_query(term: &str, size: usize) -> serde_json::Value {
    json!({
        "size": size,
        "query": {
            "multi_match": {
                "query": term,
                "type": "bool_prefix",
                "fields": ["scientific_name", "scientific_name._2gram", "scientific_name._3gram"]
            }
        },
        "_source": ["taxon_id", "scientific_name", "taxon_rank"]
    })
}
```

**Stage 2 — Exact/wildcard match**:

```rust
fn build_lookup_query(term: &str, size: usize) -> serde_json::Value {
    let wildcard_term = if term.contains('*') { term.to_string() } else { format!("{term}*") };
    json!({
        "size": size,
        "query": {
            "bool": {
                "should": [
                    { "match": { "scientific_name": { "query": term, "boost": 2 } } },
                    { "wildcard": { "scientific_name.keyword": { "value": wildcard_term } } },
                    {
                        "nested": {
                            "path": "taxon_names",
                            "query": {
                                "multi_match": {
                                    "query": term,
                                    "fields": ["taxon_names.name"]
                                }
                            }
                        }
                    }
                ]
            }
        },
        "_source": ["taxon_id", "scientific_name", "taxon_rank"]
    })
}
```

**Stage 3 — ES suggest** (if `scientific_name.trigram` exists):

```rust
fn build_suggest_query(term: &str) -> serde_json::Value {
    json!({
        "suggest": {
            "name_suggest": {
                "text": term,
                "phrase": {
                    "field": "scientific_name.trigram",
                    "size": 5,
                    "gram_size": 3,
                    "confidence": 1,
                    "collate": {
                        "query": { "source": { "match": { "{{field_name}}": "{{suggestion}}" } } },
                        "prune": true
                    }
                }
            }
        }
    })
}
```

Handler skeleton:

```rust
pub async fn get_lookup(
    Query(q): Query<LookupQuery>,
    Extension(state): Extension<Arc<AppState>>,
) -> Json<LookupResponse> {
    let result_type = q.result.as_deref().unwrap_or(&state.default_result);
    let idx = index_name::resolve_index_str(result_type, &state);
    let size = q.size.unwrap_or(10);

    // Stage 1: SAYT (only if sayt field available — check at startup and cache)
    // Stage 2: Exact/wildcard
    let body = build_lookup_query(&q.search_term, size);
    match es_client::execute_search(&state.client, &state.es_base, &idx, &body).await {
        Ok(resp) => {
            let results = extract_lookup_results(&resp, "wildcard");
            if !results.is_empty() {
                return Json(LookupResponse {
                    status: ApiStatus::query_ok(results.len() as u64, 0),
                    results,
                });
            }
        }
        Err(e) => return Json(LookupResponse { status: ApiStatus::error(e), results: vec![] }),
    }

    // Stage 3: Suggest (only if trigram field available)
    Json(LookupResponse {
        status: ApiStatus::query_ok(0, 0),
        results: vec![],
    })
}
```

**ES mapping audit:** Add a startup check in `es_metadata::populate_cache()` that reads the
index mapping and stores `has_sayt_field: bool` and `has_trigram_field: bool` in
`MetadataCache`. The lookup handler reads these flags to decide which stages to run.

---

### 3. `routes/batchSearch.rs`

**Request:** `POST /api/v3/batchSearch` with body `{ searches: [{ query_yaml, params_yaml }] }`

Uses ES `_msearch` API to batch all queries into a single HTTP round-trip.

```rust
#[derive(Deserialize, utoipa::ToSchema)]
pub struct BatchSearchItem {
    pub query_yaml: String,
    pub params_yaml: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct BatchSearchRequest {
    pub searches: Vec<BatchSearchItem>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct BatchSearchResultItem {
    pub status: ApiStatus,
    pub count: usize,
    pub hits: Vec<serde_json::Value>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct BatchSearchResponse {
    pub status: ApiStatus,
    pub results: Vec<BatchSearchResultItem>,
}
```

**ES msearch body format** (NDJSON — alternating header + body lines):

```rust
fn build_msearch_body(searches: &[(String, serde_json::Value)]) -> String {
    // Each search is two lines: index header + query body
    searches
        .iter()
        .flat_map(|(index, body)| {
            let header = serde_json::json!({ "index": index });
            vec![
                serde_json::to_string(&header).unwrap(),
                serde_json::to_string(body).unwrap(),
            ]
        })
        .collect::<Vec<_>>()
        .join("\n") + "\n"
}

async fn execute_msearch(
    client: &reqwest::Client,
    es_base: &str,
    ndjson_body: &str,
) -> Result<serde_json::Value, String> {
    let url = format!("{es_base}/_msearch");
    let resp = client
        .post(&url)
        .header("Content-Type", "application/x-ndjson")
        .body(ndjson_body.to_string())
        .send()
        .await
        .map_err(|e| format!("msearch request failed: {e}"))?;
    resp.json().await.map_err(|e| format!("parse error: {e}"))
}
```

Handler:

```rust
pub async fn post_batchSearch(
    Extension(state): Extension<Arc<AppState>>,
    Json(req): Json<BatchSearchRequest>,
) -> Json<BatchSearchResponse> {
    if req.searches.len() > 100 {
        return Json(BatchSearchResponse {
            status: ApiStatus::error("maximum 100 searches per request"),
            results: vec![],
        });
    }

    // Parse and build all searches
    let mut index_bodies: Vec<(String, serde_json::Value)> = vec![];
    for item in &req.searches {
        let query = match genomehubs_query::query::SearchQuery::from_yaml(&item.query_yaml) {
            Ok(q) => q,
            Err(e) => return Json(BatchSearchResponse {
                status: ApiStatus::error(format!("failed to parse query_yaml: {e}")),
                results: vec![],
            }),
        };
        let params = match genomehubs_query::query::QueryParams::from_yaml(&item.params_yaml) {
            Ok(p) => p,
            Err(e) => return Json(BatchSearchResponse {
                status: ApiStatus::error(format!("failed to parse params_yaml: {e}")),
                results: vec![],
            }),
        };
        let idx = index_name::resolve_index(&query.index, &state);
        let group = match query.index {
            genomehubs_query::query::SearchIndex::Taxon => "taxon",
            genomehubs_query::query::SearchIndex::Assembly => "assembly",
            genomehubs_query::query::SearchIndex::Sample => "sample",
        };
        let body = cli_generator::core::query_builder::build_search_body(
            &query, &params, group, None, &state.default_taxonomy,
        );
        index_bodies.push((idx, body));
    }

    let ndjson = build_msearch_body(&index_bodies);
    let raw = match execute_msearch(&state.client, &state.es_base, &ndjson).await {
        Ok(v) => v,
        Err(e) => return Json(BatchSearchResponse {
            status: ApiStatus::error(e),
            results: vec![],
        }),
    };

    // Parse each response from the `responses` array
    let responses = raw.get("responses").and_then(|r| r.as_array()).cloned().unwrap_or_default();
    let mut total_hits = 0u64;
    let results: Vec<MSearchResultItem> = responses.iter().map(|resp| {
        let raw_str = resp.to_string();
        let rs = genomehubs_query::parse::parse_response_status(&raw_str)
            .unwrap_or(genomehubs_query::parse::ResponseStatus { hits: 0, ok: false, error: None, took: 0 });
        let hits_json = genomehubs_query::parse::parse_search_json(&raw_str).unwrap_or_default();
        let hits: Vec<serde_json::Value> = serde_json::from_str(&hits_json).unwrap_or_default();
        total_hits += rs.hits;
        MSearchResultItem {
            status: ApiStatus::query_ok(rs.hits, rs.took),
            count: hits.len(),
            hits,
        }
    }).collect();

    Json(MSearchResponse {
        status: ApiStatus::query_ok(total_hits, 0),
        results,
    })
}
```

---

## `main.rs` Changes

Register all four routes:

```rust
.route("/api/v3/record", get(routes::record::get_record))
.route("/api/v3/lookup", get(routes::lookup::get_lookup))
.route("/api/v3/summary", get(routes::summary::get_summary))
.route("/api/v3/msearch", axum::routing::post(routes::msearch::post_msearch))
```

Extend `SUPPORTED_ENDPOINTS` in `status.rs`:

```rust
const SUPPORTED_ENDPOINTS: &[&str] = &[
    "/status", "/resultFields", "/taxonomies", "/taxonomicRanks", "/indices",
    "/count", "/search", "/record", "/lookup", "/summary", "/msearch",
];
```

Add all response types to `#[openapi]` components.

---

## ES Mapping Audit (before implementation)

Before writing the SAYT prefix logic in `/lookup` and the lineage aggregation in `/summary`,
run these mapping queries against the live ES instance:

```bash
# Check for sayt field (lookup stage 1)
curl -s http://localhost:9200/{taxon_index}/_mapping \
  | jq '.. | .scientific_name? | select(. != null)'

# Check for trigram field (lookup stage 3)
curl -s http://localhost:9200/{taxon_index}/_mapping \
  | jq '.. | .trigram? | select(. != null)'

# Check attributes nested path (summary agg)
curl -s http://localhost:9200/{taxon_index}/_mapping \
  | jq '.. | .attributes? | .type? | select(. == "nested")'

# Get a sample document to understand field shapes
curl -s http://localhost:9200/{taxon_index}/_search \
  -H 'Content-Type: application/json' \
  -d '{"size":1}' | jq '.hits.hits[0]._source | keys'
```

Results should be documented in a comment at the top of `lookup.rs` and `summary.rs`
so the implemented query paths match the actual mapping.

---

## Verification

```bash
cargo build -p genomehubs-api
cargo test -p genomehubs-api

# /record
curl -s "http://localhost:3000/api/v3/record?recordId=2759&result=taxon" \
  | jq '{success: .status.success, count: (.records | length)}'

# /lookup
curl -s "http://localhost:3000/api/v3/lookup?searchTerm=Mammal&result=taxon" \
  | jq '{hits: .status.hits, first: .results[0].name}'

# /msearch
curl -s -X POST http://localhost:3000/api/v3/msearch \
  -H 'Content-Type: application/json' \
  -d '{"searches":[
    {"query_yaml":"index: taxon\ntaxa: [Mammalia]\ntaxon_filter_type: tree\n","params_yaml":"size: 5\ntaxonomy: ncbi\n"},
    {"query_yaml":"index: taxon\ntaxa: [Insecta]\ntaxon_filter_type: tree\n","params_yaml":"size: 5\ntaxonomy: ncbi\n"}
  ]}' | jq '{total_hits: .status.hits, result_count: (.results | length)}'
```

---

## Completion Checklist

- [ ] `fetch_records.rs` extracted and registered in `main.rs`
- [ ] `routes/record.rs` created, registered, OpenAPI added
- [ ] ES mapping audited for sayt/trigram fields; flags added to `MetadataCache`
- [ ] `routes/lookup.rs` created with 2 (or 3) stages based on mapping audit
- [ ] `routes/summary.rs` created; aggregation path verified against ES mapping
- [ ] `routes/msearch.rs` created, 100-search limit enforced
- [ ] All four routes registered in `main.rs`
- [ ] `SUPPORTED_ENDPOINTS` updated in `status.rs`
- [ ] `cargo test -p genomehubs-api` passes
- [ ] Manual smoke tests pass for all four endpoints
