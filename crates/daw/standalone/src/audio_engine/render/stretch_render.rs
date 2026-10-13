//! A stretched take played at its own pitch.
//!
//! Stretch markers say where in the source each instant of a take plays.
//! Read straight through that map — what the mixer does with any take —
//! a stretch is varispeed: a drum slice's tail drawn out to a fifth of
//! its speed slides down in pitch and snaps back at the next hit, heard
//! as a record scratching on every double kick an edit pulled apart, and
//! the overheads' high end gone wherever a tail was slowed.
//!
//! A stretched take is rendered instead, once: between each pair of its
//! markers, a stretch played at speed is copied from the source as it is
//! and one played slower or faster is time-stretched with WSOLA
//! ([`pitch::wsola::stretch_planar`]) — its pitch kept, its timing
//! the map's — each joined to the next across a short crossfade. The
//! mixer then plays the render as an unstretched take. Renders are kept
//! by take, offset and markers, so a replan after an edit renders only
//! what the edit changed.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use daw_proto::StretchMarker;

use crate::audio_engine::decoder::DecodedAudio;
use crate::audio_engine::source::AudioSource;

/// The joint between two stretches of a render, seconds: a crossfade, so
/// the seam between a copy and a stretch does not click.
const JOINT: f64 = 0.002;

/// A stretch slower or faster than this is no stretch: copied as it is.
const AT_SPEED: f64 = 1e-4;

/// The WSOLA sequence at 48 kHz, frames: the shifter's own.
const GRAIN_48K: f64 = 1024.0;

/// The most renders kept before the cache starts over.
const KEPT: usize = 8192;

/// A render's key: its source (by identity), and its markers and length
/// as bits.
type Key = (usize, u64, Vec<(u64, u64)>);

fn cache() -> &'static Mutex<HashMap<Key, Arc<AudioSource>>> {
    static CACHE: OnceLock<Mutex<HashMap<Key, Arc<AudioSource>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `audio` as the take with `markers` plays it over `length` seconds of
/// its playback time, rendered at its own pitch; `None` when there is
/// nothing to render — fewer than two markers, every stretch at speed,
/// or a source read a window at a time (the browser's), which the mixer
/// plays through the map as before.
pub(crate) fn rendered(
    audio: &Arc<AudioSource>,
    markers: &[StretchMarker],
    length: f64,
) -> Option<Arc<AudioSource>> {
    if markers.len() < 2 || length <= 0.0 || matches!(**audio, AudioSource::Streamed(_)) {
        return None;
    }
    let stretched = markers.windows(2).any(|pair| {
        let (a, b) = (&pair[0], &pair[1]);
        let played = b.position - a.position;
        played > 0.0 && ((b.source_position - a.source_position) / played - 1.0).abs() > AT_SPEED
    });
    if !stretched {
        return None;
    }
    let key: Key = (
        Arc::as_ptr(audio) as usize,
        length.to_bits(),
        markers
            .iter()
            .map(|m| (m.position.to_bits(), m.source_position.to_bits()))
            .collect(),
    );
    if let Some(kept) = cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
        return Some(kept);
    }
    let render = Arc::new(render(audio, markers, length));
    if let Ok(mut c) = cache().lock() {
        if c.len() >= KEPT {
            c.clear();
        }
        c.insert(key, Arc::clone(&render));
    }
    Some(render)
}

/// The render itself: each stretch between markers copied or stretched,
/// joined across [`JOINT`].
fn render(audio: &AudioSource, markers: &[StretchMarker], length: f64) -> AudioSource {
    let sr = f64::from(audio.sample_rate().max(1));
    let channels = usize::from(audio.channels().max(1));
    let frames = (length * sr).ceil() as usize;
    let joint = (JOINT * sr).round() as usize;
    let source_frames = audio.frame_count();
    let read = |ch: usize, from: usize, n: usize| -> Vec<f32> {
        (from..from + n)
            .map(|i| {
                if i < source_frames {
                    audio.channel_interp(i, i, 0.0, ch)
                } else {
                    0.0
                }
            })
            .collect()
    };
    let map = |t: f64| StretchMarker::source_position_at(markers, t).unwrap_or(t);
    // The stretches: the playback time from 0 to `length`, cut at every
    // marker inside it.
    let mut cuts: Vec<f64> = vec![0.0];
    cuts.extend(
        markers
            .iter()
            .map(|m| m.position)
            .filter(|p| *p > 0.0 && *p < length),
    );
    cuts.push(length);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-9);

    let mut out = vec![vec![0.0_f32; frames + joint]; channels];
    let grain = (GRAIN_48K * sr / 48_000.0).round() as usize;
    let last = cuts.len().saturating_sub(2);
    for (k, pair) in cuts.windows(2).enumerate() {
        let (p0, p1) = (pair[0], pair[1]);
        let (at, until) = ((p0 * sr).round() as usize, (p1 * sr).round() as usize);
        let n = until.saturating_sub(at);
        if n == 0 {
            continue;
        }
        // Each but the last runs on a joint past its end, to fade out
        // under the next one fading in.
        let more = if k < last { joint } else { 0 };
        let (s0, s1) = (map(p0).max(0.0), map(p1).max(0.0));
        let rate = (s1 - s0) / (p1 - p0);
        let from = (s0 * sr).round() as usize;
        let piece: Vec<Vec<f32>> = if (rate - 1.0).abs() <= AT_SPEED {
            (0..channels).map(|ch| read(ch, from, n + more)).collect()
        } else {
            // Only this stretch of the source: past its end is the next
            // hit's attack, not this decay's.
            let span = ((s1 - s0) * sr).round().max(1.0) as usize;
            let input: Vec<Vec<f32>> = (0..channels).map(|ch| read(ch, from, span)).collect();
            pitch::wsola::stretch_planar(&input, 0, rate, n + more, grain)
        };
        for (ch, samples) in piece.iter().enumerate() {
            let Some(dest) = out.get_mut(ch) else {
                continue;
            };
            for (i, s) in samples.iter().enumerate() {
                let Some(slot) = dest.get_mut(at + i) else {
                    break;
                };
                // Fading in over the joint the one before fades out on.
                let gain = if k > 0 && i < joint {
                    (i as f32 + 0.5) / joint as f32
                } else if i >= n {
                    1.0 - ((i - n) as f32 + 0.5) / joint.max(1) as f32
                } else {
                    1.0
                };
                *slot += s * gain;
            }
        }
    }
    let mut interleaved = Vec::with_capacity(frames * channels);
    for i in 0..frames {
        for ch in &out {
            interleaved.push(ch.get(i).copied().unwrap_or(0.0));
        }
    }
    AudioSource::Memory(DecodedAudio::new(
        interleaved,
        u16::try_from(channels).unwrap_or(2),
        audio.sample_rate(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker(position: f64, source_position: f64) -> StretchMarker {
        StretchMarker {
            position,
            source_position,
            slope: 0.0,
        }
    }

    /// A 440 Hz tone, a second of it.
    fn tone() -> Arc<AudioSource> {
        let sr = 48_000_u32;
        let samples = (0..sr)
            .map(|i| (std::f64::consts::TAU * 440.0 * f64::from(i) / f64::from(sr)).sin() as f32)
            .collect();
        Arc::new(AudioSource::Memory(DecodedAudio::new(samples, 1, sr)))
    }

    #[test]
    fn a_drawn_out_tail_keeps_its_pitch() {
        // Its first 30 ms as played, the next 70 ms of source over 280 ms.
        let markers = [marker(0.0, 0.2), marker(0.03, 0.23), marker(0.31, 0.30)];
        let out = rendered(&tone(), &markers, 0.31).expect("rendered");
        let n = out.frame_count();
        assert_eq!(n, (0.31_f64 * 48_000.0).ceil() as usize);
        // In the slowed stretch, still 440 Hz: 880 crossings a second,
        // not the 220 a varispeed at a quarter would give.
        let (a, b) = (4_800, 14_400);
        let x: Vec<f32> = (a..b).map(|i| out.channel_interp(i, i, 0.0, 0)).collect();
        let crossings = x
            .windows(2)
            .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
            .count() as f64
            / ((b - a) as f64 / 48_000.0);
        assert!((crossings - 880.0).abs() < 880.0 * 0.05, "{crossings:.0}");
        // And where it plays at speed, the source sample for sample.
        let src = tone();
        for i in [10, 500, 1_000] {
            let want = src.channel_interp(9_600 + i, 9_600 + i, 0.0, 0);
            assert!((out.channel_interp(i, i, 0.0, 0) - want).abs() < 1e-5);
        }
    }

    #[test]
    fn a_take_played_at_speed_is_not_rendered() {
        let markers = [marker(0.0, 0.2), marker(0.1, 0.3)];
        assert!(rendered(&tone(), &markers, 0.1).is_none());
    }
}
