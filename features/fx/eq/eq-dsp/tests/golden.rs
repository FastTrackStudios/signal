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

use dsp_golden::{golden, signal, Golden};
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
/// [`shelf_alt_is_unstable_at_every_q`]. `band_shelf` (10) is present but is
/// only pinned at Q >= 0.707; see [`band_shelf_is_unstable_below_q_0_707`].
const SHAPES: [(&str, u32); 12] = [
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
    ("band_pass_variant", 12),
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

/// `shelf_alt` (Pro-Q shape 12) never produces finite output.
///
/// Measured across Q from 0.025 to 40 and gains from -30 to +30 dB: every
/// combination diverges, most to `inf` and the rest to ~1.4e31. The shape is
/// reachable from a preset — `BandConfig::shape` is a plain `u32` that the
/// parameter layer passes through — so a factory patch using Pro-Q type 12
/// renders as silence or a full-scale burst.
///
/// Not fixed here: this pass is a refactor, and repairing a filter design is a
/// change to what the EQ sounds like. Pinned so it stays visible.
///
/// When it is fixed this test fails. Move `shelf_alt` into [`SHAPES`] and
/// record its vectors.
#[test]
fn shelf_alt_is_unstable_at_every_q() {
    let input = signal::widen(&signal::transients(LEN, GENERATOR_RATE));
    let mut finite = Vec::new();
    for q in [0.025_f64, 0.1, 0.5, 0.707, 1.2, 8.0, 40.0] {
        let mut eq = prepared();
        let mut cfg = band(SHELF_ALT, 6.0, 2.0);
        cfg.q = q;
        eq.set_band(0, cfg);
        let out = render(&mut eq, &input, &input);
        let peak = out.iter().fold(0.0_f64, |m, s| m.max(s.abs()));
        if peak.is_finite() && peak < 1e4 {
            finite.push(q);
        }
    }
    assert!(
        finite.is_empty(),
        "shelf_alt is now stable at Q {finite:?} — the design was fixed. Move it \
         into SHAPES, record its reference vectors, and delete this test."
    );
}

/// `band_shelf` (Pro-Q shape 10) diverges at low Q.
///
/// Mapped across Q 0.1 .. 40 and 20 Hz .. 20 kHz at +12 dB:
///
/// ```text
///   Q\Hz      20      50     120     300    1000    4000   12000   20000
///     0.1       X       X       X       X       X       X       X       X
///     0.3       X       X       X       X       X       X       X       X
///     0.5       X       X       X       X       X       X       X       X
///   0.707       X       .       .       .       .       .       .       .
///       1       .       .       .       .       .       .       .       .
/// ```
///
/// So: unstable at Q <= 0.5 at every frequency, and marginally at the default
/// Q of 0.707 at the very bottom of the range. Broad settings are exactly
/// where a shelf is most useful, and the parameter accepts them.
///
/// Same disposition as `shelf_alt`: pinned, not fixed.
#[test]
fn band_shelf_is_unstable_below_q_0_707() {
    let input = signal::widen(&signal::transients(LEN, GENERATOR_RATE));
    let peak_at = |q: f64| {
        let mut eq = prepared();
        let mut cfg = band(BAND_SHELF, 6.0, 2.0);
        cfg.q = q;
        eq.set_band(0, cfg);
        let out = render(&mut eq, &input, &input);
        out.iter().fold(0.0_f64, |m, s| m.max(s.abs()))
    };
    for q in [0.025_f64, 0.1, 0.3, 0.5] {
        let peak = peak_at(q);
        assert!(
            !(peak.is_finite() && peak < 1e4),
            "band_shelf is now stable at Q {q} (peak {peak}) — widen the pinned \
             range and re-record."
        );
    }
    for q in [1.0_f64, 1.2, 8.0] {
        let peak = peak_at(q);
        assert!(
            peak.is_finite() && peak < 1e4,
            "band_shelf broke at Q {q}: {peak}"
        );
    }
}

/// Slope stops increasing above 8 and wraps back to the slope-2 response.
///
/// `BandConfig::slope` is documented as continuous — "`slope * 6` dB/oct up to
/// 36, then the 48 / 72 / 96 / Brickwall steps ... a band can genuinely sit at
/// 7.5 or 15.25 dB/oct — 137 bands in the factory library do". Measured
/// attenuation an octave below a 1 kHz low cut says otherwise:
///
/// ```text
///   slope  1    ->  -12.3 dB
///   slope  2    ->  -23.9
///   slope  2.5  ->  -30.8
///   slope  4    ->  -48.1
///   slope  6    ->  -72.2
///   slope  7.5  -> -144.5
///   slope  8    -> -144.5
///   slope 12    ->  -23.9   <-- back to the slope-2 curve
///   slope 15.25 ->  -23.9
///   slope 16    ->  -23.9
/// ```
///
/// Two separate problems. The fractional ladder is not continuous — 7.5 gives
/// exactly what 8 gives, so the remainder is being rounded rather than
/// realized as poles and zeros. And anything from 12 upward silently produces
/// the *shallowest* useful slope instead of the steepest, which means a
/// factory preset asking for 72 dB/oct gets 12.
///
/// Not fixed here — this is a filter-design change, not a refactor. Pinned so
/// a rewrite cannot quietly alter it and so it stays visible.
#[test]
fn slope_is_not_continuous_and_wraps_above_eight() {
    let mut eq = prepared();
    let att = |eq: &mut FtsEq, slope: f64| {
        eq.set_band(0, band(3, 0.0, slope));
        eq.static_magnitude_db(250.0)
    };
    let shallow = att(&mut eq, 2.0);
    // The fractional step does nothing: 7.5 lands exactly on 8.
    assert_eq!(att(&mut eq, 7.5).to_bits(), att(&mut eq, 8.0).to_bits());
    // And the steep settings collapse back onto the slope-2 curve.
    for slope in [12.0_f64, 15.25, 16.0] {
        assert_eq!(
            att(&mut eq, slope).to_bits(),
            shallow.to_bits(),
            "slope {slope} no longer wraps to the slope-2 response — the design \
             was fixed. Re-record the slope vectors and delete this test."
        );
    }
}

/// `band_pass_variant` (Pro-Q shape 5) is flat — it does not filter.
///
/// Its magnitude is 0 dB at every frequency and its impulse response is
/// byte-identical to `all_pass`. Flat is correct for an allpass; for a
/// bandpass it means the shape falls through to the allpass path and never
/// applies its own design. Like the two unstable shapes, it is marked
/// "previously design-only" in `FilterShape`.
#[test]
fn band_pass_variant_does_not_filter() {
    let mut variant = prepared();
    variant.set_band(0, band(12, 0.0, 2.0));
    let mut allpass = prepared();
    allpass.set_band(0, band(9, 0.0, 2.0));
    for hz in [100.0_f64, 1_000.0, 5_000.0, 15_000.0] {
        assert_eq!(
            variant.static_magnitude_db(hz).to_bits(),
            allpass.static_magnitude_db(hz).to_bits(),
            "band_pass_variant now differs from all_pass at {hz} Hz — it was \
             given a real design. Re-record its vectors and delete this test."
        );
    }
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
