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
use tauri::Manager;
use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{WebviewWindowBuilder, WebviewUrl};
use std::sync::atomic::{AtomicBool, Ordering};

static SYNC_ENABLED: AtomicBool = AtomicBool::new(cfg!(debug_assertions));

/// Deactivate our app so we don't steal focus from other apps.
fn deactivate_app() {
    use std::process::Command;
    // Use osascript to tell System Events to activate the frontmost app that isn't us
    // Simpler: just use NSApp's hide via objc if possible, but easiest is:
    // We post a Cmd+Tab-like refocus by just not taking focus.
    // Actually, use the cocoa API directly:
    let _ = Command::new("osascript")
        .arg("-e")
        .arg(r#"tell application "System Events" to set frontmost of the first process whose frontmost is false to true"#)
        .output();
}

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

fn click_target_file_path() -> std::path::PathBuf {
    let mut path = dirs::home_dir().unwrap_or_default();
    path.push(".tvorswim_click_target.json");
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

extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
    fn CGDisplayMoveCursorToPoint(display: u32, point: CGPoint) -> i32;
    fn CGMainDisplayID() -> u32;
}

// Accessibility check via Core Foundation
fn check_accessibility_permission(prompt: bool) -> bool {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::string::CFString;

    extern "C" {
        fn AXIsProcessTrustedWithOptions(
            options: core_foundation::dictionary::CFDictionaryRef,
        ) -> bool;
    }

    let key = CFString::new("AXTrustedCheckOptionPrompt");
    let value = if prompt { CFBoolean::true_value() } else { CFBoolean::false_value() };
    let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

#[tauri::command]
fn check_screen_recording_permission() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}

#[tauri::command]
fn request_screen_recording_permission() -> bool {
    unsafe { CGRequestScreenCaptureAccess() }
}

#[tauri::command]
fn check_accessibility_permission_cmd(prompt: bool) -> bool {
    check_accessibility_permission(prompt)
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

#[tauri::command]
fn get_sync_enabled() -> bool {
    SYNC_ENABLED.load(Ordering::Relaxed)
}

#[tauri::command]
fn deactivate_app_cmd() {
    deactivate_app();
}

#[tauri::command]
fn close_window(label: String, app_handle: tauri::AppHandle) {
    if let Some(win) = app_handle.get_webview_window(&label) {
        let _ = win.destroy();
    }
}

#[tauri::command]
fn save_click_target(x: f64, y: f64, app_handle: tauri::AppHandle) {
    let pos = SavedPosition { x, y };
    if let Ok(json) = serde_json::to_string(&pos) {
        let _ = fs::write(click_target_file_path(), json);
    }
    // Close the calibration window immediately
    if let Some(win) = app_handle.get_webview_window("calibrate") {
        let _ = win.destroy();
    }
}

#[tauri::command]
fn load_click_target() -> Option<SavedPosition> {
    let data = fs::read_to_string(click_target_file_path()).ok()?;
    serde_json::from_str(&data).ok()
}

/// Click at the saved target position, type the symbol, press Enter.
/// click_x/click_y are in screen points (logical, not physical).
/// Blocks until typing is complete so the caller can re-show the window after.
#[tauri::command]
fn sync_to_tos(symbol: String, click_x: f64, click_y: f64) {
    eprintln!("[sync] target ({}, {}), symbol: {}", click_x, click_y, symbol);

    // 1. Activate thinkorswim via osascript
    let script = r#"tell application "System Events"
    set frontmost of process "thinkorswim" to true
end tell
delay 0.3"#;
    eprintln!("[sync] activating thinkorswim...");
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output();
    match &output {
        Ok(o) if !o.status.success() => {
            eprintln!("[sync] activate error: {}", String::from_utf8_lossy(&o.stderr));
        }
        Err(e) => {
            eprintln!("[sync] activate failed: {}", e);
        }
        _ => {}
    }

    // 2. Click at the target position
    eprintln!("[sync] clicking...");
    let point = CGPoint::new(click_x, click_y);
    unsafe {
        CGDisplayMoveCursorToPoint(CGMainDisplayID(), point);
    }
    std::thread::sleep(std::time::Duration::from_millis(50));

    let source = CGEventSource::new(CGEventSourceStateID::Private).unwrap();

    // First click (clickState = 1)
    let mouse_down = CGEvent::new_mouse_event(
        source.clone(),
        CGEventType::LeftMouseDown,
        point,
        CGMouseButton::Left,
    ).unwrap();
    mouse_down.set_integer_value_field(core_graphics::event::EventField::MOUSE_EVENT_CLICK_STATE, 1);
    let mouse_up = CGEvent::new_mouse_event(
        source.clone(),
        CGEventType::LeftMouseUp,
        point,
        CGMouseButton::Left,
    ).unwrap();
    mouse_up.set_integer_value_field(core_graphics::event::EventField::MOUSE_EVENT_CLICK_STATE, 1);
    mouse_down.post(CGEventTapLocation::HID);
    std::thread::sleep(std::time::Duration::from_millis(10));
    mouse_up.post(CGEventTapLocation::HID);
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Second click (clickState = 2) — makes it a double-click
    let mouse_down2 = CGEvent::new_mouse_event(
        source.clone(),
        CGEventType::LeftMouseDown,
        point,
        CGMouseButton::Left,
    ).unwrap();
    mouse_down2.set_integer_value_field(core_graphics::event::EventField::MOUSE_EVENT_CLICK_STATE, 2);
    let mouse_up2 = CGEvent::new_mouse_event(
        source.clone(),
        CGEventType::LeftMouseUp,
        point,
        CGMouseButton::Left,
    ).unwrap();
    mouse_up2.set_integer_value_field(core_graphics::event::EventField::MOUSE_EVENT_CLICK_STATE, 2);
    mouse_down2.post(CGEventTapLocation::HID);
    std::thread::sleep(std::time::Duration::from_millis(10));
    mouse_up2.post(CGEventTapLocation::HID);

    std::thread::sleep(std::time::Duration::from_millis(500));

    // 3. Type each character via CGEvent
    for ch in symbol.chars() {
        let src = CGEventSource::new(CGEventSourceStateID::Private).unwrap();
        let event_down = CGEvent::new_keyboard_event(src.clone(), 0, true).unwrap();
        let event_up = CGEvent::new_keyboard_event(src, 0, false).unwrap();
        event_down.set_string(&ch.to_string());
        event_down.post(CGEventTapLocation::HID);
        std::thread::sleep(std::time::Duration::from_millis(10));
        event_up.post(CGEventTapLocation::HID);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    std::thread::sleep(std::time::Duration::from_millis(100));

    // 4. Press Enter
    let src = CGEventSource::new(CGEventSourceStateID::Private).unwrap();
    let enter_down = CGEvent::new_keyboard_event(src.clone(), 36, true).unwrap();
    enter_down.post(CGEventTapLocation::HID);
    std::thread::sleep(std::time::Duration::from_millis(10));
    let enter_up = CGEvent::new_keyboard_event(src.clone(), 36, false).unwrap();
    enter_up.post(CGEventTapLocation::HID);

    eprintln!("[sync] done");
}

/// Full auto-sync: hide window, sync, show window, deactivate.
/// Mirrors the test_sync_nvda flow exactly.
#[tauri::command]
fn auto_sync(symbol: String, app_handle: tauri::AppHandle) {
    if let Some(pos) = load_click_target() {
        let main_win = app_handle.get_webview_window("main");
        std::thread::spawn(move || {
            if let Some(ref win) = main_win {
                let _ = win.hide();
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
            sync_to_tos(symbol, pos.x, pos.y);
            std::thread::sleep(std::time::Duration::from_millis(500));
            if let Some(ref win) = main_win {
                let _ = win.show();
            }
            deactivate_app();
        });
    }
}

/// Perform a test click at the saved target position to verify coordinates.
#[tauri::command]
fn test_click_target() {
    if let Some(pos) = load_click_target() {
        eprintln!("[test] clicking at saved target ({}, {})", pos.x, pos.y);
        let point = CGPoint::new(pos.x, pos.y);
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap();
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
        std::thread::sleep(std::time::Duration::from_millis(20));
        mouse_up.post(CGEventTapLocation::HID);
    } else {
        eprintln!("[test] no saved click target found");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let sync_label = if cfg!(debug_assertions) {
                "✓ Auto-Sync Enabled (Beta)"
            } else {
                "Enable Auto-Sync (Beta)"
            };
            let sync_item = MenuItemBuilder::new(sync_label)
                .id("toggle_sync")
                .build(app)?;
            let setup_item = MenuItemBuilder::new("Setup Auto-Sync Target...")
                .id("setup_sync")
                .build(app)?;
            let test_item = MenuItemBuilder::new("Test Target")
                .id("test_target")
                .build(app)?;
            let test_sync_item = MenuItemBuilder::new("Test Sync → NVDA")
                .id("test_sync_nvda")
                .build(app)?;
            let disclaimer = MenuItemBuilder::new("⚠ Beta software – use at your own risk")
                .id("disclaimer")
                .enabled(false)
                .build(app)?;

            let help_item = MenuItemBuilder::new("Sync Positioning Help")
                .id("show_help")
                .build(app)?;

            let app_submenu = SubmenuBuilder::new(app, "TVorSwimSync")
                .item(&sync_item)
                .item(&setup_item)
                .item(&test_item)
                .item(&test_sync_item)
                .separator()
                .item(&disclaimer)
                .separator()
                .quit()
                .build()?;

            let help_submenu = SubmenuBuilder::new(app, "Help")
                .item(&help_item)
                .build()?;

            let menu = MenuBuilder::new(app)
                .item(&app_submenu)
                .item(&help_submenu)
                .build()?;
            app.set_menu(menu)?;

            app.on_menu_event(move |app_handle, event| {
                if event.id().as_ref() == "toggle_sync" {
                    let was = SYNC_ENABLED.load(Ordering::Relaxed);
                    SYNC_ENABLED.store(!was, Ordering::Relaxed);
                    let label = if !was {
                        "✓ Auto-Sync Enabled (Beta)"
                    } else {
                        "Enable Auto-Sync (Beta)"
                    };
                    let _ = sync_item.set_text(label);
                } else if event.id().as_ref() == "setup_sync" {
                    // Close test marker if it's open
                    if let Some(marker) = app_handle.get_webview_window("test_marker") {
                        let _ = marker.destroy();
                    }
                    if let Some(existing) = app_handle.get_webview_window("calibrate") {
                        let _ = existing.set_focus();
                    } else {
                        let _ = WebviewWindowBuilder::new(
                            app_handle,
                            "calibrate",
                            WebviewUrl::App("calibrate.html".into()),
                        )
                        .title("Setup Auto-Sync")
                        .transparent(true)
                        .decorations(false)
                        .always_on_top(true)
                        .maximized(true)
                        .build();
                    }
                } else if event.id().as_ref() == "test_target" {
                    // Close calibrate window if it's open
                    if let Some(cal) = app_handle.get_webview_window("calibrate") {
                        let _ = cal.destroy();
                    }
                    // Load saved click target, show marker AND perform a real click
                    if let Some(pos) = load_click_target() {
                        // Close existing test marker if any
                        if let Some(existing) = app_handle.get_webview_window("test_marker") {
                            let _ = existing.destroy();
                        }
                        let marker_size = 60.0;
                        let _ = WebviewWindowBuilder::new(
                            app_handle,
                            "test_marker",
                            WebviewUrl::App("test-marker.html".into()),
                        )
                        .title("")
                        .transparent(true)
                        .decorations(false)
                        .always_on_top(true)
                        .skip_taskbar(true)
                        .inner_size(marker_size, marker_size)
                        .position(pos.x - marker_size / 2.0, pos.y - marker_size / 2.0)
                        .build();

                        // Also perform a real CGEvent click at the saved position
                        test_click_target();
                    }
                } else if event.id().as_ref() == "test_sync_nvda" {
                    if let Some(pos) = load_click_target() {
                        let app_clone = app_handle.clone();
                        std::thread::spawn(move || {
                            // Hide main window so it doesn't intercept clicks
                            if let Some(main_win) = app_clone.get_webview_window("main") {
                                let _ = main_win.hide();
                            }
                            // Wait for window to fully hide
                            std::thread::sleep(std::time::Duration::from_millis(200));
                            sync_to_tos("NVDA".to_string(), pos.x, pos.y);
                            // Wait for TOS to process the Enter
                            std::thread::sleep(std::time::Duration::from_millis(500));
                            if let Some(main_win) = app_clone.get_webview_window("main") {
                                let _ = main_win.show();
                            }
                            // Deactivate our app so we don't steal focus
                            deactivate_app();
                        });
                    }
                } else if event.id().as_ref() == "show_help" {
                    // Open a new help window
                    if let Some(existing) = app_handle.get_webview_window("help") {
                        let _ = existing.set_focus();
                    } else {
                        let _ = WebviewWindowBuilder::new(
                            app_handle,
                            "help",
                            WebviewUrl::App("help.html".into()),
                        )
                        .title("Sync Positioning Help")
                        .inner_size(420.0, 480.0)
                        .resizable(false)
                        .build();
                    }
                }
            });

            // Prompt for Accessibility permission if not already granted
            // (required for sending synthetic clicks/keystrokes to other apps)
            if !check_accessibility_permission(false) {
                eprintln!("[permissions] Accessibility permission not granted — prompting user");
                check_accessibility_permission(true);
            }

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
            auto_sync,
            get_sync_enabled,
            save_click_target,
            load_click_target,
            check_screen_recording_permission,
            request_screen_recording_permission,
            close_window,
            test_click_target,
            check_accessibility_permission_cmd,
            deactivate_app_cmd
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
