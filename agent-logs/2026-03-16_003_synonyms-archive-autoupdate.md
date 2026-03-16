---
date: 2026-03-16
agent: GitHub Copilot
model: claude-sonnet-4-6
task: "Add synonym field matching, archive API versioning support, and auto-update CI workflow template"
files_changed:
  - src/core/fetch.rs
  - src/core/codegen.rs
  - src/core/config.rs
  - src/commands/new.rs
  - src/commands/update.rs
  - templates/autoupdate.yml.tera
---

## Task summary

Three related features were requested:

1. **Synonym field matching** — the GoaT API's `resultFields` response already
   carries a `synonyms` array on some fields (e.g. `ebp_standard_date` lists
   `ebp_metric_date` as a synonym). Patterns and explicit field names in a
   `FieldGroup` should resolve to the canonical field name even when they
   reference a deprecated synonym.

2. **Archive API versioning** — genomehubs archives use date-based version
   strings (`2025.04.21`) in place of `v2`. Since an archived API's schema is
   frozen, its field cache should never expire. A new `archive: bool` flag in
   `SiteConfig` models this.

3. **Auto-update CI workflow template** — generated repos should ship a GitHub
   Actions workflow that periodically polls the live API, runs
   `cli-generator update`, and opens a PR when generated files are stale. For
   archive sites the workflow is a no-op stub instead.

## Changes

### `src/core/fetch.rs`

- Added `pub synonyms: Vec<String>` to `FieldDef` with `#[serde(default)]` so
  it deserialises from the API response and defaults to empty when absent.
- Added `archive_mode: bool` to `FieldFetcher`; builder method
  `with_archive_mode(bool)` sets it. `load_cache()` now skips the TTL check
  when `archive_mode` is `true`.
- New tests: `parse_single_field_deserialises_synonyms`,
  `parse_single_field_defaults_synonyms_to_empty`,
  `archive_mode_cache_never_expires`.

### `src/core/codegen.rs`

- `resolve_fields()` — source 2 (explicit field names) now first checks whether
  the name is a canonical field; if not, it looks for a field whose `synonyms`
  array contains it and includes the canonical name instead. Unknown names are
  passed through unchanged so the user gets a clear compile error.
- `resolve_fields()` — source 3 (glob patterns) now also tests each field's
  synonyms against the pattern, so `ebp_metric_*` will resolve to
  `ebp_standard_date`.
- `build_context()` now inserts `archive` into the Tera context.
- `make_tera()` registers the new `autoupdate.yml` template.
- `render_all()` includes `autoupdate.yml` in the rendered output.
- `template_name_to_dest()` maps `autoupdate.yml` →
  `.github/workflows/autoupdate.yml`.
- New tests: `resolve_fields_resolves_synonym_in_fields_list`,
  `resolve_fields_resolves_synonym_via_pattern`,
  `template_name_to_dest_maps_autoupdate_workflow`.
- Updated `sample_site()` to include `archive: false`; updated `sample_fields()`
  to include `synonyms: vec![]`; updated `codegen_renders_all_templates_without_error`
  to assert the new autoupdate output key is present.

### `src/core/config.rs`

- Added `pub archive: bool` to `SiteConfig` with `#[serde(default)]`. Existing
  site YAMLs without the key continue to work (defaults to `false`).

### `src/commands/new.rs` and `src/commands/update.rs`

- Applied `.with_archive_mode(site.archive)` when constructing `FieldFetcher`
  so archive sites get indefinite cache retention.

### `templates/autoupdate.yml.tera` (new file)

- Tera template rendered to `.github/workflows/autoupdate.yml` in each
  generated repo.
- For live sites: weekly cron + `workflow_dispatch`; installs `cli-generator`,
  runs `update .  --force-fresh`, diffs `src/generated/` and `src/cli_meta.rs`,
  and opens a PR via `peter-evans/create-pull-request@v7` when changes are
  detected.
- For archive sites (`archive == true`): emits a minimal no-op workflow with
  a `workflow_dispatch`-only trigger and a comment explaining why updates are
  disabled.
- GitHub Actions `${{ }}` expressions are wrapped in `{% raw %}...{% endraw %}`
  to avoid Tera variable-expansion conflicts.

## Decisions and trade-offs

- **Synonym fallback for unknown names**: when an explicit field name in
  `fields:` is not a canonical name and not a synonym of any known field, it is
  passed through verbatim. This gives the user a compile error in the generated
  crate rather than a silent empty expansion — failing loudly is preferable.
- **`with_archive_mode` builder** instead of adding a parameter to `new()`:
  preserves the existing call sites in tests that only construct `FieldFetcher`
  for cache round-trip checks.
- **`peter-evans/create-pull-request@v7`** pinned to `@v7` (latest major as of
  session date); generated repos should pin to a SHA for production security, but
  a major-version tag is acceptable for a starter workflow that teams will
  customise.
- **Archive validate behaviour**: the existing `validate` command only checks a
  config-hash against what is stamped in `Cargo.toml`; it does not poll the API.
  No change was needed — the hash check is still valid for archives.

## Verification

```
cargo fmt --all -- --check   ✓
cargo clippy --all-targets -- -D warnings   ✓
cargo test   28/28 passed (was 22/22)
```
