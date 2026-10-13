//! Oversampling tests — verify transparent resampling, alias reduction, and latency.

use audiocore_dsp::oversampling::{OversampleQuality, OversampleRate, Oversampler};
use std::f64::consts::PI;

const SAMPLE_RATE: f64 = 48000.0;

// ── Pass-through (1x) ──────────────────────────────────────────────────

#[test]
fn x1_is_passthrough() {
    let mut os = Oversampler::new(OversampleRate::X1, OversampleQuality::Medium);
    os.update(SAMPLE_RATE);

    let original: Vec<f64> = (0..512)
        .map(|i| (2.0 * PI * 1000.0 * i as f64 / SAMPLE_RATE).sin())
        .collect();

    let mut left = original.clone();
    let mut right = original.clone();

    os.process_stereo(&mut left, &mut right, |_l, _r| {
        // no-op
    });

    for (i, (&a, &b)) in original.iter().zip(left.iter()).enumerate() {
        assert!(
            (a - b).abs() < 1e-15,
            "X1 should be perfect passthrough at sample {i}: {a} vs {b}"
        );
    }
}

#[test]
fn x1_zero_latency() {
    let os = Oversampler::new(OversampleRate::X1, OversampleQuality::High);
    assert_eq!(os.latency(), 0);
}

// ── Unity gain ──────────────────────────────────────────────────────────

#[test]
fn x2_preserves_signal_level() {
    check_level_preservation(OversampleRate::X2, OversampleQuality::Medium);
}

#[test]
fn x4_preserves_signal_level() {
    check_level_preservation(OversampleRate::X4, OversampleQuality::Medium);
}

#[test]
fn x8_preserves_signal_level() {
    check_level_preservation(OversampleRate::X8, OversampleQuality::Medium);
}

fn check_level_preservation(rate: OversampleRate, quality: OversampleQuality) {
    let mut os = Oversampler::new(rate, quality);
    os.update(SAMPLE_RATE);

    // Use a low-frequency sine well below Nyquist — should pass through
    // at approximately the same level after multiple blocks settle
    let freq = 200.0;
    let n = 2048;
    let signal: Vec<f64> = (0..n)
        .map(|i| (2.0 * PI * freq * i as f64 / SAMPLE_RATE).sin() * 0.8)
        .collect();

    // Process several blocks to let the filter settle
    for _ in 0..4 {
        let mut left = signal.clone();
        let mut right = signal.clone();
        os.process_stereo(&mut left, &mut right, |_l, _r| {
            // identity processing
        });
    }

    // Final block — measure
    let mut left = signal.clone();
    let mut right = signal.clone();
    os.process_stereo(&mut left, &mut right, |_l, _r| {});

    let input_rms: f64 = (signal.iter().map(|x| x * x).sum::<f64>() / n as f64).sqrt();
    let output_rms: f64 = (left.iter().map(|x| x * x).sum::<f64>() / n as f64).sqrt();

    let ratio = output_rms / input_rms;
    assert!(
        (ratio - 1.0).abs() < 0.15,
        "{}x oversampling should preserve level: ratio={ratio:.4} (input_rms={input_rms:.4}, output_rms={output_rms:.4})",
        rate.ratio()
    );
}

// ── Callback receives oversampled data ──────────────────────────────────

#[test]
fn callback_receives_correct_length() {
    for rate in [OversampleRate::X2, OversampleRate::X4, OversampleRate::X8] {
        let mut os = Oversampler::new(rate, OversampleQuality::Low);
        os.update(SAMPLE_RATE);

        let n = 256;
        let mut left = vec![0.5; n];
        let mut right = vec![0.5; n];
        let expected_os_len = n * rate.ratio();

        os.process_stereo(&mut left, &mut right, |l, r| {
            assert_eq!(
                l.len(),
                expected_os_len,
                "{}x: callback should receive {}x samples",
                rate.ratio(),
                rate.ratio()
            );
            assert_eq!(l.len(), r.len());
        });
    }
}

// ── Alias reduction ─────────────────────────────────────────────────────

#[test]
fn oversampling_reduces_aliasing() {
    // Verify oversampling produces different output than direct clipping.
    // With oversampling, the clipping harmonics above Nyquist are filtered
    // out rather than aliasing back into the signal.

    let n = 1024;
    let freq = 8000.0; // High enough that clipping harmonics alias at 48kHz

    let signal: Vec<f64> = (0..n)
        .map(|i| (2.0 * PI * freq * i as f64 / SAMPLE_RATE).sin() * 2.0)
        .collect();

    // Without oversampling: just clip
    let clipped_no_os: Vec<f64> = signal.iter().map(|&x| x.clamp(-1.0, 1.0)).collect();

    // With 4x oversampling: clip at oversampled rate
    let mut os = Oversampler::new(OversampleRate::X4, OversampleQuality::Medium);
    os.update(SAMPLE_RATE);

    // Settle
    for _ in 0..4 {
        let mut left = signal.clone();
        let mut right = signal.clone();
        os.process_stereo(&mut left, &mut right, |l, _r| {
            for s in l.iter_mut() {
                *s = s.clamp(-1.0, 1.0);
            }
        });
    }

    let mut clipped_os = signal.clone();
    let mut dummy = signal.clone();
    os.process_stereo(&mut clipped_os, &mut dummy, |l, _r| {
        for s in l.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
    });

    // The two outputs should differ — oversampling changes the spectral
    // content by removing aliased components.
    let diff_energy: f64 = clipped_no_os
        .iter()
        .zip(clipped_os.iter())
        .map(|(a, b)| (a - b) * (a - b))
        .sum::<f64>();

    assert!(
        diff_energy > 0.1,
        "Oversampled clipping should produce different output than direct clipping: diff_energy={diff_energy:.6}"
    );

    // Both should have valid signal
    let os_rms: f64 = (clipped_os.iter().map(|x| x * x).sum::<f64>() / n as f64).sqrt();
    assert!(
        os_rms > 0.1,
        "Oversampled output should have signal: rms={os_rms:.4}"
    );
}

// ── Latency reporting ───────────────────────────────────────────────────

#[test]
fn latency_scales_with_quality() {
    let lat_low = Oversampler::new(OversampleRate::X4, OversampleQuality::Low).latency();
    let lat_med = Oversampler::new(OversampleRate::X4, OversampleQuality::Medium).latency();
    let lat_high = Oversampler::new(OversampleRate::X4, OversampleQuality::High).latency();

    assert!(
        lat_low <= lat_med,
        "Low quality should have less latency than medium"
    );
    assert!(
        lat_med <= lat_high,
        "Medium quality should have less latency than high"
    );
    assert_eq!(lat_low, 2, "Low quality = 2 lobes = 2 samples latency");
    assert_eq!(lat_med, 4, "Medium quality = 4 lobes = 4 samples latency");
    assert_eq!(lat_high, 8, "High quality = 8 lobes = 8 samples latency");
}

// ── Mono processing ─────────────────────────────────────────────────────

#[test]
fn mono_processing_works() {
    let mut os = Oversampler::new(OversampleRate::X2, OversampleQuality::Medium);
    os.update(SAMPLE_RATE);

    let n = 512;
    let mut data: Vec<f64> = (0..n)
        .map(|i| (2.0 * PI * 200.0 * i as f64 / SAMPLE_RATE).sin() * 0.5)
        .collect();

    // Should not panic or produce NaN
    os.process_mono(&mut data, |d| {
        for s in d.iter_mut() {
            *s *= 0.9; // mild attenuation
        }
    });

    for (i, &s) in data.iter().enumerate() {
        assert!(s.is_finite(), "NaN at sample {i}");
    }
}

// ── Reset ───────────────────────────────────────────────────────────────

#[test]
fn reset_clears_state() {
    let mut os = Oversampler::new(OversampleRate::X4, OversampleQuality::Medium);
    os.update(SAMPLE_RATE);

    // Process some signal
    let mut left = vec![1.0; 256];
    let mut right = vec![1.0; 256];
    os.process_stereo(&mut left, &mut right, |_l, _r| {});

    os.reset();

    // Process silence — should produce silence (no residual from history)
    let mut left = vec![0.0; 256];
    let mut right = vec![0.0; 256];
    os.process_stereo(&mut left, &mut right, |_l, _r| {});

    // After settling, output should be zero (there may be transient from the
    // kernel history being cleared mid-signal, but the tail should be silent)
    let tail = &left[128..];
    let peak: f64 = tail.iter().map(|x| x.abs()).fold(0.0, f64::max);
    assert!(
        peak < 0.01,
        "After reset, silence should produce near-silence: peak={peak:.6}"
    );
}

// ── NaN safety ──────────────────────────────────────────────────────────

#[test]
fn no_nan_from_edge_cases() {
    let mut os = Oversampler::new(OversampleRate::X4, OversampleQuality::Low);
    os.update(SAMPLE_RATE);

    for &val in &[0.0, 1.0, -1.0, 1e6, -1e6, 1e-30, f64::MIN_POSITIVE] {
        let mut left = vec![val; 64];
        let mut right = vec![val; 64];
        os.process_stereo(&mut left, &mut right, |_l, _r| {});
        for (i, (&l, &r)) in left.iter().zip(right.iter()).enumerate() {
            assert!(l.is_finite(), "NaN from input {val} at [{i}]: left={l}");
            assert!(r.is_finite(), "NaN from input {val} at [{i}]: right={r}");
        }
    }
}

// ── Deterministic ───────────────────────────────────────────────────────

#[test]
fn deterministic_output() {
    let signal: Vec<f64> = (0..1024)
        .map(|i| (2.0 * PI * 1000.0 * i as f64 / SAMPLE_RATE).sin() * 0.7)
        .collect();

    let run = || {
        let mut os = Oversampler::new(OversampleRate::X4, OversampleQuality::Medium);
        os.update(SAMPLE_RATE);
        let mut left = signal.clone();
        let mut right = signal.clone();
        os.process_stereo(&mut left, &mut right, |l, _r| {
            for s in l.iter_mut() {
                *s = s.tanh();
            }
        });
        left
    };

    let out1 = run();
    let out2 = run();

    for (i, (a, b)) in out1.iter().zip(out2.iter()).enumerate() {
        assert!(
            (*a).to_bits() == (*b).to_bits(),
            "Non-deterministic at [{i}]: {a} vs {b}"
        );
    }
}
