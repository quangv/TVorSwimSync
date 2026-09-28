#!/bin/bash
set -e

VERSION=$1
if [ -z "$VERSION" ]; then
  echo "Usage: npm run release <version>  e.g. npm run release 0.1.10"
  exit 1
fi

echo "→ Bumping version to $VERSION in all files..."
# tauri.conf.json
sed -i '' "s/\"version\": \".*\"/\"version\": \"$VERSION\"/" tauri-app/src-tauri/tauri.conf.json
# Cargo.toml (first occurrence only — the package version, not dep versions)
sed -i '' "1,/^version = /s/^version = \".*\"/version = \"$VERSION\"/" tauri-app/src-tauri/Cargo.toml
# package.json
sed -i '' "s/\"version\": \".*\"/\"version\": \"$VERSION\"/" tauri-app/package.json
# release-notes.html current-version span
sed -i '' "s/id=\"current-version\">v[^<]*/id=\"current-version\">v$VERSION/" tauri-app/release-notes.html

echo "→ Verifying versions..."
grep '"version"' tauri-app/package.json tauri-app/src-tauri/tauri.conf.json
grep '^version' tauri-app/src-tauri/Cargo.toml | head -1
grep 'current-version' tauri-app/release-notes.html

echo ""
echo "✓ Versions bumped. Now:"
echo "  1. Add release notes for v$VERSION at the top of tauri-app/release-notes.html"
echo "  2. Run: npm run build-release $VERSION"
