# CLAUDE.md — TVorSwimSync

## Release Checklist

When releasing a new version, bump the version in **all four places** — they must match:

1. `tauri-app/package.json` → `"version"`
2. `tauri-app/src-tauri/tauri.conf.json` → `"version"`
3. `tauri-app/src-tauri/Cargo.toml` → `version`
4. `tauri-app/release-notes.html`:
   - Add new `<div class="release">` block at the top
   - Set `id="current-version"` and `class="version current"` on the new version span
   - Add `<span class="badge">current</span>`
   - Remove `current` class and badge from the previous version block
   - Write bullet points summarising changes (run `git log v<prev>..HEAD --oneline` for reference)

Then build and publish:

```bash
cd tauri-app
npm run tauri:build
# DMG output: tauri-app/src-tauri/target/release/bundle/dmg/
git commit -am "v<version> release"
git tag v<version>
git push && git push --tags
gh release create v<version> \
  --title "v<version> — <one-line summary>" \
  --notes "<release notes>" \
  tauri-app/src-tauri/target/release/bundle/dmg/TVorSwimSync_<version>_aarch64.dmg
```

## Project Structure

- `tauri-app/index.html` — widget UI
- `tauri-app/src/main.ts` — all frontend logic (polling, animations, click handlers)
- `tauri-app/src/styles.css` — minimal global styles
- `tauri-app/src-tauri/src/` — Rust backend (symbol polling, key injection, permissions)
- `tauri-app/src-tauri/tauri.conf.json` — Tauri window config

## Key Architecture Notes

- Window dragging uses `data-tauri-drag-region` on `#app`. The inner wrapper div should NOT have it (caused cursor flicker). CSS cursor control on macOS transparent windows is unreliable — don't try to override it.
- Button animations use the Web Animations API (WAAPI) with `AbortController` for clean cancellation on rapid clicks
- `pollSymbols()` runs every 1s and fires `onModeChange` callback when mode changes — used to reveal the indicator button after a click
