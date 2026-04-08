use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowListOptionOnScreenOnly, kCGWindowName,
    kCGWindowOwnerName,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Mutex;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolState {
    pub tradingview_symbol: Option<String>,
    pub thinkorswim_symbol: Option<String>,
    pub matched: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPosition {
    pub x: f64,
    pub y: f64,
}

struct AppState {
    last_tv_title: Mutex<Option<String>>,
    last_tos_title: Mutex<Option<String>>,
}

fn state_file_path() -> std::path::PathBuf {
    let mut path = dirs::home_dir().unwrap_or_default();
    path.push(".tvorswim_position.json");
    path
}

/// Get the front window title for an app using Core Graphics.
/// Iterates on-screen windows (front-to-back order) and returns the first
/// title belonging to `owner_name`.
fn get_window_title_for_app(owner_name: &str) -> Option<String> {
    let windows = copy_window_info(kCGWindowListOptionOnScreenOnly, kCGNullWindowID)?;

    let count = windows.len();
    for i in 0..count {
        let dict_ref = unsafe {
            core_foundation::array::CFArrayGetValueAtIndex(
                windows.as_concrete_TypeRef(),
                i as isize,
            )
        };
        if dict_ref.is_null() {
            continue;
        }

        let dict: core_foundation::dictionary::CFDictionary = unsafe {
            TCFType::wrap_under_get_rule(
                dict_ref as core_foundation::dictionary::CFDictionaryRef,
            )
        };

        // Check owner name
        let owner_cf_key = unsafe { CFString::wrap_under_get_rule(kCGWindowOwnerName as *const _) };
        if let Some(val) = dict.find(owner_cf_key.as_CFTypeRef()) {
            let owner_str: CFString = unsafe { TCFType::wrap_under_get_rule(*val as *const _) };
            if owner_str.to_string() != owner_name {
                continue;
            }
        } else {
            continue;
        }

        // Get window title
        let name_cf_key = unsafe { CFString::wrap_under_get_rule(kCGWindowName as *const _) };
        if let Some(val) = dict.find(name_cf_key.as_CFTypeRef()) {
            let title_str: CFString = unsafe { TCFType::wrap_under_get_rule(*val as *const _) };
            let title = title_str.to_string();
            if !title.is_empty() {
                return Some(title);
            }
        }
    }

    None
}

fn get_tradingview_title() -> Option<String> {
    get_window_title_for_app("TradingView")
}

fn get_thinkorswim_title() -> Option<String> {
    get_window_title_for_app("thinkorswim")
}

fn extract_symbol(title: &str, source: &str) -> Option<String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return None;
    }

    let first_token = if source == "thinkorswim" {
        trimmed.split([',', ' ']).next()
    } else {
        trimmed.split_whitespace().next()
    }?;

    // Trim non-alphanumeric from start/end
    let cleaned: String = first_token
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .trim_end_matches(|c: char| !c.is_alphanumeric() && c != '!' && c != '.' && c != '-')
        .to_string();

    if cleaned.len() < 2 {
        return None;
    }

    Some(cleaned.chars().take(4).collect::<String>().to_uppercase())
}

#[tauri::command]
fn poll_symbols(state: State<AppState>) -> SymbolState {
    let tv_title = get_tradingview_title();
    let tos_title = get_thinkorswim_title();

    if let Some(ref t) = tv_title {
        *state.last_tv_title.lock().unwrap() = Some(t.clone());
    }
    if let Some(ref t) = tos_title {
        *state.last_tos_title.lock().unwrap() = Some(t.clone());
    }

    let tv_sym = tv_title
        .as_deref()
        .and_then(|t| extract_symbol(t, "tradingview"));
    let tos_sym = tos_title
        .as_deref()
        .and_then(|t| extract_symbol(t, "thinkorswim"));

    let matched = match (&tv_sym, &tos_sym) {
        (Some(a), Some(b)) => a == b,
        _ => true, // if either is missing, don't alarm
    };

    SymbolState {
        tradingview_symbol: tv_sym,
        thinkorswim_symbol: tos_sym,
        matched,
    }
}

#[tauri::command]
fn save_position(x: f64, y: f64) {
    let pos = SavedPosition { x, y };
    if let Ok(json) = serde_json::to_string(&pos) {
        let _ = fs::write(state_file_path(), json);
    }
}

#[tauri::command]
fn load_position() -> Option<SavedPosition> {
    let data = fs::read_to_string(state_file_path()).ok()?;
    serde_json::from_str(&data).ok()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            last_tv_title: Mutex::new(None),
            last_tos_title: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            poll_symbols,
            save_position,
            load_position
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
