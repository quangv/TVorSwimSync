use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use core_graphics::event::{CGEvent, CGEventTapLocation, CGEventType, CGMouseButton};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowListOptionOnScreenOnly, kCGWindowName,
    kCGWindowOwnerName,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Mutex;
use tauri::State;
use tauri::menu::Menu;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolState {
    pub tradingview_symbol: Option<String>,
    pub thinkorswim_symbol: Option<String>,
    pub matched: bool,
    pub raw_tv_title: Option<String>,
    pub raw_tos_title: Option<String>,
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

/// Log all on-screen window owners and titles (for debugging).
/// Only logs once every ~10 seconds to avoid spam.
fn debug_log_all_windows() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST_LOG: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let last = LAST_LOG.load(Ordering::Relaxed);
    if now - last < 10 {
        return;
    }
    LAST_LOG.store(now, Ordering::Relaxed);

    if let Some(windows) = copy_window_info(kCGWindowListOptionOnScreenOnly, kCGNullWindowID) {
        let count = windows.len();
        eprintln!("[debug] === All on-screen windows ({} total) ===", count);
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

            let owner_cf_key =
                unsafe { CFString::wrap_under_get_rule(kCGWindowOwnerName as *const _) };
            let name_cf_key =
                unsafe { CFString::wrap_under_get_rule(kCGWindowName as *const _) };

            let owner = dict
                .find(owner_cf_key.as_CFTypeRef())
                .map(|val| {
                    let s: CFString = unsafe { TCFType::wrap_under_get_rule(*val as *const _) };
                    s.to_string()
                })
                .unwrap_or_else(|| "<no owner>".to_string());

            let title = dict
                .find(name_cf_key.as_CFTypeRef())
                .map(|val| {
                    let s: CFString = unsafe { TCFType::wrap_under_get_rule(*val as *const _) };
                    s.to_string()
                })
                .unwrap_or_else(|| "<no title>".to_string());

            if owner.contains("Trading") || owner.contains("thinkorswim") || owner.contains("Google") || owner.contains("Chrome") || owner.contains("Safari") || owner.contains("Arc") || owner.contains("Firefox") || owner.contains("Brave") {
                eprintln!("[debug]   owner={:?}  title={:?}", owner, title);
            }
        }
        eprintln!("[debug] === end ===");
    }
}

extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

#[tauri::command]
fn check_screen_recording_permission() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}

#[tauri::command]
fn request_screen_recording_permission() -> bool {
    unsafe { CGRequestScreenCaptureAccess() }
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
    debug_log_all_windows();

    let tv_title = get_tradingview_title();
    let tos_title = get_thinkorswim_title();

    eprintln!("[debug] TV raw title: {:?}", tv_title);
    eprintln!("[debug] ToS raw title: {:?}", tos_title);

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

    eprintln!("[debug] Extracted TV symbol: {:?}, ToS symbol: {:?}", tv_sym, tos_sym);

    let matched = match (&tv_sym, &tos_sym) {
        (Some(a), Some(b)) => a == b,
        _ => true, // if either is missing, don't alarm
    };

    SymbolState {
        tradingview_symbol: tv_sym,
        thinkorswim_symbol: tos_sym,
        matched,
        raw_tv_title: tv_title,
        raw_tos_title: tos_title,
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

/// Click 20px below the app window (horizontally centered), select all, type the symbol, press Enter.
/// Coordinates are in physical pixels — we convert to screen points using the scale factor.
#[tauri::command]
fn sync_to_tos(symbol: String, window_x: f64, window_y: f64, window_width: f64, window_height: f64, scale_factor: f64) {
    let scale = if scale_factor > 0.0 { scale_factor } else { 2.0 };
    let click_x = (window_x + window_width / 2.0) / scale;
    let click_y = (window_y + window_height) / scale + 20.0;
    let point = CGPoint::new(click_x, click_y);

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap();

    // Click to focus the input field
    let mouse_down = CGEvent::new_mouse_event(
        source.clone(),
        CGEventType::LeftMouseDown,
        point,
        CGMouseButton::Left,
    ).unwrap();
    let mouse_up = CGEvent::new_mouse_event(
        source.clone(),
        CGEventType::LeftMouseUp,
        point,
        CGMouseButton::Left,
    ).unwrap();
    mouse_down.post(CGEventTapLocation::HID);
    mouse_up.post(CGEventTapLocation::HID);

    std::thread::sleep(std::time::Duration::from_millis(100));

    // Type each character of the symbol via CGEvent
    for ch in symbol.chars() {
        let event_down = CGEvent::new_keyboard_event(source.clone(), 0, true).unwrap();
        let event_up = CGEvent::new_keyboard_event(source.clone(), 0, false).unwrap();
        let utf16: Vec<u16> = ch.encode_utf16(&mut [0; 2]).to_vec();
        event_down.set_string_from_utf16_unchecked(&utf16);
        event_down.post(CGEventTapLocation::HID);
        event_up.post(CGEventTapLocation::HID);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let menu = Menu::new(app)?;
            app.set_menu(menu)?;
            Ok(())
        })
        .manage(AppState {
            last_tv_title: Mutex::new(None),
            last_tos_title: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            poll_symbols,
            save_position,
            load_position,
            sync_to_tos,
            check_screen_recording_permission,
            request_screen_recording_permission
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
