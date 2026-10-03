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
