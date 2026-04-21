# cli-generator

Generate a fully-typed Rust CLI and multi-language SDKs from a YAML config for any genomehubs API.

[![CI](https://github.com/genomehubs/cli-generator/actions/workflows/ci.yml/badge.svg)](https://github.com/genomehubs/cli-generator/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Tests](https://img.shields.io/badge/Tests-371%20passed-brightgreen)](tests/python/)
[![Coverage](https://img.shields.io/badge/Coverage-65%25%2B-yellowgreen)](#testing)

## What it does

`cli-generator` reads a YAML config describing an API's field groups and
produces a fully-typed, tested **Rust CLI + Python/R/JavaScript SDKs**.

### Rust CLI Features

- `--field-groups`, `--fields`, `--expand`, and `--list-field-groups` flags
  backed by compile-time PHF tables — fast and allocation-free
- `--taxon-filter name|tree|lineage`, `--include-estimates`,
  `--format tsv|csv|json`, `--url` (dry-run), and more
- Full bash/zsh completion autogeneration
- Quarto documentation site generation

### SDK Features (Python, R, JavaScript)

- **Cross-language method parity** — all three languages expose identical APIs
- **QueryBuilder** — chainable, type-hinted query construction
  - Set filters, pagination, sorting, output format
  - Built-in validation with detailed error messages
  - Serialize to URL, YAML, or parameter map
- **Response parsing** — Rust-backed parsers for performance and correctness
  - `parse_response_status()` — extract hit count and API errors
  - `parse_search_json()` — flatten fielded results to records
  - `parse_search_tsv()` — validate and normalize tabular data
- **API introspection** — `describe()` and `snippet()` methods
  - Query description in human-readable format
  - Code snippets in CLI, Python, R, or JavaScript
- **Multi-page support** — automatic pagination across SDKs

Pre-built `goat-cli` binaries and SDK packages published to PyPI, npm, and CRAN — see
[Getting started](GETTING_STARTED.md#1-try-the-goat-cli-preview) for instructions.

## Quick start

### Try the CLI

```bash
# Install prerequisites (first time only)
cargo install cargo-generate

# Generate and build the goat-cli
cargo run -- new goat --config sites/ --output-dir /tmp
cd /tmp/goat-cli
cargo build --release

# Try it
./target/release/goat-cli taxon search --list-field-groups
./target/release/goat-cli taxon search --taxon Mammalia --field-groups busco,genome-size
./target/release/goat-cli taxon search --taxon Mammalia --field-groups G,b --url
```

### Try the Python SDK

```python
from goat_sdk import QueryBuilder

qb = QueryBuilder(index="taxon", api_base="https://...")
qb.set_taxa(["Mammalia"], filter_type="tree")
qb.add_field("genome_size")
qb.set_size(10)

# Validate the query
errors = qb.validate()
if errors:
    print("Validation errors:", errors)

# Build a URL for inspection
print("Query URL:", qb.to_url())

# Fetch paginated results
results = qb.search(format="json")
print(f"Found {len(results)} results")

# Get a description of what the query does
print(qb.describe(mode="short"))

# Generate a code snippet
snippets = qb.snippet(languages=["python", "cli"])
print("Python snippet:", snippets["python"])
```

### Try the R SDK

```r
library(goat)

qb <- QueryBuilder$new(index = "taxon", api_base = "https://...")
qb$set_taxa(c("Mammalia"), filter_type = "tree")
qb$add_field("genome_size")
qb$set_size(10)

# Validate
errors <- qb$validate()
if (length(errors) > 0) print(errors)

# Build URL
print(qb$to_url())

# Search
results <- qb$search(format = "json")
print(paste("Found", nrow(results), "results"))

# Describe query
cat(qb$describe(mode = "short"))
```

### Try the JavaScript SDK

```javascript
const { QueryBuilder } = require("goat-sdk");

const qb = new QueryBuilder("taxon", "https://...");
qb.setTaxa(["Mammalia"], "tree");
qb.addField("genome_size");
qb.setSize(10);

// Validate
const errors = qb.validate();
if (errors.length > 0) console.log("Errors:", errors);

// Build URL
console.log("Query URL:", qb.toUrl());

// Search
const results = await qb.search("json");
console.log(`Found ${results.length} results`);

// Describe query
console.log(qb.describe({ mode: "short" }));
```

### Generate a new CLI from your own config

```bash
cp sites/goat.yaml              sites/my-site.yaml
cp sites/goat-cli-options.yaml  sites/my-site-options.yaml
# edit the two files to point at your API and define your field groups
cargo run -- new my-site --config sites/ --output-dir /tmp
```

## Update an existing generated CLI

```bash
# From inside the generated repo (after editing config/):
cargo run --manifest-path /path/to/cli-generator/Cargo.toml -- update
```

## Status & Documentation

**MVP Status**: Feature-complete multi-language CLI + SDK generation. Phase 0–3 implemented:

- ✅ Rust CLI with full feature set
- ✅ Python SDK via PyO3 (PyPI)
- ✅ R SDK via extendr (CRAN)
- ✅ JavaScript SDK via WASM (npm)
- ✅ Method parity across all three SDKs
- ✅ Response parsing with Rust correctness
- ✅ Validation, introspection, and code generation
- ✅ Multi-page result fetching
- ✅ CI integration and 371+ test coverage

### For users and integrators

Start with [GETTING_STARTED.md](GETTING_STARTED.md) — choose your role to navigate to the right section:

- 📦 **SDK users** — use the generated QueryBuilder in Python, R, or JavaScript
- 🔌 **Integration teams** — add your API to cli-generator; see [Integration Runbook](docs/planning/integration-runbook.md)
- 📊 **Release managers** — publish new SDKs; see [Release Strategy](docs/planning/release-strategy.md)

### For developers & contributors

See [MAIN.md](docs/MAIN.md) for documentation index by topic.

Key references:

- [CONTRIBUTING.md](CONTRIBUTING.md) — code style, testing, PR process
- [AGENTS.md](AGENTS.md) — AI agent guidelines and multi-language extension patterns
- [Copilot Instructions](.github/copilot-instructions.md) — workspace conventions for GitHub Copilot
- [Extension Guide](docs/planning/extension-guide.md) — add parameters, validators, or new languages (Rust → Python → R/JS pattern)
- [Test Strategy](docs/testing/test-strategy.md) — coverage targets, fixture generation, parity testing

## Development

See [GETTING_STARTED.md](GETTING_STARTED.md#development) for full setup and contributor workflow.

Run verification:

```bash
bash scripts/verify_code.sh              # Rust + Python lint, format, test, coverage
bash scripts/test_sdk_fixtures.sh --help # SDK fixture generation and validation
```

## Project structure

```
src/
  core/              Pure Rust library logic — no PyO3 or clap dependencies.
  lib.rs             PyO3 module: wires core functions to Python.
  main.rs            clap CLI: wires core functions to subcommands.
  cli_meta.rs        CLI name/description constants (generator-controlled).
  generated/         Auto-generated code only. Never edited by hand.
  commands/          new, update, preview, validate subcommand handlers.

crates/
  genomehubs-query/  WASM + extendr subcrate: query parsing, validation, introspection.

python/
  cli_generator/     Python package re-exporting the Rust extension + QueryBuilder.

templates/
  rust/, python/, r/, js/  Language-specific templates rendered into each generated project.
  snippets/          Code snippet templates (python, r, javascript, cli).
  docs/              Generated project documentation (HTML, quarto).

sites/               YAML config for known sites (goat, boat, …).

tests/
  python/            pytest + Hypothesis tests for all modules + parity tests.
  generated_goat_cli.rs  Integration tests — generate goat-cli and validate.

docs/
  MAIN.md            Documentation index and navigation guide.
  HISTORY.md         Archive of completed phase documentation.
  planning/          Strategic docs: release strategy, integration runbook, extension guide.
  testing/           Test fixtures, parameter coverage, test strategies.
  reference/         API schema caching, coverage measurements.
  api-schemas/       Cached API responses for fixture generation.

agent-logs/          Committed LLM session summaries for transparency and reproducibility.

scripts/
  verify_code.sh     Run all linting, formatting, and tests.
  test_sdk_fixtures.sh  Generate SDKs and validate fixture round-trips.
  dev_site.sh        Full end-to-end test: generate, build, smoke-test all languages.
```

## Package distribution

Generated SDKs are published to all major package registries:

| Language   | Registry        | Example                                    |
| ---------- | --------------- | ------------------------------------------ |
| Python     | PyPI            | `pip install goat-sdk`                     |
| R          | CRAN            | `install.packages("goat")`                 |
| JavaScript | npm             | `npm install goat-sdk`                     |
| Rust CLI   | GitHub Releases | Pre-built binaries (`goat-cli-*-*.tar.gz`) |

Each site has independent semantic versioning (e.g., `goat-sdk` v1.2.3 ≠ `boat-sdk` v2.1.0).
Release strategy, API polling for drift detection, and publishing checklists are documented
in [Release Strategy](docs/planning/release-strategy.md).

## License

MIT — see [LICENSE](LICENSE).
