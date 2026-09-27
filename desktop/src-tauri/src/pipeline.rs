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
    let (diarize, chunked) = {
        let settings = state.settings.lock().expect("settings");
        (
            settings.diarization_enabled,
            settings.diarization_enabled && settings.diarization_chunked,
        )
    };
    if chunked {
        *state.chunk_run.lock().expect("chunks") = Some(spawn_chunk_worker(app.clone()));
    } else if !diarize {
        *state.segment_run.lock().expect("segments") = Some(spawn_segment_worker(app.clone()));
    }
    crate::hud::spawn_ticker(app.clone(), generation);
    spawn_segment_watch(app.clone(), generation, diarize, chunked);
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
    let chunk_run = state.chunk_run.lock().expect("chunks").take();
    if let Some(run) = chunk_run {
        let tail = state
            .recorder
            .lock()
            .expect("recorder")
            .stop_samples()
            .map(resample_capture);
        let origin = run.origin_seconds;
        let prefix = run.pending_prefix;
        drop(_gate);
        let handle = app.clone();
        thread::spawn(move || finish_chunks(&handle, run, tail, origin, prefix));
        return AppStatus::Transcribing;
    }
    let path = match state.recorder.lock().expect("recorder").stop() {
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
    let chunk_run = state.chunk_run.lock().expect("chunks").take();
    let recorder = state.recorder.lock().expect("recorder");
    if recorder.is_recording() {
        if run.is_some() || chunk_run.is_some() {
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
    if let Some(run) = chunk_run {
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

fn spawn_segment_watch(app: AppHandle, generation: u64, diarize: bool, chunked: bool) {
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
                .is_some_and(|run| run.error.lock().expect("segment error").is_some())
                || state
                    .chunk_run
                    .lock()
                    .expect("chunks")
                    .as_ref()
                    .is_some_and(|run| run.error.lock().expect("chunk error").is_some());
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
                chunked,
            );
            match decision {
                crate::segment::Action::Continue => {}
                crate::segment::Action::Rotate => {
                    let rotated = if chunked {
                        rotate_chunk(&state)
                    } else {
                        rotate_segment(&state)
                    };
                    match rotated {
                        Ok(seconds) => handed_seconds += seconds,
                        Err(_) => {
                            drop(_gate);
                            cancel_failed_session(&app);
                            return;
                        }
                    }
                }
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

struct ChunkJob {
    samples: Vec<f32>,
    /// Session time of `samples[0]`.
    origin: f64,
    /// Repeated head already owned by the previous chunk.
    prefix: f64,
    /// Local time where the next chunk takes over. Equal to the chunk length on the final piece.
    cut: f64,
    /// Speakers found inside this piece must not be merged with each other later.
    chunk: u32,
}

/// In-flight chunk diarization for one recording. Absent unless the experiment is on.
pub struct ChunkRun {
    tx: Sender<ChunkJob>,
    turns: Arc<Mutex<Vec<crate::chunk_diarize::KeptTurn>>>,
    prints: Arc<Mutex<Vec<crate::chunk_diarize::VoicePrint>>>,
    error: Arc<Mutex<Option<String>>>,
    /// Session time where the open buffer starts.
    origin_seconds: f64,
    /// Seconds at the front of the next chunk that repeat this chunk's tail.
    pending_prefix: f64,
    next_chunk: u32,
    worker: thread::JoinHandle<()>,
}

fn spawn_chunk_worker(app: AppHandle) -> ChunkRun {
    let (tx, rx) = mpsc::channel::<ChunkJob>();
    let turns = Arc::new(Mutex::new(Vec::new()));
    let prints = Arc::new(Mutex::new(Vec::new()));
    let error = Arc::new(Mutex::new(None));
    let turns_worker = Arc::clone(&turns);
    let prints_worker = Arc::clone(&prints);
    let error_worker = Arc::clone(&error);
    let worker = thread::Builder::new()
        .name("dictator-chunks".into())
        .spawn(move || {
            while let Ok(job) = rx.recv() {
                if error_worker.lock().expect("chunk error").is_some() {
                    break;
                }
                match transcribe_chunk(&app, &job, &prints_worker) {
                    Ok(piece) => {
                        if !piece.is_empty() {
                            turns_worker.lock().expect("chunk turns").extend(piece);
                        }
                    }
                    Err(err) => {
                        *error_worker.lock().expect("chunk error") = Some(err);
                        break;
                    }
                }
            }
        })
        .expect("chunk thread");
    ChunkRun {
        tx,
        turns,
        prints,
        error,
        origin_seconds: 0.0,
        pending_prefix: 0.0,
        next_chunk: 0,
        worker,
    }
}

fn rotate_chunk(state: &AppState) -> Result<f64, ()> {
    let (raw, tail) = state
        .recorder
        .lock()
        .expect("recorder")
        .take_segment_keeping_tail(crate::chunk_diarize::OVERLAP_SECONDS)
        .map_err(|_| ())?;
    let samples = resample_capture(raw);
    let duration = crate::wav::duration_seconds(&samples, crate::wav::SAMPLE_RATE);
    let mut slot = state.chunk_run.lock().expect("chunks");
    let Some(run) = slot.as_mut() else {
        return Err(());
    };
    let prefix = run.pending_prefix;
    let origin = run.origin_seconds;
    let exclusive = (duration - tail).max(0.0);
    let cut = if tail > 0.0 {
        duration - tail
    } else {
        duration
    };
    if !samples.is_empty() {
        let chunk = run.next_chunk;
        run.next_chunk = chunk.saturating_add(1);
        run.tx
            .send(ChunkJob {
                samples,
                origin,
                prefix,
                cut,
                chunk,
            })
            .map_err(|_| ())?;
    }
    run.origin_seconds = origin + exclusive;
    run.pending_prefix = tail;
    Ok(exclusive)
}

fn finish_chunks(
    app: &AppHandle,
    run: ChunkRun,
    tail: Result<Vec<f32>, String>,
    origin: f64,
    prefix: f64,
) {
    let chunk = run.next_chunk;
    let ChunkRun {
        tx,
        turns,
        prints,
        error,
        worker,
        ..
    } = run;
    let mut skip = false;
    match tail {
        Ok(samples) => {
            let duration = crate::wav::duration_seconds(&samples, crate::wav::SAMPLE_RATE);
            if origin <= 0.0 && prefix <= 0.0 && duration < MIN_UTTERANCE_SECONDS {
                skip = true;
                crate::notify::show("Dictator", "Слишком короткая запись");
            } else if !samples.is_empty() {
                let _ = tx.send(ChunkJob {
                    samples,
                    origin,
                    prefix,
                    cut: duration,
                    chunk,
                });
            }
        }
        Err(err) => {
            *error.lock().expect("chunk error") = Some(err);
        }
    }
    drop(tx);
    let _ = worker.join();
    if skip {
        set_status(app, AppStatus::Idle);
        return;
    }
    if let Some(err) = error.lock().expect("chunk error").clone() {
        crate::notify::show("Dictator", &format!("Ошибка: {err}"));
        *app.state::<AppState>()
            .engine_error
            .lock()
            .expect("engine_error") = Some(err);
        set_status(app, AppStatus::Idle);
        return;
    }
    let kept = turns.lock().expect("chunk turns").clone();
    let voice_prints = prints.lock().expect("chunk prints").clone();
    let speakers = app
        .state::<AppState>()
        .settings
        .lock()
        .expect("settings")
        .speaker_clusters();
    let (segments, texts) = crate::chunk_diarize::label_turns_for(&kept, &voice_prints, speakers);
    note_diarization(&segments);
    let text = crate::speakers::format(&segments, &texts);
    publish_transcript(app, text.trim().to_string());
    set_status(app, AppStatus::Idle);
}

fn transcribe_chunk(
    app: &AppHandle,
    job: &ChunkJob,
    prints: &Mutex<Vec<crate::chunk_diarize::VoicePrint>>,
) -> Result<Vec<crate::chunk_diarize::KeptTurn>, String> {
    if job.samples.is_empty() || !(job.cut > job.prefix) {
        return Ok(Vec::new());
    }
    ensure_engine(app)?;
    ensure_diarizer(app)?;
    let segments = {
        let state = app.state::<AppState>();
        let mut slot = state.diarizer.lock().expect("diarizer");
        let diarizer = slot.as_mut().ok_or("Разделение говорящих не загружено")?;
        let found = diarizer.process(&job.samples, crate::wav::SAMPLE_RATE, -1)?;
        diarizer.align_voices(&job.samples, crate::wav::SAMPLE_RATE, &found)
    };
    let embeddings = {
        let state = app.state::<AppState>();
        let mut slot = state.diarizer.lock().expect("diarizer");
        let diarizer = slot.as_mut().ok_or("Разделение говорящих не загружено")?;
        diarizer.speaker_embeddings(&job.samples, crate::wav::SAMPLE_RATE, &segments)
    };
    let turns = crate::speakers::merge_adjacent(&segments);
    let mut heard: Vec<i32> = segments.iter().map(|segment| segment.speaker).collect();
    heard.sort_unstable();
    heard.dedup();
    eprintln!(
        "dictator: chunk {} diarized speakers={} turns={}",
        job.chunk,
        heard.len(),
        turns.len()
    );
    let mut speaker_print: std::collections::HashMap<i32, Option<usize>> =
        std::collections::HashMap::new();
    let mut kept = Vec::new();
    if turns.is_empty() {
        if let Some(turn) = transcribe_owned(app, job, job.prefix, job.cut, None)? {
            kept.push(turn);
        }
        return Ok(kept);
    }
    for turn in &turns {
        let Some((from, to)) =
            crate::chunk_diarize::owned_span(turn.start, turn.end, job.prefix, job.cut)
        else {
            continue;
        };
        let embedding = embeddings
            .iter()
            .find(|(id, _)| *id == turn.speaker)
            .map(|(_, embedding)| embedding.clone())
            .unwrap_or_default();
        let print_id = if embedding.is_empty() {
            None
        } else {
            *speaker_print.entry(turn.speaker).or_insert_with(|| {
                let mut slot = prints.lock().expect("chunk prints");
                slot.push(crate::chunk_diarize::VoicePrint {
                    embedding,
                    chunk: job.chunk,
                });
                Some(slot.len() - 1)
            })
        };
        if let Some(kept_turn) = transcribe_owned(app, job, from, to, print_id)? {
            kept.push(kept_turn);
        }
    }
    Ok(kept)
}

fn transcribe_owned(
    app: &AppHandle,
    job: &ChunkJob,
    from: f64,
    to: f64,
    print_id: Option<usize>,
) -> Result<Option<crate::chunk_diarize::KeptTurn>, String> {
    let slice = crate::wav::slice_seconds(&job.samples, crate::wav::SAMPLE_RATE, from, to);
    if slice.is_empty() {
        return Ok(None);
    }
    let text = transcribe_segment(app, &slice)?;
    Ok(Some(crate::chunk_diarize::KeptTurn {
        start: job.origin + from,
        end: job.origin + to,
        text,
        print_id,
    }))
}

fn cancel_failed_session(app: &AppHandle) {
    let state = app.state::<AppState>();
    let _gate = state.session_gate.lock().expect("session");
    if *state.status.lock().expect("status") != AppStatus::Recording {
        return;
    }
    let run = state.segment_run.lock().expect("segments").take();
    let chunk_run = state.chunk_run.lock().expect("chunks").take();
    let _ = state.recorder.lock().expect("recorder").stop_samples();
    drop(_gate);
    let mut err = None;
    if let Some(run) = run {
        err = run.error.lock().expect("segment error").clone();
        drop(run.tx);
        let _ = run.worker.join();
    }
    if let Some(run) = chunk_run {
        if err.is_none() {
            err = run.error.lock().expect("chunk error").clone();
        }
        drop(run.tx);
        let _ = run.worker.join();
    }
    if let Some(err) = err {
        crate::notify::show("Dictator", &format!("Ошибка: {err}"));
        *app.state::<AppState>()
            .engine_error
            .lock()
            .expect("engine_error") = Some(err);
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
    let text = parts.lock().expect("parts").join(" ").trim().to_string();
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
    if let Err(err) = crate::paste::deliver_text(app, &text, settings.paste_enabled, focus.as_ref())
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
    let clusters = app
        .state::<AppState>()
        .settings
        .lock()
        .expect("settings")
        .speaker_clusters();
    let segments = {
        let state = app.state::<AppState>();
        let mut slot = state.diarizer.lock().expect("diarizer");
        let diarizer = slot.as_mut().ok_or("Разделение говорящих не загружено")?;
        diarizer.process(&samples, rate, clusters)?
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
            .process(&samples, crate::wav::SAMPLE_RATE, -1)
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
