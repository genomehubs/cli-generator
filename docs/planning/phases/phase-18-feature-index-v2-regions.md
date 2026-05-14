# Phase 18: Feature Index V2 — Restructured Index and Forward-Looking Positional Queries

**Status:** Planning
**Depends on:** Phase 11 (positional endpoint — implemented)
**Blocks:** nothing downstream yet
**Affects external repos:** `genomehubs-index`, `genomehubs` (GoaT / LepBase indexing pipelines)
**Estimated scope:** large — new index mapping, new query infrastructure, region algorithm,
chain-query framework, M:N visualisation support, fixture harness

---

## 0. Scope and Approach

This phase defines the **v2 feature index** — a restructured ES mapping and matching
server-side query infrastructure that makes `/api/v3/positional` both more capable and
simpler to implement than the current nested-attribute design allows.

**There is no legacy support in this phase.** The v3 `/positional` endpoint is gated on
index version detection at API startup. Sites running the current (v1) feature index
continue to use the v2 API for non-positional reports; positional reports require a
rebuilt index. The v2 API itself is unaffected (it reads `attributes` only).

This is the right time: the v3 positional endpoint is not yet in production.

---

## 1. Current Index Limitations

The v1 feature index stores all per-feature data inside a nested `attributes` array:

```
Top-level: assembly_id, feature_id, taxon_id, primary_type
           identifiers (nested), attributes (nested)

attributes entries (key / typed_value pairs):
  feature_type, sequence_id, start, end, strand, length
  name, score, status, gc, merian_unit
  midpoint, midpoint_proportion, seq_proportion, sequence_name
  ... and any custom attributes
```

Problems this creates:

| Problem | Consequence |
|---------|-------------|
| Every filter needs a `nested` ES query | Cannot combine cross-key predicates in one clause |
| `length` is ambiguous (feature vs sequence) | Cannot filter "busco genes on sequences >= 10 Mb" without a two-stage workaround |
| Cannot sort/aggregate on feature properties directly | No ES `sort` or `stats` on nested fields |
| `sequence_id` buried in nested attributes | Any sequence-based grouping requires a nested query |
| No handle for containing window or parent region | Cannot filter features by aggregate stats of overlapping regions |

---

## 2. V2 Feature Index Structure

### Design principles

1. **Promote only the universally present positional and identity fields** to top level.
   Site-specific or sparsely populated fields (`gc`, `score`, `status`, `merian_unit`, etc.)
   stay in `attributes`. Promoting them would create an open-ended mapping that varies
   across sites, requires null-handling everywhere, and opens an unbounded list of
   candidates (gc -> coverage -> repeat content -> ...).

2. **Add `sequence_length`** as a dedicated top-level field. This is the only field
   requiring cross-document enrichment at index time (lookup parent sequence length).
   It permanently disambiguates feature length from the length of the containing sequence.

3. **Add `container_ids`** as a top-level keyword array. Precomputed IDs for
   overlapping fixed-size windows at one or more resolutions. IDs are
   resolution-prefixed so a single field carries multi-resolution membership:

   | Resolution | ID format | Example |
   |---|---|---|
   | 1 Mbp  | `win_1m:<assembly>:<seq_id>:<bin>` | `win_1m:GCA_905147045.1:LR989896.1:3` |
   | 100 kbp | `win_100k:<assembly>:<seq_id>:<bin>` | `win_100k:GCA_905147045.1:LR989896.1:31` |

   `bin` = `floor(window_start / window_size)`. Any number of resolutions can be
   stored; which prefixes are populated is an indexer decision.

   **Initial overlap policy:** a feature is assigned to any window whose interval it
   touches, even by one base (ANY overlap). A configurable overlap threshold
   (minimum fraction of feature length inside the window) is reserved for a future
   phase via an `overlap_threshold` parameter on the chain filter (see section 4.3).

   Enables chain queries without server-side range intersection at query time.

4. **Keep `attributes` nested and unchanged** for all other per-feature data, so existing
   scatter/hist pipelines and v2 API clients are unaffected.

### ES mapping (promoted fields only)

```json
{
  "mappings": {
    "properties": {
      "assembly_id":     { "type": "keyword" },
      "feature_id":      { "type": "keyword" },
      "taxon_id":        { "type": "keyword" },
      "primary_type":    { "type": "keyword" },
      "sequence_id":     { "type": "keyword" },
      "start":           { "type": "long" },
      "end":             { "type": "long" },
      "length":          { "type": "long" },
      "strand":          { "type": "byte" },
      "sequence_length": { "type": "long" },
      "container_ids":   { "type": "keyword" },   // multi-resolution: win_1m:…, win_100k:…
      "attributes":      { "type": "nested", "properties": { "...unchanged..." } },
      "identifiers":     { "type": "nested", "properties": { "...unchanged..." } }
    }
  }
}
```

### What stays in `attributes` only

Everything not promoted above — including all site-specific attributes:
- `gc`, `score`, `status`, `merian_unit`, `name`, `sequence_name`
- `midpoint`, `midpoint_proportion`, `seq_proportion`
- Custom attributes: coverage, repeat content, gene model confidence, ...
- Taxonomy propagation metadata: `aggregation_method`, `aggregation_source`

These remain queryable via nested queries and via the chain query framework in section 4.

### Migration table (v1 -> v2)

| Current location | v2 top-level field | Notes |
|---|---|---|
| `attributes[key=sequence_id].keyword_value` | `sequence_id` | New |
| `attributes[key=start].long_value` | `start` | New |
| `attributes[key=end].long_value` | `end` | New |
| `attributes[key=strand].byte_value` | `strand` | New |
| `attributes[key=length].long_value` | `length` | Feature length only |
| *(not present in v1)* | `sequence_length` | Indexer enriches from parent sequence doc |
| *(not present in v1)* | `container_ids` | Indexer computes overlapping window IDs; multi-resolution prefix `win_1m:…`, `win_100k:…` |
| `attributes[key=feature_type].keyword_value[0]` | `primary_type` | Already at top level; must be single canonical value, not array |

All promoted fields are **also retained in `attributes`** for v2 API backward-compat.

---

## 3. Index Version Detection and Endpoint Gating

### Detection

On startup the metadata task fetches the ES mapping for each active feature index:

```rust
pub enum FeatureIndexVersion { V1, V2 }

fn detect_feature_index_version(mapping: &IndexMapping) -> FeatureIndexVersion {
    if mapping.properties.contains_key("start")
        && mapping.properties.contains_key("sequence_length")
    {
        FeatureIndexVersion::V2
    } else {
        FeatureIndexVersion::V1
    }
}
```

The version is stored in the metadata cache and surfaced in `/api/v3/status`.

### Gating

The `POST /api/v3/positional` handler checks immediately after parsing `positional_yaml`:

```rust
if metadata.feature_index_version != FeatureIndexVersion::V2 {
    return Err(ApiError::feature_index_v1(index_name));
}
```

Error response:

```json
{
  "status": {
    "success": false,
    "error": "feature_index_v1",
    "message": "The /positional endpoint requires a v2 feature index. The current index 'feature--ncbi--lepbase--2025.09.29' uses v1 structure. Rebuild the index with the updated genomehubs-index pipeline."
  }
}
```

No v1 fallback code is written. The current two-stage `resolve_sequence_ids_from_filters`
workaround in `positional.rs` is **removed** in this phase.

---

## 4. Query Architecture

### 4.1 Direct queries (single-pass)

With `sequence_id`, `start`, `end`, `length`, `strand`, and `sequence_length` at top
level, the core positional query is:

```
bool.filter = [
    terms("assembly_id",     [...]),
    term("primary_type",     feature_type),
    range("length",          {gte: 500}),          // optional feature-level filter
    range("sequence_length", {gte: 10_000_000}),   // optional sequence-level filter
]
```

Attributes remaining nested (`gc`, `score`, `status`, etc.) are filtered by appending
nested sub-queries to the same `bool.filter` — cheaper than v1 since all other
predicates are already evaluated top-level before the nested phase runs.

### 4.2 Chain queries

Chain queries allow filtering features by properties of **related entities** that cannot
be expressed with a single flat query. Three types are defined.

---

#### Type A — Sequence-stat chain

**Use case:** busco genes on sequences where sequence `gc > 0.45`

Sequence documents are features in the same index (`primary_type = toplevel`).
The sequence `gc` is in their `attributes` array.

```
Step 1:  query feature index
         filter: primary_type = toplevel
                 AND assembly_id IN [...]
                 AND attributes.gc > 0.45          (nested sub-query)
         return: source.sequence_id values

Step 2:  query feature index
         filter: primary_type = <group_by type>
                 AND sequence_id IN (step 1 result)  (top-level field -- fast terms query)
         return: feature positions
```

Step 1 returns O(hundreds) of sequence IDs at most; the `terms` query in step 2 is
always safe. Both steps execute server-side before the response is built.

```yaml
filter:
  - field:    gc
    operator: gt
    value:    "0.45"
    target:   sequence        # triggers Type A chain
```

---

#### Type B — Window-stat chain

**Use case:** busco genes in regions where window coverage > 10x or repeat density < 0.2

Fixed-size windows are **indexed as features** in the same feature index — they are not
a separate index. Windows have `primary_type = window_1m` (or `window_100k`, etc.) and
store aggregate per-window stats (`coverage`, `gc`, `repeat_density`, etc.) as standard
`attributes` entries. Sequences themselves are already indexed as features
(`primary_type = toplevel`). This means no new index is required; the window pipeline
only needs to write window docs into the existing feature index.

Example window document:

```json
{
  "assembly_id":  "GCA_905147045.1",
  "feature_id":   "win_1m:GCA_905147045.1:LR989896.1:0",
  "primary_type": "window_1m",
  "sequence_id":  "LR989896.1",
  "start":        0,
  "end":          1000000,
  "length":       1000000,
  "sequence_length": 15533955,
  "attributes": [
    { "key": "coverage",       "3dp_value": 12.4 },
    { "key": "repeat_density", "3dp_value": 0.18 },
    { "key": "gc",             "3dp_value": 0.41 }
  ]
}
```

Each feature document stores the IDs of windows it overlaps in `container_ids`, using
the resolution-prefixed format from section 2:

```json
{ "feature_id": "...", "container_ids": ["win_1m:GCA_905147045.1:LR989896.1:0", "win_100k:GCA_905147045.1:LR989896.1:9"] }
```

Multiple resolutions can co-exist in `container_ids`. The chain filter selects one via
`window_size` (which maps to a prefix: `1000000` → `win_1m`, `100000` → `win_100k`).

```
Step 1:  query feature index
         filter: primary_type = window_1m              (same index as features)
                 AND assembly_id IN [...]
                 AND attributes.coverage > 10          (nested sub-query)
         return: feature_id values (= container_id values)

Step 2:  query feature index
         filter: primary_type = <group_by>
                 AND container_ids IN (step 1 result)  (top-level keyword -- fast)
         return: feature positions
```

Step 1 returns O(thousands) of window IDs; ES `terms` supports up to 65,536 values.
If step 1 exceeds this, the API batches terms queries and unions results.
If no window docs exist for the requested resolution, `target: window` returns a
structured error listing available `window_*` primary types.

```yaml
filter:
  - field:       coverage
    operator:    gt
    value:       "10"
    target:      window       # triggers Type B chain
    window_size: 1000000      # selects win_1m: prefix; omit to auto-detect
```

The `window_size` parameter is optional. When omitted, the server auto-detects available
window resolutions from the index mapping and uses the coarsest one.

---

#### Type C — Cross-feature-type chain

**Use case:** busco genes in orthogroups with >= 3 members in assembly A

```
Step 1:  query feature index
         filter: primary_type = orthogroup-member AND assembly_id = A
         aggregate: by name (orthogroup ID), filter buckets: count >= 3
         return: orthogroup name values

Step 2:  query feature index
         filter: primary_type = busco-gene
                 AND name IN (step 1 result)
         return: feature positions
```

```yaml
filter:
  - field:           name
    operator:        gte_count
    value:           "3"
    target:          feature
    target_type:     orthogroup-member
    target_assembly: GCA_905147045.1
```

Type C is lower priority than A and B but must be accommodated in the filter data model
so the framework does not need to be redesigned when needed.

---

### 4.3 Filter representation

```rust
pub enum FilterTarget {
    Feature,               // direct predicate on feature attributes
    Sequence,              // Type A chain -- resolves sequence_ids first
    Window {
        /// Selects which window resolution to query.
        /// Maps to a `primary_type` prefix: `1_000_000` → `window_1m`,
        /// `100_000` → `window_100k`, etc.  `None` = auto-detect coarsest.
        size: Option<u64>,
        /// Minimum fraction of feature length that must lie inside the window.
        /// `None` means ANY overlap counts (current default; future phase).
        overlap_threshold: Option<f64>,
    },
    FeatureType {          // Type C chain -- resolves name/id set first
        feature_type: String,
        assembly_id:  Option<String>,
    },
}

pub struct AttributeFilter {
    pub field:    String,
    pub operator: Operator,      // Eq, Ne, Lt, Lte, Gt, Gte, In, GteCount
    pub value:    FilterValue,   // Scalar(f64), Text(String), List(Vec<String>)
    pub target:   FilterTarget,
}
```

---

## 5. Region Simplification

Windows and regions are distinct reduction strategies:

| | Windows | Regions |
|---|---|---|
| Interval size | Fixed (user-specified bp) | Variable (driven by feature pattern) |
| Defined by | Genome coordinates | Feature attribute values |
| Empty intervals | Possible | Never — only covers features |
| Primary use | Density heatmaps | Synteny painting, merian colouring, circos arcs |

Regions collapse adjacent features sharing the same categorical attribute value into
contiguous intervals. They are computed server-side from the full feature result set
after all filters are applied — no additional ES query is needed.

### `positional_yaml` additions

```yaml
regions:
  cat:           merian_unit   # attribute to group by; or a custom name→cat mapping file
  bounds:        feature_ends  # feature_ends | midpoints
  min_features:  1             # minimum features per region (default 1)
  max_expansion: null          # optional: max bp a region can expand beyond feature_ends
                               # e.g. 100000 or 1000000; null = unlimited
```

### Bounds modes

| `bounds` | Region `[start, end]` |
|---|---|
| `feature_ends` (default) | `[first_feat.start, last_feat.end]` in each run |
| `midpoints` | Boundaries at midpoint between adjacent feature ends and starts; first region starts at first feature start, last region ends at last feature end (i.e. chromosome extension is implicit when the caller wants it) |

`chromosome`-spanning mode is achieved by `midpoints` combined with the client extending
the first and last region to 0 and `sequence_length` respectively — this is a display
concern, not a server concern. The server always returns exact feature-driven boundaries.

### `max_expansion` (buffer / cap)

When `max_expansion` is set, a region boundary produced by the `midpoints` algorithm
cannot be more than `max_expansion` bp away from the nearest feature edge. Prevents
a large gap between two feature-dense regions from producing an arbitrarily wide
background region.

```
boundary = min(midpoint, last_feat.end + max_expansion)
         = max(midpoint, next_feat.start - max_expansion)
```

This is a hard per-boundary cap, not a per-region cap.

### Multi-assembly region computation

Regions are computed from the **union of features across all assemblies in the request**,
not per-assembly. This ensures that a region boundary reflects shared biological
breakpoints visible in all assemblies — for example, an inversion boundary seen in a
ribbon plot will generate a region split even if one assembly has no cat-change at that
position. The algorithm assigns each sequence a consensus region set derived from the
union of cat-values at each position.

Practically: the server collects all features from all assemblies, projects them onto
the reference assembly coordinate space (using the same position-mapping computed for
the ribbon layout), computes regions from the merged projection, then maps the resulting
region boundaries back to each assembly's coordinate space for the response.

### Algorithm (pseudocode)

```
Input:
  features:      sorted by (assembly_id, sequence_id, start)
  cat_field:     attribute key to group by; or name_to_cat: { feature_name -> cat }
  bounds:        feature_ends | midpoints
  max_expansion: Option<u64>   # None = unlimited

# Resolve cat for each feature
for feat in features:
    if name_to_cat:
        feat.cat = name_to_cat.get(feat.name) ?? "other"
    else:
        feat.cat = feat.attrs.get(cat_field) ?? "other"

# Compute per-assembly regions on the merged feature set
for assembly_id, seq_id in sorted(unique (assembly_id, sequence_id) pairs):
    feats = features_for[assembly_id][seq_id]  # sorted by start
    active = None
    regions = []

    for feat in feats:
        if active is None:
            active = Region(assembly_id, seq_id, feat.start, feat.end, feat.cat, count=1)

        elif feat.cat == active.cat:
            active.end = feat.end
            active.count += 1

        else:
            if bounds == midpoints:
                raw_boundary = (active.last_end + feat.start) / 2
                if max_expansion:
                    boundary = clamp(raw_boundary,
                                     active.last_end + max_expansion,
                                     feat.start - max_expansion)
                else:
                    boundary = raw_boundary
                active.end = boundary
                next_start = boundary
            else:  # feature_ends
                next_start = feat.start

            regions.append(active)
            active = Region(assembly_id, seq_id, next_start, feat.end, feat.cat, count=1)

        active.last_end = feat.end

    if active:
        regions.append(active)
```

### Output

Regions are a sibling key in the positional response, present when `regions.cat` is set:

```json
{
  "points":         [...],
  "windowedPoints": [...],
  "regions": [
    {
      "sequenceId":   "LR989896.1",
      "assemblyId":   "GCA_905147045.1",
      "start":        0,
      "end":          3182000,
      "catValue":     "MZ-12",
      "featureCount": 4,
      "xOffset":      1234567
    }
  ],
  "histograms":   {...},
  "zDomain":      [0, 58],
  "assemblyPair": [...]
}
```

---

## 6. Many-to-Many Feature Mapping

### Motivation

The current implementation assumes 1:1 or 1:few groups. Real use cases require M:N:

| Feature type | Mapping | Example |
|---|---|---|
| BUSCO gene | 1:1 (Complete) or 1:2 (Duplicated) | Current — implemented |
| Orthogroup member | M:N | 3 copies in assembly A, 2 in assembly B |
| Domain annotation | M:N | Same Pfam domain in many genes across two genomes |
| Repeat element | M:N | Same transposable element family, many loci |
| Synteny block | M:N | Whole-genome duplication, segmental duplication |

### Output schema for M:N groups

When any group identifier has more than one instance per assembly, the response adds a
`connections` key. 1:1 groups remain in `points`.

```json
{
  "connections": [
    {
      "group":    "OG0001234",
      "xCoords":  [12345678, 23456789],
      "yCoords":  [34567890, 45678901],
      "xSeqIds":  ["LR989896.1", "LR989897.1"],
      "ySeqIds":  ["LR761662.1", "LR761663.1"],
      "xStrands": [1, -1],
      "yStrands": [1, 1],
      "catValue": "OrthoFinder",
      "truncated": false
    }
  ]
}
```

The client renders one line (or arc) per (xi, yj) pair in `xCoords x yCoords`.
A `max_connections_per_group` cap (default 25) limits explosion for large groups.
`truncated: true` is set when the cap applies.

### Circos output

Circos is a display-side variant of the oxford/ribbon layout: sequences arranged in a
circle with arcs connecting feature positions. Key differences from oxford:

- Supports within-assembly connections (same-genome repeats, inversions).
- Arc weight represents connection count for M:N groups.
- Arc colour is `catValue`.
- Sequences from multiple assemblies are arranged on the same circle, separated by gaps.

`report: circos` is added alongside `oxford`, `ribbon`, and `painting`.

```json
{
  "circos": {
    "sequences": [
      {
        "sequenceId":  "LR989896.1",
        "assemblyId":  "GCA_905147045.1",
        "length":      15533955,
        "angleStart":  0.0,
        "angleEnd":    22.7
      }
    ],
    "arcs": [
      {
        "group":    "OG0001234",
        "from":     { "sequenceId": "LR989896.1", "pos": 12345678, "assemblyId": "GCA_905147045.1" },
        "to":       { "sequenceId": "LR761662.1", "pos": 34567890, "assemblyId": "GCA_902806685.2" },
        "catValue": "OrthoFinder",
        "weight":   1
      }
    ]
  }
}
```

The sequence layout algorithm (sorting, offset computation) is shared through the
existing `layout.rs`. Only angle computation and the output serialiser are
circos-specific.

---

## 7. Simplified `feature_query.rs`

With v1 support removed, the query module becomes substantially smaller.

### Query builders (all top-level fields)

```rust
pub fn assembly_id_filter(ids: &[String]) -> Value;
pub fn primary_type_filter(feature_type: &str) -> Value;
pub fn top_level_range(field: &str, op: RangeOp, value: f64) -> Value;
pub fn sequence_id_filter(ids: &[String]) -> Value;
pub fn container_id_filter(ids: &[String]) -> Value;

// Still needed for attributes-only fields (gc, score, status, ...)
pub fn nested_attr_term(key: &str, value: &str) -> Value;
pub fn nested_attr_range(key: &str, op: RangeOp, value: f64) -> Value;
```

### Record parsing

```rust
/// Extract FeatureRecord from a v2 ES hit using top-level fields.
/// Attribute-only fields (cat_value, etc.) still read from the attributes array.
pub fn parse_flat_hit(hit: &Value, group_by: &str, cat_field: Option<&str>) -> Option<FeatureRecord>;
```

### Chain query executors

```rust
/// Type A: resolve sequence_ids matching a nested attribute filter on toplevel features.
pub async fn resolve_sequence_ids(
    client: &reqwest::Client, es_base: &str, index: &str,
    assembly_ids: &[String], filter: &AttributeFilter,
) -> Result<Vec<String>, String>;

/// Type B: resolve container_ids by querying window features in the feature index.
/// Window docs have primary_type = window_<size> (e.g. window_1m, window_100k).
pub async fn resolve_window_ids(
    client: &reqwest::Client, es_base: &str, feature_index: &str,
    assembly_ids: &[String], filter: &AttributeFilter,
    window_size: Option<u64>,
) -> Result<Vec<String>, String>;
```

### Removed from codebase

- `nested_attr_terms` (multi-value two-stage sequence ID workaround)
- `extract_attributes` (used only for v1 query construction)
- `resolve_sequence_ids_from_filters` in `positional.rs`

---

## 8. Test Fixture Generation

Fixture-first development is essential because the v2 index does not yet exist.

### Fixture sets

| File | Contents |
|---|---|
| `tests/fixtures/feature_v1_GCA_905147045.json` | Raw v1 ES hits — 200 busco genes + 5 toplevel sequences |
| `tests/fixtures/feature_v2_GCA_905147045.json` | Same data in v2 flat-field structure, including v2-mapped window docs |

No separate window fixture is needed: windows are indexed as features (`primary_type =
window_1m`) in the same feature index. The v2 transformer generates synthetic window
docs from the sequence length data already present in the fixture.

### Extraction script

```bash
scripts/extract_feature_fixtures.sh GCA_905147045.1 tests/fixtures/
```

Queries `localhost:9200` for 5 toplevel sequences and up to 200 busco-gene features.

### V1 to V2 transformer

`src/bin/transform_v1_fixture.rs` reads a v1 fixture and emits a v2 fixture using the
migration table in section 2. This binary also serves as the canonical reference
specification for indexer developers.

### Test pattern

```rust
#[tokio::test]
async fn test_v2_parse_matches_v1_parse() {
    let v1 = load_fixture("feature_v1_GCA_905147045.json");
    let v2 = load_fixture("feature_v2_GCA_905147045.json");
    assert_eq!(parse_flat_hit_batch(&v2), parse_nested_attr_batch(&v1));
}
```

---

## 9. `positional_yaml` Reference (Updated)

```yaml
report:       oxford         # oxford | ribbon | painting | circos
group_by:     metazoa_odb10-busco-gene
assemblies:
  - GCA_905147045.1
  - GCA_902806685.2
reorient:     true
max_features: 10000
max_connections_per_group: 25

filter:
  # Direct top-level field (single-pass)
  - field:    length
    operator: gte
    value:    "500"
    target:   feature

  # sequence_length is top-level on every feature doc (single-pass)
  - field:    sequence_length
    operator: gte
    value:    "10000000"
    target:   feature

  # Type A chain — sequence gc from toplevel attributes
  - field:    gc
    operator: gt
    value:    "0.45"
    target:   sequence

  # Type B chain — window stats (windows are features in same index, primary_type=window_1m)
  - field:    coverage
    operator: gt
    value:    "10"
    target:   window
    window_size: 1000000      # maps to primary_type window_1m; omit to auto-detect
    # overlap_threshold: 0.5  # future: require >=50% of feature inside window

  # Attribute filter (nested) — feature status
  - field:    status
    operator: eq
    value:    "Complete"
    target:   feature

regions:
  cat:          merian_unit       # attribute key, or omit if name_to_cat is used
  # name_to_cat:                  # alternative: explicit feature-name → category map
  #   OG0001234: Clade-A
  #   OG0001235: Clade-B
  bounds:        feature_ends    # feature_ends | midpoints
  min_features:  1
  max_expansion: 500000          # optional: cap boundary expansion at 500 kbp
```

### Response shape

```json
{
  "status":  { "success": true },
  "report": {
    "type":           "oxford",
    "assemblies":     ["GCA_905147045.1", "GCA_902806685.2"],
    "points":         [...],
    "connections":    [...],
    "windowedPoints": [...],
    "regions":        [...],
    "histograms":     {...},
    "zDomain":        [0, 58],
    "assemblyPair":   [...]
  }
}
```

---

## 10. External Repository Changes

### `genomehubs-index`

1. Add promoted top-level fields to the feature index mapping template.
2. Populate `sequence_length` for every feature by enriching from the parent toplevel
   sequence during indexing.
3. Populate `container_ids` — for each feature compute overlapping window bins at each
   configured resolution, and write the prefixed IDs
   (`win_1m:<assembly>:<seq_id>:<bin>`, `win_100k:…`, etc.). Initial policy is
   ANY overlap (feature touches window interval); the indexer need not implement
   threshold logic until the `overlap_threshold` parameter is surfaced in Phase 19+.
4. Write window documents as features with `primary_type = window_1m` (or
   `window_100k`, etc.) in the same feature index. No new index is required. Window
   docs carry aggregate stats (`gc`, `coverage`, `repeat_density`) in the standard
   `attributes` nested array. Sequences already exist as `primary_type = toplevel`
   features and require no change.
5. Set `primary_type` to a single canonical value (most specific feature type string).
6. Continue writing `attributes` with all existing fields unchanged.

### Window resolution convention

| Window size | `primary_type` | `container_ids` prefix |
|---|---|---|
| 1 Mbp | `window_1m` | `win_1m:` |
| 100 kbp | `window_100k` | `win_100k:` |
| 500 kbp | `window_500k` | `win_500k:` |

The specific resolutions indexed are a site/pipeline configuration choice. The API
auto-detects available resolutions from the `primary_type` values present in the index
(queried on startup with a `terms` aggregation).

### Transition

Sites with v1 feature indices use the v2 API without positional support until their
index is rebuilt. `/api/v3/status` makes the limitation clear. No positional v1
fallback code exists in this repo.

---

## 11. Potential Problems

| Problem | Likelihood | Mitigation |
|---------|------------|------------|
| Indexer changes blocked on external repo velocity | High | Fixture-based development (section 8) fully decouples server work |
| `container_ids` bloat for features spanning many windows | Medium | 1 Mbp windows => typical gene overlaps 1-3; cap at 10 per feature |
| Window index absent at some sites | Medium | `target: window` returns structured error; other targets unaffected |
| M:N connection explosion in large orthogroups | Medium | `max_connections_per_group` cap; `truncated: true` on affected connections |
| Region algorithm over-splits fragmented genomes | Low | `min_features` parameter; server hard cap of 10,000 regions per response |
| `primary_type` must be single keyword, not array | Medium | Indexer picks canonical primary type; detection falls back to nested `feature_type` if absent |
| `sequence_length` absent for some feature types | Low | Absence treated as null with clear error; any feature with start/end can have it enriched |
| Circos with thousands of short sequences | Low | Server caps to top-N sequences by length; warns when truncated |

---

## 12. Implementation Sequence

### Step A — Fixtures and detection (no external dependency)
1. `scripts/extract_feature_fixtures.sh` — extract v1 fixtures from live `localhost:9200`
2. `src/bin/transform_v1_fixture.rs` — promote fields to v2 structure; serves as indexer spec
3. `detect_feature_index_version()` in metadata module
4. Endpoint gate in `positional.rs` handler with structured error

### Step B — Simplified query path for the v3 positional endpoint
1. Rewrite `feature_query.rs` — flat top-level field builders; remove v1 workarounds
2. Implement `parse_flat_hit()`
3. Fixture-driven tests: flat-field parse produces correct `FeatureRecord` vecs

### Step C — Chain query framework
1. `AttributeFilter` struct + `FilterTarget` enum
2. `resolve_sequence_ids()` — Type A
3. `resolve_window_ids()` — Type B (gated on window index availability)
4. Integration wiring in handler
5. Tests against synthetic fixtures

### Step D — Region simplification
1. `region.rs` — `RegionRecord`, `RegionBounds`, `compute_regions()`
2. Wire into oxford/ribbon/painting output when `regions.cat` is set
3. Unit tests: both bounds modes, `max_expansion` capping, multi-assembly merge, edge cases (single feature, all-same-cat, empty sequence)

### Step E — M:N and circos
1. `connections` output mode — detect M:N in results, build connection schema
2. `max_connections_per_group` cap + truncation flag
3. `report: circos` — angle computation + output serialiser sharing `layout.rs`

### Step F — External repo coordination (separate milestone)
1. Open issue in `genomehubs-index` with field spec from section 2 + transformer output
2. Coordinate window index pipeline design
3. Once v2 index available: live end-to-end test

---

## 13. Files to Create / Modify

| File | Action |
|---|---|
| `crates/genomehubs-api/src/report/positional/feature_query.rs` | Rewrite — v2 builders only, remove v1 code |
| `crates/genomehubs-api/src/report/positional/region.rs` | New — region algorithm |
| `crates/genomehubs-api/src/report/positional/mod.rs` | Add `pub mod region` |
| `crates/genomehubs-api/src/routes/positional.rs` | Add gate, regions, filter dispatch, M:N |
| `crates/genomehubs-api/src/metadata.rs` | Add `detect_feature_index_version()` |
| `src/bin/transform_v1_fixture.rs` | New — v1->v2 fixture transformer / indexer spec |
| `scripts/extract_feature_fixtures.sh` | New — live fixture extraction |
| `tests/fixtures/feature_v1_GCA_905147045.json` | New — v1 raw fixture |
| `tests/fixtures/feature_v2_GCA_905147045.json` | New — v2 transformed fixture (includes synthetic window docs) |
| `docs/planning/phases/phase-11-positional-family.md` | Updated ✅ — wiring status table, removed DEFERRED |

---

## 14. Next Steps Towards Full Implementation

This section maps the gap between "server is live on v1 index" and "fully wired
positional endpoint across CLI and SDK" into ordered, independently completable tasks.

### A — Endpoint gate + fixture harness (no external dep, high value)

These can be done immediately against the live v1 index at `localhost:9200`:

1. `scripts/extract_feature_fixtures.sh` — pull 200 busco-gene docs + 5 toplevel docs.
2. `src/bin/transform_v1_fixture.rs` — promote fields to v2 structure + generate
   synthetic `window_1m` docs from sequence lengths. This binary is also the canonical
   spec for the `genomehubs-index` developers.
3. `detect_feature_index_version()` in `metadata.rs` — checks for `start` +
   `sequence_length` at top level.
4. Hard gate in `positional.rs` (replace the two-stage workaround with a clear error).
5. Fixture-based unit tests for the flat-field parse path.

### B — v3 positional query path in `feature_query.rs`

Depends on: Step A fixtures.

1. Replace v1 nested builders with flat top-level field builders.
2. `parse_flat_hit()` — read `sequence_id`, `start`, `end`, `strand` from
   promoted top-level source fields; `group_value` and `cat_value` remain
   in nested `attributes` and are read via `extract_attributes`.
3. Fixture-driven round-trip test.

### C — `PositionalSpec` extension + SDK sync

No index changes needed. These spec additions drive all subsequent features:

1. Add `filter: Vec<AttributeFilter>` to `PositionalSpec` in
   `crates/genomehubs-query/src/report/positional.rs`.
2. Add `regions: Option<RegionsSpec>` and `max_connections_per_group: Option<usize>`.
3. Add `Circos` to `PositionalReportType`.
4. Sync Python `positional()` signature in `query.py` and `query.py.tera` to expose
   `filter`, `regions`, `max_connections_per_group` parameters.
5. Sync JS and R templates.

### D — CLI subcommand wiring + local file custom category mapping

No index changes needed for CLI wiring. Custom category mapping is also relevant here:

- The `regions.name_to_cat` map works identically for remote and local-file workflows.
  A user supplying a BUSCO `full_table.tsv` can also supply a JSON/YAML file mapping
  BUSCO gene names to colour categories, and the same `compute_regions()` code path
  handles it — the cat resolution step (lines `if name_to_cat: ...` in the pseudocode)
  runs before the region algorithm regardless of data source.
- The SDK `positional()` method accepts `regions` as a dict; the Python/R/JS wrappers
  should document `name_to_cat` as an alternative to `cat`.

No index changes needed for the main wiring tasks:

1. Add `positional` / `oxford` / `ribbon` / `painting` to the CLI generator YAML config.
2. These become generated subcommands that call `POST /api/v3/positional` directly.
3. Validate with `goat-cli oxford --group-by busco_gene --assemblies GCA_X,GCA_Y`.

### E — Chain query framework + regions (needs feature index v2 or fixtures)

Depends on Step B:

1. `AttributeFilter` struct + `FilterTarget` enum (section 4.3).
2. `resolve_sequence_ids()` — Type A chain.
3. `resolve_window_ids()` — Type B chain; queries `primary_type = window_1m` in the
   same feature index.
4. `region.rs` — `compute_regions()` algorithm (section 5).
5. Wire all into handler dispatch.
6. Tests against feature index v2 fixtures with synthetic window docs.

### F — M:N + circos output

Depends on Step E:

1. Detect M:N in result set, build `connections` key.
2. `max_connections_per_group` cap + `truncated` flag.
3. Angle computation for `report: circos`; serialiser sharing `layout.rs`.

### G — External repo coordination (separate milestone)

1. Open issue in `genomehubs-index` with the field spec from section 2 and the
   transformer output from Step A as the canonical reference.
2. Coordinate window pipeline: window docs as `primary_type = window_1m` features,
   multi-resolution `container_ids` population, ANY-overlap policy.
3. Once a feature index v2 is available at a test site: live end-to-end test of the
   gated endpoint.

### Priority ordering

| Priority | Task | Rationale |
|---|---|---|
| 1 | Step A — fixtures + gate | Unblocks everything; replaces two-stage workaround |
| 2 | Step C — spec extension | Needed before SDK and CLI wiring can proceed |
| 3 | Step D — CLI subcommand | Closes the SDK ↔ CLI gap for current users |
| 4 | Step B — v3 query path | Required for chain queries; blocked on feature index v2 |
| 5 | Step E — chain queries + regions | High user value; blocked on feature index v2 |
| 6 | Step F — M:N + circos | Important for orthogroup / repeat use cases |
| 7 | Step G — external repo | Long lead time; start coordination early |
