# CLAUDE.md — TVorSwimSync

## Release Process

When the user says "do a release" or "release <version>":

1. Check the current version: `grep '"version"' tauri-app/src-tauri/tauri.conf.json`
2. Determine next version (increment patch, e.g. 0.1.9 → 0.1.10)
3. Run `npm run release <version>` from `tauri-app/` — bumps all 4 files atomically
4. Write release notes: run `git log v<prev>..HEAD --oneline` to see what changed, then add a new block at the top of `tauri-app/release-notes.html` (copy previous block structure, update version/date/bullets, keep `class="version current"` and `id="current-version"` on new span, remove `current` class and badge from old block)
5. Run `npm run build-release <version>` from `tauri-app/` — builds, commits, tags, pushes, creates GitHub release

Scripts live in `scripts/release.sh` and `scripts/build-release.sh`.

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
