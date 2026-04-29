# Action Checklist: ES Query Builder → API Migration

**Goal:** Ship working `/count`, `/search`, `/resultFields` endpoints with parity to old API.

---

## 🔴 BLOCKER: `/resultFields` HTTP Endpoint

This MUST complete first because it unblocks downstream validation.

- [ ] **Audit attr_types.rs output format**
  - Ensure TypesMap, SynonymsMap capture all needed metadata
  - Verify processed_type and processed_summary fields are correct
  - Check for any missing fields (e.g., display_group, display_name, constraints)

- [ ] **Create result_fields.rs**
  - Function: `format_result_fields_response(types_map, synonyms_map) -> Value`
  - Output shape: `{ status: {success: true}, fields: {...}, identifiers: {...}, hub, release, source }`
  - Add TTL/cache logic (24-hour cache on attr_types ES query)

- [ ] **Wire HTTP handler**
  - Endpoint: `GET /api/v2/resultFields?result={taxon|assembly|sample}&index=attributes`
  - Call `attr_types()` → `format_result_fields_response()` → JSON
  - Add error handling (ES down, attributes index missing, etc.)

- [ ] **Test with live ES**
  - Manually: `curl http://localhost:9200/api/v2/resultFields?result=taxon`
  - Verify output structure, field count, type mappings
  - Compare to old API output

---

## 🟡 HIGH: Count & Search Query Building (Audit + Refinement)

### query_builder.rs Audit

- [ ] **All attribute operators**
  - [ ] Equality: `genome_size=1e9` ✅ (likely working)
  - [ ] Inequality: `genome_size!=1e9` 🤔
  - [ ] Range: `genome_size>=1e9` ✅
  - [ ] Less-than: `genome_size<5e9` ✅
  - [ ] Array match: `assembly_level in [chromosome,scaffold]` 🤔
  - [ ] NOT in: `assembly_level not in [contig]` 🤔
  - [ ] Between/range: `genome_size:1e9..5e9` 🤔
  - Create test case for each; document any failures

- [ ] **Taxon filter combinations**
  - [ ] Single taxon: `tax_rank(species)`
  - [ ] Multiple taxa: `tax_rank(species) OR tax_rank(genus)` (if supported)
  - [ ] Lineage search: `tax_name(Mammalia)`
  - [ ] Taxonomic rank filtering

- [ ] **Field projection edge cases**
  - [ ] Empty fields list (all required fields only)
  - [ ] Optional fields not in required set (should add via optional_fields)
  - [ ] Synonyms in field list (should normalize via attr_types)
  - [ ] Non-existent field (should error or warn)

- [ ] **Sorting edge cases**
  - [ ] Sort by multiple fields: `sort_by=genome_size,scientific_name sort_order=desc,asc` (if supported)
  - [ ] Sort by score (relevance)
  - [ ] Sort by nested attribute (e.g., `genome_size.min`)

- [ ] **Pagination**
  - [ ] Small offset (size=10, from=0)
  - [ ] Large offset (size=100, from=100000)
  - [ ] Offset > total results
  - [ ] Verify ES respects from/size correctly

### process_hits.rs Refinement

- [ ] **Stat sub-keys extraction**
  - [ ] For each field, extract: value, count, min, max, median, mode, mean, std_dev
  - [ ] Only emit keys present on raw object (schema flexibility)
  - [ ] Test with real ES response (get from `/search` endpoint on live system)

- [ ] **Inner hits (optional attributes)**
  - [ ] When present, merge into result.result.fields correctly
  - [ ] Test with mixed required + optional fields

- [ ] **Identity columns**
  - [ ] taxon_id, scientific_name, taxon_rank always present
  - [ ] lineage columns when requested
  - [ ] Handle missing fields gracefully (null or omit?)

- [ ] **Reason field (data source tracking)**
  - [ ] Parse reason: { source: "direct" | "ancestor" | "descendant", ... }
  - [ ] Emit as `{field}_source` in flattened output

---

## 🟢 MEDIUM: HTTP Endpoint Wiring

### /count Endpoint

- [ ] Create handler: POST `/api/v2/count`
  - Input: JSON body with `query`, `result`, `taxonomy` (optional)
  - Call: `build_count_body()` → ES `_count` → extract count
  - Output: `{ status: {success: true, hits: N}, count: N }`

- [ ] Test: `curl -X POST http://localhost:9200/api/v2/count -d '{...}'`

### /search Endpoint

- [ ] Create handler: POST `/api/v2/search`
  - Input: query params + optional JSON body
  - Call: `build_search_body()` → ES `_search` → `process_hits()`
  - Output: `{ status: {success: true, count, hits, total}, results: [...] }`

- [ ] Handle pagination:
  - Input: size, offset (or page, limit)
  - Output: total, returned_count, results

- [ ] Test: `curl -X POST http://localhost:9200/api/v2/search -d '{...}'`

### /resultFields Endpoint (see blocker section)

---

## 🔵 LOWER: Validation Integration

- [ ] Wire `/resultFields` into SDK validation
  - [ ] Fetch field metadata at query build time
  - [ ] Cache locally (per QueryBuilder instance, or global)
  - [ ] Check user input against known fields + synonyms
  - [ ] Check modifiers against field type
  - [ ] Return validation errors to user

- [ ] Test: `qb = QueryBuilder('taxon').add_attribute('unknown_field', ...); qb.validate()` → error

---

## 📋 Exclusion Filters (Arch Decision Needed)

**Status: Unknown if currently implemented**

- [ ] Audit old API for exact exclusion filter logic:
  - [ ] `excludeAncestral` — exclude records where genome_size is ancestrally derived
  - [ ] `excludeDescendant` — exclude records where genome_size is descendant-derived
  - [ ] `excludeDirect` — exclude records with directly measured genome_size
  - [ ] `excludeMissing` — exclude records with no genome_size value

- [ ] Check: How is "reason" (data source) stored in ES?
  - Nested `reason` field? Separate attribute field? Embedded in value object?
  - Get sample ES doc from live API

- [ ] Implement in query_builder if not present:
  - Add exclusion_filters to build_search_body signature
  - Build appropriate neg bool.should clauses

---

## 🚀 Quick Wins (After Core is Solid)

- [ ] `/indices` — Return list of index names from config
- [ ] `/taxonomies` — Return list of taxonomy names
- [ ] `/taxonomicRanks?taxonomy=ncbi` — Return rank list

- [ ] `/record?id={taxon_id}&result=taxon` — Fetch single record by ID

- [ ] `/lookup?id={id}` — Resolve alternate IDs (low priority if ID index doesn't exist)

---

## 🧪 Testing Checklist

### Unit Tests

```bash
cargo test --lib query_builder::tests
cargo test --lib process_hits::tests
cargo test --lib result_fields::tests
```

### Integration Tests (with live ES)

```bash
./scripts/validate_artifacts.sh --deep ./artifacts  # Existing test harness
# OR manual:
cargo run -- count --query "tax_rank(species)" > output.json
cargo run -- search --query "tax_rank(species)" --fields "genome_size" > output.json
```

### End-to-End (Old API Parity)

- [ ] Start both old API (node) and new API (Rust) on different ports
- [ ] Send identical queries to both
- [ ] Compare output structure, count, field names
- [ ] Document any differences (acceptable breaking changes vs. bugs)

---

## 📅 Suggested Timeline

| Week        | Focus                                         | Output                             |
| ----------- | --------------------------------------------- | ---------------------------------- |
| **Week 1**  | Audit query_builder, create result_fields.rs  | Documented gaps, blocker unblocked |
| **Week 1**  | HTTP wiring: /count, /resultFields            | 2 endpoints working                |
| **Week 2**  | process_hits refinement, /search HTTP handler | /search endpoint working           |
| **Week 2**  | Integration tests, parity validation          | Confidence in output correctness   |
| **Week 3+** | Batch search, pagination, export formats      | Nice-to-haves                      |

---

## 📝 Files to Touch (Checklist)

### New Files

- [ ] `src/core/result_fields.rs`
- [ ] `src/core/msearch.rs` (optional, week 3)
- [ ] `tests/test_query_builder_operators.rs`
- [ ] `tests/test_result_fields_format.rs`

### Modify

- [ ] `src/core/query_builder.rs` (audit + fixes)
- [ ] `src/core/process_hits.rs` (refinement)
- [ ] `src/core/attr_types.rs` (minor if needed)
- [ ] `src/core/validate.rs` (integrate /resultFields)

### HTTP Routing (generated code)

- [ ] Wire handlers for /count, /search, /resultFields

---

## Risks & Mitigations

| Risk                                                   | Probability | Mitigation                                     |
| ------------------------------------------------------ | ----------- | ---------------------------------------------- |
| Query builder doesn't handle all operators             | Medium      | Audit against old API; test each operator      |
| process_hits misses edge cases (stat keys, inner_hits) | Medium      | Golden tests comparing old vs new output       |
| /resultFields response shape mismatch                  | Low         | Compare old API JSON structure line-by-line    |
| ES schema changes between versions                     | Low         | Document assumed ES version; add version check |
| Performance regression (query building slower)         | Low         | Benchmark against old API; profile if slow     |
