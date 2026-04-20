# Phase 6: SDK Testing and CI Integration

## Overview

Phase 6 automates cross-SDK consistency testing to ensure all three generated SDKs (Python, JavaScript, R) maintain feature parity before code is committed.

## Test Infrastructure

### 1. Parity Test Suite (`tests/python/test_sdk_parity.py`)

**Purpose:** Verify that method signatures, parameters, and documentation are consistent across all SDKs.

**Coverage (15 tests):**

- ✅ Method presence: All canonical methods exist in Python and JavaScript
- ✅ Constructor parameters: `validation_level` and `api_base` are present
- ✅ Method parameters: `validate()` accepts `validation_level` override
- ✅ Documentation: Docstrings mention validation modes
- ✅ Utility methods: Extra methods are documented and allowed
- ⏳ R template updates pending (deferred markers in place)

**Run:**

```bash
python -m pytest tests/python/test_sdk_parity.py -v
```

**Expected output:** 11 passed, 4 skipped (R pending)

---

### 2. SDK Generation and Smoke Tests (`scripts/test_sdk_generation.sh`)

**Purpose:** Generate full SDKs from templates and run smoke tests on generated code.

**What it tests:**

- ✅ SDK generation succeeds for each language
- ✅ Python extension compiles and imports correctly
- ✅ JavaScript code structure is valid
- ✅ R code structure is valid
- ✅ Basic method chaining works (Python)
- ✅ Serialization to YAML works (Python)
- ✅ Cross-SDK parity checks pass

**Usage:**

```bash
# Test all SDKs
bash scripts/test_sdk_generation.sh

# Test specific languages
bash scripts/test_sdk_generation.sh --python
bash scripts/test_sdk_generation.sh --js
bash scripts/test_sdk_generation.sh --r

# Verbose output
bash scripts/test_sdk_generation.sh --verbose
```

**Output:**

```
Phase 6: SDK Generation and Testing
========================================
→ Generating goat SDK...
✓ goat SDK generated
→ Testing Python SDK (goat)...
✓ Python extension built
✓ Python smoke tests passed
→ Testing JavaScript SDK (goat)...
✓ JavaScript SDK structure verified
→ Testing R SDK (goat)...
✓ R SDK structure verified
→ Running SDK parity checks...
✓ All parity checks passed

========================================
✓ All SDK tests passed!
Ready for commit.
```

**Exit codes:**

- `0`: All tests passed
- `1`: SDK generation failed
- `2`: Smoke tests failed
- `3`: Parity checks failed

---

## Validation Level Configuration

All SDKs support two validation modes via `validation_level` parameter:

### Full Mode (default)

```python
qb = QueryBuilder("taxon", validation_level="full")
```

- Attempts to fetch metadata from v3 API (`GET /api/v3/metadata/*` endpoints)
- Gracefully handles 404 errors (endpoint not yet deployed)
- Falls back to local embedded files
- **Use when:** Connected to a site with v3 API

### Partial Mode

```python
qb = QueryBuilder("taxon", validation_level="partial")
```

- Uses only embedded validation files (no API calls)
- Faster startup, works offline
- **Use when:** v3 API not yet available, or developing features

### Per-Call Override

```python
errors = qb.validate(validation_level="partial")
```

---

## Verification Checklist

Before committing SDK changes:

```bash
# 1. Run all tests (parity + generation)
bash scripts/verify_code.sh

# 2. Specifically run SDK parity tests
python -m pytest tests/python/test_sdk_parity.py -v

# 3. Generate and test all SDKs (Python, JS, R)
bash scripts/test_sdk_generation.sh --verbose

# 4. Manual smoke test with the main SDK
maturin develop --features extension-module
python -c "from goat_sdk import QueryBuilder; qb = QueryBuilder('taxon'); print(qb.validate())"
```

---

## Common Issues and Solutions

### Python extension build fails

**Problem:** `maturin develop` fails in generated SDK
**Solution:**

1. Ensure Rust toolchain is installed (`rustup update`)
2. Check for syntax errors in `templates/python/query.py.tera`
3. Run parity tests first to catch issues early

### Docstring tests fail

**Problem:** Docstrings don't mention "full" or "partial" mode
**Solution:**

1. Update the relevant template docstring
2. Ensure docstring correctly describes validation modes
3. Re-run parity tests

### R template methods missing

**Problem:** R SDK tests fail (currently skipped)
**Solution:**

1. R templates need updates to match Phase 5 features:
   - Add `validation_level` and `api_base` to `initialize()`
   - Add `validate()` method with per-call override
   - Document validation modes in roxygen comments
2. Update skip markers in parity tests once complete

---

## CI/CD Integration Plan

Phase 6 final step (not yet implemented):

```yaml
# .github/workflows/sdk-integration.yml
name: SDK Parity and Integration Tests

on: [pull_request, push]

jobs:
  parity:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
      - run: python -m pytest tests/python/test_sdk_parity.py -v

  generation:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
      - run: bash scripts/test_sdk_generation.sh
```

**Gates PR merge on:**

- Parity tests pass
- SDK generation succeeds for all languages
- Smoke tests succeed

---

## Next Steps

1. ✅ Phase 6.1: Parity test infrastructure (COMPLETE)
2. ✅ Phase 6.2: SDK generation and testing script (COMPLETE)
3. 🔲 Phase 6.3: Validation mode test fixtures (comprehensive scenarios)
4. 🔲 Phase 6.4: CI/CD integration (GitHub Actions workflow)
5. 🔲 Phase 6.5: R template completion (validation_level + api_base)

---

## References

- [API Aggregation Refactoring Plan](../docs/api-aggregation-refactoring-plan.md) — v3 metadata endpoints
- [SDK Parse Parity Plan](../docs/sdk-parse-parity-plan.md) — phase roadmap
- [AGENTS.md](../AGENTS.md) — Workflow checklist for generated projects
