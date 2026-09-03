//! The one EQ engine, driven directly.
//!
//! `FtsEq` is what both front ends play through now — the rig's EQ block and
//! the FTS-EQ plugin. These cover it at its own interface, with no parameter
//! ids and no host in the way, so a failure here is the engine's and not a
//! mapping's.

// TEMPORARY: DSP rewrite pending — see the note in this crate's src/lib.rs.
// A test/example target is its own crate, so the crate-root allow there does
// not reach this file and it needs its own copy.
#![allow(
    arithmetic_side_effects,
    as_conversions,
    cast_possible_truncation,
    cast_precision_loss,
    cast_sign_loss,
    clippy::allow_attributes,
    clippy::allow_attributes_without_reason,
    indexing_slicing,
    many_single_char_names,
    reason = "pending the DSP algorithm rewrite"
)]


use eq_dsp::band::Placement;
use eq_dsp::engine::{BandConfig, BandDynamics, FtsEq};

const SR: f64 = 48_000.0;

/// Goertzel power of `freq` over `buf`.
fn goertzel(buf: &[f64], freq: f64) -> f64 {
    let w = std::f64::consts::TAU * freq / SR;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for &x in buf {
        let s0 = x + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (coeff * s1).mul_add(-s2, s1.mul_add(s1, s2 * s2)) / (buf.len() as f64).powi(2)
}

fn tone(freq: f64, amplitude: f64, frames: usize) -> Vec<f64> {
    let inc = std::f64::consts::TAU * freq / SR;
    (0..frames).map(|i| amplitude * (inc * i as f64).sin()).collect()
}

/// Run a signal through and return the settled second half.
fn render(eq: &mut FtsEq, input: &[f64]) -> Vec<f64> {
    let mut out = Vec::with_capacity(input.len());
    let mut pos = 0;
    while pos < input.len() {
        let n = 256.min(input.len() - pos);
        let mut l = input[pos..pos + n].to_vec();
        let mut r = l.clone();
        eq.process(&mut l, &mut r);
        out.extend_from_slice(&l);
        pos += n;
    }
    out.split_off(out.len() / 2)
}

fn bell(freq: f64, gain_db: f64, q: f64) -> BandConfig {
    BandConfig {
        used: true,
        enabled: true,
        freq_hz: freq,
        gain_db,
        q,
        ..BandConfig::default()
    }
}

/// A static band cuts where it is aimed.
#[test]
fn a_static_band_shapes_the_signal() {
    let mut eq = FtsEq::new(SR);
    eq.prepare(SR, 256);
    eq.set_band(0, bell(1000.0, -12.0, 2.0));

    let input = tone(1000.0, 0.5, (SR * 0.5) as usize);
    let out = render(&mut eq, &input);
    let dry = input.split_at(input.len() / 2).1.to_vec();
    let db = 10.0 * (goertzel(&out, 1000.0) / goertzel(&dry, 1000.0)).log10();
    assert!(
        (db + 12.0).abs() < 1.0,
        "a -12 dB bell applied {db:+.2} dB",
    );
}

/// The engine is silent-in, silent-out and transparent when nothing is set.
#[test]
fn an_untouched_engine_is_a_pass_through() {
    let mut eq = FtsEq::new(SR);
    eq.prepare(SR, 256);
    let input = tone(1000.0, 0.5, 4096);
    let out = render(&mut eq, &input);
    let dry = &input[input.len() / 2..];
    let worst = out
        .iter()
        .zip(dry.iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f64, f64::max);
    assert!(worst < 1e-12, "an idle EQ must not touch the signal ({worst:e})");
}

/// Dynamics reach the filter — the property the plugin used to lack entirely.
#[test]
fn a_dynamic_band_rides_with_level() {
    let gain_at = |amplitude: f64| -> f64 {
        let mut eq = FtsEq::new(SR);
        eq.prepare(SR, 256);
        eq.set_band(0, bell(1000.0, 0.0, 1.0));
        eq.set_band_dynamics(
            0,
            BandDynamics {
                range_db: -12.0,
                threshold_db: -30.0,
                attack_pct: 10.0,
                release_pct: 30.0,
                auto: false,
                ..BandDynamics::default()
            },
        );
        let input = tone(1000.0, amplitude, (SR * 2.0) as usize);
        let out = render(&mut eq, &input);
        let expected = amplitude / std::f64::consts::SQRT_2;
        let rms = (out.iter().map(|s| s * s).sum::<f64>() / out.len() as f64).sqrt();
        20.0 * (rms / expected).log10()
    };

    let quiet = gain_at(0.003);
    let loud = gain_at(0.7);
    assert!(quiet.abs() < 1.5, "a quiet signal should pass ({quiet:+.2} dB)");
    assert!(
        loud < quiet - 3.0,
        "a loud signal should be pulled down: {quiet:+.2} against {loud:+.2} dB",
    );
}

/// A spectral band puts the STFT in the path and reports its latency.
#[test]
fn a_spectral_band_engages_the_stft() {
    let mut eq = FtsEq::new(SR);
    eq.prepare(SR, 256);
    eq.set_band(0, bell(1000.0, 0.0, 1.0));
    assert_eq!(eq.latency(), 0, "a plain band adds no latency");

    eq.set_band_dynamics(
        0,
        BandDynamics {
            range_db: -12.0,
            spectral: true,
            ..BandDynamics::default()
        },
    );
    assert!(eq.spectral_engaged(), "a spectral band must engage the engine");
    assert!(eq.latency() > 0, "and it costs latency, which a host must report");
}

/// Stereo placement is honoured — the rig had it, the plugin did not.
#[test]
fn placement_restricts_a_band_to_its_side() {
    let mut eq = FtsEq::new(SR);
    eq.prepare(SR, 256);
    eq.set_band(
        0,
        BandConfig {
            placement: Placement::Left,
            ..bell(1000.0, -18.0, 2.0)
        },
    );

    let input = tone(1000.0, 0.5, (SR * 0.5) as usize);
    let mut l = input.clone();
    let mut r = input.clone();
    let mut pos = 0;
    while pos < input.len() {
        let n = 256.min(input.len() - pos);
        let (a, b) = (&mut l[pos..pos + n], &mut r[pos..pos + n]);
        eq.process(a, b);
        pos += n;
    }
    let half = input.len() / 2;
    let left_db = 10.0 * (goertzel(&l[half..], 1000.0) / goertzel(&input[half..], 1000.0)).log10();
    let right_db = 10.0 * (goertzel(&r[half..], 1000.0) / goertzel(&input[half..], 1000.0)).log10();
    assert!(left_db < -10.0, "the left side should be cut ({left_db:+.2} dB)");
    assert!(right_db.abs() < 0.5, "the right side must be untouched ({right_db:+.2} dB)");
}

/// Every band index the engine advertises actually works.
///
/// The plugin used to pre-allocate its own 24 bands and now relies on the
/// engine to have done it. If the engine ever carried fewer, the top bands
/// would silently do nothing — a preset using band 20 would load without
/// complaint and sound wrong.
#[test]
fn every_band_index_is_live() {
    for i in 0..eq_dsp::engine::EQ_BANDS {
        let mut eq = FtsEq::new(SR);
        eq.prepare(SR, 256);
        eq.set_band(i, bell(1000.0, -18.0, 2.0));

        let input = tone(1000.0, 0.5, 8192);
        let out = render(&mut eq, &input);
        let dry = &input[input.len() / 2..];
        let db = 10.0 * (goertzel(&out, 1000.0) / goertzel(dry, 1000.0)).log10();
        assert!(
            db < -10.0,
            "band {i} did not filter — it applied {db:+.2} dB",
        );
    }
}

/// Out-of-range band indices are ignored rather than panicking.
#[test]
fn a_band_index_past_the_end_is_ignored() {
    let mut eq = FtsEq::new(SR);
    eq.prepare(SR, 256);
    eq.set_band(eq_dsp::engine::EQ_BANDS, bell(1000.0, -18.0, 2.0));
    eq.set_band_dynamics(999, BandDynamics::default());

    let input = tone(1000.0, 0.5, 4096);
    let out = render(&mut eq, &input);
    let dry = &input[input.len() / 2..];
    let worst = out
        .iter()
        .zip(dry.iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f64, f64::max);
    assert!(worst < 1e-12, "a rejected band must leave the signal alone");
}

// ── Per-band spectral controls ─────────────────────────────────────────────

/// Deterministic noise — a fixed spectrum keeps the assertions stable.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        ((self.0 >> 33) as f64 / (1u64 << 31) as f64) - 1.0
    }
}

/// Goertzel power at `freq`.
fn power_at(buf: &[f64], freq: f64) -> f64 {
    goertzel(buf, freq)
}

/// Noise with resonances at each of `tones`.
fn resonant_noise(tones: &[(f64, f64)], frames: usize) -> Vec<f64> {
    let mut rng = Lcg(0xBEEF_0001);
    let mut phases = vec![0.0f64; tones.len()];
    (0..frames)
        .map(|_| {
            let mut s = 0.05 * rng.next();
            for (i, (f, a)) in tones.iter().enumerate() {
                s += a * phases[i].sin();
                phases[i] += std::f64::consts::TAU * f / SR;
            }
            s
        })
        .collect()
}

fn spectral_band(range_db: f64, density: f64, tilt: bool) -> BandDynamics {
    BandDynamics {
        range_db,
        threshold_db: -40.0,
        attack_pct: 20.0,
        release_pct: 40.0,
        auto: false,
        spectral: true,
        spectral_density: density,
        spectral_tilt: tilt,
        ..BandDynamics::default()
    }
}

/// Density sets how WIDE the reduction is, not how deep.
///
/// This test used to assert that density deepened the cut at the resonance
/// itself. Measured against Pro-Q 4 that is not what the control does: the
/// depth is the band's range at every density (an 18 dB band takes its
/// resonance down 17.99 dB whatever Density says), and what changes is how
/// much of the neighbourhood comes with it. With an 18 dB band and a
/// resonance at 1 kHz, a bin 109 Hz away came down 13 dB at Density 0, 5.5 dB
/// at 25, and was left alone at 50 and above.
///
/// So the reduction is measured beside the resonance, not on it.
#[test]
fn spectral_density_sets_the_width_not_the_depth() {
    let beside = 1090.0;
    let probe = |density: f64, at_hz: f64| -> f64 {
        let mut eq = FtsEq::new(SR);
        eq.prepare(SR, 256);
        eq.set_band(0, bell(1000.0, 0.0, 1.0));
        eq.set_band_dynamics(0, spectral_band(-18.0, density, false));
        let input = resonant_noise(&[(1000.0, 0.35)], (SR * 3.0) as usize);
        let out = render(&mut eq, &input);
        let dry = &input[input.len() / 2..];
        10.0 * (power_at(&out, at_hz) / power_at(dry, at_hz)).log10()
    };

    // The resonance itself comes down by the band's range either way.
    for density in [0.0, 100.0] {
        let centre = probe(density, 1000.0);
        assert!(
            (centre + 18.0).abs() < 2.0,
            "the resonance should come down the band's 18 dB at density \
             {density}, measured {centre:+.2}",
        );
    }

    // Its neighbour does not.
    let broad = probe(0.0, beside);
    let surgical = probe(100.0, beside);
    assert!(
        broad < surgical - 5.0,
        "a low density must take the neighbourhood with it and a high one must \
         not: {broad:+.2} dB at 0% against {surgical:+.2} at 100%",
    );
    assert!(
        surgical.abs() < 2.0,
        "at full density a bin beside the resonance should be left alone \
         ({surgical:+.2} dB)",
    );
}

/// Two bands in one instance can carry different densities.
#[test]
fn two_spectral_bands_keep_their_own_density() {
    let mut eq = FtsEq::new(SR);
    eq.prepare(SR, 256);
    eq.set_band(0, bell(500.0, 0.0, 4.0));
    eq.set_band(1, bell(5000.0, 0.0, 4.0));
    eq.set_band_dynamics(0, spectral_band(-24.0, 0.0, false));
    eq.set_band_dynamics(1, spectral_band(-24.0, 100.0, false));

    // Both regions must exist; a single global density could not describe
    // this instance at all.
    assert!(eq.spectral_engaged());

    let input = resonant_noise(&[(500.0, 0.3), (5000.0, 0.3)], (SR * 3.0) as usize);
    let out = render(&mut eq, &input);
    let dry = &input[input.len() / 2..];
    let low = 10.0 * (power_at(&out, 500.0) / power_at(dry, 500.0)).log10();
    let high = 10.0 * (power_at(&out, 5000.0) / power_at(dry, 5000.0)).log10();
    assert!(low < -2.0, "the low resonance should be suppressed ({low:+.2} dB)");
    assert!(high < -2.0, "the high resonance should be suppressed ({high:+.2} dB)");
}

/// Tilt weights the trigger by frequency.
///
/// Rewritten alongside the density test and for the same reason: with the
/// depth capped at the band's range, a resonance that triggers at all comes
/// down by the full range whether tilted or not, so tilt cannot be read off
/// the depth of a strong one. What it changes is the prominence needed to
/// trigger.
///
/// The contrast is high against low, because tilt pushes them opposite ways:
/// a pink-weighted trigger adds ~9 dB of apparent prominence at 8 kHz and
/// takes ~9 dB away at 125 Hz. Untilted, the same resonance at either
/// frequency is judged the same.
#[test]
fn spectral_tilt_weights_the_trigger_by_frequency() {
    let reduction = |freq: f64, tilt: bool| -> f64 {
        let mut eq = FtsEq::new(SR);
        eq.prepare(SR, 256);
        eq.set_band(0, bell(freq, 0.0, 1.0));
        eq.set_band_dynamics(0, spectral_band(-18.0, 100.0, tilt));
        let input = resonant_noise(&[(freq, 0.05)], (SR * 3.0) as usize);
        let out = render(&mut eq, &input);
        let dry = &input[input.len() / 2..];
        10.0 * (power_at(&out, freq) / power_at(dry, freq)).log10()
    };

    let (low_plain, high_plain) = (reduction(125.0, false), reduction(8000.0, false));
    let (low_tilt, high_tilt) = (reduction(125.0, true), reduction(8000.0, true));

    // Untilted, frequency does not decide the outcome.
    assert!(
        (low_plain - high_plain).abs() < 3.0,
        "without tilt the two should be judged alike: {low_plain:+.2} at \
         125 Hz against {high_plain:+.2} at 8 kHz",
    );
    // Tilted, the high one is favoured over the low one.
    let plain_gap = high_plain - low_plain;
    let tilt_gap = high_tilt - low_tilt;
    assert!(
        tilt_gap < plain_gap - 1.0,
        "tilt should favour the high resonance over the low one: gap \
         {plain_gap:+.2} dB without it against {tilt_gap:+.2} with it",
    );
}

// ── Side-chain range ───────────────────────────────────────────────────────

/// A filtered side-chain listens where it is told, not to itself.
///
/// Unfiltered, a dynamic band triggers on its own region — which is what makes
/// it self-regulating. Filtered, it triggers on a range someone else chose, so
/// one region can duck because a different one got loud. 57 bands in the
/// factory library rely on it.
#[test]
fn a_filtered_side_chain_listens_to_its_own_range() {
    // The band sits at 5 kHz; the side-chain listens at 200 Hz.
    let build = |filtered: bool| {
        let mut eq = FtsEq::new(SR);
        eq.prepare(SR, 256);
        eq.set_band(0, bell(5000.0, 0.0, 1.0));
        eq.set_band_dynamics(
            0,
            BandDynamics {
                range_db: -18.0,
                // Above the noise floor in the band's own region, below the
                // loud low tone. Otherwise the linked case triggers on noise
                // and the two cases stop being distinguishable.
                threshold_db: -20.0,
                attack_pct: 10.0,
                release_pct: 30.0,
                auto: false,
                side_filtered: filtered,
                side_lo_hz: 100.0,
                side_hi_hz: 300.0,
                ..BandDynamics::default()
            },
        );
        eq
    };

    // Loud low tone, quiet high tone. Only a band listening low should duck.
    let input = resonant_noise(&[(200.0, 0.7), (5000.0, 0.02)], (SR * 3.0) as usize);
    let dry = &input[input.len() / 2..];

    let mut linked = build(false);
    let linked_out = render(&mut linked, &input);
    let mut freed = build(true);
    let freed_out = render(&mut freed, &input);

    let linked_db = 10.0 * (power_at(&linked_out, 5000.0) / power_at(dry, 5000.0)).log10();
    let freed_db = 10.0 * (power_at(&freed_out, 5000.0) / power_at(dry, 5000.0)).log10();

    assert!(
        linked_db.abs() < 1.5,
        "listening to itself, the quiet 5 kHz band should sit still \
         ({linked_db:+.2} dB)",
    );
    assert!(
        freed_db < linked_db - 3.0,
        "listening at 200 Hz, the loud low tone should duck it: \
         {freed_db:+.2} dB against {linked_db:+.2} dB",
    );
}

// ── Continuous slope ───────────────────────────────────────────────────────

/// Average dB/oct of a rendered band between two frequencies.
fn measured_slope(eq: &mut FtsEq, lo: f64, hi: f64) -> f64 {
    let at = |eq: &mut FtsEq, f: f64| -> f64 {
        let input = tone(f, 0.3, 24_000);
        let out = render(eq, &input);
        let dry = &input[input.len() / 2..];
        10.0 * (goertzel(&out, f) / goertzel(dry, f)).log10()
    };
    let a = at(eq, lo);
    let b = at(eq, hi);
    (b - a) / (hi / lo).log2()
}

fn cut(shape: u32, freq: f64, slope: f64) -> BandConfig {
    BandConfig {
        used: true,
        enabled: true,
        freq_hz: freq,
        gain_db: 0.0,
        q: 0.707,
        shape,
        slope,
        ..BandConfig::default()
    }
}

/// A low cut rolls off at the rate it was asked for, integer or not.
///
/// Pro-Q's slope control is continuous and 137 bands in its factory library
/// sit between the integer orders.
#[test]
fn a_cut_rolls_off_at_its_continuous_slope() {
    // Shape 3 is the engine's Low Cut.
    for (slope, want) in [(1.0f64, 6.0f64), (1.5, 9.0), (2.0, 12.0), (2.5, 15.0)] {
        let mut eq = FtsEq::new(SR);
        eq.prepare(SR, 256);
        eq.set_band(0, cut(3, 1000.0, slope));
        // Measured below the corner, clear of the knee.
        let got = measured_slope(&mut eq, 125.0, 500.0);
        assert!(
            (got - want).abs() < 2.5,
            "slope {slope} should roll at {want} dB/oct, measured {got:.2}",
        );
    }
}

/// A fractional slope sits between its neighbours, monotonically.
#[test]
fn slope_is_monotonic_across_the_fraction() {
    let rate = |slope: f64| {
        let mut eq = FtsEq::new(SR);
        eq.prepare(SR, 256);
        eq.set_band(0, cut(3, 1000.0, slope));
        measured_slope(&mut eq, 125.0, 500.0)
    };
    let (a, b, c) = (rate(1.0), rate(1.5), rate(2.0));
    assert!(
        a < b + 0.5 && b < c + 0.5,
        "slopes must increase with the control: {a:.2} / {b:.2} / {c:.2}",
    );
}

/// A bell with a fractional slope stays a bell.
///
/// This is the regression the ladder caused when it was applied to every
/// shape: a bell's slope is the steepness of a bounded skirt, not a one-sided
/// roll-off, so a ladder tilted everything above it. On one factory preset a
/// 2.45-slope bell at 2.2 kHz picked up an unasked-for high cut worth 98 dB of
/// mean error against the plugin.
#[test]
fn a_fractional_bell_does_not_tilt_the_spectrum() {
    let mut eq = FtsEq::new(SR);
    eq.prepare(SR, 256);
    eq.set_band(
        0,
        BandConfig {
            slope: 2.4478,
            ..bell(2193.0, 4.5, 1.0)
        },
    );

    // Far above the bell, the response must return to unity — a bell is
    // bounded on both sides.
    for f in [12_000.0, 16_000.0] {
        let input = tone(f, 0.3, 24_000);
        let out = render(&mut eq, &input);
        let dry = &input[input.len() / 2..];
        let db = 10.0 * (goertzel(&out, f) / goertzel(dry, f)).log10();
        assert!(
            db.abs() < 1.0,
            "a bell must leave {f:.0} Hz alone, but it moved {db:+.2} dB",
        );
    }
}

/// A shelf with a fractional slope still settles at its gain.
#[test]
fn a_fractional_shelf_still_plateaus() {
    let mut eq = FtsEq::new(SR);
    eq.prepare(SR, 256);
    eq.set_band(
        0,
        BandConfig {
            shape: 2, // the engine's High Shelf
            slope: 2.4,
            ..bell(4000.0, 6.0, 0.707)
        },
    );
    let input = tone(16_000.0, 0.3, 24_000);
    let out = render(&mut eq, &input);
    let dry = &input[input.len() / 2..];
    let db = 10.0 * (goertzel(&out, 16_000.0) / goertzel(dry, 16_000.0)).log10();
    assert!(
        (db - 6.0).abs() < 1.5,
        "a +6 dB shelf should settle near +6 dB, measured {db:+.2}",
    );
}

/// A dynamic band pinned at full range is the same filter as a static one.
///
/// The two use different topologies — a state-variable filter so the dynamic
/// one's gain can move per sample, the MZT cascades for the static one — and
/// they have to agree anyway, because a preset does not know or care which
/// path its band took. The static designs already match Pro-Q exactly, so this
/// is the cheapest way to hold the dynamic path to the same standard: no
/// plugin, no reference capture.
///
/// Q is swept wide because that is where they came apart. A single scalar
/// lined the two up at Q 1 and nowhere else: at 0.05 the state-variable shelf
/// sat near its midpoint across the whole band while the cascade completed its
/// transition, and at 4 it overshot about twice as far. **109 of the 527
/// dynamic bands in the Pro-Q factory library sit outside Q 0.3..3.**
#[test]
fn a_dynamic_band_filters_like_the_static_one() {
    // Shape 2 is the engine's High Shelf, 1 its Low Shelf, 0 a Bell.
    for shape in [0u32, 1, 2] {
        for q in [0.05f64, 0.2, 0.5, 1.0, 2.0, 4.0, 8.0] {
            let render_at = |dynamic: bool, probe: f64| -> f64 {
                let mut eq = FtsEq::new(SR);
                eq.prepare(SR, 256);
                let cfg = BandConfig {
                    used: true,
                    enabled: true,
                    freq_hz: 1000.0,
                    // A static band carries the gain; a dynamic one starts flat
                    // and is driven to the same place by its range.
                    gain_db: if dynamic { 0.0 } else { -12.0 },
                    q,
                    shape,
                    ..BandConfig::default()
                };
                eq.set_band(0, cfg);
                if dynamic {
                    eq.set_band_dynamics(
                        0,
                        BandDynamics {
                            range_db: -12.0,
                            // Far below anything, so it sits pinned at full
                            // range and only its shape is under test.
                            threshold_db: -90.0,
                            attack_pct: 0.0,
                            release_pct: 0.0,
                            auto: false,
                            ..BandDynamics::default()
                        },
                    );
                }
                let input = tone(probe, 0.3, 48_000);
                let out = render(&mut eq, &input);
                let dry = &input[input.len() / 2..];
                10.0 * (goertzel(&out, probe) / goertzel(dry, probe)).log10()
            };

            for probe in [125.0, 500.0, 1000.0, 2000.0, 8000.0] {
                let statik = render_at(false, probe);
                let dynamic = render_at(true, probe);
                assert!(
                    (statik - dynamic).abs() < 1.5,
                    "shape {shape} Q {q} at {probe:.0} Hz: static {statik:+.2} dB \
                     against dynamic {dynamic:+.2} dB",
                );
            }
        }
    }
}
