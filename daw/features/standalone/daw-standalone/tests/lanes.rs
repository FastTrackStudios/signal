//! Fixed lanes and named comps on the standalone backend.
//!
//! The round trip the ticket asks for: a lane layout and a named comp
//! with two comp areas written through the `Tracks` / `Items` services,
//! read back identical — in memory, and again after a save and reopen.

#![cfg(feature = "rpp-save")]

use daw_proto::track::{Comp, CompArea, LaneComping, LaneDisplay};
use daw_proto::{
    Duration, ItemRef, Items, PositionInSeconds, ProjectContext, Projects, TrackRef, Tracks,
};
use daw_standalone::save::save_project_as;
use daw_standalone::sync::Standalone;

struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "daw-lanes-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One track, one one-second item, no lanes — what a fresh recording
/// looks like before anyone comps it.
fn fixture(dir: &std::path::Path) -> std::path::PathBuf {
    let rpp = concat!(
        "<REAPER_PROJECT 0.1 \"7.0/linux-x86_64\" 1700000000\n",
        "  TEMPO 120 4 4 0\n",
        "  <TRACK {AAAAAAAA-0000-0000-0000-000000000001}\n",
        "    NAME \"Lead Vox\"\n",
        "    TRACKID {AAAAAAAA-0000-0000-0000-000000000001}\n",
        "    <ITEM\n",
        "      POSITION 0\n",
        "      LENGTH 1\n",
        "      IGUID {BBBBBBBB-0000-0000-0000-000000000001}\n",
        "      NAME take\n",
        "      SOFFS 0\n",
        "      GUID {CCCCCCCC-0000-0000-0000-000000000001}\n",
        "      <SOURCE EMPTY\n",
        "      >\n",
        "    >\n",
        "  >\n",
        ">\n",
    );
    let path = dir.join("session.rpp");
    std::fs::write(&path, rpp).expect("rpp");
    path
}

const TRACK: &str = "{AAAAAAAA-0000-0000-0000-000000000001}";
const ITEM: &str = "{BBBBBBBB-0000-0000-0000-000000000001}";

fn area(comp_lane: u32, source_lane: u32, start: f64, end: f64) -> CompArea {
    CompArea {
        start: PositionInSeconds::from_seconds(start),
        end: PositionInSeconds::from_seconds(end),
        source_lane,
        comp_lane,
        fade_in: Duration::from_seconds(0.01),
        fade_out: Duration::from_seconds(0.02),
    }
}

/// The layout under test: three take lanes, a comp named COMP on lane 3
/// (the active comp) with two areas, lane 3 the only lane that plays, and
/// the item moved onto lane 2.
fn write_layout(daw: &Standalone, ctx: &ProjectContext) -> (u32, LaneComping) {
    let track = TrackRef::Guid(TRACK.into());
    daw.set_lane_count(ctx.clone(), track.clone(), 3)
        .expect("lanes");
    for (lane, name) in ["Take 1", "Take 2", "Take 3"].iter().enumerate() {
        daw.set_lane_name(ctx.clone(), track.clone(), lane as u32, name)
            .expect("name");
    }
    let comp = daw
        .create_comp(ctx.clone(), track.clone(), "COMP")
        .expect("comp");
    assert_eq!(comp, 3, "the comp is the lane after the takes");
    let areas = vec![area(comp, 0, 0.0, 0.4), area(comp, 2, 0.4, 1.0)];
    daw.set_comp_areas(ctx.clone(), track.clone(), areas.clone())
        .expect("areas");
    daw.set_lane_play_mask(ctx.clone(), track.clone(), 1 << comp)
        .expect("mask");
    daw.set_fixed_lane(ctx.clone(), ItemRef::Guid(ITEM.into()), 2)
        .expect("item lane");
    (
        comp,
        LaneComping {
            record_lane: None,
            comp_lane: Some(comp),
            last_comp_lane: None,
            areas,
        },
    )
}

fn assert_layout(daw: &Standalone, ctx: &ProjectContext, comp: u32, comping: &LaneComping) {
    let track_ref = TrackRef::Guid(TRACK.into());
    let track = Tracks::get(daw, ctx.clone(), track_ref.clone()).expect("track");
    assert_eq!(track.lane_count, 4);
    assert_eq!(track.lane_play_mask, 1 << comp);
    assert_eq!(track.lane_names, vec!["Take 1", "Take 2", "Take 3", "COMP"]);
    assert_eq!(track.lane_display, LaneDisplay::Small);
    assert_eq!(
        &daw.comping(ctx.clone(), track_ref.clone())
            .expect("comping"),
        comping
    );
    assert_eq!(
        daw.comps(ctx.clone(), track_ref).expect("comps"),
        vec![Comp {
            lane: comp,
            name: "COMP".into(),
            is_active: true,
            areas: comping.areas.clone(),
        }]
    );
    let item = daw
        .get_item(ctx.clone(), ItemRef::Guid(ITEM.into()))
        .expect("item");
    assert_eq!(item.fixed_lane, Some(2));
}

#[test]
fn a_lane_layout_and_a_named_comp_read_back_identical() {
    let dir = TempDir::new();
    let daw = Standalone::new();
    let info = Projects::open(&daw, &fixture(&dir.0).to_string_lossy()).expect("open");
    let ctx = ProjectContext::Project(info.guid);
    let (comp, comping) = write_layout(&daw, &ctx);
    assert_layout(&daw, &ctx, comp, &comping);
}

#[test]
fn the_layout_survives_a_save_and_reopen() {
    let dir = TempDir::new();
    let daw = Standalone::new();
    let info = Projects::open(&daw, &fixture(&dir.0).to_string_lossy()).expect("open");
    let ctx = ProjectContext::Project(info.guid.clone());
    let (comp, comping) = write_layout(&daw, &ctx);
    let written = save_project_as(&daw, &info.guid).expect("saved");

    let reopened = Standalone::new();
    let info = Projects::open(&reopened, &written.to_string_lossy()).expect("reopen");
    let ctx = ProjectContext::Project(info.guid);
    assert_layout(&reopened, &ctx, comp, &comping);
}

/// The lane count is the track's, not a count of the names REAPER
/// happens to have written: three unnamed take lanes under a named comp
/// are still four lanes when the project is reopened.
#[test]
fn unnamed_lanes_survive_a_save_and_reopen() {
    let dir = TempDir::new();
    let daw = Standalone::new();
    let info = Projects::open(&daw, &fixture(&dir.0).to_string_lossy()).expect("open");
    let ctx = ProjectContext::Project(info.guid.clone());
    let track = TrackRef::Guid(TRACK.into());
    daw.set_lane_count(ctx.clone(), track.clone(), 4)
        .expect("lanes");
    daw.set_lane_name(ctx.clone(), track.clone(), 3, "COMP")
        .expect("name");
    daw.set_lane_play_mask(ctx.clone(), track.clone(), 0b1000)
        .expect("mask");
    let written = save_project_as(&daw, &info.guid).expect("saved");

    let reopened = Standalone::new();
    let info = Projects::open(&reopened, &written.to_string_lossy()).expect("reopen");
    let ctx = ProjectContext::Project(info.guid);
    let t = Tracks::get(&reopened, ctx, track).expect("track");
    assert_eq!(t.lane_count, 4, "names: {:?}", t.lane_names);
    assert_eq!(t.lane_play_mask, 0b1000);
    assert_eq!(t.lane_names[3], "COMP");
}

#[test]
fn renaming_and_switching_the_active_comp() {
    let dir = TempDir::new();
    let daw = Standalone::new();
    let info = Projects::open(&daw, &fixture(&dir.0).to_string_lossy()).expect("open");
    let ctx = ProjectContext::Project(info.guid);
    let track = TrackRef::Guid(TRACK.into());
    let (first, _) = write_layout(&daw, &ctx);
    let edit = daw
        .create_comp(ctx.clone(), track.clone(), "EDIT")
        .expect("EDIT");
    daw.set_lane_name(ctx.clone(), track.clone(), first, "COMP v2")
        .expect("rename");

    let comps = daw.comps(ctx.clone(), track.clone()).expect("comps");
    let names: Vec<(&str, bool)> = comps
        .iter()
        .map(|c| (c.name.as_str(), c.is_active))
        .collect();
    assert_eq!(names, vec![("COMP v2", false), ("EDIT", true)]);
    let comping = daw.comping(ctx.clone(), track.clone()).expect("comping");
    assert_eq!(
        (comping.comp_lane, comping.last_comp_lane),
        (Some(edit), Some(first))
    );

    daw.set_active_comp(ctx.clone(), track.clone(), Some(first))
        .expect("switch back");
    let comping = daw.comping(ctx.clone(), track.clone()).expect("comping");
    assert_eq!(
        (comping.comp_lane, comping.last_comp_lane),
        (Some(first), Some(edit))
    );
}

#[test]
fn lanes_reject_what_the_track_does_not_have() {
    let dir = TempDir::new();
    let daw = Standalone::new();
    let info = Projects::open(&daw, &fixture(&dir.0).to_string_lossy()).expect("open");
    let ctx = ProjectContext::Project(info.guid);
    let track = TrackRef::Guid(TRACK.into());
    let item = ItemRef::Guid(ITEM.into());

    assert!(
        daw.set_fixed_lane(ctx.clone(), item.clone(), 0).is_err(),
        "no lanes yet"
    );
    daw.set_lane_count(ctx.clone(), track.clone(), 2)
        .expect("lanes");
    assert!(
        daw.set_fixed_lane(ctx.clone(), item.clone(), 2).is_err(),
        "past the end"
    );
    assert!(
        daw.set_lane_name(ctx.clone(), track.clone(), 2, "x")
            .is_err()
    );
    assert!(
        daw.set_comp_areas(ctx.clone(), track.clone(), vec![area(0, 5, 0.0, 1.0)])
            .is_err()
    );
    assert!(
        daw.set_active_comp(ctx.clone(), track.clone(), Some(2))
            .is_err()
    );

    // Switching lanes off clears everything lane-shaped.
    daw.set_fixed_lane(ctx.clone(), item.clone(), 1)
        .expect("lane 1");
    daw.create_comp(ctx.clone(), track.clone(), "C")
        .expect("comp");
    daw.set_lane_count(ctx.clone(), track.clone(), 0)
        .expect("off");
    let t = Tracks::get(&daw, ctx.clone(), track.clone()).expect("track");
    assert_eq!(
        (t.lane_count, t.lane_play_mask, t.lane_names.len()),
        (0, 0, 0)
    );
    assert_eq!(
        daw.comping(ctx.clone(), track).expect("comping"),
        LaneComping::default()
    );
    assert_eq!(daw.get_item(ctx, item).expect("item").fixed_lane, None);
}
