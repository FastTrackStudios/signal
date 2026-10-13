//! PSOLA cents-accuracy regression (the 452→469 Hz recenter bug).

use audiocore_dsp::{AudioConfig, Processor};

const SR: f64 = 48000.0;

fn zc_freq(buf: &[f64]) -> f64 {
    let mut c = 0;
    for i in 1..buf.len() {
        if buf[i - 1] < 0.0 && buf[i] >= 0.0 {
            c += 1;
        }
    }
    c as f64 * SR / buf.len() as f64
}

#[test]
fn psola_is_cents_accurate_at_small_and_moderate_shifts() {
    for st in [-0.467f64, -2.0, 2.0, 5.0, -5.0] {
        let mut chain = pitch_dsp::chain::PitchChain::new();
        chain.algorithm = pitch_dsp::chain::Algorithm::Psola;
        chain.semitones = st;
        chain.mix = 1.0;
        chain.update(AudioConfig {
            sample_rate: SR,
            max_buffer_size: 512,
        });
        let n = 96_000;
        let mut l: Vec<f64> = (0..n)
            .map(|i| 0.5 * (core::f64::consts::TAU * 452.0 * i as f64 / SR).sin())
            .collect();
        let mut r = l.clone();
        for s in (0..n).step_by(512) {
            let e = (s + 512).min(n);
            let (mut a, mut b) = (l[s..e].to_vec(), r[s..e].to_vec());
            chain.process(&mut a, &mut b);
            l[s..e].copy_from_slice(&a);
            r[s..e].copy_from_slice(&b);
        }
        let expected = 452.0 * 2.0f64.powf(st / 12.0);
        let got = zc_freq(&l[n / 2..]);
        let cents = 1200.0 * (got / expected).log2();
        assert!(
            cents.abs() < 25.0,
            "PSOLA at {st:+.2} st: got {got:.1} Hz, expected {expected:.1} ({cents:+.0} cents off)"
        );
    }
}

#[test]
fn print_psola_vs_wsola_cents() {
    for algo in [
        pitch_dsp::chain::Algorithm::Psola,
        pitch_dsp::chain::Algorithm::Wsola,
    ] {
        for st in [-0.467f64, -2.0, 2.0] {
            let mut chain = pitch_dsp::chain::PitchChain::new();
            chain.algorithm = algo;
            chain.semitones = st;
            chain.mix = 1.0;
            chain.update(AudioConfig {
                sample_rate: SR,
                max_buffer_size: 512,
            });
            let n = 96_000;
            let mut l: Vec<f64> = (0..n)
                .map(|i| 0.5 * (core::f64::consts::TAU * 452.0 * i as f64 / SR).sin())
                .collect();
            let mut r = l.clone();
            for s in (0..n).step_by(512) {
                let e = (s + 512).min(n);
                let (mut a, mut b) = (l[s..e].to_vec(), r[s..e].to_vec());
                chain.process(&mut a, &mut b);
                l[s..e].copy_from_slice(&a);
                r[s..e].copy_from_slice(&b);
            }
            let expected = 452.0 * 2.0f64.powf(st / 12.0);
            let got = zc_freq(&l[n / 2..]);
            println!(
                "{algo:?} {st:+.2} st: {got:.2} Hz vs {expected:.2} ({:+.1} cents)",
                1200.0 * (got / expected).log2()
            );
        }
    }
}
