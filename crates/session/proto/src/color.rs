//! Auto-colour — REAPER-facing action contract.
//!
//! The trait only; `session::color` is the implementation. Same split as
//! the other action contracts in this crate.
//!
//! Colour lives in the session domain because it is about to grow past
//! "name matches a rule, paint it": section-aware and setlist-aware
//! colouring need song structure, which only session has.

#[architect::actions(namespace = "FTS_SESSION")]
pub trait AutoColorActions {
    #[action(
        description = "Apply session auto-color rules to all tracks and keep auto-color enabled",
        category = "Session",
        group = "Auto Color"
    )]
    fn auto_color_color_all(&self);
    #[action(
        description = "Apply session auto-color rules to selected tracks",
        category = "Session",
        group = "Auto Color"
    )]
    fn auto_color_color_selected(&self);
    #[action(
        description = "Toggle session auto-color for all tracks",
        category = "Session",
        group = "Auto Color"
    )]
    fn auto_color_toggle(&self);
    #[action(
        description = "Clear colors from all tracks and disable session auto-color",
        category = "Session",
        group = "Auto Color"
    )]
    fn auto_color_clear_all(&self);
    #[action(
        description = "Clear colors from selected tracks",
        category = "Session",
        group = "Auto Color"
    )]
    fn auto_color_clear_selected(&self);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// These ids predate the move from `daw-actions` and must survive it —
    /// `apps/extensions/reaper-fts-extensions/tests/extension_loads.rs`
    /// asserts REAPER has them.
    #[test]
    fn ids_match_pre_move_command_ids() {
        let ids: Vec<_> = AutoColorActionsActions::all()
            .iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(
            ids,
            vec![
                "FTS_SESSION_AUTO_COLOR_COLOR_ALL",
                "FTS_SESSION_AUTO_COLOR_COLOR_SELECTED",
                "FTS_SESSION_AUTO_COLOR_TOGGLE",
                "FTS_SESSION_AUTO_COLOR_CLEAR_ALL",
                "FTS_SESSION_AUTO_COLOR_CLEAR_SELECTED",
            ]
        );
    }
}
