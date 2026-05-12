# 2026-05-11_003 — Phase 14: PhyloPic proxy endpoints

## Summary

Implemented the PhyloPic proxy feature for the genomehubs API, adding:

- `GET /api/v3/phylopic` — resolve a single taxon's silhouette
- `POST /api/v3/phylopic/batch` — bulk resolve up to 200 taxa

Also fixed three CI issues from the previous session (Python parity, Rust clippy), added `to_tidy_records` SDK parity across all three SDK languages, and documented three new canonical SDK methods.

---

## Changes

### New files

- `crates/genomehubs-api/src/phylopic_client.rs` — PhyloPic v2 HTTP client with 3-step resolution pipeline (NCBI → name → GBIF fallback), shared in-memory cache keyed by taxon ID, 24-hour build staleness eviction, and 16 unit tests.
- `crates/genomehubs-api/src/routes/phylopic.rs` — Axum route handlers with OpenAPI (`utoipa`) annotations. Checks cache first, fetches taxon info from Elasticsearch, delegates to the client.

### Modified files

- `crates/genomehubs-api/src/main.rs` — Extended `AppState` with `phylopic_cache`, added background build-refresh task (24-hour interval), registered new routes, added PhyloPic types to OpenAPI schema list. Fixed test `AppState` initialiser to include `phylopic_cache`.
- `crates/genomehubs-api/src/routes/mod.rs` — Added `pub mod phylopic;`.
- `crates/genomehubs-api/Cargo.toml` — Added `urlencoding = "2"`.
- `crates/genomehubs-query/src/lineage_summary.rs` — `sort_by` → `sort_by_key` (clippy `unnecessary_sort_by`).
- `src/commands/new.rs` — Collapsed `else { if … }` to `else if …` (clippy `collapsible_else_if`).
- `python/cli_generator/query.py` — Fixed isort: split grouped imports for `parse_search_json`, `parse_search_with_lineage_summary`, `to_tidy_records`.
- `tests/python/test_sdk_parity.py` — Added `set_lineage_rank_summary`, `to_flat_records`, `to_tidy_records` to `CANONICAL_METHODS`; removed them from the extra-methods allowlist.
- `templates/docs/reference/query-builder.qmd.tera` — Documented the three new methods.

---

## Architecture decisions

### 3-step resolution pipeline

The PhyloPic v2 API does not guarantee a silhouette for every NCBI taxon. The pipeline tries:

1. Direct NCBI ID lookup via `/api/v2/resolve/phylodb:ncbi/{id}`
2. Scientific-name lookup via `/api/v2/autocomplete?query=…&options[size]=5`
3. GBIF taxonomy key lookup via the same autocomplete with GBIF-sourced synonyms

This maximises coverage while keeping round-trips minimal for cache hits.

### Cache invalidation

PhyloPic publishes a monotonically increasing "build" integer. Cached entries store the build number at fetch time; entries are evicted lazily on access when the current build number has incremented. A background task refreshes the build number every 24 hours.

### Source classification

`PhylopicSource::Primary` — the matched node is directly associated with the requested taxon.
`PhylopicSource::Descendant` — matched via a child node.
`PhylopicSource::Ancestral` — matched via an ancestor node (most common fallback).

---

## Testing

- 16 unit tests in `phylopic_client.rs` covering: licence SPDX normalisation, ID prefix stripping (v2 regression guard), cache hit/miss/staleness, source classification, synonym matching, image file extraction, aspect-ratio computation.
- Existing API integration tests (`status_and_cache_routes_work`) updated for new `AppState` field.
- All `cargo test -p genomehubs-api` tests pass.
- `cargo clippy -p genomehubs-api -- -D warnings` clean.
- All Python CI checks pass (black, isort, pyright, pytest).

---

## Known limitations / follow-up

- `fetch_taxon_info()` in `routes/phylopic.rs` queries Elasticsearch directly at `/_doc/taxon-{id}`. Integration testing against a live ES instance has not been performed.
- Batch endpoint enforces a 1–200 limit; this matches common API design practice but is not derived from a PhyloPic constraint.
