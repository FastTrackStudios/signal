//! Swipe comping on `daw-standalone`: a stretch of one lane put onto the
//! comp lane (`session_daw::comp::swipe`, what `TRACK_COMP_FROM_LANE` and
//! the window's swipe run) — for exactly that stretch, the rest of the
//! comp left; on a folder, every track under it at once; one undo step.
//!
//! Built by hand: a `Drums` folder with two mics, each with two takes and
//! a comp on fixed lanes, as a kit recorded in REAPER 7 comes in.

use daw::service::{ItemRef, Items, ProjectContext, Projects, TakeRef, Takes, TrackRef, Tracks};
use daw::standalone::Standalone;
use daw_proto::ProjectInfo;
use daw_proto::primitives::{Duration, PositionInSeconds};

/// The lanes every mic has: take 1, take 2, and the comp (playing).
const TAKE_1: u32 = 0;
const TAKE_2: u32 = 1;
const COMP: u32 = 2;

/// How far into its recording each take's item starts, seconds — apart, so
/// a piece copied from the wrong take, or from the wrong place in the
/// right one, shows.
const TAKE_1_AT: f64 = 10.0;
const TAKE_2_AT: f64 = 50.0;

fn append(daw: &Standalone, project: &ProjectContext, name: &str, folder_depth: i32) -> String {
    let guid = Tracks::add(daw, project.clone(), name, None).expect("add");
    if folder_depth != 0 {
        Tracks::set_folder_depth(
            daw,
            project.clone(),
            TrackRef::Guid(guid.clone()),
            folder_depth,
        )
        .expect("set_folder_depth");
    }
    guid
}

/// An item 0–10 s on `lane` of `track`, `offset` into its recording.
fn item(daw: &Standalone, project: &ProjectContext, track: &str, lane: u32, offset: f64) {
    let guid = Items::add_item(
        daw,
        project.clone(),
        TrackRef::Guid(track.to_owned()),
        PositionInSeconds::from_seconds(0.0),
        Duration::from_seconds(10.0),
    )
    .expect("an item");
    Items::set_fixed_lane(daw, project.clone(), ItemRef::Guid(guid.clone()), lane)
        .expect("on its lane");
    Takes::add_take(daw, project.clone(), ItemRef::Guid(guid.clone())).expect("its take");
    Takes::set_start_offset(
        daw,
        project.clone(),
        ItemRef::Guid(guid),
        TakeRef::Active,
        Duration::from_seconds(offset),
    )
    .expect("into its recording");
}

/// `Drums / {In, Top}`, each mic with takes 1 and 2 and a comp of take 1
/// across the whole song. The folder, then the mics.
fn kit(daw: &Standalone) -> (ProjectContext, String, Vec<String>) {
    let guid = daw.seed_project(ProjectInfo {
        guid: "comp-kit".into(),
        name: "Comp kit".into(),
        path: String::new(),
    });
    let project = ProjectContext::Project(guid);
    let drums = append(daw, &project, "Drums", 1);
    let mics = vec![
        append(daw, &project, "In", 0),
        append(daw, &project, "Top", -1),
    ];
    for mic in &mics {
        let at = TrackRef::Guid(mic.clone());
        Tracks::set_lane_count(daw, project.clone(), at.clone(), 3).expect("lanes");
        for (lane, name) in [(TAKE_1, "1"), (TAKE_2, "2"), (COMP, "C1")] {
            Tracks::set_lane_name(daw, project.clone(), at.clone(), lane, name).expect("named");
        }
        Tracks::set_lane_play_mask(daw, project.clone(), at, 1 << COMP).expect("the comp plays");
        item(daw, &project, mic, TAKE_1, TAKE_1_AT);
        item(daw, &project, mic, TAKE_2, TAKE_2_AT);
        item(daw, &project, mic, COMP, TAKE_1_AT);
    }
    (project, drums, mics)
}

/// `track`'s items on `lane`, by start: `(start, end, offset into the
/// recording)`.
fn on_lane(
    daw: &Standalone,
    project: &ProjectContext,
    track: &str,
    lane: u32,
) -> Vec<(f64, f64, f64)> {
    let mut out: Vec<(f64, f64, f64)> =
        Items::get_items(daw, project.clone(), TrackRef::Guid(track.to_owned()))
            .into_iter()
            .filter(|i| i.fixed_lane.unwrap_or(0) == lane)
            .map(|i| {
                let start = i.position.as_seconds();
                (
                    start,
                    start + i.length.as_seconds(),
                    i.start_offset.as_seconds(),
                )
            })
            .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

fn near(a: (f64, f64, f64), b: (f64, f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9 && (a.2 - b.2).abs() < 1e-9
}

/// Take 2 over 4–6 s onto one mic's comp: take 1 either side as it was,
/// take 2 in between from 4 s into its item — exactly the stretch — the
/// takes themselves untouched, a comp area recorded, the other mic left.
#[test]
fn a_swipe_puts_exactly_that_stretch_of_the_lane_on_the_comp() {
    let daw = Standalone::new();
    let (project, _, mics) = kit(&daw);
    let (inside, top) = (&mics[0], &mics[1]);

    let swipe = |lane: &str, span: (f64, f64)| {
        session_daw::comp::swipe(&daw, &project, std::slice::from_ref(inside), lane, span)
    };
    let said = swipe("2", (4.0, 6.0)).expect("comped");
    assert_eq!(said.len(), 1, "{said:?}");

    let comp = on_lane(&daw, &project, inside, COMP);
    assert_eq!(comp.len(), 3, "{comp:?}");
    assert!(
        near(comp[0], (0.0, 4.0, TAKE_1_AT)),
        "take 1 before: {comp:?}"
    );
    assert!(
        near(comp[1], (4.0, 6.0, TAKE_2_AT + 4.0)),
        "take 2, 4 s into it: {comp:?}"
    );
    assert!(
        near(comp[2], (6.0, 10.0, TAKE_1_AT + 6.0)),
        "take 1 after, playing on from the cut: {comp:?}"
    );
    for lane in [TAKE_1, TAKE_2] {
        assert_eq!(
            on_lane(&daw, &project, inside, lane).len(),
            1,
            "the takes stay"
        );
    }
    assert_eq!(
        on_lane(&daw, &project, top, COMP),
        [(0.0, 10.0, TAKE_1_AT)],
        "a track not asked for is left"
    );

    let comping = Tracks::comping(&daw, project.clone(), TrackRef::Guid(inside.clone()))
        .expect("the comping state");
    assert_eq!(
        session_daw::comp::taken_from(&comping.areas, COMP, TAKE_2),
        [(4.0, 6.0)],
        "the comp area says where it came from"
    );

    // A second swipe over part of the first takes take 1 back there.
    swipe("1", (5.0, 7.0)).expect("comped again");
    let comp = on_lane(&daw, &project, inside, COMP);
    let spans: Vec<(f64, f64)> = comp.iter().map(|c| (c.0, c.1)).collect();
    assert_eq!(spans, [(0.0, 4.0), (4.0, 5.0), (5.0, 7.0), (7.0, 10.0)]);
    assert!(near(comp[2], (5.0, 7.0, TAKE_1_AT + 5.0)), "{comp:?}");
    let comping = Tracks::comping(&daw, project.clone(), TrackRef::Guid(inside.clone()))
        .expect("the comping state");
    assert_eq!(
        session_daw::comp::taken_from(&comping.areas, COMP, TAKE_2),
        [(4.0, 5.0)],
        "what is left of the first area"
    );
}

/// A swipe on the folder comps every mic under it — the same stretch of
/// the same take — as one undo step, which takes it all back.
#[test]
fn a_swipe_on_a_folder_comps_every_track_under_it_as_one_step() {
    let daw = Standalone::new();
    let (project, drums, mics) = kit(&daw);

    let said = session_daw::comp::swipe(&daw, &project, &[drums], "2", (2.0, 3.5)).expect("comped");
    assert_eq!(said.len(), 2, "both mics: {said:?}");
    for mic in &mics {
        let comp = on_lane(&daw, &project, mic, COMP);
        assert_eq!(comp.len(), 3, "{comp:?}");
        assert!(
            near(comp[1], (2.0, 3.5, TAKE_2_AT + 2.0)),
            "{mic}: {comp:?}"
        );
    }

    assert!(Projects::undo(&daw, project.clone()), "one step to undo");
    for mic in &mics {
        assert_eq!(
            on_lane(&daw, &project, mic, COMP),
            [(0.0, 10.0, TAKE_1_AT)],
            "{mic}: the comp as it was"
        );
    }
}

/// Two tracks named — a mic and its media-edit group — are swiped as one,
/// each by the lane of that name; a track named twice (itself, and under
/// its folder) is comped once.
#[test]
fn a_swipe_on_several_tracks_comps_each_once() {
    let daw = Standalone::new();
    let (project, drums, mics) = kit(&daw);
    let said = session_daw::comp::swipe(
        &daw,
        &project,
        &[mics[0].clone(), drums, mics[1].clone()],
        "2",
        (1.0, 2.0),
    )
    .expect("comped");
    assert_eq!(said.len(), 2, "each mic once: {said:?}");
    for mic in &mics {
        assert_eq!(on_lane(&daw, &project, mic, COMP).len(), 3, "{mic}");
    }
}

/// A swipe onto nothing is refused, and says why: backwards, from a lane
/// no track has, from the comp itself, on a track without lanes.
#[test]
fn a_swipe_with_nothing_to_comp_is_refused() {
    let daw = Standalone::new();
    let (project, drums, mics) = kit(&daw);
    let swipe = |tracks: &[String], lane: &str, span: (f64, f64)| {
        session_daw::comp::swipe(&daw, &project, tracks, lane, span)
    };
    let folder = [drums];
    assert!(swipe(&folder, "2", (6.0, 4.0)).is_err(), "backwards");
    assert!(swipe(&folder, "9", (4.0, 6.0)).is_err(), "no such lane");
    assert!(
        swipe(&mics[..1], "C1", (4.0, 6.0)).is_err(),
        "the comp itself"
    );
    assert!(swipe(&["nowhere".to_owned()], "2", (4.0, 6.0)).is_err());
    let loose = append(&daw, &project, "Loose", 0);
    assert!(swipe(&[loose], "2", (4.0, 6.0)).is_err(), "no lanes");
    assert!(
        Projects::last_undo_label(&daw, project.clone()).is_none(),
        "no step was made"
    );
    assert_eq!(
        on_lane(&daw, &project, &mics[0], COMP),
        [(0.0, 10.0, TAKE_1_AT)],
        "and nothing changed"
    );
}
