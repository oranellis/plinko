#!/usr/bin/env bash
# scripts/bump-version.sh <NEW_VERSION>
#
# Updates the application version in all four canonical locations:
#   1. Cargo.toml           (workspace root — Rust crates inherit this)
#   2. plinko-web/package.json
#   3. plinko-shared/src/protocol.rs  (VERSION constant)
#   4. plinko-web/src/protocol.ts     (PROTOCOL_VERSION constant)
#
# The app version and protocol version are kept identical so that a stale
# browser cache is always detected and the user is prompted to reload.
#
# Usage:
#   ./scripts/bump-version.sh 0.4.0
set -euo pipefail

NEW="$1"

if [[ -z "$NEW" ]]; then
    echo "Usage: $0 <NEW_VERSION>"
    exit 1
fi

# Validate semver format
if ! [[ "$NEW" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Error: version must be X.Y.Z (e.g. 0.4.0)"
    exit 1
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

OLD=$(grep -m1 '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/')
echo "Bumping $OLD → $NEW"

# 1. Workspace Cargo.toml
sed -i "s/^version = \"$OLD\"/version = \"$NEW\"/" Cargo.toml

# 2. plinko-web/package.json
sed -i "s/\"version\": \"$OLD\"/\"version\": \"$NEW\"/" plinko-web/package.json

# 3. plinko-shared/src/protocol.rs
sed -i "s/pub const VERSION: &str = \"$OLD\"/pub const VERSION: \&str = \"$NEW\"/" \
    plinko-shared/src/protocol.rs

# 4. plinko-web/src/protocol.ts
sed -i "s/export const PROTOCOL_VERSION = \"$OLD\"/export const PROTOCOL_VERSION = \"$NEW\"/" \
    plinko-web/src/protocol.ts

echo "✓ Version updated to $NEW in:"
echo "  Cargo.toml"
echo "  plinko-web/package.json"
echo "  plinko-shared/src/protocol.rs"
echo "  plinko-web/src/protocol.ts"
echo ""
echo "Next steps:"
echo "  cargo check                        # verify build"
echo "  git add Cargo.toml plinko-web/package.json plinko-shared/src/protocol.rs plinko-web/src/protocol.ts"
echo "  git commit -m \"chore: bump version to $NEW\""
echo "  git tag v$NEW"
