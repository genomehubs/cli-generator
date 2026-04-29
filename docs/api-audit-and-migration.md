# API Audit: Old API vs New Query Builder - Gaps & Priorities

**Date:** 2026-04-29
**Focus:** Simple endpoints first (/count, /search, /resultFields), skip complex reports for now

---

## Summary

The old API (`local-api-copy/src/api/v2`) has **15 major endpoints**. The new query builder covers **~40% of core logic** but lacks the HTTP endpoint wiring, response parsing refinement, and critical metadata serving infrastructure.

**Critical blocker identified:** `/resultFields` endpoint needs full ES query building. Without it, no downstream validation or field processing works. This is **TOP PRIORITY**.

---

## Old API Endpoints (from api-v2.yaml)

| Endpoint           | Purpose                                                      | Complexity    | Status                                                      |
| ------------------ | ------------------------------------------------------------ | ------------- | ----------------------------------------------------------- |
| `/count`           | Count results matching query                                 | Low           | ✅ ~80% (ES query building done, needs HTTP wiring)         |
| `/search`          | Search with field results                                    | Medium        | 🟡 ~50% (query building done, response parsing needs work)  |
| `/msearch`         | Batch multiple searches                                      | Medium        | 🔴 ~10% (query building there, batch orchestration missing) |
| `/searchPaginated` | Paginated search                                             | Medium        | 🔴 ~20% (offsets in query_builder, needs HTTP loop)         |
| `/resultFields`    | **CRITICAL** — Get field metadata from attributes index      | High          | 🔴 ~5% (**attr_types queries ES, but no HTTP endpoint**)    |
| `/record`          | Get single record by ID                                      | Low           | 🔴 0% (not started)                                         |
| `/lookup`          | Resolve alternate identifiers (e.g. NCBItax → custom ID)     | Low           | 🔴 0% (not started)                                         |
| `/download`        | Export results as CSV/TSV/JSON                               | Medium        | 🔴 0% (needs formatters + streaming)                        |
| `/report`          | Complex aggregation reports (histogram, scatter, tree, etc.) | **Very High** | 🔴 0% (out of scope for now per plan)                       |
| `/summary`         | Summary aggregations (counts by category, bounds)            | High          | 🔴 0% (depends on `/resultFields`)                          |
| `/indices`         | List available indices (taxon, assembly, etc.)               | Low           | 🔴 0% (trivial; return from config)                         |
| `/taxonomies`      | List available taxonomies (NCBI, ITIS, etc.)                 | Low           | 🔴 0% (trivial; return from config)                         |
| `/taxonomicRanks`  | List available ranks for current taxonomy                    | Low           | 🔴 0% (trivial; return from config)                         |
| `/phylopic`        | Lookup phylopic image URLs                                   | Very Low      | 🔴 0% (external API; low priority)                          |
| `/progress`        | Job progress tracking                                        | Low           | 🔴 0% (async job support; deferred)                         |

---

## Critical Path: `/resultFields` Endpoint

**Why this is TOP PRIORITY:**

1. **Enables field validation** — Query validation (`validate()` in SDKs) depends on knowing:
   - Valid field names (and synonyms)
   - Field types (keyword, long, date, nested)
   - Allowed enum values (for categorical fields)
   - Processing rules (direct/ancestor/descendant modifiers)

2. **Powers downstream aggregations** — Report building, summary generation, describe snippets all need:
   - Field metadata to know valid modifiers (min, max, median)
   - Field types to infer correct bucket strategies
   - Synonyms to normalize user input

3. **Currently: Half-implemented**
   - ✅ `attr_types()` function queries ES attributes index and returns `(TypesMap, SynonymsMap)`
   - ❌ No HTTP endpoint (`GET /api/v2/resultFields?result=taxon&index=attributes`)
   - ❌ No response shaping (old API returns `{ status, fields, identifiers, hub, release, source }`)
   - ❌ No caching/TTL logic (users call this once then cache)

**Implementation tasks:**

1. Create `src/core/result_fields.rs` that wraps `attr_types()` with response shaping
2. Add HTTP handler in generated API (wire to `POST /api/{version}/resultFields`)
3. Implement response format matching old API:
   ```json
   {
     "status": { "success": true },
     "fields": { "genome_size": {...}, "assembly_level": {...} },
     "identifiers": { "..." },
     "hub": "genomehubs",
     "release": "2026-04-28",
     "source": "NCBI"
   }
   ```
4. Add caching (probably 24-hour TTL for ES queries)

---

## Core Endpoint Gaps

### Tier 1: Essential (Blocks everything else)

| Gap                               | Old Code                          | New Status              | Notes                                                |
| --------------------------------- | --------------------------------- | ----------------------- | ---------------------------------------------------- |
| **`/resultFields` HTTP endpoint** | `resultFields.js` → `attrTypes()` | 🔴 Missing              | TOP PRIORITY — unblocks validation, field processing |
| **attr_types ES query**           | `functions/attrTypes.js`          | ✅ Done (attr_types.rs) | Query attributes index, extract types/synonyms       |
| **Response format/caching**       | `resultFields.js`                 | 🔴 Missing              | Shape JSON response, add TTL logic                   |

### Tier 2: Core (Needed for MVP search)

| Gap                         | Old Code                              | New Status                                             | Notes                                                                    |
| --------------------------- | ------------------------------------- | ------------------------------------------------------ | ------------------------------------------------------------------------ |
| **Search body building**    | `queries/*.js` + `reports/setAggs.js` | ✅ ~70% (query_builder.rs)                             | Handles attributes, sorting, pagination; needs refinement for edge cases |
| **Search response parsing** | `functions/processHits.js`            | 🟡 ~60% (process_hits.rs)                              | Converts ES hits to field-flattened format; may miss edge cases          |
| **Simple field extraction** | `functions/parseFields.js`            | 🟡 ~40% (partial in query_builder)                     | Converts field list to ES `_source` includes/excludes                    |
| **Count body building**     | (implicit in `queries/`)              | ✅ ~90% (query_builder.rs)                             | Simple bool.filter from attributes + taxon query                         |
| **Exclusion filters**       | `functions/setExclusions.js`          | 🔴 ~20% (attr_types has synonyms, not exclusion logic) | excludeAncestral, excludeDescendant, excludeDirect, excludeMissing       |

### Tier 3: Enhancement (Nice-to-have for MVP)

| Gap                           | Old Code                       | New Status | Notes                                                                    |
| ----------------------------- | ------------------------------ | ---------- | ------------------------------------------------------------------------ |
| **Batch search (`/msearch`)** | `routes/msearch.js`            | 🔴 ~20%    | Query building there, but batch orchestration and response merge missing |
| **Pagination support**        | `routes/searchPaginated.js`    | 🟡 ~40%    | Offsets in query_builder; needs HTTP loop + result accumulation          |
| **Single record fetch**       | `routes/record.js`             | 🔴 0%      | Get record by taxon_id/assembly_id; trivial ES query                     |
| **Identifier lookup**         | `routes/lookup.js`             | 🔴 0%      | Map NCBItax IDs ↔ custom IDs; needs ID mapping index                     |
| **Result export formats**     | `functions/formatCsv.js`, etc. | 🔴 0%      | CSV/TSV/JSON streaming; separate from query building                     |

### Tier 4: Deferred (Out of scope for MVP)

| Gap                                 | Old Code                                                  | New Status | Notes                                                                                                             |
| ----------------------------------- | --------------------------------------------------------- | ---------- | ----------------------------------------------------------------------------------------------------------------- |
| **Report aggregations (`/report`)** | `routes/report.js` + `reports/*.js`                       | 🔴 0%      | Complex: histograms, scatter, tree, correlation; plan exists in docs/planning/api-aggregation-refactoring-plan.md |
| **Summary aggregations**            | `routes/summary.js`                                       | 🔴 0%      | Depends on `/resultFields` being stable                                                                           |
| **Config endpoints**                | `routes/indices.js`, `taxonomies.js`, `taxonomicRanks.js` | 🔴 0%      | Trivial; return static config; low priority                                                                       |
| **Download/streaming**              | `routes/download.js`                                      | 🔴 0%      | Dependent on export formatters                                                                                    |
| **Phylopic**                        | `routes/phylopic.js`                                      | 🔴 0%      | External API lookup; aesthetic, not essential                                                                     |
| **Progress tracking**               | `routes/progress.js`                                      | 🔴 0%      | Async job support; post-MVP                                                                                       |

---

## Function-by-Function Audit

### Query Building (mostly done ✅)

| Function              | Purpose                                        | Status  | Rust Implementation                                     |
| --------------------- | ---------------------------------------------- | ------- | ------------------------------------------------------- |
| Parse simple queries  | Convert `tax_rank(species)` → ES bool filter   | ✅ 80%  | `crate::core::query::adapter::parse_url_params()`       |
| Build count body      | ES `_count` query from query string            | ✅ 90%  | `query_builder::build_count_body()`                     |
| Build search body     | ES `_search` query with fields, filters, aggs  | 🟡 70%  | `query_builder::build_search_body()`                    |
| Attribute filters     | Convert `genome_size>=1e9` → nested bool query | 🟡 65%  | `query::Attribute` parsing; attribute operator handling |
| Synonym normalization | `gc_percent` → `gc_percentage` via attr_types  | ✅ Done | `attr_types.rs` builds synonym map                      |
| Field filtering       | Include/exclude fields in `_source`            | 🟡 50%  | Partial in query_builder; needs completeness pass       |
| Sorting               | Convert sort_by + sort_order → ES sort clause  | 🟡 70%  | Done but may miss edge cases (multi-key sort, etc.)     |
| Pagination            | from/size parameters                           | ✅ 85%  | Simple offset/size; HTTP loop needed for multi-page     |

### Response Processing (partial ✅)

| Function                 | Purpose                                                                                   | Status | Rust Implementation                     |
| ------------------------ | ----------------------------------------------------------------------------------------- | ------ | --------------------------------------- |
| Parse ES hits            | Convert raw ES result → flat record array                                                 | 🟡 60% | `process_hits.rs`                       |
| Flatten attributes       | `result.fields.genome_size` → `genome_size`, `genome_size_count`, `genome_size_min`, etc. | 🟡 60% | Implemented but may miss stat sub-keys  |
| Extract identity columns | taxon_id, scientific_name, taxon_rank, lineage                                            | 🟡 70% | Mostly done                             |
| Handle nested hits       | Inner hits from optional attributes                                                       | 🟡 40% | Code exists but untested for edge cases |
| Format for TSV/CSV       | Flatten JSON to rows                                                                      | 🔴 10% | No implementation                       |

### Metadata & Validation (critical gap 🔴)

| Function                     | Purpose                                                                | Status  | Details                                                                                       |
| ---------------------------- | ---------------------------------------------------------------------- | ------- | --------------------------------------------------------------------------------------------- |
| **Attribute type inference** | Query attributes index, extract types/synonyms                         | ✅ 95%  | `attr_types.rs` — but **no HTTP endpoint**                                                    |
| **Type mapping**             | Map ES field types → processed types (integer, keyword, date, etc.)    | ✅ Done | `attr_types.rs::set_processed_type()`                                                         |
| **Enum extraction**          | For keyword fields, extract allowed values                             | ✅ Done | In `attr_types.rs`                                                                            |
| **Field validation**         | Check user query against known fields + synonyms                       | 🟡 30%  | `src/core/validate.rs` exists but incomplete; doesn't integrate with `/resultFields` response |
| **Modifier validation**      | Check that modifiers (min, max, direct, etc.) are valid for field type | 🟡 20%  | Exists but minimal; needs completeness                                                        |
| **Response shaping**         | Convert attr_types output → API `/resultFields` response               | 🔴 0%   | **Critical missing piece**                                                                    |

### Old API Helper Functions (mostly redundant in new arch)

| Old Function       | Location                      | Purpose                             | New Status            | Notes                                                   |
| ------------------ | ----------------------------- | ----------------------------------- | --------------------- | ------------------------------------------------------- |
| `getResultCount()` | `functions/getResultCount.js` | Execute count query                 | ✅ Equivalent in Rust | `count_docs()` in count.rs                              |
| `getResults()`     | `functions/getResults.js`     | Execute search query + process hits | ✅ Equivalent in Rust | `query_builder::build_search_body()` + `process_hits()` |
| `processHits()`    | `functions/processHits.js`    | Convert ES hits → result format     | ✅ Equivalent in Rust | `process_hits.rs::process_hits()`                       |
| `attrTypes()`      | `functions/attrTypes.js`      | Query attributes index              | ✅ Equivalent in Rust | `attr_types.rs::attr_types()`                           |
| `setAggs()`        | `reports/setAggs.js`          | Build nested aggregations           | 🔴 Out of scope       | Report building deferred to Phase 2                     |
| `getBounds()`      | `reports/getBounds.js`        | Infer numeric bounds for histogram  | 🔴 Out of scope       | Report building deferred                                |
| `formatCsv()`      | `functions/formatCsv.js`      | Format results as CSV               | 🔴 Missing            | Export formatting not yet implemented                   |

---

## Implementation Priority List

### PHASE 1: Unblock Validation (Week 1-2)

**Goal:** Get `/resultFields` endpoint working so downstream code can validate fields.

1. ✅ **attr_types.rs** — Already exists; queries attributes index
2. 🔴 **New: result_fields.rs** — Wrap `attr_types()` with response formatting
   - Input: `result` (taxon/assembly/sample), `index` (attributes)
   - Output: `{ status, fields, identifiers, hub, release, source }`
   - Add TTL/caching logic
3. 🔴 **New: HTTP handler** — Wire to generated API
   - Endpoint: `GET /api/v2/resultFields?result={result}&index={index}`
   - Return JSON response
4. ✅ **Tests:** Unit test result_fields formatting; integration test with live ES

**Acceptance:** `curl http://localhost:9200/api/v2/resultFields?result=taxon` returns field list with types.

---

### PHASE 2: Core Search (Week 2-3)

**Goal:** `/search` endpoint fully working with parity to old API.

1. ✅ **query_builder.rs** — Already ~70% done; audit for gaps:
   - Test all attribute operators (=, !=, <, >, <=, >=, in, not in, range)
   - Test field inclusion/exclusion edge cases
   - Test sorting with multiple keys
   - Test pagination with large offsets
2. 🟡 **process_hits.rs** — Refine hit processing:
   - Validate all stat sub-keys are extracted (min, max, median, mode, mean, count)
   - Handle inner_hits correctly for optional attributes
   - Test with real ES responses
3. 🔴 **New: HTTP handler** — Wire `/search` endpoint
   - Input: query params (query, result, fields, attributes, sort_by, sort_order, size, offset, ...)
   - Output: `{ status, count, hits, total }`
4. ✅ **Validation:** Integrate `/resultFields` so validation works

**Acceptance:** `curl http://localhost:9200/api/v2/search?result=taxon&query=...` returns results matching old API.

---

### PHASE 3: Count (Week 1, parallel with Phase 1)

**Goal:** `/count` endpoint working.

1. ✅ **query_builder::build_count_body()** — Already ~90% done
2. 🔴 **New: HTTP handler** — Wire `/count` endpoint
   - Input: query params (query, result, ...)
   - Output: `{ status, count }`
3. ✅ **Tests:** Unit + integration

**Acceptance:** `curl http://localhost:9200/api/v2/count?result=taxon&query=...` returns count.

---

### PHASE 4: Batch & Pagination (Week 3)

**Goal:** `/msearch` and `/searchPaginated` endpoints.

1. 🔴 **msearch.rs** — Batch search orchestration
   - Input: array of search requests
   - For each: build query, post to ES, collect responses
   - Output: merge results
2. 🔴 **pagination helper** — HTTP loop for multi-page results
   - Caller specifies max_total_hits
   - Loop: fetch page, append, increment offset until exhausted

**Acceptance:** Batch 3 searches in one POST; get 3 result sets back.

---

### PHASE 5: Metadata/Config Endpoints (Week 3)

**Goal:** Trivial endpoints that users need (indices, taxonomies, ranks).

1. 🔴 **indices.rs** — List available indices
   - Return: array of index names from config
   - HTTP: `GET /api/v2/indices`
2. 🔴 **taxonomies.rs** — List available taxonomies
   - Return: array of taxonomy names from config
   - HTTP: `GET /api/v2/taxonomies`
3. 🔴 **ranks.rs** — List available ranks
   - Return: array of rank names (NCBI: kingdom, phylum, class, order, family, genus, species, ...)
   - HTTP: `GET /api/v2/taxonomicRanks?taxonomy=ncbi`

**Acceptance:** 3 simple GET endpoints work.

---

### PHASE 6: Single Record + Lookup (Week 4)

**Goal:** Point-access endpoints for specific records or IDs.

1. 🔴 **record.rs** — Get single record by ID
   - Input: taxon_id or assembly_id
   - Output: full record with all fields
   - HTTP: `GET /api/v2/record?id={id}&result={result}`
2. 🔴 **lookup.rs** — Resolve alternate identifiers
   - Input: ID in one namespace (e.g. NCBItax:9606)
   - Output: equivalent IDs in other namespaces (custom_id:X, ncbi_name:Homo sapiens, ...)
   - Requires: ID mapping index

**Acceptance:** Fetch one record by ID; lookup one ID → other IDs.

---

### PHASE 7: Export Formats (Week 4)

**Goal:** CSV/TSV/JSON export.

1. 🔴 **formatters.rs** — Convert flat records to CSV/TSV/JSON
   - Input: `Vec<FlatRecord>`, field list, format choice
   - Output: string (CSV) or JSON
2. 🔴 **New: HTTP handler** — Wire `/download` endpoint or add format param to `/search`
   - Input: search params + `format=csv|tsv|json`
   - Output: formatted string + attachment header

**Acceptance:** `curl http://localhost:9200/api/v2/search?...&format=csv` returns CSV with header.

---

### PHASE 8+: Deferred (Post-MVP)

**NOT tackling now per your focus:**

- `/report` aggregation reports (histograms, scatter, tree, correlation)
- `/summary` aggregations
- Async job support (`/progress`)
- Phylopic lookup

**Reference:** `docs/planning/api-aggregation-refactoring-plan.md` has full roadmap for reports.

---

## Code Structure: What Needs Creating/Refining

### New Modules to Create

```
src/core/result_fields.rs      — Wrap attr_types() with response formatting
src/core/record.rs             — Single record fetch by ID
src/core/lookup.rs             — ID resolution
src/core/formatters.rs         — CSV/TSV/JSON export
src/core/msearch.rs            — Batch search orchestration
src/core/config_endpoints.rs   — indices, taxonomies, ranks
```

### Refinement Needed

```
src/core/query_builder.rs      — Audit all operators; edge case testing
src/core/process_hits.rs       — Stat sub-keys completeness; inner_hits correctness
src/core/attr_types.rs         — Already good; just needs /resultFields endpoint wiring
src/core/validate.rs           — Incomplete; needs /resultFields integration
```

### HTTP Wiring (in generated API)

Each of the above modules needs a handler route in the generated API:

- `src/routes/{module_name}.rs` or integrated into main handler

---

## Validation & Exclusion Filters: Key Gaps

**Old API supports excluding records by data source:**

```
excludeAncestral=[genome_size,c_value]  → Exclude records with ancestrally-derived estimates
excludeDescendant=[genome_size]         → Exclude records with descendant-derived estimates
excludeDirect=[...]                     → Exclude records with directly-measured values
excludeMissing=[...]                    → Exclude records with no value
```

**Current status:** ❌ Not implemented in new query builder.

**Implementation approach:**

1. Parse exclusion lists from URL params
2. For each attribute in ES query, add negative bool.should clauses that filter out matching `_source.reason` values
3. Requires: ES schema knowledge of how reasons are stored (check live API)

---

## Testing Strategy

### Unit Tests (by module)

```
tests/test_result_fields.rs        — attr_types() → response formatting
tests/test_query_builder.rs        — All operators, edge cases
tests/test_process_hits.rs         — ES hit conversion
tests/test_record.rs               — Single record fetch
tests/test_lookup.rs               — ID resolution
```

### Integration Tests

```
tests/test_api_endpoints.rs        — HTTP handlers + live ES
tests/test_search_parity.rs        — Old API vs new builder output (golden tests)
```

### End-to-End (via live_query_demo or manual)

- Start ES with test data
- Call `/count`, `/search`, `/resultFields`
- Verify output matches old API

---

## FAQ & Known Unknowns

**Q: Why is `/resultFields` TOP PRIORITY if it's "just" metadata serving?**
A: Because it unblocks field validation, which is required for:

- User query validation (SDK validate() method)
- Synonyms (so `gc_percent` is normalized to `gc_percentage`)
- Modifiers (so users know which fields support `min`, `max`, etc.)
- Enum values (for UI dropdowns or validation)
  Without it, queries silently fail at ES time instead of failing at validation time.

**Q: Why skip reports for now?**
A: Reports (histograms, scatter, tree) are 3-5x more complex than search:

- Nested aggregation DSL building (not just simple bool filters)
- Scale inference (log, sqrt, ordinal)
- Binning logic (Freedman-Diaconis, Sturges, etc.)
- Response reshaping (bucket → chart format)
  A full plan exists in docs/planning/api-aggregation-refactoring-plan.md; will tackle after MVP.

**Q: How does pagination work in Rust vs old API?**
A: Old API has `/searchPaginated` which is HTTP; new builder returns from/size in ES query.
SDK (Python/JS/R) owns the HTTP loop; Rust provides per-page parsing only.

**Q: What about the circular FieldFetcher dependency?**
A: Pre-MVP workaround: Users start the API first, then run cli-generator. Post-MVP: decouple in Phase 6.
Not a blocker for this audit.

---

## Summary Table: What to Do When

| Time        | Task                                | File                                   | Impact              |
| ----------- | ----------------------------------- | -------------------------------------- | ------------------- |
| **Now**     | Wrap attr_types() for /resultFields | src/core/result_fields.rs              | Unblocks validation |
| **Now**     | Audit query_builder for gaps        | src/core/query_builder.rs              | Finds edge cases    |
| **Week 1**  | HTTP /count handler                 | generated routes                       | MVP baseline        |
| **Week 2**  | HTTP /search handler                | generated routes                       | Core search         |
| **Week 2**  | Refine process_hits                 | src/core/process_hits.rs               | Correctness         |
| **Week 3**  | Batch search (/msearch)             | src/core/msearch.rs                    | Efficiency          |
| **Week 3**  | Config endpoints                    | src/core/config_endpoints.rs           | Completeness        |
| **Week 4+** | Record + Lookup + Export            | src/core/{record,lookup,formatters}.rs | Nice-to-haves       |
