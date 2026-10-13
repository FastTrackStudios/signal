//! Integrated loudness metering — ITU-R BS.1770-4 / EBU R128.
//!
//! This is the perceptual "how loud does it *sound*" measure (LUFS), the same
//! standard NAM's trainer uses to compute a model's `loudness` metadata. We use
//! it to guarantee every amp model in a library lands on the same average
//! output loudness: run a fixed DI clip through each model (see
//! [`crate::nam_calibrate`]), measure the integrated loudness here, and derive a
//! makeup gain toward a common target. Because the measure is K-weighted (not
//! raw RMS), a scooped clean and a saturated high-gain model that "read" the
//! same here actually *sound* the same — so switching amps never jumps the
//! level.
//!
//! The pipeline is the textbook BS.1770 one:
//!   1. K-weighting: a high-shelf "head" filter + a ~38 Hz high-pass (RLB).
//!   2. 400 ms mean-square blocks with 75 % overlap (100 ms hop).
//!   3. Two-stage gating: absolute −70 LUFS, then a relative −10 LU gate.
//!   4. Integrated loudness = −0.691 + 10·log10(mean gated block power).
//!
//! Mono only — guitar amp models are mono in / mono out, so the channel weight
//! is 1.0 and there is no L/R summation to worry about here.
//!
//! The measurement itself is [`dsp_core::audio::loudness::integrated_lufs`]
//! — one BS.1770 implementation for the whole stack.

pub use dsp_core::audio::loudness::{SILENCE_LUFS, integrated_lufs};

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, amp: f64, secs: f64, sr: f64) -> Vec<f64> {
        let n = (secs * sr) as usize;
        (0..n)
            .map(|i| amp * (2.0 * core::f64::consts::PI * freq * i as f64 / sr).sin())
            .collect()
    }

    /// Doubling the amplitude must raise integrated loudness by exactly ~6 dB
    /// (loudness is scale-invariant in shape, so gating picks the same blocks).
    #[test]
    fn doubling_amplitude_adds_six_db() {
        let sr = 48_000.0;
        let quiet = integrated_lufs(&sine(1000.0, 0.1, 3.0, sr), sr);
        let loud = integrated_lufs(&sine(1000.0, 0.2, 3.0, sr), sr);
        assert!(quiet.is_finite() && loud.is_finite());
        assert!(
            (loud - quiet - 6.020_599).abs() < 0.01,
            "expected +6.02 LU, got {}",
            loud - quiet
        );
    }

    /// A 1 kHz sine near −20 dBFS RMS should read in a sane LUFS range (the
    /// K-weighting adds ~+2 dB of head gain at 1 kHz, minus the 0.691 offset).
    #[test]
    fn thousand_hz_reference_is_in_range() {
        let sr = 48_000.0;
        // −20 dBFS RMS → peak amplitude 10^(−20/20)·√2.
        let amp = 10f64.powf(-20.0 / 20.0) * 2f64.sqrt();
        let lufs = integrated_lufs(&sine(1000.0, amp, 3.0, sr), sr);
        assert!(
            (-21.0..=-17.0).contains(&lufs),
            "1kHz −20dBFS sine read {lufs} LUFS, outside plausible band"
        );
    }

    /// Silence and too-short signals return the sentinel, not NaN/0.
    #[test]
    fn silence_and_short_signals_are_sentinel() {
        let sr = 48_000.0;
        assert_eq!(integrated_lufs(&[], sr), SILENCE_LUFS);
        assert_eq!(integrated_lufs(&vec![0.0; 48_000], sr), SILENCE_LUFS);
        // 100 ms < one 400 ms block.
        assert_eq!(
            integrated_lufs(&sine(1000.0, 0.5, 0.1, sr), sr),
            SILENCE_LUFS
        );
    }

    /// Works at a non-48k rate too (coefficients are SR-parametrised): the same
    /// tone at 44.1 kHz reads within a hair of the 48 kHz measurement.
    #[test]
    fn sample_rate_independent() {
        let a = integrated_lufs(&sine(1000.0, 0.25, 3.0, 48_000.0), 48_000.0);
        let b = integrated_lufs(&sine(1000.0, 0.25, 3.0, 44_100.0), 44_100.0);
        assert!((a - b).abs() < 0.15, "48k={a} vs 44.1k={b}");
    }
}
