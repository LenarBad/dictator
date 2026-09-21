//! In-process GigaAM-v3 e2e RNNT via sherpa-onnx. Does not open the microphone.

use std::path::{Path, PathBuf};

use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig};

use crate::wav::{self, SAMPLE_RATE};

const STUB_ENV: &str = "DICTATOR_STT_STUB";
const FEATURE_DIM: i32 = 64;
const NUM_THREADS: i32 = 4;

pub struct Engine {
    stub: bool,
    model_dir: PathBuf,
    recognizer: Option<OfflineRecognizer>,
}

impl Engine {
    pub fn new() -> Result<Self, String> {
        if stub_enabled() {
            return Ok(Self {
                stub: true,
                model_dir: PathBuf::new(),
                recognizer: None,
            });
        }
        Ok(Self {
            stub: false,
            model_dir: resolve_model_dir()?,
            recognizer: None,
        })
    }

    pub fn preload(&mut self) -> Result<(), String> {
        self.ensure_recognizer().map(|_| ())
    }

    pub fn transcribe(&mut self, audio: &Path) -> Result<String, String> {
        if !audio.exists() {
            return Err(format!("audio file not found: {}", audio.display()));
        }
        if self.stub {
            let (samples, rate) = wav::read_pcm16_wav(audio)?;
            let duration = wav::duration_seconds(&samples, rate);
            let name = audio
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("audio.wav");
            return Ok(format!("[stub] {name} ({duration:.1}s)"));
        }
        let (samples, rate) = wav::read_pcm16_wav(audio)?;
        if samples.is_empty() {
            return Ok(String::new());
        }
        let recognizer = self.ensure_recognizer()?;
        let mut parts = Vec::new();
        for chunk in wav::split_for_asr(&samples, rate) {
            let text = decode_chunk(recognizer, &chunk, rate)?;
            if !text.is_empty() {
                parts.push(text);
            }
        }
        Ok(parts.join(" ").trim().to_string())
    }

    fn ensure_recognizer(&mut self) -> Result<&OfflineRecognizer, String> {
        if self.stub {
            return Err("stub engine has no recognizer".into());
        }
        if self.recognizer.is_none() {
            self.recognizer = Some(load_recognizer(&self.model_dir)?);
        }
        Ok(self.recognizer.as_ref().expect("recognizer"))
    }
}

fn stub_enabled() -> bool {
    matches!(
        std::env::var(STUB_ENV)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes"
    )
}

fn load_recognizer(model_dir: &Path) -> Result<OfflineRecognizer, String> {
    let encoder = model_dir.join("encoder.int8.onnx");
    let decoder = model_dir.join("decoder.onnx");
    let joiner = model_dir.join("joiner.onnx");
    let tokens = model_dir.join("tokens.txt");
    for path in [&encoder, &decoder, &joiner, &tokens] {
        if !path.is_file() {
            return Err(format!("STT model file missing: {}", path.display()));
        }
    }

    let mut config = OfflineRecognizerConfig::default();
    config.feat_config.sample_rate = SAMPLE_RATE as i32;
    config.feat_config.feature_dim = FEATURE_DIM;
    config.model_config.transducer = OfflineTransducerModelConfig {
        encoder: Some(encoder.to_string_lossy().into_owned()),
        decoder: Some(decoder.to_string_lossy().into_owned()),
        joiner: Some(joiner.to_string_lossy().into_owned()),
    };
    config.model_config.tokens = Some(tokens.to_string_lossy().into_owned());
    config.model_config.model_type = Some("nemo_transducer".into());
    config.model_config.num_threads = NUM_THREADS;
    config.model_config.provider = Some("cpu".into());
    config.decoding_method = Some("greedy_search".into());

    OfflineRecognizer::create(&config).ok_or_else(|| {
        format!(
            "failed to create sherpa-onnx recognizer from {}",
            model_dir.display()
        )
    })
}

fn decode_chunk(
    recognizer: &OfflineRecognizer,
    samples: &[f32],
    sample_rate: u32,
) -> Result<String, String> {
    let stream = recognizer.create_stream();
    stream.accept_waveform(sample_rate as i32, samples);
    recognizer.decode(&stream);
    let result = stream
        .get_result()
        .ok_or_else(|| "sherpa-onnx returned no result".to_string())?;
    Ok(result.text.trim().to_string())
}

fn resolve_model_dir() -> Result<PathBuf, String> {
    for dir in candidate_model_dirs() {
        if is_model_dir(&dir) {
            return Ok(dir);
        }
    }
    Err(
        "GigaAM ONNX model not found. Run: bash scripts/fetch-stt-model.sh (or set DICTATOR_MODEL_DIR)."
            .into(),
    )
}

fn candidate_model_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(raw) = std::env::var("DICTATOR_MODEL_DIR") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            dirs.push(PathBuf::from(trimmed));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(macos) = exe.parent() {
            if macos.file_name().and_then(|name| name.to_str()) == Some("MacOS") {
                if let Some(contents) = macos.parent() {
                    let resources = contents.join("Resources");
                    dirs.push(resources.join("gigaam"));
                    dirs.push(resources.join("resources").join("gigaam"));
                }
            }
            dirs.push(macos.join("gigaam"));
            dirs.push(macos.join("resources").join("gigaam"));
            dirs.push(macos.join("resources").join("resources").join("gigaam"));
        }
    }
    dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/gigaam"));
    dirs
}

fn is_model_dir(dir: &Path) -> bool {
    dir.join("encoder.int8.onnx").is_file()
        && dir.join("decoder.onnx").is_file()
        && dir.join("joiner.onnx").is_file()
        && dir.join("tokens.txt").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_transcribes_without_model() {
        let mut engine = Engine {
            stub: true,
            model_dir: PathBuf::new(),
            recognizer: None,
        };
        let path = std::env::temp_dir().join("dictator-stt-stub.wav");
        crate::wav::write_pcm16_wav(&path, &[0.0; 16_000], SAMPLE_RATE).expect("wav");
        let text = engine.transcribe(&path).expect("transcribe");
        let _ = std::fs::remove_file(&path);
        assert!(
            text.contains("[stub]") || text.contains("dictator-stt-stub.wav"),
            "{text}"
        );
    }

    #[test]
    fn transcribes_official_example_when_model_present() {
        let wav =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gigaam-example.wav");
        if !wav.is_file() || resolve_model_dir().is_err() {
            return;
        }
        let mut engine = Engine::new().expect("engine");
        let text = engine.transcribe(&wav).expect("transcribe");
        let lower = text.to_lowercase();
        assert!(
            lower.contains("похвал") && lower.contains("лукоморья"),
            "{text}"
        );
    }
}
