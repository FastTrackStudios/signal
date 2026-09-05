//! Bit-exact reference vectors for the compressor cores.
//!
//! A compressor is almost entirely state: a detector envelope, a gain-reduction
//! smoother with separate attack and release, a hold counter, crest-factor
//! tracking, and a dozen parameter smoothers that only settle over hundreds of
//! samples. Almost none of that is observable one sample at a time, which is
//! why the existing unit tests can pass while the compression itself has
//! changed character.
//!
//! So these pin whole signals. Transient bursts are the important excitation —
//! that is what a compressor is *for*, and an attack coefficient that moved by
//! a ULP shows up on the leading edge of a hit long before it shows up on a
//! sine.
//!
//! Everything here runs in `f64`, matching the cores. Pinning at `f32` would
//! discard exactly the low bits a refactor is most likely to disturb.

use comp_dsp::chain::CompChain;
use comp_dsp::multiband::MultiBandCompressor;
use comp_dsp::styles::CompressionStyle;
use comp_dsp::{ProC3Compressor, gain_curve::GainCurve};
use dsp_golden::{Golden, golden, signal};

dsp_golden::install_counting_allocator!();

const SAMPLE_RATE: f64 = 48_000.0;
/// The same rate for the generators, which work in `f32`. Spelled out rather
/// than narrowed from the constant above so no conversion appears in a fixture.
const GENERATOR_RATE: f32 = 48_000.0;
const LEN: usize = 8192;

/// The styles that are stable enough to have a reference at all. `Reserved` is
/// included deliberately: it is reachable through `set_style`, so it is
/// behaviour whether or not it was meant to be.
///
/// `Optical` (3) is absent, and that is not an oversight — see
/// [`the_optical_style_diverges`]. It has no output to pin.
const STYLES: [(&str, i32); 4] = [
    ("clean", 0),
    ("fet", 1),
    ("vca", 2),
    ("reserved", 4),
];

/// Every style reachable through `set_style`, including the broken one.
const ALL_STYLE_IDS: [i32; 5] = [0, 1, 2, 3, 4];

/// Transient bursts over a steady bed — a compressor's actual diet. The bed
/// keeps the detector out of its idle corner so release behaviour is exercised
/// between hits rather than only after them.
fn program(len: usize) -> Vec<f64> {
    let hits = signal::transients(len, GENERATOR_RATE);
    let bed = signal::sine(len, 110.0, GENERATOR_RATE);
    signal::widen(&hits)
        .into_iter()
        .zip(signal::widen(&bed))
        .map(|(hit, tone)| hit.mul_add(0.9, tone * 0.15))
        .collect()
}

fn compressor(style: i32) -> ProC3Compressor {
    let mut comp = ProC3Compressor::new(SAMPLE_RATE);
    comp.set_style(style);
    comp.set_threshold(-18.0);
    comp.set_ratio(4.0);
    comp.set_knee(6.0);
    comp.set_attack_ms(5.0);
    comp.set_release_ms(80.0);
    comp.update(SAMPLE_RATE);
    comp
}

#[test]
fn every_style_holds_its_reference_on_program_material() {
    let g: Golden = golden!();
    let input = program(LEN);
    for (name, style) in STYLES {
        let mut comp = compressor(style);
        let out: Vec<f64> = input
            .iter()
            .enumerate()
            .map(|(n, x)| comp.process(*x, n % 2))
            .collect();
        dsp_golden::assert_golden!(g, &format!("style_{name}"), &out);
    }
}

#[test]
fn the_gain_reduction_trajectory_is_pinned_not_just_the_output() {
    // The output alone can hide a changed envelope: a detector that attacks
    // differently but settles to the same steady state looks identical on a
    // sustained note and wrong on every transient. This records the reduction
    // curve itself, which is the thing a listener actually hears move.
    let g: Golden = golden!();
    let input = program(LEN);
    let mut comp = compressor(1);
    let mut trajectory = Vec::with_capacity(LEN);
    for (n, x) in input.iter().enumerate() {
        let _ = comp.process(*x, n % 2);
        trajectory.push(comp.gain_reduction_db());
    }
    dsp_golden::assert_golden!(g, "gain_reduction_trajectory", &trajectory);
}

#[test]
fn the_static_gain_curve_is_pinned_per_style() {
    // The curve alone, sampled across the whole input range: knee shape,
    // threshold placement and ratio, with no state in the way.
    let g: Golden = golden!();
    for (name, style) in STYLES {
        let mut curve = GainCurve::new(SAMPLE_RATE);
        curve.set_style(CompressionStyle::from_id(style));
        curve.set_threshold(-18.0);
        curve.set_ratio(4.0);
        curve.set_knee(9.0);
        curve.update_coefficients();
        let out: Vec<f64> = (0..1024)
            .map(|n| curve.compute_gr(f64::from(n).mul_add(72.0 / 1024.0, -60.0)))
            .collect();
        dsp_golden::assert_golden!(g, &format!("curve_{name}"), &out);
    }
}

#[test]
fn extreme_settings_hold_their_references() {
    // Limiting, expansion and a wide knee all take different branches through
    // the curve, and the branches are where a rewrite goes wrong.
    let g: Golden = golden!();
    let input = program(LEN);
    for (name, threshold, ratio, knee) in [
        ("limit", -30.0_f64, 60.0_f64, 0.0_f64),
        ("gentle", -6.0, 1.5, 24.0),
        ("hard_knee", -18.0, 8.0, 0.0),
    ] {
        let mut comp = compressor(0);
        comp.set_threshold(threshold);
        comp.set_ratio(ratio);
        comp.set_knee(knee);
        let out: Vec<f64> = input
            .iter()
            .enumerate()
            .map(|(n, x)| comp.process(*x, n % 2))
            .collect();
        dsp_golden::assert_golden!(g, &format!("extreme_{name}"), &out);
    }
}

#[test]
fn the_multiband_split_holds_its_reference() {
    let g: Golden = golden!();
    let input = program(LEN);
    let mut multi = MultiBandCompressor::new(SAMPLE_RATE);
    multi.set_threshold(-20.0);
    multi.set_ratio(3.0);
    multi.set_attack_ms(8.0);
    multi.set_release_ms(120.0);
    multi.update(SAMPLE_RATE);
    let out: Vec<f64> = input
        .iter()
        .enumerate()
        .map(|(n, x)| multi.process(*x, n % 2))
        .collect();
    dsp_golden::assert_golden!(g, "multiband", &out);
}

#[test]
fn the_full_chain_holds_its_reference_in_stereo() {
    let g: Golden = golden!();
    let bed = program(LEN);
    let other = signal::widen(&signal::noise(LEN, 0x00C0_FFEE));
    let mut chain = CompChain::new();
    chain.comp.set_threshold(-15.0);
    chain.comp.set_ratio(4.0);
    chain.set_sidechain_freq(120.0);
    chain.set_lookahead(2.0);

    let mut out = Vec::with_capacity(LEN * 2);
    for (l, r) in bed.iter().zip(&other) {
        let (mut left, mut right) = (*l, *r);
        chain.process_sample(&mut left, &mut right);
        out.push(left);
        out.push(right);
    }
    dsp_golden::assert_golden!(g, "chain_stereo_lookahead", &out);
}

/// The Optical style is unstable, and this test exists so that stays visible.
///
/// `GainCurve::update_coefficients` scales the smoothing *coefficient* by 1.15
/// for Optical, intending (per its own comment) a 1.15x slower attack. But the
/// coefficient is a one-pole feedback term: `base_attack` is already
/// `exp(-2 / (sample_rate * attack_s))`, which approaches 1 as the attack time
/// grows. Multiplying it by 1.15 pushes it *above* 1, putting the pole outside
/// the unit circle, and the smoother then multiplies by >1 every sample.
///
/// Break-even is an attack of 0.298 ms at 48 kHz. Above that — which is every
/// realistic setting — the gain reduction diverges to infinity within a few
/// hundred samples, and the output is `inf` and then `NaN`.
///
/// A slower attack means a coefficient *closer to* 1, never past it. The
/// intent as documented is a time-constant scale (`attack_ms * 1.15`), which is
/// stable by construction; applying the factor to the coefficient is the bug.
/// Fixing it is a tonal change to the other styles too if the same reading is
/// applied uniformly, so it is left for a deliberate decision rather than
/// folded into a refactor.
///
/// When Optical is fixed, this test fails. That is the point: move it into
/// [`STYLES`] and record its reference.
#[test]
fn the_optical_style_diverges() {
    let input = program(2048);
    let mut comp = compressor(3);
    let diverged = input
        .iter()
        .enumerate()
        .map(|(n, x)| comp.process(*x, n % 2))
        .any(|y| !y.is_finite());
    assert!(
        diverged,
        "the Optical style is now stable — good. Move it into STYLES, record \
         its reference vector, and delete this test."
    );
}

#[test]
fn no_other_style_diverges() {
    // The counterpart to the test above: the instability must stay confined to
    // the one style whose coefficient exceeds 1, rather than being something
    // the whole smoother does under this input.
    let input = program(4096);
    for (name, style) in STYLES {
        let mut comp = compressor(style);
        for (n, x) in input.iter().enumerate() {
            let y = comp.process(*x, n % 2);
            assert!(y.is_finite(), "{name} produced {y} at sample {n}");
        }
    }
}

#[test]
fn silence_stays_silent_through_every_style() {
    // An invariant, not a reference: a compressor that emits anything on
    // silence is either self-oscillating or leaking denormals, and no recorded
    // vector should make that acceptable.
    // Every style, including the broken one: silence in must give silence out
    // even where the smoother is unstable, because nothing excites it.
    for style in ALL_STYLE_IDS {
        let mut comp = compressor(style);
        let mut worst = 0.0_f64;
        for n in 0..4096 {
            worst = worst.max(comp.process(0.0, n % 2).abs());
        }
        assert!(worst < 1e-12, "style {style} emitted {worst} on silence");
    }
}

#[test]
fn processing_allocates_nothing() {
    let input = program(1024);
    let mut comp = compressor(1);
    let mut multi = MultiBandCompressor::new(SAMPLE_RATE);
    multi.update(SAMPLE_RATE);
    let mut chain = CompChain::new();
    chain.set_lookahead(3.0); // sizes the delay lines up front

    let mut sink = 0.0_f64;
    let mut run = |sink: &mut f64| {
        for (n, x) in input.iter().enumerate() {
            let ch = n % 2;
            *sink += comp.process(*x, ch) + multi.process(*x, ch);
            let (mut left, mut right) = (*x, *x);
            chain.process_sample(&mut left, &mut right);
            *sink += left + right;
        }
    };
    run(&mut sink); // warm up outside the guard
    dsp_golden::assert_no_alloc(|| run(&mut sink));
    assert!(sink.is_finite());
}
