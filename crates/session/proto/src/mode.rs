//! Session modes — REAPER-facing action contract.
//!
//! The trait only; `session::modes::ModeActionsImpl` is the
//! implementation. Same split as `track_manager` / `playback`: the
//! contract and the macro-emitted `ActionMeta` consts +
//! `register_mode_actions` are protocol.
//!
//! The `Mode` enum itself stays in the `session` crate — it carries
//! layout / toolbar logic that doesn't belong in proto. On the wire
//! (see `services::mode`) modes are the stable lowercase slug.

/// The twenty-one mode actions: ten mode switches, ten
/// save-layout-to-screenset variants, and a debug logger.
///
/// Namespace `FTS_SESSION_MODE` renders these as `FTS: Mode - <Name>`
/// rather than nested under Session.
#[architect::actions(namespace = "FTS_SESSION_MODE")]
pub trait ModeActions {
    #[action(
        description = "Switch to Organize mode (planning, song structure, setlists)",
        category = "Session",
        group = "Switch"
    )]
    fn organize(&self);
    #[action(
        description = "Switch to Write mode (lyric/melody/idea capture)",
        category = "Session",
        group = "Switch"
    )]
    fn write(&self);
    #[action(
        description = "Switch to Produce mode (arrangement, sound design, instrument selection)",
        category = "Session",
        group = "Switch"
    )]
    fn produce(&self);
    #[action(
        description = "Switch to Record mode (tracking, takes, monitoring)",
        category = "Session",
        group = "Switch"
    )]
    fn record(&self);
    #[action(
        description = "Switch to Edit mode (comping, timing, cleanup)",
        category = "Session",
        group = "Switch"
    )]
    fn edit(&self);
    #[action(
        description = "Switch to Mix mode (mixer focus, processing, automation)",
        category = "Session",
        group = "Switch"
    )]
    fn mix(&self);
    #[action(
        description = "Switch to Master mode (master bus processing, metering, export prep)",
        category = "Session",
        group = "Switch"
    )]
    fn master(&self);
    #[action(
        description = "Switch to Live mode (performance/setlist playback view)",
        category = "Session",
        group = "Switch"
    )]
    fn live(&self);
    #[action(
        description = "Switch to Video mode (sync to picture / video editing layout)",
        category = "Session",
        group = "Switch"
    )]
    fn video(&self);
    #[action(
        description = "Switch to Scoring mode (multi-agent orchestration layout, no mode toolbars)",
        category = "Session",
        group = "Switch"
    )]
    fn scoring(&self);
    #[action(
        description = "Capture current REAPER window state to Organize's native screenset slot",
        category = "Session",
        group = "Save"
    )]
    fn save_organize(&self);
    #[action(
        description = "Capture current REAPER window state to Write's native screenset slot",
        category = "Session",
        group = "Save"
    )]
    fn save_write(&self);
    #[action(
        description = "Capture current REAPER window state to Produce's native screenset slot",
        category = "Session",
        group = "Save"
    )]
    fn save_produce(&self);
    #[action(
        description = "Capture current REAPER window state to Record's native screenset slot",
        category = "Session",
        group = "Save"
    )]
    fn save_record(&self);
    #[action(
        description = "Capture current REAPER window state to Edit's native screenset slot",
        category = "Session",
        group = "Save"
    )]
    fn save_edit(&self);
    #[action(
        description = "Capture current REAPER window state to Mix's native screenset slot",
        category = "Session",
        group = "Save"
    )]
    fn save_mix(&self);
    #[action(
        description = "Capture current REAPER window state to Master's native screenset slot",
        category = "Session",
        group = "Save"
    )]
    fn save_master(&self);
    #[action(
        description = "Capture current REAPER window state to Live's native screenset slot",
        category = "Session",
        group = "Save"
    )]
    fn save_live(&self);
    #[action(
        description = "Capture current REAPER window state to Video's native screenset slot",
        category = "Session",
        group = "Save"
    )]
    fn save_video(&self);
    #[action(
        description = "Capture current REAPER window state to Scoring's native screenset slot",
        category = "Session",
        group = "Save"
    )]
    fn save_scoring(&self);
    #[action(
        description = "Log the current session mode to the console (debug helper)",
        category = "Session",
        group = "Debug"
    )]
    fn log_current(&self);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generated ids must reproduce the exact REAPER command-id strings
    /// the retired `mode_defs` block used (prefix `fts.session.mode`) —
    /// keybindings, toolbars and `reaper-menu.ini` entries depend on
    /// these exact strings.
    #[test]
    fn ids_match_retired_mode_defs_command_ids() {
        let ids: Vec<_> = ModeActionsActions::all().iter().map(|m| m.id).collect();
        assert_eq!(
            ids,
            vec![
                "FTS_SESSION_MODE_ORGANIZE",
                "FTS_SESSION_MODE_WRITE",
                "FTS_SESSION_MODE_PRODUCE",
                "FTS_SESSION_MODE_RECORD",
                "FTS_SESSION_MODE_EDIT",
                "FTS_SESSION_MODE_MIX",
                "FTS_SESSION_MODE_MASTER",
                "FTS_SESSION_MODE_LIVE",
                "FTS_SESSION_MODE_VIDEO",
                "FTS_SESSION_MODE_SCORING",
                "FTS_SESSION_MODE_SAVE_ORGANIZE",
                "FTS_SESSION_MODE_SAVE_WRITE",
                "FTS_SESSION_MODE_SAVE_PRODUCE",
                "FTS_SESSION_MODE_SAVE_RECORD",
                "FTS_SESSION_MODE_SAVE_EDIT",
                "FTS_SESSION_MODE_SAVE_MIX",
                "FTS_SESSION_MODE_SAVE_MASTER",
                "FTS_SESSION_MODE_SAVE_LIVE",
                "FTS_SESSION_MODE_SAVE_VIDEO",
                "FTS_SESSION_MODE_SAVE_SCORING",
                "FTS_SESSION_MODE_LOG_CURRENT",
            ]
        );
    }
}
