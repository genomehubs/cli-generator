# 2026-05-15_002 — Documentation audit, consistency fixes, notice banner

## Summary

Performed a full audit of the generated Quarto documentation templates and made
targeted fixes for duplicate content, missing coverage, and a new configurable
landing-page notice.

## Changes

### `src/core/config.rs`

- Added `pub notice_text: Option<String>` field to `SiteConfig`.
  Serde-defaults to `None`; when set in a site YAML it renders as a
  `:::{.callout-note}` block on the docs landing page.

### `src/commands/new.rs` — `create_quarto_docs()`

- Injected `notice_text` (`Option<&str>`) into the Tera context.
- Computed `has_feature_index: bool` (true when any `IndexDef` has
  `name == "feature"`); injected into context to gate positional docs.

### `templates/docs/index.qmd.tera`

- Added `{% if notice_text %}:::{.callout-note}{% endif %}` block
  immediately after the YAML frontmatter paragraph.

### `templates/docs/reference/query-builder.qmd.tera`

- **Bug fix**: the entire _Positional reports_ section was duplicated verbatim
  (≈85 lines × 2). Removed the second copy.
- Wrapped the surviving section with `{% if has_feature_index %}…{% endif %}`
  so it only appears when the site actually has a `feature` index.

### `templates/docs/reference/parse.qmd.tera`

- Added three missing parse-utility sections: `values_only`, `annotated_values`,
  and `to_tidy_records` — each with Python / R / JavaScript tabsets.
- Updated the "Typical pipeline" diagram to include all three new functions.

### `sites/goat.yaml` and `sites/goat-test.yaml`

- Added `notice_text: "This is a **v3 API preview**. Endpoints, field names, and
response formats may change before the stable release."`.

## Audit findings (not yet fixed)

The following gaps were identified but are out of scope for this session:

| Gap                                                          | Location                                | Notes                                                                                                                                                                                                                                                              |
| ------------------------------------------------------------ | --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `R::ReportBuilder` has 6 extra methods not in Python         | `templates/r/query.R.tera`              | `add_ring`, `set_arc_ranks`, `set_axis_boundaries`, `set_axis_date_intervals`, `set_display`, `set_include_plot_spec` — these should be ported to Python                                                                                                           |
| JS `QueryBuilder` missing ~20 methods                        | `templates/js/query.browser.js.tera`    | `chainQuery`, `setLineageRankSummary`, `setIdSet`, `setIdType`, `report`, `searchBatch`, `countBatch`, `record`, `recordBatch`, `positional/*`, `lookup/*`, `phylopic/*`, `metadata/indices/fields`, `summary`, `fromV2Url`, `toV2Url`, `toUiUrl`, `toFlatRecords` |
| `cli.qmd.tera` does not document `positional` CLI subcommand | `templates/docs/reference/cli.qmd.tera` | Should be gated with `{% if has_feature_index %}`                                                                                                                                                                                                                  |
| `describe` missing from docs' ReportBuilder section          | `query-builder.qmd.tera`                | Python `ReportBuilder` has `describe()` but it is not in the reference page                                                                                                                                                                                        |

## Verification

- `cargo build --workspace` — zero errors
- `bash scripts/dev_site.sh --no-rebuild-wasm --python goat-test` — EXIT 0
- Generated `docs/index.qmd` contains `:::{.callout-note}` block with v3 preview text
- `docs/reference/query-builder.qmd` — positional section absent (goat-test has no feature index) ✓
- `docs/reference/parse.qmd` — all three new functions rendered ✓
- `pytest tests/python/ -q` — 553 passed, 20 skipped
