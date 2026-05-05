# Phase 3: SDK Coverage for New Endpoints (Updated - Option A)

**Strategic Decision:** Option A — Complete v3 API enhancements before SDK methods
**Status:** Waiting for Phase 3a completion
**Estimated scope:** Phase 3a (2-3 weeks v3 API) → Phase 3b (3-4 hours SDK templates)

---

## Overview

**Phase 3a: v3 API Enhancement** (BLOCKING — NEXT PRIORITY)

- Implement top-level OR support for combining multiple queries
- Implement `countBatch` endpoint for batch count operations
- Enables v2/v3 feature parity for all query types

**Phase 3b: SDK Methods** (Blocked until 3a complete)

- Add 5 new methods to Python, JavaScript, R SDKs
- Parse functions already completed (May 5, 2026)
- All methods use language-appropriate naming conventions

---

## Phase 3a: v3 API Enhancement (CURRENT FOCUS)

**Goal:** Implement missing v3 API features to reach feature parity with v2.

**Estimated effort:** 2–3 weeks
**Deliverables:** Two new v3 API endpoints enabling top-level OR and batch count

### Task 3a.1: Top-Level OR Support

**What:** Combine multiple queries with OR logic: `(query1) OR (query2) OR (query3)`

**Implementation tasks:**

1. Enhance `SearchQuery` type in `crates/genomehubs-query/src/query/mod.rs`
   - Add optional `queries: Vec<SearchQuery>` for multi-query syntax
   - Add `combine_with: "AND" | "OR"` field (default: "AND")
   - Update YAML deserialization

2. Implement OR combining logic in `crates/genomehubs-api/src/`
   - Merge multiple SearchQuery objects into single ES query
   - Use Elasticsearch `bool.should` with `minimum_should_match: 1` (v2 pattern)
   - Clean up inner_hits recursion

3. Add new API endpoint: `/api/v3/query` or extend `/search`
   - Accept: `queries: [...], combine_with: "OR"`
   - Return: Combined results with proper hit counts

4. Add integration tests for OR query combinations

### Task 3a.2: Batch Count (`countBatch`) Endpoint

**What:** Count results for multiple queries in single request (ES `_msearch` with `size: 0`)

**Implementation tasks:**

1. Add `/api/v3/countBatch` endpoint in `crates/genomehubs-api/src/routes/countBatch.rs`
   - Input: `searches: [{ query_yaml, params_yaml }, ...]` (max 100)
   - Output: Per-query count results
   - Reuse existing `batchSearch` infrastructure with `size: 0`

2. Response format:

   ```json
   {
     "status": { "success": true },
     "results": [
       { "status": { "success": true, "hits": 5250, "took": 5 } },
       { "status": { "success": true, "hits": 12000, "took": 3 } }
     ]
   }
   ```

3. Add integration tests for batch counts

### Task 3a.3: Documentation & Versioning

- Document new v3 endpoints in API reference
- Update `_v3_supported` detection to include new endpoints
- Add migration guide: v2 patterns → v3 equivalents

---

## Phase 3b: SDK Methods (Depends on Phase 3a)

**Estimated effort:** 3–4 hours
**Deliverables:** 5 new methods across Python, JavaScript, R SDKs

### Method Naming Conventions

**Language-specific naming:** Uses language convention; API endpoint names remain camelCase

| Method       | Python           | JavaScript      | R                | Notes                      |
| ------------ | ---------------- | --------------- | ---------------- | -------------------------- |
| Batch search | `search_batch()` | `searchBatch()` | `search_batch()` | Replaces `batchSearch`     |
| Batch count  | `count_batch()`  | `countBatch()`  | `count_batch()`  | NEW endpoint from Phase 3a |
| Record fetch | `record()`       | `record()`      | `record()`       | Separate from search       |
| Lookup       | `lookup()`       | `lookup()`      | `lookup()`       | Separate from search       |
| Summary      | `summary()`      | `summary()`     | `summary()`      | Separate from search       |

### Files to Modify (Phase 3b)

| File                             | Change                                                      |
| -------------------------------- | ----------------------------------------------------------- |
| `python/cli_generator/query.py`  | Add 5 new methods (with proper snake_case names)            |
| `templates/python/query.py.tera` | Mirror identical signatures for generated projects          |
| `templates/js/query.js`          | Add 5 new methods (with camelCase names)                    |
| `templates/r/query.R`            | Add 5 new methods + `search_all()` helper for R consistency |

### Parse Functions (Already Completed ✅)

See previous Phase 3 work (May 5, 2026):

- `parse_record_json()` in `crates/genomehubs-query/src/parse.rs` ✅
- `parse_lookup_json()` in `crates/genomehubs-query/src/parse.rs` ✅
- PyO3, WASM, extendr exports via 6-touchpoint checklist ✅
- Python `__init__.py` imports updated ✅
- Type stubs (`.pyi`) updated ✅

### Method Implementation Patterns

#### Python/R Pattern (for `search_batch()`, `count_batch()`, `record()`, etc.)

Build URL → HTTP request → Parse response → Return data

```python
def search_batch(self, queries, api_base=..., api_version="v3"):
    """Execute multiple searches in batch."""
    import json, urllib.request

    url = f"{api_base}/v3/searchBatch"
    payload = {"searches": [
        {"query_yaml": q.to_query_yaml(), "params_yaml": q.to_params_yaml()}
        for q in queries
    ]}
    req = urllib.request.Request(url, data=json.dumps(payload).encode(),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req) as resp:
        data = json.loads(resp.read().decode())
    # Parse results with parse_search_json
    return [json.loads(parse_search_json(json.dumps(r))) for r in data["results"]]

def count_batch(self, queries, api_base=..., api_version="v3"):
    """Get counts for multiple queries in batch."""
    # Similar to search_batch but parse results as counts
    # Extract status.hits from each result
    return [r["status"]["hits"] for r in data["results"]]
```

#### JavaScript Pattern

```javascript
async searchBatch(queryBuilders, { apiBase = API_BASE, apiVersion = "v3" } = {}) {
    const url = `${apiBase}/v3/searchBatch`;
    const payload = {
        searches: queryBuilders.map(q => ({
            query_yaml: q.toQueryYaml(),
            params_yaml: q.toParamsYaml(),
        }))
    };
    const resp = await fetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
    });
    const data = await resp.json();
    return data.results.map(r => r.hits || []);
}

async countBatch(queryBuilders, { apiBase = API_BASE, apiVersion = "v3" } = {}) {
    // Similar to searchBatch but extract hits from status
    return data.results.map(r => r.status?.hits || 0);
}
```

#### R Pattern

```r
search_batch = function(queries, api_base = private$.api_base, api_version = "v3") {
  url <- paste0(api_base, "/v3/searchBatch")
  payload <- list(searches = lapply(queries, function(q) {
    list(query_yaml = q$to_query_yaml(), params_yaml = q$to_params_yaml())
  }))
  resp <- jsonlite::fromJSON(
    httr::POST(url, body = jsonlite::toJSON(payload), encode = "json"),
    simplifyVector = FALSE
  )
  lapply(resp$results, function(r) r$hits %||% list())
},

count_batch = function(queries, api_base = private$.api_base, api_version = "v3") {
  # Similar to search_batch but extract status.hits
  sapply(resp$results, function(r) r$status$hits %||% 0)
},
```

### API Version Detection (Optional v2/v3 Fallback)

If needed, implement version probing:

```python
def _probe_v3_support(self, api_base: str) -> bool:
    """Check if API supports v3 endpoints."""
    if not hasattr(self, '_v3_checked'):
        try:
            resp = urllib.request.urlopen(f"{api_base}/v3/status", timeout=2)
            self._v3_checked = True
        except:
            self._v3_checked = False
    return self._v3_checked
```

---

## Completion Checklist (Phase 3a Prerequisites)

**Must complete before Phase 3b SDK methods:**

- [ ] Top-level OR support in `SearchQuery` type
- [ ] `/api/v3/query` endpoint (OR combining) implementation + tests
- [ ] `/api/v3/countBatch` endpoint implementation + tests
- [ ] Integration tests pass for both new endpoints
- [ ] API documentation updated
- [ ] Version detection updated

---

## Verification (Phase 3b)

Once Phase 3a complete, verify SDK methods:

```bash
# Python
pytest tests/python/ -v -k "test_search_batch or test_count_batch or test_record"

# JavaScript
npm test -- --grep "searchBatch|countBatch|record"

# R
R CMD check *.tar.gz

# End-to-end
bash scripts/dev_site.sh --python goat
```

---

## Next Steps

1. **Start Phase 3a:** Implement top-level OR support in v3 API
2. **Track progress:** Update this document as Phase 3a tasks complete
3. **Begin Phase 3b:** Once Phase 3a endpoints are live and tested
4. **Document v2/v3 transition:** Add to GETTING_STARTED guides
