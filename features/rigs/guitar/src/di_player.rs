//! The DI player's recordings: a guitar played straight into an interface,
//! looped through the chain in the instrument's place (`Rig::play_di`) so
//! tones can be dialled in with no guitar to hand.
//!
//! They ship with the app, in the order of `signal_guitar_proto::DI_CLIPS`:
//! the player's own guitar, recorded dry — chords, palm-muted picking, a
//! riff — 48 kHz mono, at the level they were played (what the captures
//! were trained to hear).

use std::sync::{Arc, Mutex, OnceLock};

/// The recordings, 16-bit mono at [`RECORDED_RATE`], in `DI_CLIPS` order.
const RECORDINGS: &[&[u8]] = &[
    include_bytes!("../default-config/di/chords.wav"),
    include_bytes!("../default-config/di/palm-mute-picking.wav"),
    include_bytes!("../default-config/di/riff.wav"),
];
const RECORDED_RATE: u32 = 48_000;

/// Recording `clip` (clamped to the ones there are) at `rate`, decoded once
/// per clip and rate.
#[must_use]
pub fn recording(clip: u32, rate: u32) -> Arc<Vec<f32>> {
    type Cache = Mutex<Vec<((usize, u32), Arc<Vec<f32>>)>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let clip = (clip as usize).min(RECORDINGS.len() - 1);
    let cache = CACHE.get_or_init(|| Mutex::new(Vec::new()));
    let mut cache = cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, samples)) = cache.iter().find(|(key, _)| *key == (clip, rate)) {
        return samples.clone();
    }
    let samples = Arc::new(resample(&decode(RECORDINGS[clip]), RECORDED_RATE, rate));
    cache.push(((clip, rate), samples.clone()));
    samples
}

/// A recording in the library (`Rig::record_loop`) at `rate`, read once
/// per file and rate; silence when it will not read.
#[must_use]
pub fn recorded(path: &std::path::Path, rate: u32) -> Arc<Vec<f32>> {
    type Cache = Mutex<Vec<((std::path::PathBuf, u32), Arc<Vec<f32>>)>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(Vec::new()));
    let mut cache = cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, samples)) = cache.iter().find(|(key, _)| key.0 == path && key.1 == rate) {
        return samples.clone();
    }
    let (samples, from) = match hound::WavReader::open(path) {
        Ok(reader) => {
            let from = reader.spec().sample_rate;
            (reader.into_samples::<i16>().map(|s| f32::from(s.unwrap_or(0)) / 32_768.0).collect::<Vec<_>>(), from)
        }
        Err(e) => {
            tracing::warn!(error = %e, "di player: a recording does not read");
            (Vec::new(), rate)
        }
    };
    let samples = Arc::new(resample(&samples, from, rate));
    cache.push(((path.to_path_buf(), rate), samples.clone()));
    samples
}

/// Write `samples` (mono, at `rate`) as a 16-bit recording at `path`.
///
/// # Errors
/// When the file cannot be written.
pub fn write(path: &std::path::Path, samples: &[f32], rate: u32) -> Result<(), hound::Error> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let spec = hound::WavSpec { channels: 1, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec)?;
    for s in samples {
        w.write_sample((s.clamp(-1.0, 1.0) * 32_767.0) as i16)?;
    }
    w.finalize()
}

/// A shipped recording as `f32`, or silence if it will not decode (it is
/// built in, so that is a build that shipped a broken file).
fn decode(bytes: &[u8]) -> Vec<f32> {
    match hound::WavReader::new(std::io::Cursor::new(bytes)) {
        Ok(reader) => reader
            .into_samples::<i16>()
            .map(|s| f32::from(s.unwrap_or(0)) / 32_768.0)
            .collect(),
        Err(e) => {
            tracing::warn!(error = %e, "di player: a shipped recording does not decode");
            Vec::new()
        }
    }
}

/// Linear resampling — a guitar DI, not a master: good enough to hear a
/// tone by, at a device that will not run at 48 kHz.
fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || to == 0 || samples.len() < 2 {
        return samples.to_vec();
    }
    let step = f64::from(from) / f64::from(to);
    let frames = (samples.len() as f64 / step) as usize;
    (0..frames)
        .map(|i| {
            let at = i as f64 * step;
            let j = at as usize;
            let frac = (at - j as f64) as f32;
            let a = samples[j.min(samples.len() - 1)];
            let b = samples[(j + 1).min(samples.len() - 1)];
            a + (b - a) * frac
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_clip_decodes_to_a_recording() {
        assert_eq!(RECORDINGS.len(), signal_guitar_proto::DI_CLIPS.len());
        for clip in 0..RECORDINGS.len() as u32 {
            let r = recording(clip, RECORDED_RATE);
            assert!(r.len() > 3 * RECORDED_RATE as usize, "clip {clip} is seconds long");
            assert!(r.iter().any(|s| s.abs() > 0.05), "clip {clip} is a recording, not silence");
        }
    }

    #[test]
    fn resampling_keeps_the_length_in_time() {
        let at48 = recording(0, 48_000).len() as f64 / 48_000.0;
        let at44 = recording(0, 44_100).len() as f64 / 44_100.0;
        assert!((at48 - at44).abs() < 0.001);
    }
}

#[cfg(test)]
mod recorded_tests {
    use super::*;

    #[test]
    fn a_recorded_loop_reads_back_as_it_was_played() {
        let path = std::env::temp_dir().join(format!("di-roundtrip-{}.wav", std::process::id()));
        let played: Vec<f32> = (0..4_800).map(|i| (i as f32 / 48.0).sin() * 0.5).collect();
        write(&path, &played, 48_000).expect("it writes");
        let back = recorded(&path, 48_000);
        assert_eq!(back.len(), played.len());
        assert!(played.iter().zip(back.iter()).all(|(a, b)| (a - b).abs() < 1e-3), "16-bit, to within a step");
        // At another rate, resampled to it.
        assert_eq!(recorded(&path, 24_000).len(), played.len() / 2);
        let _ = std::fs::remove_file(&path);
    }
}
