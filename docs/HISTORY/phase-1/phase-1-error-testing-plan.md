# Phase 1: Error Scenarios + Property-Based Tests

**Objective:** Add error path coverage and property-based invariant tests to push both Rust and Python >90%.

**Current Baseline (Phase 0.5 end):**

- Rust: 90.39% (818/905 lines) — 87 uncovered lines
- Python: 90.59% (143/152 stmts) — 9 uncovered statements

**Target:** Stay >90% while validating error paths and property invariants

---

## Rust: Error Scenarios & Property Tests

### Module 1: `src/core/fetch.rs` (14 uncovered lines)

**Coverage gaps:**

- Lines 162-164, 166-167, 169: Cache file deserialization errors
- Lines 184, 197: Parse errors and missing fields
- Lines 219-221, 223, 225-226: JSON parse failures

**Test strategy:**

1. **HTTP Error Scenarios (5xx + malformed JSON):**
   - Mock 5xx response → expect `error_for_status()` error
   - Mock invalid JSON body → expect JSON deserialization error
   - Mock missing "fields" object → expect parse error in `parse_result_fields`

2. **Cache File Errors:**
   - Corrupt JSON in cache file → `load_cache` returns error
   - Invalid UTF-8 in cache → `read_to_string` fails
   - Permission denied writing cache → `write_cache` errors

3. **Parser Edge Cases:**
   - Empty fields object `{ "fields": {} }`
   - Fields with missing required properties
   - Non-object field entries (arrays, nulls)

**Recommended approach:**

- Use `mockito` crate to mock HTTP responses (200 OK, 500, 502, malformed bodies)
- Use `tempfile` for cache corruption scenarios
- Keep tests in `src/core/fetch.rs::tests` module

**Lines to cover:** 14 → expected 0 missing = **+14 lines**

---

### Module 2: `src/core/query/validation.rs` (26 uncovered lines)

**Coverage gaps:**
Lines 177-179, 230, 280, 286, 288-291, 293, 296, 298-299, 302, 356-359, 376, 385-387, 389-391

**Error variants not reached:**

- `ValidationError::InvalidRangeOperatorOnKeyword` (lines 70-76)
- `ValidationError::InvalidRangeValue` (line 79)
- `ValidationError::InvalidAssemblyAccession` (line 88)
- `ValidationError::InvalidSampleAccession` (line 91)
- `ValidationError::InvalidTaxonNameClass` (line 94)
- `ValidationError::InvalidTaxonFilterType` (line 97)

**Test strategy:**

1. **Validate unsupported operators on keyword fields:**

   ```rust
   // Keyword field (e.g., "assembly_level") with < operator should error
   let mut attr = Attribute { name: "assembly_level", operator: Some(Lt), ... };
   assert!(validate_query(...).contains(InvalidRangeOperatorOnKeyword));
   ```

2. **Validate range value parsing:**
   - Non-numeric value for range (e.g., "abc" for `<` on numeric field)
   - Out-of-bounds numeric values

3. **Test accession prefix validation:**
   - Invalid assembly prefix (not GCA*, GCF*, etc.)
   - Invalid sample prefix (not SRS*, SRX*, etc.)

4. **Test enum constraints:**
   - Non-enum value for constrained keyword field
   - Out-of-bounds enum indices

5. **Test taxonomic filter types:**
   - Invalid `taxon_filter_type` value
   - Invalid taxon name class (must be one of defined categories)

**Lines to cover:** 26 → expected 0 missing = **+26 lines**

---

### Module 3: `src/core/query/url.rs` (11 uncovered lines)

**Coverage gaps:** Lines 137, 141, 182, 204, 208-209, 213, 229, 335, 342, 369

**Test strategy:** Property-based tests using `proptest`

**Property invariant: Query URL round-trip**

```rust
// For any valid SearchQuery:
// encode(query) → decode(encoded_query) → query_recreated
// assert!(original == recreated)
```

**Property generators needed:**

1. Generate valid `SearchQuery` instances with:
   - Valid operator/value combinations (>0 taxa only for valid operators)
   - Valid field names and modifiers from metadata
   - Valid enum values for constrained fields

2. Generate edge cases:
   - Empty taxa list (should fail at validation, but test the path)
   - Special characters in values (quotes, slashes, etc.)
   - Very long field values (test encoding length)
   - Unicode characters in taxa

**Recommended approach:**

- Use `proptest::proptest!` macro to auto-shrink failures
- Define `arb_SearchQuery()` generator that respects field metadata
- Property: `encoded_url != original_query_string but encodes all data`

**Lines to cover:** 11 → expected 0 missing = **+11 lines**

---

### Module 4: `src/core/query/attributes.rs` (10 uncovered lines)

**Coverage gaps:** Lines 208-213, 215-218

**Test strategy:** Property-based value normalization

**Property invariant: Value normalization idempotence**

```rust
// For any AttributeValue v:
// v.as_strs() always returns same result on repeated calls
// normalize(v) is idempotent
```

**Edge cases to test:**

- List values with empty strings
- List values with duplicate entries
- Mixed case keywords (if applicable)
- Numeric values near int boundaries

---

## Python: Error Scenarios & Property Tests

### Module: `python/cli_generator/query.py` (9 uncovered statements)

**Coverage gaps:**

- QueryBuilder method branches (e.g., early returns for None)
- YAML serialization edge cases
- Invalid state transitions

**Test strategy:**

1. **QueryBuilder edge cases:**

   ```python
   # Test method chaining with None values
   qb = QueryBuilder("taxon")
   result = qb.assemblies(None)  # Should not add to state

   # Test rank/sample/etc with empty lists
   qb.ranks([])  # Should handle gracefully

   # Test setting same property twice (should overwrite)
   qb.samples(["S1"]).samples(["S2"])  # Second call wins
   ```

2. **Property invariants using Hypothesis:**

   ```python
   @given(taxa=lists_of_valid_taxa())
   def test_querybuilder_taxa_always_serializable(taxa):
       qb = QueryBuilder("taxon").taxa(taxa)
       yaml_str = qb.to_yaml()
       assert "taxa:" in yaml_str
       # Can reconstruct from YAML
   ```

3. **YAML parsing edge cases:**
   - Very long taxon lists (>1000 entries)
   - Special characters in values
   - Duplicate taxa (should be deduplicated or preserved?)
   - Whitespace normalization

---

## Implementation Order

**Week 1:**

1. ✅ Identify all error paths (DONE — this doc)
2. [ ] Implement `mockito`-based HTTP error tests in fetch.rs
3. [ ] Implement validation error tests (26 lines)

**Week 2:** 4. [ ] Implement proptest generators for SearchQuery 5. [ ] Add round-trip property tests for url.rs

**Week 3:** 6. [ ] Add attributes.rs value normalization properties 7. [ ] Implement Python error scenario tests 8. [ ] Add Hypothesis property tests for QueryBuilder

**Week 4:** 9. [ ] Final coverage measurement 10. [ ] Create agent log documenting Phase 1

---

## Expected Outcomes

| Module             | Current       | Target          | Method                           |
| ------------------ | ------------- | --------------- | -------------------------------- |
| fetch.rs           | 79% (54/68)   | 100%            | 5 HTTP error + 3 cache scenarios |
| validation.rs      | 80% (106/132) | 100%            | 12 error variant tests           |
| url.rs             | 91% (116/127) | 100%            | proptest round-trip generator    |
| attributes.rs      | 70% (23/33)   | 100%            | proptest normalization           |
| query.py           | 90.45%        | 95%+            | Hypothesis edge cases            |
| **Overall Rust**   | 90.39%        | **95%+ target** | —                                |
| **Overall Python** | 90.59%        | **92%+ target** | —                                |

---

## Testing Tools Setup

**Already available:**

- ✅ `proptest` (Rust) — installed in dev dependencies
- ✅ `hypothesis` (Python) — installed as `hypothesis[cli]`
- ✅ `tempfile` (Rust) — for file I/O testing

**To add:**

- [ ] `mockito` (Rust HTTP mocking) — add to `Cargo.toml`
- [ ] `pytest-mock` (Python) — add to pyproject.toml if needed

---

## Coverage Verification

After each module:

```bash
# Rust
cargo tarpaulin --timeout 300 2>&1 | tail -5

# Python
bash scripts/measure_coverage.sh
```

---
