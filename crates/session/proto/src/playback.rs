//! Transport playback — REAPER-facing action contract.
//!
//! The trait only; `session::playback::PlaybackActionsImpl<D>`
//! is the implementation. Same split as `track_manager`: the contract
//! (and the macro-emitted `ActionMeta` consts +
//! `register_playback_actions`) is protocol, so it lives in proto where
//! any host can see it without pulling in session's implementation.
//!
//! ## Why only these two transport actions
//!
//! The setlist navigation commands (`smart_next`, `next_song`,
//! `next_section`, …) are deliberately RPC-only. They route through
//! `SetlistServiceImpl::go_to_song_impl` / `go_to_section_impl`, which
//! depend on `ensure_song_hydrated` — a real async, timeout-bounded
//! rebuild path with multiple main-thread bounces. Collapsing that into
//! a synchronous REAPER action callback has no safe option: blocking on
//! the async work risks deadlocking the very main thread it needs, and a
//! "sync fast path that no-ops on a cache miss" would be a behavior
//! regression, not a refactor. They stay reachable through
//! `SetlistService::next_song` etc. over vox.
//!
//! `toggle_playback` / `toggle_song_loop` don't have that problem: each
//! is a single sync `Transport` call.

/// Play/pause and loop toggles for the current project.
///
/// `toggle_playback` operates on `ProjectContext::Current` rather than
/// the RPC path's "cached active song" lookup: a REAPER action fires in
/// the context of whatever project tab is focused, so `Current` is the
/// correct target for a hotkey (the cached-song indirection exists to
/// serve remote callers, who have no current tab of their own).
#[architect::actions(namespace = "FTS_SESSION")]
pub trait PlaybackActions {
    #[action(
        description = "Toggle play/pause for the current project",
        category = "Transport",
        group = "Transport"
    )]
    fn toggle_playback(&self);

    #[action(
        description = "Toggle looping for the current project",
        category = "Transport",
        group = "Transport"
    )]
    fn toggle_song_loop(&self);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generated ids must reproduce the exact REAPER command-id strings
    /// the legacy `session_actions` block used — anything already wired
    /// into a keybinding or menu depends on these exact strings.
    #[test]
    fn ids_match_existing_reaper_command_convention() {
        let ids: Vec<_> = PlaybackActionsActions::all()
            .iter()
            .map(|m| (m.method_name, m.id))
            .collect();
        assert_eq!(
            ids,
            vec![
                ("toggle_playback", "FTS_SESSION_TOGGLE_PLAYBACK"),
                ("toggle_song_loop", "FTS_SESSION_TOGGLE_SONG_LOOP"),
            ]
        );
    }
}
