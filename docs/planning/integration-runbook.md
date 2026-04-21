# Integration Runbook

Generate and integrate cli-generator SDKs into an existing project (boat-cli, assessment-api, etc.).

**Status**: 📋 PLANNING (comprehensive v1 created 2026-04-21)
**Audience**: Repository maintainers onboarding to cli-generator
**Effort**: 2–3 hours for initial integration; 30 minutes for subsequent sites
**Time Estimate**: Follow this runbook yourself: ~2–3 hours; redo for a second site: ~1 hour

---

## Overview

This guide walks step-by-step through:
1. **Preparing** your site's API configuration
2. **Generating** language-specific SDKs (Python, JavaScript, R)
3. **Testing** artifacts before integration
4. **Integrating** into your project structure
5. **Versioning & updates** strategy
6. **Publishing** to registries (PyPI, npm, CRAN)
7. **Automating** API change detection

By the end, your project will have generated SDKs ready for users, with policies for keeping them in sync.

---

## Prerequisites

Before starting, ensure you have:

- [ ] **Rust toolchain** (`rustc`, `cargo`, MSRV 1.70+)
- [ ] **Python 3.9+** with `maturin` + `build` packages installed
- [ ] **Node.js 18+** with npm 9+
- [ ] **R 4.0+** (if generating R SDK)
- [ ] **cli-generator cloned locally** from genomehubs/cli-generator
- [ ] **Your API accessible** (for schema discovery and testing)
- [ ] **Your project repo cloned** (boat-cli, assessment-api, etc.)

### Verify Prerequisites

```bash
rustc --version      # Rust compiler
cargo --version      # Cargo package manager
python3 --version    # Python 3.9+
npm --version        # Node package manager
R --version          # R (optional, only if generating R SDK)

# Verify Python build tools
python3 -m pip list | grep -E 'maturin|build'
```

---

## Key Decisions Before Starting

**1. Deployment Model: Commit or CI-Generate?**

| Decision | Commit Generated Files | CI-Generate on Demand |
|----------|------------------------|----------------------|
| **Files in repo** | Yes (python/, js/, rust/) | No (generated in CI only) |
| **CI time** | Faster (skip generation) | Slower (regenerate each commit) |
| **Review burden** | Higher (large diffs) | Lower (no generated code in PRs) |
| **Reproducibility** | High (snapshot) | High (deterministic generation) |
| **User experience** | Install from committed files | Install from CI artifacts |
| **Recommended** | For stable projects | For rapid iteration projects |

→ **Recommendation**: Start with **commit** for simplicity; switch to **CI-generate** later if diffs become unwieldy.

**2. Versioning Strategy**

Each generated SDK has an **independent semantic version**, separate from cli-generator version.

**When to bump versions**:
- **MAJOR**: API breaking change (field removed, endpoint restructured)
- **MINOR**: API additive change (new fields/endpoints), or cli-generator feature release
- **PATCH**: Bugfix in generated code or cli-generator templates

See [release-strategy.md](./release-strategy.md) for detailed versioning policy.

**3. API Change Detection**

Will you:
- [ ] Poll for API changes automatically (recommended) → see [polling-config.yml](./polling-config.yml)
- [ ] Update SDKs manually when you remember
- [ ] Hybrid: humans detect changes, polling validates

---

## Step 1: Prepare Your Site Configuration

### 1a. Create a Site Config in cli-generator Repo

**Location** (in cli-generator): `config/{site-name}.yaml`

For example, `config/boat.yaml`:

```yaml
---
name: boat
api_base: "https://boat.genomehubs.org/api"
api_version: "v1"
description: "Boat of Life Assembly metadata API"

# Where to fetch live schema (for polling + fixture generation)
schema_endpoint: "/search"  # Full URL: api_base + schema_endpoint
schema_format: "graphql"    # or "json-schema", "rest-discovery"

# Package metadata for generated projects
python:
  package_name: "boat_sdk"
  author: "GenomeHubs"
  license: "MIT"

javascript:
  package_scope: "@genomehubs"
  package_name: "boat-sdk"
  author: "GenomeHubs"

r:
  package_name: "boat.sdk"
  author: "GenomeHubs"
```

### 1b. Fetch Live API Schema

Get your API's schema to ensure field metadata is current:

```bash
# REST endpoint that returns field definitions
curl "https://boat.genomehubs.org/api/v1/search?fields=true" \
  -H "Accept: application/json" \
  > /tmp/boat-schema.json

# Validate it's valid JSON
jq . /tmp/boat-schema.json | head -20
```

Save this for later: you'll use it for fixture generation and API change polling.

---

## Step 2: Generate SDKs

### 2a. Run cli-generator

From the cli-generator repo:

```bash
cd /path/to/cli-generator

# Generate for a single site (all languages)
cargo run --release -- new boat \
  --config config/boat.yaml \
  --output-dir /tmp/boat-sdks/

# Or generate multiple sites
cargo run --release -- new \
  --config config/ \
  --output-dir /tmp/sdks/
```

**Output structure**:
```
/tmp/boat-sdks/
  boat-cli/              # CLI (Rust binary)
    Cargo.toml
    src/main.rs
    ...
  python/                # Python SDK
    boat_sdk/
    pyproject.toml
    tests/
  js/                    # JavaScript SDK
    package.json
    src/
    test/
  r/                     # R SDK
    DESCRIPTION
    R/
    tests/
```

### 2b. (Optional) Customize Generated Code

The generated code is production-ready as-is. Customization is **optional** and should be minimal.

If you need to customize:
- **Parameter validation**: Add rules to generated `validator.rs` (Rust)
- **Documentation**: Update docstrings in generated modules
- **Snippet languages**: Add custom query templates to `templates/snippets/`
- **Custom types**: Extend `QueryBuilder` constructor for site-specific params

**Do NOT edit**:
- Core generation logic (stays in cli-generator repo)
- Template files (update in cli-generator if needed)
- Build configuration (Cargo.toml, package.json versioning logic)

---

## Step 3: Test Generated SDKs Locally

Before committing/integrating, verify each language works independently.

### Python Testing

```bash
cd /tmp/boat-sdks/python

# Install in development mode
pip install -e .
# Or with maturin
maturin develop --features extension-module

# Run tests
pytest tests/ -v

# Quick smoke test
python3 -c "
from boat_sdk import QueryBuilder
qb = QueryBuilder(index='boat')
qb.add_query(query_type='taxon', ranks=['family'])
print('✓ Python SDK loads and functions work')
"
```

### JavaScript Testing

```bash
cd /tmp/boat-sdks/js

# Install dependencies
npm install

# Run tests
npm test

# Quick smoke test
node -e "
const {QueryBuilder} = require('./dist/index.js');
const qb = new QueryBuilder({index: 'boat'});
console.log('✓ JavaScript SDK loads');
"
```

### R Testing

```bash
cd /tmp/boat-sdks/r

# Build and check
R CMD build .
R CMD check boat.sdk_*.tar.gz

# Quick smoke test
R -e "
library('boat.sdk')
qb <- QueryBuilder(index = 'boat')
cat('✓ R SDK loads\n')
"
```

### Integration Test: Live API Query

Test against your actual API:

```python
# Python example
from boat_sdk import QueryBuilder

qb = QueryBuilder(
    index='boat',
    api_base='https://boat.genomehubs.org/api'
)
qb.add_query(query_type='taxon', ranks=['family'])
qb.set_limit(10)

results = qb.fetch()
print(f"✓ Live query returned {len(results)} results")
```

---

## Step 4: Integration into Your Project

### Option A: Commit Generated Files (Recommended for MVP)

```bash
# Copy to your repo
cp -r /tmp/boat-sdks/{python,js,r} /path/to/boat-cli/

cd /path/to/boat-cli

# Update .gitignore to allow generated code
cat >> .gitignore << 'EOF'
# Allow generated SDKs
!python/
!js/
!r/
EOF

# Commit
git add python/ js/ r/
git commit -m "feat: add generated SDKs (boat-sdk v1.0.0)"
```

**Project structure after**:
```
boat-cli/
  cli/           # Your handwritten CLI code
    src/main.rs
  python/        # Generated Python SDK
    boat_sdk/
    pyproject.toml
  js/            # Generated JavaScript SDK
    package.json
  r/             # Generated R SDK
    DESCRIPTION
  docs/
    ...
```

### Option B: CI-Generate on Demand (For Later)

Add this to `.github/workflows/generate-sdks.yml` (configure _after_ MVP):

```yaml
name: Generate SDKs

on:
  push:
    branches: [main]
    paths:
      - config/boat.yaml
      - 'cli-generator/*'
  workflow_dispatch:

jobs:
  generate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Checkout cli-generator
        uses: actions/checkout@v4
        with:
          repository: genomehubs/cli-generator
          ref: main
          path: cli-generator

      - uses: dtolnay/rust-toolchain@stable

      - name: Generate SDKs
        run: |
          cd cli-generator
          cargo run --release -- new boat \
            --config ../config/boat.yaml \
            --output-dir /tmp/sdks/

      - name: Upload artifacts
        uses: actions/upload-artifact@v4
        with:
          name: boat-sdks
          path: |
            /tmp/sdks/python/
            /tmp/sdks/js/
            /tmp/sdks/r/
          retention-days: 30
```

---

## Step 5: Versioning & Update Strategy

### Per-Project Version Manifest

Create `docs/SDK-VERSION-MANIFEST.md` in your project:

```markdown
# Boat SDK Versions

| Language | Version | Released | cli-generator | API State |
|----------|---------|----------|---------------|-----------|
| Python   | 1.0.0   | 2026-04-21 | main @ abc123 | hash-xyz |
| JavaScript | 1.0.0 | 2026-04-21 | main @ abc123 | hash-xyz |
| R        | 1.0.0   | 2026-04-21 | main @ abc123 | hash-xyz |
```

Update this whenever you regenerate.

### When to Regenerate

**Regenerate SDKs when**:
1. **API changes detected** (new fields, endpoint changes) → MINOR or MAJOR bump
2. **cli-generator releases a feature** you want → MINOR bump
3. **cli-generator bugfix** → PATCH bump

**Do NOT regenerate for**:
- Minor doc fixes in cli-generator
- Unrelated cli-generator features you don't use
- Cosmetic code changes

### Update Process (Manual, Post-MVP)

```bash
# 1. Get latest cli-generator
cd /path/to/cli-generator
git pull origin main

# 2. Regenerate
cargo run --release -- new boat \
  --config config/boat.yaml \
  --output-dir /path/to/boat-cli/

# 3. Test
cd /path/to/boat-cli
bash scripts/verify_code.sh   # Or language-specific test scripts

# 4. Commit
git add python/ js/ r/
git commit -m "chore: regenerate SDKs (cli-generator @ <hash>, API hash-xyz)"

# 5. Bump version + publish (see Step 6)
```

See [release-strategy.md](./release-strategy.md) for full publishing workflow.

---

## Step 6: Publishing to Registries (Post-MVP)

Once SDKs are stable and tested by initial users, publish to package managers.

### Python → PyPI

```bash
cd python/

# Build wheel
pip install build
python -m build

# Upload (requires PyPI token)
pip install twine
twine upload dist/*.whl

# Or use GitHub Actions (see release-strategy.md)
```

### JavaScript → npm

```bash
cd js/

# Configure npm credentials
npm login

# Publish
npm publish

# Verify
npm view @genomehubs/boat-sdk versions
```

### R → CRAN

```bash
cd r/

# Build and check
R CMD build .
R CMD check boat.sdk_*.tar.gz

# Submit to CRAN
# https://cran.r-project.org/submit.html
```

See [release-strategy.md](./release-strategy.md) for credential management and signing.

---

## Step 7: API Change Detection (Optional, Recommended)

Set up automated monitoring so you know when your API changes before users do.

### 1. Cache Current API Schema

After first release, save your API's current schema:

```bash
curl "https://boat.genomehubs.org/api/v1/search?fields=true" \
  > docs/api-schemas/boat-api-schema.json
```

Commit this: it's your baseline.

### 2. Update polling-config.yml

In cli-generator repo, `docs/planning/polling-config.yml`:

```yaml
sites:
  boat:
    enabled: true
    discovery_endpoint: "https://boat.genomehubs.org/api/v1/search"
    poll_interval: "weekly"
    timeout_seconds: 30
```

### 3. Polling Job (Runs in cli-generator CI)

The cli-generator's polling job will:
- Fetch current boat API schema weekly
- Compare hash against cached version
- Create PR suggesting version bump if changed
- Flag for regeneration and re-testing

You'll be notified via PR when changes occur.

---

## Step 8: CI/CD Template for Your Project

Add these GitHub Actions workflows to `{your-project}/.github/workflows/`:

### Test Generated SDKs on Every Push

```yaml
name: SDK Tests

on: [push, pull_request]

jobs:
  python:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v4
        with:
          python-version: "3.11"
      - run: cd python && pip install -e . && pytest tests/ -v

  javascript:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: "20"
      - run: cd js && npm install && npm test

  r:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: r-lib/actions/setup-r@v2
      - run: cd r && R CMD build . && R CMD check boat.sdk_*.tar.gz
```

### Manual Release Workflow (Post-MVP)

See [release-strategy.md](./release-strategy.md) for complete `.github/workflows/release.yml` template.

---

## Step 9: Troubleshooting

### "Generated code doesn't import"

**Symptom**: `ModuleNotFoundError: No module named 'boat_sdk'`

**Solution**:
```bash
# Rebuild the extension
cd python/
maturin develop --features extension-module

# Or reinstall
pip install -e .
```

### "Tests fail on new API fields"

**Symptom**: Tests expect a field that no longer exists in API

**Solution**:
1. Regenerate fixtures
2. Update test cases
3. Re-run tests

```bash
bash scripts/test_sdk_fixtures.sh --site boat --python
```

### "WASM not found in JavaScript SDK"

**Symptom**: `Error: Cannot find module './wasm'`

**Solution**:
```bash
cd js/
npm install
npm run build  # Rebuild WASM bindings
```

### "R CMD check warnings about dependencies"

**Symptom**: `Warning: Dependencies are not installed`

**Solution**:
```bash
# Install R dependencies
R -e "install.packages(c('R6', 'jsonlite'))"

# Then check
R CMD check boat.sdk_*.tar.gz
```

### "Version mismatch across languages"

**Symptom**: Python is v1.0.0, JS is v0.9.5

**Solution**:
Regenerate all at once from same cli-generator commit, ensure all `pyproject.toml`, `package.json`, `DESCRIPTION` have identical version strings.

---

## Worked Example: Boat-CLI Replacement

Boat-CLI is a legacy implementation that will be replaced with a generated version. Here's how to do it:

### Context

- **Old boat-cli**: Hardcoded Rust CLI based on boat API
- **New boat-cli**: Uses generated Python/JS/R SDKs + generated Rust CLI
- **Legacy**: Some users rely on old boat-cli; need smooth transition

### Step-by-Step

#### 1. Create config/boat.yaml in cli-generator

```yaml
name: boat
api_base: "https://boat.genomehubs.org/api"
description: "Boat of Life Assembly metadata API (generated)"

python:
  package_name: "boat_sdk"

javascript:
  package_scope: "@genomehubs"
  package_name: "boat-sdk"

r:
  package_name: "boat.sdk"
```

#### 2. Generate SDKs

```bash
cd /path/to/cli-generator
cargo run --release -- new boat --config config/boat.yaml --output-dir /tmp/boat-gen/
```

#### 3. Create new boat-cli Repo (or branch)

```bash
# Fresh repo for generated version
mkdir /path/to/boat-cli-generated
cd /path/to/boat-cli-generated
git init
```

#### 4. Copy Generated CLI + SDKs

```bash
# Copy generated CLI structure
cp -r /tmp/boat-gen/boat-cli/* .

# Directory structure
boat-cli-generated/
  Cargo.toml        # Generated Rust CLI
  src/
    main.rs
  python/           # Generated Python SDK
  js/               # Generated JavaScript SDK
  r/                # Generated R SDK
  docs/
    CLI.md          # Auto-generated from Rust docstrings
```

#### 5. Integrate Legacy Features

If old boat-cli had custom commands/validations:

```bash
# Copy custom validation rules
cp /path/to/old-boat-cli/src/validation.rs src/custom_validation.rs

# Edit src/main.rs to call custom validation
```

#### 6. Test

```bash
# CLI smoke test
cargo run -- taxon search --help

# SDK tests
cd python && pytest tests/ -v
cd ../js && npm test
cd ../r && R CMD check boat.sdk_*.tar.gz
```

#### 7. Release

```bash
# Version: initial release from generated code
git tag v2.0.0
git push origin v2.0.0

# Publish SDKs
cd python && python -m build && twine upload dist/*.whl
cd ../js && npm publish
cd ../r && R CMD build . && # submit to CRAN
```

### Migration Path for Users

```markdown
## Upgrade from boat-cli v1 → v2

Old boat-cli (v1) is hardcoded; new boat-cli (v2) is generated.

### For CLI users:
- Commands are mostly compatible; see `boat search --help`
- Report issues to boat-cli repo

### For SDK users:
- Install from new registries
- Python: `pip install boat-sdk` (instead of custom install)
- JavaScript: `npm install @genomehubs/boat-sdk`
- API is identical; no code changes needed
```

---

## Worked Example: Assessment-API Integration

Assessment-API is a smaller service that doesn't have its own CLI, just SDKs.

### Context

- **assessment-api**: REST API for genomic assessments
- **Use case**: Data scientists import `assessment_sdk` into Jupyter notebooks
- **Goal**: Generate Python/JS/R SDKs, users install from PyPI/npm/CRAN

### Step-by-Step

#### 1. Create config/assessment-api.yaml

```yaml
name: assessment-api
api_base: "https://api.assessment.genomehubs.org"
description: "GenomeHubs Assessment API"

python:
  package_name: "assessment_sdk"
  author: "Assessment Team"

javascript:
  package_scope: "@genomehubs"
  package_name: "assessment-api"

r:
  package_name: "assessment.api"
```

#### 2. Generate (All Languages)

```bash
cargo run --release -- new assessment-api \
  --config config/assessment-api.yaml \
  --output-dir /tmp/assessment-sdks/
```

#### 3. Minimal Repo Structure

Unlike boat-cli, assessment-api doesn't have a CLI component:

```
assessment-api-sdks/
  python/              # Users install: pip install assessment-sdk
  js/                  # Users install: npm install @genomehubs/assessment-api
  r/                   # Users install: devtools::install_github("genomehubs/assessment-api")
  docs/
    README.md          # Installation + quick start
    CHANGELOG.md
```

#### 4. Test (focused on SDKs, no CLI testing)

```bash
cd python && pytest tests/ -v
cd ../js && npm test
cd ../r && R CMD check assessment.api_*.tar.gz
```

#### 5. Publish SDKs Only

```bash
# Python to PyPI
cd python && twine upload dist/*.whl

# JavaScript to npm
cd ../js && npm publish

# R: Host on GitHub releases or submit to CRAN
```

#### 6. Users Can Install

```bash
# Python data scientist
pip install assessment-sdk
python -c "from assessment_sdk import QueryBuilder; print('✓ Works!')"

# Node developer
npm install @genomehubs/assessment-api
node -e "const {QueryBuilder} = require('@genomehubs/assessment-api'); console.log('✓ Works!');"

# R analyst
install.packages("assessment.api", repos="https://cran.genomehubs.org")
library(assessment.api)
```

---

## Summary Checklist

- [ ] **Prerequisites installed** (Rust, Python, Node, R)
- [ ] **Site config created** (`config/{site}.yaml`)
- [ ] **SDKs generated** and tested locally
- [ ] **Committed or CI-generated** (decide deployment model)
- [ ] **Tests passing** (all languages)
- [ ] **Version manifest created** (track versions)
- [ ] **API schema cached** (for polling)
- [ ] **CI/CD workflows added** (test on every push)
- [ ] **Published to registries** (post-MVP, guided by release-strategy.md)
- [ ] **Polling configured** (so you know when API changes)
- [ ] **Documentation updated** (users can find + install SDKs)

---

## Next Steps

1. **Follow this runbook** for your first site (1–2 hours)
2. **Repeat for second site** (30–60 minutes, faster)
3. **Post-MVP**: Flesh out **release-strategy.md** with actual registry credentials + CI automation
4. **Ongoing**: Monitor polling for API changes; regenerate when needed

---

## Related Documents

- [release-strategy.md](./release-strategy.md) — Full publishing + versioning policy
- [polling-config.yml](./polling-config.yml) — API change detection configuration
- [sites-version-manifest.yml](./sites-version-manifest.yml) — Version tracking template
- [GETTING_STARTED.md](../../GETTING_STARTED.md) — Quick start (for users installing SDKs)

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
