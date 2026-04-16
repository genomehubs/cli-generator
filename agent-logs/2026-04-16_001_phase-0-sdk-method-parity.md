# 2026-04-16_001 Phase 0 – SDK method parity across Python / JS / R

## Summary

Completed Phase 0 of `docs/sdk-parse-parity-plan.md`: standardised method names and added missing
methods across all three language SDKs so that every builder exposes the same public interface.

---

## Changes

### `templates/r/query.R`

- **Private fields added**: `rank_name`, `assemblies`, `samples`, `names_list`, `ranks_list`
- **`initialize()`**: initialises all five new fields
- **`add_attribute()`**: added `modifiers = NULL` parameter
- **`set_attributes()`** _(new)_: replaces the entire attribute filter list
- **`set_taxa()`**: changed from variadic `...` to explicit `taxa` vector parameter
- **`set_rank()`** _(new)_: restricts to a taxonomic rank
- **`set_assemblies()`** _(new)_: filters by assembly accession IDs
- **`set_samples()`** _(new)_: filters by sample accession IDs
- **`add_field()`**: added `modifiers = NULL` parameter; now stores `list(name = name)` objects instead of bare strings
- **`set_fields()`**: updated signature to accept a list of `list(name = ...)` objects (was a `names` character vector)
- **`set_names()`** _(new)_: sets taxon name classes
- **`set_ranks()`** _(new)_: sets lineage rank columns
- **`add_sort()` → `set_sort()`**: renamed for consistency
- **`set_include_estimates()`** _(new)_: controls estimated-value inclusion
- **`set_taxonomy()`** _(new)_: sets the taxonomy source
- **`to_query_yaml()`**: updated to serialise `rank`, `assemblies`, `samples`, `names`, `ranks`; fields are now serialised directly (no more `lapply` wrapping)
- **`count()`**: fixed JSON path from `body[["count"]]` → `body[["status"]][["hits"]]`
- **`snippet()`**: fixed `selections` extraction from `as.character` to `function(f) f[["name"]]`
- **`reset()`** _(new)_: clears all filter/sort state while preserving index and params
- **`merge()`** _(new)_: merges non-default state from another builder (accesses `other$.__enclos_env__$private`)
- **`combine()`** _(new)_: static-style factory that creates a merged builder from multiple instances

### `templates/js/query.js`

Status before this session: already had `setAttributes`, `setFields`, `setSort`, fixed `count()` JSON
path (`status.hits`), and public `toQueryYaml`/`toParamsYaml`.

- **`describe()`** _(new stub)_: throws an informative error; will be wired to WASM in Phase 3
- **`snippet()`** _(new stub)_: throws an informative error; will be wired to WASM in Phase 3

### `templates/snippets/r_snippet.tera`

- `qb$add_sort(...)` → `qb$set_sort(...)`
- Replaced `qb$set_fields(c(...))` with per-field `qb$add_field("...")` calls
- `qb$build()` → `qb$to_url()`
- Replaced httr-based comment with `qb$search()` idiom

### `templates/snippets/python_snippet.tera`

- `qb.add_sort(...)` → `qb.set_sort(...)`
- Replaced `qb.set_fields([...])` with per-field `qb.add_field("...")` calls
- `qb.build()` → `qb.to_url()`
- Replaced requests-based comment with `qb.search()` idiom

---

## Verification

```
cargo fmt --all && cargo clippy --all-targets -- -D warnings  → clean
cargo test                                                     → 15/15 passed
maturin develop --features extension-module
pytest tests/python/ -q                                        → 70/70 passed
```

---

## Method parity matrix after Phase 0

| Method                        | Python | JS                       | R                   |
| ----------------------------- | ------ | ------------------------ | ------------------- |
| `set_taxa`                    | ✅     | ✅ `setTaxa`             | ✅ (vector param)   |
| `set_rank`                    | ✅     | ✅ `setRank`             | ✅                  |
| `set_assemblies`              | ✅     | ✅ `setAssemblies`       | ✅                  |
| `set_samples`                 | ✅     | ✅ `setSamples`          | ✅                  |
| `add_attribute(…, modifiers)` | ✅     | ✅                       | ✅                  |
| `set_attributes`              | ✅     | ✅                       | ✅                  |
| `add_field(…, modifiers)`     | ✅     | ✅                       | ✅                  |
| `set_fields`                  | ✅     | ✅                       | ✅                  |
| `set_names`                   | ✅     | ✅ `setNames`            | ✅                  |
| `set_ranks`                   | ✅     | ✅ `setRanks`            | ✅                  |
| `set_size`                    | ✅     | ✅                       | ✅                  |
| `set_page`                    | ✅     | ✅                       | ✅                  |
| `set_sort`                    | ✅     | ✅ `setSort`             | ✅ (was `add_sort`) |
| `set_include_estimates`       | ✅     | ✅ `setIncludeEstimates` | ✅                  |
| `set_taxonomy`                | ✅     | ✅ `setTaxonomy`         | ✅                  |
| `to_query_yaml`               | ✅     | ✅                       | ✅                  |
| `to_params_yaml`              | ✅     | ✅                       | ✅                  |
| `to_url`                      | ✅     | ✅ `toUrl`               | ✅                  |
| `count`                       | ✅     | ✅                       | ✅                  |
| `search`                      | ✅     | ✅                       | ✅                  |
| `describe`                    | ✅     | ✅ (stub)                | ✅                  |
| `snippet`                     | ✅     | ✅ (stub)                | ✅                  |
| `reset`                       | ✅     | ✅                       | ✅                  |
| `merge`                       | ✅     | ✅                       | ✅                  |
| `combine`                     | ✅     | ✅ (static)              | ✅                  |

---

## Decisions made

- `set_taxa` in R changed from variadic `...` to explicit `taxa` vector — breaking change, but aligned
  with Python/JS and more ergonomic for programmatic use.
- `add_field` / `add_attribute` in R now store objects (`list(name=...)`) not bare strings; snippet
  templates updated accordingly.
- JS `describe()` / `snippet()` are stubs that throw until Phase 3 wires them to WASM exports.
- `set_sort` replaces `add_sort` in R; only one sort key is supported (last call wins).

---

## Next steps (Phase 1)

Add parse functions in a `parse` subcrate:

- `ResponseStatus` struct
- `parse_search_json(json_str) -> ResponseStatus`
- `parse_search_tsv(tsv_str) -> Vec<HashMap<String, String>>`

Expose in Python, expose in generated R/JS once WASM bindings are extended (Phase 3).
