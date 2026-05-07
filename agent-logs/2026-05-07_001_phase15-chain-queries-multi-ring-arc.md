# Agent Log — Phase 15: Chain Queries & Multi-Ring Arc

**Date:** 2026-05-07
**Session:** 001
**Agent:** GitHub Copilot (Claude Sonnet 4.6)

---

## Summary

Implemented Phase 15 of the genomehubs v3 API, covering:

1. **Gap 1 — `chainQueries` pre-processor**: `queryA.field` substitution in
   main queries, backed by a WASM-safe pure-Rust type layer in
   `genomehubs-query` and an HTTP execution layer in `genomehubs-api`.

2. **Gap 2 — Multi-ring arc**: Extended the Phase 7.3 `arc` report with an
   optional `rings` array. All count queries for N rings are batched into a
   single `_msearch` request.

---

## Changes

### New files

| File                                                 | Purpose                                                                                                  |
| ---------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `crates/genomehubs-query/src/query/chain.rs`         | `NamedQuerySpec`, `ChainRef`, `ChainError`, `collect_chain_refs`, `resolve_chain_refs` — pure, WASM-safe |
| `crates/genomehubs-api/src/routes/chain_executor.rs` | `execute_named_queries` — HTTP execution of named sub-queries via `_msearch`, batched by index           |

### Modified files

| File                                          | Change                                                                                                                      |
| --------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `crates/genomehubs-query/src/query/mod.rs`    | Added `pub mod chain;` and `named_queries: Option<HashMap<String, NamedQuerySpec>>` field to `SearchQuery`                  |
| `crates/genomehubs-query/src/query/url.rs`    | Added `named_queries: None` to all `SearchQuery` struct literals in test code                                               |
| `crates/genomehubs-api/src/report/arc.rs`     | Added `RingSpec`, `rings: Option<Vec<RingSpec>>` to `ArcConfig`; `run_rings_report` with `_msearch` batch; 2 new unit tests |
| `crates/genomehubs-api/src/routes/mod.rs`     | Added `pub mod chain_executor;`                                                                                             |
| `crates/genomehubs-api/src/routes/report.rs`  | Added chain substitution pre-processing before report dispatch                                                              |
| `src/core/query/adapter.rs`                   | Added `named_queries: None` to `SearchQuery` initializer                                                                    |
| `docs/planning/phases/phase-7-arc-reports.md` | Updated section 3 to use `feature`/`reference`/`context` naming (fix from previous session)                                 |

---

## Design decisions

### Pure/IO split

`chain.rs` in `genomehubs-query` has no async or network dependencies — it
stays WASM-safe. `chain_executor.rs` in `genomehubs-api` holds all HTTP
execution logic. The boundary: `execute_named_queries` returns
`HashMap<String, Vec<String>>`, which is passed to `resolve_chain_refs`.

### `HashMap` instead of `IndexMap`

The spec doc called for `IndexMap` (deterministic YAML serialisation order).
`indexmap` is not currently a dependency of `genomehubs-query`, so
`HashMap<String, NamedQuerySpec>` is used for `named_queries` — the named
queries are not serialised in user-facing output, so insertion order does not
matter in practice. Can be changed to `IndexMap` when `indexmap` is added as
a dependency.

### `NamedQuerySpec::filter_expr` vs `Vec<Attribute>`

The spec doc proposed `filters: Vec<Attribute>`. Instead, `filter_expr:
String` is used — the same compact syntax already supported by
`filter_expr_to_es_query` in `genomehubs-api`. This avoids exposing the full
`Attribute` type in the YAML format for sub-queries and keeps the YAML concise.
The API layer converts `filter_expr` → ES query via the existing parser.

### v2 legacy string compatibility

`NamedQuerySpec::from_legacy_string("assembly--assembly_span>1e9")` is
implemented and parses the v2 `index--filter` format at the URL-params
boundary. The route wiring for URL params (`queryA=...`) is not yet hooked up
(that requires extending the URL params deserialiser in `search.rs`), but the
conversion function is ready.

### Multi-ring `_msearch` batching

`run_rings_report` builds one query per ring (feature AND ring_reference) plus
one query for the outer reference, submits them as a single `_msearch` body,
and zips results back. This matches the Phase 5 `AggBuilder` optimisation
principle. The `_count` API is not used for rings (no `size=0` shortcut) —
instead `_msearch` with `size: 0` serves the same purpose.

---

## Test results

```
test result: ok. 256 passed; 0 failed
  genomehubs-query: 216 unit tests (url, mod, chain) + 7 integration
  genomehubs-api:   25 unit tests (arc ×10, filter_expr ×14, other)
  search_builder:    5 integration tests
```

New tests added:

- `chain.rs`: `chain_ref_parses_plain_field`, `chain_ref_parses_summary_field`,
  `chain_ref_rejects_plain_field_name`, `chain_ref_rejects_uppercase_key`,
  `chain_ref_rejects_empty_key`, `chain_ref_rejects_empty_field`,
  `named_query_spec_parses_cross_index`, `named_query_spec_parses_same_index`,
  `named_query_spec_rejects_unknown_index`, `named_query_spec_taxon_index`,
  `collect_finds_chain_refs`, `resolve_substitutes_values`,
  `resolve_errors_on_undefined_key`, `resolve_errors_on_too_many_hits` (14 tests)
- `arc.rs`: `arc_config_parses_rings`, `arc_config_rings_can_override_reference`
  (2 tests)

---

## Smoke tests

Single arc (unchanged):

```json
{
  "arc": 0.127,
  "arc2": 18.0,
  "feature_count": 16,
  "reference_count": 126,
  "context_count": 7
}
```

Multi-ring arc (Canidae, 2 rings via `_msearch`):

```json
{
  "arc": [
    {
      "ring": 0,
      "label": ">1Gb",
      "arc": 1.0,
      "feature_count": 126,
      "reference_count": 126
    },
    {
      "ring": 1,
      "label": ">3Gb",
      "arc": 0.127,
      "feature_count": 16,
      "reference_count": 126
    }
  ],
  "referenceTerm": "genome_size>0",
  "reference_count": 126
}
```

---

## Pending (not implemented)

- URL-param `queryA=assembly--filter` → `named_queries` conversion in route
  handler prologue (function ready, wiring deferred)
- Chain substitution in `search.rs` (report route done; search route deferred)
- SDK/CLI integration (`arc()` with `rings` param) — Phase 6b
- `terms` lookup workaround for sub-queries > 10,000 results — deferred
- Query-defined categories — Phase 16

---

## Phase ordering (updated)

```
6a ✅ → 7.1 ✅ → 7.2 ✅ → 7.3 ✅ → 15 ✅ → 6b → 16
```
