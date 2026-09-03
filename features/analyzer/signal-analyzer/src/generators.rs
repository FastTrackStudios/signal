//! Deterministic test signals.
//!
//! Every generator is a pure function of its arguments — no clocks, no RNG
//! seeded from the environment — so a comparison run is reproducible and can
//! be asserted on in CI.

use std::f64::consts::TAU;

/// A single-sample impulse: the excitation for decay and impulse-response
/// measurement.
#[must_use]
pub fn impulse(length: usize) -> Vec<f32> {
    let mut buf = vec![0.0f32; length];
    if let Some(first) = buf.first_mut() {
        *first = 1.0;
    }
    buf
}

/// A sine at `freq_hz`, amplitude 1.0.
#[must_use]
pub fn sine(freq_hz: f64, sample_rate: f64, length: usize) -> Vec<f32> {
    (0..length)
        .map(|i| (TAU * freq_hz * i as f64 / sample_rate).sin() as f32)
        .collect()
}

/// A sine at a given amplitude in dBFS.
#[must_use]
pub fn sine_db(freq_hz: f64, sample_rate: f64, length: usize, db: f64) -> Vec<f32> {
    let gain = 10.0f64.powf(db / 20.0) as f32;
    sine(freq_hz, sample_rate, length)
        .into_iter()
        .map(|s| s * gain)
        .collect()
}

/// An exponential (log) sine sweep from `start_hz` to `end_hz`.
///
/// Exponential rather than linear because it spends equal time per octave,
/// which is what a frequency-response comparison wants — and because its
/// harmonic distortion products separate cleanly in the deconvolved response.
#[must_use]
pub fn sweep(start_hz: f64, end_hz: f64, sample_rate: f64, length: usize) -> Vec<f32> {
    if length == 0 || start_hz <= 0.0 || end_hz <= 0.0 {
        return vec![0.0; length];
    }
    let duration = length as f64 / sample_rate;
    let ratio = end_hz / start_hz;
    let k = duration / ratio.ln();
    (0..length)
        .map(|i| {
            let t = i as f64 / sample_rate;
            let phase = TAU * start_hz * k * ((t / k).exp() - 1.0);
            phase.sin() as f32
        })
        .collect()
}

/// Reproducible white noise in `-1..1`, from a fixed seed.
///
/// xorshift64 rather than a real RNG so the sequence is identical on every
/// platform and every run — a comparison threshold is only meaningful if the
/// stimulus is bit-identical between the reference and the candidate.
#[must_use]
pub fn white_noise(length: usize, seed: u64) -> Vec<f32> {
    let mut state = if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed };
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            // Take the top 32 bits — the low bits of xorshift are weaker.
            let v = (state >> 32) as u32;
            (v as f64 / u32::MAX as f64 * 2.0 - 1.0) as f32
        })
        .collect()
}

/// Silence — used to measure a reverb tail after the excitation stops.
#[must_use]
pub fn silence(length: usize) -> Vec<f32> {
    vec![0.0; length]
}

/// An impulse followed by silence long enough to capture the whole tail.
///
/// This is the standard reverb stimulus: `tail_seconds` should exceed the
/// longest RT60 being measured, or the decay fit runs off the end of the
/// buffer.
#[must_use]
pub fn impulse_with_tail(tail_seconds: f64, sample_rate: f64) -> Vec<f32> {
    let n = (tail_seconds * sample_rate).max(1.0) as usize;
    impulse(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    fn rms(x: &[f32]) -> f64 {
        (x.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / x.len() as f64).sqrt()
    }

    #[test]
    fn impulse_is_one_then_silence() {
        let x = impulse(16);
        assert_eq!(x[0], 1.0);
        assert!(x[1..].iter().all(|&s| s == 0.0));
        assert!(impulse(0).is_empty());
    }

    #[test]
    fn sine_has_the_expected_amplitude_and_period() {
        let x = sine(1000.0, SR, 48_000);
        // RMS of a unit sine is 1/√2.
        assert!((rms(&x) - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-3);
        // 1 kHz at 48 kHz → 48 samples per cycle, so it repeats.
        assert!((x[0] - x[48]).abs() < 1e-6);
    }

    #[test]
    fn sine_db_scales_amplitude() {
        let full = sine(440.0, SR, 4800);
        let quiet = sine_db(440.0, SR, 4800, -20.0);
        // -20 dB is a factor of 10.
        assert!((rms(&full) / rms(&quiet) - 10.0).abs() < 0.01);
    }

    #[test]
    fn sweep_starts_low_and_ends_high() {
        let n = 48_000;
        let x = sweep(20.0, 20_000.0, SR, n);
        // Count zero crossings in the first and last tenth — frequency rises.
        let crossings = |s: &[f32]| s.windows(2).filter(|w| w[0].signum() != w[1].signum()).count();
        let head = crossings(&x[..n / 10]);
        let tail = crossings(&x[n - n / 10..]);
        assert!(tail > head * 10, "sweep should rise: {head} → {tail}");
    }

    #[test]
    fn sweep_handles_degenerate_arguments() {
        assert!(sweep(20.0, 20_000.0, SR, 0).is_empty());
        assert!(sweep(0.0, 20_000.0, SR, 16).iter().all(|&s| s == 0.0));
    }

    #[test]
    fn white_noise_is_reproducible_and_centred() {
        let a = white_noise(8192, 42);
        let b = white_noise(8192, 42);
        assert_eq!(a, b, "same seed must give the same sequence");
        assert_ne!(a, white_noise(8192, 43));

        let mean = a.iter().map(|&s| s as f64).sum::<f64>() / a.len() as f64;
        assert!(mean.abs() < 0.05, "roughly zero-mean, got {mean}");
        assert!(a.iter().all(|&s| (-1.0..=1.0).contains(&s)));
    }

    #[test]
    fn a_zero_seed_still_produces_noise() {
        // xorshift is stuck at zero forever if seeded with zero.
        let x = white_noise(256, 0);
        assert!(x.iter().any(|&s| s != 0.0));
    }

    #[test]
    fn impulse_with_tail_is_the_requested_length() {
        assert_eq!(impulse_with_tail(2.0, SR).len(), 96_000);
        assert_eq!(silence(10).len(), 10);
    }
}
