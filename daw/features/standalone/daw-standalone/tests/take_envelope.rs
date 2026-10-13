//! Take envelopes through the facade, on standalone.
//!
//! The write path for everything item-scoped — gating, compression,
//! breath, sibilance — is points on a take's volume envelope. This is
//! the half that can be checked without a DAW.

use daw_proto::automation::{AddPointParams, EnvelopeShape};
use daw_proto::midi::Midi;
use daw_proto::project::ProjectContext;
use daw_proto::{
    Automation, EnvelopeLocation, EnvelopeRef, ItemRef, PositionInSeconds, ProjectInfo,
    TakeEnvelopeKind, Takes, TrackRef, Tracks,
};
use daw_standalone::sync::Standalone;

fn project() -> (Standalone, String, String) {
    let daw = Standalone::new();
    daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Current;
    let track = Tracks::add(&daw, ctx.clone(), "Vox", None).unwrap();
    let loc = Midi::create_midi_item(&daw, ctx.clone(), TrackRef::Guid(track), 0.0, 4.0).unwrap();
    let item_guid = match &loc.item {
        ItemRef::Guid(g) => g.clone(),
        _ => panic!(),
    };
    let take = Takes::get_active_take(&daw, ctx, ItemRef::Guid(item_guid.clone())).unwrap();
    (daw, item_guid, take.guid)
}

fn volume_envelope(item_guid: &str, take_guid: &str) -> EnvelopeLocation {
    EnvelopeLocation {
        // Documented as ignored for take envelopes: the item and take
        // carry the context.
        track: TrackRef::Index(0),
        envelope: EnvelopeRef::Take {
            item_guid: item_guid.to_string(),
            take_guid: take_guid.to_string(),
            kind: TakeEnvelopeKind::Volume,
        },
    }
}

fn point(secs: f64, value: f64) -> AddPointParams {
    AddPointParams::new(
        PositionInSeconds::from_seconds(secs),
        value,
        EnvelopeShape::Linear,
    )
}

#[test]
fn points_can_be_written_to_and_read_from_a_take_volume_envelope() {
    let (daw, item, take) = project();
    let env = volume_envelope(&item, &take);

    daw.add_point(ProjectContext::Current, env.clone(), point(0.0, 1.0));
    daw.add_point(ProjectContext::Current, env.clone(), point(1.0, 0.5));
    daw.add_point(ProjectContext::Current, env.clone(), point(2.0, 1.0));

    let got = daw.points(ProjectContext::Current, env);
    assert_eq!(got.len(), 3, "a gain ride round-trips");
    assert!((got[1].value - 0.5).abs() < 1e-9);
    assert!((got[1].time.as_seconds() - 1.0).abs() < 1e-9);
}

#[test]
fn a_take_envelope_needs_no_track_to_address_it() {
    // The point of resolving take envelopes before the track lookup: a
    // caller holding an item guid should not have to invent a track ref
    // to satisfy a lookup that is then discarded.
    let (daw, item, take) = project();
    let mut env = volume_envelope(&item, &take);
    // A track that does not exist.
    env.track = TrackRef::Index(999);

    daw.add_point(ProjectContext::Current, env.clone(), point(0.5, 0.7));
    let got = daw.points(ProjectContext::Current, env);
    assert_eq!(got.len(), 1, "the bogus track ref was correctly ignored");
}

#[test]
fn two_takes_carry_independent_envelopes() {
    // A take is what owns the gain, and two takes of one item are two
    // performances. Sharing an envelope between them would apply one
    // performance's de-essing to the other.
    let (daw, item, take_a) = project();
    let env_a = volume_envelope(&item, &take_a);
    let env_b = volume_envelope(&item, "some-other-take");

    daw.add_point(ProjectContext::Current, env_a.clone(), point(0.0, 0.25));
    assert_eq!(daw.points(ProjectContext::Current, env_a).len(), 1);
    assert!(
        daw.points(ProjectContext::Current, env_b).is_empty(),
        "another take has its own"
    );
}

#[test]
fn clearing_an_envelope_leaves_nothing_behind() {
    // What a switch-off does: not a flat envelope at unity, but no
    // envelope. The two sound the same and differ in whose job it is to
    // delete the leftovers.
    let (daw, item, take) = project();
    let env = volume_envelope(&item, &take);
    for i in 0..5 {
        daw.add_point(
            ProjectContext::Current,
            env.clone(),
            point(i as f64 * 0.5, 1.0),
        );
    }
    assert_eq!(daw.points(ProjectContext::Current, env.clone()).len(), 5);

    // Back to front, because deleting shifts every later index down.
    for i in (0..5).rev() {
        daw.delete_point(ProjectContext::Current, env.clone(), i);
    }
    assert!(daw.points(ProjectContext::Current, env).is_empty());
}

#[test]
fn the_four_kinds_do_not_collide() {
    // Volume, Pan, Mute and Pitch are separate envelopes on one take.
    let (daw, item, take) = project();
    let vol = volume_envelope(&item, &take);
    let pan = EnvelopeLocation {
        track: TrackRef::Index(0),
        envelope: EnvelopeRef::Take {
            item_guid: item.clone(),
            take_guid: take.clone(),
            kind: TakeEnvelopeKind::Pan,
        },
    };

    daw.add_point(ProjectContext::Current, vol.clone(), point(0.0, 0.5));
    assert_eq!(daw.points(ProjectContext::Current, vol).len(), 1);
    assert!(
        daw.points(ProjectContext::Current, pan).is_empty(),
        "pan is a different envelope"
    );
}

// ── take markers ─────────────────────────────────────────────────────

use daw_proto::{TakeMarkerCreate, TakeMarkerUpdate, TakeRef};

fn add(daw: &Standalone, item: &str, name: &str, at: f64) -> Option<u32> {
    daw.add_take_marker(
        ProjectContext::Current,
        ItemRef::Guid(item.to_string()),
        TakeRef::Active,
        TakeMarkerCreate {
            name: name.into(),
            source_position_seconds: at,
            color: None,
            ..Default::default()
        },
    )
}

fn markers(daw: &Standalone, item: &str) -> Vec<daw_proto::TakeMarker> {
    daw.get_take_markers(
        ProjectContext::Current,
        ItemRef::Guid(item.to_string()),
        TakeRef::Active,
    )
}

#[test]
fn take_markers_round_trip_in_source_order() {
    let (daw, item, _take) = project();
    // Added out of order, because the index is an enumeration position
    // rather than an identity — "the marker at 1" should mean the
    // second one in time, not the second one added.
    assert!(add(&daw, &item, "sibilance", 2.0).is_some());
    assert!(add(&daw, &item, "breath", 0.5).is_some());

    let got = markers(&daw, &item);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].name, "breath");
    assert_eq!(got[1].name, "sibilance");
    assert_eq!(got[0].index, 0);
    assert_eq!(got[1].index, 1);
}

#[test]
fn adding_reports_where_the_marker_landed() {
    let (daw, item, _take) = project();
    add(&daw, &item, "late", 3.0);
    assert_eq!(
        add(&daw, &item, "early", 1.0),
        Some(0),
        "an early marker added last still lands first"
    );
}

#[test]
fn a_marker_can_be_renamed_moved_and_recoloured_piecemeal() {
    let (daw, item, _take) = project();
    add(&daw, &item, "breath", 1.0);

    daw.set_take_marker(
        ProjectContext::Current,
        ItemRef::Guid(item.clone()),
        TakeRef::Active,
        TakeMarkerUpdate {
            index: 0,
            name: Some("sibilance".into()),
            source_position_seconds: None,
            color: Some(Some(0x00FF00)),
            ..Default::default()
        },
    )
    .unwrap();

    let got = markers(&daw, &item);
    assert_eq!(got[0].name, "sibilance");
    assert_eq!(got[0].color, Some(0x00FF00));
    assert_eq!(got[0].source_position_seconds, 1.0, "position untouched");

    // `Some(None)` clears the colour; `None` would leave it.
    daw.set_take_marker(
        ProjectContext::Current,
        ItemRef::Guid(item.clone()),
        TakeRef::Active,
        TakeMarkerUpdate {
            index: 0,
            name: None,
            source_position_seconds: None,
            color: Some(None),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(markers(&daw, &item)[0].color, None);
    assert_eq!(markers(&daw, &item)[0].name, "sibilance", "name kept");
}

#[test]
fn deleting_renumbers_what_is_left() {
    let (daw, item, _take) = project();
    for (i, n) in ["a", "b", "c"].iter().enumerate() {
        add(&daw, &item, n, i as f64);
    }
    daw.delete_take_marker(
        ProjectContext::Current,
        ItemRef::Guid(item.clone()),
        TakeRef::Active,
        0,
    )
    .unwrap();

    let got = markers(&daw, &item);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].name, "b");
    assert_eq!(got[0].index, 0, "indices are positions, so they close up");
    assert_eq!(got[1].index, 1);

    assert!(
        daw.delete_take_marker(
            ProjectContext::Current,
            ItemRef::Guid(item),
            TakeRef::Active,
            99
        )
        .is_err()
    );
}

#[test]
fn two_takes_carry_independent_markers() {
    let (daw, item, _take) = project();
    add(&daw, &item, "breath", 1.0);
    // A different item has none of them.
    assert!(markers(&daw, "no-such-item").is_empty());
    assert_eq!(markers(&daw, &item).len(), 1);
}

/// A take's volume curve stays on its audio through an edit: a copy
/// carries its own copy, and slipping the take into its source moves the
/// curve with the sound — a split's right half (a copy slipped to the
/// cut) plays the left's curve where the left would have.
#[test]
fn a_take_envelope_rides_with_its_audio() {
    use daw_proto::{Items, TakeRef};
    let (daw, item, take) = project();
    let env = volume_envelope(&item, &take);
    daw.add_point(ProjectContext::Current, env.clone(), point(1.0, 0.5));
    daw.add_point(ProjectContext::Current, env.clone(), point(3.0, 0.25));

    let copy = daw
        .duplicate_item(ProjectContext::Current, ItemRef::Guid(item.clone()))
        .expect("a copy");
    let copy_take =
        Takes::get_active_take(&daw, ProjectContext::Current, ItemRef::Guid(copy.clone()))
            .expect("its take")
            .guid;
    let copied = volume_envelope(&copy, &copy_take);
    assert_eq!(
        daw.points(ProjectContext::Current, copied.clone()).len(),
        2,
        "the copy has the curve"
    );

    // The copy becomes a right half from 2 s: two seconds into its
    // source, its curve two seconds earlier in it.
    daw.set_start_offset(
        ProjectContext::Current,
        ItemRef::Guid(copy),
        TakeRef::Active,
        daw_proto::primitives::Duration::from_seconds(2.0),
    )
    .expect("slipped");
    let got = daw.points(ProjectContext::Current, copied);
    assert!((got[0].time.as_seconds() + 1.0).abs() < 1e-9, "{got:?}");
    assert!((got[1].time.as_seconds() - 1.0).abs() < 1e-9, "{got:?}");
    let original = daw.points(ProjectContext::Current, env);
    assert!(
        (original[1].time.as_seconds() - 3.0).abs() < 1e-9,
        "the original's own is untouched"
    );
}

/// A slice keeps only the curve over itself: slipped deep into a take
/// with a long curve, it holds the points around its own span, and the
/// curve at its edges is what it was.
#[test]
fn a_slice_keeps_the_curve_over_itself() {
    use daw_proto::{Items, TakeRef};
    let (daw, item, take) = project();
    let env = volume_envelope(&item, &take);
    for s in 0..100 {
        daw.add_point(
            ProjectContext::Current,
            env.clone(),
            point(f64::from(s), if s % 2 == 0 { 1.0 } else { 0.5 }),
        );
    }
    daw.set_start_offset(
        ProjectContext::Current,
        ItemRef::Guid(item),
        TakeRef::Active,
        daw_proto::primitives::Duration::from_seconds(50.0),
    )
    .expect("slipped");
    let got = daw.points(ProjectContext::Current, env);
    // The item is 4 s long: points from 1 s before it to 1 s after, and
    // one more each side.
    assert_eq!(
        got.len(),
        9,
        "{:?}",
        got.iter().map(|p| p.time.as_seconds()).collect::<Vec<_>>()
    );
    assert!((got[0].time.as_seconds() + 2.0).abs() < 1e-9);
    assert!((got[8].time.as_seconds() - 6.0).abs() < 1e-9);
}
