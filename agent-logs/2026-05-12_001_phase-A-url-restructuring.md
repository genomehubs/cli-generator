# 2026-05-12_001 — Phase A: URL restructuring (batch + metadata)

## Summary

Implemented all Phase A URL renames agreed in `docs/planning/phases/phase-XX-metadata-endpoints.md`. No new functionality; purely a URL reshape.

## Changes

### Rust API (`crates/genomehubs-api/`)

| File                                            | Change                                                                                                                                                  |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/routes/countBatch.rs` → `count_batch.rs`   | Renamed; updated utoipa path `/countBatch` → `/count/batch`; removed `#[allow(non_snake_case)]`; renamed handler `post_countBatch` → `post_count_batch` |
| `src/routes/searchBatch.rs` → `search_batch.rs` | Same treatment; `/searchBatch` → `/search/batch`; `post_searchBatch` → `post_search_batch`                                                              |
| `src/routes/mod.rs`                             | Removed `#[path = "countBatch.rs"]` and `#[path = "searchBatch.rs"]`; added `pub mod metadata;`                                                         |
| `src/routes/indices.rs`                         | utoipa path → `/api/v3/metadata/indices`                                                                                                                |
| `src/routes/result_fields.rs`                   | utoipa path → `/api/v3/metadata/fields`                                                                                                                 |
| `src/routes/taxonomies.rs`                      | utoipa path → `/api/v3/metadata/taxonomies`                                                                                                             |
| `src/routes/taxonomic_ranks.rs`                 | utoipa path → `/api/v3/metadata/ranks`                                                                                                                  |
| `src/routes/metadata.rs`                        | **New file** — `GET /api/v3/metadata` aggregator returning `{ indices, taxonomies, ranks }` in one call                                                 |
| `src/routes/status.rs`                          | `SUPPORTED_ENDPOINTS` updated to new paths; `phylopic` + `phylopic/batch` also added (were missing)                                                     |
| `src/main.rs`                                   | Route registrations, OpenAPI paths/schemas updated throughout                                                                                           |

### SDK templates

- `templates/python/query.py.tera`: `searchBatch` → `search/batch`, `countBatch` → `count/batch`
- `templates/js/query.js`: same
- `templates/r/query.R`: same
- `templates/docs/reference/query-builder.qmd.tera`: curl examples updated

### Live SDK

- `python/cli_generator/query.py`: same two URL changes

### Tests

- `tests/api_endpoints.rs`: all URL strings updated
- `tests/python/test_batch_operations.py`: all URL assertions updated (including a `v4/searchBatch` edge case missed by bulk sed)
- `tests/javascript/test_batch_operations.mjs`: URL strings updated

### Examples / docs

- `examples/test-queries.sh`, `examples/batch/*.yaml`, `examples/QUERY-EXAMPLES.md`, `examples/README-STRUCTURE.md`: URL strings updated
- `GETTING_STARTED-api.md`: endpoint table rewritten; `curl` example updated

## Verification

- `cargo clippy --all-targets -- -D warnings`: clean
- `cargo test --workspace --lib`: 272/272 passed
- `pytest tests/python/`: 483 passed, 12 skipped (live-API tests skip without server)

## What was NOT changed

- SDK method names (`search_batch`, `searchBatch`, `count_batch`, `countBatch`) — only the URL paths they call
- Agent logs and internal code comments referencing `resultFields` — historical docs
- `docs/resultfields-implementation-guide.md` and `docs/api-audit-executive-summary.md` — v2 planning docs, not user-facing

## Next step

Phase B: Metadata SDK methods (`metadata()`, `indices()`, `fields(index)`, `taxonomies()`, `ranks()`) across Python, JS, R — see planning doc Part B.
