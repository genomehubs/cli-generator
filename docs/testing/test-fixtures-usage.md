# SDK Test Fixtures — Usage & Setup Guide

## Overview

Test fixtures provide **real API response examples** that comprehensively cover SDK functionality across all parameter types and combinations.

**Coverage Areas:**

- ✅ 14 fixture scenarios (taxon, assembly, sample indexes)
- ✅ Numeric fields (integer, float with ranges)
- ✅ Categorical fields (enum, keyword)
- ✅ Date fields
- ✅ Taxonomic constraints (tree filters, exclusions)
- ✅ Multi-field selections with modifiers
- ✅ Pagination (different sizes, page numbers)
- ✅ Complex multi-filter queries

**Files:**

- `tests/python/discover_fixtures.py` — Fixture discovery & caching
- `tests/python/test_sdk_fixtures.py` — Fixture-based tests
- `tests/python/conftest.py` — Pytest configuration & shared fixtures
- `tests/python/fixtures/` — Cached API responses (auto-created)

## Quick Start

### 1. Discover & Cache Fixtures (First Time)

```bash
# Query live API and cache all fixture responses
python tests/python/discover_fixtures.py --update

# Output:
#   → Querying API for basic_taxon_search...
#   ✓ Loaded basic_taxon_search from cache
#   → Querying API for numeric_field_integer_filter...
#   ✓ Received 20 results
#   ... (14 fixtures total)
#   ✓ Cached 14 fixtures in tests/python/fixtures/
```

### 2. Run Fixture-Based Tests

```bash
# Run all fixture tests
pytest tests/python/test_sdk_fixtures.py -v

# Run only validation tests
pytest tests/python/test_sdk_fixtures.py::TestFixtureValidation -v

# Run specific fixture test
pytest tests/python/test_sdk_fixtures.py::TestFixtureValidation::test_builder_creates_valid_url -v
```

### 3. Update Cached Fixtures

When the API changes or you need fresh data, re-cache without API calls:

```bash
# Use cached data for most fixtures, but query API for any missing
python tests/python/discover_fixtures.py

# Force refresh all from live API
python tests/python/discover_fixtures.py --update
```

## Fixture Definitions

Each fixture is a **standard query pattern** mapped to both:

1. A real API response (cached in `tests/python/fixtures/*.json`)
2. A QueryBuilder pattern (in `FIXTURE_TO_BUILDER` dict)

### Available Fixtures

| Fixture Name                      | Query Pattern                                    | Coverage              |
| --------------------------------- | ------------------------------------------------ | --------------------- |
| `basic_taxon_search`              | Simple taxon index query                         | Basic index search    |
| `numeric_field_integer_filter`    | Integer field filter (chromosome_count > 10)     | Numeric comparisons   |
| `numeric_field_range`             | Range filter (genome_size 1G-3G)                 | Range operations      |
| `enum_field_filter`               | Enum filter (assembly_level = 'complete genome') | Categorical fields    |
| `taxa_filter_tree`                | Subtree traversal (Mammalia)                     | Taxonomic tree ops    |
| `taxa_with_negative_filter`       | Exclusion filter (Mammalia !Rodentia)            | Negative filters      |
| `multiple_fields_single_filter`   | 3 fields + 1 filter                              | Multi-field selection |
| `fields_with_modifiers`           | Fields with min/max/median modifiers             | Field modifiers       |
| `pagination_size_variation`       | Custom page size (50 results)                    | Pagination params     |
| `pagination_second_page`          | Page 2 of results                                | Pagination offsets    |
| `complex_multi_constraint`        | Taxa + rank + filter + modifiers                 | Complex queries       |
| `complex_multi_filter_same_field` | Multiple filters on same field                   | Advanced filtering    |
| `assembly_index_basic`            | Assembly index search                            | Assembly index        |
| `sample_index_basic`              | Sample index search                              | Sample index          |

## Test Organization

### `TestFixtureValidation`

Parametrized tests that run **the same test logic against all fixtures**.

Each test is automatically run 14 times (once per fixture).

**Example: `test_fixture_no_api_error`**

```
✓ test_fixture_no_api_error[basic_taxon_search]
✓ test_fixture_no_api_error[numeric_field_integer_filter]
✓ test_fixture_no_api_error[enum_field_filter]
... (all 14 fixtures)
```

**Tests in this class:**

1. `test_fixture_no_api_error` — Verify response has no error field
2. `test_fixture_has_results_or_hits` — Verify response structure
3. `test_builder_creates_valid_url` — URL generation
4. `test_fixture_counts_are_reasonable` — Result counts check bounds
5. `test_builder_to_yaml` — YAML serialization
6. `test_fixture_can_describe` — Description generation
7. `test_fixture_can_generate_snippet` — Code snippet generation
8. `test_fixture_can_tidy_records` — Data transformation

### `TestFixtureRegressionCatches`

Specialized tests for **fixture-specific patterns and regressions**.

**Tests in this class:**

1. `test_complex_multi_constraint_has_results` — Complex queries work
2. `test_pagination_size_respected` — Pagination is enforced
3. `test_numeric_filters_effective` — Filters reduce result count
4. `test_taxa_tree_filter_returns_results` — Taxa filters work
5. `test_fixture_mapping_completeness` — All fixtures have builders
6. `test_builders_match_fixture_patterns` — Builders match their fixtures

## Adding New Fixtures

### Step 1: Define in `discover_fixtures.py`

Add to `FIXTURE_DEFINITIONS` list:

```python
{
    "name": "my_custom_fixture",
    "label": "Custom query for testing nested modifiers",
    "query_builder": lambda: {
        "index": "taxon",
        "attributes": [
            {
                "name": "genome_size",
                "operator": "ge",
                "value": "1G",
                "modifier": ["direct", "min"]  # Specific modifiers
            }
        ],
        "fields": ["genome_size"],
        "size": 15,
    },
    "validate_response": lambda r: len(r.get("results", [])) > 0,
}
```

### Step 2: Map Builder in `test_sdk_fixtures.py`

Add to `FIXTURE_TO_BUILDER` dict:

```python
"my_custom_fixture": lambda: QueryBuilder("taxon")
    .add_attribute("genome_size", "ge", "1G", modifiers=["direct", "min"])
    .add_field("genome_size")
    .set_size(15),
```

### Step 3: Discover & Cache

```bash
python tests/python/discover_fixtures.py --update
```

### Step 4: Run Tests

Your new fixture will automatically be included in all parametrized tests:

```bash
pytest tests/python/test_sdk_fixtures.py -v
```

## Caching Strategy

### How Caching Works

1. **First run:** `discover_fixtures.py --update` queries live API
2. **Subsequent runs:** Uses cached JSON files from `tests/python/fixtures/`
3. **CI environment:** Loads pre-cached files without API calls
4. **Local development:** Can refresh with `--update` flag

### Cache Files

```
tests/python/fixtures/
├── basic_taxon_search.json
├── numeric_field_integer_filter.json
├── enum_field_filter.json
├── taxa_filter_tree.json
... (one file per fixture)
```

Each file contains the complete API response for that fixture.

### Updating Caches

```bash
# Update all fixtures from live API
python tests/python/discover_fixtures.py --update

# Update only missing fixtures (skip API for cached ones)
python tests/python/discover_fixtures.py
```

## Test Output Example

```bash
$ pytest tests/python/test_sdk_fixtures.py::TestFixtureValidation -v

test_fixture_no_api_error[basic_taxon_search] PASSED
test_fixture_no_api_error[numeric_field_integer_filter] PASSED
... (42 tests total: 14 fixtures × 3 test parametrizations)

test_builder_creates_valid_url[basic_taxon_search] PASSED
test_builder_creates_valid_url[numeric_field_integer_filter] PASSED
... (42 total)

test_complex_multi_constraint_has_results PASSED
test_pagination_size_respected PASSED
test_numeric_filters_effective PASSED

======================== 42 passed in 2.34s ========================
```

## Pytest Fixtures Available

The following fixtures are automatically available to all tests via `conftest.py`:

### `all_fixtures` (session scope)

```python
def test_something(all_fixtures):
    """Access all cached fixtures."""
    response = all_fixtures["basic_taxon_search"]
    assert response["hits"]["total"]["value"] > 0
```

### `fixture_name` (parametrized)

```python
def test_something(fixture_name):
    """Iterate over all fixture names."""
    assert fixture_name in [
        "basic_taxon_search",
        "numeric_field_integer_filter",
        ...
    ]
```

### `fixture_response` (parametrized)

```python
def test_something(fixture_response):
    """Iterate over fixture responses."""
    results = fixture_response.get("results", [])
    assert len(results) >= 0
```

## Continuous Integration

### GitHub Actions

In `.github/workflows/` CI config:

```yaml
- name: Discover test fixtures
  run: |
    # Use pre-cached fixtures (commit them to repo)
    if [[ ! -d tests/python/fixtures ]]; then
      python tests/python/discover_fixtures.py --update
    fi

- name: Run fixture tests
  run: |
    pytest tests/python/test_sdk_fixtures.py -v
```

### Pre-caching Fixtures for CI

```bash
# Generate and commit cached fixtures locally
python tests/python/discover_fixtures.py --update
git add tests/python/fixtures/*.json
git commit -m "test: update SDK fixture cache from live API"
```

## Troubleshooting

### "Fixtures not cached" Error

```
pytest.skip: Fixtures not cached. Run:
    python tests/python/discover_fixtures.py --update
```

**Solution:**

```bash
python tests/python/discover_fixtures.py --update
```

### API Query Timeout

```
urllib.error.URLError: <urlopen error timed out>
```

**Solution:**

- Try again later (API may be slow)
- Check live API status: https://goat.genomehubs.org/api/v2/search
- Use cached fixtures if available: `python discover_fixtures.py`

### Fixture Count Mismatch

If a fixture returns fewer results than expected, it may be due to:

1. Database growth/shrinkage (update caches: `--update`)
2. API changes (check API docs)
3. Network issues (retry with `--update`)

### Builder Mapping Error

If you get "Fixture X not yet mapped to QueryBuilder":

1. Add mapping in `FIXTURE_TO_BUILDER` (see "Adding New Fixtures" above)
2. Run tests again

## Performance Characteristics

### Test Execution Time

**With cached fixtures:**

- ~2-3 seconds for all 40+ tests
- No network I/O
- Deterministic

**With live API (first time):**

- ~30-60 seconds (14 API calls)
- Depends on network & API response times

### Memory Usage

Fixture responses are held in memory during test session:

- Typical total size: 1-5 MB
- Safe for all environments

## Next Steps

1. **Phase 6.4:** Add edge case fixtures (errors, extreme values)
2. **Phase 6.5:** Integrate with CI/CD pipeline
3. **Phase 7:** Build performance benchmarks using fixtures
4. **Phase 8:** Add fixture-based fuzzing tests
