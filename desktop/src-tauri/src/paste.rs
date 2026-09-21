use std::thread;
use std::time::Duration;

use tauri::AppHandle;

const PASTE_DELAY: Duration = Duration::from_millis(80);

pub type Focus = crate::platform::FocusTarget;

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
    crate::platform::deliver_text(app, text, paste, restore)
}
