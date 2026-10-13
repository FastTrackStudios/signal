//! Guide track generation — REAPER-facing action contract.
//!
//! The trait only; `session::guide` is the implementation.
//!
//! Stamping the guide as MIDI is the authoring counterpart to playing it.
//! `session_guide`'s engine renders count-in and section cues as audio
//! live from a block clock; these actions write the same schedule into
//! the project as notes on the Click / Count / Guide tracks, so the guide
//! becomes material you can see, edit, and drive an external sampler
//! with — including the FTS Guide plugin.
//!
//! Both come from one `CueSchedule`, so what gets stamped and what gets
//! played can't drift.

use daw_proto::DawResult;

/// Generate the Click / Count / Guide tracks for the current project's
/// song.
///
/// `#[action(undo)]` on everything that writes: the backend brackets each
/// in a single host undo block, so one keystroke undoes a whole stamp
/// rather than several hundred individual note insertions.
///
/// Every generator is idempotent — it clears its own tracks over the
/// song's span before writing, so re-running after editing sections
/// replaces rather than layers.
#[architect::actions(namespace = "FTS_SESSION_GUIDE")]
pub trait GuideActions {
    #[action(
        undo,
        description = "Generate the Click, Count and Guide tracks for the current project — click grid from the tempo map, count-in and section announcements from the guide schedule",
        category = "Session",
        group = "Guide"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the tracks
    /// cannot be written.
    fn generate_guide_tracks(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Generate only the Click track for the current project, from the project tempo map",
        category = "Session",
        group = "Guide"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the tracks
    /// cannot be written.
    fn generate_click_track(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Generate only the Count and Guide tracks — count-in beats and section announcements, no click",
        category = "Session",
        group = "Guide"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the tracks
    /// cannot be written.
    fn generate_cue_tracks(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Delete all items from the Click, Count and Guide tracks in the current project",
        category = "Session",
        group = "Guide"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the tracks
    /// cannot be written.
    fn clear_guide_tracks(&self) -> DawResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_follow_the_reaper_command_convention() {
        let ids: Vec<_> = GuideActionsActions::all().iter().map(|m| m.id).collect();
        assert_eq!(
            ids,
            vec![
                "FTS_SESSION_GUIDE_GENERATE_GUIDE_TRACKS",
                "FTS_SESSION_GUIDE_GENERATE_CLICK_TRACK",
                "FTS_SESSION_GUIDE_GENERATE_CUE_TRACKS",
                "FTS_SESSION_GUIDE_CLEAR_GUIDE_TRACKS",
            ]
        );
    }

    /// Everything that writes must be undoable in one block.
    #[test]
    fn writing_actions_are_undo_bracketed() {
        for meta in GuideActionsActions::all() {
            assert!(meta.undo, "{} should be undo-bracketed", meta.id);
        }
    }
}
