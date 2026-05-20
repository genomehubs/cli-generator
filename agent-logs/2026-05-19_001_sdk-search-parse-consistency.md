# 2026-05-19_001 — SDK search/parse consistency: uniform raw-fetch + reshape API

## Summary

Redesigned the search/batch API across Python, R, and JavaScript SDKs to make
raw-fetch methods and reshaping functions fully consistent. The old
`search_batch()` pre-parsed records internally (making the output incompatible
with `to_flat_records` / `to_tidy_records`), and `count_batch()` had a bug
reading `status.hits` instead of `total` from `parse_batch_json` output.

---

## Changes

### `python/cli_generator/query.py`

**`search_batch()`**

- Removed internal call to `parse_batch_json`.
- Now restructures each batch result as a search-response-like dict
  `{"results": result["hits"], "status": {"hits": result["total"]}}`, which is
  exactly the format `parse_search_json` (and `to_flat_records`) expects.
- Return type changed from `Any` to `list[dict[str, Any]]`.

**`to_flat_records(raw_response=None, ...)`**

- Added `raw_response` parameter (first positional after `self`).
- When provided, parses the pre-fetched dict directly without calling
  `self.search()`. Works on both `search()` output and individual items from
  `search_batch()`.

**`to_tidy_records(records=None, raw_response=None, ...)`**

- Added `raw_response` parameter.
- Input priority: `records` > `raw_response` > internal `self.search()` call.
- When `raw_response` is given, it is first flattened via `parse_search_json`
  (or `parse_search_with_lineage_summary` if `lineage_summary` is set).

**`count_batch()`**

- Fixed bug: was reading `result.get("status", {}).get("hits")` but
  `parse_batch_json` returns items with `"total"`, not `"status.hits"`.
  Changed to `result.get("total") or 0`.

### `templates/python/query.py.tera`

Identical changes to `search_batch`, `to_flat_records`, `to_tidy_records`, and
`count_batch`.

### `templates/r/query.R`

**`search_batch()`**

- No longer calls `parse_batch_json`.
- Returns a list of JSON character strings, each a search-response-like object
  (`{"results":[...hits...], "status":{"hits":N}}`), compatible with
  `to_flat_records(raw_response = ...)`.

**`to_flat_records(raw_response=NULL, lineage_summary=NULL)`**

- Added `raw_response` as the first parameter.
- Accepts either a character JSON string or an R list.
- Falls back to `self$search(format="json")` when `NULL`.

**`to_tidy_records(records=NULL, raw_response=NULL, lineage_summary=NULL)`**

- Added `raw_response` parameter, same input-priority logic as Python.

### `templates/js/query.js`

**`searchBatch()`**

- No longer calls `_parseBatchJson`.
- Returns array of search-response-like objects, one per query.

**`toFlatRecords(rawResponse=null, lineageSummary=null, apiBase=API_BASE)`**

- Added `rawResponse` as the first parameter.
- Falls back to `this.search()` when `null`.

**`toTidyRecords(records, lineageSummary=null)` (module-level)**

- Now accepts a raw search-response object (with `results` key) in addition to
  flat records or a JSON string. Auto-detects by checking for `"results"` key
  on a non-array object.

### `tests/python/test_batch_operations.py`

- Updated all `search_batch` mocks to use the correct batch API format:
  `{"results": [{"hits": [...], "total": N, "error": null}]}`.
- Removed stale `parse_batch_json` mocks from `search_batch` tests (no longer
  called by `search_batch`).
- Updated `count_batch` mock data to use `"total"` instead of `"status.hits"`.
- Added new `TestToFlatRecordsRawResponse` class with 4 tests covering:
  - `to_flat_records(raw_response=<search_result>)`
  - `to_flat_records(raw_response=<batch_item>)` (using restructured format)
  - `to_tidy_records(raw_response=<search_result>)`
  - `records` taking priority over `raw_response` in `to_tidy_records`

---

## The consistent pattern

```python
# Single search — unchanged
raw = qb.search()
records = qb.to_flat_records(raw)      # or: qb.to_flat_records(raw_response=raw)
tidy    = qb.to_tidy_records(raw_response=raw)

# Batch search — now consistent
batch_raws = qb.search_batch([q1, q2])
for raw in batch_raws:
    records = qb.to_flat_records(raw)  # same call, same semantics
    tidy    = qb.to_tidy_records(raw_response=raw)
```

---

## Verification

- `python -m pytest tests/python/test_batch_operations.py` → **25 passed**
- `python -m pytest tests/python/` → **556 passed, 1 pre-existing failure**
  (TestDocumentationParity — missing Quarto docs for 7 methods, pre-existing)
- `pyright python/ tests/python/` → **0 errors**

---

## Decisions

- `search_batch()` returns raw-search-like dicts rather than pre-parsed records,
  keeping the "fetch raw, reshape separately" contract consistent.
- `count_batch()` still uses `parse_batch_json` internally (returns `list[int]`,
  no consistency issue) but with the correct `"total"` field access.
- The `parse_batch_json` PyO3 function is preserved for any future direct use;
  it is simply no longer called by `search_batch`.
