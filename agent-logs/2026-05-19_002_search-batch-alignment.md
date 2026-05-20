# Agent Log: 2026-05-19_002 — Align search_batch with search

## Summary

Fixed the server-side `/api/v3/search/batch` endpoint so that its response format
is fully compatible with `parse_search_json` and the result-shaping helpers
(`to_flat_records`, `to_tidy_records`). Also added `lineage_rank_summary` support,
`search_after` cursors, and proper attribute field extraction.

---

## Problem Statement

`search_batch()` results returned empty records when passed to `to_flat_records()`
or `to_tidy_records()`. Root cause: two independent server-side bugs in
`crates/genomehubs-api/src/routes/search_batch.rs`:

1. **Wrong hit format** — hits were extracted as raw `_source` objects:

   ```rust
   .filter_map(|hit| hit.get("_source").cloned())
   ```

   `parse_search_json` expects `{index, id, score, result}` objects (the output
   of `deserialize_helpers::transform_es_hit`). Raw `_source` objects have none
   of those top-level keys, so `parse_search_json` returned empty records.

2. **No `lineage_rank_summary` support** — the batch endpoint ignored the
   `lineage_rank_summary` field in each query's YAML, so aggregations were never
   built and `lineage_summary` was always absent from results.

3. **No `types_map`** — `build_search_body` was called with `None` for the
   `types_map` parameter, so field type optimisation (selecting the single correct
   typed-value docvalue field) did not apply in batch mode.

4. **Stale SDK mapping** — the SDK `search_batch()` read `result.get("hits", [])`
   but the new server format uses `results`. Also `lineage_summary` was not
   propagated from server response to the SDK return value.

---

## Changes

### `crates/genomehubs-api/src/routes/search_batch.rs`

- **`SearchBatchResultItem`** struct: replaced `{status, count, hits}` with
  `{total, results, search_after?, lineage_summary?, error?}`. The new `results`
  field contains `{index, id, score, result}` objects identical to those returned
  by `/api/v3/search`, making the response directly consumable by `parse_search_json`.

- **`BatchQueryMeta`** struct (private): new helper carrying per-query metadata
  (`group`, `include_lineage`, `include_taxon_names`, `lineage_specs`) alongside
  each `(index, body)` pair so the response processing loop can transform each
  `_msearch` response correctly.

- **`types_map`** acquisition from `state.cache` added before the main loop
  (mirrors `search.rs`). Passed to both `build_search_body` call sites.

- **Lineage agg injection**: after building each query body and injecting the
  `id_set` filter, `lineage_rank_summary` specs are validated and injected as ES
  aggregations into the body (mirrors `search.rs`).

- **Response loop** replaced:
  - `for response in responses` → `for (response, meta) in responses.iter().zip(metas.iter())`
  - Hit extraction: `hit.get("_source").cloned()` → `deserialize_helpers::transform_es_hit(hit, &meta.group, meta.include_lineage, meta.include_taxon_names)`
  - Added `search_after` cursor extraction from each response.
  - Added `lineage_summary` extraction via `lineage_agg::extract_lineage_summary`.
  - `all_ok` check updated to `r.error.is_none()`.

### `python/cli_generator/query.py`

- `search_batch()`: `result.get("hits", [])` → `result.get("results", [])`.
- `search_batch()`: propagates `lineage_summary` from each batch result item.

### Templates

- `templates/python/query.py.tera`, `templates/r/query.R`, `templates/js/query.js`:
  same `results`/`lineage_summary` propagation changes as the canonical Python SDK.

### `tests/python/test_batch_operations.py`

- Updated 3 mock server responses to use the new format (`results` instead of
  `hits` in per-item batch result objects).

---

## Verification

```
cargo build -p genomehubs-api   # clean
cargo clippy -p genomehubs-api  # clean
cargo test --workspace          # all pass
pytest tests/python/ -q         # 556 passed, 1 pre-existing doc-parity failure
```

The pre-existing failure (`test_documented_methods_include_all_canonical`) was
present before these changes and concerns undocumented methods unrelated to this
work.

---

## End-to-End Data Flow (after fix)

```
POST /api/v3/search/batch  →  ES _msearch  →  per-response transform_es_hit
  →  SearchBatchResultItem { total, results:[{index,id,score,result}...],
                              search_after?, lineage_summary?, error? }
  →  SDK search_batch() returns [{"results":[...], "status":{"hits":N},
                                   "lineage_summary":..., ...}, ...]
  →  to_flat_records(raw_response=batch_item)   ✓  (same as single search)
  →  to_tidy_records(raw_response=batch_item)   ✓  (same as single search)
```
