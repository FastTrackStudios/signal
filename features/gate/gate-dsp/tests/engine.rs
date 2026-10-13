//! The measured laws, checked on synthetic signals.

use dsp_core::num;
use gate_dsp::{DrumGate, Mode, Settings, db_to_gain, debleed_exponent, output_gain};

const SR: f64 = 48_000.0;

fn db(x: f64) -> f64 {
    20.0 * x.abs().max(1e-300).log10()
}

fn sine(n: usize, f: f64, amp: f64) -> impl Iterator<Item = f64> {
    (0..n).map(move |i| amp * (2.0 * std::f64::consts::PI * f * num::count_to_f64(i) / SR).sin())
}

/// Run a mono signal (duplicated to stereo) and return the left output.
fn run(gate: &mut DrumGate, x: &[f64]) -> Vec<f64> {
    x.iter()
        .map(|&s| {
            let mut fr = [s, s];
            gate.process_frame(&mut fr, None);
            fr[0]
        })
        .collect()
}

#[test]
fn latency_is_the_mode_lookahead() {
    assert_eq!(DrumGate::latency_for(Mode::Kick, SR), 240);
    assert_eq!(DrumGate::latency_for(Mode::SnareTop, SR), 72);
    assert_eq!(DrumGate::latency_for(Mode::Toms, 96_000.0), 480);
    assert_eq!(DrumGate::latency_for(Mode::SnareTop, 44_100.0), 66);
    assert_eq!(DrumGate::latency_for(Mode::Kick, 44_100.0), 220);
}

#[test]
fn open_gate_is_a_pure_delay() {
    for mode in Mode::ALL {
        let settings = Settings { mode, threshold_db: -40.0, debleed: 100.0, ..Settings::default() };
        let mut gate = DrumGate::new(SR, 2, settings);
        let input: Vec<f64> = sine(24_000, 997.0, 0.5).collect();
        let out = run(&mut gate, &input);
        let lat = gate.latency();
        let err = (12_000..24_000).map(|n| (out[n] - input[n - lat]).abs()).fold(0.0, f64::max);
        assert!(err < 1e-12, "{mode:?}: open gate deviates from a delay by {err}");
    }
}

#[test]
fn closed_gate_low_band_sits_at_reduction() {
    // 40 Hz, well below threshold; Debleed −100 with a low threshold keeps the HF expander open.
    let s = Settings { threshold_db: -70.0, reduction_db: -37.0, debleed: -100.0, ..Settings::default() };
    let mut g = DrumGate::new(SR, 2, s);
    let x: Vec<f64> = sine(96_000, 40.0, db_to_gain(-71.0)).collect();
    let y = run(&mut g, &x);
    let peak = y[48_000..].iter().fold(0.0f64, |m, v| m.max(v.abs()));
    assert!((db(peak) - (-71.0 - 37.0)).abs() < 0.05, "closed level {:.3} dB", db(peak));
}

#[test]
fn release_follows_fifth_power_ramp_over_length() {
    let length = 400.0;
    let s = Settings { threshold_db: -30.0, reduction_db: -80.0, length_ms: length, debleed: -100.0, ..Settings::default() };
    let mut g = DrumGate::new(SR, 2, s);
    // One open frame (a full-scale click), then silence: ramp starts after the 50 ms hold
    // plus the detector's decay below threshold.
    let mut gains = Vec::new();
    for n in 0..48_000 {
        let v = if n == 0 { 1.0 } else { 0.0 };
        g.process_frame(&mut [v, v], None);
        gains.push(g.gain());
    }
    let opened = gains.iter().position(|&v| v >= 1.0).expect("click opens the gate");
    let start = opened + gains[opened..].iter().position(|&v| v < 1.0).expect("release starts");
    let floor = db_to_gain(-80.0);
    for frac in [0.1, 0.3, 0.5, 0.8] {
        let k = start + num::f64_to_index((frac * length * 0.001 * SR).round());
        let want = (1.0 - floor).mul_add((1.0 - frac).powi(5), floor);
        assert!((gains[k] - want).abs() < 2e-4, "tau {frac}: {} vs {want}", gains[k]);
    }
    let end = start + num::f64_to_index(length * 0.001 * SR) + 2;
    assert!((gains[end] - floor).abs() < 1e-12);
}

#[test]
fn ghost_lowers_the_threshold_by_20_db() {
    let tone: Vec<f64> = sine(9_600, 400.0, db_to_gain(-45.0)).collect();
    for (ghost, opens) in [(false, false), (true, true)] {
        let s = Settings { threshold_db: -30.0, ghost, debleed: -100.0, ..Settings::default() };
        let mut g = DrumGate::new(SR, 2, s);
        run(&mut g, &tone);
        assert_eq!(g.gain() > 0.5, opens, "ghost {ghost}");
    }
}

#[test]
fn detection_is_linked_on_the_louder_channel() {
    let s = Settings { threshold_db: -30.0, debleed: -100.0, ..Settings::default() };
    let mut g = DrumGate::new(SR, 2, s);
    for v in sine(4_800, 300.0, 0.3) {
        let mut fr = [v, 0.0];
        g.process_frame(&mut fr, None);
    }
    assert!(g.gain() > 0.99, "a left-only hit must open both channels");
}

#[test]
fn debleed_exponent_knee() {
    for (d, a) in [(-100.0, 2.15), (0.0, 0.65), (10.0, 0.5), (100.0, 0.1625)] {
        assert!((debleed_exponent(d) - a).abs() < 1e-12, "a({d})");
    }
}

#[test]
fn output_law_is_exact_on_6_db_nodes() {
    for n in [-48.0, -24.0, -6.0, 0.0, 6.0] {
        assert!((db(output_gain(n)) - n).abs() < 1e-9);
    }
    assert!((db(output_gain(3.0)) - 2.995_090).abs() < 1e-5);
    assert!((db(output_gain(-3.0)) - -3.004_910).abs() < 1e-5);
}

#[test]
fn hf_band_expands_2_to_1_below_the_debleed_threshold() {
    // Reduction 0 → the main gain is 1; a 10 kHz tone lives in the HF band.
    // extra = peak_dB − a·threshold_dB (when negative): 1 dB per dB.
    let thr = -30.0;
    let a = debleed_exponent(0.0);
    for peak in [-60.0, -50.0] {
        let settings = Settings { threshold_db: thr, reduction_db: 0.0, debleed: 0.0, ..Settings::default() };
        let mut gate = DrumGate::new(SR, 2, settings);
        let input: Vec<f64> = sine(48_000, 10_000.0, db_to_gain(peak)).collect();
        let rendered = run(&mut gate, &input);
        let out = rendered[24_000..].iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let want = peak + (peak - a * thr);
        assert!((db(out) - want).abs() < 0.1, "peak {peak}: {:.2} vs {want:.2}", db(out));
    }
}

#[test]
fn sidechain_key_replaces_the_input_for_detection() {
    let settings = Settings { threshold_db: -30.0, debleed: -100.0, ..Settings::default() };
    // Loud input, silent key: stays closed.
    let mut gate = DrumGate::new(SR, 2, settings);
    for v in sine(4_800, 300.0, 0.5) {
        gate.process_frame(&mut [v, v], Some(&[0.0, 0.0]));
    }
    assert!(gate.gain() < 1e-3, "a silent key must keep the gate shut");
    // Quiet input, loud key: opens.
    let mut gate = DrumGate::new(SR, 2, settings);
    for v in sine(4_800, 300.0, 0.5) {
        gate.process_frame(&mut [1e-4, 1e-4], Some(&[v, v]));
    }
    assert!(gate.gain() > 0.99, "a loud key must open the gate");
}
