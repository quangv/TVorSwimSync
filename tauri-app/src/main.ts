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
const permBanner = document.getElementById("perm-banner")!;

let hasScreenPermission = true;

async function checkPermission() {
  try {
    hasScreenPermission = await invoke<boolean>(
      "check_screen_recording_permission",
    );
    if (!hasScreenPermission) {
      permBanner.style.display = "flex";
    } else {
      permBanner.style.display = "none";
    }
  } catch {
    // ignore
  }
}

permBanner.addEventListener("click", async () => {
  try {
    await invoke<boolean>("request_screen_recording_permission");
    // Re-check after a short delay (user may need to toggle in Settings)
    setTimeout(checkPermission, 2000);
  } catch {
    // ignore
  }
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

    console.log("[debug] raw_tv_title:", state.raw_tv_title);
    console.log("[debug] raw_tos_title:", state.raw_tos_title);
    console.log(
      "[debug] extracted TV:",
      state.tradingview_symbol,
      "ToS:",
      state.thinkorswim_symbol,
    );

    if (state.tradingview_symbol) {
      symbolEl.textContent = state.tradingview_symbol;
    } else {
      symbolEl.textContent = "--";
    }

    emojiEl.textContent = state.matched ? "🌊" : "🛑";
  } catch {
    // TradingView/thinkorswim may not be running
  }
}

// Initialize
restorePosition();
checkPermission();
pollSymbols();
setInterval(pollSymbols, POLL_INTERVAL_MS);
// Re-check permission periodically in case user grants it
setInterval(checkPermission, 5000);
