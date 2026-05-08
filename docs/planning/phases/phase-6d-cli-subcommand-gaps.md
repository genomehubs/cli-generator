# Phase 6d: CLI Subcommand Gaps

**Depends on:** Phase 6b (v3 migration — Python SDK), Phase 6c (ReportBuilder)
**Blocks:** Phase 6g (Quarto docs)
**Scope:** Generated CLI (`templates/rust/main.rs.tera`) and generated Rust client (`templates/rust/client.rs.tera`)

---

## Motivation

The generated CLI currently has `search`, `count`, and `lookup` subcommands per index. Missing:

- `record` — fetch a single record by ID (useful for accession-based lookup)
- `summary` — per-record field aggregation (specialist use but already in the API)
- `report` — all seven visualisation report types (high value; only accessible via SDK today)
- `search-batch` / `count-batch` — file-driven batch queries (useful for scripting)

Additionally, the `count` subcommand has fewer attribute-filtering flags than `search`; they should be in sync. The CLI `count` transport also needs verifying against v3.

---

## Work Items

### 1. Verify and update CLI `count` transport

**File:** `templates/rust/client.rs.tera`

The generated CLI calls `client::count_query()` (or similar) which currently builds a v2 URL. Verify this and migrate to POST `/v3/count` using the same Rust `SearchQuery` + `QueryParams` serialisation used by `search_query()`.

```rust
/// Count matching records via POST /v3/count.
pub fn count_query(opts: &CountOptions, api_base: &str) -> anyhow::Result<u64> {
    let query_yaml = build_query_yaml(opts)?;
    let params_yaml = build_params_yaml(opts)?;
    let payload = serde_json::json!({
        "query_yaml": query_yaml,
        "params_yaml": params_yaml,
    });
    let resp: serde_json::Value = ureq::post(&format!("{api_base}/v3/count"))
        .send_json(payload)?
        .into_json()?;
    Ok(resp["status"]["hits"].as_u64().unwrap_or(0))
}
```

**Verify:** `goat-cli taxon count --taxon "Primates"` returns a non-zero integer via the local API.

---

### 2. Align `count` attribute flags with `search`

**File:** `templates/rust/main.rs.tera`

The `Count` enum variant currently only accepts `--query` (raw query string). Add the same attribute-filtering flags that `Search` accepts:

| Flag                         | Description                                                            |
| ---------------------------- | ---------------------------------------------------------------------- |
| `--taxon` / `--taxon-filter` | Taxon name + filter type                                               |
| `--rank`                     | Taxonomic rank                                                         |
| `--filter`                   | Attribute filter (repeatable)                                          |
| `--exclude`                  | Exclusion flags                                                        |
| `--include-estimates`        | Include estimated values                                               |
| `--taxonomy`                 | Taxonomy name                                                          |
| `--fields`                   | Fields to include (for count, affects which aggregations are resolved) |

The `count` handler in `main.rs.tera` constructs a `QueryBuilder` from these flags identically to `search`, then calls `count_query()` instead of `search_query()`.

---

### 3. Add `record` subcommand

**File:** `templates/rust/main.rs.tera`

One `Record` variant per index (same per-index nesting as `Search`):

```rust
/// Fetch a single record by its identifier.
Record {
    /// Record ID or accession.
    #[arg(value_name = "ID")]
    record_id: String,

    /// Output format: json (default) or tsv.
    #[arg(long, default_value = "json")]
    format: String,
},
```

Handler calls `client::fetch_record(record_id, index, api_base)` which GETs `/v3/record?recordId={id}&result={index}` and returns the parsed JSON.

---

### 4. Add `summary` subcommand

**File:** `templates/rust/main.rs.tera`

```rust
/// Fetch summary aggregations for a record.
Summary {
    /// Record ID.
    #[arg(value_name = "ID")]
    record_id: String,

    /// Comma-separated field names.
    #[arg(long)]
    fields: String,

    /// Summary types (default: "min,max,mean").
    #[arg(long, default_value = "min,max,mean")]
    summary_types: String,
},
```

Handler GETs `/v3/summary?recordId={id}&result={index}&fields={fields}&summary={types}`.

---

### 5. Add `report` subcommand

**File:** `templates/rust/main.rs.tera`

This is the most complex new subcommand. The `Report` variant accepts the full report configuration as flags. Because the set of valid flags depends on the report type, a single flat set of optional flags is cleanest:

```rust
/// Run a visualisation report query.
Report {
    /// Report type: histogram, scatter, map, tree, xPerRank, sources, arc.
    #[arg(value_name = "TYPE")]
    report_type: String,

    /// X-axis field name.
    #[arg(long)]
    x: Option<String>,

    /// X-axis options (e.g. "scale=log10").
    #[arg(long, default_value = "")]
    x_opts: String,

    /// Y-axis field name.
    #[arg(long)]
    y: Option<String>,

    /// Y-axis options.
    #[arg(long, default_value = "")]
    y_opts: String,

    /// Category field name.
    #[arg(long)]
    cat: Option<String>,

    /// Category options.
    #[arg(long, default_value = "")]
    cat_opts: String,

    /// Taxonomic rank for aggregation.
    #[arg(long)]
    rank: Option<String>,

    /// Additional fields to include.
    #[arg(long)]
    fields: Option<String>,

    /// Status filter value.
    #[arg(long)]
    status_filter: Option<String>,

    /// Rank for category labels.
    #[arg(long)]
    cat_rank: Option<String>,

    /// Collapse monotypic nodes (tree reports).
    #[arg(long)]
    collapse_monotypic: bool,

    /// Hex resolution for map reports (1–12).
    #[arg(long, default_value = "3")]
    hex_resolution: u8,

    /// Map threshold.
    #[arg(long, default_value = "2000")]
    map_threshold: u32,

    /// Scatter threshold.
    #[arg(long, default_value = "100")]
    scatter_threshold: u32,

    // Query flags (same as Search) for specifying what to query
    #[arg(long)]
    taxon: Vec<String>,
    #[arg(long, default_value = "ancestor")]
    taxon_filter: String,
    #[arg(long)]
    query: Option<String>,
    #[arg(long)]
    filter: Vec<String>,
    // ... (same query flags as Search)

    /// Output format: json (default).
    #[arg(long, default_value = "json")]
    format: String,
},
```

Handler:

1. Constructs `QueryBuilder` from query flags (identical to `Search` handler)
2. Constructs report YAML from report flags
3. POSTs to `/v3/report` via `client::run_report(query_yaml, params_yaml, report_yaml, api_base)`
4. Prints JSON response (or pipes to a formatter if `--format` is extended later)

`client::run_report()` in `client.rs.tera`:

```rust
pub fn run_report(
    query_yaml: &str, params_yaml: &str, report_yaml: &str, api_base: &str,
) -> anyhow::Result<serde_json::Value> {
    let payload = serde_json::json!({
        "query_yaml": query_yaml,
        "params_yaml": params_yaml,
        "report_yaml": report_yaml,
    });
    let resp: serde_json::Value = ureq::post(&format!("{api_base}/v3/report"))
        .send_json(payload)?
        .into_json()?;
    Ok(resp)
}
```

---

### 6. Add `search-batch` and `count-batch` subcommands

**File:** `templates/rust/main.rs.tera`

File-driven batch: each line of a JSONL or TSV input file becomes one query in the batch.

```rust
/// Execute multiple search queries from a file.
SearchBatch {
    /// Path to a JSONL file (one {query_yaml, params_yaml} per line)
    /// or a TSV file (columns = query flag values).
    #[arg(value_name = "FILE")]
    file: PathBuf,

    /// Output format: json (default) or tsv.
    #[arg(long, default_value = "json")]
    format: String,
},

/// Count results for multiple queries from a file.
CountBatch {
    #[arg(value_name = "FILE")]
    file: PathBuf,
},
```

Handler reads lines, builds a `Vec<(query_yaml, params_yaml)>`, POSTs to `/v3/searchBatch` or `/v3/countBatch`.

> **Note:** These are not per-index subcommands. They live at the top level or under a `batch` subgroup. The input file format specifies the index per query.

---

## Verification

After implementing, rebuild the generated test CLI and verify:

```bash
# Rebuild the test workdir CLI
cargo run -- new goat --output-dir workdir/

# Smoke tests
cd workdir/goat-test-cli
cargo build --release

./target/release/goat-cli taxon count --taxon "Primates"
./target/release/goat-cli taxon record GCA_000001405.15
./target/release/goat-cli taxon summary GCA_000001405.15 --fields genome_size
./target/release/goat-cli taxon report histogram --x genome_size --rank species \
    --taxon "Primates" --taxon-filter ancestor
```

Also run `bash scripts/dev_site.sh --no-rebuild-wasm goat` to confirm generated project compiles.

---

## Tests

| Test                                            | Location                                     |
| ----------------------------------------------- | -------------------------------------------- |
| `count` transport is v3 POST (unit, mock HTTP)  | `tests/python/test_batch_integration.py`     |
| `count` with `--filter` attribute flag          | generated CLI smoke test                     |
| `record` returns valid JSON for known accession | generated CLI smoke test (requires live API) |
| `summary` returns aggregation data              | generated CLI smoke test (requires live API) |
| `report histogram` returns `report.buckets`     | generated CLI smoke test (requires live API) |
| `search-batch` from JSONL file                  | generated CLI smoke test (requires live API) |

---

## Ordering

1. Verify and migrate `count` transport to v3 POST in `client.rs.tera`
2. Align `count` flags with `search` in `main.rs.tera`
3. Add `record` subcommand + client function
4. Add `summary` subcommand + client function
5. Add `report` subcommand + client function (requires 6c ReportBuilder types for YAML construction)
6. Add `search-batch` / `count-batch` subcommands
7. Rebuild test workdir and run smoke tests
