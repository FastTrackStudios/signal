//! Guitar Pro onto the six-string roll (#168).
//!
//! The mapping is where our logic lives, so that is what these assert:
//! string as row, fret as label, the file's own tuning driving the row
//! space, and a bend becoming a per-note curve in semitones.

use expression_editor_core::doc::Dimension;
use expression_editor_core::rows::{RowSpace, StringTuning};
use expression_editor_guitarpro::parse::Format;
use expression_editor_guitarpro::{
    BendFidelity, BendPoint, GpNote, bend_curve, row_of, to_document,
};

fn standard() -> StringTuning {
    StringTuning::guitar_standard()
}

fn note(string: usize, fret: i32, start: f64, len: f64) -> GpNote {
    GpNote {
        string,
        fret,
        start,
        length: len,
        bend: Vec::new(),
        prebend: false,
        articulation: None,
    }
}

// ── The row space ────────────────────────────────────────────────────

#[test]
fn the_files_tuning_drives_the_row_space() {
    // Not a default six-string: a drop-D or a seven-string has to read
    // correctly or every fret label is wrong.
    let drop_d = StringTuning {
        name: "Drop D",
        open_pitches: vec![38, 45, 50, 55, 59, 64],
        frets: 24,
        capo: 0,
    };
    let out = to_document(&[note(0, 0, 0.0, 960.0)], drop_d.clone());
    match &out.doc.row_space {
        RowSpace::Strings(t) => {
            assert_eq!(t.open_pitches, drop_d.open_pitches);
            assert_eq!(t.frets, 24);
        }
        other => panic!("expected a string roll, got {other:?}"),
    }
}

#[test]
fn a_seven_string_file_gets_seven_rows() {
    let seven = StringTuning {
        name: "Seven",
        open_pitches: vec![35, 40, 45, 50, 55, 59, 64],
        frets: 24,
        capo: 0,
    };
    let out = to_document(&[note(6, 5, 0.0, 960.0)], seven);
    match &out.doc.row_space {
        RowSpace::Strings(t) => assert_eq!(t.strings(), 7),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_row_is_the_pitch_and_the_string_rides_on_the_note() {
    // A guitar roll is a full MIDI roll. #161's prototype put the
    // string on the vertical axis; that was reversed, because a guitar
    // part is notes first and fingering second — the string is what
    // colours a note and makes its fret computable, not where it sits.
    let t = standard();
    let out = to_document(
        &[note(2, 5, 0.0, 960.0), note(2, 12, 960.0, 960.0)],
        t.clone(),
    );
    assert_eq!(out.doc.notes[0].row, t.open(2) + 5);
    assert_eq!(
        out.doc.notes[1].row,
        t.open(2) + 12,
        "the twelfth fret sounds an octave above the fifth, and the \
         roll has to show that"
    );
    for n in &out.doc.notes {
        assert_eq!(n.string, Some(2), "both are played on the D string");
    }
    // And the fret comes back out.
    assert_eq!(
        expression_editor_core::rows::fret_of(&out.doc.notes[1], &t),
        Some(12)
    );
}

#[test]
fn the_sounding_pitch_is_still_derivable_from_string_and_fret() {
    // The label is the fret; the pitch is what the row space knows.
    let t = standard();
    assert_eq!(row_of(&t, 0, 0), 40, "open low E");
    assert_eq!(row_of(&t, 0, 5), 45, "fifth fret is the A string open");
    assert_eq!(row_of(&t, 5, 12), 76, "twelfth on the high E");
}

#[test]
fn a_capo_raises_every_open_string() {
    let t = StringTuning {
        capo: 2,
        ..standard()
    };
    assert_eq!(row_of(&t, 0, 0), 42, "open low E behind a second-fret capo");
}

// ── Bends ────────────────────────────────────────────────────────────

#[test]
fn a_bend_becomes_a_curve_in_semitones() {
    // #160's mapping: position is sixtieths of the note, value is
    // hundredths of a whole tone, so 50 is one semitone.
    let curve = bend_curve(
        &[
            BendPoint {
                position: 0.0,
                value: 0.0,
            },
            BendPoint {
                position: 30.0,
                value: 100.0,
            },
            BendPoint {
                position: 60.0,
                value: 100.0,
            },
        ],
        960.0,
    );
    let pts = curve.points();
    assert_eq!(pts.len(), 3);
    assert_eq!(pts[0].t, 0.0);
    assert_eq!(pts[1].t, 480.0, "halfway through the note");
    assert_eq!(pts[1].value, 2.0, "a whole tone is two semitones");
    assert_eq!(pts[2].t, 960.0);
}

#[test]
fn a_bend_that_holds_keeps_its_plateau() {
    // The two middle offsets exist precisely so a bend can hold. A
    // curve that lost them would ramp continuously and sound wrong.
    let curve = bend_curve(
        &[
            BendPoint {
                position: 0.0,
                value: 0.0,
            },
            BendPoint {
                position: 15.0,
                value: 50.0,
            },
            BendPoint {
                position: 45.0,
                value: 50.0,
            },
            BendPoint {
                position: 60.0,
                value: 0.0,
            },
        ],
        960.0,
    );
    let pts = curve.points();
    assert_eq!(pts.len(), 4);
    assert_eq!(pts[1].value, pts[2].value, "the plateau survived");
    assert!(pts[2].t > pts[1].t, "and it has width");
}

#[test]
fn the_curve_lands_on_the_notes_pitch_dimension() {
    let mut n = note(1, 7, 0.0, 480.0);
    n.bend = vec![
        BendPoint {
            position: 0.0,
            value: 0.0,
        },
        BendPoint {
            position: 60.0,
            value: 50.0,
        },
    ];
    let out = to_document(&[n], standard());
    let curve = out.doc.notes[0].curve(Dimension::Pitch);
    assert_eq!(curve.points().len(), 2);
    assert_eq!(curve.points()[1].value, 1.0, "a semitone bend");
}

#[test]
fn an_unbent_note_carries_no_curve() {
    let out = to_document(&[note(0, 3, 0.0, 960.0)], standard());
    assert!(out.doc.notes[0].curve(Dimension::Pitch).points().is_empty());
    assert_eq!(out.bends, BendFidelity::None);
}

#[test]
fn a_full_point_list_is_reported_as_full_fidelity() {
    let mut n = note(0, 3, 0.0, 960.0);
    n.bend = vec![
        BendPoint {
            position: 0.0,
            value: 0.0,
        },
        BendPoint {
            position: 20.0,
            value: 50.0,
        },
        BendPoint {
            position: 40.0,
            value: 50.0,
        },
        BendPoint {
            position: 60.0,
            value: 0.0,
        },
    ];
    assert_eq!(to_document(&[n], standard()).bends, BendFidelity::Full);
}

#[test]
fn a_two_point_bend_is_reported_as_endpoints_only() {
    // What the dependency's GPIF path produces. Surfaced so a scenario
    // built on GPIF bends knows it is looking at a straight line.
    let mut n = note(0, 3, 0.0, 960.0);
    n.bend = vec![
        BendPoint {
            position: 0.0,
            value: 0.0,
        },
        BendPoint {
            position: 60.0,
            value: 100.0,
        },
    ];
    assert_eq!(
        to_document(&[n], standard()).bends,
        BendFidelity::EndpointsOnly
    );
}

// ── Format selection ─────────────────────────────────────────────────

#[test]
fn the_extension_picks_the_reader() {
    assert_eq!(Format::of_path("song.gp3"), Some(Format::Gp3));
    assert_eq!(Format::of_path("song.gp4"), Some(Format::Gp4));
    assert_eq!(Format::of_path("song.gp5"), Some(Format::Gp5));
    // Different containers for the same payload: BCFZ/BCFS versus ZIP.
    // Routing both to the ZIP reader made every GP6 file fail.
    assert_eq!(Format::of_path("song.gpx"), Some(Format::Gpx));
    assert_eq!(Format::of_path("song.gp"), Some(Format::Gpif));
    assert_eq!(Format::of_path("SONG.GP5"), Some(Format::Gp5), "case");
    assert_eq!(Format::of_path("song.mid"), None);
}

#[test]
fn only_the_binary_formats_keep_bend_shape() {
    assert!(Format::Gp5.keeps_bend_shape());
    assert!(!Format::Gpif.keeps_bend_shape(), "GPIF loses the middles");
    assert!(
        !Format::Gpx.keeps_bend_shape(),
        "and so does the GP6 container"
    );
}

// ── The document ─────────────────────────────────────────────────────

#[test]
fn notes_keep_their_place_on_the_timeline() {
    let out = to_document(
        &[note(0, 0, 0.0, 480.0), note(1, 2, 480.0, 480.0)],
        standard(),
    );
    assert_eq!(out.doc.notes[0].start, 0.0);
    assert_eq!(out.doc.notes[1].start, 480.0);
    assert_eq!(out.doc.notes[1].end, 960.0);
}

#[test]
fn the_document_spans_the_material() {
    let out = to_document(
        &[note(0, 0, 0.0, 480.0), note(0, 0, 1920.0, 960.0)],
        standard(),
    );
    assert_eq!(out.doc.end, 2880.0);
}

#[test]
fn an_empty_file_produces_an_empty_roll_rather_than_failing() {
    let out = to_document(&[], standard());
    assert!(out.doc.notes.is_empty());
    assert!(matches!(out.doc.row_space, RowSpace::Strings(_)));
}

// ── A real file, when one is available ───────────────────────────────

#[test]
fn any_gp_fixture_present_imports() {
    // The crate ships no corpus, so this runs only when someone drops a
    // file in. It is the end-to-end path: bytes -> parser -> roll.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    if !dir.exists() {
        return;
    }
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.to_string_lossy().to_string();
        if Format::of_path(&name).is_none() {
            continue;
        }
        let out = expression_editor_guitarpro::import_file(&name)
            .unwrap_or_else(|e| panic!("importing {name}: {e}"));
        assert!(
            matches!(out.doc.row_space, RowSpace::Strings(_)),
            "{name} did not produce a string roll"
        );
        seen += 1;
    }
    if seen > 0 {
        eprintln!("imported {seen} Guitar Pro fixture(s)");
    }
}

// ── Articulations ────────────────────────────────────────────────────

#[test]
fn an_articulation_reaches_the_note() {
    use expression_editor_core::rows::Articulation;
    let mut n = note(0, 3, 0.0, 960.0);
    n.articulation = Some(Articulation::PalmMute);
    let out = to_document(&[n], standard());
    assert_eq!(out.doc.notes[0].articulation, Some(Articulation::PalmMute));
}

#[test]
fn a_plainly_picked_note_carries_none_rather_than_sustain() {
    // "The file said nothing" has to stay distinguishable from "the
    // file said let-ring".
    let out = to_document(&[note(0, 3, 0.0, 960.0)], standard());
    assert_eq!(out.doc.notes[0].articulation, None);
}
