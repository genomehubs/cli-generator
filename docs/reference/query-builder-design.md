# Query builder — design and implementation plan

Written: 2026-03-17

---

## MVP status and focus (updated 2026-03-17)

### Context

Work has forked across three tracks: the CLI (`main.rs.tera`/`client.rs.tera`),
a generic Python SDK (`python/cli_generator/query.py`), and the query-builder
core (`src/core/query/`). The plan below re-focuses on two concrete MVPs needed
for user testing, with strict scope control to avoid half-implemented code.

---

### What is solid today

| Component                                                                                       | State                                  |
| ----------------------------------------------------------------------------------------------- | -------------------------------------- |
| `src/core/query/` — `SearchQuery`, `QueryParams`, URL builder, static validation                | ✅ 56 tests pass                       |
| `python/cli_generator/QueryBuilder` + `build_url` + `merge`/`combine`                           | ✅ 15 tests pass                       |
| Generator infrastructure — fetches `resultFields`, generates `field_meta.rs`, `fields.rs`, etc. | ✅ works end-to-end                    |
| `templates/sdk.rs.tera`, `templates/query.py.tera`                                              | ✅ restored after formatter corruption |
| `.vscode/settings.json` — `.tera` files mapped to plaintext                                     | ✅ formatter locked out                |

### What is broken / disconnected

- **CLI and query module do not talk to each other.** `main.rs.tera` takes a
  raw `--query` string and passes it directly to `client::search()`. The entire
  `SearchQuery`/`QueryParams`/validation stack is bypassed. This is fine for
  the CLI MVP — the raw query string path works — but needs documenting so
  nobody adds a half-wired bridge mid-sprint.
- **No generated repo exists yet.** `cli-generator new goat` has not been run.
  Until it has, the generated templates are untested at compile time.
- **`sdk.rs.tera` / `query.py.tera`** had been reformatted by VS Code's Rust/Python
  formatters. Now fixed; `files.associations` in `.vscode/settings.json`
  prevents recurrence.

---

### CLI MVP — scope

Goal: `goat-cli taxon search --taxon "Mammalia" --taxon-type tree --genome-size`
produces TSV output. User can run it without understanding query YAML.

| Work item                                                               | File(s)                                              | Priority                |
| ----------------------------------------------------------------------- | ---------------------------------------------------- | ----------------------- |
| Fix YAML config gaps: `biosample`, assembly `--assembly`/`--date` flags | `sites/goat-cli-options.yaml`                        | P0 — YAML only, no code |
| `--taxon`/`-t` + `--taxon-type name\|tree\|lineage`                     | `templates/main.rs.tera`                             | P0                      |
| `--url` print mode (print URL, do not fetch)                            | `templates/main.rs.tera`                             | P0 — 5 lines            |
| `--include-estimates` flag                                              | `templates/main.rs.tera`, `templates/client.rs.tera` | P1                      |
| Run `cli-generator new goat` and verify generated repo compiles         | —                                                    | P0 — gate on above      |
| User docs (`GETTING_STARTED.md` with real examples)                     | generated `GETTING_STARTED.md.tera`                  | P0 for user testing     |

**Deferred from CLI MVP:**

- `--file`/msearch batch input
- Count-before-search warning + `searchPaginated`
- `--rank` vs `--ranks` disambiguation
- `--tidy` / `--goat-ui-url`
- `--exclude` (excludeAncestral/excludeMissing)
- Async client + progress bar

---

### Python SDK / MCP MVP — scope

Goal: MCP server (or any Python script) does:

```python
from goat_cli.query import QueryBuilder
results = QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree").search()
```

| Work item                                                                | File(s)                                   | Priority                         |
| ------------------------------------------------------------------------ | ----------------------------------------- | -------------------------------- |
| Run `cli-generator new goat`                                             | —                                         | P0 — generates the goat-cli repo |
| Verify generated goat-cli Python extension builds with `maturin develop` | generated repo                            | P0                               |
| Smoke-test `QueryBuilder.search()` round-trip against live API           | —                                         | P0                               |
| User docs for Python SDK (install, QueryBuilder usage, search/count)     | `docs/` or generated `GETTING_STARTED.md` | P0 for user testing              |

**Deferred from SDK MVP:**

- Rank validation (`ranks.rs.tera`, `/taxonomicRanks` fetch) — plan iteration 2
- Taxon name lookup (`check_taxon_name()`) — plan iteration 3
- Report endpoint (`build_report_url`, `ReportOptions`, `AxisDef`) — plan iteration 4
- `searchPaginated` pagination helper

---

### What should NOT be built yet

- Report endpoint (plan iteration 4) — no user requirement driving it yet
- `--file`/msearch — useful but not blocking user testing
- Rank validation — adds safety but MVP without it is acceptable
- Taxon name lookup — same

---

### Template corruption prevention

VS Code was treating `.tera` files as Rust or Python and reformatting them.
Fix committed to `.vscode/settings.json`:

```json
"files.associations": { "*.tera": "plaintext" }
```

**Never open a `.tera` file and save it without checking this setting is active.**
If the setting is not applied (e.g. on a fresh clone without `.vscode/`), run
`cargo test` immediately after any template edit — the `codegen_renders_all_templates_without_error`
test will catch rendering failures introduced by corruption.

---

Covers `src/core/query/` — a new Rust module that models the intent-driven
query pipeline currently implemented in `goat-nlp/mcp-server` as Python, and
exposes it to:

- the CLI as a `--query-file <path>` flag accepting YAML/JSON
- the Python SDK via PyO3
- the mcp-server (replacing its hand-written URL builders by importing the SDK)

---

## Source of truth: what the mcp-server does today

The mcp-server decomposes an LLM query into two artifacts that converge into a
URL-encoded GoaT API query string. The key Python files and their Rust
equivalents are mapped below.

### Identifiers (`process_identifiers`)

```python
{
  "taxa": ["Mammalia", "!Felis"],   # scientific names; "!" = NOT filter
  "assemblies": ["GCF_000002305.6"],
  "samples": [],
  "rank": "species",                # taxonomic rank for filtering
  "taxon_filter_type": "children",  # "children" | "matching" | "lineage"
}
```

Produces query fragment:
`tax_tree%28Mammalia%2C%21Felis%29` `%20AND%20` `tax_rank%28species%29`

### Attributes (`process_attributes`)

```python
{
  "attributes": [                   # filter conditions
    {"name": "genome_size", "operator": "<", "value": "3000000000",
     "modifier": ["min", "direct"]}
  ],
  "fields": [                       # columns to return
    {"name": "genome_size", "modifier": ["min"]}
  ],
  "names": ["scientific_name"],     # taxon name classes
  "ranks": ["genus", "family"],     # taxonomic ranks to return as columns
}
```

Attribute operators: `=`, `!=`, `<`, `<=`, `>`, `>=`, `in`, `not in`,
`exists`, `missing`.

Attribute modifiers:

- **status** (affect traversal): `direct`, `ancestral`, `descendant`,
  `estimated`, `missing`
- **summary** (aggregate over traversal): `min`, `max`, `median`, `mean`,
  `sum`, `list`, `length`

Both modifier sets may be combined in one attribute dict. Status modifiers are
converted to `excludeXxx[N]=field` URL params; summary modifiers are encoded as
`summary%28field%29` in the query string.

### URL encoding

All query fragments are joined with `%20AND%20`. Identifiers, ranks and
attribute conditions are each URL-percent-encoded individually then joined.
Exclusion params are appended as separate `&excludeXxx[N]=field` query params.

The final API URL for a `search` call looks like:

```
{api_base}/search
  ?query={encoded_query_string}
  &result={index}
  &includeEstimates=true
  &taxonomy=ncbi
  &fields={comma_separated}
  &names={comma_separated}
  &ranks={comma_separated}
  &excludeDirect[0]=field_name
  ...
```

Report calls reuse the same `query`/`result`/`fields` params and add
report-specific ones (`report=histogram`, `x=field`, etc.).

---

## Validation catalogue

Every parameter class goes through validation before URL construction.
The table below records the source of truth for each kind, and whether it can
be resolved at **build time** (static/cached from the API once during code
generation), at **startup** (fetched once when the binary/SDK is first used),
or requires a **live API call** per query.

| Parameter                 | Validation rule                                                                                              | Data source                                                              | Caching strategy                                                                                                                    |
| ------------------------- | ------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------- |
| Attribute name            | Must exist in `resultFields` for that index; if not, try synonym list and normalise to canonical name        | `GET /resultFields?result=<index>` (includes `synonyms` array per field) | **Build-time** — `FieldMeta` map keyed by canonical name; synonym → canonical lookup table also generated.                          |
| Attribute operator        | Must be in allowed set (type-aware: no `<`/`>` for keywords)                                                 | Derived from `processed_type` in `resultFields` response                 | Build-time (type metadata)                                                                                                          |
| Attribute value (enum)    | Must be in `constraint.enum` list                                                                            | `resultFields` `constraint.enum`                                         | Build-time                                                                                                                          |
| Attribute value (numeric) | Must satisfy `constraint.min` / `constraint.max`                                                             | `resultFields` `constraint`                                              | Build-time                                                                                                                          |
| Modifier validity         | Must be in `summary` list + fixed status set; `ancestral`/`descendant` require matching `traverse_direction` | `resultFields` `summary`, `traverse_direction`                           | Build-time                                                                                                                          |
| Rank (filter)             | Must be a valid taxonomic rank                                                                               | `GET /taxonomicRanks`                                                    | **Startup** — fetch once, cache for session (24 h TTL in mcp-server); bake into generated code at build time for offline validation |
| Rank (return column)      | Same as rank (filter)                                                                                        | Same                                                                     | Same                                                                                                                                |
| Taxon name                | Must exist in the datastore                                                                                  | `GET /count?query=tax_tree(name)`                                        | **Live API** — cannot be pre-cached; check_taxon_exists() is its own SDK function                                                   |
| Assembly accession prefix | Must start with GCA*/GCF*/etc.                                                                               | `site.yaml` → `valid_accession_prefixes.assembly` list                   | **Site-configured** — not hard-coded; allows BoaT or other sites to differ                                                          |
| Sample accession prefix   | Must start with SRS/SRR/ERR/etc.                                                                             | `site.yaml` → `valid_accession_prefixes.sample` list                     | **Site-configured**                                                                                                                 |
| Taxon name class          | Must be in allowed set (scientific_name, common_name, …)                                                     | `site.yaml` → `valid_name_classes` list                                  | **Site-configured** — not hard-coded; default list in `site.yaml` template                                                          |
| `taxon_filter_type`       | Must be `name`, `tree`, or `lineage`                                                                         | `site.yaml` → `valid_taxon_filter_types` list                            | **Site-configured** — not hard-coded                                                                                                |
| Search index              | Must be `taxon`, `assembly`, or `sample`                                                                     | Generated from `SiteConfig.indexes`                                      | Build-time                                                                                                                          |

### What can be baked into generated code

The cli-generator already fetches `resultFields` for each index at `update`
time and uses the response to generate `src/generated/fields.rs` (field names)
and `src/generated/indexes.rs` (index constants). **Attribute name, type,
operator validity, value enum constraints, modifier validity,
`traverse_direction`, and synonym → canonical name mapping** can all be derived
from that same response and baked into static Rust data structures in a new
`src/generated/field_meta.rs`. No live API call is needed for attribute
validation at query time.

Taxonomic ranks can be fetched once during `cli-generator update` from
`GET /taxonomicRanks` and baked into `src/generated/ranks.rs`. A 24 h TTL
runtime cache should still be offered for SDK users who don't regenerate often.

Taxon name validation cannot be pre-cached — it requires a live call to
`/count?query=tax_tree(<name>)` per query. This belongs in a standalone
`check_taxon_name()` SDK function (phase 2, after first iteration).

Validation sets that are currently hard-coded in the mcp-server (assembly
prefixes, sample prefixes, taxon name classes, taxon filter types) must be
centrally configurable rather than hard-coded in Rust. They live in `site.yaml`
under a new `validation:` block with sensible defaults, so a custom GoaT
instance or BoaT can override them without touching generated code.

---

## Module layout

```
src/core/
  query/
    mod.rs          — re-exports; SearchQuery top-level struct + build_query_url()
    identifiers.rs  — Identifiers struct + URL fragment builder
    attributes.rs   — Attribute, AttributeOperator, Modifier, Field structs
    url.rs          — params_dict_to_url(), encode helpers, exclusion builder
    validation.rs   — validate_attribute(), validate_operator(), validate_value()
                      All static; no async. Uses FieldMeta from generated code.
```

Generated files (new, added to generator templates):

```
src/generated/
  field_meta.rs     — per-index HashMap<&str, FieldMeta>; built from resultFields
  ranks.rs          — static &[&str] of valid taxonomic ranks
```

---

## Core structs (serde + YAML/JSON round-trip)

### `SearchQuery` — the WHAT

Represents what to search for. Serialises to/from YAML or JSON for
`--query-file` input and SDK usage.

```rust
/// Top-level query — corresponds to process_identifiers + process_attributes combined.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    /// Which index to search.
    pub index: SearchIndex,
    #[serde(flatten)]
    pub identifiers: Identifiers,
    #[serde(flatten)]
    pub attributes: AttributeSet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchIndex { Taxon, Assembly, Sample }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Identifiers {
    #[serde(default)]
    pub taxa: Vec<String>,            // "!" prefix = NOT filter
    #[serde(default)]
    pub assemblies: Vec<String>,
    #[serde(default)]
    pub samples: Vec<String>,
    #[serde(default)]
    pub rank: Option<String>,         // single rank for query filter (--rank)
    #[serde(default = "default_taxon_filter_type")]
    pub taxon_filter_type: TaxonFilterType,
}

/// Controls which API taxon wrapper function is used.
///
/// CLI flag: `--taxon-type` (aligns with gap-analysis item 1).
/// Variant names match the gap-analysis user-facing names.
///
/// | Variant   | API function      | Old CLI flag       | mcp-server value |
/// |-----------|-------------------|--------------------|------------------|
/// | `Name`    | `tax_name(X)`     | (default)          | `matching`       |
/// | `Tree`    | `tax_tree(X)`     | `--descendants`    | `children`       |
/// | `Lineage` | `tax_lineage(X)`  | `--lineage`        | `lineage`        |
///
/// `--descendants` and `--lineage` from old goat-cli are deprecated in favour
/// of `--taxon-type tree` / `--taxon-type lineage` (gap-analysis item 1).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaxonFilterType { #[default] Name, Tree, Lineage }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AttributeSet {
    #[serde(default)]
    pub attributes: Vec<Attribute>,   // filter conditions
    #[serde(default)]
    pub fields: Vec<Field>,           // columns to return
    #[serde(default)]
    pub names: Vec<String>,           // taxon name classes (passed as &names=, NOT &fields=)
    #[serde(default)]
    pub ranks: Vec<String>,           // rank columns to return (--ranks, gap-analysis item 4)
}

/// An attribute filter or presence test.
///
/// `name` may be a synonym; validation normalises it to the canonical API name
/// using the generated synonym → canonical lookup table in `field_meta.rs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribute {
    pub name: String,
    #[serde(default)]
    pub operator: Option<AttributeOperator>,
    #[serde(default)]
    pub value: Option<AttributeValue>,
    #[serde(default)]
    pub modifier: Vec<Modifier>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    pub name: String,                 // may be a synonym; normalised during validation
    #[serde(default)]
    pub modifier: Vec<Modifier>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttributeOperator { Eq, Ne, Lt, Le, Gt, Ge, Exists, Missing }

/// Single string or comma-list value.  Size suffixes (3G, 500M, 1K) are
/// expanded to bytes during validation, before URL encoding.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AttributeValue {
    Single(String),
    List(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modifier {
    // summary (kept in query string as summary%28field%29)
    Min, Max, Median, Mean, Sum, List, Length,
    // status (converted to excludeXxx[N]=field URL params)
    Direct, Ancestral, Descendant, Estimated, Missing,
}
```

### `QueryParams` — the HOW

Represents how to fetch and present the results. Separate from `SearchQuery`
because the same query can be issued as count/search/report with different
pagination and formatting. Corresponds to `submit_query` parameters in the
mcp-server.

```rust
/// Execution parameters for submit_query / CLI search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryParams {
    /// Max records per page (maps to `&size=`; default 10).
    #[serde(default = "default_size")]
    pub size: usize,
    /// 1-based page number; offset = (page - 1) * size.
    #[serde(default = "default_page")]
    pub page: usize,
    /// Field to sort by.
    #[serde(default)]
    pub sort_by: Option<String>,
    /// Sort direction (default asc).
    #[serde(default)]
    pub sort_order: SortOrder,
    /// Include ancestrally estimated values (maps to `&includeEstimates=true`).
    /// Default true — matches API default and mcp-server behaviour.
    /// Exposed as `--include-estimates` CLI flag (gap-analysis item 5).
    #[serde(default = "default_true")]
    pub include_estimates: bool,
    /// Request tidy (long) format via `&summaryValues=false`
    /// (gap-analysis item 11 — prefer API native tidy over client-side pivot).
    #[serde(default)]
    pub tidy: bool,
    /// Taxonomy backbone (default "ncbi"; site-level default from SiteConfig).
    #[serde(default = "default_taxonomy")]
    pub taxonomy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder { #[default] Asc, Desc }
```

`build_query_url()` takes both `&SearchQuery` and `&QueryParams`.

A `SearchQuery` can be loaded from YAML:

```yaml
# example-query.yaml
index: taxon
taxa: [Mammalia, "!Felis"]
rank: species
taxon_filter_type: tree # was "children" in mcp-server; "tree" is the new canonical name
attributes:
  - name: genome_size # synonym accepted; normalised to canonical name
    operator: lt
    value: "3G" # size suffix; expanded to 3000000000 during validation
    modifier: [min, direct]
fields:
  - name: genome_size
    modifier: [min]
names: [scientific_name]
ranks: [genus]
```

---

## URL builder

```rust
/// Build a full API URL from a validated SearchQuery, QueryParams, and base URL.
pub fn build_query_url(
    query: &SearchQuery,
    params: &QueryParams,
    api_base: &str,
    endpoint: &str,   // "search", "count", "searchPaginated", "report"
) -> String
```

All strings remain **unencoded raw values** throughout the builder. A single
percent-encoding pass happens at the very end when serialising the complete
query string. No intermediate encoded strings; no `safe="%"` workarounds.
This eliminates the double-encoding risk entirely at the cost of one
well-defined encoding boundary.

Steps (mirroring the Python `build_query_string` + `params_dict_to_url`):

1. Build taxa fragment using raw strings: `tax_tree(A,!B)` / `tax_name(A)` /
   `tax_lineage(A)` depending on `taxon_filter_type`.
2. Append rank fragment: `tax_rank(species)`.
3. Append assembly/sample ID fragment: `assembly_id=ACC1,ACC2` /
   `sample_id=...`.
4. For each attribute with a summary modifier: wrap name as `summary(name)`.
   Append `name OP value` (raw), joined with `AND`.
5. Join all query fragments with `AND`; percent-encode the whole string once.
6. Build the outer param list (unencoded values): `result`, `includeEstimates`,
   `taxonomy`, `query` (the encoded string from step 5), `fields` (with
   modifier suffixes), `names`, `ranks`, `size`, `offset`, `sortBy`,
   `sortOrder`, `summaryValues`.
7. Derive `excludeXxx[N]=field` params from status modifiers; append as
   additional query params.
8. Percent-encode each param value and serialise to `?key=value&...`.

The function is pure (no I/O, no async) and deterministic.

---

## Validation strategy (first iteration)

First iteration uses only **static/build-time** data — enough to catch the most
common errors without live API calls:

1. **Attribute name** — look up in synonym → canonical table; if found,
   normalise to canonical name. Then look up in `FieldMeta` map. Error if
   neither lookup succeeds. Both tables are generated from `resultFields`.
2. **Operator** — check against `FieldMeta.processed_type` (no `<`/`>` for
   `keyword` types).
3. **Enum value** — check against `FieldMeta.constraint_enum` when present.
4. **Modifier** — check against `FieldMeta.summary` + fixed status set; check
   `traverse_direction` for `ancestral`/`descendant`.
5. **Assembly/sample prefix** — check against `site.yaml`
   `validation.valid_accession_prefixes` (not hard-coded).
6. **Taxon name class** — check against `site.yaml` `validation.valid_name_classes`.
7. **`taxon_filter_type`** — check against `site.yaml`
   `validation.valid_taxon_filter_types`.
8. **Search index** — check against generated index list from `SiteConfig.indexes`.

Items deferred to later iterations:

- **Rank validation** — requires `ranks.rs` generator addition; straightforward
  but out of scope for iteration 1.
- **Taxon name lookup** — `check_taxon_name(name) -> Result<TaxonInfo>` as an
  async SDK call using the existing `fetch::fetch_url` infrastructure; out of
  scope for iteration 1.
- **Numeric range constraints** — `FieldMeta.constraint_min/max`; low priority.

---

## Generator additions needed

### 1. `field_meta.rs.tera`

Renders `src/generated/field_meta.rs`:

```rust
pub struct FieldMeta {
    pub processed_type: &'static str,
    pub traverse_direction: Option<&'static str>,  // "up" | "down" | "both"
    pub summary: &'static [&'static str],
    pub constraint_enum: Option<&'static [&'static str]>,
}

/// Canonical name → metadata.
pub static TAXON_FIELD_META: phf::Map<&str, FieldMeta> = phf_map! { ... };
pub static ASSEMBLY_FIELD_META: phf::Map<&str, FieldMeta> = phf_map! { ... };
pub static SAMPLE_FIELD_META: phf::Map<&str, FieldMeta> = phf_map! { ... };

/// Synonym / alias → canonical name.  Emitted from the `synonyms` array
/// in each `resultFields` field object.  Validation resolves via this map
/// before looking up in `*_FIELD_META`.
pub static TAXON_FIELD_SYNONYMS: phf::Map<&str, &str> = phf_map! { ... };
pub static ASSEMBLY_FIELD_SYNONYMS: phf::Map<&str, &str> = phf_map! { ... };
pub static SAMPLE_FIELD_SYNONYMS: phf::Map<&str, &str> = phf_map! { ... };
```

The template iterates the same `fields` context variable already available to
`fields.rs.tera`. The `synonyms` array may be absent in some `resultFields`
entries; the template skips it gracefully.

### 2. `ranks.rs.tera`

Renders `src/generated/ranks.rs`:

```rust
pub static VALID_RANKS: &[&str] = &["species", "genus", "family", ...];
```

Requires a new fetch step in the generator: `GET /taxonomicRanks` → extract
`ranks` array → pass to Tera context as `valid_ranks`.

---

## Iteration plan

### Iteration 1 — URL builder + static validation (this iteration)

1. **`src/core/query/mod.rs`** — define `SearchQuery`, `QueryParams`, sub-structs,
   enums. All types `Serialize`/`Deserialize`.
2. **`src/core/query/url.rs`** — implement `build_query_url(query, params,
api_base, endpoint)` as a pure function. Encoding: raw strings throughout;
   single percent-encode pass at serialisation. Add `phf` + `phf_codegen` as
   deps from the start.
3. **`src/core/query/validation.rs`** — static validation functions that accept
   `&phf::Map<&str, FieldMeta>` and `&phf::Map<&str, &str>` (synonyms) borrows;
   no async. Synonym normalisation happens here before field lookup.
4. **`src/generated/field_meta.rs.tera`** + generator fetch logic — emit
   `FieldMeta` maps and synonym tables from `resultFields`.
5. **`site.yaml` additions** — add `validation:` block with
   `valid_accession_prefixes`, `valid_name_classes`, `valid_taxon_filter_types`.
   Defaults populated in the `goat.yaml` site file; propagated to generated
   validation via a new `ValidationConfig` struct in `config.rs`.
6. **Tests** — unit tests for URL builder (round-trip YAML → URL assertions);
   proptest invariants (no double-encoding, no empty AND fragments, synonym
   normalisation idempotent).
7. **CLI plumbing** — add `--query-file <path>` to the search/count commands
   in generated `main.rs.tera` (or hand-wired for first iteration); also
   `--taxon-type name|tree|lineage` aligning with gap-analysis item 1.
8. **SDK** — `#[pyfunction] fn build_url(query_yaml: &str, params_yaml: &str, api_base: &str, endpoint: &str) -> PyResult<String>`
   in `lib.rs`; `.pyi` stub entry.

Output of this iteration: the mcp-server's `build_query_string` +
`params_dict_to_url` can be replaced by a call to the Python SDK.

### Iteration 2 — Rank validation

1. Fetch `/taxonomicRanks` in generator; emit `src/generated/ranks.rs`.
2. Add rank check to `validation.rs`.
3. Add 24 h runtime rank cache in SDK for users who don't regenerate.

### Iteration 3 — Taxon name lookup (live API)

1. `src/core/query/taxon.rs` — async `check_taxon_name(name, api_base) ->
Result<TaxonInfo>` using existing `fetch::fetch_url`.
2. Expose as `#[pyfunction]` in `lib.rs`.
3. mcp-server can replace its `check_taxon_exists` tool with the SDK call.

### Iteration 4 — Report endpoint

The `get_report` tool in the mcp-server reuses `build_search_params` with
additional axis/visualisation parameters. Once the core URL builder is solid,
extending `SearchQuery` with an optional `ReportOptions` field and a
`build_report_url()` function is purely additive — no existing interfaces
change.

Report types to support (gap-analysis item 9, all via `/report` endpoint):
`histogram`, `scatter`, `tree` (Newick), `sources`, `arc`, `xPerRank`, `map`,
`table`. Each report type has an axis definition matching the mcp-server's
`process_axis` concept:

```rust
pub struct AxisDef {
    pub field: String,                // attribute/rank name
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub bin_count: Option<u32>,
    pub scale: Option<String>,        // "linear" | "log2" | "log10" | "sqrt"
}

pub struct ReportOptions {
    pub report_type: ReportType,      // histogram | scatter | tree | …
    pub x_axis: Option<AxisDef>,
    pub y_axis: Option<AxisDef>,
    pub category: Option<AxisDef>,
}
```

The report types the site supports are listed in `site.yaml` under
`reports:` so BoaT or similar can restrict the list.

---

## mcp-server migration path

Once iteration 1 is complete:

```python
# tools/helpers/query.py — replace build_query_string + params_dict_to_url
import cli_generator  # pip install cli-generator (or maturin develop)

def build_api_url(identifiers: dict, attributes: dict, endpoint: str) -> str:
    query_dict = {**identifiers, **attributes}
    query_yaml = yaml.dump(query_dict)
    return cli_generator.build_url(query_yaml, API_BASE, endpoint)
```

The rest of `query.py` (merge helpers, `build_user_facing_url`) can be retired
incrementally.

---

## Resolved decisions

1. **`phf` from the start.** `phf` + `phf_codegen` added as deps in iteration 1.
   Zero-cost at runtime; natural fit for static generated maps.

2. **Numeric size conversions** (e.g. `"3G"` → `3_000_000_000`): handled in
   `AttributeValue` parsing/validation, before URL encoding. Supported suffixes:
   G (×10⁹), M (×10⁶), K (×10³), B (×1). Same as mcp-server `convert_size_to_bytes`.

3. **No pre-encoding.** All strings stay raw throughout the builder pipeline.
   A single percent-encode pass happens at the final serialisation step (step 8
   of the URL builder). No `Encoded(String)` wrapper type needed. This is
   simpler than the Python `safe="%"` approach and eliminates the double-encoding
   class of bugs entirely.

---

## Gap analysis cross-check

Items from `goat-cli-gap-analysis.md` that interact with this plan:

| Gap item                                               | Interaction                                                   | Resolution                                                                                                                                                                                                 |
| ------------------------------------------------------ | ------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Item 1 — `--taxon-type name\|tree\|lineage`            | `TaxonFilterType` variant names should match                  | **Aligned**: variants renamed `Name`, `Tree`, `Lineage`; old mcp-server strings `matching`/`children`/`lineage` documented as aliases in serde                                                             |
| Item 3 — `--taxonomy` as ncbi constant                 | `QueryParams.taxonomy` defaults to `"ncbi"`                   | **Aligned**: site-level default in `SiteConfig`; not a user flag unless site differs                                                                                                                       |
| Item 4 — `--rank` vs `--ranks`                         | `Identifiers.rank` (filter) vs `AttributeSet.ranks` (columns) | **Aligned**: already distinct in the struct design                                                                                                                                                         |
| Item 5 — `--include-estimates`                         | Was hardcoded `true` in URL builder draft                     | **Fixed**: moved into `QueryParams.include_estimates` (default true); exposed as CLI flag                                                                                                                  |
| Item 6 — `--exclude` (excludeAncestral/excludeMissing) | Status modifiers on `Attribute` generate these params         | **Aligned**: modifier → exclusion param conversion is in the URL builder                                                                                                                                   |
| Item 7 — `--url` print mode                            | `build_query_url()` is a pure function                        | **Aligned**: CLI calls `build_query_url()` and prints without fetching                                                                                                                                     |
| Item 8 — count warning + `searchPaginated`             | `endpoint` param on `build_query_url()`                       | **Noted**: client (`client.rs.tera`) issues a count first, then chooses `search` vs `searchPaginated`; not a query-builder concern but the URL builder must accept `"searchPaginated"` as a valid endpoint |
| Item 9 — report endpoint expansion                     | `ReportOptions` / `AxisDef` added in iteration 4              | **Aligned**: all report types listed; reports section in `site.yaml`                                                                                                                                       |
| Item 11 — `--tidy` via `summaryValues=false`           | `QueryParams.tidy`                                            | **Added**                                                                                                                                                                                                  |
| Config gap 4 — `names` as `&names=`, not `&fields=`    | `AttributeSet.names` is a distinct list                       | **Aligned**: URL builder emits `&names=...` separately                                                                                                                                                     |
| `--variables` deprecation                              | No freeform field bypass needed                               | **Aligned**: synonym normalisation via `field_meta.rs` replaces the old static hardcoded DB                                                                                                                |
