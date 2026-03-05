#!/bin/bash
# Usage: ./bump.sh
# Auto-increments the patch digit in Cargo.toml and adds a CHANGELOG.md entry.
set -euo pipefail

CURRENT=$(grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
MAJOR=$(echo "$CURRENT" | cut -d. -f1)
MINOR=$(echo "$CURRENT" | cut -d. -f2)
PATCH=$(echo "$CURRENT" | cut -d. -f3)
VERSION="$MAJOR.$MINOR.$((PATCH + 1))"
echo "$CURRENT → $VERSION"

DATE=$(date +%Y-%m-%d)

# Update Cargo.toml
sed -i "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml
echo "Cargo.toml → $VERSION"

# Prepend CHANGELOG entry
ENTRY="## v$VERSION — $DATE\n\n- \n"
sed -i "s/^# Changelog$/# Changelog\n\n$ENTRY/" CHANGELOG.md
echo "CHANGELOG.md → v$VERSION entry added"

echo "Done. Edit CHANGELOG.md to fill in the release note, then commit."
