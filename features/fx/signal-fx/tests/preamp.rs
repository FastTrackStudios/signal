//! Class-A preamp block: the Q-point demo, measured.

use signal_fx::{NativePreamp, PREAMP_HARMONIC_BASE};
use signal_plugin_host::{PluginEvents, PluginInstance};

const SR: f64 = 48000.0;

fn no_events() -> PluginEvents<'static> {
    PluginEvents {
        params: &[],
        midi: &[],
        note_expressions: &[],
    }
}

/// Harmonic energy of `buf` at k×f0.
fn harmonic(buf: &[f32], f0: f64, k: usize) -> f64 {
    let (mut re, mut im) = (0.0f64, 0.0f64);
    for (i, &x) in buf.iter().enumerate() {
        let ph = core::f64::consts::TAU * f0 * k as f64 * i as f64 / SR;
        re += f64::from(x) * ph.cos();
        im += f64::from(x) * ph.sin();
    }
    (re * re + im * im).sqrt() / buf.len() as f64
}

fn run_100hz(q_point: f64) -> Vec<f32> {
    let mut p = NativePreamp::new(SR);
    p.set_named("drive", 12.0);
    p.set_named("pos_shaper", 1.0); // Op-Amp both sides = symmetric
    p.set_named("neg_shaper", 1.0);
    p.set_named("q_point", q_point);
    p.prepare(SR, 512).unwrap();
    let n = 96_000;
    let input: Vec<f32> = (0..n)
        .map(|i| (0.5 * (core::f64::consts::TAU * 100.0 * i as f64 / SR).sin()) as f32)
        .collect();
    let mut out_l = vec![0.0f32; n];
    let mut out_r = vec![0.0f32; n];
    for (i, chunk) in input.chunks(512).enumerate() {
        let s = i * 512;
        p.process_block(
            chunk,
            chunk,
            &mut out_l[s..s + chunk.len()],
            &mut out_r[s..s + chunk.len()],
            &no_events(),
        )
        .unwrap();
    }
    out_l
}

#[test]
fn q_point_springs_even_harmonics_100hz_demo() {
    // Fundamental 100 Hz. Centered Q: 300/500 Hz (odd) only.
    let sym = run_100hz(0.0);
    let late = &sym[48_000..];
    let h1 = harmonic(late, 100.0, 1);
    let h2_sym = harmonic(late, 100.0, 2) / h1;
    let h3_sym = harmonic(late, 100.0, 3) / h1;
    assert!(h3_sym > 0.01, "3rd present when driven: {h3_sym:.4}");
    assert!(
        h2_sym < h3_sym * 0.05,
        "2nd buried when symmetric: h2={h2_sym:.5} h3={h3_sym:.5}"
    );

    // Raise the Q point: 200/400/600 Hz spring into life.
    let asym = run_100hz(0.5);
    let late = &asym[48_000..];
    let h1 = harmonic(late, 100.0, 1);
    let h2 = harmonic(late, 100.0, 2) / h1;
    let h4 = harmonic(late, 100.0, 4) / h1;
    assert!(
        h2 > h2_sym * 20.0,
        "2nd springs up with bias: {h2:.4} vs {h2_sym:.5}"
    );
    assert!(h4 > 1.0e-3, "4th follows: {h4:.5}");

    // DC stays inside the box.
    let mean: f64 = late.iter().map(|&x| f64::from(x)).sum::<f64>() / late.len() as f64;
    assert!(mean.abs() < 1.0e-3, "DC blocked at the output: {mean:e}");
}

#[test]
fn harmonic_readback_matches_the_visualization_contract() {
    let mut p = NativePreamp::new(SR);
    p.set_named("drive", 12.0);
    p.set_named("pos_shaper", 1.0);
    p.set_named("neg_shaper", 1.0);
    p.set_named("q_point", 0.5);
    let h1 = p.param_value(PREAMP_HARMONIC_BASE).unwrap();
    let h2 = p.param_value(PREAMP_HARMONIC_BASE + 1).unwrap();
    let h3 = p.param_value(PREAMP_HARMONIC_BASE + 2).unwrap();
    assert!((h1 - 1.0).abs() < 1.0e-6, "H1 normalized to 1: {h1}");
    assert!(h2 > 0.01, "readback shows the 2nd harmonic: {h2:.4}");
    assert!(h3 > 0.001, "and the 3rd: {h3:.4}");

    // Back to symmetric: H2 collapses in the readback too.
    p.set_named("q_point", 0.0);
    let h2_sym = p.param_value(PREAMP_HARMONIC_BASE + 1).unwrap();
    assert!(
        h2_sym < h2 * 0.05,
        "readback tracks setting changes: {h2_sym:.5} vs {h2:.4}"
    );
}
