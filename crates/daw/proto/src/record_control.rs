//! Recording — REAPER-facing action contract plus the RPC service.
//!
//! Both halves of the same domain, colocated: [`RecordActions`] is what a
//! REAPER hotkey binds, `record_control_service` is what a remote client
//! calls. `daw_actions::record` implements both over one shared
//! `dispatch`.

pub mod record_control_service {
    use crate::DawError;

    #[architect::rpc]
    pub trait RecordControlService {
        /// Stop the current recording (DELETE all recorded media this
        /// pass) and immediately start a fresh recording pass. One
        /// undo block.
        async fn restart_recording(&self) -> Result<(), DawError>;

        /// Toggle record monitor on selected tracks between **on (1)**
        /// and **off (0)** — skips auto/tape.
        async fn toggle_monitor_on_off(&self) -> Result<(), DawError>;

        /// Toggle record monitor on selected tracks between
        /// **auto/tape (2)** and **off (0)**.
        async fn toggle_monitor_tape_off(&self) -> Result<(), DawError>;
    }
}

#[cfg(feature = "vox")]
pub use record_control_service::RecordControlServiceClient;
pub use record_control_service::{
    RecordControlService, RecordControlServiceDispatcher, Service as RecordControlServiceLayer,
    layer as record_control_service_layer, record_control_service_rpc_service_descriptor,
    record_control_service_service_descriptor, serve as serve_record_control_service,
};

// ── Actions ─────────────────────────────────────────────────────────────

#[architect::actions(namespace = "FTS_SESSION")]
pub trait RecordActions {
    #[action(
        description = "Start a recording pass in the focused project — the current song's tab. Uses the existing arm / monitor / input settings.",
        category = "Transport",
        group = "Recording"
    )]
    fn record(&self);
    #[action(
        description = "Stop the transport in the focused project, keeping the media captured this pass.",
        category = "Transport",
        group = "Recording"
    )]
    fn record_stop(&self);
    #[action(
        description = "Toggle recording in the focused project — the current song's tab.",
        category = "Transport",
        group = "Recording"
    )]
    fn record_toggle(&self);
    #[action(
        description = "Arm every selected track (I_RECARM = 1) in the focused project so it captures input on the next recording pass.",
        category = "Tracks",
        group = "Recording"
    )]
    fn arm_selected(&self);
    #[action(
        description = "Disarm every selected track (I_RECARM = 0) in the focused project.",
        category = "Tracks",
        group = "Recording"
    )]
    fn disarm_selected(&self);
    #[action(
        description = "Stop the current recording (DELETE all recorded media this pass) and immediately start a fresh recording pass. For aborting a bad take without leaving stray media behind.",
        category = "Transport",
        group = "Recording"
    )]
    fn record_restart(&self);
    #[action(
        description = "Toggle the record-monitor state of every selected track between 'on' and 'off' only, skipping the auto/tape state that REAPER's native cycle action walks through. If any selected track is currently 'on', all go to off; otherwise all go to on.",
        category = "Tracks",
        group = "Recording"
    )]
    fn monitor_toggle_on_off(&self);
    #[action(
        description = "Toggle the record-monitor state of every selected track between 'auto/tape' (monitor input only while recording) and 'off'. If any selected track is currently 'auto/tape', all go to off; otherwise all go to auto/tape.",
        category = "Tracks",
        group = "Recording"
    )]
    fn monitor_toggle_tape_off(&self);
}

#[cfg(test)]
mod record_id_tests {
    use super::*;

    /// The exact REAPER command-name strings. These predate the move out
    /// of `daw-actions` and must survive it — keybindings, toolbars and
    /// `extension_loads.rs` all depend on them.
    #[test]
    fn ids_match_pre_move_command_ids() {
        let ids: Vec<_> = RecordActionsActions::all().iter().map(|m| m.id).collect();
        assert_eq!(
            ids,
            vec![
                "FTS_SESSION_RECORD",
                "FTS_SESSION_RECORD_STOP",
                "FTS_SESSION_RECORD_TOGGLE",
                "FTS_SESSION_ARM_SELECTED",
                "FTS_SESSION_DISARM_SELECTED",
                "FTS_SESSION_RECORD_RESTART",
                "FTS_SESSION_MONITOR_TOGGLE_ON_OFF",
                "FTS_SESSION_MONITOR_TOGGLE_TAPE_OFF",
            ]
        );
    }
}
