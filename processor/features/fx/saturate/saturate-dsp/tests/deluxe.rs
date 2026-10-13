//! Deluxe behaves like the limiter it was measured from: a fixed small-signal
//! colour, a heavy feedback limit around −20 dBFS, Crunch distorting rather
//! than compressing, Darkness darkening, a dry Mix of 0 — and it stays
//! finite under anything at any rate.

use dsp_core::num::{count_to_f64, f64_to_index};
use saturate_dsp::deluxe::{Deluxe, DeluxeParams};

fn sine(hz: f64, amp: f64, secs: f64, sr: f64) -> Vec<f64> {
    (0..f64_to_index(secs * sr)).map(|i| amp * (2.0 * std::f64::consts::PI * hz * count_to_f64(i) / sr).sin()).collect()
}

fn run(p: DeluxeParams, x: &[f64], sr: f64) -> Vec<f64> {
    let mut fx = Deluxe::new(sr);
    fx.set(p);
    x.iter().map(|&s| fx.process(s, s).0).collect()
}

fn rms_db(x: &[f64]) -> f64 {
    let n = count_to_f64(x.len());
    10.0 * (x.iter().map(|v| v * v).sum::<f64>() / n).log10()
}

fn tail(x: &[f64]) -> &[f64] {
    x.get(x.len() / 2..).unwrap_or(x)
}

#[test]
fn silence_in_silence_out() {
    let y = run(DeluxeParams { crush: 10.0, crunch: 10.0, ..DeluxeParams::default() }, &vec![0.0; 48_000], 48_000.0);
    assert!(y.iter().all(|v| v.abs() < 1e-9));
}

#[test]
fn mix_zero_is_the_dry_signal() {
    let x = sine(440.0, 0.5, 0.5, 48_000.0);
    let y = run(DeluxeParams { crush: 10.0, crunch: 10.0, mix: 0.0, ..DeluxeParams::default() }, &x, 48_000.0);
    assert!(x.iter().zip(&y).all(|(a, b)| (a - b).abs() < 1e-12));
}

#[test]
fn quiet_signals_get_the_measured_colour_and_loud_ones_are_held_near_minus_twenty() {
    let sr = 48_000.0;
    // −60 dBFS, below the knee: the measured small-signal gain is +5.5 dB at 1 kHz
    let quiet = run(DeluxeParams::default(), &sine(1000.0, 1e-3, 1.0, sr), sr);
    let gain = rms_db(tail(&quiet)) - rms_db(tail(&sine(1000.0, 1e-3, 1.0, sr)));
    assert!((gain - 5.5).abs() < 0.5, "small-signal gain {gain:.2} dB");
    // 0 and +6 dBFS both come out within a couple of dB of −20 dBFS rms
    for amp in [1.0, 2.0] {
        let loud = run(DeluxeParams::default(), &sine(1000.0, amp, 1.0, sr), sr);
        let level = rms_db(tail(&loud));
        assert!((-24.0..-17.0).contains(&level), "{amp}: held at {level:.1} dBFS");
    }
}

#[test]
fn crunch_distorts_instead_of_compressing() {
    let sr = 48_000.0;
    let x = sine(1000.0, 0.01, 1.0, sr); // −40 dBFS
    let clean = run(DeluxeParams::default(), &x, sr);
    let crunched = run(DeluxeParams { crunch: 10.0, ..DeluxeParams::default() }, &x, sr);
    // 30 dB more into the curve, which starts to rail: the reference comes out
    // 26.7 dB louder here, with its third harmonic up near −14 dB
    let more = rms_db(tail(&crunched)) - rms_db(tail(&clean));
    assert!((24.0..29.0).contains(&more), "crunch raised the level by {more:.1} dB");
    let h3 = harmonic_db(tail(&crunched), 3, 1000.0, sr) - harmonic_db(tail(&crunched), 1, 1000.0, sr);
    assert!(h3 > -20.0, "third harmonic {h3:.1} dB");
}

/// Level of harmonic `k` of `f0` by direct correlation (the window holds
/// whole cycles).
fn harmonic_db(x: &[f64], k: u32, f0: f64, sr: f64) -> f64 {
    let w = 2.0 * std::f64::consts::PI * f0 * f64::from(k) / sr;
    let (re, im) = x.iter().enumerate().fold((0.0, 0.0), |(re, im), (i, v)| {
        let ph = w * count_to_f64(i);
        (re + v * ph.cos(), im + v * ph.sin())
    });
    20.0 * (re.hypot(im) * 2.0 / count_to_f64(x.len())).log10()
}

#[test]
fn darkness_takes_the_top_off() {
    let sr = 48_000.0;
    let gain = |hz: f64, dark: f64| {
        let x = sine(hz, 1e-3, 0.5, sr);
        rms_db(tail(&run(DeluxeParams { darkness_hz: dark, ..DeluxeParams::default() }, &x, sr))) - rms_db(tail(&x))
    };
    let open = gain(8000.0, 20_000.0) - gain(1000.0, 20_000.0);
    let dark = gain(8000.0, 2000.0) - gain(1000.0, 2000.0);
    assert!(dark < open - 15.0, "8 kHz re 1 kHz: open {open:.1} dB, dark {dark:.1} dB");
}

#[test]
fn stays_finite_at_every_rate_and_extreme() {
    for sr in [44_100.0, 48_000.0, 96_000.0] {
        // a loud, broadband, spiky signal
        let mut seed = 1u32;
        let noise: Vec<f64> = (0..48_000)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (f64::from(seed) / f64::from(u32::MAX) - 0.5) * 4.0
            })
            .collect();
        for (crush, crunch, dark, rel) in [(10.0, 10.0, 500.0, true), (10.0, 0.0, 20_000.0, false), (0.0, 10.0, 1100.0, true)] {
            let y = run(DeluxeParams { crush, crunch, darkness_hz: dark, mix: 100.0, release_on: rel }, &noise, sr);
            assert!(y.iter().all(|v| v.is_finite() && v.abs() < 4.0), "sr {sr} crush {crush} crunch {crunch} dark {dark}");
        }
    }
}
