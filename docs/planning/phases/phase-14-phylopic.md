# Phase 14: PhyloPic Integration

**Depends on:** Phase 1 (AppState, es_client infrastructure)
**Blocks:** nothing downstream (tree report in Phase 6 can use this, but is not blocked by it)
**Estimated scope:** 1 new route file, 1 new module, 1 AppState extension, no new ES queries

---

## Goal

Implement `GET /api/v3/phylopic` and `POST /api/v3/phylopic/batch` — proxy endpoints
that resolve taxon IDs to PhyloPic silhouette images. The batch endpoint is required
for tree reports, which need silhouettes for each leaf node.

This is a **proxy and cache** layer around the PhyloPic API v2 (current: `2.7.3`,
build `538` as of 2026-05-01). It handles resolution strategy, caching, and
normalisation; clients receive a clean uniform response.

---

## v2 Audit: Issues in `local-api-copy/src/api/v2/routes/phylopic.js`

### 1. Fragile `external` link parsing

`resolveByTaxId` identifies which taxon in the lineage was matched by parsing the
`external` link href:

```javascript
let external = (json._links.external || []).find((link) =>
  link.href.includes("ncbi.nlm.nih.gov/taxid"),
);
let validRank = ranks[external.href.split("/")[4].replace(/\?.+/, "")] || rank;
```

This hard-codes the assumption that the matched taxon ID sits at path segment 4 of an
NCBI URL. The URL format `https://www.ncbi.nlm.nih.gov/taxonomy/{id}` has the ID at
segment 3 when counting from 0 (after removing the empty string for the leading `/`).
The code happens to work because the href stored in phylopic is
`/resolve/ncbi.nlm.nih.gov/taxid/{id}` (a relative API path, not the full NCBI URL),
where segment 4 is the ID. This is brittle — any URL format change breaks rank
detection silently (falls back to the requested taxon's rank, which may be wrong).

### 2. Two-round-trip node fetch

After resolving to a node href, the code fetches the node separately to get
`rasterFiles`. The PhyloPic v2 API supports `embed_primaryImage=true` on the resolve
call and on node fetches, reducing this to one HTTP request.

### 3. Fragile synonym matching in `resolveByName`

```javascript
if (items.length >= 1) {
  let synonyms = taxonNames.map(({ name }) => name.toLowerCase());
  items =
    items.filter(({ title }) => synonyms.includes(title.toLowerCase())) ||
    items[0];
}
if (items.length != 1) { return { status: { success: false ... } } }
```

The `|| items[0]` never fires because an empty array is truthy. If the filter returns
zero matches, `items` becomes `[]`, then `items.length != 1` is true, and the whole
name resolution fails. The intent was to fall back to `items[0]` when no synonyms
match, but the implementation is wrong. This causes unnecessary failures when the
scientific name search returns results but none match the synonym list exactly.

### 4. In-process memory cache only

```javascript
let phylopics = {};
```

Module-level object — lost on every restart, not shared across processes, unbounded
growth, no TTL or build-number invalidation. If the PhyloPic build advances (new
images published), cached stale responses persist indefinitely.

### 5. No vector file support

The v2 response returns only `rasterFiles[1]` (second-largest raster). PhyloPic v2 API
also provides `vectorFile` (SVG), which is resolution-independent and superior for UI
rendering at arbitrary sizes. v2 phylopic.js never returns it.

### 6. No batch resolution

Tree reports need silhouettes for N taxa. v2 makes N independent HTTP calls (plus
cache hits). There is no batching of the PhyloPic `/resolve` endpoint, which accepts
up to ~200 comma-separated IDs in a single request.

### 7. No GBIF fallback

The PhyloPic API documentation documents `/resolve/gbif.org/species` as an alternative
resolution path when NCBI tax IDs are not found. The v2 implementation has no fallback
beyond name search.

### 8. No build number tracking

The PhyloPic API requires a `build` query parameter on paginated requests. v2 obtains
the current build only as a side-effect of the first successful resolve call, and
stores it in the response object. There is no proactive build-check on startup or
scheduled refresh.

---

## v3 Design

### Resolution strategy

For a single taxon, the resolution pipeline is:

```
1. NCBI batch resolve
   POST /resolve/ncbi.nlm.nih.gov/taxid
   objectIDs = [taxon_id, ...lineage_ids_most_to_least_specific]
   embed_primaryImage = true

2. If result is Ancestral → try PhyloPic name search
   GET /nodes?build=N&filter_name={normalised_name}&page=0
   Match returned nodes against scientific_name + taxon_names synonyms
   (see corrected synonym matching below)

3. If name search fails → try GBIF bridge (if GBIF ID available in record)
   GET /resolve/gbif.org/species
   objectIDs = gbif lineage keys in order
   embed_primaryImage = true
```

Steps 2 and 3 only run if step 1 fails or produces an Ancestral result.

### Build number management

On startup, `PhylopicClient` fetches `https://api.phylopic.org/` (one request, no
`build` param, follows the 307 redirect) and caches the `build` field. This is
refreshed every 24 hours by a background `tokio::spawn` task. Cache entries are keyed
by `(taxon_id, build)` — when the build advances, old entries are not served.

### Caching

```rust
// Added to AppState
pub phylopic_cache: Arc<RwLock<PhylopicCache>>,
```

`PhylopicCache` is:

```rust
pub struct PhylopicCache {
    pub current_build: u32,
    pub entries: HashMap<String, PhylopicRecord>,  // key: taxon_id
    pub build_at_fetch: HashMap<String, u32>,       // key: taxon_id → build when fetched
}
```

Cache lookup: entry is valid only if `build_at_fetch[taxon_id] == current_build`.
Entries from a previous build are transparently re-fetched.

---

## Files to Create

```
crates/genomehubs-api/src/routes/phylopic.rs   — GET /api/v3/phylopic
                                                  POST /api/v3/phylopic/batch
crates/genomehubs-api/src/phylopic_client.rs   — HTTP client, resolution strategy, build refresh
```

## Files to Modify

| File                                      | Change                                                                |
| ----------------------------------------- | --------------------------------------------------------------------- |
| `crates/genomehubs-api/src/routes/mod.rs` | `pub mod phylopic;`                                                   |
| `crates/genomehubs-api/src/main.rs`       | Register routes + OpenAPI; init `phylopic_cache` in `AppState`        |
| `crates/genomehubs-api/src/main.rs`       | Spawn background build-refresh task                                   |
| `Cargo.toml` (genomehubs-api)             | No new deps needed (`reqwest`, `serde_json`, `tokio` already present) |

---

## `PhylopicRecord` Type

```rust
use serde::{Deserialize, Serialize};

/// A silhouette image resolved from PhyloPic for one taxon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhylopicRecord {
    /// NCBI taxon ID the record was requested for.
    pub taxon_id: String,
    /// URL of a raster (PNG) file — second-largest, or largest if only one exists.
    pub raster_url: String,
    /// URL of the SVG vector file (resolution-independent). Prefer over `raster_url` in UI.
    pub vector_url: Option<String>,
    /// Aspect ratio (width / height) of the raster image.
    pub ratio: f32,
    /// Free-text attribution string from the contributor.
    pub attribution: Option<String>,
    /// SPDX licence identifier (e.g. `"CC0-1.0"`) derived from the licence URL.
    pub license: String,
    /// Licence URL.
    pub license_url: String,
    /// Display name of the contributor.
    pub contributor: Option<String>,
    /// Scientific name of the taxon the image directly illustrates.
    pub image_name: String,
    /// Canonical URL of this image on phylopic.org.
    pub source_url: String,
    /// Taxonomic rank of the node the image represents.
    pub image_rank: String,
    /// Resolution source: `Primary` | `Descendant` | `Ancestral`.
    pub source: PhylopicSource,
    /// PhyloPic build number at time of fetch.
    pub build: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhylopicSource {
    /// Image directly illustrates this taxon (or a descendant at species level).
    Primary,
    /// Image illustrates a descendant of the requested taxon.
    Descendant,
    /// Image illustrates an ancestor of the requested taxon.
    Ancestral,
}
```

---

## API Endpoints

### `GET /api/v3/phylopic`

**Query parameters:**

| Parameter  | Required | Description                 |
| ---------- | -------- | --------------------------- |
| `taxon_id` | yes      | NCBI taxon ID               |
| `taxonomy` | yes      | Taxonomy name (e.g. `ncbi`) |

The route handler:

1. Checks `phylopic_cache` (valid if `build_at_fetch[taxon_id] == current_build`)
2. If miss: fetches the taxon record from ES (`/api/v3/record`) to get lineage, rank, scientific_name, taxon_names, and optionally the GBIF ID
3. Delegates to `phylopic_client::resolve()`
4. Stores result in cache; returns `PhylopicRecord`

**Response (success):**

```json
{
  "status": { "success": true },
  "phylopic": {
    "taxon_id": "9606",
    "raster_url": "https://images.phylopic.org/images/.../raster/512x512.png",
    "vector_url": "https://images.phylopic.org/images/.../vector.svg",
    "ratio": 1.2,
    "attribution": "T. Michael Keesey",
    "license": "CC0-1.0",
    "license_url": "https://creativecommons.org/publicdomain/zero/1.0/",
    "contributor": "T. Michael Keesey",
    "image_name": "Homo sapiens",
    "source_url": "https://www.phylopic.org/images/045279d5.../",
    "image_rank": "species",
    "source": "Primary",
    "build": 538
  }
}
```

**Response (not found):**

```json
{
  "status": { "success": false, "error": "no image found for taxon_id 12345" }
}
```

Not-found is a `200 OK` with `success: false` — consistent with all other v3 endpoints.

---

### `POST /api/v3/phylopic/batch`

For tree reports. Accepts up to 200 taxon IDs.

**Request body:**

```json
{
  "taxon_ids": ["9606", "10090", "7227"],
  "taxonomy": "ncbi"
}
```

**Response:**

```json
{
  "status": { "success": true },
  "results": {
    "9606":  { "status": { "success": true }, "phylopic": { ... } },
    "10090": { "status": { "success": true }, "phylopic": { ... } },
    "7227":  { "status": { "success": false, "error": "no image found" } }
  }
}
```

Batch resolution groups all requested IDs into a single PhyloPic `/resolve` call where
possible, then fans out individual name-fallback requests only for those that returned
Ancestral results.

---

## `phylopic_client.rs` Resolution Implementation

### `resolve_by_ncbi()` — corrected from v2

```rust
// Build the objectIDs list: [taxon_id, ...lineage_ids_most_specific_first]
// (The PhyloPic /resolve endpoint returns the best match across the whole list)
let object_ids: Vec<String> = std::iter::once(taxon_id.to_string())
    .chain(lineage.iter().map(|t| t.taxon_id.to_string()))
    .collect();

let url = format!(
    "https://api.phylopic.org/resolve/ncbi.nlm.nih.gov/taxid\
     ?build={build}&objectIDs={ids}&embed_primaryImage=true",
    build = build,
    ids = object_ids.join(","),
);
```

The matched taxon ID is extracted from `_links.external[].href` by stripping the known
prefix `/resolve/ncbi.nlm.nih.gov/taxid/` rather than by index:

```rust
// v3 — robust prefix strip instead of index split
let matched_id = external_href
    .strip_prefix("/resolve/ncbi.nlm.nih.gov/taxid/")
    .and_then(|s| s.split('?').next())
    .ok_or(PhylopicError::MalformedResponse)?;
```

With `embed_primaryImage=true`, the image metadata (including `rasterFiles` and
`vectorFile`) is available in the resolve response directly — no second node fetch
needed.

### `resolve_by_name()` — corrected synonym matching

```rust
// Build synonym set: scientific_name + all taxon_names entries
let synonyms: HashSet<String> = std::iter::once(scientific_name.to_lowercase())
    .chain(taxon_names.iter().map(|n| n.name.to_lowercase()))
    .collect();

// Search PhyloPic nodes
// filter_name must be lowercase, alphanumeric + spaces only
let normalised = normalise_name(scientific_name);
let url = format!(
    "https://api.phylopic.org/nodes?build={build}&filter_name={name}&page=0",
    name = urlencoding::encode(&normalised),
);
let items: Vec<NodeLink> = response._links.items.unwrap_or_default();

// v3 corrected logic: prefer synonym match; fall back to first result
let matched = items.iter()
    .find(|item| synonyms.contains(&item.title.to_lowercase()))
    .or_else(|| items.first());

match matched {
    None => return Err(PhylopicError::NotFound),
    Some(node_link) => { /* fetch primaryImage for this node */ }
}
```

The `|| items[0]` bug from v2 is fixed: `or_else(|| items.first())` correctly falls
back to the first item when no synonym matches.

### `resolve_by_gbif()` — new in v3

Only attempted when GBIF species key is available in the taxon record:

```rust
// gbif_id is the speciesKey from GBIF; gbif_lineage_keys is [speciesKey, genusKey, ...]
let object_ids = gbif_lineage_keys.join(",");
let url = format!(
    "https://api.phylopic.org/resolve/gbif.org/species\
     ?build={build}&objectIDs={ids}&embed_primaryImage=true",
);
```

This path requires the taxon record to carry GBIF taxon keys in its lineage data. If
the genomehubs instance does not expose GBIF IDs, this path is skipped silently.

### Image file extraction

```rust
fn extract_image_files(links: &ImageLinks) -> (String, Option<String>) {
    // raster: second-largest if available, else largest (same as v2)
    let raster_url = if links.raster_files.len() > 1 {
        links.raster_files[1].href.clone()
    } else {
        links.raster_files[0].href.clone()
    };
    // vector: new in v3
    let vector_url = links.vector_file.as_ref().map(|v| v.href.clone());
    (raster_url, vector_url)
}
```

### Licence normalisation

```rust
/// Map a PhyloPic licence URL to an SPDX identifier.
///
/// Only the licences that PhyloPic actually uses are needed.
fn licence_to_spdx(url: &str) -> &'static str {
    let normalised = url.trim_end_matches('/');
    match normalised {
        "https://creativecommons.org/publicdomain/zero/1.0" => "CC0-1.0",
        "https://creativecommons.org/licenses/by/4.0"       => "CC-BY-4.0",
        "https://creativecommons.org/licenses/by/3.0"       => "CC-BY-3.0",
        "https://creativecommons.org/licenses/by-nc/3.0"    => "CC-BY-NC-3.0",
        "https://creativecommons.org/licenses/by-sa/4.0"    => "CC-BY-SA-4.0",
        _                                                    => "unknown",
    }
}
```

This replaces the `spdx-license-list` npm package dependency from v2 with a minimal
inline match. PhyloPic only issues a small, stable set of licences; the full SPDX list
is not needed.

---

## Background Build Refresh

```rust
// Spawned in main.rs after AppState construction
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(86_400)); // 24h
    loop {
        interval.tick().await;
        if let Ok(build) = fetch_current_build(&http_client).await {
            let mut cache = phylopic_cache.write().await;
            cache.current_build = build;
            // Entries with a different build_at_fetch are now stale; they
            // will be re-fetched on next request (lazy invalidation).
        }
    }
});
```

---

## Testing

- Unit test: `licence_to_spdx` maps all known PhyloPic licence URLs correctly
- Unit test: `resolve_by_name` synonym matching prefers exact synonym match over first result
- Unit test: `resolve_by_name` falls back to first result when no synonyms match (v2 bug regression test)
- Unit test: matched taxon ID extracted from external href using prefix strip (not index)
- Unit test: `extract_image_files` returns second raster and vector URL when both present
- Unit test: cache entry is stale when `build_at_fetch[id] != current_build`
- Integration test: `GET /api/v3/phylopic?taxon_id=9606&taxonomy=ncbi` returns success
- Integration test: `GET /api/v3/phylopic?taxon_id=9999999999&taxonomy=ncbi` returns `success: false` (not an error status)
- Integration test: `POST /api/v3/phylopic/batch` with 3 known taxon IDs returns 3 results
- Integration test: second identical request is served from cache (verify by timing or mock)
- Integration test: build number mismatch causes cache miss and re-fetch
