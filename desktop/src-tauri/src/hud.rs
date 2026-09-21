use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, Position, WebviewWindow};

use crate::{AppState, AppStatus};

pub const HUD_LABEL: &str = "hud";

const HUD_WIDTH: f64 = 290.0;
const HUD_HEIGHT: f64 = 54.0;
const HUD_TOP_PAD: f64 = 12.0;

#[derive(Clone, Serialize)]
pub struct HudFrame {
    pub status: AppStatus,
    pub elapsed: f64,
    pub bars: Vec<f32>,
    pub hotkey: String,
}

pub fn snapshot(app: &AppHandle) -> HudFrame {
    let state = app.state::<AppState>();
    let status = *state.status.lock().expect("status");
    let hotkey = state.settings.lock().expect("settings").hotkey.clone();
    let recorder = state.recorder.lock().expect("recorder");
    HudFrame {
        status,
        elapsed: recorder.elapsed_seconds(),
        bars: recorder.bars(),
        hotkey,
    }
}

pub fn prefetch(app: &AppHandle) {
    let _ = ensure(app);
}

pub fn sync(app: &AppHandle, status: AppStatus) {
    match status {
        AppStatus::Recording | AppStatus::Transcribing => show(app),
        AppStatus::Idle => hide(app),
    }
    let _ = app.emit("hud-frame", snapshot(app));
}

pub fn spawn_ticker(app: AppHandle, generation: u64) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(40));
        let state = app.state::<AppState>();
        if *state.record_gen.lock().expect("gen") != generation {
            break;
        }
        if *state.status.lock().expect("status") != AppStatus::Recording {
            break;
        }
        drop(state);
        let _ = app.emit("hud-frame", snapshot(&app));
    });
}

fn show(app: &AppHandle) {
    let Some(window) = ensure(app) else {
        return;
    };
    position(&window, app);
    style(&window);
    let _ = window.show();
    #[cfg(target_os = "macos")]
    demote_key(&window);
}

fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(HUD_LABEL) {
        let _ = window.hide();
    }
}

fn ensure(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(window) = app.get_webview_window(HUD_LABEL) {
        return Some(window);
    }
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == HUD_LABEL)
        .cloned()?;
    match tauri::WebviewWindowBuilder::from_config(app, &config).and_then(|builder| {
        builder
            .transparent(true)
            .theme(Some(tauri::Theme::Dark))
            .visible(false)
            .focused(false)
            .build()
    }) {
        Ok(window) => {
            style(&window);
            Some(window)
        }
        Err(err) => {
            eprintln!("dictator: failed to open hud: {err}");
            None
        }
    }
}

fn style(window: &WebviewWindow) {
    let _ = window.set_theme(Some(tauri::Theme::Dark));
    let _ = window.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)));
    #[cfg(target_os = "macos")]
    {
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
}

fn position(window: &WebviewWindow, app: &AppHandle) {
    let Some(monitor) = monitor_for(app) else {
        return;
    };
    let scale = monitor.scale_factor();
    let work = monitor.work_area();
    let hud_w = (HUD_WIDTH * scale).round() as u32;
    let pad = (HUD_TOP_PAD * scale).round() as i32;
    let (x, y) = hud_origin(
        work.position.x,
        work.position.y,
        work.size.width,
        hud_w,
        pad,
    );
    let _ = window.set_size(tauri::LogicalSize::new(HUD_WIDTH, HUD_HEIGHT));
    let _ = window.set_position(Position::Physical(PhysicalPosition { x, y }));
}

/// Center the pill horizontally at the top of the monitor work area.
pub fn hud_origin(work_x: i32, work_y: i32, work_w: u32, hud_w: u32, pad: i32) -> (i32, i32) {
    let x = work_x + (work_w as i32 - hud_w as i32) / 2;
    let y = work_y + pad;
    (x, y)
}

fn monitor_for(app: &AppHandle) -> Option<tauri::Monitor> {
    if let Ok(pos) = app.cursor_position() {
        if let Ok(Some(monitor)) = app.monitor_from_point(pos.x, pos.y) {
            return Some(monitor);
        }
    }
    app.primary_monitor().ok().flatten()
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
fn demote_key(window: &WebviewWindow) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hud_centers_on_the_work_area() {
        let (x, y) = hud_origin(0, 38, 1440, 624, 24);
        assert_eq!(x, (1440 - 624) / 2);
        assert_eq!(y, 62);
    }
}
