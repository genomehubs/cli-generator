---
date: 2026-11-17
agent: GitHub Copilot
model: claude-haiku-5.5
task: Add a CLI `validate` subcommand for queries that mirrors the SDK `validate()` and consolidate duplicate validation logic
files_changed:
  - crates/genomehubs-query/src/validation.rs
  - src/core/codegen.rs
  - src/core/runtime_query.rs
  - src/commands/check.rs
  - src/commands/validate.rs
  - src/commands/mod.rs
  - src/main.rs
  - tests/generated_goat_cli.rs
  - templates/shared/GETTING_STARTED.md.tera
---

## Task summary

The user asked for a `validate` subcommand so the CLI works like the SDKs, and
asked that the implementation be consolidated rather than fragmented. The
existing `validate [REPO]` command checked repo sync and clashed with the
name, so it was renamed to `check` (user decision). The new `validate`
subcommand accepts the same inputs as `describe` (v2 `--query`/`--field`, or
v3 `--query-yaml`) and runs the portable validator from the
`genomehubs-query` subcrate, the same engine used by the SDK `validate()`
methods.

## Key decisions

- **Decision:** Rename the repo-sync command to `check`; `validate` is the query validator.
  **Alternative considered:** keep `validate` for repo sync and name the query check `lint` or `check-query`. Rejected because the user wanted SDK-like naming.
- **Decision:** Validate through `validate_query_for_fields` in the subcrate with the default config.
  **Alternative considered:** a CLI-only validator. Rejected to keep one implementation.
- **Decision:** Share the query flags between `describe` and `validate` through a flattened `QueryArgs` clap struct.
  **Alternative considered:** duplicate the flag definitions. Rejected as fragmenting.
- **Decision:** Codegen uses `field_meta_map` from the subcrate instead of an inline `json!` conversion.
  **Alternative considered:** leave codegen unchanged. Rejected because it was a third copy of the FieldDef to field-meta conversion. The API keeps its own conversion because its input is Elasticsearch JSON, not `FieldDef`.

## Interaction log

| Turn | Role  | Summary                                                                                                                 |
| ---- | ----- | ----------------------------------------------------------------------------------------------------------------------- |
| 1    | User  | Asked for a CLI validate subcommand that mirrors the SDKs                                                               |
| 2    | Agent | Asked which name the query validator should use, given the existing `validate`                                          |
| 3    | User  | Chose to rename the repo-sync command to `check`                                                                        |
| 4    | Agent | Added subcrate helpers, rewired codegen, renamed `DescribeInput` to `QueryInput`, added `check` and `validate` commands |
| 5    | Agent | Ran clippy and tests, smoke-tested v2 and v3 inputs against the local API                                               |

## Changes made

- `crates/genomehubs-query/src/validation.rs`: added `validate_query_for_fields`,
  `field_meta_map`, and `synonym_map`; split `check_query` out of
  `validate_query_impl`; added `Default` for `ValidationConfig`; added unit tests.
- `src/core/codegen.rs`: builds `field_meta.json` with `field_meta_map`.
- `src/core/runtime_query.rs`: `DescribeInput` renamed to `QueryInput`; added
  `query_yaml_from_request`, `query_yaml_target`, and `validate_input`; tests for
  the v2 and v3 target index.
- `src/commands/check.rs`: the former `commands/validate.rs` repo-sync logic, renamed
  to `check`. Behaviour is unchanged.
- `src/commands/validate.rs`: new query validator. It prints the error list as JSON
  and exits non-zero when errors exist.
- `src/main.rs`: `QueryArgs` shared by `describe` and `validate`; `Check` command.
- `tests/generated_goat_cli.rs`: repo-check tests now call `commands::check::run`.
- `templates/shared/GETTING_STARTED.md.tera`: `cli-generator validate .` changed to `cli-generator check .`.

## Notes / warnings

- Breaking change: `cli-generator validate [REPO]` is now `cli-generator check [REPO]`.
  Any scripts that call the old form need updating.
- The API's `build_field_meta` and `build_synonyms` still convert Elasticsearch JSON
  rather than `FieldDef`. Aligning them would need a shared intermediate type.
- The Rust SDK template (`templates/rust/sdk.rs.tera`) still uses the root phf
  validator. Migrating it needs user confirmation.
- Search and count field checks in `metadata::validate_fields` remain name-only and are unchanged.
- Smoke tests run against `http://localhost:3000/api`: an unknown attribute and an
  unknown field are reported, and a valid v3 query returns `[]` with exit 0.
