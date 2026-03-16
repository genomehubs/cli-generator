# Getting started

This guide covers two audiences:

1. **Spinning up a new project** from this template (human and agent).
2. **Contributing to an existing project** derived from this template.

---

## Prerequisites

| Tool           | Install                                                           |
| -------------- | ----------------------------------------------------------------- |
| Rust (stable)  | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Python ≥ 3.9   | [python.org](https://www.python.org/downloads/) or `pyenv`        |
| maturin        | `pip install maturin`                                             |
| cargo-generate | `cargo install cargo-generate` (for template instantiation)       |
| pre-commit     | `pip install pre-commit` (optional, recommended)                  |

---

## Spinning up a new project from this template

### Option A — cargo-generate (recommended)

cargo-generate substitutes `cli-generator`, `cli_generator`, author details,
and other placeholders throughout all files and directory names.

```bash
cargo generate --git https://github.com/genomehubs/rust-py-template --name my-project
cd my-project
```

You will be prompted for:

- Project description
- Author name and email
- Minimum Python version (default: `3.9`)

After generation, initialise git and install hooks:

```bash
git init && git add -A && git commit -m "chore: initial commit from template"
pre-commit install        # optional but strongly recommended
```

### Option B — GitHub "Use this template"

Click **Use this template** on the GitHub repo page. This copies the files as-is
(template variables are **not** substituted). After cloning your new repo, run the
rename script to substitute placeholder names manually:

```bash
# Replace every occurrence of the template placeholder with your project name.
# Run from the repo root.
find . -not -path './.git/*' -type f | xargs sed -i '' \
  -e 's/cli-generator/my-project/g' \
  -e 's/cli_generator/my_project/g' \
  -e 's/Generic CLI generator for genomehubs instances/My project description/g' \
  -e 's/genomehubs/Your Name/g' \
  -e 's/genomehubs@genomehubs.org/you@example.com/g'

# Rename the Python package directory.
mv python/'cli_generator' python/my_project
```

cargo-generate (Option A) is less error-prone — prefer it.

---

## Development workflow

### 1. Build the Python extension in-place

This compiles the Rust code and installs the extension module into your active
Python environment so `import cli_generator` works immediately.

```bash
maturin develop --features extension-module
```

Re-run this after any change to Rust source files.

### 2. Run the Rust tests (unit + proptest)

```bash
cargo test
```

Note: run _without_ `--features extension-module`. The `rlib` crate type links
without libpython; adding the extension-module feature during testing is not
needed and causes linker errors on some platforms.

### 3. Run the Python tests (pytest + Hypothesis)

```bash
pytest tests/python/ -v
```

### 4. Lint and format

```bash
# Rust
cargo fmt --all
cargo clippy --all-targets -- -D warnings

# Python
black --line-length 120 python/ tests/python/
isort --profile black --line-length 120 python/ tests/python/
pyright python/ tests/python/
```

With VS Code and the recommended extensions installed, formatting runs
automatically on every save.

### 5. Install pre-commit hooks (optional)

```bash
pre-commit install
```

The hooks run `cargo fmt`, `cargo clippy`, `black`, and `isort` before every
commit, catching issues before they reach CI.

---

## VS Code setup

1. Open the project folder in VS Code.
2. Install the recommended extensions when prompted (or open
   `.vscode/extensions.json` and install manually).
3. Rust and Python files now auto-format on save.

Key extensions:

| Extension                   | Purpose                                      |
| --------------------------- | -------------------------------------------- |
| `rust-lang.rust-analyzer`   | Rust LSP, inline diagnostics, format on save |
| `ms-python.pylance`         | Python LSP (powered by pyright)              |
| `ms-python.black-formatter` | Python format on save                        |
| `ms-python.isort`           | Python import sorting on save                |
| `tamasfe.even-better-toml`  | TOML syntax and formatting                   |

---

## CI

Three GitHub Actions jobs run on every push and pull request:

| Job                 | Checks                                                 |
| ------------------- | ------------------------------------------------------ |
| `rust-checks`       | `cargo fmt`, `cargo clippy`, `cargo test` (+ proptest) |
| `python-checks`     | `black`, `isort`, `pyright`                            |
| `integration-tests` | `maturin develop` + `pytest` (+ Hypothesis)            |

> **Template repo note:** CI will fail on the uninstantiated template repo
> itself because `Cargo.toml` contains `cli-generator` which is not a valid
> Rust identifier. This is expected — CI is designed to run on generated projects.

---

## Agent-specific setup

See [AGENTS.md](AGENTS.md) for:

- How to create an agent-log entry for each work session.
- File naming convention and log schema.
- When to ask the user vs. proceed autonomously.
