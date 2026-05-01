# v3 API Parity, Report Endpoints, and Full SDK Coverage

**Status**: Authoritative plan — supersedes `api-aggregation-refactoring-plan.md` and `api-audit-action-checklist.md`
**Created**: 2026-05-01
**Scope**: `crates/genomehubs-api`, `crates/genomehubs-query`, all three SDK templates (Python, JS, R)

---

## Context: Audit Summary

### genomehubs-api (Rust/Axum) — v3, current state

| Endpoint                     | Status                                                   |
| ---------------------------- | -------------------------------------------------------- |
| `GET /api/v3/status`         | ✅                                                       |
| `GET /api/v3/resultFields`   | ✅ (cached)                                              |
| `GET /api/v3/taxonomies`     | ✅ wrapped `{ taxonomies: [], last_updated }`            |
| `GET /api/v3/taxonomicRanks` | ✅ but uses wrong key `taxonomic_ranks` (fix in Phase 0) |
| `GET /api/v3/indices`        | ✅ string list                                           |
| `POST /api/v3/count`         | ✅                                                       |
| `POST /api/v3/search`        | ❌                                                       |
| `GET /api/v3/record`         | ❌                                                       |
| `GET /api/v3/lookup`         | ❌                                                       |
| `GET /api/v3/summary`        | ❌                                                       |
| `POST /api/v3/msearch`       | ❌                                                       |
| `POST /api/v3/report`        | ❌                                                       |
| `GET /api/v3/download`       | ❌ (deferred)                                            |

### genomehubs-query crate — current state

✅ `SearchQuery` + `QueryParams` (YAML roundtrip), URL builder, parse pipeline
(`parse_search_json`, `parse_response_status`, `annotate_source_labels`, `split_source_columns`,
`values_only`, `annotated_values`, `to_tidy_records`), `validate_query_json`, `describe`, `snippet`

❌ No report axis types / aggregation builder, no `parse_record_json` / `parse_lookup_json` / `parse_report_json`

### SDK / CLI — current state

✅ `count()`, `search()`, `search_all()` in all 3 languages with full parse pipeline
❌ No `record()`, `lookup()`, `summary()`, `report()` methods; no `ReportBuilder`

### v2 → v3 breaking changes (accepted)

| Endpoint              | v2               | v3                                                       |
| --------------------- | ---------------- | -------------------------------------------------------- |
| Request format        | URL query params | JSON body `{ query_yaml, params_yaml }`                  |
| `/taxonomies`         | plain `[]`       | `{ taxonomies: [], last_updated }`                       |
| `/taxonomicRanks` key | `ranks`          | currently wrong as `taxonomic_ranks`; **fix to `ranks`** |
| `/indices` items      | rich objects     | string list                                              |

### ES search templates note

The Mustache template files in `genomehubs/src/genomehubs/templates/scripts/` are consumed by
the Python **data-loader**, not by the API. All v3 query logic is expressed in direct ES DSL.
No stored scripts are needed. The one exception is ES phrase-suggest, which requires
`trigram`/`reverse` field mappings; implement basic prefix/wildcard `/lookup` first and add
phrase-suggest only if the ES indices have the required mappings.

---

## Phase 0: Return Envelope Consistency

**Goal:** Fix-first before adding any new routes.

All v3 endpoints return `{ status: { success, hits?, took?, error? }, ...payload }`.
Metadata-only endpoints (taxonomies, indices, ranks) include `success` but omit `hits`/`took`.

1. Add shared `StatusBlock` struct to `crates/genomehubs-api/src/routes/mod.rs`
2. **Rename `taxonomic_ranks` → `ranks`** in `crates/genomehubs-api/src/routes/taxonomic_ranks.rs`
3. Update all existing response structs (status, resultFields, taxonomies, taxonomicRanks,
   indices, count) to embed `StatusBlock`

**Files:** `crates/genomehubs-api/src/routes/mod.rs` (new struct), all existing route files (add field)

---

## Phase 1: Shared ES Infrastructure + `/search`

1. Extract `resolve_index(query_index, state) -> String` to `crates/genomehubs-api/src/index_name.rs`
   (currently duplicated inline in `count.rs`)
2. Extract `execute_es_request(es_base, index, body) -> Result<Value>` to
   `crates/genomehubs-api/src/es_client.rs`
3. Create `crates/genomehubs-api/src/routes/search.rs`:
   - `POST /api/v3/search`, accepts `{ query_yaml, params_yaml }`
   - Calls `build_search_body()` → `execute_es_request()` → `process_hits()`
   - Returns `{ status: { hits, ok, took }, results: [...] }` with optional `search_after` cursor in status
4. **Update `/status`** to include `supported: ["/count", "/search", ...]` — SDK probes this once
   at build time; `404` → treat all endpoints as v2; if mixed-mode proves complex, fall back to a
   build-time config variable
5. Register route in `routes/mod.rs` and `main.rs`
6. Add integration tests in `crates/genomehubs-api/tests/` covering all attribute operators

**Files:** `crates/genomehubs-api/src/` — `es_client.rs` (new), `index_name.rs` (new),
`routes/search.rs` (new), `routes/status.rs` (add `supported`), `routes/mod.rs`, `main.rs`

---

## Phase 2: `/record`, `/lookup`, `/summary`, `/msearch`

Steps 1–4 are parallel; step 5 is a shared prerequisite for 1 and 3.

1. **`/record`** — `crates/genomehubs-api/src/routes/record.rs`
   - `GET /api/v3/record?recordId={id}&result={type}`
   - ES `_doc/{id}` or `mget`; returns `{ status, records: [{ record, recordId, result }] }`

2. **`/lookup`** — `crates/genomehubs-api/src/routes/lookup.rs`
   - `GET /api/v3/lookup?searchTerm={term}&result={type}`
   - 3-stage fallback: prefix match on `scientific_name` → exact/wildcard → basic suggest
   - Returns `{ status: { hits, result }, results: [{ id, name, reason }] }`
   - Audit ES index for `scientific_name.sayt` field before implementing SAYT prefix match;
     phrase-suggest (trigram mapping) deferred

3. **`/summary`** — `crates/genomehubs-api/src/routes/summary.rs`
   - `GET /api/v3/summary?recordId={id}&fields={f1,f2}&summary={min,max}`
   - Fetch record → nested lineage aggregation → `{ status, summaries: [{ name, field, lineage, summary: { min, max, avg } }] }`
   - Most complex of these four (requires nested agg across taxon lineage path)

4. **`/msearch`** — `crates/genomehubs-api/src/routes/msearch.rs`
   - `POST /api/v3/msearch`, body: `{ searches: [{ query_yaml, params_yaml }] }`
   - Batches into single ES `_msearch` call
   - Returns `{ status, results: [{ status, count, hits: [...] }] }`

5. Factor `fetch_records_by_id()` helper shared by `/record` and `/summary`

---

## Phase 3: SDK Coverage for New Endpoints

_Depends on Phases 1–2._

1. Add `parse_record_json(raw) -> String` and `parse_lookup_json(raw) -> String` to
   `crates/genomehubs-query/src/parse.rs`
2. Expose both via PyO3 (`src/lib.rs`), WASM (`crates/genomehubs-query/src/lib.rs`),
   and extendr (`templates/r/lib.rs.tera` + `templates/r/extendr-wrappers.R.tera`)
   — follows the 6-touchpoint checklist in AGENTS.md
3. Add `record()`, `lookup()`, `summary()`, `msearch()` methods to all 3 SDK templates,
   following the same pattern as `count()` / `search()`
4. **v2/v3 SDK transition:** HTTP methods default to v3 (JSON body); for any endpoint absent
   from the `/status` `supported` list, fall back to v2 URL-param format; metadata endpoints
   (`get_taxonomies()`, `get_ranks()`) auto-unwrap v3 envelopes to return plain lists matching
   the v2 contract

**Files:** `crates/genomehubs-query/src/parse.rs`, `src/lib.rs`,
`crates/genomehubs-query/src/lib.rs`, `templates/r/lib.rs.tera`,
`templates/r/extendr-wrappers.R.tera`, `templates/r/query.R`, `templates/js/query.js`,
`python/cli_generator/query.py`

---

## Phase 4: Report Axis Type System

_New module in `genomehubs-query`; pure types and parsers, no I/O._

New module: `crates/genomehubs-query/src/report/`

### Key types

```rust
pub enum AxisRole { X, Y, Z, Cat }
pub enum ValueType { Numeric, Keyword, Date, GeoPoint, TaxonRank }
pub enum Scale { Linear, Log, Log2, Log10, Sqrt, Ordinal, Date }
pub enum AxisSummary { Value, Min, Max, Count, Length, Mean, Median }

pub struct AxisOpts {
    pub fixed_values: Vec<String>,       // pre-defined terms
    pub domain: Option<[f64; 2]>,        // explicit min/max
    pub size: usize,                     // max buckets / categories
    pub show_other: bool,                // emit "other" bucket
    pub scale: Scale,
    pub sort: SortMode,
    pub interval: Option<DateInterval>,  // 1d | 1w | 1M | 3M | 1y | 10y
}

pub struct AxisSpec {
    pub field: String,
    pub role: AxisRole,
    pub summary: AxisSummary,
    pub value_type: Option<ValueType>,   // inferred from field metadata
    pub opts: AxisOpts,
}

pub struct BoundsResult {
    pub domain: [f64; 2],
    pub tick_count: usize,
    pub interval: Option<DateInterval>,
    pub scale: Scale,
    pub value_type: ValueType,
    pub fixed_terms: Vec<String>,
    pub cat_labels: HashMap<String, String>,
}
```

### Axis interchangeability rule

Any field type may be assigned to any axis role:

- Keyword field as x/y → ordinal histogram (terms agg)
- Numeric/date field as `cat` → binned into terms using `interval` or `size`
- Taxon rank as `cat` → terms agg on `lineage.taxon_rank`

### Parsers

`AxisOpts::from_str(s)` — parses the `;`-delimited opts string used by the existing UI:

```
"fixed_values;domain_min,domain_max;size[+];scale"
```

Examples: `";;20;log10"`, `"flow cytometry,null;;5+"`, `";;1M"` (monthly date bins).

Cover with proptest round-trip property tests.

### Date interval

`DateInterval` enum with variants `Day`, `Week`, `Month`, `Quarter`, `Year`, `Decade` maps to
ES `calendar_interval` values (`1d`, `1w`, `1M`, `3M`, `1y`, `10y`). When set, overrides the
tick-count-driven auto-interval that currently produces inflexible bins.

---

## Phase 5: Report Infrastructure in `genomehubs-api`

_Depends on Phase 4; sits in `crates/genomehubs-api/src/report/`._

### 5.1 Bounds computation — `bounds.rs`

```rust
pub async fn compute_bounds(
    spec: &AxisSpec,
    base_query: &SearchQuery,
    taxonomy: &str,
    es_client: &EsClient,
) -> Result<BoundsResult>
```

- Single ES round-trip per axis:
  - Numeric → `stats` agg for min/max
  - Keyword / taxon rank → `terms` agg for top-N values
  - Date → `date_range` agg; passes `calendar_interval` from `spec.opts.interval`
- Replaces the 5–15 independent `getBounds()` calls scattered across v2 report handlers

### 5.2 Aggregation builder — `agg.rs`

```rust
pub trait AggBuilder {
    fn build(&self) -> serde_json::Value;
    fn extract(&self, resp: &serde_json::Value) -> RawBuckets;
}
```

Concrete types:

- `HistogramAggBuilder` — `histogram` agg for numeric fields
- `DateHistogramAggBuilder` — `date_histogram` agg, accepts `calendar_interval`
- `TermsAggBuilder` — `terms` agg for keyword / rank fields
- `StatsAggBuilder` — `stats` agg for bounds
- `GeoHashAggBuilder` — `geohash_grid` agg for map reports
- `ReverseNestedAggBuilder` — for climbing lineage in tree reports

`CompositeAggBuilder` nests children: `.with_sub_agg(builder)`.

### 5.3 Post-processing pipeline — `pipeline.rs`

```rust
pub trait PipelineStep {
    fn apply(&self, input: RawBuckets, ctx: &ReportContext) -> RawBuckets;
}

pub struct Pipeline {
    steps: Vec<Box<dyn PipelineStep>>,
}
impl Pipeline {
    pub fn run(&self, raw: RawBuckets) -> ProcessedBuckets { ... }
}
```

Steps:

- `ScaleStep` — apply scale transform to bucket keys (log, sqrt, date formatting)
- `NullStep` — allocate missing-value buckets proportionally (1D and 2D)
- `CatLabelStep` — resolve category IDs to display names via supplementary ES query
- `RawDataStep` — fetch raw records when count < scatter threshold

---

## Phase 6: Report Types (API + SDK)

**API route:** `POST /api/v3/report` with body `{ query_yaml, params_yaml, report_yaml }`

`report_yaml` drives all report-specific options:

```yaml
report: histogram
x: genome_size
x_opts: ";;20;log10"
cat: assembly_level
cat_opts: ";;5+"
scatter_threshold: 100 # scatter only
```

Implement in order (each builds on the previous):

| Step | Report type            | Key notes                                                                                                                                                    |
| ---- | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 6.1  | **Histogram**          | 1D + 1D+cat + 2D scatter grid; reference implementation proving Phase 5 architecture                                                                         |
| 6.2  | **Scatter (raw mode)** | When `result_count < scatter_threshold`, fetch raw points `{ x, y, cat, taxonId, scientific_name }`; grid and raw modes in same handler, dispatched by count |
| 6.3  | **xPerRank**           | Parallel count at each rank; reuses `/count` infrastructure                                                                                                  |
| 6.4  | **Sources**            | Terms agg on source metadata fields; no bucketing                                                                                                            |
| 6.5  | **Tree**               | `reverse_nested` + `terms` on `lineage.taxon_id`; LCA from result set; JSON + `text/x-nh` Newick via content negotiation                                     |
| 6.6  | **Map**                | `geohash_grid` + `terms` on region field; response `{ status, report: { hexBins, regionCounts, locationBounds } }`                                           |

### SDK additions (parallel with API steps)

**`ReportBuilder`** in `crates/genomehubs-query/src/report/builder.rs`:

- Wraps `SearchQuery`; adds axes via `set_x(field, opts?)`, `set_y(field, opts?)`, `set_cat(field, opts?)`
- `set_report_type(type)`, `to_report_yaml()` — YAML roundtrip matching API request body
- Exposed via PyO3, WASM, extendr (6-touchpoint checklist)

**SDK methods** in all 3 templates:
`histogram(x, opts?)`, `scatter(x, y, opts?)`, `tree(x?, y?)`, `map(location_field)`

**Parse functions** in `crates/genomehubs-query/src/parse.rs`:

- `parse_histogram_json(raw) -> String` — buckets → flat JSON `[{ x, count, cat? }]`
- `parse_tree_json(raw) -> String` — tree nodes → flat list `[{ id, parent, name, count }]`
- `to_plot_dataframe(raw, report_type, format) -> String` — long-format JSON for matplotlib / ggplot / Vega:
  - Histogram → `[{ x, y, cat }]`
  - Scatter → `[{ x, y, z, cat }]`
  - Tree → flat node list with `parent` column
  - Map → GeoJSON FeatureCollection

---

## Phase 7: Arc Reports

_Depends on Phases 5–6. Structurally simple but has a unique axis semantics._

**Arc reports** show proportional relationships between three queries: "of taxa matching Y,
what fraction also match X? And of those matching Z?"

Key distinction from all other report types: **x, y, z are query strings** (e.g.
`"genome_size>1000000"`), ANDed with the main query — not field names.

`report_yaml`:

```yaml
report: arc
x: "country=BR"
y: "genome_size>1000000"
z: "gc_percent>45" # optional; defaults to y
```

1. `crates/genomehubs-api/src/report/arc.rs`:
   - `combine_queries(a: &str, b: &str) -> String` — joins with `AND`
   - Runs 3 parallel count queries: `main+x+y`, `main+y`, `main+x+z`; reuses `build_count_body()`
2. Response: `{ status, report: { arc, arc2?, x, y, z?, xTerm, yTerm, zTerm?, xQuery, yQuery, queryString } }`
3. SDK: `arc(x, y, z?)` method on `ReportBuilder`; no special parse transform (scalars only)

---

## Phase 8: Per-Site Swagger Customization

_Independent of Phases 1–7; can start at any time._

1. **`docs/api_examples.yaml`** per generated project (written at `new`/`update` time):

   ```yaml
   endpoints:
     search:
       examples:
         - name: "Find all eukaryotic species with genome size data"
           query_yaml: |
             index: taxon
             taxa: ["Eukaryota"]
             attributes:
               - name: genome_size
                 operator: exists
   ```

2. **Generator** (`src/commands/new.rs`): reads `sites/{site}/examples.yaml` if present; otherwise
   auto-generates illustrative examples from the site's known fields; writes
   `docs/api_examples.yaml` to generated project

3. **API server** (`crates/genomehubs-api/src/main.rs`): reads `docs/api_examples.yaml` at
   startup; merges into utoipa-generated spec's `components.examples` and per-path `examples`;
   per-site file overrides generic defaults

---

## Phase 9 (Late): URL Query String Support

_Maintains v2 URL parity; enables copy-paste from browser URL bar._

The v2 API accepts all parameters as URL query strings; UI URLs map directly to these with minor
key differences (e.g. `fields` comma-separated vs array). Supporting query strings in v3 (as a
fallback alongside JSON body) is valuable for user exploration.

1. `crates/genomehubs-api/src/qs_adapter.rs`:
   - `qs_to_query_yaml(params: &HashMap<String, String>) -> Result<String>` — converts
     `query=`, `result=`, `rank=`, `fields=`, etc. to `SearchQuery` YAML
   - `qs_to_params_yaml(params: &HashMap<String, String>) -> Result<String>` — converts
     `size=`, `from=`, `sortBy=`, `sortOrder=` etc. to `QueryParams` YAML
   - Handles UI key aliases (e.g. `tax_rank(X)` query fragment → `rank: X` in YAML)
2. All query routes accept query string **or** JSON body; JSON body takes priority
3. SDK always uses JSON body; this is a server-side compatibility layer only

---

## Scope Boundaries

### In scope

- All v2 API endpoints except `/download`
- SDK coverage in Python, JavaScript, R for all new endpoints
- Report types: histogram (1D + 2D + raw scatter), xPerRank, sources, tree, map, arc
- Date interval user control
- Axis interchangeability (any type in any role)
- Per-site swagger examples via YAML config
- URL query string fallback (Phase 9, late)

### Out of scope (deferred)

| Item                                      | Reason                                                                    |
| ----------------------------------------- | ------------------------------------------------------------------------- |
| `/download`                               | File streaming requires infrastructure decisions (disk path, S3 redirect) |
| `/phylopic`                               | External service proxy                                                    |
| `/progress`                               | Async job tracking — not needed initially                                 |
| Oxford plot                               | Part of wider report family; separate design effort                       |
| Arc/arc2 multi-axis queries at rank level | Complex; first iteration covers simple arc                                |
| ES phrase-suggest in `/lookup`            | Requires trigram field mapping in ES index                                |
| R/JS CLI snippet type                     | Separate plan (sdk-parse-parity-plan.md Phase 2)                          |

---

## Key Decisions

| Topic                             | Decision                                                                                                                                        |
| --------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| **Return envelope**               | All endpoints: `{ status: { success, hits?, took?, error? }, ...payload }`                                                                      |
| **`taxonomicRanks` response key** | `ranks` — fix current wrong `taxonomic_ranks` in Phase 0                                                                                        |
| **`taxonomies` wrapping**         | Keep `{ taxonomies: [], last_updated }` (v3 breaking change accepted)                                                                           |
| **`indices` as string list**      | Keep (v3 breaking change accepted)                                                                                                              |
| **API version detection**         | `/status` returns `supported: [...]`; SDK probes once at build time; `404` → all v2; mixed-mode only if simple; else build-time config variable |
| **Report request format**         | Separate `report_yaml` body field alongside `query_yaml` + `params_yaml`                                                                        |
| **Axis interchangeability**       | Any field type usable in any axis role; numeric/date → binned cats; keyword → ordinal histogram                                                 |
| **Date interval**                 | `interval` key in `AxisOpts` YAML (`1d`, `1w`, `1M`, `3M`, `1y`, `10y`); maps to ES `calendar_interval`; overrides auto tick-count logic        |
| **Scatter raw/grid threshold**    | Configurable `scatter_threshold` in `report_yaml`; default 100                                                                                  |
| **Arc y/z axes**                  | Query strings combined with main query using `AND`; not field selectors                                                                         |
| **ES search templates**           | Python data-loader only; NOT used by the API; all v3 logic in direct ES DSL                                                                     |
| **`ReportBuilder`**               | Separate type wrapping `SearchQuery`; not methods on `QueryBuilder`                                                                             |
| **Plotting output**               | `to_plot_dataframe()` returns long-format JSON; consumer calls pandas/polars/R                                                                  |
| **Oxford plot**                   | Deferred: wider report family, separate design phase                                                                                            |

---

## Verification Per Phase

1. `cargo test --workspace` after each phase
2. Manual `curl` against live ES to validate response shapes
3. Compare v2 (Node.js) vs v3 (Rust) output on identical queries
4. `bash scripts/dev_site.sh --python goat` after Phase 6 (end-to-end with report SDK)
5. SDK fixture tests: `bash scripts/test_sdk_fixtures.sh --site goat --python`
6. Plotting smoke test: generate histogram, call `to_plot_dataframe()`, render with matplotlib/ggplot
