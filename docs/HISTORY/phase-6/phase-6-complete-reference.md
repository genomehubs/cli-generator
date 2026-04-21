# Phase 6.3: Test Fixtures — Complete Setup Guide

## The Complete Picture

You now have a **three-file testing system** that works together:

```
1. discover_fixtures.py
   ↓ (queries live API once, caches results)
2. tests/python/fixtures/*.json
   ↓ (reused by all subsequent tests)
3. test_sdk_fixtures.py OR test_sdk_generation.sh
   ↓ (run multiple times without API calls)
```

## Three Ways to Run Tests

### Option 1: Direct pytest (testing generator SDK)

```bash
# Cache fixtures (one-time)
python tests/python/discover_fixtures.py --update

# Run tests directly
pytest tests/python/test_sdk_fixtures.py -v
```

**Use when:** Testing the generator's own QueryBuilder (`cli_generator.QueryBuilder`)

### Option 2: Convenience script (testing generated SDK)

```bash
# Cache fixtures (one-time)
python tests/python/discover_fixtures.py --update

# Generate SDK
bash scripts/dev_site.sh --python --output ./workdir/my-goat goat

# Test the generated SDK
bash scripts/test_sdk_fixtures.sh --site goat --python
```

**Use when:** Testing a generated SDK (`goat_sdk.QueryBuilder`)

### Option 3: Full integration test (from `test_sdk_generation.sh`)

```bash
# Runs everything in one command
bash scripts/test_sdk_generation.sh --verbose

# What it does:
# 1. Generates SDKs (Python, JS, R)
# 2. Tests Python compilation
# 3. Tests JS structure
# 4. Runs parity tests
# 5. Includes fixture tests
```

**Use when:** Complete end-to-end testing (build → validate)

## Complete Example Workflow

### For a developer making template changes:

```bash
cd /Users/rchallis/projects/genomehubs/cli-generator

# Step 1: Ensure fixtures are cached
python tests/python/discover_fixtures.py --update

# Step 2: Make your changes
# (edit files in templates/, src/core/, etc.)

# Step 3: Regenerate SDK
bash scripts/dev_site.sh --python --output ./workdir/my-goat goat

# Step 4: Test your changes
bash scripts/test_sdk_fixtures.sh --site goat --python

# Output:
# ✓ Found 14 cached fixtures
# ✓ Found generated SDK at: workdir/my-goat/goat-cli
# → Testing Python SDK...
# test_fixture_no_api_error[basic_taxon_search] PASSED
# test_fixture_no_api_error[numeric_field_integer_filter] PASSED
# ... (40+ tests)
# ✓ Python fixture tests passed
# ✓ All fixture tests passed for goat SDK
```

### If tests fail:

```bash
# 1. Check the error details
# 2. Review your template changes
# 3. Regenerate SDK
bash scripts/dev_site.sh --python --output ./workdir/my-goat goat

# 4. Run tests again
bash scripts/test_sdk_fixtures.sh --site goat --python --verbose

# 5. If still failing, debug specific test
pytest tests/python/test_sdk_fixtures.py::TestFixtureValidation::test_builder_creates_valid_url -vv
```

## File Locations

### Fixtures (cached API responses)

```
tests/python/fixtures/
├── basic_taxon_search.json
├── numeric_field_integer_filter.json
├── enum_field_filter.json
├── taxa_filter_tree.json
├── taxa_with_negative_filter.json
├── multiple_fields_single_filter.json
├── fields_with_modifiers.json
├── pagination_size_variation.json
├── pagination_second_page.json
├── complex_multi_constraint.json
├── complex_multi_filter_same_field.json
├── assembly_index_basic.json
└── sample_index_basic.json
```

### Generated SDKs (for testing)

```
workdir/my-goat/goat-cli/
├── python/
│   └── goat_sdk/          ← Python SDK (Rust extension)
├── js/
│   └── goat/              ← JavaScript SDK
├── r/
│   └── goat/              ← R SDK (partial, Phase 6.5)
└── docs/                  ← Quarto documentation
```

## What Gets Tested

### Core functionality

- ✅ All field types (integer, float, keyword, date, enum)
- ✅ All operators (eq, ne, lt, le, gt, ge, exists, missing)
- ✅ All modifiers (min, max, median, mode, count)
- ✅ Taxonomic operations (tree, exclusion, rank)
- ✅ Multi-field selections
- ✅ Pagination (various sizes and pages)
- ✅ Data transformations (describe, snippet, tidy_records)

### Across 14 real-world scenarios

1. Basic taxon search
2. Numeric filters (integer, range)
3. Enum filters
4. Taxonomic tree operations
5. Exclusion filters
6. Multi-field selections
7. Field modifiers
8. Pagination variations
9. Complex multi-constraint queries
10. Multiple filters on same field
11. Assembly index
12. Sample index

### In two SDKs

- ✅ Generator SDK (`cli_generator.QueryBuilder`)
- ✅ Generated SDKs (`goat_sdk.QueryBuilder`, etc.)

## Key Commands Reference

```bash
# Fixture management
python tests/python/discover_fixtures.py --update      # Cache from live API
python tests/python/discover_fixtures.py                # Load from cache

# SDK generation
bash scripts/dev_site.sh --python --output ./workdir/my-goat goat

# Testing
pytest tests/python/test_sdk_fixtures.py -v             # Direct test
bash scripts/test_sdk_fixtures.sh --site goat --python  # Convenience script
bash scripts/test_sdk_generation.sh --verbose           # Full integration

# Debugging
pytest tests/python/test_sdk_fixtures.py -k enum_field_filter -vv
PYTHONPATH="workdir/my-goat/goat-cli:." pytest ...
```

## Performance

| Operation             | Time    | Notes                            |
| --------------------- | ------- | -------------------------------- |
| Fixture discovery     | 30-60s  | One-time, queries live API       |
| Fixture tests         | 2-3s    | Cached, no network               |
| Full integration test | 5-10min | Includes compilation, WASM build |

## Troubleshooting Quick Reference

| Issue                     | Solution                                                            |
| ------------------------- | ------------------------------------------------------------------- |
| "Fixtures not cached"     | `python tests/python/discover_fixtures.py --update`                 |
| "Generated SDK not found" | `bash scripts/dev_site.sh --python --output ./workdir/my-goat goat` |
| Cannot import SDK         | Check `workdir/my-goat/goat-cli/python/goat_sdk/` exists            |
| Tests fail after changes  | Regenerate SDK: `bash scripts/dev_site.sh ...`                      |
| API timeout               | Retry discovery later or use cached fixtures                        |

## Next Phases

**Phase 6.4:** Edge Cases & Error Testing

- Add fixtures for invalid queries
- Test error handling
- Boundary value testing

**Phase 6.5:** CI/CD Integration

- Commit cached fixtures to Git
- GitHub Actions workflow
- Automated testing on PRs

**Phase 7:** Performance & Benchmarks

- Use fixtures to measure SDK speed
- Compare across languages
- Track regressions

**Phase 8:** Advanced Testing

- Fuzzing based on fixtures
- Property-based testing
- Chaos engineering

## Related Documentation

- **Quick Reference:** [test-fixtures-quick-reference.md](test-fixtures-quick-reference.md)
- **Usage Guide:** [test-fixtures-usage.md](test-fixtures-usage.md)
- **Strategy Doc:** [test-fixtures-strategy.md](test-fixtures-strategy.md)
- **Testing Generated SDKs:** [testing-generated-sdks.md](testing-generated-sdks.md) ← Start here!
- **SDK Parity Tests:** [phase-6-sdk-testing.md](phase-6-sdk-testing.md)
- **Documentation Parity:** [phase-6-documentation-parity.md](phase-6-documentation-parity.md)
