//! A translated Pro-Q 4 dynamic band actually behaves dynamically.
//!
//! The unit tests on either side of this one are both green and both
//! insufficient: `proq4` proves the right numbers come out of the preset, and
//! `eq-dsp`'s `dyn_band` proves the filter compresses when driven. Neither
//! says the numbers reach the filter. That join is where a translation quietly
//! fails — a name that does not match, a unit that does not match, a parameter
//! the engine ignores — and it is worth a test of its own because **131 of the
//! 171 Pro-Q 4 factory presets use dynamic bands**. A library that recalled
//! only their static curves would look right and sound wrong.
//!
//! So this drives real audio: quiet tone in, loud tone in, and the band has to
//! pull the loud one down by something like its dynamic range while leaving
//! the quiet one alone.

use signal_fx::NativeEq;
use signal_plugin_host::{PluginEvents, PluginInstance};

const SR: f64 = 48_000.0;
const BLOCK: usize = 256;
/// The band under test, and the tone that sits in it.
const BAND_HZ: f64 = 1_000.0;

/// Feed a steady tone at `amplitude` and return the output RMS once settled.
fn steady_state_rms(eq: &mut NativeEq, amplitude: f64, seconds: f64) -> f64 {
    let events = PluginEvents::default();
    let frames = (SR * seconds) as usize;
    // Measure only the last quarter: the detector has ballistics, and the
    // attack ramp is not the steady state.
    let measure_from = frames * 3 / 4;

    let mut phase = 0.0f64;
    let inc = std::f64::consts::TAU * BAND_HZ / SR;
    let (mut sum, mut n) = (0.0f64, 0usize);

    let mut pos = 0;
    while pos < frames {
        let len = BLOCK.min(frames - pos);
        let mut l = vec![0.0f32; len];
        for s in &mut l {
            *s = (amplitude * phase.sin()) as f32;
            phase += inc;
        }
        let r = l.clone();
        let (mut ol, mut or) = (vec![0.0f32; len], vec![0.0f32; len]);
        eq.process_block(&l, &r, &mut ol, &mut or, &events)
            .expect("process");
        for (i, s) in ol.iter().enumerate() {
            if pos + i >= measure_from {
                sum += f64::from(*s) * f64::from(*s);
                n += 1;
            }
        }
        pos += len;
    }
    (sum / n.max(1) as f64).sqrt()
}

/// A band set up the way the Pro-Q translator emits one.
fn dynamic_eq(range_db: f64) -> NativeEq {
    let mut eq = NativeEq::new(SR);
    for (name, value) in [
        ("b1_used", 1.0),
        ("b1_on", 1.0),
        ("b1_freq", BAND_HZ),
        ("b1_gain", 0.0),
        ("b1_q", 1.0),
        ("b1_shape", 0.0),
        ("b1_slope", 2.0),
        ("b1_dyn_range", range_db),
        ("b1_dyn_thr", -30.0),
        ("b1_dyn_atk", 5.0),
        ("b1_dyn_rel", 20.0),
        ("b1_dyn_auto", 0.0),
    ] {
        eq.set_named(name, value);
    }
    eq.prepare(SR, BLOCK as u32).expect("prepare");
    eq
}

/// The gain the band applies at `amplitude`, in dB relative to the input.
fn applied_gain_db(eq: &mut NativeEq, amplitude: f64) -> f64 {
    let out = steady_state_rms(eq, amplitude, 2.0);
    // A full-scale sine's RMS is a/sqrt(2).
    let expected = amplitude / std::f64::consts::SQRT_2;
    20.0 * (out / expected).log10()
}

/// A negative dynamic range compresses: loud gets pulled down, quiet does not.
#[test]
fn a_translated_dynamic_band_compresses_when_driven() {
    const RANGE_DB: f64 = -9.0;

    // Well under the -30 dB threshold, so the band should be sitting still.
    let quiet = applied_gain_db(&mut dynamic_eq(RANGE_DB), 0.003);
    // Well over it.
    let loud = applied_gain_db(&mut dynamic_eq(RANGE_DB), 0.7);

    assert!(
        quiet.abs() < 1.5,
        "below the threshold the band should be near unity, but it applied {quiet:+.2} dB",
    );
    assert!(
        loud < quiet - 3.0,
        "above the threshold the band should pull down: quiet {quiet:+.2} dB against \
         loud {loud:+.2} dB — the dynamics are not reaching the filter",
    );
    // It should not exceed the range it was given, either.
    assert!(
        loud > RANGE_DB - 3.0,
        "the band pulled down {loud:+.2} dB, past its {RANGE_DB} dB range",
    );
}

/// A positive range expands instead, on the same wiring.
///
/// Sign matters: Pro-Q's ring is bipolar and a good number of the factory
/// presets use a positive range. A translator that dropped the sign would
/// still pass a test that only ever checked compression.
#[test]
fn a_positive_range_rides_up_rather_than_down() {
    let loud_boost = applied_gain_db(&mut dynamic_eq(6.0), 0.7);
    let loud_cut = applied_gain_db(&mut dynamic_eq(-6.0), 0.7);
    assert!(
        loud_boost > loud_cut + 3.0,
        "a positive range should boost where a negative one cuts, but got \
         {loud_boost:+.2} dB against {loud_cut:+.2} dB",
    );
}

/// A band with no dynamics is left alone by level.
#[test]
fn a_static_band_does_not_move_with_level() {
    let quiet = applied_gain_db(&mut dynamic_eq(0.0), 0.003);
    let loud = applied_gain_db(&mut dynamic_eq(0.0), 0.7);
    assert!(
        (quiet - loud).abs() < 1.0,
        "a static band moved with level: {quiet:+.2} dB quiet against {loud:+.2} dB loud",
    );
}
