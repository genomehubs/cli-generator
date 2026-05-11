# 2026-05-11_002 — Lineage summary support in generated Rust CLI

## Summary

Added `--lineage-rank-summary` flag to the generated Rust CLI template, giving
the CLI parity with the Python, R, and JavaScript SDKs for lineage aggregate
columns.

## Changes

### `templates/rust/main.rs.tera`

- Added `--lineage-rank-summary <RANK:FIELD[:MODE],...>` clap argument (repeatable,
  one per rank) to the `Search` subcommand.
- Added `lineage_rank_summary` to the destructured match arm.
- Added call to new `parse_lineage_rank_summary_args()` helper before
  constructing `SearchOptions`.
- Updated `SearchOptions` construction to pass the two new fields.
- Added `fn parse_lineage_rank_summary_args()` at end of file: parses each arg
  into (a) a `lineage_rank_summary` JSON array for the POST body and (b) a
  config JSON object for `parse_search_with_lineage_summary`.

### `templates/rust/client.rs.tera`

- Added two fields to `SearchOptions`:
  - `lineage_summary_specs: Vec<serde_json::Value>` — POST body field
  - `lineage_summary_config: serde_json::Value` — parse config (`Null` = no-op)
- Updated `Default` impl to initialise both fields to empty/Null.
- `build_search_post_body`: appends `lineage_rank_summary` to the body when
  `specs` is non-empty.
- `search()` (v3 path): calls `parse_search_with_lineage_summary` when config
  is non-null, otherwise `parse_search_json`.
- `search_all_v3()`: same conditional dispatch on the inner parse call.

## Arg format

```
--lineage-rank-summary RANK:FIELD[:MODE][,FIELD[:MODE]]
```

Examples:

```bash
# top mode (default) for assembly_level at genus rank
--lineage-rank-summary genus:assembly_level

# stats for genome_size, explicit top for assembly_level
--lineage-rank-summary genus:assembly_level:top,genome_size:stats

# multiple ranks (repeat the flag)
--lineage-rank-summary genus:genome_size:stats \
--lineage-rank-summary family:assembly_level
```

## Output columns (with double-underscore separators)

| Mode  | Column name pattern                                             |
| ----- | --------------------------------------------------------------- |
| top   | `genus__assembly_level`                                         |
| all   | `genus__assembly_level` (object)                                |
| count | `genus__assembly_level__count`                                  |
| stats | `genus__genome_size__avg`, `...__count`, `...__min`, `...__max` |

## Verification

- End-to-end test via local API + `parse_search_with_lineage_summary` confirmed
  correct columns: `genus__assembly_level`, `genus__genome_size__{avg,count,min,max}`.
- 23 Rust unit tests pass (`cargo test -p genomehubs-query lineage_summary`).
- Generated site compiles and `--help` shows the new flag correctly.
