//! Onset detection and percussive analysis, against patterns whose hit
//! times are known by construction.
//!
//! Every case builds a kit part from explicit `(time, kind)` pairs, so
//! "did it find the hits" is a real assertion rather than a plausible
//! one — the answer is compared against the list the audio was
//! synthesised from.

use expression_editor_audio::onsets::{OnsetConfig, detect, onset_seconds};
use expression_editor_audio::{PercussiveConfig, analyze_percussive};
use expression_editor_core::rows::{RowSpace, SliceBands};

const SR: f64 = 44100.0;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Hit {
    /// Low sine thump with a fast decay.
    Kick,
    /// Band-ish noise burst with body.
    Snare,
    /// Short bright noise.
    Hat,
}

/// Deterministic noise — a test that changes its answer run to run is
/// not a test.
fn noise(seed: &mut u64) -> f64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    ((*seed >> 33) as f64 / (1u64 << 31) as f64) - 1.0
}

fn render(hits: &[(f64, Hit)], secs: f64) -> Vec<f64> {
    let mut out = vec![0.0; (SR * secs) as usize];
    let mut seed = 0x5eed_1234u64;
    for &(at, kind) in hits {
        let start = (at * SR) as usize;
        let (decay, amp) = match kind {
            Hit::Kick => (0.12, 0.9),
            Hit::Snare => (0.09, 0.7),
            Hit::Hat => (0.03, 0.45),
        };
        let n = (decay * SR) as usize;
        for i in 0..n {
            let Some(slot) = out.get_mut(start + i) else {
                break;
            };
            let t = i as f64 / SR;
            let env = (-t / (decay * 0.35)).exp();
            let s = match kind {
                // A kick's pitch drops through the hit; a static sine
                // reads as a pitched note and would be tracked as one.
                Hit::Kick => (core::f64::consts::TAU * (90.0 - 40.0 * t / decay) * t).sin(),
                // Noise plus a 200 Hz body.
                Hit::Snare => {
                    0.6 * noise(&mut seed) + 0.4 * (core::f64::consts::TAU * 200.0 * t).sin()
                }
                // High-passed noise, crudely: differencing tilts the
                // spectrum up, which is what a hat is. Halved because
                // the difference of two samples spans twice the range
                // of one, and a "quiet" hat that peaks louder than the
                // kick is not the fixture this file means to build.
                Hit::Hat => {
                    let a = noise(&mut seed);
                    let b = noise(&mut seed);
                    (a - b) * 0.5
                }
            };
            *slot += s * env * amp;
        }
    }
    out
}

/// A straight bar: kick on 1 and 3, snare on 2 and 4, hats on eighths.
fn bar() -> (Vec<(f64, Hit)>, Vec<f64>) {
    let beat = 0.5; // 120 bpm
    let mut hits = Vec::new();
    for b in 0..4 {
        let t = b as f64 * beat;
        hits.push((t, if b % 2 == 0 { Hit::Kick } else { Hit::Snare }));
        hits.push((t + beat / 2.0, Hit::Hat));
    }
    hits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let audio = render(&hits, 2.2);
    (hits, audio)
}

fn times(onsets: &[expression_editor_audio::Onset], cfg: &OnsetConfig) -> Vec<f64> {
    onsets.iter().map(|o| onset_seconds(o, SR, cfg)).collect()
}

/// How far each expected hit is from the nearest detected one.
fn misses(expected: &[f64], got: &[f64]) -> Vec<(f64, f64)> {
    expected
        .iter()
        .map(|&e| {
            let d = got
                .iter()
                .map(|&g| (g - e).abs())
                .fold(f64::INFINITY, f64::min);
            (e, d)
        })
        .collect()
}

#[test]
fn every_hit_in_a_straight_bar_is_found_and_none_are_invented() {
    let (hits, audio) = bar();
    let cfg = OnsetConfig::default();
    let got = times(&detect(&audio, SR, cfg), &cfg);
    let expected: Vec<f64> = hits.iter().map(|h| h.0).collect();

    for (at, off) in misses(&expected, &got) {
        assert!(off < 0.02, "hit at {at}s was missed by {off}s");
    }
    // And nothing extra: a detector that fires on decays finds twice as
    // many, which the assertion above would happily allow.
    assert_eq!(
        got.len(),
        expected.len(),
        "found {} onsets for {} hits: {got:?}",
        got.len(),
        expected.len()
    );
}

#[test]
fn a_decay_is_not_an_onset() {
    // One hit with a long tail. A detector summing *signed* flux fires
    // again on the way down, which is the single most common way to get
    // this wrong.
    let audio = render(&[(0.2, Hit::Kick)], 1.0);
    let onsets = detect(&audio, SR, OnsetConfig::default());
    assert_eq!(onsets.len(), 1, "one hit, one onset: {onsets:?}");
}

#[test]
fn silence_produces_no_hits() {
    let quiet = vec![0.0; (SR * 0.5) as usize];
    assert!(detect(&quiet, SR, OnsetConfig::default()).is_empty());
    assert!(detect(&[], SR, OnsetConfig::default()).is_empty());
}

#[test]
fn a_flam_is_one_hit() {
    // Two strikes 12 ms apart — one gesture, and the default 50 ms floor
    // should fuse them.
    let audio = render(&[(0.3, Hit::Snare), (0.312, Hit::Snare)], 1.0);
    let onsets = detect(&audio, SR, OnsetConfig::default());
    assert_eq!(onsets.len(), 1, "a flam is one hit: {onsets:?}");
}

#[test]
fn the_spacing_floor_is_a_parameter_not_a_rule() {
    // Two hats 40 ms apart: fused at the default, separate when the
    // floor is lowered. A shaker or a 32nd-note hat pattern lives here.
    //
    // Hats rather than snares on purpose. The floor is the only thing
    // under test, and a second snare 40 ms into the first one's decay is
    // partly masked by it — that is a question about *thresholds*, and
    // one this detector answers conservatively (see the note on flams in
    // `onsets.rs`).
    let audio = render(&[(0.3, Hit::Hat), (0.34, Hit::Hat)], 1.0);
    assert_eq!(detect(&audio, SR, OnsetConfig::default()).len(), 1);

    let tight = OnsetConfig {
        min_spacing_secs: 0.01,
        ..OnsetConfig::default()
    };
    assert_eq!(
        detect(&audio, SR, tight).len(),
        2,
        "lowering the floor separates them"
    );
}

#[test]
fn the_strongest_hit_wins_a_spacing_contest() {
    // A ghost note 20 ms before the backbeat. With a 50 ms floor only
    // one survives, and it must be the backbeat — keeping whichever came
    // first would let the ghost suppress the hit itself.
    let audio = render(&[(0.5, Hit::Hat), (0.52, Hit::Snare)], 1.2);
    let cfg = OnsetConfig::default();
    let got = times(&detect(&audio, SR, cfg), &cfg);
    assert_eq!(got.len(), 1);
    assert!(
        (got[0] - 0.52).abs() < 0.02,
        "kept the ghost at {:?} instead of the backbeat",
        got[0]
    );
}

#[test]
fn a_track_that_gets_louder_does_not_lose_its_quiet_half() {
    // The case a fixed threshold cannot pass: the same pattern twice,
    // the second time four times louder. A threshold set from the whole
    // take's peak misses every hit in the first half.
    let mut audio = render(&[(0.1, Hit::Snare), (0.6, Hit::Snare)], 1.1);
    for s in &mut audio {
        *s *= 0.25;
    }
    audio.extend(render(&[(0.1, Hit::Snare), (0.6, Hit::Snare)], 1.1));

    let onsets = detect(&audio, SR, OnsetConfig::default());
    assert_eq!(onsets.len(), 4, "all four hits survive: {onsets:?}");
}

#[test]
fn hits_sort_into_bands_by_brightness() {
    let (_, audio) = bar();
    let p = analyze_percussive(&audio, SR, PercussiveConfig::default());
    assert!(!p.doc.notes.is_empty());

    // The kick is the darkest thing in the bar and the hats the
    // brightest, whatever the exact split points are.
    let kick = p
        .onsets
        .iter()
        .map(|o| o.centroid_hz)
        .fold(f64::INFINITY, f64::min);
    let hat = p
        .onsets
        .iter()
        .map(|o| o.centroid_hz)
        .fold(0.0f64, f64::max);
    assert!(hat > kick * 2.0, "kick {kick} Hz vs hat {hat} Hz");

    let rows: Vec<i32> = p.doc.notes.iter().map(|n| n.row).collect();
    assert!(
        rows.iter().max() > rows.iter().min(),
        "everything landed in one band: {rows:?}"
    );
    assert!(matches!(p.doc.row_space, RowSpace::Bands(_)));
}

#[test]
fn rebanding_moves_rows_without_reanalysing() {
    let (_, audio) = bar();
    let mut p = analyze_percussive(&audio, SR, PercussiveConfig::default());
    let before: Vec<i32> = p.doc.notes.iter().map(|n| n.row).collect();
    let centroids: Vec<f64> = p.onsets.iter().map(|o| o.centroid_hz).collect();

    // One split, far above everything: every hit is now "low".
    p.reband(SliceBands {
        splits: vec![20_000.0],
        names: vec!["Low".into(), "High".into()],
    });
    assert!(p.doc.notes.iter().all(|n| n.row == 0));

    // And back — the measured centroids were never touched, so the
    // original banding returns exactly.
    p.reband(SliceBands::default());
    let after: Vec<i32> = p.doc.notes.iter().map(|n| n.row).collect();
    assert_eq!(before, after);
    assert_eq!(
        centroids,
        p.onsets.iter().map(|o| o.centroid_hz).collect::<Vec<_>>()
    );
}

#[test]
fn slices_tile_the_take_so_a_gain_edit_leaves_no_gap() {
    let (_, audio) = bar();
    let p = analyze_percussive(&audio, SR, PercussiveConfig::default());
    for pair in p.doc.notes.windows(2) {
        assert_eq!(
            pair[0].end, pair[1].start,
            "slice {:?} does not meet the next",
            pair[0].id
        );
    }
    let last = p.doc.notes.last().expect("hits");
    assert!(last.end <= p.doc.end + 1.0);
}

#[test]
fn a_harder_hit_gets_a_higher_velocity() {
    let audio = render(&[(0.2, Hit::Hat), (0.7, Hit::Kick)], 1.2);
    let p = analyze_percussive(&audio, SR, PercussiveConfig::default());
    assert_eq!(p.doc.notes.len(), 2);
    assert!(
        p.doc.notes[1].velocity > p.doc.notes[0].velocity,
        "the kick should read louder than the hat: {:?}",
        p.doc.notes.iter().map(|n| n.velocity).collect::<Vec<_>>()
    );
}

#[test]
fn a_kit_reads_as_percussive_and_a_sung_line_does_not() {
    use expression_editor_audio::percussive::{looks_percussive, voiced_fraction};
    use expression_editor_audio::{TakeConfig, analyze_take};

    let (_, kit) = bar();
    let kit_voiced = voiced_fraction(&analyze_take(&kit, SR, TakeConfig::default()), 0.001);

    // A sustained sung line: one pitch, held.
    let n = (SR * 1.5) as usize;
    let sung: Vec<f64> = (0..n)
        .map(|i| {
            let t = i as f64 / SR;
            let s = (core::f64::consts::TAU * 220.0 * t).sin()
                + 0.5 * (core::f64::consts::TAU * 440.0 * t).sin();
            s * 0.3
        })
        .collect();
    let sung_voiced = voiced_fraction(&analyze_take(&sung, SR, TakeConfig::default()), 0.001);

    assert!(
        looks_percussive(kit_voiced),
        "a kit should read unpitched (voiced fraction {kit_voiced})"
    );
    assert!(
        !looks_percussive(sung_voiced),
        "a held note should read pitched (voiced fraction {sung_voiced})"
    );
}
