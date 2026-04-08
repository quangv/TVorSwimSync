use serde::{Deserialize, Serialize};
use std::fs;
use std::process::Command;
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

fn run_applescript(script: &str) -> Option<String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;

    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    } else {
        None
    }
}

fn get_tradingview_title() -> Option<String> {
    run_applescript(
        r#"tell application "System Events"
  if exists process "TradingView" then
    tell process "TradingView"
      return name of front window
    end tell
  end if
end tell"#,
    )
}

fn get_thinkorswim_title() -> Option<String> {
    run_applescript(
        r#"tell application "System Events"
  if exists process "thinkorswim" then
    tell process "thinkorswim"
      return name of front window
    end tell
  end if
end tell"#,
    )
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
