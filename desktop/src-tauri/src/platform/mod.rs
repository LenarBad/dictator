//! OS adapters. Callers go through this module; macOS and Windows
//! implement the same steps with different APIs.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod win;

#[cfg(target_os = "macos")]
use macos as sys;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use other as sys;
#[cfg(target_os = "windows")]
use win as sys;

pub use sys::{
    capture_frontmost, deliver_text, demote_hud, is_trusted, microphone_trusted, notify,
    on_settings_closed, on_settings_opened, on_setup, open_permission, style_hud, style_settings,
    FocusTarget,
};

pub fn paste_done_message(preview: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{preview}  (если не появилось — Ctrl+V)")
    } else if is_trusted() {
        format!("{preview}  (если не появилось — Cmd+V)")
    } else {
        "Текст в буфере. Добавьте Dictator в Универсальный доступ — иначе вставка не сработает."
            .to_string()
    }
}
