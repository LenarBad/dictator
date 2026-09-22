//! macOS TCC helpers, focus restore, Cmd+V paste, HUD chrome, and toasts.

#![cfg(target_os = "macos")]

use std::ffi::CStr;
use std::fs;
use std::ptr;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSApplicationActivationOptions, NSPasteboard, NSPasteboardTypeString, NSPasteboardWriting,
    NSRunningApplication, NSWorkspace,
};
use objc2_foundation::{NSArray, NSString};
use tauri::{AppHandle, WebviewWindow};

const MAIN_THREAD_TIMEOUT: Duration = Duration::from_secs(4);

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: core_foundation::dictionary::CFDictionaryRef)
        -> bool;
}

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {}

pub fn is_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// `AVAuthorizationStatusAuthorized` (3). NotDetermined/Denied keep the settings row visible.
pub fn microphone_trusted() -> bool {
    microphone_auth_status() == 3
}

fn microphone_auth_status() -> isize {
    unsafe {
        let Some(cls) = objc2::runtime::AnyClass::get(cstr(b"AVCaptureDevice\0")) else {
            return 0;
        };
        let media = NSString::from_str("soun");
        objc2::msg_send![cls, authorizationStatusForMediaType: &*media]
    }
}

fn cstr(bytes: &'static [u8]) -> &'static CStr {
    CStr::from_bytes_with_nul(bytes).expect("C string")
}

pub fn prompt_if_needed() {
    if is_trusted() {
        return;
    }
    let key = CFString::new("AXTrustedCheckOptionPrompt");
    let yes = CFBoolean::true_value();
    let dict = CFDictionary::from_CFType_pairs(&[(key, yes)]);
    unsafe {
        AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef());
    }
}

const SKIP_BUNDLES: &[&str] = &[
    "io.lenar.dictator",
    "com.apple.systemuiserver",
    "com.apple.controlcenter",
    "com.apple.notificationcenterui",
    "com.apple.dock",
    "com.apple.Spotlight",
    "com.apple.loginwindow",
    "com.apple.WindowManager",
];

#[derive(Debug, Clone)]
pub struct FocusTarget {
    pub bundle: String,
    pub pid: i32,
}

pub fn capture_frontmost() -> Option<FocusTarget> {
    let workspace = NSWorkspace::sharedWorkspace();
    for candidate in [
        workspace.frontmostApplication(),
        workspace.menuBarOwningApplication(),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(target) = target_from_running(&candidate) {
            return Some(target);
        }
    }
    for app in workspace.runningApplications() {
        if app.isActive() {
            if let Some(target) = target_from_running(&app) {
                return Some(target);
            }
        }
    }
    None
}

fn target_from_running(app: &NSRunningApplication) -> Option<FocusTarget> {
    let bundle = app
        .bundleIdentifier()
        .map(|id| id.to_string())
        .unwrap_or_default();
    if is_skipped(&bundle) {
        return None;
    }
    let pid = app.processIdentifier();
    if pid <= 0 && bundle.is_empty() {
        return None;
    }
    Some(FocusTarget { bundle, pid })
}

fn is_skipped(bundle: &str) -> bool {
    if bundle.is_empty() {
        return false;
    }
    if SKIP_BUNDLES.contains(&bundle) {
        return true;
    }
    let self_bundle = NSRunningApplication::currentApplication()
        .bundleIdentifier()
        .map(|id| id.to_string())
        .unwrap_or_default();
    !self_bundle.is_empty() && bundle == self_bundle
}

pub fn restore_focus(target: &FocusTarget) -> bool {
    let app = if !target.bundle.is_empty() {
        let identifier = NSString::from_str(&target.bundle);
        NSRunningApplication::runningApplicationsWithBundleIdentifier(&identifier)
            .firstObject()
            .or_else(|| {
                if target.pid > 0 {
                    NSRunningApplication::runningApplicationWithProcessIdentifier(target.pid)
                } else {
                    None
                }
            })
    } else if target.pid > 0 {
        NSRunningApplication::runningApplicationWithProcessIdentifier(target.pid)
    } else {
        None
    };
    let Some(app) = app else {
        return false;
    };
    #[allow(deprecated)]
    if app.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps) {
        return true;
    }
    let current = NSRunningApplication::currentApplication();
    app.activateFromApplication_options(
        &current,
        NSApplicationActivationOptions::ActivateAllWindows,
    )
}

pub fn insert_or_paste(text: &str, target: Option<&FocusTarget>) -> Result<String, String> {
    let mut log = String::new();
    write_clipboard_text(text)?;
    log.push_str(&format!("clipboard ok ({} chars)\n", text.chars().count()));

    if let Some(target) = target {
        let restored = restore_focus(target);
        thread::sleep(Duration::from_millis(150));
        log.push_str(&format!(
            "restore={restored} pid={} bundle={}\n",
            target.pid, target.bundle
        ));
    } else {
        log.push_str("no focus target captured at recording start\n");
    }

    // One path for every app: clipboard + Cmd+V. Writing AXSelectedText
    // duplicated text in Telegram and other native fields.
    match paste_command_v() {
        Ok(()) => {
            log.push_str("posted Cmd+V\n");
            write_note(&log);
            Ok("cgevent".into())
        }
        Err(err) => {
            log.push_str(&format!("Cmd+V failed: {err}\n"));
            write_note(&log);
            Err(err)
        }
    }
}

/// `clearContents` + `setString:forType:` is a no-op: the type is no longer
/// declared, so the pasteboard stays empty. `writeObjects` is the AppKit path
/// that actually publishes a string (same as arboard).
pub fn write_clipboard_text(text: &str) -> Result<(), String> {
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    let objects =
        NSArray::from_retained_slice(&[ProtocolObject::<dyn NSPasteboardWriting>::from_retained(
            NSString::from_str(text),
        )]);
    if !pasteboard.writeObjects(&objects) {
        write_note("NSPasteboard writeObjects failed");
        return Err("NSPasteboard writeObjects failed".into());
    }
    let written = pasteboard
        .stringForType(unsafe { NSPasteboardTypeString })
        .map(|value| value.to_string())
        .unwrap_or_default();
    if written != text {
        write_note(&format!(
            "clipboard readback mismatch: wrote {} chars, read {}\n",
            text.chars().count(),
            written.chars().count()
        ));
        return Err("clipboard did not keep the recognized text".into());
    }
    Ok(())
}

fn paste_command_v() -> Result<(), String> {
    unsafe {
        let cmd_down = CGEventCreateKeyboardEvent(ptr::null_mut(), KEY_COMMAND, true);
        let down = CGEventCreateKeyboardEvent(ptr::null_mut(), KEY_V, true);
        let up = CGEventCreateKeyboardEvent(ptr::null_mut(), KEY_V, false);
        let cmd_up = CGEventCreateKeyboardEvent(ptr::null_mut(), KEY_COMMAND, false);
        if cmd_down.is_null() || down.is_null() || up.is_null() || cmd_up.is_null() {
            release_event(cmd_down);
            release_event(down);
            release_event(up);
            release_event(cmd_up);
            return Err("CGEventCreateKeyboardEvent failed".into());
        }
        CGEventSetFlags(cmd_down, FLAG_COMMAND);
        CGEventSetFlags(down, FLAG_COMMAND);
        CGEventSetFlags(up, FLAG_COMMAND);
        CGEventPost(HID_TAP, cmd_down);
        thread::sleep(Duration::from_millis(8));
        CGEventPost(HID_TAP, down);
        thread::sleep(Duration::from_millis(15));
        CGEventPost(HID_TAP, up);
        thread::sleep(Duration::from_millis(8));
        CGEventPost(HID_TAP, cmd_up);
        release_event(cmd_down);
        release_event(down);
        release_event(up);
        release_event(cmd_up);
    }
    Ok(())
}

fn release_event(event: CGEventRef) {
    if !event.is_null() {
        unsafe { CFRelease(event.cast()) };
    }
}

fn write_note(message: &str) {
    eprintln!("dictator: {message}");
    let mut path = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    path.push("dictator");
    let _ = fs::create_dir_all(&path);
    path.push("last-paste.log");
    let trusted = is_trusted();
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "?".into());
    let body = format!("trusted={trusted}\nexe={exe}\n{message}");
    let _ = fs::write(path, body);
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
    let title = serde_json::to_string(title).unwrap_or_else(|_| "\"Dictator\"".into());
    let message = serde_json::to_string(message).unwrap_or_else(|_| "\"\"".into());
    // osascript can hang on Automation prompts. Never wait for it on the
    // caller's thread — that used to freeze the Carbon hotkey callback.
    let _ = std::thread::Builder::new()
        .name("dictator-notify".into())
        .spawn(move || {
            let script = format!("display notification {message} with title {title}");
            let _ = std::process::Command::new("osascript")
                .args(["-e", &script])
                .status();
        });
}

pub fn open_permission(kind: &str) -> Result<(), String> {
    let url = match kind {
        "microphone" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        "accessibility" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        _ => return Err("unknown permission pane".into()),
    };
    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn on_setup(app: &AppHandle) {
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    prompt_if_needed();
}

pub fn on_settings_opened(app: &AppHandle) {
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
}

pub fn on_settings_closed(app: &AppHandle) {
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
}

pub fn style_settings(window: &WebviewWindow) {
    if let Err(err) = window_vibrancy::apply_vibrancy(
        window,
        window_vibrancy::NSVisualEffectMaterial::WindowBackground,
        Some(window_vibrancy::NSVisualEffectState::Active),
        None,
    ) {
        eprintln!("dictator: vibrancy failed: {err}");
    }
}

pub fn style_hud(window: &WebviewWindow) {
    if let Err(err) = window_vibrancy::apply_vibrancy(
        window,
        window_vibrancy::NSVisualEffectMaterial::HudWindow,
        Some(window_vibrancy::NSVisualEffectState::Active),
        Some(24.0),
    ) {
        eprintln!("dictator: hud vibrancy failed: {err}");
    }
    configure_panel(window);
}

pub fn demote_hud(window: &WebviewWindow) {
    let Ok(ptr) = window.ns_window() else {
        return;
    };
    if ptr.is_null() {
        return;
    }
    unsafe {
        let ns = &*ptr.cast::<objc2::runtime::AnyObject>();
        let _: () = objc2::msg_send![ns, resignKeyWindow];
        let _: () = objc2::msg_send![ns, orderFrontRegardless];
    }
}

fn configure_panel(window: &WebviewWindow) {
    let Ok(ptr) = window.ns_window() else {
        return;
    };
    if ptr.is_null() {
        return;
    }
    unsafe {
        let ns = &*ptr.cast::<objc2::runtime::AnyObject>();
        let _: () = objc2::msg_send![ns, setHidesOnDeactivate: false];
        // CanJoinAllSpaces | Transient | IgnoresCycle | FullScreenAuxiliary
        let behavior: usize = (1 << 0) | (1 << 3) | (1 << 6) | (1 << 8);
        let _: () = objc2::msg_send![ns, setCollectionBehavior: behavior];
    }
}

const KEY_V: u16 = 9; // kVK_ANSI_V
const KEY_COMMAND: u16 = 0x37; // kVK_Command
const FLAG_COMMAND: u64 = 0x0010_0000; // kCGEventFlagMaskCommand
const HID_TAP: u32 = 0; // kCGHIDEventTap

type CGEventRef = *mut std::ffi::c_void;
type CGEventSourceRef = *mut std::ffi::c_void;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreateKeyboardEvent(
        source: CGEventSourceRef,
        virtual_key: u16,
        key_down: bool,
    ) -> CGEventRef;
    fn CGEventSetFlags(event: CGEventRef, flags: u64);
    fn CGEventPost(tap: u32, event: CGEventRef);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *mut std::ffi::c_void);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_system_ui_and_self() {
        assert!(is_skipped("com.apple.systemuiserver"));
        assert!(is_skipped("io.lenar.dictator"));
        assert!(!is_skipped("com.apple.Notes"));
    }
}
