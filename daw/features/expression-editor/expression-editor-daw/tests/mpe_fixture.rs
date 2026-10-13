//! The synthesized MPE fixture, and what makes it MPE.
//!
//! MPE is the one mode with no real material (#159), so scenario 3
//! stands on a generated fixture. These tests are what stop it becoming
//! multi-channel MIDI wearing an MPE label: they assert the four
//! properties the real files that were found did *not* have — notes
//! overlapping across member channels, all three expression dimensions
//! on every note, a Configuration Message, and a bend range stated on
//! the wire rather than assumed by the reader.

use daw::service::midi::MidiTakeSnapshot;
use expression_editor_core::doc::Dimension;
use expression_editor_daw::fixture::{
    self, MASTER_CHANNEL, MEMBER_CHANNELS, PER_NOTE_BEND_RANGE, TIMBRE_CC,
};
use expression_editor_daw::{DEFAULT_BEND_RANGE, to_content, to_doc};

fn snapshot() -> MidiTakeSnapshot {
    fixture::snapshot()
}

#[test]
fn notes_overlap_and_are_spread_across_member_channels() {
    let snap = snapshot();
    assert!(snap.notes.len() >= 6, "a phrase, not a couple of notes");

    let mut overlaps = 0;
    for (i, a) in snap.notes.iter().enumerate() {
        for b in &snap.notes[i + 1..] {
            let overlapping = a.start_ppq < b.start_ppq + b.length_ppq
                && b.start_ppq < a.start_ppq + a.length_ppq;
            if overlapping {
                overlaps += 1;
                // The whole point of MPE: a sounding note owns its
                // channel, so its bend is attributable.
                assert_ne!(
                    a.channel, b.channel,
                    "notes {} and {} overlap on channel {}",
                    a.pitch, b.pitch, a.channel
                );
            }
        }
    }
    assert!(overlaps >= 6, "expected a genuinely polyphonic phrase");

    let used: std::collections::BTreeSet<u8> = snap.notes.iter().map(|n| n.channel).collect();
    assert!(
        used.len() >= 4,
        "voices must be spread, not stacked: {used:?}"
    );
    for ch in &used {
        assert_ne!(
            *ch, MASTER_CHANNEL,
            "notes never sound on the master channel"
        );
        assert!(
            (MASTER_CHANNEL + 1..MASTER_CHANNEL + 1 + MEMBER_CHANNELS).contains(ch),
            "channel {ch} is outside the declared zone"
        );
    }
}

#[test]
fn a_channel_is_only_reused_after_its_note_has_ended() {
    let snap = snapshot();
    // The fixture deliberately contains a note starting exactly where
    // an earlier one ends, on the channel it just freed. That is the
    // case a naive allocator gets wrong.
    let mut reuse = 0;
    for a in &snap.notes {
        for b in &snap.notes {
            if a.index != b.index && a.channel == b.channel && b.start_ppq >= a.start_ppq {
                assert!(
                    b.start_ppq >= a.start_ppq + a.length_ppq,
                    "channel {} reused before its note ended",
                    a.channel
                );
                if (b.start_ppq - (a.start_ppq + a.length_ppq)).abs() < 1e-9 {
                    reuse += 1;
                }
            }
        }
    }
    assert!(reuse >= 1, "the boundary-reuse case must be exercised");
}

#[test]
fn every_note_carries_bend_pressure_and_timbre() {
    let snap = snapshot();
    for n in &snap.notes {
        let end = n.start_ppq + n.length_ppq;
        let within = |ch: u8, t: f64| ch == n.channel && t >= n.start_ppq && t < end;

        let bends = snap
            .pitch_bends
            .iter()
            .filter(|b| within(b.channel, b.position_ppq))
            .count();
        let pressures = snap
            .channel_pressures
            .iter()
            .filter(|p| within(p.channel, p.position_ppq))
            .count();
        let timbre = snap
            .ccs
            .iter()
            .filter(|c| c.controller == TIMBRE_CC && within(c.channel, c.position_ppq))
            .count();

        // A stream, not one value at note-on: several instruments
        // ignore sparse expression entirely.
        assert!(bends >= 8, "note {} has {bends} bend events", n.pitch);
        assert!(
            pressures >= 8,
            "note {} has {pressures} pressure events",
            n.pitch
        );
        assert!(timbre >= 8, "note {} has {timbre} CC74 events", n.pitch);
    }

    // Pressure moves. A flat dimension would pass every count above while
    // being exactly the data a player never produces.
    let mut by_channel: std::collections::BTreeMap<u8, Vec<u8>> = Default::default();
    for p in &snap.channel_pressures {
        by_channel.entry(p.channel).or_default().push(p.pressure);
    }
    for (ch, vals) in by_channel {
        let min = *vals.iter().min().unwrap();
        let max = *vals.iter().max().unwrap();
        assert!(max - min > 20, "channel {ch} pressure barely moves");
    }
}

#[test]
fn the_stream_opens_with_an_mpe_configuration_message() {
    let snap = snapshot();
    let config = fixture::parse_config(&snap.ccs)
        .expect("an MPE stream without an RPN 6 zone declaration is not an MPE stream");

    assert_eq!(config.master_channel, MASTER_CHANNEL);
    assert_eq!(config.member_channels, MEMBER_CHANNELS);

    // The configuration precedes every note, or a receiver has already
    // rendered the first chord as ordinary MIDI by the time it arrives.
    let first_note = snap
        .notes
        .iter()
        .map(|n| n.start_ppq)
        .fold(f64::INFINITY, f64::min);
    for cc in snap
        .ccs
        .iter()
        .filter(|c| matches!(c.controller, 6 | 38 | 100 | 101))
    {
        assert!(
            cc.position_ppq <= first_note,
            "config CC{} lands after the first note",
            cc.controller
        );
    }
}

#[test]
fn the_declared_bend_range_is_the_one_the_editor_loads_with() {
    let snap = snapshot();
    let config = fixture::parse_config(&snap.ccs).expect("configuration message");

    // Stated, not assumed. This is the check a consumer should make
    // before loading: expression-editor-reaper defaults to 48, and a
    // file that meant 2 produces pitch curves wrong by a factor of 24
    // with nothing visible to say so.
    assert_eq!(config.per_note_bend_range, Some(PER_NOTE_BEND_RANGE));
    assert_eq!(PER_NOTE_BEND_RANGE, DEFAULT_BEND_RANGE);

    // The master's own range is a different quantity and must not be
    // mistaken for the per-note one.
    assert_ne!(fixture::MASTER_BEND_RANGE, PER_NOTE_BEND_RANGE);
}

#[test]
fn the_document_gets_per_note_expression_on_every_note() {
    let doc = fixture::doc();
    assert_eq!(doc.bend_range, PER_NOTE_BEND_RANGE);
    assert!(!doc.notes.is_empty());

    for n in &doc.notes {
        assert!(!n.ambiguous, "note {} came back ambiguous", n.row);
        assert!(
            n.channel.is_some_and(|c| c >= 2),
            "member channels are 2..=16 to a musician"
        );
        for dimension in Dimension::ALL {
            assert!(
                !n.curve(dimension).is_empty(),
                "note {} lost its {dimension:?} dimension on the way into the document",
                n.row
            );
        }
    }
}

#[test]
fn a_gesture_survives_into_the_document_at_its_written_depth() {
    let doc = fixture::doc();

    // The scoop: written a semitone flat and arriving at pitch. Twelve
    // steps of a 48-semitone range is a coarse grid, so the tolerance
    // is the quantization, not slack.
    let scooped = doc
        .notes
        .iter()
        .find(|n| n.row == 64)
        .expect("the scooped note");
    let first = scooped.curve(Dimension::Pitch).sample(scooped.start, 0.0);
    let last = scooped
        .curve(Dimension::Pitch)
        .sample(scooped.end - 1.0, 0.0);
    assert!(
        (first + 1.0).abs() < 0.05,
        "scoop starts at {first}, not a semitone flat"
    );
    assert!(last.abs() < 0.2, "scoop ends at {last}, not at pitch");

    // The wide one: a full octave of bend, which only reads correctly
    // at the declared range.
    let wide = doc
        .notes
        .iter()
        .find(|n| n.row == 76)
        .expect("the octave scoop");
    let start = wide.curve(Dimension::Pitch).sample(wide.start, 0.0);
    assert!(
        (start - 12.0).abs() < 0.05,
        "octave scoop starts at {start} semitones"
    );
}

#[test]
fn reading_at_the_wrong_bend_range_is_wrong_by_exactly_that_factor() {
    // Why the fixture states its range on the wire. Reading the same
    // bytes at MPE's *master* range instead of its per-note range does
    // not fail — it silently returns a curve 24 times too small.
    let snap = snapshot();
    let right = to_doc(&snap, PER_NOTE_BEND_RANGE);
    let wrong = to_doc(&snap, fixture::MASTER_BEND_RANGE);

    let pick = |doc: &expression_editor_core::doc::ExpressionDoc| {
        let n = doc.notes.iter().find(|n| n.row == 76).unwrap();
        n.curve(Dimension::Pitch).sample(n.start, 0.0)
    };
    let factor = pick(&right) / pick(&wrong);
    assert!(
        (factor - PER_NOTE_BEND_RANGE / fixture::MASTER_BEND_RANGE).abs() < 1e-6,
        "expected the ratio of the two ranges, got {factor}"
    );
}

#[test]
fn the_fixture_writes_back_without_losing_a_note_or_a_channel() {
    let doc = fixture::doc();
    let content = to_content(&doc);

    assert_eq!(content.notes.len(), doc.notes.len());
    for (a, b) in doc.notes.iter().zip(&content.notes) {
        assert_eq!(
            b.channel + 1,
            a.channel.unwrap(),
            "channel numbering flipped"
        );
        assert_eq!(b.pitch as i32, a.row);
    }
    // No note was ambiguous, so nothing was dropped on the way out —
    // the write path only discards expression it cannot attribute.
    assert!(!content.pitch_bends.is_empty());
    assert!(content.ccs.iter().any(|c| c.controller == TIMBRE_CC));
    // Changed by #167. This used to assert pressure leaving as CC11,
    // because `MidiTakeContent` had no field for it — but the snapshot
    // reads pressure from `channel_pressures`, so the dimension did not
    // survive a round trip. It now goes out as real channel pressure.
    assert!(
        !content.channel_pressures.is_empty(),
        "pressure must leave as pressure, or it cannot come back"
    );
    assert!(
        !content.ccs.iter().any(|c| c.controller == 11),
        "and not as CC11, which a genuine CC11 lane would then collide with"
    );
}
