//! Behavior tests for the EQ-suite param surfaces: dynamic bands on
//! NativeEq, the spectral shaper, and the transient EQ.

use signal_fx::{NativeEq, NativeSpectral, NativeTransientEq};
use signal_plugin_host::{PluginEvents, PluginInstance};

const SR: f64 = 48000.0;
const N: usize = 96_000;

fn no_events() -> PluginEvents<'static> {
    PluginEvents {
        params: &[],
        midi: &[],
        note_expressions: &[],
    }
}

fn run(plugin: &mut dyn PluginInstance, input: &[f32]) -> Vec<f32> {
    plugin.prepare(SR, 512).unwrap();
    let mut out = vec![0.0f32; input.len()];
    let mut out_r = vec![0.0f32; input.len()];
    for (i, chunk) in input.chunks(512).enumerate() {
        let start = i * 512;
        let (l, rest) = out.split_at_mut(start);
        let _ = l;
        let out_chunk = &mut rest[..chunk.len()];
        let (r0, r_rest) = out_r.split_at_mut(start);
        let _ = r0;
        let out_chunk_r = &mut r_rest[..chunk.len()];
        plugin
            .process_block(chunk, chunk, out_chunk, out_chunk_r, &no_events())
            .unwrap();
    }
    out
}

fn rms(buf: &[f32]) -> f64 {
    (buf.iter().map(|&x| f64::from(x) * f64::from(x)).sum::<f64>() / buf.len() as f64).sqrt()
}

fn sine(freq: f64, amp: f64, n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| (amp * (core::f64::consts::TAU * freq * i as f64 / SR).sin()) as f32)
        .collect()
}

#[test]
fn dynamic_band_ducks_a_loud_band() {
    let mut eq = NativeEq::new(SR);
    // Band 1: 1 kHz bell, no static gain, −12 dB dynamic range,
    // manual threshold −20 dB.
    eq.set_named("b1_used", 1.0);
    eq.set_named("b1_on", 1.0);
    eq.set_named("b1_freq", 1000.0);
    eq.set_named("b1_q", 1.0);
    eq.set_named("b1_shape", 0.0);
    eq.set_named("b1_dyn_range", -12.0);
    eq.set_named("b1_dyn_auto", 0.0);
    eq.set_named("b1_dyn_thr", -20.0);
    eq.set_named("b1_dyn_atk", 20.0);

    let loud = sine(1000.0, 0.5, N);
    let out = run(&mut eq, &loud);
    let gr_db = 20.0 * (rms(&out[N / 2..]) / rms(&loud[N / 2..])).log10();
    assert!(gr_db < -6.0, "loud 1 kHz should be ducked: {gr_db:.1} dB");

    // Quiet input passes at unity.
    let mut eq2 = NativeEq::new(SR);
    eq2.set_named("b1_used", 1.0);
    eq2.set_named("b1_on", 1.0);
    eq2.set_named("b1_freq", 1000.0);
    eq2.set_named("b1_dyn_range", -12.0);
    eq2.set_named("b1_dyn_auto", 0.0);
    eq2.set_named("b1_dyn_thr", -20.0);
    let quiet = sine(1000.0, 0.004, N);
    let out2 = run(&mut eq2, &quiet);
    let g2 = 20.0 * (rms(&out2[N / 2..]) / rms(&quiet[N / 2..])).log10();
    assert!(g2.abs() < 1.0, "quiet input passes: {g2:.2} dB");
}

#[test]
fn slope_param_steepens_a_cut() {
    // 200 Hz sine through a 1 kHz low cut: 96 dB/oct attenuates far
    // more than 12 dB/oct.
    let run_slope = |slope: f64| -> f64 {
        let mut eq = NativeEq::new(SR);
        eq.set_named("b1_used", 1.0);
        eq.set_named("b1_on", 1.0);
        eq.set_named("b1_shape", 3.0); // LowCut (canonical)
        eq.set_named("b1_freq", 1000.0);
        eq.set_named("b1_q", 0.707);
        eq.set_named("b1_slope", slope);
        let input = sine(200.0, 0.3, 48_000);
        let out = run(&mut eq, &input);
        20.0 * (rms(&out[24_000..]) / rms(&input[24_000..])).log10()
    };
    let gentle = run_slope(2.0); // 12 dB/oct
    let steep = run_slope(9.0); // 96 dB/oct
    assert!(gentle < -10.0, "12 dB/oct cuts some: {gentle:.1}");
    assert!(
        steep < gentle - 20.0,
        "96 dB/oct must be far steeper: steep={steep:.1} gentle={gentle:.1}"
    );
}

#[test]
fn output_gain_applies() {
    let mut eq = NativeEq::new(SR);
    eq.set_named("output_gain", -6.0);
    let input = sine(500.0, 0.3, 24_000);
    let out = run(&mut eq, &input);
    let g = 20.0 * (rms(&out[12_000..]) / rms(&input[12_000..])).log10();
    assert!((g + 6.0).abs() < 0.3, "output gain: {g:.2} dB");
}

#[test]
fn spectral_block_suppresses_resonance() {
    let mut sp = NativeSpectral::new(SR);
    sp.set_named("amount", 1.0);
    sp.set_named("threshold", 3.0);
    // Noise bed + strong 2 kHz resonance.
    let mut seed = 5u64;
    let input: Vec<f32> = (0..N)
        .map(|i| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let noise = ((seed >> 33) as f64 / (1u64 << 31) as f64) - 1.0;
            (0.02 * noise + 0.5 * (core::f64::consts::TAU * 2000.0 * i as f64 / SR).sin()) as f32
        })
        .collect();
    let out = run(&mut sp, &input);
    // Correlate against 2 kHz on the late half.
    let tone_e = |buf: &[f32]| -> f64 {
        let mut re = 0.0;
        let mut im = 0.0;
        for (i, &x) in buf.iter().enumerate() {
            let ph = core::f64::consts::TAU * 2000.0 * i as f64 / SR;
            re += f64::from(x) * ph.cos();
            im += f64::from(x) * ph.sin();
        }
        (re * re + im * im) / buf.len() as f64
    };
    let red = 10.0 * (tone_e(&out[N / 2..]) / tone_e(&input[N / 2..])).log10();
    assert!(red < -6.0, "resonance should be suppressed: {red:.1} dB");
}

#[test]
fn transient_eq_null_and_split_gains() {
    // Flat: null.
    let mut teq = NativeTransientEq::new(SR);
    let input: Vec<f32> = (0..48_000)
        .map(|i| {
            let tone = 0.2 * (core::f64::consts::TAU * 220.0 * i as f64 / SR).sin();
            let click = if i % 12000 < 48 { 0.7 } else { 0.0 };
            (tone + click) as f32
        })
        .collect();
    let out = run(&mut teq, &input);
    let mut max_err = 0.0f64;
    for i in 0..input.len() {
        max_err = max_err.max((f64::from(out[i]) - f64::from(input[i])).abs());
    }
    assert!(max_err < 1.0e-4, "flat transient EQ must null: {max_err:e}");

    // Steady stream cut by 30 dB: held tone drops, clicks survive.
    let mut teq2 = NativeTransientEq::new(SR);
    teq2.set_named("steady_gain", -30.0);
    let out2 = run(&mut teq2, &input);
    let tone_idx: Vec<usize> = (24_000..48_000).filter(|i| i % 12000 > 6000).collect();
    let tone_in = (tone_idx.iter().map(|&i| f64::from(input[i]).powi(2)).sum::<f64>()
        / tone_idx.len() as f64)
        .sqrt();
    let tone_out = (tone_idx.iter().map(|&i| f64::from(out2[i]).powi(2)).sum::<f64>()
        / tone_idx.len() as f64)
        .sqrt();
    let tone_g = 20.0 * (tone_out / tone_in).log10();
    assert!(tone_g < -20.0, "steady stream should drop: {tone_g:.1} dB");
    let click_peak = (24_000..48_000)
        .filter(|i| i % 12000 < 48)
        .map(|i| f64::from(out2[i]).abs())
        .fold(0.0f64, f64::max);
    assert!(click_peak > 0.3, "clicks must survive: {click_peak:.2}");
}
