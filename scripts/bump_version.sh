#!/usr/bin/env bash

set -euo pipefail

# Version bump script for cli-generator.
# Usage: bash scripts/bump_version.sh major|minor|patch
#
# Examples:
#   bash scripts/bump_version.sh major    # 0.1.0 -> 1.0.0, tag v1.0.0, commit
#   bash scripts/bump_version.sh minor    # 0.1.0 -> 0.2.0, tag v0.2.0, commit
#   bash scripts/bump_version.sh patch    # 0.1.0 -> 0.1.1, tag v0.1.1, commit
#
# After running, push with: git push origin main && git push origin --tags

if [[ $# -ne 1 ]]; then
    echo "Usage: $0 major|minor|patch"
    echo ""
    echo "Examples:"
    echo "  $0 major    # Bump major version (x.0.0)"
    echo "  $0 minor    # Bump minor version (0.x.0)"
    echo "  $0 patch    # Bump patch version (0.0.x)"
    exit 1
fi

BUMP_TYPE="$1"

if [[ ! "$BUMP_TYPE" =~ ^(major|minor|patch)$ ]]; then
    echo "Error: bump type must be major, minor, or patch"
    exit 1
fi

# Check we're in the repo root
if [[ ! -f "Cargo.toml" ]]; then
    echo "Error: Cargo.toml not found. Run from repo root."
    exit 1
fi

# Check git is clean
if [[ -n $(git status -s) ]]; then
    echo "Error: Working directory has uncommitted changes. Stash or commit first."
    exit 1
fi

# Extract current version from main Cargo.toml
CURRENT_VERSION=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
echo "Current version: $CURRENT_VERSION"

# Parse version components
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"
PATCH="${PATCH%%-*}"  # Strip any pre-release suffix

# Bump the appropriate component
case "$BUMP_TYPE" in
    major)
        NEW_VERSION="$((MAJOR + 1)).0.0"
        ;;
    minor)
        NEW_VERSION="$MAJOR.$((MINOR + 1)).0"
        ;;
    patch)
        NEW_VERSION="$MAJOR.$MINOR.$((PATCH + 1))"
        ;;
esac

echo "New version: $NEW_VERSION"

# Update main Cargo.toml
sed -i.bak "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" Cargo.toml
rm -f Cargo.toml.bak

# Update crates/genomehubs-query/Cargo.toml (same version for consistency)
QUERY_TOML="crates/genomehubs-query/Cargo.toml"
if [[ -f "$QUERY_TOML" ]]; then
    QUERY_CURRENT=$(grep '^version = ' "$QUERY_TOML" | head -1 | sed 's/version = "\(.*\)"/\1/')
    sed -i.bak "s/^version = \"$QUERY_CURRENT\"/version = \"$NEW_VERSION\"/" "$QUERY_TOML"
    rm -f "$QUERY_TOML.bak"
    echo "Updated $QUERY_TOML to $NEW_VERSION"
fi

# Commit
git add Cargo.toml crates/genomehubs-query/Cargo.toml 2>/dev/null || git add Cargo.toml
git commit -m "chore(release): bump to v$NEW_VERSION"
echo "✓ Committed version bump"

# Tag
TAG="$NEW_VERSION"
git tag -a "$TAG" -m "Release $TAG"
echo "✓ Tagged as $TAG"

echo ""
echo "Version bump complete! To finish:"
echo ""
echo "  git push origin main"
echo "  git push origin --tags"
echo ""
echo "The release workflow will automatically:"
echo "  - Build release binary"
echo "  - Create GitHub Release"
echo "  - Publish to crates.io (if CARGO_REGISTRY_TOKEN is set)"
