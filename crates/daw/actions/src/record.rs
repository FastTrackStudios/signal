//! Recording-workflow actions.
//!
//! These exist alongside `take_ranking` and `mode_actions` and are
//! registered under the `fts.session.*` action namespace via the
//! `session_actions` `define_actions!` block in `crate::lib`.

use daw::service::record_control::{RecordActions, register_record_actions};
use daw::service::transport::service::Transport;
use daw::service::{InputMonitoringMode, ProjectContext, TrackRef};
use tracing::info;

/// REAPER native command IDs we call. Kept as named constants here so
/// the handlers below read like a story rather than a sequence of
/// magic numbers.
mod cmd {
    /// `Transport: Stop (DELETE all recorded media)` — abandons the
    /// in-flight recording pass and removes the media that was just
    /// captured. Used as the first half of `RestartRecording`.
    pub const STOP_DELETE: u32 = 40668;
    /// `Transport: Record` — starts a new recording pass with the
    /// current arm / monitor / input-quantize settings. Pressed again
    /// while recording, it stops — i.e. it toggles.
    pub const RECORD: u32 = 1013;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordAction {
    /// "Bad take, do it again" — stop and discard the just-captured
    /// media, then immediately start a new recording pass.
    RestartRecording,
    /// Toggle record monitor on selected tracks between **on (1)** and
    /// **off (0)** only — skips the auto/tape state that REAPER's
    /// native cycle action (40495) walks through. Semantics: if any
    /// selected track is currently in the "on" state, set all to off;
    /// otherwise set all to on.
    ToggleMonitorOnOff,
    /// Toggle record monitor on selected tracks between **auto/tape
    /// (2)** and **off (0)**. Same any/all semantics as
    /// [`Self::ToggleMonitorOnOff`].
    ToggleMonitorTapeOff,
    /// Start recording into the focused project (the current song's tab).
    Record,
    /// Stop the transport (keeps captured media).
    StopRecording,
    /// Toggle recording in the focused project.
    ToggleRecording,
    /// Arm the selected tracks (set `I_RECARM` = 1) in the focused project.
    ArmSelected,
    /// Disarm the selected tracks (set `I_RECARM` = 0) in the focused project.
    DisarmSelected,
}

pub fn dispatch(action: RecordAction) {
    let daw = daw::reaper::Reaper;
    match action {
        RecordAction::RestartRecording => restart_recording(),
        RecordAction::ToggleMonitorOnOff => {
            toggle_monitor_between(InputMonitoringMode::Normal, InputMonitoringMode::Off)
        }
        RecordAction::ToggleMonitorTapeOff => toggle_monitor_between(
            InputMonitoringMode::NotWhenPlaying,
            InputMonitoringMode::Off,
        ),
        RecordAction::Record => {
            let _ = daw.record(ProjectContext::Current);
            info!("[record] Record");
        }
        RecordAction::StopRecording => {
            let _ = daw.stop_recording(ProjectContext::Current);
            info!("[record] Stop");
        }
        RecordAction::ToggleRecording => {
            let _ = daw.toggle_recording(ProjectContext::Current);
            info!("[record] Toggle record");
        }
        RecordAction::ArmSelected => set_arm_selected(true),
        RecordAction::DisarmSelected => set_arm_selected(false),
    }
}

/// Arm/disarm every selected track in the focused project, in one undo block.
fn set_arm_selected(armed: bool) {
    use daw::service::{Projects as _, Tracks as _};

    let daw = daw::reaper::Reaper;
    let selected = daw.selected(ProjectContext::Current);
    if selected.is_empty() {
        info!("[record] arm: no tracks selected");
        return;
    }

    let label = if armed {
        "Arm selected tracks"
    } else {
        "Disarm selected tracks"
    };
    daw.begin_undo_block(ProjectContext::Current, label);
    for track in &selected {
        let _ = daw.set_armed(
            ProjectContext::Current,
            TrackRef::Guid(track.guid.clone()),
            armed,
        );
    }
    daw.end_undo_block(ProjectContext::Current, label, None);
    info!(
        armed,
        tracks = selected.len(),
        "[record] Set record arm on selected tracks"
    );
}

/// Toggle record-input monitoring on every selected track between `a` and `b`.
///
/// Semantic: if *any* selected track is currently set to `a`, push them all
/// to `b`; otherwise push them all to `a`. This avoids the per-track flip
/// that would leave a multi-track selection in mixed states and gives a
/// predictable "master switch" feel.
fn toggle_monitor_between(a: InputMonitoringMode, b: InputMonitoringMode) {
    use daw::service::{Projects as _, Tracks as _};

    let daw = daw::reaper::Reaper;
    let selected = daw.selected(ProjectContext::Current);
    if selected.is_empty() {
        return;
    }

    let any_at_a = selected.iter().any(|t| t.input_monitor == a);
    let target = if any_at_a { b } else { a };

    let label = format!(
        "Toggle record monitor ({} ↔ {})",
        monitor_label(a),
        monitor_label(b)
    );
    daw.begin_undo_block(ProjectContext::Current, &label);
    for track in &selected {
        let _ = daw.set_input_monitor(
            ProjectContext::Current,
            TrackRef::Guid(track.guid.clone()),
            target,
        );
    }
    daw.end_undo_block(ProjectContext::Current, &label, None);
    info!(
        tracks = selected.len(),
        new_state = monitor_label(target),
        "[record] Set record monitor"
    );
}

fn monitor_label(m: InputMonitoringMode) -> &'static str {
    match m {
        InputMonitoringMode::Off => "off",
        InputMonitoringMode::Normal => "on",
        InputMonitoringMode::NotWhenPlaying => "auto/tape",
    }
}

/// Stop the current recording (discarding anything captured this pass) and
/// immediately start a fresh recording pass, wrapped in a single undo block
/// so the history records one "Restart recording" entry.
fn restart_recording() {
    use daw::service::{ActionRegistration as _, Projects as _};

    let daw = daw::reaper::Reaper;
    daw.begin_undo_block(ProjectContext::Current, "Restart recording");
    daw.run_action(cmd::STOP_DELETE);
    daw.run_action(cmd::RECORD);
    daw.end_undo_block(ProjectContext::Current, "Restart recording", None);
    info!("[record] Restarted recording (stop+delete then record)");
}

// ── architect::actions implementation ───────────────────────────────────
//
// The action enum + `dispatch` below stay: they are the shared body every
// action method (and, where one exists, the RPC service impl) calls into.
// What's gone is the string-keyed `action_for_id` lookup and the
// `session_actions` `define_actions!` entries that declared the same
// `FTS_SESSION_*` command ids a second time.

/// Bridges the eight recording-workflow actions onto
/// `#[architect::actions]`. Every method forwards to the existing
/// synchronous `dispatch` — no behavior change, just a declarative front
/// door with real metadata.
pub struct RecordActionsImpl;

impl RecordActions for RecordActionsImpl {
    fn record(&self) {
        dispatch(RecordAction::Record);
    }
    fn record_stop(&self) {
        dispatch(RecordAction::StopRecording);
    }
    fn record_toggle(&self) {
        dispatch(RecordAction::ToggleRecording);
    }
    fn arm_selected(&self) {
        dispatch(RecordAction::ArmSelected);
    }
    fn disarm_selected(&self) {
        dispatch(RecordAction::DisarmSelected);
    }
    fn record_restart(&self) {
        dispatch(RecordAction::RestartRecording);
    }
    fn monitor_toggle_on_off(&self) {
        dispatch(RecordAction::ToggleMonitorOnOff);
    }
    fn monitor_toggle_tape_off(&self) {
        dispatch(RecordAction::ToggleMonitorTapeOff);
    }
}

/// Registers all eight recording-workflow actions with `backend`.
pub fn register_actions<B>(backend: &B)
where
    B: ::architect::action::ActionBackend + ?Sized,
{
    register_record_actions(backend, std::sync::Arc::new(RecordActionsImpl));
}

// ─── RPC service ───────────────────────────────────────────────────────

/// [`daw::service::RecordControlService`] over the same handlers the
/// keyboard actions use; each call bounces to REAPER's main thread.
#[derive(Clone, Default)]
pub struct RecordControlServiceImpl;

impl daw::service::RecordControlService for RecordControlServiceImpl {
    async fn restart_recording(&self) -> Result<(), daw::service::DawError> {
        dispatch_on_main(RecordAction::RestartRecording).await
    }

    async fn toggle_monitor_on_off(&self) -> Result<(), daw::service::DawError> {
        dispatch_on_main(RecordAction::ToggleMonitorOnOff).await
    }

    async fn toggle_monitor_tape_off(&self) -> Result<(), daw::service::DawError> {
        dispatch_on_main(RecordAction::ToggleMonitorTapeOff).await
    }
}

async fn dispatch_on_main(action: RecordAction) -> Result<(), daw::service::DawError> {
    daw::main_thread::query(move || dispatch(action))
        .await
        .ok_or(daw::service::DawError::MainThreadUnavailable)?;
    Ok(())
}
