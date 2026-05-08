# V3 API — SDK & CLI Gap Analysis

**Date:** 2026-05-08
**Branch:** develop
**Context:** Generated after completing Python SDK v3 migration (phase 6b). Used as input for phase 6c–6h planning.

---

## 1. Complete v3 endpoint surface vs SDK coverage

| Endpoint          | Method | Python lib   | Python template | R template | JS template | Generated CLI                       |
| ----------------- | ------ | ------------ | --------------- | ---------- | ----------- | ----------------------------------- |
| `/count`          | POST   | ✅ v3 POST   | ✅ v3 POST      | ❌ v2 GET  | ❌ v2 GET   | ✅ (Rust client — verify transport) |
| `/countBatch`     | POST   | ✅           | ✅              | ✅         | ✅          | ❌ no subcommand                    |
| `/indices`        | GET    | ❌ no method | ❌              | ❌         | ❌          | ❌                                  |
| `/lookup`         | GET    | ✅           | ✅              | ✅         | ✅          | ✅ per index                        |
| `/record`         | GET    | ✅           | ✅              | ✅         | ✅          | ❌ no subcommand                    |
| `/report`         | POST   | ✅           | ✅              | ❌         | ❌          | ❌ no subcommand                    |
| `/resultFields`   | GET    | ❌ no method | ❌              | ❌         | ❌          | ❌                                  |
| `/search`         | POST   | ✅ v3 POST   | ✅ v3 POST      | ❌ v2 GET  | ❌ v2 GET   | ✅ (Rust client)                    |
| `/searchBatch`    | POST   | ✅           | ✅              | ✅         | ✅          | ❌ no subcommand                    |
| `/status`         | GET    | ❌ no method | ❌              | ❌         | ❌          | ❌                                  |
| `/summary`        | GET    | ✅           | ✅              | ✅         | ✅          | ❌ no subcommand                    |
| `/taxonomicRanks` | GET    | ❌ no method | ❌              | ❌         | ❌          | ❌                                  |
| `/taxonomies`     | GET    | ❌ no method | ❌              | ❌         | ❌          | ❌                                  |

**Legend:** ✅ implemented and correct transport; ❌ missing or wrong transport.

---

## 2. SDK method transport status

| Method                                | Python lib        | Python template   | R template               | JS template                                    |
| ------------------------------------- | ----------------- | ----------------- | ------------------------ | ---------------------------------------------- |
| `count()`                             | ✅ v3 POST        | ✅ v3 POST        | ❌ v2 GET                | ❌ v2 GET                                      |
| `search()`                            | ✅ v3 POST        | ✅ v3 POST        | ❌ v2 GET                | ❌ v2 GET                                      |
| `search_all()` / `searchAll()`        | ✅ v3 cursor POST | ✅ v3 cursor POST | ❌ missing entirely      | ❌ uses `/searchPaginated` (nonexistent in v3) |
| `report()`                            | ✅                | ✅                | ❌ missing               | ❌ missing                                     |
| `to_v2_url()` / `toV2Url()`           | ✅ renamed        | ✅ renamed        | ❌ still `to_url()`      | ❌ still `toUrl()`                             |
| `_post_json()` / internal POST helper | ✅                | ✅                | uses `httr::POST` inline | uses `fetch` inline                            |
| `search_batch()` / `searchBatch()`    | ✅                | ✅                | ✅                       | ✅                                             |
| `count_batch()` / `countBatch()`      | ✅                | ✅                | ✅                       | ✅                                             |
| `record()`                            | ✅                | ✅                | ✅                       | ✅                                             |
| `lookup()`                            | ✅                | ✅                | ✅                       | ✅                                             |
| `summary()`                           | ✅                | ✅                | ✅                       | ✅                                             |
| `validate()`                          | ✅                | ✅                | ✅                       | ✅                                             |
| `describe()`                          | ✅                | ✅                | ✅                       | ✅                                             |
| `snippet()`                           | ✅                | ✅                | ✅                       | ✅                                             |

---

## 3. Parse function coverage

These Rust functions exist in `crates/genomehubs-query/src/parse.rs` and are exposed via PyO3 (`src/lib.rs`) and extendr (`templates/r/extendr-wrappers.R.tera`):

| Parse function          | Rust core | PyO3 | R extendr | JS WASM | Notes                                           |
| ----------------------- | --------- | ---- | --------- | ------- | ----------------------------------------------- |
| `parse_response_status` | ✅        | ✅   | ✅        | ✅      | Used by count/search                            |
| `parse_search_json`     | ✅        | ✅   | ✅        | ✅      | Used by search/search_all                       |
| `parse_paginated_json`  | ✅        | ✅   | ✅        | ✅      | v2 only; unused in v3                           |
| `parse_batch_json`      | ✅        | ✅   | ✅        | ✅      | Used by search_batch/count_batch                |
| `parse_record_json`     | ✅        | ✅   | ✅        | ✅      | Used by record                                  |
| `parse_lookup_json`     | ✅        | ✅   | ✅        | ✅      | Used by lookup                                  |
| `parse_histogram_json`  | ✅        | ✅   | ✅        | ✅      | Used internally; not surfaced on `report()` yet |
| `parse_tree_json`       | ✅        | ✅   | ✅        | ✅      | Used internally; not surfaced on `report()` yet |

---

## 4. Generated CLI subcommand gaps

The generated CLI has `search`, `count`, `lookup` per index. Missing:

| Subcommand     | API endpoint   | Notes                                               |
| -------------- | -------------- | --------------------------------------------------- |
| `record`       | `/record`      | Fetch by ID — useful for direct lookup by accession |
| `summary`      | `/summary`     | Field aggregation per record — specialist use       |
| `report`       | `/report`      | All visualisation report types — high value         |
| `search-batch` | `/searchBatch` | File-driven batch — useful for scripts              |
| `count-batch`  | `/countBatch`  | File-driven batch counts                            |

Additionally, the `count` subcommand has fewer attribute flags than `search`; they should be kept in sync.

---

## 5. Report builder UX analysis

### Current implementation (flat kwargs on `QueryBuilder.report()`)

```python
qb.report("histogram", x="genome_size", x_opts="scale=log10", rank="species")
```

Ergonomic for simple one-off calls. Problems at scale:

- 15+ optional kwargs; right combination depends on report type
- No reuse across multiple queries
- No client-side validation before POST
- No `describe()` or `snippet()` support

### Proposed `ReportBuilder` class

Mirrors `QueryBuilder` design: chainable setter methods, `to_report_yaml()`, `validate()`, `describe()`.

```python
rb = ReportBuilder("histogram")
rb.set_x("genome_size", opts="scale=log10")
rb.set_rank("species")

# Two equivalent call patterns:
data = rb.run(qb)          # ReportBuilder drives the call
data = qb.report(rb)       # QueryBuilder accepts a ReportBuilder
```

Advantages:

- **Reuse**: same `rb` applied across multiple queries (loop over taxa, indices)
- **Validation**: `rb.validate()` checks required axes for report type before network call
- **Serialisation**: `rb.to_report_yaml()` makes the config explicit and inspectable
- **Composition with describe/snippet**: `rb.describe()` + `qb.describe()` → combined prose

The `report_yaml` is already a first-class concept in the API. `ReportBuilder` is a typed wrapper that produces it.

The flat-kwargs `qb.report("histogram", x=...)` convenience stays as a thin wrapper that constructs a `ReportBuilder` internally.

**Implementation pattern (Rust-first):**

- `src/core/report_builder.rs` — `ReportBuilder` struct and logic
- `src/lib.rs` — PyO3 exposure
- `python/cli_generator.pyi` — stub
- `templates/python/query.py.tera` — `ReportBuilder` class + `qb.report()` thin wrapper
- `templates/r/query.R` — R6 `ReportBuilder` class
- `templates/js/query.js` — JS `ReportBuilder` class

### Report type validation rules

| Report type    | Required | Optional                                                      |
| -------------- | -------- | ------------------------------------------------------------- |
| `histogram`    | `x`      | `y`, `cat`, `rank`, `fields`, `status_filter`, `cat_rank`     |
| `scatter`      | `x`, `y` | `cat`, `rank`, `fields`, `status_filter`, `scatter_threshold` |
| `map`          | —        | `location_field`, `hex_resolution`, `map_threshold`           |
| `tree`         | `rank`   | `collapse_monotypic`, `preserve_rank`, `count_rank`           |
| `countPerRank` | `query`  | `ranks`, `cat`                                                |
| `sources`      | —        | `rank`, `fields`                                              |
| `arc`          | `x`      | `y`, `cat`                                                    |

---

## 6. Validation extension

### Current scope

`validate_query_json` validates `query_yaml` only: index name, field names, attribute operators, type compatibility. Runs entirely client-side using bundled `field_meta.json`.

### Extensions

**`validate_report_yaml(report_yaml, field_meta_json) -> String`** (new Rust function)
Validates required axes per report type (table above). Checks field names in `x`/`y`/`cat` against `field_meta`. The report type enum already exists implicitly in `crates/genomehubs-api/src/routes/report.rs` — moving it to `crates/genomehubs-query` gives the validator access without depending on the API crate.

**`validate_batch(queries) -> Vec<String>`** — validate each `(query_yaml, params_yaml)` pair in a batch before sending. Pure client-side, no network call.

**Count validation** — count shares `query_yaml`/`params_yaml` with search; `validate_query_json` already works. No new function needed.

**What validation cannot do client-side:** field _value_ validity against actual data requires a server call. The existing `validation_level` config controls this boundary.

---

## 7. Breaking changes: v2 → v3

For users migrating from direct v2 API use or v2-era SDK use.

### Transport

1. `GET /v2/search?tax_name=...&fields=...` → `POST /v3/search` with JSON body `{query_yaml, params_yaml}`
2. `GET /v2/count?...` → `POST /v3/count` with same body (v2 required `size=0` on `/search`)
3. `/searchPaginated` does not exist in v3; use repeated `/search` POST with `search_after` cursor

### Response envelope

4. `results[].result.attributes` (v2: array of attribute objects) → `results[].result.fields` (v3: object keyed by field name)
5. Per-field shape: v2 had `{name, value, aggregation_source, from, ...}` as an array element; v3 has `{value, min, max, aggregation_source}` as an object directly under the field key
6. Pagination: v2 used `from`/`size` offset → v3 uses opaque `search_after` cursor. Page-number pagination is gone.
7. `status.results` in v2 → `status.hits` in v3

### New endpoints (v3 only)

8. `/report` — POST with `{query_yaml, params_yaml, report_yaml}` — new unified report interface
9. `/count` — dedicated count endpoint (v2 required size=0 hack on `/search`)

### Unchanged endpoints

10. `/summary` — GET with URL params in both versions (params identical)
11. `/record`, `/lookup` — GET in both versions (params identical)

### SDK-level breaking changes

12. `to_url()` deprecated; use `to_v2_url()` for the v2 query-string URL
13. `search_all()` loop is cursor-based internally; callers see no API change
14. `count()` now hits the dedicated `/count` endpoint; return value is unchanged

### Infrastructure

15. v3 requires the `genomehubs-api` Axum server (default port 3000); v2 was the Node.js genomehubs server. These are separate processes.

---

## 8. Describe and snippet — future scope (deferred to phase-XX)

### Describe extensions

- `describe_report_yaml(report_yaml) -> String` in Rust: prose description of a report configuration
- Combined `qb.describe()` + `rb.describe()` → "taxon genome_size ≥ 1 Gbp filtered to primates, visualised as a histogram by species rank"

### Snippet extensions

- `call_type` context variable in snippet templates (`"search"`, `"count"`, `"report"`)
- `QuerySnapshot` gains `report: Option<ReportSnapshot>` field so report config is captured
- CLI snippet for `report` (requires `report` subcommand in generated CLI)
- Batch snippet: requires `MultiQueryBuilder.snippet()` capturing N builders

### Metadata endpoint methods (deferred to phase-XX)

- `indices()` → `/indices` — list available indices
- `result_fields(index)` → `/resultFields` — field metadata for an index
- `taxonomies()` → `/taxonomies` — list taxonomies
- `taxonomic_ranks()` → `/taxonomicRanks` — list ranks

These are low-priority discovery endpoints useful for programmatic introspection but not required for core query workflows.
