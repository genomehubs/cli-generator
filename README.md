# cli-generator

Generic CLI generator for genomehubs instances

[![CI](https://github.com/Richard Challis/cli-generator/actions/workflows/ci.yml/badge.svg)](https://github.com/Richard Challis/cli-generator/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Overview

<!-- TODO: Replace this section with a description of what this project does. -->

This project exposes its core functionality as both:

- A **Rust CLI binary** (`cli-generator`) using the full Rust toolchain.
- A **Python library** (`cli_generator`) via a compiled PyO3 extension module.

## Quick start

### Rust CLI

```bash
cargo build --release
./target/release/cli-generator gc-content --sequence ATGCGCTA
```

### Python library

```bash
pip install maturin
maturin develop --features extension-module
python -c "import cli_generator; print(cli_generator.gc_content('ATGCGCTA'))"
```

## Development

See [GETTING_STARTED.md](GETTING_STARTED.md) for full environment setup and
the development workflow.

See [CONTRIBUTING.md](CONTRIBUTING.md) for code style, testing requirements,
PR process, and commit conventions.

See [AGENTS.md](AGENTS.md) for AI agent guidelines including the agent-log
convention.

## Project structure

```
src/
  core/         Pure Rust library logic — no PyO3 or clap dependencies.
  lib.rs        PyO3 module: wires core functions to Python.
  main.rs       clap CLI: wires core functions to subcommands.
  cli_meta.rs   CLI name/description constants (generator-controlled).
  generated/    Auto-generated code only. Never edited by hand.
python/
  cli_generator/   Python package re-exporting the Rust extension.
tests/
  python/       pytest + Hypothesis tests.
config/         YAML config snapshots (used by the CLI generator).
agent-logs/     Committed LLM session summaries for transparency.
```
