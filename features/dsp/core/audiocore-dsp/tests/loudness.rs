//! Loudness metering and autogain tests.

use audiocore_dsp::loudness::{AutoGain, KWeightingFilter, LoudnessMeter};
use std::f64::consts::PI;

const SAMPLE_RATE: f64 = 48000.0;

// ── K-weighting filter ──────────────────────────────────────────────────

#[test]
fn k_weight_boosts_highs() {
    let n = 4096;

    let low_rms = {
        let mut f = KWeightingFilter::new();
        f.update(SAMPLE_RATE);
        let mut sum = 0.0;
        for i in 0..n {
            let s = (2.0 * PI * 100.0 * i as f64 / SAMPLE_RATE).sin() * 0.5;
            let out = f.tick(s);
            sum += out * out;
        }
        (sum / n as f64).sqrt()
    };

    let high_rms = {
        let mut f = KWeightingFilter::new();
        f.update(SAMPLE_RATE);
        let mut sum = 0.0;
        for i in 0..n {
            let s = (2.0 * PI * 4000.0 * i as f64 / SAMPLE_RATE).sin() * 0.5;
            let out = f.tick(s);
            sum += out * out;
        }
        (sum / n as f64).sqrt()
    };

    assert!(
        high_rms > low_rms,
        "K-weighting should boost highs relative to lows: low={low_rms:.4}, high={high_rms:.4}"
    );
}

#[test]
fn k_weight_attenuates_sub_bass() {
    let mut filt = KWeightingFilter::new();
    filt.update(SAMPLE_RATE);

    let n = 8192;
    let input_rms = 0.5_f64 / 2.0_f64.sqrt();

    let mut sum = 0.0;
    for i in 0..n {
        let s = (2.0 * PI * 20.0 * i as f64 / SAMPLE_RATE).sin() * 0.5;
        let out = filt.tick(s);
        sum += out * out;
    }
    let out_rms = (sum / n as f64).sqrt();

    assert!(
        out_rms < input_rms * 0.5,
        "K-weighting should attenuate 20Hz: in={input_rms:.4}, out={out_rms:.4}"
    );
}

#[test]
fn k_weight_passes_1khz_near_unity() {
    let mut filt = KWeightingFilter::new();
    filt.update(SAMPLE_RATE);

    let n = 4096;
    let mut sum_in = 0.0;
    let mut sum_out = 0.0;
    for i in 0..n {
        let s = (2.0 * PI * 1000.0 * i as f64 / SAMPLE_RATE).sin() * 0.5;
        let out = filt.tick(s);
        sum_in += s * s;
        sum_out += out * out;
    }

    let ratio = (sum_out / sum_in).sqrt();
    assert!(
        (ratio - 1.0).abs() < 0.15,
        "K-weighting at 1kHz should be near unity: ratio={ratio:.4}"
    );
}

// ── Loudness meter ──────────────────────────────────────────────────────

#[test]
fn silence_reads_very_low() {
    let mut meter = LoudnessMeter::new();
    meter.update(SAMPLE_RATE);

    let n = (SAMPLE_RATE * 0.5) as usize;
    let left = vec![0.0; n];
    let right = vec![0.0; n];
    meter.process(&left, &right);

    assert!(
        meter.momentary() < -60.0,
        "Silence should read very low: {:.1} LUFS",
        meter.momentary()
    );
}

#[test]
fn full_scale_sine_reads_near_0_lufs() {
    let mut meter = LoudnessMeter::new();
    meter.update(SAMPLE_RATE);

    let n = (SAMPLE_RATE * 1.0) as usize;
    let left: Vec<f64> = (0..n)
        .map(|i| (2.0 * PI * 1000.0 * i as f64 / SAMPLE_RATE).sin())
        .collect();
    let right = left.clone();

    meter.process(&left, &right);

    assert!(
        meter.momentary() > -6.0 && meter.momentary() < 3.0,
        "Full-scale 1kHz sine should be near 0 LUFS: {:.1}",
        meter.momentary()
    );
}

#[test]
fn louder_signal_reads_higher() {
    let mut meter_quiet = LoudnessMeter::new();
    meter_quiet.update(SAMPLE_RATE);

    let mut meter_loud = LoudnessMeter::new();
    meter_loud.update(SAMPLE_RATE);

    let n = (SAMPLE_RATE * 0.5) as usize;

    let quiet: Vec<f64> = (0..n)
        .map(|i| (2.0 * PI * 1000.0 * i as f64 / SAMPLE_RATE).sin() * 0.1)
        .collect();
    let loud: Vec<f64> = (0..n)
        .map(|i| (2.0 * PI * 1000.0 * i as f64 / SAMPLE_RATE).sin() * 0.8)
        .collect();

    meter_quiet.process(&quiet, &quiet);
    meter_loud.process(&loud, &loud);

    assert!(
        meter_loud.momentary() > meter_quiet.momentary(),
        "Louder signal should read higher: loud={:.1}, quiet={:.1}",
        meter_loud.momentary(),
        meter_quiet.momentary()
    );

    let diff = meter_loud.momentary() - meter_quiet.momentary();
    assert!(
        diff > 14.0 && diff < 22.0,
        "Level difference should be ~18dB: got {diff:.1}"
    );
}

#[test]
fn short_term_responds_slower_than_momentary() {
    let mut meter = LoudnessMeter::new();
    meter.update(SAMPLE_RATE);

    let loud_n = (SAMPLE_RATE * 2.0) as usize;
    let silent_n = (SAMPLE_RATE * 0.5) as usize;

    let loud: Vec<f64> = (0..loud_n)
        .map(|i| (2.0 * PI * 1000.0 * i as f64 / SAMPLE_RATE).sin() * 0.8)
        .collect();
    let silent = vec![0.0; silent_n];

    meter.process(&loud, &loud);
    meter.process(&silent, &silent);

    assert!(
        meter.short_term() > meter.momentary(),
        "Short-term should be higher than momentary after silence: short={:.1}, momentary={:.1}",
        meter.short_term(),
        meter.momentary()
    );
}

#[test]
fn meter_reset_clears_state() {
    let mut meter = LoudnessMeter::new();
    meter.update(SAMPLE_RATE);

    let n = (SAMPLE_RATE * 0.5) as usize;
    let loud: Vec<f64> = (0..n)
        .map(|i| (2.0 * PI * 1000.0 * i as f64 / SAMPLE_RATE).sin() * 0.8)
        .collect();
    meter.process(&loud, &loud);

    meter.reset();

    // After reset, feed silence and check readings dropped
    let silent = vec![0.0; (SAMPLE_RATE * 0.5) as usize];
    meter.process(&silent, &silent);

    assert!(
        meter.momentary() < -60.0,
        "After reset + silence, momentary should be very low: {:.1}",
        meter.momentary()
    );
}

// ── True peak (ebur128-specific) ────────────────────────────────────────

#[cfg(feature = "loudness")]
#[test]
fn true_peak_detects_clipped_signal() {
    let mut meter = LoudnessMeter::new();
    meter.update(SAMPLE_RATE);

    // Full-scale sine — true peak should be close to 1.0
    let n = (SAMPLE_RATE * 0.5) as usize;
    let signal: Vec<f64> = (0..n)
        .map(|i| (2.0 * PI * 1000.0 * i as f64 / SAMPLE_RATE).sin())
        .collect();
    meter.process(&signal, &signal);

    let tp = meter.true_peak_left().max(meter.true_peak_right());
    assert!(
        tp > 0.9 && tp < 1.2,
        "True peak of full-scale sine should be ~1.0: {tp:.4}"
    );

    let tp_db = meter.true_peak_dbtp();
    assert!(
        tp_db > -1.0 && tp_db < 1.0,
        "True peak dBTP should be near 0: {tp_db:.1}"
    );
}

#[cfg(feature = "loudness")]
#[test]
fn integrated_loudness_with_gating() {
    let mut meter = LoudnessMeter::new();
    meter.update(SAMPLE_RATE);

    // Feed 5 seconds of steady signal for integrated measurement
    let n = (SAMPLE_RATE * 5.0) as usize;
    let signal: Vec<f64> = (0..n)
        .map(|i| (2.0 * PI * 1000.0 * i as f64 / SAMPLE_RATE).sin() * 0.5)
        .collect();
    meter.process(&signal, &signal);

    let integrated = meter.integrated();
    assert!(
        integrated > -10.0 && integrated < 0.0,
        "Integrated loudness should be reasonable: {integrated:.1} LUFS"
    );
}

// ── Autogain ────────────────────────────────────────────────────────────

#[test]
fn autogain_boosts_quiet_signal() {
    let mut ag = AutoGain::new();
    ag.target_lufs = -14.0;
    ag.silence_lufs = -80.0;
    ag.update(SAMPLE_RATE);

    let n = (SAMPLE_RATE * 2.0) as usize;
    let amp = 0.03;
    let mut left: Vec<f64> = (0..n)
        .map(|i| (2.0 * PI * 1000.0 * i as f64 / SAMPLE_RATE).sin() * amp)
        .collect();
    let mut right = left.clone();

    ag.process(&mut left, &mut right);

    assert!(
        ag.gain_db() > 5.0,
        "Autogain should boost quiet signal: gain={:.1} dB",
        ag.gain_db()
    );
}

#[test]
fn autogain_attenuates_loud_signal() {
    let mut ag = AutoGain::new();
    ag.target_lufs = -14.0;
    ag.silence_lufs = -80.0;
    ag.update(SAMPLE_RATE);

    let n = (SAMPLE_RATE * 2.0) as usize;
    let amp = 0.9;
    let mut left: Vec<f64> = (0..n)
        .map(|i| (2.0 * PI * 1000.0 * i as f64 / SAMPLE_RATE).sin() * amp)
        .collect();
    let mut right = left.clone();

    ag.process(&mut left, &mut right);

    assert!(
        ag.gain_db() < -5.0,
        "Autogain should attenuate loud signal: gain={:.1} dB",
        ag.gain_db()
    );
}

#[test]
fn autogain_freezes_on_silence() {
    let mut ag = AutoGain::new();
    ag.target_lufs = -14.0;
    ag.silence_lufs = -72.0;
    ag.update(SAMPLE_RATE);

    let n = (SAMPLE_RATE * 0.5) as usize;
    let mut left: Vec<f64> = (0..n)
        .map(|i| (2.0 * PI * 1000.0 * i as f64 / SAMPLE_RATE).sin() * 0.1)
        .collect();
    let mut right = left.clone();
    ag.process(&mut left, &mut right);

    // Flush short meter with silence
    let flush_n = (SAMPLE_RATE * 0.05) as usize;
    let mut flush_l = vec![0.0; flush_n];
    let mut flush_r = vec![0.0; flush_n];
    ag.process(&mut flush_l, &mut flush_r);

    let gain_before = ag.gain_db();

    let mut silence_l = vec![0.0; (SAMPLE_RATE * 0.5) as usize];
    let mut silence_r = vec![0.0; (SAMPLE_RATE * 0.5) as usize];
    ag.process(&mut silence_l, &mut silence_r);

    let gain_after = ag.gain_db();

    assert!(
        (gain_after - gain_before).abs() < 0.5,
        "Gain should freeze during silence: before={gain_before:.2}, after={gain_after:.2}"
    );
}

#[test]
fn autogain_respects_max_gain() {
    let mut ag = AutoGain::new();
    ag.target_lufs = -14.0;
    ag.max_gain_db = 12.0;
    ag.silence_lufs = -100.0;
    ag.update(SAMPLE_RATE);

    let n = (SAMPLE_RATE * 3.0) as usize;
    let amp = 0.001;
    let mut left: Vec<f64> = (0..n)
        .map(|i| (2.0 * PI * 1000.0 * i as f64 / SAMPLE_RATE).sin() * amp)
        .collect();
    let mut right = left.clone();

    ag.process(&mut left, &mut right);

    assert!(
        ag.gain_db() <= 12.1,
        "Gain should be capped at max_gain_db: {:.1} dB",
        ag.gain_db()
    );
}

#[test]
fn autogain_reset_returns_to_unity() {
    let mut ag = AutoGain::new();
    ag.target_lufs = -14.0;
    ag.update(SAMPLE_RATE);

    let n = (SAMPLE_RATE * 1.0) as usize;
    let mut left: Vec<f64> = (0..n)
        .map(|i| (2.0 * PI * 1000.0 * i as f64 / SAMPLE_RATE).sin() * 0.5)
        .collect();
    let mut right = left.clone();
    ag.process(&mut left, &mut right);

    ag.reset();

    assert!(
        ag.gain_db().abs() < 0.01,
        "After reset, gain should be 0 dB: {:.4}",
        ag.gain_db()
    );
}

// ── Autogain provides meter access ──────────────────────────────────────

#[test]
fn autogain_exposes_meter() {
    let mut ag = AutoGain::new();
    ag.target_lufs = -14.0;
    ag.update(SAMPLE_RATE);

    let n = (SAMPLE_RATE * 1.0) as usize;
    let mut left: Vec<f64> = (0..n)
        .map(|i| (2.0 * PI * 1000.0 * i as f64 / SAMPLE_RATE).sin() * 0.5)
        .collect();
    let mut right = left.clone();
    ag.process(&mut left, &mut right);

    let meter = ag.meter();
    // The meter should have valid readings from the input signal
    assert!(
        meter.momentary() > -20.0,
        "Meter should show input loudness: {:.1} LUFS",
        meter.momentary()
    );
}

// ── NaN safety ──────────────────────────────────────────────────────────

#[test]
fn loudness_no_nan() {
    let mut meter = LoudnessMeter::new();
    meter.update(SAMPLE_RATE);

    for &val in &[0.0, 1.0, -1.0, 1e6, -1e6, 1e-30, f64::MIN_POSITIVE] {
        let left = vec![val; 256];
        let right = vec![val; 256];
        meter.process(&left, &right);
        // momentary should be finite or very negative (silence)
        let m = meter.momentary();
        assert!(
            m.is_finite() || m <= -100.0,
            "NaN from input {val}: momentary={m}"
        );
    }
}

#[test]
fn autogain_no_nan() {
    let mut ag = AutoGain::new();
    ag.update(SAMPLE_RATE);

    for &val in &[0.0, 1.0, -1.0, 1e6, -1e6, 1e-30, f64::MIN_POSITIVE] {
        let mut left = vec![val; 256];
        let mut right = vec![val; 256];
        ag.process(&mut left, &mut right);
        for (i, (&l, &r)) in left.iter().zip(right.iter()).enumerate() {
            assert!(l.is_finite(), "NaN from input {val} at [{i}]: left={l}");
            assert!(r.is_finite(), "NaN from input {val} at [{i}]: right={r}");
        }
    }
}
