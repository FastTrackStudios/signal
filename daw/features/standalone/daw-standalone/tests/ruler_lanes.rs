//! Ruler lanes on daw-standalone behave like REAPER's API: 0-based lane
//! indices (the `.rpp` file's 1-based numbering is converted on load),
//! a contiguous lane count, and default-lane flags that decide where a
//! fresh region or marker lands.

use daw_proto::project::ProjectContext;
use daw_proto::{Markers, Projects, Regions};
use daw_standalone::project_loader::load_rpp_text;
use daw_standalone::sync::Standalone;

/// The golden session's lane layout: SONG / SECTIONS / MARKS on file rows
/// 1-3, a song region on row 1, a section region on row 2.
const RPP: &str = r#"<REAPER_PROJECT 0.1 "7.62/test" 0
  RULERLANE 1 4 "SONG" 0 -1 0
  RULERLANE 2 8 "SECTIONS" 0 -1 0
  RULERLANE 3 0 "MARKS" 0 -1 0
  TEMPO 120 4 4
  MARKER 1 0 "My Song" 1 0 1 B {A4E59D0E-DC34-3054-3EE6-36CC9140B9A0} 0 1
  MARKER 1 60 "" 1
  MARKER 2 8 "Verse 1" 1 0 1 B {5DA91CE2-066A-22E5-608E-E1D7B21D1FF4} 0 2
  MARKER 2 24 "" 1
  MARKER 3 4 "SONGSTART" 0 0 1 B {F3DD5A61-C84C-FEF1-F4A7-0B18C8FA8FA0} 0 3
>
"#;

fn loaded() -> (Standalone, ProjectContext) {
    let daw = Standalone::new();
    let summary = load_rpp_text(&daw, "Lanes", "/tmp/lanes.rpp", RPP).unwrap();
    (daw, ProjectContext::Project(summary.project_guid))
}

#[test]
fn a_loaded_projects_lanes_are_zero_based() {
    let (daw, project) = loaded();
    assert_eq!(daw.ruler_lane_count(project.clone()), 3);
    assert_eq!(daw.get_ruler_lane_name(project.clone(), 0), "SONG");
    assert_eq!(daw.get_ruler_lane_name(project.clone(), 1), "SECTIONS");
    assert_eq!(daw.get_ruler_lane_name(project.clone(), 2), "MARKS");

    let lane_of = |name: &str| {
        Regions::all(&daw, project.clone())
            .into_iter()
            .find(|r| r.name == name)
            .and_then(|r| r.lane)
    };
    assert_eq!(lane_of("My Song"), Some(0), "file row 1 is API lane 0");
    assert_eq!(lane_of("Verse 1"), Some(1));
    let marker = Markers::all(&daw, project.clone())
        .into_iter()
        .find(|m| m.name == "SONGSTART")
        .expect("marker");
    assert_eq!(marker.lane, Some(2));

    // The file's flags come through as project info, as REAPER reports them.
    assert_eq!(
        daw.get_project_info(project.clone(), "RULER_LANE_FLAGS:0"),
        4.0
    );
    assert_eq!(daw.get_project_info(project, "RULER_LANE_FLAGS:1"), 8.0);
}

#[test]
fn fresh_regions_and_markers_land_on_the_default_lanes() {
    let (daw, project) = loaded();
    let region = Regions::add(&daw, project.clone(), 10.0, 20.0, "CH").unwrap();
    let marker = Markers::add(&daw, project.clone(), 5.0, "X").unwrap();
    let region = Regions::get(&daw, project.clone(), region).unwrap();
    let marker = Markers::get(&daw, project.clone(), marker).unwrap();
    assert_eq!(region.lane, Some(1), "SECTIONS carries the region default");
    assert_eq!(marker.lane, Some(0), "SONG carries the marker default");

    // Moving the default is exclusive, as in REAPER.
    daw.set_project_info(project.clone(), "RULER_LANE_FLAGS:2", 8.0);
    assert_eq!(
        daw.get_project_info(project.clone(), "RULER_LANE_FLAGS:1"),
        0.0
    );
    let moved = Regions::add(&daw, project.clone(), 30.0, 40.0, "BR").unwrap();
    assert_eq!(Regions::get(&daw, project, moved).unwrap().lane, Some(2));
}

#[test]
fn naming_a_lane_past_the_end_grows_the_count_contiguously() {
    let daw = Standalone::new();
    let summary = load_rpp_text(
        &daw,
        "Empty",
        "/tmp/empty.rpp",
        "<REAPER_PROJECT 0.1 \"7.0\" 0\n>\n",
    )
    .unwrap();
    let project = ProjectContext::Project(summary.project_guid);
    assert_eq!(daw.ruler_lane_count(project.clone()), 0);
    daw.set_ruler_lane_name(project.clone(), 2, "MARKS");
    assert_eq!(
        daw.ruler_lane_count(project.clone()),
        3,
        "lanes 0 and 1 exist, unnamed"
    );
    assert_eq!(daw.get_ruler_lane_name(project, 1), "");
}
