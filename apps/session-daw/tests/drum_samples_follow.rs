//! The drum samples stay on the kit's hits through every edit: a Sample
//! track's notes are where its piece's hits play now — after a quantize,
//! a hit placed by hand, an undo, a revert — never where they were when
//! the samples were written.
//!
//! On the golden session, the kick and snare marked where the drummer
//! played them, their Sample tracks given notes from those hits; then
//! each edit run as a model runs it (the window's part done here as the
//! window does it), and every piece's notes held up to its hits.
//!
//! One test, one process: the daw facade is bound to the first engine
//! opened.

#![cfg(feature = "signal")]

use std::path::Path;
use std::time::{Duration, Instant};

use daw::service::{ProjectContext, TrackRef, Tracks};
use daw_proto::Midi as _;
use dynamic_template::golden_session::{perform, shape};
use session_daw::actions::{list, take_window, window_pending};
use session_daw::arrange_edit::{Editor, Effect, SlipCut, kit_items, seconds_of};
use session_daw::keys::Action;
use session_daw::studio::StudioSession;

/// How near its hit a sample's note must be, seconds: the note is on a
/// MIDI tick (well under a millisecond).
const ON_ITS_HIT: f64 = 0.001;

/// Carry out what the Action List queued for the window — the slip — on
/// the session as the engine has it now.
fn be_the_window(path: &Path) {
    for action in take_window() {
        let Action::SlipDrums {
            moves,
            pre_roll_us,
            crossfade_us,
            stretch_over_us,
            attacks_us,
        } = action
        else {
            continue;
        };
        let session =
            StudioSession::read_current(Some(path), None).expect("the session reads back");
        let mut project = (*session.project.0).clone();
        let items = kit_items(&project, &[]);
        let seconds = |us: i64| num_traits::ToPrimitive::to_f64(&us).unwrap_or(0.0) / 1_000_000.0;
        let cut = SlipCut {
            pre_roll: seconds(pre_roll_us),
            crossfade: seconds(crossfade_us),
            stretch_over: (stretch_over_us > 0).then(|| seconds(stretch_over_us)),
            attacks: attacks_us.iter().map(|us| seconds(*us)).collect(),
        };
        let mut effects = Vec::new();
        Editor::slip_items(
            &mut project,
            &items,
            &seconds_of(&moves),
            ("Slip edit drums", cut),
            &mut effects,
        );
        for effect in effects {
            if let Effect::Send(edit) = effect {
                assert!(session_daw::engine::apply_now(&edit), "{edit:?} applies");
            }
        }
    }
}

/// Run `id` with `args` as a model does, being the window while it runs.
fn run_as_a_model(path: &Path, id: &'static str, args: String) -> String {
    let run = std::thread::spawn(move || list().run_json(id, args.as_bytes()));
    let began = Instant::now();
    while !run.is_finished() {
        assert!(began.elapsed() < Duration::from_secs(60), "{id} finishes");
        if window_pending() {
            be_the_window(path);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let said = run.join().expect("it ran").expect("it was not refused");
    String::from_utf8_lossy(&said).into_owned()
}

/// The Sample track beside a piece's trigger (`Sample`, `T1 Sample`).
fn sample_of(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    trigger: &str,
) -> Option<String> {
    let tracks = Tracks::all(daw, project.clone());
    let parent = tracks
        .iter()
        .find(|t| t.guid == trigger)
        .and_then(|t| t.parent_guid.clone());
    tracks
        .iter()
        .find(|t| t.parent_guid == parent && t.name.trim().to_ascii_lowercase().ends_with("sample"))
        .map(|t| t.guid.clone())
}

/// Every piece's sample notes held up to its hits where they play now:
/// one note a hit, each on it.
fn samples_on_the_hits(daw: &daw::standalone::Standalone, project: &ProjectContext, after: &str) {
    let played = session_daw::hits::playback(daw, project);
    for (piece, trigger) in session_daw::drum_reference::pieces(daw, project) {
        let mut hits: Vec<f64> = played
            .iter()
            .filter(|p| {
                p.piece == piece && !(p.mark.bleed && p.mark.origin == daw_proto::MarkOrigin::Auto)
            })
            .map(|p| p.time)
            .collect();
        if hits.is_empty() {
            continue;
        }
        hits.sort_by(f64::total_cmp);
        let sample = sample_of(daw, project, &trigger).expect("a Sample beside a played piece");
        let notes: Vec<f64> = session_daw::drum_reference::notes(daw, project, &sample)
            .into_iter()
            .map(|n| n.time)
            .collect();
        assert_eq!(
            notes.len(),
            hits.len(),
            "after {after}: {piece}'s samples, one a hit"
        );
        for (note, hit) in notes.iter().zip(&hits) {
            assert!(
                (note - hit).abs() < ON_ITS_HIT,
                "after {after}: {piece}'s sample at {note:.4}, its hit at {hit:.4}"
            );
        }
    }
}

#[test]
fn the_samples_stay_on_the_hits_through_every_edit() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../features/dynamic-template/fixtures/golden/template.rpp");
    dynamic_template::golden_session::write_media(path.parent().expect("the fixtures"))
        .expect("the golden media writes");
    session_daw::open::open_silent(&path).expect("the golden session opens");

    let song = shape::maximal();
    let bar = 240.0 / song.bpm;
    let section = song
        .sections
        .iter()
        .find(|s| s.start >= 8 && s.end > s.start.saturating_add(3))
        .expect("a section past the intro");
    let (from, to) = (f64::from(section.start) * bar, f64::from(section.end) * bar);
    for (on, piece, _) in perform::groove(&song) {
        let name = match piece {
            perform::Piece::Kick => "Kick",
            perform::Piece::Snare => "Snare",
            _ => continue,
        };
        if on < from - 2.0 * bar || on > to + 2.0 * bar {
            continue;
        }
        let played = perform::kit_played(on, piece, song.bpm);
        let args = format!(r#"{{"piece":"{name}","time":{played},"snap":false}}"#);
        list()
            .run_json("HITS_ADD", args.as_bytes())
            .expect("a hit placed");
    }

    // The samples written where the hits play.
    let (daw, project) = session_daw::actions::song().expect("the song is open");
    for (_, trigger) in session_daw::drum_reference::pieces(daw, &project) {
        let Some(sample) = sample_of(daw, &project, &trigger) else {
            continue;
        };
        daw.create_midi_item(project.clone(), TrackRef::Guid(sample), 0.0, 1.0)
            .expect("a MIDI item on the Sample track");
    }
    session_daw::drum_samples::resync(daw, &project).expect("the samples written");
    samples_on_the_hits(daw, &project, "writing them");

    let said = run_as_a_model(
        &path,
        "QUANTIZE",
        format!(r#"{{"from":{from},"to":{to},"grid":16,"target":"Grid","brief":true}}"#),
    );
    assert!(said.contains(r#""applied":true"#), "{said}");
    samples_on_the_hits(daw, &project, "a quantize");

    let mid = session_daw::hits::playback(daw, &project)
        .into_iter()
        .filter(|p| p.time > from && p.time < to)
        .nth(4)
        .expect("a hit in the section");
    run_as_a_model(
        &path,
        "HITS_PLACE",
        format!(r#"{{"id":"{}","by_ms":20,"snap":false}}"#, mid.id),
    );
    samples_on_the_hits(daw, &project, "a hit placed by hand");

    list().run_json("SESSION_UNDO", b"{}").expect("an undo");
    samples_on_the_hits(daw, &project, "an undo");

    run_as_a_model(
        &path,
        "DRUMS_REVERT",
        format!(r#"{{"from":{from},"to":{to}}}"#),
    );
    samples_on_the_hits(daw, &project, "a revert");
}
