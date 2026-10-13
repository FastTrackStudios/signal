//! Original Composition's start (`docs/spec/session/original-composition.md`,
//! phases 1–3 and the structure of 4–6), through the Action List as a
//! model runs it: a new song from nothing, its count-in, start and guide
//! placed; the drums, bass and guitar added from Organize; the tempo set
//! with the structure keeping its bars and the click going to eighths;
//! the intro, verse and chorus inserted over a time selection and after
//! each other, the guide following; a track renamed; the first save
//! naming the song.
//!
//! Recording (M4) is not here: the takes, comps and rigs of the later
//! phases build on what this leaves.
//!
//! Bars are the song's, as every action reads them: bar 1 is SONGSTART's
//! (the spec's table counts from the count-in, two bars earlier).
//!
//! One test, one process: the daw facade is bound to the first engine
//! opened.

use daw::service::{ItemRef, Items, Markers, ProjectContext, TakeRef, TempoMap as _, TrackRef};
use daw_proto::{Midi as _, MidiTakeLocation};
use serde_json::{Value, json};
use session_daw::actions::{list, song, take_mode_request};

/// How close two times are to be the same, seconds.
const MS: f64 = 0.001;

/// One bar at 68 bpm in 4/4.
const BAR_68: f64 = 240.0 / 68.0;

/// The action's answer, as JSON.
fn run(id: &str, args: &Value) -> Value {
    let out = list()
        .run_json(id, args.to_string().as_bytes())
        .unwrap_or_else(|e| panic!("{id} {args}: {e}"));
    serde_json::from_slice(&out).unwrap_or_else(|e| panic!("{id} said: {e}"))
}

/// The action's refusal.
fn refused(id: &str, args: &Value) -> String {
    match list().run_json(id, args.to_string().as_bytes()) {
        Ok(out) => panic!(
            "{id} {args} should have been refused, and said {}",
            String::from_utf8_lossy(&out)
        ),
        Err(e) => e.to_string(),
    }
}

/// Every track, as SESSION_TRACKS lists them.
fn tracks() -> Vec<Value> {
    run("SESSION_TRACKS", &json!({}))
        .as_array()
        .cloned()
        .expect("a list of tracks")
}

fn paths(rows: &[Value]) -> Vec<String> {
    rows.iter()
        .filter_map(|r| r["path"].as_str().map(str::to_owned))
        .collect()
}

/// The track whose path is `path`.
fn row(rows: &[Value], path: &str) -> Value {
    rows.iter()
        .find(|r| r["path"] == path)
        .cloned()
        .unwrap_or_else(|| panic!("no {path} among {:?}", paths(rows)))
}

/// Where the marker `name` is, seconds.
fn marker(name: &str) -> f64 {
    let (daw, project) = song().expect("a song is open");
    Markers::all(daw, project)
        .into_iter()
        .find(|m| m.name == name)
        .and_then(|m| m.position.seconds())
        .unwrap_or_else(|| panic!("no {name} marker"))
}

/// Every note's pitch on the track at `path`.
fn pitches(path: &str) -> Vec<u8> {
    let (daw, project) = song().expect("a song is open");
    let guid = row(&tracks(), path)["guid"]
        .as_str()
        .expect("a guid")
        .to_owned();
    Items::get_items(daw, project.clone(), TrackRef::Guid(guid))
        .into_iter()
        .flat_map(|item| {
            daw.notes(MidiTakeLocation::new(
                project.clone(),
                ItemRef::Guid(item.guid),
                TakeRef::Active,
            ))
        })
        .map(|note| note.pitch)
        .collect()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < MS
}

// r[verify compose.new-project]
// r[verify compose.organize]
#[test]
fn a_song_from_nothing_gets_its_start_its_instruments_its_tempo_and_its_structure() {
    // ── 1.2 New Project ──────────────────────────────────────────────
    run("SESSION_NEW", &json!({}));
    assert_eq!(
        take_mode_request(),
        Some(session::modes::Mode::Organize),
        "a new song opens in Organize"
    );
    let (daw, project) = song().expect("the new song is open");
    let ProjectContext::Project(guid) = project.clone() else {
        panic!("the song is a project of its own");
    };
    assert_eq!(
        daw.read_project(&guid, |p| (p.info.name.clone(), p.info.path.clone())),
        Some(("Untitled".to_owned(), String::new())),
        "untitled, and no file behind it"
    );
    // Only the guide: its folder and the tracks it generates.
    let rows = tracks();
    for path in ["Guide", "Guide/Click", "Guide/Count", "Guide/Guide"] {
        row(&rows, path);
    }
    assert!(
        paths(&rows).iter().all(|p| p.starts_with("Guide")),
        "{:?}",
        paths(&rows)
    );
    // The count-in, the song's start two bars on, its end after it (at
    // the 120 bpm a song starts at).
    assert!(near(marker("COUNT-IN"), 0.0));
    assert!(near(marker("SONGSTART"), 4.0), "{}", marker("SONGSTART"));
    assert!(marker("SONGEND") > marker("SONGSTART") + 1.0);
    // The click runs; the count is in the two bars before the song.
    assert!(!pitches("Guide/Click").is_empty(), "the click is there");
    assert!(
        pitches("Guide/Count").iter().any(|p| (72..=79).contains(p)),
        "the count is there"
    );
    // 1.3: nowhere to save until it is named.
    let why = refused("SESSION_SAVE", &json!({}));
    assert!(why.contains("untitled"), "{why}");

    // ── 2.2–2.4 the instruments ──────────────────────────────────────
    run("TRACK_ADD_INSTRUMENT", &json!({ "instrument": "Drums" }));
    let rows = tracks();
    for path in [
        "Drums/MIDI",
        "Drums/Kick/Sum/In",
        "Drums/Kick/Sum/Trig",
        "Drums/Kick/Sum/Sample",
        "Drums/Snare/Sum/Top",
        "Drums/Toms",
        "Drums/Toms/Tom 1/T1",
        "Drums/Cymbals/OH",
        "Drums/Rooms",
        "Drums/Process",
    ] {
        row(&rows, path);
    }
    let drums = row(&rows, "Drums");
    assert_eq!(drums["folder"], true);
    assert_eq!(drums["open"], false, "the kit is folded to one row");
    assert_eq!(row(&rows, "Drums/MIDI")["record_input"], "midi");
    // The triggers are never heard.
    assert_eq!(row(&rows, "Drums/Kick/Sum/Trig")["muted"], true);
    // The kit reaches the mix by its bus.
    let drums_guid = drums["guid"].as_str().expect("a guid").to_owned();
    let bus = rows
        .iter()
        .find(|r| r["name"] == "DRUM BUS")
        .and_then(|r| r["guid"].as_str())
        .map(str::to_owned);
    assert!(bus.is_some(), "the song has a DRUM BUS: {:?}", paths(&rows));
    assert!(
        daw::service::Routing::sends(daw, project.clone(), TrackRef::Guid(drums_guid))
            .iter()
            .any(|send| send.dest_track_guid == bus),
        "Drums sends to DRUM BUS"
    );
    // A second kit is refused.
    let why = refused("TRACK_ADD_INSTRUMENT", &json!({ "instrument": "drums" }));
    assert!(why.contains("Drums"), "{why}");

    run("TRACK_ADD_INSTRUMENT", &json!({ "instrument": "Bass" }));
    run("TRACK_ADD_INSTRUMENT", &json!({ "instrument": "Guitar" }));
    let rows = tracks();
    for path in ["Bass", "Guitar"] {
        let track = row(&rows, path);
        assert_eq!(track["folder"], false, "{path} is one track");
        assert_eq!(track["items"], 0);
        assert_eq!(track["armed"], false);
    }
    // The top level: the guide, the instruments, then the buses.
    let top: Vec<String> = paths(&rows)
        .into_iter()
        .filter(|p| !p.contains('/'))
        .collect();
    let instruments: Vec<&str> = top
        .iter()
        .map(String::as_str)
        .filter(|p| !p.contains("BUS"))
        .collect();
    assert_eq!(instruments, ["Guide", "Drums", "Bass", "Guitar"], "{top:?}");
    let last_instrument = top.iter().position(|p| p == "Guitar");
    let first_bus = top.iter().position(|p| p.contains("BUS"));
    assert!(last_instrument < first_bus, "{top:?}");
    // Nothing has items but the guide.
    for track in &rows {
        let path = track["path"].as_str().unwrap_or_default();
        if !path.starts_with("Guide") {
            assert_eq!(track["items"], 0, "{path}");
        }
    }
    // One undo step each: the guitar goes, and comes back.
    run("SESSION_UNDO", &json!({}));
    assert!(!paths(&tracks()).iter().any(|p| p == "Guitar"));
    run("SESSION_REDO", &json!({}));
    row(&tracks(), "Guitar");

    // ── 3.6 the tempo ────────────────────────────────────────────────
    assert!(
        pitches("Guide/Click").iter().all(|p| *p != 62),
        "no eighths at 120"
    );
    run("TEMPO_SET", &json!({ "bpm": 68 }));
    assert!((daw.get_tempo_at(project.clone(), 0.0) - 68.0).abs() < 1e-9);
    assert!(
        near(marker("SONGSTART"), 2.0 * BAR_68),
        "SONGSTART kept its bar: {}",
        marker("SONGSTART")
    );
    assert!(
        pitches("Guide/Click").contains(&62),
        "below 75 bpm the click plays eighths"
    );
    // Negative control: at 80 the eighths go; back to 68.
    run("TEMPO_SET", &json!({ "bpm": 80 }));
    assert!(pitches("Guide/Click").iter().all(|p| *p != 62));
    run("TEMPO_SET", &json!({ "bpm": 68 }));
    assert!(near(marker("SONGSTART"), 2.0 * BAR_68));
    let why = refused("TEMPO_SET", &json!({ "bpm": 0 }));
    assert!(why.contains("bpm"), "{why}");

    // ── 4.3 the intro over a time selection ──────────────────────────
    let selected = run("TIME_SELECTION_SET", &json!({ "from": "1", "to": "5" }));
    assert!(selected.as_str().is_some(), "{selected}");
    let intro = run("SECTION_INSERT", &json!({ "kind": "Intro" }));
    assert!(
        intro["name"].as_str().is_some_and(|n| n.starts_with("IN")),
        "{intro}"
    );
    let at = |v: &Value, key: &str| v[key].as_f64().unwrap_or(f64::NAN);
    assert!(near(at(&intro, "start"), 2.0 * BAR_68), "{intro}");
    assert!(near(at(&intro, "end"), 6.0 * BAR_68), "{intro}");
    // The cursor at its end and the selection used up — in the engine
    // and the arrangement alike.
    let engine = session_daw::organize::engine_edit(daw, &project);
    assert!(near(engine.at, 6.0 * BAR_68), "{}", engine.at);
    assert!(engine.selection.is_none());
    let arranged = session_daw::cursor::current().expect("the arrangement was told");
    assert!(near(arranged.at, 6.0 * BAR_68));
    assert!(arranged.selection.is_none());
    assert!(
        pitches("Guide/Guide")
            .iter()
            .any(|p| (84..=102).contains(p)),
        "the guide has the intro's cue"
    );

    // ── 5.1 and 6.0: the verse and chorus after it ───────────────────
    let verse = run("SECTION_INSERT", &json!({ "kind": "verse" }));
    assert!(
        verse["name"].as_str().is_some_and(|n| n.starts_with("VS")),
        "{verse}"
    );
    assert!(near(at(&verse, "start"), 6.0 * BAR_68), "{verse}");
    assert!(near(at(&verse, "end"), 14.0 * BAR_68), "{verse}");
    assert!(
        marker("SONGEND") > 14.0 * BAR_68 - MS,
        "SONGEND moved out to the verse"
    );
    let chorus = run("SECTION_INSERT", &json!({ "kind": "Chorus" }));
    assert!(near(at(&chorus, "start"), 14.0 * BAR_68), "{chorus}");
    assert!(near(at(&chorus, "end"), 22.0 * BAR_68), "{chorus}");
    assert!(marker("SONGEND") > 22.0 * BAR_68 - MS);
    let sections = run("SESSION_SECTIONS", &json!({}));
    let names: Vec<&str> = sections
        .as_array()
        .expect("the sections")
        .iter()
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert_eq!(names.len(), 3, "{names:?}");
    for (name, abbreviation) in names.iter().zip(["IN", "VS", "CH"]) {
        assert!(name.starts_with(abbreviation), "{names:?}");
    }
    let why = refused("SECTION_INSERT", &json!({ "kind": "coda please" }));
    assert!(why.contains("section kind"), "{why}");

    // A marker at a bar: =END at the chorus's end (song bar 21).
    run("MARKER_INSERT", &json!({ "kind": "=END", "at": "21" }));
    assert!(near(marker("=END"), 22.0 * BAR_68), "{}", marker("=END"));
    let why = refused("MARKER_INSERT", &json!({ "kind": "middle" }));
    assert!(why.contains("marker kind"), "{why}");

    // Clearing the selection.
    run("TIME_SELECTION_SET", &json!({ "from": "3", "to": "4" }));
    assert!(session_daw::cursor::current().is_some_and(|e| e.selection.is_some()));
    run("TIME_SELECTION_SET", &json!({}));
    assert!(session_daw::cursor::current().is_some_and(|e| e.selection.is_none()));
    assert!(
        session_daw::organize::engine_edit(daw, &project)
            .selection
            .is_none()
    );

    // ── 11.1's rename ────────────────────────────────────────────────
    let guitar = row(&tracks(), "Guitar")["guid"].clone();
    let renamed = run(
        "TRACK_RENAME",
        &json!({ "track": "Guitar", "name": "Guitar Clean" }),
    );
    assert_eq!(renamed, guitar, "the track keeps its GUID");
    assert_eq!(row(&tracks(), "Guitar Clean")["guid"], guitar);
    let why = refused("TRACK_RENAME", &json!({ "track": "Bass", "name": "  " }));
    assert!(why.contains("empty"), "{why}");

    // ── 14.1's first save names the song ─────────────────────────────
    let dir = tempfile::tempdir().expect("a folder");
    let file = dir.path().join("Original Composition.session");
    run("SESSION_SAVE", &json!({ "to": file.to_string_lossy() }));
    let (name, path) = daw
        .read_project(&guid, |p| (p.info.name.clone(), p.info.path.clone()))
        .expect("the song");
    assert_eq!(name, "Original Composition");
    assert!(!path.is_empty(), "it has its file now");
    // Its own file from now on: a save needs no `to`.
    run("SESSION_SAVE", &json!({}));
}
