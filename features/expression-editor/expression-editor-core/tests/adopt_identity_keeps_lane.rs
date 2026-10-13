//! A host adopting its identity for the active track keeps that track in
//! its lane: the lane layout names tracks by guid, and renaming the track
//! alone used to leave its lane naming a track that no longer existed —
//! a stacked view then drew every track but the one being edited.

use expression_editor_core::{Editor, ExpressionDoc, TimeBase, Viewport};

#[test]
fn the_adopted_track_keeps_its_lane() {
    let mut ed = Editor::new(
        ExpressionDoc::new(TimeBase::Ppq { ppq: 960.0 }, 0.0, 3840.0),
        Viewport::new(800.0, 400.0),
    );
    ed.adopt_track_identity("item-main", Some("Main".to_owned()));
    ed.add_track_with_guid(
        "item-dbl",
        "DBL",
        ExpressionDoc::new(TimeBase::Ppq { ppq: 960.0 }, 0.0, 3840.0),
    );
    let lanes: Vec<Vec<usize>> = (0..2).map(|l| ed.tracks.lane_tracks(l)).collect();
    assert_eq!(
        lanes,
        [vec![0], vec![1]],
        "a lane a track, the adopted one first"
    );
}
