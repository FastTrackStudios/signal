//! Fixed lanes and comp areas, read from a REAPER-saved project.
//!
//! The fixture is `resources/Template-with-takes-lanes.RPP`, saved by
//! REAPER 7: its `In` track has five lanes (`LANENAME "Custom Lane Name"
//! C1 1 2 3`), lane 0 playing (`LANESOLO 1 …`), lane 0 as the comping
//! lane and lane 1 as the previous one (`LANEREC -1 0 1`), and two comp
//! areas both taken from lane 4 (`LINKEDLANE 2 5.205… 4 0 -1 0.01 0.01`
//! and the same for comp lane 1).

use daw_proto::track::LaneDisplay;
use dawfile_reaper::types::RppSerialize;
use dawfile_reaper::types::track::{CompAreaSettings, Track};
use dawfile_reaper::{ReaperProject, parse_project_text};

const FIXTURE: &str = include_str!("../resources/Template-with-takes-lanes.RPP");

fn in_track() -> Track {
    let project: ReaperProject = parse_project_text(FIXTURE).expect("fixture parses");
    project
        .tracks
        .iter()
        .find(|t| t.name == "In")
        .cloned()
        .expect("the fixture has an `In` track")
}

#[test]
fn comp_areas_are_read_from_linkedlane_lines() {
    let track = in_track();
    assert_eq!(track.item_lanes, Some(5));
    assert_eq!(
        track.comp_areas,
        vec![
            CompAreaSettings {
                start: 2.0,
                end: 5.20533333333333,
                source_lane: 4,
                comp_lane: 0,
                unknown_field_5: -1,
                fade_in: 0.01,
                fade_out: 0.01,
            },
            CompAreaSettings {
                start: 2.0,
                end: 5.20533333333333,
                source_lane: 4,
                comp_lane: 1,
                unknown_field_5: -1,
                fade_in: 0.01,
                fade_out: 0.01,
            },
        ]
    );
}

#[test]
fn comp_areas_serialize_and_parse_back() {
    let mut track = in_track();
    // Serialization prefers the verbatim block when one is present; drop it
    // so the typed fields are what gets written.
    track.raw_content.clear();
    let text = track.to_rpp_string();
    assert!(text.contains("ITEMLANES 5\n"), "{text}");
    assert!(
        text.contains("LINKEDLANE 2 5.20533333333333 4 0 -1 0.01 0.01\n"),
        "{text}"
    );
    let project = parse_project_text(&format!("<REAPER_PROJECT 0.1 \"7.0\" 0\n{text}>\n"))
        .expect("serialized track parses");
    let back = &project.tracks[0];
    assert_eq!(back.item_lanes, track.item_lanes);
    assert_eq!(back.comp_areas, track.comp_areas);
    assert_eq!(back.lane_record, track.lane_record);
    assert_eq!(back.lane_names, track.lane_names);
    assert_eq!(back.lane_solo, track.lane_solo);
    assert_eq!(back.fixed_lanes, track.fixed_lanes);
}

#[test]
fn fixed_lane_fields_decode_to_the_sdk_meanings() {
    // `FIXEDLANES 9 0 0 0 0`: field 1 is `C_LANESETTINGS` (&1 auto-remove
    // empty lanes, &8 big lanes); the lane count is the number of LANENAME
    // tokens, never field 1; the play mask is LANESOLO, never field 2.
    let track = in_track();
    let state = track.fixed_lane_state();
    assert_eq!(state.lane_count, 5);
    assert_eq!(state.lane_play_mask, 0b1);
    assert_eq!(
        state.lane_names,
        vec!["Custom Lane Name", "C1", "1", "2", "3"]
    );
    assert_eq!(state.lane_display, LaneDisplay::Big);
}

#[test]
fn lane_comping_surfaces_lanerec_and_the_areas() {
    let comping = in_track().lane_comping();
    assert_eq!(comping.record_lane, None);
    assert_eq!(comping.comp_lane, Some(0));
    assert_eq!(comping.last_comp_lane, Some(1));
    assert_eq!(comping.areas.len(), 2);
    let a = &comping.areas[1];
    assert_eq!(a.start.as_seconds(), 2.0);
    assert_eq!(a.end.as_seconds(), 5.20533333333333);
    assert_eq!((a.source_lane, a.comp_lane), (4, 1));
    assert_eq!(a.fade_in.as_seconds(), 0.01);
    assert_eq!(a.fade_out.as_seconds(), 0.01);

    let comps = comping.comps(&in_track().fixed_lane_state().lane_names);
    assert_eq!(comps.len(), 2);
    assert_eq!(comps[0].name, "Custom Lane Name");
    assert!(comps[0].is_active);
    assert_eq!(comps[1].name, "C1");
}
