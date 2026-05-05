# V3 API Audit: Search Types, OR Support, and Naming

**Date:** May 5, 2026
**Scope:** Review v3 API completeness for Phase 3 SDK method implementation
**Status:** ⚠️ Gaps identified requiring decision on v3 completeness strategy

---

## Executive Summary

| Feature               | v2 API                                | v3 API                                  | Gap       | Impact                                                        |
| --------------------- | ------------------------------------- | --------------------------------------- | --------- | ------------------------------------------------------------- |
| **Basic search**      | `search`                              | `search` ✅                             | None      | Ready                                                         |
| **Paginated search**  | `searchPaginated` (cursor-based)      | `search` (cursor via `search_after`) ✅ | Naming/UX | Minor—functional but different endpoint structure             |
| **Batch search**      | `msearch`                             | `batchSearch` ✅                        | Renamed   | No impact; v3 is drop-in replacement                          |
| **Count (single)**    | `count`                               | `count` ✅                              | None      | Ready                                                         |
| **Count (batch)**     | Via `msearch` with `limit: 0`         | ❌ NOT IMPLEMENTED                      | Missing   | Blocks batch count feature; workaround unclear                |
| **Top-level OR**      | Full support via `combineOrQueries()` | ❌ NOT IMPLEMENTED                      | Missing   | Blocks multi-query OR combining; affects count, search, batch |
| **Query combination** | `query1 OR query2 OR query3`          | Single query per POST                   | Missing   | Requires architectural change                                 |

---

## Detailed Findings

### 1. Search Endpoint Types ✅ (Mostly complete)

#### `search` endpoint (v3: `/api/v3/search`)

- **Function:** Execute single search query
- **Input:** `query_yaml`, `params_yaml` → `SearchQuery`, `QueryParams`
- **Output:** Flat results array, `search_after` cursor for pagination
- **Status:** ✅ **Fully implemented and tested**
- **Pagination model:** **Cursor-based** (Elasticsearch `search_after` token)
- **Example response:**
  ```json
  {
    "status": { "success": true, "hits": 5250, "took": 42 },
    "results": [...],
    "search_after": [42, "last_id_token"]
  }
  ```

#### `searchPaginated` endpoint (v2 only)

- **v2 function:** Explicit cursor-based pagination with `hasMore` flag
- **v3 status:** ❌ **NO SEPARATE ENDPOINT** — functionality merged into `search`
- **v3 approach:** `search` endpoint returns `search_after` token (cursor) directly
- **Migration path:** Use `search` with iterative `search_after` cursor
- **Difference:**
  - v2: Explicit `/searchPaginated` endpoint (UX: "fetch page N")
  - v3: Single `/search` endpoint with implicit pagination (UX: "fetch next batch")
- **Impact:** **Low** — v3 is actually more elegant (one endpoint, not two)

#### `batchSearch` endpoint (v3: `/api/v3/batchSearch`) ✅

- **Function:** Execute multiple independent searches (renamed from v2 `msearch`)
- **v2 name:** `msearch`
- **v3 name:** `batchSearch` (explicit, clearer semantics)
- **Input:** Array of up to 100 `{ query_yaml, params_yaml }` objects
- **Output:** Per-query results with individual counts
- **Status:** ✅ **Fully implemented**
- **Example response:**
  ```json
  {
    "status": { "success": true },
    "results": [
      { "status": { "success": true, "hits": 100, "took": 10 }, "count": 100, "hits": [...] },
      { "status": { "success": true, "hits": 250, "took": 15 }, "count": 250, "hits": [...] }
    ]
  }
  ```

---

### 2. Count Support ⚠️ (Partial)

#### Single count ✅

- **v3 endpoint:** `POST /api/v3/count`
- **Input:** `query_yaml`, `params_yaml` (same as search)
- **Output:** Total count via `status.hits`
- **Status:** ✅ **Fully implemented**
- **Example response:**
  ```json
  {
    "status": { "success": true, "hits": 5250, "took": 8 },
    "url": "http://es:9200/taxon-ncbi-goat-2021.10.15/_count?..."
  }
  ```

#### Batch count ❌

- **v2 pattern:** Send `msearch` requests with `limit: 0` to get counts without results
- **v3 status:** ❌ **NO BATCH COUNT ENDPOINT**
- **Workaround options:**
  1. **Individual count calls:** Send N separate POST requests to `/api/v3/count` (N round-trips, slower)
  2. **batchSearch with size=0:** Send `batchSearch` with `size: 0` in each `params_yaml` (returns counts in envelope)
  3. **Implement `/api/v3/batchCount`:** New endpoint parallel to `batchSearch`
- **Recommended:** Option 2 (use `batchSearch` with `size: 0`)
- **SDK impact:** Batch count would delegate to `batchSearch` with `size: 0`

#### Count modes (v2 vs v3)

- **v2:** Single total only; no built-in unique/distinct count
- **v3:** Same—single total per query
- **Top-level OR requirement:** To support "count with OR" (combining results from multiple queries), need OR support first

---

### 3. Top-Level OR Support ❌ (NOT IMPLEMENTED)

#### v2 implementation

- **How it works:** `combineOrQueries()` function in v2 takes multiple queries and combines them
- **Semantics:** `query1 OR query2 OR query3` → Elasticsearch `bool.should` with `minimum_should_match: 1`
- **Example:** `tax_rank(species) OR tax_rank(genus)` → union of both ranks
- **All inner_hits recursively stripped** to avoid nesting issues

#### v3 status

- **Query structure:** Single `SearchQuery` per API call (no multi-query support in type system)
- **Top-level OR logic:** ❌ **NOT IMPLEMENTED**
- **Required to support:**
  - Combining multiple queries at the top level
  - Count across OR-combined queries (without N separate calls)
  - Batch search with OR combinations

#### Implementation gap

- **v3 SearchQuery type:** Represents single query only
  ```rust
  pub struct SearchQuery {
      pub index: SearchIndex,
      pub identifiers: Identifiers,
      pub attributes: AttributeSet,
  }
  ```
- **No union/OR field** in the struct
- **Solution options:**
  1. Add `queries: Vec<SearchQuery>` + `combine_with: "AND" | "OR"` to allow multi-query syntax
  2. Pre-process OR queries into a single combined query (v2 approach)
  3. Support OR at SDK level (client-side decomposition)

---

## Naming Consistency Review

### Current endpoint names

| v2 API            | v3 API        | Pattern                   | Clarity                                      |
| ----------------- | ------------- | ------------------------- | -------------------------------------------- |
| `search`          | `search`      | `verb-noun` ✅            | Clear: primary action                        |
| `searchPaginated` | (merged)      | `verb-adjective`          | Awkward; "search" already implies pagination |
| `msearch`         | `batchSearch` | `m-prefix` vs `verb-noun` | v3 is clearer: `batchSearch` reads naturally |
| `count`           | `count`       | `noun` ⚠️                 | Inconsistent: other endpoints are verbs      |

### Recommendation: Consistent verb-first naming

**Option A: Keep current naming**

- `search`, `batchSearch`, `count`
- Count is oddball (noun instead of verb)
- Users: `builder.search()`, `builder.batchSearch()`, `builder.count()`

**Option B: Make everything verb-first**

- Rename `count` → `getCount()` or keep as `count()` (conventionally "countResults()")
- Rename `batchSearch` → `searchBatch()` or keep `batchSearch()`
- Pattern: All methods start with action verb
- Users: `builder.search()`, `builder.searchBatch()`, `builder.count()`

**Option C: Use `search*` prefix for all search types**

- `search()` → basic search
- `searchBatch()` → batch search (rename from `batchSearch`)
- `searchCount()` → count (rename from `count`)
- `searchPaginated()` → iterative paginated search (new SDK helper)
- Pattern: All search operations under `search*` namespace
- Users: `builder.search()`, `builder.searchBatch()`, `builder.searchCount()`

### Recommendation: **Option C** (`search*` prefix)

**Rationale:**

- ✅ Consistent verb-first pattern
- ✅ Groups related operations (all search types under `search*` prefix)
- ✅ Easier IDE autocomplete (`builder.search[TAB]` → suggests all variants)
- ✅ Aligns with v2 naming philosophy (`search`, `searchPaginated`, `msearch` were all search operations)
- ✅ Clear for new users: "I want to search in a batch mode" → `searchBatch()`
- ✅ Future-proof: `searchFilteredRecord()`, `searchAggregated()`, etc. can fit pattern

**SDK method naming under Option C:**

```python
# Core search methods
builder.search()              # Basic search → SearchResponse { status, results, search_after }
builder.searchBatch()         # Batch search → BatchSearchResponse { status, results[...] }
builder.searchCount()         # Count query → CountResponse { status.hits }
builder.searchRecord()        # Fetch record → RecordResponse { status, records }
builder.searchLookup()        # Lookup → LookupResponse { status, results }
builder.searchSummary()       # Summary → SummaryResponse { status, summary }

# Optional SDK helpers (built on top of core methods)
builder.searchPaginated()     # Iterative pagination wrapper around search()
builder.searchBatchCount()    # Wrapper: batchSearch with size=0 to get counts
```

---

## Gap Analysis: What's Missing from v3 API for Phase 3

### Critical gaps (blocking feature parity)

| Gap             | v2                      | v3                   | Impact                                                                   | Priority   |
| --------------- | ----------------------- | -------------------- | ------------------------------------------------------------------------ | ---------- |
| Top-level OR    | ✅ Supported            | ❌ Missing           | Cannot combine queries with OR; blocks batch OR-count                    | **HIGH**   |
| Batch count     | ✅ Via msearch + size:0 | ⚠️ Workaround exists | Users must use `searchBatch` with `size:0` instead of dedicated endpoint | **MEDIUM** |
| searchPaginated | ✅ Dedicated endpoint   | ✅ Via search_after  | Functionality present; naming/UX different                               | **LOW**    |

### Decision needed: v3 API completion strategy

**Option A: Wait for v3 to mature** (Postpone Phase 3)

- Implement top-level OR support in v3 API first
- Implement `/api/v3/batchCount` endpoint
- Proceed with full v2/v3 parity
- **Timeline:** +2-3 weeks

**Option B: Phase 3 with v3 partial support** (Proceed now)

- Implement all SDK methods for complete features (`search`, `searchBatch`, `searchCount`, `searchRecord`, `searchLookup`, `searchSummary`)
- Document limitations: "Top-level OR and batch count not yet supported in v3"
- Add TODOs to phase plan for future v3 API enhancements
- SDK detects v3 version; falls back to v2 for OR/batch-count operations
- **Timeline:** Phase 3 proceeds; v3 gaps addressed in Phase 3b

**Option C: v2-only for Phase 3** (Conservative)

- Complete SDK methods using v2 API only
- v3 is treated as beta/experimental
- Once v3 reaches feature parity, implement v3 path
- **Timeline:** Longer; v3 adoption deferred

---

## Recommendations

### 1. **Naming: Adopt Option C** (`search*` prefix)

Update Phase 3 plan to use:

- `search()` → basic search
- `searchBatch()` → batch search (rename SDK method)
- `searchCount()` → count query (rename SDK method)
- `searchRecord()` → fetch record
- `searchLookup()` → lookup
- `searchSummary()` → summary

Update method signature docs in:

- `python/cli_generator/query.py`
- `templates/python/query.py.tera`
- `templates/js/query.js`
- `templates/r/query.R`

### 2. **Top-level OR: Decision required**

Before implementing Phase 3 SDK methods, decide:

- **If proceeding with v3 partial support (Option B):**
  - Implement v3 methods without OR
  - Document: "Top-level OR available in v2 API; v3 will add this in Phase 3b"
  - Set up automatic v2 fallback in SDK version detection

- **If waiting for v3 maturity (Option A):**
  - Implement `/api/v3/combineOrQueries()` wrapper or enhance SearchQuery type
  - Implement `/api/v3/batchCount` endpoint
  - Then proceed with Phase 3 SDK methods with full v2/v3 parity

- **If conservative (Option C):**
  - Implement Phase 3 SDK methods for v2 API only
  - Phase 3b (future) will add v3 API paths

### 3. **Batch count: Recommend delegating to searchBatch**

Until `/api/v3/batchCount` exists:

- SDK `searchBatchCount(queries)` → calls `searchBatch(queries_with_size_0)`
- Extract counts from `status.hits` in batch response
- Document: "Batch count returns individual counts; no deduplication for OR queries"

### 4. **Update Phase 3 plan document**

Create new sections:

- **Phase 3a: Complete methods (v2/v3)** – `search`, `searchBatch`, `searchCount`, `searchRecord`, `searchLookup`, `searchSummary`
- **Phase 3b (Future): Top-level OR support** – Depends on v3 API OR implementation
- **Phase 3c (Future): Batch count optimization** – Depends on `/api/v3/batchCount` endpoint

---

## Dependency chart for SDK methods

```
Phase 3a (Ready now):
├── search()           ✅ v2/v3 ready
├── searchBatch()      ✅ v2/v3 ready (renamed from msearch/batchSearch)
├── searchCount()      ✅ v2/v3 ready (renamed from count)
├── searchRecord()     ✅ v2/v3 ready (new in Phase 2 API)
├── searchLookup()     ✅ v2/v3 ready (new in Phase 2 API)
└── searchSummary()    ✅ v2/v3 ready (new in Phase 2 API)

Phase 3b (Blocked – waiting for v3 enhancements):
├── searchWith(OR)     ❌ requires v3 top-level OR implementation
└── searchBatchCount() ⚠️ workaround: use searchBatch(size:0)

Pagination (SDK helper):
└── iterativePagination() ✅ wrapper around search_after logic
```

---

## Next Steps

1. **Make strategic decision:** Option A, B, or C? (Recommend: **Option B** — proceed with Phase 3a now)
2. **If Option B:** Update phase-3-sdk-coverage.md to:
   - Split into Phase 3a (v2/v3 ready) and Phase 3b (future v3 enhancements)
   - Update method names: `search`, `searchBatch`, `searchCount`, `searchRecord`, `searchLookup`, `searchSummary`
   - Add implementation notes: "Top-level OR not available in v3 API yet"
3. **Implement Phase 3a** with v2 API paths and v3 API paths (both working)
4. **Track Phase 3b** improvements in technical debt or roadmap for v3 API enhancements
