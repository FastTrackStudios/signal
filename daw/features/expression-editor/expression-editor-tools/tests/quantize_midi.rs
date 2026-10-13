//! Quantize over the MIDI side of the seam.
//!
//! The audio side is covered, unchanged, by
//! `expression-editor-audio/tests/quantize.rs` — the same planner, the
//! same cases, a different kind of event. What these prove is the part
//! that is new: notes go onto a grid stated in ticks, they keep their
//! length and the expression they own, velocity is what a division
//! contest is decided on, and an event with no end goes through the
//! identical code path without a branch anywhere.

use expression_editor_core::doc::{Curve, Note, NoteId, Point};
use expression_editor_tools::quantize::{self, QuantizeConfig};
use expression_editor_tools::{Sustained, Timed};

/// 96 ppq — one sixteenth is 24 ticks.
const PPQ: f64 = 96.0;
const SIXTEENTH: f64 = PPQ / 4.0;

fn cfg(grid: f64) -> QuantizeConfig {
    QuantizeConfig {
        grid,
        ..QuantizeConfig::default()
    }
}

/// A note of `len` ticks starting at `start`, hit at `velocity`.
fn note(id: u64, start: f64, len: f64, velocity: f64) -> Note {
    let mut n = Note::new(NoteId(id), start, start + len, 60);
    n.velocity = velocity;
    n
}

#[test]
fn late_notes_are_pulled_onto_their_division() {
    // Sixteenths, each two ticks late.
    let mut notes: Vec<Note> = (0..8)
        .map(|i| note(i as u64 + 1, i as f64 * SIXTEENTH + 2.0, 12.0, 0.8))
        .collect();

    let plan = quantize::plan(&notes, cfg(SIXTEENTH));
    assert_eq!(plan.moves.len(), 8);
    quantize::apply(&mut notes, &plan);

    for (i, n) in notes.iter().enumerate() {
        assert_eq!(
            n.start,
            i as f64 * SIXTEENTH,
            "note {i} is off its division"
        );
    }
}

#[test]
fn a_note_keeps_its_length_and_its_end_moves_with_it() {
    // The reason the second trait exists, stated as behaviour: nothing
    // in the planner or the caller mentions length, and the note is
    // still the same length afterwards.
    let mut notes = vec![note(1, 26.0, 18.0, 0.8)];
    let plan = quantize::plan(&notes, cfg(SIXTEENTH));
    quantize::apply(&mut notes, &plan);

    assert_eq!(notes[0].start, SIXTEENTH);
    assert_eq!(notes[0].end, SIXTEENTH + 18.0);
    assert_eq!(plan.moves[0].length, Some(18.0));
    assert_eq!(plan.moves[0].span(), Some((SIXTEENTH, SIXTEENTH + 18.0)));
}

#[test]
fn the_expression_a_note_owns_moves_with_it() {
    // A bare `start = to` would leave the pitch curve and the zone
    // splits behind — the note would arrive on the beat with someone
    // else's bend on it.
    let mut n = note(1, 26.0, 24.0, 0.8);
    n.pitch = Curve::from_points(vec![
        Point {
            t: 26.0,
            value: 0.0,
            ..Point::default()
        },
        Point {
            t: 38.0,
            value: 1.5,
            ..Point::default()
        },
    ]);
    n.splits = vec![38.0];
    let mut notes = vec![n];

    let plan = quantize::plan(&notes, cfg(SIXTEENTH));
    quantize::apply(&mut notes, &plan);

    let shift = SIXTEENTH - 26.0;
    let ts: Vec<f64> = notes[0].pitch.points().iter().map(|p| p.t).collect();
    assert_eq!(ts, vec![26.0 + shift, 38.0 + shift]);
    assert_eq!(notes[0].splits, vec![38.0 + shift]);
}

#[test]
fn velocity_is_what_wins_a_division() {
    // A flam: two notes inside one division's window. Only one can have
    // it, and it must be the one that was played, not the grace note
    // that happens to sit a tick closer.
    let notes = vec![
        note(1, SIXTEENTH - 1.0, 12.0, 0.2),
        note(2, SIXTEENTH + 3.0, 12.0, 0.9),
    ];
    let plan = quantize::plan(
        &notes,
        QuantizeConfig {
            grid: SIXTEENTH,
            tolerance: Some(6.0),
            ..QuantizeConfig::default()
        },
    );

    assert_eq!(plan.moves.len(), 1, "one division, one move");
    assert_eq!(plan.moves[0].index, 1, "the harder-hit note wins");
    assert_eq!(plan.unmatched, vec![SIXTEENTH - 1.0]);
}

#[test]
fn the_sensitivity_filter_leaves_ghost_notes_where_they_were() {
    // Audio gets this from the detector's gate. MIDI has no detector,
    // so a programmed ghost note is excluded here — left alone, not
    // deleted, and not quantized onto a beat it was deliberately behind.
    let notes = vec![
        note(1, 2.0, 12.0, 0.9),
        note(2, SIXTEENTH / 2.0 + 2.0, 6.0, 0.1),
        note(3, SIXTEENTH + 2.0, 12.0, 0.9),
    ];
    let plan = quantize::plan(
        &notes,
        QuantizeConfig {
            grid: SIXTEENTH,
            tolerance: Some(6.0),
            min_strength: 0.3,
            ..QuantizeConfig::default()
        },
    );

    assert_eq!(plan.moves.len(), 2);
    assert!(plan.moves.iter().all(|m| m.index != 1));
    assert_eq!(plan.unmatched, vec![SIXTEENTH / 2.0 + 2.0]);
}

#[test]
fn strength_scales_the_correction_the_same_way_it_does_for_audio() {
    let notes = vec![note(1, 26.0, 12.0, 0.8)];
    let at = |strength| {
        quantize::plan(
            &notes,
            QuantizeConfig {
                grid: SIXTEENTH,
                strength,
                ..QuantizeConfig::default()
            },
        )
    };
    assert_eq!(at(1.0).moves[0].to, SIXTEENTH);
    assert_eq!(at(0.5).moves[0].to, 25.0);
    assert_eq!(at(0.0).moves[0].shift(), 0.0);
    // Strength never changes which division a note belongs to.
    assert_eq!(at(0.0).moves[0].division, SIXTEENTH);
}

#[test]
fn a_plan_previews_as_the_rectangles_it_will_produce() {
    let notes = vec![note(1, 26.0, 18.0, 0.8), note(2, 50.0, 6.0, 0.8)];
    let plan = quantize::plan(&notes, cfg(SIXTEENTH));
    assert_eq!(
        quantize::spans(&notes, &plan),
        vec![(24.0, 42.0), (48.0, 54.0)]
    );
}

// ── The other side of the seam ────────────────────────────────────────

/// An event with an onset and nothing else — the shape of an audio
/// transient, without depending on the audio crate.
struct Tick {
    at: f64,
    level: f64,
}

impl Timed for Tick {
    fn onset(&self) -> f64 {
        self.at
    }
    fn move_to(&mut self, to: f64) {
        self.at = to;
    }
    fn strength(&self) -> f64 {
        self.level
    }
}

#[test]
fn an_event_with_no_end_takes_the_same_path_and_reports_no_length() {
    // Same call, same planner, no branch: the only difference in the
    // result is the length nobody claimed to have.
    let mut ticks = vec![
        Tick {
            at: 2.0,
            level: 0.9,
        },
        Tick {
            at: SIXTEENTH + 2.0,
            level: 0.9,
        },
    ];
    let plan = quantize::plan(&ticks, cfg(SIXTEENTH));
    quantize::apply(&mut ticks, &plan);

    assert_eq!(
        ticks.iter().map(|t| t.at).collect::<Vec<_>>(),
        vec![0.0, SIXTEENTH]
    );
    assert!(plan.moves.iter().all(|m| m.length.is_none()));
    assert!(plan.moves.iter().all(|m| m.span().is_none()));
}

// ── Pitch-detected notes ──────────────────────────────────────────────

#[test]
fn a_detected_note_moves_in_whole_frames_and_keeps_its_frame_count() {
    // The blob's drift and modulation are one value per frame of the
    // note, so quantizing may move it but may never change how long it
    // is.
    use tune_dsp::model::NoteBlob;

    let mut blob = NoteBlob {
        start_frame: 26,
        end_frame: 44,
        center_midi: 60.0,
        analyzed_center_midi: 60.0,
        drift: vec![0.0; 19],
        modulation: vec![0.0; 19],
        drift_amount: 1.0,
        modulation_amount: 1.0,
        formant_shift: 0.0,
        gain_db: 0.0,
        retune_s: 0.0,
        rms: 0.4,
    };
    assert_eq!(blob.length(), Some(18.0));
    assert_eq!(Sustained::end(&blob), 44.0);
    assert_eq!(blob.strength(), 0.4);

    let mut blobs = [blob.clone()];
    let plan = quantize::plan(&blobs, cfg(SIXTEENTH));
    quantize::apply(&mut blobs, &plan);
    blob = blobs[0].clone();

    assert_eq!(blob.start_frame, 24);
    assert_eq!(
        blob.end_frame, 42,
        "the frame count is not the tool's to change"
    );
}
