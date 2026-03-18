# cli-generator

Generate a typed, testable Rust+Python CLI for any genomehubs API instance
from a YAML config file.

[![CI](https://github.com/genomehubs/cli-generator/actions/workflows/ci.yml/badge.svg)](https://github.com/genomehubs/cli-generator/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## What it does

`cli-generator` reads a YAML config describing an API's field groups and
produces a fully-typed Rust CLI with a Python SDK. The generated CLI:

- Has `--field-groups`, `--fields`, `--expand`, and `--list-field-groups` flags
  backed by compile-time PHF tables — fast and allocation-free
- Handles `--taxon-filter name|tree|lineage`, `--include-estimates`,
  `--format tsv|csv|json`, `--url` (dry-run), and more
- Ships with a Python `QueryBuilder` extension module built via PyO3 + maturin
- Includes a `PREVIEW.md` documenting interface changes for testers

Pre-built `goat-cli` binaries are published as CI artifacts — see
[Getting started](GETTING_STARTED.md#1-try-the-goat-cli-preview) for download
instructions.

## Quick start

### Generate a CLI from the goat config

```bash
# Install prerequisites (first time only)
cargo install cargo-generate

# Generate and build
cargo run -- new goat --config sites/ --output-dir /tmp
cd /tmp/goat-cli
cargo build --release

# Try it
./target/release/goat-cli taxon search --list-field-groups
./target/release/goat-cli taxon search --taxon Mammalia --field-groups busco,genome-size
./target/release/goat-cli taxon search --taxon Mammalia --field-groups G,b --url
```

### Generate a CLI from your own config

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

## Development

See [GETTING_STARTED.md](GETTING_STARTED.md) for full setup, usage examples,
and contributor workflow.

See [CONTRIBUTING.md](CONTRIBUTING.md) for code style, testing requirements,
PR process, and commit conventions.

See [AGENTS.md](AGENTS.md) for AI agent guidelines and the agent-log
convention.

## Project structure

```
src/
  core/         Pure Rust library logic — no PyO3 or clap dependencies.
  lib.rs        PyO3 module: wires core functions to Python.
  main.rs       clap CLI: wires core functions to subcommands.
  cli_meta.rs   CLI name/description constants (generator-controlled).
  generated/    Auto-generated code only. Never edited by hand.
  commands/     new, update, preview, validate subcommand handlers.
python/
  cli_generator/   Python package re-exporting the Rust extension.
templates/      Tera templates rendered into each generated project.
sites/          YAML config for known sites (goat, boat, …).
tests/
  python/       pytest + Hypothesis tests.
  generated_goat_cli.rs  Integration tests — generate goat-cli and check content.
agent-logs/     Committed LLM session summaries for transparency.
```
