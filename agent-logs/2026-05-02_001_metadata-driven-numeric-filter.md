---
date: 2026-05-02
agent: GitHub Copilot
model: Claude Haiku 5.5
task: Fix numeric feature filters returning zero by using only the metadata-declared value field
files_changed:
  - crates/genomehubs-api/src/report/filter_expr.rs
  - crates/genomehubs-api/src/report/arc.rs
  - crates/genomehubs-api/src/report/report_types.rs
  - crates/genomehubs-api/src/routes/report.rs
  - crates/genomehubs-api/src/routes/chain_executor.rs
---

## Task summary

Arc feature filters with numeric thresholds (for example `assembly_span>20000000`) returned zero hits, while the same filter without the `>n` threshold worked. The cause was that the numeric range clause for each attribute was emitted for every value type (`long`, `float`, `half_float`). Elasticsearch rejects an out-of-range `half_float` bound with HTTP 400, which failed the whole `should` query. The shared count helper then reported zero without surfacing the error.

## Key decisions

- **Decision:** Resolve the numeric value field from the attribute metadata cache (`get_attribute_value_field`) and emit one range clause on that field only.
  - **Alternative considered:** Skip `half_float` when the bound is outside the half-float range. Rejected by the user: each attribute stores one `*_value` determined by its declared type, so the check should target that field alone.
- **Decision:** Thread `CacheRef` from `AppState.cache` through `filter_expr_to_es_query` and all callers (tree status filter, arc, chain sub-queries).
  - **Alternative considered:** Look up types inside each caller. Rejected because it duplicates logic across callers.
- **Decision:** `count_matching` in `arc.rs` returns an error when the ES response has no `count`, instead of defaulting to zero.
  - **Alternative considered:** Keep the silent default. Rejected because it hides ES errors as empty results.
- **Decision:** `execute_batch` in `chain_executor.rs` takes `&AppState` in place of separate `client` and `es_base` arguments. This keeps the parameter count within the clippy limit of 7 while giving access to the cache.

## Interaction log

| Turn | Role  | Summary                                                                                        |
| ---- | ----- | ---------------------------------------------------------------------------------------------- |
| 1    | User  | Feature query with `>n` returns nothing; asked to find the cause                               |
| 2    | Agent | Queried ES directly; found HTTP 400 `[half_float] supports only finite values`                 |
| 3    | Agent | Added a half_float skip workaround                                                             |
| 4    | User  | Rejected the workaround: numeric checks must use only the metadata-declared value field        |
| 5    | Agent | Made `NumericRange` metadata-driven, threaded the cache, removed the workaround, updated tests |
| 6    | Agent | Ran fmt, tests and clippy; restarted the API; confirmed the feature count                      |

## Changes made

- `crates/genomehubs-api/src/report/filter_expr.rs`: `NumericRange` uses `get_attribute_value_field`; `CacheRef` alias; `build_nested_attribute_query` and `filter_expr_to_es_query` take the cache; removed `HALF_FLOAT_MAX` and `numeric_range_clauses`. Added `numeric_range_uses_only_declared_long_value_field`, `numeric_range_uses_declared_half_float_value_field`, and `numeric_range_unknown_attribute_returns_error`.
- `crates/genomehubs-api/src/report/arc.rs`: `run_arc_report`, `run_rings_report` and `run_per_rank_report` take `cache: &CacheRef`; `count_matching` surfaces ES errors.
- `crates/genomehubs-api/src/report/report_types.rs`: tree status filter passes `&state.cache`.
- `crates/genomehubs-api/src/routes/report.rs`: arc route passes `&state.cache`.
- `crates/genomehubs-api/src/routes/chain_executor.rs`: `execute_batch` takes `&AppState`; sub-query filter passes `&state.cache`; removed the unused `reqwest::Client` import.

Verification:

- `cargo fmt --all` applied.
- `cargo check -p genomehubs-api --all-targets` is clean.
- `cargo test -p genomehubs-api` passes (145 unit tests, plus integration tests).
- `cargo clippy -p genomehubs-api --all-targets -- -D warnings` is clean.
- After restarting the API, `assembly_span>20000000` returns `feature_count` 1009 (previously 0). `genome_size>300000000` returns 1155.

## Notes / warnings

- Restart the API after changes to pick up the filter fix.
- The `half_float` risk also exists in `crates/genomehubs-api/src/report/positional/feature_query.rs`, which builds a numeric `should` over `long`, `3dp_value` and `half_float`. Positional remains parked until a v2 feature index exists.
- The arc centre label (`arc_centre_labels` in `crates/genomehubs-query/src/report/mod.rs`) still shows `0 / N` when the feature is absent because it ignores context. Not addressed in this session.
- Silent zero defaults elsewhere in the report code may still hide ES errors.
