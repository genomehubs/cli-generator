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

| Prefix     | Meaning                                        | Examples                                             |
| ---------- | ---------------------------------------------- | ---------------------------------------------------- |
| `set_*`    | Replaces a scalar or whole-list value          | `set_taxa`, `set_sort`, `set_size`, `set_rank`       |
| `add_*`    | Appends one item to a list                     | `add_attribute`, `add_field`                         |
| `to_*`     | Serialises state (no side-effects, no network) | `to_url`, `to_query_yaml`                            |
| bare verbs | Triggers I/O or computation                    | `count`, `search`, `validate`, `describe`, `snippet` |

JavaScript: camelCase. Python/R: snake_case.

---

## Canonical method list

| Concept              | Python                               | JavaScript                          | R                                    |
| -------------------- | ------------------------------------ | ----------------------------------- | ------------------------------------ |
| Construct            | `__init__(index)`                    | `constructor(index)`                | `initialize(index)`                  |
| Set taxon filter     | `set_taxa(taxa, filter_type)`        | `setTaxa(taxa, filterType)`         | `set_taxa(..., filter_type)`         |
| Set rank             | `set_rank(rank)`                     | `setRank(rank)`                     | `set_rank(rank)`                     |
| Set assemblies       | `set_assemblies(accessions)`         | `setAssemblies(accessions)`         | `set_assemblies(accessions)`         |
| Set samples          | `set_samples(accessions)`            | `setSamples(accessions)`            | `set_samples(accessions)`            |
| Add attribute filter | `add_attribute(name, op, val, mods)` | `addAttribute(name, op, val, mods)` | `add_attribute(name, op, val, mods)` |
| Add response field   | `add_field(name, mods)`              | `addField(name, mods)`              | `add_field(name, mods)`              |
| Set name classes     | `set_names(classes)`                 | `setNames(classes)`                 | `set_names(classes)`                 |
| Set lineage ranks    | `set_ranks(ranks)`                   | `setRanks(ranks)`                   | `set_ranks(ranks)`                   |
| Page size            | `set_size(n)`                        | `setSize(n)`                        | `set_size(n)`                        |
| Page number          | `set_page(n)`                        | `setPage(n)`                        | `set_page(n)`                        |
| Sort                 | `set_sort(field, order)`             | `setSort(field, order)`             | `set_sort(name, direction)`          |
| Include estimates    | `set_include_estimates(bool)`        | `setIncludeEstimates(bool)`         | `set_include_estimates(bool)`        |
| Taxonomy source      | `set_taxonomy(name)`                 | `setTaxonomy(name)`                 | `set_taxonomy(name)`                 |
| Serialise query      | `to_query_yaml()`                    | `toQueryYaml()`                     | `to_query_yaml()`                    |
| Serialise params     | `to_params_yaml()`                   | `toParamsYaml()`                    | `to_params_yaml()`                   |
| Build URL            | `to_url()`                           | `toUrl()`                           | `to_url()`                           |
| Count results        | `count()`                            | `count()`                           | `count()`                            |
| Fetch results        | `search(format)`                     | `search(format)`                    | `search(format)`                     |
| Validate query       | `validate()`                         | `validate()`                        | `validate()`                         |
| Describe query       | `describe(meta, mode)`               | `describe(meta, mode)`              | `describe(meta, mode)`               |
| Generate snippets    | `snippet(languages, ...)`            | `snippet(languages, ...)`           | `snippet(languages, ...)`            |
| Reset state          | `reset()`                            | `reset()`                           | `reset()`                            |
| Merge another QB     | `merge(other)`                       | `merge(other)`                      | `merge(other)`                       |
| Merge many QBs       | `combine(*builders)`                 | `combine(...builders)`              | `combine(...)`                       |

**Removed:** `set_fields()` — use `add_field()` per field in all SDKs (R had this;
removed for uniformity with the `add_*` convention).

**No Rust snippet language:** The Rust API is internal to the generated CLI binary,
not a public library interface.

**Snippet languages supported:** `python`, `r`, `javascript`, `cli`.

---

## Phase 0: Method naming + missing setters _(can start immediately)_

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

## Phase 1: Fix broken snippet templates _(depends on Phase 0)_

Bugs found in audit:

| Template              | Wrong                  | Correct                            |
| --------------------- | ---------------------- | ---------------------------------- |
| `python_snippet.tera` | `qb.build()`           | `qb.to_url()`                      |
| `python_snippet.tera` | `qb.add_sort(...)`     | `qb.set_sort(...)`                 |
| `python_snippet.tera` | `qb.set_fields([...])` | multiple `qb.add_field(...)` calls |
| `r_snippet.tera`      | `qb$build()`           | `qb$to_url()`                      |
| `r_snippet.tera`      | `qb$add_sort(...)`     | `qb$set_sort(...)`                 |
| `js_snippet.tera`     | correct                | —                                  |

Fix `templates/snippets/python_snippet.tera` and `templates/snippets/r_snippet.tera`
after Phase 0 naming is finalised.

---

## Phase 2: Add CLI snippet type _(depends on Phase 0)_

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

## Phase 3: Add parse functions to subcrate _(can start immediately)_

New file: `crates/genomehubs-query/src/parse.rs`
Exposed via PyO3 (`src/lib.rs`) and WASM (`crates/genomehubs-query/src/lib.rs`).

### 3.1 `ResponseStatus` + `parse_response_status` ✅ Done

```rust
pub struct ResponseStatus { pub hits: u64, pub ok: bool, pub error: Option<String> }
pub fn parse_response_status(raw: &str) -> Result<ResponseStatus, String>
```

FFI returns JSON: `{"hits":42,"ok":true,"error":null}`

**API `Status` schema (confirmed from spec and live calls):**

```json
{ "hits": 1985, "success": true, "took": 16, "size": 0, "offset": 0 }
```

Fields: `success` (bool — NOT `ok`), `hits`, `took`, `size`, `offset`, `error`.
`ApiStatus` serde struct uses `success` field; public `ResponseStatus.ok` maps from it.

### 3.2 `parse_search_json` — flattened records

Input: raw API JSON from `/search` (one page).
Output: JSON array of flat records.

**Confirmed `/search` top-level envelope:**

```json
{"status": {...}, "results": [...], "aggs": {...}, "fields": {...}, "query": "..."}
```

**Each `results[i]`:**

```json
{"index": "taxon", "id": "9606", "score": 1.0, "result": {...}}
```

**`result` keys by index (all confirmed from live API):**

| Index    | Fixed keys on `result`                                                                                   |
| -------- | -------------------------------------------------------------------------------------------------------- |
| taxon    | `scientific_name`, `taxon_id`, `taxon_rank`, `taxon_names`, `lineage`, `parent`, `fields`                |
| assembly | `assembly_id`, `scientific_name`, `taxon_id`, `taxon_rank`, `lineage`, `parent`, `identifiers`, `fields` |
| sample   | `sample_id`, `scientific_name`, `taxon_id`, `taxon_rank`, `lineage`, `parent`, `fields`                  |

#### Domain model for field values (essential context for the parser)

**Taxon is the only index with summary/aggregation.** Assembly and sample records
represent a single entity each, so their fields are always direct `{value, count}`.
Taxon records aggregate over potentially many assemblies and samples, requiring
summary statistics.

**`aggregation_method`** describes _how_ the representative value was chosen:

- `"primary"` — one value was selected as most authoritative from a list of candidates
  (not a mathematical summary); `value` may still be an array if multiple values share
  the same authority level
- `"enum"` — single best-fit categorical value from across candidates
- `"list"` — all distinct values collected into a set
- `"mode"` / `"mode_low"` — most frequent value (mode_low prefers smaller on tie)
- `"max"` / `"min"` / `"median"` / `"mean"` — standard statistical summaries

**`aggregation_source`** describes the origin of the data:

- `"direct"` (string) — value came from a record directly associated with this taxon
- `["descendant"]` (array) — value was rolled up from descendant taxa
- `["ancestor"]` (array) — value was inherited from an ancestor taxon

**`sp_count`** = number of **species** with direct values for this attribute at or below
this node. Present when `aggregation_source = "direct"` or `["descendant"]`.
Absent when `aggregation_source = ["ancestor"]` (ancestor-inherited values don't
carry their own species count down). `sp_count: 0` means the species itself has the
value directly; higher values at genus/family reflect aggregation.

**`count`** = number of _inputs that went into the summary at this level_, not a total
count of all underlying values. Example: at family level with 2 genera having data
across 6 species, `count = 2` (genera), because each genus already summarised its
species. Not the same as `sp_count`.

**`stub shape`** — a field object with only `{"sp_count": N}` and no `value` or `count`
means this taxon has descendants with data but no value was rolled up to this node.
Likely a known API limitation for certain field types. Parser must skip/null these.

**Which summary stats appear per field** is determined by the import config for each
field, available from the `/resultFields` endpoint (which the CLI already queries for
validation). The stat keys present are therefore knowable in advance, not arbitrary.

#### Field value shapes (all confirmed, 2026-04-17)

All possible sub-keys:
`value`, `count`, `min`, `max`, `median`, `mode`, `mean`, `from`, `to`, `length`,
`has_descendants`, `sp_count`, `aggregation_method`, `aggregation_source`,
`aggregation_rank`, `aggregation_taxon_id`

**Taxon index:**

| Field category                 | Examples                                                  | `value` type                        | Stat keys present              | Extra keys                                                                                                     |
| ------------------------------ | --------------------------------------------------------- | ----------------------------------- | ------------------------------ | -------------------------------------------------------------------------------------------------------------- |
| numeric, direct                | `genome_size`, `assembly_span`, `contig_n50`              | number                              | `min`, `max`, `median`         | `sp_count`                                                                                                     |
| numeric with mode              | `chromosome_number`, `haploid_number`                     | number                              | `min`, `max`, `median`, `mode` | `sp_count`                                                                                                     |
| numeric, ancestor-estimated    | `ploidy`, `ploidy_inferred`                               | number                              | `min`, `max`, `median`, `mode` | `aggregation_rank`, `aggregation_taxon_id`; **`sp_count` absent**; `aggregation_source` is **array**           |
| numeric, descendant-aggregated | `mitochondrion_gc_percent`                                | number                              | `median` only                  | `sp_count`; `aggregation_source` is **array**                                                                  |
| half_float                     | `c_value`                                                 | number                              | `min`, `max`, `median`, `mean` | `sp_count`; `mean` unique to this type                                                                         |
| 1dp                            | `busco_completeness`, `btk_target`, `btk_nohit`           | number                              | `max` only                     | `sp_count`; subset of stats configured at import                                                               |
| date                           | `assembly_date`, `ebp_standard_date`                      | string (ISO date `YYYY-MM-DD`)      | —                              | `from`, `to` (ISO datetime), `sp_count`; no numeric stat keys                                                  |
| date (with has_descendants)    | `assembly_date` at genus level                            | string                              | —                              | `from`, `to`, `sp_count`, `has_descendants`                                                                    |
| enum                           | `assembly_level`, `sequencing_status`                     | string                              | —                              | `sp_count`                                                                                                     |
| list, single                   | `c_value_method`, `sex_determination`                     | string (not array)                  | —                              | `length: 1`, `sp_count`                                                                                        |
| list, multiple                 | `bioproject`, `country_list`, `ebp_standard_criteria`     | array of strings (may be truncated) | —                              | `length` (true total), `sp_count`, `has_descendants` (sometimes)                                               |
| list, ancestor-estimated       | `cites_category`, `conservation_list`                     | string                              | —                              | `length`, `aggregation_rank`, `aggregation_taxon_id`; **`sp_count` absent**; `aggregation_source` is **array** |
| geo_point                      | `sample_location`                                         | string `"lat, lon"`                 | —                              | `length`, `sp_count`; NOT a geo object                                                                         |
| stub                           | `mitochondrion_assembly_span`, some `sequencing_status_*` | — (absent)                          | —                              | **only `sp_count`** — no `value` or `count`                                                                    |

**Assembly and sample indexes:**

No aggregation metadata — each record is a single entity:

| Shape  | Keys                       | When                       |
| ------ | -------------------------- | -------------------------- |
| Scalar | `value`, `count`           | Numeric, date, enum fields |
| List   | `value`, `count`, `length` | Keyword list fields        |

Assembly date fields return `{value: "YYYY-MM-DD", count: 1}` — no `from`/`to`.

#### Parser rules for `parse_search_json`

1. Skip the entire field (emit null/absent) if the field object has no `value` key.
2. `aggregation_source` must be handled as either a string or an array.
3. `sp_count` may be absent — treat as optional, not as zero.
4. `value` for a list field may be a string (when `length: 1`) or an array —
   normalise to array for consistent output.
5. Which stat keys are available per field is knowable from `/resultFields` metadata;
   the parser should not assume any specific stat keys are present.
6. `count` is not a record count — do not expose it as a "number of results".

Flattened output target:

```json
{
  "taxon_id": "9606",
  "scientific_name": "Homo sapiens",
  "genome_size": 3423000000,
  "genome_size_count": 1,
  "genome_size_min": 3423000000,
  "genome_size_max": 3423000000,
  "ebp_standard_date": "2004-09-01",
  "ebp_standard_date_from": "2004-09-01T00:00:00.000Z",
  "bioproject": ["CNP0000066", "PRJDB10452"],
  "bioproject_length": 2176,
  "ploidy_aggregation_source": "ancestor",
  "ploidy_aggregation_rank": "clade"
}
```

Returns: JSON string (WASM, extendr) or `Vec<HashMap<String, PyObject>>` (PyO3, no copy).

### 3.3 `parse_search_tsv` — validated passthrough

Validates column presence and normalises encoding. Returns string or error.
Python/R pass output directly to `pandas.read_csv` / `read.table`.

### 3.4 Pagination

**`/searchPaginated` envelope (confirmed from API spec):**

```json
{
  "status": {"hits": N, "success": true, ...},
  "hits": [...],
  "pagination": {"limit": 100, "count": 100, "hasMore": true, "searchAfter": [...]}
}
```

Note: uses `hits` (not `results`) for records; `pagination.searchAfter` is the cursor
passed back as the `searchAfter` query param on the next request.

HTTP stays in each language SDK. Each SDK's `search_all()` drives a loop:

```
while pagination.hasMore:
    raw = fetch(url + searchAfter=pagination.searchAfter)
    rows += parse_search_json(raw)
```

### 3.5 `msearch` envelope (confirmed from API spec)

**POST `/msearch` response:**

```json
{
  "status": {"success": true, ...},
  "results": [
    {"status": "ok", "count": 50, "total": 1985, "hits": [...], "error": null}
  ]
}
```

Each `results[i]` corresponds to one input search. `hits` is the record array
(same shape as `/search` `results`).

### 3.6 Reports and record endpoints (deferred)

- `parse_report_json(raw, report_type)` — histogram, scatter, etc.
- `parse_record_json(raw)` — single-entity `/record` endpoint

Rust structs enforce API schema at compile time once shapes are confirmed.

---

## Phase 4: Fix WASM FFI divergences _(depends on Phase 3)_

`crates/genomehubs-query/src/lib.rs` currently diverges from PyO3 and extendr:

|                              | PyO3     | WASM (current)           | extendr                      |
| ---------------------------- | -------- | ------------------------ | ---------------------------- |
| `build_url` endpoint         | explicit | **hardcoded `"search"`** | explicit                     |
| `build_url` api_base/version | explicit | explicit                 | **absent (uses `cli_meta`)** |
| `describe_query`             | ✓        | **missing**              | ✓                            |
| `render_snippet`             | ✓        | **missing**              | ✓                            |
| `version`                    | ✓        | **missing**              | **missing**                  |

**Changes to `crates/genomehubs-query/src/lib.rs`:**

- `build_url`: add `endpoint` param
- Add `describe_query(query_yaml, params_yaml, field_metadata_json, mode) -> String`
- Add `render_snippet(snapshot_json, site_name, api_base, sdk_name, languages) -> String`
- Add `version() -> String`

After these changes: expose `describe()` and `snippet()` in `templates/js/query.js`.

---

## Phase 5: `validate()` parity _(depends on Phase 3)_

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

## Phase 6: E2E testing + CI _(depends on Phases 0–5)_

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

| File                                     | Changes                                                                           |
| ---------------------------------------- | --------------------------------------------------------------------------------- |
| `templates/r/query.R`                    | Missing setters, modifier params, remove `set_fields`, rename `add_sort→set_sort` |
| `templates/js/query.js`                  | Public `toQueryYaml`/`toParamsYaml`, add `describe`/`snippet`, fix `count`        |
| `python/cli_generator/query.py`          | Add `to_url`, `count`, `search` instance methods                                  |
| `crates/genomehubs-query/src/lib.rs`     | `endpoint` param, `describe`/`snippet`/`version` WASM exports                     |
| `src/lib.rs`                             | `parse_response_status`, `parse_search_json`, `parse_search_tsv` PyO3 exports     |
| `src/core/snippet.rs`                    | Register `"cli"` language                                                         |
| `templates/snippets/python_snippet.tera` | Fix method names                                                                  |
| `templates/snippets/r_snippet.tera`      | Fix method names                                                                  |

### New

| File                                        | Purpose                                                      |
| ------------------------------------------- | ------------------------------------------------------------ |
| `templates/snippets/cli_snippet.tera`       | CLI command snippet template                                 |
| `crates/genomehubs-query/src/parse.rs`      | `ResponseStatus`, `parse_search_json`, `parse_search_tsv`    |
| `crates/genomehubs-query/src/validation.rs` | Shared `FieldMeta`, `ValidationError`, `validate_query_json` |
| `scripts/test_sdk_generation.sh`            | Full generation + test driver                                |
| `templates/js/test_basic.js.tera`           | Generated JS smoke test                                      |
| `templates/r/test_basic.R.tera`             | Generated R smoke test                                       |
| `tests/python/test_generated_goat_sdk.py`   | Python SDK integration tests                                 |
| `tests/python/test_sdk_parity.py`           | Cross-SDK method parity assertion                            |
| `.github/workflows/sdk-integration.yml`     | CI job for all language SDKs                                 |

### Generator output (not hand-written)

| File                                   | Purpose                                             |
| -------------------------------------- | --------------------------------------------------- |
| `src/generated/field_meta.json`        | JSON field metadata for WASM/extendr validation     |
| `src/generated/validation_config.json` | JSON `ValidationConfig` for WASM/extendr validation |

---

## Scope boundaries

- HTTP stays language-native. Rust handles per-page parsing and API status errors.
- HTTP errors handled per-language. API `status.error` surfaced via `parse_response_status`.
- Reports and record endpoints deferred pending `genomehubs-api` response shape audit.
- WASM target: `--target nodejs` only. Browser support is future work.
- No Rust snippet type: Rust API is internal to the binary, not a public library interface.
- `AGENTS.md` updated only after Phases 0–5 merged and CI is green.
