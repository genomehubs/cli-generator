# Getting started

Choose your path based on what you want to do:

| Goal                                      | Path                                                                 | Time   |
| ----------------------------------------- | -------------------------------------------------------------------- | ------ |
| Try the CLI without installing anything   | [1. Try the goat-CLI preview](#1-try-the-goat-cli-preview)           | 5 min  |
| Generate a CLI for your own API           | [2. Generate a custom CLI](#2-generate-a-custom-cli)                 | 10 min |
| Use the Python SDK (programmatic queries) | [GETTING_STARTED-python.md](GETTING_STARTED-python.md)               | 5 min  |
| Use the R SDK                             | [GETTING_STARTED-r.md](GETTING_STARTED-r.md)                         | 5 min  |
| Use the JavaScript/Node.js SDK            | [GETTING_STARTED-javascript.md](GETTING_STARTED-javascript.md)       | 5 min  |
| Contribute to cli-generator               | [8. Contributing to cli-generator](#8-contributing-to-cli-generator) | 20 min |

**New to cli-generator?** Start with section 1, then read [MAIN.md](docs/MAIN.md) (project overview) or [extension-guide.md](docs/planning/extension-guide.md) (how to extend it).

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

Download and unzip, then make the binary executable:

```bash
# Make executable (Linux / macOS)
chmod +x goat-cli
```

### Verify the download

Before using the CLI, run the validation script to confirm it works:

```bash
# From the cli-generator repo root (quick validation ~30 sec)
bash scripts/validate_artifacts.sh ./path/to/extracted/artifacts

# Or, for comprehensive testing with real API calls (~1-2 min per language)
bash scripts/validate_artifacts.sh --deep ./path/to/extracted/artifacts
```

- **Quick validation** runs smoke tests (import, instantiate, URL generation)
- **Deep validation** tests `.count()`, `.search()`, `.validate()`, and response parsing with real API calls

Both should complete with all tests passing (✓).

### Accessing Documentation

The artifacts include full **interactive documentation** built with Quarto:

**In the extracted artifacts:**

```bash
# Open the docs in your browser
open goat-cli/docs/index.html

# Or rebuild the docs locally
cd goat-cli/docs
quarto preview
```

The docs include:

- **[QueryBuilder reference](goat-cli/docs/reference/query-builder.html)** — Complete method reference with examples in Python, R, and JavaScript
- **[Quickstart guide](goat-cli/docs/quickstart.html)** — Step-by-step tutorials for all SDKs
- **[Parse reference](goat-cli/docs/reference/parse.html)** — Response parsing API and examples

**Language-specific quick references** (in the repo):

- [GETTING_STARTED-python.md](GETTING_STARTED-python.md) — Python operators, patterns, debugging tips
- [GETTING_STARTED-r.md](GETTING_STARTED-r.md) — R patterns, piping examples, troubleshooting
- [GETTING_STARTED-javascript.md](GETTING_STARTED-javascript.md) — REPL examples, async patterns, file usage

### Basic usage

Once validated:

```bash
# Help and discovery
./goat-cli --help
./goat-cli taxon search --help
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

Generate your own CLI from a YAML config file pointing to your API.

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

**→ See [GETTING_STARTED-python.md](GETTING_STARTED-python.md) for comprehensive examples, operators, and API reference.**

Each generated CLI includes a Python extension module (`{{ site_name }}_sdk`)
built with [maturin](https://github.com/PyO3/maturin). The SDK provides a
`QueryBuilder` class for programmatic queries without CLI overhead.

### Try the goat_sdk preview

Pre-built wheel files are uploaded alongside the CLI binary as CI artifacts
on every push to `main`.

Go to the [Actions tab](https://github.com/genomehubs/cli-generator/actions) →
most recent **"Generated CLI tests"** run → **Artifacts** button → find **`goat-sdk-wheel-*`**.

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

**→ See [GETTING_STARTED-r.md](GETTING_STARTED-r.md) for comprehensive examples, operators, and API reference.**

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

**→ See [GETTING_STARTED-javascript.md](GETTING_STARTED-javascript.md) for comprehensive examples, operators, and API reference.**

Each generated CLI includes a JavaScript package (`js/{{ js_package_name }}/`) with a `QueryBuilder`
class that works in Node.js (≥ 18).

The JavaScript SDK delegates URL building to a pre-compiled WebAssembly module (`pkg/`) that is
bundled into the generated package by the generator. No separate build step is needed.

### Use the QueryBuilder in Node.js

```bash
# In the generated CLI repo (e.g. /tmp/my-cli/my-site-cli/js/my_site)
cd js/my_site

# The pre-built WASM module is already included; just start Node.js
node
```

**Option 1: Dynamic import (recommended for REPL)**

In the Node.js REPL, use dynamic `import()` with `await`:

```javascript
const { QueryBuilder } = await import("./query.js");

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

**Option 2: CommonJS (if you prefer)**

```javascript
const { QueryBuilder } = require("./query.js");

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

**Option 3: In a script file (ES modules)**

If running from a `.js` script file (not the REPL), use standard ES modules:

```javascript
import { QueryBuilder } from "./query.js";

const qb = new QueryBuilder("taxon")
  .setTaxa(["Mammalia"], "tree")
  .addAttribute("genome_size", "ge", "1000000000")
  .addField("assembly_span")
  .setSize(10);

console.log("Query URL:", qb.toUrl());
```

**In the REPL:** Use Option 1 (dynamic import with `await`) or Option 2 (CommonJS).

**Troubleshooting: Module not found error**

If you get `Error [ERR_MODULE_NOT_FOUND]: Cannot find module '.../pkg-nodejs/genomehubs_query.js'`:

_For artifacts from CI:_

- Make sure you extracted the artifact zip fully
- Check that `pkg-nodejs/` exists in your `js/my_site/` directory
- If missing, re-download the artifact from GitHub Actions

_If you generated a custom CLI yourself:_

- The `build-wasm.sh` script should be in the `js/` directory
- Run: `bash build-wasm.sh`
- This compiles the WASM module and creates the `pkg-nodejs/` directory
- First build takes a few minutes; subsequent builds are faster

**How it works:**

1. `import { QueryBuilder } from "./query.js"` loads the WASM module via the bundled `pkg/` (or use `require("./query.js")` for CommonJS)
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

### Verify your changes

`scripts/verify_code.sh` runs all static checks and unit tests in one step:

```bash
bash scripts/verify_code.sh          # fmt, clippy, cargo test, black, pyright, pytest
bash scripts/verify_code.sh --verbose  # show full output on failure
```

If the Rust extension has changed, rebuild it first so the Python tests can
import the compiled module:

```bash
maturin develop --features extension-module
```

### End-to-end dev site test

Unit tests do not compile generated projects. After changing templates, the
embedded module system, or the WASM subcrate, use `scripts/dev_site.sh` to
regenerate and smoke-test a full site:

```bash
bash scripts/dev_site.sh                  # generate goat, Rust + JS smoke-tests
bash scripts/dev_site.sh --python         # also build Python extension + smoke-test
bash scripts/dev_site.sh --rebuild-wasm   # rebuild WASM pkg/ first (required when
                                          # a new #[wasm_bindgen] export is added)
bash scripts/dev_site.sh --rebuild-wasm --python goat  # full check
bash scripts/dev_site.sh boat             # test a different site
```

| Script                               | What it does                                       |
| ------------------------------------ | -------------------------------------------------- |
| `scripts/verify_code.sh`             | fmt, clippy, tests, black, pyright, pytest         |
| `scripts/dev_site.sh`                | Generate + Rust `--url` + JS `toUrl()` smoke-tests |
| `scripts/dev_site.sh --python`       | As above + maturin develop + Python smoke-test     |
| `scripts/dev_site.sh --rebuild-wasm` | Rebuild `crates/genomehubs-query/pkg/` first       |

---

## 9. Where to go next

**Got cli-generator working?** Here's the documentation structure:

### Quick Navigation

| I want to...                                      | Read this                                                                                          |
| ------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| Understand what cli-generator is and how it works | [MAIN.md](docs/MAIN.md) — Overview + documentation index                                           |
| Add a new parameter, language, or validator       | [extension-guide.md](docs/planning/extension-guide.md) — Full task-based patterns                  |
| Integrate cli-generator into my project           | [integration-runbook.md](docs/planning/integration-runbook.md) — Step-by-step walkthrough          |
| Learn the release strategy                        | [release-strategy.md](docs/planning/release-strategy.md) — Package managers + versioning           |
| Understand SDK parity across Python/R/JS          | [sdk-parity-testing.md](docs/testing/sdk-parity-testing.md) — Verification approach                |
| Set up Python/R/JS packaging and CI               | [release-strategy.md](docs/planning/release-strategy.md) — Full CI/CD template                     |
| See test fixtures + examples                      | [fixtures-complete-guide.md](docs/testing/fixtures-complete-guide.md) — Fixture discovery, caching |

### By Role

**SDK developers** (adding parameters, validators):
→ [extension-guide.md](docs/planning/extension-guide.md) + [.github/copilot-instructions.md](.github/copilot-instructions.md)

**Integration teams** (deploying to boat-cli, assessment-api):
→ [integration-runbook.md](docs/planning/integration-runbook.md)

**Release managers** (publishing Python/R/JS packages):
→ [release-strategy.md](docs/planning/release-strategy.md)

**Contributors** (fixing bugs, improving templates):
→ [CONTRIBUTING.md](CONTRIBUTING.md) + [AGENTS.md](AGENTS.md)

### Where the Docs Live

```
docs/
  MAIN.md                        # Start here: overview + index
  HISTORY.md                     # Archive of completed phases
  planning/                      # Planning documents + roadmaps
    extension-guide.md           # How to extend cli-generator
    integration-runbook.md       # How to integrate into your project
    release-strategy.md          # Package manager strategy
    GAPS-AND-OPPORTUNITIES.md    # What's missing + priorities
  testing/                       # Test documentation
    sdk-parity-testing.md        # Cross-language consistency
    fixtures-complete-guide.md   # Test fixture strategies
  reference/                     # Design & architecture docs
    python-sdk-design.md
    query-builder-design.md
```

### Still have questions?

- **For CLI usage**: Run `./my-site-cli --help` or check the `PREVIEW.md` from CI artifacts
- **For SDK questions**: Read the docstrings in generated code + examples in this file
- **For extending**: See [extension-guide.md](docs/planning/extension-guide.md) tasks 1–5
- **For bugs/issues**: Check [CONTRIBUTING.md](CONTRIBUTING.md) → "When to ask for help"
