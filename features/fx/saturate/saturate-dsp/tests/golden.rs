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
