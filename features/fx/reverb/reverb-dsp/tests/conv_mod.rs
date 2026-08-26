//! Convolution modulation options: default transparency, motion,
//! mod sources (wet duck, predelay), and dual-IR morph.

use std::f64::consts::PI;

use reverb_dsp::algorithm::{ConvolutionModParams, IrSlot, ReverbAlgorithm};
use reverb_dsp::algorithms::convolution::Convolution;

const SR: f64 = 48000.0;

/// Deterministic probe input: impulse + two sines (matches the
/// pre-change baseline capture exactly).
fn probe_input(i: usize) -> f64 {
    let t = i as f64 / SR;
    (if i == 0 { 1.0 } else { 0.0 })
        + 0.25 * (2.0 * PI * 440.0 * t).sin()
        + 0.1 * (2.0 * PI * 1337.0 * t).sin()
}

/// All mod params at defaults must reproduce the pre-modulation
/// convolver bit-for-bit. Expected values were captured from the
/// implementation BEFORE the modulation options landed (same seed,
/// same FFT path).
#[test]
fn defaults_are_bit_transparent() {
    // (index, left, right) probes + total energy captured pre-change.
    const EXPECTED_ENERGY: f64 = 7.490_316_960_892_111e2;
    const EXPECTED: [(usize, f64, f64); 10] = [
        (9000, 3.551_274_578_495_904_3e-1, 1.523_937_982_014_087_2e-1),
        (0, 0.0, 0.0),
        (1000, -1.303_661_719_155_252_6e-2, -4.190_476_673_827_804e-3),
        (2000, 1.192_044_793_109_639_9e-1, -2.526_242_376_635_289e-1),
        (3000, 6.060_384_060_909_755e-2, -5.617_114_973_418_321e-1),
        (4000, 1.246_689_431_522_149e-2, -6.038_074_718_058_465e-2),
        (5000, -5.518_698_769_813_348e-1, 3.609_453_731_800_272_3e-1),
        (
            6000,
            -3.108_835_884_884_094_7e-1,
            1.946_006_050_124_746_4e-1,
        ),
        (7000, 2.027_685_565_699_985_4e-1, 1.139_684_129_683_064_8e-1),
        (8000, 4.649_203_732_518_243e-1, 9.377_744_615_108_695e-2),
    ];

    let mut c = Convolution::new(SR);
    // Defaults pushed explicitly through the trait — same as the chain does.
    c.set_conv_mod_params(&ConvolutionModParams::default(), true);

    let mut energy = 0.0f64;
    let mut probes = Vec::new();
    for i in 0..9600usize {
        let x = probe_input(i);
        let (l, r) = c.tick(x, x * 0.8);
        energy += l * l + r * r;
        if i % 1000 == 0 {
            probes.push((i, l, r));
        }
    }

    assert!(
        (energy - EXPECTED_ENERGY).abs() < 1e-9,
        "default-path energy drifted: {energy} vs {EXPECTED_ENERGY}"
    );
    for (idx, el, er) in EXPECTED {
        let (_, l, r) = probes[idx / 1000];
        assert!(
            (l - el).abs() < 1e-12 && (r - er).abs() < 1e-12,
            "default path not bit-transparent at {idx}: ({l}, {r}) vs ({el}, {er})"
        );
    }
}

/// Render `seconds` of output for a given config mutation.
fn render(setup: impl Fn(&mut ConvolutionModParams), seconds: f64) -> (Vec<f64>, Vec<f64>) {
    let mut c = Convolution::new(SR);
    let mut p = ConvolutionModParams::default();
    setup(&mut p);
    c.set_conv_mod_params(&p, true);
    let n = (SR * seconds) as usize;
    let mut out_l = Vec::with_capacity(n);
    let mut out_r = Vec::with_capacity(n);
    for i in 0..n {
        let x = probe_input(i);
        let (l, r) = c.tick(x, x);
        out_l.push(l);
        out_r.push(r);
    }
    (out_l, out_r)
}

/// Motion on: output differs from motion off, stays finite, and the
/// *difference* evolves over time (it's modulation, not a static filter).
#[test]
fn motion_moves_and_is_finite() {
    let (base_l, _) = render(|_| {}, 2.0);
    let (mov_l, mov_r) = render(
        |p| {
            p.motion_depth = 0.7;
            p.motion_rate = 1.0;
        },
        2.0,
    );

    for (i, (&l, &r)) in mov_l.iter().zip(mov_r.iter()).enumerate() {
        assert!(l.is_finite() && r.is_finite(), "NaN at {i}");
    }

    // Differs from bypass at all…
    let diff: f64 = base_l
        .iter()
        .zip(mov_l.iter())
        .map(|(a, b)| (a - b).abs())
        .sum();
    assert!(diff > 1e-3, "motion should alter the wet signal: {diff}");

    // …and the alteration itself moves: compare the base→motion delta over
    // two windows a second apart against the same input phase. The input
    // sines (440/1337 Hz) repeat every 48000 samples, so identical windows
    // of a STATIC filter would produce identical deltas.
    let w = 4800;
    let d1: f64 = (24000..24000 + w)
        .map(|i| (base_l[i] - mov_l[i]).abs())
        .sum();
    let d2: f64 = (72000..72000 + w)
        .map(|i| (base_l[i] - mov_l[i]).abs())
        .sum();
    let ratio = (d1 - d2).abs() / d1.max(1e-12);
    assert!(
        ratio > 0.01,
        "motion delta should evolve over time (got ratio {ratio})"
    );
}

/// Envelope ducking: wet output during a loud burst is quieter with
/// duck_wet_depth engaged than without.
#[test]
fn duck_reduces_wet_during_burst() {
    let run = |duck: f64| -> f64 {
        let mut c = Convolution::new(SR);
        let p = ConvolutionModParams {
            duck_wet_depth: duck,
            ..Default::default()
        };
        c.set_conv_mod_params(&p, true);
        // Prime the tail with an impulse, then a loud sustained burst.
        let n = (SR * 1.5) as usize;
        let mut burst_energy = 0.0;
        for i in 0..n {
            let x = if i < 100 {
                0.8
            } else if i > 24000 {
                0.9 * (2.0 * PI * 220.0 * i as f64 / SR).sin()
            } else {
                0.0
            };
            let (l, r) = c.tick(x, x);
            if i > 30000 {
                burst_energy += l * l + r * r;
            }
        }
        burst_energy
    };

    let no_duck = run(0.0);
    let ducked = run(1.0);
    assert!(
        ducked < no_duck * 0.7,
        "ducking should reduce wet during input: ducked={ducked}, free={no_duck}"
    );
}

/// Base predelay shifts the convolution's arrival time.
#[test]
fn predelay_shifts_arrival() {
    let arrival = |pd_ms: f64| -> usize {
        let mut c = Convolution::new(SR);
        // Distinct, known IR so arrival is unambiguous.
        let ir: Vec<f64> = (0..256).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();
        c.load_ir_stereo(&ir, &ir);
        let p = ConvolutionModParams {
            predelay_ms: pd_ms,
            ..Default::default()
        };
        c.set_conv_mod_params(&p, true);
        for i in 0..48000 {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let (l, _) = c.tick(x, x);
            if l.abs() > 1e-3 {
                return i;
            }
        }
        usize::MAX
    };

    let base = arrival(0.0);
    let delayed = arrival(50.0);
    let shift = delayed as i64 - base as i64;
    let expect = (0.050 * SR) as i64;
    assert!(
        (shift - expect).abs() < 256,
        "50 ms predelay should shift arrival ~{expect} samples, got {shift} (base {base}, delayed {delayed})"
    );
}

/// LFO on predelay: modulated read produces pitch wobble — outputs at
/// identical input phases differ, and everything stays finite.
#[test]
fn predelay_modulation_wobbles() {
    let (static_l, _) = render(|p| p.predelay_ms = 40.0, 2.0);
    let (wobble_l, _) = render(
        |p| {
            p.predelay_ms = 40.0;
            p.mod_predelay_depth = 0.8;
            p.lfo_rate = 1.0;
        },
        2.0,
    );
    let diff: f64 = static_l
        .iter()
        .zip(wobble_l.iter())
        .skip(4800)
        .map(|(a, b)| (a - b).abs())
        .sum();
    assert!(diff > 1e-2, "predelay LFO should modulate output: {diff}");
    assert!(wobble_l.iter().all(|x| x.is_finite()));
}

/// Morph at 0.5 mixes energy from both IR slots. Loads two disjoint
/// single-tap IRs (A at 0 samples, B at 4000 samples) so each slot's
/// contribution is separable in time.
#[test]
fn morph_blends_both_slots() {
    let ir_a: Vec<f64> = (0..8000).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();
    let ir_b: Vec<f64> = (0..8000)
        .map(|i| if i == 4000 { 1.0 } else { 0.0 })
        .collect();

    let run = |morph: f64| -> (f64, f64) {
        let mut c = Convolution::new(SR);
        c.load_ir_stereo_slot(&ir_a, &ir_a, IrSlot::A);
        c.load_ir_stereo_slot(&ir_b, &ir_b, IrSlot::B);
        let p = ConvolutionModParams {
            morph,
            ..Default::default()
        };
        c.set_conv_mod_params(&p, true);
        let mut early = 0.0; // A's tap window (0..2000 + block latency)
        let mut late = 0.0; // B's tap window (4000..6000 + block latency)
        for i in 0..12000usize {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let (l, _) = c.tick(x, x);
            if i < 3000 {
                early += l * l;
            } else if (4000..8000).contains(&i) {
                late += l * l;
            }
        }
        (early, late)
    };

    let (early_a, late_a) = run(0.0);
    assert!(early_a > 1e-6, "morph 0: A tap audible: {early_a}");
    assert!(
        late_a < early_a * 1e-6,
        "morph 0: B tap must be silent: {late_a}"
    );

    let (early_b, late_b) = run(1.0);
    assert!(late_b > 1e-6, "morph 1: B tap audible: {late_b}");
    assert!(
        early_b < late_b * 1e-6,
        "morph 1: A tap must be silent: {early_b}"
    );

    let (early_m, late_m) = run(0.5);
    assert!(
        early_m > 1e-7 && late_m > 1e-7,
        "morph 0.5: both slots audible: early={early_m}, late={late_m}"
    );
    // Equal-power: each side sits ~3 dB under its solo level.
    let ratio_a = early_m / early_a;
    let ratio_b = late_m / late_b;
    assert!(
        (0.3..0.7).contains(&ratio_a) && (0.3..0.7).contains(&ratio_b),
        "equal-power crossfade at 0.5: A ratio {ratio_a}, B ratio {ratio_b}"
    );
}

/// LFO-swept morph changes the mix over time and stays finite.
#[test]
fn morph_lfo_sweeps() {
    let (l1, _) = render(
        |p| {
            p.morph = 0.5;
            p.morph_lfo_depth = 0.5;
            p.lfo_rate = 0.8;
        },
        2.0,
    );
    assert!(l1.iter().all(|x| x.is_finite()));

    let (l_static, _) = render(|p| p.morph = 0.5, 2.0);
    let diff: f64 = l1
        .iter()
        .zip(l_static.iter())
        .skip(9600)
        .map(|(a, b)| (a - b).abs())
        .sum();
    assert!(
        diff > 1e-3,
        "LFO-swept morph should differ from static: {diff}"
    );
}

/// Wet-gain LFO: amplitude of the tail breathes at the LFO rate.
#[test]
fn wet_gain_lfo_breathes() {
    let (l_mod, _) = render(
        |p| {
            p.mod_wet_depth = 1.0;
            p.lfo_rate = 2.0;
        },
        2.0,
    );
    let (l_base, _) = render(|_| {}, 2.0);

    // Per-window RMS ratio mod/base should swing around 1 (±6 dB peak).
    let w = 2400; // 50 ms
    let mut ratios = Vec::new();
    for k in (9600..l_mod.len() - w).step_by(w) {
        let rm: f64 = l_mod[k..k + w].iter().map(|x| x * x).sum::<f64>().sqrt();
        let rb: f64 = l_base[k..k + w].iter().map(|x| x * x).sum::<f64>().sqrt();
        if rb > 1e-9 {
            ratios.push(rm / rb);
        }
    }
    let max = ratios.iter().cloned().fold(0.0, f64::max);
    let min = ratios.iter().cloned().fold(f64::MAX, f64::min);
    assert!(
        max > 1.2 && min < 0.85,
        "wet gain should breathe with the LFO: min={min}, max={max}"
    );
}

/// Damping LFO darkens/brightens the tail over time.
#[test]
fn damp_modulation_is_active_and_finite() {
    let (l_mod, _) = render(
        |p| {
            p.mod_damp_depth = 1.0;
            p.lfo_rate = 1.0;
        },
        2.0,
    );
    assert!(l_mod.iter().all(|x| x.is_finite()));
    let (l_base, _) = render(|_| {}, 2.0);
    let diff: f64 = l_mod
        .iter()
        .zip(l_base.iter())
        .skip(4800)
        .map(|(a, b)| (a - b).abs())
        .sum();
    assert!(diff > 1e-2, "damping LFO should alter the wet: {diff}");
}
