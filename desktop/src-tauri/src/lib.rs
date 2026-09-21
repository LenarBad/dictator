mod hotkey;
mod hud;
#[cfg(target_os = "macos")]
mod macos;
mod notify;
mod paste;
mod pipeline;
mod recorder;
mod settings;
mod stt;
mod wav;

use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

use crate::hotkey::parse_hotkey;
use crate::settings::{is_macos_conflict_hotkey, Settings};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AppStatus {
    Idle,
    Recording,
    Transcribing,
}

impl AppStatus {
    fn label_ru(self) -> &'static str {
        match self {
            Self::Idle => "Ожидание",
            Self::Recording => "Запись",
            Self::Transcribing => "Распознавание",
        }
    }
}

struct TrayItems {
    status: MenuItem<Wry>,
    hotkey: MenuItem<Wry>,
    toggle: MenuItem<Wry>,
}

pub(crate) struct AppState {
    pub settings: Mutex<Settings>,
    pub status: Mutex<AppStatus>,
    last_toggle: Mutex<Option<Instant>>,
    tray: Mutex<Option<TrayItems>>,
    pub recorder: Mutex<recorder::Recorder>,
    pub engine: Mutex<Option<stt::Engine>>,
    pub engine_error: Mutex<Option<String>>,
    pub focus: Mutex<Option<crate::paste::Focus>>,
    pub record_gen: Mutex<u64>,
}

#[derive(Serialize)]
struct UiState {
    status: AppStatus,
    status_label: String,
    settings: Settings,
    settings_path: String,
    recording_wired: bool,
    engine_ready: bool,
    engine_error: Option<String>,
    microphones: Vec<recorder::MicrophoneInfo>,
    accessibility_trusted: bool,
    microphone_trusted: bool,
}

#[tauri::command]
fn get_state(app: AppHandle) -> UiState {
    let state = app.state::<AppState>();
    let status = *state.status.lock().expect("status");
    let engine_ready = state
        .engine
        .try_lock()
        .ok()
        .is_some_and(|slot| slot.is_some());
    let engine_error = state.engine_error.lock().expect("engine_error").clone();
    let microphones = recorder::list_microphones();
    let names: Vec<String> = microphones.iter().map(|mic| mic.name.clone()).collect();
    let settings = {
        let mut settings = state.settings.lock().expect("settings");
        settings.apply_microphone_preference(&names);
        settings.clone()
    };
    UiState {
        status,
        status_label: status.label_ru().to_string(),
        settings,
        settings_path: crate::settings::settings_path().display().to_string(),
        recording_wired: true,
        engine_ready,
        engine_error,
        microphones,
        accessibility_trusted: accessibility_trusted(),
        microphone_trusted: microphone_trusted(),
    }
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: Settings) -> Result<Settings, String> {
    let mut next = settings;
    next.hotkey = next.hotkey.trim().to_string();
    parse_hotkey(&next.hotkey)?;
    if cfg!(target_os = "macos") && is_macos_conflict_hotkey(&next.hotkey) {
        return Err(
            "На macOS ctrl+space занят сменой языка. Выберите другое сочетание, например ctrl+shift+d."
                .into(),
        );
    }
    next.model_name = crate::settings::DEFAULT_MODEL.to_string();
    if let Err(err) = register_current_hotkey(&app, &next.hotkey) {
        restore_saved_hotkey(&app);
        return Err(err);
    }
    next.save()?;
    *app.state::<AppState>().settings.lock().expect("settings") = next.clone();
    refresh_tray(&app);
    Ok(next)
}

#[tauri::command]
fn pause_hotkey(app: AppHandle) -> Result<(), String> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn resume_hotkey(app: AppHandle) -> Result<(), String> {
    restore_saved_hotkey(&app)
}

#[tauri::command]
fn hud_snapshot(app: AppHandle) -> crate::hud::HudFrame {
    crate::hud::snapshot(&app)
}

#[tauri::command]
fn toggle_recording(app: AppHandle) -> Result<AppStatus, String> {
    run_toggle_on_main(&app)
}

#[tauri::command]
fn open_permission(kind: String) -> Result<(), String> {
    open_permission_inner(&kind)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let settings = Settings::load();
    let startup_hotkey = settings.hotkey.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(shortcut_plugin(&startup_hotkey))
        .manage(AppState {
            settings: Mutex::new(settings),
            status: Mutex::new(AppStatus::Idle),
            last_toggle: Mutex::new(None),
            tray: Mutex::new(None),
            recorder: Mutex::new(recorder::Recorder::default()),
            engine: Mutex::new(None),
            engine_error: Mutex::new(None),
            focus: Mutex::new(None),
            record_gen: Mutex::new(0),
        })
        .setup(|app| {
            build_tray(app.handle())?;
            refresh_tray(app.handle());
            pipeline::spawn_engine_in_background(app.handle().clone());

            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                crate::macos::prompt_if_needed();
            }
            crate::hud::prefetch(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == crate::hud::HUD_LABEL {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
                return;
            }
            if window.label() != "settings" {
                return;
            }
            match event {
                tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed => {
                    let app = window.app_handle().clone();
                    let _ = thread::Builder::new()
                        .name("dictator-hotkey-restore".into())
                        .spawn(move || {
                            if let Err(err) = restore_saved_hotkey(&app) {
                                eprintln!("dictator: failed to restore hotkey: {err}");
                            }
                        });
                    #[cfg(target_os = "macos")]
                    {
                        let _ = window
                            .app_handle()
                            .set_activation_policy(tauri::ActivationPolicy::Accessory);
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            save_settings,
            hud_snapshot,
            toggle_recording,
            open_permission,
            pause_hotkey,
            resume_hotkey
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            match event {
                tauri::RunEvent::ExitRequested { api, code, .. } => {
                    // Last window closed (the settings pane). Keep the tray app alive.
                    // `app.exit(0)` from the tray Quit item still has Some(code).
                    if code.is_none() {
                        api.prevent_exit();
                    }
                }
                tauri::RunEvent::Exit => pipeline::shutdown(app_handle),
                _ => {}
            }
        });
}

fn shortcut_plugin(hotkey: &str) -> tauri::plugin::TauriPlugin<Wry> {
    let builder = tauri_plugin_global_shortcut::Builder::new().with_handler(on_global_shortcut);
    let builder = match parse_hotkey(hotkey) {
        Ok(shortcut) => builder.with_shortcut(shortcut).unwrap_or_else(|err| {
            eprintln!("dictator: failed to attach startup hotkey: {err}");
            tauri_plugin_global_shortcut::Builder::new().with_handler(on_global_shortcut)
        }),
        Err(_) => builder,
    };
    builder.build()
}

fn on_global_shortcut(app: &AppHandle, _shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state() == ShortcutState::Pressed {
        // Return immediately. tauri-plugin-global-shortcut holds a non-reentrant
        // Mutex around this callback; doing AppKit/tray work here deadlocks the
        // main thread when Carbon delivers KeyReleased (hotkeys appear dead).
        schedule_toggle(app);
    }
}

fn schedule_toggle(app: &AppHandle) {
    let app = app.clone();
    let _ = thread::Builder::new()
        .name("dictator-toggle".into())
        .spawn(move || {
            if let Err(err) = run_toggle_on_main(&app) {
                crate::notify::show("Dictator", &err);
            }
        });
}

fn run_toggle_on_main(app: &AppHandle) -> Result<AppStatus, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(toggle_recording_inner(&handle));
    })
    .map_err(|err| err.to_string())?;
    rx.recv()
        .map_err(|_| "main thread dropped toggle".to_string())?
}

fn restore_saved_hotkey(app: &AppHandle) -> Result<(), String> {
    let combo = app
        .state::<AppState>()
        .settings
        .lock()
        .expect("settings")
        .hotkey
        .clone();
    register_current_hotkey(app, &combo)
}

fn register_current_hotkey(app: &AppHandle, combo: &str) -> Result<(), String> {
    let shortcut = parse_hotkey(combo)?;
    let manager = app.global_shortcut();
    if manager.is_registered(shortcut) {
        return Ok(());
    }
    manager.unregister_all().map_err(|err| err.to_string())?;
    manager.register(shortcut).map_err(|err| err.to_string())
}

fn build_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let _ = app.remove_tray_by_id("main");
    let status = MenuItem::with_id(app, "status", "Статус: Ожидание", false, None::<&str>)?;
    let hotkey = MenuItem::with_id(app, "hotkey", "Хоткей: ctrl+shift+d", false, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "Записать", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Настройки…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &status,
            &hotkey,
            &PredefinedMenuItem::separator(app)?,
            &toggle,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id("main")
        .icon(tray_image(AppStatus::Idle))
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("Dictator")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => schedule_toggle(app),
            "settings" => show_settings(app),
            "quit" => {
                pipeline::shutdown(app);
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    app.state::<AppState>()
        .tray
        .lock()
        .expect("tray")
        .replace(TrayItems {
            status,
            hotkey,
            toggle,
        });
    Ok(())
}

fn toggle_recording_inner(app: &AppHandle) -> Result<AppStatus, String> {
    let state = app.state::<AppState>();
    {
        let mut last = state.last_toggle.lock().expect("debounce");
        if let Some(previous) = *last {
            if previous.elapsed() < Duration::from_millis(350) {
                return Ok(*state.status.lock().expect("status"));
            }
        }
        *last = Some(Instant::now());
    }

    let status = *state.status.lock().expect("status");
    if status == AppStatus::Transcribing {
        return Ok(status);
    }
    if status == AppStatus::Recording {
        return Ok(pipeline::stop_and_transcribe(app));
    }
    pipeline::start_recording(app)
}

pub(crate) fn refresh_tray(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || refresh_tray_on_main(&handle));
}

fn refresh_tray_on_main(app: &AppHandle) {
    let state = app.state::<AppState>();
    let status = *state.status.lock().expect("status");
    let hotkey = state.settings.lock().expect("settings").hotkey.clone();
    if let Some(items) = state.tray.lock().expect("tray").as_ref() {
        let _ = items
            .status
            .set_text(format!("Статус: {}", status.label_ru()));
        let _ = items.hotkey.set_text(format!("Хоткей: {hotkey}"));
        let toggle_label = if status == AppStatus::Recording {
            "Остановить запись"
        } else {
            "Записать"
        };
        let _ = items.toggle.set_text(toggle_label);
        let _ = items.toggle.set_enabled(status != AppStatus::Transcribing);
    }
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(format!("Dictator — {} ({hotkey})", status.label_ru())));
        let _ = tray
            .set_icon_with_as_template(Some(tray_image(status)), matches!(status, AppStatus::Idle));
    }
}

fn tray_image(status: AppStatus) -> Image<'static> {
    let bytes: &[u8] = match status {
        AppStatus::Idle => include_bytes!("../icons/tray-idle.png"),
        AppStatus::Recording => include_bytes!("../icons/tray-recording.png"),
        AppStatus::Transcribing => include_bytes!("../icons/tray-transcribing.png"),
    };
    Image::from_bytes(bytes).expect("tray icon png")
}

fn apply_window_glass(window: &tauri::WebviewWindow) {
    let _ = window.set_theme(Some(tauri::Theme::Dark));
    let _ = window.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)));
    #[cfg(target_os = "macos")]
    {
        if let Err(err) = window_vibrancy::apply_vibrancy(
            window,
            window_vibrancy::NSVisualEffectMaterial::HudWindow,
            Some(window_vibrancy::NSVisualEffectState::Active),
            None,
        ) {
            eprintln!("dictator: vibrancy failed: {err}");
        }
    }
}

fn show_settings(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    }
    if let Some(window) = app.get_webview_window("settings") {
        apply_window_glass(&window);
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let Some(config) = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == "settings")
        .cloned()
    else {
        eprintln!("dictator: settings window is not configured");
        return;
    };
    match tauri::WebviewWindowBuilder::from_config(app, &config).and_then(|builder| {
        builder
            .transparent(true)
            .theme(Some(tauri::Theme::Dark))
            .visible(true)
            .build()
    }) {
        Ok(window) => {
            apply_window_glass(&window);
            let _ = window.show();
            let _ = window.set_focus();
        }
        Err(err) => eprintln!("dictator: failed to open settings: {err}"),
    }
}

fn open_permission_inner(kind: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
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
        return Ok(());
    }
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
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = kind;
        Err("opening system settings is not wired on this OS yet".into())
    }
}

fn microphone_trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::macos::microphone_trusted()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

fn accessibility_trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::macos::is_trusted()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}
