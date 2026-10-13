//! QUANTIZE end to end on the golden session: the kick and the snare
//! marked where the drummer played them (`golden_session::perform`, up to
//! 18 ms off the grid), one section quantized through the Action List as
//! a model runs it — the window's part, carrying out the slip it queues,
//! done here as the window does it — and the hits held up to the grid:
//! each in the section on its sixteenth, every one outside it where it
//! was, and the quantize's own check clean.
//!
//! One test, one process: the daw facade is bound to the first engine
//! opened.

use std::path::Path;
use std::time::{Duration, Instant};

use dynamic_template::golden_session::{perform, shape};
use session_daw::actions::{list, take_window, window_pending};
use session_daw::arrange_edit::{Editor, Effect, SlipCut, kit_items, seconds_of};
use session_daw::keys::Action;
use session_daw::studio::StudioSession;

/// How far from its line a quantized hit may be, seconds: one already
/// within the quantize's tolerance (3 ms) stays where it is; a moved one
/// lands to the sample.
const ON_THE_LINE: f64 = 0.0031;

/// Carry out what the Action List queued for the window — the slip —
/// on the session as the engine has it now.
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

/// r[verify flow.drums.editing.quantize]
/// r[verify quantize.reads-markers]
/// r[verify quantize.check]
/// r[verify quantize.stays-in-span]
#[test]
fn quantized_from_its_markers_the_kit_is_on_the_grid() {
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

    // Every kick and snare of the section and the bars either side, by
    // hand, where the drummer played it — no detection.
    let mut placed = 0_usize;
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
        placed = placed.saturating_add(1);
    }
    assert!(placed > 16, "the kit plays: {placed} hits");

    let (daw, project) = session_daw::actions::song().expect("the song is open");
    let before = session_daw::hits::playback(daw, &project);
    let step = bar / 16.0;
    let off_line = |t: f64| (t - (t / step).round() * step).abs();
    let inside = |t: f64| t >= from && t < to;
    assert!(
        before
            .iter()
            .filter(|p| inside(p.time))
            .any(|p| off_line(p.time) > 0.004),
        "the drummer is off the grid before"
    );

    let args = format!(r#"{{"from":{from},"to":{to},"grid":16,"target":"Grid","brief":true}}"#);
    let quantize = std::thread::spawn(move || list().run_json("QUANTIZE", args.as_bytes()));
    let began = Instant::now();
    while !quantize.is_finished() {
        assert!(
            began.elapsed() < Duration::from_secs(60),
            "the quantize finishes"
        );
        if window_pending() {
            be_the_window(&path);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let said = quantize
        .join()
        .expect("the quantize ran")
        .expect("the quantize was not refused");
    let said = String::from_utf8_lossy(&said);
    assert!(said.contains(r#""applied":true"#), "{said}");
    assert!(said.contains(r#""ok":true"#), "its check is clean: {said}");
    assert!(said.contains(r#""strayed":0"#), "{said}");

    let after = session_daw::hits::playback(daw, &project);
    let mut on_grid = 0_usize;
    for hit in &before {
        let now: Vec<f64> = after
            .iter()
            .filter(|a| a.id == hit.id)
            .map(|a| a.time)
            .collect();
        assert_eq!(now.len(), 1, "{} plays once: {now:?}", hit.id);
        let t = now.first().copied().unwrap_or_default();
        if inside(hit.time) {
            assert!(
                off_line(t) < ON_THE_LINE,
                "{} on its line: {:.4} → {t:.4}",
                hit.id,
                hit.time
            );
            on_grid = on_grid.saturating_add(1);
        } else if hit.time < from - 0.05 || hit.time > to + 0.05 {
            assert!(
                (t - hit.time).abs() < 0.001,
                "{} outside the section stays: {:.4} → {t:.4}",
                hit.id,
                hit.time
            );
        }
    }
    assert!(on_grid > 8, "hits quantized: {on_grid}");

    // r[verify hits.place]
    // A hand puts one hit 20 ms later: the kit moves under it, the hits
    // either side stay where they play.
    let inside_now: Vec<&session_daw::hits::Played> =
        after.iter().filter(|p| inside(p.time)).collect();
    let (Some(mid), Some(prev), Some(next)) = (
        inside_now.get(inside_now.len() / 2),
        inside_now.get(inside_now.len() / 2 - 1),
        inside_now.get(inside_now.len() / 2 + 1),
    ) else {
        panic!("hits in the section");
    };
    let (id, was) = (mid.id.clone(), mid.time);
    let args = format!(r#"{{"id":"{id}","by_ms":20,"snap":false}}"#);
    let place = std::thread::spawn(move || list().run_json("HITS_PLACE", args.as_bytes()));
    let began = Instant::now();
    while !place.is_finished() {
        assert!(
            began.elapsed() < Duration::from_secs(60),
            "the place finishes"
        );
        if window_pending() {
            be_the_window(&path);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let said = place.join().expect("it ran").expect("it was not refused");
    let said = String::from_utf8_lossy(&said);
    assert!(said.contains(r#""doubled":0"#), "{said}");
    let placed = session_daw::hits::playback(daw, &project);
    let at = |id: &str| placed.iter().find(|p| p.id == id).map(|p| p.time);
    let landed = at(&id).expect("it plays");
    assert!(
        (landed - (was + 0.020)).abs() < 0.001,
        "{was:.4} → {landed:.4}"
    );
    for neighbour in [prev, next] {
        let now = at(&neighbour.id).expect("it plays");
        assert!(
            (now - neighbour.time).abs() < 0.001,
            "{} beside it stays: {:.4} → {now:.4}",
            neighbour.id,
            neighbour.time
        );
    }

    // r[verify hits.revert]
    // The section put back as played: every hit where the drummer played
    // it, the markers all still there, a no-quantize zone over it.
    let args = format!(r#"{{"from":{from},"to":{to}}}"#);
    let revert = std::thread::spawn(move || list().run_json("DRUMS_REVERT", args.as_bytes()));
    let began = Instant::now();
    while !revert.is_finished() {
        assert!(
            began.elapsed() < Duration::from_secs(60),
            "the revert finishes"
        );
        if window_pending() {
            be_the_window(&path);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let said = revert.join().expect("it ran").expect("it was not refused");
    let said = String::from_utf8_lossy(&said);
    assert!(
        said.contains(r#""doubled":0"#) && said.contains(r#""lost":0"#),
        "{said}"
    );
    let back = session_daw::hits::playback(daw, &project);
    assert_eq!(back.len(), before.len(), "no marker lost");
    for hit in before
        .iter()
        .filter(|h| h.time >= from + 0.05 && h.time < to - 0.05)
    {
        let now = back
            .iter()
            .find(|p| p.id == hit.id)
            .map(|p| p.time)
            .expect("it plays");
        assert!(
            (now - hit.time).abs() < 0.0031,
            "{} back where it was played: {:.4} → {now:.4}",
            hit.id,
            hit.time
        );
    }
    let zones = session_daw::hits::zones(daw, &project);
    assert!(
        zones
            .iter()
            .any(|z| z.kind == session_daw::hits::ZoneKind::Exception
                && (z.start - from).abs() < 1e-6),
        "a no-quantize zone over it: {zones:?}"
    );
}
