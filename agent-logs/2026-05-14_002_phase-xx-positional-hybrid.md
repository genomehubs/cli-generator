# 2026-05-14_002 Phase XX: Positional Hybrid Reports

## Summary

Implemented Phase XX: Positional Hybrid — combining remote API positional reports with
locally-supplied BUSCO/FAI/lengths data. Adds all-local (`positional_from_features`) and
hybrid (`hybrid_positional`) paths, plus the three file-parser helpers.

## Changes

### New files

| File | Purpose |
|------|---------|
| `crates/genomehubs-query/src/parse_local/mod.rs` | Re-export of parse_local types |
| `crates/genomehubs-query/src/parse_local/feature_set.rs` | `LocalFeature` + `LocalFeatureSet` |
| `crates/genomehubs-query/src/parse_local/busco.rs` | BUSCO `full_table.tsv` parser (v4/v5) |
| `crates/genomehubs-query/src/parse_local/fai.rs` | `.fai` index parser |
| `crates/genomehubs-query/src/parse_local/lengths.rs` | Two-column lengths TSV parser |
| `crates/genomehubs-query/src/report/layout.rs` | Layout algorithms moved from API crate |
| `crates/genomehubs-query/src/report/hybrid.rs` | `positional_from_features` + `hybrid_positional` |

### Modified files

| File | Change |
|------|--------|
| `crates/genomehubs-api/src/report/positional/layout.rs` | Replaced with 7-line re-export |
| `crates/genomehubs-query/src/report/mod.rs` | Added `pub mod hybrid; pub mod layout;` |
| `crates/genomehubs-query/src/lib.rs` | Added `parse_local` module + 5 WASM exports |
| `src/lib.rs` | Added 5 PyO3 functions registered in pymodule |
| `python/cli_generator/cli_generator.pyi` | Added 5 function stubs |
| `python/cli_generator/__init__.py` | Added 5 imports + `__all__` entries |
| `python/cli_generator/query.py` | Added `hybrid_positional()` method |
| `templates/python/query.py.tera` | Added `hybrid_positional()` method |
| `templates/js/query.js` | Added `hybridPositional()` method |
| `templates/r/query.R` | Added `hybrid_positional()` method |
| `templates/r/extendr-wrappers.R.tera` | Added 5 R wrapper stubs |
| `templates/rust/lib.rs.tera` | Added 5 PyO3 functions + pymodule registrations |
| `src/commands/new.rs` | Added `parse_local/` copy in both embedded module functions; updated `core/mod.rs` and `__init__.py` templates |

## Architecture

```
positional_from_features_json()     hybrid_positional_json()
         |                                    |
         v                                    v
positional_from_features()          hybrid_positional()
         |                                    |
    build_local_layouts()           parse remote JSON report
         |                          build_local_layouts()
    dispatch oxford/painting        merge remote reference layout
                                    build new points/connections
```

All logic lives in `genomehubs-query` (WASM-compatible, no HTTP deps).
`genomehubs-api` layout code was moved here; the API crate re-exports it.

## Test results

- `genomehubs-query` — 343 tests pass (8 new in hybrid.rs, 15 in parse_local)
- `genomehubs-api` — 5 tests pass
- `cargo clippy --workspace -- -D warnings` — clean
- Integration tests that call live API skipped (network: API 404)
