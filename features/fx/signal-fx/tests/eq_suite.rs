//! Behavior tests for the integrated FTS-EQ block: static bands,
//! dynamic bands, per-band spectral mode, and transient dual-stream
//! mode — all toggles inside `signal.fx.eq`.

use signal_fx::NativeEq;
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
        let out_chunk = &mut out[start..start + chunk.len()];
        let out_chunk_r = &mut out_r[start..start + chunk.len()];
        plugin
            .process_block(chunk, chunk, out_chunk, out_chunk_r, &no_events())
            .unwrap();
    }
    out
}

fn rms(buf: &[f32]) -> f64 {
    (buf.iter()
        .map(|&x| f64::from(x) * f64::from(x))
        .sum::<f64>()
        / buf.len() as f64)
        .sqrt()
}

fn sine(freq: f64, amp: f64, n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| (amp * (core::f64::consts::TAU * freq * i as f64 / SR).sin()) as f32)
        .collect()
}

#[test]
fn dynamic_band_ducks_a_loud_band() {
    let mut eq = NativeEq::new(SR);
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
    let gentle = run_slope(2.0);
    let steep = run_slope(9.0);
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
fn spectral_band_toggle_suppresses_resonance_in_range_only() {
    // Band 1: 2 kHz bell, spectral on, −24 dB range. A screaming 2 kHz
    // resonance in a noise bed gets pulled down; the noise bed and
    // out-of-band content stay put.
    let mut eq = NativeEq::new(SR);
    eq.set_named("b1_used", 1.0);
    eq.set_named("b1_on", 1.0);
    eq.set_named("b1_freq", 2000.0);
    eq.set_named("b1_q", 1.0);
    eq.set_named("b1_dyn_range", -24.0);
    eq.set_named("b1_spectral", 1.0);
    assert!(
        eq.spectral_engaged(),
        "spectral toggle must engage the engine"
    );

    let mut seed = 5u64;
    let input: Vec<f32> = (0..N)
        .map(|i| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let noise = ((seed >> 33) as f64 / (1u64 << 31) as f64) - 1.0;
            (0.02 * noise
                + 0.5 * (core::f64::consts::TAU * 2000.0 * i as f64 / SR).sin()
                + 0.1 * (core::f64::consts::TAU * 6000.0 * i as f64 / SR).sin()) as f32
        })
        .collect();
    let out = run(&mut eq, &input);
    let tone_e = |buf: &[f32], freq: f64| -> f64 {
        let mut re = 0.0;
        let mut im = 0.0;
        for (i, &x) in buf.iter().enumerate() {
            let ph = core::f64::consts::TAU * freq * i as f64 / SR;
            re += f64::from(x) * ph.cos();
            im += f64::from(x) * ph.sin();
        }
        (re * re + im * im) / buf.len() as f64
    };
    let res = 10.0 * (tone_e(&out[N / 2..], 2000.0) / tone_e(&input[N / 2..], 2000.0)).log10();
    let far = 10.0 * (tone_e(&out[N / 2..], 6000.0) / tone_e(&input[N / 2..], 6000.0)).log10();
    assert!(res < -6.0, "in-band resonance suppressed: {res:.1} dB");
    assert!(far.abs() < 2.0, "out-of-band tone untouched: {far:.1} dB");
}

#[test]
fn transient_mode_null_and_stream_routing() {
    let input: Vec<f32> = (0..48_000)
        .map(|i| {
            let tone = 0.2 * (core::f64::consts::TAU * 220.0 * i as f64 / SR).sin();
            let click = if i % 12000 < 48 { 0.7 } else { 0.0 };
            (tone + click) as f32
        })
        .collect();

    // Transient mode on, everything flat: null.
    let mut eq = NativeEq::new(SR);
    eq.set_named("transient_mode", 1.0);
    let out = run(&mut eq, &input);
    let mut max_err = 0.0f64;
    for i in 0..input.len() {
        max_err = max_err.max((f64::from(out[i]) - f64::from(input[i])).abs());
    }
    assert!(
        max_err < 1.0e-4,
        "flat transient mode must null: {max_err:e}"
    );

    // Steady stream cut 30 dB: tone drops, clicks survive.
    let mut eq2 = NativeEq::new(SR);
    eq2.set_named("transient_mode", 1.0);
    eq2.set_named("steady_gain", -30.0);
    let out2 = run(&mut eq2, &input);
    let tone_idx: Vec<usize> = (24_000..48_000).filter(|i| i % 12000 > 6000).collect();
    let t_in = (tone_idx
        .iter()
        .map(|&i| f64::from(input[i]).powi(2))
        .sum::<f64>()
        / tone_idx.len() as f64)
        .sqrt();
    let t_out = (tone_idx
        .iter()
        .map(|&i| f64::from(out2[i]).powi(2))
        .sum::<f64>()
        / tone_idx.len() as f64)
        .sqrt();
    let tone_g = 20.0 * (t_out / t_in).log10();
    assert!(tone_g < -20.0, "steady stream should drop: {tone_g:.1} dB");
    let click_peak = (24_000..48_000)
        .filter(|i| i % 12000 < 48)
        .map(|i| f64::from(out2[i]).abs())
        .fold(0.0f64, f64::max);
    assert!(click_peak > 0.3, "clicks must survive: {click_peak:.2}");

    // Band assigned to the steady stream only: a bell boost at 220 Hz
    // brightens the tone without touching the transient stream.
    let mut eq3 = NativeEq::new(SR);
    eq3.set_named("transient_mode", 1.0);
    eq3.set_named("b1_used", 1.0);
    eq3.set_named("b1_on", 1.0);
    eq3.set_named("b1_freq", 220.0);
    eq3.set_named("b1_q", 1.0);
    eq3.set_named("b1_gain", 6.0);
    eq3.set_named("b1_stream", 2.0); // steady only
    let out3 = run(&mut eq3, &input);
    let t3 = (tone_idx
        .iter()
        .map(|&i| f64::from(out3[i]).powi(2))
        .sum::<f64>()
        / tone_idx.len() as f64)
        .sqrt();
    let tone_boost = 20.0 * (t3 / t_in).log10();
    assert!(
        (tone_boost - 6.0).abs() < 1.5,
        "steady-stream band should boost the held tone: {tone_boost:.1} dB"
    );
}

#[test]
fn idle_eq_is_a_bit_exact_copy() {
    // Nothing enabled: the block must take the copy path — output is
    // BIT-exact, proving no DSP ran.
    let mut eq = NativeEq::new(SR);
    let input = sine(997.0, 0.37, 24_000);
    let out = run(&mut eq, &input);
    assert_eq!(out, input, "idle EQ must be a bit-exact passthrough");

    // Toggle a feature on, then fully off again: back on the copy path.
    let mut eq2 = NativeEq::new(SR);
    eq2.set_named("b1_used", 1.0);
    eq2.set_named("b1_on", 1.0);
    eq2.set_named("b1_gain", 6.0);
    eq2.set_named("transient_mode", 1.0);
    eq2.set_named("b1_spectral", 1.0);
    eq2.set_named("b1_dyn_range", -12.0);
    // ...and off.
    eq2.set_named("b1_used", 0.0);
    eq2.set_named("transient_mode", 0.0);
    eq2.set_named("b1_spectral", 0.0);
    eq2.set_named("b1_dyn_range", 0.0);
    let out2 = run(&mut eq2, &input);
    assert_eq!(
        out2, input,
        "after disabling every feature the EQ must return to the copy path"
    );
}

#[test]
fn side_of_transients_high_shelf_combo() {
    // THE combo: transient mode + a high-shelf band assigned to the
    // TRANSIENT stream with SIDE placement, boosting +9 dB.
    // Signal: mono steady tone (pure mid) + wide high-frequency clicks
    // (pure side: L = +click, R = −click). Only the clicks' side
    // content may grow; the mono tone must stay bit-identical in mid.
    let mut eq = NativeEq::new(SR);
    eq.set_named("transient_mode", 1.0);
    eq.set_named("b1_used", 1.0);
    eq.set_named("b1_on", 1.0);
    eq.set_named("b1_shape", 2.0); // HighShelf (canonical)
    eq.set_named("b1_freq", 3000.0);
    eq.set_named("b1_q", 0.707);
    eq.set_named("b1_gain", 9.0);
    eq.set_named("b1_stream", 1.0); // transient stream only
    eq.set_named("b1_placement", 4.0); // Side

    let n = 96_000;
    let mut in_l = vec![0.0f32; n];
    let mut in_r = vec![0.0f32; n];
    for i in 0..n {
        let tone = 0.2 * (core::f64::consts::TAU * 220.0 * i as f64 / SR).sin();
        // 8 kHz click bursts, opposite polarity L/R → pure side.
        let click = if i % 12000 < 96 {
            0.4 * (core::f64::consts::TAU * 8000.0 * i as f64 / SR).sin()
        } else {
            0.0
        };
        in_l[i] = (tone + click) as f32;
        in_r[i] = (tone - click) as f32;
    }

    eq.prepare(SR, 512).unwrap();
    let mut out_l = vec![0.0f32; n];
    let mut out_r = vec![0.0f32; n];
    for (i, chunk) in in_l.chunks(512).enumerate() {
        let start = i * 512;
        eq.process_block(
            chunk,
            &in_r[start..start + chunk.len()],
            &mut out_l[start..start + chunk.len()],
            &mut out_r[start..start + chunk.len()],
            &no_events(),
        )
        .unwrap();
    }

    // Side energy during clicks must rise ≈ +9 dB.
    let side = |l: &[f32], r: &[f32], i: usize| 0.5 * (f64::from(l[i]) - f64::from(r[i]));
    let mid = |l: &[f32], r: &[f32], i: usize| 0.5 * (f64::from(l[i]) + f64::from(r[i]));
    let click_idx: Vec<usize> = (24_000..n).filter(|i| i % 12000 < 96).collect();
    let tone_idx: Vec<usize> = (24_000..n).filter(|i| i % 12000 > 6000).collect();
    let e = |f: &dyn Fn(usize) -> f64, idx: &[usize]| -> f64 {
        (idx.iter().map(|&i| f(i).powi(2)).sum::<f64>() / idx.len() as f64).sqrt()
    };
    let side_in = e(&|i| side(&in_l, &in_r, i), &click_idx);
    let side_out = e(&|i| side(&out_l, &out_r, i), &click_idx);
    let side_gain = 20.0 * (side_out / side_in).log10();
    assert!(
        side_gain > 6.0,
        "side content of high transients should be boosted: {side_gain:+.1} dB"
    );

    // Mono tone (mid, steady) untouched.
    let mid_in = e(&|i| mid(&in_l, &in_r, i), &tone_idx);
    let mid_out = e(&|i| mid(&out_l, &out_r, i), &tone_idx);
    let mid_gain = 20.0 * (mid_out / mid_in).log10();
    assert!(
        mid_gain.abs() < 0.5,
        "mono steady tone must pass untouched: {mid_gain:+.2} dB"
    );
}

#[test]
fn mid_placement_dynamic_band_ducks_center_only() {
    // Dynamic band on MID placement: loud mono content ducks, side
    // content sails through — "duck just the center".
    let mut eq = NativeEq::new(SR);
    eq.set_named("b1_used", 1.0);
    eq.set_named("b1_on", 1.0);
    eq.set_named("b1_freq", 1000.0);
    eq.set_named("b1_q", 1.0);
    eq.set_named("b1_dyn_range", -12.0);
    eq.set_named("b1_dyn_auto", 0.0);
    eq.set_named("b1_dyn_thr", -20.0);
    eq.set_named("b1_dyn_atk", 20.0);
    eq.set_named("b1_placement", 3.0); // Mid

    let n = 96_000;
    let mut in_l = vec![0.0f32; n];
    let mut in_r = vec![0.0f32; n];
    for i in 0..n {
        // Loud mono 1 kHz + quiet wide 1.5 kHz (opposite polarity).
        let mono = 0.5 * (core::f64::consts::TAU * 1000.0 * i as f64 / SR).sin();
        let wide = 0.05 * (core::f64::consts::TAU * 1500.0 * i as f64 / SR).sin();
        in_l[i] = (mono + wide) as f32;
        in_r[i] = (mono - wide) as f32;
    }
    eq.prepare(SR, 512).unwrap();
    let mut out_l = vec![0.0f32; n];
    let mut out_r = vec![0.0f32; n];
    for (i, chunk) in in_l.chunks(512).enumerate() {
        let start = i * 512;
        eq.process_block(
            chunk,
            &in_r[start..start + chunk.len()],
            &mut out_l[start..start + chunk.len()],
            &mut out_r[start..start + chunk.len()],
            &no_events(),
        )
        .unwrap();
    }
    let mid_e = |l: &[f32], r: &[f32]| -> f64 {
        (24_000..n)
            .map(|i| (0.5 * (f64::from(l[i]) + f64::from(r[i]))).powi(2))
            .sum::<f64>()
    };
    let side_e = |l: &[f32], r: &[f32]| -> f64 {
        (24_000..n)
            .map(|i| (0.5 * (f64::from(l[i]) - f64::from(r[i]))).powi(2))
            .sum::<f64>()
    };
    let mid_g = 10.0 * (mid_e(&out_l, &out_r) / mid_e(&in_l, &in_r)).log10();
    let side_g = 10.0 * (side_e(&out_l, &out_r) / side_e(&in_l, &in_r)).log10();
    assert!(mid_g < -6.0, "loud center should duck: {mid_g:+.1} dB");
    assert!(
        side_g.abs() < 1.0,
        "side content untouched: {side_g:+.2} dB"
    );
}

#[test]
fn cut_slopes_audit_and_zero_slope_bypass() {
    // Low cut at 1 kHz, 200 Hz probe: attenuation must increase
    // monotonically with the slope index, and slope 0 = TRUE bypass.
    let run_cut = |shape: f64, slope: f64, probe: f64| -> f64 {
        let mut eq = NativeEq::new(SR);
        eq.set_named("b1_used", 1.0);
        eq.set_named("b1_on", 1.0);
        eq.set_named("b1_shape", shape);
        eq.set_named("b1_freq", 1000.0);
        eq.set_named("b1_q", 0.707);
        eq.set_named("b1_slope", slope);
        let input = sine(probe, 0.3, 48_000);
        let out = run(&mut eq, &input);
        20.0 * (rms(&out[24_000..]) / rms(&input[24_000..])).log10()
    };
    // LowCut (canonical 3): slope 0 must be bit-flat (bypass).
    let bypass = run_cut(3.0, 0.0, 200.0);
    assert!(
        bypass.abs() < 0.01,
        "0 dB/oct low cut = bypass: {bypass:+.3} dB"
    );
    // NOTE: slope index 1 (6 dB/oct, order 1) currently designs to
    // passthrough — the odd-order-1 HP path gap is tracked in #73.
    // Probe half an octave below the cutoff so even 96 dB/oct stays
    // above the numeric floor and the ladder stays measurable.
    let mut prev = 0.0;
    for slope in [2.0, 4.0, 7.0, 9.0] {
        let g = run_cut(3.0, slope, 700.0);
        assert!(
            g < prev - 1.5,
            "low-cut slope {slope} must attenuate more than the previous: {g:+.1} vs {prev:+.1}"
        );
        prev = g;
    }
    // HighCut (canonical 4) mirrored with an 8 kHz probe.
    let hc_gentle = run_cut(4.0, 2.0, 8000.0);
    let hc_steep = run_cut(4.0, 9.0, 8000.0);
    assert!(hc_gentle < -10.0, "12 dB/oct high cut: {hc_gentle:+.1}");
    assert!(
        hc_steep < hc_gentle - 20.0,
        "96 dB/oct high cut far steeper: {hc_steep:+.1}"
    );
}

#[test]
fn listen_solo_isolates_the_band_region() {
    // Bell at 2 kHz, listen=solo: a 2 kHz tone passes, a 200 Hz tone
    // mostly vanishes.
    let run_probe = |probe: f64| -> f64 {
        let mut eq = NativeEq::new(SR);
        eq.set_named("b1_used", 1.0);
        eq.set_named("b1_on", 1.0);
        eq.set_named("b1_freq", 2000.0);
        eq.set_named("b1_q", 2.0);
        eq.set_named("b1_gain", 3.0);
        eq.set_named("b1_listen", 1.0);
        let input = sine(probe, 0.3, 48_000);
        let out = run(&mut eq, &input);
        20.0 * (rms(&out[24_000..]) / rms(&input[24_000..])).log10()
    };
    let in_band = run_probe(2000.0);
    let below = run_probe(200.0);
    assert!(
        in_band > -2.0,
        "in-region content passes solo: {in_band:+.1} dB"
    );
    assert!(
        below < in_band - 20.0,
        "out-of-region content vanishes: {below:+.1} dB"
    );
}

#[test]
fn listen_delta_hears_only_the_change() {
    // +6 dB bell at 1 kHz, delta listen: with a 1 kHz tone the delta
    // carries signal; with an out-of-band 6 kHz tone it is near
    // silence (the EQ changes nothing there).
    let run_probe = |probe: f64| -> f64 {
        let mut eq = NativeEq::new(SR);
        eq.set_named("b1_used", 1.0);
        eq.set_named("b1_on", 1.0);
        eq.set_named("b1_freq", 1000.0);
        eq.set_named("b1_q", 2.0);
        eq.set_named("b1_gain", 6.0);
        eq.set_named("b1_listen", 2.0);
        let input = sine(probe, 0.3, 48_000);
        let out = run(&mut eq, &input);
        20.0 * (rms(&out[24_000..]) / rms(&input[24_000..])).log10()
    };
    let changed = run_probe(1000.0);
    let unchanged = run_probe(6000.0);
    assert!(
        changed > -8.0,
        "delta carries the boosted region: {changed:+.1} dB"
    );
    // Far out-of-band the bell's real skirt is tiny but nonzero — the
    // delta must sit well below the in-band delta (here it IS the
    // skirt, ~-22 dB: physically correct, not leakage).
    assert!(
        unchanged < changed - 12.0,
        "delta far out of band stays well below in-band: {unchanged:+.1} vs {changed:+.1} dB"
    );
}

#[test]
fn listen_delta_reveals_spectral_action() {
    // Spectral band suppressing a 2 kHz resonance: delta listen must
    // contain mostly the resonance (what the spectral engine removes),
    // latency-aligned so the noise bed cancels.
    let mut eq = NativeEq::new(SR);
    eq.set_named("b1_used", 1.0);
    eq.set_named("b1_on", 1.0);
    eq.set_named("b1_freq", 2000.0);
    eq.set_named("b1_q", 1.0);
    eq.set_named("b1_dyn_range", -24.0);
    eq.set_named("b1_spectral", 1.0);
    eq.set_named("b1_listen", 2.0);
    let mut seed = 5u64;
    let input: Vec<f32> = (0..N)
        .map(|i| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let noise = ((seed >> 33) as f64 / (1u64 << 31) as f64) - 1.0;
            (0.05 * noise + 0.5 * (core::f64::consts::TAU * 2000.0 * i as f64 / SR).sin()) as f32
        })
        .collect();
    let out = run(&mut eq, &input);
    // The delta's energy should be dominated by 2 kHz content.
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
    let total_e: f64 = out[N / 2..]
        .iter()
        .map(|&x| f64::from(x) * f64::from(x))
        .sum::<f64>()
        / (N / 2) as f64;
    let res_e = tone_e(&out[N / 2..]);
    assert!(
        total_e > 1.0e-8,
        "delta must carry the removed resonance: {total_e:e}"
    );
    assert!(
        res_e > total_e * 0.5,
        "the delta should be mostly the 2 kHz resonance: res={res_e:e} total={total_e:e}"
    );
}
