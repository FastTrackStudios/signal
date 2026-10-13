//! Note tracking + time-varying separation: two notes played *in sequence*
//! (not a chord) must come back as two notes with the right time spans, and the
//! separator must place each note's audio only in its own time region — the
//! thing whole-buffer separation cannot do.

use tune_dsp::detect::{hz_to_midi, midi_to_hz};
use tune_dsp::{spans_from_frames, track_notes, DnaConfig, DnaEngine, TrackConfig};

const SR: f64 = 48_000.0;

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

fn energy(x: &[f64]) -> f64 {
    x.iter().map(|s| s * s).sum()
}

/// 220 Hz for 0.35 s, then 350 Hz for 0.35 s — sequential, no overlap.
fn sequential() -> Vec<f64> {
    let half = (SR * 0.35) as usize;
    let mut out = tone(220.0, half);
    out.extend(tone(350.0, half));
    out
}

#[test]
fn tracks_two_sequential_notes() {
    let sig = sequential();
    let eng = DnaEngine::new(SR, DnaConfig::default());
    let frames = eng.analyze_pitch_frames(&sig);
    let notes = track_notes(&frames, TrackConfig::default());

    // Expect (at least) a low note and a high note.
    let a3 = hz_to_midi(220.0);
    let f4 = hz_to_midi(350.0);
    let low = notes
        .iter()
        .find(|n| (n.median_midi - a3).abs() < 0.5)
        .expect("should track the 220 Hz note");
    let high = notes
        .iter()
        .find(|n| (n.median_midi - f4).abs() < 0.5)
        .expect("should track the 350 Hz note");

    // The low note must come first and end before the high note begins.
    assert!(
        low.end_frame < high.start_frame + 2,
        "notes should be roughly sequential (low ends {}, high starts {})",
        low.end_frame,
        high.start_frame
    );
    // Onset of the high note should land near 0.35 s. A 4096-sample analysis
    // window smears onsets ~half a window (~43 ms) early, so allow for that.
    let onset = high.onset_sec(eng.hop(), SR);
    let half_window = eng.window() as f64 / SR / 2.0;
    assert!(
        onset <= 0.35 + 0.02 && onset >= 0.35 - half_window - 0.02,
        "high note onset near 0.35s (minus window smear), got {onset:.3}s"
    );
}

#[test]
fn separation_respects_time_spans() {
    let sig = sequential();
    let eng = DnaEngine::new(SR, DnaConfig::default());
    let frames = eng.analyze_pitch_frames(&sig);
    let spans = spans_from_frames(&frames, TrackConfig::default());
    assert!(spans.len() >= 2, "expected at least two note spans");

    let notes = eng.separate_spans(&sig, &spans);
    let mid = sig.len() / 2;

    // Identify which separated stream is the low (early) note by its f0.
    let (low, high): (Vec<_>, Vec<_>) = notes
        .iter()
        .partition(|n| (n.f0_hz - 220.0).abs() < (n.f0_hz - 350.0).abs());
    assert!(
        !low.is_empty() && !high.is_empty(),
        "need a low and a high note"
    );

    // Low note's energy must live in the first half; high note's in the second.
    let low_first = energy(&low[0].audio[..mid]);
    let low_second = energy(&low[0].audio[mid..]);
    assert!(
        low_first > 5.0 * low_second,
        "low note should be concentrated in the first half ({low_first:.3} vs {low_second:.3})"
    );
    let high_first = energy(&high[0].audio[..mid]);
    let high_second = energy(&high[0].audio[mid..]);
    assert!(
        high_second > 5.0 * high_first,
        "high note should be concentrated in the second half ({high_first:.3} vs {high_second:.3})"
    );

    // Sanity: midi_to_hz round-trips the tracked pitch we keyed on.
    assert!((midi_to_hz(hz_to_midi(220.0)) - 220.0).abs() < 1e-6);
}
