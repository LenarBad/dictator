use std::collections::VecDeque;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, SupportedStreamConfig};
use serde::Serialize;

const MIC_CACHE_TTL: Duration = Duration::from_secs(30);
static MIC_CACHE: Mutex<Option<(Instant, Vec<MicrophoneInfo>)>> = Mutex::new(None);

pub const HUD_BARS: usize = 17;

use crate::wav::{self, SAMPLE_RATE};

enum RecCmd {
    Start {
        device: Option<String>,
        reply: Sender<Result<(), String>>,
    },
    Stop {
        reply: Sender<Result<std::path::PathBuf, String>>,
    },
    /// Stop the microphone and return the open buffer without writing a file.
    StopSamples {
        reply: Sender<Result<RawCapture, String>>,
    },
    /// Hand off the open buffer and keep the microphone running.
    TakeSegment {
        reply: Sender<Result<RawCapture, String>>,
    },
    Snapshot {
        reply: Sender<SegmentSnapshot>,
    },
    IsRecording {
        reply: Sender<bool>,
    },
    Elapsed {
        reply: Sender<f64>,
    },
}

/// Mono host-rate samples already downmixed in the capture callback.
pub struct RawCapture {
    pub samples: Vec<f32>,
    pub capture_rate: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SegmentSnapshot {
    pub segment_seconds: f64,
    pub silence_seconds: f64,
}

struct CaptureBuf {
    samples: Vec<f32>,
    silent_seconds: f64,
    sample_rate: u32,
}

impl CaptureBuf {
    fn new() -> Self {
        Self {
            samples: Vec::new(),
            silent_seconds: 0.0,
            sample_rate: SAMPLE_RATE,
        }
    }

    fn reset(&mut self, sample_rate: u32) {
        self.samples.clear();
        self.silent_seconds = 0.0;
        self.sample_rate = sample_rate.max(1);
    }

    fn snapshot(&self) -> SegmentSnapshot {
        let rate = self.sample_rate.max(1);
        SegmentSnapshot {
            segment_seconds: self.samples.len() as f64 / f64::from(rate),
            silence_seconds: self.silent_seconds,
        }
    }

    fn take(&mut self) -> RawCapture {
        let samples = std::mem::take(&mut self.samples);
        self.silent_seconds = 0.0;
        RawCapture {
            samples,
            capture_rate: self.sample_rate.max(1),
        }
    }

    fn push_mono(&mut self, mono: &[f32]) {
        if mono.is_empty() {
            return;
        }
        let sum_sq: f64 = mono
            .iter()
            .map(|sample| {
                let sample = f64::from(*sample);
                sample * sample
            })
            .sum();
        self.samples.extend_from_slice(mono);
        let rate = self.sample_rate.max(1);
        let segment_seconds = self.samples.len() as f64 / f64::from(rate);
        let chunk_seconds = mono.len() as f64 / f64::from(rate);
        let rms = (sum_sq / mono.len() as f64).sqrt();
        self.silent_seconds =
            crate::segment::next_silence(segment_seconds, chunk_seconds, rms, self.silent_seconds);
    }
}

pub struct Recorder {
    tx: Sender<RecCmd>,
    meter: Arc<Mutex<LevelMeter>>,
}

pub struct LevelMeter {
    bars: VecDeque<f32>,
    hop: usize,
    acc_n: usize,
    acc_peak: f32,
    acc_sumsq: f32,
    /// Slow peak envelope so quiet laptop mics still fill the HUD.
    peak_env: f32,
}

impl Default for LevelMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl LevelMeter {
    pub fn new() -> Self {
        Self {
            bars: std::iter::repeat(0.0).take(HUD_BARS).collect(),
            hop: 320,
            acc_n: 0,
            acc_peak: 0.0,
            acc_sumsq: 0.0,
            peak_env: 0.08,
        }
    }

    pub fn reset(&mut self, sample_rate: u32) {
        self.bars.clear();
        self.bars.extend(std::iter::repeat(0.0).take(HUD_BARS));
        self.hop = (sample_rate as usize / 40).clamp(160, 2000);
        self.acc_n = 0;
        self.acc_peak = 0.0;
        self.acc_sumsq = 0.0;
        self.peak_env = 0.08;
    }

    pub fn push(&mut self, samples: &[f32]) {
        for &sample in samples {
            let amplitude = sample.abs();
            if amplitude > self.acc_peak {
                self.acc_peak = amplitude;
            }
            self.acc_sumsq += sample * sample;
            self.acc_n += 1;
            if self.acc_n >= self.hop {
                self.flush();
            }
        }
    }

    fn flush(&mut self) {
        let rms = (self.acc_sumsq / self.acc_n.max(1) as f32).sqrt();
        let amp = self.acc_peak.max(rms);
        if amp > self.peak_env {
            self.peak_env = amp;
        } else {
            self.peak_env *= 0.993;
        }
        // Quiet built-in mics often peak around 0.02–0.05; lift them into the
        // visualizer's useful range without clipping already-loud speech.
        let makeup = (0.16 / self.peak_env.max(0.045)).clamp(1.0, 4.0);
        if self.bars.len() >= HUD_BARS {
            self.bars.pop_front();
        }
        self.bars
            .push_back(visual_bar(self.acc_peak * makeup, rms * makeup));
        self.acc_n = 0;
        self.acc_peak = 0.0;
        self.acc_sumsq = 0.0;
    }

    pub fn bars(&self) -> Vec<f32> {
        self.bars.iter().copied().collect()
    }
}

pub fn visual_bar(peak: f32, rms: f32) -> f32 {
    let amp = (peak * 0.72 + rms * 0.28).max(1e-6);
    let db = 20.0 * amp.log10();
    // Speech on a laptop mic lives around −40…−12 dB. Map that onto the
    // full HUD so the wave matches the hero pill instead of a flat line.
    let t = ((db + 46.0) / 40.0).clamp(0.0, 1.0);
    0.16 + 0.84 * t.powf(0.72)
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Recorder {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let meter = Arc::new(Mutex::new(LevelMeter::new()));
        let meter_thread = Arc::clone(&meter);
        thread::Builder::new()
            .name("dictator-mic".into())
            .spawn(move || recorder_loop(rx, meter_thread))
            .expect("microphone thread");
        Self { tx, meter }
    }

    pub fn is_recording(&self) -> bool {
        let (reply, rx) = mpsc::channel();
        if self.tx.send(RecCmd::IsRecording { reply }).is_err() {
            return false;
        }
        rx.recv().unwrap_or(false)
    }

    pub fn elapsed_seconds(&self) -> f64 {
        let (reply, rx) = mpsc::channel();
        if self.tx.send(RecCmd::Elapsed { reply }).is_err() {
            return 0.0;
        }
        rx.recv().unwrap_or(0.0)
    }

    pub fn bars(&self) -> Vec<f32> {
        self.meter
            .lock()
            .map(|meter| meter.bars())
            .unwrap_or_else(|_| vec![0.18; HUD_BARS])
    }

    pub fn start(&self, device_name: Option<&str>) -> Result<(), String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(RecCmd::Start {
                device: device_name.map(str::to_string),
                reply,
            })
            .map_err(|_| "microphone thread exited".to_string())?;
        rx.recv()
            .map_err(|_| "microphone thread exited".to_string())?
    }

    pub fn stop(&self) -> Result<std::path::PathBuf, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(RecCmd::Stop { reply })
            .map_err(|_| "microphone thread exited".to_string())?;
        rx.recv()
            .map_err(|_| "microphone thread exited".to_string())?
    }

    pub fn stop_samples(&self) -> Result<RawCapture, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(RecCmd::StopSamples { reply })
            .map_err(|_| "microphone thread exited".to_string())?;
        rx.recv()
            .map_err(|_| "microphone thread exited".to_string())?
    }

    pub fn take_segment(&self) -> Result<RawCapture, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(RecCmd::TakeSegment { reply })
            .map_err(|_| "microphone thread exited".to_string())?;
        rx.recv()
            .map_err(|_| "microphone thread exited".to_string())?
    }

    pub fn segment_snapshot(&self) -> SegmentSnapshot {
        let (reply, rx) = mpsc::channel();
        if self.tx.send(RecCmd::Snapshot { reply }).is_err() {
            return SegmentSnapshot::default();
        }
        rx.recv().unwrap_or_default()
    }
}

struct Inner {
    stream: Option<Stream>,
    samples: Arc<Mutex<CaptureBuf>>,
    meter: Arc<Mutex<LevelMeter>>,
    capture_rate: u32,
    started_at: Option<Instant>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        release_stream(self);
    }
}

fn recorder_loop(rx: mpsc::Receiver<RecCmd>, meter: Arc<Mutex<LevelMeter>>) {
    let mut inner = Inner {
        stream: None,
        samples: Arc::new(Mutex::new(CaptureBuf::new())),
        meter,
        capture_rate: SAMPLE_RATE,
        started_at: None,
    };
    while let Ok(cmd) = rx.recv() {
        match cmd {
            RecCmd::Start { device, reply } => {
                let _ = reply.send(start_inner(&mut inner, device.as_deref()));
            }
            RecCmd::Stop { reply } => {
                let _ = reply.send(stop_inner(&mut inner));
            }
            RecCmd::StopSamples { reply } => {
                let _ = reply.send(stop_samples_inner(&mut inner));
            }
            RecCmd::TakeSegment { reply } => {
                let _ = reply.send(take_segment_inner(&mut inner));
            }
            RecCmd::Snapshot { reply } => {
                let snapshot = inner
                    .samples
                    .lock()
                    .map(|buf| buf.snapshot())
                    .unwrap_or_default();
                let _ = reply.send(snapshot);
            }
            RecCmd::IsRecording { reply } => {
                let _ = reply.send(inner.stream.is_some());
            }
            RecCmd::Elapsed { reply } => {
                let elapsed = inner
                    .started_at
                    .map(|started| started.elapsed().as_secs_f64())
                    .unwrap_or(0.0);
                let _ = reply.send(elapsed);
            }
        }
    }
}

fn start_inner(inner: &mut Inner, device_name: Option<&str>) -> Result<(), String> {
    if inner.stream.is_some() {
        return Err("recording is already in progress".into());
    }
    let device = resolve_input_device(device_name)?;
    let config = device
        .default_input_config()
        .map_err(|err| format!("microphone config: {err}"))?;
    inner.capture_rate = config.sample_rate().0;
    inner
        .samples
        .lock()
        .expect("samples")
        .reset(inner.capture_rate);
    inner.meter.lock().expect("meter").reset(inner.capture_rate);
    let stream = build_stream(
        &device,
        &config,
        Arc::clone(&inner.samples),
        Arc::clone(&inner.meter),
    )?;
    stream
        .play()
        .map_err(|err| format!("microphone start: {err}"))?;
    inner.stream = Some(stream);
    inner.started_at = Some(Instant::now());
    Ok(())
}

fn stop_inner(inner: &mut Inner) -> Result<std::path::PathBuf, String> {
    let captured = stop_samples_inner(inner)?;
    // The callback already downmixed to mono. Passing the device channel count
    // would average adjacent samples a second time.
    let prepared = wav::resample(&captured.samples, captured.capture_rate, SAMPLE_RATE);
    let path = unique_wav_path();
    wav::write_pcm16_wav(&path, &prepared, SAMPLE_RATE)?;
    Ok(path)
}

fn stop_samples_inner(inner: &mut Inner) -> Result<RawCapture, String> {
    release_stream(inner);
    inner.started_at = None;
    Ok(inner.samples.lock().expect("samples").take())
}

fn take_segment_inner(inner: &mut Inner) -> Result<RawCapture, String> {
    if inner.stream.is_none() {
        return Err("recording is not in progress".into());
    }
    Ok(inner.samples.lock().expect("samples").take())
}

fn release_stream(inner: &mut Inner) {
    if let Some(stream) = inner.stream.take() {
        // Stop HAL I/O before drop so macOS clears the Control Center mic indicator.
        let _ = stream.pause();
        drop(stream);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicKind {
    BuiltIn,
    Other,
    Virtual,
    Bluetooth,
}

impl MicKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::BuiltIn => "builtin",
            Self::Other => "other",
            Self::Virtual => "virtual",
            Self::Bluetooth => "bluetooth",
        }
    }

    fn sort_key(self) -> u8 {
        match self {
            Self::BuiltIn => 0,
            Self::Other => 1,
            Self::Virtual => 2,
            Self::Bluetooth => 3,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MicrophoneInfo {
    pub name: String,
    pub kind: String,
}

pub fn list_microphones() -> Vec<MicrophoneInfo> {
    if let Ok(cache) = MIC_CACHE.lock() {
        if let Some((at, list)) = cache.as_ref() {
            if at.elapsed() < MIC_CACHE_TTL {
                return list.clone();
            }
        }
    }
    let mut list = input_device_names()
        .into_iter()
        .map(|name| {
            let kind = classify_microphone(&name);
            (
                kind,
                MicrophoneInfo {
                    kind: kind.as_str().to_string(),
                    name,
                },
            )
        })
        .collect::<Vec<_>>();
    list.sort_by_key(|(kind, _)| kind.sort_key());
    let list: Vec<MicrophoneInfo> = list.into_iter().map(|(_, info)| info).collect();
    if let Ok(mut cache) = MIC_CACHE.lock() {
        *cache = Some((Instant::now(), list.clone()));
    }
    list
}

pub fn input_device_names() -> Vec<String> {
    let host = cpal::default_host();
    let Ok(devices) = host.input_devices() else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for device in devices {
        if let Ok(name) = device.name() {
            let name = name.trim().to_string();
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

/// Keep an explicit saved mic if it is still plugged in. Otherwise pick a
/// built-in computer mic and never fall back to Bluetooth while a better
/// device exists — macOS "system default" follows the headset.
pub fn preferred_microphone(names: &[String], saved: Option<&str>) -> Option<String> {
    if let Some(saved) = saved.map(str::trim).filter(|name| !name.is_empty()) {
        if names.is_empty() || names.iter().any(|name| name == saved) {
            return Some(saved.to_string());
        }
    }
    pick_preferred(names)
}

pub fn classify_microphone(name: &str) -> MicKind {
    let name = name.to_lowercase();
    if is_virtual_mic(&name) {
        MicKind::Virtual
    } else if is_bluetooth_mic(&name) {
        MicKind::Bluetooth
    } else if is_builtin_mic(&name) {
        MicKind::BuiltIn
    } else {
        MicKind::Other
    }
}

fn pick_preferred(names: &[String]) -> Option<String> {
    let ranked = |want: MicKind| {
        names
            .iter()
            .find(|name| classify_microphone(name) == want)
            .cloned()
    };
    ranked(MicKind::BuiltIn)
        .or_else(|| ranked(MicKind::Other))
        .or_else(|| ranked(MicKind::Virtual))
        .or_else(|| names.first().cloned())
}

fn is_builtin_mic(name: &str) -> bool {
    const MARKERS: &[&str] = &[
        "macbook",
        "imac",
        "mac mini",
        "mac studio",
        "mac pro",
        "built-in",
        "builtin",
        "internal microphone",
        "микрофон mac",
        "встроенн",
    ];
    MARKERS.iter().any(|marker| name.contains(marker))
}

fn is_bluetooth_mic(name: &str) -> bool {
    const MARKERS: &[&str] = &[
        "airpods",
        "beats",
        "headset",
        "hands-free",
        "handsfree",
        "bluetooth",
        "hfp",
        "galaxy buds",
        "pixel buds",
        "freebuds",
        "wh-1000",
        "wf-1000",
    ];
    MARKERS.iter().any(|marker| name.contains(marker)) || looks_like_bt_uid(name)
}

fn looks_like_bt_uid(name: &str) -> bool {
    // CoreAudio sometimes exposes the device UID, e.g. F8-AB-E5-D8-F2-FF:input
    let Some((mac, _)) = name.split_once(':') else {
        return false;
    };
    let parts: Vec<&str> = mac.split(|ch| ch == '-' || ch == ':').collect();
    parts.len() == 6
        && parts
            .iter()
            .all(|part| part.len() == 2 && part.chars().all(|ch| ch.is_ascii_hexdigit()))
}

fn is_virtual_mic(name: &str) -> bool {
    const MARKERS: &[&str] = &[
        "blackhole",
        "loopback",
        "soundflower",
        "aggregate",
        "multi-output",
        "zoomaudio",
        "vb-audio",
        "cable input",
        "virtual",
    ];
    MARKERS.iter().any(|marker| name.contains(marker))
}

fn resolve_input_device(name: Option<&str>) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    let mut listed = Vec::new();
    if let Ok(devices) = host.input_devices() {
        for device in devices {
            if let Ok(device_name) = device.name() {
                let device_name = device_name.trim().to_string();
                if !device_name.is_empty() {
                    listed.push((device_name, device));
                }
            }
        }
    }
    let names: Vec<String> = listed.iter().map(|(name, _)| name.clone()).collect();
    if let Some(chosen) = preferred_microphone(&names, name) {
        if let Some((_, device)) = listed
            .into_iter()
            .find(|(device_name, _)| *device_name == chosen)
        {
            return Ok(device);
        }
        eprintln!("dictator: microphone {chosen:?} vanished while opening");
    }
    host.default_input_device()
        .ok_or_else(|| "no default microphone".into())
}

fn build_stream(
    device: &cpal::Device,
    config: &SupportedStreamConfig,
    sink: Arc<Mutex<CaptureBuf>>,
    meter: Arc<Mutex<LevelMeter>>,
) -> Result<Stream, String> {
    let channels = config.channels();
    let err_fn = |err| eprintln!("dictator: microphone stream error: {err}");
    let stream_config = config.clone().into();
    let stream = match config.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            &stream_config,
            move |data: &[f32], _| append_samples(data, channels, &sink, &meter),
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &stream_config,
            move |data: &[i16], _| {
                let converted: Vec<f32> = data.iter().map(|s| *s as f32 / 32768.0).collect();
                append_samples(&converted, channels, &sink, &meter);
            },
            err_fn,
            None,
        ),
        SampleFormat::I32 => device.build_input_stream(
            &stream_config,
            move |data: &[i32], _| {
                let converted: Vec<f32> =
                    data.iter().map(|s| *s as f32 / 2_147_483_648.0).collect();
                append_samples(&converted, channels, &sink, &meter);
            },
            err_fn,
            None,
        ),
        other => return Err(format!("unsupported microphone format: {other}")),
    };
    stream.map_err(|err| format!("microphone stream: {err}"))
}

fn append_samples(
    input: &[f32],
    channels: u16,
    sink: &Arc<Mutex<CaptureBuf>>,
    meter: &Arc<Mutex<LevelMeter>>,
) {
    if channels <= 1 {
        if let Ok(mut meter) = meter.lock() {
            meter.push(input);
        }
        sink.lock().expect("samples").push_mono(input);
        return;
    }
    let channels = channels as usize;
    let mut mono = Vec::with_capacity(input.len() / channels.max(1));
    for frame in input.chunks(channels) {
        mono.push(frame.iter().sum::<f32>() / frame.len() as f32);
    }
    if let Ok(mut meter) = meter.lock() {
        meter.push(&mono);
    }
    sink.lock().expect("samples").push_mono(&mono);
}

fn unique_wav_path() -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("dictator-{}-{nanos}.wav", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_builtin_and_headset_names() {
        assert_eq!(
            classify_microphone("MacBook Pro Microphone"),
            MicKind::BuiltIn
        );
        assert_eq!(
            classify_microphone("Микрофон MacBook Air"),
            MicKind::BuiltIn
        );
        assert_eq!(classify_microphone("AirPods Pro"), MicKind::Bluetooth);
        assert_eq!(
            classify_microphone("F8-AB-E5-D8-F2-FF:input"),
            MicKind::Bluetooth
        );
        assert_eq!(classify_microphone("BlackHole 2ch"), MicKind::Virtual);
        assert_eq!(classify_microphone("USB Microphone"), MicKind::Other);
    }

    #[test]
    fn prefers_builtin_over_system_default_headset() {
        let names = vec![
            "AirPods Pro".into(),
            "BlackHole 2ch".into(),
            "MacBook Pro Microphone".into(),
        ];
        assert_eq!(
            preferred_microphone(&names, None).as_deref(),
            Some("MacBook Pro Microphone")
        );
        assert_eq!(
            preferred_microphone(&names, Some("AirPods Pro")).as_deref(),
            Some("AirPods Pro")
        );
        assert_eq!(
            preferred_microphone(&names, Some("Missing Mic")).as_deref(),
            Some("MacBook Pro Microphone")
        );
    }

    #[test]
    fn keeps_saved_mic_when_device_list_is_empty() {
        assert_eq!(
            preferred_microphone(&[], Some("MacBook Pro Microphone")).as_deref(),
            Some("MacBook Pro Microphone")
        );
    }

    #[test]
    fn visual_bar_stays_in_unit_range() {
        assert!(visual_bar(0.0, 0.0) >= 0.16);
        assert!(visual_bar(1.0, 1.0) <= 1.0);
        assert!(visual_bar(0.3, 0.15) > visual_bar(0.05, 0.02));
    }

    #[test]
    fn quiet_laptop_speech_is_not_a_flat_line() {
        let silence = visual_bar(0.0, 0.0);
        let quiet = visual_bar(0.04, 0.015);
        let loud = visual_bar(0.25, 0.1);
        assert!(quiet > silence + 0.15);
        assert!(loud > quiet + 0.12);
        assert!(loud <= 1.0);
    }

    #[test]
    fn meter_fills_a_full_window_of_bars() {
        let mut meter = LevelMeter::new();
        meter.reset(16_000);
        let hop = meter.hop;
        meter.push(&vec![0.4; hop * HUD_BARS]);
        let bars = meter.bars();
        assert_eq!(bars.len(), HUD_BARS);
        assert!(bars.iter().all(|value| *value > 0.2));
    }

    #[test]
    fn quiet_meter_still_reaches_visible_heights() {
        let mut meter = LevelMeter::new();
        meter.reset(16_000);
        let hop = meter.hop;
        meter.push(&vec![0.035; hop * HUD_BARS * 2]);
        let bars = meter.bars();
        assert_eq!(bars.len(), HUD_BARS);
        let mean = bars.iter().sum::<f32>() / bars.len() as f32;
        assert!(mean > 0.4, "mean {mean} should look like a speaking wave");
    }
}
