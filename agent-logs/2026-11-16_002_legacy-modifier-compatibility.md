---
date: 2026-11-16
agent: GitHub Copilot
model: MAI-Code-1.1-Flash
task: Restore compatibility for legacy plural modifiers and validate the live Mammalia assembly example
files_changed:
  - crates/genomehubs-query/src/query/attributes.rs
  - crates/genomehubs-api/src/routes/search.rs
---

## Task summary

I traced the remaining example breakage to two related issues: the live API request shape was still being written incorrectly in the Swagger example, and the request parser rejected the older plural `modifiers` alias that older examples and SDK payloads still use. I fixed both and rechecked the example against the local API.

## Key decisions

- **Decision:** accept both `modifier` and legacy `modifiers` in the YAML schema so older payloads keep working without breaking the canonical snake_case contract.
- **Decision:** keep the example request in the API’s real JSON contract: field selections remain under `query`, while pagination and taxonomy settings remain under `params`.
- **Alternative considered:** reject the alias and force all callers to update at once. This was rejected because it would unnecessarily break existing examples and compatibility for older query documents.

## Interaction log

| Turn | Role  | Summary                                                                                                                           |
| ---- | ----- | --------------------------------------------------------------------------------------------------------------------------------- |
| 1    | User  | Reported the example still failed and wanted the underlying contract checked, not just a superficial rename.                      |
| 2    | Agent | Verified the live route rejected the example because `fields` was placed under `params_yaml` and the parser rejected `modifiers`. |
| 3    | Agent | Fixed the compatibility alias and updated the example to the actual valid contract.                                               |
| 4    | Agent | Re-ran the live API request and confirmed it returned real Mammalia assembly hits.                                                |

## Changes made

- Updated [crates/genomehubs-query/src/query/attributes.rs](../crates/genomehubs-query/src/query/attributes.rs) to accept `modifiers` as an alias for the canonical `modifier` field.
- Updated the live Swagger example in [crates/genomehubs-api/src/routes/search.rs](../crates/genomehubs-api/src/routes/search.rs) so it reflects the actual request shape the server expects.
- Added a regression test to cover the legacy plural alias and confirm the compatibility layer remains in place.

## Notes / warnings

The API contract is intentionally strict: JSON bodies use snake_case keys such as `exclude_ancestral`, while the older URL form is camelCase (for example `excludeAncestral`). The valid body layout for search requests is `query` + `params`, not `fields` inside the `params` block.
