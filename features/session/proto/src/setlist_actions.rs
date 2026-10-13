//! Setlist build / demo / diagnostics — REAPER-facing action contract.
//!
//! The trait only; `session::setlist::actions::SetlistActionsImpl<D>` is
//! the implementation. Same split as `track_manager` / `playback` /
//! `mode`.
//!
//! Namespace is `FTS_SESSION` (not the macro's trait-derived default
//! `SETLIST`) so generated ids match the REAPER named-command
//! convention already in use — `FTS_SESSION_BUILD_SETLIST` etc.

#[architect::actions(namespace = "FTS_SESSION")]
pub trait SetlistActions {
    #[action(
        description = "Scan every open REAPER project tab, parse SONGSTART/SONGEND markers and section regions, and rebuild the cached Setlist",
        category = "Setlist",
        group = "Build"
    )]
    fn build_setlist(&self);

    #[action(
        description = "Stamp a 3-song demo setlist (markers + section regions) into the current project, then rebuild the cached Setlist",
        category = "Setlist",
        group = "Demo"
    )]
    fn load_demo_setlist(&self);

    #[action(
        description = "Log every marker and region in the current project with position, name, color and ruler lane index — diagnostic for lane-assignment issues",
        category = "Debug",
        group = "Diagnostics"
    )]
    fn dump_ruler_state(&self);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generated ids must reproduce the exact REAPER command-id strings
    /// the retired `session_actions` entries produced.
    #[test]
    fn ids_match_retired_session_actions_command_ids() {
        let ids: Vec<_> = SetlistActionsActions::all().iter().map(|m| m.id).collect();
        assert_eq!(
            ids,
            vec![
                "FTS_SESSION_BUILD_SETLIST",
                "FTS_SESSION_LOAD_DEMO_SETLIST",
                "FTS_SESSION_DUMP_RULER_STATE",
            ]
        );
    }
}
