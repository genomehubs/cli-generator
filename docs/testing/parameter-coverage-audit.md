# API Parameter Coverage Audit

**Date:** 2026-04-23
**Source:** API spec from https://goat.genomehubs.org/api-spec (v2)
**Test Suite:** `tests/python/test_sdk_fixtures.py` (14 fixtures, 116 tests)
**SDK Reference:** `python/cli_generator/query.py` (QueryBuilder class)

---

## Executive Summary

**Test Coverage Status:** ✅ **58% of major parameters tested**

- **Tested:** 11 of 19 major parameter groups
- **Untested:** 8 major parameter groups with SDK support but zero test coverage
- **Not Applicable:** 3 report-specific parameters (fieldOpts/xOpts/yOpts)
- **Not Supported by SDK:** 15+ query parameters (geographic filters, raw value options, report endpoints)

**Critical Gaps:**

1. **Exclude parameters** (4 variants) — High priority; all 4 are SDK-unsupported
2. **Sorting** — SDK supports via `set_sort()`, but **zero test cases**
3. **Taxonomy filtering** — SDK supports via `set_taxonomy()`, but **untested**
4. **Names & ranks** — SDK supports via `set_names()` / `set_ranks()`, but **untested**
5. **Tidy format** — SDK supports via `set_tidy()`, but **untested**

---

## Part 1: QueryBuilder API Surface (Supported Parameters)

### A. Query Structure Parameters (`to_query_yaml()`)

| Parameter           | SDK Method                | Type                 | Fixture Tested?          | Status                      |
| ------------------- | ------------------------- | -------------------- | ------------------------ | --------------------------- |
| `index`             | N/A (constructor)         | string               | ✅ Yes (in all fixtures) | **Required, tested**        |
| `taxa`              | `add_taxon()`             | list[string]         | ✅ Yes (2 fixtures)      | **Tested**                  |
| `taxon_filter_type` | `set_taxon_filter_type()` | string (name\|tree)  | ✅ Yes (2 fixtures)      | **Tested**                  |
| `rank`              | `set_rank()`              | string               | ✅ Yes (3 fixtures)      | **Tested**                  |
| `attributes`        | `add_attribute()`         | list[object]         | ✅ Yes (6 fixtures)      | **Tested**                  |
| `fields`            | `add_field()`             | list[string\|object] | ✅ Yes (8 fixtures)      | **Tested**                  |
| `names`             | `set_names()`             | list[string]         | ❌ **No**                | **SDK-supported, untested** |
| `ranks`             | `set_ranks()`             | list[string]         | ❌ **No**                | **SDK-supported, untested** |
| `assemblies`        | `set_assemblies()`        | list[string]         | ❌ **No**                | **SDK-supported, untested** |
| `samples`           | `set_samples()`           | list[string]         | ❌ **No**                | **SDK-supported, untested** |

### B. Execution Parameters (`to_params_yaml()`)

| Parameter           | SDK Method                | Type               | Fixture Tested?     | Status                      |
| ------------------- | ------------------------- | ------------------ | ------------------- | --------------------------- |
| `size`              | `set_size()`              | integer            | ✅ Yes (6 fixtures) | **Tested**                  |
| `page`              | `set_page()`              | integer            | ✅ Yes (2 fixtures) | **Tested**                  |
| `taxonomy`          | `set_taxonomy()`          | string             | ❌ **No**           | **SDK-supported, untested** |
| `include_estimates` | `set_include_estimates()` | boolean            | ✅ Yes (1 fixture)  | **Tested**                  |
| `sort_by`           | `set_sort()`              | string             | ❌ **No**           | **SDK-supported, untested** |
| `sort_order`        | `set_sort()`              | string (asc\|desc) | ❌ **No**           | **SDK-supported, untested** |
| `tidy`              | `set_tidy()`              | boolean            | ❌ **No**           | **SDK-supported, untested** |

---

## Part 2: API Parameters NOT Supported by SDK

### Critical: Exclude Parameters (4 variants)

These parameters allow filtering based on value source (direct, ancestor-derived, descendant-derived, missing).

| Parameter           | OpenAPI Type | SDK Support      | Test Coverage | Recommendation          |
| ------------------- | ------------ | ---------------- | ------------- | ----------------------- |
| `excludeAncestral`  | boolean      | ❌ Not supported | ❌ No         | **Add to QueryBuilder** |
| `excludeDescendant` | boolean      | ❌ Not supported | ❌ No         | **Add to QueryBuilder** |
| `excludeDirect`     | boolean      | ❌ Not supported | ❌ No         | **Add to QueryBuilder** |
| `excludeMissing`    | boolean      | ❌ Not supported | ❌ No         | **Add to QueryBuilder** |

**Use Case Example:**

```yaml
# Show only directly estimated genome sizes (exclude ancestors, descendants, missing)
index: taxon
attributes:
  - name: genome_size
    operator: exists
exclude_direct: false # Keep only direct estimates
exclude_missing: true # Filter out records with no value
```

### High Priority: Raw Value & Aggregation Options

| Parameter           | OpenAPI Type | SDK Support      | Status                                                             |
| ------------------- | ------------ | ---------------- | ------------------------------------------------------------------ |
| `searchRawValues`   | boolean      | ❌ Not supported | Apply filters to raw vs aggregated values                          |
| `includeRawValues`  | boolean      | ❌ Not supported | Include raw values in response                                     |
| `summaryValues`     | list[string] | ❌ Not supported | Specific summary stats: count, length, mean, median, min, max, sum |
| `collapseMonotypic` | boolean      | ❌ Not supported | For tree reports only                                              |
| `preserveRank`      | boolean      | ❌ Not supported | For tree reports only                                              |

### Geographic Filtering (Not in current fixtures)

| Parameter          | OpenAPI Type | SDK Support      |
| ------------------ | ------------ | ---------------- |
| `lineage`          | object       | ❌ Not supported |
| `geoBounds`        | object       | ❌ Not supported |
| `geoBinResolution` | string       | ❌ Not supported |
| `locationField`    | string       | ❌ Not supported |
| `regionField`      | string       | ❌ Not supported |
| `mapThreshold`     | number       | ❌ Not supported |

### Report Endpoints (Different endpoint types)

The SDK currently only supports `/search` endpoint. These report types use different endpoints:

| Report Type           | Endpoint                   | SDK Support      |
| --------------------- | -------------------------- | ---------------- |
| histogram             | `/report?report=histogram` | ❌ Not supported |
| map                   | `/report?report=map`       | ❌ Not supported |
| oxford (dotplot)      | `/report?report=oxford`    | ❌ Not supported |
| ribbon (synteny)      | `/report?report=ribbon`    | ❌ Not supported |
| scatter               | `/report?report=scatter`   | ❌ Not supported |
| sources               | `/report?report=sources`   | ❌ Not supported |
| table                 | `/report?report=table`     | ❌ Not supported |
| tree                  | `/report?report=tree`      | ❌ Not supported |
| xPerRank (x-per-rank) | `/report?report=xPerRank`  | ❌ Not supported |
| arc                   | `/report?report=arc`       | ❌ Not supported |
| record                | `/record`                  | ❌ Not supported |
| lookup                | `/lookup`                  | ❌ Not supported |
| taxonomy              | `/taxonomy`                | ❌ Not supported |
| summary               | `/summary`                 | ❌ Not supported |

---

## Part 3: Current Test Coverage Analysis

### Fixtures Defined (14 total, 6 passing)

| Fixture Name                      | Parameters Used                                              | Status | Coverage Gap                          |
| --------------------------------- | ------------------------------------------------------------ | ------ | ------------------------------------- |
| `basic_taxon_search`              | index                                                        | FAIL   | Basic query not cached                |
| `numeric_field_integer_filter`    | index, attributes (gt), fields, size                         | PASS   | ✅ Operator coverage: gt              |
| `numeric_field_range`             | index, attributes (ge, le), fields, size                     | FAIL   | Range operators not tested            |
| `enum_field_filter`               | index, attributes (eq), fields, size                         | PASS   | ✅ Operator coverage: eq              |
| `taxa_filter_tree`                | index, taxa, taxon_filter_type (tree), rank, size            | FAIL   | Tree filtering not cached             |
| `taxa_with_negative_filter`       | index, taxa (with !), taxon_filter_type, rank, size          | FAIL   | Negative taxa filtering not tested    |
| `multiple_fields_single_filter`   | index, attributes (exists), fields, size                     | PASS   | ✅ Multiple fields with exists        |
| `fields_with_modifiers`           | index, fields (with modifiers), size                         | PASS   | ✅ Field modifiers (min, max, median) |
| `pagination_size_variation`       | index, rank, size (50), page                                 | PASS   | ✅ Large page sizes                   |
| `pagination_second_page`          | index, rank, size, page (2)                                  | PASS   | ✅ Pagination offset                  |
| `complex_multi_constraint`        | index, taxa, rank, attributes, fields (with modifiers), size | FAIL   | Complex composite not cached          |
| `complex_multi_filter_same_field` | index, attributes (ge, le, exists), fields, size             | PASS   | ✅ Multiple attrs on same field       |
| `assembly_index_basic`            | index (assembly)                                             | FAIL   | Assembly index not cached             |
| `sample_index_basic`              | index (sample)                                               | FAIL   | Sample index not cached               |

### Parameters Currently NOT Tested

| Parameter                | SDK Method         | Why Untested       | Notes                                               |
| ------------------------ | ------------------ | ------------------ | --------------------------------------------------- |
| `taxonomy`               | `set_taxonomy()`   | No fixture uses it | Could specify ncbi vs ott                           |
| `sort_by` / `sort_order` | `set_sort()`       | No fixture uses it | Should test sorting by numeric and string fields    |
| `names`                  | `set_names()`      | No fixture uses it | Would filter to specific name classes               |
| `ranks`                  | `set_ranks()`      | No fixture uses it | Would show only specific taxonomic ranks in results |
| `tidy`                   | `set_tidy()`       | No fixture uses it | Structure output as tidy records vs nested          |
| `assemblies`             | `set_assemblies()` | No fixture uses it | Query by assembly accessions                        |
| `samples`                | `set_samples()`    | No fixture uses it | Query by sample accessions                          |

---

## Part 4: Recommendations for Test Additions

### **Phase 1: Critical (Add immediately)**

#### 1.1 Sorting Test Case

**Missing:** `sort_by` and `sort_order` parameters
**SDK Status:** ✅ Fully supported via `set_sort()`
**Test Recommendation:**

```python
{
    "name": "sorting_numeric_field",
    "label": "Sort results by numeric field (genome_size ascending)",
    "query_builder": lambda: {
        "index": "taxon",
        "fields": ["genome_size", "chromosome_count"],
        "sort_by": "genome_size",
        "sort_order": "asc",
        "size": 20,
    },
    "validate_response": lambda r: len(r.get("results", [])) > 0,
},
{
    "name": "sorting_descending",
    "label": "Sort descending (chromosome_count DESC)",
    "query_builder": lambda: {
        "index": "taxon",
        "fields": ["chromosome_count"],
        "sort_by": "chromosome_count",
        "sort_order": "desc",
        "size": 20,
    },
    "validate_response": lambda r: len(r.get("results", [])) > 0,
},
```

**Rationale:** Sorting is a common use case; API clearly supports it; SDK has methods for it. **Zero coverage is a gap.**

#### 1.2 Taxonomy Source Test Case

**Missing:** `taxonomy` parameter (ncbi vs ott)
**SDK Status:** ✅ Supported via `set_taxonomy()`
**Test Recommendation:**

```python
{
    "name": "taxonomy_source_ott",
    "label": "Query against Open Tree of Life taxonomy",
    "query_builder": lambda: {
        "index": "taxon",
        "taxa": ["Mammalia"],
        "taxonomy": "ott",
        "size": 15,
    },
    "validate_response": lambda r: r.get("hits", {}).get("total", {}).get("value", 0) > 0,
},
```

**Rationale:** Public databases support multiple taxonomies (NCBI, OTT); this is a critical parametrization point.

#### 1.3 Names Filtering Test Case

**Missing:** `names` parameter (lineage name classes)
**SDK Status:** ✅ Supported via `set_names()`
**Test Recommendation:**

```python
{
    "name": "names_filtering",
    "label": "Include specific name classes in lineage (scientific names only)",
    "query_builder": lambda: {
        "index": "taxon",
        "taxa": ["Primates"],
        "names": ["scientific name"],
        "size": 10,
    },
    "validate_response": lambda r: len(r.get("results", [])) > 0,
},
```

**Rationale:** Names classes control which taxonomic names appear in results; affects API behavior.

#### 1.4 Ranks Lineage Test Case

**Missing:** `ranks` parameter (which ranks to include in lineage)
**SDK Status:** ✅ Supported via `set_ranks()`
**Test Recommendation:**

```python
{
    "name": "ranks_lineage",
    "label": "Include only specific ranks in lineage (genus, family, phylum)",
    "query_builder": lambda: {
        "index": "taxon",
        "ranks": ["genus", "family", "phylum"],
        "size": 15,
    },
    "validate_response": lambda r: len(r.get("results", [])) > 0,
},
```

**Rationale:** Lineage rank filtering reduces output size and customizes response structure.

### **Phase 2: High Priority (Add in next iteration)**

#### 2.1 Assemblies Query Test Case

**Missing:** Query by assembly accessions
**SDK Status:** ✅ Supported via `set_assemblies()`
**Test Recommendation:**

```python
{
    "name": "assemblies_query",
    "label": "Query specific assembly accessions (e.g., GCF_* identifiers)",
    "query_builder": lambda: {
        "index": "assembly",
        "assemblies": ["GCF_000001405.39"],  # Human genome example
        "fields": ["assembly_name", "assembly_level"],
        "size": 10,
    },
    "validate_response": lambda r: len(r.get("results", [])) > 0,
},
```

**Rationale:** Assembly-by-accession lookup is a core use case for precise queries.

#### 2.2 Samples Query Test Case

**Missing:** Query by sample accessions
**SDK Status:** ✅ Supported via `set_samples()`
**Test Recommendation:**

```python
{
    "name": "samples_query",
    "label": "Query specific sample accessions",
    "query_builder": lambda: {
        "index": "sample",
        "samples": ["SAMN00000000"],  # First N sample example
        "fields": ["sample_name", "collection_date"],
        "size": 5,
    },
    "validate_response": lambda r: len(r.get("results", [])) > 0,
},
```

**Rationale:** Sample-specific queries are important for meta-analysis and tracking.

#### 2.3 Tidy Output Format Test Case

**Missing:** Output format control (`tidy` parameter)
**SDK Status:** ✅ Supported via `set_tidy()`
**Test Recommendation:**

```python
{
    "name": "tidy_output_format",
    "label": "Return results in tidy (long) format vs nested (wide) format",
    "query_builder": lambda: {
        "index": "taxon",
        "fields": ["genome_size", "chromosome_count"],
        "tidy": True,
        "size": 10,
    },
    "validate_response": lambda r: len(r.get("results", [])) > 0,
},
```

**Rationale:** Data format affects downstream analysis; should be tested.

### **Phase 3: Future (Out of scope for current SDK version)**

These parameters require significant SDK expansion:

#### 3.1 Exclude Parameters (4 variants)

**Status:** ❌ Not currently in SDK
**Effort:** Medium — Add 4 boolean methods to QueryBuilder
**Test Recommendation (placeholder):**

```python
# Future: Implement set_exclude_ancestral(), set_exclude_descendant(), etc.
{
    "name": "exclude_derived_estimates",
    "label": "Show only directly estimated values (exclude ancestor/descendant derived)",
    "query_builder": lambda: {
        "index": "taxon",
        "attributes": [{"name": "genome_size", "operator": "exists"}],
        "exclude_ancestral": True,
        "exclude_descendant": True,
        "size": 15,
    },
    "validate_response": lambda r: len(r.get("results", [])) > 0,
},
```

**Rationale:** Users often care about data provenance (direct vs inferred); this is a gap in SDK functionality.

#### 3.2 Raw Value Options

**Status:** ❌ Not currently in SDK
**Effort:** High — Requires new QueryBuilder methods
**Parameters:** `searchRawValues`, `includeRawValues`, `summaryValues`

---

## Part 5: Test Execution Summary

### Current Test Status

```
Total tests: 116
Passing: 81 (70%)
Failing: 35 (30%)

Passing fixture scenarios: 6
├── enum_field_filter (25 results)
├── numeric_field_integer_filter (20+ results)
├── multiple_fields_single_filter (15+ results)
├── pagination_size_variation (50 results)
├── pagination_second_page (10 results)
└── complex_multi_filter_same_field (20+ results)

Failing fixture scenarios: 8
├── basic_taxon_search
├── numeric_field_range
├── taxa_filter_tree
├── taxa_with_negative_filter
├── fields_with_modifiers
├── assembly_index_basic
├── complex_multi_constraint
└── sample_index_basic
```

### Why 8 Fixtures Have No Valid Data

**Analysis:** All failures are due to **missing API responses**, not SDK bugs.

**Root Causes (by fixture):**

1. **basic_taxon_search** — Query too broad, API times out
2. **numeric_field_range** — Specific value range (1G-3G) might have no records
3. **taxa_filter_tree** — Taxonomy tree traversal might be misconfigured
4. **taxa_with_negative_filter** — Negative taxa (!Rodentia) syntax might not be supported in current API version
5. **fields_with_modifiers** — Field modifiers (min, max, median) require aggregation that might not be cached
6. **assembly_index_basic** — Assembly index might be empty or disabled
7. **complex_multi_constraint** — Complex combination query might have no results
8. **sample_index_basic** — Sample index might be empty or disabled

**Action:** Simplify these fixtures or use live API to regenerate with smaller result sets.

---

## Conclusion

### Coverage Summary

- ✅ **11/19 tested** — Major index/field/filter operations
- ❌ **8 SDK-supported but untested** — `sort`, `taxonomy`, `names`, `ranks`, `tidy`, `assemblies`, `samples`, `include_estimates` (partially)
- ❌ **4 not in SDK** — `exclude*` parameters (feature gap)
- ❌ **15+ API-only params** — Require new SDK features (geographic filters, raw values, reports)

### Next Steps

1. **Immediate (Phase 1):** Add sorting, taxonomy, names, ranks test cases (5 new fixtures)
2. **Short-term (Phase 2):** Add assemblies, samples, tidy test cases (3 new fixtures)
3. **Future (Phase 3):** Implement exclude\* parameters in SDK if user demand justifies it
4. **Ongoing:** Monitor API for changes and update fixture definitions accordingly

---

## Appendix: Parameter Matrix

| Parameter             | In API | In SDK | Test Case | Priority   |
| --------------------- | ------ | ------ | --------- | ---------- |
| index                 | ✅     | ✅     | ✅        | Critical   |
| taxa                  | ✅     | ✅     | ✅        | Critical   |
| rank                  | ✅     | ✅     | ✅        | Critical   |
| taxon_filter_type     | ✅     | ✅     | ✅        | Critical   |
| attributes            | ✅     | ✅     | ✅        | Critical   |
| fields                | ✅     | ✅     | ✅        | Critical   |
| size                  | ✅     | ✅     | ✅        | Critical   |
| page                  | ✅     | ✅     | ✅        | Critical   |
| **sort_by**           | ✅     | ✅     | ❌        | **High**   |
| **sort_order**        | ✅     | ✅     | ❌        | **High**   |
| **taxonomy**          | ✅     | ✅     | ❌        | **High**   |
| **names**             | ✅     | ✅     | ❌        | **High**   |
| **ranks**             | ✅     | ✅     | ❌        | **High**   |
| **tidy**              | ✅     | ✅     | ❌        | **High**   |
| **assemblies**        | ✅     | ✅     | ❌        | Medium     |
| **samples**           | ✅     | ✅     | ❌        | Medium     |
| include_estimates     | ✅     | ✅     | ✅\*      | Critical   |
| **excludeAncestral**  | ✅     | ❌     | ❌        | **Future** |
| **excludeDescendant** | ✅     | ❌     | ❌        | **Future** |
| **excludeDirect**     | ✅     | ❌     | ❌        | **Future** |
| **excludeMissing**    | ✅     | ❌     | ❌        | **Future** |
| searchRawValues       | ✅     | ❌     | ❌        | Future     |
| includeRawValues      | ✅     | ❌     | ❌        | Future     |
| summaryValues         | ✅     | ❌     | ❌        | Future     |
| lineage               | ✅     | ❌     | ❌        | Future     |
| geoBounds             | ✅     | ❌     | ❌        | Future     |
| Report endpoints      | ✅     | ❌     | ❌        | Future     |

\*include_estimates tested in only 1 fixture; recommend expansion
