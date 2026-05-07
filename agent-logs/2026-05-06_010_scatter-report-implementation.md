---
date: 2026-05-06
agent: GitHub Copilot
model: claude-sonnet-4-6
task: Implement scatter report with 2D binning, category breakdowns, and raw point data
files_changed:
  - crates/genomehubs-api/src/report/agg.rs
  - crates/genomehubs-api/src/report/report_types.rs
  - crates/genomehubs-api/src/routes/report.rs
  - examples/report/scatter-categorized.json
---

## Summary

Completed implementation of the scatter report type, continuing from the previous session
where histogram was fixed to use the v2-pattern `filters` aggregation (no fake placeholder
category codes).

## Changes

### agg.rs

- Added `build_y_histogram_sub_agg()`: builds the `yHistograms` sub-aggregation for 2D
  binning — `reverse_nested → nested(attributes) → filter(y_field) → histogram`.
- Added `build_nested_attribute_scatter_agg()`: full scatter aggregation mirroring the v2
  API structure. Produces x-histogram with nested y-histograms per x-bucket, plus
  `categoryHistograms` (v2 `filters` pattern) each with their own nested y-histograms.

### report_types.rs

- Added `find_attr_numeric()`: finds first numeric attribute value from `_source.attributes`
  trying long/integer/float/double_value in order.
- Added `find_attr_keyword()`: finds keyword attribute value from `_source.attributes`.
- Added `extract_scatter_by_cat()`: extracts per-category per-x-bucket counts and
  per-x-bucket y-counts from scatter ES response. Falls back to subtracting named category
  sums from main counts to compute "other" if ES doesn't return it directly.
- Added `compute_z_domain()`: computes `[min_nonzero, max]` z-domain across all y-bucket
  counts (for colour scale in 2D heatmap view).
- Added `fetch_raw_point_data()`: issues a separate ES `_source` search returning
  `{cat: [{scientific_name, taxonId, x, y, cat}]}` when total hits ≤ scatter_threshold.
- Added `run_scatter_report()`: main scatter handler. Computes x/y bounds, builds scatter
  agg, extracts `allValues`, `allYValues` (2D), `by_cat`, `yValuesByCat`, `zDomain`, and
  `rawData`. Returns structured JSON matching v2 API output.
- Fixed `run_histogram_report()`: now includes `allValues` (flat count array parallel to
  buckets) and `tickCount` in the `x` axis info.

### routes/report.rs

- Split `"histogram" | "scatter"` dispatch arm into separate arms so `"scatter"` calls
  `run_scatter_report` instead of `run_histogram_report`.

### examples/report/scatter-categorized.json

- Updated `cat_opts` from `";;1+"` to `";;2+"` so tests cover both Chromosome and Scaffold
  categories plus other.

## Verification

Live test against Mammalia species data:

```json
{
  "type": "scatter",
  "allValues": [23, 0, 0, 0, 0, 0, 0, 24, 70, 15, 1],
  "cats": ["Chromosome", "Scaffold"],
  "by_cat_keys": ["Chromosome", "Scaffold", "other"],
  "has_rawData": true,
  "has_allYValues": true,
  "yBuckets_len": 11,
  "zDomain": [1, 66]
}
```

Build: clean (`cargo build -p genomehubs-api` — no errors).

## Key design decisions

- Categories use `filters` aggregation with named buckets (not `terms`), matching v2 API.
  This guarantees no fake placeholder category codes in output.
- y-histogram uses `reverse_nested` to escape the x-bucket nested context before
  re-entering `attributes` for the y-field filter.
- Raw point data fetched via a separate ES `_source` search only when total hits are within
  `scatter_threshold` (default 1000; configurable in report config).
- "other" category in `by_cat` falls back to subtracting named category sums from main
  `allValues` per x-bucket if ES does not return it directly.
