# Release Strategy

## Overview

Multi-language SDK releases are driven by changes in **two dimensions**:
1. **cli-generator code changes** (core logic, templates, features)
2. **Target API changes** (taxonomic, metadata, or functional API updates at each site)

Each generated project has an **independent semantic version**, decoupled from cli-generator's version. This allows legacy sites (e.g., goat-cli replacement) to revision their SDKs based on API updates without waiting for cli-generator feature releases.

---

## Decision Matrix

| Aspect | Decision | Rationale |
|--------|----------|-----------|
| **Package managers** | All four: PyPI, conda-forge, npm, CRAN | Support all user ecosystems; subset later if maintenance burden is high |
| **Versioning scheme** | Semantic versioning (MAJOR.MINOR.PATCH) per site | Independent from cli-generator version; decouples API changes from tool updates |
| **Version storage** | Generated project's `Cargo.toml` + `pyproject.toml` (or equivalent) | Single source of truth, co-located with release artifacts |
| **Signing** | Production: signed; Development: unsigned | PyPI/CRAN/npm all support unsigned in dev, required in prod |
| **Release cadence** | On-demand, triggered by code or API changes | Prefer control from this repo rather than delegate to generated projects |
| **Change detection** | Polling job: compare cached API state vs. live | Monitor both `main` branch changes + external API drifts |
| **Pre-release testing** | Mandatory: test artifacts before publish | Prevent broken builds from reaching users |

---

## Version Management

### Per-Site Versioning

Each generated project tracks its own version independently:

```toml
# Generated site's Cargo.toml (example: goat-cli)
[package]
name = "goat-cli"
version = "2.1.3"  # Independent from cli-generator version
```

```toml
# Generated site's pyproject.toml (goat_sdk)
[project]
name = "goat-sdk"
version = "2.1.3"  # Mirrors Cargo.toml for consistency
```

**Versioning trigger** (in priority order):
1. **API breaking change** → MAJOR bump (e.g., field removed, endpoint restructured)
2. **API additive change** (new fields, new endpoints) → MINOR bump
3. **cli-generator feature release** (new snippet languages, validation rules) → MINOR bump
4. **Bugfix in cli-generator** (template logic, generated code) → PATCH bump
5. **Bugfix in generated code only** → PATCH bump

### Version Storage in This Repo

Create a **version manifest** file to track all active sites:

```yaml
# docs/planning/sites-version-manifest.yml
sites:
  goat-cli:
    version: "2.1.3"
    last_released: "2026-04-21"
    last_api_hash: "abc123def456"  # Hash of cached API schema
    python_version: "2.1.3"
    npm_version: "2.1.3"
    conda_version: "2.1.3"
    cran_version: "2.1.3"

  boat-cli:
    version: "1.0.0"
    last_released: null
    last_api_hash: null
    python_version: "1.0.0"
    npm_version: "1.0.0"
    conda_version: "1.0.0"
    cran_version: "1.0.0"
```

This manifest:
- Drives CI release decisions (which sites need new builds)
- Tracks API change hashes (detect drift)
- Documents registry versions (verify publish success)

---

## Release Process Workflow

### 1. Change Detection (Polling Job)

**Trigger**: Scheduled daily + on push to `main`

**Steps**:
```bash
# 1. Pull latest API schemas (or taxonomic data) from target sites
# 2. Compare against cached version in docs/api-schemas/
# 3. If hash differs, flag as "API changed"
# 4. If main branch changed, regenerate and compare SDKs
# 5. Output: list of sites needing new releases
```

**Cached API state location**:
```
docs/api-schemas/
  goat-api-schema.json      # Cached at last release
  boat-api-schema.json
  (etc.)
```

Decision: Create per-site polling logic or shared polling framework?
→ **Shared framework** (one polling job, per-site config), parameterized by `sites-version-manifest.yml`.

### 2. Release Readiness Check

**Conditions to proceed**:
- ✅ Changes detected (cli-generator OR API)
- ✅ All tests pass (unit + integration + fixture tests)
- ✅ No merge conflicts with existing generated projects
- ✅ Version bump approved in manifest (manual or auto-suggest)

**Manual override option**:
```bash
# Force release of a specific site (e.g., after manual testing)
gh workflow run release.yml -f site=goat-cli -f version=2.2.0
```

### 3. Artifact Building (Parallelizable)

**For each site** (in parallel):

```bash
# Regenerate SDKs from templates
maturin build --features extension-module  # Python wheel
npm pack                                    # JavaScript tgz
R CMD build                                 # R tar.gz
conda build                                 # Conda package (or conda-forge PR)
```

**Output**: Signed artifacts (production) or unsigned (dev)

### 4. Artifact Testing (Before Publish)

**Test each artifact in ephemeral environments**:

```bash
# Python: pip install from wheel, run quick smoke test
# JavaScript: npm install from tgz, run Node test
# R: R CMD check on tar.gz, run R script test
# Conda: mamba install from package, verify imports
```

**Smoke test template**: Verify all exported functions importable + basic queries work

**Failure recovery**: If any artifact fails, halt and report; do not publish others.

### 5. Publishing to Registries

**Sequence** (all-or-nothing per site):
1. **PyPI** (primary Python source)
2. **conda-forge** (via PR or direct publish if account available)
3. **npm** (npm registry)
4. **CRAN** (or manual submission if repo not available)

**Rollback strategy**: Keep previous version tags on all registries for 30 days before cleanup.

**Update manifest** after successful publish:
```yaml
sites:
  goat-cli:
    version: "2.2.0"
    last_released: "2026-04-25"
    last_api_hash: "xyz789abc123"
    python_version: "2.2.0"
    npm_version: "2.2.0"
    (etc.)
```

---

## Signing & Credentials

### Production Signing

**Python (PyPI)**:
- Sign wheels with `sigstore` (distutils toolchain)
- Store OIDC credentials in GitHub org secrets (no manual key rotation)

**JavaScript (npm)**:
- Sign packages with `npm provenance` (OIDC attestation via GitHub)
- Requires npm account with provenance feature enabled

**R (CRAN)**:
- CRAN maintainer email required for submissions
- Signing optional; CRAN validates R CMD check results

**Conda**:
- Conda-forge enforces code review (pull request workflow)
- No per-package signing; trust via org review

### Development Signing

**Unsigned builds** allowed in dev workflows:
```yaml
# CI flag
if: github.ref == 'refs/heads/main'  # Only sign on main branch
  run: sigstore sign ...
else
  echo "Skipping signature (dev build)"
```

---

## API Polling Strategy

### Concept

Instead of waiting for users to report API changes, proactively detect when:
- New taxonomic fields appear (goat: new taxa, rank levels)
- Metadata schema changes (field renames, new types)
- Endpoint structure changes (new filters, response format)

### Implementation

**Per-site polling config** (in manifest or separate file):

```yaml
# docs/planning/polling-config.yml
sites:
  goat:
    discovery_endpoint: "https://goat.genomehubs.org/api/v2/_search"
    schema_endpoint: "https://goat.genomehubs.org/api/v2/schema"
    poll_interval: "daily"
    api_version_key: "meta.api_version"  # If API reports its version

  boat:
    discovery_endpoint: "https://boat.org/api/v0/metadata"
    poll_interval: "weekly"
```

**Polling job steps**:
1. Fetch live schema from each endpoint
2. Hash it and compare to cached version in `docs/api-schemas/`
3. If hash differs:
   - Flag the site for regeneration
   - Suggest MINOR or MAJOR bump (heuristic: breaking if fields removed)
   - Create issue/PR summarizing API change
4. If `main` branch also changed: regenerate all sites, compare SHAs

**Example output**:
```
Site: goat
API change: DETECTED (field 'description' added to taxon_name_statement)
Last cached: 2026-04-01
Current hash: abc123xyz789
Suggested bump: MINOR (2.1.3 → 2.2.0)
---
Regenerating goat SDKs... ✓
Artifact sizes changed:
  Python wheel: 1.2 MB → 1.3 MB
  npm tgz: 840 KB → 920 KB
Ready to publish? (manual approval required)
```

### Decision Point

**Should polling job auto-commit version bumps, or require manual approval?**

→ **Recommendation: Require manual approval** (via PR or GitHub issue)
- Prevents surprise version bumps in registries
- Gives time to document API changes in release notes
- Allows batching multiple site releases if desired

---

## Publishing Workflow (High-Level)

### Trigger Event

**Option A: Manual trigger**
```bash
# User explicitly requests release for one or more sites
gh workflow run release.yml \
  -f sites='["goat-cli", "boat-cli"]' \
  -f versions='["2.2.0", "1.0.1"]'
```

**Option B: Automatic (on approval)**
```
1. Polling job detects change → creates PR with updated manifest
2. User reviews PR + approves
3. Merge to main → automated CI job publishes artifacts
```

**Recommendation: Hybrid**
- Polling job detects changes but requires manual approval (label or comment)
- Manual trigger available for ad-hoc releases (e.g., security patches)

### CI Job Structure (Skeleton)

```yaml
# .github/workflows/release.yml (template, not active yet)

name: Release SDKs

on:
  workflow_dispatch:
    inputs:
      sites:
        description: "Sites to release (comma-separated or 'all')"
        required: true
      versions:
        description: "Versions (same order as sites, or 'auto-bump')"
        required: false

jobs:
  detect-changes:
    name: Detect SDK Changes
    runs-on: ubuntu-latest
    outputs:
      sites_to_release: ${{ steps.detect.outputs.sites }}
    steps:
      - uses: actions/checkout@v4
      - name: Compare manifests
        id: detect
        run: |
          # Parse sites-version-manifest.yml
          # Compare against live APIs and main branch changes
          # Output: JSON list of sites needing release
          echo "sites=['goat-cli']" >> $GITHUB_OUTPUT

  test-artifacts:
    name: Test ${{ matrix.site }} Artifacts
    needs: detect-changes
    strategy:
      matrix:
        site: ${{ fromJson(needs.detect-changes.outputs.sites_to_release) }}
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Generate SDK
        run: cargo run -- new --site ${{ matrix.site }} --output /tmp/${{ matrix.site }}
      - name: Build Python wheel
        run: cd /tmp/${{ matrix.site }} && maturin build --features extension-module
      - name: Test Python wheel
        run: |
          pip install /tmp/${{ matrix.site }}/target/wheels/*.whl
          python -c "from ${{ matrix.site }}_sdk import QueryBuilder; print('✓ Python import OK')"
      - name: Build npm package
        run: cd /tmp/${{ matrix.site }} && npm pack
      - name: Test npm package
        run: |
          npm install /tmp/${{ matrix.site }}/${{ matrix.site }}-sdk-*.tgz
          node -e "const {QueryBuilder} = require('${{ matrix.site }}-sdk'); console.log('✓ npm import OK')"
      - name: Build R package
        run: cd /tmp/${{ matrix.site }} && R CMD build .
      - name: Test R package
        run: |
          R CMD check /tmp/${{ matrix.site }}/DESCRIPTION
          R -e "library('goat.sdk'); cat('✓ R load OK')"
      - name: Upload artifacts
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.site }}-artifacts
          path: |
            /tmp/${{ matrix.site }}/target/wheels/*.whl
            /tmp/${{ matrix.site }}/*.tgz
            /tmp/${{ matrix.site }}/*.tar.gz

  publish:
    name: Publish ${{ matrix.site }}
    needs: [detect-changes, test-artifacts]
    strategy:
      matrix:
        site: ${{ fromJson(needs.detect-changes.outputs.sites_to_release) }}
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Download artifacts
        uses: actions/download-artifact@v4
        with:
          name: ${{ matrix.site }}-artifacts
      - name: Publish to PyPI
        env:
          PYPI_TOKEN: ${{ secrets.PYPI_TOKEN }}
        run: |
          # pip install twine
          # twine upload --skip-existing *.whl
          echo "TODO: Implement PyPI publish (requires PYPI_TOKEN)"
      - name: Publish to npm
        env:
          NPM_TOKEN: ${{ secrets.NPM_TOKEN }}
        run: |
          # npm set //registry.npmjs.org/:_authToken=$NPM_TOKEN
          # npm publish *.tgz
          echo "TODO: Implement npm publish (requires NPM_TOKEN)"
      - name: Publish to CRAN
        env:
          CRAN_EMAIL: ${{ secrets.CRAN_EMAIL }}
        run: |
          echo "TODO: Implement CRAN submission (manual or auto-submit)"
      - name: Update manifest
        run: |
          # Update docs/planning/sites-version-manifest.yml
          # Update last_released, last_api_hash
          git add docs/planning/sites-version-manifest.yml
          git commit -m "chore: release ${{ matrix.site }} v$(cat VERSION)"
          git push

  notify:
    name: Notify Release
    needs: publish
    runs-on: ubuntu-latest
    if: always()  # Run even if publish fails
    steps:
      - name: Create GitHub release
        run: |
          # gh release create v... --generate-notes
          echo "TODO: Implement GitHub release creation + changelog"
      - name: Slack notification
        env:
          SLACK_WEBHOOK: ${{ secrets.SLACK_WEBHOOK }}
        run: |
          # Post to Slack: "goat-cli v2.2.0 released to PyPI, npm, conda"
          echo "TODO: Implement Slack notification"
```

---

## Registry Setup Checklist

- [ ] **PyPI**: Account created, `PYPI_TOKEN` (fine-grained) stored in GitHub org secrets
- [ ] **conda-forge**: Feedstock created OR direct access to conda account
- [ ] **npm**: Account created, public scope `@genomehubs/`, `NPM_TOKEN` stored (provenance enabled)
- [ ] **CRAN**: Maintainer email registered, submission process documented
- [ ] **sigstore**: OIDC federation enabled for PyPI + npm (no manual key rotation)
- [ ] **GitHub**: Default branch protection requires PR reviews before publish job runs
- [ ] **Manifest file**: `docs/planning/sites-version-manifest.yml` created and tracked

---

## Rollout Plan (Post-MVP)

### Phase 1: Single Site (1–2 weeks)
- Set up PyPI + npm publishing for goat-cli
- Manual trigger only
- Test publish → verify downloads work → test imported SDKs

### Phase 2: Add Remaining Registries (2–3 weeks)
- Add conda-forge publishing
- Add CRAN publishing (or plan manual submission process)
- Set up signing for production builds

### Phase 3: API Polling (2–3 weeks)
- Implement polling job for API schema drift
- Set up manifest file + version suggestions
- Test with live goat + boat APIs

### Phase 4: Full Automation (1–2 weeks)
- Enable automatic PR creation when changes detected
- Set up approval workflow
- Document for external contributors

---

## Known Decisions Deferred

- **Artifact storage**: GitHub Releases vs. artifact registry? (Use GitHub Releases for now; migrate to Artifactory if volumes grow)
- **Conda-forge approval latency**: PR-based workflow may have review delays—acceptable for now
- **CRAN submission delays**: Manual step; plan to automate if submissions become frequent
- **Versioning for beta releases**: Semver allows `-beta.1` suffix—document policy when needed
- **Security: Artifact verification**: Consider how users verify downloaded wheels/packages (sigstore attestations help here)

---

## Related Documents

- [sites-version-manifest.yml](./sites-version-manifest.yml) — Master version tracking (to be created)
- [polling-config.yml](./polling-config.yml) — API change detection config (to be created)
- `.github/workflows/release.yml` — Release automation job (skeleton provided above, activate post-MVP testing)
- [GETTING_STARTED.md](../../GETTING_STARTED.md) — Update with install instructions for each package manager
