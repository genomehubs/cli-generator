# Coverage Measurement Quick Start

## Overview

Coverage measurement tools are now set up for both Rust and Python. This guide shows how to get your baseline coverage.

## Prerequisites

```bash
# Install Rust toolchain (already installed if you follow CONTRIBUTING.md)
rustup install stable

# Install Python 3.9+
python --version  # Should be 3.9+

# Set up Python environment
python -m venv .venv
source .venv/bin/activate  # or .venv\Scripts\activate on Windows
pip install -e ".[dev]"
```

## Measure Coverage Locally

### Quick Start (Automated)

Run the measurement script to generate coverage reports for both Rust and Python:

```bash
bash scripts/measure_coverage.sh
```

This will:

1. Install `cargo-tarpaulin` (if not already installed)
2. Measure Rust code coverage and generate HTML report
3. Run Python tests with coverage measurement
4. Generate HTML reports in `./coverage/`

**Output:**

- Rust report: `coverage/tarpaulin-report.html`
- Python report: `coverage/python/index.html`

**Prerequisites:** The script will install missing tools automatically, but you may need to prepare the Rust extension:

```bash
# One-time setup (required for Python tests):
maturin develop --features extension-module
```

### Manual Steps

#### Rust Coverage

```bash
# Install tarpaulin (one-time):
cargo install cargo-tarpaulin

# Run coverage measurement:
cargo tarpaulin --out Html --output-dir ./coverage --timeout 300

# View HTML report:
open coverage/index.html
```

#### Python Coverage

```bash
# Run tests with coverage:
coverage run -m pytest tests/python/ -v

# View summary report:
coverage report --skip-empty

# Generate and view HTML report:
coverage html --directory ./coverage/python
open coverage/python/index.html
```

## Interpreting Results

### Coverage Metrics

- **Line coverage:** Percentage of lines executed during tests
- **Branch coverage:** Percentage of conditional branches taken
- **Target:** 85%+ line coverage, 80%+ branch coverage

### Identifying Gaps

Both HTML reports highlight:

- **Red lines:** Not covered by tests
- **Yellow lines:** Partially covered (some branches untested)
- **Green lines:** Fully covered

Look for:

- Untested error handling (`if` statements with minimal coverage)
- Unused code paths (functions not called in tests)
- Edge cases (boundary conditions, invalid inputs)

### Current Baseline (Phase 0)

You'll establish a baseline with this measurement. Expected coverage:

- **Rust:** ~60-75% (core logic well-tested, but error cases and CLI commands untested)
- **Python:** ~70-80% (SDK well-tested, but some edge cases untested)

These are **starting points**, not targets.

## CI Integration

Coverage measurement runs automatically on every push to `main` and pull request:

1. **Rust coverage** — Measured in `rust-checks` job
2. **Python coverage** — Measured in `integration-tests` job
3. **Reports uploaded** — To Codecov (if token configured)

View results in GitHub Actions > CI workflow > job output.

## Next Steps (Phase 1)

After establishing baseline:

1. **Identify gaps** — Review HTML reports, note untested code
2. **Write property tests** — Add proptest/Hypothesis tests (see test-coverage-strategy.md)
3. **Write error tests** — Add tests for invalid inputs, edge cases
4. **Raise thresholds** — Update `tarpaulin.toml` and `pyproject.toml` to enforce 85%+ coverage

## Troubleshooting

### `cargo-tarpaulin` timeout error

**Error:** `invalid digit found in string` when using `--timeout 300s`

**Fix:** The `--timeout` flag expects a plain number (seconds), not a duration string:

```bash
# ❌ Wrong (command-line)
cargo tarpaulin --timeout 300s

# ✅ Correct (command-line)
cargo tarpaulin --timeout 300
```

The script already handles this correctly by passing `--timeout 300`.

### Coverage percentage doesn't match HTML report

The parsing of coverage percentage from stdout can fail if there are warnings or multiple output lines.

**Always check the actual HTML reports:**

- Rust: `coverage/index.html`
- Python: `coverage/python/index.html`

These HTML files are the authoritative source.

### `cargo-tarpaulin` fails or is slow

```bash
# Reduce timeout if running on slow machine:
cargo tarpaulin --out Html --output-dir ./coverage --timeout 600s

# Or use a faster backend:
cargo tarpaulin --out Html --output-dir ./coverage --engine llvm
```

### Coverage report doesn't include all code

- Ensure dev dependencies are installed: `pip install -e ".[dev]"`
- Run full test suite: `pytest tests/python/` (not a single test)
- Check `pyproject.toml` `[tool.coverage.run]` for correct source paths

### HTML reports not opening

Coverage HTML is generated in the working directory. Check:

- Rust: `ls -la coverage/index.html`
- Python: `ls -la coverage/python/index.html`

If missing, re-run measurement script.

## Resources

- [Tarpaulin documentation](https://github.com/xd009642/tarpaulin)
- [coverage.py documentation](https://coverage.readthedocs.io/)
- [Test coverage strategy](test-coverage-strategy.md) — Full testing plan
- [Contributing guide](../CONTRIBUTING.md) — Development setup

---

**Last updated:** 19 March 2026
**Status:** Phase 0 baseline measurement setup complete
