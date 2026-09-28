#!/bin/bash
set -e

VERSION=$1
if [ -z "$VERSION" ]; then
  echo "Usage: npm run build-release <version>"
  exit 1
fi

DMG="tauri-app/src-tauri/target/release/bundle/dmg/TVorSwimSync_${VERSION}_aarch64.dmg"

echo "→ Building v$VERSION..."
cd tauri-app && npm run tauri:build && cd ..

echo "→ Committing..."
git add -A
git commit -m "v$VERSION release"

echo "→ Tagging..."
git tag "v$VERSION"

echo "→ Pushing..."
git push && git push --tags

echo "→ Creating GitHub release..."
gh release create "v$VERSION" \
  --title "v$VERSION" \
  --notes "See release notes in app (Help → Release Notes)" \
  "$DMG"

echo ""
echo "✓ v$VERSION released!"
