//! A real Guitar Pro file, end to end (#168).
//!
//! **These are `#[ignore]`d, and the reason is a finding rather than an
//! omission.**
//!
//! The crate ships no corpus, so the plan was to generate a `.gp5` with
//! the same library that reads it — `guitarpro` writes gp3/4/5 as well.
//! That does not work: its GP5 **writer does not round-trip through its
//! own reader**. Two separate faults, both in the write path only:
//!
//! 1. `write_lyrics` indexes `lyrics.lines[0..5]` unconditionally while
//!    its own `Default` leaves the vec empty, so writing any default
//!    song panics. Worked around below by populating five lines.
//! 2. Past that, reading the bytes back fails with a nonsense page
//!    width (`687865856`), which is byte-offset drift — the writer emits
//!    a layout the reader does not expect.
//!
//! The **read path we actually ship is unaffected**, and it is the only
//! half the importer uses. But it does mean this cannot manufacture its
//! own fixture, so **no file that Guitar Pro itself wrote has been
//! through the importer yet**. That is the honest remaining gap on #168.
//!
//! Drop a real `.gp3/.gp4/.gp5/.gpx/.gp` into `tests/fixtures` and
//! `import.rs` exercises it end to end. Remove the `#[ignore]`s here if
//! the dependency's writer is ever fixed.

use expression_editor_core::doc::Dimension;
use expression_editor_core::rows::RowSpace;
use expression_editor_guitarpro::parse::Format;
use expression_editor_guitarpro::{BendFidelity, import_bytes};
use guitarpro::model::legacy::beat::{Beat, Voice};
use guitarpro::model::legacy::effects::{BendEffect, BendPoint};
use guitarpro::model::legacy::measure::Measure;
use guitarpro::model::legacy::note::Note;
use guitarpro::model::legacy::song::Song;
use guitarpro::model::legacy::track::Track;

/// A one-bar riff on a standard-tuned guitar, with one bent note.
fn write_gp5() -> Vec<u8> {
    let mut song = Song::default();
    // The library's writer indexes `lyrics.lines[0..5]` unconditionally
    // while its own `Default` leaves the vec empty, so writing any
    // default song panics. Populating the five lines is the workaround;
    // it is a bug in the dependency's *write* path, which is why the
    // read path we actually ship is unaffected.
    song.lyrics.lines = (0..5).map(|_| (0u8, 1u16, String::new())).collect();

    let mut track = Track {
        number: 1,
        name: "Guitar".into(),
        // GP numbers strings from 1, highest first.
        strings: vec![(1, 64), (2, 59), (3, 55), (4, 50), (5, 45), (6, 40)],
        fret_count: 24,
        ..Track::default()
    };

    let mut voice = Voice::default();

    // Plain note, low E open.
    let mut plain = Note {
        value: 0,
        string: 6,
        ..Note::default()
    };
    plain.effect.palm_mute = true;

    // A bend on the G string: up a whole tone, held, released.
    let mut bent = Note {
        value: 7,
        string: 3,
        ..Note::default()
    };
    bent.effect.bend = Some(BendEffect {
        points: vec![
            BendPoint {
                position: 0,
                value: 0,
                vibrato: false,
            },
            BendPoint {
                position: 6,
                value: 4,
                vibrato: false,
            },
            BendPoint {
                position: 9,
                value: 4,
                vibrato: false,
            },
            BendPoint {
                position: 12,
                value: 0,
                vibrato: false,
            },
        ],
        ..BendEffect::default()
    });

    voice.beats.push(Beat {
        notes: vec![plain],
        ..Beat::default()
    });
    voice.beats.push(Beat {
        notes: vec![bent],
        ..Beat::default()
    });

    let mut measure = Measure::default();
    measure.voices.push(voice);
    track.measures.push(measure);
    // The writer reaches `channels[track.channel_index]`, so a track
    // needs a channel to exist for it — another place `Default` alone
    // is not a writable song.
    song.channels
        .push(guitarpro::audio::midi::MidiChannel::default());
    song.measure_headers
        .push(guitarpro::model::legacy::headers::MeasureHeader::default());
    song.tracks.push(track);

    song.write((5, 1, 0), None).expect("write a gp5")
}

/// The same song at an earlier version.
fn write_version(major: u8, minor: u8, patch: u8) -> Vec<u8> {
    let mut song = Song::default();
    song.lyrics.lines = (0..5).map(|_| (0u8, 1u16, String::new())).collect();
    let mut track = Track {
        number: 1,
        name: "Guitar".into(),
        strings: vec![(1, 64), (2, 59), (3, 55), (4, 50), (5, 45), (6, 40)],
        fret_count: 24,
        ..Track::default()
    };
    let mut voice = Voice::default();
    voice.beats.push(Beat {
        notes: vec![Note {
            value: 5,
            string: 6,
            ..Note::default()
        }],
        ..Beat::default()
    });
    let mut measure = Measure::default();
    measure.voices.push(voice);
    track.measures.push(measure);
    song.channels
        .push(guitarpro::audio::midi::MidiChannel::default());
    song.measure_headers
        .push(guitarpro::model::legacy::headers::MeasureHeader::default());
    song.tracks.push(track);
    song.write((major, minor, patch), None).expect("write")
}

/// Which versions the library can read back after writing.
#[test]
fn which_versions_round_trip_through_the_library() {
    for (name, bytes, fmt) in [
        ("gp3", write_version(3, 0, 0), Format::Gp3),
        ("gp4", write_version(4, 0, 6), Format::Gp4),
        ("gp5", write_version(5, 1, 0), Format::Gp5),
    ] {
        match import_bytes(&bytes, fmt) {
            Ok(_) => eprintln!("{name}: round-trips"),
            Err(e) => eprintln!("{name}: does NOT round-trip — {e}"),
        }
    }
}

#[test]
#[ignore = "guitarpro 0.4.2's gp5 writer does not round-trip through its own reader; see the module docs"]
fn a_real_gp5_file_imports_as_a_string_roll() {
    let bytes = write_gp5();
    assert!(bytes.len() > 32, "the writer produced nothing");

    let out = import_bytes(&bytes, Format::Gp5).expect("import the gp5 we just wrote");

    match &out.doc.row_space {
        RowSpace::Strings(t) => {
            assert_eq!(t.strings(), 6, "six strings");
            assert_eq!(
                t.open_pitches[0], 40,
                "row 0 is the low E — the file's order is reversed once, at the boundary"
            );
            assert_eq!(t.open_pitches[5], 64, "and row 5 is the high E");
        }
        other => panic!("expected a string roll, got {other:?}"),
    }
    assert!(!out.doc.notes.is_empty(), "no notes survived the trip");
}

#[test]
#[ignore = "guitarpro 0.4.2's gp5 writer does not round-trip through its own reader; see the module docs"]
fn a_bend_written_to_a_real_file_comes_back_as_a_curve() {
    let out = import_bytes(&write_gp5(), Format::Gp5).expect("import");

    let bent = out
        .doc
        .notes
        .iter()
        .find(|n| !n.curve(Dimension::Pitch).points().is_empty())
        .expect("the bent note lost its curve on the way through the format");

    let pts = bent.curve(Dimension::Pitch).points();
    assert!(
        pts.len() >= 3,
        "a held bend needs its middle points: {pts:?}"
    );

    let peak = pts.iter().map(|p| p.value).fold(f64::MIN, f64::max);
    assert!(
        peak > 0.5,
        "the bend came back flat — peak {peak} semitones"
    );
}

#[test]
#[ignore = "guitarpro 0.4.2's gp5 writer does not round-trip through its own reader; see the module docs"]
fn a_binary_file_keeps_full_bend_fidelity() {
    // The claim the format split rests on: gp3/4/5 carry point lists,
    // GPIF does not.
    let out = import_bytes(&write_gp5(), Format::Gp5).expect("import");
    assert_eq!(
        out.bends,
        BendFidelity::Full,
        "a binary file should not be reported as endpoints-only"
    );
}

#[test]
#[ignore = "guitarpro 0.4.2's gp5 writer does not round-trip through its own reader; see the module docs"]
fn articulations_survive_the_format() {
    use expression_editor_core::rows::Articulation;
    let out = import_bytes(&write_gp5(), Format::Gp5).expect("import");

    let arts: Vec<Option<Articulation>> = out.doc.notes.iter().map(|n| n.articulation).collect();
    assert!(
        arts.contains(&Some(Articulation::PalmMute)),
        "the palm mute did not survive: {arts:?}"
    );
    assert!(
        arts.contains(&Some(Articulation::Bend)),
        "the bent note is not marked as bent: {arts:?}"
    );
}

#[test]
fn the_importer_refuses_a_file_it_cannot_read() {
    // Rather than producing an empty roll that looks like a valid
    // import of nothing.
    let err = import_bytes(b"this is not a guitar pro file", Format::Gp5);
    assert!(err.is_err());
}
