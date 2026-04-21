# Releasing cli-generator

This document describes the process for releasing cli-generator to crates.io and GitHub Releases.

## Quick start

```bash
# Bump version, commit, and tag
bash scripts/bump_version.sh patch    # or minor, major

# Push commits and tags (triggers release workflow)
git push origin main
git push origin --tags
```

The GitHub Actions workflow will automatically:
- Build release binaries for Linux and macOS (x86_64 and arm64)
- Create a GitHub Release with downloadable tarballs
- Publish to crates.io

Tags are plain semver (e.g., `1.0.0`, not `v1.0.0`).

## Version numbering

cli-generator uses semantic versioning: `MAJOR.MINOR.PATCH`.

- **Major (1.0.0 → 2.0.0)**: Breaking changes to CLI or generation logic
  - E.g., renaming generated project structure, changing template syntax
  - Users must rerun `cli-generator` to upgrade generated projects
- **Minor (1.0.0 → 1.1.0)**: New generating features or template enhancements (backward compatible)
  - E.g., new snippet language, new QueryBuilder method
  - `cli-generator update` in generated projects still works
- **Patch (1.0.0 → 1.0.1)**: Bug fixes and docs (backward compatible)
  - E.g., template rendering fixes, documentation updates

## Release process

### 1. Prepare the release

Ensure all PRs are merged and CI passing on `main`:

```bash
git checkout main
git pull origin main
cargo test              # Verify all tests pass locally
bash scripts/verify_code.sh  # Run full code quality checks
```

### 2. Update version

The `bump_version.sh` script handles all version updates:

```bash
bash scripts/bump_version.sh patch    # Update Cargo.toml + commit + tag
```

This script:
- Updates `version` in `Cargo.toml` and `crates/genomehubs-query/Cargo.toml`
- Commits with message `chore(release): bump to <VERSION>` (e.g., `1.0.0`)
- Creates annotated git tag `<VERSION>` (plain semver, no `v` prefix)

You can preview the new version before committing:

```bash
# Just show what would be changed (don't commit/tag)
grep '^version = ' Cargo.toml
```

### 3. Push to trigger release workflow

```bash
git push origin main       # Push version bump commit
git push origin --tags     # Push tag (triggers workflow)
```

The GitHub Actions release workflow will:
1. Create a GitHub Release
2. Build release binaries for three platforms
3. Upload tarballs as release assets
4. Publish to crates.io

Monitor progress in [Actions](https://github.com/genomehubs/cli-generator/actions).

### 4. Verify the release

Once the workflow completes:

```bash
# Check GitHub Releases
open https://github.com/genomehubs/cli-generator/releases/tag/<VERSION>

# Verify crates.io
cargo search cli-generator --limit 1
```

## Installation from release

### From crates.io

```bash
cargo install cli-generator  # Installs latest version from crates.io
```

### From GitHub release

```bash
# Download tarball (example for macOS arm64)
wget https://github.com/genomehubs/cli-generator/releases/download/1.0.0/cli-generator-1.0.0-macos-aarch64.tar.gz
tar xzf cli-generator-1.0.0-macos-aarch64.tar.gz
./target/aarch64-apple-darwin/release/cli-generator --help
```

## Troubleshooting

### "Release already published to crates.io"

If the crates.io publish step fails with "duplicate version", check:
1. Was the version already published? Run `cargo search cli-generator`
2. If yes, increment the version and retry: `bash scripts/bump_version.sh patch`

The workflow sets `continue-on-error: true` so it won't block the GitHub Release if crates.io fails.

### "CARGO_REGISTRY_TOKEN not set"

The crates.io publish step requires the `CARGO_REGISTRY_TOKEN` secret in GitHub Actions settings:

1. Go to [Settings → Secrets and variables → Actions](https://github.com/genomehubs/cli-generator/settings/secrets/actions)
2. Create `CARGO_REGISTRY_TOKEN` (get token from [crates.io](https://crates.io/me))
3. Re-run the release workflow

### Dry-run publish locally

To test the publish without actually uploading to crates.io:

```bash
cargo publish --dry-run --token <your-crates-io-token>
```

## Release cadence

cli-generator uses on-demand releases:
- Release when features are complete and tested
- No fixed schedule
- Changes between releases are documented in [CHANGELOG.md](../docs/HISTORY/CHANGELOG.md)

For a history of releases, see [GitHub Releases](https://github.com/genomehubs/cli-generator/releases).
