# Test Fixtures Strategy — Phase 6.3+

## Goal

Create comprehensive pytest fixtures covering all SDK parameter combinations and interaction patterns, sourced from live API examples.

## Approach

### 1. Parameter Coverage Matrix

Extract all parameters from metadata and create fixtures for:

**A. Field-type combinations** (from `field_meta.json`)

- `integer` fields: `chromosome_count`, `assembly_span`, `c_value`
- `float` fields: `btk_nohit`, `btk_target`
- `keyword` fields: `bioproject`, `biosample`
- `date` fields: `assembly_date`
- `ordered_keyword` fields: `assembly_level`

**B. Operator combinations** (per field type)

- Numeric (`integer`, `float`): `eq`, `ne`, `lt`, `le`, `gt`, `ge`
- Keyword/Enum: `eq`, `ne`, `exists`, `missing`
- Date: `lt`, `le`, `gt`, `ge`

**C. Summary modifier combinations**

- Numeric: `min`, `max`, `median`, `mean`
- Categorical: `enum`, `mode`
- Counts: `count`

**D. Traversal direction combinations**

- Upward: ancestor-level aggregation
- Downward: descendant-level aggregation
- Bidirectional: both directions

**E. Query state combinations**

- Single filter + single field
- Multiple filters (2-3) + multiple fields
- Taxa + rank constraints
- Optional params: size, page, sort, include_estimates
- Taxonomy choice: `"ncbi"` vs `"ott"` (if available)

### 2. Live API Sampling Strategy

For each coverage area, generate **3-5 example queries** against `goat.genomehubs.org/api`:

**Phase 2a: Discover Available Values**

```python
# For enum fields (assembly_level), fetch distinct values
# GET /api/v2/search?query={"index":"taxon","fields":["assembly_level"]}

# For numeric ranges, fetch min/max from aggregations
# GET /api/v2/search?query={"index":"taxon","size":0}&agg=genome_size
```

**Phase 2b: Generate Valid Fixture Queries**

```python
fixture_queries = [
    # Basic single-field numeric filter
    {
        "name": "genome_size_ge_1g",
        "builder": lambda: QueryBuilder("taxon")
            .add_attribute("genome_size", "ge", "1G")
            .add_field("genome_size")
            .set_size(5),
        "expected_count_range": (500, 5000),  # Documented from live API
    },
    # Enum filter with specific value
    {
        "name": "assembly_level_complete",
        "builder": lambda: QueryBuilder("taxon")
            .add_attribute("assembly_level", "eq", "complete genome")
            .set_rank("species")
            .set_names(["scientific_name"])
            .set_size(10),
        "expected_count_range": (1000, 10000),
    },
    # Multi-constraint query
    {
        "name": "mammals_with_large_assembly",
        "builder": lambda: QueryBuilder("taxon")
            .set_taxa(["Mammalia"], filter_type="tree")
            .add_attribute("assembly_span", "ge", "2000000000")
            .add_field("assembly_span:median")
            .add_field("chromosome_number:mode")
            .set_rank("species")
            .set_size(20),
        "expected_count_range": (50, 500),
    },
]
```

### 3. Pytest Fixture Design

Create `conftest.py` with pytest fixtures:

```python
# tests/python/conftest.py

@pytest.fixture(scope="session")
def api_config():
    """Live API configuration."""
    return {
        "api_base": "https://goat.genomehubs.org/api",
        "api_version": "v2",
    }

@pytest.fixture(scope="session", params=[...])
def fixture_query(request):
    """Parametrized fixture for each test scenario."""
    return request.param

@pytest.fixture(scope="session")
def fixture_responses(api_config, fixture_query):
    """Execute all fixtures against live API once per session."""
    # Cache responses to avoid hammering API
    # Store in .pytest_cache/ for reuse across test runs
    return execute_and_cache(fixture_query, api_config)
```

### 4. Test Classes Using Fixtures

```python
# tests/python/test_sdk_fixtures.py

class TestSDKFixtures:
    """Validate SDK behavior against real API responses."""

    def test_fixture_query_builds_valid_url(self, fixture_query, api_config):
        """Verify each fixture builds a valid URL."""
        url = fixture_query["builder"]().to_url(**api_config)
        assert url.startswith(api_config["api_base"])

    def test_fixture_query_returns_data(self, fixture_query, fixture_responses):
        """Verify each fixture returns expected result count."""
        response = fixture_responses[fixture_query["name"]]
        count = response["hits"]["total"]["value"]

        min_expected, max_expected = fixture_query["expected_count_range"]
        assert min_expected <= count <= max_expected, \
            f"Count {count} outside expected range ({min_expected}, {max_expected})"

    def test_fixture_fields_present(self, fixture_query, fixture_responses):
        """Verify requested fields are in response."""
        response = fixture_responses[fixture_query["name"]]
        fields_requested = fixture_query.get("expected_fields", [])

        if response["results"]:
            first_record = response["results"][0]
            for field in fields_requested:
                assert field in first_record, \
                    f"Requested field '{field}' not in response"

    def test_parseability(self, fixture_query, fixture_responses):
        """Verify all SDK methods work on response data."""
        response = fixture_responses[fixture_query["name"]]
        qb = fixture_query["builder"]()

        # Test tidy_records
        tidy = qb.to_tidy_records(response)
        assert len(tidy) > 0

        # Test describe
        description = qb.describe()
        assert len(description) > 0
```

### 5. Fixture Storage & Caching

Store cached API responses in:

```
.pytest_cache/
  fixtures/
    genome_size_ge_1g.json
    assembly_level_complete.json
    mammals_with_large_assembly.json
```

Benefits:

- Fast test runs (no API calls during CI)
- Reproducible results (same data across runs)
- Easy to update with `pytest --fixtures-update`

### 6. Comprehensive Coverage Checklist

- [ ] All field types (integer, float, keyword, date, ordered_keyword)
- [ ] All operators per type (eq, ne, lt, le, gt, ge, exists, missing)
- [ ] All summary modifiers (min, max, median, mode, count, enum)
- [ ] Traversal directions (up, down, both)
- [ ] Multi-field selections (single → 5+ fields)
- [ ] Multi-filter combinations (1 → 3 filters)
- [ ] Taxa + rank constraints
- [ ] Pagination (size, page)
- [ ] Sorting (by different fields, asc/desc)
- [ ] Taxonomy switching (ncbi, ott)
- [ ] include_estimates toggle
- [ ] Field modifier combinations (multiple modifiers per field)
- [ ] Query state merging (merge + combine)
- [ ] Error cases (invalid field, unsupported operator)

## Implementation Timeline

**Phase 6.3: Fixture Discovery**

- Query live API to discover available values/ranges
- Generate initial 15-20 fixture scenarios
- Implement fixture parametrization in conftest

**Phase 6.4: Fixture Validation**

- Implement test_sdk_fixtures.py test classes
- Verify all fixtures pass on live API
- Cache responses for CI

**Phase 6.5: CI Integration**

- Add fixture tests to GitHub Actions workflow
- Set up response caching strategy
- Document fixture update process

## Notes

- Live API calls should be cached aggressively to avoid rate limiting
- Fixtures should be deterministic (use fixed seed for randomness if needed)
- Document expected count ranges separately (may change as GoaT grows)
- Consider seasonal/time-based parameters (assembly_date ranges)
