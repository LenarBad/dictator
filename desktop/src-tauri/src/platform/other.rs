//! Non-macOS stand-ins until `windows.rs` exists. Clipboard-only paste.

#![cfg(not(target_os = "macos"))]

use tauri::AppHandle;
use tauri::WebviewWindow;
use tauri_plugin_clipboard_manager::ClipboardExt;

#[derive(Debug, Clone)]
pub struct FocusTarget {
    pub bundle: String,
    pub pid: i32,
}

pub fn is_trusted() -> bool {
    true
}

pub fn microphone_trusted() -> bool {
    true
}

pub fn capture_frontmost() -> Option<FocusTarget> {
    None
}

pub fn deliver_text(
    app: &AppHandle,
    text: &str,
    paste: bool,
    restore: Option<&FocusTarget>,
) -> Result<(), String> {
    let _ = restore;
    app.clipboard()
        .write_text(text)
        .map_err(|err| err.to_string())?;
    if paste {
        return Err("paste is only wired on macOS in this build; text is on the clipboard".into());
    }
    Ok(())
}

pub fn notify(title: &str, message: &str) {
    eprintln!("dictator: {title}: {message}");
}

pub fn open_permission(kind: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let uri = match kind {
            "microphone" => "ms-settings:privacy-microphone",
            _ => "ms-settings:easeofaccess",
        };
        std::process::Command::new("cmd")
            .args(["/C", "start", uri])
            .spawn()
            .map_err(|err| err.to_string())?;
        return Ok(());
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = kind;
        Err("opening system settings is not wired on this OS yet".into())
    }
}

pub fn on_setup(_app: &AppHandle) {}

pub fn on_settings_opened(_app: &AppHandle) {}

pub fn on_settings_closed(_app: &AppHandle) {}

pub fn style_settings(_window: &WebviewWindow) {}

pub fn style_hud(_window: &WebviewWindow) {}

pub fn demote_hud(_window: &WebviewWindow) {}
