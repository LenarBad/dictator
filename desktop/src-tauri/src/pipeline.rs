use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
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
    if let Some(target) = crate::platform::capture_frontmost() {
        eprintln!(
            "dictator: captured focus pid={} bundle={}",
            target.pid, target.bundle
        );
        *state.focus.lock().expect("focus") = Some(target);
    } else {
        eprintln!("dictator: no focus target at recording start; will Cmd+V into current focus");
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
    let diarize = state.settings.lock().expect("settings").diarization_enabled;
    if !diarize {
        *state.segment_run.lock().expect("segments") = Some(spawn_segment_worker(app.clone()));
    }
    crate::hud::spawn_ticker(app.clone(), generation);
    spawn_segment_watch(app.clone(), generation, diarize);
    Ok(AppStatus::Recording)
}

/// In-flight segment transcripts for one recording. Absent while diarization
/// holds the whole session for a single pass at the end.
pub struct SegmentRun {
    tx: Sender<Vec<f32>>,
    parts: Arc<Mutex<Vec<String>>>,
    error: Arc<Mutex<Option<String>>>,
    /// Seconds already queued, not including the open microphone buffer.
    handed_seconds: f64,
    worker: thread::JoinHandle<()>,
}

pub fn stop_and_transcribe(app: &AppHandle) -> AppStatus {
    let state = app.state::<AppState>();
    let _gate = state.session_gate.lock().expect("session");
    {
        let status = state.status.lock().expect("status");
        if *status != AppStatus::Recording {
            return *status;
        }
    }
    set_status(app, AppStatus::Transcribing);
    let run = state.segment_run.lock().expect("segments").take();
    if let Some(run) = run {
        let tail = state
            .recorder
            .lock()
            .expect("recorder")
            .stop_samples()
            .map(resample_capture);
        let handed_seconds = run.handed_seconds;
        drop(_gate);
        let handle = app.clone();
        thread::spawn(move || finish_segments(&handle, run, tail, handed_seconds));
        return AppStatus::Transcribing;
    }
    let path = match state.recorder.lock().expect("recorder").stop()
    {
        Ok(path) => path,
        Err(err) => {
            drop(_gate);
            crate::notify::show("Dictator", &format!("Не удалось остановить запись: {err}"));
            set_status(app, AppStatus::Idle);
            return AppStatus::Idle;
        }
    };
    drop(_gate);
    let handle = app.clone();
    thread::spawn(move || transcribe_and_deliver(&handle, path));
    AppStatus::Transcribing
}

pub fn shutdown(app: &AppHandle) {
    let state = app.state::<AppState>();
    let _gate = state.session_gate.lock().expect("session");
    *state.record_gen.lock().expect("gen") += 1;
    *state.status.lock().expect("status") = AppStatus::Idle;
    let run = state.segment_run.lock().expect("segments").take();
    let recorder = state.recorder.lock().expect("recorder");
    if recorder.is_recording() {
        if run.is_some() {
            let _ = recorder.stop_samples();
        } else if let Ok(path) = recorder.stop() {
            let _ = std::fs::remove_file(path);
        }
    }
    drop(recorder);
    if let Some(run) = run {
        drop(run.tx);
        let _ = run.worker.join();
    }
    *state.engine.lock().expect("engine") = None;
    *state.diarizer.lock().expect("diarizer") = None;
}

/// Deletes the temp WAV when dropped (normal return, error, or unwind).
struct TempWav(PathBuf);

impl Drop for TempWav {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn transcribe_and_deliver(app: &AppHandle, path: PathBuf) {
    let wav = TempWav(path);
    let diarize = app
        .state::<AppState>()
        .settings
        .lock()
        .expect("settings")
        .diarization_enabled;
    let result = (|| {
        let duration = {
            let reader = hound::WavReader::open(&wav.0).map_err(|err| err.to_string())?;
            reader.duration() as f64 / f64::from(reader.spec().sample_rate.max(1))
        };
        if duration < MIN_UTTERANCE_SECONDS {
            crate::notify::show("Dictator", "Слишком короткая запись");
            return Ok(());
        }
        ensure_engine(app)?;
        let text = if diarize {
            ensure_diarizer(app)?;
            transcribe_with_speakers(app, &wav.0)?
        } else {
            let state = app.state::<AppState>();
            let mut slot = state.engine.lock().expect("engine");
            let engine = slot.as_mut().ok_or("STT engine missing")?;
            engine.transcribe(&wav.0)?
        };
        publish_transcript(app, text.trim().to_string());
        Ok(())
    })();
    if let Err(err) = result {
        crate::notify::show("Dictator", &format!("Ошибка: {err}"));
        *app.state::<AppState>()
            .engine_error
            .lock()
            .expect("engine_error") = Some(err);
    }
    drop(wav);
    set_status(app, AppStatus::Idle);
}

fn spawn_segment_worker(app: AppHandle) -> SegmentRun {
    let (tx, rx) = mpsc::channel::<Vec<f32>>();
    let parts = Arc::new(Mutex::new(Vec::new()));
    let error = Arc::new(Mutex::new(None));
    let parts_worker = Arc::clone(&parts);
    let error_worker = Arc::clone(&error);
    let worker = thread::Builder::new()
        .name("dictator-segments".into())
        .spawn(move || {
            while let Ok(samples) = rx.recv() {
                if error_worker.lock().expect("segment error").is_some() {
                    break;
                }
                match transcribe_segment(&app, &samples) {
                    Ok(text) => {
                        if !text.is_empty() {
                            parts_worker.lock().expect("parts").push(text);
                        }
                    }
                    Err(err) => {
                        *error_worker.lock().expect("segment error") = Some(err);
                        break;
                    }
                }
            }
        })
        .expect("segment thread");
    SegmentRun {
        tx,
        parts,
        error,
        handed_seconds: 0.0,
        worker,
    }
}

fn spawn_segment_watch(app: AppHandle, generation: u64, diarize: bool) {
    thread::spawn(move || {
        let mut handed_seconds = 0.0;
        loop {
            thread::sleep(Duration::from_millis(100));
            let state = app.state::<AppState>();
            let _gate = state.session_gate.lock().expect("session");
            if *state.record_gen.lock().expect("gen") != generation {
                return;
            }
            if *state.status.lock().expect("status") != AppStatus::Recording {
                return;
            }
            let failed = state
                .segment_run
                .lock()
                .expect("segments")
                .as_ref()
                .is_some_and(|run| run.error.lock().expect("segment error").is_some());
            if failed {
                drop(_gate);
                cancel_failed_session(&app);
                return;
            }
            let snapshot = state.recorder.lock().expect("recorder").segment_snapshot();
            let decision = crate::segment::action(
                snapshot.segment_seconds,
                handed_seconds,
                snapshot.silence_seconds,
                diarize,
            );
            match decision {
                crate::segment::Action::Continue => {}
                crate::segment::Action::Rotate => match rotate_segment(&state) {
                    Ok(seconds) => handed_seconds += seconds,
                    Err(_) => {
                        drop(_gate);
                        cancel_failed_session(&app);
                        return;
                    }
                },
                crate::segment::Action::Stop => {
                    drop(_gate);
                    let _ = stop_and_transcribe(&app);
                    return;
                }
            }
        }
    });
}

fn rotate_segment(state: &AppState) -> Result<f64, ()> {
    let raw = state
        .recorder
        .lock()
        .expect("recorder")
        .take_segment()
        .map_err(|_| ())?;
    let seconds = if raw.capture_rate == 0 {
        0.0
    } else {
        raw.samples.len() as f64 / f64::from(raw.capture_rate)
    };
    let samples = resample_capture(raw);
    if samples.is_empty() {
        return Ok(seconds);
    }
    let mut slot = state.segment_run.lock().expect("segments");
    let Some(run) = slot.as_mut() else {
        return Err(());
    };
    run.handed_seconds += seconds;
    run.tx.send(samples).map_err(|_| ())?;
    Ok(seconds)
}

fn cancel_failed_session(app: &AppHandle) {
    let state = app.state::<AppState>();
    let _gate = state.session_gate.lock().expect("session");
    if *state.status.lock().expect("status") != AppStatus::Recording {
        return;
    }
    let run = state.segment_run.lock().expect("segments").take();
    let _ = state.recorder.lock().expect("recorder").stop_samples();
    drop(_gate);
    if let Some(run) = run {
        let err = run.error.lock().expect("segment error").clone();
        drop(run.tx);
        let _ = run.worker.join();
        if let Some(err) = err {
            crate::notify::show("Dictator", &format!("Ошибка: {err}"));
            *app.state::<AppState>()
                .engine_error
                .lock()
                .expect("engine_error") = Some(err);
        }
    }
    set_status(app, AppStatus::Idle);
}

fn finish_segments(
    app: &AppHandle,
    run: SegmentRun,
    tail: Result<Vec<f32>, String>,
    handed_seconds: f64,
) {
    let SegmentRun {
        tx,
        parts,
        error,
        worker,
        ..
    } = run;
    let mut skip_tail = false;
    match tail {
        Ok(samples) => {
            let duration = crate::wav::duration_seconds(&samples, crate::wav::SAMPLE_RATE);
            if handed_seconds <= 0.0 && duration < MIN_UTTERANCE_SECONDS {
                skip_tail = true;
                crate::notify::show("Dictator", "Слишком короткая запись");
            } else if !samples.is_empty() {
                let _ = tx.send(samples);
            }
        }
        Err(err) => {
            *error.lock().expect("segment error") = Some(err);
        }
    }
    drop(tx);
    let _ = worker.join();
    if skip_tail {
        set_status(app, AppStatus::Idle);
        return;
    }
    if let Some(err) = error.lock().expect("segment error").clone() {
        crate::notify::show("Dictator", &format!("Ошибка: {err}"));
        *app.state::<AppState>()
            .engine_error
            .lock()
            .expect("engine_error") = Some(err);
        set_status(app, AppStatus::Idle);
        return;
    }
    let text = parts
        .lock()
        .expect("parts")
        .join(" ")
        .trim()
        .to_string();
    publish_transcript(app, text);
    set_status(app, AppStatus::Idle);
}

fn resample_capture(raw: crate::recorder::RawCapture) -> Vec<f32> {
    crate::wav::resample(&raw.samples, raw.capture_rate, crate::wav::SAMPLE_RATE)
}

fn transcribe_segment(app: &AppHandle, samples: &[f32]) -> Result<String, String> {
    if crate::wav::duration_seconds(samples, crate::wav::SAMPLE_RATE) < MIN_UTTERANCE_SECONDS {
        return Ok(String::new());
    }
    ensure_engine(app)?;
    let state = app.state::<AppState>();
    let mut slot = state.engine.lock().expect("engine");
    let engine = slot.as_mut().ok_or("STT engine missing")?;
    engine.transcribe_samples(samples, crate::wav::SAMPLE_RATE)
}

fn publish_transcript(app: &AppHandle, text: String) {
    let settings = app
        .state::<AppState>()
        .settings
        .lock()
        .expect("settings")
        .clone();
    let focus = app.state::<AppState>().focus.lock().expect("focus").clone();
    if text.is_empty() {
        crate::notify::show("Dictator", "Пустой результат распознавания");
        return;
    }
    if let Err(err) =
        crate::paste::deliver_text(app, &text, settings.paste_enabled, focus.as_ref())
    {
        crate::notify::show("Dictator", &format!("Ошибка: {err}"));
        *app.state::<AppState>()
            .engine_error
            .lock()
            .expect("engine_error") = Some(err);
        return;
    }
    let preview = if text.chars().count() <= 80 {
        text
    } else {
        format!("{}…", text.chars().take(77).collect::<String>())
    };
    if settings.paste_enabled {
        crate::notify::show("Dictator", &crate::platform::paste_done_message(&preview));
    } else {
        crate::notify::show("Dictator", &format!("Скопировано: {preview}"));
    }
}

fn set_status(app: &AppHandle, status: AppStatus) {
    *app.state::<AppState>().status.lock().expect("status") = status;
    let _ = app.emit("status-changed", status);
    crate::hud::sync(app, status);
    refresh_tray(app);
}

pub fn spawn_engine_in_background(app: AppHandle) {
    thread::spawn(move || {
        let (preload, diarize) = {
            let state = app.state::<AppState>();
            let settings = state.settings.lock().expect("settings");
            (settings.preload_model, settings.diarization_enabled)
        };
        if preload {
            crate::notify::show("Dictator", "Загрузка модели…");
        }
        if let Err(err) = ensure_engine(&app) {
            *app.state::<AppState>()
                .engine_error
                .lock()
                .expect("engine_error") = Some(err.clone());
            crate::notify::show("Dictator", &err);
        } else if diarize {
            if let Err(err) = ensure_diarizer(&app) {
                *app.state::<AppState>()
                    .engine_error
                    .lock()
                    .expect("engine_error") = Some(err.clone());
                crate::notify::show("Dictator", &err);
            }
        }
        let _ = app.emit("status-changed", AppStatus::Idle);
        refresh_tray(&app);
    });
}

fn ensure_diarizer(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let slot = state.diarizer.lock().expect("diarizer");
        if slot.is_some() {
            return Ok(());
        }
    }
    let preload = state.settings.lock().expect("settings").preload_model;
    let mut diarizer = crate::diarize::Diarizer::new()?;
    if preload {
        diarizer.preload()?;
    }
    *state.diarizer.lock().expect("diarizer") = Some(diarizer);
    Ok(())
}

/// Diarize the whole recording, then run GigaAM on each merged turn.
/// The toggle-off path does not call this.
fn transcribe_with_speakers(app: &AppHandle, audio: &std::path::Path) -> Result<String, String> {
    let (samples, rate) = crate::wav::read_pcm16_wav(audio)?;
    let segments = {
        let state = app.state::<AppState>();
        let mut slot = state.diarizer.lock().expect("diarizer");
        let diarizer = slot.as_mut().ok_or("Разделение говорящих не загружено")?;
        diarizer.process(&samples, rate)?
    };
    let segments = {
        let state = app.state::<AppState>();
        let mut slot = state.diarizer.lock().expect("diarizer");
        let diarizer = slot.as_mut().ok_or("Разделение говорящих не загружено")?;
        diarizer.align_voices(&samples, rate, &segments)
    };
    note_diarization(&segments);
    let state = app.state::<AppState>();
    let mut slot = state.engine.lock().expect("engine");
    let engine = slot.as_mut().ok_or("STT engine missing")?;
    transcribe_turns(engine, &segments, &samples, rate)
}

fn transcribe_turns(
    engine: &mut crate::stt::Engine,
    segments: &[crate::speakers::Segment],
    samples: &[f32],
    sample_rate: u32,
) -> Result<String, String> {
    if samples.is_empty() || sample_rate == 0 {
        return Ok(String::new());
    }
    let turns = crate::speakers::merge_adjacent(&crate::speakers::absorb_overlaps(segments));
    // No speech regions: still paste a normal transcript, without speaker labels.
    if turns.is_empty() {
        return engine.transcribe_samples(samples, sample_rate);
    }
    let mut texts = Vec::with_capacity(turns.len());
    for turn in &turns {
        let slice = crate::wav::slice_seconds(samples, sample_rate, turn.start, turn.end);
        if slice.is_empty() {
            texts.push(String::new());
            continue;
        }
        texts.push(engine.transcribe_samples(&slice, sample_rate)?);
    }
    Ok(crate::speakers::format(&turns, &texts))
}

fn note_diarization(segments: &[crate::speakers::Segment]) {
    let mut speakers: Vec<i32> = segments.iter().map(|segment| segment.speaker).collect();
    speakers.sort_unstable();
    speakers.dedup();
    let spans = segments
        .iter()
        .map(|segment| {
            format!(
                "{:.2}-{:.2}:{}",
                segment.start, segment.end, segment.speaker
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let message = format!(
        "segments={} speakers={} [{}]",
        segments.len(),
        speakers.len(),
        spans
    );
    eprintln!("dictator: diarization {message}");
    let mut path = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    path.push("dictator");
    let _ = std::fs::create_dir_all(&path);
    path.push("diarization.log");
    let _ = std::fs::write(path, message + "\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_diarization_has_no_speaker_prefix() {
        let mut engine = crate::stt::Engine::stub();
        let mut diarizer = crate::diarize::Diarizer::stub();
        let samples = vec![0.0_f32; crate::wav::SAMPLE_RATE as usize];
        let segments = diarizer
            .process(&samples, crate::wav::SAMPLE_RATE)
            .expect("segments");
        let text = transcribe_turns(&mut engine, &segments, &samples, crate::wav::SAMPLE_RATE)
            .expect("text");
        assert!(text.contains("[stub]"), "{text}");
        assert!(!text.contains("Спикер"), "{text}");
    }

    #[test]
    fn empty_diarization_falls_back_to_plain_stub() {
        let mut engine = crate::stt::Engine::stub();
        let samples = vec![0.0_f32; crate::wav::SAMPLE_RATE as usize];
        let text =
            transcribe_turns(&mut engine, &[], &samples, crate::wav::SAMPLE_RATE).expect("text");
        assert!(text.contains("[stub]"), "{text}");
        assert!(!text.contains("Спикер"), "{text}");
    }
}
