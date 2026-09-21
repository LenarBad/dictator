//! OS adapters. Callers go through this module; macOS and (later) Windows
//! implement the same steps with different APIs.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod other;

#[cfg(target_os = "macos")]
use macos as sys;
#[cfg(not(target_os = "macos"))]
use other as sys;

pub use sys::{
    capture_frontmost, deliver_text, demote_hud, is_trusted, microphone_trusted, notify,
    on_settings_closed, on_settings_opened, on_setup, open_permission, style_hud, style_settings,
    FocusTarget,
};

pub fn paste_done_message(preview: &str) -> String {
    if is_trusted() {
        format!("{preview}  (если не появилось — Cmd+V)")
    } else {
        "Текст в буфере. Добавьте Dictator в Универсальный доступ — иначе вставка не сработает."
            .to_string()
    }
}
