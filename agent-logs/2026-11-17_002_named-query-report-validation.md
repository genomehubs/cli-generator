---
date: 2026-11-17
agent: GitHub Copilot
model: Claude Haiku 5.5
task: Validate named queries and chain references on the report route
files_changed:
  - crates/genomehubs-query/src/query/chain.rs
  - crates/genomehubs-api/src/es_metadata.rs
  - crates/genomehubs-api/src/query_validation.rs
  - crates/genomehubs-api/src/routes/report.rs
---

## Task summary

The report route is the only route that executes `named_queries`, but it
previously skipped all name validation. This session added validation for
named-query filter names, chain references, and chain summaries, wired it into
the report route, and checked the result against the live API. Metadata was not
changed.

## Key decisions

- **Decision:** Add `chain_ref_errors` to the pure `chain.rs` module. It reports
  undefined chain keys (each once) and unsupported `summary` values, and runs
  without the metadata cache.
  **Alternative considered:** Put the check in the API crate. Rejected because
  the logic is pure and belongs with the other chain code.
- **Decision:** Validate each named query's filter names against that query's
  own index, inheriting the top-level index when `index` is absent.
  **Alternative considered:** Validate against the top-level index only. Rejected
  because named queries may target a different index.
- **Decision:** Strip chain-reference values (`queryA.taxon_id`) from attribute
  value checks.
  **Alternative considered:** Leave them in. Rejected because a chain reference
  is not an attribute value.
- **Decision:** Exempt chain entries on top-level keyword fields of the target
  index from metadata name checks. The field names come from the Elasticsearch
  index mappings, read once at startup into `MetadataCache.keyword_fields`.
  **Reason:** Live testing showed the documented pattern `name: taxon_id` with
  `value: queryA.taxon_id` was falsely rejected as an unknown attribute. Keyword
  fields resolve from identifiers, not cached attributes. Plain attributes, and
  chain references on non-keyword names, are still checked as before.
  **Alternative considered:** A hardcoded list of identity names (`taxon_id`,
  `assembly_id`, `sample_id`). Rejected because each new keyword field would
  need a code change.
  **Alternative considered:** Add identity names to the metadata. Rejected
  because the user asked for no metadata changes.
- **Decision:** Run chain and filter-parse checks even without the cache. Run
  name checks only when the cache is present.

## Interaction log

| Turn | Role  | Summary                                                                                                        |
| ---- | ----- | -------------------------------------------------------------------------------------------------------------- |
| 1    | User  | Approve the four-step named-query validation plan; start with live check                                       |
| 2    | Agent | Confirm live baseline before changes                                                                           |
| 3    | Agent | Implement chain helper, name checks, report wiring, and tests                                                  |
| 4    | Agent | Live check finds `name: taxon_id` false positive; add identity exemption                                       |
| 5    | Agent | Re-run fmt, clippy, and tests; re-run live checks                                                              |
| 6    | Agent | Write this log                                                                                                 |
| 7    | User  | Ask for a cache-based keyword lookup instead of special-casing names                                           |
| 8    | Agent | Read top-level keyword fields from ES mappings into `MetadataCache`; replace the hardcoded list; re-run checks |

## Changes made

- [crates/genomehubs-query/src/query/chain.rs](crates/genomehubs-query/src/query/chain.rs):
  added `ChainError::UnsupportedSummary` and `chain_ref_errors`, with unit tests.
- [crates/genomehubs-api/src/es_metadata.rs](crates/genomehubs-api/src/es_metadata.rs):
  added `MetadataCache.keyword_fields`, read from each index's `_mapping` at
  startup (404 gives an empty list), with `keyword_fields_of` and unit tests.
  Metadata documents are unchanged.
- [crates/genomehubs-api/src/query_validation.rs](crates/genomehubs-api/src/query_validation.rs):
  added `validate_report_query`, named-query name checks per index, chain value
  stripping, and the keyword-field exemption for chain references, with unit
  tests. Module doc updated.
- [crates/genomehubs-api/src/routes/report.rs](crates/genomehubs-api/src/routes/report.rs):
  validation runs after YAML parse and before index resolution and chain
  substitution. Failures return `invalid query: <message>`.

## Verification

- `cargo fmt --all`: clean.
- `cargo clippy -p genomehubs-query -p genomehubs-api --all-targets -- -D warnings`: clean.
- `cargo test -p genomehubs-query -p genomehubs-api --lib --bins`:
  genomehubs-query 381 passed; genomehubs-api 126 passed; 0 failed (final run,
  after the keyword-field cache change).
- Live API (`127.0.0.1:3000`, Elasticsearch on `localhost:9200`):
  - Unknown named filter name is rejected with
    `named query 'queryA': unknown attribute 'not_a_real_field' for index 'assembly'`.
  - Undefined chain key is rejected with
    `chain reference uses undefined query key: "queryZ"`.
  - Documented pattern `name: taxon_id` with `value: queryA.taxon_id` passes
    validation. The request then fails on a report-config error (missing `x`
    axis) in the test payload, which is after validation.
  - Chain reference `name: scientific_name` with `value: queryA.scientific_name`
    on taxon passes validation (keyword field from the mapping cache). The
    request then fails in the sub-query with an Elasticsearch `half_float`
    error for the value `1.0E9` in the test filter, which is after validation.
  - Chain reference on non-keyword `name: not_a_real_field` is still rejected:
    `unknown attribute 'not_a_real_field' for index 'taxon'`.

## Notes / warnings

- The legacy `queryA=` injection (`deserialize_helpers.rs`,
  `inject_legacy_named_queries`) reaches the search route only. It cannot be
  exercised through the report route.
- Known metadata mismatch, deferred to the next data import and not changed here:
  `mitochondrion_gc_percent` and `plastid_gc_percent` have a null traverse
  direction in metadata, but ancestor/descendant values appear in the data.
- The keyword-field exemption applies only to chain references. Plain attributes
  with the same names are still checked against metadata.
- The keyword set comes from the live mappings at startup, so a new keyword
  field is exempt after restart, with no code change. A mapping that cannot be
  read fails the cache populate, as the attribute metadata fetch does.
