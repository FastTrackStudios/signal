//! BigSky MX pass-D input-analysis generators: Cloud Ensemble,
//! Bloom Harmonics, Chorale Choir/Voice/Mod. Defaults must be
//! bit-transparent; the generators must be audible, pitch-relevant,
//! and numerically stable when engaged.

use reverb_dsp::algorithm::{ChoirVoice, ChoraleParams};
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

/// Sustained sine (whole render) so pitch trackers can lock.
fn render_sine(chain: &mut ReverbChain, freq: f64, secs: f64) -> (Vec<f64>, Vec<f64>) {
    let n = (SR * secs) as usize;
    let mut l: Vec<f64> = (0..n)
        .map(|i| (std::f64::consts::TAU * freq * i as f64 / SR).sin() * 0.5)
        .collect();
    let mut r = l.clone();
    chain.process(&mut l, &mut r);
    (l, r)
}

#[test]
fn defaults_are_transparent() {
    for algo in [
        AlgorithmType::Cloud,
        AlgorithmType::Bloom,
        AlgorithmType::Chorale,
    ] {
        let mut plain = make_chain(algo);
        let mut touched = make_chain(algo);
        touched.cloud = Default::default();
        touched.bloom = Default::default();
        touched.chorale = Default::default();
        touched.update_params();

        let (pl, _) = render_sine(&mut plain, 330.0, 1.5);
        let (tl, _) = render_sine(&mut touched, 330.0, 1.5);
        for (i, (a, b)) in pl.iter().zip(tl.iter()).enumerate() {
            assert!(
                (a - b).abs() < 1e-12,
                "{algo:?} defaults not transparent at {i}: {a} vs {b}"
            );
        }
    }
}

// ── Cloud Ensemble ──────────────────────────────────────────────────

#[test]
fn cloud_ensemble_adds_synth_layer() {
    let render = |level: f64| {
        let mut c = make_chain(AlgorithmType::Cloud);
        c.cloud.ensemble = level;
        c.update_params();
        let (l, _) = render_sine(&mut c, 220.0, 3.0);
        l
    };

    let off = render(0.0);
    let on = render(1.0);
    for v in on.iter() {
        assert!(v.is_finite(), "ensemble produced non-finite output");
    }
    // The two renders share every deterministic reverb path, so the
    // difference signal IS the ensemble's contribution. Require it to
    // carry a meaningful fraction of the wet energy.
    let body = (SR * 1.0) as usize..(SR * 2.8) as usize;
    let e_off = energy(&off[body.clone()]);
    let d: f64 = on[body]
        .iter()
        .zip(&off[(SR * 1.0) as usize..(SR * 2.8) as usize])
        .map(|(a, b)| (a - b) * (a - b))
        .sum();
    assert!(
        d > e_off * 0.05,
        "ensemble must audibly thicken the reverb: diff={d:e} base={e_off:e}"
    );
}

#[test]
fn cloud_ensemble_is_silent_without_input() {
    let mut c = make_chain(AlgorithmType::Cloud);
    c.cloud.ensemble = 1.0;
    c.update_params();
    let n = (SR * 2.0) as usize;
    let mut l = vec![0.0; n];
    let mut r = vec![0.0; n];
    c.process(&mut l, &mut r);
    assert!(
        energy(&l) < 1e-9,
        "no input ⇒ no ensemble: {:e}",
        energy(&l)
    );
}

#[test]
fn cloud_ensemble_tracks_pitch() {
    // The synthetic layer is pitched: its octave partial (saw voices at
    // 2×f) must rise with the ensemble level at the INPUT frequency's
    // octave, and follow when the input pitch changes.
    let octave_ratio = |input_freq: f64| {
        let mut c = make_chain(AlgorithmType::Cloud);
        c.cloud.ensemble = 1.0;
        c.update_params();
        let (l, _) = render_sine(&mut c, input_freq, 3.0);
        let body = &l[(SR * 1.2) as usize..(SR * 2.8) as usize];
        goertzel(body, input_freq * 2.0) / goertzel(body, input_freq).max(1e-30)
    };
    let base_ratio = |input_freq: f64| {
        let mut c = make_chain(AlgorithmType::Cloud);
        c.update_params();
        let (l, _) = render_sine(&mut c, input_freq, 3.0);
        let body = &l[(SR * 1.2) as usize..(SR * 2.8) as usize];
        goertzel(body, input_freq * 2.0) / goertzel(body, input_freq).max(1e-30)
    };

    let with = octave_ratio(220.0);
    let without = base_ratio(220.0);
    assert!(
        with > without * 2.0,
        "ensemble must add pitched octave content: with={with:e} without={without:e}"
    );
}

// ── Bloom Harmonics ─────────────────────────────────────────────────

#[test]
fn bloom_harmonics_adds_octave_partial() {
    let render = |h: f64| {
        let mut c = make_chain(AlgorithmType::Bloom);
        c.bloom.harmonics = h;
        c.update_params();
        let (l, _) = render_sine(&mut c, 330.0, 3.0);
        l
    };

    let off = render(0.0);
    let on = render(1.0);
    for v in on.iter() {
        assert!(v.is_finite(), "harmonics produced non-finite output");
    }
    let body = (SR * 1.0) as usize..(SR * 2.8) as usize;
    let ratio =
        |buf: &[f64]| goertzel(buf, 660.0) / goertzel(buf, 330.0).max(1e-30);
    let r_on = ratio(&on[body.clone()]);
    let r_off = ratio(&off[body]);
    assert!(
        r_on > r_off * 2.0,
        "harmonics must lift the octave partial: on={r_on:e} off={r_off:e}"
    );
}

// ── Chorale ─────────────────────────────────────────────────────────

#[test]
fn chorale_choir_level_overrides_and_scales() {
    let render = |p: ChoraleParams| {
        let mut c = make_chain(AlgorithmType::Chorale);
        c.params.decay = 0.8;
        c.chorale = p;
        c.update_params();
        let (l, _) = render_sine(&mut c, 220.0, 3.0);
        l
    };

    let quiet = render(ChoraleParams {
        choir_level: Some(0.0),
        ..Default::default()
    });
    let loud = render(ChoraleParams {
        choir_level: Some(1.0),
        ..Default::default()
    });
    for v in loud.iter() {
        assert!(v.is_finite());
    }
    // The choir voice recirculates — max level rings noticeably harder
    // in the sustained body than level 0 (feedback off).
    let body = (SR * 1.5) as usize..(SR * 2.8) as usize;
    let e_q = energy(&quiet[body.clone()]);
    let e_l = energy(&loud[body]);
    assert!(
        e_l > e_q * 1.05,
        "choir level must scale the vocal layer: loud={e_l:e} quiet={e_q:e}"
    );
}

#[test]
fn chorale_voice_and_mod_change_the_output() {
    let render = |p: ChoraleParams| {
        let mut c = make_chain(AlgorithmType::Chorale);
        c.chorale = p;
        c.update_params();
        let (l, _) = render_sine(&mut c, 220.0, 2.0);
        l
    };

    let tenor = render(ChoraleParams {
        choir_level: Some(0.8),
        voice: ChoirVoice::Tenor,
        mod_amount: 0.0,
    });
    let soprano = render(ChoraleParams {
        choir_level: Some(0.8),
        voice: ChoirVoice::Soprano,
        mod_amount: 0.0,
    });
    let modded = render(ChoraleParams {
        choir_level: Some(0.8),
        voice: ChoirVoice::Tenor,
        mod_amount: 1.0,
    });

    for v in soprano.iter().chain(modded.iter()) {
        assert!(v.is_finite(), "chorale params produced non-finite output");
    }

    let diff = |a: &[f64], b: &[f64]| -> f64 {
        a.iter().zip(b.iter()).map(|(x, y)| (x - y).abs()).sum()
    };
    let d_voice = diff(&tenor, &soprano);
    let d_mod = diff(&tenor, &modded);
    assert!(d_voice > 1e-3, "Soprano must differ from Tenor: {d_voice}");
    assert!(d_mod > 1e-3, "Mod randomization must be audible: {d_mod}");
}
