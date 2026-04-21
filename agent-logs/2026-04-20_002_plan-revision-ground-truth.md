# Agent Log: Plan Revision — Ground-Truthed Against API/UI/goat-ui

**Date:** 2026-04-20
**Sequence:** 002
**Task:** Review api-aggregation-refactoring-plan.md; ground-truth against genomehubs-api, genomehubs-ui, and goat-ui source; add detail, identify gaps, produce integration test suite from goat-ui config; catalog v3 API design improvements including `result`→`entity` rename and `opts` string decomposition

---

## Changes Made

Single file updated: [docs/api-aggregation-refactoring-plan.md](../docs/api-aggregation-refactoring-plan.md)

---

## Source Files Inspected

### genomehubs-api (v2 reports)

- `src/api/v2/reports/setAggs.js` — full function inventory: `attributeTerms`, `attributeCategory`, `nestedHistograms`, `lineageTerms`, `lineageCategory`, `treeAgg`, `termsAgg`
- `src/api/v2/reports/getBounds.js` — two-pass architecture confirmed; nice-tick algorithm uses d3-scale
- `src/api/v2/reports/setScale.js` — opts string parsing (comma/semicolon, 5-element format)
- `src/api/v2/reports/setTerms.js` — `field[N]+`, `val@Label`, `nsort` prefix, async lineage lookup
- `src/api/v2/queries/histogramAgg.js` — `scaleFuncs`, `duration` (date interval), date vs numeric routing
- `src/api/v2/reports/scaleBuckets.js` — bucket key back-conversion
- `src/api/v2/reports/histogram.js` — full stacked/cumulative/2D orchestration
- `src/api/v2/reports/queryParams.js` — field parsing, summary prefix handling
- `src/api/v2/functions/processHits.js` — hit post-processing
- `src/api/v2/reports/{scales,fmt,valueTypes,incrementDate,precision}.js`
- `src/api/v2/routes/report.js` — full route dispatchers including arc, arcPerRank, xPerRank, getTree, scatterPerRank, histPerRank; confirmed `queryA`..`queryJ` pattern; confirmed `result` values used in conditionals
- `src/api/v2/functions/setExclusions.js` — five separate exclusion params
- `src/api/v2/functions/summaries.js` — full summary list
- `src/api/v2/functions/parseFields.js` — field wildcard, `summary(field)` syntax
- `src/api/v2/functions/attrTypes.js` — confirmed `result` → ES index group mapping
- `src/api/v2/functions/indexName.js` — confirmed `result` → index name construction

### genomehubs-ui

- `src/client/views/selectors/report.js` — confirmed per-report-type parameter whitelist; identified UI-only params; full `reportTerms` whitelist extracted; confirmed `queryA`..`queryJ` as pass-through params
- `src/client/views/selectors/report.js:sortReportQuery` — complete parameter routing table extracted

### goat-ui (144 markdown files)

- All 144 static markdown files scanned
- Confirmed `result` values in use: `taxon`, `assembly`, `sample` (and `feature`, `file`, `analysis`, `multi` from API code)

---

## Key Gaps Found in Original Plan

### From first session:

1. Two-pass nature of getBounds was understated
2. opts string format was not specified (module added)
3. Phase 1.0 fast-path was missing
4. UI-only parameters not called out
5. histogram/arc come pre-shaped from API
6. stacked + cumulative histogram complexity
7. Date histogram uses different ES agg type
8. `getCatLabels` is async lineage resolution (deferred to Phase 3)
9. Integration test suite from goat-ui was missing
10. File structure lacked `opts.rs` and `scale.rs`

### From second session (v3 design improvements):

11. `result` parameter clashes with response body keys; renamed to `entity`
12. `opts`/`xOpts`/`yOpts`/`catOpts`/`cat` mini-DSLs replaced by structured objects (`AxisConfig`, `CategoryConfig`)
13. Five separate `exclude*` params collapsed into `exclude` object with `derived` shorthand
14. `queryA`..`queryJ` → `params` map
15. Threshold params given consistent names (`scatter_max_points`, `tree_max_nodes`, `map_max_bins`)
16. `stacked`+`cumulative` booleans → `histogram_mode` enum
17. `summaryValues=count` → `scatter_mode=counts`
18. `xField`/`yField` made explicit requirement for compound `x`/`y` queries
19. v3 response envelope redesigned: `data.*` replaces `report.<type>.*`, ES internals removed
20. `caption` removed as input parameter; `queryId` made server-generated
21. `opts.rs` given dual role: v2 string parser (compat shim) + v3 structured validator
22. Full v2→v3 compat shim responsibilities documented

---

## Decisions Made

- `entity` is the v3 replacement for `result` (unambiguous, doesn't clash with response keys)
- `opts.rs` is the single module responsible for both directions: parsing v2 strings and validating v3 objects
- The compat shim lives at the API level (Phase 3), not in the Rust SDK
- `histogram_mode` enum replaces the two `stacked`/`cumulative` booleans
- v3 response removes `xQuery`/`yQuery` (ES internals); adds `meta.share_url` for link generation
- `derived` shorthand in `ExclusionConfig` covers the common "both ancestral and missing" case
- Integration test fixtures updated to use v3 parameter names (`entity=`, `exclude=` object, etc.)

---

## Verification

Documentation-only update. No Rust or Python source changes. No verification step required.

**Date:** 2026-04-20
**Sequence:** 002
**Task:** Review api-aggregation-refactoring-plan.md; ground-truth against genomehubs-api, genomehubs-ui, and goat-ui source; add detail, identify gaps, produce integration test suite from goat-ui config

---

## Changes Made

Single file updated: [docs/api-aggregation-refactoring-plan.md](../docs/api-aggregation-refactoring-plan.md)

---

## Source Files Inspected

### genomehubs-api (v2 reports)

- `src/api/v2/reports/setAggs.js` — full function inventory: `attributeTerms`, `attributeCategory`, `nestedHistograms`, `lineageTerms`, `lineageCategory`, `treeAgg`, `termsAgg`
- `src/api/v2/reports/getBounds.js` — two-pass architecture confirmed; nice-tick algorithm uses d3-scale
- `src/api/v2/reports/setScale.js` — opts string parsing (comma/semicolon, 5-element format)
- `src/api/v2/reports/setTerms.js` — `field[N]+`, `val@Label`, `nsort` prefix, async lineage lookup
- `src/api/v2/queries/histogramAgg.js` — `scaleFuncs`, `duration` (date interval), date vs numeric routing
- `src/api/v2/reports/scaleBuckets.js` — bucket key back-conversion
- `src/api/v2/reports/histogram.js` — full stacked/cumulative/2D orchestration
- `src/api/v2/reports/queryParams.js` — field parsing, summary prefix handling
- `src/api/v2/functions/processHits.js` — hit post-processing
- `src/api/v2/reports/{scales,fmt,valueTypes,incrementDate,precision}.js`

### genomehubs-ui

- `src/client/views/selectors/report.js` — confirmed per-report-type parameter whitelist; identified UI-only params (`highlightArea`, `treeStyle`, `collapseMonotypic`, `disableModal`, `plotRatio`, `pointSize`, `zScale`, `palette`); confirmed histogram/arc NOT post-processed in `processReport` (come pre-shaped from API); unbounded `Map()` cache identified

### goat-ui (144 markdown files)

- All 144 static markdown files scanned
- Confirmed report types actually used: histogram, scatter, tree, arc, map, table, xPerRank
- Full parameter set extracted: 28 distinct parameter keys documented
- Representative report configurations captured for integration test suite

---

## Key Gaps Found in Original Plan

1. **Two-pass nature of getBounds was understated** — bounds always requires a full ES round-trip before the main aggregation can be built. Explicitly documented in Problem Analysis.

2. **opts string format was not specified** — The 5-element `min,max,tickCount,scale,label` format with dual comma/semicolon delimiter and special notations (`nsort`, `field[N]+`, `val@Label`) is non-trivial and a source of bugs. `opts.rs` added as a dedicated module.

3. **Phase 1.0 fast-path was missing** — Added Week 1-3 milestone: `opts.rs`, `scale.rs`, `bounds.rs`, `histogram.rs` giving interactive usability against real GoAT by Week 3.

4. **UI-only parameters not called out** — 8 parameters are parsed by the UI and never reach the API. Listed explicitly to avoid implementing SDK support for them unnecessarily.

5. **histogram/arc come pre-shaped** — confirmed by reading `processReport` in report.js. `processReport` only branches on scatter, tree, oxford, table. Histogram and arc are already in display format from the API.

6. **stacked + cumulative histogram complexity** — identified as distinct sub-features requiring separate test cases and care in the processor.

7. **Date histogram is a different ES agg type** — `date_histogram` not `histogram`. Separate code path required in `histogram.rs`.

8. **`getCatLabels` is async lineage resolution** — needed for lineage-rank categories (e.g., `class[13]+`). Cannot be done purely in Rust without API round-trip; Phase 1 stubs this, Phase 3 implements via API call.

9. **Integration test suite from goat-ui was missing entirely** — added a full section with 13+ parametrised test cases covering all major histogram/scatter/arc/tree variants actually used in production.

10. **File structure lacked `opts.rs` and `scale.rs`** — added as essential first modules.

---

## Decisions Made

- `opts.rs` is a prerequisite for all other modules; build first
- `scale.rs` is a prerequisite for `bounds.rs` and `histogram.rs`; build second
- `getCatLabels` equivalent is deferred to Phase 3 (requires API); Phase 1 tests use attribute-based categories only
- UI-only parameters documented but no SDK code planned for them
- Integration tests use two modes: unit (fixture-based, no ES) and integration (live ES, gated with `--integration` flag)
- `insta` crate recommended for ES DSL snapshot tests (catches accidental structure changes)

---

## Verification

No code changes to Rust or Python source — documentation-only update. No verification step required.
