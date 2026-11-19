---
date: 2026-11-18
agent: GitHub Copilot
model: Claude Haiku 5.5
task: Align the v3 PhyloPic resolver with the latest v2 handler before adding the phylopic CLI
files_changed:
  - crates/genomehubs-api/src/phylopic_client.rs
  - crates/genomehubs-api/src/routes/phylopic.rs
  - docs/planning/phases/phase-14-phylopic.md
---

## Task summary

The user asked for the v3 PhyloPic API to match the latest v2 handler
(`/Users/rchallis/projects/genomehubs/genomehubs/src/genomehubs-api/src/api/v2/routes/phylopic.js`)
before the phylopic CLI is added. The v2 handler contains fixes that v3 lacked.
The response shape is unchanged. Only the values chosen for `source`,
`image_rank`, `license`/`license_url`, and the image selected can change.

## Key decisions

- **Decision:** source classification follows v2. A self match is `Primary` at
  species rank and `Descendant` otherwise. Any other match is `Ancestral`. A
  title match is `Primary`.
  **Alternative considered:** keep the v3 `classify_source(matched_id, taxon_id,
lineage_ids)`. Rejected because it made every name-search result `Ancestral`,
  which is not what v2 returns.
- **Decision:** `image_rank` is the taxon's rank for a self match. For an
  ancestor it is the lineage rank from `lineage_ranks`, falling back to
  `lineage`. Lineage ranks are read from the taxon document in
  `fetch_taxon_info`.
- **Decision:** name search requires exactly one item whose title is a synonym,
  as v2 does. The v3 first-result fallback is removed. This is a behaviour
  change: a name search with no synonym hit now returns `NotFound` instead of
  an unrelated image.
  **Alternative considered:** keep the first-result fallback, as the v3 comment
  described. Rejected because it can attach another taxon's image.
- **Decision:** the name-search query uses the lowercased scientific name only.
  The v3 `normalise_name` (which stripped punctuation) is removed, as in v2.
- **Decision:** if the selected image is not public domain (licence URL lacks
  `publicdomain`), look up a public-domain image of the same node
  (`images?filter_node=...&filter_license_by=false&...`) and use it. Any failure
  keeps the original. This is the v2 rule.
- **Decision:** `extract_attribution` defaults `license_url` to an empty string,
  not CC0. With the CC0 default, an unknown licence would look public domain
  and the swap would never run. An unknown licence now reports `unknown`.
- **Decision:** the GBIF bridge is unchanged in source (`Ancestral`). It has no
  v2 equivalent. It now passes the node href (for the public-domain swap) and
  the rank `lineage`.

## Interaction log

| Turn | Role  | Summary                                                                             |
| ---- | ----- | ----------------------------------------------------------------------------------- |
| 1    | User  | Asked for the v3 phylopic API to match v2 before the CLI is added                   |
| 2    | Agent | Read the latest v2 handler and the v3 client, then applied the parity changes above |

## Changes made

- `phylopic_client.rs`:
  - `TaxonInfo.lineage_ranks`.
  - `ResolveLinks.self_link` (node href for the public-domain lookup).
  - `classify_source(is_self, title_matches, rank)`.
  - `resolve_by_ncbi` computes `image_rank` and `is_self`.
  - `resolve_by_name` uses `unique_synonym_match`, classifies as a self match,
    and passes the node href.
  - `build_record` wraps `record_from_image` and applies the public-domain swap.
  - New helpers: `is_public_domain`, `node_uuid_from_href`, `get_json`,
    `fetch_public_domain_image`, `unique_synonym_match`. `normalise_name` removed.
  - `extract_attribution` licence default is empty.
  - Tests: four `classify_source` tests, three `unique_synonym_match` tests,
    `node_uuid_extracted_from_node_href`, `public_domain_detected_from_licence_url`.
- `routes/phylopic.rs`: `fetch_taxon_info` builds `lineage_ranks` from the
  lineage `taxon_rank` values.
- `docs/planning/phases/phase-14-phylopic.md`: synonym section and test list
  updated to the v2 rule.

No route, query parameter, or response field changed.

## Verification

- `cargo fmt --all` and `cargo clippy -p genomehubs-api --all-targets -- -D warnings`: pass.
- `cargo test -p genomehubs-api phylopic`: 18 passed.
- Not run: a live check against the local API or PhyloPic. The public-domain
  lookup uses `images/{uuid}?embed_primaryImage=true`, which reuses the flag
  from the existing name-search fetch. It is unverified against the live API.

## Notes / warnings

- Parity gap: v3 still tries name search when NCBI returns `NotFound` or an
  error. v2 returns the error in that case. The v3 behaviour is kept as a
  superset.
- Parity gap: the NCBI path cannot check the node title for the `Primary`
  title-match rule, because the embedded image node carries no title. The rule
  applies only to name search.
- Behaviour change to confirm with the user: the synonym rule, lowercase-only
  name query, and unknown-licence default. These affect cached and returned
  records.
