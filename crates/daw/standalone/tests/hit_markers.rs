//! Take markers as drum hits: kept per recording, with stable ids and a
//! hit's properties, replaced wholesale by a detection run.

use daw_proto::project::ProjectContext;
use daw_proto::{
    Duration, HitMark, ItemRef, Items, MarkOrigin, PositionInSeconds, ProjectInfo, TakeMarker,
    TakeMarkerCreate, TakeMarkerUpdate, TakeRef, Takes, TrackRef, Tracks,
};
use daw_standalone::sync::Standalone;

const SNARE_TOP: &str = "/media/Snare Top-take1.wav";

/// A project with two items on one track, both cut from the same
/// recording — what a slip edit leaves behind.
fn project() -> (Standalone, String, String) {
    let daw = Standalone::new();
    daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Current;
    let track = Tracks::add(&daw, ctx.clone(), "Top", None).unwrap();
    let mut items = Vec::new();
    for at in [0.0, 4.0] {
        let item = Items::add_item(
            &daw,
            ctx.clone(),
            TrackRef::Guid(track.clone()),
            PositionInSeconds::from_seconds(at),
            Duration::from_seconds(4.0),
        )
        .unwrap();
        // An audio item is a take plus a source.
        let take = Takes::add_take(&daw, ctx.clone(), ItemRef::Guid(item.clone())).unwrap();
        Takes::set_source_file(
            &daw,
            ctx.clone(),
            ItemRef::Guid(item.clone()),
            TakeRef::Guid(take),
            SNARE_TOP.into(),
        )
        .unwrap();
        items.push(item);
    }
    let second = items.pop().unwrap();
    let first = items.pop().unwrap();
    (daw, first, second)
}

fn markers(daw: &Standalone, item: &str) -> Vec<TakeMarker> {
    daw.get_take_markers(
        ProjectContext::Current,
        ItemRef::Guid(item.to_string()),
        TakeRef::Active,
    )
}

fn hit(piece: &str, confidence: f64) -> HitMark {
    HitMark {
        piece: piece.into(),
        confidence,
        from_trigger: true,
        from_mic: true,
        peak_db: -12.0,
        ..HitMark::default()
    }
}

#[test]
fn two_items_cut_from_one_recording_share_its_markers() {
    let (daw, first, second) = project();
    daw.add_take_marker(
        ProjectContext::Current,
        ItemRef::Guid(first.clone()),
        TakeRef::Active,
        TakeMarkerCreate {
            source_position_seconds: 5.25,
            hit: Some(hit("Snare", 0.95)),
            ..TakeMarkerCreate::default()
        },
    )
    .unwrap();
    let seen = markers(&daw, &second);
    assert_eq!(
        seen.len(),
        1,
        "the second item reads the recording's markers"
    );
    assert_eq!(
        seen[0].hit.as_ref().map(|h| h.piece.as_str()),
        Some("Snare")
    );
}

#[test]
fn an_id_survives_inserts_before_it() {
    let (daw, item, _) = project();
    let add = |at: f64| {
        daw.add_take_marker(
            ProjectContext::Current,
            ItemRef::Guid(item.clone()),
            TakeRef::Active,
            TakeMarkerCreate {
                source_position_seconds: at,
                ..TakeMarkerCreate::default()
            },
        )
    };
    add(2.0);
    let id = markers(&daw, &item)[0].id.clone();
    assert!(!id.is_empty(), "a marker is given an id");
    add(1.0);
    let now = markers(&daw, &item);
    assert_eq!(now[1].id, id, "its index moved, its id did not");
    assert_eq!(now[1].index, 1);
}

#[test]
fn a_hit_can_be_approved_and_turned_back_into_a_plain_marker() {
    let (daw, item, _) = project();
    daw.add_take_marker(
        ProjectContext::Current,
        ItemRef::Guid(item.clone()),
        TakeRef::Active,
        TakeMarkerCreate {
            source_position_seconds: 1.0,
            hit: Some(hit("Kick", 0.4)),
            ..TakeMarkerCreate::default()
        },
    )
    .unwrap();
    let approved = HitMark {
        origin: MarkOrigin::Approved,
        ..hit("Kick", 0.4)
    };
    daw.set_take_marker(
        ProjectContext::Current,
        ItemRef::Guid(item.clone()),
        TakeRef::Active,
        TakeMarkerUpdate {
            index: 0,
            hit: Some(Some(approved)),
            ..TakeMarkerUpdate::default()
        },
    )
    .unwrap();
    assert_eq!(
        markers(&daw, &item)[0].hit.as_ref().map(|h| h.origin),
        Some(MarkOrigin::Approved)
    );
    daw.set_take_marker(
        ProjectContext::Current,
        ItemRef::Guid(item.clone()),
        TakeRef::Active,
        TakeMarkerUpdate {
            index: 0,
            hit: Some(None),
            ..TakeMarkerUpdate::default()
        },
    )
    .unwrap();
    assert_eq!(markers(&daw, &item)[0].hit, None);
}

#[test]
fn a_run_replaces_them_all_at_once_keeping_given_ids() {
    let (daw, item, _) = project();
    let run: Vec<TakeMarker> = [(3.0, "b"), (1.0, "a"), (2.0, "")]
        .into_iter()
        .map(|(at, id)| TakeMarker {
            id: id.into(),
            source_position_seconds: at,
            hit: Some(hit("Snare", 0.9)),
            ..TakeMarker::default()
        })
        .collect();
    daw.set_take_markers(
        ProjectContext::Current,
        ItemRef::Guid(item.clone()),
        TakeRef::Active,
        run,
    )
    .unwrap();
    let got = markers(&daw, &item);
    let ids: Vec<&str> = got.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids[0], "a");
    assert!(!ids[1].is_empty(), "an empty id is given one");
    assert_eq!(ids[2], "b");
    assert_eq!(
        got.iter().map(|m| m.index).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    daw.set_take_markers(
        ProjectContext::Current,
        ItemRef::Guid(item.clone()),
        TakeRef::Active,
        Vec::new(),
    )
    .unwrap();
    assert!(markers(&daw, &item).is_empty());
}
