//! The Overheads preset (FTS EQ issue #2 in `processor`) drives the real
//! engine, and every band arrives at rest.
//!
//! "At rest" is a claim about audio, not about the numbers in the preset
//! file — the whole point of shipping a band with `dyn_range` 0 or `gain` 0
//! is that it does nothing until someone pulls the one control that matters.
//! This proves it the only way that means anything: install the six bands
//! exactly as the preset would, at the values in
//! `features/fx/eq/eq-ui/tests/overheads_preset.rs` (kept in sync with
//! `/run/media/AudioHaven/Signal/Libraries/Presets/FTS-EQ/fts/Overheads.json`),
//! and confirm a block of audio comes out unchanged.
//!
//! The attack-percentage-to-milliseconds law the same preset leans on is
//! pinned separately, in `eq-dsp`'s own `engine::routing::attack_law_tests`
//! (it needs a private field the engine's own module can see; this file only
//! has the public surface).

use dsp_core::num::count_to_f64;
use eq_dsp::engine::{BandConfig, BandDynamics, FtsEq};

const SAMPLE_RATE: f64 = 48_000.0;
const BLOCK: u32 = 512;

/// One band's static config, at the Overheads preset's values.
fn band(freq_hz: f64, gain_db: f64, q: f64, shape: u32, enabled: bool) -> BandConfig {
    BandConfig {
        used: true,
        enabled,
        freq_hz,
        gain_db,
        q,
        shape,
        slope: 2.0,
        ..BandConfig::default()
    }
}

/// A band's dynamics, at rest (`range_db` 0) exactly as the preset ships it —
/// the range is the one control the preset leaves untouched, and the engine
/// treats a zero range as a static band regardless of what attack, density
/// or threshold sit next to it.
fn dynamics_at_rest(attack_pct: f64, spectral: bool, spectral_density: f64) -> BandDynamics {
    BandDynamics {
        range_db: 0.0,
        threshold_db: -18.0,
        attack_pct,
        release_pct: 50.0,
        auto: false,
        spectral,
        spectral_density,
        ..BandDynamics::default()
    }
}

/// Install the Overheads preset's six bands into a fresh engine.
fn overheads(eq: &mut FtsEq) {
    // 1 — High Pass: off by default.
    eq.set_band(0, band(150.0, 0.0, 1.0, 3, false));

    // 2 — Clank: dynamic bell, at rest.
    eq.set_band(1, band(300.0, 0.0, 1.0, 0, true));
    eq.set_band_dynamics(1, dynamics_at_rest(65.0, false, 50.0));

    // 3 — Snare Ring: dynamic bell, at rest.
    eq.set_band(2, band(450.0, 0.0, 6.0, 0, true));
    eq.set_band_dynamics(2, dynamics_at_rest(30.0, false, 50.0));

    // 4 — Lowest Cymbal: spectral, at rest.
    eq.set_band(3, band(3500.0, 0.0, 1.2, 0, true));
    eq.set_band_dynamics(3, dynamics_at_rest(95.0, true, 75.0));

    // 5 — Highest Cymbal: spectral, at rest.
    eq.set_band(4, band(6500.0, 0.0, 1.2, 0, true));
    eq.set_band_dynamics(4, dynamics_at_rest(95.0, true, 75.0));

    // 6 — Air: high shelf, gain at rest.
    eq.set_band(5, band(12000.0, 0.0, 0.7, 2, true));
}

/// A deterministic, full-spectrum-ish test signal — several sines summed, not
/// a pure tone, so a filter with any real gain at any of the preset's six
/// frequencies would show up.
fn test_signal(len: u32) -> Vec<f64> {
    let freqs = [80.0, 150.0, 300.0, 450.0, 1000.0, 3500.0, 6500.0, 12000.0];
    let count = count_to_f64(freqs.len());
    (0..len)
        .map(|i| {
            let t = count_to_f64(dsp_core::num::u32_to_index(i)) / SAMPLE_RATE;
            freqs
                .iter()
                .map(|f| (std::f64::consts::TAU * f * t).sin())
                .sum::<f64>()
                / count
        })
        .collect()
}

/// The largest sample-by-sample difference between two equal-length buffers.
fn max_abs_diff(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0_f64, f64::max)
}

#[test]
fn loading_the_overheads_preset_engages_no_dynamics_or_spectral_processing() {
    let mut eq = FtsEq::new(SAMPLE_RATE);
    eq.prepare(SAMPLE_RATE, BLOCK);
    overheads(&mut eq);

    // Every "one control" in the table is at the value that does nothing:
    // dyn_range 0 for bands 2-5, so nothing routes through the dynamics or
    // spectral engines even though the dynamics/spectral flags are set.
    assert!(
        !eq.any_dynamic(),
        "dyn_range 0 must leave every band static, not merely quiet"
    );
    assert!(
        !eq.spectral_engaged(),
        "spectral bands with dyn_range 0 must not engage the spectral engine"
    );
}

#[test]
fn loading_the_overheads_preset_changes_no_audio() {
    let mut eq = FtsEq::new(SAMPLE_RATE);
    eq.prepare(SAMPLE_RATE, BLOCK);
    overheads(&mut eq);

    let input = test_signal(BLOCK);
    let mut left = input.clone();
    let mut right = input.clone();
    eq.process(&mut left, &mut right);

    // Bands 1-6 loaded, placed and documented, and none of it audible: the
    // high pass is off, the four dynamic/spectral bands sit at a zero range,
    // and the shelf's gain is 0 dB. -120 dB of headroom below the signal
    // itself is generous for what should be exact identity coefficients.
    let max_err = max_abs_diff(&input, &left).max(max_abs_diff(&input, &right));
    assert!(
        max_err < 1.0e-6,
        "loading Overheads changed the signal by up to {max_err}, expected silence-of-change"
    );
}
