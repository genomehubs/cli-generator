# Phase 0: Return Envelope Consistency - Implementation Complete

**Date**: 2026-05-01
**Status**: ✅ COMPLETED

## Summary

Implemented Phase 0 of the v3 API parity plan. All v3 API endpoints now return a uniform `ApiStatus` envelope containing `success`, `hits`, `took`, and optional `error` fields, replacing ad-hoc response structures.

## Changes Made

### 1. Added `ApiStatus` struct to `crates/genomehubs-api/src/routes/mod.rs`

- **Fields**: `success: bool`, `hits: Option<u64>`, `took: Option<u64>`, `error: Option<String>`
- **Derives**: `Debug, Clone, Serialize, utoipa::ToSchema`
- **Constructor methods**:
  - `ok()` — metadata endpoint success (no hits/took)
  - `query_ok(hits, took)` — query endpoint success
  - `error(msg)` — any failure

### 2. Updated 6 response structs

| File                        | Changes                                                                                  |
| --------------------------- | ---------------------------------------------------------------------------------------- |
| `routes/status.rs`          | Added `status: ApiStatus`, `supported: Vec<String>`                                      |
| `routes/taxonomic_ranks.rs` | Added `status: ApiStatus`; **renamed field `taxonomic_ranks` → `ranks`**                 |
| `routes/taxonomies.rs`      | Added `status: ApiStatus`                                                                |
| `routes/indices.rs`         | Added `status: ApiStatus`                                                                |
| `routes/result_fields.rs`   | Replaced `status: serde_json::Value` with `status: ApiStatus`                            |
| `routes/count.rs`           | **Refactored response**: removed `hits`, `ok`, `error` fields; added `status: ApiStatus` |

### 3. Updated handlers

- All 7 handlers now construct `ApiStatus` using the constructor methods
- `post_count` extracts ES response `took` value and calls `query_ok(hits, took)`
- Error returns use `ApiStatus::error(msg)` instead of inline JSON

### 4. Updated OpenAPI metadata

- Added `routes::ApiStatus` to `main.rs` components schema list
- Added `post_count` to paths list and `CountResponse` to components

### 5. Fixed unit tests in `main.rs`

- Updated all assertions to check `status_body.status.success` instead of `status_body.ready`
- Updated `ranks.taxonomic_ranks` to `ranks.ranks` for the renamed field
- Updated `rf.status.get("success")` to `rf.status.success`

## Test Results

```
running 1 test
test tests::status_and_cache_routes_work ... ok

Running tests/count_builder.rs
running 3 tests
test builder_returns_empty_for_invalid_yaml ... ok
test builder_accepts_minimal_query ... ok
test builder_returns_url_for_valid_yaml ... ok

test result: ok. 4 passed; 0 failed
```

Build: ✅ `cargo build -p genomehubs-api` completes successfully

## Breaking Changes

1. **Response envelope structure** — All responses now have `status: { success, hits?, took?, error? }` at the top level
2. **taxonomicRanks field renamed** — `/api/v3/taxonomicRanks` now returns `{ status, ranks, last_updated }` instead of `{ status, taxonomic_ranks, last_updated }`
3. **count response restructured** — `/api/v3/count` now returns `{ status: { success, hits, took }, url }` instead of `{ status, url, hits, ok, error }`

## Unblocked Work

Phase 0 completion unblocks all subsequent phases:

- **Phase 1** — Elasticsearch client extraction (uses ApiStatus)
- **Phase 2** — Search response parsing (uses ApiStatus)
- Phases 3-9 — All depend on Phase 0 structure

## Files Modified

1. `crates/genomehubs-api/src/routes/mod.rs` — Added ApiStatus struct
2. `crates/genomehubs-api/src/routes/status.rs` — Updated StatusResponse
3. `crates/genomehubs-api/src/routes/taxonomic_ranks.rs` — Updated RanksResponse; renamed field
4. `crates/genomehubs-api/src/routes/taxonomies.rs` — Updated TaxonomiesResponse
5. `crates/genomehubs-api/src/routes/indices.rs` — Updated IndicesResponse
6. `crates/genomehubs-api/src/routes/result_fields.rs` — Updated ResultFieldsResponse
7. `crates/genomehubs-api/src/routes/count.rs` — Refactored CountResponse
8. `crates/genomehubs-api/src/main.rs` — Updated OpenAPI schema; fixed unit tests

## Next Steps

- Begin Phase 1: Elasticsearch client extraction
- Phase 1 will introduce a shared `SearchClient` to reduce code duplication in `post_count` and future search endpoints
