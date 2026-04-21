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

### 3.5 Batch queries: `MultiQueryBuilder` + CLI `--file` + dynamic pagination

Batch queries enable users to run multiple independent searches in one operation. The strategy is:

- **SDK**: new `MultiQueryBuilder` struct; frozen shared params; per-query-overridable filters/size/sort
- **CLI**: `--file <path>` flag accepts taxa lists or YAML patch arrays
- **Dynamic execution**: mcount assessment → adaptive strategy (single msearch / batch msearch / paginated mix)
- **Result reassembly**: all results returned preserving query → result association

#### 3.5.1 `MultiQueryBuilder` struct (Phase A)

**Design principle:** Filters vary per-query; non-filter params are shared or overridable.

**Public interface (Python/JS/R):**

```python
# Build batch query
mq = MultiQueryBuilder("taxon")
mq.set_size(100)  # shared across all queries
mq.set_include_estimates(True)  # frozen; error on divergence
mq.set_taxonomy("ncbi")  # frozen; error on divergence

# Add individual queries (filters per-query, size/sort overridable)
q1 = QueryBuilder("taxon").set_taxa("caenorhabditis").set_sort("genome_size", "asc")
q2 = QueryBuilder("taxon").set_taxa("homo sapiens").set_sort("genome_size", "desc")  # size override
mq.add_query(q1)  # warn if q1.size diverges from batch size=100
mq.add_query(q2, warn_on_param_divergence=False)  # suppress warning

# Execute with smart pagination
results = mq.search(format="json")  # or search(format="tsv")
```

**Param categories:**

| Category               | Parameters                                  | Per-query? | Behavior                                                        |
| ---------------------- | ------------------------------------------- | ---------- | --------------------------------------------------------------- |
| **Filters**            | taxa, rank, attributes, assemblies, samples | ✓          | Unrestricted; per-query variation expected                      |
| **Frozen params**      | include_estimates, taxonomy                 | ✗          | Stored on MultiQueryBuilder; hard error on per-query divergence |
| **Overridable params** | limit (size), sortBy, sortOrder             | ⚠️         | Shared default; warn if query diverges (suppressible)           |

**Execution logic:**

1. **Assess payload risk**
   - Calculate: `total_payload_size = num_queries × median_limit`
   - If > 1000: proceed to Step 2 (run mcount)
   - Else: jump to Step 3 (execute with full strategy)

2. **Mcount phase** (hit count assessment)
   - POST msearch to API with `limit: 0` per query (no result rows, just hit counts)
   - Parse response; sum `results[i].total` across all entries

3. **Determine execution strategy** (post-mcount)
   - If `total_hits < 10,000`: execute **single msearch** with full limit per query
     - Results overflow (per-query `total > limit`) paginated via `/searchPaginated` per-query
   - If `total_hits >= 10,000`: **mixed strategy**
     - Move queries with `total > 5,000` to `search_all()` (fetch all via /searchPaginated)
     - Batch remaining queries into groups (max **100** searches per request — API hard limit)
     - Send multiple msearch payloads sequentially
   - Never submit > 100 searches in a single msearch call

4. **Reassemble results**
   - Collect results from all msearch batches + paginated queries
   - Preserve input query order (map result[i] to query[i])
   - Return per-query result boundaries (SDK: `Vec<Vec<Record>>`; CLI: flat with optional query column)

**API request schema (POST `/api/v2/msearch`):**

```json
{
  "searches": [
    {
      "query": "tax_tree(Caenorhabditis) AND tax_rank(species)",
      "result": "taxon",
      "taxonomy": "ncbi",
      "fields": "genome_size,assembly_level",
      "limit": 100,
      "offset": 0,
      "sortBy": "genome_size",
      "sortOrder": "asc",
      "includeEstimates": true
    },
    {
      "query": "tax_name(Homo sapiens) AND tax_rank(species)",
      "result": "taxon",
      "fields": "genome_size",
      "limit": 50,
      "sortBy": "genome_size",
      "sortOrder": "desc"
    }
  ]
}
```

Key points from the API spec:

- Array key is **`searches`** (not `queries`)
- `limit` is the page size key (not `size`); range 1–10 000, default 100
- Sort uses separate **`sortBy`** and **`sortOrder`** fields (not `sort: field:order`)
- Fields are **per-search** (no batch-level `fields` or `columns`)
- Max **100** items in `searches` array (API `maxItems: 100`)
- The raw `query` string is the compiled filter string from `build_query_url`, not YAML

**API response schema (POST `/api/v2/msearch`):**

```json
{
  "status": {"success": true, "hits": 12500, "took": 245},
  "results": [
    {"status": "ok", "count": 50, "total": 5200, "hits": [...], "error": null},
    {"status": "ok", "count": 50, "total": 7300, "hits": [...], "error": null}
  ]
}
```

Each `results[i]` corresponds to one input query; `hits` is a record array
(same shape as `/search` `results`). Top-level `status.hits` is sum of all `results[*].total`.

#### 3.5.2 CLI `--file` flag (Phase B)

**New flags to `main.rs.tera`:**

- `--file <path>`: read queries from file (full search YAML, patch array, or bare taxon list)

**File format detection** (auto, no flag needed):

| Format           | Detected by                            | Behavior                                                                         |
| ---------------- | -------------------------------------- | -------------------------------------------------------------------------------- |
| Full search YAML | Top-level `queries:` key present       | Parse `shared:` section into base QB; patch each entry in `queries:` list on top |
| Patch array      | First non-comment token is `[` or `- ` | Parse each dict as a query patch; apply to CLI-derived base QB                   |
| Bare taxon list  | Neither of the above                   | Treat each non-empty line as a taxon name                                        |

**Full search YAML format:**

```yaml
# queries.yaml — self-contained, no CLI flags needed alongside
shared:
  rank: species
  fields: [genome_size, assembly_level]
  sort: genome_size:desc
  size: 100
  include_estimates: true
  taxonomy: ncbi

queries:
  - taxon: caenorhabditis
  - taxon: homo sapiens
    size: 50 # overrides shared size; warn unless suppressed
    sort: genome_size:asc
  - taxon: mus musculus
    filter: "assembly_level = chromosome"
```

**Allowed keys in `shared:`:**

All `QueryBuilder` setter keys **except** `taxa`, `assemblies`, and `samples` (those are per-query by definition):

| Key                  | Maps to                          |
| -------------------- | -------------------------------- |
| `rank`               | `set_rank`                       |
| `fields`             | `add_field` (list)               |
| `names`              | `set_names`                      |
| `ranks`              | `set_ranks`                      |
| `sort`               | `set_sort`                       |
| `size`               | `set_size`                       |
| `include_estimates`  | `set_include_estimates`          |
| `taxonomy`           | `set_taxonomy`                   |
| `filter` / `filters` | `add_attribute` (list or single) |

Using `taxa`, `assemblies`, or `samples` in `shared:` is a hard error:
`"'taxa' is not valid in shared:; set per-query under queries:"`.

**Flag precedence** (highest to lowest):

```
CLI flag  >  shared: section  >  per-query default
```

A CLI `--sort` overrides the file's `shared: sort:`; a per-query `sort:` overrides
`shared:` only (not the CLI). This lets users override a stored file ad-hoc without
editing it.

**Integration with existing flags:**

- `--file --filter "genome_size >= 1e9"`: applies attribute to all queries (CLI > shared)
- `--size 100`: overrides `shared: size:` for all queries
- `--sort field:order`: overrides `shared: sort:` for all queries
- `--include-estimates`, `--taxonomy`: override `shared:` equivalents
- `--all`: enables paginated fetch per query
- `--format json|tsv|csv`: controls output format
- `--include-query-column`: show query index/label in output (default: true if >1 query)
- `--suppress-divergence-warnings`: suppress per-query size/sort divergence warnings
- `--verbose`: show execution plan (hit counts, batch strategy, pagination strategy)

**Result output:**

| Format          | Output                                       | Preserves association? |
| --------------- | -------------------------------------------- | ---------------------- |
| `--format json` | `[{query: {...}, results: [...]}, ...]`      | ✓ Structured           |
| `--format tsv`  | Merged table with `query` column (0-indexed) | ✓ Via column           |
| `--format csv`  | Same as TSV, comma-delimited                 | ✓ Via column           |

**Example usage:**

```bash
# Bare taxon list — remaining params from CLI
goat-cli taxon search --file taxa.txt --fields genome_size --sort genome_size:desc

# Patch array — CLI flags augment per-query patches
cat > queries.yaml << EOF
- taxon: caenorhabditis
  size: 50
- taxon: homo sapiens
  size: 100
  sort: genome_size:desc
EOF
goat-cli taxon search --file queries.yaml --filter "assembly_level = chromosome" --format tsv

# Full search YAML — entirely self-contained
goat-cli taxon search --file full_search.yaml --format json

# Ad-hoc override of a stored file
goat-cli taxon search --file full_search.yaml --size 10 --format tsv
```

**Parsing logic:**

```python
FORBIDDEN_SHARED_KEYS = {"taxa", "assemblies", "samples"}

SHARED_KEY_ALLOWED = {
    "rank", "fields", "names", "ranks", "sort", "size",
    "include_estimates", "taxonomy", "filter", "filters",
}

def load_queries_from_file(file_path, cli_qb):
    """Parse file; return (base_qb, list[patch_dict]).

    cli_qb is a QueryBuilder already populated from CLI flags.
    The returned base_qb has shared: merged in under CLI flags.
    """
    content = read_file(file_path).strip()
    parsed = yaml.safe_load(content)

    if isinstance(parsed, dict) and "queries" in parsed:
        # Full search YAML
        shared = parsed.get("shared", {})
        bad = set(shared) & FORBIDDEN_SHARED_KEYS
        if bad:
            raise ValueError(f"{bad} not valid in shared:; set per-query under queries:")
        base_qb = apply_shared(cli_qb.clone(), shared)   # CLI already wins (clone preserves it)
        patches = parsed["queries"]
    elif isinstance(parsed, list):
        # Patch array
        base_qb = cli_qb.clone()
        patches = parsed
    else:
        # Bare taxon list
        base_qb = cli_qb.clone()
        patches = [{"taxon": line} for line in content.splitlines() if line.strip()]

    queries = []
    for patch in patches:
        qb = base_qb.clone()
        apply_patch(qb, patch)   # warns on size/sort divergence from base_qb
        queries.append(qb)
    return queries
```

#### 3.5.3 Divergence handling & validation (Phase C)

**Frozen param divergence** (hard error):

- `include_estimates` and `taxonomy` may be set in `shared:` or via CLI flags
- They may **not** be overridden in a per-query entry (they must be uniform across a batch)
- If a per-query entry carries either key: hard error immediately
- Message: `"'include_estimates' cannot be set per-query; set it in shared: or via --include-estimates"`

**Overridable param divergence** (suppressible warning):

- `size` and `sort` can diverge per-query
- Default: warn on divergence from the shared/CLI value
- Suppress via: `add_query(..., warn_on_param_divergence=False)` (SDK) / `--suppress-divergence-warnings` (CLI)
- Message: `"Query 3: size=50 overrides shared size=100"`
- Applied at parse time (not execution time) so the user sees warnings before any network call

**Filter validation:**

- Filters (`taxa`, `rank`, `attributes`, etc.) are per-query; no batch-level constraint
- Validation happens at API time (not pre-flight)

#### 3.5.4 User feedback & capacity limits

**CLI logging:**

- If `num_queries × size > 1000`: `"Assessing batch size... (mcount in progress)"`
- If total_hits > 5000: `"Large result set detected; using adaptive pagination"`
- Per-query warning (if >500 hits): `"Query <N>: <hits> results; paginating..."`
- Summary after execution: `"Executed <N> queries; total <M> results; <K> msearch batches + <L> paginated fetches. Total time: <Xs."`
- Optional `--verbose`: show hit counts and strategy per query

**Capacity limits:**

- Warn if file > 100 queries (single msearch batch will be split)
- Hard error if file > 1000 queries
- Warn if single query `total > limit` (results will be paginated)

**SDK feedback (logging):**

- Python: `logging.info()` calls (no output unless configured)
- JS: `console.warn()` / `console.info()` (suppressible)
- R: `message()` / `warning()` (suppressible via R options)

#### 3.5.5 File inventory (Phase 3.5)

**New files:**

| File                                          | Purpose                                                                      |
| --------------------------------------------- | ---------------------------------------------------------------------------- |
| `python/cli_generator/multi_query_builder.py` | `MultiQueryBuilder` class; implements mcount / strategy / reassembly         |
| `tests/python/test_multiquery_builder.py`     | Unit tests: SDK parity, divergence warnings, mcount logic, result reassembly |
| `tests/python/test_msearch_cli.py`            | CLI file parsing tests (YAML, bare-list, edge cases)                         |

**Modified files:**

| File                             | Changes                                                                                              |
| -------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `src/main.rs.tera`               | Add `--file`, `--include-query-column`, `--suppress-divergence-warnings`, `--verbose` flags          |
| `src/commands/mod.rs`            | New `msearch.rs` module (if CLI handler separated)                                                   |
| `templates/js/query.js`          | Add `MultiQueryBuilder` class (mirrors Python); implements same mcount / strategy / reassembly logic |
| `templates/r/query.R`            | Add `multi_query_builder()` reference class (mirrors Python/JS)                                      |
| `python/cli_generator/query.py`  | Add per-language SDK execution method (HTTP + result parsing)                                        |
| `scripts/test_sdk_generation.sh` | Add msearch integration test (3+ queries, mcount assessment, pagination if >10k)                     |
| `GETTING_STARTED.md`             | Add "Batch Queries" section with file format examples and SDK examples                               |

**Generated (not hand-written):**

- None new (msearch uses existing parse functions)

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

## Phase 4.5: Universal JavaScript SDK (Node + Browser) _(depends on Phase 4)_

The JavaScript SDK currently targets Node.js only. Browser support requires:

1. Dual WASM builds (Node.js + web)
2. Universal module resolution and entry points
3. CORS-aware HTTP wrapper
4. Browser-specific CI testing

### 4.5.1 WASM Build Targets

**Current state:** `build-wasm.sh.tera` uses `--target nodejs`.

**Change:** Generate dual WASM builds:

```bash
# In build-wasm.sh.tera
cd "$REPO_ROOT/crates/genomehubs-query"
wasm-pack build --target web     --features wasm  # pkg-web/
wasm-pack build --target nodejs  --features wasm  # pkg-nodejs/
```

**Output structure** (in generated project):

```
js/{site}/
  ├── pkg-web/              # WASM + .js glue (browser-ready)
  │   ├── genomehubs_query.wasm
  │   ├── genomehubs_query.js
  │   ├── genomehubs_query.d.ts
  │   └── package.json       # type: module
  ├── pkg-nodejs/            # Node.js-specific bindings
  │   ├── genomehubs_query.wasm
  │   ├── genomehubs_query.js
  │   └── package.json
  ├── query.js               # Universal wrapper (env-aware)
  └── package.json           # Declares both entry points
```

**Key differences:**

| Aspect                  | `--target web` | `--target nodejs`           |
| ----------------------- | -------------- | --------------------------- |
| Contains `.wasm` binary | ✓              | ✓ (same)                    |
| Module format           | ESM            | ESM (Node-specific)         |
| Async initialization    | Required       | Optional                    |
| File path resolution    | Bundler-aware  | Native path                 |
| Browser-compatible      | ✓              | ✗ (Node-only require paths) |
| Tree-shaking            | ✓              | ✗                           |

### 4.5.2 Universal Module Entry Points

**Update `templates/js/package.json.tera`:**

```json
{
  "name": "@{{ site_name }}/goat",
  "version": "1.0.0",
  "type": "module",
  "main": "./query.js",
  "exports": {
    ".": {
      "node": "./query.node.js",
      "browser": "./query.browser.js",
      "import": "./query.js"
    }
  },
  "browser": {
    "./query.js": "./query.browser.js"
  },
  "engines": {
    "node": ">=18"
  },
  "files": [
    "query.js",
    "query.node.js",
    "query.browser.js",
    "pkg-web/",
    "pkg-nodejs/"
  ]
}
```

**New templates:**

- `query.node.js` — Imports from `./pkg-nodejs/`; includes Node.js-specific setup
- `query.browser.js` — Imports from `./pkg-web/`; includes browser-specific setup
- `query.js` — Universal wrapper (uses environment detection to require the right one)

**Environment detection logic:**

```javascript
// query.js (universal wrapper)
const isNode = typeof process !== 'undefined'
  && process.versions?.node;
const entrypoint = isNode ? './query.node.js' : './query.browser.js';
export * from entrypoint;
```

Or use conditional exports (preferred for bundlers):

```javascript
// query.js — merely re-exports from the correct target
export * from "#node"; // for Node.js
export * from "#web"; // for browsers
```

With `package.json` `exports` field directing bundlers to the right file.

### 4.5.3 CORS and Fetch Wrapping

Browser fetch requests are blocked by CORS if the API doesn't send appropriate headers.

**Required:** API server must set:

```
Access-Control-Allow-Origin: *
// or: Access-Control-Allow-Origin: https://your-ui-domain.com
Access-Control-Allow-Methods: GET, POST, OPTIONS
Access-Control-Allow-Headers: Content-Type, Accept
```

**SDK wrapper** (already uses `fetch`, which respects CORS):

The current QueryBuilder methods (`count()`, `search()`, `searchAll()`) already use
native `fetch()`, which works in browsers. No changes needed if the API sets CORS
headers.

**For problematic APIs** (CORS not enabled):

Add an optional `apiProxy` configuration:

```javascript
const qb = new QueryBuilder("taxon", {
  apiBase: "{{ api_base_url }}",
  apiProxy: "https://your-proxy.com/api", // Optional: proxy all requests through this
});
```

Document this in `GETTING_STARTED.md` as a fallback only (prefer fixing the API).

### 4.5.4 Browser-specific build files

**New template files:**

| File                                 | Purpose                                                  |
| ------------------------------------ | -------------------------------------------------------- |
| `templates/js/query.node.js.tera`    | Node.js entry; loads `./pkg-nodejs/`                     |
| `templates/js/query.browser.js.tera` | Browser entry; loads `./pkg-web/`; includes CORS warning |
| `templates/js/test.browser.js.tera`  | Vite/Vitest smoke test for browser                       |

**`templates/js/test.browser.js.tera` example:**

```javascript
// Run with Vitest or similar browser test runner
import { QueryBuilder } from './query.js';

describe('QueryBuilder (browser)', () => {
  it('builds a valid URL', () => {
    const qb = new QueryBuilder('taxon')
      .setTaxa(['Mammalia'], 'tree')
      .setSize(10);
    const url = qb.toUrl();
    expect(url).toMatch(/^https:\/\/);
    expect(url).toContain('limit=10');
  });

  it('parses response JSON', () => {
    const mockResponse = {
      status: { hits: 10, ok: true },
      results: [
        { index: 'taxon', id: '9606', result: { scientific_name: 'Homo' } }
      ]
    };
    const records = parseSearchJson(mockResponse);
    expect(records).toHaveLength(1);
    expect(records[0].scientific_name).toBe('Homo');
  });

  // Network tests skipped in browser (no --network flag equivalent)
});
```

### 4.5.5 CI: Browser testing

**Add to `.github/workflows/sdk-integration.yml`:**

```yaml
- name: Browser SDK tests
  run: |
    cd /tmp/e2e-goat/goat-cli/js/goat
    npm install -D vitest jsdom @vitest/browser
    vitest run --browser jsdom test.browser.js
```

Alternatively, use Playwright for e2e browser testing:

```yaml
- name: Browser e2e test (Playwright)
  run: |
    cd /tmp/e2e-goat/goat-cli/js/goat
    npx playwright install chromium
    npx playwright test test.playwright.js
```

### 4.5.6 Generated project updates

**bundle structure** (at generation time):

```bash
cd js/{site}
sh build-wasm.sh          # Produces pkg-web/ and pkg-nodejs/
npm install               # Installs both pkg dirs as local deps
```

Or use `workspaces` in monorepo style:

```json
{
  "workspaces": ["pkg-web", "pkg-nodejs"]
}
```

**Type definitions:** Both `pkg-web` and `pkg-nodejs` include identical `.d.ts` files
from wasm-pack, so TypeScript consumers see the same interface regardless of target.

### 4.5.7 Documentation updates

**In generated project `README.md`:**

```markdown
## Browser Support

The JavaScript SDK works in modern browsers (ES2020+) via WebAssembly.

### Setup

For a browser app (Vite, webpack, etc.):

\`\`\`javascript
import { QueryBuilder, parseSearchJson } from '@{{ site_name }}/goat';

const qb = new QueryBuilder('taxon')
.setTaxa(['Mammalia'], 'tree')
.setSize(50);

const url = qb.toUrl();
const response = await fetch(url);
const data = await response.json();
const records = parseSearchJson(data);
\`\`\`

### Requirements

- **CORs:** The API must allow cross-origin requests. If not, configure a proxy or ask the API owner to enable CORS headers.
- **Modern browser:** ES2020+ (Chrome 91+, Safari 15+, Firefox 88+, Edge 91+)
- **Bundler:** Vite, webpack, Parcel, esbuild, or similar; ensures `.wasm` files are copied to output

### Bundler configuration examples

**Vite (`vite.config.js`):**

\`\`\`javascript
import { defineConfig } from 'vite';
export default defineConfig({
// wasm-pack output is automatically handled
// Ensure public/ or assets/ copies the .wasm file if needed
});
\`\`\`

**webpack:**

\`\`\`javascript
module.exports = {
module: {
rules: [
{
test: /\.wasm$/,
type: 'webassembly/async',
},
],
},
experiments: { asyncWebAssembly: true },
};
\`\`\`
```

**In main `GETTING_STARTED.md` (cli-generator):**

Add "Browser Support" section:

```markdown
### JavaScript: Browser + Node.js

Generated JavaScript SDKs work in both Node.js (≥18) and browsers.

**In Node.js:** Install and use directly.
\`\`\`javascript
const { QueryBuilder } = require('./query.js');
const qb = new QueryBuilder('taxon').setTaxa(['Homo']).toUrl();
\`\`\`

**In browsers:** Use with any bundler (Vite, webpack, etc.).
\`\`\`javascript
import { QueryBuilder } from '@{{ site }}/{{ site }}-cli';
const qb = new QueryBuilder('taxon').setTaxa(['Homo']).toUrl();
\`\`\`

**Important:** The API must have CORS enabled. If requests fail with "blocked by CORS",
contact the API owner to enable `Access-Control-Allow-Origin` headers.
```

### 4.5.8 File inventory (Phase 4.5)

**New/Modified:**

| File                                    | Changes                                                      |
| --------------------------------------- | ------------------------------------------------------------ |
| `templates/js/build-wasm.sh.tera`       | Add `--target web` build; output to separate `pkg-web/`      |
| `templates/js/package.json.tera`        | Add `exports`, `browser` fields; include both `pkg-*` dirs   |
| `templates/js/query.js.tera`            | Universal wrapper using conditional imports or env detection |
| `templates/js/query.node.js.tera`       | **New:** Node.js-specific entry; imports from `pkg-nodejs/`  |
| `templates/js/query.browser.js.tera`    | **New:** Browser-specific entry; imports from `pkg-web/`     |
| `templates/js/test.browser.js.tera`     | **New:** Browser smoke tests (Vitest/jsdom or Playwright)    |
| `.github/workflows/sdk-integration.yml` | Add browser test job (Vitest + jsdom or Playwright)          |
| `GETTING_STARTED.md`                    | Add "Browser Support" section with bundler examples          |

---

## Phase 5: `validate()` parity _(depends on Phase 3)_

**STATUS: ✅ COMPLETE (2026-04-21)**

All tasks for Phase 5 have been successfully implemented:

### 5.1 Move shared types to subcrate ✅

`FieldMeta`, `ValidationConfig`, `ValidationError` → `crates/genomehubs-query/src/validation.rs`.
Main crate re-exports. Subcrate uses `HashMap<String, FieldMeta>` instead of `phf::Map`.

### 5.2 Generator emits `field_meta.json` ✅

Generator writes `src/generated/field_meta.json` alongside `field_meta.rs`.
Generated code:

```rust
pub const FIELD_META_JSON: &str = include_str!("field_meta.json");
pub const VALIDATION_CONFIG_JSON: &str = include_str!("validation_config.json");
```

### 5.3 `validate_query_json` in subcrate ✅

```rust
pub fn validate_query_json(
    query_yaml: &str,
    field_meta_json: &str,
    config_json: &str,
) -> String  // JSON array of error strings
```

Same logic as `validate_query`, but `HashMap` not `phf::Map`.

### 5.4 Expose via WASM and extendr ✅

- WASM: `#[wasm_bindgen]` in `crates/genomehubs-query/src/lib.rs` ✓
- extendr: added to `templates/r/lib.rs.tera` ✓
- Verified via `bash scripts/dev_site.sh --rebuild-wasm --browser --python` ✓

### 5.5 Add `validate()` to JS and R ✅

- JS: `async validate(validationLevel) -> string[]` ✓
- R: `validate() -> character vector` ✓
- Python: JSON path supports both full and partial validation modes ✓

### 5.6 Add validation level configuration ✅ (NEW)

**JavaScript and Python SDKs now support two validation modes:**

- **Full mode** (default): Attempts to fetch metadata from `/api/v3/metadata/fields` and `/api/v3/metadata/validation-config` with graceful 404 fallback. Falls back to embedded metadata if API unavailable.
- **Partial mode**: Skips API fetch entirely, uses only embedded local files. Recommended until v3 API endpoints deployed.

**Implementation:**

```javascript
// JavaScript
const qb = new QueryBuilder("taxon", {
  validationLevel: "full", // or "partial"
  apiBase: "https://goat.genomehubs.org/api",
});
const errors = await qb.validate(); // Uses configured level
const errors = await qb.validate("partial"); // Override for this call
```

```python
# Python
qb = QueryBuilder("taxon", validation_level="full", api_base="https://goat.genomehubs.org/api")
errors = qb.validate()  # Uses configured level
errors = qb.validate(validation_level="partial")  # Override for this call
```

**Graceful Error Handling:**

- Both SDKs silently handle 404s from API endpoints (no log spam)
- Network timeouts caught and fallback to local metadata
- Validation always succeeds; preflight metadata is enhancement, not requirement
- Allows immediate production deployment with partial validation; seamless upgrade to full validation when v3 API ready

**API Refactoring Plan Integration:**

- See [api-aggregation-refactoring-plan.md](api-aggregation-refactoring-plan.md#phase-3-api-v3-endpoints-4-6-weeks) Phase 3 for `/api/v3/metadata/*` endpoint specifications
- Validation metadata endpoints designed for deployment alongside other v3 routes
- Pre-deployment: SDKs work fine in partial mode (embedded metadata only)

**Verification:**

- ✅ Python tests pass with both validation modes
- ✅ JavaScript browser tests demonstrate partial and full mode fallback behavior
- ✅ Dev site build succeeds with all smoke tests passing
- ✅ Validation level parameters properly documented in docstrings
- ✅ Agent-log entry: [agent-logs/2026-04-21_001_validation-level-configuration.md](../../agent-logs/2026-04-21_001_validation-level-configuration.md)

---

## Phase 6: E2E testing + CI _(depends on Phases 0–5)_

**STATUS:** In Planning (Start ~Week 4 post-Phase-5)

### 6.1 SDK parity test (`tests/python/test_sdk_parity.py`)

Introspects `query.py`, `query.js` template, `query.R` template and asserts all
canonical methods from the table above are present in all three. Runs on every PR.
Catches method name drift before it reaches `main`.

**Additional checks:**

- Validation level parameters present in all SDKs (full, partial modes)
- Constructor docstrings document validation_level and api_base options
- API metadata endpoint URLs correctly templated (site-specific api_base)

### 6.2 `scripts/test_sdk_generation.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail
cargo build --release
rm -rf /tmp/e2e-goat
cargo run --release -- new goat --config sites/ --output-dir /tmp/e2e-goat
cd /tmp/e2e-goat/goat-cli

# Rust CLI
cargo test

# Python SDK
maturin develop --features extension-module && pytest python/ -q

# JavaScript SDK
cd js/goat && npm test && npm run validate-test && cd ../..

# R SDK
cd r/goat && Rscript test_basic.R && cd ../..

# Test validation modes (new)
cd js/goat && node test_validation_modes.js && cd ../..
python3 -c "from goat_sdk import QueryBuilder; qb=QueryBuilder('taxon', validation_level='partial'); assert qb.validate() == []"
```

### 6.3 Generated smoke test fixtures

**`templates/js/test_basic.js.tera`:**

- `toUrl()` returns a non-empty HTTPS URL
- `validate()` returns `[]` for a valid query; non-empty for unknown attribute name
- `count()` > 0 (skip if `--no-network`)
- `search()` returns array (skip if `--no-network`)

**`templates/js/test_validation_modes.js.tera` (NEW):**

- **Partial mode:** `validate()` with `validationLevel="partial"` does not attempt API fetch
- **Full mode:** `validate()` with `validationLevel="full"` gracefully handles missing API endpoints (no error)
- Both modes return valid error array type
- Invalid query triggers errors in both modes (unknown field detection)

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

**`tests/python/test_validation_modes.py` (NEW):**

- **Partial mode validation:** QueryBuilder respects `validation_level="partial"`
- **Full mode validation:** QueryBuilder accepts `validation_level="full"`, gracefully handles missing API
- Per-call override: `validate(validation_level="partial")` overrides instance setting
- Custom API base: validation requests directed to custom endpoint if provided
- Timeout handling: Python urllib timeout fires after 5s, falls back gracefully

### 6.4 CI job (`.github/workflows/sdk-integration.yml`)

```yaml
name: SDK Integration Tests

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main, develop]

jobs:
  sdk-parity:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rs/toolchain@v1
        with: { toolchain: stable }
      - uses: actions/setup-python@v4
        with: { python-version: "3.11" }
      - run: pip install -e ".[dev]"
      - run: pytest tests/python/test_sdk_parity.py -v

  sdk-generation:
    needs: sdk-parity
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rs/toolchain@v1
        with: { toolchain: stable, targets: wasm32-unknown-unknown }
      - uses: jetli/wasm-pack-action@v0.4.0
      - uses: actions/setup-python@v4
        with: { python-version: "3.11" }
      - uses: actions/setup-node@v3
        with: { node-version: "18" }
      - uses: r-lib/actions/setup-r@v2
        with: { r-version: "4.2" }
      - run: bash scripts/verify_code.sh
      - run: bash scripts/test_sdk_generation.sh
      - uses: actions/upload-artifact@v3
        with:
          name: generated-pkg-${{ matrix.os }}
          path: crates/genomehubs-query/pkg/
          if-no-files-found: warn

  validation-modes:
    needs: sdk-generation
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rs/toolchain@v1
        with: { toolchain: stable, targets: wasm32-unknown-unknown }
      - uses: jetli/wasm-pack-action@v0.4.0
      - uses: actions/setup-python@v4
        with: { python-version: "3.11" }
      - uses: actions/setup-node@v3
        with: { node-version: "18" }
      - run: cargo build --release
      - run: cargo run --release -- new test_site --config sites/ --output-dir /tmp/test-proj
      - run: |
          cd /tmp/test-proj/test_site-cli/js/test_site
          npm test && node test_validation_modes.js
      - run: |
          cd /tmp/test-proj/test_site-cli
          python3 -m pytest tests/python/test_validation_modes.py -v
```

**Network-dependent tests:**

- Gated to `push` to `main` only (not PRs, to avoid rate limits)
- Marked with `@pytest.mark.network` and `--skip-network` flag
- CI runs with `--skip-network` on PRs, full suite on merge

**Artifact handling:**

- WASM packages built on each platform
- Uploaded as CI artifacts for inspection
- Defer npm publish until post-MVP (avoid version churn)

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

---

## Ongoing parity governance

Once Phases 0–5 complete and CI is green, add to `AGENTS.md`:

- Every new `QueryBuilder` method must be added to all three SDKs in the same PR.
  The parity test (Phase 6.1) enforces this automatically.
- Snippet templates must be updated when methods are renamed.
- Every new parse function needs PyO3, WASM, and extendr exports in the same PR
  (extends the existing 6-touchpoint checklist in `AGENTS.md`).
- Validation level configuration must be consistent across all SDKs:
  - `validation_level` parameter in all constructors
  - Graceful API fetch error handling (404 silence, no log spam)
  - Per-call override support in `validate()` method
- `AGENTS.md` update: only after Phases 0–5 merged and CI green.
