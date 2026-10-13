//! The window's automation item edits against the engine: an item made
//! with its curve on one track, another of its pool on a second track
//! (the same curve, edited from either), its bounds moved, and deleted.

#![cfg(feature = "native")]

use std::path::{Path, PathBuf};

use daw_proto::automation::{EnvelopePoint, EnvelopeRef, EnvelopeType};
use daw_proto::primitives::PositionInSeconds;
use session_daw::engine::{Edit, apply_now};

fn template() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../features/dynamic-template/fixtures/golden/template.rpp")
}

/// Two tracks that are not folders.
fn two_tracks() -> (String, String) {
    session_daw::open::block_on_engine(async move {
        let daw = daw::rpc::Daw::try_get().expect("the facade");
        let project = daw.current_project().await.expect("a project");
        let tracks: Vec<String> = project
            .tracks()
            .all()
            .await
            .expect("tracks")
            .into_iter()
            .filter(|t| !t.is_folder)
            .map(|t| t.guid)
            .take(2)
            .collect();
        (tracks[0].clone(), tracks[1].clone())
    })
    .expect("the engine")
}

/// A track's volume envelope's items, each with its curve's values.
fn items(track: &str) -> Vec<(daw_proto::automation::AutomationItem, Vec<f64>)> {
    let track = track.to_owned();
    session_daw::open::block_on_engine(async move {
        let daw = daw::rpc::Daw::try_get().expect("the facade");
        let project = daw.current_project().await.expect("a project");
        let handle = project
            .tracks()
            .by_guid(&track)
            .await
            .expect("found")
            .expect("there");
        let volume = handle.envelopes().volume();
        let mut out = Vec::new();
        for item in volume.automation_items().await.unwrap_or_default() {
            let curve = volume
                .automation_item_points(item.index)
                .await
                .unwrap_or_default()
                .iter()
                .map(|p| p.value)
                .collect();
            out.push((item, curve));
        }
        out
    })
    .expect("the engine")
}

fn point(t: f64, value: f64) -> EnvelopePoint {
    EnvelopePoint {
        time: PositionInSeconds::from_seconds(t),
        value,
        ..EnvelopePoint::default()
    }
}

// r[verify automation.items]
#[test]
fn an_item_is_made_pooled_across_tracks_resized_and_deleted() {
    let _opened = session_daw::open::open_silent(&template()).expect("the template opens");
    let (a, b) = two_tracks();
    let volume = EnvelopeRef::Type(EnvelopeType::Volume);

    assert!(apply_now(&Edit::AddAutomationItem(
        a.clone(),
        volume.clone(),
        (2.0, 2.0, -1),
        vec![point(0.0, 0.5), point(2.0, 0.5)],
    )));
    let made = items(&a);
    let [(item, curve)] = made.as_slice() else {
        panic!("one item on the first track: {made:?}");
    };
    assert_eq!(curve, &[0.5, 0.5]);

    // Another of its pool, on the second track.
    assert!(apply_now(&Edit::AddAutomationItem(
        b.clone(),
        volume.clone(),
        (8.0, 2.0, item.pool_id),
        Vec::new(),
    )));
    let copied = items(&b);
    assert_eq!(
        copied.first().map(|(i, c)| (i.pool_id, c.clone())),
        Some((item.pool_id, vec![0.5, 0.5]))
    );

    // Edited on the second track: the first plays it too.
    assert!(apply_now(&Edit::SetItemCurve(
        b.clone(),
        volume.clone(),
        0,
        vec![point(0.0, 0.25), point(1.0, 1.0), point(2.0, 0.25)],
    )));
    assert_eq!(
        items(&a).first().map(|(_, c)| c.clone()),
        Some(vec![0.25, 1.0, 0.25])
    );

    // Its bounds, then gone.
    assert!(apply_now(&Edit::SetAutomationItemBounds(
        a.clone(),
        volume.clone(),
        0,
        (3.0, 1.5, 1.0)
    )));
    let moved = items(&a);
    let (moved, _) = moved.first().expect("still there");
    assert!(
        (moved.position.as_seconds() - 3.0).abs() < 1e-9
            && (moved.length.as_seconds() - 1.5).abs() < 1e-9
    );
    assert!((moved.start_offset.as_seconds() - 1.0).abs() < 1e-9);
    assert!(apply_now(&Edit::DeleteAutomationItem(a.clone(), volume, 0)));
    assert!(items(&a).is_empty());
    assert_eq!(items(&b).len(), 1, "the other instance stays");
}
