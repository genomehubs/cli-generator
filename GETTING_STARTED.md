# Getting started

Two scenarios are covered here:

1. **Try the goat-CLI preview** — download a pre-built binary, no tools required.
2. **Generate a custom CLI** from a modified YAML config — requires Rust.

---

## 1. Try the goat-CLI preview

Pre-built binaries are uploaded as CI artifacts on every push to `main`.
Each artifact zip contains the binary **and** a `PREVIEW.md` describing
what has changed from the old `goat-cli` and how to give feedback.

### Download

Go to the [Actions tab](https://github.com/genomehubs/cli-generator/actions) →
most recent **"Generated CLI tests"** run → **Artifacts**:

| Artifact name            | Platform              |
| ------------------------ | --------------------- |
| `goat-cli-linux-x86_64`  | Linux (x86-64)        |
| `goat-cli-macos-aarch64` | macOS (Apple Silicon) |

Download and unzip, then:

```bash
# Make executable (Linux / macOS)
chmod +x goat-cli

# Basic usage
./goat-cli --help
./goat-cli taxon search --help

# List available field groups and their short codes
./goat-cli taxon search --list-field-groups

# Search examples
./goat-cli taxon search --taxon Mammalia --field-groups busco
./goat-cli taxon search --taxon Insecta --field-groups genome-size --format tsv
./goat-cli taxon search --taxon Mammalia --field-groups genome-size,busco,karyotype
./goat-cli taxon search --taxon Mammalia --field-groups G,b,k   # short codes
./goat-cli taxon search --taxon "Homo sapiens" --field-groups legislation
./goat-cli taxon search --taxon Insecta --taxon-filter tree --field-groups n50

# Print the API URL without fetching (useful for debugging)
./goat-cli taxon search --taxon Mammalia --field-groups busco --url
./goat-cli taxon search --taxon Mammalia --field-groups busco --include-estimates=false --url
```

Read `PREVIEW.md` (included in the zip) for a full list of what works,
what has changed, and how to give feedback on the design.

---

## 2. Generate a custom CLI

### Prerequisites

| Tool           | Install                                                           |
| -------------- | ----------------------------------------------------------------- |
| Rust (stable)  | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| cargo-generate | `cargo install cargo-generate`                                    |

### 2a. Clone cli-generator

```bash
git clone https://github.com/genomehubs/cli-generator
cd cli-generator
```

### 2b. Prepare config files

The `sites/` directory holds YAML config for each site.
Use the goat config as a starting point:

```
sites/
  goat.yaml                 # site metadata: API base URL, available indexes
  goat-cli-options.yaml     # field definitions: flags, field groups, synonyms
```

Copy them for your site:

```bash
cp sites/goat.yaml              sites/my-site.yaml
cp sites/goat-cli-options.yaml  sites/my-site-options.yaml
```

Edit `sites/my-site.yaml` to point at your API:

```yaml
name: my-site
display_name: My Site
api_url: https://my-api.example.org/api/v2
indexes:
  - taxon
  - assembly
```

Edit `sites/my-site-options.yaml` to add, remove, or rename field groups
and flags. Each entry maps a CLI flag to one or more API field names.

### 2c. Generate the CLI

```bash
cargo run -- new my-site --config sites/ --output-dir /tmp/my-cli
```

This will:

1. Fetch live field definitions from the API.
2. Scaffold a new Rust+Python project from [rust-py-template](https://github.com/genomehubs/rust-py-template).
3. Render generated source files into `src/generated/`.
4. Copy your config into the new repo's `config/` directory.

### 2d. Build and run

```bash
cd /tmp/my-cli/my-site-cli
cargo build --release
./target/release/my-site-cli --help
./target/release/my-site-cli taxon search --list-field-groups
./target/release/my-site-cli taxon search --taxon Mammalia --field-groups busco
```

### 2e. Verify URL generation (no network required)

`--url` prints the API URL that would be called without making a network
request — fast way to verify flags are wired up correctly:

```bash
./target/release/my-site-cli taxon search --taxon Mammalia --field-groups busco --url
./target/release/my-site-cli taxon search --taxon Mammalia --field-groups busco --include-estimates=false --url
```

### 2f. Run the test suite

```bash
cargo test
```

The generated project includes field-coverage tests that confirm every
flag in your config appears in the generated source and in the API URL.

---

## 3. Python SDK

Each generated CLI includes a Python extension module (`{{ site_name }}_sdk`)
built with [maturin](https://github.com/PyO3/maturin). The SDK provides a
`QueryBuilder` class for programmatic queries without CLI overhead.

### Try the goat_sdk preview

Pre-built wheel files are uploaded alongside the CLI binary as CI artifacts
on every push to `main`.

Go to the [Actions tab](https://github.com/genomehubs/cli-generator/actions) →
most recent **"Generated CLI tests"** run → **Artifacts** and download
`goat-sdk-wheel-<platform>`.

```bash
# Install the wheel
pip install goat_sdk-*.whl pyyaml
```

### Build the SDK locally

If you have generated a custom CLI (section 2) or are working on the
cli-generator itself, you can build the Python extension in-place with
[maturin](https://github.com/PyO3/maturin):

```bash
# Prerequisites
pip install maturin pyyaml

# In the generated CLI repo (e.g. /tmp/my-cli/my-site-cli)
maturin develop --features extension-module
```

Or, to build the cli-generator's own extension (useful when working on
`describe()` or `snippet()` locally):

```bash
# In the cli-generator repo root
maturin develop --features extension-module
```

After the build completes the package is installed into the current Python
environment in editable mode — no separate `pip install` step is needed.
Re-run the same command after any Rust source change to pick up updates.

### Try the QueryBuilder

```bash
# pandas and polars are optional dependencies
pip install pandas polars

python
```

```python
from goat_sdk.query import QueryBuilder

# Count records
count = QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree").count()
print(f"Mammals: {count} records")

# Get results as a pandas DataFrame
df = (
    QueryBuilder("taxon")
    .set_taxa(["Insecta"], filter_type="tree")
    .add_field("genome_size")
    .set_size(100)
    .search_df()
)
print(f"Insects with genome_size: {len(df)} records")
print(df.head())

# Or use polars for faster parsing
df = (
    QueryBuilder("assembly")
    .set_taxa(["Homo sapiens"])
    .add_attribute("assembly_span", operator="gt", value="3000000000")
    .add_field("assembly_span")
    .set_size(50)
    .search_polars()
)
print(f"Human assemblies > 3Gb: {len(df)} records")
print(df.select(["assembly_id", "assembly_span"]))

# Validate a query before fetching
qb = (
    QueryBuilder("taxon")
    .set_taxa(["Primates"], filter_type="tree")
    .add_field("genome_size")
    .add_attribute("genome_size", operator="ge", value="2500000000")
)
errors = qb.validate()
if errors:
    print(f"Validation errors: {errors}")
else:
    print("Query is valid ✓")
    results = qb.search_df()
```

**Notes:**

- `search_df()` and `search_polars()` require `pandas` or `polars` respectively.
  They'll display a helpful error message if the package is not installed.
- `add_attribute(name, operator, value)` lets you filter by field values
  (e.g. `assembly_span > 3G`, `genome_size >= 2.5G`).
- comparison operators `>`, `>=`, etc must currently be referred to as `gt`, `ge`, ...
- `validate()` checks the query against the baked-in field metadata without
  making a network call (may need wrapping an a try-catch block for now)
- Both `search_df()` and `search_polars()` fetch TSV by default for better type
  preservation; use `.search(format="json")` if you need raw JSON.

### Query descriptions

`describe()` returns a human-readable summary of what a query does, without
making any network call:

```python
from goat_sdk.query import QueryBuilder

qb = (
    QueryBuilder("taxon")
    .set_taxa(["Mammalia"], filter_type="tree")
    .add_attribute("genome_size", operator="ge", value="1000000000")
    .add_field("assembly_span")
)

# One-line summary
print(qb.describe())
# Search for taxa (Mammalia (including all descendants in the taxonomy tree)),
# filtered to genome_size >= 1000000000, returning assembly span.

# Detailed breakdown
print(qb.describe(mode="verbose"))
# Search for taxa in the Mammalia taxonomy branch...
# Filters applied:
#   • genome size >= 1000000000
# Returning fields:
#   • assembly span
```

Pass `field_metadata` to use display names from the API's `resultFields`
endpoint instead of canonical field names:

```python
meta = {"genome_size": {"display_name": "Genome Size (bp)"}}
print(qb.describe(field_metadata=meta))
# ...filtered to Genome Size (bp) >= 1000000000...
```

### Code snippet generation

`snippet()` renders a ready-to-run code example reproducing the current query,
suitable for embedding in UIs or documentation:

```python
from goat_sdk.query import QueryBuilder

qb = (
    QueryBuilder("taxon")
    .set_taxa(["Mammalia"], filter_type="tree")
    .add_attribute("genome_size", operator="ge", value="1000000000")
    .add_field("assembly_span")
    .set_sort("genome_size", "desc")
)

# Generate a Python snippet (default)
snippets = qb.snippet(site_name="goat", sdk_name="goat_sdk")
print(snippets["python"])
```

Output:

```python
import goat_sdk as sdk

# Create a query builder for goat
qb = sdk.QueryBuilder("taxon")

# Add filter: genome_size ge 1000000000
qb.add_attribute("genome_size", operator="ge", value="1000000000")

# Sort by genome_size desc
qb.add_sort("genome_size", "desc")

# Select specific fields
qb.set_fields([
    "assembly_span",
])

# Build the query and get the API URL
url = qb.to_url()
print(f"Query URL: {url}")
```

**To URL generation (no network required):**

```python
from goat_sdk.query import QueryBuilder

qb = (
    QueryBuilder("taxon")
    .set_taxa(["Mammalia"], filter_type="tree")
    .add_attribute("genome_size", operator="ge", value="1000000000")
    .add_field("assembly_span")
    .set_sort("genome_size", "desc")
)

# Generate the API URL without making a network call
url = qb.to_url()
print(f"Query URL: {url}")
```

`snippet()` accepts a `languages` list; both `"r"` and `"javascript"` are now supported:

```python
# Generate Python snippet (default)
snippets = qb.snippet(languages=["python"])
print(snippets["python"])

# Generate R snippet
snippets = qb.snippet(languages=["r"])
print(snippets["r"])

# Generate both Python and R
snippets = qb.snippet(languages=["python", "r"])
print(snippets["python"])
print(snippets["r"])

# Generate JavaScript
snippets = qb.snippet(languages=["javascript"])
print(snippets["javascript"])
```

**R snippet example:**

```r
library(goat)

# Create a query builder for goat
qb <- QueryBuilder$new("taxon")

# Add filter: genome_size ge 1000000000
qb$add_attribute("genome_size", "ge", "1000000000")

# Sort by genome_size desc
qb$add_sort("genome_size", "desc")

# Select specific fields
qb$set_fields(c("assembly_span"))

# Build the URL and fetch results
cat("Query URL:", qb$to_url(), "\n")
results <- qb$search()
```

---

## 4. R SDK

Each generated CLI includes an R package (`r/<pkg_name>/`) with a `QueryBuilder` R6 class
that has full parity with the Python SDK. URL building, `describe()`, and `snippet()` all
delegate to the same Rust engine via [extendr](https://extendr.github.io/). HTTP calls
(`count()` and `search()`) are made in pure R via `httr`.

### Build and install the R SDK

You need Rust (≥ 1.65) and the R build tools (`devtools`) installed.

```bash
# In the generated CLI repo (e.g. /tmp/my-cli/my-site-cli/r/my_site)
cd r/my_site
R -e "install.packages(c('devtools', 'R6', 'httr', 'jsonlite', 'yaml'))"
R -e "devtools::install()"
```

`devtools::install()` compiles the Rust extension via the bundled `configure` script and
installs the package. The first build downloads Rust crates and takes a few minutes;
subsequent builds are incremental.

### Try the QueryBuilder in R

```r
library(goat)

# Build a query (method chaining or step-by-step)
qb <- QueryBuilder$new("taxon")
qb$set_taxa(c("Mammalia"), filter_type = "tree")
qb$add_attribute("genome_size", "ge", "1000000000")
qb$add_field("assembly_span")

# Build the URL without making a network call
cat(qb$to_url(), "\n")

# Get a human-readable description of the query
cat(qb$describe(), "\n")

# Count and search
n <- qb$count()
results <- qb$search()          # returns a data.frame
results_json <- qb$search(format = "json")

# Generate code snippets
snippets <- qb$snippet(languages = c("r", "python"))
cat(snippets[["r"]], "\n")
```

---

## 5. JavaScript SDK

Each generated CLI includes a JavaScript package (`js/{{ js_package_name }}/`) with a `QueryBuilder`
class that works in Node.js (≥ 18).

The JavaScript SDK delegates URL building to a pre-compiled WebAssembly module (`pkg/`) that is
bundled into the generated package by the generator. No separate build step is needed.

### Use the QueryBuilder in Node.js

```bash
# In the generated CLI repo (e.g. /tmp/my-cli/my-site-cli/js/my_site)
cd js/my_site
node
```

```javascript
const { QueryBuilder } = require("./query");

// Build URL (synchronous, no network)
const qb = new QueryBuilder("taxon")
  .setTaxa(["Mammalia"], "tree")
  .addAttribute("genome_size", "ge", "1000000000")
  .addField("assembly_span")
  .setSize(10);

console.log("Query URL:", qb.toUrl());

// Count matching records (async)
qb.count().then((n) => console.log("Count:", n));
```

**How it works:**

1. `require("./query")` loads the WASM module synchronously via the bundled `pkg/`
2. `QueryBuilder` methods set up the query state (taxa, filters, fields, etc.)
3. `toUrl()` serialises the state to YAML and passes it to the WASM `build_url()` function
4. WASM runs the same Rust URL-building logic used by the Python SDK and returns the API URL
5. `count()` and `search()` make HTTP requests using the URL

**Why WASM?**
URL building logic lives in one place (Rust), and all language SDKs delegate to it via their
respective FFI boundaries (PyO3 for Python, WASM for JavaScript, extendr for R).
This guarantees JavaScript, Python, and R always produce identical URLs.

### Rebuilding the WASM module

The pre-built `pkg/` is sufficient for normal use. If you update cli-generator or want to
rebuild from source:

```bash
# In the generated CLI repo
cd js/{{ js_package_name }}
bash build-wasm.sh
```

The script locates the cli-generator repo, runs `wasm-pack build --target nodejs` in the
`crates/genomehubs-query/` subcrate, and copies the resulting `pkg/` directory here.

---

## 6. Update an existing generated CLI

After editing the config in your generated repo (`config/site.yaml` or
`config/cli-options.yaml`), re-run the generator to rebuild generated files:

```bash
# From inside the generated repo:
cargo run --manifest-path /path/to/cli-generator/Cargo.toml -- update
```

Or, to pull from a separate config directory (must contain `site.yaml` and
`cli-options.yaml` at the top level):

```bash
cargo run -- update /path/to/my-site-cli --config /path/to/my-config/
```

`update` only overwrites `src/generated/` and `src/cli_meta.rs`.
Hand-written code (`src/core/`, `src/main.rs`, etc.) is never touched.

---

## 7. Preview changes before generating

`preview` renders all templates to a temporary directory and prints a diff
against the currently-generated version — nothing is written to disk:

```bash
cargo run -- preview --site my-site --config sites/
# or, for an existing repo:
cargo run -- preview --repo /path/to/my-site-cli
```

---

## 8. Contributing to cli-generator

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full coding standards.

| Command                                                        | What it does                             |
| -------------------------------------------------------------- | ---------------------------------------- |
| `cargo test`                                                   | Unit tests + proptests                   |
| `cargo test --test generated_goat_cli`                         | Integration tests (needs cargo-generate) |
| `cargo fmt --all && cargo clippy --all-targets -- -D warnings` | Lint                                     |
| `maturin develop --features extension-module`                  | Build Python extension in-place          |
| `pytest tests/python/ -v`                                      | Python tests                             |
