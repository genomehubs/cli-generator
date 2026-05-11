# 2026-05-11_001 — Lineage rank summary DRY refactor

## Summary

Audited and refactored `lineage_rank_summary` result flattening across all three SDK
languages (Python, JavaScript, R) to comply with the project's Rust-first / DRY standards.

## Root causes found

### 1. JavaScript DRY violation (`templates/js/query.js`)

The `toFlatRecords()` method had been re-implemented in JavaScript instead of delegating
to the Rust WASM function `_parseSearchWithLineageSummary`. The hand-rolled JS code:

- Hard-coded categorical values (`if (statKey === 'Chromosome' || statKey === 'Scaffold')`)
- Attempted a wrong taxon-ID lookup (querying by the result's own taxon_id instead of
  the ancestor's taxon_id from `result.lineage`)
- Ignored all `SummaryMode` semantics (top, stats, min, max, avg, count, top_n, all)

**Fix**: Replaced the entire ~60-line JS implementation with a 3-line delegate:

```javascript
return JSON.parse(
  _parseSearchWithLineageSummary(responseJson, JSON.stringify(lineageSummary)),
);
```

Without an explicit config the method falls back to `_parseSearchJson`, matching Python.

### 2. R embedded module missing `report/` directory (`src/commands/new.rs`)

`copy_r_embedded_modules()` copied `validation.rs` verbatim (no path rewriting) and
did not copy the `report/` sub-directory. Since `validation.rs` references
`crate::report::ReportType`, the R package Rust compilation failed with E0432.

**Fix**: Applied `crate:: → crate::embedded::core::` rewriting to `validation.rs` in
`copy_r_embedded_modules()`, and added a loop to copy and rewrite the `report/` directory
(mirroring the existing pattern in `copy_embedded_modules()`). Also added `pub mod report;`
to the generated `core/mod.rs` string.

### 3. R template config format (`templates/r/query.R`)

`to_flat_records()` built a config as `{rank: [field1, field2]}` (an array) but
`parse_summary_config` in Rust requires `{rank: {field: "mode"}}` (a JSON object).

**Fix**: Simplified the R method to only call `parse_search_with_lineage_summary` when
the caller provides an explicit `lineage_summary` argument (matching Python), and use
`jsonlite::toJSON(lineage_summary, auto_unbox = TRUE)` directly.

## Files changed

| File                    | Change                                                                                                     |
| ----------------------- | ---------------------------------------------------------------------------------------------------------- |
| `templates/js/query.js` | Replaced `toFlatRecords()` body — delegates to `_parseSearchWithLineageSummary`                            |
| `templates/r/query.R`   | Simplified `to_flat_records()` — explicit config only, correct JSON object format                          |
| `src/commands/new.rs`   | `copy_r_embedded_modules()`: rewrite `validation.rs` paths, copy `report/` directory, add `pub mod report` |

## Verified outcomes

Python and JavaScript now produce byte-identical logical output for the same query:

```
Records: 1, Lineage cols: 5
  genus_assembly_level: Chromosome
  genus_genome_size__avg: 2748180000
  genus_genome_size__count: 1
  genus_genome_size__max: 2748180000
  genus_genome_size__min: 2748180000
```

R package Rust compilation: ✓ (no errors, one pre-existing unused-variable warning).

## Architecture after refactor

All reduction logic lives exclusively in `crates/genomehubs-query/src/lineage_summary.rs`
(Rust). Each SDK language is a thin wiring layer:

- **Python**: calls `parse_search_with_lineage_summary(raw, config_json)` via PyO3
- **JavaScript**: calls `_parseSearchWithLineageSummary(raw, configJson)` via WASM
- **R**: calls `parse_search_with_lineage_summary(raw, config_json)` via extendr
