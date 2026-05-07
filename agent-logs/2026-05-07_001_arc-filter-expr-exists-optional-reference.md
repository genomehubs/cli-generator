# 2026-05-07_001 — Arc filter-expr Exists variant and optional reference

## Summary

Extended the V3 arc report to accept bare field names (V2 semantics) and an
optional `reference` parameter, and added three Rust unit tests for the new
`Exists` `FilterTerm` variant.

## Changes

### `crates/genomehubs-api/src/report/filter_expr.rs`

- Added `Exists { field: String }` variant to `FilterTerm` enum.
- `parse_simple_term()` now recognises bare identifiers (alphanumeric + `_` + `-`)
  and returns `FilterTerm::Exists`.
- `term_to_nested_query()` maps `Exists` → `nested.query.term["attributes.key"]`
  (equivalent to V2's `excludeMissing` auto-filter).
- Added 3 unit tests: `test_parse_bare_field_name`, `test_bare_field_nested_query`,
  `test_bare_field_with_hyphen`. Total filter_expr tests: 17.

### `crates/genomehubs-api/src/report/arc.rs`

- `reference` config key changed from required (`ok_or(...)`) to optional
  (`unwrap_or("")`). Empty reference resolves to the base query (all taxa at
  rank), matching V2 default where `y = undefined`.

### `tests/parity/translate.py`

- Arc translation: `x` → `feature`, `y` → `reference`, `z` → `context`.
- `rank` placed in `report_yaml` as `ranks: [...]` list (not in `query_yaml`).
- `opts_map` keys corrected to `x_opts`/`y_opts`/`z_opts`.

### `scripts/collect_v3_responses.py`

- Arc request updated to use `feature: assembly_span\nranks: [phylum, class,
order, family]\n` (bare field, no reference, per-rank).

## Motivation

V2 arc calls `queryParams({term: undefined})` for y by default (all taxa at
rank), and accepts bare field names with auto `excludeMissing`. V3 previously
required an explicit `reference` filter expression and rejected bare identifiers,
breaking translation of any V2 arc URL that omitted `y`.

## Test results

```
cargo test report::filter_expr   → 17 passed
pytest tests/parity/             → 55 passed
```
