//! Stretch markers through the facade.
//!
//! Non-destructive timing: the media is untouched and the warp lives in
//! the project, which is what makes a timing edit reversible and still
//! editable in the host afterwards.

use daw_proto::midi::Midi;
use daw_proto::project::ProjectContext;
use daw_proto::{
    ItemRef, ProjectInfo, StretchMarker, StretchMarkers, StretchMode, StretchTakeRef, TakeRef,
    TrackRef, Tracks,
};
use daw_standalone::sync::Standalone;

fn project() -> (Standalone, ItemRef) {
    let daw = Standalone::new();
    daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "p".into(),
        path: String::new(),
    });
    let ctx = ProjectContext::Current;
    let track = Tracks::add(&daw, ctx.clone(), "Vox", None).unwrap();
    let loc = Midi::create_midi_item(&daw, ctx, TrackRef::Guid(track), 0.0, 4.0).expect("item");
    (daw, loc.item)
}

fn loc(item: &ItemRef) -> StretchTakeRef {
    StretchTakeRef::new(ProjectContext::Current, item.clone(), TakeRef::Active)
}

fn read(daw: &Standalone, item: &ItemRef) -> Vec<StretchMarker> {
    daw.get_stretch_markers(ProjectContext::Current, item.clone(), TakeRef::Active)
}

#[test]
fn markers_round_trip_and_stay_sorted() {
    let (daw, item) = project();
    // Deliberately out of order: the host keeps them sorted, and every
    // reader treats them as the knots of a piecewise map — which is
    // only a map if they are in order.
    daw.add_stretch_marker(loc(&item), StretchMarker::new(2.0, 2.0))
        .unwrap();
    daw.add_stretch_marker(loc(&item), StretchMarker::new(0.5, 0.5))
        .unwrap();
    daw.add_stretch_marker(loc(&item), StretchMarker::new(1.0, 1.2))
        .unwrap();

    let got = read(&daw, &item);
    assert_eq!(got.len(), 3);
    let times: Vec<f64> = got.iter().map(|m| m.position).collect();
    assert_eq!(times, vec![0.5, 1.0, 2.0]);
    assert_eq!(got[1].source_position, 1.2);
}

#[test]
fn adding_returns_where_it_landed_not_where_it_was_pushed() {
    let (daw, item) = project();
    daw.add_stretch_marker(loc(&item), StretchMarker::new(3.0, 3.0))
        .unwrap();
    // Earlier than the first, so it sorts to the front.
    let idx = daw
        .add_stretch_marker(loc(&item), StretchMarker::new(1.0, 1.0))
        .unwrap();
    assert_eq!(idx, 0, "a caller cannot assume the last index");
}

#[test]
fn setting_the_whole_map_replaces_rather_than_merges() {
    let (daw, item) = project();
    daw.set_stretch_markers(
        loc(&item),
        vec![StretchMarker::new(0.0, 0.0), StretchMarker::new(1.0, 1.0)],
    )
    .unwrap();
    daw.set_stretch_markers(loc(&item), vec![StretchMarker::new(2.0, 2.5)])
        .unwrap();

    let got = read(&daw, &item);
    assert_eq!(got.len(), 1, "a map is a whole object, not an addition");
    assert_eq!(got[0].position, 2.0);
}

#[test]
fn an_empty_map_clears_the_take_back_to_its_recorded_timing() {
    let (daw, item) = project();
    daw.set_stretch_markers(loc(&item), vec![StretchMarker::new(1.0, 1.0)])
        .unwrap();
    daw.set_stretch_markers(loc(&item), Vec::new()).unwrap();
    assert!(read(&daw, &item).is_empty());

    daw.set_stretch_markers(loc(&item), vec![StretchMarker::new(1.0, 1.0)])
        .unwrap();
    daw.clear_stretch_markers(loc(&item)).unwrap();
    assert!(read(&daw, &item).is_empty());
}

#[test]
fn a_marker_can_be_replaced_and_deleted_by_index() {
    let (daw, item) = project();
    daw.set_stretch_markers(
        loc(&item),
        vec![
            StretchMarker::new(0.0, 0.0),
            StretchMarker::new(1.0, 1.0),
            StretchMarker::new(2.0, 2.0),
        ],
    )
    .unwrap();

    daw.set_stretch_marker(loc(&item), 1, StretchMarker::new(1.0, 1.5))
        .unwrap();
    assert_eq!(read(&daw, &item)[1].source_position, 1.5);

    daw.delete_stretch_marker(loc(&item), 1).unwrap();
    let got = read(&daw, &item);
    assert_eq!(got.len(), 2);
    assert_eq!(got[1].position, 2.0);

    assert!(daw.delete_stretch_marker(loc(&item), 99).is_err());
    assert!(
        daw.set_stretch_marker(loc(&item), 99, StretchMarker::new(0.0, 0.0))
            .is_err()
    );
}

#[test]
fn the_stretch_mode_is_remembered() {
    let (daw, item) = project();
    assert!(daw.set_stretch_mode(loc(&item), StretchMode::Tonal).is_ok());
}

// ── the coordinate conversion ────────────────────────────────────────

#[test]
fn project_time_converts_into_the_two_coordinate_systems() {
    // The part everyone gets wrong. A marker's position is in take
    // playback time — seconds from the item start, scaled by play rate
    // — and its source position is seconds into the media including the
    // take's start offset. They are not the same number and neither is
    // project time.
    let m = StretchMarker::at_project_time(
        /* project_pos        */ 12.0, /* source_project_pos */ 11.5,
        /* item_position      */ 10.0, /* start_offset       */ 3.0,
        /* play_rate          */ 2.0,
    );
    assert_eq!(m.position, 4.0, "(12 - 10) * 2");
    assert_eq!(m.source_position, 6.0, "3 + (11.5 - 10) * 2");
    assert_eq!(m.slope, 0.0);
}

#[test]
fn a_unit_take_is_the_simple_case_it_looks_like() {
    // An item at zero, no offset, rate 1: the conversion is identity,
    // which is the case that hides the bug in every other case.
    let m = StretchMarker::at_project_time(2.5, 2.5, 0.0, 0.0, 1.0);
    assert_eq!(m.position, 2.5);
    assert_eq!(m.source_position, 2.5);
}

#[test]
fn a_zero_play_rate_is_treated_as_one_rather_than_collapsing() {
    // A rate of zero would put every marker at the item start and the
    // take would be a single frame held forever.
    let m = StretchMarker::at_project_time(5.0, 5.0, 1.0, 0.0, 0.0);
    assert_eq!(m.position, 4.0);
}

#[test]
fn the_rate_between_markers_says_how_far_material_is_stretched() {
    let a = StretchMarker::new(0.0, 0.0);
    // One second of source played over two: half speed.
    let b = StretchMarker::new(2.0, 1.0);
    assert_eq!(a.rate_to(&b), Some(2.0));

    // Two seconds of source played over one: double speed.
    let c = StretchMarker::new(1.0, 2.0);
    assert_eq!(a.rate_to(&c), Some(0.5));

    // Markers sharing a source position carry no rate.
    assert_eq!(a.rate_to(&StretchMarker::new(1.0, 0.0)), None);
}
