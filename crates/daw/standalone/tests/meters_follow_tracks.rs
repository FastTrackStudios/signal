//! Tracks added while an engine runs are metered: the meter bank grows with
//! the track set once an engine has installed one. One engine hosts many
//! rigs (docs/design/one-engine.md), and each rig adds its tracks after the
//! engine is already up.

use daw_proto::ProjectInfo;
use daw_proto::handle::DawHandle as _;
use daw_standalone::metering::Meters;
use daw_standalone::sync::Standalone;

fn project(daw: &Standalone) {
    daw.seed_project(ProjectInfo {
        guid: "p".into(),
        name: "one engine".into(),
        path: String::new(),
    });
    daw.set_current_project("p");
}

#[test]
fn a_track_added_after_the_engine_started_gets_a_meter_cell() {
    let daw = Standalone::new();
    project(&daw);
    daw.current()
        .add_track("Guitar")
        .expect("first rig's track");
    // What attaching an engine does: a bank sized to the tracks it found.
    daw.set_meters(Meters::new(1));

    daw.current().add_track("Keys").expect("second rig's track");
    daw.current().add_track("Drums").expect("third rig's track");

    assert!(
        daw.meters().len() >= 3,
        "bank has {} cells",
        daw.meters().len()
    );
}

#[test]
fn without_an_engine_the_bank_stays_empty() {
    let daw = Standalone::new();
    project(&daw);
    daw.current().add_track("Guitar").expect("track");
    assert_eq!(daw.meters().len(), 0);
}
