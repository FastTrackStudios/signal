//! Pre-roll / count-in duration — REAPER-facing action contract.
//!
//! The trait only; `daw_actions::preroll` is the implementation. Contracts
//! are protocol and live here, where any host can see them without pulling
//! in a crate that drives a live backend.

#[architect::actions(namespace = "FTS_SESSION")]
pub trait PreRollActions {
    #[action(
        description = "Double the project pre-roll/count-in duration",
        category = "Transport",
        group = "Pre-Roll"
    )]
    fn pre_roll_double_duration(&self);
    #[action(
        description = "Halve the project pre-roll/count-in duration",
        category = "Transport",
        group = "Pre-Roll"
    )]
    fn pre_roll_half_duration(&self);
    #[action(
        description = "Set the project pre-roll/count-in duration to half a measure",
        category = "Transport",
        group = "Pre-Roll",
        toggleable
    )]
    fn pre_roll_set_half_measure(&self);
    #[action(
        description = "Set the project pre-roll/count-in duration to one measure",
        category = "Transport",
        group = "Pre-Roll",
        toggleable
    )]
    fn pre_roll_set_1_measure(&self);
    #[action(
        description = "Set the project pre-roll/count-in duration to two measures",
        category = "Transport",
        group = "Pre-Roll",
        toggleable
    )]
    fn pre_roll_set_2_measures(&self);
}

#[cfg(test)]
mod preroll_id_tests {
    use super::*;

    /// The exact REAPER command-name strings. These predate the move out
    /// of `daw-actions` and must survive it — keybindings, toolbars and
    /// `extension_loads.rs` all depend on them.
    #[test]
    fn ids_match_pre_move_command_ids() {
        let ids: Vec<_> = PreRollActionsActions::all().iter().map(|m| m.id).collect();
        assert_eq!(
            ids,
            vec![
                "FTS_SESSION_PRE_ROLL_DOUBLE_DURATION",
                "FTS_SESSION_PRE_ROLL_HALF_DURATION",
                "FTS_SESSION_PRE_ROLL_SET_HALF_MEASURE",
                "FTS_SESSION_PRE_ROLL_SET_1_MEASURE",
                "FTS_SESSION_PRE_ROLL_SET_2_MEASURES",
            ]
        );
    }
}
