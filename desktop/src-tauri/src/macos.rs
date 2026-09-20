//! macOS TCC helpers, focus restore, AX insert, and Cmd+V paste.

#![cfg(target_os = "macos")]

use std::fs;
use std::ptr;
use std::thread;
use std::time::Duration;

use core_foundation::base::{CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::{CFString, CFStringRef};
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSApplicationActivationOptions, NSPasteboard, NSPasteboardTypeString, NSPasteboardWriting,
    NSRunningApplication, NSWorkspace,
};
use objc2_foundation::{NSArray, NSString};

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: core_foundation::dictionary::CFDictionaryRef) -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> i32;
}

pub fn is_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
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

    let use_ax = target.is_some_and(|target| target.pid > 0 && !is_browser_like(&target.bundle));
    if use_ax {
        let pid = target.expect("checked").pid;
        match ax_insert_text(pid, text) {
            Ok(()) => {
                log.push_str("inserted via AXSelectedText\n");
                write_note(&log);
                return Ok("ax".into());
            }
            Err(err) => log.push_str(&format!("AX insert failed: {err}\n")),
        }
    } else if target.is_some_and(|target| is_browser_like(&target.bundle)) {
        log.push_str("target is Electron-like; skip AXSelectedText\n");
    } else {
        log.push_str("skip AXSelectedText (no reliable native target)\n");
    }

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

fn is_browser_like(bundle: &str) -> bool {
    let bundle = bundle.to_ascii_lowercase();
    bundle.starts_with("com.todesktop.")
        || bundle.starts_with("com.google.chrome")
        || bundle.starts_with("company.thebrowser.")
        || bundle.starts_with("com.microsoft.vscode")
        || bundle.starts_with("com.visualstudio.code")
        || bundle.starts_with("com.apple.safari")
        || bundle.starts_with("org.mozilla.firefox")
        || bundle.contains("electron")
        || bundle.contains("chrome")
}

/// `clearContents` + `setString:forType:` is a no-op: the type is no longer
/// declared, so the pasteboard stays empty. `writeObjects` is the AppKit path
/// that actually publishes a string (same as arboard).
pub fn write_clipboard_text(text: &str) -> Result<(), String> {
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    let objects = NSArray::from_retained_slice(&[ProtocolObject::<dyn NSPasteboardWriting>::from_retained(
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

fn ax_insert_text(pid: i32, text: &str) -> Result<(), String> {
    if !is_trusted() {
        return Err("accessibility not granted".into());
    }
    if pid <= 0 {
        return Err("no target pid for AX insert".into());
    }
    unsafe {
        let app = AXUIElementCreateApplication(pid);
        if app.is_null() {
            return Err("AXUIElementCreate failed".into());
        }
        let focused_attr = CFString::new("AXFocusedUIElement");
        let mut focused: CFTypeRef = ptr::null();
        let err = AXUIElementCopyAttributeValue(app, focused_attr.as_concrete_TypeRef(), &mut focused);
        if err != 0 || focused.is_null() {
            CFRelease(app.cast());
            return Err(format!("no focused UI element (ax={err})"));
        }
        let selected_attr = CFString::new("AXSelectedText");
        let value = CFString::new(text);
        let set_err = AXUIElementSetAttributeValue(
            focused.cast_mut(),
            selected_attr.as_concrete_TypeRef(),
            value.as_CFTypeRef(),
        );
        let readback = ax_copy_string(focused.cast_mut(), "AXSelectedText");
        CFRelease(focused.cast_mut());
        CFRelease(app.cast());
        if set_err != 0 {
            return Err(format!("AXSelectedText failed (ax={set_err})"));
        }
        if readback.as_deref() != Some(text) {
            return Err(format!(
                "AXSelectedText did not stick (read {:?})",
                readback.as_deref().unwrap_or("")
            ));
        }
        Ok(())
    }
}

fn ax_copy_string(element: AXUIElementRef, attribute: &str) -> Option<String> {
    unsafe {
        let attr = CFString::new(attribute);
        let mut value: CFTypeRef = ptr::null();
        let err = AXUIElementCopyAttributeValue(element, attr.as_concrete_TypeRef(), &mut value);
        if err != 0 || value.is_null() {
            return None;
        }
        let cf = CFString::wrap_under_create_rule(value as CFStringRef);
        Some(cf.to_string())
    }
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

const KEY_V: u16 = 9; // kVK_ANSI_V
const KEY_COMMAND: u16 = 0x37; // kVK_Command
const FLAG_COMMAND: u64 = 0x0010_0000; // kCGEventFlagMaskCommand
const HID_TAP: u32 = 0; // kCGHIDEventTap

type AXUIElementRef = *mut std::ffi::c_void;
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
    fn electron_and_browser_apps_skip_ax() {
        assert!(is_browser_like("com.todesktop.example"));
        assert!(is_browser_like("com.microsoft.VSCode"));
        assert!(!is_browser_like("com.apple.Notes"));
    }

    #[test]
    fn skips_system_ui_and_self() {
        assert!(is_skipped("com.apple.systemuiserver"));
        assert!(is_skipped("io.lenar.dictator"));
        assert!(!is_skipped("com.apple.Notes"));
    }
}
