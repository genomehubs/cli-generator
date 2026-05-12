# Phase 12 – Reusable Plot Specifications

**Date:** 2026-05-12
**Session:** 003
**Status:** Complete

---

## Summary

Implemented Phase 12: a typed `PlotSpec` wire format that renderers can
consume without re-querying the API or re-interpreting raw `DisplaySpec`
hints.  The implementation spans Rust (both crates), WASM/PyO3 exports,
the JS template, and the Python SDK.

---

## Changes

### `crates/genomehubs-query/src/report/plot_spec.rs` (new)
- `PlotReportType` enum (7 variants; `parse()` accepts camelCase and snake_case)
- `AxisMeta` struct — field, label, scale, domain, tick_values/labels, value_type, and the three resolved display fields (`tick_label_placement`, `tick_label_stride`, `tick_label_max_length`)
- `SeriesMeta` struct (key, label, optional colour)
- `PlotSpec` struct (report_type, x/y/z axes, series, display, raw data)
- 3 unit tests

### `crates/genomehubs-query/src/report/mod.rs`
- Added `pub mod plot_spec; pub use plot_spec::PlotSpec;`

### `crates/genomehubs-api/src/report/spec_builder.rs` (new)
- `build_plot_spec(report_type_str, report_data, display) -> PlotSpec` — extracts axis metadata from pipeline JSON, resolves display hints
- `resolve_axis_display(meta, opts)` — single source of truth for placement/stride/max_length resolution rules
- Private helpers: `axis_meta_from_report_data`, `extract_domain`, `infer_value_type`, `extract_series`
- 6 unit tests (histogram extraction, axis display resolution, auto between-ticks for keyword, series extraction, unknown type default)

### `crates/genomehubs-api/src/report/mod.rs`
- Added `pub mod spec_builder;`

### `crates/genomehubs-api/src/routes/report.rs`
- `ReportRequest` — added `include_plot_spec: bool`
- `ReportResponse` — added `plot_spec: Option<PlotSpec>`
- Handler — builds `PlotSpec` when `include_plot_spec` is true or `display` is present; updated all `bail!` call sites to include `plot_spec: None`
- Imports: added `PlotSpec`, `spec_builder`

### `crates/genomehubs-query/src/lib.rs`
- Added `parse_plot_spec_json(raw: &str) -> String` WASM export — extracts the `plot_spec` field from a raw `/report` response

### `src/lib.rs`
- Added `parse_plot_spec_json` PyO3 function (delegates to `genomehubs_query::parse_plot_spec_json`)
- Registered in `#[pymodule]`

### `python/cli_generator/cli_generator.pyi`
- Added `parse_plot_spec_json(raw: str) -> str` stub

### `python/cli_generator/__init__.py`
- Imported `parse_plot_spec_json` and `plot_spec_to_vega_lite`; both added to `__all__`

### `python/cli_generator/query.py`
- `ReportBuilder.__init__` — added `_include_plot_spec: bool = False`
- `ReportBuilder.set_include_plot_spec(value=True)` — new method
- `QueryBuilder.report()` — includes `"include_plot_spec": True` in payload when set; returns full response dict when `plot_spec` is present
- `plot_spec_to_vega_lite(spec)` — module-level function; converts a PlotSpec dict to a Vega-Lite v5 spec for histogram, scatter, countPerRank, and sources report types

### `templates/js/query.js`
- `ReportBuilder` constructor — added `_includePlotSpec = false`
- `ReportBuilder.setIncludePlotSpec(value=true)` — new method
- `QueryBuilder.report()` — passes `include_plot_spec: true` when set; returns full response when `plot_spec` present
- `plotSpecToVegaLite(plotSpec)` — new exported function with private helpers `_vegaConfig`, `_histogramSpec`, `_scatterSpec`, `_barSpec`, `_treeSpec`, `_mapSpec`, `_arcSpec`; handles all 7 report types
- Added `plotSpecToVegaLite` to the module `export {}` block

---

## Test results

```
cargo test -p genomehubs-query -p genomehubs-api
288 passed; 0 failed (genomehubs-query)
69 passed; 0 failed (genomehubs-api)
```

```
cargo clippy -p genomehubs-query -p genomehubs-api --all-targets -- -D warnings
Finished dev profile — no warnings
```

```
pyright python/
0 errors, 0 warnings, 0 informations
```

---

## Design decisions

- **`PlotReportType::parse()` not `from_str()`** — avoids the clippy
  `should-implement-trait` lint; callers that need `FromStr` can add it later.
- **`build_plot_spec` is triggered when `display` is present** — a `display`
  field in the request carries `AxisOptions` hints that are only useful if a
  `PlotSpec` is returned, so we build it implicitly.
- **`tick_values`/`tick_labels` left empty** — these are renderer concerns
  (log tick spacing, label truncation) that require more context than the
  pipeline provides; left as `[]` for renderers to fill.
- **`infer_value_type` is a heuristic** — future work: have pipeline handlers
  write `value_type` directly into report data to remove the guess.
