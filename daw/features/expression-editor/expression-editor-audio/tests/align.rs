//! Take-to-take alignment, against pairs whose true offset is known.
//!
//! Every case here is a reference and a dub built from the same phrase
//! description, with one property deliberately changed — a shift, a
//! stretch, a different pitch. That is the only way to assert that an
//! alignment is *correct* rather than merely plausible.

use expression_editor_audio::{AlignConfig, Analysis, TakeConfig, align, analyze_take};

const SR: f64 = 44100.0;

fn midi_to_hz(m: f64) -> f64 {
    440.0 * 2f64.powf((m - 69.0) / 12.0)
}

/// One syllable: a pitched tone with an attack and release.
fn syllable(midi: f64, secs: f64) -> Vec<f64> {
    let n = (SR * secs) as usize;
    let mut phase = 0.0;
    (0..n)
        .map(|i| {
            let t = i as f64 / SR;
            phase += core::f64::consts::TAU * midi_to_hz(midi) / SR;
            let s = phase.sin() + 0.5 * (phase * 2.0).sin() + 0.3 * (phase * 3.0).sin();
            s * (t / 0.02).min(1.0) * ((secs - t) / 0.04).clamp(0.0, 1.0) * 0.3
        })
        .collect()
}

fn gap(secs: f64) -> Vec<f64> {
    vec![0.0; (SR * secs).max(0.0) as usize]
}

/// A phrase: `(midi, length, gap-after)` repeated.
fn phrase(parts: &[(f64, f64, f64)], transpose: f64, rate: f64) -> Vec<f64> {
    let mut out = Vec::new();
    for &(midi, len, after) in parts {
        out.extend(syllable(midi + transpose, len * rate));
        out.extend(gap(after * rate));
    }
    out
}

const LINE: &[(f64, f64, f64)] = &[
    (60.0, 0.30, 0.10),
    (64.0, 0.25, 0.08),
    (67.0, 0.35, 0.12),
    (65.0, 0.30, 0.10),
];

fn analyse(audio: &[f64]) -> Analysis {
    analyze_take(audio, SR, TakeConfig::default())
}

/// Seconds the map moves dub time `t` by.
fn shift_at(a: &expression_editor_audio::Alignment, secs: f64) -> f64 {
    let i = ((secs * a.frame_rate).round() as usize).min(a.map.len().saturating_sub(1));
    (a.map[i] - i as f64) / a.frame_rate
}

#[test]
fn an_identical_take_needs_no_correction() {
    let audio = phrase(LINE, 0.0, 1.0);
    let r = analyse(&audio);
    let d = analyse(&audio);
    let a = align(&r, &d, AlignConfig::default()).expect("aligned");

    assert_eq!(a.map.len(), d.frames.frames.len());
    assert!(
        a.max_shift_secs() < 0.02,
        "a take against itself should barely move: {} s",
        a.max_shift_secs()
    );
}

#[test]
fn a_late_dub_is_pulled_back_by_how_late_it_is() {
    let reference = phrase(LINE, 0.0, 1.0);
    // The same performance, starting 120 ms later.
    let mut dub = gap(0.12);
    dub.extend(phrase(LINE, 0.0, 1.0));

    let a = align(&analyse(&reference), &analyse(&dub), AlignConfig::default()).expect("aligned");

    // Mid-phrase, the map should point about 120 ms earlier.
    let s = shift_at(&a, 0.6);
    assert!(
        (s + 0.12).abs() < 0.06,
        "wanted about -0.12 s of correction, got {s}"
    );
}

#[test]
fn a_dub_sung_slower_is_compressed_progressively() {
    let reference = phrase(LINE, 0.0, 1.0);
    // Same line, 15% slower — the drift grows across the phrase, which
    // is what a constant offset cannot fix and alignment must.
    let dub = phrase(LINE, 0.0, 1.15);

    let a = align(&analyse(&reference), &analyse(&dub), AlignConfig::default()).expect("aligned");

    let early = shift_at(&a, 0.2);
    let late = shift_at(&a, 1.4);
    assert!(
        late < early - 0.05,
        "the correction grows as the takes drift apart: {early} then {late}"
    );
    // And the map never goes backwards, which would render as a stutter.
    for pair in a.map.windows(2) {
        assert!(pair[1] >= pair[0]);
    }
}

#[test]
fn a_harmony_at_a_different_pitch_still_aligns_on_timing() {
    // The case that decides whether energy or pitch is the primary
    // feature. A stacked third is deliberately at another pitch and
    // must still line up.
    let reference = phrase(LINE, 0.0, 1.0);
    let mut dub = gap(0.1);
    dub.extend(phrase(LINE, 4.0, 1.0));

    let a = align(&analyse(&reference), &analyse(&dub), AlignConfig::default()).expect("aligned");
    let s = shift_at(&a, 0.7);
    assert!(
        (s + 0.1).abs() < 0.07,
        "a harmony aligns on when, not what: got {s}"
    );
}

#[test]
fn strength_scales_the_correction_without_changing_its_shape() {
    let reference = phrase(LINE, 0.0, 1.0);
    let mut dub = gap(0.12);
    dub.extend(phrase(LINE, 0.0, 1.0));
    let (r, d) = (analyse(&reference), analyse(&dub));

    let strength = |v: f64| AlignConfig {
        anchors: expression_editor_audio::align::AnchorConfig {
            strength: v,
            ..Default::default()
        },
        ..Default::default()
    };
    let full = align(&r, &d, strength(1.0)).unwrap();
    let half = align(&r, &d, strength(0.5)).unwrap();
    let none = align(&r, &d, strength(0.0)).unwrap();

    // Measured against the offset. Strength is a judgement about the
    // performance — how much of the dub's own phrasing to keep — and
    // where the take sits on the timeline is not a matter of phrasing,
    // so the global offset is applied in full at every strength and only
    // the warp on top of it is scaled.
    let warp = |a: &expression_editor_audio::Alignment| shift_at(a, 0.6) - a.offset.seconds;
    let (f, h) = (warp(&full), warp(&half));
    assert!(
        (h - f * 0.5).abs() < 0.1 * f.abs() + 0.01,
        "half is half: {f} vs {h}"
    );
    assert!(
        warp(&none).abs() < 1e-9,
        "zero strength leaves the dub's phrasing exactly as sung"
    );
}

#[test]
fn a_badly_placed_dub_moves_as_far_as_it_must_but_warps_only_as_far_as_allowed() {
    // A dub a second and a half late. This used to be unfixable and the
    // limit said so: corrections were capped at `max_shift_secs`, and a
    // take outside that was left partly corrected rather than silently
    // dragged. With a macro offset stage the two are no longer the same
    // question. *Where the take sits* is a fact, and it is fixed in
    // full; *how its phrasing is bent* is a judgement, and that is what
    // the limit governs.
    let reference = phrase(LINE, 0.0, 1.0);
    let mut dub = gap(1.5);
    dub.extend(phrase(LINE, 0.0, 1.0));

    let cfg = AlignConfig {
        anchors: expression_editor_audio::align::AnchorConfig {
            max_shift_secs: 0.3,
            ..Default::default()
        },
        ..Default::default()
    };
    let a = align(&analyse(&reference), &analyse(&dub), cfg).expect("aligned");

    assert!(
        (a.offset.seconds + 1.5).abs() < 0.05,
        "the offset stage should find the whole 1.5 s: {} s",
        a.offset.seconds
    );
    // Sampled inside the phrase, past the lead-in the offset moved.
    for t in [1.8, 2.2, 2.6] {
        let warp = shift_at(&a, t) - a.offset.seconds;
        assert!(
            warp.abs() <= 0.31,
            "warped {warp} s at {t} s, beyond the 0.3 s asked for"
        );
    }
}

#[test]
fn markers_land_on_anchors_and_keep_both_ends() {
    let reference = phrase(LINE, 0.0, 1.0);
    let mut dub = gap(0.1);
    dub.extend(phrase(LINE, 0.0, 1.0));
    let d = analyse(&dub);
    let a = align(&analyse(&reference), &d, AlignConfig::default()).expect("aligned");

    let hop = d.frames.hop;
    let markers = expression_editor_audio::align::warp_markers(&a, hop);

    // One per anchor, not one per frame. A marker per frame pins the
    // inside of every sustained note to whatever the matcher decided,
    // and leaves a result no one can edit by hand afterwards.
    assert_eq!(markers.len(), a.anchors.len());
    assert!(
        markers.len() < a.map.len() / 8,
        "{} markers for {} frames is not a reduction",
        markers.len(),
        a.map.len()
    );

    // Both ends survive, or the map is undefined at the edges and the
    // take snaps back there.
    assert_eq!(markers.first().unwrap().sample, 0.0);
    assert_eq!(
        markers.last().unwrap().sample,
        ((a.map.len() - 1) * hop) as f64
    );
    // Anchored at dub time, sorted, and carrying a real correction.
    for pair in markers.windows(2) {
        assert!(pair[1].sample > pair[0].sample);
    }
    assert!(markers.iter().any(|m| m.d_time.abs() > 1.0));
}

#[test]
fn an_empty_take_declines_to_align() {
    let real = analyse(&phrase(LINE, 0.0, 1.0));
    let empty = analyse(&[]);
    assert!(align(&real, &empty, AlignConfig::default()).is_none());
    assert!(align(&empty, &real, AlignConfig::default()).is_none());
}

#[test]
fn the_map_covers_every_dub_frame_and_moves_no_further_than_allowed() {
    let reference = phrase(LINE, 0.0, 1.0);
    let dub = phrase(LINE, 0.0, 1.2);
    let (r, d) = (analyse(&reference), analyse(&dub));
    let cfg = AlignConfig::default();
    let a = align(&r, &d, cfg).expect("aligned");

    assert_eq!(a.map.len(), d.frames.frames.len());
    for pair in a.map.windows(2) {
        assert!(pair[1] >= pair[0], "monotonic, or it stutters: {pair:?}");
    }

    // Deliberately *not* "inside the reference". A map value is a time,
    // not an index into the reference's frames, and a dub that has to
    // move earlier than the reference's own first frame produces
    // negative values that mean exactly what they say. What is bounded
    // is the warp: how far the phrasing is bent, over and above wherever
    // the offset stage decided the take belongs.
    let offset_frames = a.offset.seconds * a.frame_rate;
    let limit = cfg.anchors.max_shift_secs * a.frame_rate + 1.0;
    for (i, &v) in a.map.iter().enumerate() {
        let warp = v - i as f64 - offset_frames;
        assert!(
            warp.abs() <= limit,
            "frame {i} warped {warp} frames, beyond the {limit} allowed"
        );
    }
}

#[cfg(feature = "render")]
#[test]
fn an_aligned_dub_renders_and_keeps_its_pitch() {
    // Alignment is a timing edit, so the rendered dub must still be the
    // dub: retimed, not retuned.
    let reference = phrase(LINE, 0.0, 1.0);
    let dub_audio = phrase(LINE, 0.0, 1.15);
    let r = analyse(&reference);
    let mut d = analyse(&dub_audio);

    let a = align(&r, &d, AlignConfig::default()).expect("aligned");
    d.pitch.markers = expression_editor_audio::align::warp_markers(&a, d.frames.hop);
    let out = d.render(&dub_audio);
    assert!(!out.is_empty());

    let again = analyze_take(&out, SR, TakeConfig::default());
    assert!(!again.doc.notes.is_empty(), "the rendered dub still sings");
    // First note of the line is C4 in both takes.
    let n = &again.doc.notes[0];
    assert!(
        (n.row - 60).abs() <= 1,
        "retimed, not retuned — got row {}",
        n.row
    );
}
