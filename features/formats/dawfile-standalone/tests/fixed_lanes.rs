//! Fixed lanes and comps through the standalone loader.
//!
//! Two claims. First, this loader and `dawfile-reaper` decode a
//! REAPER-saved project's lanes identically — the same fixture, the same
//! `Track` lane fields, the same comping state. Second, lanes and comps
//! written through the document come back after an export/import trip,
//! both on a track REAPER wrote (the patch path) and on one the editor
//! added (the build path).

use daw_proto::track::{CompArea, LaneComping, LaneDisplay};
use daw_proto::{Duration, PositionInSeconds};
use dawfile_standalone::{DawProject, DocumentEdit, DocumentQuery};

const FIXTURE: &str = include_str!("../../dawfile-reaper/resources/Template-with-takes-lanes.RPP");

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

#[test]
fn both_loaders_decode_the_fixture_lanes_the_same_way() {
    let (project, _) = DawProject::import_rpp(FIXTURE, "lanes").expect("import");
    let reaper = dawfile_reaper::parse_project_text(FIXTURE).expect("parse");

    let ours = project
        .document()
        .tracks
        .iter()
        .find(|t| t.track.name == "In")
        .expect("In track");
    let theirs = reaper
        .tracks
        .iter()
        .find(|t| t.name == "In")
        .expect("In track");
    let state = theirs.fixed_lane_state();

    assert_eq!(ours.track.lane_count, state.lane_count);
    assert_eq!(ours.track.lane_count, 5);
    assert_eq!(ours.track.lane_play_mask, state.lane_play_mask);
    assert_eq!(ours.track.lane_play_mask, 0b1);
    assert_eq!(ours.track.lane_names, state.lane_names);
    assert_eq!(ours.track.lane_display, state.lane_display);
    assert_eq!(ours.track.lane_display, LaneDisplay::Big);
    assert_eq!(ours.comping, theirs.lane_comping());
    assert_eq!(ours.comping.comp_lane, Some(0));
    assert_eq!(ours.comping.last_comp_lane, Some(1));
    assert_eq!(ours.comping.areas.len(), 2);

    // Items carry their lane from YPOS, exactly as dawfile-reaper derives it.
    let our_lanes: Vec<Option<u32>> = ours.items.iter().map(|i| i.item.fixed_lane).collect();
    let their_lanes: Vec<Option<u32>> = theirs
        .items
        .iter()
        .map(|i| i.lane.map(|l| l as u32))
        .collect();
    assert_eq!(our_lanes, their_lanes);
    assert_eq!(our_lanes[0], Some(4));
}

#[test]
fn a_lane_track_without_lanesolo_plays_lane_zero_in_both_loaders() {
    // `key-signatures.RPP`: `FIXEDLANES 1 0 1 0 0` + `LANENAME 1`, no
    // LANESOLO — one lane, and it plays.
    let text = include_str!("../../dawfile-reaper/tests/fixtures/key-signatures.RPP");
    let (project, _) = DawProject::import_rpp(text, "keys").expect("import");
    let reaper = dawfile_reaper::parse_project_text(text).expect("parse");
    let ours: Vec<(u32, u64, LaneDisplay)> = project
        .document()
        .tracks
        .iter()
        .map(|t| {
            (
                t.track.lane_count,
                t.track.lane_play_mask,
                t.track.lane_display,
            )
        })
        .collect();
    let theirs: Vec<(u32, u64, LaneDisplay)> = reaper
        .tracks
        .iter()
        .map(|t| {
            let s = t.fixed_lane_state();
            (s.lane_count, s.lane_play_mask, s.lane_display)
        })
        .collect();
    assert_eq!(ours, theirs);
    assert!(ours.contains(&(1, 0b1, LaneDisplay::One)), "{ours:?}");

    // And an unedited export rewrites none of it.
    let (_, report) = project.to_rpp_patched().expect("patch");
    assert!(report.changes.is_empty(), "{:?}", report.changes);
}

#[test]
fn lanes_and_comps_survive_the_patch_path() {
    let (mut project, _) = DawProject::import_rpp(FIXTURE, "lanes").expect("import");
    let in_id = project
        .document()
        .tracks
        .iter()
        .find(|t| t.track.name == "In")
        .map(|t| t.id.clone())
        .expect("In track");
    let comping = LaneComping {
        record_lane: Some(2),
        comp_lane: Some(1),
        last_comp_lane: Some(0),
        areas: vec![area(1, 3, 2.0, 3.5), area(1, 4, 3.5, 5.2)],
    };
    project.edit(|doc| {
        let node = doc.track_mut(&in_id).expect("track");
        node.track.lane_count = 6;
        node.track.lane_play_mask = 0b10;
        node.track.lane_names = vec!["EDIT", "COMP", "1", "2", "3", "4"]
            .into_iter()
            .map(String::from)
            .collect();
        node.track.lane_display = LaneDisplay::Small;
        node.comping = comping.clone();
        node.items[0].item.fixed_lane = Some(5);
    });
    let text = project.to_rpp().expect("export");
    assert!(text.contains("LANENAME EDIT COMP 1 2 3 4\n"), "{text}");
    assert!(text.contains("LANEREC 2 1 0\n"), "{text}");
    assert!(text.contains("ITEMLANES 6\n"), "{text}");

    let (back, _) = DawProject::import_rpp(&text, "lanes").expect("reimport");
    let node = back.document().track(&in_id).expect("track");
    assert_eq!(node.track.lane_count, 6);
    assert_eq!(node.track.lane_play_mask, 0b10);
    assert_eq!(node.track.lane_names[0], "EDIT");
    assert_eq!(node.track.lane_display, LaneDisplay::Small);
    assert_eq!(node.comping, comping);
    assert_eq!(node.items[0].item.fixed_lane, Some(5));

    // dawfile-reaper reads the patched text the same way.
    let reaper = dawfile_reaper::parse_project_text(&text).expect("parse");
    let theirs = reaper.tracks.iter().find(|t| t.name == "In").expect("In");
    assert_eq!(theirs.fixed_lane_state().lane_count, 6);
    assert_eq!(theirs.fixed_lane_state().lane_play_mask, 0b10);
    assert_eq!(theirs.lane_comping(), comping);
    assert_eq!(theirs.items[0].lane, Some(5));
}

#[test]
fn lanes_and_comps_survive_the_build_path() {
    let (mut project, _) = DawProject::import_rpp(FIXTURE, "lanes").expect("import");
    let comping = LaneComping {
        record_lane: None,
        comp_lane: Some(2),
        last_comp_lane: None,
        areas: vec![area(2, 0, 0.0, 1.0), area(2, 1, 1.0, 2.0)],
    };
    let (track_id, item_id) = project.edit(|doc| {
        let track_id = doc.add_track("Vox");
        let item_id = doc.add_item(&track_id, 0.0, 2.0).expect("item");
        let node = doc.track_mut(&track_id).expect("track");
        node.track.lane_count = 3;
        node.track.lane_play_mask = 0b100;
        node.track.lane_names = vec!["1".into(), "2".into(), "COMP".into()];
        node.track.lane_display = LaneDisplay::Big;
        node.comping = comping.clone();
        node.items[0].item.fixed_lane = Some(2);
        (track_id, item_id)
    });
    let text = project.to_rpp().expect("export");
    let (back, _) = DawProject::import_rpp(&text, "lanes").expect("reimport");
    let node = back.document().track(&track_id).expect("track");
    assert_eq!(node.track.lane_count, 3);
    assert_eq!(node.track.lane_play_mask, 0b100);
    assert_eq!(node.track.lane_names, vec!["1", "2", "COMP"]);
    assert_eq!(node.track.lane_display, LaneDisplay::Big);
    assert_eq!(node.comping, comping);
    assert_eq!(
        back.document()
            .item(&item_id)
            .expect("item")
            .item
            .fixed_lane,
        Some(2)
    );
    assert_eq!(node.comping.comps(&node.track.lane_names)[0].name, "COMP");
}

#[test]
fn a_track_without_lanes_writes_no_lane_lines_and_reads_none_back() {
    let (mut project, _) = DawProject::import_rpp(FIXTURE, "lanes").expect("import");
    let track_id = project.edit(|doc| {
        let track_id = doc.add_track("Plain");
        doc.add_item(&track_id, 0.0, 1.0).expect("item");
        track_id
    });
    let text = project.to_rpp().expect("export");
    let (back, _) = DawProject::import_rpp(&text, "lanes").expect("reimport");
    let node = back.document().track(&track_id).expect("track");
    assert_eq!(node.track.lane_count, 0);
    assert_eq!(node.comping, LaneComping::default());
    assert_eq!(node.items[0].item.fixed_lane, None);
}
