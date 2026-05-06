# Phase 3c: Testing, Parity Validation, and Documentation

**Phase 3a, 3b Status:** ✅ COMPLETE
**Phase 3c Status:** 🚧 PLANNING
**Current Date:** 6 May 2026

---

## Overview

Phase 3c completes Phase 3 by implementing cross-language testing, parity validation, and documentation for the 5 new SDK methods added in Phase 3b (`search_batch`, `count_batch`, `record`, `lookup`, `summary`).

This phase ensures that:

- All three language bindings (Python, JavaScript, R) have identical method signatures
- Each method is thoroughly tested (unit + integration)
- Documentation is auto-generated and up-to-date
- CI/CD pipeline validates all tests before merge

---

## Phase 3c Tasks

### Task 3c.1: SDK Parity Validation

**Purpose:** Ensure all three language bindings have identical method signatures and parameters.

**Location:** [tests/python/test_sdk_parity.py](../../tests/python/test_sdk_parity.py)

**Work Items:**

| Item                                     | Description                                                                                            | Status  |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------------ | ------- |
| Add batch methods to `CANONICAL_METHODS` | Add entries for `search_batch`, `count_batch`, `record`, `lookup`, `summary` with canonical parameters | ⏳ TODO |
| Python signature validation              | Parse python/cli_generator/query.py and verify batch methods match canonical                           | ⏳ TODO |
| JavaScript signature validation          | Parse templates/js/query.js and verify batch methods match canonical                                   | ⏳ TODO |
| R signature validation                   | Parse templates/r/query.R and verify batch methods match canonical                                     | ⏳ TODO |
| Run parity tests locally                 | Execute `pytest tests/python/test_sdk_parity.py -v`                                                    | ⏳ TODO |
| Verify CI passes                         | Check GitHub Actions workflow for test_sdk_parity.py                                                   | ⏳ TODO |

**Canonical Method Signatures (to be added to CANONICAL_METHODS):**

```python
"search_batch": {
    "params": ["queries", "api_base", "api_version"],
    "python_name": "search_batch",
    "js_name": "searchBatch",
    "r_name": "search_batch",
    "notes": "Max 100 queries per batch request; api_version defaults to 'v3'"
},
"count_batch": {
    "params": ["queries", "api_base", "api_version"],
    "python_name": "count_batch",
    "js_name": "countBatch",
    "r_name": "count_batch",
    "notes": "Returns list/array of hit counts; max 100 queries"
},
"record": {
    "params": ["api_base", "api_version"],
    "python_name": "record",
    "js_name": "record",
    "r_name": "record",
    "notes": "Called on QueryBuilder instance; requires prior query setup"
},
"lookup": {
    "params": ["api_base", "api_version"],
    "python_name": "lookup",
    "js_name": "lookup",
    "r_name": "lookup",
    "notes": "Called on QueryBuilder instance for identifier lookup"
},
"summary": {
    "params": ["api_base", "api_version"],
    "python_name": "summary",
    "js_name": "summary",
    "r_name": "summary",
    "notes": "Called on QueryBuilder instance; returns aggregations"
}
```

---

### Task 3c.2: Batch Operation Unit Tests

**Purpose:** Add unit tests for batch methods across all three languages.

**Test Files:**

- [tests/python/test_batch_operations.py](../../tests/python/test_batch_operations.py) (NEW)
- [tests/javascript/test_batch_operations.mjs](../../tests/javascript/test_batch_operations.mjs) (NEW)
- [tests/r/test_batch_operations.R](../../tests/r/test_batch_operations.R) (NEW)

**Work Items:**

| Item                                              | Description                                             | Status  |
| ------------------------------------------------- | ------------------------------------------------------- | ------- |
| **Python Unit Tests**                             |                                                         |         |
| → Mock HTTP for batch methods                     | Mock urllib.request.urlopen for batch endpoints         | ⏳ TODO |
| → Test search_batch with mock responses           | Verify correct URL, payload structure, response parsing | ⏳ TODO |
| → Test count_batch with mock responses            | Verify hit counts extracted correctly                   | ⏳ TODO |
| → Test record, lookup, summary mocks              | Verify each method builds correct POST requests         | ⏳ TODO |
| → Constraint validation (max 100)                 | Verify errors raised for >100 searches                  | ⏳ TODO |
| **JavaScript Unit Tests**                         |                                                         |         |
| → Mock fetch for batch methods                    | Mock global fetch for batch endpoints                   | ⏳ TODO |
| → Test async searchBatch                          | Verify correct URL, await resolution, error handling    | ⏳ TODO |
| → Test async countBatch, record, lookup, summary  | Verify async/await patterns                             | ⏳ TODO |
| → Constraint validation (max 100)                 | Verify errors thrown for >100 searches                  | ⏳ TODO |
| **R Unit Tests**                                  |                                                         |         |
| → Mock httr::POST for batch methods               | Mock httr library calls                                 | ⏳ TODO |
| → Test search_batch with mock responses           | Verify correct URL, httr parameters, response parsing   | ⏳ TODO |
| → Test count_batch, record, lookup, summary mocks | Verify jsonlite integration                             | ⏳ TODO |
| → Constraint validation (max 100)                 | Verify stop() called for >100 searches                  | ⏳ TODO |

**Test Coverage Targets:**

- Happy path: Single query, multiple queries, full batch (100 queries)
- Error paths: HTTP errors, parse failures, constraint violations
- Edge cases: Empty queries, single query in batch, response with no results

---

### Task 3c.3: Integration Tests Against Live API

**Purpose:** Verify batch methods work correctly against a running API server.

**Prerequisites:**

- API server running locally at http://localhost:3000
- Test fixtures cached (run `python tests/python/discover_fixtures.py --update`)

**Test Files:**

- [tests/python/test_batch_integration.py](../../tests/python/test_batch_integration.py) (NEW)
- [tests/javascript/test_batch_integration.mjs](../../tests/javascript/test_batch_integration.mjs) (NEW)
- [tests/r/test_batch_integration.R](../../tests/r/test_batch_integration.R) (NEW)

**Work Items:**

| Item                                | Description                                                     | Status  |
| ----------------------------------- | --------------------------------------------------------------- | ------- |
| **Python Integration**              |                                                                 |         |
| → Create live API test base         | Set up test class with API connection                           | ⏳ TODO |
| → Test search_batch against API     | Execute batch search against running server                     | ⏳ TODO |
| → Test count_batch against API      | Execute batch count, verify hit counts                          | ⏳ TODO |
| → Test record, lookup, summary live | Execute each against real API responses                         | ⏳ TODO |
| → Verify response parsing           | Validate parse_batch_json, parse_record_json, parse_lookup_json | ⏳ TODO |
| **JavaScript Integration**          |                                                                 |         |
| → Create live API test base         | Set up test runner with API connection                          | ⏳ TODO |
| → Test async searchBatch            | Execute against running server with real responses              | ⏳ TODO |
| → Test async batch operations       | Verify Promise resolution and error handling                    | ⏳ TODO |
| → Verify WASM parsing               | Validate WASM parse functions work with real responses          | ⏳ TODO |
| **R Integration**                   |                                                                 |         |
| → Create live API test base         | Set up test harness with API connection                         | ⏳ TODO |
| → Test batch operations             | Execute against running server                                  | ⏳ TODO |
| → Verify httr POST integration      | Validate POST body structure and response handling              | ⏳ TODO |

**Integration Test Scenarios:**

| Scenario                        | Expected Behavior                     | Notes                   |
| ------------------------------- | ------------------------------------- | ----------------------- |
| Single search in batch          | Returns array with 1 result object    | Basic test              |
| Multiple searches (10) in batch | Returns array with 10 result objects  | Standard use case       |
| Full batch (100 searches)       | Returns array with 100 result objects | Max constraint boundary |
| Batch >100 searches             | Error raised before HTTP call         | Constraint validation   |
| Mixed result sizes              | Each result has correct count/size    | Heterogeneous queries   |
| API error (5xx)                 | Error propagated to caller            | Error handling          |
| Malformed response              | Parse function handles gracefully     | Robustness              |

---

### Task 3c.4: Generated SDK Integration Tests

**Purpose:** Verify batch methods work correctly in generated SDK projects (not just generator library).

**Location:** `scripts/test_sdk_fixtures.sh`, `scripts/dev_site.sh`

**Work Items:**

| Item                                           | Description                                                | Status  |
| ---------------------------------------------- | ---------------------------------------------------------- | ------- |
| Create generated project                       | Run `cli-generator new --site test-batch`                  | ⏳ TODO |
| Add batch operation tests to generated project | Copy integration tests to generated tests/ dir             | ⏳ TODO |
| Test Python generated SDK                      | Build extension via maturin, run pytest                    | ⏳ TODO |
| Test JavaScript generated SDK                  | Run npm test with batch scenarios                          | ⏳ TODO |
| Test R generated SDK                           | Install package, run devtools::test() with batch scenarios | ⏳ TODO |
| Verify generated SDK uses correct endpoints    | Inspect query.py, query.js, query.R in generated output    | ⏳ TODO |

---

### Task 3c.5: Quarto Documentation Updates

**Purpose:** Update auto-generated Quarto documentation to include batch method examples and API reference.

**Location:** [docs/quarto/](../../docs/quarto/) and template-generated docs

**Work Items:**

| Item                                   | Description                                             | Status  |
| -------------------------------------- | ------------------------------------------------------- | ------- |
| Add batch methods to SDK API reference | Generate docstring sections for new methods             | ⏳ TODO |
| Create batch operations tutorial       | Write Quarto guide with batch search/count examples     | ⏳ TODO |
| Update SDK reference docs              | Regenerate SDK function index with batch methods        | ⏳ TODO |
| Add Python examples to Quarto          | Include python/cli_generator/query.py batch method docs | ⏳ TODO |
| Add JavaScript examples to Quarto      | Include templates/js/query.js batch method docs         | ⏳ TODO |
| Add R examples to Quarto               | Include templates/r/query.R batch method docs           | ⏳ TODO |
| Rebuild Quarto site                    | Run `quarto render` to regenerate HTML                  | ⏳ TODO |
| Verify batch method docs rendered      | Check Quarto output contains new methods                | ⏳ TODO |

**Documentation Patterns to Follow:**

Each batch method should document:

1. Method signature (all three languages)
2. Parameters and constraints (e.g., max 100 searches)
3. Return type and structure
4. Error conditions
5. Usage example (Python, JavaScript, R)
6. Integration with parse functions

---

### Task 3c.6: CI/CD Pipeline Updates

**Purpose:** Configure GitHub Actions to run all Phase 3c tests automatically on PR/merge.

**Location:** [.github/workflows/](../../.github/workflows/)

**Work Items:**

| Item                                  | Description                                               | Status  |
| ------------------------------------- | --------------------------------------------------------- | ------- |
| Add parity test to CI                 | Run `pytest tests/python/test_sdk_parity.py -v` in CI     | ⏳ TODO |
| Add Python batch tests to CI          | Run `pytest tests/python/test_batch_*.py -v` in CI        | ⏳ TODO |
| Add JavaScript batch tests to CI      | Run `npm test -- tests/javascript/test_batch_*.mjs` in CI | ⏳ TODO |
| Add R batch tests to CI               | Run `R -f tests/r/test_batch_*.R` in CI                   | ⏳ TODO |
| Set up live API for integration tests | Docker container or test API in CI                        | ⏳ TODO |
| Add generated SDK integration test    | Run full SDK build + test in CI pipeline                  | ⏳ TODO |
| Configure test reporters              | Capture coverage and generate reports                     | ⏳ TODO |
| Block PR merge on test failures       | Set up branch protection rules                            | ⏳ TODO |

---

## Implementation Approach

### Incremental Testing Strategy

1. **Week 1: Unit Tests**
   - Implement Python batch operation unit tests
   - Implement JavaScript batch operation unit tests
   - Implement R batch operation unit tests
   - All tests use mocked HTTP responses

2. **Week 2: Integration Tests**
   - Implement Python integration tests against live API
   - Implement JavaScript integration tests against live API
   - Implement R integration tests against live API
   - Run `scripts/dev_site.sh --python` to test generated SDK

3. **Week 3: Parity & Documentation**
   - Update test_sdk_parity.py with batch method definitions
   - Run all parity tests to verify cross-language consistency
   - Update Quarto documentation
   - Create agent-log documenting all work

4. **Week 4: CI Integration**
   - Update GitHub Actions workflows
   - Run full test suite in CI
   - Verify all tests pass on main branch

---

## Key Validation Checkpoints

### Checkpoint 1: Parity Tests Pass

```bash
pytest tests/python/test_sdk_parity.py -v
# ✅ All batch methods found in canonical definitions
# ✅ Python signatures match canonical
# ✅ JavaScript signatures match canonical
# ✅ R signatures match canonical
```

### Checkpoint 2: Unit Tests Pass (All Languages)

```bash
pytest tests/python/test_batch_operations.py -v
npm test -- tests/javascript/test_batch_operations.mjs
R -f tests/r/test_batch_operations.R
# ✅ 100% coverage of happy path
# ✅ Error cases handled correctly
# ✅ Constraints enforced
```

### Checkpoint 3: Integration Tests Pass (Live API)

```bash
# Start API server
cd crates/genomehubs-api && cargo run &

# Run integration tests
pytest tests/python/test_batch_integration.py -v
npm test -- tests/javascript/test_batch_integration.mjs
R -f tests/r/test_batch_integration.R
# ✅ Batch operations work against running API
# ✅ Response parsing works correctly
# ✅ Error handling works as expected
```

### Checkpoint 4: Generated SDK Works

```bash
bash scripts/dev_site.sh --python goat
# ✅ Generated Python SDK builds successfully
# ✅ Generated tests pass
# ✅ Batch methods available in generated project
```

### Checkpoint 5: CI Tests Pass

```bash
# All GitHub Actions workflows pass on PR
# ✅ Parity tests pass
# ✅ Unit tests pass (Python, JS, R)
# ✅ Integration tests pass
# ✅ Generated SDK tests pass
```

---

## Testing Infrastructure

### Test Helpers and Fixtures

**Python test utilities** (to be added to tests/python/conftest.py):

- `mock_batch_response()` — Generate mock batch responses
- `create_test_queries()` — Create list of QueryBuilder objects
- `assert_batch_url_structure()` — Verify batch POST body structure

**JavaScript test utilities** (to be added to tests/javascript/helpers.js):

- `mockFetch()` — Mock global fetch for batch endpoints
- `createTestQueries()` — Create array of QueryBuilder objects
- `assertBatchPayload()` — Verify batch POST body structure

**R test utilities** (to be added to tests/r/helpers.R):

- `mock_httr_post()` — Mock httr::POST calls
- `create_test_queries()` — Create list of QueryBuilder objects
- `assert_batch_payload()` — Verify batch POST body structure

### API Test Server Setup

For integration tests, use:

- Local development server (crates/genomehubs-api)
- Start in background: `./target/debug/genomehubs-api &`
- Health check endpoint: `curl http://localhost:3000/api/v3/status`

### Generated SDK Test Framework

Use scripts/dev_site.sh with:

- `--site goat` — Use GoaT as test site
- `--python` — Build and test Python SDK
- `--no-rebuild-wasm` — Skip WASM rebuild (unless templates changed)

---

## Validation Against Batch Endpoint Constraints

### Batch Endpoints to Validate

| Endpoint              | Max Searches     | Response Format      | Parse Function       |
| --------------------- | ---------------- | -------------------- | -------------------- |
| `/api/v3/searchBatch` | 100              | Batch response       | `parse_batch_json`   |
| `/api/v3/countBatch`  | 100              | Batch response       | `parse_batch_json`   |
| `/api/v3/record`      | 1 (single query) | Record response      | `parse_record_json`  |
| `/api/v3/lookup`      | 1 (single query) | Lookup response      | `parse_lookup_json`  |
| `/api/v3/summary`     | 1 (single query) | Aggregation response | N/A (parse directly) |

### Constraint Tests

Each test file must verify:

1. **Max Search Constraint (100)**

   ```
   ✓ 1 search succeeds
   ✓ 50 searches succeed
   ✓ 100 searches succeed
   ✗ 101 searches raise error immediately (no HTTP call made)
   ```

2. **Parse Function Integration**

   ```
   ✓ parse_batch_json correctly parses multi-result responses
   ✓ parse_record_json correctly parses single record responses
   ✓ parse_lookup_json correctly parses lookup results
   ```

3. **Cross-Language Consistency**
   ```
   ✓ All three languages produce identical HTTP requests for same input
   ✓ All three languages produce identical parsed output from same response
   ✓ Error conditions handled consistently across languages
   ```

---

## Deliverables

### Code Deliverables

- [ ] [tests/python/test_batch_operations.py](../../tests/python/test_batch_operations.py) — Python unit tests
- [ ] [tests/python/test_batch_integration.py](../../tests/python/test_batch_integration.py) — Python integration tests
- [ ] [tests/javascript/test_batch_operations.mjs](../../tests/javascript/test_batch_operations.mjs) — JavaScript unit tests
- [ ] [tests/javascript/test_batch_integration.mjs](../../tests/javascript/test_batch_integration.mjs) — JavaScript integration tests
- [ ] [tests/r/test_batch_operations.R](../../tests/r/test_batch_operations.R) — R unit tests
- [ ] [tests/r/test_batch_integration.R](../../tests/r/test_batch_integration.R) — R integration tests
- [ ] Updated [tests/python/test_sdk_parity.py](../../tests/python/test_sdk_parity.py) with batch methods
- [ ] Updated GitHub Actions workflows (`.github/workflows/`)

### Documentation Deliverables

- [ ] Batch method docs in [docs/quarto/](../../docs/quarto/)
- [ ] Integration guide for generated SDKs
- [ ] Agent-log documenting Phase 3c work

---

## Success Criteria

**Phase 3c is complete when:**

1. ✅ Parity tests pass (all 3 languages have correct signatures)
2. ✅ Unit tests pass (all batch operations work with mocked HTTP)
3. ✅ Integration tests pass (all batch operations work with live API)
4. ✅ Generated SDK integration tests pass
5. ✅ Quarto documentation is updated and renders correctly
6. ✅ CI/CD pipeline runs all tests successfully on main branch
7. ✅ All code follows project coding standards (formatting, linting, type checking)
8. ✅ Agent-log created documenting all work

**Phase 3 overall is complete when Phase 3c succeeds.**

---

## Notes

- Batch methods (`search_batch`, `count_batch`) operate on multiple QueryBuilder objects and are distinct from fixture tests (which test individual builder methods)
- Single-query methods (`record`, `lookup`, `summary`) can potentially be tested via fixtures, but benefit from dedicated tests
- All integration tests should run against a consistent test API configuration
- Cross-language parity is critical: identical inputs should produce identical HTTP requests and parsed outputs
