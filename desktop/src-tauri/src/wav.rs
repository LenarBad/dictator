//! Host-owned capture format: WAV 16 kHz mono PCM16.

use std::path::{Path, PathBuf};

pub const SAMPLE_RATE: u32 = 16_000;
pub const CHANNELS: u16 = 1;
pub const MIN_UTTERANCE_SECONDS: f64 = 0.35;

pub fn to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    let channels = channels.max(1) as usize;
    if channels == 1 {
        return samples.to_vec();
    }
    samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect()
}

pub fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if samples.is_empty() || from == 0 || to == 0 {
        return Vec::new();
    }
    if from == to {
        return samples.to_vec();
    }
    let ratio = f64::from(from) / f64::from(to);
    let out_len = ((samples.len() as f64) / ratio).round().max(0.0) as usize;
    if out_len == 0 {
        return Vec::new();
    }
    let last = samples.len() - 1;
    (0..out_len)
        .map(|index| {
            let src = index as f64 * ratio;
            let left = (src.floor() as usize).min(last);
            let right = (left + 1).min(last);
            let frac = (src - left as f64) as f32;
            samples[left] * (1.0 - frac) + samples[right] * frac
        })
        .collect()
}

pub fn duration_seconds(samples: &[f32], sample_rate: u32) -> f64 {
    if sample_rate == 0 {
        return 0.0;
    }
    samples.len() as f64 / f64::from(sample_rate)
}

pub fn write_pcm16_wav(path: &Path, samples: &[f32], sample_rate: u32) -> Result<PathBuf, String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let mut writer = hound::WavWriter::create(path, spec).map_err(|err| err.to_string())?;
    for sample in samples {
        let clipped = sample.clamp(-1.0, 1.0);
        let pcm = (clipped * f32::from(i16::MAX)) as i16;
        writer.write_sample(pcm).map_err(|err| err.to_string())?;
    }
    writer.finalize().map_err(|err| err.to_string())?;
    Ok(path.to_path_buf())
}

pub fn prepare_host_wav(samples: &[f32], channels: u16, capture_rate: u32) -> Vec<f32> {
    let mono = to_mono(samples, channels);
    resample(&mono, capture_rate, SAMPLE_RATE)
}

pub fn read_pcm16_wav(path: &Path) -> Result<(Vec<f32>, u32), String> {
    let mut reader = hound::WavReader::open(path).map_err(|err| err.to_string())?;
    let spec = reader.spec();
    let samples: Result<Vec<f32>, _> = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .map(|sample| sample.map(|value| f32::from(value) / f32::from(i16::MAX)))
            .collect(),
        hound::SampleFormat::Float => reader.samples::<f32>().collect(),
    };
    let samples = samples.map_err(|err| err.to_string())?;
    let mono = to_mono(&samples, spec.channels);
    let prepared = resample(&mono, spec.sample_rate, SAMPLE_RATE);
    Ok((prepared, SAMPLE_RATE))
}

/// GigaAM short-form RNNT is trained on clips up to ~25 s. Split longer audio.
pub fn split_for_asr(samples: &[f32], sample_rate: u32) -> Vec<Vec<f32>> {
    if samples.is_empty() || sample_rate == 0 {
        return Vec::new();
    }
    let max_short = (24.0 * f64::from(sample_rate)) as usize;
    if samples.len() <= max_short {
        return vec![samples.to_vec()];
    }
    let chunk = (20.0 * f64::from(sample_rate)) as usize;
    let overlap = (0.4 * f64::from(sample_rate)) as usize;
    let hop = chunk.saturating_sub(overlap).max(1);
    let min_chunk = (0.2 * f64::from(sample_rate)) as usize;
    let mut out = Vec::new();
    let mut start = 0;
    while start < samples.len() {
        let end = (start + chunk).min(samples.len());
        if end.saturating_sub(start) < min_chunk && !out.is_empty() {
            break;
        }
        out.push(samples[start..end].to_vec());
        if end >= samples.len() {
            break;
        }
        start += hop;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn mono_is_unchanged() {
        let samples = vec![0.1, -0.2, 0.3];
        assert_eq!(to_mono(&samples, 1), samples);
    }

    #[test]
    fn stereo_averages_pairs() {
        let samples = vec![0.0, 1.0, 0.5, 0.5];
        let mono = to_mono(&samples, 2);
        assert_eq!(mono.len(), 2);
        assert!((mono[0] - 0.5).abs() < f32::EPSILON);
        assert!((mono[1] - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn resample_identity() {
        let samples = vec![0.1, 0.2, 0.3];
        assert_eq!(resample(&samples, 16_000, 16_000), samples);
    }

    #[test]
    fn resample_down_3x() {
        let samples: Vec<f32> = (0..48).map(|i| i as f32 / 47.0).collect();
        let out = resample(&samples, 48_000, 16_000);
        assert_eq!(out.len(), 16);
    }

    #[test]
    fn writes_pcm16_wav() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("dictator-wav-test-{nanos}.wav"));
        let samples = vec![0.0_f32; 160];
        write_pcm16_wav(&path, &samples, SAMPLE_RATE).expect("write");
        let reader = hound::WavReader::open(&path).expect("read");
        let spec = reader.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, SAMPLE_RATE);
        assert_eq!(spec.bits_per_sample, 16);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn short_audio_is_one_chunk() {
        let samples = vec![0.1_f32; 16_000];
        let chunks = split_for_asr(&samples, SAMPLE_RATE);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), samples.len());
    }

    #[test]
    fn long_audio_is_split_with_overlap() {
        let samples = vec![0.1_f32; SAMPLE_RATE as usize * 50];
        let chunks = split_for_asr(&samples, SAMPLE_RATE);
        assert!(chunks.len() >= 3, "{}", chunks.len());
        assert!(chunks
            .iter()
            .all(|chunk| chunk.len() <= SAMPLE_RATE as usize * 20 + 1));
    }
}
