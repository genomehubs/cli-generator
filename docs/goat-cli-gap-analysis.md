# GoaT CLI gap analysis

Comparison of old `goat-cli` (Rust async, hardcoded fields) against the
generator-produced `goat-cli` (sync, config-driven fields).

Conducted: 2026-03-16

---

## ✅ Already covered

| Feature                              | Notes                                                         |
| ------------------------------------ | ------------------------------------------------------------- |
| `taxon search / count / lookup`      | Generated subcommands match                                   |
| `assembly search / count / lookup`   | Generated subcommands match                                   |
| All field-group flags (both indexes) | Covered via `goat-cli-options.yaml` display_groups / patterns |
| Default TSV output + JSON / CSV      | `--format` flag on generated CLI                              |
| `--size`                             | Present                                                       |
| URL encoding of query params         | `pct_encode()` handles all special chars                      |

---

## 🔧 Config gaps

Quick YAML fixes in `sites/goat-cli-options.yaml` — no generator changes needed.

1. **`taxon --bioproject` is missing `biosample`** — old CLI returned both.
   Old mapping: `["bioproject", "biosample"]`.

2. **Assembly index is missing an `--assembly` flag** — the taxon index has one
   (span + level) but the assembly `field_groups:` list does not.

3. **Assembly index is missing a `--date` flag** — old assembly CLI had a date
   flag just like the taxon index.

4. **`taxon --names` and assembly `--identifiers` use non-field API params** —
   `names` is passed as `&names=synonym%2Ctol_id%2Ccommon_name`, not as a
   `&fields=` entry. Assembly similarly has identifier-type metadata that
   does not map to the fields system. Both are distinct "metadata categories"
   rather than result fields and need first-class support in the client template
   as a new param type (e.g. `extra_params` alongside `fields`). This is a
   generator change, not just a YAML fix.

---

## 🚧 Implement

Roughly priority-ordered.

### Core query ergonomics

1. **`--taxon` / `-t` and a `--taxon-type` selector**
   Old CLI used `-t/--taxon` as the primary search entry point. Rather than
   carrying two separate flags (`--descendants`, `--lineage`), introduce a
   `--taxon-type` option (default `name`) that controls the query wrapper:
   - `name` → `tax_name(X)` (default, point lookup)
   - `tree` → `tax_tree(X)` (all descendants)
   - `lineage` → `tax_lineage(X)` (all ancestors)

   This drops `--descendants` and `--lineage` in favour of a single,
   composable option. Best implemented as hand-written args in `main.rs`
   that build the query string before forwarding to the generated search
   function.

2. **`--file` / `-f` batch input — use `msearch`**
   Read a file of taxon IDs/names and query them all at once. The API now
   supports `/msearch` which accepts multiple queries in a single request,
   making this far more efficient than one HTTP call per row (the old
   approach). Implementation plan:
   - Read file into a `Vec<String>` of taxon names/IDs
   - Build each as a `tax_name(X)` query (or `tax_tree`/`tax_lineage` if
     `--taxon-type` is set)
   - POST to `/msearch` with all queries in one payload
   - Merge response TSVs with deduped headers
     The file-size limit from old CLI (1000 entries) should be revisited in
     light of msearch capacity.

3. **`--taxonomy` — note only, no option needed**
   Only NCBI taxonomy is planned for the foreseeable future. Hardcode
   `&taxonomy=ncbi` in the client template (already implicit from API default)
   rather than exposing an option. Revisit if BoaT or another instance uses a
   different taxonomy backbone — at that point it becomes a `site.yaml` field,
   not a CLI arg.

4. **`--tax-rank` — two distinct uses, needs careful design**
   The old CLI conflated two different API behaviours under one flag:
   - `&rank=species` in **search/count** — constrains which rank is returned
     (i.e. `tax_rank(species)` query wrapper, equivalent to
     `--taxon-type rank --taxon-rank species`).
   - `&ranks=species,genus,...` in **reports** — adds ancestor rank columns to
     the output table; primarily meaningful for report endpoints.
     These should be two separate options rather than one overloaded flag.
     Consider `--rank` (singular, for search result rank) vs `--ranks` (plural,
     for report rank columns).

5. **`--include-estimates`**
   Include ancestrally-inferred values. Maps to `&includeEstimates=true`.
   Currently the generated client never sends this param (behaviour is whatever
   the API default is).

6. **`--exclude` (excludeAncestral + excludeMissing)**
   Filter out rows where all values are inferred or absent. Old CLI built
   `&excludeAncestral[0]=field&excludeMissing[0]=field` per requested field.
   Significant for data-quality pipelines.

7. **`--url` (print URL mode)**
   Print the constructed API URL and exit without fetching. Essential for
   debugging. One-liner addition to generated search/count/lookup handlers.

8. **Count-before-search warning + `searchPaginated` for large results**
   Before `search` executes, run a silent `count` query; if `count > size`
   print a warning to stderr. Old CLI did this for every search.
   The API now also has `searchPaginated` for result sets beyond the single-page
   limit — this is the correct path for large queries rather than bumping `size`
   arbitrarily. Should be wired in alongside the count warning: if
   `count >> page_size`, automatically paginate rather than truncating.

### New subcommands

9. **Report API expansion**
   Both `newick` and `sources` are part of the `/report` endpoint. Rather than
   adding them as isolated subcommands, build a general `report` function in
   `client.rs.tera` and expose all the main report types:
   - `tree` → Newick string (current `newick`)
   - `sources` → data provenance table
   - `histogram` → binned variable distribution
   - `scatter` → two-variable scatter (requires `x` + `y`)
   - `arc` → arc plot (requires `x` + `rank`)
   - `xPerRank` → per-rank breakdown
   - `map` → geographic map data
   - `table` → generic tabular report
     The generator could expose these via a `reports:` section in `site.yaml`
     listing which report types the site supports.

### Reconsider from deprecation list

10. **`--goat-ui-url` — keep**
    The UI URL is fully predictable from the API URL at generation time. Useful
    for interactive exploration: `goat-cli taxon search --taxon Homo sapiens
--goat-ui-url` opens the equivalent GoaT browser view. Low cost to support.

11. **`--tidy` — keep, but use API tidy format**
    Rather than a bespoke pivot in the CLI, pass `&summaryValues=false` (or
    the equivalent tidy parameter) to the API to get the established tidy-data
    format natively. This is a cleaner approach than the old CLI's custom
    pivoting logic and produces the format that downstream R/Python tools expect.

### Async vs sync — design note

12. **Progress bar requires async**
    The current generator produces sync reqwest code, which blocks for the full
    response before printing anything. A progress bar for large result sets
    (especially with `searchPaginated`) requires streaming the response, which
    in turn requires async reqwest + tokio. Trade-offs:
    - **Sync**: simpler code, no tokio dep, works fine for small–medium queries.
    - **Async**: enables progress bars, concurrent msearch, non-blocking
      pagination — all high-value for large workflows.
      Recommendation: start sync (current approach), add an optional `async`
      feature flag later, or make the generated `main.rs` async from the start
      (tokio is a light dependency) and add the progress bar as a follow-on.
      The `client.rs.tera` HTTP calls are the only things that need changing.

---

## 🗑️ Deprecate

Do not carry these forward.

| Feature                  | Reason                                                                                                                                                                                                       |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **`--variables` / `-v`** | Freeform field names backed by a static hardcoded database that goes stale. Replaced by explicit flags; future `--fields` free-text arg supersedes it cleanly.                                               |
| **`--raw`**              | Changes response schema in non-trivial ways; rarely used. Users who need it can build the URL directly with `--url`.                                                                                         |
| **`--toggle-direct`**    | Adds `:direct/:ancestor/:descendant` column variants per field. Niche; users can pass `field:direct` in `--query` strings manually.                                                                          |
| **`--print-expression`** | Printed a hardcoded static table of field names + types. Replaced by the dynamic field list from `resultFields` (generator already fetches this). Future `goat-cli fields list` subcommand covers it better. |
| **`--progress-bar`**     | Depended on async reqwest. Re-evaluate when async client is adopted (see design note 12).                                                                                                                    |
| **`--descendants`**      | Subsumed by `--taxon-type tree` (see item 1 above).                                                                                                                                                          |
| **`--lineage`**          | Subsumed by `--taxon-type lineage` (see item 1 above).                                                                                                                                                       |

---

## Suggested implementation order

1. Config gaps (YAML) — `biosample`, assembly `--assembly`/`--date` flags, `names` param type
2. `--taxon` / `-t` + `--taxon-type` (name/tree/lineage)
3. `--file` batch input via `msearch`
4. `--url` print mode
5. `--include-estimates` + `--exclude`
6. Count warning + `searchPaginated` for large results
7. `--rank` (search) vs `--ranks` (report columns)
8. `--tidy` via API native tidy format
9. `--goat-ui-url`
10. Report API expansion (tree/sources/histogram/scatter/…)
11. Async client + progress bar
