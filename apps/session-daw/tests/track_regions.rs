//! A track's own regions on the kit, end to end on the golden session: a
//! Grid region sends its stretch's hits to its own grid while the rest of
//! the section keeps the quantize's; a No quantize one keeps its hits as
//! played; edit zones are kept on the kit's folder; and an undo takes a
//! region back.
//!
//! One test, one process: the daw facade is bound to the first engine
//! opened.

#![cfg(feature = "signal")]

use std::path::Path;

use dynamic_template::golden_session::{perform, shape};
use session_daw::actions::list;

/// Run `id` with `args`, as a model does; its answer as JSON.
fn run(id: &str, args: &serde_json::Value) -> serde_json::Value {
    let said = list()
        .run_json(id, args.to_string().as_bytes())
        .unwrap_or_else(|e| panic!("{id} was refused: {e}"));
    serde_json::from_slice(&said).unwrap_or(serde_json::Value::Null)
}

/// Every move a dry-run quantize lists: `(hit, target)`, seconds.
fn moves(said: &serde_json::Value) -> Vec<(f64, f64)> {
    said.as_array()
        .into_iter()
        .flatten()
        .flat_map(|span| {
            span["result"]["biggest"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|m| {
            let hit = m["time"].as_f64()?;
            Some((hit, hit + m["off_ms"].as_f64()? / 1_000.0))
        })
        .collect()
}

/// Whether `t` is on a line `step` seconds apart, counted from 0.
fn on_line(t: f64, step: f64) -> bool {
    (t - (t / step).round() * step).abs() < 0.001
}

#[test]
fn the_kits_own_regions_say_how_a_stretch_is_edited() {
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
        .find(|s| s.start >= 8 && s.end > s.start.saturating_add(5))
        .expect("a section past the intro");
    let (from, to) = (f64::from(section.start) * bar, f64::from(section.end) * bar);
    // The kick and snare marked where the drummer played them.
    for (on, piece, _) in perform::groove(&song) {
        let name = match piece {
            perform::Piece::Kick => "Kick",
            perform::Piece::Snare => "Snare",
            _ => continue,
        };
        if on < from - bar || on > to + bar {
            continue;
        }
        let played = perform::kit_played(on, piece, song.bpm);
        run(
            "HITS_ADD",
            &serde_json::json!({ "piece": name, "time": played, "snap": false }),
        );
    }

    // A bar of half-note triplets in the middle of the section: a figure,
    // its hits on its grid whatever the MIDI says.
    let (grid_from, grid_to) = (from + bar, from + 2.0 * bar);
    let id = run(
        "TRACK_REGION_ADD",
        &serde_json::json!({
            "from": format!("{grid_from}s"),
            "to": format!("{grid_to}s"),
            "kind": "Grid",
            "grid": "1/2T",
        }),
    );
    let id = id.as_str().expect("its id").to_owned();
    let marks = run("TRACK_MARKS", &serde_json::json!({}));
    let region = &marks[0]["regions"][0];
    assert_eq!(region["grid"], "1/2T", "{marks}");
    assert_eq!(region["kind"], "Grid", "{marks}");

    let quantize = |from: f64, to: f64| {
        run(
            "QUANTIZE",
            &serde_json::json!({
                "from": from, "to": to, "grid": 16, "target": "Grid", "dry_run": true,
            }),
        )
    };
    let said = quantize(from, to);
    let sent = moves(&said);
    let (half_triplet, sixteenth) = (bar / 3.0, bar / 16.0);
    let inside: Vec<&(f64, f64)> = sent
        .iter()
        .filter(|(h, _)| *h >= grid_from && *h < grid_to)
        .collect();
    assert!(!inside.is_empty(), "hits in the region were moved: {said}");
    for (h, t) in &inside {
        assert!(
            on_line(*t, half_triplet),
            "{h:.4} sent to {t:.4}, not a half-note triplet"
        );
    }
    for (h, t) in sent
        .iter()
        .filter(|(h, _)| *h < grid_from - 0.05 || *h >= grid_to + 0.05)
    {
        assert!(
            on_line(*t, sixteenth),
            "{h:.4} sent to {t:.4}, not a sixteenth"
        );
    }

    // As played: nothing in it moves.
    run(
        "TRACK_REGION_EDIT",
        &serde_json::json!({ "id": id, "kind": "No quantize" }),
    );
    let said = quantize(from, to);
    assert!(
        moves(&said)
            .iter()
            .all(|(h, _)| *h < grid_from - 0.05 || *h >= grid_to),
        "a No quantize region keeps its hits as played: {said}"
    );

    // An edit zone is the kit folder's own now.
    let zones = run("ZONES_LIST", &serde_json::json!({}));
    assert!(
        zones.to_string().contains("track:"),
        "the No quantize region is an edit zone: {zones}"
    );

    // And an undo takes a region back.
    run("TRACK_MARKS_REMOVE", &serde_json::json!({ "ids": [id] }));
    let marks = run("TRACK_MARKS", &serde_json::json!({}));
    assert_eq!(marks, serde_json::json!([]), "removed: {marks}");
    std::thread::sleep(std::time::Duration::from_millis(500));
    let undid = list().run_json("SESSION_UNDO", b"{}").expect("an undo");
    let undid = String::from_utf8_lossy(&undid).into_owned();
    let marks = run("TRACK_MARKS", &serde_json::json!({}));
    assert_eq!(
        marks[0]["regions"].as_array().map(Vec::len),
        Some(1),
        "the undo ({undid}) brings it back: {marks}"
    );
}
