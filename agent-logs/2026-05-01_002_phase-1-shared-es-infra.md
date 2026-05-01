# Phase 1: Shared ES Infrastructure + `/search` - Implementation Complete

**Date**: 2026-05-01
**Status**: ✅ COMPLETED

## Summary

Implemented Phase 1 of the v3 API parity plan. Extracted reusable Elasticsearch infrastructure helpers and implemented the `POST /api/v3/search` endpoint to return paginated results. The `/api/v3/status` endpoint now reports supported endpoints, enabling SDKs to probe at build time.

## Changes Made

### 1. New Files Created

#### `crates/genomehubs-api/src/index_name.rs`

- `resolve_index()` — Maps `SearchIndex` + server suffix to ES index name (e.g., `taxon--ncbi--goat--2021.10.15`)
- `resolve_index_str()` — Variant for string-based index names (currently unused but available for `/resultFields` param)

#### `crates/genomehubs-api/src/es_client.rs`

- `execute_search()` — POST to `{es_base}/{index}/_search`, return parsed ES response
- `execute_count()` — POST to `{es_base}/{index}/_count`, return parsed ES response
- Both handle HTTP errors and JSON parsing failures with descriptive error messages

#### `crates/genomehubs-api/src/routes/search.rs`

- `post_search()` — `POST /api/v3/search` handler
- `SearchRequest` struct — YAML query/params input
- `SearchResponse` struct — Status envelope + flat results + pagination cursor
- Reuses existing `build_search_body()` from cli-generator
- Extracts ES response status and pagination cursor for SDK

#### `crates/genomehubs-api/tests/search_builder.rs`

- 5 integration tests verifying query body construction:
  - `equality_operator` — Basic equality filter
  - `inequality_not_equal` — Negation logic
  - `range_gte` — Range queries with operators
  - `field_projection` — Field selection
  - `pagination_offset` — Offset calculation from page/size

### 2. Modified Files

#### `crates/genomehubs-api/src/main.rs`

- Added `mod es_client;` and `mod index_name;`
- Added `client: reqwest::Client` field to `AppState`
- Initialize client once at startup and share across all handlers
- Updated OpenAPI schema with `post_search`, `SearchRequest`, `SearchResponse`
- Registered `/api/v3/search` route
- Fixed unit test to include `client` in `AppState` initializer

#### `crates/genomehubs-api/src/routes/mod.rs`

- Added `pub mod search;`

#### `crates/genomehubs-api/src/routes/status.rs`

- Added `SUPPORTED_ENDPOINTS` const listing all implemented v3 routes
- Updated handler to populate `supported` field with the list
- `supported` now includes `/search` (and will grow as phases complete)

#### `crates/genomehubs-api/src/routes/count.rs`

- Replaced inline `index_name_for()` function with call to `index_name::resolve_index()`
- Replaced inline `reqwest::Client::new()` with `state.client`
- Updated to call `es_client::execute_count()` instead of direct reqwest call
- Reduced from ~200 lines to ~145 lines
- Now uses same pattern as `/search` for ES communication

### 3. Architectural Changes

**Shared Infrastructure Pattern:**

- All ES endpoints now use the same `es_client` module
- All endpoints use the same `index_name` resolution logic
- Single `reqwest::Client` instance shared across handlers (connection pooling, performance)
- Consistent error handling and response wrapping via `ApiStatus`

**Response Consistency:**

- `/search` follows same envelope structure as `/count` (from Phase 0)
- Status block includes `hits` and `took` extracted from ES response
- Pagination cursor (`search_after`) included for scrolling

## Test Results

```
running 9 tests total:

crates/genomehubs-api/src/main.rs::tests::status_and_cache_routes_work  ... ok
tests/count_builder.rs::builder_returns_empty_for_invalid_yaml            ... ok
tests/count_builder.rs::builder_accepts_minimal_query                     ... ok
tests/count_builder.rs::builder_returns_url_for_valid_yaml                ... ok
tests/search_builder.rs::equality_operator                                ... ok
tests/search_builder.rs::inequality_not_equal                             ... ok
tests/search_builder.rs::range_gte                                        ... ok
tests/search_builder.rs::field_projection                                 ... ok
tests/search_builder.rs::pagination_offset                                ... ok

Test Result: 9 passed; 0 failed
```

Build: ✅ `cargo build -p genomehubs-api` succeeds with 1 unused function warning
Format: ✅ `cargo fmt --check` passes
Lint: ✅ No new clippy warnings in genomehubs-api (existing warning in cli-generator unaffected)

## Unblocked Work

Phase 1 completion unblocks:

- **Phase 2** — Search response parsing (uses `/search` endpoint structure)
- **Phase 3-9** — All downstream phases that depend on shared ES infrastructure

## Files Modified

```
New files (3):
  crates/genomehubs-api/src/index_name.rs
  crates/genomehubs-api/src/es_client.rs
  crates/genomehubs-api/src/routes/search.rs

New tests (1):
  crates/genomehubs-api/tests/search_builder.rs (5 tests)

Modified files (4):
  crates/genomehubs-api/src/main.rs
  crates/genomehubs-api/src/routes/mod.rs
  crates/genomehubs-api/src/routes/status.rs
  crates/genomehubs-api/src/routes/count.rs
```

## Key Design Decisions

1. **Single shared `reqwest::Client`** — Stored in `AppState` to enable connection pooling and reduce per-request overhead. All ES operations reuse it.

2. **Index name resolution** — Extracted to separate module to support both `SearchIndex` enum (query/count/search) and string-based (resultFields query param).

3. **Query builder reuse** — Both `/count` and `/search` call `cli_generator::core::query_builder::build_search_body()` with the same arguments. Single source of truth for ES body construction.

4. **ES client error handling** — All HTTP and JSON parsing errors wrapped in descriptive strings. ES HTTP errors include status code and response body preview (512 chars).

5. **Pagination cursor in response** — `/search` extracts `sort` array from last hit to enable cursor-based pagination (keyset pagination pattern).

## Next Steps

- Begin Phase 2: Search response parsing
- Phase 2 will parse ES `hits` array into flat record format for SDK consumption
