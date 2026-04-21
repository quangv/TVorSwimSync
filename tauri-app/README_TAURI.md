# TVorSwimSync

A lightweight macOS menubar widget that keeps your TradingView and thinkorswim symbols in sync. It displays the current TradingView symbol overlaid on an emoji indicator — 🌊 when both apps show the same symbol, 🛑 when they differ.

## How It Works

The app polls every **1 second** using the macOS **Core Graphics API** (`CGWindowListCopyWindowInfo`) to enumerate all on-screen windows. It finds windows owned by "TradingView" and "thinkorswim", reads their window titles, and extracts the ticker symbol from each. This is a direct system call (not screen scraping), so it has negligible performance overhead.

**Screen Recording permission** is required on macOS to read window titles of other apps. Grant it to the app (or the terminal running it in dev mode) via **System Settings → Privacy & Security → Screen Recording**.

## Versioning

The app version is defined in [Cargo.toml](src-tauri/Cargo.toml) under `[package] version`. This single source of truth is automatically used throughout the app (About window, CLI tools, etc.) via the `get_app_version` Tauri command that reads `env!("CARGO_PKG_VERSION")` at build time.

## Dev Setup

### Prerequisites

- [Node.js](https://nodejs.org/)
- [Rust](https://www.rust-lang.org/tools/install)
- [Tauri CLI](https://tauri.app/start/)

### Recommended IDE

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

### Run

```bash
npm install
npm run tauri:dev
```

### Build

```bash
npm run tauri:build
```

> **Note:** In dev mode, screen recording permission must be granted to the **terminal app** running the process (e.g., VS Code, Terminal.app, iTerm2), not the Tauri binary itself.
