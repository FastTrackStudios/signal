//! Per-frame features beyond pitch.
//!
//! The pitch tracker already reports level and whether a frame is
//! voiced. Two more are needed to tell one *unvoiced* sound from
//! another, because "no pitch" covers a sibilant, a breath, a plosive
//! and silence, and those want opposite treatment: a harsh "s" is
//! ridden down, a breath is often kept.
//!
//! Neither needs a spectrum. Both are cheap enough to compute for a
//! whole take without anyone noticing, which matters because they are
//! recomputed whenever the analysis is.
//!
//! - **Zero-crossing rate** — how often the waveform changes sign.
//!   High for fricatives, low for vowels. The oldest trick in speech
//!   processing and still the most robust for this one question.
//! - **High-band ratio** — energy above roughly 4 kHz over total. A
//!   sibilant is mostly up there; a breath is broader and quieter.
//!
//! Taken together they separate the three unvoiced cases that a level
//! alone cannot: loud-and-hissy, quiet-and-airy, and nothing at all.

/// One analysis frame's non-pitch features.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameFeature {
    /// Linear RMS.
    pub rms: f64,
    /// Zero crossings per sample, `0.0..=1.0`.
    pub zcr: f64,
    /// Energy above the split frequency as a fraction of the total.
    pub high_ratio: f64,
}

/// Where the high band starts, in Hz.
///
/// Sibilance in a voice sits from about 5 kHz up, and vowels have very
/// little there. Four is slightly below that on purpose: the point is
/// to separate an "s" from a vowel, and a margin costs nothing while a
/// split that is too high misses a dull "sh".
pub const HIGH_SPLIT_HZ: f64 = 4_000.0;

/// Compute features per frame, aligned to the pitch analysis.
///
/// `window` and `hop` must be the ones the pitch tracker used, or the
/// two feature streams describe different moments and every downstream
/// comparison is off by a fraction of a frame.
pub fn frame_features(
    samples: &[f64],
    sample_rate: f64,
    window: usize,
    hop: usize,
    frames: usize,
) -> Vec<FrameFeature> {
    let hop = hop.max(1);
    let window = window.max(2);
    // One-pole high-pass, run per frame. Cheap, and its exact shape
    // does not matter: what is being measured is a ratio between two
    // broad bands, not a filter response.
    let dt = 1.0 / sample_rate.max(1.0);
    let rc = 1.0 / (core::f64::consts::TAU * HIGH_SPLIT_HZ);
    let alpha = rc / (rc + dt);

    (0..frames)
        .map(|f| {
            let start = f * hop;
            let end = (start + window).min(samples.len());
            if start >= end {
                return FrameFeature::default();
            }
            let slice = &samples[start..end];

            let mut sum_sq = 0.0;
            let mut high_sq = 0.0;
            let mut crossings = 0usize;
            let mut prev = slice[0];
            let mut hp_prev_in = slice[0];
            let mut hp = 0.0;

            for &s in slice {
                sum_sq += s * s;
                hp = alpha * (hp + s - hp_prev_in);
                hp_prev_in = s;
                high_sq += hp * hp;
                if (s >= 0.0) != (prev >= 0.0) {
                    crossings += 1;
                }
                prev = s;
            }

            let n = slice.len() as f64;
            FrameFeature {
                rms: (sum_sq / n).sqrt(),
                zcr: crossings as f64 / n,
                // Guarded: a silent frame has no ratio, and reporting
                // one would make silence look like the brightest thing
                // in the take.
                high_ratio: if sum_sq > 1e-12 {
                    (high_sq / sum_sq).clamp(0.0, 1.0)
                } else {
                    0.0
                },
            }
        })
        .collect()
}
