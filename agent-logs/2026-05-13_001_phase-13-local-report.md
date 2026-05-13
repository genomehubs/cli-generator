# 2026-05-13_001 — Phase 13: Hybrid Local + Remote Reports

## Summary

Implemented Phase 13: local report generation from TSV/CSV files without an API call,
plus `merge_annotations()` helper for augmenting remote reports. Also fixed the residual
Phase 12 gap where `parse_plot_spec_json` was missing from R and generated-project templates.

## Changes

### Rust — `crates/genomehubs-query`

- **Moved `resolve_axis_display`** from `genomehubs-api/src/report/spec_builder.rs` into the
  new `crates/genomehubs-query/src/report/spec_builder.rs` so WASM-compatible code can call
  it. `genomehubs-api` now imports from the query crate.
- **Created `crates/genomehubs-query/src/local_report/`**:
  - `tsv.rs`: `detect_delimiter(path)` (auto-detect from extension) + `read_delimited_with_headers()`
    / `read_delimited()` (parse TSV/CSV with type auto-detection).
  - `builder.rs`: `local_plot_spec()` (typed) + `local_plot_spec_json()` (WASM/PyO3-facing JSON).
  - `mod.rs`: re-exports.
- Added `thiserror = "1"` to `crates/genomehubs-query/Cargo.toml`.
- Declared `pub mod local_report` in `crates/genomehubs-query/src/lib.rs`.

### Rust — `src/` (cli-generator)

- **`src/lib.rs`**: Added `local_plot_spec_json` PyO3 `#[pyfunction]` and registered it in
  the `#[pymodule]`.
- **`src/main.rs`**: Added `local-report` subcommand (`LocalReport` variant) with flags
  `--input`, `--report`, `--x`, `--y`, `--display`, `--delimiter`, `--output`.
- **`src/commands/local_report.rs`**: New handler that reads a file (or stdin), calls
  `local_plot_spec_json`, checks for errors, and writes JSON to stdout or a file.
- **`src/commands/mod.rs`**: Declared `pub mod local_report`.
- **`src/commands/new.rs`**: Added `local_report/` to both `copy_embedded_modules()` (PyO3
  generated projects) and `copy_r_embedded_modules()` (R generated projects); added
  `local_plot_spec_json` and `parse_plot_spec_json` to `patch_python_init()` imports/`__all__`;
  updated both `core/mod.rs` string constants to include `pub mod local_report`.

### Templates

- **`templates/rust/lib.rs.tera`**: Added `parse_plot_spec_json` and `local_plot_spec_json`
  functions and registered both in the `#[pymodule]` block.
- **`templates/python/query.py.tera`**: Added `plot_spec_to_vega_lite()`, `local_plot_spec()`,
  and `merge_annotations()` functions (mirrors `python/cli_generator/query.py`).
- **`templates/js/query.js`**: Added `local_plot_spec_json` to the WASM module destructuring;
  added `localPlotSpec()` and `mergeAnnotations()` functions; added both to the `export {}` block.
- **`templates/r/lib.rs.tera`**: Added `parse_plot_spec_json` and `local_plot_spec_json`
  extendr functions and registered both in `extendr_module!`.
- **`templates/r/extendr-wrappers.R.tera`**: Added R wrappers for `parse_plot_spec_json` and
  `local_plot_spec_json`.
- **`templates/r/query.R`**: Added `local_plot_spec()` and `merge_annotations()` as standalone
  R functions after the `ReportBuilder` class definition.

### Python SDK

- **`python/cli_generator/query.py`**: Added `local_plot_spec()` and `merge_annotations()`.
- **`python/cli_generator/__init__.py`**: Added `local_plot_spec_json` to the Rust extension
  import; added `local_plot_spec`, `local_plot_spec_json`, and `merge_annotations` to the
  `from .query import ...` line and to `__all__`.
- **`python/cli_generator/cli_generator.pyi`**: Added docstring stub for `local_plot_spec_json`.

## Test Results

- `cargo test --workspace --lib`: **307 passed, 0 failed**
- `pytest tests/python/`: **483 passed, 12 skipped (all live-API), 0 failed**

## Decisions

- `bar` maps to `PlotReportType::CountPerRank` — the parse string `"bar"` is not added as a
  new variant; the plan's "Supported report types" table is honoured by accepting
  `"count_per_rank"` via the existing `parse()` method. A `"bar"` alias could be added later
  if needed.
- `local_plot_spec_json` auto-detects `","` delimiter from `delimiter_str == ","` only; all
  other values (including `""`) default to `'\t'`. Callers are expected to call
  `detect_delimiter()` on the file path before invoking the JSON entry point.
- `merge_annotations()` is implemented in pure Python, JS, and R — no Rust binding needed.
- `plot_spec_to_vega_lite` was already in `python/cli_generator/query.py` (Phase 12) but was
  missing from the Python Tera template — added in this session.
- `parse_plot_spec_json` was missing from R templates (Phase 12 gap) — added alongside the
  new Phase 13 function.
