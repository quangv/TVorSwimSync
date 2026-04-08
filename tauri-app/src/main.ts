import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, PhysicalPosition } from "@tauri-apps/api/window";

interface SymbolState {
  tradingview_symbol: string | null;
  thinkorswim_symbol: string | null;
  matched: boolean;
}

interface SavedPosition {
  x: number;
  y: number;
}

const POLL_INTERVAL_MS = 1000;
const SAVE_DEBOUNCE_MS = 500;

const emojiEl = document.getElementById("emoji")!;
const symbolEl = document.getElementById("symbol")!;

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

    emojiEl.textContent = state.matched ? "🌊" : "🛑";
  } catch {
    // TradingView/thinkorswim may not be running
  }
}

// Initialize
restorePosition();
pollSymbols();
setInterval(pollSymbols, POLL_INTERVAL_MS);
