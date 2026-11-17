---
date: 2026-11-17
agent: GitHub Copilot
model: Claude Haiku 5.5
task: Fix accession prefix matching, validate names inside nested multi-queries, and confirm direct modifier handling against live data
files_changed:
  - crates/genomehubs-query/src/validation.rs
  - crates/genomehubs-api/src/query_validation.rs
---

## Task summary

The user approved two fixes to query validation: correct accession prefix
matching (for example `GCA_000001405.29` and `SAMEA123`), and extend name
validation so that nested `queries` entries are checked against their own
index. The user also asked for a live check of `aggregation_source` before
accepting `direct` on taxon fields with no traverse direction. The accession
and nested fixes are implemented with tests. The live check is complete and
confirmed that `direct` should be accepted on taxon fields with a null
traverse direction.

## Key decisions

- **Decision:** Match accession prefixes with a case-insensitive `starts_with`
  against the configured prefixes (`has_accession_prefix`).
  **Alternative considered:** Keep splitting on `_`. Rejected because it turned
  `GCA_000001405.29` into `gca` and `SAMEA123` into `samea123`, so neither
  matched.
- **Decision:** Walk `queries` recursively and check each node against its own
  `index` (`collect_scopes`, `scoped_names_error`).
  **Alternative considered:** Validate only top-level `attributes` and `fields`.
  Rejected because nested queries bypassed name checks.
- **Decision:** Keep `direct` accepted on taxon fields whose traverse direction
  is null. Status modifiers return before the inheritance check.
  **Alternative considered:** Reject `direct` when direction is unknown. Rejected
  because the live data contains `direct` on these fields.

## Interaction log

| Turn | Role  | Summary                                                                                   |
| ---- | ----- | ----------------------------------------------------------------------------------------- |
| 1    | User  | Approved accession fix and recursive nested-query name validation                         |
| 2    | User  | Required `direct` on taxon with null direction; asked for live `aggregation_source` check |
| 3    | Agent | Queried Elasticsearch; found `aggregation_source` on the taxon index                      |
| 4    | Agent | Implemented accession fix and nested validation with tests                                |
| 5    | Agent | Ran fmt, clippy (`-D warnings`), and tests; all passing                                   |
| 6    | User  | Requested this agent log once verification passed                                         |

## Changes made

- [crates/genomehubs-query/src/validation.rs](crates/genomehubs-query/src/validation.rs):
  added `has_accession_prefix` and used it in the assembly and sample loops, so
  matching is case-insensitive and no longer splits on `_`. Added four accession
  tests and `test_direct_modifier_accepted_when_direction_unknown`.
- [crates/genomehubs-api/src/query_validation.rs](crates/genomehubs-api/src/query_validation.rs):
  `validate_names` now calls `scoped_names_error`, which uses `collect_scopes`
  to walk nested `queries` and resolve each node's index with `item_index_key`.
  Added three tests, including `nested_query_is_checked_against_its_own_index`
  and `deeply_nested_query_is_checked_recursively`.

Verification: `cargo fmt --all` clean; `cargo clippy -p genomehubs-query
-p genomehubs-api --all-targets -- -D warnings` clean; `genomehubs-query` 376
tests passed; `genomehubs-api` 111 tests passed (17 in `query_validation`).

## Notes / warnings

- **Live data findings:** `aggregation_source` is not on attribute metadata
  documents in `attributes--ncbi--goat--2026.11.16`. It is on the taxon index at
  `attributes.aggregation_source`, with values `ancestor` (20,476,681), `direct`
  (2,270,857) and `descendant` (729,574).
- Across 22 taxon keys with null traverse direction in metadata, `direct` is
  present in the data, so accepting `direct` there matches the data.
- **Open mismatch:** `mitochondrion_gc_percent` and `plastid_gc_percent` have a
  null traverse direction in metadata but carry `ancestor` and `descendant`
  values in the data. The validator still rejects those modifiers on them.
  Decide whether to fix the metadata or relax the rule.
- **Open gap:** `NamedQuerySpec` in
  [crates/genomehubs-query/src/query/chain.rs](crates/genomehubs-query/src/query/chain.rs)
  has only `index`, `filter_expr`, `inherit_scope` and `limit`. It has no
  `attributes` or `fields`, so there are no name-bearing keys in `named_queries`
  to validate. Confirm whether this is acceptable.
- The live API smoke test (POST a nested bogus name and a `direct` field on a
  null-direction taxon field) was not run in this session.
- The module doc in `query_validation.rs` still says identifier and name-class
  checks are excluded. Update it if that scope has changed.
