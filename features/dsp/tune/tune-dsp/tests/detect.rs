//! YIN + note + correction sanity checks on synthesized pitched tones.

use tune_dsp::{
    analyze, correct_notes, hz_to_midi, AnalyzeConfig, CorrectConfig, Scale, YinConfig, YinDetector,
};

const SR: f64 = 48_000.0;

fn sine(freq: f64, secs: f64, start: usize) -> Vec<f64> {
    let n = (SR * secs) as usize;
    (0..n)
        .map(|i| 0.4 * (core::f64::consts::TAU * freq * (start + i) as f64 / SR).sin())
        .collect()
}

#[test]
fn yin_detects_a440() {
    let mut yin = YinDetector::new(SR, YinConfig::default());
    let buf = sine(440.0, 0.1, 0);
    let f = yin.detect(&buf);
    let hz = f.f0_hz.expect("A440 should be voiced");
    assert!(
        (hz - 440.0).abs() < 5.0,
        "expected ~440 Hz, got {hz} (aperiodicity {})",
        f.aperiodicity
    );
}

#[test]
fn detuned_note_snaps_to_scale() {
    // 445 Hz ≈ A4 + ~19.5 cents sharp. Chromatic snap should pull toward A4.
    let buf = sine(445.0, 0.3, 0);
    let a = analyze(&buf, SR, AnalyzeConfig::default());
    assert!(!a.notes.is_empty(), "expected at least one note");

    let corr = correct_notes(&a.notes, Scale::CHROMATIC, CorrectConfig::default());
    let c = corr[0];
    let a4 = hz_to_midi(440.0);
    assert!(
        (c.target_midi - a4).abs() < 0.05,
        "445 Hz should snap to A4 ({a4}), got {}",
        c.target_midi
    );
    // Correction is downward (sharp → down) and non-trivial.
    assert!(
        c.applied_cents < -10.0,
        "expected downward correction, got {} cents",
        c.applied_cents
    );
    assert!(c.ratio < 1.0, "downward shift ratio should be < 1");
}

#[test]
fn in_tune_note_is_left_alone() {
    let buf = sine(440.0, 0.3, 0);
    let a = analyze(&buf, SR, AnalyzeConfig::default());
    let corr = correct_notes(&a.notes, Scale::CHROMATIC, CorrectConfig::default());
    assert!(!corr.is_empty());
    // Within the deadband → no shift.
    assert_eq!(corr[0].ratio, 1.0);
    assert_eq!(corr[0].applied_cents, 0.0);
}

/// Naive O(W²) difference function — the reference the FFT path must match.
fn naive_yin_f0(frame: &[f64], sr: f64, window: usize, threshold: f64) -> Option<f64> {
    let half = window / 2;
    let mut diff = vec![0.0f64; half];
    for tau in 1..half {
        let mut sum = 0.0;
        for j in 0..half {
            let d = frame[j] - frame[j + tau];
            sum += d * d;
        }
        diff[tau] = sum;
    }
    let mut cmnd = vec![1.0f64; half];
    let mut running = 0.0;
    for tau in 1..half {
        running += diff[tau];
        cmnd[tau] = if running > 0.0 {
            diff[tau] * tau as f64 / running
        } else {
            1.0
        };
    }
    let min_lag = (sr / 1200.0) as usize;
    let max_lag = ((sr / 65.0) as usize).min(half - 1);
    let mut tau = min_lag.max(2);
    while tau < max_lag {
        if cmnd[tau] < threshold {
            while tau + 1 < max_lag && cmnd[tau + 1] < cmnd[tau] {
                tau += 1;
            }
            // Parabolic refinement.
            let (s0, s1, s2) = (cmnd[tau - 1], cmnd[tau], cmnd[tau + 1]);
            let denom = 2.0 * (2.0 * s1 - s2 - s0);
            let refined = if denom.abs() < 1e-12 {
                tau as f64
            } else {
                tau as f64 + (s2 - s0) / denom
            };
            return Some(sr / refined);
        }
        tau += 1;
    }
    None
}

#[test]
fn fft_difference_matches_naive_reference() {
    let sr = 48000.0;
    let cfg = tune_dsp::detect::YinConfig::default();
    let window = cfg.window;
    let mut det = tune_dsp::detect::YinDetector::new(sr, cfg);
    let mut seed = 17u64;
    for &freq in &[82.4, 110.0, 220.0, 452.0, 880.0] {
        // Sine + light noise + a 2nd harmonic (realistic-ish).
        let frame: Vec<f64> = (0..window)
            .map(|i| {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let n = ((seed >> 33) as f64 / (1u64 << 31) as f64) - 1.0;
                let ph = core::f64::consts::TAU * freq * i as f64 / sr;
                0.5 * ph.sin() + 0.15 * (2.0 * ph).sin() + 0.01 * n
            })
            .collect();
        let fft_f0 = det.detect(&frame).f0_hz.expect("fft path voiced");
        let naive_f0 = naive_yin_f0(&frame, sr, window, 0.12).expect("naive path voiced");
        let cents = 1200.0 * (fft_f0 / naive_f0).log2();
        assert!(
            cents.abs() < 1.0,
            "FFT and naive must agree at {freq} Hz: fft={fft_f0:.3} naive={naive_f0:.3} ({cents:+.2}c)"
        );
    }
}

#[test]
fn fft_yin_is_fast() {
    // Not a strict benchmark — just a guard that the FFT path stays in
    // its complexity class (~1000 frames well under a second even in
    // debug builds; the naive path took ~1M mults per frame).
    let sr = 48000.0;
    let cfg = tune_dsp::detect::YinConfig::default();
    let window = cfg.window;
    let mut det = tune_dsp::detect::YinDetector::new(sr, cfg);
    let frame: Vec<f64> = (0..window)
        .map(|i| (core::f64::consts::TAU * 220.0 * i as f64 / sr).sin())
        .collect();
    let start = std::time::Instant::now();
    for _ in 0..1000 {
        let _ = det.detect(&frame);
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs_f64() < 2.0,
        "1000 detections should be fast: {elapsed:?}"
    );
}

#[test]
fn analysis_repairs_octave_glitches_and_gates_noise() {
    // 1.5 s of clean 220 Hz, then 0.5 s of pure noise (unvoiced).
    let sr = 48000.0;
    let n_voiced = 72_000;
    let n_noise = 24_000;
    let mut seed = 23u64;
    let samples: Vec<f64> = (0..n_voiced + n_noise)
        .map(|i| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let noise = ((seed >> 33) as f64 / (1u64 << 31) as f64) - 1.0;
            if i < n_voiced {
                0.5 * (core::f64::consts::TAU * 220.0 * i as f64 / sr).sin() + 0.02 * noise
            } else {
                0.3 * noise
            }
        })
        .collect();
    let analysis = tune_dsp::analyze(&samples, sr, tune_dsp::AnalyzeConfig::default());

    // Voiced region: every frame within a semitone of 220 (no octave
    // outliers survive the repair pass).
    let hop = analysis.hop;
    let voiced_frames = (n_voiced / hop).saturating_sub(8);
    let mut worst = 0.0f64;
    for (i, f) in analysis
        .frames
        .iter()
        .enumerate()
        .take(voiced_frames)
        .skip(4)
    {
        if let Some(hz) = f.f0_hz {
            let dev = (tune_dsp::detect::hz_to_midi(hz) - 57.0).abs();
            worst = worst.max(dev);
        } else {
            panic!("frame {i} in the tone should be voiced");
        }
    }
    assert!(
        worst < 1.0,
        "no octave glitches survive: worst dev {worst:.2} st"
    );

    // Noise region: the confidence gate keeps it unvoiced.
    let noise_start = n_voiced / hop + 4;
    let voiced_in_noise = analysis.frames[noise_start.min(analysis.frames.len())..]
        .iter()
        .filter(|f| f.f0_hz.is_some())
        .count();
    let total_noise = analysis.frames.len().saturating_sub(noise_start);
    assert!(
        voiced_in_noise * 5 < total_noise.max(1),
        "noise mostly gated unvoiced: {voiced_in_noise}/{total_noise}"
    );
}
