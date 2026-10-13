//! Session Track Manager — headless, no REAPER.
//!
//! Drives `session::track_manager::TrackManager` (the REAPER
//! actions "Session Track Manager - Add Channel/Multi-Mic/Arrangement/
//! Layer/Performer") wrapping `daw_standalone::sync::Standalone`, the same
//! in-memory backend used by `standalone_setlist_harness.rs`. `TrackManager<D>`
//! is generic over any `Tracks + Items + Projects` backend, so the exact
//! same `#[action]`-decorated trait methods production wires to
//! `daw::reaper::Reaper` (via `register_actions` -> `ScopedActionBackend`
//! -> `ActionBackend`) run here as plain method calls on the same wrapper
//! type — no action-id string/enum indirection, and no calling actions
//! directly on the raw DAW backend (`add_channel` is session's business
//! logic, not something `Standalone`/`Reaper` themselves know about).
//!
//! Assertions reuse `daw_proto`'s existing `TrackHierarchy` /
//! `TrackStructureBuilder` / `assert_tracks_equal` test infra (the same
//! helpers `dynamic-template`'s guitar-layer tests build expected
//! hierarchies with) rather than a one-off comparison type.
//!
//! Run: cargo test -p session --test `track_manager_actions` -- --nocapture

use daw::service::{ProjectContext, Tracks, TracksExt};
use daw_proto::{
    FolderDepthChange, ProjectInfo, TrackHierarchy, TrackNode, TrackStructureBuilder,
    assert_tracks_equal,
};
use daw_standalone::sync::Standalone;
use session::track_manager::{TrackManager, TrackManagerActions};

/// `Track.folder_depth` (raw REAPER-style relative depth change) is exactly
/// `FolderDepthChange`'s raw representation, so a live project's flat,
/// index-ordered track list converts straight into a `TrackHierarchy` with
/// no reshaping.
fn hierarchy_of(daw: &Standalone) -> TrackHierarchy {
    let mut all = daw.all(ProjectContext::Current);
    all.sort_by_key(|t| t.index);
    TrackHierarchy::from_tracks(
        all.into_iter()
            .map(|t| TrackNode {
                is_folder: t.folder_depth > 0,
                folder_depth_change: FolderDepthChange::from_raw_value(t.folder_depth),
                ..TrackNode::new(t.name)
            })
            .collect(),
    )
}

/// `Standalone` is a cheap `Clone` (Arc-based) handle onto shared state —
/// `daw` stays around for setup/inspection (seeding, `all`/`selected`),
/// while `tm` (sharing the same underlying state via the clone) is the
/// session-facing object the actions actually run through, mirroring how
/// production wraps `daw_reaper::Reaper` the same way.
fn setup() -> (Standalone, TrackManager<Standalone>) {
    let daw = Standalone::new();
    daw.seed_project(ProjectInfo {
        guid: "demo-proj".into(),
        name: "Demo".into(),
        path: String::new(),
    });
    let tm = TrackManager::new(daw.clone());
    (daw, tm)
}

/// Insert Electric GTR, then Add Multi-Mic twice -> Amp, DI (in the order
/// `electric_guitar.rs` declares its multi-mic descriptors), selection stays
/// on Electric GTR throughout (`run_track_edit` restores the pre-edit
/// selection after every action).
#[test]
fn multi_mic_then_channel_on_electric_guitar() {
    let (daw, tm) = setup();

    let electric_gtr = daw.insert_track("Electric GTR").unwrap();
    daw.select(&electric_gtr).unwrap();

    tm.add_multi_mic().expect("first Add Multi-Mic");
    tm.add_multi_mic().expect("second Add Multi-Mic");

    let expected = TrackStructureBuilder::new()
        .folder("Electric GTR")
        .track("Amp")
        .track("DI")
        .end()
        .build();
    assert_tracks_equal(&hierarchy_of(&daw), &expected).unwrap();

    // Electric GTR (the originally-selected track) is still selected, not
    // one of its new children.
    let selected = daw.selected(ProjectContext::Current);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].guid, electric_gtr);

    // A third Add Multi-Mic keeps going down the electric's source list
    // (Amp, DI, Pedalboard, Amp 1, Amp 2, SM57, Royer) — the seven
    // sources of one channel, `flow.guitars.dimensions`.
    tm.add_multi_mic().expect("third Add Multi-Mic");
    let expected = TrackStructureBuilder::new()
        .folder("Electric GTR")
        .track("Amp")
        .track("DI")
        .track("Pedalboard")
        .end()
        .build();
    assert_tracks_equal(&hierarchy_of(&daw), &expected).unwrap();

    // Select the Amp child and Add Channel -> Amp gains L/R children.
    let amp_guid = daw.find_track("Amp").unwrap().guid;
    daw.select(&amp_guid).unwrap();
    tm.add_channel().expect("Add Channel on Amp");

    let expected = TrackStructureBuilder::new()
        .folder("Electric GTR")
        .folder("Amp")
        .track("L")
        .track("R")
        .end()
        .track("DI")
        .track("Pedalboard")
        .end()
        .build();
    assert_tracks_equal(&hierarchy_of(&daw), &expected).unwrap();
}

/// Add Arrangement on the top-level instrument track appends a new
/// `<ArrangementDescriptor>` sibling under the same scope — it does not
/// (yet) duplicate the existing Amp/DI subtree into a second numbered
/// arrangement the way a future "Insert Electric GTR" / record-group flow
/// would. This test documents today's actual behavior.
#[test]
fn add_arrangement_appends_placeholder_sibling() {
    let (daw, tm) = setup();

    let electric_gtr = daw.insert_track("Electric GTR").unwrap();
    daw.select(&electric_gtr).unwrap();
    tm.add_multi_mic().expect("Add Multi-Mic (Amp)");

    daw.select(&electric_gtr).unwrap();
    tm.add_arrangement().expect("Add Arrangement");

    let expected = TrackStructureBuilder::new()
        .folder("Electric GTR")
        .track("Amp")
        .track("<ArrangementDescriptor>")
        .end()
        .build();
    assert_tracks_equal(&hierarchy_of(&daw), &expected).unwrap();
}

/// Add Layer wraps the selected track's existing channel/multi-mic shape
/// into a new "DBL" sibling scope, inheriting the same children shape.
#[test]
fn add_layer_inherits_multi_mic_shape() {
    let (daw, tm) = setup();

    let electric_gtr = daw.insert_track("Electric GTR").unwrap();
    daw.select(&electric_gtr).unwrap();
    tm.add_multi_mic().expect("Add Multi-Mic (Amp)");

    daw.select(&electric_gtr).unwrap();
    tm.add_layer().expect("Add Layer");

    let expected = TrackStructureBuilder::new()
        .folder("Electric GTR")
        .track("Amp")
        .folder("DBL")
        .track("Amp")
        .end()
        .end()
        .build();
    assert_tracks_equal(&hierarchy_of(&daw), &expected).unwrap();
}

/// Add Channel on a bare track (no channels, no mics) scaffolds the first
/// L/R pair underneath it.
#[test]
fn add_channel_scaffolds_first_l_r_pair() {
    let (daw, tm) = setup();

    let electric_gtr = daw.insert_track("Electric GTR").unwrap();
    daw.select(&electric_gtr).unwrap();
    tm.add_channel().expect("Add Channel");

    let expected = TrackStructureBuilder::new()
        .folder("Electric GTR")
        .track("L")
        .track("R")
        .end()
        .build();
    assert_tracks_equal(&hierarchy_of(&daw), &expected).unwrap();
}

/// Add Channel on a scope whose children are bare multi-mics folds those
/// mics under an L channel in place, then mirrors them under a new R.
#[test]
fn add_channel_splits_existing_multi_mics_across_l_r() {
    let (daw, tm) = setup();

    let electric_gtr = daw.insert_track("Electric GTR").unwrap();
    daw.select(&electric_gtr).unwrap();
    tm.add_multi_mic().expect("Add Multi-Mic (Amp)");
    tm.add_multi_mic().expect("Add Multi-Mic (DI)");

    daw.select(&electric_gtr).unwrap();
    tm.add_channel().expect("Add Channel over bare multi-mics");

    let expected = TrackStructureBuilder::new()
        .folder("Electric GTR")
        .folder("L")
        .track("Amp")
        .track("DI")
        .end()
        .folder("R")
        .track("Amp")
        .track("DI")
        .end()
        .end()
        .build();
    assert_tracks_equal(&hierarchy_of(&daw), &expected).unwrap();
}

/// A third channel inherits the shape of the existing channels rather
/// than arriving bare — Add Channel on an already-channelled scope copies
/// the first channel's mic subtree onto the new one.
#[test]
fn add_channel_inherits_existing_channel_shape() {
    let (daw, tm) = setup();

    let electric_gtr = daw.insert_track("Electric GTR").unwrap();
    daw.select(&electric_gtr).unwrap();
    tm.add_multi_mic().expect("Add Multi-Mic (Amp)");
    daw.select(&electric_gtr).unwrap();
    tm.add_channel().expect("split into L/R");

    daw.select(&electric_gtr).unwrap();
    tm.add_channel().expect("third channel");

    // C is the next configured channel after L/R, and comes with the same
    // single Amp mic the existing channels carry.
    let expected = TrackStructureBuilder::new()
        .folder("Electric GTR")
        .folder("L")
        .track("Amp")
        .end()
        .folder("R")
        .track("Amp")
        .end()
        .folder("C")
        .track("Amp")
        .end()
        .end()
        .build();
    assert_tracks_equal(&hierarchy_of(&daw), &expected).unwrap();
}

/// Add Performer mirrors Add Layer's inherit-the-shape behaviour, under a
/// differently-named scope.
#[test]
fn add_performer_inherits_multi_mic_shape() {
    let (daw, tm) = setup();

    let electric_gtr = daw.insert_track("Electric GTR").unwrap();
    daw.select(&electric_gtr).unwrap();
    tm.add_multi_mic().expect("Add Multi-Mic (Amp)");

    daw.select(&electric_gtr).unwrap();
    tm.add_performer().expect("Add Performer");

    let expected = TrackStructureBuilder::new()
        .folder("Electric GTR")
        .track("Amp")
        .folder("New Performer")
        .track("Amp")
        .end()
        .end()
        .build();
    assert_tracks_equal(&hierarchy_of(&daw), &expected).unwrap();
}

/// The reorganize actions are registered but their hierarchy-rewrite
/// policy is still pending — they must at least require a selection and
/// otherwise no-op cleanly rather than erroring or mutating.
#[test]
fn reorganize_actions_require_selection_and_no_op() {
    let (daw, tm) = setup();

    assert!(
        tm.reorganize_selected_by_performer()
            .unwrap_err()
            .to_string()
            .contains("no track is selected")
    );

    let electric_gtr = daw.insert_track("Electric GTR").unwrap();
    daw.select(&electric_gtr).unwrap();
    tm.reorganize_selected_by_performer().expect("performer");
    tm.reorganize_selected_by_arrangement()
        .expect("arrangement");

    let expected = TrackStructureBuilder::new().track("Electric GTR").build();
    assert_tracks_equal(&hierarchy_of(&daw), &expected).unwrap();
}

/// Every action requires a selection first.
#[test]
fn action_without_selection_errors() {
    let (_daw, tm) = setup();
    let err = tm.add_multi_mic().unwrap_err();
    assert!(err.to_string().contains("no track is selected"));
}
