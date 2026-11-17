---
date: 2026-11-16
agent: GitHub Copilot
model: MAI-Code-1.1-Flash
task: Fix Swagger example payloads and validate them against the local GoaT API
files_changed:
  - config/swagger-examples-goat.yaml
---

## Task summary

I checked the Swagger example payloads used for GoaT API examples and validated them against the local instance running on port 3000. The main issue was a mismatch between the JSON contract expected by the API and the example payloads: some batch examples were using field names and shapes that the live server rejected. I corrected the examples and re-tested them end-to-end with the running API.

## Key decisions

- **Decision:** align the examples with the server's actual JSON contract rather than the URL query parameter names used by the client SDKs.
- **Alternative considered:** leave the batch examples as-is and report them as API bugs. This was rejected because the Swagger example file is the canonical source for request bodies and should match the actual contract.

## Interaction log

| Turn | Role  | Summary                                                                                                                   |
| ---- | ----- | ------------------------------------------------------------------------------------------------------------------------- |
| 1    | User  | Identified a broken example using `excludeAncestral` instead of `exclude_ancestral` and asked for a full validation pass. |
| 2    | Agent | Checked the Swagger examples against the local API contract and narrowed the failures to batch examples.                  |
| 3    | User  | Confirmed the issue was in the example payloads rather than the server.                                                   |
| 4    | Agent | Updated the invalid batch request shapes and verified they succeed against the live API.                                  |

## Changes made

Updated the examples in [config/swagger-examples-goat.yaml](config/swagger-examples-goat.yaml) to match the live request schema:

- `summary/batch` now uses `recordId` keys and a comma-delimited `fields` string.
- `report/batch` now includes a required `params` block inside each item.
- The rest of the request bodies were validated unchanged and confirmed to work against the instance running locally.

## Notes / warnings

The API accepts snake_case keys inside JSON bodies (for example `exclude_ancestral`), while URL query strings use the older camelCase form (for example `excludeAncestral`). This difference is important when writing examples for Swagger and direct `curl` payloads.
