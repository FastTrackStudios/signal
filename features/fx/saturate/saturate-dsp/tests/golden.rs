//! Bit-exact reference vectors for the saturation curves.
//!
//! These exist so the pending idiomatic-Rust rewrite of this crate can be
//! proven to change nothing. Every curve is a static nonlinearity, so its
//! behaviour is captured by pushing known signals through it: a sweep covers
//! the input range continuously, a DC staircase samples the transfer curve
//! itself, and fixed-seed noise catches anything that depends on sample
//! history when it should not.
//!
//! If one of these files changes, the audio changed. That is the whole point.

use dsp_golden::{Golden, golden, signal};
use saturate_dsp::{SaturationCurve, Saturator};

dsp_golden::install_counting_allocator!();

const SAMPLE_RATE: f32 = 48_000.0;
const LEN: usize = 4096;

const CURVES: [(&str, SaturationCurve); 4] = [
    ("tanh", SaturationCurve::Tanh),
    ("tape", SaturationCurve::Tape),
    ("tube", SaturationCurve::Tube),
    ("hard", SaturationCurve::Hard),
];

fn render(curve: SaturationCurve, drive: f32, mix: f32, output_db: f32, input: &[f32]) -> Vec<f32> {
    let mut sat = Saturator::new();
    sat.set_curve(curve);
    sat.set_drive(drive);
    sat.set_mix(mix);
    sat.set_output_db(output_db);
    input.iter().map(|x| sat.process(*x)).collect()
}

#[test]
fn every_curve_holds_its_reference_across_the_drive_range() {
    let g: Golden = golden!();
    let sweep = signal::log_sweep(LEN, 20.0, 18_000.0, SAMPLE_RATE);
    for (name, curve) in CURVES {
        for drive in [0.0_f32, 0.25, 0.5, 1.0] {
            let out = render(curve, drive, 1.0, 0.0, &sweep);
            let drive_tag = (drive * 100.0).round();
            dsp_golden::assert_golden!(g, &format!("sweep_{name}_drive{drive_tag:.0}"), &out);
        }
    }
}

#[test]
fn the_transfer_curve_itself_is_pinned() {
    let g: Golden = golden!();
    // A staircase of DC levels across the full input range: the transfer curve
    // sampled directly, rather than its response to a signal. This is where a
    // tube curve's asymmetry lives.
    let steps: Vec<f32> = (0..LEN)
        .map(|n| dsp_golden::num::count_to_f32(n).mul_add(4.0 / 4096.0, -2.0))
        .collect();
    for (name, curve) in CURVES {
        let out = render(curve, 0.75, 1.0, 0.0, &steps);
        dsp_golden::assert_golden!(g, &format!("transfer_{name}"), &out);
    }
}

#[test]
fn mix_and_output_trim_hold_their_references() {
    let g: Golden = golden!();
    let noise = signal::noise(LEN, 0x5A7);
    for mix in [0.0_f32, 0.35, 1.0] {
        for db in [-12.0_f32, 0.0, 6.0] {
            let out = render(SaturationCurve::Tube, 0.9, mix, db, &noise);
            let mix_tag = (mix * 100.0).round();
            dsp_golden::assert_golden!(g, &format!("mix{mix_tag:.0}_out{db:+.0}db"), &out);
        }
    }
}

#[test]
fn silence_in_is_silence_out_for_every_curve() {
    // Not a golden but an invariant: a saturator that emits DC on silence is
    // audible as a click the moment it is bypassed, and no reference vector
    // would make that acceptable.
    let quiet = signal::silence(256);
    for (name, curve) in CURVES {
        let out = render(curve, 1.0, 1.0, 0.0, &quiet);
        let peak = out.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(peak < 1e-6, "{name} emitted {peak} on silence");
    }
}

#[test]
fn processing_allocates_nothing() {
    let mut sat = Saturator::new();
    sat.set_curve(SaturationCurve::Tube);
    sat.set_drive(0.7);
    let input = signal::noise(512, 1);
    let mut output = vec![0.0_f32; input.len()];

    // Warm up outside the guard: construction may allocate, steady state may not.
    for (dst, src) in output.iter_mut().zip(&input) {
        *dst = sat.process(*src);
    }

    dsp_golden::assert_no_alloc(|| {
        for (dst, src) in output.iter_mut().zip(&input) {
            *dst = sat.process(*src);
        }
    });
}

// ── The stateful stages ───────────────────────────────────────────────────
//
// `Saturator` above is memoryless, so a mistake in it shows up on any input.
// These three carry state — a DC blocker, a sag envelope, tilt filters, six
// biquads, a zero-order hold — and state is where a refactor actually goes
// wrong: an index folded onto the wrong channel, a filter history updated in
// the wrong order, a counter that used to wrap and now saturates. None of that
// is visible in a per-sample test, so these run continuous signals through and
// pin the whole tail.

use saturate_dsp::digital::DigitalStage;
use saturate_dsp::emphasis::{EmphBand, EmphShape, EmphasisEq, BANDS};
use saturate_dsp::preamp::{ClassAPreamp, Makeup, SideShaper};

const SHAPERS: [(&str, SideShaper); 6] = [
    ("clean", SideShaper::Clean),
    ("opamp", SideShaper::OpAmp),
    ("tube", SideShaper::Tube),
    ("transformer", SideShaper::Transformer),
    ("diode", SideShaper::Diode),
    ("hard", SideShaper::Hard),
];

/// Interleaved stereo, so a channel-state mix-up shows as a changed sample
/// rather than as identical channels that happen to agree.
fn stereo_pairs(len: usize) -> Vec<(f32, f32)> {
    let left = signal::log_sweep(len, 30.0, 12_000.0, SAMPLE_RATE);
    let right = signal::noise(len, 0xBEEF);
    left.into_iter().zip(right).collect()
}

#[test]
fn every_preamp_shaper_holds_its_reference_in_stereo() {
    let g: Golden = golden!();
    let input = stereo_pairs(LEN);
    for (name, shaper) in SHAPERS {
        let mut pre = ClassAPreamp::new(SAMPLE_RATE);
        pre.drive = 4.0;
        pre.positive = shaper;
        pre.negative = SideShaper::Tube; // asymmetric on purpose: even harmonics
        pre.q_point = 0.15;
        pre.sag = 0.5;
        pre.mix = 1.0;
        pre.makeup = Makeup::InverseDrive;
        pre.set_tilt_db(6.0);
        pre.refresh_makeup();

        let mut out = Vec::with_capacity(LEN * 2);
        for (left, right) in &input {
            out.push(pre.process(0, *left));
            out.push(pre.process(1, *right));
        }
        dsp_golden::assert_golden!(g, &format!("preamp_{name}"), &out);
    }
}

#[test]
fn the_preamps_static_transfer_is_pinned_per_shaper() {
    let g: Golden = golden!();
    for (name, shaper) in SHAPERS {
        let out: Vec<f32> = (0..LEN)
            .map(|n| shaper.shape(dsp_golden::num::count_to_f32(n).mul_add(8.0 / 4096.0, -4.0)))
            .collect();
        dsp_golden::assert_golden!(g, &format!("shaper_{name}"), &out);
    }
}

#[test]
fn the_emphasis_pair_holds_its_reference_and_stays_invertible() {
    let g: Golden = golden!();
    let mut bands = [EmphBand::default(); BANDS];
    // A curve with something in every shape, so all three coefficient paths
    // are exercised rather than only the bell.
    for (i, band) in bands.iter_mut().enumerate() {
        band.shape = EmphShape::from_index(u32::try_from(i % 3).unwrap_or(0));
        band.freq_hz = 80.0 * 2.0_f32.powi(i32::try_from(i).unwrap_or(0));
        band.gain_db = if i % 2 == 0 { 6.0 } else { -4.5 };
        band.q = 0.7 + 0.3 * dsp_golden::num::count_to_f32(i);
    }

    let mut eq = EmphasisEq::new(SAMPLE_RATE);
    eq.set_bands(&bands);
    let input = signal::log_sweep(LEN, 20.0, 20_000.0, SAMPLE_RATE);

    let pre: Vec<f32> = input.iter().map(|x| eq.pre(*x)).collect();
    dsp_golden::assert_golden!(g, "emphasis_pre", &pre);

    let round_trip: Vec<f32> = pre.iter().map(|x| eq.post(*x)).collect();
    dsp_golden::assert_golden!(g, "emphasis_round_trip", &round_trip);

    // The pair is designed to be 1/H(z) of itself. That is an invariant of the
    // design, not of this implementation, so it is asserted rather than pinned
    // — a golden alone would happily record a broken inverse.
    let worst = round_trip
        .iter()
        .zip(&input)
        .skip(64) // filter startup
        .fold(0.0_f32, |m, (out, want)| m.max((out - want).abs()));
    assert!(worst < 1e-2, "emphasis/de-emphasis is not transparent: {worst}");
}

#[test]
fn the_digital_stage_holds_its_reference_across_word_and_rate() {
    let g: Golden = golden!();
    let input = stereo_pairs(LEN);
    for (bits, rate, dither) in [
        (24.0_f32, 1.0_f32, 0.0_f32), // transparent
        (8.0, 1.0, 0.0),              // quantise only
        (24.0, 6.0, 0.0),             // decimate only
        (5.5, 3.7, 0.0),              // both, fractional
        (6.0, 1.0, 1.0),              // dithered
    ] {
        let mut stage = DigitalStage::new();
        stage.bits = bits;
        stage.rate = rate;
        stage.dither = dither;

        let mut out = Vec::with_capacity(LEN * 2);
        for (left, right) in &input {
            out.push(stage.process(0, *left));
            out.push(stage.process(1, *right));
        }
        dsp_golden::assert_golden!(
            g,
            &format!("digital_b{bits:.1}_r{rate:.1}_d{dither:.1}"),
            &out
        );
    }
}

#[test]
fn the_stateful_stages_allocate_nothing_either() {
    let input = signal::noise(512, 9);

    let mut pre = ClassAPreamp::new(SAMPLE_RATE);
    pre.drive = 3.0;
    pre.sag = 0.7;
    pre.set_tilt_db(-4.0);
    pre.refresh_makeup();

    let mut stage = DigitalStage::new();
    stage.bits = 7.0;
    stage.rate = 3.0;
    stage.dither = 0.5;

    let mut eq = EmphasisEq::new(SAMPLE_RATE);
    eq.set_bands(&[EmphBand::default(); BANDS]);

    let mut sink = 0.0_f32;
    let mut run = |sink: &mut f32| {
        for (i, x) in input.iter().enumerate() {
            let ch = i % 2;
            let emphasised = eq.pre(*x);
            let restored = eq.post(emphasised);
            *sink += stage.process(ch, pre.process(ch, restored));
        }
    };
    run(&mut sink); // warm up outside the guard
    dsp_golden::assert_no_alloc(|| run(&mut sink));
    assert!(sink.is_finite());
}
