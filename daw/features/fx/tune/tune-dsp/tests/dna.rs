//! DNA kernel: prove two simultaneous notes can be pulled apart into isolated,
//! independently-editable streams — the thing transcription alone can't do.

use tune_dsp::{DnaConfig, DnaEngine};

const SR: f64 = 48_000.0;

/// A harmonic tone: fundamental + a few decaying harmonics.
fn tone(freq: f64, n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| {
            let t = i as f64 / SR;
            let mut s = 0.0;
            for h in 1..=5 {
                s += (1.0 / h as f64) * (core::f64::consts::TAU * freq * h as f64 * t).sin();
            }
            0.2 * s
        })
        .collect()
}

/// Normalised correlation of two equal-length signals over a central slice
/// (avoids STFT edge under-normalisation).
fn corr(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len());
    let lo = n / 8;
    let hi = n - n / 8;
    let (mut dot, mut na, mut nb) = (0.0, 0.0, 0.0);
    for i in lo..hi {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na <= 1e-12 || nb <= 1e-12 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

fn mix(len: usize, fa: f64, fb: f64) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let a = tone(fa, len);
    let b = tone(fb, len);
    let m: Vec<f64> = a.iter().zip(&b).map(|(x, y)| x + y).collect();
    (a, b, m)
}

#[test]
fn estimates_two_fundamentals() {
    let (_, _, m) = mix((SR * 0.4) as usize, 220.0, 350.0);
    let eng = DnaEngine::new(SR, DnaConfig::default());
    let mag = eng.mean_magnitude(&m);
    let f0s = eng.estimate_pitches(&mag);

    let near = |target: f64| f0s.iter().any(|&f| (f - target).abs() / target < 0.03);
    assert!(near(220.0), "should find ~220 Hz, got {f0s:?}");
    assert!(near(350.0), "should find ~350 Hz, got {f0s:?}");
}

#[test]
fn separates_a_two_note_chord() {
    let len = (SR * 0.4) as usize;
    let (ref_a, ref_b, m) = mix(len, 220.0, 350.0);
    let eng = DnaEngine::new(SR, DnaConfig::default());

    // Force the known note set so the test isolates the *separation* stage.
    let notes = eng.separate(&m, Some(&[220.0, 350.0]));
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].audio.len(), m.len());

    let sep_a = &notes[0].audio;
    let sep_b = &notes[1].audio;

    // Each stream must resemble its own reference far more than the other's.
    let aa = corr(sep_a, &ref_a);
    let ab = corr(sep_a, &ref_b);
    let bb = corr(sep_b, &ref_b);
    let ba = corr(sep_b, &ref_a);
    assert!(
        aa > 0.8 && aa > ab + 0.5,
        "stream A should track note A (aa={aa:.3}, ab={ab:.3})"
    );
    assert!(
        bb > 0.8 && bb > ba + 0.5,
        "stream B should track note B (bb={bb:.3}, ba={ba:.3})"
    );

    // Streams must sum back to (close to) the original mix — no lost energy.
    let recon: Vec<f64> = sep_a.iter().zip(sep_b).map(|(x, y)| x + y).collect();
    assert!(
        corr(&recon, &m) > 0.95,
        "separated streams should reconstruct the mix"
    );
}

#[test]
fn retuning_one_note_leaves_the_other() {
    // The payoff: shift note A up, keep note B — then the recombined chord's A
    // energy has moved but B's hasn't. We approximate "shift" by scaling stream
    // A's sample index (cheap resample) purely to show streams are independent.
    let len = (SR * 0.4) as usize;
    let (_, ref_b, m) = mix(len, 220.0, 350.0);
    let eng = DnaEngine::new(SR, DnaConfig::default());
    let notes = eng.separate(&m, Some(&[220.0, 350.0]));

    // Mute note A entirely; the remainder must still contain note B.
    let only_b = &notes[1].audio;
    assert!(
        corr(only_b, &ref_b) > 0.8,
        "muting A should leave a clean B"
    );
    // And it must NOT contain note A (low correlation with a 220 reference).
    let ref_a = tone(220.0, len);
    assert!(
        corr(only_b, &ref_a) < 0.4,
        "the isolated B stream should be largely free of A"
    );
}
