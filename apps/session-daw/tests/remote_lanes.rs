//! A window whose engine is elsewhere (a browser, a remote desktop) reads
//! the automation lanes through the daw facade: the same lanes, points and
//! automation items as a read from the engine in this process.

#![cfg(feature = "native")]

use std::path::{Path, PathBuf};

use daw_proto::automation::{EnvelopePoint, EnvelopeRef, EnvelopeType};
use daw_proto::primitives::PositionInSeconds;
use session_daw::engine::{Edit, apply_now};

fn template() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../features/dynamic-template/fixtures/golden/template.rpp")
}

fn point(t: f64, value: f64) -> EnvelopePoint {
    EnvelopePoint {
        time: PositionInSeconds::from_seconds(t),
        value,
        ..EnvelopePoint::default()
    }
}

#[test]
fn the_facade_reads_the_lanes_the_engine_has() {
    let _opened = session_daw::open::open_silent(&template()).expect("the template opens");
    let tracks: Vec<daw_proto::Track> = session_daw::open::block_on_engine(async {
        let daw = daw::rpc::Daw::try_get().expect("the facade");
        let project = daw.current_project().await.expect("a project");
        project.tracks().all().await.expect("tracks")
    })
    .expect("the engine");
    let track = tracks
        .iter()
        .find(|t| !t.is_folder)
        .expect("a track")
        .guid
        .clone();
    let volume = EnvelopeRef::Type(EnvelopeType::Volume);
    assert!(apply_now(&Edit::SetEnvelope(
        track.clone(),
        volume.clone(),
        vec![point(0.0, 1.0), point(4.0, 0.5)],
    )));
    assert!(apply_now(&Edit::AddAutomationItem(
        track.clone(),
        volume.clone(),
        (1.0, 2.0, -1),
        vec![point(0.0, 0.25), point(2.0, 0.25)],
    )));
    assert!(apply_now(&Edit::ShowEnvelope(track.clone(), volume, true)));

    let (remote, _) = session_daw::open::block_on_engine({
        let tracks = tracks.clone();
        async move { session_daw::automation::read_remote(&tracks).await }
    })
    .expect("the engine");
    let daw = session_daw::open::local_engine().expect("the engine in this process");
    let song = session_daw::open::current_song().expect("a song");
    let local =
        session_daw::automation::read(daw, &daw_proto::ProjectContext::Project(song), &tracks);
    let summary =
        |lanes: &std::collections::HashMap<String, Vec<session_daw::automation::Lane>>| {
            lanes.get(&track).map(|lanes| {
                lanes
                    .iter()
                    .map(|l| {
                        (
                            l.name.clone(),
                            l.points.len(),
                            l.items.len(),
                            l.value_at(1.5),
                            l.value_at(3.5),
                        )
                    })
                    .collect::<Vec<_>>()
            })
        };
    let (r, l) = (summary(&remote), summary(&local));
    assert!(r.is_some(), "a lane through the facade: {remote:?}");
    assert_eq!(r, l, "the same lanes either way");
    let lanes = r.unwrap_or_default();
    let (_, points, items, in_item, after) = lanes.first().cloned().unwrap_or_default();
    assert_eq!((points, items), (2, 1));
    assert!((in_item - 0.25).abs() < 1e-9, "the item plays: {in_item}");
    assert!(
        (after - 0.5625).abs() < 1e-9,
        "the envelope after it: {after}"
    );
}
