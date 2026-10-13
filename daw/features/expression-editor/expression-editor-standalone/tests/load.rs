//! The loading half, headless.
//!
//! No window is opened here — that is the whole reason the library and
//! the examples are split. What is tested is the part that decides
//! *what* the window will show: argument classification, target
//! selection, and the round trip from a project on disk to a document
//! with notes in it.

use std::path::PathBuf;

use dawfile_reaper::RppSerialize;
use dawfile_reaper::builder::ReaperProjectBuilder;
use dawfile_reaper::types::item::SourceType;
use expression_editor_core::{Mode, Viewport};
use expression_editor_standalone::{Args, LoadError, Runner, Source, Target, scene_by_name};

fn viewport() -> Viewport {
    Viewport::new(1100.0, 400.0)
}

/// A two-track project: one MIDI item, one audio item pointing at a
/// file that does not exist.
///
/// The missing file is deliberate. An audio take whose source will not
/// decode is the ordinary case for a project moved between machines,
/// and the runner has to skip past it to the item it *can* open rather
/// than opening nothing.
fn fixture_rpp() -> String {
    ReaperProjectBuilder::new()
        .tempo(120.0)
        .track("Vox", |t| {
            t.item(0.0, 4.0, |i| i.take("vox.wav", SourceType::Wave))
        })
        .track("Keys", |t| {
            t.item(0.0, 4.0, |i| i.take("keys", SourceType::Midi))
        })
        .build()
        .to_rpp_string()
}

fn write_fixture(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("fts-expression-editor-standalone");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn scene_names_resolve_bare_slugged_and_labelled() {
    assert!(scene_by_name("guitar").is_some());
    assert!(scene_by_name("26-guitar").is_some());
    assert!(scene_by_name("Guitar riff").is_some());
    assert!(scene_by_name("GUITAR").is_some());
    assert!(scene_by_name("banjo").is_none());
}

#[test]
fn a_name_beats_a_path_and_extensions_dispatch() {
    // A job wins over a fixture that shares its name. `drums` names both
    // a workflow and a scene, and the workflow is what a person means —
    // the scene list is named after behaviours to demonstrate, not
    // things to open.
    assert!(matches!(Source::parse("drums"), Ok(Source::Workflow(_))));
    // A fixture with no job of that name still resolves to itself.
    assert!(matches!(Source::parse("zones"), Ok(Source::Scene(_))));
    assert!(matches!(Source::parse("song.rpp"), Ok(Source::Rpp(_))));
    assert!(matches!(Source::parse("part.MID"), Ok(Source::Midi(_))));
    assert!(matches!(
        Source::parse("take.wav"),
        Err(LoadError::BareAudio(_))
    ));
    assert!(matches!(
        Source::parse("notes.txt"),
        Err(LoadError::Unrecognised(_))
    ));
}

#[test]
fn no_arguments_opens_a_scene() {
    let args = Args::parse(Vec::<String>::new()).unwrap();
    assert!(matches!(args.source, Source::Scene(_)));
    assert_eq!(args.mode, None);
}

#[test]
fn flags_parse() {
    let args = Args::parse(
        [
            "guitar", "--mode", "guitar", "--track", "Vox", "--item", "2", "--size", "800x600",
        ]
        .map(String::from),
    )
    .unwrap();
    assert_eq!(args.mode, Some(Mode::Guitar));
    assert_eq!(args.target.track.as_deref(), Some("Vox"));
    assert_eq!(args.target.item, Some(2));
    assert_eq!((args.width, args.height), (800, 600));
    // The roll gets the window minus the chrome, never the whole
    // window — see `CHROME_HEIGHT`.
    assert!(args.viewport().h < 600.0);
}

#[test]
fn a_scene_loads_with_notes() {
    let runner = Runner::open(
        &Source::parse("guitar").unwrap(),
        &Target::default(),
        viewport(),
        None,
    )
    .unwrap();
    assert!(!runner.loaded.editor().doc.notes.is_empty());
    assert!(runner.daw.is_none(), "a scene has no project behind it");
}

#[test]
fn the_mode_override_wins_over_what_the_source_implies() {
    let runner = Runner::open(
        &Source::parse("phrase").unwrap(),
        &Target::default(),
        viewport(),
        Some(Mode::UnpitchedAudio),
    )
    .unwrap();
    assert_eq!(runner.loaded.editor().mode, Mode::UnpitchedAudio);
}

#[test]
fn a_project_opens_on_its_first_editable_item() {
    let path = write_fixture("first-editable.rpp", &fixture_rpp());
    let runner = Runner::open(&Source::Rpp(path), &Target::default(), viewport(), None).unwrap();
    // Vox comes first but its source will not decode, so the runner
    // must have fallen through to the MIDI item on Keys.
    assert!(runner.label.contains("Keys"), "got {:?}", runner.label);
    assert!(runner.daw.is_some(), "the backend outlives the load");
}

#[test]
fn a_named_track_narrows_the_search() {
    let path = write_fixture("named-track.rpp", &fixture_rpp());
    let err = Runner::open(
        &Source::Rpp(path.clone()),
        &Target {
            track: Some("Vox".into()),
            ..Target::default()
        },
        viewport(),
        None,
    )
    .expect_err("Vox holds only the undecodable audio item");
    assert!(matches!(err, LoadError::NoEditableItem { items: 1, .. }));

    let runner = Runner::open(
        &Source::Rpp(path),
        &Target {
            track: Some("Keys".into()),
            ..Target::default()
        },
        viewport(),
        None,
    )
    .unwrap();
    assert!(runner.label.contains("Keys"));
}

#[test]
fn an_unknown_track_says_so_rather_than_opening_something_else() {
    let path = write_fixture("unknown-track.rpp", &fixture_rpp());
    let err = Runner::open(
        &Source::Rpp(path),
        &Target {
            track: Some("Bagpipes".into()),
            ..Target::default()
        },
        viewport(),
        None,
    )
    .expect_err("there is no such track");
    assert!(matches!(err, LoadError::NoSuchTarget(_)));
}

/// An item with no take reads back from the MIDI service as a take
/// with no notes. If the runner accepted that, an empty slot early in
/// the project would win over the real take later in it and the user
/// would get a blank roll with nothing to explain it.
#[test]
fn an_item_with_no_take_does_not_win_over_a_real_one() {
    let rpp = ReaperProjectBuilder::new()
        .track("Empty", |t| t.item(0.0, 2.0, |i| i))
        .track("Keys", |t| {
            t.item(0.0, 4.0, |i| i.take("keys", SourceType::Midi))
        })
        .build()
        .to_rpp_string();
    let path = write_fixture("empty-item.rpp", &rpp);
    let runner = Runner::open(&Source::Rpp(path), &Target::default(), viewport(), None).unwrap();
    assert!(runner.label.contains("Keys"), "got {runner:?}");
}

/// A mono 16-bit PCM sine, written by hand.
///
/// Hand-written rather than pulled from a fixture directory because
/// the analyser only needs *a* pitch to find, and a checked-in wav
/// would be a binary in the tree that nothing else reads.
fn write_sine_wav(path: &std::path::Path, hz: f64, secs: f64, rate: u32) {
    let frames = (secs * rate as f64) as u32;
    let data_len = frames * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM header size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        let t = i as f64 / rate as f64;
        let v = (t * hz * std::f64::consts::TAU).sin() * 0.6;
        out.extend_from_slice(&((v * i16::MAX as f64) as i16).to_le_bytes());
    }
    std::fs::write(path, out).unwrap();
}

/// The audio branch, end to end: a project whose take points at a real
/// file, decoded through the standalone accessor and analysed into a
/// document. This is the path the Melodyne surface lives on, and the
/// only way to know the runner reaches it rather than falling through
/// to an empty MIDI read.
#[test]
fn an_audio_take_is_analysed_rather_than_read_as_midi() {
    let dir = std::env::temp_dir().join("fts-expression-editor-standalone");
    std::fs::create_dir_all(&dir).unwrap();
    let wav = dir.join("sine-a3.wav");
    write_sine_wav(&wav, 220.0, 1.5, 48_000);

    let rpp = ReaperProjectBuilder::new()
        .tempo(120.0)
        .track("Vox", |t| {
            t.item(0.0, 1.5, |i| {
                i.take(wav.to_string_lossy().into_owned(), SourceType::Wave)
            })
        })
        .build()
        .to_rpp_string();
    let path = write_fixture("audio-take.rpp", &rpp);

    let runner = Runner::open(&Source::Rpp(path), &Target::default(), viewport(), None).unwrap();
    assert_eq!(runner.loaded.kind(), "audio", "got {runner:?}");
    // Analysis sets the mode, so the surface matches the material
    // without the runner being told.
    assert_eq!(runner.loaded.editor().mode, Mode::PitchedAudio);
    assert!(
        !runner.loaded.editor().doc.notes.is_empty(),
        "a steady 220 Hz tone must analyse to at least one note"
    );
}

#[test]
fn a_missing_project_reports_the_path() {
    let err = Runner::open(
        &Source::Rpp(PathBuf::from("/nonexistent/nope.rpp")),
        &Target::default(),
        viewport(),
        None,
    )
    .expect_err("the file is not there");
    assert!(matches!(err, LoadError::Read(..)), "got {err:?}");
}

// ── Guitar Pro: scenario 4's material, openable in the app ───────────

#[test]
fn a_guitar_pro_extension_is_recognised() {
    for name in ["solo.gp", "solo.gpx", "solo.gp5", "SOLO.GP"] {
        assert!(
            matches!(Source::parse(name), Ok(Source::GuitarPro(_))),
            "{name} should load as a transcription"
        );
    }
}

#[test]
fn a_transcription_opens_as_a_guitar_roll() {
    // The fixture the importer's own suite uses, so this cannot drift
    // from what the parser is tested against.
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../expression-editor-guitarpro/tests/fixtures");
    let Some(file) = std::fs::read_dir(&fixture).ok().and_then(|d| {
        d.flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension().is_some_and(|x| {
                    let x = x.to_string_lossy().to_ascii_lowercase();
                    x.starts_with("gp")
                })
            })
            .min()
    }) else {
        eprintln!("SKIP: no .gp fixture in {}", fixture.display());
        return;
    };

    let source = Source::parse(&file.to_string_lossy()).expect("recognised");
    let runner = Runner::open(
        &source,
        &Target::default(),
        Viewport::new(1100.0, 700.0),
        None,
    )
    .expect("open the transcription");

    let editor = runner.loaded.editor();
    assert_eq!(editor.mode, Mode::Guitar, "a .gp opens in guitar mode");
    assert!(!editor.doc.notes.is_empty(), "no notes reached the roll");
    // The file's own tuning, not the mode preset's — a drop-D or a
    // seven-string must not be forced onto standard six.
    assert!(
        matches!(
            editor.doc.row_space,
            expression_editor_core::RowSpace::Strings(_)
        ),
        "a transcription must open on a string row space"
    );
    // Rows are pitches, so every note must land on the neck.
    let (lo, hi) = editor.doc.row_space.bounds();
    assert!(editor.doc.notes.iter().all(|n| n.row >= lo && n.row <= hi));
}

/// A format-1 MIDI file opens on the track that has the music (#real).
///
/// The regression: SMF format 1 — nearly every file anyone has — puts
/// tempo, time signature and the sequence name in track 0 and the notes
/// in track 1 onward. The runner opened track 0, so a real `.mid` showed
/// an empty roll saying "No notes" while the notes sat one track over.
#[test]
fn a_format_1_midi_file_opens_on_the_track_with_notes() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../crates/keyflow/keyflow/tests/midi/Bennie And The Jets - Elton John.mid");
    if !path.exists() {
        eprintln!("SKIP: no fixture at {}", path.display());
        return;
    }
    let source = Source::parse(path.to_str().unwrap()).expect("a .mid");
    let runner = Runner::open(
        &source,
        &Target::default(),
        Viewport::new(1200.0, 536.0),
        None,
    )
    .expect("opens");
    assert!(
        !runner.loaded.editor().doc.notes.is_empty(),
        "opened an empty roll: the conductor track again"
    );
}
