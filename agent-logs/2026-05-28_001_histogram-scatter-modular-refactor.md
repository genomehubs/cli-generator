---
date: 2026-05-28
agent: GitHub Copilot
model: claude-sonnet-4-6
task: "Refactor histogram and scatter report functions and parsing to be modular, DRY, and cover all edge cases"
files_changed:
  - crates/genomehubs-api/src/report/field.rs
  - crates/genomehubs-api/src/report/mod.rs
  - crates/genomehubs-api/src/report/agg.rs
  - crates/genomehubs-api/src/report/bounds.rs
  - crates/genomehubs-api/src/report/report_types.rs
  - crates/genomehubs-api/src/report/spec_builder.rs
  - crates/genomehubs-query/src/report/mod.rs
---

## Task summary

The user requested a full architectural refactor of the server-side report infrastructure in
`crates/genomehubs-api/src/report/`. The prior code had duplicated field-type helpers spread
across `agg.rs` and `bounds.rs`, a 4-case branch in `build_nested_attribute_histogram_with_categories`
(~200 lines), non-deterministic category histogram extraction paths (4 candidate paths
searched at runtime), and ~200 lines of duplicated tick label/value extraction in
`spec_builder.rs`. Two additional bug fixes preceded this session: presence filters were
not ANDed into histogram bounds queries, and per-category histograms used `{x_field}` as
the inner container name, making extraction non-deterministic.

This session replaced all of the above with a clean, type-agnostic architecture.

## Key decisions

- **New `field.rs` module as single source of truth**: All field-type resolution
  (`is_rank`, `is_attribute`, `get_attribute_value_field`) and all ES nested-path logic
  now live in a `FieldStorage` enum (`Attribute{key, es_value_field}`, `Lineage{rank}`,
  `Root{es_field}`). Methods encode every path decision in one place, eliminating drift
  between builder and extractor code.

- **Canonical container naming enforced in `build_inner_x_agg_block`**: The per-category
  inner x aggregation container is now always `"by_key"` (attribute) or `"at_rank"`
  (lineage), never `{x_field}`. This makes extraction paths `O(1)` pointer dereferences
  instead of a 4-candidate runtime search. The previous `{x_field}` naming was the root
  cause of the "most cats fall in first bin" bug fixed in the prior session.

- **`GenericBucketAgg` replaces 5 typed builder structs**: A single
  `GenericBucketAgg { storage, bucket_type, bucket_params }` implements `AggBuilder` for
  all field types. `build()` delegates wrapping to `wrap_in_nested(storage, …)`.
  `extract()` delegates path resolution to `storage.main_bucket_path(agg_name, bucket_type)`.

- **`build_nested_attribute_histogram_with_categories` reduced from ~200 to ~80 lines**:
  Replaced 4 hand-written `(x_type, cat_type)` cases with type-agnostic composition:
  `build_inner_x_agg_block` + `wrap_cat_in_nested` + `inject_category_histograms`.
  Adding a new field storage type in future requires no changes here.

- **`fill_tick_data_from_buckets` helper in `spec_builder.rs`**: Extracts the identical
  ~60-line pattern (keyword label list OR numeric boundary computation) shared between the
  histogram x-axis and scatter x-axis branches. Y-axis handling reuses the same helper
  with an `explicit_labels` parameter for `yBucketLabels`. Raw scalar `yBuckets` values
  are wrapped into pseudo-bucket objects before dispatch so the helper has a uniform
  interface.

- **`grouped_bar_size_px` helper in `genomehubs-query/src/report/mod.rs`**: Extracted
  the 5-line pixel-width computation duplicated in the keyword and numeric branches of the
  grouped histogram path into a pure helper: `grouped_bar_size_px(n_bins, n_cats, plot_width_px)`.

- **No speculative abstraction**: Only patterns that appeared at least twice in the
  current code were extracted. The VL `vl_scatter` function was not restructured because
  its internal branching differs significantly from `vl_histogram`.

## Interaction log

| Turn | Role  | Summary                                                                           |
| ---- | ----- | --------------------------------------------------------------------------------- |
| 1    | User  | Requested full modular refactor of histogram/scatter report infrastructure        |
| 2    | Agent | Created `field.rs` with `FieldStorage`, `resolve_field_storage`, wrapping helpers |
| 3    | Agent | Registered `pub mod field;` in `mod.rs`; refactored `agg.rs` (GenericBucketAgg)   |
| 4    | Agent | Refactored `bounds.rs` — removed duplicated helpers, imported from `field.rs`     |
| 5    | Agent | Refactored `report_types.rs` — new `extract_cat_histograms`, `align_to_keys`      |
| 6    | Agent | Confirmed clean build (0 errors, 0 warnings)                                      |
| 7    | Agent | Added `bucket_keys_to_boundaries` + `fill_tick_data_from_buckets` to spec_builder |
| 8    | Agent | (New session) Replaced duplicate tick extraction blocks in `build_plot_spec`      |
| 9    | Agent | Extracted `grouped_bar_size_px` from `vl_histogram` numeric/keyword branches      |
| 10   | Agent | Confirmed clean `cargo clippy --all-targets -- -D warnings` across workspace      |

## Changes made

### `crates/genomehubs-api/src/report/field.rs` (new file, ~350 lines)

- `FieldStorage` enum with `nested_path()`, `key_filter()`, `x_container_name()`,
  `cat_wrapper_names()`, `presence_filter()`, `bucket_field()`, `main_bucket_path()`,
  `cat_histograms_base()`, `inner_x_path()`
- `resolve_field_storage(field, value_type, cache)` — prefers `TaxonRank` over `Attribute`
- `is_rank()`, `is_attribute()`, `get_attribute_value_field()` — canonical, previously duplicated
- `wrap_in_nested()`, `wrap_cat_in_nested()`, `build_inner_x_agg_block()` — composition helpers

### `crates/genomehubs-api/src/report/agg.rs`

- Removed: `HistogramAggBuilder`, `DateHistogramAggBuilder`, `TermsAggBuilder`,
  `StatsAggBuilder`, `NestedAttributeAggBuilder`, `NestedRankAggBuilder`,
  `CompositeAggBuilder`, `ReverseNestedAggBuilder`, `GeoHashAggBuilder`
- Added: `GenericBucketAgg` — single `AggBuilder` impl for all field types
- `build_nested_attribute_histogram_with_categories`: 200 lines → 80 lines, fully type-agnostic
- `inject_category_histograms`: uses `x_storage.x_container_name()` for deterministic insertion

### `crates/genomehubs-api/src/report/bounds.rs`

- Removed duplicated `is_rank`, `is_attribute`, `get_attribute_value_field` functions
- Imported canonical versions from `field.rs`

### `crates/genomehubs-api/src/report/report_types.rs`

- Removed `presence_filter_for_axis` — replaced by `FieldStorage::presence_filter()`
- Replaced old 4-candidate-path `extract_cat_histograms` with `FieldStorage`-based deterministic version
- Added `align_to_keys` shared helper for per-category count alignment

### `crates/genomehubs-api/src/report/spec_builder.rs`

- Added `bucket_keys_to_boundaries(sorted_keys, axis_obj)` — `N` keys → `N+1` VL bin boundaries
- Added `fill_tick_data_from_buckets(meta, axis_obj, buckets, label_source)` — unified tick extraction
- Replaced two ~60-line duplicated blocks (histogram x-axis and scatter x-axis) with calls to helper
- Replaced ~80-line y-axis block (scatter) with wrapped call to same helper using `explicit_labels`

### `crates/genomehubs-query/src/report/mod.rs`

- Added `grouped_bar_size_px(n_bins, n_cats, plot_width_px)` — extracted from two identical 5-line computations in `vl_histogram`

## Notes / warnings

- The new `"by_key"` / `"at_rank"` canonical container names are a **breaking change** relative
  to any cached Elasticsearch responses or client-side code that expected `{x_field}` as the
  container name. Any stored ES aggregation responses will be unaffected (they are computed fresh),
  but any client that manually inspects the raw ES response shape should be updated.

- `geohash_precision_for_size` in `agg.rs` has `#[allow(dead_code)]` — it is used by the
  geo report path which is not currently exercised by the test suite.

- The scatter `vl_scatter` function in `genomehubs-query/src/report/mod.rs` still has some
  duplication with `vl_histogram` in the category handling paths. Full extraction was deferred
  because the two functions diverge significantly in their data transformation logic.

- Pending feature (deferred): 3-level nested binning for x/y/cat scatter (x-binned + y-binned +
  category breakdown). The `FieldStorage` composition pattern makes this straightforward to add.
