//! Round trips through the DAW MIDI API.
//!
//! Everything here is pure conversion plus the standard-MIDI-file
//! codec, so it runs with no DAW attached — which is the point of
//! putting the codec in `daw-proto` and the conversion in a plain
//! function.

use daw::service::midi::{
    MidiCC, MidiCCCreate, MidiNote, MidiNoteCreate, MidiPitchBend, MidiTakeContent,
    MidiTakeSnapshot, smf,
};
use expression_editor_core::doc::Dimension;
use expression_editor_daw::{to_content, to_doc, write_warnings};

const PPQ: f64 = 960.0;

fn note(index: u32, channel: u8, pitch: u8, start: f64, len: f64) -> MidiNote {
    MidiNote {
        index,
        channel,
        pitch,
        velocity: 100,
        start_ppq: start,
        length_ppq: len,
        selected: false,
        muted: false,
    }
}

fn snapshot(notes: Vec<MidiNote>) -> MidiTakeSnapshot {
    MidiTakeSnapshot {
        length_ppq: notes
            .iter()
            .map(|n| n.start_ppq + n.length_ppq)
            .fold(0.0, f64::max),
        notes,
        ccs: Vec::new(),
        pitch_bends: Vec::new(),
        channel_pressures: Vec::new(),
        poly_pressures: Vec::new(),
        note_expressions: Vec::new(),
        ppq: PPQ,
    }
}

#[test]
fn notes_survive_a_round_trip() {
    let snap = snapshot(vec![
        note(0, 0, 60, 0.0, PPQ),
        note(1, 1, 64, PPQ, PPQ * 2.0),
        note(2, 2, 67, PPQ * 3.0, PPQ / 2.0),
    ]);
    let doc = to_doc(&snap, 48.0);
    assert_eq!(doc.notes.len(), 3);

    let back = to_content(&doc);
    assert_eq!(back.notes.len(), 3);
    for (a, b) in snap.notes.iter().zip(&back.notes) {
        assert_eq!(a.pitch, b.pitch);
        assert_eq!(a.channel, b.channel, "wire channels must come back 0-based");
        assert!((a.start_ppq - b.start_ppq).abs() < 1e-6);
        assert!((a.length_ppq - b.length_ppq).abs() < 1e-6);
        assert_eq!(a.velocity, b.velocity);
    }
}

#[test]
fn channels_convert_between_wire_and_musician_numbering() {
    let snap = snapshot(vec![note(0, 0, 60, 0.0, PPQ), note(1, 15, 64, 0.0, PPQ)]);
    let doc = to_doc(&snap, 48.0);
    // Channel 0 on the wire is channel 1 to a musician.
    assert_eq!(doc.notes[0].channel, Some(1));
    assert_eq!(doc.notes[1].channel, Some(16));
    let back = to_content(&doc);
    assert_eq!(back.notes[0].channel, 0);
    assert_eq!(back.notes[1].channel, 15);
}

#[test]
fn a_pitch_bend_lands_on_the_note_sounding_on_its_channel() {
    let mut snap = snapshot(vec![
        note(0, 0, 60, 0.0, PPQ * 2.0),
        note(1, 1, 67, 0.0, PPQ * 2.0),
    ]);
    // +2 semitones at 48-semitone range, on channel 1 only.
    let raw = ((2.0f64 / 48.0) * 8191.0).round() as i16;
    snap.pitch_bends.push(MidiPitchBend {
        index: 0,
        channel: 1,
        value: raw,
        position_ppq: PPQ,
        selected: false,
    });

    let doc = to_doc(&snap, 48.0);
    let bent = doc.notes.iter().find(|n| n.row == 67).unwrap();
    let plain = doc.notes.iter().find(|n| n.row == 60).unwrap();
    assert!(
        (bent.pitch.sample(PPQ, 0.0) - 2.0).abs() < 0.02,
        "got {}",
        bent.pitch.sample(PPQ, 0.0)
    );
    assert!(plain.pitch.is_empty(), "the other channel is untouched");
}

#[test]
fn bend_range_scales_the_curve() {
    let raw = 4095i16; // half of full deflection
    let mut snap = snapshot(vec![note(0, 0, 60, 0.0, PPQ)]);
    snap.pitch_bends.push(MidiPitchBend {
        index: 0,
        channel: 0,
        value: raw,
        position_ppq: 0.0,
        selected: false,
    });

    // The same wire value means different semitones at different
    // ranges — which is why bend_range has to travel with the doc.
    let wide = to_doc(&snap, 48.0);
    let narrow = to_doc(&snap, 2.0);
    let w = wide.notes[0].pitch.sample(0.0, 0.0);
    let n = narrow.notes[0].pitch.sample(0.0, 0.0);
    assert!(w > 20.0 && w < 28.0, "48-semitone range gave {w}");
    assert!(n > 0.8 && n < 1.2, "2-semitone range gave {n}");
}

#[test]
fn a_bend_on_a_shared_channel_is_not_attributed() {
    // Two notes sounding together on one channel: ownership is
    // genuinely undecidable, so guessing would corrupt the take.
    let mut snap = snapshot(vec![
        note(0, 0, 60, 0.0, PPQ * 2.0),
        note(1, 0, 64, 0.0, PPQ * 2.0),
    ]);
    snap.pitch_bends.push(MidiPitchBend {
        index: 0,
        channel: 0,
        value: 2000,
        position_ppq: PPQ,
        selected: false,
    });
    let doc = to_doc(&snap, 48.0);
    assert!(doc.notes.iter().all(|n| n.pitch.is_empty()));
    assert!(doc.notes.iter().all(|n| n.ambiguous), "and it is flagged");

    // And the caller is warned before anything is overwritten.
    let warnings = write_warnings(&doc);
    assert!(warnings.iter().any(|w| w.contains("share a channel")));
}

#[test]
fn ambiguous_notes_go_out_but_their_expression_does_not() {
    let mut snap = snapshot(vec![
        note(0, 0, 60, 0.0, PPQ * 2.0),
        note(1, 0, 64, 0.0, PPQ * 2.0),
    ]);
    snap.pitch_bends.push(MidiPitchBend {
        index: 0,
        channel: 0,
        value: 2000,
        position_ppq: PPQ,
        selected: false,
    });
    let mut doc = to_doc(&snap, 48.0);
    // Author expression anyway, as a user might.
    doc.notes[0].pitch.set(0.0, 1.0);
    doc.mark_ambiguity();

    let content = to_content(&doc);
    assert_eq!(content.notes.len(), 2, "the notes still get written");
    assert!(
        content.pitch_bends.is_empty(),
        "but not expression that cannot be attributed"
    );
}

#[test]
fn cc74_becomes_the_timbre_lane_and_other_ccs_become_document_lanes() {
    let mut snap = snapshot(vec![note(0, 0, 60, 0.0, PPQ * 2.0)]);
    snap.ccs.push(MidiCC {
        index: 0,
        channel: 0,
        controller: 74,
        value: 100,
        position_ppq: PPQ,
        selected: false,
    });
    snap.ccs.push(MidiCC {
        index: 1,
        channel: 0,
        controller: 11,
        value: 64,
        position_ppq: PPQ,
        selected: false,
    });

    let doc = to_doc(&snap, 48.0);
    let n = &doc.notes[0];
    assert!(
        (n.curve(Dimension::Timbre).sample(PPQ, 0.0) - 100.0 / 127.0).abs() < 1e-6,
        "CC74 is the MPE timbre dimension"
    );
    // CC11 is a document-level controller, not per-note.
    let cc11 = doc.cc.get(11).expect("CC11 lane created");
    assert_eq!(cc11.value(PPQ), 64);
    assert!(cc11.pinned, "the controllers an orchestral part rides show");
    assert!(
        doc.cc.get(74).is_none(),
        "CC74 is not duplicated as a dimension"
    );
}

#[test]
fn document_controllers_are_written_back_as_authored() {
    let snap = snapshot(vec![note(0, 0, 60, 0.0, PPQ * 2.0)]);
    let mut doc = to_doc(&snap, 48.0);
    let i = doc.cc.ensure(1);
    doc.cc.lanes[i].curve.set(0.0, 0.0);
    doc.cc.lanes[i].curve.set(PPQ, 1.0);

    let content = to_content(&doc);
    let cc1: Vec<&MidiCCCreate> = content.ccs.iter().filter(|c| c.controller == 1).collect();
    assert_eq!(cc1.len(), 2, "authored points, not resampled");
    assert_eq!(cc1[0].value, 0);
    assert_eq!(cc1[1].value, 127);
}

// ── standard MIDI files, through the same types ──────────────────────

#[test]
fn a_midi_file_round_trips_through_the_codec() {
    let content = MidiTakeContent {
        notes: vec![
            MidiNoteCreate {
                channel: 0,
                pitch: 60,
                velocity: 100,
                start_ppq: 0.0,
                length_ppq: PPQ,
            },
            MidiNoteCreate {
                channel: 2,
                pitch: 67,
                velocity: 80,
                start_ppq: PPQ * 2.0,
                length_ppq: PPQ / 2.0,
            },
        ],
        ccs: vec![MidiCCCreate {
            channel: 0,
            controller: 11,
            value: 90,
            position_ppq: PPQ,
        }],
        pitch_bends: Vec::new(),
        note_expressions: Vec::new(),
        ..Default::default()
    };

    let bytes = smf::encode(&content, PPQ);
    let back = smf::parse(&bytes, 0).expect("the file we just wrote must parse");

    assert_eq!(back.notes.len(), 2);
    assert_eq!(back.notes[0].pitch, 60);
    assert_eq!(back.notes[0].velocity, 100);
    assert!((back.notes[0].length_ppq - PPQ).abs() < 2.0);
    assert_eq!(back.notes[1].pitch, 67);
    assert_eq!(back.notes[1].channel, 2);
    assert!((back.notes[1].start_ppq - PPQ * 2.0).abs() < 2.0);
    assert_eq!(back.ccs.len(), 1);
    assert_eq!(back.ccs[0].value, 90);
}

#[test]
fn a_repeated_pitch_at_one_tick_does_not_swallow_its_retrigger() {
    // Note-offs must be written before note-ons at the same tick, or a
    // repeated pitch reads as one long note.
    let content = MidiTakeContent {
        notes: vec![
            MidiNoteCreate {
                channel: 0,
                pitch: 60,
                velocity: 100,
                start_ppq: 0.0,
                length_ppq: PPQ,
            },
            MidiNoteCreate {
                channel: 0,
                pitch: 60,
                velocity: 90,
                start_ppq: PPQ,
                length_ppq: PPQ,
            },
        ],
        ccs: Vec::new(),
        pitch_bends: Vec::new(),
        note_expressions: Vec::new(),
        ..Default::default()
    };
    let back = smf::parse(&smf::encode(&content, PPQ), 0).unwrap();
    assert_eq!(back.notes.len(), 2, "two notes, not one held one");
}

#[test]
fn a_file_at_another_division_is_rescaled() {
    let content = MidiTakeContent {
        notes: vec![MidiNoteCreate {
            channel: 0,
            pitch: 60,
            velocity: 100,
            start_ppq: 96.0,
            length_ppq: 96.0,
        }],
        ccs: Vec::new(),
        pitch_bends: Vec::new(),
        note_expressions: Vec::new(),
        ..Default::default()
    };
    // Written at 96 PPQ, read back at the snapshot's own 960.
    let back = smf::parse(&smf::encode(&content, 96.0), 0).unwrap();
    assert_eq!(back.ppq, 960.0, "callers never carry a per-file time base");
    assert!(
        (back.notes[0].start_ppq - 960.0).abs() < 2.0,
        "one quarter note in, got {}",
        back.notes[0].start_ppq
    );
}

#[test]
fn a_file_and_a_take_produce_the_same_document() {
    let content = MidiTakeContent {
        notes: vec![MidiNoteCreate {
            channel: 1,
            pitch: 64,
            velocity: 100,
            start_ppq: 0.0,
            length_ppq: PPQ,
        }],
        ccs: Vec::new(),
        pitch_bends: Vec::new(),
        note_expressions: Vec::new(),
        ..Default::default()
    };
    let from_file = to_doc(&smf::parse(&smf::encode(&content, PPQ), 0).unwrap(), 48.0);
    let from_take = to_doc(&snapshot(vec![note(0, 1, 64, 0.0, PPQ)]), 48.0);

    // The whole point of routing files through the same service.
    assert_eq!(from_file.notes.len(), from_take.notes.len());
    assert_eq!(from_file.notes[0].row, from_take.notes[0].row);
    assert_eq!(from_file.notes[0].channel, from_take.notes[0].channel);
}

#[test]
fn garbage_is_rejected_rather_than_panicking() {
    assert!(smf::parse(b"", 0).is_none());
    assert!(smf::parse(b"not a midi file at all", 0).is_none());
    // A truncated header must not index past the end.
    assert!(smf::parse(&smf::encode(&MidiTakeContent::default(), PPQ)[..10], 0).is_none());
}
