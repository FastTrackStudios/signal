//! The analysis ↔ document path, exercised with no audio and no DSP.

use expression_editor_audio::{FORMANT_RANGE, GAIN_RANGE_DB, Span, apply_to, spans, to_doc};
use expression_editor_core::doc::{Dimension, NoteId};
use tune_dsp::TrackedNote;
use tune_dsp::model::{NoteBlob, PitchDoc};

const FRAME_RATE: f64 = 100.0;

const HOP: usize = 441;
const SAMPLE_RATE: f64 = 44100.0;

/// A blob sung around `center`, with a scoop in and a growing vibrato.
///
/// Built through the real [`tune_dsp::model::decompose`] rather than by
/// hand, because the model has an invariant that matters here:
/// `center` is the *median* of the contour and `drift` is the residual
/// about it. A handcrafted blob whose drift has a non-zero median
/// describes a note that analysis could never produce, and every
/// centre-related assertion against it would be measuring the fixture.
fn blob(start: usize, end: usize, center: f64) -> NoteBlob {
    let n = end - start + 1;
    // The scoop occupies the first 15% and then the note sustains. A
    // scoop spread across the whole note would drag the median with it,
    // and the note's centre would no longer be the pitch it is heard
    // as — which is a fixture artefact, not something a singer does.
    let f0: Vec<f64> = (0..n)
        .map(|i| {
            let f = i as f64 / (n.max(2) - 1) as f64;
            let scoop = -1.5 * (1.0 - (f / 0.15).min(1.0)).powi(3);
            let vib = 0.2 * (f * 18.0).sin() * f;
            tune_dsp::midi_to_hz(center + scoop + vib)
        })
        .collect();
    let mut midi: Vec<f64> = f0.iter().map(|&hz| tune_dsp::hz_to_midi(hz)).collect();
    midi.sort_by(f64::total_cmp);
    let note = TrackedNote {
        start_frame: start,
        end_frame: end,
        f0,
        median_midi: midi[midi.len() / 2],
    };
    let mut b = tune_dsp::model::decompose(&note, HOP, SAMPLE_RATE);
    b.rms = 0.5;
    b
}

fn doc_of(blobs: Vec<NoteBlob>) -> (PitchDoc, expression_editor_core::ExpressionDoc) {
    let pitch = PitchDoc {
        blobs,
        markers: Vec::new(),
        hop: HOP,
        sample_rate: SAMPLE_RATE,
    };
    let doc = to_doc(&pitch, FRAME_RATE);
    (pitch, doc)
}

#[test]
fn a_note_sits_on_its_literal_row_with_the_detune_in_the_curve() {
    // Sung 40 cents flat of C4.
    let (_, doc) = doc_of(vec![blob(0, 99, 60.0 - 0.4)]);
    let n = doc.note(NoteId(1)).unwrap();

    assert_eq!(n.row, 60, "the rectangle is on the row it is nearest");
    // The flatness has to be in the curve, not in the row. Read it as
    // the centre of the contour rather than at any single frame —
    // drift and vibrato are passing through at every one of those.
    let d = expression_editor_core::blob::decompose(&n.pitch, n.start, n.end, 128, FRAME_RATE, 0.0);
    assert!(
        (d.center - -0.4).abs() < 0.05,
        "the flatness lives in the curve, got {}",
        d.center
    );
}

#[test]
fn the_curve_is_the_sounding_contour_not_just_the_centre() {
    let (pitch, doc) = doc_of(vec![blob(0, 99, 62.0)]);
    let n = doc.note(NoteId(1)).unwrap();
    let b = &pitch.blobs[0];

    // Every frame agrees with what the blob says it sounds.
    for frame in [0usize, 25, 50, 75, 99] {
        let want = b.target_midi(frame).unwrap() - n.row as f64;
        let got = n.pitch.sample(frame as f64, 0.0);
        assert!(
            (want - got).abs() < 1e-9,
            "frame {frame}: blob says {want}, curve says {got}"
        );
    }
    // And the scoop is genuinely there, not flattened to the centre.
    let scoop = n.pitch.sample(0.0, 0.0);
    assert!(scoop < -1.0, "the sung scoop survived, got {scoop}");
}

#[test]
fn frames_and_document_time_are_the_same_number() {
    let (_, doc) = doc_of(vec![blob(120, 300, 60.0)]);
    let n = doc.note(NoteId(1)).unwrap();
    assert_eq!(n.start, 120.0);
    assert_eq!(n.end, 300.0);
    assert!(matches!(
        doc.time_base,
        expression_editor_core::TimeBase::Frames { frame_rate } if frame_rate == FRAME_RATE
    ));
}

#[test]
fn moving_a_note_writes_its_new_centre_back() {
    let (mut pitch, mut doc) = doc_of(vec![blob(0, 99, 60.0)]);
    // Transpose up a tone, the way a drag on the note body would.
    doc.note_mut(NoteId(1)).unwrap().row = 62;

    assert_eq!(apply_to(&doc, &mut pitch), 1);
    assert!(
        (pitch.blobs[0].center_midi - 62.0).abs() < 0.05,
        "got {}",
        pitch.blobs[0].center_midi
    );
}

#[test]
fn writing_back_never_rewrites_what_was_sung() {
    let (mut pitch, doc) = doc_of(vec![blob(0, 99, 60.0)]);
    let sung_drift = pitch.blobs[0].drift.clone();
    let sung_mod = pitch.blobs[0].modulation.clone();
    let analyzed = pitch.blobs[0].analyzed_center_midi;

    apply_to(&doc, &mut pitch);

    assert_eq!(pitch.blobs[0].drift, sung_drift);
    assert_eq!(pitch.blobs[0].modulation, sung_mod);
    assert_eq!(
        pitch.blobs[0].analyzed_center_midi, analyzed,
        "the analyzed centre is the anchor edits are deltas from"
    );
}

#[test]
fn the_per_note_trims_round_trip_exactly() {
    let mut b = blob(0, 99, 60.0);
    b.formant_shift = -3.5;
    b.gain_db = 6.0;
    let (mut pitch, doc) = doc_of(vec![b]);

    // They survive the trip out...
    let n = doc.note(NoteId(1)).unwrap();
    let mid = (n.start + n.end) * 0.5;
    assert!(n.timbre.sample(mid, Dimension::Timbre.default_value()) < 0.5);
    assert!(n.pressure.sample(mid, Dimension::Pressure.default_value()) > 0.5);

    // ...and come back as the same numbers, not re-derived ones.
    pitch.blobs[0].formant_shift = 0.0;
    pitch.blobs[0].gain_db = 0.0;
    apply_to(&doc, &mut pitch);
    assert!((pitch.blobs[0].formant_shift - -3.5).abs() < 1e-6);
    assert!((pitch.blobs[0].gain_db - 6.0).abs() < 1e-6);
}

#[test]
fn a_trim_at_the_range_limit_still_round_trips() {
    let mut b = blob(0, 99, 60.0);
    b.formant_shift = FORMANT_RANGE;
    b.gain_db = -GAIN_RANGE_DB;
    let (mut pitch, doc) = doc_of(vec![b]);
    apply_to(&doc, &mut pitch);
    assert!((pitch.blobs[0].formant_shift - FORMANT_RANGE).abs() < 1e-6);
    assert!((pitch.blobs[0].gain_db - -GAIN_RANGE_DB).abs() < 1e-6);
}

#[test]
fn several_notes_keep_their_pairing() {
    let (mut pitch, mut doc) = doc_of(vec![
        blob(0, 49, 60.0),
        blob(50, 99, 64.0),
        blob(100, 149, 67.0),
    ]);
    assert_eq!(doc.notes.len(), 3);

    doc.note_mut(NoteId(2)).unwrap().row = 65;
    assert_eq!(apply_to(&doc, &mut pitch), 3);
    assert!((pitch.blobs[0].center_midi - 60.0).abs() < 0.05);
    assert!((pitch.blobs[1].center_midi - 65.0).abs() < 0.05);
    assert!((pitch.blobs[2].center_midi - 67.0).abs() < 0.05);
}

#[test]
fn a_deleted_note_leaves_its_blob_alone_rather_than_shifting_the_rest() {
    let (mut pitch, mut doc) = doc_of(vec![blob(0, 49, 60.0), blob(50, 99, 64.0)]);
    doc.notes.retain(|n| n.id != NoteId(1));

    // Only the surviving pairing is written; blob 0 is not silently
    // reassigned to what is now the first note.
    assert_eq!(apply_to(&doc, &mut pitch), 1);
    assert!((pitch.blobs[0].center_midi - 60.0).abs() < 0.05);
}

// ── unvoiced spans ───────────────────────────────────────────────────

#[test]
fn unvoiced_runs_become_spans() {
    let f0 = vec![
        Some(220.0),
        Some(220.0),
        None,
        None,
        None,
        Some(230.0),
        None,
        None,
    ];
    assert_eq!(
        spans::unvoiced_spans(&f0),
        vec![Span { start: 2, end: 4 }, Span { start: 6, end: 7 }]
    );
}

#[test]
fn a_single_dropped_frame_is_a_glitch_not_a_consonant() {
    let f0 = vec![Some(220.0), None, Some(220.0), None, None, Some(220.0)];
    assert_eq!(
        spans::unvoiced_spans(&f0),
        vec![Span { start: 3, end: 4 }],
        "one frame is a tracker miss; the roll must not fill with slivers"
    );
}

#[test]
fn an_entirely_unvoiced_take_is_one_span() {
    let f0 = vec![None; 10];
    assert_eq!(spans::unvoiced_spans(&f0), vec![Span { start: 0, end: 9 }]);
    assert!(spans::unvoiced_spans(&[]).is_empty());
    assert!(spans::unvoiced_spans(&[Some(1.0), Some(2.0)]).is_empty());
}

#[test]
fn sibilant_scope_clips_to_the_note_rather_than_dropping_the_overlap() {
    let all = vec![
        Span { start: 0, end: 20 },
        Span { start: 45, end: 55 },
        Span { start: 90, end: 99 },
    ];
    // A note covering 10..=50 owns the tails of the first two spans.
    let within = spans::spans_within(&all, 10, 50);
    assert_eq!(
        within,
        vec![Span { start: 10, end: 20 }, Span { start: 45, end: 50 }],
        "the half inside the note is still the note's to ride"
    );
}

#[test]
fn a_frame_finds_the_span_it_is_in() {
    let all = vec![Span { start: 5, end: 9 }, Span { start: 20, end: 25 }];
    assert_eq!(spans::span_at(&all, 7), Some(Span { start: 5, end: 9 }));
    assert_eq!(spans::span_at(&all, 5), Some(Span { start: 5, end: 9 }));
    assert_eq!(spans::span_at(&all, 9), Some(Span { start: 5, end: 9 }));
    assert_eq!(spans::span_at(&all, 12), None);
}

// ── warp markers from timing edits ───────────────────────────────────

#[test]
fn moving_a_note_produces_the_warp_that_gets_it_there() {
    let (pitch, mut doc) = doc_of(vec![blob(0, 99, 60.0), blob(200, 299, 62.0)]);
    // Slide the second note ten frames late, as a timing drag would.
    let n = doc.note_mut(NoteId(2)).unwrap();
    n.start += 10.0;
    n.end += 10.0;

    let markers = expression_editor_audio::warp_markers(&doc, &pitch);
    assert_eq!(markers.len(), 4, "one per note edge");

    // The untouched note warps by nothing.
    assert!(markers[0].d_time.abs() < 1e-9);
    assert!(markers[1].d_time.abs() < 1e-9);
    // The moved one carries the offset, in samples.
    let hop = HOP as f64;
    assert!((markers[2].d_time - 10.0 * hop).abs() < 1e-6);
    assert!((markers[3].d_time - 10.0 * hop).abs() < 1e-6);
    // Anchored at the analyzed positions, not the edited ones — the
    // warp maps *from* the recording.
    assert!((markers[2].sample - 200.0 * hop).abs() < 1e-6);
}

#[test]
fn stretching_a_note_maps_its_interior_rather_than_shifting_it() {
    let (pitch, mut doc) = doc_of(vec![blob(0, 99, 60.0)]);
    // Twice as long, anchored at the start.
    doc.note_mut(NoteId(1)).unwrap().end = 199.0;

    let markers = expression_editor_audio::warp_markers(&doc, &pitch);
    assert_eq!(markers.len(), 2);
    assert!(markers[0].d_time.abs() < 1e-9, "the start stayed put");
    assert!(
        markers[1].d_time > 0.0,
        "and the end moved later, so everything between is stretched"
    );
}

#[test]
fn abutting_notes_do_not_leave_a_doubled_marker() {
    // Two blobs sharing an edge: a marker list with a repeated sample
    // has an undefined slope there.
    let (pitch, doc) = doc_of(vec![blob(0, 99, 60.0), blob(99, 199, 62.0)]);
    let markers = expression_editor_audio::warp_markers(&doc, &pitch);

    let mut samples: Vec<f64> = markers.iter().map(|m| m.sample).collect();
    let before = samples.len();
    samples.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    assert_eq!(samples.len(), before, "no repeated anchor");
    // And they come out sorted, which the piecewise map assumes.
    let mut sorted = samples.clone();
    sorted.sort_by(f64::total_cmp);
    assert_eq!(samples, sorted);
}
