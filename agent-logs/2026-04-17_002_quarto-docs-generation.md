---
date: 2026-04-17
agent: GitHub Copilot
model: claude-sonnet-4-6
task: Add Quarto documentation generation to `cargo run -- new`
files_changed:
  - templates/docs/_quarto.yml.tera
  - templates/docs/index.qmd.tera
  - templates/docs/quickstart.qmd.tera
  - templates/docs/reference/query-builder.qmd.tera
  - templates/docs/reference/parse.qmd.tera
  - templates/docs/reference/cli.qmd.tera
  - src/commands/new.rs
---

## Task summary

Added Quarto documentation generation to the CLI generator's `new` subcommand.
Running `cargo run -- new <site>` now produces a `docs/` directory in the
generated repository with a Quarto website covering installation, a quick-start
guide, and reference pages for the QueryBuilder API, parsing functions, and CLI.
The docs are populated with site-specific examples using the same Tera template
context as the rest of the SDK scaffolding.

## Key decisions

- **One function, one responsibility**: `create_quarto_docs()` follows the
  same structure as `create_r_package()` and `create_js_package()` — a single
  function that builds a Tera context from `SiteConfig`, renders all templates,
  and writes to `{repo_dir}/docs/`. No logic outside this function.
- **Six template files, not one monolithic template**: Each page (`index`,
  `quickstart`, `reference/query-builder`, `reference/parse`, `reference/cli`)
  is a separate `.tera` file. This keeps authoring isolated and matches how
  Quarto projects are structured on disk.
- **`r_package_name` passed as context variable**: Rather than relying on a
  Tera `replace` filter in the template, the R package name is computed in
  Rust (matching `create_r_package()`) and inserted as a named context key.
  This keeps templates readable and avoids filter chaining confusion.
- **Missing `repo_url` handled with Tera default filter**: `SiteConfig` has no
  `repo_url` field. The `_quarto.yml.tera` template uses
  `{{ repo_url | default(value='...') }}` so generation works without the field;
  a future SiteConfig addition will automatically flow through.
- **Autoescape disabled for Quarto templates**: `tera::Tera::one_off(…, false)`
  is used (as in `create_js_package`) so Markdown content, backticks, and angle
  brackets are not HTML-escaped in the rendered `.qmd` files.

## Interaction log

| Turn | Role  | Summary                                                                                |
| ---- | ----- | -------------------------------------------------------------------------------------- |
| 1    | User  | Session resumed from prior summary; Quarto template work was in progress               |
| 2    | Agent | Inspected existing `_quarto.yml.tera` and `index.qmd.tera`; reviewed `new.rs` patterns |
| 3    | Agent | Created `quickstart.qmd.tera` with tabbed Python/R/JS/CLI examples                     |
| 4    | Agent | Created `reference/query-builder.qmd.tera` covering all QB methods                     |
| 5    | Agent | Created `reference/parse.qmd.tera` covering all three parse functions + domain model   |
| 6    | Agent | Created `reference/cli.qmd.tera` with per-index subcommands, filter syntax, examples   |
| 7    | Agent | Added `create_quarto_docs()` to `new.rs`; wired into `run()`                           |
| 8    | Agent | `cargo check` — clean; `verify_code.sh` — all meaningful checks pass                   |

## Changes made

### `templates/docs/` (all new)

Six Tera templates that form a complete Quarto website skeleton:

| Template                           | Output path                        | Content                                                                                             |
| ---------------------------------- | ---------------------------------- | --------------------------------------------------------------------------------------------------- |
| `_quarto.yml.tera`                 | `docs/_quarto.yml`                 | Quarto project config + navbar                                                                      |
| `index.qmd.tera`                   | `docs/index.qmd`                   | Landing page with feature matrix and index list                                                     |
| `quickstart.qmd.tera`              | `docs/quickstart.qmd`              | Install + two end-to-end query examples in all 4 SDKs                                               |
| `reference/query-builder.qmd.tera` | `docs/reference/query-builder.qmd` | Full QB method reference with tabbed examples                                                       |
| `reference/parse.qmd.tera`         | `docs/reference/parse.qmd`         | `parse_search_json`, `annotate_source_labels`, `split_source_columns` with domain model explanation |
| `reference/cli.qmd.tera`           | `docs/reference/cli.qmd`           | Per-index subcommands, filter syntax, output formats                                                |

All templates use the standard Tera context:
`site_name`, `site_display_name`, `api_base`, `api_version`, `sdk_name`,
`r_package_name`, `indexes` (array of `{name}` objects).

### `src/commands/new.rs`

- Added `create_quarto_docs(repo_dir, site)` function after `create_js_package`.
- Added `create_quarto_docs(&repo_dir, &site)?;` call in `run()` after
  `create_js_package`.

## Notes / warnings

- **`quarto render` requires Quarto CLI** — the generated `docs/` directory
  is ready to render but Quarto itself must be installed separately. A
  `README.md` in the docs directory would be a useful follow-up.
- **WASM `pkg/` not rebuilt this session** — the three new parse functions
  (`parse_search_json`, `annotate_source_labels`, `split_source_columns`) were
  wired into WASM in the prior session but `pkg/` has not been rebuilt.
  JS users of generated projects will hit a runtime error until
  `wasm-pack build --target nodejs --features wasm` is run in
  `crates/genomehubs-query/` and the resulting `pkg/` is committed.
- **`reference/cli.qmd.tera` uses `trim_start_matches`** — this is a Tera
  built-in, but the rendered URL will include the full `api_base` (with
  `https://`) unless Tera's `trim_start_matches` filter works as expected.
  End-to-end test with a real site config is recommended.
