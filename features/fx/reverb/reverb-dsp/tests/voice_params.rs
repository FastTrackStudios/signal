//! BigSky MX pass-E: Voice pairs (MX/Classic), Hall Mid EQ + Swell,
//! named-Size selection. Defaults must be bit-transparent.

use reverb_dsp::algorithm::{ReverbVoice, SwellType};
use reverb_dsp::chain::ReverbChain;
use reverb_dsp::AlgorithmType;

use audiocore_dsp::{AudioConfig, Processor};

const SR: f64 = 48000.0;

fn config() -> AudioConfig {
    AudioConfig {
        sample_rate: SR,
        max_buffer_size: 512,
    }
}

fn make_chain(algo: AlgorithmType) -> ReverbChain {
    let mut c = ReverbChain::new();
    c.set_algorithm(algo);
    c.mix = 1.0;
    c.update(config());
    c
}

fn energy(buf: &[f64]) -> f64 {
    buf.iter().map(|s| s * s).sum()
}

fn goertzel(buf: &[f64], freq: f64) -> f64 {
    let w = std::f64::consts::TAU * freq / SR;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for &x in buf {
        let s0 = x + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (s1 * s1 + s2 * s2 - coeff * s1 * s2) / (buf.len() as f64).powi(2)
}

fn render_burst(chain: &mut ReverbChain, secs: f64) -> (Vec<f64>, Vec<f64>) {
    let n = (SR * secs) as usize;
    let burst = (SR * 0.4) as usize;
    let mut l: Vec<f64> = (0..n)
        .map(|i| {
            if i < burst {
                (std::f64::consts::TAU * 440.0 * i as f64 / SR).sin() * 0.5
            } else {
                0.0
            }
        })
        .collect();
    let mut r = l.clone();
    chain.process(&mut l, &mut r);
    (l, r)
}

// ── defaults transparent ────────────────────────────────────────────

#[test]
fn defaults_are_transparent() {
    for algo in [
        AlgorithmType::Hall,
        AlgorithmType::Plate,
        AlgorithmType::Room,
    ] {
        let mut plain = make_chain(algo);
        let mut touched = make_chain(algo);
        touched.voice = ReverbVoice::Mx;
        touched.hall = Default::default();
        touched.update_params();

        let (pl, _) = render_burst(&mut plain, 1.5);
        let (tl, _) = render_burst(&mut touched, 1.5);
        for (i, (a, b)) in pl.iter().zip(tl.iter()).enumerate() {
            assert!(
                (a - b).abs() < 1e-12,
                "{algo:?} defaults not transparent at {i}: {a} vs {b}"
            );
        }
    }
}

// ── Voice pairs ─────────────────────────────────────────────────────

#[test]
fn voice_classic_pair_maps_plate_and_spring() {
    for algo in [AlgorithmType::Plate, AlgorithmType::Spring] {
        let mut c = make_chain(algo);
        assert_eq!(c.variant(), 0);
        c.voice = ReverbVoice::Classic;
        c.update_params();
        assert_eq!(
            c.variant(),
            1,
            "{algo:?} Classic must select the heritage variant"
        );
        c.voice = ReverbVoice::Mx;
        c.update_params();
        assert_eq!(c.variant(), 0, "{algo:?} back to MX variant");
    }
}

#[test]
fn voice_pairing_does_not_clobber_explicit_variant() {
    // Progenitor (variant 2) chosen explicitly; unrelated update_params
    // calls with an unchanged voice must leave it alone.
    let mut c = make_chain(AlgorithmType::Plate);
    c.set_variant(2);
    c.params.decay = 0.7;
    c.update_params();
    assert_eq!(c.variant(), 2, "explicit variant must survive updates");
}

#[test]
fn voice_classic_retunes_hall_and_room() {
    for algo in [AlgorithmType::Hall, AlgorithmType::Room, AlgorithmType::Shimmer] {
        let mut mx = make_chain(algo);
        let mut classic = make_chain(algo);
        classic.voice = ReverbVoice::Classic;
        classic.update(config());

        let (ml, _) = render_burst(&mut mx, 2.0);
        let (cl, _) = render_burst(&mut classic, 2.0);
        for v in cl.iter() {
            assert!(v.is_finite());
        }
        let diff: f64 = ml.iter().zip(cl.iter()).map(|(a, b)| (a - b).abs()).sum();
        assert!(
            diff > 1e-3,
            "{algo:?} Classic re-tune must be audible: {diff}"
        );
    }
}

// ── Hall Mid EQ ─────────────────────────────────────────────────────

#[test]
fn hall_mid_cut_scoops_1k() {
    let render = |mid_db: f64| {
        let mut c = make_chain(AlgorithmType::Hall);
        c.hall.mid_db = mid_db;
        c.update_params();
        let (l, _) = render_burst(&mut c, 2.0);
        l
    };

    let flat = render(0.0);
    let cut = render(-6.0);
    let boost = render(6.0);
    let body = (SR * 0.1) as usize..(SR * 1.6) as usize;
    // Normalize the 1 kHz band by an out-of-band reference (200 Hz) so
    // the comparison reads EQ shape, not overall level.
    let shape = |buf: &[f64]| {
        let b = &buf[body.clone()];
        goertzel(b, 1000.0) / goertzel(b, 200.0).max(1e-30)
    };
    let s_flat = shape(&flat);
    let s_cut = shape(&cut);
    let s_boost = shape(&boost);
    assert!(
        s_cut < s_flat * 0.6,
        "mid cut must scoop ~1 kHz: cut={s_cut:e} flat={s_flat:e}"
    );
    assert!(
        s_boost > s_flat * 1.5,
        "mid boost must lift ~1 kHz: boost={s_boost:e} flat={s_flat:e}"
    );
}

// ── Hall Swell ──────────────────────────────────────────────────────

#[test]
fn hall_swell_ramps_the_wet() {
    let render = |rise: f64| {
        let mut c = make_chain(AlgorithmType::Hall);
        c.hall.swell_rise = rise;
        c.hall.swell_type = SwellType::Wet;
        c.update(config());
        let (l, _) = render_burst(&mut c, 2.5);
        l
    };

    let plain = render(0.0);
    let swelled = render(0.8); // ~1.65 s rise
    for v in swelled.iter() {
        assert!(v.is_finite());
    }
    // Early wet (during the first 150 ms) is suppressed by the swell;
    // late wet recovers.
    let early = (SR * 0.02) as usize..(SR * 0.15) as usize;
    let e_plain = energy(&plain[early.clone()]);
    let e_swell = energy(&swelled[early]);
    assert!(
        e_swell < e_plain * 0.25,
        "swell must suppress the onset: swell={e_swell:e} plain={e_plain:e}"
    );
}

#[test]
fn hall_swell_wet_plus_dry_shapes_dry() {
    // mix 0.5: with Wet type the dry onset passes at full level; with
    // WetPlusDry the whole output (dry included) swells.
    let render = |ty: SwellType| {
        let mut c = make_chain(AlgorithmType::Hall);
        c.mix = 0.5;
        c.hall.swell_rise = 0.8;
        c.hall.swell_type = ty;
        c.update(config());
        let (l, _) = render_burst(&mut c, 1.0);
        l
    };

    let wet_only = render(SwellType::Wet);
    let wet_dry = render(SwellType::WetPlusDry);
    let onset = (SR * 0.005) as usize..(SR * 0.08) as usize;
    let e_wet_only = energy(&wet_only[onset.clone()]);
    let e_wet_dry = energy(&wet_dry[onset]);
    assert!(
        e_wet_dry < e_wet_only * 0.25,
        "wet+dry swell must also duck the dry onset: {e_wet_dry:e} vs {e_wet_only:e}"
    );
}

// ── Size selection ──────────────────────────────────────────────────

#[test]
fn size_index_maps_variants_and_size() {
    // Hall: Concert / Arena onto variants 0 / 2.
    let mut hall = make_chain(AlgorithmType::Hall);
    hall.set_size_index(1);
    assert_eq!(hall.variant(), 2, "Hall Arena = variant 2");
    hall.set_size_index(0);
    assert_eq!(hall.variant(), 0, "Hall Concert = variant 0");

    // Room: Studio / Club onto variants 2 / 0.
    let mut room = make_chain(AlgorithmType::Room);
    room.set_size_index(0);
    assert_eq!(room.variant(), 2, "Room Studio = variant 2");
    room.set_size_index(1);
    assert_eq!(room.variant(), 0, "Room Club = variant 0");

    // Plate (heritage variants): sizes step params.size instead.
    let mut plate = make_chain(AlgorithmType::Plate);
    let v_before = plate.variant();
    plate.set_size_index(2);
    assert_eq!(plate.variant(), v_before, "Plate size must not touch variant");
    assert!((plate.params.size - 0.8).abs() < 1e-12);

    // Size names exist for every engine.
    for algo in AlgorithmType::ALL {
        assert!(!algo.size_names().is_empty());
    }
}
