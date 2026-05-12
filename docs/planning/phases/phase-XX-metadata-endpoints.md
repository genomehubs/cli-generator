# Phase XX: API URL Restructuring + Metadata Endpoint Methods

**Status:** Design capture — ready to implement, pre-v3-launch blocking
**Rationale:** Align all batch endpoints to the `/<resource>/batch` pattern established by phylopic; rationalise metadata endpoint names to lowercase, RESTful paths; surface metadata as SDK methods.
**Priority:** Must complete before v3 API is declared stable. Breaking changes are acceptable — there are already many v2→v3 breaks.
**Depends on:** Phase 14 (PhyloPic proxy, done) — establishes the `/<resource>/batch` pattern.

---

## Part A: URL Renaming

### A1. Batch endpoint alignment

Phylopic correctly uses `/phylopic` + `/phylopic/batch`. Count and search still use the old camelCase suffix style. Align them.

| Current URL           | New URL                | HTTP method |
| --------------------- | ---------------------- | ----------- |
| `/api/v3/countBatch`  | `/api/v3/count/batch`  | POST        |
| `/api/v3/searchBatch` | `/api/v3/search/batch` | POST        |

No change to `/api/v3/count` or `/api/v3/search` (single-query endpoints stay flat).

### A2. Metadata endpoint rationalisation

All metadata endpoints move under a `/metadata/` prefix. This groups them visually in API docs, makes the URL structure self-documenting, and enables the bare `/metadata` endpoint as a one-stop init call (see below).

| Current URL              | New URL                       | HTTP method | Notes                           |
| ------------------------ | ----------------------------- | ----------- | ------------------------------- |
| `/api/v3/indices`        | `/api/v3/metadata/indices`    | GET         | List of index names             |
| `/api/v3/resultFields`   | `/api/v3/metadata/fields`     | GET         | `?result=taxon` param unchanged |
| `/api/v3/taxonomies`     | `/api/v3/metadata/taxonomies` | GET         | List of taxonomy names          |
| `/api/v3/taxonomicRanks` | `/api/v3/metadata/ranks`      | GET         | Drops camelCase                 |
| _(new)_                  | `/api/v3/metadata`            | GET         | Aggregated response (see below) |

**Recommendation: bare `/metadata`** — Return a single JSON object containing the three non-parameterised resources:

```json
{ "indices": ["taxon","assembly","sample"], "taxonomies": ["ncbi","ott"], "ranks": ["species","genus",...] }
```

Fields are intentionally excluded: they require a `?result=<index>` qualifier so cannot be returned without parameters. The bare `/metadata` call gives clients everything needed to populate UI dropdowns or validate SDK inputs in a single round-trip. This is distinct from `/status` (which reports instance health/capability) — `/metadata` reports schema, not state.

Implement this as a new `metadata.rs` route handler that calls the three sub-handlers and assembles the response. Do not aggregate fields here.

### A3. Assessment: should `/lookup` move under `/search`?

**Recommendation: keep `/lookup` at base level.**

Rationale: `/lookup` is a name-resolution service — given a taxon name string, return matching taxon IDs and display names. It is used as a prerequisite for `/search`, `/count`, `/report`, and `/record` equally. Nesting it under `/search` would imply its output is search-like (it isn't — it returns ID resolution candidates, not records) and would mislead clients that need it before a `/count` or `/report` call.

`/search/lookup` would also be anomalous: `/search` is a POST that queries records; nesting a GET resolution endpoint under it breaks the single-resource-per-path pattern. If the name is wrong, `/resolve` would be more accurate — but that is a pure rename with no structural benefit and a bigger migration cost.

**Decision: no change to `/lookup`.**

---

## Part B: Metadata SDK methods (formerly the whole scope of this doc)

Surface the five metadata endpoints as SDK methods across all three languages. Part A (URL renames) is a prerequisite.

---

## Complete Touchpoint Inventory

### Rust API (`crates/genomehubs-api/`)

#### Route files to rename/move

| Current file                     | Action                                                                                                                        |
| -------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `src/routes/countBatch.rs`       | Rename to `count_batch.rs`; update `path` to `/api/v3/count/batch`; rename handler `post_countBatch` → `post_count_batch`     |
| `src/routes/searchBatch.rs`      | Rename to `search_batch.rs`; update `path` to `/api/v3/search/batch`; rename handler `post_searchBatch` → `post_search_batch` |
| `src/routes/indices.rs`          | Update `path` annotation to `/api/v3/metadata/indices`                                                                        |
| `src/routes/result_fields.rs`    | Update `path` annotation to `/api/v3/metadata/fields`                                                                         |
| `src/routes/taxonomies.rs`       | Update `path` annotation to `/api/v3/metadata/taxonomies`                                                                     |
| `src/routes/taxonomic_ranks.rs`  | Update `path` annotation to `/api/v3/metadata/ranks`                                                                          |
| _(new)_ `src/routes/metadata.rs` | New handler for `GET /api/v3/metadata`; aggregates indices + taxonomies + ranks                                               |

#### `src/routes/mod.rs`

- Remove `#[path = "countBatch.rs"] pub mod count_batch;` → `pub mod count_batch;`
- Remove `#[path = "searchBatch.rs"] pub mod search_batch;` → `pub mod search_batch;`
- Add `pub mod metadata;`

#### `src/main.rs`

Route registration — 8 changes:

```rust
// Before:
.route("/api/v3/countBatch", axum::routing::post(routes::count_batch::post_countBatch))
.route("/api/v3/searchBatch", axum::routing::post(routes::search_batch::post_searchBatch))
.route("/api/v3/indices", get(routes::indices::get_indices))
.route("/api/v3/resultFields", get(routes::result_fields::get_result_fields))
.route("/api/v3/taxonomies", get(routes::taxonomies::get_taxonomies))
.route("/api/v3/taxonomicRanks", get(routes::taxonomic_ranks::get_taxonomic_ranks))

// After:
.route("/api/v3/count/batch", axum::routing::post(routes::count_batch::post_count_batch))
.route("/api/v3/search/batch", axum::routing::post(routes::search_batch::post_search_batch))
.route("/api/v3/metadata", get(routes::metadata::get_metadata))
.route("/api/v3/metadata/indices", get(routes::indices::get_indices))
.route("/api/v3/metadata/fields", get(routes::result_fields::get_result_fields))
.route("/api/v3/metadata/taxonomies", get(routes::taxonomies::get_taxonomies))
.route("/api/v3/metadata/ranks", get(routes::taxonomic_ranks::get_taxonomic_ranks))
```

Handler use statements — update `post_countBatch` → `post_count_batch`, `post_searchBatch` → `post_search_batch`.

#### `src/routes/status.rs`

The `SUPPORTED_PATHS` array — 6 entries change:

```rust
// Before:
"/countBatch",
"/searchBatch",
"/indices",
"/resultFields",
"/taxonomies",
"/taxonomicRanks",

// After:
"/count/batch",
"/search/batch",
"/metadata",
"/metadata/indices",
"/metadata/fields",
"/metadata/taxonomies",
"/metadata/ranks",
```

---

### SDK Templates

#### `templates/python/query.py.tera`

| Method           | Current URL fragment | New URL fragment  |
| ---------------- | -------------------- | ----------------- |
| `search_batch()` | `v3/searchBatch`     | `v3/search/batch` |
| `count_batch()`  | `v3/countBatch`      | `v3/count/batch`  |

#### `templates/js/query.js`

| Method          | Current URL fragment | New URL fragment  |
| --------------- | -------------------- | ----------------- |
| `searchBatch()` | `v3/searchBatch`     | `v3/search/batch` |
| `countBatch()`  | `v3/countBatch`      | `v3/count/batch`  |

#### `templates/r/query.R`

| Method           | Current URL fragment | New URL fragment  |
| ---------------- | -------------------- | ----------------- |
| `search_batch()` | `v3/searchBatch`     | `v3/search/batch` |
| `count_batch()`  | `v3/countBatch`      | `v3/count/batch`  |

#### `templates/rust/client.rs.tera`

Not directly affected (CLI uses `/v3/search` and `/v3/count` single-query endpoints, not batch).

---

### Live Python SDK (`python/cli_generator/query.py`)

Mirror the template changes exactly:

| Method           | Line (approx) | Change                         |
| ---------------- | ------------- | ------------------------------ |
| `search_batch()` | ~1067         | `searchBatch` → `search/batch` |
| `count_batch()`  | ~1121         | `countBatch` → `count/batch`   |

---

### Tests

#### Python (`tests/python/`)

| File                        | What to update                                                                                                                 |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `test_batch_operations.py`  | URL assertion strings: `v3/searchBatch` → `v3/search/batch`, `v3/countBatch` → `v3/count/batch` (lines 92, 109, 173, 219, 324) |
| `test_batch_integration.py` | Test docstrings and any hardcoded URL strings                                                                                  |
| `test_sdk_parity.py`        | No change — method names unchanged, only URLs change                                                                           |

> **Note:** SDK method names (`search_batch`, `searchBatch`, `count_batch`, `countBatch`) do NOT change — only the URL paths they call change.

#### JavaScript (`tests/javascript/`)

| File                         | What to update                                                                                                 |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `test_batch_operations.mjs`  | URL strings: `v3/searchBatch` → `v3/search/batch`, `v3/countBatch` → `v3/count/batch` (lines 52, 72, 219, 324) |
| `test_batch_integration.mjs` | Test description strings only (method calls unchanged)                                                         |

#### R (`tests/r/`)

| File                       | What to update         |
| -------------------------- | ---------------------- |
| `test_batch_integration.R` | Test descriptions only |

#### Rust integration (`tests/api_endpoints.rs`)

URL strings at lines 343, 412, 446, 487, 563 and surrounding:

- `v3/searchBatch` → `v3/search/batch`
- `v3/countBatch` → `v3/count/batch`
- `v3/indices` → `v3/metadata/indices`
- `v3/resultFields` → `v3/metadata/fields`
- `v3/taxonomies` → `v3/metadata/taxonomies`
- `v3/taxonomicRanks` → `v3/metadata/ranks`
- File header comment (line 1)

---

### Examples / Scripts

| File                                           | What to update                                                                                                                                     |
| ---------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| `examples/test-queries.sh`                     | curl URL strings: `$API/countBatch` → `$API/count/batch`, `$API/searchBatch` → `$API/search/batch`, `$API/indices` → `$API/metadata/indices`, etc. |
| `examples/batch/query-batch-search.yaml`       | Header comment URLs                                                                                                                                |
| `examples/batch/query-batch-count-multi.yaml`  | Header comment URLs                                                                                                                                |
| `examples/batch/query-batch-count-single.yaml` | Header comment URLs                                                                                                                                |

---

### Documentation

#### `templates/docs/reference/query-builder.qmd.tera`

- curl examples for `search_batch` / `count_batch`: replace URL strings
- curl examples for all four metadata methods: update to `/metadata/...` paths
- Add `metadata()` method section

#### `docs/resultfields-implementation-guide.md`

All occurrences of `resultFields` in URL contexts → `metadata/fields` (many; bulk replace).

#### `docs/api-audit-executive-summary.md`

All old URL references → new paths.

#### `GETTING_STARTED.md`

`resultFields` reference (line ~351) → `metadata/fields`.

---

## Part B: Metadata SDK methods

The five metadata endpoints (after Part A renames):

| URL                                        | Returns                             | SDK method      | Rust fn name       |
| ------------------------------------------ | ----------------------------------- | --------------- | ------------------ |
| `GET /api/v3/metadata`                     | `{indices, taxonomies, ranks}` dict | `metadata()`    | `fetch_metadata`   |
| `GET /api/v3/metadata/indices`             | `["taxon","assembly","sample"]`     | `indices()`     | `fetch_indices`    |
| `GET /api/v3/metadata/fields?result=taxon` | field metadata dict                 | `fields(index)` | `fetch_fields`     |
| `GET /api/v3/metadata/taxonomies`          | `["ncbi","ott",...]`                | `taxonomies()`  | `fetch_taxonomies` |
| `GET /api/v3/metadata/ranks`               | `["species","genus",...]`           | `ranks()`       | `fetch_ranks`      |

SDK method names are intentionally short (`fields`, `ranks`) — they live on `QueryBuilder` so context is clear. In JavaScript they are camelCase where needed: `fields(index)`, `ranks()`.

### B1. Rust core (`crates/genomehubs-query/src/meta.rs` — new file)

Five `pub fn` helpers returning `Result<String, String>` (raw JSON). Blocking HTTP, matching existing transport pattern.

```rust
pub fn fetch_metadata(api_base: &str, api_version: &str) -> Result<String, String>
pub fn fetch_indices(api_base: &str, api_version: &str) -> Result<String, String>
pub fn fetch_fields(api_base: &str, api_version: &str, index: &str) -> Result<String, String>
pub fn fetch_taxonomies(api_base: &str, api_version: &str) -> Result<String, String>
pub fn fetch_ranks(api_base: &str, api_version: &str) -> Result<String, String>
```

### B2. PyO3 / WASM / extendr exposure

Follow the 6-touchpoint checklist (AGENTS.md):

1. `src/lib.rs` — PyO3 wrappers + `add_function` registration
2. `templates/rust/lib.rs.tera` — mirror wrappers
3. `src/commands/new.rs` — `patch_python_init` imports + `__all__`
4. `python/cli_generator/cli_generator.pyi` — stubs
5. `python/cli_generator/__init__.py` — imports + `__all__`
6. `crates/genomehubs-query/src/lib.rs` — WASM `#[wasm_bindgen]` exports
7. `templates/r/lib.rs.tera` + `extendr-wrappers.R.tera` — extendr bindings

### B3. SDK methods (all three languages)

**Python** (`python/cli_generator/query.py` + `templates/python/query.py.tera`):

```python
def metadata(self, api_base=..., api_version=...) -> dict
def indices(self, api_base=..., api_version=...) -> list[str]
def fields(self, index: str, api_base=..., api_version=...) -> dict
def taxonomies(self, api_base=..., api_version=...) -> list[str]
def ranks(self, api_base=..., api_version=...) -> list[str]
```

**JavaScript** (`templates/js/query.js`): async instance methods. Names: `metadata()`, `indices()`, `fields(index)`, `taxonomies()`, `ranks()`.

**R** (`templates/r/query.R`): R6 public methods. Names: `metadata()`, `indices()`, `fields(index)`, `taxonomies()`, `ranks()`.

**R extendr wrappers** (`templates/r/extendr-wrappers.R.tera` + `templates/r/lib.rs.tera`): five `.Call` wrappers.

### B4. Parity tests

Add five entries to `CANONICAL_METHODS` in `tests/python/test_sdk_parity.py`:

```python
"metadata": {"params": [], "python_name": "metadata", "js_name": "metadata", "r_name": "metadata"},
"indices": {"params": [], "python_name": "indices", "js_name": "indices", "r_name": "indices"},
"fields": {"params": ["index"], "python_name": "fields", "js_name": "fields", "r_name": "fields"},
"taxonomies": {"params": [], "python_name": "taxonomies", "js_name": "taxonomies", "r_name": "taxonomies"},
"ranks": {"params": [], "python_name": "ranks", "js_name": "ranks", "r_name": "ranks"},
```

### B5. Documentation

Add five sections to `templates/docs/reference/query-builder.qmd.tera` following the existing `phylopic` section format, with Python/R/JS/API tab panels and curl examples pointing to the `/metadata/...` URLs.

---

## Implementation order

### Phase A (URL renames — do first, no new functionality)

1. Rename `countBatch.rs` → `count_batch.rs`; rename `searchBatch.rs` → `search_batch.rs`; update handlers inside.
2. Update path annotations in `indices.rs`, `result_fields.rs`, `taxonomies.rs`, `taxonomic_ranks.rs`.
3. Create `src/routes/metadata.rs` — new aggregating handler.
4. Update `routes/mod.rs` — remove `#[path]` overrides; add `pub mod metadata;`.
5. Update `main.rs` — route strings and use statements.
6. Update `routes/status.rs` — `SUPPORTED_PATHS`.
7. Update all three SDK templates (Python/JS/R) — URL strings in `search_batch` / `count_batch` methods.
8. Update `python/cli_generator/query.py` — same two URL strings.
9. Update tests: `api_endpoints.rs`, `test_batch_operations.py`, `test_batch_operations.mjs`.
10. Update examples: `test-queries.sh`, batch YAML headers.
11. Update documentation: `query-builder.qmd.tera`, `resultfields-implementation-guide.md`, `api-audit-executive-summary.md`, `GETTING_STARTED.md`.
12. Run full CI (`bash scripts/verify_code.sh`). Regenerate workdir.

### Phase B (Metadata SDK methods — after A is merged)

Follow the B1→B5 sequence above, using the 6-touchpoint PyO3 checklist from AGENTS.md.
