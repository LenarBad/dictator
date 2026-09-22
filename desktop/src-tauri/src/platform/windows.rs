//! Windows focus restore, clipboard + Ctrl+V, HUD chrome, and toasts.

#![cfg(target_os = "windows")]

use std::fs;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use clipboard_win::{formats, get_clipboard, set_clipboard};
use cpal::traits::HostTrait;
use tauri::{AppHandle, WebviewWindow};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GetWindowThreadProcessId, IsWindow,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, HWND_TOPMOST,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

const MAIN_THREAD_TIMEOUT: Duration = Duration::from_secs(4);
const SKIP_CLASSES: &[&str] = &[
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Progman",
    "WorkerW",
    "NotifyIconOverflowWindow",
];

#[derive(Debug, Clone)]
pub struct FocusTarget {
    pub bundle: String,
    pub pid: i32,
    hwnd: isize,
}

pub fn is_trusted() -> bool {
    true
}

pub fn microphone_trusted() -> bool {
    cpal::default_host().default_input_device().is_some()
}

pub fn capture_frontmost() -> Option<FocusTarget> {
    unsafe {
        let hwnd = GetForegroundWindow();
        target_from_hwnd(hwnd)
    }
}

fn target_from_hwnd(hwnd: HWND) -> Option<FocusTarget> {
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return None;
        }
        let class = class_name(hwnd);
        if is_skipped_class(&class) {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid as *mut u32));
        if pid == 0 || pid == GetCurrentProcessId() {
            return None;
        }
        Some(FocusTarget {
            bundle: class,
            pid: pid as i32,
            hwnd: hwnd.0 as isize,
        })
    }
}

fn is_skipped_class(class: &str) -> bool {
    SKIP_CLASSES
        .iter()
        .any(|name| class.eq_ignore_ascii_case(name))
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..n as usize])
}

fn hwnd_from_isize(value: isize) -> HWND {
    HWND(value as _)
}

fn restore_focus(target: &FocusTarget) -> bool {
    unsafe {
        let hwnd = hwnd_from_isize(target.hwnd);
        if !IsWindow(Some(hwnd)).as_bool() {
            return false;
        }
        let fg = GetForegroundWindow();
        let fg_tid = GetWindowThreadProcessId(fg, None);
        let dest_tid = GetWindowThreadProcessId(hwnd, None);
        let cur = GetCurrentThreadId();
        let _ = AttachThreadInput(cur, fg_tid, true);
        let _ = AttachThreadInput(cur, dest_tid, true);
        let ok = SetForegroundWindow(hwnd).as_bool();
        let _ = AttachThreadInput(cur, dest_tid, false);
        let _ = AttachThreadInput(cur, fg_tid, false);
        ok
    }
}

fn write_clipboard_text(text: &str) -> Result<(), String> {
    set_clipboard(formats::Unicode, text).map_err(|err| format!("clipboard write: {err}"))?;
    let written: String = get_clipboard(formats::Unicode).unwrap_or_default();
    if written != text {
        write_note(&format!(
            "clipboard readback mismatch: wrote {} chars, read {}",
            text.chars().count(),
            written.chars().count()
        ));
        return Err("clipboard did not keep the recognized text".into());
    }
    Ok(())
}

fn send_ctrl_v() -> Result<(), String> {
    let inputs = [
        key_input(VK_CONTROL, false),
        key_input(VK_V, false),
        key_input(VK_V, true),
        key_input(VK_CONTROL, true),
    ];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        return Err(format!("SendInput posted {sent}/{} events", inputs.len()));
    }
    Ok(())
}

fn key_input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn insert_or_paste(text: &str, target: Option<&FocusTarget>) -> Result<String, String> {
    let mut log = String::new();
    write_clipboard_text(text)?;
    log.push_str(&format!("clipboard ok ({} chars)\n", text.chars().count()));

    if let Some(target) = target {
        let restored = restore_focus(target);
        thread::sleep(Duration::from_millis(120));
        log.push_str(&format!(
            "restore={restored} pid={} class={}\n",
            target.pid, target.bundle
        ));
    } else {
        log.push_str("no focus target captured at recording start\n");
    }

    match send_ctrl_v() {
        Ok(()) => {
            log.push_str("posted Ctrl+V\n");
            write_note(&log);
            Ok("sendinput".into())
        }
        Err(err) => {
            log.push_str(&format!("Ctrl+V failed: {err}\n"));
            write_note(&log);
            // Text is already on the clipboard — same fallback as Mac without AX.
            Ok("clipboard".into())
        }
    }
}

pub fn deliver_text(
    app: &AppHandle,
    text: &str,
    paste: bool,
    restore: Option<&FocusTarget>,
) -> Result<(), String> {
    let text = text.to_string();
    let target = restore.cloned();
    match run_on_main(app, move || {
        if paste {
            insert_or_paste(&text, target.as_ref())
        } else {
            write_clipboard_text(&text).map(|_| "clipboard".into())
        }
    }) {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(err)) => {
            eprintln!("dictator: paste failed ({err})");
            Err(err)
        }
        Err(err) => {
            eprintln!("dictator: paste dispatch failed ({err})");
            Err(err)
        }
    }
}

fn run_on_main<T, F>(app: &AppHandle, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(work());
    })
    .map_err(|err| err.to_string())?;
    rx.recv_timeout(MAIN_THREAD_TIMEOUT)
        .map_err(|_| "timed out waiting for the main thread".into())
}

pub fn notify(title: &str, message: &str) {
    let title = title.to_string();
    let message = message.to_string();
    let _ = thread::Builder::new()
        .name("dictator-notify".into())
        .spawn(move || {
            let shown = tauri_winrt_notification::Toast::new(
                tauri_winrt_notification::Toast::POWERSHELL_APP_ID,
            )
            .title(&title)
            .text1(&message)
            .show();
            if let Err(err) = shown {
                eprintln!("dictator: toast failed: {err}");
            }
        });
}

pub fn open_permission(kind: &str) -> Result<(), String> {
    let uri = match kind {
        "microphone" => "ms-settings:privacy-microphone",
        _ => "ms-settings:easeofaccess",
    };
    std::process::Command::new("cmd")
        .args(["/C", "start", "", uri])
        .spawn()
        .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn on_setup(_app: &AppHandle) {}

pub fn on_settings_opened(_app: &AppHandle) {}

pub fn on_settings_closed(_app: &AppHandle) {}

pub fn style_settings(window: &WebviewWindow) {
    let _ = window.set_skip_taskbar(false);
}

pub fn style_hud(window: &WebviewWindow) {
    apply_noactivate(window);
}

pub fn demote_hud(window: &WebviewWindow) {
    apply_noactivate(window);
}

fn apply_noactivate(window: &WebviewWindow) {
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    unsafe {
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let updated = current | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, updated);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

fn write_note(message: &str) {
    eprintln!("dictator: {message}");
    let mut path = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    path.push("dictator");
    let _ = fs::create_dir_all(&path);
    path.push("last-paste.log");
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "?".into());
    let body = format!("exe={exe}\n{message}");
    let _ = fs::write(path, body);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_shell_and_desktop() {
        assert!(is_skipped_class("Shell_TrayWnd"));
        assert!(is_skipped_class("Progman"));
        assert!(!is_skipped_class("Notepad"));
    }
}
