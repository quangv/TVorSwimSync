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
let suppressEmojiUntil = 0;
let lastMapMode: string | null = null;
let onModeChange: (() => void) | null = null;

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

document.getElementById("app")!.addEventListener("mousedown", (e) => {
  if ((e.target as HTMLElement).closest("#map-mode")) return;
  document.body.style.cursor = "grabbing";
  getCurrentWindow().startDragging();
});

document.addEventListener("mouseup", () => {
  document.body.style.cursor = "";
});


async function pollSymbols() {
  try {
    const state = await invoke<SymbolState>("poll_symbols");

    if (state.tradingview_symbol) {
      const mappedForDisplay = await invoke<string>("apply_symbol_mapping", { symbol: state.tradingview_symbol });
      symbolEl.textContent = mappedForDisplay;
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

    const { mode } = await invoke<{ mode: string; long_mappings: string; short_mappings: string }>("load_mappings");
    let matched = state.matched;
    if (mode !== "off" && state.tradingview_symbol && state.thinkorswim_symbol) {
      const mappedSymbol = await invoke<string>("apply_symbol_mapping", { symbol: state.tradingview_symbol });
      matched = mappedSymbol.toUpperCase() === state.thinkorswim_symbol.toUpperCase();
    }
    if (Date.now() < suppressEmojiUntil) return;
    emojiEl.textContent = matched ? "🌊" : "🛑";
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

    if (mode !== lastMapMode) {
      onModeChange?.();
      onModeChange = null;
    }
    lastMapMode = mode;

    // Wave crashing over symbol when synced; centered on stop sign when unsynced
    if (matched) {
      symbolEl.style.transform = "translateY(22px)";
    } else {
      symbolEl.style.transform = "";
    }
  } catch {
    // TradingView/thinkorswim may not be running
  }
}

function flashEmoji() {
  emojiEl.style.opacity = "0.15";
  setTimeout(() => { emojiEl.style.opacity = ""; }, 1500);
}

function animateEl(el: HTMLElement, keyframes: Keyframe[], options: KeyframeAnimationOptions & { signal: AbortSignal }): Promise<void> {
  return new Promise((resolve, reject) => {
    if (options.signal.aborted) return reject(new DOMException("Aborted", "AbortError"));
    const anim = el.animate(keyframes, options);
    options.signal.addEventListener("abort", () => { anim.cancel(); reject(new DOMException("Aborted", "AbortError")); });
    anim.finished.then(() => resolve()).catch(reject);
  });
}

function waitForModeChange(signal: AbortSignal, minMs: number): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) return reject(new DOMException("Aborted", "AbortError"));
    const deadline = Date.now() + minMs;
    signal.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")));
    onModeChange = () => {
      const remaining = deadline - Date.now();
      if (remaining > 0) setTimeout(resolve, remaining); else resolve();
    };
    // max wait fallback
    setTimeout(resolve, 3000);
  });
}

let hideController: AbortController | null = null;
let pressAnim: Animation | null = null;

mapModeEl.addEventListener("mousedown", () => {
  hideController?.abort();
  hideController = null;
  pressAnim?.cancel();
  mapModeEl.style.transform = "";
  mapModeEl.style.opacity = "";
  pressAnim = mapModeEl.animate(
    [{ transform: "scale(1)" }, { transform: "scale(0.75)" }],
    { duration: 100, fill: "forwards" }
  );
});

mapModeEl.addEventListener("mouseup", async () => {
  pressAnim?.cancel();
  pressAnim = null;
  hideController?.abort();
  const ctrl = new AbortController();
  hideController = ctrl;
  try {
    // explosion
    await animateEl(mapModeEl, [{ transform: "scale(0.75)", opacity: 1 }, { transform: "scale(4)", opacity: 0 }], { duration: 150, easing: "ease-in", fill: "forwards", signal: ctrl.signal });
    // snap hidden, wait for mode change
    mapModeEl.style.transform = "scale(0.3)";
    mapModeEl.style.opacity = "0";
    mapModeEl.getAnimations().forEach(a => a.cancel());
    await waitForModeChange(ctrl.signal, 800);
    // reveal
    await animateEl(mapModeEl, [{ transform: "scale(0.3)", opacity: 0 }, { transform: "scale(1)", opacity: 1 }], { duration: 300, easing: "ease-out", fill: "forwards", signal: ctrl.signal });
    mapModeEl.style.transform = "";
    mapModeEl.style.opacity = "";
  } catch {
    // aborted by next mousedown — leave cleanup to it
  }
});

let mapModeClickTimer: ReturnType<typeof setTimeout> | null = null;

mapModeEl.addEventListener("click", async () => {
  flashEmoji();
  suppressEmojiUntil = Date.now() + 1500;
  if (mapModeClickTimer) {
    clearTimeout(mapModeClickTimer);
    mapModeClickTimer = null;
    const { long_mappings, short_mappings } = await invoke<{ mode: string; long_mappings: string; short_mappings: string }>("load_mappings");
    await invoke("save_mappings", { mappings: { mode: "off", long_mappings, short_mappings } });
    await invoke("force_sync_cmd");
  } else {
    mapModeClickTimer = setTimeout(async () => {
      mapModeClickTimer = null;
      const { mode, long_mappings, short_mappings } = await invoke<{ mode: string; long_mappings: string; short_mappings: string }>("load_mappings");
      const next = mode === "long" ? "short" : mode === "short" ? "long" : "long";
      await invoke("save_mappings", { mappings: { mode: next, long_mappings, short_mappings } });
      await invoke("force_sync_cmd");
    }, 300);
  }
});

// Initialize
restorePosition();
checkPermissions();
pollSymbols();
setInterval(pollSymbols, POLL_INTERVAL_MS);
// Re-check permissions periodically in case user grants them
setInterval(checkPermissions, 5000);
