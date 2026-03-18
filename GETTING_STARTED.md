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

### Using the SDK

After generation, build and install the wheel:

```bash
cd /tmp/my-cli/my-site-cli
maturin develop --features extension-module
pip install pyyaml
```

Then in Python:

```python
from my_site_sdk.query import QueryBuilder

# Build a URL (no network)
url = QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree").to_url()

# Count records
count = QueryBuilder("taxon").set_taxa(["Mammalia"], filter_type="tree").count()

# Search
results = (
    QueryBuilder("taxon")
    .set_taxa(["Insecta"], filter_type="tree")
    .add_field("genome_size")
    .set_size(20)
    .search()
)
```

The SDK will be available as a CI artifact (wheel) once the generated CLI
is built. See `PREVIEW.md` in future releases for SDK limitations and status updates.

---

## 4. Update an existing generated CLI

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

## 4. Preview changes before generating

`preview` renders all templates to a temporary directory and prints a diff
against the currently-generated version — nothing is written to disk:

```bash
cargo run -- preview --site my-site --config sites/
# or, for an existing repo:
cargo run -- preview --repo /path/to/my-site-cli
```

---

## 5. Contributing to cli-generator

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full coding standards.

| Command                                                        | What it does                             |
| -------------------------------------------------------------- | ---------------------------------------- |
| `cargo test`                                                   | Unit tests + proptests                   |
| `cargo test --test generated_goat_cli`                         | Integration tests (needs cargo-generate) |
| `cargo fmt --all && cargo clippy --all-targets -- -D warnings` | Lint                                     |
| `maturin develop --features extension-module`                  | Build Python extension in-place          |
| `pytest tests/python/ -v`                                      | Python tests                             |
