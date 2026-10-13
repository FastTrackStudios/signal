//! Setlist navigation — declaration only, no REAPER handler.
//!
//! These six are deliberately not implemented as sync REAPER action
//! callbacks. They route through `SetlistServiceImpl::go_to_song_impl` /
//! `go_to_section_impl`, which depend on `ensure_song_hydrated`'s async,
//! timeout-bounded rebuild path. Collapsing that into a sync callback means
//! either blocking the main thread on async work (deadlock risk) or adding a
//! sync fast-path that silently no-ops on a cache miss. Reachable today only
//! via `SetlistService`'s async RPC methods — CLI, desktop and web clients.
//!
//! They stay declared so the ids exist for those clients and for keymaps,
//! and so the command palette can list them. `session::daw_module` registers
//! them with REAPER for discoverability; triggering one there logs that no
//! DAW handler is registered, which is the truth.
//!
//! The keymap owns *when* these are live: `keymap.styx`'s `keymap_context`
//! binds them under `when: "tab:performance"`. That is where context is
//! actually evaluated, so it is not restated here.
//!
//! Namespace is `FTS_SESSION` so generated ids match the REAPER
//! named-command convention already in use.

#[architect::actions(namespace = "FTS_SESSION")]
pub trait NavigationActions {
    #[action(
        description = "Go to next section, or next song if at last section",
        category = "Session",
        group = "Navigate",
        shortcut = "Right"
    )]
    fn smart_next(&self);

    #[action(
        description = "Go to previous section, or previous song if at first section",
        category = "Session",
        group = "Navigate",
        shortcut = "Left"
    )]
    fn smart_previous(&self);

    #[action(
        description = "Go to the next song in the setlist",
        category = "Session",
        group = "Navigate",
        shortcut = "Down"
    )]
    fn next_song(&self);

    #[action(
        description = "Go to the previous song in the setlist",
        category = "Session",
        group = "Navigate",
        shortcut = "Up"
    )]
    fn previous_song(&self);

    #[action(
        description = "Go to the next section in the current song",
        category = "Session",
        group = "Navigate"
    )]
    fn next_section(&self);

    #[action(
        description = "Go to the previous section in the current song",
        category = "Session",
        group = "Navigate"
    )]
    fn previous_section(&self);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generated ids must reproduce the exact REAPER command-id strings
    /// the retired `session_actions` entries produced.
    #[test]
    fn ids_match_retired_session_actions_command_ids() {
        let ids: Vec<_> = NavigationActionsActions::all()
            .iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(
            ids,
            vec![
                "FTS_SESSION_SMART_NEXT",
                "FTS_SESSION_SMART_PREVIOUS",
                "FTS_SESSION_NEXT_SONG",
                "FTS_SESSION_PREVIOUS_SONG",
                "FTS_SESSION_NEXT_SECTION",
                "FTS_SESSION_PREVIOUS_SECTION",
            ]
        );
    }
}
