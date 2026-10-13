//! [`EnvelopeTap`] → [`EnvelopeReader`]: per-voice envelope playheads
//! (stage, position in the stage, level). Each voice is one `AtomicU64`,
//! so a read never sees half of an update.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::feed::proto::{FeedInfo, FeedKind, FeedOptions, VoiceState};

use crate::feed::dsp::FeedSource;

const IDLE: u64 = 0xFFFF;

#[inline]
fn pack(stage: Option<u16>, position: f32, level: f32) -> u64 {
    let s = stage.map_or(IDLE, |s| u64::from(s.min(0xFFFE)));
    let p = (position.clamp(0.0, 1.0) * 65535.0).round() as u64;
    (s << 48) | (p << 32) | u64::from(level.to_bits())
}

fn unpack(bits: u64) -> VoiceState {
    let s = bits >> 48;
    VoiceState {
        stage: (s != IDLE).then_some(s as usize),
        position: ((bits >> 32) & 0xFFFF) as f64 / 65535.0,
        level: f64::from(f32::from_bits(bits as u32)),
    }
}

/// The audio-thread end.
pub struct EnvelopeTap {
    voices: Arc<[AtomicU64]>,
}

/// The control-thread end.
pub struct EnvelopeReader {
    voices: Arc<[AtomicU64]>,
    shown: usize,
}

/// Playheads for `voices` voices, all idle. Allocates here; the tap never
/// does.
#[must_use]
pub fn envelope(voices: usize) -> (EnvelopeTap, EnvelopeReader) {
    let v: Arc<[AtomicU64]> = (0..voices)
        .map(|_| AtomicU64::new(pack(None, 0.0, 0.0)))
        .collect();
    (
        EnvelopeTap { voices: v.clone() },
        EnvelopeReader {
            voices: v,
            shown: voices,
        },
    )
}

impl EnvelopeTap {
    #[must_use]
    pub fn voices(&self) -> usize {
        self.voices.len()
    }

    /// Voice `voice` is in `stage` (an index into the feed's stages),
    /// `position` (0..1) through it, at `level` (0..1). Wait-free, no
    /// allocation; out-of-range voices are ignored.
    #[inline]
    pub fn set(&self, voice: usize, stage: u16, position: f32, level: f32) {
        if let Some(v) = self.voices.get(voice) {
            v.store(pack(Some(stage), position, level), Ordering::Relaxed);
        }
    }

    /// Voice `voice` finished (or was stolen).
    #[inline]
    pub fn idle(&self, voice: usize) {
        if let Some(v) = self.voices.get(voice) {
            v.store(pack(None, 0.0, 0.0), Ordering::Relaxed);
        }
    }
}

impl EnvelopeReader {
    /// Report only the first `voices` voices.
    pub fn set_voices(&mut self, voices: usize) {
        self.shown = voices.min(self.voices.len());
    }

    /// Every reported voice's state.
    pub fn states(&self) -> impl Iterator<Item = VoiceState> + '_ {
        self.voices
            .iter()
            .take(self.shown)
            .map(|v| unpack(v.load(Ordering::Relaxed)))
    }

    /// A frame's `values`: `[stage, position, level]` per voice.
    pub fn read_into(&self, out: &mut Vec<f64>) {
        out.clear();
        out.extend(self.states().flat_map(|s| s.encode()));
    }
}

impl FeedSource for EnvelopeReader {
    fn configure(&mut self, info: &FeedInfo, _options: &FeedOptions) {
        if let FeedKind::Envelope { voices, .. } = info.kind {
            self.set_voices(usize::from(voices));
        }
    }

    fn read(&mut self, _dt: f64, out: &mut Vec<f64>) {
        self.read_into(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voices_round_trip() {
        let (tap, mut r) = envelope(3);
        tap.set(0, 1, 0.5, 0.8);
        tap.set(2, 3, 1.0, 0.25);
        tap.set(7, 0, 0.0, 0.0);
        let mut out = Vec::new();
        r.read_into(&mut out);
        assert_eq!(out.len(), 9);
        let v: Vec<_> = VoiceState::decode(&out).collect();
        assert_eq!(v[0].stage, Some(1));
        assert!((v[0].position - 0.5).abs() < 1e-4 && (v[0].level - 0.8).abs() < 1e-6);
        assert_eq!(v[1].stage, None);
        assert_eq!(v[2].stage, Some(3));
        tap.idle(0);
        r.set_voices(1);
        r.read_into(&mut out);
        assert_eq!(out, [-1.0, 0.0, 0.0]);
    }
}
