# 2026-05-08_001 Phase 6b — Rust/CLI parse functions and v3 template migration

## Summary

Implemented the Rust-first portion of the phase-6b v3 API migration:
new parse functions for report types, full cross-language exposure, and
migration of the generated CLI templates to v3 POST endpoints.

## Changes

### `crates/genomehubs-query/src/parse.rs`

- Added `parse_histogram_json(raw: &str) -> Result<String, String>` — extracts
  `report.buckets` from a `/report` response as a compact JSON array.
- Added `parse_tree_json(raw: &str) -> Result<String, String>` — flattens
  `report.treeNodes` into a sorted JSON array with one object per node.
- Added 9 unit tests covering happy paths, missing fields, and
  `descendant_count` presence.

### `crates/genomehubs-query/src/lib.rs`

- Exposed `parse_histogram_json` and `parse_tree_json` as `#[wasm_bindgen]`
  functions (WASM target).

### `src/lib.rs`

- Exposed `parse_histogram_json` and `parse_tree_json` as `#[pyfunction]`
  wrappers and registered them in `#[pymodule]`.

### `python/cli_generator/cli_generator.pyi`

- Added stub signatures with full docstrings for `parse_histogram_json` and
  `parse_tree_json`.

### `python/cli_generator/__init__.py`

- Added `parse_histogram_json` and `parse_tree_json` to imports and `__all__`.

### `templates/rust/lib.rs.tera`

- Added `parse_histogram_json` and `parse_tree_json` as `#[pyfunction]` wrappers.
- Registered both in the generated `#[pymodule]` block.

### `templates/r/lib.rs.tera`

- Added `#[extendr]` wrappers for `parse_histogram_json` and `parse_tree_json`.
- Registered both in `extendr_module!`.

### `templates/r/extendr-wrappers.R.tera`

- Added R wrapper stubs for `parse_histogram_json` and `parse_tree_json`.

### `templates/js/query.js`

- Added WASM imports for `parse_histogram_json` and `parse_tree_json`.
- Added `parseHistogramJson()` and `parseTreeJson()` JS wrapper functions.
- Added both to the named export block.

### `src/commands/new.rs`

- Added `parse_histogram_json` and `parse_tree_json` to the generated
  `__init__.py` import list and `__all__` in `patch_python_init()`.

### `templates/rust/client.rs.tera`

- Added `API_BASE_URL`, `API_VERSION` constants (re-exported from `cli_meta`).
- Added `is_v3() -> bool` helper.
- Added `post_json()` shared POST helper.
- Added `build_search_post_body()` — builds the v3 JSON POST body for search/count.
- `count()` now dispatches to `count_v3()` (POST `/v3/count`) or `count_v2()`
  (GET, legacy) based on `API_VERSION`.
- `search()` now uses `search_v3()` path (POST `/v3/search`, returns flat JSON
  records from `parse_search_json`) when `API_VERSION == "v3"`.
- `search_all()` dispatches to `search_all_v3()` (cursor-based via repeated
  POST `/v3/search`) or `search_all_v2()` (GET `/searchPaginated`).
- Added `ReportOptions` struct for all report configuration fields.
- Added `report()` function — POST to `/v3/report` with query + report config body.

### `templates/rust/output.rs.tera`

- Added `print_records(records_json, format)` — converts flat JSON records
  (v3 search output) to TSV, JSON, or CSV with auto-generated column headers.
- Added `records_to_tsv()`, `records_to_csv()`, `collect_headers()`,
  `json_value_to_str()` private helpers.

### `templates/rust/main.rs.tera`

- Expanded `Count` subcommand to accept `--taxon`, `--taxon-filter`, `--filter`,
  `--rank`, `--query`, `--taxonomy`, `--include-estimates`, `--took`, and field
  group flags (matching the capability of `Search`).
- Added `Report` subcommand with `--report-type`, `--taxon`, `--x`, `--y`,
  `--cat`, `--cat-rank`, `--count-rank`, `--collapse-monotypic`, `--rank`,
  `--query`, `--filter`, `--taxonomy`.
- v3 search path uses `print_records()` instead of `print_output()` to handle
  JSON-formatted v3 responses correctly.

## Tests

- 6 new unit tests in `parse::report_parse_tests` — all pass.
- 222 total `genomehubs-query` library tests pass.
- Clean `cargo build --workspace` with no errors.

## Remaining (phase 6b)

- Python SDK `query.py` migration: `count()` → v3 POST, `search()` → v3 POST,
  `search_all()` → cursor pagination, `report()` method, `_post_json()` helper,
  `to_v2_url()` rename, `from_v2_url()` classmethod, default version → `"v3"`.
