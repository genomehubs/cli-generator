# Wiring `/lookup` and `/lookup/batch` into generated CLI

This note documents the touchpoints and step-by-step changes required to make
`goat-cli <index> lookup` support both single-term lookups and batch lookups from
`--file` input (mapping to `/api/v3/lookup/batch`). It re-uses existing client
helpers and CLI patterns (see `search --file` and `count --file`).

Goals

- `goat-cli taxon lookup --term "Canis lupus"` → call `/api/v3/lookup` (existing)
- `goat-cli taxon lookup --file /path/to/names.txt` → parse file into items, POST `/api/v3/lookup/batch` in up-to-100 chunks, print results in the selected format

High-level approach

1. Re-use the generated `client::lookup(index, search_term, size)` for single-term GET lookups.
2. Add a `client::lookup_batch(index, queries: &[String], sizes: &[usize]) -> Result<Vec<Vec<LookupResult>>>` helper in the generated `client.rs` (Rust template) that POSTs to `/v3/lookup/batch` and returns per-input result arrays (respecting 100-item batching). There are already patterns for `msearch` and `search_all` to follow.
3. Wire `--file` in `templates/rust/main.rs.tera`'s `Lookup` case to:
   - Read the file and parse terms (one per line) or support simple CSV/TXT formats.
   - Chunk into arrays of at most 100 items.
   - Call the `client::lookup_batch` helper for each chunk and accumulate results in input order.
   - Serialize the output according to the `--format` flag (TSV/JSON). Use `generated::output::print_output` as other subcommands do.
4. Add/extend tests and docs.

File-level touchpoints

- Server (already present)
  - `crates/genomehubs-api/src/routes/lookup.rs` — GET `/api/v3/lookup` (single-term behavior and query building)
  - `crates/genomehubs-api/src/routes/lookup_batch.rs` — POST `/api/v3/lookup/batch` (batch implementation)

- Generator (where to change templates / wiring)
  - `templates/rust/client.rs.tera` — add `pub fn lookup_batch(...)` near other client helpers (`msearch`, `record`, `lookup`, `summary`). Implement chunking, POST to `API_BASE_URL/v3/lookup/batch`, and parse the JSON response (use serde_json to map into results). Follow `msearch` / `search` style.

  - `templates/rust/main.rs.tera` — `Lookup` subcommand dispatch block. Add handling for `if let Some(ref file_path) = file { ... }` mirroring `Search`/`Count`'s file handling:
    - Call helper `load_lookup_file(file_path) -> Result<Vec<(String, Option<usize>)>>` (or reuse `load_batch_file` if it supports simple term lists). If `load_batch_file` supports name-only list, reuse it.
    - For each chunk of 100 input items, call `generated::client::lookup_batch(...)` and append results.
    - Print combined results with `generated::output::print_output` respecting `--format`.

  - `templates/rust/lib.rs.tera` — ensure a `parse_lookup_json` wrapper exists in the Python module (it does already) and that any new client helper is exported in the generated `sdk` module if needed.

  - `templates/rust/indexes.rs.tera` / `templates/rust/cli_flags.rs.tera` — no change required unless you need special file-format flags.

- SDK templates (optional parity)
  - `templates/python/query.py.tera`, `templates/js/query.js`, `templates/r/query.R` — add `lookup_batch` client method (if SDK needs batch support). The repo already contains reference to `/v3/lookup/batch` in templates and the Python `query.py` includes `lookup/batch` URL; verify parity and update if necessary.

Helper functions to reuse

- `templates/rust/client.rs.tera` contains `post_json`, `post_json(&url, &body)`, and `msearch` which demonstrates chunking and batch POST patterns.
- `templates/rust/main.rs.tera` already has `load_batch_file(...)` used by `Search` and `Count`. Reuse this to parse files if the format matches (simple term-per-line). If `load_batch_file` supports the three formats described in the `Search` docs, it's a perfect fit.
- `generated::output` helpers already handle printing JSON/TSV/TSV; reuse `generated::output::print_output`.

Suggested code snippets

1. `client.rs.tera` — skeleton `lookup_batch` function (follow `msearch` style):

```rust
pub fn lookup_batch(
    index: super::indexes::Index,
    terms: &[String],
    sizes: &[usize],
) -> Result<Vec<Vec<serde_json::Value>>> {
    // chunk into 100, POST { lookups: [{ search_term, size, result }] }
    // parse into LookupBatchResponse and map to per-item Vec<LookupResult>
}
```

2. `main.rs.tera` — inside `Lookup` case, add:

```rust
if let Some(ref file_path) = file {
    // load simple list (reuse load_batch_file if appropriate)
    let raw_items = load_batch_file(file_path, /*defaults*/)?; // or a simpler helper
    // raw_items -> Vec<String>
    let mut all_results = Vec::new();
    for chunk in raw_items.chunks(100) {
        let terms: Vec<String> = chunk.iter().map(|s| s.clone()).collect();
        let sizes: Vec<usize> = chunk.iter().map(|_| size).collect();
        let batch_results = generated::client::lookup_batch(
            generated::indexes::Index::{{ index.name | capitalize }},
            &terms,
            &sizes,
        )?;
        all_results.extend(batch_results.into_iter());
    }
    // Serialize and print
}
```

Testing and verification

- Add unit tests for `client::lookup_batch` in generated crate (if you run generator locally, ensure tests compile). Mock `post_json` using a local test double.
- Extend `tests/python/test_core.py` / `tests/python/test_sdk_fixtures.py` with a scenario for `lookup/batch` if SDK parity tests need it.

Developer workflow

1. Edit `templates/rust/client.rs.tera` to add `lookup_batch` (copy `msearch` chunking pattern and adjust request/response shape to `lookup/batch`).
2. Edit `templates/rust/main.rs.tera` to add `--file` handling to `Lookup` command by reusing `load_batch_file` (if compatible) or adding `load_lookup_file` for a simple name list.
3. Regenerate a CLI with `cli-generator new` or `update` for an existing site (or test by writing temporary files in `workdir/`).
4. Build generated CLI and test manual runs:

```bash
# from generated repo root
cargo build --release
./target/release/goat-cli taxon lookup --term "Canis lupus"
./target/release/goat-cli taxon lookup --file examples/names.txt
```

Notes and pitfalls

- The batch endpoint accepts up to 100 items per POST; remember to chunk.
- Maintain input order in output — `lookup_batch.rs` already preserves input order.
- `load_batch_file` supports several input formats; confirm it handles the "one name per line" case.
- `LookupResult` shape in Rust templates differs from client `lookup` returning raw JSON string — `lookup_batch` should return structured JSON suitable for `generated::output::print_output` or return a JSON string and let `print_output` handle it.
