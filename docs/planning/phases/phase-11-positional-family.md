# Phase 11: Positional Report Family (Oxford / Ribbon / Painting)

**Depends on:** Phase 5 (es_client, bounds, pipeline), Phase 6 (report route pattern)
**Blocks:** Phase 13 (hybrid mode extends this endpoint)
**Estimated scope:** 1 new endpoint, ~4 new Rust files, SDK method additions

---

## Goal

Implement `POST /api/v3/positional` — a new endpoint handling the full "positional"
family of reports, which compare or display genomic marker positions across one or more
assemblies.

| Sub-type   | Assemblies | Primary use                                       |
| ---------- | ---------- | ------------------------------------------------- |
| `oxford`   | 2          | Compare shared marker positions (Oxford dot-plot) |
| `ribbon`   | N ≥ 2      | Multi-assembly ribbon/synteny diagram             |
| `painting` | 1          | Single assembly chromosome colour map             |

All three share the same data ingestion pipeline (ES feature search) and layout
infrastructure (sequence ordering, offset computation, optional windowing). They differ
only in output shape.

**Relationship to v2:** The v2 `oxford.js` is the spiritual predecessor of this
endpoint. Key improvements in v3:

1. Generalised to N assemblies instead of hard-coded 2.
2. Server-side regional windowing reduces payload for large assemblies.
3. Explicit `positional_yaml` config replaces bespoke URL parameters.
4. Clean separation from scatter plot infrastructure.
5. Local file hybrid deferred to Phase 13 (client-side SDK concern).

---

## `positional_yaml` Format

```yaml
report: oxford # oxford | ribbon | painting
group_by: busco_gene # field to use as shared marker identifier
assemblies: # explicit assembly IDs; derived from query if absent
  - GCA_000001405.28
  - GCA_000003625.1
window_size: null # null = individual positions; integer = bp per window
reorient: true # auto-orient comparison sequences (default: true)
max_features: 10000 # max individual features to fetch (hard cap; default 10000)
cat: null # optional category field for colour
cat_opts: ";;5+" # category options (same format as report_yaml cat_opts)
```

### Field notes

- `assemblies`: If absent, the server derives assembly IDs from the query results.
  Explicit IDs are preferred for reproducibility.
- `window_size`: When set, positions are aggregated into regional intervals server-side
  in Rust after the ES query. This is O(n) in the number of features and requires no
  additional ES queries. For assemblies with > 1000 BUSCO genes, `window_size: 1000000`
  reduces points by ~10–50×.
- `reorient`: Uses the v2 algorithm: compute median ref-position for each comparison
  sequence, then use linear regression on the scatter of (ref_pos, cmp_pos) pairs to
  determine strand orientation. Disable if assemblies are pre-oriented.

---

## v2 → v3 Algorithm Mapping

The v2 `oxford.js` algorithm is preserved in Rust with the following changes:

| v2 step                              | v3 equivalent                                                  | Notes                                                                        |
| ------------------------------------ | -------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| `parseAssemblies(query)`             | Extracted from `positional_yaml.assemblies`                    | Explicit instead of query-parsed                                             |
| `getSequenceLengths(...)`            | ES search for `feature_type=topLevel`                          | Unchanged                                                                    |
| `getResults({size: count})`          | ES search with `size = min(count, max_features)`               | Adds hard cap                                                                |
| Median scoring for sequence ordering | `crate::report::positional::layout::order_sequences_by_median` | Same algorithm                                                               |
| Linear regression for orientation    | `crate::report::positional::layout::orient_sequence`           | Same algorithm, `simple-statistics` → manual Rust impl or `linregress` crate |
| Offset accumulation                  | `crate::report::positional::layout::compute_offsets`           | Unchanged                                                                    |
| Optional windowing                   | `crate::report::positional::window::apply_window`              | **New in v3**                                                                |
| Output: `rawData[cat][...]`          | `PositionalResponse.points`                                    | Renamed, structured                                                          |

---

## Files to Create

```
crates/genomehubs-api/src/routes/positional.rs      — POST /api/v3/positional handler
crates/genomehubs-api/src/report/positional/
    mod.rs      — re-exports
    layout.rs   — sequence ordering, orientation, offset computation
    window.rs   — regional windowing (group individual positions into intervals)
    painter.rs  — painting-mode output shaping
crates/genomehubs-query/src/report/positional.rs    — PositionalSpec + request types
```

## Files to Modify

| File                                            | Change                                                    |
| ----------------------------------------------- | --------------------------------------------------------- |
| `crates/genomehubs-api/src/routes/mod.rs`       | `pub mod positional;`                                     |
| `crates/genomehubs-api/src/report/mod.rs`       | `pub mod positional;`                                     |
| `crates/genomehubs-api/src/main.rs`             | Register route + OpenAPI                                  |
| `crates/genomehubs-query/src/report/mod.rs`     | `pub mod positional; pub use positional::PositionalSpec;` |
| `crates/genomehubs-query/src/report/builder.rs` | `positional()` method on `ReportBuilder`                  |
| `python/cli_generator/query.py`                 | `oxford()`, `ribbon()`, `painting()` methods              |
| `templates/python/query.py.tera`                | Mirror                                                    |
| `templates/js/query.js`                         | Same                                                      |
| `templates/r/query.R`                           | Same                                                      |

---

## `PositionalSpec` Type (`crates/genomehubs-query/src/report/positional.rs`)

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PositionalReportType {
    Oxford,
    Ribbon,
    Painting,
}

/// Configuration for a positional (oxford / ribbon / painting) report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionalSpec {
    pub report: PositionalReportType,
    /// Field used as shared marker identifier (e.g. `busco_gene`).
    pub group_by: String,
    /// Explicit assembly IDs. Derived from query results if absent.
    pub assemblies: Option<Vec<String>>,
    /// Window size in bp for regional grouping. `None` = individual positions.
    pub window_size: Option<u64>,
    /// Auto-orient comparison sequences relative to the reference.
    #[serde(default = "default_true")]
    pub reorient: bool,
    /// Maximum features to fetch from ES.
    #[serde(default = "default_max_features")]
    pub max_features: usize,
    pub cat: Option<String>,
    pub cat_opts: Option<String>,
}

fn default_true() -> bool { true }
fn default_max_features() -> usize { 10_000 }
```

---

## Data Model: Feature Records

The ES query fetches features with these fields:

```
assembly_id, sequence_id, start, end, strand, <group_by field>, [cat field]
```

The ES query targets the `feature` index (not `taxon` or `assembly`) with:

- `feature_type` filtered to the relevant type (default: inferred from `group_by`)
- `assembly_id` filtered to `positional_yaml.assemblies`
- The base query from `query_yaml` applied

---

## Sequence Layout Algorithm

Extracted from v2 `oxford.js` into clean Rust functions:

### `layout::order_sequences_by_median`

```
Input:  features for all assemblies, sorted sequences of assembly[0]
Output: sorted sequences for assembly[i] with score (median ref position)

For each sequence_id in assembly[i]:
  - Collect all features belonging to shared groups that exist in assembly[0]
  - For each such feature, its "score" is the position of its group in assembly[0]
    (= start + cumulative offset of its assembly[0] sequence)
  - median(scores) is the sort key for this sequence
```

### `layout::orient_sequence`

```
Input:  pairs of (ref_position, cmp_position) for features in a comparison sequence
Output: orientation (+1 or -1)

Fit a linear regression y = mx + b through the scatter of position pairs.
If m >= 0, orientation = +1; otherwise -1.
```

When `reorient = false`, all sequences get orientation +1 (no flipping).

### `layout::compute_offsets`

```
Input:  sorted sequences with lengths and orientations
Output: cumulative offset table (sequence_id → offset, buckets array, labels array)

For each sequence in sorted order:
  - If orientation = -1, add length to current offset (flipped)
  - Record offset as bucket boundary
  - Advance offset by length
```

---

## Regional Windowing (`window.rs`)

When `window_size` is set, after collecting all raw feature positions, bin them:

```
For each assembly:
  For each sequence_id:
    Divide [0, sequence_length) into windows of size window_size
    For each window [w_start, w_end):
      features_in_window = features where start >= w_start AND start < w_end
      If non-empty:
        emit WindowedPoint {
          seq: sequence_id,
          w_start, w_end,
          count: features_in_window.len(),
          cats: HashMap<cat_key, count>  // category breakdown
        }
```

For Oxford/Ribbon output: two windowed point sets are cross-joined by group membership.
For Painting output: single assembly windowed points with category breakdown.

This is pure Rust computation (no additional ES queries) and runs in O(n features).

---

## Response Format

### Oxford (2 assemblies)

```json
{
  "status": { "success": true, "hits": 1423, "took": 45 },
  "report": {
    "type": "oxford",
    "assemblies": {
      "GCA_000001405.28": {
        "label": "GRCh38",
        "sequences": [
          { "id": "chr1", "length": 248956422, "offset": 0 },
          { "id": "chr2", "length": 242193529, "offset": 248956422 }
        ],
        "domain": [0, 3234830000],
        "buckets": [0, 248956422, 491149951]
      },
      "GCA_000003625.1": { "..." }
    },
    "points": [
      {
        "x": 1234567, "x2": 1234800,
        "y": 9876543, "y2": 9877100,
        "group": "10at7742",
        "cat": "vertebrata_odb10",
        "strand": 1, "y_strand": -1
      }
    ],
    "windowed_points": null,
    "cat": "busco_lineage",
    "cats": [{ "key": "vertebrata_odb10", "label": "Vertebrata" }],
    "z_domain": [0, 12]
  }
}
```

`points` contains individual positions when `window_size = null`.
`windowed_points` contains regional intervals when `window_size` is set.
Exactly one of these is non-null.

### Ribbon (N assemblies)

Same structure as Oxford. `assemblies` contains N entries. `points` has entries for
each pairwise combination of (assembly[0], assembly[i]) for i > 0. Each point has
`assembly_pair: ["GCA_A", "GCA_B"]` to identify which pair it belongs to.

### Painting (1 assembly)

```json
{
  "report": {
    "type": "painting",
    "assemblies": { "GCA_000001405.28": { ... } },
    "segments": [
      {
        "sequence_id": "chr1", "start": 0, "end": 1000000,
        "cat": "complete", "count": 42
      }
    ]
  }
}
```

---

## SDK Methods

### Python / R / JS

```python
# Python - Oxford
result = (
    QueryBuilder()
    .index("feature")
    .taxa(["Homo sapiens"])
    .oxford(
        group_by="busco_gene",
        assemblies=["GCA_000001405.28", "GCA_000003625.1"],
        window_size=1_000_000,
        reorient=True,
    )
    .fetch()
)

# Ribbon (N assemblies)
result = (
    QueryBuilder()
    .index("feature")
    .taxa(["Homo sapiens"])
    .ribbon(
        group_by="busco_gene",
        assemblies=["GCA_A", "GCA_B", "GCA_C"],
        window_size=1_000_000,
    )
    .fetch()
)

# Painting (single assembly)
result = (
    QueryBuilder()
    .index("feature")
    .taxa(["Homo sapiens"])
    .painting(
        group_by="busco_gene",
        assembly="GCA_000001405.28",
        cat="busco_status",
        window_size=500_000,
    )
    .fetch()
)
```

`oxford()`, `ribbon()`, and `painting()` are convenience wrappers around a single
`positional()` method on `ReportBuilder` that sets the `report:` field in
`positional_yaml`.

---

## Notes on N-Assembly Generalisation (Ribbon)

The v2 algorithm sorts comparison sequences by their median reference position. For
ribbon plots with N > 2 assemblies:

1. Assembly 0 is always the reference. Its sequence order is fixed (sort by length,
   largest first — same as v2).
2. For each assembly i > 0, independently sort its sequences by median ref position
   and determine orientation vs. assembly 0.
3. Output: for each pair (assembly 0, assembly i), the same Oxford scatter data.
   The browser renders ribbons connecting the cross-assembly scatter into contiguous
   bands.

There is no requirement to sort assemblies 1..N relative to each other; only relative
to assembly 0.

---

## Testing

- Unit test: `order_sequences_by_median` with known input produces expected sort order
- Unit test: `orient_sequence` with positive slope returns +1; negative returns -1
- Unit test: `compute_offsets` accumulates correctly for both orientations
- Unit test: `apply_window` bins features into non-overlapping windows
- Proptest: windowing with arbitrary `window_size` and feature positions produces
  non-overlapping intervals covering all features
- API integration test: `POST /api/v3/positional` with oxford config returns 2 assemblies
- API integration test: painting config returns `segments` array
- API integration test: `window_size: 1000000` returns `windowed_points` non-null and
  `points` null
