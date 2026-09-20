use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, SupportedStreamConfig};
use serde::Serialize;

const MIC_CACHE_TTL: Duration = Duration::from_secs(30);
static MIC_CACHE: Mutex<Option<(Instant, Vec<MicrophoneInfo>)>> = Mutex::new(None);

use crate::wav::{self, SAMPLE_RATE};

enum RecCmd {
    Start {
        device: Option<String>,
        reply: Sender<Result<(), String>>,
    },
    Stop {
        reply: Sender<Result<std::path::PathBuf, String>>,
    },
    IsRecording {
        reply: Sender<bool>,
    },
    Elapsed {
        reply: Sender<f64>,
    },
}

pub struct Recorder {
    tx: Sender<RecCmd>,
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Recorder {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("dictator-mic".into())
            .spawn(move || recorder_loop(rx))
            .expect("microphone thread");
        Self { tx }
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

    pub fn start(&self, device_name: Option<&str>) -> Result<(), String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(RecCmd::Start {
                device: device_name.map(str::to_string),
                reply,
            })
            .map_err(|_| "microphone thread exited".to_string())?;
        rx.recv().map_err(|_| "microphone thread exited".to_string())?
    }

    pub fn stop(&self) -> Result<std::path::PathBuf, String> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(RecCmd::Stop { reply })
            .map_err(|_| "microphone thread exited".to_string())?;
        rx.recv().map_err(|_| "microphone thread exited".to_string())?
    }
}

struct Inner {
    stream: Option<Stream>,
    samples: Arc<Mutex<Vec<f32>>>,
    capture_rate: u32,
    channels: u16,
    started_at: Option<Instant>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        release_stream(self);
    }
}

fn recorder_loop(rx: mpsc::Receiver<RecCmd>) {
    let mut inner = Inner {
        stream: None,
        samples: Arc::new(Mutex::new(Vec::new())),
        capture_rate: SAMPLE_RATE,
        channels: 1,
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
    inner.channels = config.channels();
    inner.samples.lock().expect("samples").clear();
    let stream = build_stream(&device, &config, Arc::clone(&inner.samples))?;
    stream.play().map_err(|err| format!("microphone start: {err}"))?;
    inner.stream = Some(stream);
    inner.started_at = Some(Instant::now());
    Ok(())
}

fn stop_inner(inner: &mut Inner) -> Result<std::path::PathBuf, String> {
    release_stream(inner);
    inner.started_at = None;
    let captured = {
        let mut samples = inner.samples.lock().expect("samples");
        std::mem::take(&mut *samples)
    };
    let prepared = wav::prepare_host_wav(&captured, inner.channels, inner.capture_rate);
    let path = unique_wav_path();
    wav::write_pcm16_wav(&path, &prepared, SAMPLE_RATE)?;
    Ok(path)
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
            (kind, MicrophoneInfo {
                kind: kind.as_str().to_string(),
                name,
            })
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
    sink: Arc<Mutex<Vec<f32>>>,
) -> Result<Stream, String> {
    let channels = config.channels();
    let err_fn = |err| eprintln!("dictator: microphone stream error: {err}");
    let stream_config = config.clone().into();
    let stream = match config.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            &stream_config,
            move |data: &[f32], _| append_samples(data, channels, &sink),
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &stream_config,
            move |data: &[i16], _| {
                let converted: Vec<f32> = data.iter().map(|s| *s as f32 / 32768.0).collect();
                append_samples(&converted, channels, &sink);
            },
            err_fn,
            None,
        ),
        SampleFormat::I32 => device.build_input_stream(
            &stream_config,
            move |data: &[i32], _| {
                let converted: Vec<f32> = data.iter().map(|s| *s as f32 / 2_147_483_648.0).collect();
                append_samples(&converted, channels, &sink);
            },
            err_fn,
            None,
        ),
        other => return Err(format!("unsupported microphone format: {other}")),
    };
    stream.map_err(|err| format!("microphone stream: {err}"))
}

fn append_samples(input: &[f32], channels: u16, sink: &Arc<Mutex<Vec<f32>>>) {
    let mut samples = sink.lock().expect("samples");
    if channels <= 1 {
        samples.extend_from_slice(input);
        return;
    }
    let channels = channels as usize;
    for frame in input.chunks(channels) {
        samples.push(frame.iter().sum::<f32>() / frame.len() as f32);
    }
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
}
