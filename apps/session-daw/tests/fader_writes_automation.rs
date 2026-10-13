//! A fader moved while the song plays writes automation, the way the
//! window sends it: a touch, the volumes the drag makes, a release. In
//! Latch the moves are written as the volume envelope's points; in
//! Trim/Read the same moves only set the fader.

#![cfg(feature = "native")]

use session_daw::engine::{Edit, Touched, apply_now};
use std::path::{Path, PathBuf};

fn template() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../features/dynamic-template/fixtures/golden/template.rpp")
}

/// The first track that is not a folder, and its volume envelope's points
/// as (seconds, value).
fn volume_points(track: Option<&str>) -> (String, Vec<(f64, f64)>) {
    let track = track.map(str::to_owned);
    session_daw::open::block_on_engine(async move {
        let daw = daw::rpc::Daw::try_get().expect("the facade");
        let project = daw.current_project().await.expect("a project");
        let guid = match track {
            Some(guid) => guid,
            None => {
                project
                    .tracks()
                    .all()
                    .await
                    .expect("tracks")
                    .into_iter()
                    .find(|t| !t.is_folder)
                    .expect("a track")
                    .guid
            }
        };
        let handle = project
            .tracks()
            .by_guid(&guid)
            .await
            .expect("found")
            .expect("there");
        let points = handle
            .envelopes()
            .volume()
            .points()
            .await
            .unwrap_or_default()
            .iter()
            .map(|p| (p.time.as_seconds(), p.value))
            .collect();
        (guid, points)
    })
    .expect("the engine")
}

fn transport(play: bool) {
    session_daw::open::block_on_engine(async move {
        let daw = daw::rpc::Daw::try_get().expect("the facade");
        let project = daw.current_project().await.expect("a project");
        if play {
            project.transport().play().await.expect("plays");
        } else {
            project.transport().stop().await.expect("stops");
        }
    })
    .expect("the engine");
}

/// The song played on to `seconds` — a test engine has no audio device to
/// move its play position, so the test moves it.
fn played_to(seconds: f64) {
    session_daw::open::block_on_engine(async move {
        let daw = daw::rpc::Daw::try_get().expect("the facade");
        let project = daw.current_project().await.expect("a project");
        project
            .transport()
            .set_position(seconds)
            .await
            .expect("moved");
    })
    .expect("the engine");
}

/// A drag: touched, a volume at 1 s and another at 2 s, let go.
fn drag(track: &str) {
    assert!(apply_now(&Edit::Touch(
        track.to_owned(),
        Touched::Volume,
        true
    )));
    played_to(1.0);
    assert!(apply_now(&Edit::SetVolume(track.to_owned(), 0.5)));
    played_to(2.0);
    assert!(apply_now(&Edit::SetVolume(track.to_owned(), 0.25)));
    assert!(apply_now(&Edit::Touch(
        track.to_owned(),
        Touched::Volume,
        false
    )));
}

// r[verify automation.write]
#[test]
fn a_fader_moved_in_latch_writes_its_moves() {
    let _opened = session_daw::open::open_silent(&template()).expect("the template opens");
    let (track, before) = volume_points(None);
    assert!(
        before.is_empty(),
        "no volume automation to start: {before:?}"
    );

    // Trim/Read (the default): the fader moves, nothing is written.
    transport(true);
    drag(&track);
    transport(false);
    let (_, trimmed) = volume_points(Some(&track));
    assert!(trimmed.is_empty(), "Trim/Read writes nothing: {trimmed:?}");

    // Latch: the same drag is written while the song plays.
    assert!(apply_now(&Edit::SetAutomationMode(
        track.clone(),
        daw_proto::primitives::AutomationMode::Latch,
    )));
    transport(true);
    drag(&track);
    transport(false);
    let (_, written) = volume_points(Some(&track));
    let values: Vec<f64> = written.iter().map(|(_, v)| *v).collect();
    assert!(
        values.iter().any(|v| (v - 0.5).abs() < 1e-6)
            && values.iter().any(|v| (v - 0.25).abs() < 1e-6),
        "both moves written: {written:?}"
    );
    let times: Vec<f64> = written.iter().map(|(t, _)| *t).collect();
    assert!(
        times.windows(2).all(|w| w[0] <= w[1]),
        "in time order: {written:?}"
    );
}
