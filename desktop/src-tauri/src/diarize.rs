//! Offline speaker diarization via sherpa-onnx (pyannote segmentation + TitaNet).
//! GigaAM stays in `stt`. This module only returns time ranges and speaker ids.

use std::path::{Path, PathBuf};

use sherpa_onnx::{
    FastClusteringConfig, OfflineSpeakerDiarization, OfflineSpeakerDiarizationConfig,
    OfflineSpeakerSegmentationModelConfig, OfflineSpeakerSegmentationPyannoteModelConfig,
    SpeakerEmbeddingExtractor, SpeakerEmbeddingExtractorConfig,
};

use crate::speakers::Segment;
use crate::wav;

const NUM_THREADS: i32 = 2;
/// A voice with less than this much speech can be the same person as a longer turn
/// whose embedding sherpa put in another cluster (a short «И нам это надо?» before
/// the same guest speaks at length).
const SHORT_SPEAKER_SECONDS: f64 = 2.0;
const SHORT_SPEAKER_MAX_DISTANCE: f64 = 0.65;
const EMBED_MAX_SECONDS: f64 = 8.0;

pub struct Diarizer {
    stub: bool,
    model_dir: PathBuf,
    inner: Option<OfflineSpeakerDiarization>,
    embedder: Option<SpeakerEmbeddingExtractor>,
}

impl Diarizer {
    pub fn new() -> Result<Self, String> {
        if crate::stt::stub_enabled() {
            return Ok(Self::stub());
        }
        Ok(Self {
            stub: false,
            model_dir: resolve_model_dir()?,
            inner: None,
            embedder: None,
        })
    }

    pub(crate) fn stub() -> Self {
        Self {
            stub: true,
            model_dir: PathBuf::new(),
            inner: None,
            embedder: None,
        }
    }

    pub fn preload(&mut self) -> Result<(), String> {
        if self.stub {
            return Ok(());
        }
        self.ensure().map(|_| ())
    }

    /// Speaker ids are sherpa's raw indexes (0-based). Callers renumber for display.
    pub fn process(&mut self, samples: &[f32], sample_rate: u32) -> Result<Vec<Segment>, String> {
        if samples.is_empty() || sample_rate == 0 {
            return Ok(Vec::new());
        }
        if self.stub {
            return Ok(vec![Segment {
                start: 0.0,
                end: wav::duration_seconds(samples, sample_rate),
                speaker: 0,
            }]);
        }

        let diarizer = self.ensure()?;
        let expected = diarizer.sample_rate();
        let audio = if expected > 0 && expected as u32 != sample_rate {
            wav::resample(samples, sample_rate, expected as u32)
        } else {
            samples.to_vec()
        };
        let result = diarizer
            .process(&audio)
            .ok_or_else(|| "Разделение говорящих не вернуло результат".to_string())?;
        let mut segments: Vec<Segment> = result
            .sort_by_start_time()
            .into_iter()
            .map(|segment| Segment {
                start: f64::from(segment.start),
                end: f64::from(segment.end),
                speaker: segment.speaker,
            })
            .filter(|segment| segment.end > segment.start)
            .collect();
        segments.sort_by(|left, right| {
            left.start
                .partial_cmp(&right.start)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(segments)
    }

    /// Give a short fragment the speaker id of the longer turn it actually matches.
    pub fn align_voices(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
        segments: &[Segment],
    ) -> Vec<Segment> {
        let absorbed = crate::speakers::absorb_overlaps(segments);
        if self.stub || samples.is_empty() || sample_rate == 0 {
            return absorbed;
        }
        let Ok(embedder) = self.ensure_embedder() else {
            return absorbed;
        };
        let prints = speaker_prints(embedder, samples, sample_rate, &absorbed);
        crate::speakers::link_short_speakers(
            &absorbed,
            &prints,
            SHORT_SPEAKER_SECONDS,
            SHORT_SPEAKER_MAX_DISTANCE,
        )
    }

    fn ensure(&mut self) -> Result<&OfflineSpeakerDiarization, String> {
        if self.stub {
            return Err("stub diarizer has no model".into());
        }
        if self.inner.is_none() {
            self.inner = Some(load_diarizer(&self.model_dir)?);
        }
        Ok(self.inner.as_ref().expect("diarizer"))
    }

    fn ensure_embedder(&mut self) -> Result<&SpeakerEmbeddingExtractor, String> {
        if self.embedder.is_none() {
            let path = self.model_dir.join("embedding.onnx");
            if !path.is_file() {
                return Err(missing_models());
            }
            let config = SpeakerEmbeddingExtractorConfig {
                model: Some(path.to_string_lossy().into_owned()),
                num_threads: NUM_THREADS,
                debug: false,
                provider: Some("cpu".into()),
            };
            self.embedder = SpeakerEmbeddingExtractor::create(&config);
        }
        self.embedder.as_ref().ok_or_else(|| {
            format!(
                "Не удалось загрузить отпечаток голоса из {}",
                self.model_dir.display()
            )
        })
    }
}

fn speaker_prints(
    embedder: &SpeakerEmbeddingExtractor,
    samples: &[f32],
    sample_rate: u32,
    segments: &[Segment],
) -> Vec<(i32, Vec<f32>)> {
    let mut speakers: Vec<i32> = segments.iter().map(|segment| segment.speaker).collect();
    speakers.sort_unstable();
    speakers.dedup();
    speakers
        .into_iter()
        .filter_map(|speaker| {
            let segment = segments
                .iter()
                .filter(|segment| segment.speaker == speaker)
                .max_by(|left, right| {
                    (left.end - left.start)
                        .partial_cmp(&(right.end - right.start))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })?;
            let audio = embed_window(samples, sample_rate, segment);
            let embedding = embed(embedder, &audio, sample_rate)?;
            Some((speaker, embedding))
        })
        .collect()
}

fn embed_window(samples: &[f32], sample_rate: u32, segment: &Segment) -> Vec<f32> {
    let slice = wav::slice_seconds(samples, sample_rate, segment.start, segment.end);
    let max_len = (EMBED_MAX_SECONDS * f64::from(sample_rate)) as usize;
    if slice.len() <= max_len || max_len == 0 {
        return slice;
    }
    let start = (slice.len() - max_len) / 2;
    slice[start..start + max_len].to_vec()
}

fn embed(
    embedder: &SpeakerEmbeddingExtractor,
    samples: &[f32],
    sample_rate: u32,
) -> Option<Vec<f32>> {
    if samples.is_empty() {
        return None;
    }
    let stream = embedder.create_stream()?;
    stream.accept_waveform(sample_rate as i32, samples);
    stream.input_finished();
    if !embedder.is_ready(&stream) {
        return None;
    }
    embedder.compute(&stream)
}

fn load_diarizer(model_dir: &Path) -> Result<OfflineSpeakerDiarization, String> {
    let segmentation = model_dir.join("segmentation.int8.onnx");
    let embedding = model_dir.join("embedding.onnx");
    for path in [&segmentation, &embedding] {
        if !path.is_file() {
            return Err(missing_models());
        }
    }

    let config = OfflineSpeakerDiarizationConfig {
        segmentation: OfflineSpeakerSegmentationModelConfig {
            pyannote: OfflineSpeakerSegmentationPyannoteModelConfig {
                model: Some(segmentation.to_string_lossy().into_owned()),
                window_shift_ratio: 0.1,
            },
            num_threads: NUM_THREADS,
            debug: false,
            provider: Some("cpu".into()),
        },
        embedding: SpeakerEmbeddingExtractorConfig {
            model: Some(embedding.to_string_lossy().into_owned()),
            num_threads: NUM_THREADS,
            debug: false,
            provider: Some("cpu".into()),
        },
        clustering: FastClusteringConfig {
            num_clusters: -1,
            // Sherpa default. 0.4 split one voice across pauses into several speakers.
            threshold: 0.5,
            compute_confidence: false,
        },
        // Keep a short reply, but bridge only a breath — not the next person.
        // Sherpa's 0.5 off-gap glued a fast dialogue into one speaker.
        min_duration_on: 0.2,
        min_duration_off: 0.15,
    };

    OfflineSpeakerDiarization::create(&config).ok_or_else(|| {
        format!(
            "Не удалось загрузить разделение говорящих из {}",
            model_dir.display()
        )
    })
}

fn missing_models() -> String {
    "Не найдены модели разделения говорящих. Запустите: bash scripts/fetch-stt-model.sh".into()
}

fn resolve_model_dir() -> Result<PathBuf, String> {
    for dir in candidate_model_dirs() {
        if is_model_dir(&dir) {
            return Ok(dir);
        }
    }
    Err(missing_models())
}

fn candidate_model_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(raw) = std::env::var("DICTATOR_MODEL_DIR") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            let dir = PathBuf::from(trimmed);
            if let Some(parent) = dir.parent() {
                dirs.push(parent.join("diarize"));
            }
            dirs.push(dir.join("diarize"));
            dirs.push(dir);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(macos) = exe.parent() {
            if macos.file_name().and_then(|name| name.to_str()) == Some("MacOS") {
                if let Some(contents) = macos.parent() {
                    let resources = contents.join("Resources");
                    dirs.push(resources.join("diarize"));
                    dirs.push(resources.join("resources").join("diarize"));
                }
            }
            dirs.push(macos.join("diarize"));
            dirs.push(macos.join("resources").join("diarize"));
            dirs.push(macos.join("resources").join("resources").join("diarize"));
        }
    }
    dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/diarize"));
    dirs
}

fn is_model_dir(dir: &Path) -> bool {
    dir.join("segmentation.int8.onnx").is_file() && dir.join("embedding.onnx").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_covers_the_whole_clip() {
        let mut diarizer = Diarizer::stub();
        let samples = vec![0.0_f32; 8_000];
        let segments = diarizer.process(&samples, 16_000).expect("process");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].speaker, 0);
        assert!(segments[0].start.abs() < 1e-6);
        assert!((segments[0].end - 0.5).abs() < 1e-3);
    }

    #[test]
    fn processes_fixture_when_model_present() {
        if resolve_model_dir().is_err() || crate::stt::stub_enabled() {
            return;
        }
        let wav =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gigaam-example.wav");
        if !wav.is_file() {
            return;
        }
        let (samples, rate) = wav::read_pcm16_wav(&wav).expect("wav");
        let mut diarizer = Diarizer::new().expect("diarizer");
        let segments = diarizer.process(&samples, rate).expect("process");
        assert!(!segments.is_empty(), "diarization API returned no segments");
    }
}
