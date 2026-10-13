//! Opening a real multitrack kit session through `Projects::open`.
//!
//! The reference project lives outside the repo (`02 LORD OF THE FIGHT`,
//! the drum-mode target in `features/expression-editor/spec/drum-mode.md`);
//! the test skips when it is not mounted so CI stays green, and bites when
//! it is.

#![cfg(feature = "rpp-loader")]

use daw_proto::{ItemRef, Items, Peaks, ProjectContext, Projects, TakeRef, TrackRef, Tracks};
use daw_standalone::sync::Standalone;

const RPP: &str = "/run/media/AudioHaven/Project/02 LORD OF THE FIGHT/02 LORD OF THE FIGHT.RPP";

// r[verify drums.open.rpp]
// r[verify drums.open.peaks]
#[test]
fn the_kit_session_opens_with_media() {
    if !std::path::Path::new(RPP).exists() {
        eprintln!("skipping: {RPP} not mounted");
        return;
    }
    let daw = Standalone::new();
    let info = daw.open(RPP).expect("open");
    let ctx = ProjectContext::Project(info.guid.clone());
    let tracks = daw.all(ctx.clone());
    let names: Vec<&str> = tracks.iter().map(|t| t.name.as_str()).collect();
    for want in [
        "Drums",
        "Kick",
        "In",
        "Snare",
        "Top",
        "Toms",
        "T2",
        "Hi-Hat",
        "Overheads",
        "Room",
    ] {
        assert!(names.contains(&want), "track {want} missing from {names:?}");
    }
    // The kick-in mic has audio items whose peaks are real.
    let kick_in = tracks
        .iter()
        .find(|t| t.name == "In")
        .expect("Kick/In track");
    let items = Items::get_items(&daw, ctx.clone(), TrackRef::Guid(kick_in.guid.clone()));
    assert!(!items.is_empty(), "Kick/In has items");

    // Fixed item lanes: `LANENAME 1 2 3` + `LANESOLO 4` — three comp
    // lanes, lane 2 is the one that plays. Items carry their lane from
    // `YPOS`, and the playing lane has audio on it.
    assert_eq!(kick_in.lane_count, 3, "Kick/In has three fixed lanes");
    assert_eq!(kick_in.lane_play_mask, 0b100, "lane 2 plays");
    let playing: Vec<_> = items
        .iter()
        .filter(|i| {
            i.fixed_lane
                .is_some_and(|l| kick_in.lane_play_mask & (1 << l) != 0)
        })
        .collect();
    assert!(!playing.is_empty(), "items on the playing lane");
    assert!(
        items.iter().any(|i| i.fixed_lane == Some(0)),
        "items on the non-playing lane 0 are kept, just not played"
    );

    let mut any_peaks = false;
    for item in &playing {
        let peaks = daw.take_peaks(
            ctx.clone(),
            ItemRef::Guid(item.guid.clone()),
            TakeRef::Active,
            4096,
        );
        if peaks.peaks.iter().any(|p| p.abs() > 0.01) {
            any_peaks = true;
            break;
        }
    }
    assert!(
        any_peaks,
        "no Kick/In item produced peaks — media did not resolve"
    );
}
