# SDK Testing Scripts — Quick Command Reference

Three complementary scripts for testing at different levels:

## 1. `scripts/test_sdk_generation.sh`

**Full integration test (generator → SDK generation → validation)**

```bash
bash scripts/test_sdk_generation.sh --verbose
```

### What it does:

1. Generates Python, JavaScript, R SDKs from templates
2. Compiles Python extension (`maturin develop`)
3. Runs Python smoke tests (method chaining)
4. Verifies JS/R SDK structure
5. Runs SDK parity tests
6. **NEW:** Integrates fixture tests (Phase 6.3)

### Use this when:

- You want end-to-end validation
- Testing template changes
- CI/CD pipeline
- Full build verification

### Time: ~5-10 minutes

### Output examples:

```
→ Building Python SDK...
✓ Python extension compiled successfully
→ Running Python smoke tests...
✓ Python method chaining tests passed
→ Validating JavaScript SDK structure...
✓ JavaScript SDK structure valid
→ Running parity tests...
test_python_canonical_methods_present PASSED
... (14 more parity tests)
✓ All validation passed
```

---

## 2. `scripts/test_sdk_fixtures.sh`

**Fixture-based testing (tests SDK against real API scenarios)**

```bash
# Test generated SDK with live API fixtures
bash scripts/test_sdk_fixtures.sh --site goat --python

# Or test all languages
bash scripts/test_sdk_fixtures.sh --site goat --all
```

### What it does:

1. Loads 14 cached API response fixtures
2. Imports generated SDK from `workdir/my-goat/goat-cli/`
3. Runs 40+ parametrized tests:
   - URL generation
   - Field type handling
   - Pagination
   - Data transformations
   - Error handling

### Use this when:

- Testing against real API scenarios
- Validating generated SDK behavior
- After SDK generation
- Checking parameter combinations

### Prerequisites:

```bash
# Must be done once:
python tests/python/discover_fixtures.py --update
bash scripts/dev_site.sh --python --output ./workdir/my-goat goat
```

### Time: ~2-3 seconds

### Output examples:

```
→ Testing fixtures against goat SDK
✓ Found 14 cached fixtures
✓ Found generated SDK at: workdir/my-goat/goat-cli
→ Testing Python SDK...
test_fixture_no_api_error[basic_taxon_search] PASSED
test_fixture_no_api_error[numeric_field_integer_filter] PASSED
test_builder_creates_valid_url[basic_taxon_search] PASSED
... (40+ tests)
✓ Python fixture tests passed
✓ All fixture tests passed for goat SDK
```

---

## 3. `tests/python/discover_fixtures.py`

**Fixture management (caches real API responses)**

```bash
# First time: query live API and cache responses
python tests/python/discover_fixtures.py --update

# Check what's cached
python tests/python/discover_fixtures.py
```

### What it does:

1. Queries 14 standard query patterns against `goat.genomehubs.org/api`
2. Caches JSON responses in `tests/python/fixtures/`
3. Validates responses are valid
4. Shows result counts per fixture

### Use this when:

- Initial setup (one-time)
- API changes (refresh fixtures)
- Adding new fixture scenarios

### Time: 30-60 seconds (one-time)

### Output examples:

```
→ Querying API for basic_taxon_search: Basic taxon search (10 results)...
  ✓ Received 10 results
→ Querying API for numeric_field_integer_filter: Filter by integer field...
  ✓ Received 20 results
... (all 14 fixtures)

✓ Cached 14 fixtures in tests/python/fixtures/

Summary:
  ✓ basic_taxon_search: 10 results (total hits: 234567)
  ✓ numeric_field_integer_filter: 20 results (total hits: 85392)
  ... (all 14)
```

---

## Quick Workflow Examples

### Example 1: Testing a template change

```bash
# 1. Make changes
# (edit templates/python/query.py.tera)

# 2. Regenerate SDK
bash scripts/dev_site.sh --python --output ./workdir/my-goat goat

# 3. Test the SDK
bash scripts/test_sdk_fixtures.sh --site goat --python

# Result: ✓ All 40+ tests pass with your changes
# → Safe to commit!
```

### Example 2: Complete validation before PR

```bash
# Full integration test
bash scripts/test_sdk_generation.sh --verbose

# Output shows:
# ✓ Generation succeeded
# ✓ Compilation succeeded
# ✓ Parity tests passed
# ✓ Fixture tests passed
# → Ready to open PR!
```

### Example 3: First-time setup

```bash
# Step 1: Cache fixtures from live API (one-time)
python tests/python/discover_fixtures.py --update

# Step 2: Generate a sample SDK
bash scripts/dev_site.sh --python --output ./workdir/my-goat goat

# Step 3: Test it
bash scripts/test_sdk_fixtures.sh --site goat --python

# Result: You now have a validated generated SDK!
```

### Example 4: CI/CD pipeline

```yaml
# .github/workflows/test.yml
- name: Cache fixtures (one-time)
  run: python tests/python/discover_fixtures.py --update

- name: Full validation
  run: bash scripts/test_sdk_generation.sh --verbose
  # This includes all tests:
  # - Generation
  # - Compilation
  # - Parity
  # - Fixtures
```

---

## Script Comparison

| Script                   | Purpose                 | Input              | Output             | Time                      |
| ------------------------ | ----------------------- | ------------------ | ------------------ | ------------------------- |
| `test_sdk_generation.sh` | Full build + validation | None               | ✓/✗ all tests pass | 5-10min                   |
| `test_sdk_fixtures.sh`   | Test generated SDK      | Generated SDK path | ✓/✗ all tests pass | 2-3s                      |
| `discover_fixtures.py`   | Cache API responses     | Live API           | JSON files         | 30-60s (1st), <1s (cache) |

---

## Fixture Coverage

All 14 fixtures test:

- ✅ Basic index searches (taxon, assembly, sample)
- ✅ Numeric filters (integer, range, comparisons)
- ✅ Categorical filters (enums, keywords)
- ✅ Taxonomic operations (tree, exclusion, rank)
- ✅ Multi-field selections with modifiers
- ✅ Pagination (various sizes and offsets)
- ✅ Complex multi-filter scenarios
- ✅ Data transformations (describe, snippet, tidy_records)

Each fixture runs through 8+ parametrized tests (40+ total).

---

## Troubleshooting

| Problem                   | Solution                                                            |
| ------------------------- | ------------------------------------------------------------------- |
| "Fixtures not cached"     | `python tests/python/discover_fixtures.py --update`                 |
| "Generated SDK not found" | `bash scripts/dev_site.sh --python --output ./workdir/my-goat goat` |
| Tests fail after changes  | Regenerate SDK, run tests again                                     |
| API timeout               | Retry discovery, or use cached fixtures                             |
| Import errors             | Check PYTHONPATH includes `workdir/my-goat/goat-cli/`               |

---

## Complete Documentation

For more details, see:

- **Testing Generated SDKs:** `docs/testing-generated-sdks.md` ← Start here!
- **Fixtures Quick Reference:** `docs/test-fixtures-quick-reference.md`
- **Fixtures Usage Guide:** `docs/test-fixtures-usage.md`
- **Phase 6 Complete Reference:** `docs/phase-6-complete-reference.md`
