# Post-MVP roadmap

Written: 2026-03-18

Captures the features deferred from the MVP release, their rationale,
implementation notes, and a suggested priority order. Intended as a living
reference for sprint planning after the MVP preview has collected user
feedback.

---

## Priority order and rationale

| #   | Feature                                                 | Priority | Effort | Why                                                                                     |
| --- | ------------------------------------------------------- | -------- | ------ | --------------------------------------------------------------------------------------- |
| 1   | `--file` batch input via `/msearch`                     | P0       | M      | Most-requested missing feature; far more efficient than the old per-row approach        |
| 2   | Auto-pagination via `searchPaginated`                   | P0       | M      | Essential for any query returning > default page size; `--size` semantics need redesign |
| 3   | Names / identifiers (`&names=` param)                   | P1       | S      | Currently broken — names fields are silently passed as `&fields=` which the API ignores |
| 4   | `--filter` / `--query` semantics and renaming           | P1       | S      | Multi-value AND join, `--tax-rank` sugar, SDK coherence; rename `--query` → `--filter`  |
| 5   | `--exclude` (excludeAncestral / excludeMissing)         | P1       | S      | Query builder already supports this; mostly wiring                                      |
| 6   | YAML query input (`--query-file`) + full-query `--file` | P2       | S      | Reproducibility, msearch with arbitrary queries, YAML/jsonlines structured input        |
| 7   | Top-level OR query chains                               | P2       | M      | Powerful multi-taxon/condition queries; contained to URL building and validation        |
| 8   | Report subcommands (sources, newick, hist, …)           | P2       | L      | Major feature; important for ecosystem but separable from search work                   |
| 9   | Async client + progress bar                             | P3       | L      | Nice-to-have; blocks on tokio adoption decision                                         |
| 10  | `--raw` flag                                            | P3       | XS     | Low demand; trivial to add                                                              |

`--ranks` (ancestor rank columns), `--tax-rank`, `--tidy`, and `--goat-ui-url`
are not on this list because they have no blocking dependencies and can be
wired in at any point. They are noted at the end.

---

## 1. `--file` batch input via `/msearch` _(P0, effort M)_

### Why it is P0

Batch input was present in the old CLI and is the primary use case for
comparative genomics workflows — "give me genome-size data for all species in
this list". The old CLI issued one HTTP request per row, which is slow and
breaks at scale. The newer `/msearch` API endpoint accepts multiple queries in
a single POST, making this significantly more efficient.

### Semantics

```bash
goat-cli taxon search --file taxa.txt --field-groups genome-size --size 10
```

- Each line in the file becomes one query (wraps in `tax_name(X)` by default,
  respects `--taxon-filter tree|lineage`).
- All queries are sent as a single `/msearch` POST payload.
- Size limit per-query vs total needs defining: old CLI capped file at 500;
  revisit in light of `/msearch` capacity (test empirically).
- Output: merged TSV with deduplicated headers; one row per hit per query.

### Implementation sketch

1. Read file into `Vec<String>`, trimming blank lines and `#` comments.
2. Build each as `tax_name(X)` (or tree/lineage variant).
3. `POST /api/v2/msearch` — request body is newline-delimited JSON or YAML
   depending on API spec (verify against GoaT `/msearch` docs).
4. Stream/merge response TSVs — headers appear once; data rows are
   concatenated. Consider outputting a `query` column to identify which input
   row each result came from.
5. Wire into `generated::client` as `pub fn msearch(...)`.

### Design questions

- Should there be a `--query-column` flag to name the provenance column?
- What is the right behaviour when one query returns zero results — skip
  silently, emit a header-only row, or warn on stderr?
- Should `--file` also accept full query strings (not just taxon names/IDs)?
  See §6 below for the full-query file input design.

---

## 2. Auto-pagination via `searchPaginated` _(P0, effort M)_

### Why it is P0

The current client truncates at `--size`. For large datasets the user must
either know the count and set `--size` appropriately, or get silently
incomplete results. Auto-pagination removes this footgun.

### The `--size` semantics problem

The old CLI treated `--size` as "I want exactly N results" with a large
default implying "give me everything". This conflates two different
intentions:

- **"I want a sample"** — user sets `--size 50` deliberately.
- **"I want everything"** — user omits `--size` or sets it very high.

Proposal: split into two flags:

| Flag         | Meaning                                                                                                      |
| ------------ | ------------------------------------------------------------------------------------------------------------ |
| `--size <n>` | Return at most N results. If the result set is larger, paginate automatically up to N. Default remains `50`. |
| `--all`      | Retrieve all results, however many there are, using `searchPaginated`.                                       |

`--all` triggers the `searchPaginated` endpoint using `search_after` for
cursor-based pagination with no upper bound. This avoids the risk of
accidentally paginating a query that returns millions of rows when the user
just wanted a sample.

A lightweight count-before-fetch warning remains useful: if the server count
exceeds `--size` and `--all` is not set, print a stderr warning and the count
so the user can add `--all` knowingly.

### Implementation sketch

1. Add `--all` bool flag to `Search` subcommands in `main.rs.tera`.
2. Add `pub fn search_paginated(...)` to `client.rs.tera` — loops on
   `search_after` tokens until exhausted or `--size` cap hit.
3. Count-warning: run a lightweight `count` before `search` when `--all` is
   not set and print to stderr if `count > size`.
4. Streaming output: print TSV header once, emit rows as each page arrives
   (important for large result sets — avoid buffering everything in memory).

### Async dependency

Full streaming output and progress bars require async reqwest. This work can
be done synchronously first (page-at-a-time, buffer each page, print) and
upgraded to async in item 7. The interface is identical from the user's
perspective.

---

## 3. Names and identifiers (`&names=` param) _(P1, effort S)_

### Why it is P1

This is the only item on the list that is **currently broken rather than
missing**. The `names` field group in `goat-cli-options.yaml` lists
`common_name`, `synonym`, and `tolid_prefix` as ordinary `fields:` entries.
The generator therefore emits them into `&fields=common_name,synonym,...`.
The GoaT API ignores these values there — names are a separate metadata
category controlled by a dedicated `&names=` query parameter:

```
# What is currently generated (wrong)
&fields=genome_size,common_name,synonym,tolid_prefix

# What the API actually requires
&fields=genome_size&names=common_name,synonym,tolid_prefix
```

The result is that `--field-groups names` silently returns no name columns.

### API behaviour

The `&names=` parameter accepts a comma-separated list of name classes:

| Value          | Returns                        |
| -------------- | ------------------------------ |
| `common_name`  | Vernacular names               |
| `synonym`      | Taxonomic synonyms             |
| `tolid_prefix` | Tree of Life ID (ToLID) prefix |

The assembly index has a parallel `&identifiers=` parameter for accession
aliases and cross-references.

### What needs to change

This requires a small generator extension — a new `extra_params:` field type
in `FieldGroup` (or a dedicated `names:` section in `cli-options.yaml`) so
the generator knows to route certain groups to `&names=` rather than
`&fields=`. The generated `client::search_url` then needs a `names: &[&str]`
argument alongside `fields`.

#### Option A — `extra_params:` on FieldGroup (general)

Add an optional `extra_params:` key to a field group in `cli-options.yaml`:

```yaml
- flag: names
  short: n
  description: "Include common name and synonym fields"
  extra_params:
    names: [common_name, synonym, tolid_prefix]
```

The generator collects all active `extra_params` across enabled groups and
appends them as `&key=val1,val2` to the URL. General enough to handle any
future param that works this way.

#### Option B — hard-coded `names_param:` field (specific)

Add a boolean `names_param: true` flag to the group config; the generator
knows to route its `fields:` list to `&names=` instead. Simpler but only
solves this one case.

**Recommendation:** Option A. The assembly `&identifiers=` case is
immediately in scope and a general mechanism costs almost no extra work.

### Implementation sketch

1. Add `extra_params: Option<HashMap<String, Vec<String>>>` to `FieldGroup`
   in `config.rs`.
2. Surface it on `TemplateFlag` in `codegen.rs`.
3. In `cli_flags.rs.tera`, add `all_extra_params()` alongside `all_fields()`
   — returns a `HashMap<String, Vec<String>>` of merged extra params from all
   active groups.
4. Update `client::search_url` (and `client::search`) in `client.rs.tera` to
   accept an `extra_params: &HashMap<String, Vec<String>>` argument and
   append `&key=val1%2Cval2` for each entry.
5. Update `main.rs.tera` dispatch to collect `flags.all_extra_params()` and
   pass it through.
6. Update `sites/goat-cli-options.yaml`: remove `common_name`, `synonym`,
   `tolid_prefix` from the `names` group's `fields:` list and add them under
   `extra_params: {names: [...]}` instead.

### Also in scope: assembly `--identifiers`

The assembly index has accession aliases that map to `&identifiers=`. Apply
the same `extra_params` treatment to a new `identifiers` field group in the
assembly section of `goat-cli-options.yaml`.

---

## 4. `--filter` / `--query` semantics and renaming _(P1, effort S)_

### Current state

The MVP exposes `--query` as a single free-text string that is ANDed with the
taxon clause. `--tax-rank` from the old CLI is not yet implemented — users
must write `tax_rank(species)` directly in `--query`.

### Rename: `--query` → `--filter`

`--filter` is more precise ("filter results where…") and avoids confusion
with the concept of a full API query string. Retain `--query` as a hidden
alias during the transition period.

### Multi-value `--filter` (AND join)

Allow `--filter` to be repeated; multiple values are joined with `AND`
before being combined with the taxon clause:

```bash
# Equivalent to: tax_tree(Mammalia) AND assembly_span > 300000000 AND assembly_level=chromosome
goat-cli taxon search \
  --taxon Mammalia --taxon-filter tree \
  --filter "assembly_span > 300000000" \
  --filter "assembly_level=chromosome"
```

The user never writes `AND` themselves. The CLI constructs the full query
string before handing it to the client.

### `--tax-rank` sugar

The old `--tax-rank <rank>` flag wrapped the search in `tax_rank(rank)`. This
is purely convenience — it adds `tax_rank(X)` to the AND chain alongside
`--taxon` and `--filter`:

```bash
# Equivalent to: tax_tree(Chordata) AND tax_rank(species)
goat-cli taxon search --taxon Chordata --taxon-filter tree --tax-rank species
```

Implementation: add `--tax-rank` to the `Search` subcommand in `main.rs.tera`
as `Option<String>`; when present, push `format!("tax_rank({v})")` into the
AND-join list before constructing `full_query`.

### Query construction order

The final API `query=` string is assembled in this order, each part present
only when non-empty, joined with `AND`:

1. Taxon clause from `--taxon` + `--taxon-filter`
2. `tax_rank(X)` from `--tax-rank`
3. Each `--filter` value in order given

### Implementation sketch

1. Change `--query: String` → `--filter: Vec<String>` (repeatable) in
   `main.rs.tera`; add `alias = "query"`.
2. Add `--tax-rank: Option<String>` to `Search`.
3. Replace current two-way `match (taxon_clause, query.is_empty())` with a
   `Vec<String>` clause list that is `.join(" AND ")`.

---

## 5. `--exclude` _(P1, effort S)_

### Why P1

`--exclude` was a frequently-used quality-control flag in the old CLI — it
filters out rows where all requested fields are inferred or absent, making
output directly usable for analyses that require direct measurements.

### API mapping

The API accepts `excludeAncestral[0]=field&excludeMissing[0]=field` per
field. When `--exclude` is set, emit these params for every field in the
active field list.

The query builder (`src/core/query/`) already models this. Wiring it up is
mostly passing the flag value through `client::search_url`.

### Implementation sketch

1. Add `--exclude` bool to `Search` subcommand in `main.rs.tera`.
2. Add `exclude: bool` param to `client::search_url` and `client::search`.
3. When `exclude` is `true`, append
   `&excludeAncestral[{i}]={field}&excludeMissing[{i}]={field}` for each
   field index `i` in the active field list.

---

## 6. YAML query input and full-query `--file` _(P2, effort S)_

### Why P2

These two features extend the batch input story (§1) from "a list of taxon
names" to "a list of arbitrary queries". They are natural complements:
`--file` handles the interactive pipeline use case; `--query-file` handles
the reproducibility/sharing use case.

### `--query-file` — reproducible YAML queries

A YAML file describing a complete query, runnable without any flags:

```yaml
# query.yaml
taxon: Mammalia
taxon_filter: tree
tax_rank: species
field_groups:
  - genome-size
  - assembly
filter:
  - "assembly_span > 300000000"
  - "assembly_level=chromosome"
size: 500
all: true
```

```bash
goat-cli taxon search --query-file query.yaml
# CLI flags override file values:
goat-cli taxon search --query-file query.yaml --tax-rank genus
```

Mirrors the Python SDK's `QueryBuilder` YAML idiom. Parse with `serde_yaml`
(already a transitive dep); merge with explicit CLI flags (flags win).
Contained entirely in `main.rs` — no generator changes.

### Full-query mode for `--file`

When the file lines are full GoaT query strings rather than bare taxon names,
each line is sent as-is to `/msearch` without wrapping in `tax_name(X)`.
A header comment or a `--file-type queries` flag distinguishes the two modes:

```
# goat-query-list
tax_tree(Mammalia) AND assembly_level=chromosome
tax_tree(Reptilia) AND assembly_span > 1000000000
```

### Structured input: YAML arrays and JSON Lines

Full queries can also be expressed as structured input for programmatic
generation:

```yaml
# queries.yaml — array of query objects
- taxon: Mammalia
  taxon_filter: tree
  filter: assembly_level=chromosome
- taxon: Reptilia
  taxon_filter: tree
  filter: "assembly_span > 1000000000"
```

or JSON Lines (one JSON object per line), convenient for pipeline generation:

```jsonl
{"taxon": "Mammalia", "taxon_filter": "tree", "filter": "assembly_level=chromosome"}
{"taxon": "Reptilia", "taxon_filter": "tree", "filter": "assembly_span > 1000000000"}
```

Each object is expanded using the same query-construction logic as the CLI
flags (AND-join of taxon clause + tax_rank + filter list) before being passed
to `/msearch`. Field groups and fields are shared across all queries in the
batch (specified via the normal CLI flags or the `--query-file` defaults).

### Design question

Should structured input be a separate `--batch-file` flag (distinct from the
taxon-name `--file`) or detected automatically from file content/extension?
Automatic detection is more ergonomic but less explicit.

---

## 7. Top-level OR query chains _(P2, effort M)_

### What this is

GoaT's query language supports OR at the top level to chain full query
clauses:

```
tax_name(canis lupus) AND assembly_level=contig OR tax_name(felis sylvestris) AND assembly_level=scaffold
```

This is distinct from within-field OR (comma-separated values, e.g.
`assembly_level=contig,scaffold`), which already works via `--filter`.

### Use cases

- Multi-taxon queries where each taxon has different filter conditions
- Queries that cannot be expressed as a single AND chain
- Power users who need full API query language access

### Why it is P2 and not lower

With `--filter` supporting multiple values and `--file` supporting full query
strings, most real use cases can already be handled. Top-level OR is needed
when different filter conditions apply to different taxa in a single request.

### Design

OR is contained to the API `query=` parameter — field groups, exclusions,
size, etc. are shared across all OR branches. This limits the blast radius
considerably.

Proposed interface: `--or-filter` flag that starts a new OR branch. Everything
between two `--or-filter` flags (or between `--or-filter` and end of args) is
ANDed together as normal:

```bash
# Produces: tax_name(canis lupus) AND assembly_level=contig OR tax_name(felis sylvestris) AND assembly_level=scaffold
goat-cli taxon search \
  --taxon "canis lupus" --filter assembly_level=contig \
  --or-filter \
  --taxon "felis sylvestris" --filter assembly_level=scaffold \
  --field-groups assembly
```

Alternatively, a `--query-or` flag that accepts a complete pre-built OR
branch string for users who are comfortable writing query syntax directly:

```bash
goat-cli taxon search \
  --query-or "tax_name(canis lupus) AND assembly_level=contig" \
  --query-or "tax_name(felis sylvestris) AND assembly_level=scaffold" \
  --field-groups assembly
```

The `--query-or` form is simpler to implement and less surprising. It
explicitly requires the user to understand the query syntax for OR use cases,
keeping the common AND case simple.

### Implications for URL building

The query builder in `src/core/query/url.rs` likely needs OR support. Worth
checking whether the existing `SearchQuery` / `QueryParams` structures already
model this before designing the client-side OR representation.

---

## 8. Report subcommands _(P2, effort L)_

### Why P2

`sources`, `newick`, and `hist` (histogram) were used regularly in the old
CLI. `newick` in particular is essential for phylogenetic visualisations.

### Proposed subcommand structure

```
goat-cli taxon report sources   --taxon Mammalia --taxon-filter tree
goat-cli taxon report newick    --taxon Mammalia --taxon-filter tree --rank species
goat-cli taxon report histogram --taxon Mammalia --taxon-filter tree --field genome-size
goat-cli taxon report scatter   --taxon Mammalia --taxon-filter tree --x genome-size --y assembly_span
```

All share the same `/report` API endpoint with a `report=` query param.

### Generator support

Add a `reports:` section to `site.yaml` listing which report types the
instance supports. The generator emits a `Report` subcommand with type-safe
`ReportType` enum variants. Hand-written `main.rs` dispatches to
`generated::client::report(...)`.

This is the largest item on the list — it touches `site.yaml` schema,
`codegen.rs`, a new `client.rs.tera` section, and a new `main.rs.tera`
dispatch block. Worth a dedicated sprint.

---

## 9. Async client + progress bar _(P3, effort L)_

### Why P3

The progress bar is "nice to have" — the current sync client is fully
functional for the use cases supported post-MVP. Async becomes more
compelling once pagination is in place (streaming pages as they arrive is
much better UX than blocking until all pages are fetched).

### Approach

Add an optional `async` Cargo feature. When enabled, swap `reqwest::blocking`
for `reqwest` (async) and add a `tokio::main` runtime in `main.rs`. The
generated `client.rs` gets both sync and async variants, selected by feature
flag. Progress bar uses `indicatif`.

The feature flag approach avoids a hard tokio dependency for users who embed
the generated crate in contexts where async is not wanted.

---

## 10. `--raw` _(P3, effort XS)_

### Why P3

Niche use case; the API's raw response format is undocumented and changes
schema in hard-to-predict ways. Most users who needed it were using it to
bypass summary aggregation — `field:direct` in `--filter` covers the common
case.

### Implementation

Pass `&summaryValues=raw` (or whatever the current API param is) when `--raw`
is set. Single line addition to `client::search_url`.

---

## Smaller items (no blocking dependencies, wire in at any point)

| Feature                                            | Notes                                                                      |
| -------------------------------------------------- | -------------------------------------------------------------------------- |
| `--tax-rank <rank>`                                | Adds `tax_rank(X)` to the AND chain; sugar for a common `--filter` pattern |
| `--ranks`                                          | Adds ancestor rank columns; API param `&ranks=species,genus,...`           |
| `--tidy`                                           | Pass `&summaryValues=false` (or equivalent); emits tidy-format TSV         |
| `--goat-ui-url`                                    | Construct UI URL from API URL at generation time; print and exit           |
| Short-form flags for `--field-groups` / `--expand` | Defer until full flag set is stable post-MVP                               |
| Runtime `--fields` glob patterns                   | See appendix below                                                         |
| `fields list` subcommand                           | Queries live `resultFields` endpoint; replaces `--print-expression`        |

---

## Appendix — `--fields` glob patterns

Low priority but noted here for completeness. See the analysis in the
session discussion (2026-03-18): entirely self-contained in
`cli_flags.rs.tera`, no changes to `codegen.rs`. Add a `fn
{{ index.name }}_all_field_names() -> &'static [&'static str]` constant (already
available via `field_meta.rs`) and a runtime `matches_pattern_rt` function
mirroring the compile-time version. Branch in `all_fields()` when an entry
contains `*`. Estimated ~30 lines.
