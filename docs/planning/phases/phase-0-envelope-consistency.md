# Phase 0: Return Envelope Consistency

**Depends on:** nothing — fix-first before adding new routes
**Blocks:** Phases 1–9 (all new routes must use the shared struct)
**Estimated scope:** ~6 files, no new files

---

## Goal

Every v3 endpoint returns `{ status: { success, hits?, took?, error? }, ...payload }`.
This phase:

1. Defines a shared `ApiStatus` struct in `routes/mod.rs`
2. Renames `taxonomic_ranks` → `ranks` in the `RanksResponse` struct (the v2 field name)
3. Updates all six existing response structs to embed `ApiStatus`

No behaviour changes — only response shape changes.

---

## Background: Current State

Every existing route returns its own ad-hoc envelope with no consistent `status` block.
`/count` has `ok: bool` + `error: Option<String>` but no `status` wrapper.
`/taxonomicRanks` uses the key `taxonomic_ranks` (should be `ranks`).

The v2 API uniformly returns:

```json
{ "status": { "success": true, "hits": 42, "took": 15 }, ...payload }
```

For metadata endpoints that have no result count, `hits` and `took` are omitted.

---

## Files to Change

| File                                                  | Change                                                      |
| ----------------------------------------------------- | ----------------------------------------------------------- |
| `crates/genomehubs-api/src/routes/mod.rs`             | Add `ApiStatus` struct + `ApiStatusBuilder`                 |
| `crates/genomehubs-api/src/routes/status.rs`          | Embed `ApiStatus` in `StatusResponse`                       |
| `crates/genomehubs-api/src/routes/taxonomic_ranks.rs` | Rename field `taxonomic_ranks` → `ranks`; embed `ApiStatus` |
| `crates/genomehubs-api/src/routes/taxonomies.rs`      | Embed `ApiStatus`                                           |
| `crates/genomehubs-api/src/routes/indices.rs`         | Embed `ApiStatus`                                           |
| `crates/genomehubs-api/src/routes/result_fields.rs`   | Replace ad-hoc `status: serde_json::Value` with `ApiStatus` |
| `crates/genomehubs-api/src/routes/count.rs`           | Replace `ok`/`error` fields with embedded `ApiStatus`       |
| `crates/genomehubs-api/src/main.rs`                   | Update `#[openapi]` component list for renamed structs      |

---

## Implementation

### 1. `routes/mod.rs` — shared `ApiStatus`

Add at the top of the existing `pub mod ...` declarations:

```rust
use serde::Serialize;

/// Uniform status block present in every v3 API response.
///
/// Metadata-only endpoints (taxonomies, ranks, indices) omit `hits` and `took`.
/// Query endpoints always populate all four fields.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ApiStatus {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hits: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub took: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ApiStatus {
    /// Status for a successful metadata-only endpoint (no hits/took).
    pub fn ok() -> Self {
        Self { success: true, hits: None, took: None, error: None }
    }

    /// Status for a successful query endpoint.
    pub fn query_ok(hits: u64, took: u64) -> Self {
        Self { success: true, hits: Some(hits), took: Some(took), error: None }
    }

    /// Status for a failed request.
    pub fn error(msg: impl Into<String>) -> Self {
        Self { success: false, hits: None, took: None, error: Some(msg.into()) }
    }
}
```

### 2. `routes/taxonomic_ranks.rs` — rename field

```rust
// BEFORE
pub struct RanksResponse {
    pub taxonomic_ranks: Vec<String>,
    pub last_updated: Option<String>,
}

// AFTER
#[derive(Serialize, utoipa::ToSchema)]
pub struct RanksResponse {
    pub status: super::ApiStatus,
    pub ranks: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}
```

Handler body change (return value only):

```rust
Json(RanksResponse {
    status: super::ApiStatus::ok(),
    ranks,           // was: taxonomic_ranks
    last_updated: last,
})
```

### 3. `routes/status.rs`

```rust
#[derive(Serialize, utoipa::ToSchema)]
pub struct StatusResponse {
    pub status: super::ApiStatus,
    pub ready: bool,
    pub supported: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}
```

The `supported` field is the Phase 1 extension but add the struct field now; populate it with
the current list of implemented endpoints so Phase 1 can simply extend it.

Initial `supported` list (current endpoints only):

```rust
let supported = vec![
    "/status".to_string(),
    "/resultFields".to_string(),
    "/taxonomies".to_string(),
    "/taxonomicRanks".to_string(),
    "/indices".to_string(),
    "/count".to_string(),
];
```

### 4. `routes/taxonomies.rs`

```rust
#[derive(Serialize, utoipa::ToSchema)]
pub struct TaxonomiesResponse {
    pub status: super::ApiStatus,
    pub taxonomies: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}
// handler: status: super::ApiStatus::ok()
```

### 5. `routes/indices.rs`

```rust
#[derive(Serialize, utoipa::ToSchema)]
pub struct IndicesResponse {
    pub status: super::ApiStatus,
    pub indices: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}
// handler: status: super::ApiStatus::ok()
```

### 6. `routes/result_fields.rs`

Replace the existing `pub status: serde_json::Value` field:

```rust
#[derive(Serialize, utoipa::ToSchema)]
pub struct ResultFieldsResponse {
    pub status: super::ApiStatus,
    pub fields: serde_json::Value,
    pub identifiers: serde_json::Value,
    pub hub: String,
    pub release: String,
    pub source: String,
}
```

In the handler, replace the `json!({"success":...})` construction:

```rust
// success path:
status: super::ApiStatus::ok(),
// error path (no attr_types in cache):
return Json(ResultFieldsResponse {
    status: super::ApiStatus::error("no attr_types in cache"),
    fields: json!({}),
    identifiers: json!({}),
    hub: state.hub_name.clone(),
    release: state.default_version.clone(),
    source: "api-v3".to_string(),
});
```

### 7. `routes/count.rs`

```rust
#[derive(Serialize, utoipa::ToSchema)]
pub struct CountResponse {
    pub status: super::ApiStatus,
    /// The executed ES URL
    pub url: String,
}
```

Replace return values:

```rust
// error paths (parse failures, ES errors):
return Json(CountResponse {
    status: super::ApiStatus::error(format!("failed to parse query_yaml: {}", e)),
    url: "".to_string(),
});

// success path:
Json(CountResponse {
    status: super::ApiStatus::query_ok(hits, took),
    url: built_url,
})
```

### 8. `main.rs` — OpenAPI components

Add `routes::mod::ApiStatus` to the components list:

```rust
#[derive(OpenApi)]
#[openapi(
    paths(...),
    components(schemas(
        routes::ApiStatus,           // new
        routes::result_fields::ResultFieldsResponse,
        routes::status::StatusResponse,
        routes::taxonomies::TaxonomiesResponse,
        routes::taxonomic_ranks::RanksResponse,
        routes::indices::IndicesResponse,
        routes::count::CountResponse,
    ))
)]
struct ApiDoc;
```

---

## Test: `main.rs` inline tests

The existing `status_and_cache_routes_work` test in `main.rs` calls the handler directly and
checks the response shape. Update the assertions after the struct change:

```rust
// BEFORE
assert_eq!(status_resp.ready, true);

// AFTER
assert_eq!(status_resp.status.success, true);
assert_eq!(status_resp.ready, true);
assert!(status_resp.supported.contains(&"/count".to_string()));
```

Similarly update any test that checks `taxonomic_ranks`:

```rust
// BEFORE
assert!(!ranks_resp.taxonomic_ranks.is_empty());
// AFTER
assert!(!ranks_resp.ranks.is_empty());
```

---

## Verification

```bash
cargo build -p genomehubs-api
cargo test -p genomehubs-api

# Manual smoke tests against live ES:
curl -s http://localhost:3000/api/v3/status | jq '.status'
# expected: {"success":true}

curl -s http://localhost:3000/api/v3/taxonomicRanks | jq '.ranks[:3]'
# expected: ["species","genus","family"] (not "taxonomic_ranks" key)

curl -s -X POST http://localhost:3000/api/v3/count \
  -H 'Content-Type: application/json' \
  -d '{"query_yaml":"index: taxon\n","params_yaml":"taxonomy: ncbi\n"}' | jq '.status'
# expected: {"success":true,"hits":N,"took":M}
```

---

## Completion Checklist

- [ ] `ApiStatus` struct + `impl ApiStatus` in `routes/mod.rs`
- [ ] `taxonomic_ranks.rs`: field renamed, `ApiStatus` embedded
- [ ] `status.rs`: `ApiStatus` embedded, `supported` field added
- [ ] `taxonomies.rs`: `ApiStatus` embedded
- [ ] `indices.rs`: `ApiStatus` embedded
- [ ] `result_fields.rs`: `ApiStatus` replaces ad-hoc `serde_json::Value`
- [ ] `count.rs`: `ApiStatus` replaces `ok`/`error`/`hits` fields
- [ ] `main.rs`: OpenAPI components updated, tests updated
- [ ] `cargo test -p genomehubs-api` passes
