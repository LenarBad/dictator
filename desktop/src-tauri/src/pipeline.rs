use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::wav::MIN_UTTERANCE_SECONDS;
use crate::{refresh_tray, AppState, AppStatus};

pub fn ensure_engine(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let slot = state.engine.lock().expect("engine");
        if slot.is_some() {
            return Ok(());
        }
    }
    let settings = state.settings.lock().expect("settings").clone();
    let mut engine = crate::stt::Engine::new()?;
    if settings.preload_model {
        engine.preload()?;
        crate::notify::show("Dictator", "Модель готова");
    }
    *state.engine.lock().expect("engine") = Some(engine);
    *state.engine_error.lock().expect("engine_error") = None;
    Ok(())
}

pub fn start_recording(app: &AppHandle) -> Result<AppStatus, String> {
    let state = app.state::<AppState>();
    #[cfg(target_os = "macos")]
    {
        if let Some(target) = crate::macos::capture_frontmost() {
            eprintln!(
                "dictator: captured focus pid={} bundle={}",
                target.pid, target.bundle
            );
            *state.focus.lock().expect("focus") = Some(target);
        } else {
            eprintln!("dictator: no focus target at recording start; will Cmd+V into current focus");
        }
    }
    let device = {
        let names: Vec<String> = crate::recorder::list_microphones()
            .into_iter()
            .map(|mic| mic.name)
            .collect();
        let mut settings = state.settings.lock().expect("settings");
        settings.apply_microphone_preference(&names)
    };
    state
        .recorder
        .lock()
        .expect("recorder")
        .start(device.as_deref())
        .map_err(|err| {
            crate::notify::show("Dictator", &format!("Микрофон недоступен: {err}"));
            err
        })?;
    let generation = {
        let mut gen = state.record_gen.lock().expect("gen");
        *gen += 1;
        *gen
    };
    set_status(app, AppStatus::Recording);
    crate::notify::show(
        "Dictator",
        "Запись… Нажмите хоткей или пункт меню, чтобы остановить.",
    );
    spawn_limit_watch(app.clone(), generation);
    Ok(AppStatus::Recording)
}

pub fn stop_and_transcribe(app: &AppHandle) -> AppStatus {
    set_status(app, AppStatus::Transcribing);
    let path = match app.state::<AppState>().recorder.lock().expect("recorder").stop() {
        Ok(path) => path,
        Err(err) => {
            crate::notify::show("Dictator", &format!("Не удалось остановить запись: {err}"));
            set_status(app, AppStatus::Idle);
            return AppStatus::Idle;
        }
    };
    let handle = app.clone();
    thread::spawn(move || transcribe_and_deliver(&handle, path));
    AppStatus::Transcribing
}

pub fn shutdown(app: &AppHandle) {
    let state = app.state::<AppState>();
    let recorder = state.recorder.lock().expect("recorder");
    if recorder.is_recording() {
        if let Ok(path) = recorder.stop() {
            let _ = std::fs::remove_file(path);
        }
    }
    drop(recorder);
    *state.engine.lock().expect("engine") = None;
}

fn transcribe_and_deliver(app: &AppHandle, path: PathBuf) {
    let settings = app.state::<AppState>().settings.lock().expect("settings").clone();
    let focus = app.state::<AppState>().focus.lock().expect("focus").clone();
    let result = (|| {
        let duration = {
            let reader = hound::WavReader::open(&path).map_err(|err| err.to_string())?;
            reader.duration() as f64 / f64::from(reader.spec().sample_rate.max(1))
        };
        if duration < MIN_UTTERANCE_SECONDS {
            crate::notify::show("Dictator", "Слишком короткая запись");
            return Ok(());
        }
        ensure_engine(app)?;
        let text = {
            let state = app.state::<AppState>();
            let mut slot = state.engine.lock().expect("engine");
            let engine = slot.as_mut().ok_or("STT engine missing")?;
            engine.transcribe(&path)?
        };
        let text = text.trim().to_string();
        if text.is_empty() {
            crate::notify::show("Dictator", "Пустой результат распознавания");
            return Ok(());
        }
        crate::paste::deliver_text(app, &text, settings.paste_enabled, focus.as_ref())?;
        let preview = if text.chars().count() <= 80 {
            text.clone()
        } else {
            format!("{}…", text.chars().take(77).collect::<String>())
        };
        if settings.paste_enabled {
            #[cfg(target_os = "macos")]
            {
                if crate::macos::is_trusted() {
                    crate::notify::show("Dictator", &format!("{preview}  (если не появилось — Cmd+V)"));
                } else {
                    crate::notify::show(
                        "Dictator",
                        "Текст в буфере. Добавьте Dictator в Универсальный доступ — иначе вставка не сработает.",
                    );
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                crate::notify::show("Dictator", &format!("{preview}  (если не появилось — Cmd+V)"));
            }
        } else {
            crate::notify::show("Dictator", &format!("Скопировано: {preview}"));
        }
        Ok(())
    })();
    if let Err(err) = result {
        crate::notify::show("Dictator", &format!("Ошибка: {err}"));
        *app.state::<AppState>()
            .engine_error
            .lock()
            .expect("engine_error") = Some(err);
    }
    let _ = std::fs::remove_file(&path);
    set_status(app, AppStatus::Idle);
}

fn spawn_limit_watch(app: AppHandle, generation: u64) {
    let limit = app
        .state::<AppState>()
        .settings
        .lock()
        .expect("settings")
        .max_recording_seconds
        .max(5.0);
    thread::spawn(move || {
        thread::sleep(Duration::from_secs_f64(limit));
        let state = app.state::<AppState>();
        if *state.record_gen.lock().expect("gen") != generation {
            return;
        }
        if *state.status.lock().expect("status") != AppStatus::Recording {
            return;
        }
        let _ = crate::pipeline::stop_and_transcribe(&app);
    });
}

fn set_status(app: &AppHandle, status: AppStatus) {
    *app.state::<AppState>().status.lock().expect("status") = status;
    let _ = app.emit("status-changed", status);
    refresh_tray(app);
}

pub fn spawn_engine_in_background(app: AppHandle) {
    thread::spawn(move || {
        let preload = app
            .state::<AppState>()
            .settings
            .lock()
            .expect("settings")
            .preload_model;
        if preload {
            crate::notify::show("Dictator", "Загрузка модели…");
        }
        if let Err(err) = ensure_engine(&app) {
            *app.state::<AppState>()
                .engine_error
                .lock()
                .expect("engine_error") = Some(err.clone());
            crate::notify::show("Dictator", &err);
        }
        let _ = app.emit("status-changed", AppStatus::Idle);
        refresh_tray(&app);
    });
}
