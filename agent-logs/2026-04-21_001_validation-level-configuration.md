# Validation Level Configuration for v3 API Preparation

**Date:** 2026-04-21
**Session:** 001_validation-level-configuration
**Duration:** ~1.5 hours
**Status:** ✅ Complete

---

## Summary

Implemented configurable validation levels ("`partial`" and "`full`") in both JavaScript and Python SDKs to support phased v3 API migration. The configuration allows users to delay API fetch attempts until v3 endpoints are deployed, avoiding 404 log spam during the transition period.

**Key Achievement:** QueryBuilder now accepts validation level settings that gracefully handle missing v3 API endpoints while maintaining forward compatibility.

---

## Changes Made

### 1. JavaScript Template (`templates/js/query.js`)

**Constructor Update:**

- Added `options` parameter with `validationLevel` ("full" | "partial") and `apiBase` defaults
- Stores `_validationLevel` and `_apiBase` as instance variables
- Defaults: `validationLevel="full"`, `apiBase=API_BASE` (site-specific)

**`validate()` Method:**

- Added `validationLevel` parameter for per-call override
- **Full mode logic:**
  - Attempts fetch from `/api/v3/metadata/fields` and `/api/v3/metadata/validation-config`
  - Gracefully silences 404s and network errors (no console spam)
  - Falls back to local files or empty metadata if API unavailable
- **Partial mode logic:**
  - Skips API fetch entirely
  - Uses only local embedded metadata files

**Graceful Degradation:**

- Wrapped fs import in try-catch for browser safety
- Returns JSON.parse() with fallback to single-error array
- Maintains compatibility with both Node.js and browsers

### 2. Python Template (`templates/python/query.py.tera`)

**Constructor Update:**

- Added `validation_level` parameter (default "full")
- Added `api_base` parameter (default `API_BASE` constant)
- Stores both as instance variables `_validation_level` and `_api_base`
- Docstring explains modes and v3 integration strategy

**`validate()` Method:**

- Added `validation_level` parameter for per-call override
- **Full mode logic:**
  - Uses `urllib.request.urlopen()` with 5-second timeout
  - Attempts to fetch from `/api/v3/metadata/fields` and `/api/v3/metadata/validation-config`
  - Silently catches `HTTPError` (404), `URLError`, `OSError`, and `TimeoutError`
  - Falls back to local files or empty metadata
- **Partial mode logic:**
  - Skips API fetch entirely
  - Local file fallback only

**Error Handling:**

- Graceful timeout handling (5 seconds)
- Silent failure for all HTTP/network errors
- Comprehensive exception catching prevents log spam

### 3. Main Python SDK (`python/cli_generator/query.py`)

**Constructor Update:**

- Added `validation_level` parameter (default "full")
- Added `api_base` parameter (default "https://genomehubs.org")
- Docstring documents both modes and v3 integration
- Note: This file doesn't have `validate()` method since it's for the generator itself, not generated projects

### 4. Browser Tests (`workdir/my-goat/goat-cli/js/goat/index.html`)

**Enhanced `testValidate()` Function:**

- Test 1: Partial mode with Mammalia query → validates without API calls
- Test 2: Full mode with Homo sapiens query → demonstrates graceful 404 handling
- Test 3: Invalid query detection with unknown field → tests error reporting
- All tests confirm proper async/await behavior with validation method

**Output Structure:**

- Clear distinction between partial and full validation modes
- Documents that API endpoints return 404 (v3 not ready) without error spam
- Shows fallback behavior in action

---

## Code Integration Points

### Files Modified:

1. `templates/js/query.js` — JavaScript SDK template
2. `templates/python/query.py.tera` — Python SDK template (generated)
3. `python/cli_generator/query.py` — Main Python SDK (documentation)
4. `workdir/my-goat/goat-cli/js/goat/index.html` — Browser test page

### Generated Project Files Updated:

- `./workdir/my-goat/goat-cli/python/goat_sdk/query.py` ✅ Verified
- `./workdir/my-goat/goat-cli/js/goat/query.js` ✅ Verified
- `./workdir/my-goat/goat-cli/js/goat/index.html` ✅ Updated with test cases

---

## Validation & Testing

### Python Validation (Automated Test):

```bash
python3 /tmp/test_validation_levels.py
```

✅ **Results:**

- Default mode detected as "full" ✓
- Partial mode correctly set ✓
- Custom API base accepted ✓
- Constructor parameters verified ✓
- `validate()` method signature confirmed ✓

### Dev Site Build:

```bash
bash scripts/dev_site.sh --rebuild-wasm --browser --python --output ./workdir/my-goat goat
```

✅ **Results:**

- Dual WASM builds successful (web + nodejs) ✓
- Python extension compiled via maturin ✓
- JS and Python smoke tests passed ✓
- All generated files correct ✓

### Browser Testing:

- ✅ Module loads correctly in Node.js context
- ✅ Partial validation executes without API calls
- ✅ Full validation gracefully handles missing API endpoints
- ✅ Error reporting works for invalid queries
- ✅ Async/await properly integrated

---

## Architectural Decisions

### 1. API Fetch Strategy

**Chosen:** Graceful 404 handling with silent failures

- No console.error spam from unavailable endpoints
- Allows production UI to function immediately with partial validation
- Seamless upgrade to full validation when v3 endpoints deployed

**Alternative Rejected:** Conditional API fetch based on environment

- Would require additional configuration complexity
- Less forward-compatible with mixed deployment scenarios

### 2. Default Mode

**Chosen:** `validationLevel="full"` (default)

- Enables automatic upgrade when v3 API ready
- Users can opt into "partial" temporarily if needed
- Zero migration cost for production deployments

**Rationale:** Forward compatibility + opt-out option

### 3. Error Handling

**Silent failures for:**

- HTTP 404 (endpoint not deployed)
- Network timeouts (temporary unavailability)
- fs import errors (browser environment)

**Rationale:** Validation should never block query execution; metadata is enhancement, not requirement

---

## Integration with api-refactoring-plan.md

### Current State (Pre-v3):

- Users create `QueryBuilder(index, validation_level="partial")`
- Validation uses only embedded metadata
- No API overhead or 404 log spam

**Example:**

```python
# Until v3 API ready
qb = QueryBuilder("taxon", validation_level="partial")
```

### Future State (v3 Deployed):

- v3 API deployed with endpoints:
  - `GET /api/v3/metadata/fields`
  - `GET /api/v3/metadata/validation-config`
- Users can upgrade to: `QueryBuilder(index, validation_level="full")`
- Full metadata-driven validation automatically enabled

**Example:**

```python
# After v3 API deployment
qb = QueryBuilder("taxon", validation_level="full")  # Now uses API
```

### Zero Migration Cost:

- Existing code with `validation_level="partial"` keeps working
- No code changes needed when v3 ready; just change setting
- Graceful fallback handles mixed deployment scenarios (some endpoints available, others not)

---

## Testing Checklist

- ✅ Python constructor accepts `validation_level` and `api_base` parameters
- ✅ JavaScript constructor accepts options object with both keys
- ✅ Both SDKs default to "full" mode
- ✅ Python `urllib` fetch properly silences HTTP errors
- ✅ JavaScript `fetch` API properly handles 404s without logs
- ✅ Local file fallback works when API unavailable
- ✅ Per-call override works (parameter to `validate()`)
- ✅ Browser tests pass for both partial and full modes
- ✅ Invalid query detection still works correctly
- ✅ Dev site build succeeds with all smoke tests passing

---

## Documentation

**Inline Documentation Added:**

- Constructor docstrings explain modes and v3 integration
- `validate()` docstrings document the two modes and override parameter
- Comments in code explain graceful fallback logic
- Browser test output clearly shows mode switching

**Future Documentation (api-refactoring-plan.md):**

- [ ] Link this agent log in v3 API section
- [ ] Add migration guide for switching from partial to full
- [ ] Document the three metadata sources: API (v3+), local files (all versions), empty (fallback)

---

## Known Limitations & Trade-offs

1. **Timeout not configurable:** 5 seconds hardcoded in Python
   - Rationale: Sufficient for metadata endpoints; can be parameterized if needed
   - Mitigation: Partial mode available for users needing faster startup

2. **No progress indication:** Silent failures make debugging harder
   - Rationale: Matches design requirement ("no 404 log spam")
   - Mitigation: Can add `?debug=true` mode in future if needed

3. **IPv6 compatibility:** Not explicitly tested
   - Impact: Low (genomehubs infrastructure assumed dual-stack)
   - Future: Add explicit IPv6 tests if issues reported

---

## Files Changed Summary

| File                                                | Changes                                    | Status            |
| --------------------------------------------------- | ------------------------------------------ | ----------------- |
| `templates/js/query.js`                             | Constructor + validate() with levels       | ✅ Committed      |
| `templates/python/query.py.tera`                    | Constructor + validate() with urllib fetch | ✅ Committed      |
| `python/cli_generator/query.py`                     | Constructor docs only (no validate)        | ✅ Committed      |
| `workdir/my-goat/goat-cli/js/goat/index.html`       | Enhanced test with both modes              | ✅ Updated        |
| `workdir/my-goat/goat-cli/js/goat/query.js`         | Generated from template                    | ✅ Auto-generated |
| `workdir/my-goat/goat-cli/python/goat_sdk/query.py` | Generated from template                    | ✅ Auto-generated |

---

## Next Steps

1. **API Refactoring (Separate Task):**
   - Implement v3 API endpoints:
     - `GET /api/v3/metadata/fields`
     - `GET /api/v3/metadata/validation-config`
   - Update api-refactoring-plan.md with endpoint schemas

2. **User Migration Guide:**
   - Document how to switch from partial to full validation
   - Provide upgrade checklist for deployments

3. **Monitoring (Optional):**
   - Track which mode most users adopt
   - Add telemetry for API endpoint availability

---

## Closing Notes

This implementation successfully bridges the gap between current partial validation (until v3 API ready) and future full validation (when endpoints deployed). The design prioritizes:

- **No production impact** — existing code works immediately
- **Zero migration cost** — just change a setting when ready
- **No log spam** — graceful 404 handling
- **Forward compatible** — automatic upgrade path built in

The code is production-ready and can be deployed immediately without requiring v3 API endpoints to be available.
