# Phase 13: Hybrid Local + Remote Reports

**Depends on:** Phase 12 (PlotSpec exists)
**Blocks:** nothing downstream
**Estimated scope:** SDK-side data loading helpers, `local_plot_spec()` Rust function, `genomehubs local-report` CLI subcommand; no new API endpoints

> **Note:** The positional-family hybrid workflow (oxford/ribbon/painting against local BUSCO
> files) is deferred to [phase-XX-positional-hybrid.md](phase-XX-positional-hybrid.md)
> because it depends on Phase 11 (the positional endpoint) which does not apply to GoaT.
> This phase covers hybrid workflows that **do not** require the positional endpoint.

---

## Goal

Enable workflows where the user has data that is not in the genomehubs database and
wants to combine it with remote API results, or render a complete report from local
data. Three scenarios are in scope:

1. **Local render of remote data** — fetch a report from the API, receive a `PlotSpec`,
   render it locally via `genomehubs plot` (CLI) or `plotSpecToVegaLite()` (JS/Python/R).
   Phase 12 enables this; Phase 13 adds `--display` / `--include-plot-spec` CLI flags
   and the `save_plot_spec()` helper.

2. **Local report from a data file** — user has a TSV/CSV with numeric or categorical
   columns and wants to generate a histogram, scatter, or bar chart without an API call.
   Rust reads the file, builds a `PlotSpec` directly (no ES query).

3. **Augmented remote report** — user fetches a remote report then adds locally
   computed annotations (e.g. custom category labels, derived metrics) before rendering.
   The SDK provides a `merge_annotations()` helper that patches `plot_spec.data`.

No new API endpoint is required.

---

## Supported Local File Formats
---

## Scenario 1: Local Render of Remote Data

Phase 12 enables fetching a `PlotSpec` from the API. Phase 13 adds the CLI flags
and helpers to complete the render pipeline.

### CLI additions

```bash
# Fetch a report and render it to SVG in one pipeline
genomehubs report \
  --query '{"index":"taxon","query":"tax_tree(Mammalia)"}' \
  --report '{"report":"histogram","x":"genome_size"}' \
  --display '{"title":"Genome size in Mammalia","width":900}' \
  --params '{"include_plot_spec":true}' \
  | genomehubs plot --output genome_size.svg

# Save the full response for later
genomehubs report ... | tee response.json | genomehubs plot --output plot.svg

# Plot from a saved response
genomehubs plot --input response.json --output plot.svg --format png
```

The `genomehubs report` CLI subcommand gains:
- `--display` flag (string or @file path) → sets `display` in the request body
- `--include-plot-spec` flag → sets `include_plot_spec: true` in `params`
- `--display` implies `--include-plot-spec`

The `genomehubs plot` subcommand (introduced in Phase 12) gains:
- `--format svg|png` (default: `svg`)
- `--width` / `--height` overrides (override `display.width`/`display.height`)

### SDK helpers

```python
# Python — save PlotSpec for CLI rendering
result = qb.report(rb, include_plot_spec=True)
spec = result["plot_spec"]

from cli_generator import plot_spec_to_vega_lite
vl_json = plot_spec_to_vega_lite(spec)
# → pass to altair.Chart.from_dict(vl_json)

import json
with open("spec.json", "w") as f:
    json.dump(spec, f)
# $ genomehubs plot --input spec.json --output plot.svg
```

```javascript
// JavaScript — render to Vega-Lite
import { QueryBuilder, plotSpecToVegaLite } from "./query.js";
const result = await qb.report(rb, { includePlotSpec: true });
const vlSpec = plotSpecToVegaLite(result.plot_spec);
// → embed in vegaEmbed("#chart", vlSpec)
```

---

## Scenario 2: Local Report from a Data File

Build a `PlotSpec` entirely from a local TSV/CSV file, with no API call.

### Supported input formats

- **TSV** (tab-separated values, optional header row)
- **CSV** (comma-separated values, optional header row)
- Column types are auto-detected (numeric → float, else keyword)

### Supported report types from local data

| Report type  | Minimum columns required                     |
| ------------ | -------------------------------------------- |
| `histogram`  | One numeric column (`x`)                     |
| `scatter`    | Two numeric columns (`x`, `y`)               |
| `bar`        | One keyword column (`x`) + one numeric (`y`) |

### API (Rust)

```
crates/genomehubs-query/src/local_report/
    mod.rs        — re-exports
    tsv.rs        — TSV/CSV reader → Vec<HashMap<String, Value>>
    builder.rs    — local_plot_spec() entry point
```

```rust
/// Build a `PlotSpec` from a local TSV/CSV file.
///
/// `column_map` maps report axis names to column names in the file:
/// `{"x": "genome_size", "cat": "assembly_level"}`
/// `delimiter` is `'\t'` for TSV or `','` for CSV.
pub fn local_plot_spec(
    content: &str,
    report_type: PlotReportType,
    column_map: &HashMap<String, String>,
    display: Option<DisplaySpec>,
    delimiter: char,
) -> Result<PlotSpec, LocalReportError>
```

### WASM / PyO3 / extendr exports

```rust
// crates/genomehubs-query/src/lib.rs
// Returns PlotSpec as JSON, or {"error": "..."} on failure
pub fn local_plot_spec_json(
    content: &str,
    report_type_str: &str,
    column_map_json: &str,
    display_json: &str,
    delimiter: &str,
) -> String
```

### SDK usage

```python
# Python
from cli_generator import local_plot_spec_json
import json

with open("data.tsv") as f:
    content = f.read()

spec = json.loads(local_plot_spec_json(
    content,
    "histogram",
    json.dumps({"x": "genome_size"}),
    json.dumps({"title": "My data", "width": 800}),
    "\t",
))
vl = plot_spec_to_vega_lite(spec)
```

```bash
# CLI
genomehubs local-report \
  --input data.tsv \
  --report histogram \
  --x genome_size \
  --display '{"title":"My data"}' \
  --output plot.svg
```

The `genomehubs local-report` subcommand reads the file, calls `local_plot_spec()`,
and dispatches to the `plot` renderer.

---

## Scenario 3: Augmented Remote Report

User fetches a report, adds locally computed annotations, then renders.

### `merge_annotations()` helper

```python
# Python utility function in python/cli_generator/query.py
def merge_annotations(
    plot_spec: dict,
    annotations: list[dict],
    join_key: str,
) -> dict:
    """Merge annotation dicts into plot_spec.data rows by join_key.

    For each row in plot_spec.data matching an annotation entry on join_key,
    add the annotation fields to the row. Returns the modified plot_spec.
    """
```

No Rust involved — pure Python dict manipulation. The R and JS equivalents are
similarly pure-language helpers.

---

## Files to Create

```
crates/genomehubs-query/src/local_report/
    mod.rs
    tsv.rs
    builder.rs
```

## Files to Modify

| File                                       | Change                                                              |
| ------------------------------------------ | ------------------------------------------------------------------- |
| `crates/genomehubs-query/src/lib.rs`       | WASM export `local_plot_spec_json`                                  |
| `src/lib.rs`                               | PyO3 export `local_plot_spec_json`                                  |
| `templates/r/lib.rs.tera`                  | extendr export                                                      |
| `templates/r/extendr-wrappers.R.tera`      | R wrapper                                                           |
| `src/main.rs`                              | Register `local-report` subcommand                                  |
| `python/cli_generator/query.py`            | `local_plot_spec()` wrapper; `merge_annotations()` helper           |
| `templates/python/query.py.tera`           | Mirror                                                              |
| `templates/js/query.js`                    | `localPlotSpec()` wrapper; `mergeAnnotations()` helper              |
| `templates/r/query.R`                      | `local_plot_spec()` wrapper; `merge_annotations()` helper           |

---

## Scope Boundaries

| In scope                                              | Out of scope                                               |
| ----------------------------------------------------- | ---------------------------------------------------------- |
| Local render of remote `PlotSpec` (CLI + SDK)         | Positional (oxford/ribbon/painting) hybrid — phase-XX      |
| `local_plot_spec()` from TSV/CSV                      | Database joins or multi-file merges                        |
| `merge_annotations()` helper (pure Python/JS/R)       | Server-side local data upload                              |
| `genomehubs local-report` CLI subcommand              | Streaming/large file support                               |
| `--display` and `--include-plot-spec` CLI flags       |                                                            |

---

## Testing

- Unit test: `local_plot_spec()` histogram from valid TSV returns `PlotSpec` with correct `x` axis
- Unit test: `local_plot_spec()` scatter returns correct `x` and `y` axes
- Unit test: `local_plot_spec()` with missing column returns `LocalReportError`
- Unit test: auto-detection correctly classifies numeric vs keyword columns
- Unit test: `merge_annotations()` adds fields for matched rows; unmatched rows unchanged
- Proptest: `local_plot_spec()` with fuzzed TSV input never panics
- Integration test: `genomehubs local-report` CLI writes valid SVG for a histogram
- Integration test: `genomehubs report ... | genomehubs plot` pipeline produces a file


```
# Busco id   Status     Sequence   Gene Start  Gene End  Strand  Score  Length  OrthoDB url  Description
10at7742      Complete   chr1       58346930    58426905  -       6867.8 26991   https://...  KIAA0196...
```

Relevant columns: `Busco_id` (group identifier), `Status`, `Sequence` (sequence_id),
`Gene Start`, `Gene End`, `Strand`.

Status filter: by default include only `Complete` features. `Duplicated` features are
included but deduplicated by taking the highest-score instance. `Fragmented` and
`Missing` are excluded.

### GFF3 (secondary format, lower priority)

Standard GFF3 for arbitrary feature types. The `group_by` field maps to a GFF3
attribute (e.g. `ID`, `Name`, or a custom attribute). Requires the user to specify
which attribute to use as the marker identifier.

---

## Files to Create

```
crates/genomehubs-query/src/parse_local/
    mod.rs          — re-exports
    busco.rs        — BUSCO full_table.tsv parser
    fai.rs          — samtools .fai index parser (sequence lengths)
    lengths.rs      — explicit lengths TSV parser + derive_lengths fallback
    gff3.rs         — GFF3 parser (lower priority)
    feature_set.rs  — LocalFeatureSet type (shared output of all parsers)
crates/genomehubs-query/src/report/
    hybrid.rs       — positional_from_features() pure computation function
```

## Files to Modify

| File                                  | Change                                                                                        |
| ------------------------------------- | --------------------------------------------------------------------------------------------- |
| `crates/genomehubs-query/src/lib.rs`  | WASM exports: `parse_busco_tsv`, `parse_fai`, `parse_lengths_tsv`, `positional_from_features` |
| `src/lib.rs`                          | PyO3 exports: same                                                                            |
| `templates/r/lib.rs.tera`             | extendr exports                                                                               |
| `templates/r/extendr-wrappers.R.tera` | R wrappers                                                                                    |
| `python/cli_generator/query.py`       | `hybrid_positional()` method on `QueryBuilder`                                                |
| `templates/python/query.py.tera`      | Mirror                                                                                        |
| `templates/js/query.js`               | `hybridPositional()` method                                                                   |
| `templates/r/query.R`                 | `hybrid_positional()` method                                                                  |

---

## Chromosome Length Sources for Local Assemblies

Oxford, ribbon, and painting plots all require chromosome/sequence lengths to compute
proportional offsets. For **remote** assemblies these are fetched from ES
(`feature_type=topLevel`) by the Phase 11 server-side code. For **local** assemblies
there is no ES to query; the user must supply lengths alongside the feature file.

Three sources are supported, in priority order:

### 1. FASTA index (`.fai`) — preferred

Produced by `samtools faidx genome.fa`. Five-column TSV, no header:

```
chr1    248956422    52    60    61
chr2    242193529    252513167    60    61
```

Only the first two columns (`name`, `length`) are used. This file is present in the
overwhelming majority of BUSCO workflows because the genome FASTA is required input.

`parse_fai()` is a standalone function in `parse_local/fai.rs` (~10 lines).

### 2. Explicit lengths table — fallback

A user-supplied two-column TSV (`sequence_id\tlength`, no header) for when `.fai` is
not available:

```
chr1    248956422
chr2    242193529
```

`parse_lengths_tsv()` handles this format.

### 3. Derived from features — zero-configuration fallback

When no length source is provided, `max(end)` per sequence is used as a lower bound.
This is always an underestimate (the last marker rarely sits at the chromosome tip) so:

- The `LocalFeatureSet` carries `lengths_derived: true`
- The `PlotSpec` response carries `lengths_derived: true` for any assembly using this mode
- SDK renders a warning; CLI warns to stderr
- The plot is still useful for exploration but axis proportions will be approximate

### Painting-specific note

For painting (single assembly), the chromosome track is drawn proportionally. Derived
lengths produce visibly wrong proportions (last chromosome appears shorter than it is).
Users should be encouraged to supply `.fai` or lengths TSV for painting.
For oxford/ribbon, the distortion only affects the relative length of the last segment
and is less visually prominent.

---

## `LocalFeatureSet` Type

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single genomic feature position from a local file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalFeature {
    /// Shared marker identifier (e.g. BUSCO gene ID).
    pub group: String,
    pub sequence_id: String,
    pub start: u64,
    pub end: u64,
    /// +1 or -1. Default +1 when absent in source.
    pub strand: i8,
    /// Optional category label (e.g. BUSCO lineage, feature type).
    pub cat: Option<String>,
}

/// A parsed set of local features for one assembly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalFeatureSet {
    /// Nominal assembly ID — user-supplied label for this local assembly.
    pub assembly_id: String,
    pub features: Vec<LocalFeature>,
    /// Sequence lengths keyed by sequence_id.
    ///
    /// Required for proportional axis layout. If absent, derived from
    /// `max(feature.end)` per sequence and `lengths_derived` is set to true.
    pub sequence_lengths: HashMap<String, u64>,
    /// True when `sequence_lengths` were derived from feature positions rather
    /// than supplied by the user. Axis proportions will be approximate.
    pub lengths_derived: bool,
}

impl LocalFeatureSet {
    /// Derive sequence lengths from `max(feature.end)` per sequence.
    ///
    /// Used as a fallback when no `.fai` or lengths table is supplied.
    /// Sets `lengths_derived = true`.
    pub fn derive_lengths(&mut self) {
        self.sequence_lengths.clear();
        for feat in &self.features {
            let entry = self.sequence_lengths.entry(feat.sequence_id.clone()).or_insert(0);
            if feat.end > *entry {
                *entry = feat.end;
            }
        }
        self.lengths_derived = true;
    }
}
```

---

## Parsers (`parse_local/`)

### `parse_busco_tsv()` (`busco.rs`)

```rust
/// Parse a BUSCO `full_table.tsv` into a `LocalFeatureSet`.
///
/// Lines starting with `#` are skipped (header/comments).
/// Only `Complete` and `Duplicated` statuses are included.
/// For `Duplicated` genes, the instance with the highest score is kept.
/// `sequence_lengths` is left empty; caller should call `parse_fai()` or
/// `parse_lengths_tsv()` to populate it, or call `derive_lengths()` as a fallback.
pub fn parse_busco_tsv(
    assembly_id: &str,
    content: &str,
) -> Result<LocalFeatureSet, ParseError> { ... }
```

### `parse_fai()` (`fai.rs`)

```rust
/// Parse a samtools `.fai` index and return a sequence-length map.
///
/// Only the first two columns (name, length) are used; remaining columns are ignored.
pub fn parse_fai(content: &str) -> Result<HashMap<String, u64>, ParseError> { ... }
```

Typical SDK usage — parse both files, merge lengths into the feature set:

```python
from cli_generator import parse_busco_tsv, parse_fai

feature_set = parse_busco_tsv("my_assembly", open("full_table.tsv").read())
feature_set["sequence_lengths"] = parse_fai(open("genome.fa.fai").read())
feature_set["lengths_derived"] = False
```

### `parse_lengths_tsv()` (`lengths.rs`)

```rust
/// Parse a two-column TSV (`sequence_id\tlength`) into a sequence-length map.
pub fn parse_lengths_tsv(content: &str) -> Result<HashMap<String, u64>, ParseError> { ... }
```

If neither `.fai` nor lengths TSV is supplied, `LocalFeatureSet::derive_lengths()` is
called automatically before layout computation, setting `lengths_derived: true`.

---

## `positional_from_features()` (`report/hybrid.rs`)

A pure Rust function (no HTTP, no ES) that runs the Phase 11 layout algorithm on
`LocalFeatureSet` inputs and returns a `PlotSpec`. Handles all three sub-types:

```rust
/// Compute an oxford, ribbon, or painting plot from local feature sets.
///
/// For oxford/ribbon, all assemblies in `feature_sets` must have populated
/// `sequence_lengths` (or `derive_lengths()` will be called automatically,
/// setting `lengths_derived: true` in the output `PlotSpec`).
/// For painting, a single assembly is expected.
pub fn positional_from_features(
    feature_sets: &[LocalFeatureSet],
    spec: &PositionalSpec,
    display: Option<&DisplaySpec>,
) -> Result<PlotSpec, LayoutError> { ... }

/// Compute a positional plot from a remote PlotSpec + one or more local feature sets.
///
/// `remote` is the PlotSpec returned by `/api/v3/positional` for the remote assemblies.
/// `local` contains one LocalFeatureSet per local assembly to add.
/// The report type in `spec` determines whether this is oxford, ribbon, or painting.
pub fn hybrid_positional(
    remote: &PlotSpec,
    local: &[LocalFeatureSet],
    spec: &PositionalSpec,
) -> Result<PlotSpec, LayoutError> { ... }
```

---

## SDK Workflow: `hybrid_positional()`

The SDK method orchestrates the full workflow as a single call. `report_type` controls
which positional sub-type is produced (`oxford`, `ribbon`, or `painting`).

```python
# Python — hybrid oxford (one remote assembly, one local)
result = (
    QueryBuilder()
    .index("feature")
    .taxa(["Homo sapiens"])
    .hybrid_positional(
        report="oxford",
        remote_assemblies=["GCA_000001405.28"],  # fetch from API
        local_files=[{
            "busco": "path/to/full_table.tsv",
            "fai": "path/to/genome.fa.fai",      # optional; omit to use derived lengths
            "assembly_id": "my_new_assembly",
        }],
        group_by="busco_gene",
        window_size=1_000_000,
    )
    .fetch()
)

# Python — painting (single local assembly, no remote needed)
result = (
    QueryBuilder()
    .hybrid_positional(
        report="painting",
        local_files=[{
            "busco": "path/to/full_table.tsv",
            "fai": "path/to/genome.fa.fai",
            "assembly_id": "my_assembly",
        }],
        group_by="busco_gene",
        cat="busco_status",
        window_size=500_000,
    )
    .fetch()
)
```

Internally, `fetch()` for a hybrid positional request:

1. For each `local_files` entry: read and parse the BUSCO file via `parse_busco_tsv()`;
   populate sequence lengths from `.fai` via `parse_fai()` (or `derive_lengths()` if absent)
2. If `remote_assemblies` is non-empty: fetch remote features via `POST /api/v3/positional`
3. If both remote and local: call `hybrid_positional(remote_spec, local_feature_sets, spec)`
4. If local-only: call `positional_from_features(feature_sets, spec, display)`
5. Return the resulting `PlotSpec`

```javascript
// JavaScript — hybrid oxford
const result = await builder
  .index("feature")
  .taxa(["Homo sapiens"])
  .hybridPositional({
    report: "oxford",
    remoteAssemblies: ["GCA_000001405.28"],
    localFiles: [
      {
        busco: tsvContent, // string (read by caller)
        fai: faiContent, // string (read by caller), optional
        assemblyId: "my_assembly",
      },
    ],
    groupBy: "busco_gene",
    windowSize: 1_000_000,
  })
  .fetch();
```

The JS SDK caller is responsible for reading file content as strings before passing to
`hybridPositional()`, because WASM cannot access the file system directly.

---

## Two-Local / All-Local Mode

`positional_from_features()` supports any number of local assemblies with no API calls.
All plot types are available:

```python
from cli_generator import parse_busco_tsv, parse_fai, positional_from_features

asm_a = parse_busco_tsv("assembly_A", open("full_table_A.tsv").read())
asm_a["sequence_lengths"] = parse_fai(open("A.fa.fai").read())

asm_b = parse_busco_tsv("assembly_B", open("full_table_B.tsv").read())
asm_b["sequence_lengths"] = parse_fai(open("B.fa.fai").read())

# Oxford — two local assemblies
spec = positional_from_features([asm_a, asm_b], {"report": "oxford", "window_size": 1_000_000})

# Ribbon — three local assemblies
asm_c = parse_busco_tsv("assembly_C", open("full_table_C.tsv").read())
asm_c["sequence_lengths"] = parse_fai(open("C.fa.fai").read())
spec = positional_from_features([asm_a, asm_b, asm_c], {"report": "ribbon", "window_size": 1_000_000})

# Painting — single local assembly
spec = positional_from_features([asm_a], {"report": "painting", "cat": "busco_status", "window_size": 500_000})
```

When `.fai` is omitted, `derive_lengths()` is called automatically. The returned
`PlotSpec` includes `"lengths_derived": true` for the affected assembly and the SDK
logs a warning.

---

## blobtk / external BUSCO parsing

The blobtk crate (`../../blobtoolkit/blobtk`) contains BUSCO parsing and plotting code.
The approach here **does not depend on blobtk** as an upstream crate, for two reasons:

1. blobtk has its own rendering stack that would conflict with `plotters`.
2. The BUSCO `full_table.tsv` format is simple enough to parse independently in ~50 lines.

If blobtk adopts the `LocalFeatureSet` type or the `parse_busco_tsv()` function from
this crate in future, that is a separate decision. For now, this project defines a
minimal, standalone parser that is sufficient for the hybrid mode use case.

---

## Testing

- Unit test: `parse_busco_tsv` correctly parses a valid BUSCO file
- Unit test: `parse_busco_tsv` deduplicates `Duplicated` genes by max score
- Unit test: `parse_busco_tsv` skips `Fragmented` and `Missing` lines
- Unit test: `parse_fai` returns correct lengths for a known `.fai` file
- Unit test: `parse_fai` ignores columns 3–5 (offset, linebases, linewidth)
- Unit test: `parse_lengths_tsv` parses a two-column TSV correctly
- Unit test: `LocalFeatureSet::derive_lengths` sets `lengths_derived = true` and uses `max(end)` per sequence
- Unit test: `positional_from_features` with oxford config + two feature sets with lengths produces valid `PlotSpec`
- Unit test: `positional_from_features` with painting config + single feature set produces `segments`
- Unit test: `positional_from_features` with no supplied lengths falls back to derived, sets `lengths_derived: true` in output
- Unit test: `positional_from_features` with non-overlapping feature sets returns a `LayoutError` (not a panic)
- Proptest: `parse_busco_tsv` with arbitrary line content never panics (returns error for malformed lines)
- Proptest: `parse_fai` with arbitrary line content never panics
- Proptest: `derive_lengths` on any feature set never panics; every sequence_id in features appears in lengths
- Integration test (SDK): `hybrid_positional()` oxford mode with a mocked API response and real BUSCO + `.fai` files produces `PlotSpec` with two assemblies, `lengths_derived: false`
- Integration test (SDK): same without `.fai` produces `PlotSpec` with `lengths_derived: true` and emits a warning
- Integration test (SDK): all-local ribbon mode with three BUSCO files + `.fai` files produces correct output without an HTTP call
- Integration test (SDK): painting mode with a single local assembly produces a `segments` array
