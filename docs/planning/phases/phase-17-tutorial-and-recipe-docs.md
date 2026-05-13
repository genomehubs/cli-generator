# Phase 17: Tutorial and Recipe Documentation

**Status:** Design (revised)
**Depends on:** Phase 6g (Quarto docs), Phase XX (describe/snippet extensions)
**Blocks:** Public GoaT v3 user-facing launch
**Estimated scope:** 3–4 new template files, 1–2 new Rust functions, 1 new `sites/` YAML key, documentation in `docs/`
**See also:** Phase 18 (technical docs audit — planned once active report-type / phase-N development stabilises)

---

## 1. Audit of current documentation

### What exists

| Layer                                             | Files                                                                    | Coverage                                                                |
| ------------------------------------------------- | ------------------------------------------------------------------------ | ----------------------------------------------------------------------- |
| **GETTING_STARTED.md** (generator-level)          | `GETTING_STARTED.md`, `-python.md`, `-r.md`, `-javascript.md`, `-api.md` | Installation, first query, core operations. No multi-step workflows.    |
| **Quarto `index.qmd`** (generated, per-site)      | `templates/docs/index.qmd.tera`                                          | What you can do — method table + links. No narrative.                   |
| **Quarto `quickstart.qmd`** (generated, per-site) | `templates/docs/quickstart.qmd.tera`                                     | Count → search → DataFrame, all languages in tabsets. Good first-touch. |
| **Quarto `reference/query-builder.qmd`**          | `templates/docs/reference/query-builder.qmd.tera`                        | Method-by-method reference with code blocks. Reference, not tutorial.   |
| **Quarto `reference/parse.qmd`**                  | `templates/docs/reference/parse.qmd.tera`                                | Response structure, `parse_search_json`, `to_tidy_records`. Reference.  |
| **Quarto `reference/cli.qmd`**                    | `templates/docs/reference/cli.qmd.tera`                                  | CLI flag reference.                                                     |
| **`examples/`** (generator repo)                  | `QUERY-EXAMPLES.md`, `REPORT-TESTING.md`, `examples/report/*.json`       | Curl examples for dev testing. Not user-facing.                         |
| **`snippets` CLI command**                        | `templates/snippets/*.tera`                                              | Generates single-query code in all languages.                           |

### What is missing

1. **No recipes** — end-to-end worked examples with narrative context, e.g. "How do I find all mammals without a genome assembly?"
2. **No complex/advanced patterns** — combining multiple queries, joining with external data, iterating over clades.
3. **No site-specific examples** — quickstart uses `Mammalia`/`genome_size` generically; GoaT has richer fields (C-value, BioProject, ToLID).
4. **No `--report` integration in docs** — report type examples are absent from the Quarto docs entirely.
5. **No automation bridge** — the UI's `::report` directives and YAML blocks in the site config are never mined for examples.
6. **No URL → code path** — `parse_url_params` exists in Rust but is not exposed as a user-facing tool to turn a GoaT UI URL into SDK code.
7. **`_quarto.yml` navbar** — has `Home`, `Quick start`, `Reference` but no `Recipes` or `Tutorials` section.

---

## 2. Documentation architecture decision

### Recommendation: Recipes live inside the generated Quarto site

**Rationale:**

- Recipes are site-specific. A recipe for GoaT (C-value, BUSCO scores, ToLID prefixes) would be wrong or nonsensical for BoaT. Recipes belong in the per-site docs, generated from site-specific YAML inputs.
- Quarto panel-tabsets (`## Python`, `## R`, `## JavaScript`, `## CLI`) already provide the right multi-language scaffolding.
- Quarto `.qmd` files can contain executable cells (if rendered with a kernel), enabling live outputs. Even without a kernel, static code blocks are the standard for this style of docs.
- Centralising in Quarto means one `quarto render` produces HTML, PDF, and (optionally) Jupyter notebook exports.

**What lives where:**

| Content type                                           | Home                               | Generation                       |
| ------------------------------------------------------ | ---------------------------------- | -------------------------------- |
| Quick start (install + first query)                    | `docs/quickstart.qmd`              | Template (`quickstart.qmd.tera`) |
| SDK method reference                                   | `docs/reference/query-builder.qmd` | Template                         |
| Response parsing reference                             | `docs/reference/parse.qmd`         | Template                         |
| CLI flag reference                                     | `docs/reference/cli.qmd`           | Template                         |
| **Simple recipes** (single query, one concept)         | `docs/recipes/simple.qmd`          | Template + site YAML             |
| **Intermediate recipes** (multi-query, DataFrame ops)  | `docs/recipes/intermediate.qmd`    | Template + site YAML             |
| **Advanced recipes** (custom data join, batch, export) | `docs/recipes/advanced.qmd`        | Template + site YAML             |
| **Report gallery** (one section per report type)       | `docs/recipes/reports.qmd`         | Template + site YAML             |

The `GETTING_STARTED-*.md` files remain as quick GitHub-readable entry points but link into the Quarto site for depth.

---

## 3. Site YAML recipe input format

Site config files (`sites/goat.yaml`, `sites/boat.yaml`) gain a new optional `recipes:` key. The generator reads this and renders the recipe templates.

### Schema

```yaml
# sites/goat.yaml (additions)
recipes:
  # Simple recipes: demonstrate one concept each
  # These become sections in docs/recipes/simple.qmd
  simple:
    - title: "Find taxa missing genome size data"
      slug: missing_genome_size
      description: |
        Identify species-rank taxa in a clade that do not yet have a genome
        size estimate, to guide collection priorities.
      index: taxon
      taxa: ["Mammalia"]
      taxon_filter: tree
      rank: species
      filters:
        - [genome_size, missing, ""]
      fields: [scientific_name, genome_size]
      sort: [genome_size, asc]
      size: 20
      call_type: search # search | count | report

    - title: "Count assemblies per order in Lepidoptera"
      slug: lepidoptera_assembly_count
      description: |
        Count how many assemblies exist for each order in the butterfly and moth
        clade — useful for identifying under-represented families.
      index: assembly
      taxa: ["Lepidoptera"]
      taxon_filter: tree
      rank: order
      call_type: count

  # Intermediate recipes: multi-step or DataFrame operations
  intermediate:
    - title: "Compare genome sizes across vertebrate classes"
      slug: vertebrate_genome_size_comparison
      description: |
        Retrieve genome size data for multiple vertebrate classes in a single
        batch query, then combine into a single DataFrame for comparison.
      steps:
        - title: "Batch query"
          queries:
            - index: taxon
              taxa: ["Mammalia"]
              taxon_filter: tree
              rank: species
              fields: [genome_size, scientific_name]
            - index: taxon
              taxa: ["Aves"]
              taxon_filter: tree
              rank: species
              fields: [genome_size, scientific_name]
        - title: "Combine and plot"
          prose: |
            Concatenate the two DataFrames, add a `class` label column, then
            plot genome size distributions side by side.
          code:
            python: |
              import polars as pl
              df = pl.concat([df_mammalia.with_columns(pl.lit("Mammalia").alias("class")),
                              df_aves.with_columns(pl.lit("Aves").alias("class"))])
              # (plotting with matplotlib or plotly here)
            r: |
              library(dplyr)
              df <- bind_rows(
                mutate(df_mammalia, class = "Mammalia"),
                mutate(df_aves,     class = "Aves")
              )

    - title: "Annotate your own species list with GoaT data"
      slug: annotate_external_list
      description: |
        Start from a CSV of species names, look up each taxon in GoaT to
        retrieve genome size and chromosome count, then merge back.
      steps:
        - title: "Load your list"
          code:
            python: |
              import pandas as pd
              df = pd.read_csv("my_species_list.csv")   # column: scientific_name
            r: |
              df <- read.csv("my_species_list.csv")
        - title: "Query GoaT for each taxon"
          query:
            index: taxon
            taxon_filter: name
            fields: [genome_size, chromosome_number]
            call_type: search

  # Report gallery: one entry per report type, rendered into docs/recipes/reports.qmd
  reports:
    - title: "Genome size distribution in mammals"
      slug: mammalia_genome_size_histogram
      description: |
        A log10-scaled histogram showing the distribution of genome sizes
        across mammal species with direct measurements.
      report:
        report_type: histogram
        index: taxon
        taxa: ["Mammalia"]
        taxon_filter: tree
        rank: species
        x: genome_size
        x_opts: ";;20;log10"
        filters:
          - [genome_size, exists, ""]

    - title: "Genome size vs assembly span scatter"
      slug: genome_size_vs_assembly_span
      description: |
        Scatter plot of genome size against assembly span for Mammalia,
        revealing the relationship between cytological and sequenced genome
        estimates.
      report:
        report_type: scatter
        index: taxon
        taxa: ["Mammalia"]
        taxon_filter: tree
        x: genome_size
        y: assembly_span

    - title: "Canidae genome size tree"
      slug: canidae_tree
      description: |
        Phylogenetic tree of Canidae at genus rank, coloured by genome size.
      report:
        report_type: tree
        index: taxon
        taxa: ["Canidae"]
        taxon_filter: tree
        rank: genus
        y: genome_size
```

---

## 4. `--to-recipe` flag and the pipe pattern

### Design principles

`--to-recipe` is a **flag**, consistent with `--snippet`, `--describe`, and
`--to-url` on the `taxon`/`assembly`/`sample` subcommands. It is not a standalone
subcommand.

URLs cannot fully represent all query types — batch endpoints in particular have
no URL equivalent — so a second input mode is needed: reading a full JSON/YAML
query representation piped from another subcommand. This mirrors the existing
`plot` subcommand pattern, where `taxon report --include-plot-spec | plot` pipes
a Vega-Lite spec between subcommands.

### Mode 1: flag on a search/count/report subcommand

```bash
# Emit Python recipe from a CLI search invocation
goat-cli taxon search --taxon Mammalia --taxon-filter tree \
  --rank species --filter genome_size exists \
  --to-recipe python

# Emit all-language Quarto tabset
goat-cli taxon report --report-type histogram --taxon Mammalia \
  --x genome_size --to-recipe qmd

# Emit YAML suitable for copy-paste into sites/goat.yaml
goat-cli taxon search --taxon Mammalia --rank species \
  --to-recipe yaml
```

Output modes for `--to-recipe`:

| Value        | Output                                                               |
| ------------ | -------------------------------------------------------------------- |
| `python`     | Runnable Python SDK code block                                       |
| `r`          | Runnable R SDK code block                                            |
| `javascript` | Runnable JavaScript SDK code block                                   |
| `cli`        | Re-emit as CLI flags (round-trip)                                    |
| `qmd`        | Quarto panel-tabset Markdown block (all 4 languages)                 |
| `yaml`       | YAML recipe entry formatted for `sites/goat.yaml` `recipes.simple[]` |

The `yaml` output mode is new relative to `--snippet`. It produces a recipe
entry that can be pasted directly into `sites/goat.yaml` and will be rendered
into docs on the next `cli-generator new` run:

```yaml
- title: "<auto: describe() output>"
  slug: mammalia_genome_size_search
  description: |
    <user should fill in: describe() provides a one-liner as a starting point>
  index: taxon
  taxa: ["Mammalia"]
  taxon_filter: tree
  rank: species
  filters:
    - [genome_size, exists, ""]
  call_type: search
```

The `qmd` output is a Quarto-ready tabset block for pasting into a `.qmd` file:

````markdown
::: {.panel-tabset group="language"}

## Python

```python
from goat_sdk.query import QueryBuilder

qb = (
    QueryBuilder("taxon")
    .set_taxa(["Mammalia"], filter_type="tree")
    .set_rank("species")
    .add_attribute("genome_size", "exists")
)
results = qb.search()
```

## R

```r
...
```

## JavaScript

```javascript
...
```

## CLI

```bash
...
```

:::
````

### Mode 2: pipe from a full JSON/YAML query representation

For batch queries and any other workflow that cannot be expressed as a single
set of CLI flags, a `to-recipe` subcommand accepts a piped JSON/YAML query
representation on stdin:

```bash
# Pipe a batch search spec to to-recipe
goat-cli taxon search --batch batch-spec.yaml --emit-spec | \
  goat-cli to-recipe --language qmd
```

The `--emit-spec` flag (new, added alongside `--to-recipe`) causes the
subcommand to print a full machine-readable JSON/YAML representation of the
query rather than executing it. `to-recipe` reads stdin if no `--url` or
`--spec` flag is given. This makes both single-query and batch workflows
pipeable.

The pipe protocol mirrors `taxon report --include-plot-spec | plot`:

```
cli subcommand --emit-spec  →  JSON/YAML query spec on stdout
                                       ↓ (pipe)
              to-recipe [--language python|r|javascript|cli|qmd|yaml]
                                       ↓
                              recipe output on stdout
```

### Mode 3: URL input (limited)

For convenience, the `to-recipe` subcommand also accepts a `--url` flag. Because
URLs cannot represent batch queries or report parameters fully, this mode must
fail gracefully:

```bash
goat-cli to-recipe --url "https://goat.genomehubs.org/search?..." --language python
```

If the URL contains parameters that cannot be parsed or would produce a
silently wrong result, the command prints a clear error and exits non-zero
rather than producing incomplete output. See also §4.1 on `from_url`.

#### 4.1 `from_url` and the `--to-url` refresh

The current `--to-url` flag emits a deprecation warning and redirects to
`--to-v2-url`. Phase 9 added full v3 URL support, so this should be corrected:

- **`--to-url`** → emits a v3 URL (fix; remove deprecation warning)
- **`--to-v2-url`** → emits a v2 URL (kept for backward compatibility)

`from_url` (used internally by `to-recipe --url`) should be a general-purpose
function available across the whole SDK, not just for recipe conversion:

1. Default to v3 URL parsing
2. Fall back to v2 parsing if `"v2"` appears in the URL string
3. Return a clear structured error (not a panic or silent wrong result) for any
   parameters it cannot represent, naming the unrepresentable parameters explicitly
4. Batch specs cannot be represented as URLs at all — `--emit-spec | to-recipe`
   is the correct path for those

**Touch-points for `--to-url` fix:**

- `src/core/query/adapter.rs` — add `from_url(url: &str)` wrapping
  `parse_url_params` with percent-decoding + v2/v3 detection
- `python/cli_generator/query.py` — update `to_url()` docstring; ensure it
  calls the v3 URL builder; remove deprecation warning
- `python/cli_generator.pyi` — update stub
- `templates/python/query.py.tera` — same as `query.py`
- `src/generated/` CLI flags — `--to-url` wired to v3, `--to-v2-url` kept

### Implementation touch-points

**New Rust function** — add to `crates/genomehubs-query/src/url_to_snapshot.rs`
(or `adapter.rs`):

```rust
/// Parse a v3 (or v2) GoaT UI URL into a `QuerySnapshot`.
/// Falls back to v2 parsing if "v2" appears in the URL.
/// Returns a descriptive error for parameters that cannot be represented.
pub fn from_url(url: &str) -> anyhow::Result<QuerySnapshot>
```

**`--emit-spec` flag** — added to all `taxon`/`assembly`/`sample` search,
count, and report subcommands; prints the `QuerySnapshot` (+ `ReportSnapshot`
if present) as JSON to stdout instead of executing the query.

**`to-recipe` subcommand** — reads a `QuerySnapshot` JSON from stdin (or from
`--url` via `from_url`), calls `render_snippet` for the requested language(s).

**`--to-recipe` flag** — added alongside `--snippet` and `--describe` on all
search/count/report subcommands; internally calls `render_snippet` with the
additional `yaml` and `qmd` output modes.

**Snippet template changes** — add `yaml` and `qmd` output modes to
`SnippetGenerator::render`. The `qmd` mode wraps all four language snippets in
a single Quarto tabset string. The `yaml` mode serialises the `QuerySnapshot`
as a recipe YAML entry.

---

## 5. New Quarto doc pages (templates)

### 5.1 `templates/docs/recipes/simple.qmd.tera`

**Structure:**

````markdown
---
title: "Simple recipes"
---

Short, self-contained examples demonstrating one concept each.
These can be run in under a minute once the SDK is installed.

## {% for recipe in site.recipes.simple %}

## {{ recipe.title }}

{{ recipe.description }}

::: {.panel-tabset group="language"}

## Python

```python
{{ recipe | render_python_snippet }}
```
````

## R

```r
{{ recipe | render_r_snippet }}
```

## JavaScript

```javascript
{
  {
    recipe | render_js_snippet;
  }
}
```

## CLI

```bash
{{ recipe | render_cli_snippet }}
```

:::

{% endfor %}

````

The `render_*_snippet` Tera filters call the existing snippet infrastructure
(already used for the generated `snippet` CLI subcommand).

### 5.2 `templates/docs/recipes/intermediate.qmd.tera`

Multi-step recipes. Each step is either a query block (same tabset pattern as
simple) or a free-form code block (`step.code.python`, `step.code.r`). Steps
are rendered in sequence with their `title` and `prose` as section headers.

### 5.3 `templates/docs/recipes/reports.qmd.tera`

Report gallery. Each entry shows:
1. A description paragraph.
2. The SDK code to generate the report (from the report YAML → snippet).
3. The equivalent CLI command.
4. *(Optional)* The raw JSON report spec as a collapsed `<details>` block.

### 5.4 `_quarto.yml.tera` additions

Add a `Recipes` menu item:

```yaml
- text: Recipes
  menu:
    - href: recipes/simple.qmd
      text: Simple recipes
    - href: recipes/intermediate.qmd
      text: Intermediate recipes
    - href: recipes/advanced.qmd
      text: Advanced recipes
    - href: recipes/reports.qmd
      text: Report gallery
````

---

## 6. Mining site UI config for examples

The UI sites define reports in `::report` directives and YAML front matter.
These are the canonical "what GoaT actually displays" examples — the highest
quality source for recipes.

### Proposed pipeline

```
sites/goat.yaml (recipes: section)
      │
      ▼
cli-generator template engine
      │                         ─── existing snippet.rs ──────────────────┐
      ▼                                                                    ▼
docs/recipes/*.qmd.tera ─────► docs/recipes/simple.qmd    (4-language tabsets)
                                docs/recipes/reports.qmd   (report gallery)
```

The `recipes:` key in `sites/goat.yaml` is **hand-authored** — it contains the
curated, narrative descriptions and the exact query parameters. The key insight
is that the code generation is fully automated (no hand-written SDK code in the
recipes), so maintaining a recipe means only editing the YAML, not touching
multiple language-specific files.

For the report gallery specifically, the YAML recipe → `ReportSnapshot` →
`render_snippet()` path already has the infrastructure (once phase XX is
complete). The new work is only:

1. Adding `recipes:` schema to `sites/*.yaml`
2. Writing the three `.qmd.tera` recipe templates
3. Registering the new pages in `_quarto.yml.tera`

---

## 7. Automation level analysis

| Content                                   | Automatable?                          | Approach                                        |
| ----------------------------------------- | ------------------------------------- | ----------------------------------------------- |
| Code blocks in recipes                    | **Fully automated**                   | `render_snippet()` from query/report YAML       |
| Recipe titles and descriptions            | **Manual** (1–3 sentences per recipe) | Written in `sites/goat.yaml`                    |
| Multi-step narrative prose                | **Mostly manual**                     | Written in `sites/goat.yaml` step `prose:` keys |
| Custom code steps (DataFrame merge, plot) | **Manual**                            | Written as literal code in `sites/goat.yaml`    |
| Report gallery entries                    | **Fully automated**                   | From `examples/report/*.json` + YAML recipes    |
| `--to-recipe` URL → snippet               | **Fully automated**                   | `url_to_snapshot` + `render_snippet`            |
| `--language all` tabset output            | **Fully automated**                   | Wrap all 4 snippets in Quarto tabset string     |

**The practical split:** Write the query parameters and a one-paragraph
description in YAML. Everything else (multi-language code, Quarto tabsets,
cross-linking) is generated.

---

## 8. Recipe taxonomy (initial content for GoaT)

### Tier 1 — Simple (single concept, <10 lines of code)

| Slug                | Concept demonstrated                      |
| ------------------- | ----------------------------------------- |
| `count_by_rank`     | Count records, iterate over ranks         |
| `missing_data`      | `missing` filter operator                 |
| `exclude_taxon`     | `!Rodentia` exclusion syntax              |
| `large_genomes`     | Numeric threshold filter (`ge`)           |
| `by_assembly_level` | `assembly_level` keyword field            |
| `chromosome_count`  | Integer field + sorting                   |
| `tolid_lookup`      | `tolid_prefix` taxon name class           |
| `busco_score`       | Nested field access, `busco_completeness` |

### Tier 2 — Intermediate (multi-step or DataFrame)

| Slug                     | Concept demonstrated                  |
| ------------------------ | ------------------------------------- |
| `vertebrate_comparison`  | Batch query + DataFrame concat        |
| `annotate_external_list` | User CSV + GoaT lookup join           |
| `source_annotation`      | `annotate_source_labels()` + split    |
| `count_before_search`    | Pagination planning with `count()`    |
| `multi_field_export`     | Multiple fields + `to_tidy_records()` |

### Tier 3 — Advanced (SDK composition, external data)

| Slug                       | Concept demonstrated                      |
| -------------------------- | ----------------------------------------- |
| `clade_completeness_sweep` | Loop over a list of clades, count each    |
| `genome_size_choropleth`   | Merge with geography data                 |
| `export_to_bioinformatics` | Write FASTA headers from assembly records |
| `custom_report_extension`  | Add a column not in GoaT via NCBI lookup  |

### Report gallery (one per report type)

| Slug                 | Report type |
| -------------------- | ----------- |
| `mammalia_histogram` | `histogram` |
| `genome_scatter`     | `scatter`   |
| `canidae_tree`       | `tree`      |
| `sources_summary`    | `sources`   |
| `rank_counts`        | `xPerRank`  |
| `clade_map`          | `map`       |
| `arc_diagram`        | `arc`       |

---

## 9. File touch-points summary

| File                                              | Change                                                                          |
| ------------------------------------------------- | ------------------------------------------------------------------------------- |
| `sites/goat.yaml`                                 | Add `recipes:` block                                                            |
| `sites/boat.yaml`                                 | Add `recipes:` block (BoaT-specific fields)                                     |
| `templates/docs/_quarto.yml.tera`                 | Add `Recipes` navbar menu; add `ipynb` format for recipe pages                  |
| `templates/docs/recipes/simple.qmd.tera`          | **New**                                                                         |
| `templates/docs/recipes/intermediate.qmd.tera`    | **New**                                                                         |
| `templates/docs/recipes/advanced/{slug}.qmd.tera` | **New** (one template, one output file per advanced recipe slug)                |
| `templates/docs/recipes/advanced.qmd.tera`        | **New** (index page linking to per-recipe files)                                |
| `templates/docs/recipes/reports.qmd.tera`         | **New**                                                                         |
| `crates/genomehubs-query/src/url_to_snapshot.rs`  | **New** — `from_url(url)` with v3/v2 detection                                  |
| `src/core/snippet.rs`                             | Add `qmd` and `yaml` output modes to `SnippetGenerator::render`                 |
| `src/lib.rs`                                      | Expose `from_url` via PyO3                                                      |
| `src/generated/` CLI flags                        | `--to-recipe` flag; `--emit-spec` flag; `--to-url` fix (v3); `--to-v2-url` kept |
| `src/generated/cli_meta.rs`                       | Add `to-recipe` subcommand metadata                                             |
| `src/commands/new.rs`                             | Register new templates + `to-recipe` / `--emit-spec` wiring                     |
| `python/cli_generator/query.py`                   | Update `to_url()` to call v3 builder; remove deprecation warning                |
| `python/cli_generator.pyi`                        | Update stub                                                                     |
| `templates/python/query.py.tera`                  | Same as `query.py`                                                              |
| `templates/snippets/python_snippet.tera`          | Add `call_type=count/report` branches (phase XX); add `qmd`/`yaml` wrappers     |
| `templates/snippets/r_snippet.tera`               | Same                                                                            |
| `templates/snippets/js_snippet.tera`              | Same                                                                            |
| `templates/snippets/cli_snippet.tera`             | Same                                                                            |

---

## 10. Dependency on phase XX

Phase XX (describe/snippet extensions) adds `call_type` and `ReportSnapshot` to
`QuerySnapshot`. The recipe templates for `call_type: count` and `call_type: report`
entries depend on that. The simple `call_type: search` recipes and all `--to-recipe`
URL conversion work is **independent** of phase XX and can ship first.

**Suggested sequencing:**

```
--to-url fix (§4.1)               ← independent, fix first
      │
Phase XX (snippet call_type, ReportSnapshot)
      │
      └──► Phase 17-A: --to-recipe flag + --emit-spec + from_url
           Simple + Intermediate recipes (search/count call types)
           qmd + yaml output modes
           ipynb export enabled in _quarto.yml
                 │
                 └──► Phase 17-B: Report gallery (requires ReportSnapshot)
                            │
                            └──► Phase 18: Technical docs audit
                                 (once report types + phase-N plans stable)
```

---

## 11. Decisions

1. **Live vs static docs** — Default: **static code blocks** with `# Uncomment to run`
   comments. A `--live-docs` flag passed to the doc-build script enables executable
   cells for GoaT maintainers (requires a running API). This keeps CI doc builds
   fast and reproducible.

2. **Advanced recipe file structure** — **One `.qmd` file per advanced recipe.**
   `docs/recipes/advanced.qmd` is a generated index page listing all advanced
   recipes with a one-line description and link. Each recipe gets its own file
   at `docs/recipes/advanced/{slug}.qmd` rendered from a single
   `advanced/{slug}.qmd.tera` template instantiated per slug.

3. **URL parsing scope** — `from_url` should be a general-purpose function across
   the whole SDK, not just for `--to-recipe`. It should default to v3 URL format,
   fall back to v2 if `"v2"` appears in the URL string, and return a structured
   error for any parameters it cannot represent (e.g. batch specs). The clear
   error message should name the missing parameters explicitly. Batch queries
   cannot be represented as URLs at all — use `--emit-spec | to-recipe` instead.
   URL input remains a convenience feature, not the primary path.

4. **Notebook export** — Enable `.ipynb` export in `_quarto.yml.tera` for recipe
   pages (low implementation cost). Add `format: ipynb: default` to the recipe
   page YAML front matter so users can download a pre-populated notebook from
   the docs site.

---

## 12. Future: Phase 18 — Technical docs audit

A full technical docs audit is deferred until active development on additional
report types, phase-N plans, and the API has stabilised. Phase 18 will:

- Audit all generated Quarto pages for accuracy and completeness
- Cross-reference every public SDK method against its documentation
- Identify missing reference pages (e.g. no `describe()` or `validate()` reference)
- Review `GETTING_STARTED-*.md` files for freshness
- Identify gaps between what the CLI `--help` text says and what the reference
  docs say
- Plan any structural reorganisation of the generated doc site

This phase is explicitly **not** scoped here and will be filed as its own
planning document once the trigger condition (end of active phase-N development)
is met.
