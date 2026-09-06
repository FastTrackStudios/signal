//! Cloud engine character tests: the manual's Diffusion continuum and
//! two-segment Mod law.

use audiocore_dsp::{AudioConfig, Processor};
use reverb_dsp::{AlgorithmType, ReverbChain};

const SR: f64 = 48000.0;

fn make(diffusion: f64, modulation: f64) -> ReverbChain {
    let mut c = ReverbChain::new();
    c.set_algorithm(AlgorithmType::Cloud);
    c.mix = 1.0;
    c.params.diffusion = diffusion;
    c.params.modulation = modulation;
    c.params.decay = 0.5;
    c.update(AudioConfig {
        sample_rate: SR,
        max_buffer_size: 4096,
    });
    c
}

fn render_impulse(c: &mut ReverbChain, n: usize) -> Vec<f64> {
    let mut l = vec![0.0f64; n];
    let mut r = vec![0.0f64; n];
    l[0] = 1.0;
    r[0] = 1.0;
    c.process(&mut l, &mut r);
    l
}

#[test]
fn diffusion_min_is_grainy_max_is_fog() {
    // The manual: min Diffusion = "grainier yet mesmerizing on
    // transient attacks" (discrete skittery taps), max = smoothed fog.
    // Crest factor (peak/RMS) of the early wet field separates the two:
    // discrete taps spike, fog is statistically smooth.
    let crest = |diffusion: f64| -> f64 {
        let mut c = make(diffusion, 0.2);
        let out = render_impulse(&mut c, (0.25 * SR) as usize);
        let window = &out[(0.01 * SR) as usize..];
        let peak = window.iter().fold(0.0f64, |a, &x| a.max(x.abs()));
        let rms = (window.iter().map(|x| x * x).sum::<f64>() / window.len() as f64).sqrt();
        peak / rms.max(1e-12)
    };
    let grainy = crest(0.0);
    let fog = crest(1.0);
    assert!(
        grainy > fog * 1.3,
        "min diffusion should be spikier than fog: grainy={grainy:.2} fog={fog:.2}"
    );
}

#[test]
fn mod_law_two_segments_stay_finite_and_distinct() {
    // Depth segment (0.6) vs rate segment (1.0): both must differ from
    // the unmodulated render and from each other, without instability.
    let render = |modulation: f64| -> Vec<f64> {
        let mut c = make(0.6, modulation);
        let n = (1.0 * SR) as usize;
        let mut l: Vec<f64> = (0..n)
            .map(|i| (core::f64::consts::TAU * 330.0 * i as f64 / SR).sin() * 0.4)
            .collect();
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        l
    };
    let still = render(0.0);
    let deep = render(0.6);
    let fast = render(1.0);
    let diff =
        |a: &[f64], b: &[f64]| -> f64 { a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum() };
    let ref_e: f64 = still.iter().map(|x| x * x).sum();
    assert!(diff(&still, &deep) > ref_e * 1e-4, "depth segment inert");
    assert!(diff(&deep, &fast) > ref_e * 1e-4, "rate segment inert");
    for v in deep.iter().chain(fast.iter()) {
        assert!(v.is_finite());
    }
}
