//! Bit-exact reference vectors for the EQ engine.
//!
//! An EQ is mostly linear, and that shapes what is worth pinning. Two things
//! capture almost all of it:
//!
//! - **The impulse response.** For a linear filter it *is* the filter — every
//!   coefficient, every section of the cascade, and the order they run in are
//!   all visible in it, and nothing else needs to be guessed at.
//! - **The magnitude curve**, sampled log-spaced across the audible band. That
//!   is what the user sees drawn, it comes from a different code path
//!   (`static_magnitude_db`, evaluated analytically rather than by running
//!   audio), and the two agreeing is itself a useful invariant.
//!
//! What is *not* linear gets its own fixtures: the dynamic bands, the
//! transient split, and the character/saturation modes all have state and a
//! detector, so they are driven with programme material rather than an
//! impulse.
//!
//! Every shape is covered rather than a representative few. The cascade
//! designs thirteen filter shapes across a continuous slope parameter, and the
//! ones that break in a rewrite are never the ones anybody thought to spot
//! check.

use dsp_golden::{Golden, golden, signal};
use eq_dsp::engine::{BandConfig, BandDynamics, FtsEq};
use eq_dsp::runtime::band::Placement;

dsp_golden::install_counting_allocator!();

const SR: f64 = 48_000.0;
const GENERATOR_RATE: f32 = 48_000.0;
const BLOCK: u32 = 512;
/// Long enough for the low-frequency sections to ring out fully.
const LEN: usize = 8192;

/// The shapes that produce finite output and can therefore have a reference.
///
/// `shelf_alt` (11) is absent because it is unstable at every Q tested — see
/// [`unstable_designs_retain_the_previous_stable_cascade`]. `band_shelf` (10) is present but is
/// only pinned at Q >= 0.707; see [`unstable_designs_retain_the_previous_stable_cascade`].
const SHAPES: [(&str, u32); 11] = [
    ("bell", 0),
    ("low_shelf", 1),
    ("high_shelf", 2),
    ("low_cut", 3),
    ("high_cut", 4),
    ("notch", 5),
    ("band_pass", 6),
    ("tilt_shelf", 7),
    ("flat_tilt", 8),
    ("all_pass", 9),
    ("band_shelf", 10),
];

/// Shapes whose filter design goes unstable. Both are marked "previously
/// design-only" in [`eq_dsp::slope::FilterShape`] — they were exposed as
/// processing shapes without ever having been run as filters.
const BAND_SHELF: u32 = 10;
const SHELF_ALT: u32 = 11;

fn prepared() -> FtsEq {
    let mut eq = FtsEq::new(SR);
    eq.prepare(SR, BLOCK);
    eq
}

/// One band, used and enabled, at a musically central place.
const fn band(shape: u32, gain_db: f64, slope: f64) -> BandConfig {
    BandConfig {
        used: true,
        enabled: true,
        freq_hz: 1000.0,
        gain_db,
        q: 1.2,
        shape,
        slope,
        placement: Placement::Stereo,
        stream: 0,
    }
}

/// Render a stereo signal through `eq` in host-sized blocks, interleaved.
///
/// Blocks rather than one long call on purpose: an engine that only works when
/// handed the whole signal at once has a buffer-boundary bug this would miss,
/// and `prepare` was given the same block size.
fn render(eq: &mut FtsEq, left_in: &[f64], right_in: &[f64]) -> Vec<f64> {
    let mut left = left_in.to_vec();
    let mut right = right_in.to_vec();
    let block = BLOCK.try_into().unwrap_or(512_usize);
    for (l, r) in left.chunks_mut(block).zip(right.chunks_mut(block)) {
        eq.process(l, r);
    }
    left.into_iter()
        .zip(right)
        .flat_map(<[f64; 2]>::from)
        .collect()
}

fn impulse_pair() -> (Vec<f64>, Vec<f64>) {
    let left = signal::widen(&signal::impulse(LEN));
    // The right channel gets its own impulse one sample later, so a fixture
    // cannot pass while the two channels are swapped or summed.
    let mut right = vec![0.0; LEN];
    if let Some(slot) = right.get_mut(1) {
        *slot = 1.0;
    }
    (left, right)
}

#[test]
fn every_shape_holds_its_impulse_response() {
    let g: Golden = golden!();
    let (left, right) = impulse_pair();
    for (name, shape) in SHAPES {
        let mut eq = prepared();
        eq.set_band(0, band(shape, 6.0, 2.0));
        let out = render(&mut eq, &left, &right);
        dsp_golden::assert_golden!(g, &format!("impulse_{name}"), &out);
    }
}

#[test]
fn every_shape_holds_its_magnitude_curve() {
    // A different code path from the impulse response above: this is evaluated
    // analytically rather than by running audio, and it is what the editor
    // draws. The two can drift apart independently.
    let g: Golden = golden!();
    for (name, shape) in SHAPES {
        let mut eq = prepared();
        eq.set_band(0, band(shape, 6.0, 2.0));
        let curve: Vec<f64> = (0..512)
            .map(|k| {
                // 20 Hz .. 20 kHz, log spaced.
                let t = f64::from(k) / 511.0;
                eq.static_magnitude_db(20.0 * 1000.0_f64.powf(t))
            })
            .collect();
        dsp_golden::assert_golden!(g, &format!("magnitude_{name}"), &curve);
    }
}

#[test]
fn the_continuous_slope_ladder_holds_its_reference() {
    // Slope is continuous: the integer part picks the filter order and the
    // remainder is realized as a pole-zero ladder, so 7.5 dB/oct is a real
    // setting that 137 bands in the factory library use. The fractional cases
    // exercise ladder code the integer ones never reach.
    let g: Golden = golden!();
    let (left, right) = impulse_pair();
    for slope in [1.0_f64, 2.5, 4.0, 7.5, 12.0, 15.25] {
        let mut eq = prepared();
        eq.set_band(0, band(3, 0.0, slope)); // low cut
        let out = render(&mut eq, &left, &right);
        let tag = (slope * 100.0).round();
        dsp_golden::assert_golden!(g, &format!("slope_{tag:.0}"), &out);
    }
}

#[test]
fn every_placement_holds_its_reference() {
    // Left/Right/Mid/Side routing is where a stereo rewrite goes wrong, and it
    // is invisible to any mono fixture.
    let g: Golden = golden!();
    let left = signal::widen(&signal::log_sweep(LEN, 30.0, 16_000.0, GENERATOR_RATE));
    let right = signal::widen(&signal::noise(LEN, 0x00EA_5E11));
    for (name, placement) in [
        ("stereo", Placement::Stereo),
        ("left", Placement::Left),
        ("right", Placement::Right),
        ("mid", Placement::Mid),
        ("side", Placement::Side),
    ] {
        let mut eq = prepared();
        let mut cfg = band(0, 9.0, 2.0);
        cfg.placement = placement;
        eq.set_band(0, cfg);
        let out = render(&mut eq, &left, &right);
        dsp_golden::assert_golden!(g, &format!("placement_{name}"), &out);
    }
}

#[test]
fn a_full_multiband_curve_holds_its_reference() {
    // Eight bands at once: the cascade's section ordering and the accumulated
    // gain staging, which a single band never exercises.
    let g: Golden = golden!();
    let left = signal::widen(&signal::log_sweep(LEN, 20.0, 20_000.0, GENERATOR_RATE));
    let right = signal::widen(&signal::transients(LEN, GENERATOR_RATE));
    let mut eq = prepared();
    for (i, (freq, gain, shape)) in [
        (60.0, -4.0, 3_u32),
        (120.0, 3.5, 1),
        (400.0, -6.0, 0),
        (900.0, 2.0, 0),
        (2_000.0, -3.0, 5),
        (4_500.0, 5.0, 0),
        (9_000.0, -2.5, 2),
        (14_000.0, 4.0, 4),
    ]
    .into_iter()
    .enumerate()
    {
        let mut cfg = band(shape, gain, 2.0);
        cfg.freq_hz = freq;
        eq.set_band(i, cfg);
    }
    let out = render(&mut eq, &left, &right);
    dsp_golden::assert_golden!(g, "multiband_eight", &out);
}

#[test]
fn the_dynamic_bands_hold_their_reference() {
    // Dynamics are the non-linear part: a detector, an attack/release ride,
    // and optionally a per-bin spectral mode. Driven with programme material,
    // because an impulse tells a compressor nothing.
    let g: Golden = golden!();
    let left = signal::widen(&signal::transients(LEN, GENERATOR_RATE));
    let right = signal::widen(&signal::sine(LEN, 220.0, GENERATOR_RATE));

    for (name, range_db, spectral) in [
        ("compress", -9.0_f64, false),
        ("expand", 6.0, false),
        ("spectral", -9.0, true),
    ] {
        let mut eq = prepared();
        eq.set_band(0, band(0, 0.0, 2.0));
        let dyn_cfg = BandDynamics {
            range_db,
            threshold_db: -24.0,
            attack_pct: 20.0,
            release_pct: 40.0,
            spectral,
            ..BandDynamics::default()
        };
        eq.set_band_dynamics(0, dyn_cfg);
        let out = render(&mut eq, &left, &right);
        dsp_golden::assert_golden!(g, &format!("dynamics_{name}"), &out);
    }
}

#[test]
fn a_bypassed_band_is_bit_transparent() {
    // An invariant, not a reference. `used: false` must render the input
    // untouched — not "nearly", since a band nobody switched on has no licence
    // to alter a single sample.
    let left = signal::widen(&signal::noise(LEN, 3));
    let right = signal::widen(&signal::noise(LEN, 4));
    let mut eq = prepared();
    let mut cfg = band(0, 12.0, 2.0);
    cfg.used = false;
    eq.set_band(0, cfg);
    let out = render(&mut eq, &left, &right);
    for (n, (got, want)) in out.chunks_exact(2).zip(left.iter().zip(&right)).enumerate() {
        let [got_l, got_r] = got else { continue };
        assert_eq!(got_l.to_bits(), want.0.to_bits(), "left drifted at {n}");
        assert_eq!(got_r.to_bits(), want.1.to_bits(), "right drifted at {n}");
    }
}

#[test]
fn block_size_does_not_change_the_output() {
    // A rewrite that caches something per block rather than per sample breaks
    // exactly this, and no single-block fixture would notice.
    let left = signal::widen(&signal::log_sweep(LEN, 40.0, 15_000.0, GENERATOR_RATE));
    let right = signal::widen(&signal::noise(LEN, 11));
    let render_at = |block: usize| {
        let mut eq = FtsEq::new(SR);
        eq.prepare(SR, BLOCK);
        eq.set_band(0, band(0, 6.0, 2.0));
        eq.set_band(1, band(3, 0.0, 4.0));
        let mut l = left.clone();
        let mut r = right.clone();
        for (a, b) in l.chunks_mut(block).zip(r.chunks_mut(block)) {
            eq.process(a, b);
        }
        (l, r)
    };
    let (big_l, big_r) = render_at(512);
    let (small_l, small_r) = render_at(64);
    for (n, (a, b)) in big_l.iter().zip(&small_l).enumerate() {
        assert_eq!(a.to_bits(), b.to_bits(), "left diverged at sample {n}");
    }
    for (n, (a, b)) in big_r.iter().zip(&small_r).enumerate() {
        assert_eq!(a.to_bits(), b.to_bits(), "right diverged at sample {n}");
    }
}

/// Unsupported designs must never install unstable poles in the audio path.
#[test]
fn unstable_designs_retain_the_previous_stable_cascade() {
    let input = signal::widen(&signal::transients(LEN, GENERATOR_RATE));
    for (shape, qs) in [
        (SHELF_ALT, &[0.025, 0.1, 0.5, 0.707, 1.2, 8.0, 40.0][..]),
        (BAND_SHELF, &[0.025, 0.1, 0.3, 0.5][..]),
    ] {
        for &q in qs {
            let mut eq = prepared();
            let mut cfg = band(shape, 6.0, 2.0);
            cfg.q = q;
            eq.set_band(0, cfg);
            assert_eq!(
                eq.last_design_error(0),
                Some(if shape == SHELF_ALT {
                    eq_dsp::Error::UnsupportedFilter
                } else {
                    eq_dsp::Error::UnstableFilter
                })
            );
            let out = render(&mut eq, &input, &input);
            assert!(out.iter().all(|x| x.is_finite() && x.abs() < 10.0));
        }
    }
}

/// Host slope indices beyond the supported range saturate at the steepest
/// choice. They never wrap around to a shallow cut.
#[test]
fn excessive_host_slopes_saturate_at_brickwall() {
    let mut eq = prepared();
    eq.set_band(0, band(3, 0.0, 10.0));
    let steepest = eq.static_magnitude_db(250.0);
    for slope in [12.0, 15.25, 16.0] {
        eq.set_band(0, band(3, 0.0, slope));
        assert_eq!(eq.static_magnitude_db(250.0).to_bits(), steepest.to_bits());
    }
}

/// An unidentified shape is rejected rather than substituted with an allpass.
#[test]
fn unsupported_bandpass_variant_is_rejected() {
    let mut eq = prepared();
    eq.set_band(0, band(12, 0.0, 2.0));
    assert_eq!(
        eq.last_design_error(0),
        Some(eq_dsp::Error::UnsupportedFilter)
    );
}

#[test]
fn no_shape_is_unstable_at_extreme_settings() {
    // Every pinnable shape, at maximum boost and cut, at the extremes of Q and
    // at both ends of the spectrum. A filter design that goes unstable does it
    // in the corners, and a reference vector recorded in the middle never sees
    // it — which is how the two shapes above went unnoticed.
    let input = signal::widen(&signal::transients(LEN, GENERATOR_RATE));
    let mut bad = Vec::new();
    for (name, shape) in SHAPES {
        for gain in [-30.0_f64, 30.0] {
            // `band_shelf` is excluded from the low-Q corners it is known to
            // fail in; that failure has its own test above rather than being
            // rediscovered here.
            let corners: &[(f64, f64)] = if shape == BAND_SHELF {
                &[(20.0, 1.0), (20_000.0, 40.0), (1_000.0, 2.0)]
            } else {
                &[(20.0, 0.1), (20_000.0, 40.0), (1_000.0, 0.025)]
            };
            for &(freq, q) in corners {
                let mut eq = prepared();
                let mut cfg = band(shape, gain, 2.0);
                cfg.freq_hz = freq;
                cfg.q = q;
                eq.set_band(0, cfg);
                let out = render(&mut eq, &input, &input);
                let peak = out.iter().fold(0.0_f64, |m, s| m.max(s.abs()));
                if !(peak.is_finite() && peak < 1e4) {
                    bad.push(format!(
                        "{name:>18} {gain:+5.0} dB {freq:>8.0} Hz Q {q:<6} -> {peak}"
                    ));
                }
            }
        }
    }
    assert!(bad.is_empty(), "unstable:\n{}", bad.join("\n"));
}

#[test]
fn processing_allocates_nothing() {
    let mut eq = prepared();
    eq.set_band(0, band(0, 6.0, 2.0));
    eq.set_band(1, band(3, 0.0, 4.0));
    eq.set_band_dynamics(
        0,
        BandDynamics {
            range_db: -6.0,
            ..BandDynamics::default()
        },
    );

    let mut left = signal::widen(&signal::noise(512, 5));
    let mut right = signal::widen(&signal::noise(512, 6));

    // Warm up outside the guard: `prepare` and the first block may size buffers.
    eq.process(&mut left, &mut right);
    dsp_golden::assert_no_alloc(|| eq.process(&mut left, &mut right));
}
