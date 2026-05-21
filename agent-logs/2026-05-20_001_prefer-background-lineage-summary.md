---
date: 2026-05-20
agent: GitHub Copilot
model: GPT-5 mini
task: prefer-background-lineage-summary-and-expose-param
files_changed:
  - crates/genomehubs-query/src/parse.rs
  - python/cli_generator/query.py
  - GETTING_STARTED-python.md
---

## Task summary

Update the parsing and SDK surface so lineage summaries returned by the API use
background distributions by default. Expose a Python-side parameter to opt into
matched-only summaries and document the new default.

## Key decisions

- **Decision:** Prefer `lineage_summary_background` when present in API
  responses and fall back to `lineage_summary` for compatibility. This keeps
  server-side changes backward compatible while switching SDK default to
  background semantics.
- **Decision:** Add a Python `QueryBuilder.set_lineage_summary_mode()` setter
  and include `lineage_summary_mode` in the params YAML so users can explicitly
  request matched-only summaries.
- **Alternative considered:** Modifying all SDK languages (R/JS) in this
  session. Deferred to keep the change minimal and focused — update JS/R in a
  follow-up if desired.

## Interaction log

| Turn | Role  | Summary                                                            |
| ---- | ----- | ------------------------------------------------------------------ |
| 1    | User  | Request to update SDKs/docs to prefer background lineage summaries |
| 2    | Agent | Propose changing parser to prefer `lineage_summary_background` and |
|      |       | exposing Python param; request permission to patch files           |
| 3    | User  | Proceed                                                            |
| 4    | Agent | Applied patches, ran tests, updated docs                           |

## Changes made

- `crates/genomehubs-query/src/parse.rs`
  - Prefer `lineage_summary_background` when present and fall back to
    `lineage_summary`.
- `python/cli_generator/query.py`
  - Add `_lineage_summary_mode` attribute (default `"background"`).
  - Add `set_lineage_summary_mode()` setter with validation.
  - Include `lineage_summary_mode` in `to_params_yaml()` output.
- `GETTING_STARTED-python.md`
  - Document the new default behaviour and how to opt into matched-only
    summaries using `qb.set_lineage_summary_mode("matched")` or the params YAML.

## Notes / warnings

- The Python SDK change is in the builder code. To use the updated parsing
  behaviour in a built extension you must rebuild the Python extension with
  `maturin develop --features extension-module` if consuming the compiled
  `genomehubs-query` extension.
- JS and R SDK templates were not modified in this session; they will still
  observe the previous behaviour until updated. Consider synchronising those
  SDKs in a follow-up task.
- No end-to-end integration test for Phase A → Phase B `_msearch` was added
  here; adding an integration test would be a good next step.
