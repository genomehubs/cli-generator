# SDK Parse Functions, Method Parity, and E2E Testing Plan

## Overview

Four areas to address together to achieve and maintain consistent SDKs
across Python, JavaScript, and R:

1. Method naming standardisation — one canonical name per concept
2. Shared parse functions — API response parsing in Rust via PyO3/WASM/extendr
3. `validate()` parity — query validation in all three SDKs
4. End-to-end testing + CI — generated SDK tests for all languages

---

## Naming conventions (authoritative)

| Prefix | Meaning | Examples |
|---|---|---|
| `set_*` | Replaces a scalar or whole-list value | `set_taxa`, `set_sort`, `set_size`, `set_rank` |
| `add_*` | Appends one item to a list | `add_attribute`, `add_field` |
| `to_*` | Serialises state (no side-effects, no network) | `to_url`, `to_query_yaml` |
| bare verbs | Triggers I/O or computation | `count`, `search`, `validate`, `describe`, `snippet` |

JavaScript: camelCase. Python/R: snake_case.

---

## Canonical method list

| Concept | Python | JavaScript | R |
|---|---|---|---|
| Construct | `__init__(index)` | `constructor(index)` | `initialize(index)` |
| Set taxon filter | `set_taxa(taxa, filter_type)` | `setTaxa(taxa, filterType)` | `set_taxa(..., filter_type)` |
| Set rank | `set_rank(rank)` | `setRank(rank)` | `set_rank(rank)` |
| Set assemblies | `set_assemblies(accessions)` | `setAssemblies(accessions)` | `set_assemblies(accessions)` |
| Set samples | `set_samples(accessions)` | `setSamples(accessions)` | `set_samples(accessions)` |
| Add attribute filter | `add_attribute(name, op, val, mods)` | `addAttribute(name, op, val, mods)` | `add_attribute(name, op, val, mods)` |
| Add response field | `add_field(name, mods)` | `addField(name, mods)` | `add_field(name, mods)` |
| Set name classes | `set_names(classes)` | `setNames(classes)` | `set_names(classes)` |
| Set lineage ranks | `set_ranks(ranks)` | `setRanks(ranks)` | `set_ranks(ranks)` |
| Page size | `set_size(n)` | `setSize(n)` | `set_size(n)` |
| Page number | `set_page(n)` | `setPage(n)` | `set_page(n)` |
| Sort | `set_sort(field, order)` | `setSort(field, order)` | `set_sort(name, direction)` |
| Include estimates | `set_include_estimates(bool)` | `setIncludeEstimates(bool)` | `set_include_estimates(bool)` |
| Taxonomy source | `set_taxonomy(name)` | `setTaxonomy(name)` | `set_taxonomy(name)` |
| Serialise query | `to_query_yaml()` | `toQueryYaml()` | `to_query_yaml()` |
| Serialise params | `to_params_yaml()` | `toParamsYaml()` | `to_params_yaml()` |
| Build URL | `to_url()` | `toUrl()` | `to_url()` |
| Count results | `count()` | `count()` | `count()` |
| Fetch results | `search(format)` | `search(format)` | `search(format)` |
| Validate query | `validate()` | `validate()` | `validate()` |
| Describe query | `describe(meta, mode)` | `describe(meta, mode)` | `describe(meta, mode)` |
| Generate snippets | `snippet(languages, ...)` | `snippet(languages, ...)` | `snippet(languages, ...)` |
| Reset state | `reset()` | `reset()` | `reset()` |
| Merge another QB | `merge(other)` | `merge(other)` | `merge(other)` |
| Merge many QBs | `combine(*builders)` | `combine(...builders)` | `combine(...)` |

**Removed:** `set_fields()` — use `add_field()` per field in all SDKs (R had this;
removed for uniformity with the `add_*` convention).

**No Rust snippet language:** The Rust API is internal to the generated CLI binary,
not a public library interface.

**Snippet languages supported:** `python`, `r`, `javascript`, `cli`.

---

## Phase 0: Method naming + missing setters *(can start immediately)*

### `templates/r/query.R`
- Rename `add_sort` → `set_sort`
- Remove `set_fields`
- Add `modifiers` param to `add_attribute(name, operator, value, modifiers=NULL)` and `add_field(name, modifiers=NULL)`
- Add missing: `set_rank`, `set_assemblies`, `set_samples`, `set_names`, `set_ranks`,
  `set_include_estimates`, `set_taxonomy`, `reset`, `merge`, `combine`

### `templates/js/query.js`
- Rename `_toQueryYaml` → `toQueryYaml` (public)
- Rename `_toParamsYaml` → `toParamsYaml` (public)
- Update `toUrl()` call-sites to use new public names

### `python/cli_generator/query.py`
- Add `to_url()` instance method (wraps `build_url(self.to_query_yaml(), self.to_params_yaml(), ...)`)
- Add `count()` instance method
- Add `search(format)` instance method

**Verification:**
```bash
cargo test && pytest tests/python/ -q
node -e "const {QueryBuilder}=require('./query'); new QueryBuilder('taxon').toQueryYaml()"
Rscript -e "library(goat); qb<-QueryBuilder\$new('taxon'); qb\$set_sort('genome_size','desc')"
```

---

## Phase 1: Fix broken snippet templates *(depends on Phase 0)*

Bugs found in audit:

| Template | Wrong | Correct |
|---|---|---|
| `python_snippet.tera` | `qb.build()` | `qb.to_url()` |
| `python_snippet.tera` | `qb.add_sort(...)` | `qb.set_sort(...)` |
| `python_snippet.tera` | `qb.set_fields([...])` | multiple `qb.add_field(...)` calls |
| `r_snippet.tera` | `qb$build()` | `qb$to_url()` |
| `r_snippet.tera` | `qb$add_sort(...)` | `qb$set_sort(...)` |
| `js_snippet.tera` | correct | — |

Fix `templates/snippets/python_snippet.tera` and `templates/snippets/r_snippet.tera`
after Phase 0 naming is finalised.

---

## Phase 2: Add CLI snippet type *(depends on Phase 0)*

A `"cli"` snippet shows the equivalent `{site}-cli` command for the current query.

**Example output:**
```bash
goat-cli taxon search \
  --taxon "Mammalia" --taxon-filter tree \
  --attribute "genome_size>=1e9" \
  --field-groups genome-size \
  --size 10
```

**Implementation:**
1. Add `templates/snippets/cli_snippet.tera`
   - `taxa` → `--taxon` entries; `taxon_filter_type` → `--taxon-filter`
   - attribute filters → `--attribute "name op value"` (raw form; no field-group lookup required)
   - `fields` via `flags` → `--field-groups` when populated
2. Register `"cli"` in `SnippetGenerator::new()` in `src/core/snippet.rs`
3. Add `"cli"` to accepted languages in all three `snippet()` methods

---

## Phase 3: Add parse functions to subcrate *(can start immediately)*

New file: `crates/genomehubs-query/src/parse.rs`
Exposed via PyO3 (`src/lib.rs`) and WASM (`crates/genomehubs-query/src/lib.rs`).

### 3.1 `ResponseStatus` + `parse_response_status`

```rust
pub struct ResponseStatus { pub hits: u64, pub ok: bool, pub error: Option<String> }
pub fn parse_response_status(raw: &str) -> Result<ResponseStatus, ParseError>
```

FFI returns JSON: `{"hits":42,"ok":true,"error":null}`

Fixes existing bug: JS `count()` reads `results.count` but the field is `status.hits`.
All three SDK `count()` methods should delegate to this function.

### 3.2 `parse_search_json` — flattened records

Input: raw API JSON (one page).
Output: JSON array of flat records.

The API returns `results[].result.fields` where each field has `value`/`count`/`min`/`max`
sub-keys. Output is flattened to:
```json
{"taxon_id": "...", "genome_size": 2.5e9, "genome_size_count": 1, "genome_size_min": 2.1e9}
```

Returns: JSON string (WASM, extendr) or `Vec<HashMap<String, PyObject>>` (PyO3, no copy).

### 3.3 `parse_search_tsv` — validated passthrough

Validates column presence and normalises encoding. Returns string or error.
Python/R pass output directly to `pandas.read_csv` / `read.table`.

### 3.4 Pagination

HTTP stays in each language SDK. Each SDK's `search_all()` drives a loop:
```
while has_more_pages:
    raw = fetch(url_for_page(n))
    rows += parse_search_json(raw)   # Rust
    n += 1
```

### 3.5 Reports and record endpoints (deferred)

Require audit of `genomehubs/genomehubs/src/genomehubs-api` for API response shapes.
Planned stubs:
- `parse_report_json(raw, report_type)` — histogram, scatter, etc.; nested per-field
  bucket structures need mapping/expansion
- `parse_record_json(raw)` — single-entity detail endpoint

Rust structs enforce API schema at compile time once shapes are confirmed.

---

## Phase 4: Fix WASM FFI divergences *(depends on Phase 3)*

`crates/genomehubs-query/src/lib.rs` currently diverges from PyO3 and extendr:

| | PyO3 | WASM (current) | extendr |
|---|---|---|---|
| `build_url` endpoint | explicit | **hardcoded `"search"`** | explicit |
| `build_url` api_base/version | explicit | explicit | **absent (uses `cli_meta`)** |
| `describe_query` | ✓ | **missing** | ✓ |
| `render_snippet` | ✓ | **missing** | ✓ |
| `version` | ✓ | **missing** | **missing** |

**Changes to `crates/genomehubs-query/src/lib.rs`:**
- `build_url`: add `endpoint` param
- Add `describe_query(query_yaml, params_yaml, field_metadata_json, mode) -> String`
- Add `render_snippet(snapshot_json, site_name, api_base, sdk_name, languages) -> String`
- Add `version() -> String`

After these changes: expose `describe()` and `snippet()` in `templates/js/query.js`.

---

## Phase 5: `validate()` parity *(depends on Phase 3)*

### 5.1 Move shared types to subcrate

`FieldMeta`, `ValidationConfig`, `ValidationError` →
`crates/genomehubs-query/src/validation.rs`.
Main crate re-exports. Subcrate uses `HashMap<String, FieldMeta>` instead of `phf::Map`.

### 5.2 Generator emits `field_meta.json`

Generator writes `src/generated/field_meta.json` alongside `field_meta.rs`.
Generated code:
```rust
pub const FIELD_META_JSON: &str = include_str!("field_meta.json");
pub const VALIDATION_CONFIG_JSON: &str = include_str!("validation_config.json");
```

### 5.3 `validate_query_json` in subcrate

```rust
pub fn validate_query_json(
    query_yaml: &str,
    field_meta_json: &str,
    config_json: &str,
) -> String  // JSON array of error strings
```

Same logic as `validate_query`, but `HashMap` not `phf::Map`.

### 5.4 Expose via WASM and extendr

- WASM: `#[wasm_bindgen]` in `crates/genomehubs-query/src/lib.rs`
- extendr: add to `templates/r/lib.rs.tera`

### 5.5 Add `validate()` to JS and R

- JS: `validate() -> string[]`
- R: `validate() -> character vector`
- Python: keep phf path as primary (faster); JSON path added for cross-SDK parity tests

---

## Phase 6: E2E testing + CI *(depends on Phases 0–5)*

### 6.1 SDK parity test (`tests/python/test_sdk_parity.py`)

Introspects `query.py`, `query.js` template, `query.R` template and asserts all
canonical methods from the table above are present in all three. Runs on every PR.
Catches method name drift before it reaches `main`.

### 6.2 `scripts/test_sdk_generation.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail
cargo build --release
rm -rf /tmp/e2e-goat
cargo run --release -- new goat --config sites/ --output-dir /tmp/e2e-goat
cd /tmp/e2e-goat/goat-cli

cargo test
maturin develop --features extension-module && pytest python/ -q
cd js/goat && node test_basic.js && cd ../..
cd r/goat && Rscript test_basic.R && cd ../..
```

### 6.3 Generated smoke test fixtures

**`templates/js/test_basic.js.tera`:**
- `toUrl()` returns a non-empty HTTPS URL
- `validate()` returns `[]` for a valid query; non-empty for unknown attribute name
- `count()` > 0 (skip if `--no-network`)
- `search()` returns array (skip if `--no-network`)

**`templates/r/test_basic.R.tera`:**
- `to_url()` returns non-empty string
- `validate()` returns zero-length character for valid query
- `count()` > 0 (skip if `--no-network`)
- `describe()` returns non-empty string

**`tests/python/test_generated_goat_sdk.py`:**
- `to_url()` round-trip matches known URL
- `validate()` empty for good query, non-empty for bad
- `describe()` returns non-empty string
- `snippet(["python","r","javascript","cli"])` returns all four keys
- `count()` > 0 (`@pytest.mark.network`)
- `search()` first-page shape (`@pytest.mark.network`)

### 6.4 CI job (`.github/workflows/sdk-integration.yml`)

```yaml
strategy:
  matrix:
    os: [ubuntu-latest, macos-latest]
steps:
  - Rust toolchain + wasm-pack
  - Python setup + maturin
  - R setup + devtools + pak
  - Run scripts/test_sdk_generation.sh
  - Upload generated pkg/ artifacts
```

Network-dependent tests gated: only on `push` to `main` (not PRs, to avoid rate limits).

---

## Ongoing parity governance

Once Phases 0–5 complete and CI is green, add to `AGENTS.md`:

- Every new `QueryBuilder` method must be added to all three SDKs in the same PR.
  The parity test (Phase 6.1) enforces this automatically.
- Snippet templates must be updated when methods are renamed.
- Every new parse function needs PyO3, WASM, and extendr exports in the same PR
  (extends the existing 6-touchpoint checklist in `AGENTS.md`).
- `AGENTS.md` update: only after Phases 0–5 merged and CI green.

---

## File inventory

### Modify

| File | Changes |
|---|---|
| `templates/r/query.R` | Missing setters, modifier params, remove `set_fields`, rename `add_sort→set_sort` |
| `templates/js/query.js` | Public `toQueryYaml`/`toParamsYaml`, add `describe`/`snippet`, fix `count` |
| `python/cli_generator/query.py` | Add `to_url`, `count`, `search` instance methods |
| `crates/genomehubs-query/src/lib.rs` | `endpoint` param, `describe`/`snippet`/`version` WASM exports |
| `src/lib.rs` | `parse_response_status`, `parse_search_json`, `parse_search_tsv` PyO3 exports |
| `src/core/snippet.rs` | Register `"cli"` language |
| `templates/snippets/python_snippet.tera` | Fix method names |
| `templates/snippets/r_snippet.tera` | Fix method names |

### New

| File | Purpose |
|---|---|
| `templates/snippets/cli_snippet.tera` | CLI command snippet template |
| `crates/genomehubs-query/src/parse.rs` | `ResponseStatus`, `parse_search_json`, `parse_search_tsv` |
| `crates/genomehubs-query/src/validation.rs` | Shared `FieldMeta`, `ValidationError`, `validate_query_json` |
| `scripts/test_sdk_generation.sh` | Full generation + test driver |
| `templates/js/test_basic.js.tera` | Generated JS smoke test |
| `templates/r/test_basic.R.tera` | Generated R smoke test |
| `tests/python/test_generated_goat_sdk.py` | Python SDK integration tests |
| `tests/python/test_sdk_parity.py` | Cross-SDK method parity assertion |
| `.github/workflows/sdk-integration.yml` | CI job for all language SDKs |

### Generator output (not hand-written)

| File | Purpose |
|---|---|
| `src/generated/field_meta.json` | JSON field metadata for WASM/extendr validation |
| `src/generated/validation_config.json` | JSON `ValidationConfig` for WASM/extendr validation |

---

## Scope boundaries

- HTTP stays language-native. Rust handles per-page parsing and API status errors.
- HTTP errors handled per-language. API `status.error` surfaced via `parse_response_status`.
- Reports and record endpoints deferred pending `genomehubs-api` response shape audit.
- WASM target: `--target nodejs` only. Browser support is future work.
- No Rust snippet type: Rust API is internal to the binary, not a public library interface.
- `AGENTS.md` updated only after Phases 0–5 merged and CI is green.
