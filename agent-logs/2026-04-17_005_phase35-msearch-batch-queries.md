---
date: 2026-04-17
session: "005"
description: "Phase 3.5 — batch queries via /msearch"
model: Claude Sonnet 4.6
---

## Summary

Implemented Phase 3.5: batch search execution via the `/msearch` endpoint.
This session completed work started in a previous session (steps 1–3 were
already done) and added the Python `MultiQueryBuilder` class, tests, and
exports.

## Changes

### New files

- `python/cli_generator/multi_query_builder.py` — `MultiQueryBuilder` class
  that accumulates `QueryBuilder` instances sharing common execution params
  (fields, size, sort, taxonomy, include_estimates) and issues them as a
  single POST to `/msearch`, auto-batching into groups of 100. Also includes
  a module-level `from_file()` convenience constructor that auto-detects
  three batch file formats: bare taxon list, patch YAML array, full YAML with
  `shared:` + `queries:` sections.

### Modified files

- `python/cli_generator/__init__.py` — exported `MultiQueryBuilder` and
  `from_file` in both the import block and `__all__`.

- `crates/genomehubs-query/src/parse.rs` — added 11 Rust unit tests for
  `parse_msearch_json` and `msearch_result_to_json` (happy paths, error
  handling, round-trip, invalid input).

- `tests/python/test_core.py` — added 6 tests for `parse_msearch_json` Python
  binding and 16 unit tests for `MultiQueryBuilder` (init, add_query,
  validation, from_file in all three formats). Also moved `import json` and
  added `import pathlib` to the top-level imports.

## Design decisions

- **Shared-only fields**: `include_estimates` and `taxonomy` are frozen across
  the whole batch (any per-query divergence raises `ValueError`). `size` and
  `sort` emit a warning on divergence but do not raise, since they may
  reasonably differ in interactive use.
- **`_FORBIDDEN_IN_SHARED`** keys: `taxa`, `assemblies`, `samples` are not
  valid in the `shared:` section of a full YAML file (they are per-query
  identifiers by definition).
- **`from_file` format detection**: a YAML doc with a top-level `queries:` key
  is treated as full YAML; a YAML sequence is a patch array; anything else is
  treated as a bare newline-delimited taxon list (most ergonomic for simple
  batch jobs).
- **Batch size**: 100 per POST, matching the API hard limit. Results are
  reassembled in input order across batches.

## Verification

All checks pass:

```
pytest tests/python/ -q        → 123 passed
cargo test --workspace         → 71+15+138 passed, 0 failed
cargo clippy --all-targets     → no warnings
pyright python/ tests/python/  → 0 errors
```
