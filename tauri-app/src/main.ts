import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, PhysicalPosition } from "@tauri-apps/api/window";

interface SymbolState {
  tradingview_symbol: string | null;
  thinkorswim_symbol: string | null;
  matched: boolean;
  raw_tv_title: string | null;
  raw_tos_title: string | null;
}

interface SavedPosition {
  x: number;
  y: number;
}

const POLL_INTERVAL_MS = 1000;
const SAVE_DEBOUNCE_MS = 500;

const emojiEl = document.getElementById("emoji")!;
const symbolEl = document.getElementById("symbol")!;
const mapModeEl = document.getElementById("map-mode")!;
const permScreen = document.getElementById("perm-screen")!;
const permA11y = document.getElementById("perm-a11y")!;

let hasScreenPermission = true;
let hasA11yPermission = true;
let lastTvSymbol: string | null = null;
let lastSyncEnabled = false;
let syncing = false;

async function checkPermissions() {
  try {
    hasScreenPermission = await invoke<boolean>(
      "check_screen_recording_permission",
    );
  } catch {
    hasScreenPermission = true; // assume ok if check fails
  }

  try {
    hasA11yPermission = await invoke<boolean>(
      "check_accessibility_permission_cmd",
      { prompt: false },
    );
  } catch {
    hasA11yPermission = true; // assume ok if check fails
  }

  // Show screen recording banner first (takes priority)
  if (!hasScreenPermission) {
    permScreen.style.display = "flex";
    permA11y.style.display = "none";
  } else if (!hasA11yPermission) {
    permScreen.style.display = "none";
    permA11y.style.display = "flex";
  } else {
    permScreen.style.display = "none";
    permA11y.style.display = "none";
  }
}

permScreen.addEventListener("click", async () => {
  try {
    await invoke<boolean>("request_screen_recording_permission");
  } catch {
    // ignore
  }
  // Re-check frequently — user may grant in Settings and come back
  const recheckId = setInterval(checkPermissions, 1500);
  setTimeout(() => clearInterval(recheckId), 30000);
});

permA11y.addEventListener("click", async () => {
  try {
    // This opens the system Accessibility prompt dialog
    await invoke<boolean>("check_accessibility_permission_cmd", {
      prompt: true,
    });
  } catch {
    // ignore
  }
  // Re-check frequently — user may grant in Settings and come back
  const recheckId = setInterval(checkPermissions, 1500);
  setTimeout(() => clearInterval(recheckId), 30000);
});

async function restorePosition() {
  try {
    const pos = await invoke<SavedPosition | null>("load_position");
    if (pos) {
      await getCurrentWindow().setPosition(new PhysicalPosition(pos.x, pos.y));
    }
  } catch {
    // No saved position yet
  }
}

let saveTimeout: ReturnType<typeof setTimeout> | null = null;

async function saveCurrentPosition() {
  try {
    const pos = await getCurrentWindow().outerPosition();
    await invoke("save_position", { x: pos.x, y: pos.y });
  } catch {
    // Ignore save errors
  }
}

function debounceSavePosition() {
  if (saveTimeout) clearTimeout(saveTimeout);
  saveTimeout = setTimeout(saveCurrentPosition, SAVE_DEBOUNCE_MS);
}

// Listen for window move events
getCurrentWindow().onMoved(() => {
  debounceSavePosition();
});

async function pollSymbols() {
  try {
    const state = await invoke<SymbolState>("poll_symbols");

    if (state.tradingview_symbol) {
      symbolEl.textContent = state.tradingview_symbol;
    } else {
      symbolEl.textContent = "--";
    }

    // Auto-sync: when TV symbol changes (or sync just got armed), type it into thinkorswim input
    const syncEnabled = await invoke<boolean>("get_sync_enabled");
    const justArmed = syncEnabled && !lastSyncEnabled;
    lastSyncEnabled = syncEnabled;
    if (
      syncEnabled &&
      state.tradingview_symbol &&
      (justArmed || (state.tradingview_symbol !== lastTvSymbol && lastTvSymbol !== null)) &&
      !syncing
    ) {
      syncing = true;
      try {
        console.log("[sync] TV symbol:", state.tradingview_symbol);
        const symbolToSync = await invoke<string>("apply_symbol_mapping", { symbol: state.tradingview_symbol });
        console.log("[sync] mapped symbol:", symbolToSync);
        await invoke("auto_sync", { symbol: symbolToSync });
        console.log("[sync] auto_sync called with:", symbolToSync);
      } catch (e) {
        console.error("[sync] error:", e);
        try {
          await getCurrentWindow().show();
        } catch {}
      }
      syncing = false;
    }
    lastTvSymbol = state.tradingview_symbol;

    emojiEl.textContent = state.matched ? "🌊" : "🛑";

    const { mode } = await invoke<{ mode: string }>("load_mappings");
    if (mode === "long") {
      mapModeEl.textContent = "▲";
      mapModeEl.style.color = "#22c55e";
    } else if (mode === "short") {
      mapModeEl.textContent = "▼";
      mapModeEl.style.color = "#ef4444";
    } else {
      mapModeEl.textContent = "⏺";
      mapModeEl.style.color = "#FECB09";
    }

    // Wave crashing over symbol when synced; centered on stop sign when unsynced
    if (state.matched) {
      symbolEl.style.transform = "translateY(22px)";
    } else {
      symbolEl.style.transform = "";
    }
  } catch {
    // TradingView/thinkorswim may not be running
  }
}

mapModeEl.addEventListener("click", async () => {
  const { mode, long_mappings, short_mappings } = await invoke<{ mode: string; long_mappings: string; short_mappings: string }>("load_mappings");
  const next = mode === "off" ? "long" : mode === "long" ? "short" : "off";
  await invoke("save_mappings", { mappings: { mode: next, long_mappings, short_mappings } });
  await invoke("force_sync_cmd");
});

// Initialize
restorePosition();
checkPermissions();
pollSymbols();
setInterval(pollSymbols, POLL_INTERVAL_MS);
// Re-check permissions periodically in case user grants them
setInterval(checkPermissions, 5000);
