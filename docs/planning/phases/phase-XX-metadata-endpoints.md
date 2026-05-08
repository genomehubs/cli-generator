# Phase XX: Metadata Endpoint Methods

**Status:** Design capture (not sequenced into ordered phases yet)
**Rationale:** Discovery/introspection endpoints useful for programmatic SDK use but not required for core query workflows
**Priority:** Post-6h nice-to-have; unblock before any "build a UI on top of the SDK" use case
**Depends on:** Phase 6b (v3 SDK migration complete)

---

## Overview

The v3 API exposes four metadata endpoints that return information about the API itself — what indices exist, what fields are available, what taxonomies are loaded, what ranks are recognised. None of these are currently surfaced as SDK methods or CLI subcommands.

They are pure GET requests with no query body, returning stable JSON that changes only when the API schema is updated. This makes them excellent candidates for client-side caching.

---

## Endpoints

| Endpoint          | Method | Returns                                                   | SDK method name        |
| ----------------- | ------ | --------------------------------------------------------- | ---------------------- |
| `/indices`        | GET    | List of available index names                             | `indices()`            |
| `/resultFields`   | GET    | Field metadata per index (name, type, units, description) | `result_fields(index)` |
| `/taxonomies`     | GET    | List of available taxonomy names                          | `taxonomies()`         |
| `/taxonomicRanks` | GET    | List of recognised taxonomic rank names                   | `taxonomic_ranks()`    |

These are read-only, index-independent (except `/resultFields` which is filtered by index). They have no query or params YAML — the full response is the API's self-description.

---

## Implementation

### 1. Rust transport helpers

Add to `crates/genomehubs-query/src/` a new file `meta.rs`:

```rust
//! Thin helpers that fetch metadata endpoints and return raw JSON strings.
//! These are blocking (matching the rest of the SDK transport pattern).

use std::io::Read;

/// Fetch the list of available indices from `{api_base}/v3/indices`.
pub fn fetch_indices(api_base: &str) -> Result<String, String> { ... }

/// Fetch field metadata for a given index from `{api_base}/v3/resultFields?result={index}`.
pub fn fetch_result_fields(api_base: &str, index: &str) -> Result<String, String> { ... }

/// Fetch the list of available taxonomies from `{api_base}/v3/taxonomies`.
pub fn fetch_taxonomies(api_base: &str) -> Result<String, String> { ... }

/// Fetch the list of taxonomic ranks from `{api_base}/v3/taxonomicRanks`.
pub fn fetch_taxonomic_ranks(api_base: &str) -> Result<String, String> { ... }
```

All functions return `Result<String, String>` (raw JSON or error string) following the same pattern as `parse_*` functions.

**PyO3 exposure in `src/lib.rs`:**

```rust
#[pyfunction]
fn fetch_indices(api_base: &str) -> PyResult<String> { ... }

#[pyfunction]
fn fetch_result_fields(api_base: &str, index: &str) -> PyResult<String> { ... }

#[pyfunction]
fn fetch_taxonomies(api_base: &str) -> PyResult<String> { ... }

#[pyfunction]
fn fetch_taxonomic_ranks(api_base: &str) -> PyResult<String> { ... }
```

**R extendr exposure in `templates/r/lib.rs.tera` and `extendr-wrappers.R.tera`** — same pattern as `parse_*` functions.

**WASM exposure in `crates/genomehubs-query/src/lib.rs`** — `#[wasm_bindgen]` wrappers.

> **Note:** These are blocking HTTP calls in the Rust helpers. For Python and R that is fine (both use blocking HTTP throughout). For JS/WASM the pattern needs an `async` wrapper (these endpoints are already called with `fetch` in the JS template directly where needed).

---

### 2. Python SDK methods

Add to `python/cli_generator/query.py` and `templates/python/query.py.tera` — as **standalone functions** on the module, not `QueryBuilder` methods, since they are index-independent (except `result_fields`):

```python
def indices(api_base: str = "https://goat.genomehubs.org/api") -> list[str]:
    """Return the list of available indices from the API."""
    ...

def result_fields(
    index: str,
    api_base: str = "https://goat.genomehubs.org/api",
) -> dict[str, Any]:
    """Return field metadata for the given index."""
    ...

def taxonomies(api_base: str = "https://goat.genomehubs.org/api") -> list[str]:
    """Return the list of available taxonomies."""
    ...

def taxonomic_ranks(api_base: str = "https://goat.genomehubs.org/api") -> list[str]:
    """Return the list of recognised taxonomic ranks."""
    ...
```

`result_fields` is also useful as a `QueryBuilder` method (`qb.result_fields()`) since it naturally scopes to the builder's index.

---

### 3. R SDK methods

Add as R6 public methods on `QueryBuilder` for `result_fields` (index-scoped), and as package-level functions for the others:

```r
# Package-level functions
goat_indices <- function(api_base = NULL) { ... }
goat_taxonomies <- function(api_base = NULL) { ... }
goat_taxonomic_ranks <- function(api_base = NULL) { ... }

# QueryBuilder method
qb$result_fields()
```

---

### 4. JS SDK methods

Add as async static methods on `QueryBuilder` and as module-level exports:

```js
// Static methods (index-independent)
static async indices(apiBase = API_BASE) { ... }
static async taxonomies(apiBase = API_BASE) { ... }
static async taxonomicRanks(apiBase = API_BASE) { ... }

// Instance method (index-scoped)
async resultFields(apiBase = API_BASE) { ... }
```

---

### 5. CLI subcommands (generated CLI)

Add global (non-index-specific) subcommands to the generated CLI `main.rs.tera`:

```
goat-cli indices
goat-cli taxonomies
goat-cli taxonomic-ranks
goat-cli result-fields --index taxon
```

These print JSON to stdout. No per-index nesting required since they are not index operations.

---

### 6. Caching

All four endpoints return data that is stable for the lifetime of a running API instance. The SDK should cache results in memory for the session:

**Python:**

```python
_METADATA_CACHE: dict[str, Any] = {}

def _cached_get(url: str) -> Any:
    if url not in _METADATA_CACHE:
        _METADATA_CACHE[url] = json.loads(urllib.request.urlopen(url).read())
    return _METADATA_CACHE[url]
```

**R:** use an environment-based cache (`pkg_env$metadata_cache`).

**JS:** use a `Map` module-level constant.

This is a simple optimisation — no TTL or invalidation needed; the cache lives for the Python process / R session / page load.

---

## Tests

| Test                                                    | Location                                                    |
| ------------------------------------------------------- | ----------------------------------------------------------- |
| `test_fetch_indices_returns_list`                       | `tests/python/test_core.py`                                 |
| `test_fetch_result_fields_returns_dict`                 | `tests/python/test_core.py`                                 |
| `test_fetch_taxonomies_returns_list`                    | `tests/python/test_core.py`                                 |
| `test_fetch_taxonomic_ranks_returns_list`               | `tests/python/test_core.py`                                 |
| Integration: all four hit local API at `localhost:3000` | `tests/python/test_batch_integration.py` (skip without API) |

---

## Ordering within phase

1. Rust `meta.rs` helpers + unit tests (mock HTTP)
2. PyO3 exposure + `.pyi` stubs
3. Python module-level functions + `QueryBuilder.result_fields()`
4. R package-level functions + `qb$result_fields()`
5. JS static methods + `resultFields()` instance method
6. Generated CLI subcommands in `main.rs.tera`
7. In-memory caching across all three SDKs
8. Tests
