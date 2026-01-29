#!/bin/bash
set -euo pipefail

# Release script for janus
# Usage: ./release.sh <major|minor|patch>

if [[ $# -ne 1 ]]; then
    echo "Usage: $0 <major|minor|patch>" >&2
    exit 1
fi

BUMP_TYPE="$1"

if [[ "$BUMP_TYPE" != "major" && "$BUMP_TYPE" != "minor" && "$BUMP_TYPE" != "patch" ]]; then
    echo "Error: argument must be 'major', 'minor', or 'patch'" >&2
    exit 1
fi

# Read current version from Cargo.toml
CURRENT_VERSION=$(grep -m1 '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/')

if [[ -z "$CURRENT_VERSION" ]]; then
    echo "Error: could not read version from Cargo.toml" >&2
    exit 1
fi

# Parse version components
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"

# Calculate new version
case "$BUMP_TYPE" in
    major)
        NEW_MAJOR=$((MAJOR + 1))
        NEW_MINOR=0
        NEW_PATCH=0
        ;;
    minor)
        NEW_MAJOR=$MAJOR
        NEW_MINOR=$((MINOR + 1))
        NEW_PATCH=0
        ;;
    patch)
        NEW_MAJOR=$MAJOR
        NEW_MINOR=$MINOR
        NEW_PATCH=$((PATCH + 1))
        ;;
esac

NEW_VERSION="${NEW_MAJOR}.${NEW_MINOR}.${NEW_PATCH}"

echo "Bumping version: $CURRENT_VERSION -> $NEW_VERSION"

# Update version in Cargo.toml
sed -i '' "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" Cargo.toml

# Run cargo check to update Cargo.lock
echo "Running cargo check..."
cargo check

# Create commit
echo "Creating commit..."
git add Cargo.toml Cargo.lock
git commit -m "chore: release v$NEW_VERSION"

# Create tag and push
echo "Creating tag v$NEW_VERSION..."
git tag -a "v$NEW_VERSION" -m "Release v$NEW_VERSION"

echo "Pushing to origin..."
git push origin main
git push origin "v$NEW_VERSION"

echo "Released v$NEW_VERSION"
