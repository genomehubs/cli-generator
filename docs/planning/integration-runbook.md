# Integration Runbook

Generate and integrate cli-generator SDKs into an existing project.

**Status**: 📋 PLANNING (stub created 2026-04-21)
**Owner**: [Your team]
**Effort**: 2–3 hours to write with examples

---

## Overview

This guide walks through integrating cli-generator into a new repository (e.g., boat-cli, assessment-api).

**Audience**: Repository maintainers onboarding to cli-generator
**Outcome**: Generated SDKs ready to use in your project

---

## Prerequisites

- [ ] Rust toolchain + cargo
- [ ] CLI generator cloned locally
- [ ] Your site's API metadata (field definitions, ranks, etc.)
- [ ] Decision: commit generated files or CI-generate?

---

## Step 1: Prepare Your Site Configuration

### 1a. Create a site YAML config

**Location**: `sites/{site-name}.yaml` (in cli-generator repo)

```yaml
# Example: sites/boat.yaml
name: boat
api_base: https://boat.example.com/api
description: Boat of Life Assembly metadata API
```

### 1b. Fetch field metadata from live API

```bash
# Discover fields from your API
curl https://your-api.example.com/api/v2/search?fields=true > field_metadata.json
```

## Step 2: Generate Your SDK

### Option A: Commit generated files

```bash
cd /path/to/your-project

# Generate SDKs for all languages
cargo run --release -- new boat --output-dir . --config ../cli-generator/sites/

# Commit generated files
git add goat-cli/python goat-cli/js goat-cli/rust
git commit -m "feat: add generated SDKs"
```

### Option B: CI-generate on every release

Create a GitHub Actions workflow:

```yaml
name: Generate SDKs
on: [push, pull_request]
jobs:
  generate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          repository: genomehubs/cli-generator
          path: cli-generator
      - run: cargo run -- new boat --output-dir . --config cli-generator/sites/
      - uses: actions/upload-artifact@v4
        with:
          name: generated-sdks
          path: |
            python/
            js/
```

---

## Step 3: Test the Generated SDK

### Python

```bash
cd goat-cli/python
maturin develop --features extension-module
python -m pytest tests/ -v
```

### JavaScript

```bash
cd goat-cli/js/goat
npm install
npm test
```

---

## Step 4: Expose Generated SDK to End Users

### Python: Publish to PyPI

```bash
cd goat-cli/python
maturin build --release
twine upload target/wheels/*
```

### JavaScript: Publish to npm

```bash
cd goat-cli/js/goat
npm publish
```

### R: Publish to CRAN

```bash
cd goat-cli/r
# (R publishing process TBD)
```

---

## Step 5: Keep in Sync with cli-generator Updates

### Monthly check-in

```bash
# Regenerate from latest cli-generator
cd ../cli-generator && git pull origin main
cargo run -- new boat --output-dir /path/to/your-repo --config sites/

# Review changes
git diff goat-cli/
```

### Breaking changes

Subscribe to cli-generator releases and review:
- Template changes
- Rust API changes (if you use it directly)
- Python/JS/R parity updates

---

## Troubleshooting

**Q: Generation fails with "site config not found"**
A: Ensure sites/{site-name}.yaml exists in cli-generator/sites/

**Q: Generated Python extension won't import**
A: Run `maturin develop --features extension-module` again; rebuild is needed after code changes

**Q: JS WASM fails to build**
A: Ensure wasm-pack is installed; set REPO_ROOT env var if generation happens outside cli-generator

---

## Checklists

### Before Integration
- [ ] Site YAML config created
- [ ] Field metadata fetched from live API
- [ ] Generated locally and tested
- [ ] Decision made: commit or CI-generate?

### After Integration
- [ ] SDKs pass all tests (Python, JS, R if applicable)
- [ ] README updated with SDK usage examples
- [ ] Team trained on SDK usage
- [ ] Monitoring set up for API changes

### Before Each Release
- [ ] Regenerate from latest cli-generator (if not CI-generated)
- [ ] All tests pass
- [ ] Breaking changes documented
- [ ] Publish to package managers (PyPI, npm, etc.)

---

## Next Steps

1. [Create your site config in cli-generator](../sites/your-site.yaml)
2. Run generation and test locally
3. Choose commit or CI generation strategy
4. Set up package publishing (PyPI, npm, etc.)
5. Document SDK usage for your team

---

## Related Docs

- [Extension Guide](extension-guide.md) — How to customize the generated SDKs
- [Release Strategy](release-strategy.md) — Version and publishing decisions
- [Troubleshooting](troubleshooting.md) — Common errors and fixes
