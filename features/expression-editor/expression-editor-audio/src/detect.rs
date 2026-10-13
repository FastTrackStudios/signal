//! Transient detection for quantizing.
//!
//! The detector is [`crate::gate`] — two envelope followers racing each
//! other, sample by sample. This module is what wraps it in the controls
//! that make it usable on a real kit, and each of those exists because
//! the bare detector fails in a specific way:
//!
//! - **Filters** ahead of detection. Not for tone: a kick's transient is
//!   buried under a ringing snare in the full-band signal, and
//!   band-limiting to where the hit lives makes it the loudest thing
//!   again. Only detection sees the filtered signal — everything written
//!   applies to the original.
//! - **Threshold** and **crest** are the gate's own two conditions,
//!   passed straight through: loud enough to be real, and struck rather
//!   than swelled.
//! - **Sensitivity** on a peak or RMS scale, applied *after* detection.
//!   A second, softer filter on hits that already passed the gate:
//!   which loudness measure ranks them is the user's call, because peak
//!   favours sharp attacks and RMS favours weight, and a kit wants one
//!   where a bass wants the other.
//! - **Time offset**, a fixed shift on every hit. Filters have latency
//!   and any detector fires once an attack is underway, so the honest
//!   position of a hit is usually a hair before where it was found.
//!
//! Why not the spectral flux detector in [`crate::onsets`]: an STFT
//! reports the *frame* a change happened in, so its answer is quantised
//! to a hop — 5.8 ms at the usual settings. Quantizing with that
//! replaces one timing jitter with another the same size. Flux keeps its
//! job of segmenting a take, where a hop is fine and the spectral
//! centroid it produces is what sorts a hit into a band.

use crate::gate::{self, GateConfig, Hit};

/// Which loudness measure ranks a hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Scale {
    /// Loudest sample in the attack. Favours sharp hits.
    #[default]
    Peak,
    /// Energy over the attack. Favours weight — a soft but full kick
    /// outranks a sharp stick click.
    Rms,
}

/// Band-limiting applied before detection only.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Filters {
    /// High-pass corner in Hz, or `None`. Steep — three one-pole stages,
    /// about 18 dB/oct — because the point is to *remove* a competing
    /// band, and a gentle slope leaves enough of it to keep winning.
    pub high_pass_hz: Option<f64>,
    /// Low-pass corner in Hz, or `None`. Two poles, about 12 dB/oct: a
    /// steeper one rings, and ringing on a low-pass is indistinguishable
    /// from the transient it is meant to isolate.
    pub low_pass_hz: Option<f64>,
    /// Linear gain after filtering, to make up what the filters took.
    /// Detection thresholds are absolute, so filtering without this
    /// silently raises every one of them.
    pub gain: f64,
}

impl Filters {
    pub fn none() -> Self {
        Self {
            high_pass_hz: None,
            low_pass_hz: None,
            gain: 1.0,
        }
    }

    pub fn is_flat(&self) -> bool {
        self.high_pass_hz.is_none() && self.low_pass_hz.is_none() && (self.gain - 1.0).abs() < 1e-9
    }

    /// Apply to a copy of the signal.
    pub fn apply(&self, samples: &[f64], sample_rate: f64) -> Vec<f64> {
        let mut out = samples.to_vec();
        if let Some(hz) = self.high_pass_hz {
            // Three cascaded one-pole stages. Cascading rather than one
            // biquad keeps it unconditionally stable at any corner a
            // user can dial, which matters more here than the exact
            // shape of the knee.
            for _ in 0..3 {
                one_pole_high(&mut out, hz, sample_rate);
            }
        }
        if let Some(hz) = self.low_pass_hz {
            for _ in 0..2 {
                one_pole_low(&mut out, hz, sample_rate);
            }
        }
        if (self.gain - 1.0).abs() > 1e-9 {
            for s in &mut out {
                *s *= self.gain;
            }
        }
        out
    }
}

fn one_pole_low(buf: &mut [f64], hz: f64, sample_rate: f64) {
    let dt = 1.0 / sample_rate.max(1.0);
    let rc = 1.0 / (core::f64::consts::TAU * hz.max(1.0));
    let a = dt / (rc + dt);
    let mut y = 0.0;
    for s in buf.iter_mut() {
        y += a * (*s - y);
        *s = y;
    }
}

fn one_pole_high(buf: &mut [f64], hz: f64, sample_rate: f64) {
    let dt = 1.0 / sample_rate.max(1.0);
    let rc = 1.0 / (core::f64::consts::TAU * hz.max(1.0));
    let a = rc / (rc + dt);
    let mut prev_in = 0.0;
    let mut y = 0.0;
    for s in buf.iter_mut() {
        let x = *s;
        y = a * (y + x - prev_in);
        prev_in = x;
        *s = y;
    }
}

/// How transients are found for quantizing.
#[derive(Clone, Copy, Debug)]
pub struct DetectConfig {
    /// The underlying gate: threshold, crest and retrigger live here.
    pub gate: GateConfig,
    pub filters: Filters,
    /// Which loudness measure ranks hits.
    pub scale: Scale,
    /// Rank hits against the loudest in *this take* rather than against
    /// full scale.
    ///
    /// On by default because a take is whatever level it was recorded
    /// at, and a sensitivity that meant one thing at −6 dBFS meaning
    /// something else at −20 makes the control useless.
    pub normalize: bool,
    /// `0.0..=1.0`. Higher keeps softer hits. Compared against the
    /// scaled loudness, so it means the same thing on either scale.
    pub sensitivity: f64,
    /// Shift every detected transient by this many seconds.
    ///
    /// Almost always slightly negative. Filters have latency and a flux
    /// detector fires once the attack is underway, so the honest
    /// position of a hit is a little before where it was found.
    pub time_offset_secs: f64,
}

impl Default for DetectConfig {
    fn default() -> Self {
        Self {
            gate: GateConfig::default(),
            filters: Filters::none(),
            scale: Scale::default(),
            normalize: true,
            sensitivity: 0.5,
            time_offset_secs: 0.0,
        }
    }
}

/// A transient, with the measurements the quantizer ranks it by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transient {
    /// Position in seconds from the start of the take, offset applied.
    pub at: f64,
    /// Loudness on the configured scale, `0.0..=1.0` after normalising.
    pub loudness: f64,
    /// How far the fast envelope led the slow one when this fired, in
    /// dB — how struck the sound was.
    pub crest_db: f64,
    /// The gate hit this came from, for callers that want both loudness
    /// measures rather than the one `loudness` was taken on.
    pub hit: Hit,
}

/// Find transients for quantizing.
///
/// `samples` is the *original* take. Filtering happens here and is not
/// visible to the caller, because the filtered signal exists only to be
/// measured — writing anything derived from it back would be applying a
/// detection aid to the audio itself.
pub fn transients(samples: &[f64], sample_rate: f64, cfg: DetectConfig) -> Vec<Transient> {
    if samples.is_empty() || sample_rate <= 0.0 {
        return Vec::new();
    }
    let filtered;
    let detect_on: &[f64] = if cfg.filters.is_flat() {
        samples
    } else {
        filtered = cfg.filters.apply(samples, sample_rate);
        &filtered
    };

    let hits = gate::detect(detect_on, sample_rate, cfg.gate);
    let mut measured: Vec<Transient> = hits
        .iter()
        .map(|hit| Transient {
            at: (hit.sample as f64 / sample_rate + cfg.time_offset_secs).max(0.0),
            loudness: match cfg.scale {
                Scale::Peak => hit.peak,
                Scale::Rms => hit.rms,
            },
            crest_db: hit.crest_db,
            hit: *hit,
        })
        .collect();

    if cfg.normalize {
        // Against the loudest hit in *this* take rather than full scale.
        // A take is whatever level it was recorded at, and a sensitivity
        // that means one thing at -6 dBFS and another at -20 is not a
        // control anyone can learn.
        let loudest = measured.iter().map(|t| t.loudness).fold(0.0f64, f64::max);
        if loudest > 0.0 {
            for t in &mut measured {
                t.loudness /= loudest;
            }
        }
    }

    // Sensitivity is the only filter left — the gate already applied the
    // threshold and the crest test, at the moment each hit fired, which
    // is both cheaper and more honest than re-deriving them from a
    // measurement window afterwards.
    let keep = 1.0 - cfg.sensitivity.clamp(0.0, 1.0);
    measured.retain(|t| t.loudness >= keep);
    measured
}
