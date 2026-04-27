# Search Builder: Plan & Notes

Purpose
-------
Capture the current state of the Rust-side search builder work, key decisions, scope, and an actionable plan so this can be picked up later with minimal context switching.

Scope & assumptions
-------------------
- Rust `build_search_body` will be the authoritative generator for ES request bodies used by SDKs.
- For the immediate iteration we will *not* implement aggregations; they are noted and scheduled for later.
- We will use the example fixtures in `tests/python/fixtures-goat` to capture the range of query parameters to support. Some fixtures may fail against the local ES instance; those are recorded but not blocking.

Key findings so far
-------------------
- A working `build_search_body` exists and the `examples/live_query_demo.rs` successfully POSTs the constructed body to a local ES instance and returns hits with `inner_hits`.
- The canonical shapes for nested `attributes` filters, `inner_hits`, and taxon-name/id matching were reverse-engineered from the generated CLI and the local API `searchByTaxon` fragments.

Immediate goals
---------------
1. Inventory all example fixtures in `tests/python/fixtures-goat` to enumerate the parameter space (taxa, identifiers, attributes, filters, exclusions, paging, fields, snippet options, etc.).
2. Map each fixture to the minimal set of builder features required to reproduce its query body.
3. Implement a Rust-side `process_hits` (name: `process_hits`) by inspecting the API's `processHits` function and drafting a Rust equivalent to convert raw ES output to SDK return shapes.
4. Run fixtures against local ES where possible and record successes & failures.
5. Incrementally extend `build_search_body` to cover the mapped features (explicitly skip aggregations in this pass; add TODOs where aggregations would be required).
6. Add an integration test harness that can POST built bodies to a configurable ES and assert the response shape (presence of `hits`, `inner_hits`, and required fields).
7. Write a short agent-log entry once the above are underway.

ProcessHits focus
-----------------
- Why: `processHits` maps raw ES `hits` and `inner_hits` into the SDK return object; having this early makes it easier to validate builder correctness without full parity on every ES feature.
- Actions:
  - Locate the JS `processHits` implementation in the local API (already inspected during reverse engineering).
  - List the transformation steps it performs (extract `fields`, map nested attribute values, compute aggregated summaries, reformat types).
  - Draft a Rust implementation that mirrors those steps, with unit tests driven by saved example ES responses (e.g., `docs/planning/debug_search_response_mammalia.json`).

Fixtures and data sources
-------------------------
- Fixtures folder: `tests/python/fixtures-goat`
- Saved real response used as reference: `docs/planning/debug_search_response_mammalia.json`
- Local API canonical fragments: `local-api-copy/src/api/v2/queries/searchByTaxon.js` and its `queryFragments`.

Next immediate actions (what I'll do now if you approve)
------------------------------------------------------
1. Enumerate the fixtures under `tests/python/fixtures-goat` and produce a short CSV / table mapping fixture → features used.
2. Inspect `processHits` in the local API and produce a short transformation spec to implement in Rust.

Notes / TODOs
------------
- Aggregations: deferred — mark places in the builder where aggs are required and add sample fixture IDs that will need them.
- Authentication/headers: tests should allow an override for secured ES endpoints; add this later if needed.

Commands to run locally
-----------------------
List fixtures:

```
ls -1 tests/python/fixtures-goat
```

Run the debug demo (already used):

```
cargo run --example live_query_demo -- --result taxon --taxa "Mammalia" --fields "genome_size" --debug
```

Run unit tests after changes:

```
cargo test
```

Contact / ownership
-------------------
Created by: agent — pick up here and continue with fixture inventory when ready.

Latest progress (2026-04-24)
---------------------------
- `build_search_body` implemented and exercised by `examples/live_query_demo.rs` against local ES.
- Added a Rust port of `processHits` at `src/core/process_hits.rs` that now:
  - converts `attributes` → `fields` (including `is_primary` and `rawValues`),
  - parses `inner_hits.taxon_names` and `inner_hits.identifiers` into `result.names`,
  - merges per-attribute `inner_hits.attributes` into `result.result.fields[<key>].inner_hits` (array of simplified maps),
  - normalizes date-like attribute values by stripping trailing midnight (`T00:00:00...Z`) into `YYYY-MM-DD`.

Remaining `processHits` functionality (not yet implemented)
-------------------------------------------------------
- Aggregation bucket/binning label generation for numeric/temporal fields (used for `binned` labels).
- Heuristics to prefer `is_primary` values across merged inner_hits (currently merged as-is; no dedup/priority logic).
- Advanced identifier merging rules (e.g., collapsing duplicates, canonicalization across classes).
- Full normalization of varied ISO date formats (currently strips exact midnight suffixes only).
- Any coverage for scripted fields or runtime fields that require special parsing.

Next immediate tasks
--------------------
- Finish identifier canonicalization rules and primary-value preference.
- Implement stronger date parsing/normalization for diverse ISO formats.
- Add unit tests that assert date-normalized values and inner_hit merges for representative fixtures.
- Run end-to-end fixture validations against local ES and record mismatches.
