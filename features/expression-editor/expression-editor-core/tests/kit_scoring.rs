//! Picking the right `Drums` folder when a session has more than one.
//!
//! Modelled on a real project (`set in stone`, Crescendum): a four-track
//! folder of reference stems named `Drums`, sitting above the actual
//! 20-track tracked kit, also named `Drums`. Taking the first match opened
//! the stems — which then presented as "the tom lanes are broken", because
//! that folder has no toms in it.

use expression_editor_core::kit::{KitScore, score_kit};

/// The reference-stem folder: four leaf tracks, no sub-folders, no toms.
fn stems() -> Vec<(&'static str, bool)> {
    vec![
        ("Snare - Set in Stone", false),
        ("OH R - Set in Stone", false),
        ("OH L - Set in Stone", false),
        ("Kick - Set in Stone", false),
    ]
}

/// The tracked kit, exactly as the project nests it.
fn tracked_kit() -> Vec<(&'static str, bool)> {
    vec![
        ("Kick", true),
        ("In", false),
        ("Out", false),
        ("Trig", false),
        ("Snare", true),
        ("Top", false),
        ("Bottom", false),
        ("Trig", false),
        ("Toms", true),
        ("T1", false),
        ("T2", false),
        ("T3", false),
        ("T4", false),
        ("Trig", true),
        ("T1 Trig", false),
        ("T2 Trig", false),
        ("T3 Trig", false),
        ("T4 Trig", false),
        ("Cymbals", true),
        ("OHs", false),
        ("Hi Hat", false),
        ("Ride", false),
        ("Rooms", false),
    ]
}

#[test]
fn the_tracked_kit_outscores_a_stem_folder_of_the_same_name() {
    let kit = score_kit(&tracked_kit());
    let stems = score_kit(&stems());
    assert!(
        kit > stems,
        "the tracked kit must win: kit={kit:?} stems={stems:?}"
    );
}

#[test]
fn a_stem_folder_covers_no_toms() {
    // The specific reason the stems produced Kick/Snare/Other and no Toms
    // lane. Kick and Snare classify; nothing there is a tom.
    let s = score_kit(&stems());
    assert_eq!(s.roles, 2, "kick and snare only: {s:?}");
    assert_eq!(s.sub_folders, 0, "stems are flat: {s:?}");
}

#[test]
fn the_tracked_kit_covers_all_three_targeted_roles() {
    let s = score_kit(&tracked_kit());
    assert_eq!(s.roles, 3, "kick, snare and toms: {s:?}");
    assert!(s.sub_folders >= 3, "grouped mics: {s:?}");
}

#[test]
fn roles_dominate_size() {
    // A big folder of overheads is not a kit, however many tracks it has.
    let overheads: Vec<(&str, bool)> = (0..50).map(|_| ("OHs", false)).collect();
    let tiny_kit = vec![("Kick", false), ("Snare", false), ("T1", false)];
    assert!(
        score_kit(&tiny_kit) > score_kit(&overheads),
        "three targeted roles must beat fifty overheads"
    );
}

#[test]
fn an_empty_folder_scores_nothing() {
    assert_eq!(score_kit(&[]), KitScore::default());
}
