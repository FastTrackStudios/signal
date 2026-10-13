//! The drum workspace against the real kit session.
//!
//! `02 LORD OF THE FIGHT` is the drum-mode target project
//! (`spec/drum-mode.md`); it lives outside the repo, so this skips when
//! the mount is absent and bites when it is there.

use expression_editor_core::kit::LaneRole;
use expression_editor_core::{Mode, Viewport};
use expression_editor_standalone::{Loaded, Runner, Source, Target};

const RPP: &str = "/run/media/AudioHaven/Project/02 LORD OF THE FIGHT/02 LORD OF THE FIGHT.RPP";

// r[verify drums.verify.open]
#[test]
fn the_real_kit_opens_as_role_lanes() {
    if !std::path::Path::new(RPP).exists() {
        eprintln!("skipping: {RPP} not mounted");
        return;
    }
    let runner = Runner::open(
        &Source::Rpp(RPP.into()),
        &Target {
            drums: Some(None),
            ..Target::default()
        },
        Viewport {
            w: 1600.0,
            h: 900.0,
        },
        None,
    )
    .expect("open drums");

    let Loaded::DrumWorkspace(ed) = &runner.loaded else {
        panic!("expected a drum workspace ({})", runner.label);
    };

    let names_of = |role: LaneRole| -> Vec<String> {
        ed.tracks
            .role_members(role)
            .iter()
            .filter_map(|g| ed.tracks.track_by_guid(g).map(|t| t.name.clone()))
            .collect()
    };
    // `Sub` and `Verb` are receive-only buses with no items — nothing to
    // edit, so they are rightly not members.
    assert_eq!(names_of(LaneRole::Kick), ["In", "Out", "Trig"]);
    assert_eq!(names_of(LaneRole::Snare), ["Top", "Bottom", "Alt", "Trig"]);
    assert_eq!(names_of(LaneRole::Toms), ["T1 - Unused", "T2", "T3", "T4"]);
    let other = names_of(LaneRole::Other);
    for want in ["Hi-Hat", "Overheads", "Room"] {
        assert!(other.iter().any(|n| n == want), "{want} missing: {other:?}");
    }

    // Every member is a slice surface with real audio behind it.
    for t in ed.tracks.tracks() {
        assert_eq!(t.mode, Mode::UnpitchedAudio, "{} mode", t.name);
    }
    for (i, t) in ed.tracks.tracks().iter().enumerate() {
        if i == ed.tracks.active() {
            assert!(!ed.doc.peaks.is_empty(), "active doc has peaks");
        } else {
            let doc = ed.tracks.doc_of(i).expect("parked doc");
            assert!(!doc.peaks.is_empty(), "{} has peaks", t.name);
        }
    }
    assert!(ed.stacked, "opens in the stacked view");
    // r[verify drums.group.tempo]
    assert!(
        (ed.bpm - 84.0).abs() < 1e-6,
        "grid runs at the session's 84 bpm, got {}",
        ed.bpm
    );
}
