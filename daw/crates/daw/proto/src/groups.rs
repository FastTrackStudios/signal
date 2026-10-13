//! Track grouping over the canonical 128-slot partition — REAPER-facing
//! action contract.
//!
//! The trait only; `daw_actions::groups` is the implementation.

#[architect::actions(namespace = "FTS_SESSION")]
pub trait GroupActions {
    #[action(
        description = "Name the project's 128 track groups by the FTS instrument partition (Drums 1-10, Bass 11-20, Electric Gtr 21-40, Acoustic Gtr 41-60, Keys 61-70, Synths 71-80, Lead Vocal 81-100, Background Vox 101-120, Spare 121-128).",
        category = "Tracks",
        group = "Track Groups"
    )]
    fn group_apply_naming(&self);
    #[action(
        description = "Add the selected tracks to the next free Drums group slot as a mutual group (all flag families).",
        category = "Tracks",
        group = "Track Groups"
    )]
    fn group_assign_drums(&self);
    #[action(
        description = "Add the selected tracks to the next free Bass group slot as a mutual group.",
        category = "Tracks",
        group = "Track Groups"
    )]
    fn group_assign_bass(&self);
    #[action(
        description = "Add the selected tracks to the next free Electric Gtr group slot as a mutual group.",
        category = "Tracks",
        group = "Track Groups"
    )]
    fn group_assign_electric_gtr(&self);
    #[action(
        description = "Add the selected tracks to the next free Acoustic Gtr group slot as a mutual group.",
        category = "Tracks",
        group = "Track Groups"
    )]
    fn group_assign_acoustic_gtr(&self);
    #[action(
        description = "Add the selected tracks to the next free Keys group slot as a mutual group.",
        category = "Tracks",
        group = "Track Groups"
    )]
    fn group_assign_keys(&self);
    #[action(
        description = "Add the selected tracks to the next free Synths group slot as a mutual group.",
        category = "Tracks",
        group = "Track Groups"
    )]
    fn group_assign_synths(&self);
    #[action(
        description = "Add the selected tracks to the next free Lead Vocal group slot as a mutual group.",
        category = "Tracks",
        group = "Track Groups"
    )]
    fn group_assign_lead_vocal(&self);
    #[action(
        description = "Add the selected tracks to the next free Background Vox group slot as a mutual group.",
        category = "Tracks",
        group = "Track Groups"
    )]
    fn group_assign_background_vox(&self);
}

#[cfg(test)]
mod group_id_tests {
    use super::*;

    /// The exact REAPER command-name strings. These predate the move out
    /// of `daw-actions` and must survive it — keybindings, toolbars and
    /// `extension_loads.rs` all depend on them.
    #[test]
    fn ids_match_pre_move_command_ids() {
        let ids: Vec<_> = GroupActionsActions::all().iter().map(|m| m.id).collect();
        assert_eq!(
            ids,
            vec![
                "FTS_SESSION_GROUP_APPLY_NAMING",
                "FTS_SESSION_GROUP_ASSIGN_DRUMS",
                "FTS_SESSION_GROUP_ASSIGN_BASS",
                "FTS_SESSION_GROUP_ASSIGN_ELECTRIC_GTR",
                "FTS_SESSION_GROUP_ASSIGN_ACOUSTIC_GTR",
                "FTS_SESSION_GROUP_ASSIGN_KEYS",
                "FTS_SESSION_GROUP_ASSIGN_SYNTHS",
                "FTS_SESSION_GROUP_ASSIGN_LEAD_VOCAL",
                "FTS_SESSION_GROUP_ASSIGN_BACKGROUND_VOX",
            ]
        );
    }
}
