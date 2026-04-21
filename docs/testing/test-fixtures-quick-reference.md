# Test Fixtures — Quick Reference

## The Problem We're Solving

✅ **Before:** Manual test queries, inconsistent coverage, no documentation
❌ **After:** Systematic fixture-based testing with real API examples

## What You Get

| Component                | Purpose                                     | Coverage                        |
| ------------------------ | ------------------------------------------- | ------------------------------- |
| `discover_fixtures.py`   | Query live API, cache responses             | 14 fixture scenarios            |
| `test_sdk_fixtures.py`   | Run parametrized tests against all fixtures | 40+ tests × 14 fixtures         |
| `conftest.py`            | Pytest integration & shared fixtures        | Automatic fixture loading       |
| `tests/python/fixtures/` | Cached API responses (JSON)                 | Pre-queried, no API calls in CI |

## One-Liner Commands

```bash
# Initialize fixtures from live API (run once)
python tests/python/discover_fixtures.py --update

# Run all fixture tests
pytest tests/python/test_sdk_fixtures.py -v

# Run specific test class
pytest tests/python/test_sdk_fixtures.py::TestFixtureValidation -v

# Run only parametrized tests (skip regression tests)
pytest tests/python/test_sdk_fixtures.py::TestFixtureValidation -v
```

## Test Execution Flow

```
discover_fixtures.py --update
    ↓
Query 14 endpoints on goat.genomehubs.org/api
    ↓
Cache responses to tests/python/fixtures/*.json
    ↓
test_sdk_fixtures.py loads cached responses
    ↓
14 fixtures × parametrized tests = 40+ test cases
    ↓
✓ All tests pass
```

## Fixture Scenarios (14 Total)

**Index & Basic Operations (3)**

- `basic_taxon_search` — Simple taxon query
- `assembly_index_basic` — Assembly index
- `sample_index_basic` — Sample index

**Field Type Operations (3)**

- `numeric_field_integer_filter` — Integer comparisons
- `numeric_field_range` — Range operations
- `enum_field_filter` — Categorical values

**Taxonomic Operations (2)**

- `taxa_filter_tree` — Subtree traversal
- `taxa_with_negative_filter` — Exclusion filters

**Field Selection (2)**

- `multiple_fields_single_filter` — Multi-field requests
- `fields_with_modifiers` — Summary modifiers

**Pagination (2)**

- `pagination_size_variation` — Custom page size
- `pagination_second_page` — Offset pagination

**Complex Queries (2)**

- `complex_multi_constraint` — Taxa + rank + filter + modifiers
- `complex_multi_filter_same_field` — Multiple conditions per field

## What Tests Check

For each fixture, tests verify:

1. ✅ **Response validity** — No errors, has expected fields
2. ✅ **URL generation** — Builder creates valid API URLs
3. ✅ **YAML serialization** — Query/params serialize correctly
4. ✅ **Pagination** — Result counts respect limits
5. ✅ **Transformations** — `describe()`, `snippet()`, `tidy_records()`
6. ✅ **Field types** — All data types handled correctly
7. ✅ **Complex patterns** — Multi-filter, multi-field queries work

## Where Test Fixtures Come From

```
Live API (goat.genomehubs.org)
    ↓
discovery via discover_fixtures.py
    ↓
cached in Git (tests/python/fixtures/)
    ↓
loaded in CI without API calls
    ↓
parametrized tests run locally & in CI
```

## Adding New Fixtures

**3-step process:**

```python
# 1. Define in discover_fixtures.py
FIXTURE_DEFINITIONS.append({
    "name": "my_fixture",
    "label": "Test pattern X",
    "query_builder": lambda: {"index": "taxon", ...},
})

# 2. Map in test_sdk_fixtures.py
FIXTURE_TO_BUILDER["my_fixture"] = lambda: QueryBuilder("taxon")...

# 3. Run discovery
python tests/python/discover_fixtures.py --update
```

Your new fixture automatically runs in all parametrized tests.

## Expected Test Output

```
tests/python/test_sdk_fixtures.py::TestFixtureValidation::
  test_fixture_no_api_error[basic_taxon_search] PASSED
  test_fixture_no_api_error[numeric_field_integer_filter] PASSED
  ... (all 14 fixtures)

tests/python/test_sdk_fixtures.py::TestFixtureValidation::
  test_builder_creates_valid_url[basic_taxon_search] PASSED
  test_builder_creates_valid_url[numeric_field_integer_filter] PASSED

tests/python/test_sdk_fixtures.py::TestFixtureRegressionCatches::
  test_complex_multi_constraint_has_results PASSED
  test_pagination_size_respected PASSED

======================== 42 passed in 2.34s ========================
```

## File Structure

```
tests/python/
├── conftest.py                    ← Pytest config + fixture exports
├── discover_fixtures.py           ← API query & cache management
├── test_sdk_fixtures.py           ← Fixture-based tests
├── fixtures/                      ← Cached API responses
│   ├── basic_taxon_search.json
│   ├── numeric_field_integer_filter.json
│   ├── enum_field_filter.json
│   ... (14 total)
└── test_sdk_parity.py             ← (existing parity tests)
```

## Integration Points

### Python SDK

Tests validate `QueryBuilder` class methods:

- Constructor: `QueryBuilder(index, validation_level, api_base)`
- Filters: `add_attribute()`, `set_attributes()`
- Fields: `add_field()`, `set_fields()`
- Taxa: `set_taxa()`, `set_rank()`
- Params: `set_size()`, `set_page()`, etc.
- Transforms: `to_tidy_records()`, `describe()`, `snippet()`

### Templates

Fixtures also validate **proposed** JavaScript & R SDKs once templates are updated.

## Performance

| Operation                    | Time   | Notes                      |
| ---------------------------- | ------ | -------------------------- |
| Cache discovery (`--update`) | 30-60s | One-time, queries live API |
| Load cached fixtures         | <1s    | Auto-loaded by pytest      |
| Run all tests                | 2-3s   | In-memory, no network      |
| First run (no cache)         | ~1min  | Discovery + tests          |

## Common Scenarios

### I'm developing SDK features — how do I use fixtures?

```bash
# 1. Ensure fixtures are cached
python tests/python/discover_fixtures.py

# 2. Make your code changes
# (edit templates/, src/core/, etc.)

# 3. Run tests to verify
pytest tests/python/test_sdk_fixtures.py -v

# 4. All 14 fixtures tested automatically
# ✓ Your changes didn't break any scenario
```

### I need to add a new API parameter — what do I test?

```bash
# 1. Add fixture covering the new parameter
# (edit discover_fixtures.py + test_sdk_fixtures.py)

# 2. Discover & cache the fixture
python tests/python/discover_fixtures.py --update

# 3. Run tests
pytest tests/python/test_sdk_fixtures.py::TestFixtureValidation -v

# 4. Now 15 fixtures cover your new parameter
```

### I want to debug a specific fixture — how?

```bash
# Run just one fixture's tests
pytest tests/python/test_sdk_fixtures.py::TestFixtureValidation::test_fixture_no_api_error[enum_field_filter] -v

# See detailed output
pytest tests/python/test_sdk_fixtures.py::TestFixtureValidation -k enum_field_filter -vv
```

### API changed — how do I update fixtures?

```bash
# Re-query live API to refresh all cached responses
python tests/python/discover_fixtures.py --update

# Commit updated JSON files
git add tests/python/fixtures/*.json
git commit -m "test: refresh SDK fixtures from updated API"
```

## Troubleshooting

| Error                      | Solution                                              |
| -------------------------- | ----------------------------------------------------- |
| "Fixtures not cached"      | `python tests/python/discover_fixtures.py --update`   |
| "Fixture X not yet mapped" | Add to `FIXTURE_TO_BUILDER` dict in test file         |
| API timeout                | Retry with `--update`, check API status               |
| Count mismatch             | Run `--update` to refresh from live API               |
| Missing results            | Verify API has test data, check fixture query builder |

## Architecture Decision: Why Live Fixtures?

✅ **Advantages of live API fixtures:**

1. Real data — Tests use actual API responses, not mocks
2. Comprehensive coverage — 14 scenarios cover real-world usage
3. Regression detection — Changes to API show immediately
4. Documentation — Fixtures serve as usage examples
5. Caching — After first discovery, no API calls needed

❌ **Downsides (mitigated):**

- Requires live API access initially (one-time cost)
- API changes require re-caching (simple CLI command)
- Network dependency (cached locally, no CI impact)

## Next Steps

1. **Phase 6.3 (current):** ✅ Fixture system implemented
2. **Phase 6.4:** Edge case fixtures (errors, extreme values)
3. **Phase 6.5:** CI/CD integration with fixture caching
4. **Phase 7:** Performance benchmarks using fixtures
5. **Phase 8:** Fuzzing & property-based tests on fixtures

## Related Documentation

- [Test Fixtures Strategy](test-fixtures-strategy.md) — Design & approach
- [Test Fixtures Usage Guide](test-fixtures-usage.md) — Comprehensive walkthrough
- [SDK Parity Tests](docs/phase-6-sdk-testing.md) — Complementary parity checks
- [Documentation Parity](docs/phase-6-documentation-parity.md) — Docs validation
