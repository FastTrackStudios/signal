//! Bluebox behaves like the two-tube processor it models: it starts on its
//! operating point without a thump, is unity and non-inverting at its
//! defaults, compresses where the reference does, mixes coherently with a
//! latency-aligned dry path, mutes on closed knobs, glides rather than
//! clicks — and its tabulated curves are its equations.

use dsp_core::num::{count_to_f64, f64_to_index};
use saturate_dsp::bluebox::{Bluebox, BlueboxParams, Cal, SatFreq};

const SR: f64 = 48_000.0;

fn sine(hz: f64, amp: f64, secs: f64, sr: f64) -> Vec<f64> {
    (0..f64_to_index(secs * sr)).map(|i| amp * (2.0 * std::f64::consts::PI * hz * count_to_f64(i) / sr).sin()).collect()
}

fn run_on(fx: &mut Bluebox, x: &[f64]) -> Vec<f64> {
    x.iter().map(|&s| fx.process(s, s).0).collect()
}

fn run(p: BlueboxParams, x: &[f64], sr: f64) -> Vec<f64> {
    let mut fx = Bluebox::new(sr);
    fx.set(p);
    run_on(&mut fx, x)
}

/// Complex amplitude of `hz` over the last `secs` of `y` (a whole number of
/// cycles), relative to a unit sine starting at sample 0 delayed by `delay`.
fn phasor(y: &[f64], hz: f64, secs: f64, delay: usize, sr: f64) -> (f64, f64) {
    let n = f64_to_index(secs * sr);
    let start = y.len().saturating_sub(n);
    let (mut re, mut im) = (0.0, 0.0);
    for (i, v) in y.iter().enumerate().skip(start) {
        let t = (count_to_f64(i) - count_to_f64(delay)) / sr;
        let w = 2.0 * std::f64::consts::PI * hz * t;
        re += v * w.sin();
        im += v * w.cos();
    }
    let (re, im) = (2.0 * re / count_to_f64(n), 2.0 * im / count_to_f64(n));
    (re.hypot(im), im.atan2(re).to_degrees())
}

/// 1 kHz gain in dB at `amp` (peak, full scale = 1).
fn gain_db(p: BlueboxParams, amp: f64) -> f64 {
    let x = sine(1000.0, amp, 0.5, SR);
    let mut fx = Bluebox::new(SR);
    fx.set(p);
    let lat = fx.latency();
    let y = run_on(&mut fx, &x);
    20.0 * (phasor(&y, 1000.0, 0.2, lat, SR).0 / amp).log10()
}

#[test]
fn starts_on_its_operating_point_without_a_thump() {
    let settings = [
        BlueboxParams::default(),
        BlueboxParams { pentode: 100.0, triode: 100.0, density: 100.0, ..BlueboxParams::default() },
        BlueboxParams { saturation_in: true, saturation: 100.0, alt_tube: true, saturation_freq: SatFreq::Low, ..BlueboxParams::default() },
        BlueboxParams { air_in: true, air: 100.0, cal: Cal::Bright, output: 100.0, ..BlueboxParams::default() },
    ];
    for p in settings {
        let mut fx = Bluebox::new(SR);
        fx.set(p);
        let worst = run_on(&mut fx, &vec![0.0; 48_000]).iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        assert!(worst < 1e-9, "{p:?}: silence comes out at {worst:e}");
        // and again after a reset in the middle of a loud passage
        run_on(&mut fx, &sine(50.0, 1.0, 0.3, SR));
        fx.reset();
        let worst = run_on(&mut fx, &vec![0.0; 4_800]).iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        assert!(worst < 1e-9, "{p:?}: after reset {worst:e}");
    }
}

#[test]
fn defaults_are_unity_and_non_inverting() {
    let x = sine(1000.0, 0.01, 0.5, SR);
    let mut fx = Bluebox::new(SR);
    fx.set(BlueboxParams::default());
    let lat = fx.latency();
    let y = run_on(&mut fx, &x);
    let (mag, deg) = phasor(&y, 1000.0, 0.2, lat, SR);
    let db = 20.0 * (mag / 0.01).log10();
    // the reference: −0.05 dB and +2.5° at 1 kHz
    assert!(db.abs() < 0.3, "small-signal gain {db:.2} dB");
    assert!(deg.abs() < 10.0, "phase {deg:.1}° — inverted or misaligned");
    // and the impulse peaks, positive, exactly at the reported latency
    let mut imp = vec![0.0; 400];
    imp[100] = 0.01;
    let mut fx = Bluebox::new(SR);
    fx.set(BlueboxParams::default());
    let y = run_on(&mut fx, &imp);
    let peak = y.iter().enumerate().fold((0, 0.0_f64), |(bi, bv), (i, &v)| if v.abs() > bv.abs() { (i, v) } else { (bi, bv) });
    assert_eq!(peak.0, 100 + lat, "impulse peak");
    assert!(peak.1 > 0.0, "impulse peak is negative");
}

#[test]
fn latency_follows_the_oversampling_factor() {
    for (sr, lat) in [(44_100.0, 37), (48_000.0, 37), (96_000.0, 31), (192_000.0, 0)] {
        assert_eq!(Bluebox::new(sr).latency(), lat, "at {sr}");
    }
}

#[test]
fn compression_knee_sits_where_the_reference_has_it() {
    // Pentode 50, Triode 50, Density 0: the reference's 1 kHz gain at 0, +6
    // and +12 dBFS peak, re its small-signal gain
    let p = BlueboxParams::default();
    let quiet = gain_db(p, 0.01);
    for (dbfs, want) in [(0.0, -0.8), (6.0, -4.0), (12.0, -9.4)] {
        let got = gain_db(p, 10f64.powf(dbfs / 20.0)) - quiet;
        assert!((got - want).abs() < 0.5, "at {dbfs:+} dBFS: {got:.2} dB, the reference {want}");
    }
}

#[test]
fn mix_zero_is_the_dry_signal_delayed_by_the_latency() {
    let x = sine(440.0, 0.5, 0.25, SR);
    let mut fx = Bluebox::new(SR);
    fx.set(BlueboxParams { pentode: 100.0, triode: 100.0, mix: 0.0, ..BlueboxParams::default() });
    let lat = fx.latency();
    let y = run_on(&mut fx, &x);
    // bit for bit
    assert!(y.iter().take(lat).all(|v| v.to_bits() == 0));
    assert!(x.iter().zip(y.iter().skip(lat)).all(|(a, b)| a.to_bits() == b.to_bits()), "Mix 0 is not exactly the dry signal");
}

#[test]
fn half_mix_is_coherent_across_the_band() {
    // a misaligned dry path would comb: notches where the two cancel
    let p = BlueboxParams { mix: 50.0, ..BlueboxParams::default() };
    let wet = BlueboxParams::default();
    for hz in [50.0, 200.0, 1000.0, 3000.0, 6000.0, 10_000.0, 14_000.0, 18_000.0] {
        let x = sine(hz, 0.01, 0.4, SR);
        let mut a = Bluebox::new(SR);
        a.set(p);
        let mut b = Bluebox::new(SR);
        b.set(wet);
        let lat = a.latency();
        let (ma, _) = phasor(&run_on(&mut a, &x), hz, 0.1, lat, SR);
        let (mb, _) = phasor(&run_on(&mut b, &x), hz, 0.1, lat, SR);
        // half wet + half dry lands between the two, never below both
        let mix_db = 20.0 * (ma / 0.01).log10();
        let wet_db = 20.0 * (mb / 0.01).log10();
        assert!(mix_db >= wet_db.min(0.0) - 0.05 && mix_db <= wet_db.max(0.0) + 0.05, "{hz} Hz: mix {mix_db:.2} wet {wet_db:.2}");
    }
}

#[test]
fn closed_knobs_mute() {
    for p in [
        BlueboxParams { pentode: 0.0, ..BlueboxParams::default() },
        BlueboxParams { triode: 0.0, ..BlueboxParams::default() },
        BlueboxParams { output: 0.0, ..BlueboxParams::default() },
    ] {
        assert!(gain_db(p, 0.1) < -80.0, "{p:?}");
    }
}

#[test]
fn density_keeps_quiet_levels_and_saturation_adds_level() {
    let base = gain_db(BlueboxParams::default(), 0.01);
    // the reference: flat to Density +50, then +1.9 dB at +100
    for (d, want) in [(-100.0, 0.0), (-50.0, 0.0), (50.0, 0.0), (100.0, 2.0)] {
        let g = gain_db(BlueboxParams { density: d, ..BlueboxParams::default() }, 0.01) - base;
        assert!((g - want).abs() < 0.2, "Density {d}: {g:.2} dB");
    }
    // the parallel path sums in phase: +6.4 dB at Saturation 100 on the reference
    let sat = gain_db(BlueboxParams { saturation_in: true, saturation: 100.0, ..BlueboxParams::default() }, 0.01) - base;
    assert!((sat - 6.45).abs() < 0.3, "saturation added {sat:.2} dB");
}

#[test]
fn every_drive_meets_the_same_ceiling() {
    // +12 dBFS in: the output sits ≈ 9 dB under the input at every
    // Pentode/Triode setting — the last stage clips at a fixed level
    let levels: Vec<f64> = [(50.0, 50.0), (100.0, 50.0), (50.0, 100.0), (100.0, 100.0)]
        .iter()
        .map(|&(pentode, triode)| gain_db(BlueboxParams { pentode, triode, ..BlueboxParams::default() }, 4.0))
        .collect();
    let (lo, hi) = levels.iter().fold((f64::MAX, f64::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    assert!(hi - lo < 1.5, "ceilings spread {levels:?}");
    assert!((-10.5..-7.5).contains(&lo), "ceiling {levels:?}");
}

#[test]
fn switching_glides_instead_of_clicking() {
    // a quiet low tone; flip every switch at once mid-stream: the output's
    // sample-to-sample step must not exceed the tone's own, before or after
    let x = sine(100.0, 0.05, 1.5, SR);
    let mut fx = Bluebox::new(SR);
    // (Sat Freq stays put: moving it crossfades the band over 0.15 s,
    // which is a change of sound, not a click)
    fx.set(BlueboxParams { saturation_freq: SatFreq::High, ..BlueboxParams::default() });
    let mut y = run_on(&mut fx, &x[..24_000]);
    fx.set(BlueboxParams {
        saturation_in: true,
        saturation: 100.0,
        saturation_freq: SatFreq::High,
        alt_tube: true,
        air_in: true,
        air: 100.0,
        cal: Cal::Bright,
        pentode: 80.0,
        output: 70.0,
        mix: 60.0,
        ..BlueboxParams::default()
    });
    y.extend(run_on(&mut fx, &x[24_000..]));
    let step = |r: core::ops::Range<usize>| y[r].windows(2).fold(0.0_f64, |m, w| m.max((w[1] - w[0]).abs()));
    // the louder, brighter setting after the glides have landed
    let steady = step(12_000..24_000).max(step(60_000..72_000));
    let switched = step(24_000..36_000);
    assert!(switched < 1.2 * steady, "switching stepped {switched:e} vs {steady:e} steady");
}

#[test]
fn tabulated_curves_null_against_their_equations() {
    let hot = BlueboxParams {
        pentode: 100.0,
        triode: 90.0,
        saturation_in: true,
        saturation: 80.0,
        alt_tube: true,
        air_in: true,
        air: 70.0,
        density: 60.0,
        ..BlueboxParams::default()
    };
    for p in [BlueboxParams::default(), hot] {
        let x: Vec<f64> = sine(220.0, 0.9, 0.2, SR).iter().zip(sine(3100.0, 0.3, 0.2, SR)).map(|(a, b)| a + b).collect();
        let mut fast = Bluebox::new(SR);
        fast.set(p);
        let mut exact = Bluebox::new_exact(SR);
        exact.set(p);
        let a = run_on(&mut fast, &x);
        let b = run_on(&mut exact, &x);
        let err = a.iter().zip(&b).map(|(u, v)| (u - v) * (u - v)).sum::<f64>();
        let sig = b.iter().map(|v| v * v).sum::<f64>();
        let db = 10.0 * (err / sig).log10();
        assert!(db < -110.0, "{p:?}: table vs closed form {db:.1} dB");
    }
}

#[test]
fn stays_finite_at_every_rate_and_extreme() {
    for sr in [44_100.0, 48_000.0, 96_000.0, 192_000.0] {
        let mut seed = 7u32;
        let noise: Vec<f64> = (0..48_000)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (f64::from(seed) / f64::from(u32::MAX) - 0.5) * 8.0
            })
            .collect();
        for alt in [false, true] {
            let p = BlueboxParams {
                pentode: 100.0,
                triode: 100.0,
                saturation_in: true,
                saturation: 100.0,
                saturation_freq: SatFreq::Low,
                alt_tube: alt,
                air_in: true,
                air: 100.0,
                density: 100.0,
                input_db: 12.0,
                output: 100.0,
                ..BlueboxParams::default()
            };
            let y = run(p, &noise, sr);
            let worst = y.iter().fold(0.0_f64, |m, v| if v.is_finite() { m.max(v.abs()) } else { f64::INFINITY });
            // +12 dB in, +6 dB out and full Air on noise peaking at ±4: finite and
            // bounded is the claim, not a particular level
            assert!(worst < 16.0, "sr {sr} alt {alt}: peak {worst}");
        }
    }
}
