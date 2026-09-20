use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tauri::AppHandle;
#[cfg(not(target_os = "macos"))]
use tauri_plugin_clipboard_manager::ClipboardExt;

const PASTE_DELAY: Duration = Duration::from_millis(80);
const MAIN_THREAD_TIMEOUT: Duration = Duration::from_secs(4);

pub fn deliver_text(
    app: &AppHandle,
    text: &str,
    paste: bool,
    restore: Option<&Focus>,
) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    thread::sleep(PASTE_DELAY);
    #[cfg(target_os = "macos")]
    {
        return deliver_on_macos(app, text, paste, restore);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = restore;
        app.clipboard()
            .write_text(text)
            .map_err(|err| err.to_string())?;
        if paste {
            return Err("paste is only wired on macOS in this build; text is on the clipboard".into());
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub type Focus = crate::macos::FocusTarget;

#[cfg(not(target_os = "macos"))]
pub type Focus = String;

#[cfg(target_os = "macos")]
fn deliver_on_macos(
    app: &AppHandle,
    text: &str,
    paste: bool,
    restore: Option<&crate::macos::FocusTarget>,
) -> Result<(), String> {
    let text = text.to_string();
    let target = restore.cloned();
    match run_on_main(app, move || {
        if paste {
            crate::macos::insert_or_paste(&text, target.as_ref())
        } else {
            crate::macos::write_clipboard_text(&text).map(|_| "clipboard".into())
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

#[cfg(target_os = "macos")]
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
