# Executive Summary: Old API vs Query Builder Audit

**Prepared:** 2026-04-29
**Status:** Ready to implement. **Blocker identified.** Path clear for MVP.

---

## The One Thing Blocking Everything Else

### 🔴 **Critical Blocker: `/resultFields` HTTP Endpoint**

You have `attr_types()` function in Rust that queries the ES attributes index and extracts field metadata (types, synonyms, modifiers). **But there's no HTTP endpoint exposing this.**

**Why this matters:**

- Query validation (SDK `validate()` method) needs to know valid field names
- Synonyms aren't normalized (user says `gc_percent`, API expects `gc_percentage`)
- Modifiers validation fails silently (users ask for `genome_size[min]` on non-aggregatable fields)
- Downstream code has no way to know enum values or field constraints

**The fix:**

1. Wrap `attr_types()` with response formatting (1–2 days)
2. Wire `GET /api/v2/resultFields?result=taxon` (1 day)
3. Add caching/TTL (1 day)
4. **Result:** Unblocks validation, field processing, describe/snippet generation

---

## What You Have (40% Done) ✅

| Component            | Status | What Works                                                        |
| -------------------- | ------ | ----------------------------------------------------------------- |
| **Query building**   | 70%    | Count/search body builders; basic operators; pagination           |
| **Response parsing** | 60%    | Flattens ES hits; extracts identity fields; handles stat sub-keys |
| **Field metadata**   | 95%    | `attr_types()` queries ES; builds type & synonym maps             |
| **Attribute query**  | 80%    | Parses `genome_size>=1e9` into bool filters                       |
| **Sorting**          | 70%    | Sort by field + order                                             |
| **Validation**       | 30%    | Code exists but incomplete; not integrated with `/resultFields`   |

---

## What You Don't Have (Needs Implementing)

| Component                                         | Priority    | Effort                        | Impact                              |
| ------------------------------------------------- | ----------- | ----------------------------- | ----------------------------------- |
| **HTTP endpoint wiring**                          | 🔴 Critical | 3-5 days                      | Without it, no API at all           |
| **Exclusion filters** (excludeAncestral, etc.)    | 🟡 High     | 2-3 days                      | Data quality; users expect this     |
| **Batch search (`/msearch`)**                     | 🟡 High     | 2-3 days                      | Performance; users want parallelism |
| **Export formats** (CSV/TSV)                      | 🟡 High     | 3-4 days                      | Data accessibility                  |
| **Single record fetch** (`/record`)               | 🟢 Medium   | 1-2 days                      | Pointwise access                    |
| **ID lookup** (`/lookup`)                         | 🟢 Medium   | 2-3 days (if ID index exists) | ID translation                      |
| **Config endpoints** (indices, ranks, taxonomies) | 🟢 Medium   | 1 day                         | Trivia; nice-to-have                |
| **Report aggregations** (`/report`)               | 🔵 Deferred | 3-4 weeks                     | Out of MVP scope                    |

---

## MVP Path (2-3 weeks)

### Week 1: Unblock + Foundation

- Audit query_builder.rs for gaps (operator coverage, edge cases)
- Create result_fields.rs (wrap attr_types, format response)
- Wire `/resultFields` HTTP endpoint
- Wire `/count` HTTP endpoint
- **Checkpoint:** Can fetch field metadata; can count results

### Week 2: Core Search

- Refine process_hits.rs (stat sub-keys, inner_hits correctness)
- Wire `/search` HTTP endpoint
- Integrate validation (field lookup, synonym normalization)
- Integration tests: old API vs new builder output
- **Checkpoint:** Can search with results; results match old API shape

### Week 3: Enhancements

- Batch search (`/msearch`)
- Pagination support (HTTP loop)
- Exclusion filters (if not done earlier)
- CSV/TSV export
- **Checkpoint:** Performance and data accessibility features working

---

## Gap Analysis: Operators & Features

### Attribute Filters (Query Building)

**Status:** ~70% — Most operators likely work, but untested

| Operator     | Example                                   | Status         | Notes                          |
| ------------ | ----------------------------------------- | -------------- | ------------------------------ |
| Equals       | `genome_size=1e9`                         | ✅ Likely done | Simple term match              |
| Not equals   | `genome_size!=1e9`                        | 🤔 Unknown     | Neg bool query                 |
| Greater-than | `genome_size>1e9`                         | ✅ Likely done | Range query                    |
| Less-than    | `genome_size<5e9`                         | ✅ Likely done | Range query                    |
| Range        | `genome_size:1e9..5e9`                    | 🤔 Unknown     | Between two values             |
| Array in     | `assembly_level in [chromosome,scaffold]` | 🤔 Unknown     | Multiple terms                 |
| Array not-in | `assembly_level not in [contig]`          | 🤔 Unknown     | Neg terms                      |
| Exists       | `has genome_size`                         | 🤔 Unknown     | Exists query                   |
| Missing      | `not has genome_size`                     | 🤔 Unknown     | Exists=false                   |
| Modifiers    | `genome_size[min,direct]`                 | 🤔 Unknown     | Nested path + source filtering |

**Action:** Test each operator; document failures; fix or defer to non-MVP.

### Exclusion Filters (Data Source)

**Status:** 🔴 **Unknown if implemented at all**

The old API supports:

```
excludeAncestral=[genome_size]    → skip if genome_size is ancestrally derived
excludeDescendant=[genome_size]   → skip if genome_size from descendant taxa
excludeDirect=[genome_size]       → skip if directly measured
excludeMissing=[genome_size]      → skip if no value
```

**Action:** Check if reason field is in ES docs; audit query_builder for exclusion logic; implement if missing.

---

## Key Implementation Details

### `/resultFields` Response Shape

**Old API output:**

```json
{
  "status": { "success": true },
  "fields": {
    "genome_size": {
      "type": "long",
      "display_name": "Genome size",
      "processed_type": "float",
      "summary": ["min", "max", "median"],
      "synonyms": ["gc_percent"]
    },
    ...
  },
  "identifiers": { ... },
  "hub": "genomehubs",
  "release": "2026-04-28",
  "source": "NCBI"
}
```

**Rust implementation:**

- Call `attr_types(es_base, "attributes", "taxon")`
- Returns: `(TypesMap, SynonymsMap)`
- Format into JSON response above
- Add 24-hour cache on ES query

### Query Building Workflow

Current → New:

```
User query: "genome_size>=1e9 and assembly_level=chromosome"

1. Parse query string (done: query::adapter::parse_url_params)
2. Normalize field names via synonyms (done: attr_types provides map)
3. Validate operators + field types (partial: validate.rs exists but incomplete)
4. Build ES bool.filter clauses (done: query_builder::build_search_body)
5. Add nested attributes query if present (done: query_builder)
6. Add sorting, pagination (done: query_builder)
7. Submit to ES (not in Rust; HTTP handler does this)
8. Parse ES response (done: process_hits.rs)
9. Return flattened results (done: process_hits.rs)
```

**Gaps to fill:**

- Validation completeness
- Exclusion filter logic
- HTTP handler wiring

---

## Old API Endpoints Inventory

| Endpoint            | Purpose                                                 | Complexity    | Status                                                      |
| ------------------- | ------------------------------------------------------- | ------------- | ----------------------------------------------------------- |
| `/count`            | Count results matching query                            | Low           | ✅ ~80% (ES query building done, needs HTTP wiring)         |
| `/search`           | Search with field results                               | Medium        | 🟡 ~50% (query building done, response parsing needs work)  |
| `/msearch`          | Batch multiple searches                                 | Medium        | 🔴 ~10% (query building there, batch orchestration missing) |
| `/searchPaginated`  | Paginated search                                        | Medium        | 🔴 ~20% (offsets in query_builder, needs HTTP loop)         |
| **`/resultFields`** | **CRITICAL — Get field metadata from attributes index** | High          | 🔴 ~5% (**attr_types queries ES, but no HTTP endpoint**)    |
| `/record`           | Get single record by ID                                 | Low           | 🔴 0% (not started)                                         |
| `/lookup`           | Resolve alternate identifiers                           | Low           | 🔴 0% (not started)                                         |
| `/download`         | Export results as CSV/TSV/JSON                          | Medium        | 🔴 0% (needs formatters + streaming)                        |
| `/report`           | Complex aggregation reports                             | **Very High** | 🔴 0% (out of scope for now per plan)                       |
| `/summary`          | Summary aggregations                                    | High          | 🔴 0% (depends on `/resultFields`)                          |
| `/indices`          | List available indices                                  | Low           | 🔴 0% (trivial; return from config)                         |
| `/taxonomies`       | List available taxonomies                               | Low           | 🔴 0% (trivial; return from config)                         |
| `/taxonomicRanks`   | List available ranks                                    | Low           | 🔴 0% (trivial; return from config)                         |
| `/phylopic`         | Lookup phylopic image URLs                              | Very Low      | 🔴 0% (external API; low priority)                          |
| `/progress`         | Job progress tracking                                   | Low           | 🔴 0% (async job support; deferred)                         |

---

## Recommended Immediate Actions (Next 48 Hours)

1. **Audit query_builder.rs**
   - Test each attribute operator with real ES data
   - Document which operators work, which fail
   - Identify quick wins (1-line fixes) vs. bigger refactors

2. **Compare process_hits.rs to old processHits.js**
   - Line up stat sub-key extraction logic
   - Identify any missing transformations
   - Test with real ES response (run old `/search` endpoint, pass response to new parser)

3. **Create result_fields.rs stub**
   - Wrap `attr_types()` → JSON response
   - Verify shape matches old API
   - No HTTP handler yet; just the formatter function

4. **Identify exclusion filter gaps**
   - Check if `reason` field exists in ES documents
   - Determine how it's structured (nested? parallel?)
   - Decide: implement in MVP or defer?

---

## High-Risk Unknowns

| Unknown                       | Impact | How to Resolve                                                  |
| ----------------------------- | ------ | --------------------------------------------------------------- |
| **Exclusion filter logic**    | High   | Query live ES; get sample doc with reason field                 |
| **Query operator coverage**   | High   | Test each operator against live ES                              |
| **Stat sub-key completeness** | Medium | Compare old process_hits.js to new process_hits.rs line-by-line |
| **Nested attribute handling** | Medium | Run test with optional fields; inspect inner_hits parsing       |
| **Performance at scale**      | Low    | Benchmark: 100k documents, 50 fields                            |

---

## Success Criteria for MVP

✅ `/count` endpoint returns correct count for any valid query
✅ `/search` endpoint returns results matching old API shape
✅ `/resultFields` endpoint returns field metadata with types + synonyms
✅ Validation works: unknown field → error; synonym → normalized
✅ All tests pass; zero silent failures (errors bubble up)
✅ Parity tests: old API vs new builder output (random 10 queries)

---

## What's NOT Included (Defer Post-MVP)

- ❌ Report aggregations (histogram, scatter, tree, correlation)
- ❌ Async job support (for long-running reports)
- ❌ Summary aggregations (requires reports)
- ❌ Phylopic lookup (external API; aesthetic)
- ❌ Browser WASM optimization (JS only; not urgent)

---

## Next Steps

1. **Review this audit** — Do the priorities and gaps match your mental model?
2. **Read the detailed audit** (`docs/api-audit-and-migration.md`)
3. **Run the action checklist** (`docs/api-audit-action-checklist.md`)
4. **Start Week 1:** Audit query_builder.rs + create result_fields.rs

See detailed documentation for comprehensive information on each endpoint, implementation details, and step-by-step guidance.
