# Testing Fixtures Against Generated SDKs

## ⭐ Key Insight: Site-Specific Fixtures

**Each site has its own API**, so fixtures must be site-specific:

- Different fields, indexes, and data per site
- Fixtures are discovered **from the site's API**
- Tests validate SDKs against **their site's API**

## Quick Start (Two-Command)

```bash
# Step 1: Generate the SDK
bash scripts/dev_site.sh --python --output ./workdir/my-goat goat

# Step 2: Test it (auto-discovers fixtures from site's API)
bash scripts/test_sdk_fixtures.sh --site goat --python
```

That's it! The script:

1. Auto-constructs the API base URL: `https://goat.genomehubs.org/api`
2. Discovers fixtures from that site's API
3. Tests the generated SDK against those fixtures

## How It Works

### Complete Flow

```
test_sdk_fixtures.sh --site goat --python
  ↓
  Auto-construct: https://goat.genomehubs.org/api
  ↓
  discover_fixtures.py --api-base https://goat.genomehubs.org/api
    ↓ (query goat's API)
    Cache in: tests/python/fixtures-goat/
  ↓
  test_sdk_fixtures.py
    ↓ (test generated goat SDK)
    ✓ All tests pass
```

### Site-Specific Fixtures

Each site gets its own fixture cache:

```
tests/python/
├── fixtures-goat/          ← Goat-specific (queried from goat.genomehubs.org)
│   ├── basic_taxon_search.json
│   ├── numeric_field_integer_filter.json
│   ... (14 total)
├── fixtures-mouse/         ← Mouse-specific (if you run with --site mouse)
│   ├── basic_taxon_search.json
│   ... (14 total, may differ from goat)
```

Why? Because each site's API may have:

- Different available fields
- Different result counts
- Different constraints and modifiers
- Site-specific configuration

## Running Against Different Sites

Each site test is **completely independent** with its own fixture cache:

```bash
# Test goat SDK (discovers from goat.genomehubs.org/api)
bash scripts/test_sdk_fixtures.sh --site goat --python

# Test mouse SDK (discovers from mouse.genomehubs.org/api)
bash scripts/test_sdk_fixtures.sh --site mouse --python

# Test plant SDK (discovers from plant.genomehubs.org/api)
bash scripts/test_sdk_fixtures.sh --site plant --python
```

**Why separate caches?** Each site's API has:

- Different fields and indexes
- Different data distributions
- Different constraints and modifiers
- Site-specific configurations

```
tests/python/
├── fixtures-goat/    ← 14 fixtures from goat.genomehubs.org/api
├── fixtures-mouse/   ← 14 fixtures from mouse.genomehubs.org/api
├── fixtures-plant/   ← 14 fixtures from plant.genomehubs.org/api
```

## Testing Different Languages

For any site, test specific languages or all:

```bash
# Python only
bash scripts/test_sdk_fixtures.sh --site goat --python

# JavaScript only
bash scripts/test_sdk_fixtures.sh --site goat --javascript

# All languages
bash scripts/test_sdk_fixtures.sh --site goat --all
```

## Advanced: Custom API Endpoints

For non-standard API endpoints:

```bash
# Discover fixtures from custom API
python tests/python/discover_fixtures.py --api-base https://custom.site.org/api --update

# Test against those fixtures
bash scripts/test_sdk_fixtures.sh --api-base https://custom.site.org/api --python
```

## Auto-Discovery: Re-cache on Demand

If fixtures are outdated or missing, they're automatically re-discovered:

```bash
# Delete cached fixtures to force re-discovery on next run
rm -rf tests/python/fixtures-goat/

# Next run auto-populates fresh fixtures
bash scripts/test_sdk_fixtures.sh --site goat --python
```

## Workflow Example

### Testing a Template Change

When you modify templates (e.g., `templates/python/query.py.tera`):

```bash
# 1. Edit your template files
# (modify src/, templates/, etc.)

# 2. Regenerate the SDK
bash scripts/dev_site.sh --python --output ./workdir/my-goat goat

# 3. Test it (fixtures auto-discovered if needed)
bash scripts/test_sdk_fixtures.sh --site goat --python

# 4. Check results
# ✓ All tests pass → Your changes are safe
# ✗ Tests fail → Investigate the error
```

### Multi-Site Testing Workflow

Test the same template changes against multiple sites:

```bash
# 1. Make template changes

# 2. Test goat
bash scripts/dev_site.sh --python --output ./workdir/test-goat goat
bash scripts/test_sdk_fixtures.sh --site goat --python
# ✓ goat tests pass

# 3. Test mouse
bash scripts/dev_site.sh --python --output ./workdir/test-mouse mouse
bash scripts/test_sdk_fixtures.sh --site mouse --python
# ✓ mouse tests pass

# 4. Test plant
bash scripts/dev_site.sh --python --output ./workdir/test-plant plant
bash scripts/test_sdk_fixtures.sh --site plant --python
# ✓ plant tests pass
```

Each site is tested against **its own fixtures**, ensuring compatibility.

## How the Test Script Works (Internals)

When you run `bash scripts/test_sdk_fixtures.sh --site goat --python`, here's what happens:

### 1. Detect and validate site

```bash
SITE="goat"
API_BASE="https://goat.genomehubs.org/api"
FIXTURES_CACHE_DIR="tests/python/fixtures-goat"
```

### 2. Auto-discover fixtures if needed

```bash
# Check if fixtures are cached
if [ ! -d "$FIXTURES_CACHE_DIR" ] || [ -z "$(ls -A "$FIXTURES_CACHE_DIR")" ]; then
    # Cache doesn't exist or is empty → discover fresh fixtures from API
    python tests/python/discover_fixtures.py \
        --api-base "$API_BASE" \
        --update
fi
```

This only happens on **first run** or if you delete the cache.

### 3. Locate generated SDK

```bash
SDK_PATH="workdir/my-goat/goat-cli/python/goat_sdk"

# Verify it exists
if [ ! -d "$SDK_PATH" ]; then
    echo "Error: Generated SDK not found at $SDK_PATH"
    exit 1
fi
```

### 4. Run tests with SDK on path

```bash
PYTHONPATH="$SDK_PATH:.:${PYTHONPATH:-}" \
    pytest tests/python/test_sdk_fixtures.py \
        -v \
        --fixtures-dir "$FIXTURES_CACHE_DIR"
```

This makes `import goat_sdk` load from the **generated** SDK, not the generator.

### 5. Parametrized tests execute

Each test:

1. Loads a fixture (e.g., `basic_taxon_search.json`)
2. Reconstructs the query using the **generated** SDK
3. Compares the reconstructed URL to the original
4. Validates the SDK produces correct results

```python
# Inside test_sdk_fixtures.py
from goat_sdk import QueryBuilder  # From generated SDK!

def test_fixture_query_builds(fixture_name, fixture_data):
    """Reconstruct the original query using generated SDK."""
    # Parse fixture and rebuild with generated QueryBuilder
    qb = FIXTURE_TO_BUILDER[fixture_name](fixture_data)
    rebuilt_url = qb.to_url()

    # Verify the rebuilt query matches original
    assert rebuilt_url == fixture_data['original_url']
```

All **14 fixtures** × **multiple test types** = **40+ tests** per language

### 6. Results

```
pytest output:
test_fixture_no_api_error[basic_taxon_search] PASSED
test_fixture_no_api_error[numeric_field_integer_filter] PASSED
...
test_pagination_size_respected PASSED
✓ All fixtures validated against goat SDK
✓ goat SDK is production-ready
```

## Troubleshooting

### "Fixtures not found" error

Fixtures are auto-discovered on first run, but if you get an error:

```bash
# For goat site
python tests/python/discover_fixtures.py --site goat --update

# For custom API
python tests/python/discover_fixtures.py --api-base https://custom.site.org/api --update
```

### "Generated SDK not found" error

Verify the SDK was generated correctly:

```bash
# Generate it
bash scripts/dev_site.sh --python --output ./workdir/my-goat goat

# Verify structure
ls -la workdir/my-goat/goat-cli/python/goat_sdk/
```

### Test failures after SDK changes

If tests fail after modifying templates:

```bash
# 1. Regenerate SDK
bash scripts/dev_site.sh --python --output ./workdir/my-goat goat

# 2. Force fresh fixtures (optional - only if API changed)
rm -rf tests/python/fixtures-goat/

# 3. Re-run tests
bash scripts/test_sdk_fixtures.sh --site goat --python

# 4. Check the actual error in output
```

### Python import issues

If tests can't find the generated SDK:

```bash
# Verify the path exists
ls -la workdir/my-goat/goat-cli/python/goat_sdk/__init__.py

# Debug imports manually
PYTHONPATH="workdir/my-goat/goat-cli:.:${PYTHONPATH:-}" \
    python -c "import goat_sdk; print(goat_sdk.__file__)"

# Should output the generated SDK location, not cli-generator's version
```

### Site-specific fixture issues

If you're testing goat but getting mouse fixture results:

```bash
# Verify correct site cache is being used
ls tests/python/fixtures-goat/
# Should contain: basic_taxon_search.json, numeric_field_*.json, etc.

# Check what API was queried
grep -r "genomehubs.org" tests/python/fixtures-goat/ | head -1
# Should show results from goat.genomehubs.org, not another site
```

### Clear all caches and start fresh

```bash
# Remove all fixture caches
rm -rf tests/python/fixtures-*/

# Remove generated SDKs
rm -rf workdir/

# Regenerate everything
bash scripts/dev_site.sh --python --output ./workdir/my-goat goat

# Re-run tests (will auto-discover fixtures)
bash scripts/test_sdk_fixtures.sh --site goat --python
```

## Next Steps

**Phase 6.4:** Edge Cases & Error Handling

- Test error fixture scenarios (invalid queries, edge cases)
- Validate error messages from SDKs
- Boundary value testing across fixture ranges

**Phase 6.5:** CI/CD Integration

- Add fixture tests to GitHub Actions
- Per-site fixture caching strategy
- Automated testing on every PR

**Phase 7:** Performance Benchmarking

- Use fixtures as performance baselines
- Measure SDK speed across languages
- Track improvements/regressions

**Phase 8:** Advanced Testing

- Fuzzing based on fixture patterns
- Property-based testing (Hypothesis)
- Mutation testing to find edge cases

## Related Commands

### Fixture Discovery

```bash
# Discover fixtures for goat site
python tests/python/discover_fixtures.py --site goat --update

# Discover for any API endpoint
python tests/python/discover_fixtures.py --api-base https://site.org/api --update

# View discovery help
python tests/python/discover_fixtures.py --help
```

### SDK Generation

```bash
# Generate Python SDK for goat
bash scripts/dev_site.sh --python --output ./workdir/my-goat goat

# Generate all languages for goat
bash scripts/dev_site.sh --all --output ./workdir/my-goat goat

# Generate for multiple sites
bash scripts/dev_site.sh --python --output ./workdir/test-goat goat
bash scripts/dev_site.sh --python --output ./workdir/test-mouse mouse
```

### Fixture Testing

```bash
# Test specific site and language
bash scripts/test_sdk_fixtures.sh --site goat --python

# Test all languages for a site
bash scripts/test_sdk_fixtures.sh --site goat --all

# Test with custom API
bash scripts/test_sdk_fixtures.sh --api-base https://custom.org/api --python

# View test help
bash scripts/test_sdk_fixtures.sh --help
```

### Other Validation Tests

```bash
# Test SDK generation script itself
bash scripts/test_sdk_generation.sh --verbose

# Run parity tests (code structure validation)
pytest tests/python/test_sdk_parity.py -v

# Run documentation parity tests
pytest tests/python/test_sdk_parity.py::TestDocumentationParity -v

# Run ALL validation tests (fixtures + parity + generation)
pytest tests/python/ -v
```

## File Structure

```
cli-generator/
├── scripts/
│   ├── test_sdk_generation.sh     ← Generate SDKs (Phase 6.2)
│   ├── test_sdk_fixtures.sh       ← Test fixtures against SDKs (Phase 6.3) ← You are here
│   └── dev_site.sh                ← Full build + dev server
├── tests/python/
│   ├── discover_fixtures.py       ← Auto-discover from any site's API
│   ├── test_sdk_fixtures.py       ← Parametrized tests (40+ tests)
│   ├── test_sdk_parity.py         ← Code/docs parity checks
│   └── fixtures-{SITE}/           ← Per-site fixture caches
│       ├── fixtures-goat/         ← From goat.genomehubs.org/api
│       │   ├── basic_taxon_search.json
│       │   ├── numeric_field_integer_filter.json
│       │   ... (14 total)
│       ├── fixtures-mouse/        ← From mouse.genomehubs.org/api
│       │   ├── basic_taxon_search.json
│       │   ... (14 total, may differ from goat)
│       └── fixtures-plant/        ← From plant.genomehubs.org/api
│           ... (14 total)
└── workdir/
    ├── my-goat/
    │   └── goat-cli/
    │       ├── python/goat_sdk/   ← Generated Python SDK
    │       ├── js/goat/           ← Generated JavaScript SDK
    │       ├── r/goat/            ← Generated R SDK
    │       └── docs/              ← Generated Quarto documentation
    ├── my-mouse/
    │   └── mouse-cli/
    │       ├── python/mouse_sdk/
    │       ... (similar structure)
    └── my-plant/
        └── plant-cli/
            ... (similar structure)
```

### Key Directories Explained

**`tests/python/fixtures-{SITE}/`** (Site-Specific Fixture Caches)

- Auto-populated on first run of `test_sdk_fixtures.sh --site {SITE}`
- Contains 14 JSON files with real API responses
- Each site has independent cache (fixtures from different APIs)
- Can be deleted to force re-discovery

**`workdir/`** (Generated SDKs)

- Each site gets its own directory
- Contains compiled SDKs in Python, JavaScript, R
- Can be regenerated anytime with `dev_site.sh`
- Tests import SDKs from here, not from generator

**`tests/python/test_sdk_fixtures.py`** (Test Suite)

- Parametrized tests using fixtures from `fixtures-{SITE}/`
- Tests any language (Python default, JS, R with flags)
- Independent of which SDK/site you're testing
