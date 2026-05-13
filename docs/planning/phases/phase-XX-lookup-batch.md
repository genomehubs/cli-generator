# Phase XX: Batch Endpoints and Summary Correctness

**Status:** Design — ready for implementation sequencing
**Covers:**

1. [`/lookup/batch` — msearch refactor](#1-lookupbatch--msearch-waterfall-refactor) (currently implemented but inefficient)
2. [`/record/batch` — explicit batch record endpoint](#2-recordbatch--explicit-batch-endpoint)
3. [`/summary` — correctness fix + full implementation](#3-summary--correctness-fix-and-full-implementation)
4. [`/summary/batch`](#4-summarybatch--batch-summary-endpoint) — batch summary endpoint (depends on §3)

---

## 1. `/lookup/batch` — msearch waterfall refactor

### Current implementation

`POST /api/v3/lookup/batch` is implemented and working. It accepts up to 100 items and resolves them by spawning one `tokio::spawn` task per item. Each task calls `perform_single_lookup`, which runs up to **three sequential ES HTTP calls**:

1. SAYT (`_search` via `execute_search`) — only for `result=taxon` when `has_sayt_field`
2. Wildcard (`_search`)
3. Suggest (`_search` with `suggest` body) — only for `result=taxon` when `has_trigram_field`

**For 100 items in the common case (all taxon, SAYT index present):** up to 300 individual HTTP connections open concurrently. These are multiplexed via the underlying `reqwest` connection pool, but each is a separate round-trip. ES connection overhead dominates at this scale.

### Proposed msearch refactor

Replace the per-item sequential waterfall with three round-robin msearch rounds. The key insight is that the waterfall is a **priority fallback**: SAYT > wildcard > suggest. With msearch the same priority can be respected by:

1. **Round 1 — SAYT:** Single `_msearch` for all taxon items with SAYT queries. Collect results. Mark items that got a non-empty SAYT result as **resolved**.
2. **Round 2 — Wildcard:** Single `_msearch` for **unresolved** items (all result types). Collect results. Mark resolved.
3. **Round 3 — Suggest:** Single `_msearch` for remaining unresolved taxon items with trigram suggest.

**HTTP calls: 3 at most** (regardless of N items), vs. up to 3N in the current approach.

**Trade-off analysis:**

|                                            | Current (tokio::spawn) | Proposed (3-round msearch) |
| ------------------------------------------ | ---------------------- | -------------------------- |
| HTTP calls (100 items, worst case)         | 300                    | 3                          |
| HTTP calls (100 items, all SAYT hits)      | 100                    | 1                          |
| Latency (100 items, all SAYT hits)         | ~1×ES RTT (parallel)   | ~1×ES RTT                  |
| Latency (100 items, all fall to wildcard)  | ~2×ES RTT (parallel)   | ~2×ES RTT                  |
| Latency (100 items, 50 SAYT + 50 wildcard) | ~2×ES RTT (parallel)   | ~2×ES RTT                  |
| Connection pool pressure                   | Very high (300 conns)  | Very low (3 conns)         |
| ES thread pool saturation risk             | High at N>20           | Negligible                 |
| Code complexity                            | Low                    | Medium                     |

**Verdict:** The msearch refactor is worth implementing for production reliability — 300 concurrent connections to ES is a genuine risk at higher concurrency. Latency is roughly equivalent in the common case. The main complexity increase is post-processing msearch responses (array of `responses[i].hits`) and the round-iteration logic.

**Prerequisite:** Promote `execute_msearch` from private in `search_batch.rs` to `pub` in `es_client.rs`. Also `build_msearch_body`.

**Suggest stage note:** The v3 suggest query uses the `suggest` key (not `query`), so msearch handles it transparently — the response appears at `responses[i].suggest` rather than `responses[i].hits`. The extraction function `extract_suggest_results` already handles this shape. Msearch sends it to the same index, so no routing changes.

### Implementation steps

1. Move `build_msearch_body` + `execute_msearch` into `crates/genomehubs-api/src/es_client.rs` as `pub`.
2. Rewrite `post_lookup_batch` in `lookup_batch.rs`:
   - Round 1: build SAYT msearch bodies for taxon items (skip if `!has_sayt`), fire once, mark resolved.
   - Round 2: build wildcard msearch bodies for unresolved items (all result types), fire once, mark resolved.
   - Round 3: build suggest msearch for remaining unresolved taxon items (skip if `!has_trigram`), fire once.
   - Recombine results in input order.
3. `perform_single_lookup` in `lookup.rs` is unchanged (it remains correct for the single-item GET case).
4. Tests: existing Python unit tests (which mock urlopen) are unaffected. Add an integration test once the API server is running.

---

## 2. `/record/batch` — explicit batch endpoint

### Current state

`GET /api/v3/record` already supports **multiple record IDs** via comma-separated `recordId` query param. Internally it calls `fetch_records::fetch_records_by_id` which uses `_mget` when more than one ID is supplied. There is therefore **no efficiency gap** — a single GET already batches at the ES level.

The problem is discoverability and user behaviour: users hitting v2 `/record` in a loop have not noticed the multi-ID support.

### Proposal

Add `POST /api/v3/record/batch` with an explicit array body to:

- Signal clearly that batch is the expected pattern
- Avoid URL length limits on very large ID sets (comma-separated record IDs in a query string will hit browser/proxy limits above ~1,000 IDs)
- Match the ergonomic pattern of `/search/batch`, `/count/batch`, `/lookup/batch`

**Request body:**

```json
{
  "record_ids": ["taxon-9606", "taxon-10090", "taxon-7955"],
  "result": "taxon"
}
```

**Response:** same shape as `GET /record` (reuse `RecordResponse` / `RecordItem`).

**Implementation:** `record_batch.rs` delegates directly to `fetch_records::fetch_records_by_id` (same `_mget` path), with a size cap of 1,000 IDs.

**SDK changes:** add `record_batch(record_ids, result)` to Python/JS/R templates and source. No change to `record()` single-item method.

### Implementation steps

1. Create `crates/genomehubs-api/src/routes/record_batch.rs`.
2. Register `pub mod record_batch` in `routes/mod.rs`.
3. Register `POST /api/v3/record/batch` in `main.rs`.
4. Add `record_batch` to Python, JS, R SDK templates and source.
5. Add to `CANONICAL_METHODS` in `test_sdk_parity.py`.
6. Add to Quarto reference doc template.

---

## 3. `/summary` — correctness fix and full implementation

### Current state: the Rust implementation is a stub

`GET /api/v3/summary` in `crates/genomehubs-api/src/routes/summary.rs`:

- Fetches the requested record by ID (confirms it exists).
- Returns `summary: {}` for every requested field — **no ES aggregation is run**.
- The `summary` query param (which selects aggregation type) is read but marked `#[allow(dead_code)]`.

**The endpoint returns successfully but produces no useful data.** Any SDK call to `.summary()` currently returns empty objects.

### v2 reference implementation

`local-api-copy/src/api/v2/routes/summary.js` runs `aggregateRawValuesByTaxon`, which builds a nested ES aggregation:

```
aggs.attributes (nested: path="attributes")
  └── aggs.[field] (filter: key=field, aggregation_source="direct")
        └── aggs.summary (nested: path="attributes.values")
              ├── histogram  (ES `histogram` or `date_histogram`)
              └── terms      (ES `terms`)
```

**Query scope:** documents where `taxon_id = lineage` OR lineage contains `lineage.taxon_id = lineage` (a taxon clade query — not just the single record). This gives distributions across all taxa in the clade.

**`summary` param values** in v2: `"histogram"` or `"terms"` (not `"min,max,mean"` as the current SDK default implies). This is a **SDK mismatch** that needs correcting.

### Histogram interval computation

The v2 `histogramAgg.js` is non-trivial:

- Reads per-field metadata (`meta.type`, `meta.bins.scale`, `meta.bins.min`, `meta.bins.max`) from the type cache (`attrTypes`).
- Supports scales: `linear`, `log2`, `log10`, `log`, `sqrt`.
- For `date` fields: uses `date_histogram` with calendar intervals (`1h`, `1d`, `1w`, `1M`, `1q`, `1y`) computed from the domain range.
- For numeric fields: computes `interval = (max - min) / tickCount` (default `tickCount = 11`).
- Accepts `bounds` override from the caller (`domain`, `scale`, `tickCount`).
- Applies `extended_bounds` so empty buckets are included.
- Optionally applies `script` for log-scaled fields (inline Painless script: `Math.log10(_value)` etc.).

**The Rust implementation does not have:**

- A type metadata cache (`attrTypes` equivalent).
- Any histogram interval or bounds computation logic.
- Painless script generation.

### Implementation plan for `/summary`

The summary implementation has two independently useful parts:

#### 3a. `terms` aggregation (simpler, implement first)

For `summary=terms`, no interval or scale computation is needed. The query is:

```json
{
  "size": 0,
  "query": {
    "bool": {
      "should": [
        { "match": { "taxon_id": "<lineage>" } },
        {
          "nested": {
            "path": "lineage",
            "query": { "match": { "lineage.taxon_id": "<lineage>" } }
          }
        }
      ]
    }
  },
  "aggs": {
    "attributes": {
      "nested": { "path": "attributes" },
      "aggs": {
        "<field>": {
          "filter": {
            "bool": {
              "filter": [
                { "term": { "attributes.key": "<field>" } },
                { "term": { "attributes.aggregation_source": "direct" } }
              ]
            }
          },
          "aggs": {
            "summary": {
              "nested": { "path": "attributes.values" },
              "aggs": {
                "terms": {
                  "terms": { "field": "attributes.values.<type>_value" }
                }
              }
            }
          }
        }
      }
    }
  }
}
```

**Blocker:** The `attributes.values.<type>_value` field name requires knowing the ES type of the attribute (e.g. `long_value`, `double_value`, `keyword_value`). This requires either:

- A type-metadata lookup (equivalent to v2 `attrTypes`), **or**
- A simpler probe: run a small `_search` with `_source: ["attributes"]` on the target record to infer the value key, **or**
- Accept a required `value_type` param from the caller (`"long"`, `"double"`, `"keyword"`, `"date"`).

**Recommended:** add optional `value_type` param (explicit, avoids metadata cache). Default: attempt to infer from the target record's `attributes` array.

#### 3b. `histogram` aggregation (more complex, implement second)

Requires computing `interval`, `extended_bounds`, `offset`, and optionally a Painless script for log scales. The Rust equivalent of `histogramAgg.js`:

- Accept `bounds` object: `{ "scale": "log10", "domain": [1e6, 1e10], "tick_count": 10 }` (or use defaults).
- Compute `interval = (scale(max) - scale(min)) / (tick_count - 1)`.
- Build ES `histogram` body with `interval`, `offset`, `extended_bounds`.
- For log-scaled fields: add `script` with Painless. **Note:** scripts require ES scripting to be enabled on the cluster. If not available, fall back to `linear` scale.
- For `date` fields: use `date_histogram` with `calendar_interval`.

**Recommended approach:** Implement in `crates/genomehubs-query/src/query/mod.rs` as a `build_histogram_agg` function (keeps logic in `core`). Accept a `HistogramParams` struct with scale, domain, tick_count.

#### 3c. SDK param mismatch fix

The current SDK default is `summary_types = "min,max,mean"` in all three SDKs. This needs to change to `summary = "histogram"` (singular, matching v2). The `summary` param is a single aggregation type selector, not a comma-separated list. Update:

- `python/cli_generator/query.py`: rename `summary_types` → `summary`, default `"histogram"`
- `templates/python/query.py.tera`: same
- `templates/js/query.js`: rename `summaryTypes` → `summary`, default `"histogram"`
- `templates/r/query.R`: rename `summary_types` → `summary`, default `"histogram"`

#### Implementation order

1. Fix SDK param mismatch (§3c) — quick, zero Rust changes.
2. Implement `terms` aggregation (§3a) — medium complexity.
3. Implement `histogram` aggregation (§3b) — higher complexity, may need a follow-up.

---

## 4. `/summary/batch` — batch summary endpoint

### Rationale

Once `/summary` is fully implemented (§3), a batch endpoint that resolves multiple `(record_id, field, summary_type)` triples in one request is straightforward using `_msearch`. Each triple maps to one ES aggregation query. The response would be a parallel array.

**Defer until §3 is complete.**

### Request shape (proposed)

```json
{
  "summaries": [
    { "record_id": "9606", "fields": "genome_size", "summary": "histogram" },
    { "record_id": "9606", "fields": "assembly_level", "summary": "terms" },
    { "record_id": "10090", "fields": "genome_size", "summary": "histogram" }
  ],
  "result": "taxon"
}
```

**Each item produces one ES aggregation query (one `_msearch` pair).** Round-trip count: 1 regardless of N (up to the msearch max of 500 pairs).

### Implementation steps

1. §3 must be complete.
2. Extract `build_summary_agg_body(record_id, field, summary_type, value_type, bounds) -> Value` as a pure function (testable).
3. `post_summary_batch`: build msearch NDJSON from items, fire once, parse responses in order.
4. SDK: `summary_batch(summaries, result)` in Python/JS/R templates.

---

## Implementation sequencing

| Priority | Task                                          | Depends on          | Complexity |
| -------- | --------------------------------------------- | ------------------- | ---------- |
| High     | §3c — fix SDK `summary` param mismatch        | nothing             | Trivial    |
| High     | §3a — implement `terms` agg in summary.rs     | §3c                 | Medium     |
| High     | §2 — `/record/batch` endpoint + SDK           | nothing             | Low        |
| Medium   | §1 — lookup/batch msearch refactor            | es_client promotion | Medium     |
| Medium   | §3b — implement `histogram` agg in summary.rs | §3a, type metadata  | High       |
| Low      | §4 — `/summary/batch`                         | §3 complete         | Medium     |

---

## Open questions

1. **Type metadata for histogram/terms:** Does a field metadata endpoint or cache already exist in the Rust API that maps field names → ES value types? Check `crates/genomehubs-api/src/routes/result_fields.rs` — if it has `type` info it can be used to infer `long_value` vs `double_value` etc.

2. **Log-scale Painless scripts:** Does the target ES cluster have scripting enabled? If not, `histogram` with `script` will fail silently. May need a feature flag or fallback.

3. **`lineage` vs `taxon_id` in summary query scope:** The v2 query matches documents where `taxon_id = lineage` OR `lineage.taxon_id = lineage`. This means the summary is across the **clade rooted at that taxon**, not just the single record. Confirm this is the intended semantics for v3.

4. **`bounds` parameter surface:** The v2 `histogramAgg` accepts `bounds` from a report config. For the REST API, bounds could be an optional JSON body param or a query-string JSON blob. Recommend: optional JSON body field `bounds: { scale, domain, tick_count }`.

---

## Overview

### The problem

Autocomplete/name-resolution workflows (e.g. resolving a spreadsheet of taxon names to NCBI taxon IDs) currently require one HTTP GET per name. There is no server-side batching primitive, so large lists incur avoidable latency from repeated round-trips.

### The solution

`POST /api/v3/lookup/batch` accepts an array of lookup items and runs them concurrently via `tokio::spawn`, returning results in input order. The three-stage waterfall logic (SAYT → wildcard → suggest) is unchanged and reused verbatim.

---

## Implementation

### 1. Rust — extract `perform_single_lookup` helper

Refactor `crates/genomehubs-api/src/routes/lookup.rs` to extract a `pub(crate)` helper:

```rust
/// Run the three-stage lookup waterfall for a single search term.
pub(crate) async fn perform_single_lookup(
    state: &Arc<AppState>,
    search_term: &str,
    result_type: &str,
    size: usize,
) -> LookupResponse { … }
```

`get_lookup` becomes a thin wrapper:

```rust
pub async fn get_lookup(…) -> Json<LookupResponse> {
    let result_type = q.result.as_deref().unwrap_or(&state.default_result);
    Json(perform_single_lookup(&state, &q.search_term, result_type, q.size.unwrap_or(10)).await)
}
```

---

### 2. Rust — new `lookup_batch.rs`

```
crates/genomehubs-api/src/routes/lookup_batch.rs  (new file)
```

Request body:

```json
{
  "lookups": [
    { "search_term": "Homo sapiens", "result": "taxon", "size": 10 },
    { "search_term": "Mus musculus" },
    { "search_term": "GCA_000001405", "result": "assembly" }
  ]
}
```

- `result` defaults to the builder's index (`"taxon"` if unset).
- `size` defaults to `10`.
- Maximum 100 items per request (same cap as `/search/batch`).

Response:

```json
{
  "status": { "ok": true, "hits": 3 },
  "results": [
    {
      "status": { "ok": true, "hits": 1 },
      "results": [
        {
          "id": "9606",
          "name": "Homo sapiens",
          "rank": "species",
          "reason": "sayt"
        }
      ]
    },
    {
      "status": { "ok": true, "hits": 1 },
      "results": [
        {
          "id": "10090",
          "name": "Mus musculus",
          "rank": "species",
          "reason": "sayt"
        }
      ]
    },
    {
      "status": { "ok": true, "hits": 1 },
      "results": [
        {
          "id": "GCA_000001405.40",
          "name": "GCA_000001405.40",
          "rank": null,
          "reason": "wildcard"
        }
      ]
    }
  ]
}
```

All lookups run concurrently via `tokio::spawn`; results are returned in input order.

---

### 3. Route registration

`routes/mod.rs`:

```rust
pub mod lookup_batch;
```

`main.rs` — route:

```rust
.route("/api/v3/lookup/batch", axum::routing::post(routes::lookup_batch::post_lookup_batch))
```

`main.rs` — OpenAPI paths + schemas:

```rust
routes::lookup_batch::post_lookup_batch,
routes::lookup_batch::LookupBatchItem,
routes::lookup_batch::LookupBatchRequest,
routes::lookup_batch::LookupBatchResponse,
routes::lookup_batch::LookupBatchResultItem,
```

---

### 4. SDK interface — Python

Add `lookup_batch` to `QueryBuilder` in `python/cli_generator/query.py` and
`templates/python/query.py.tera`:

```python
def lookup_batch(
    self,
    lookups: list[str | dict[str, Any]],
    result: str | None = None,
    size: int = 10,
    api_base: str = "https://goat.genomehubs.org/api",
    api_version: str = "v3",
) -> Any:
    """Resolve multiple search terms to record IDs in a single request.

    Each element of ``lookups`` is either a plain search-term string or a dict
    with keys ``search_term`` (required), ``result`` (optional), ``size``
    (optional).  Per-item ``result``/``size`` override the method-level defaults.

    Args:
        lookups: List of search terms (strings or dicts).
        result: Default result type for items that don't specify one.
        size: Default page size for items that don't specify one.
        api_base: API base URL.
        api_version: API version string.

    Returns:
        Parsed batch lookup response.
    """
```

Usage:

```python
names = ["Homo sapiens", "Mus musculus", "Danio rerio"]
response = QueryBuilder("taxon").lookup_batch(names)
for item in response["results"]:
    for hit in item["results"]:
        print(hit["id"], hit["name"])
```

Mixed with per-item overrides:

```python
lookups = [
    {"search_term": "Homo sapiens", "size": 3},
    {"search_term": "GCA_000001405", "result": "assembly"},
]
response = QueryBuilder("taxon").lookup_batch(lookups)
```

---

### 5. SDK interface — JavaScript

Add `lookupBatch` to `templates/js/query.js`:

```javascript
async lookupBatch(lookups, result = null, size = 10) {
    /**
     * Resolve multiple search terms to record IDs in a single request.
     * @param {Array<string|object>} lookups  Items as strings or {search_term, result?, size?}
     * @param {string} [result]   Default result type
     * @param {number} [size=10]  Default page size
     */
}
```

---

### 6. SDK interface — R

Add `lookup_batch` to `templates/r/query.R`:

```r
lookup_batch = function(lookups, result = NULL, size = 10) {
    #' Resolve multiple search terms to record IDs in a single POST.
    #' @param lookups Character vector of search terms, or a list of named lists
    #'   with elements \code{search_term}, \code{result} (optional),
    #'   \code{size} (optional).
    #' @param result Default result type for items that omit it (default: index).
    #' @param size Default page size for items that omit it (default: 10).
    #' @return Parsed batch lookup response list.
}
```

---

## Test coverage

| Test                                                                                          | Location                                                    |
| --------------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| `test_lookup_batch_normalises_strings` — string items expand to `{search_term, result, size}` | `tests/python/test_core.py`                                 |
| `test_lookup_batch_normalises_dicts` — dict items merge with defaults                         | `tests/python/test_core.py`                                 |
| `test_lookup_batch_empty` — raises `ValueError` for empty input                               | `tests/python/test_core.py`                                 |
| Integration: batch of known names returns matching IDs                                        | `tests/python/test_batch_integration.py` (skip without API) |

---

## Backward compatibility

Purely additive — new POST endpoint and new SDK methods. All existing GET `/lookup` callers are unaffected.
