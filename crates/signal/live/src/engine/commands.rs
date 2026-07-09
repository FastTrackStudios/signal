//! Command dispatch and event streaming for rig control.
//!
//! `RigControlService` wraps the `RigEngine` with a command/event pattern:
//! - **Commands** are discrete actions (LoadPatch, ApplySnapshot, NextSong, etc.)
//! - **Events** are emitted after each command for reactive UI updates
//!
//! This decouples the UI from the engine's async internals — the UI sends
//! commands and subscribes to events.

use signal_proto::module_type::ModuleType;
use signal_proto::rig::RigSceneId;
use signal_proto::{ModulePresetId, ModuleSnapshotId};

use super::rig_engine::{PresetLoadHandle, TransitionResult};
use super::slot::InstanceState;

/// A command sent to the rig control service.
#[derive(Debug, Clone)]
pub enum RigControlCommand {
    /// Load a patch (profile entry) — resolves and transitions all slots.
    LoadPatch {
        profile_id: String,
        patch_id: String,
    },
    /// Load a specific song section — resolves scene for current rig.
    LoadSongSection { song_id: String, section_id: String },
    /// Apply a snapshot to a specific module slot (parameter changes only).
    ApplySnapshot {
        module_type: ModuleType,
        module_preset_id: ModulePresetId,
        snapshot_id: ModuleSnapshotId,
    },
    /// Preload a section for gapless transition.
    PreloadSection { song_id: String, section_id: String },
    /// Mute a specific slot.
    DisableSlot { module_type: ModuleType },
    /// Un-mute a specific slot.
    EnableSlot { module_type: ModuleType },
    /// Navigate to next song in the setlist.
    NextSong,
    /// Navigate to previous song in the setlist.
    PreviousSong,
    /// Navigate to next section in the current song.
    NextSection,
    /// Navigate to previous section in the current song.
    PreviousSection,
    /// Load a specific rig scene by ID.
    LoadScene { scene_id: RigSceneId },
    /// Set the morph position (0.0 = scene A, 1.0 = scene B).
    SetMorphPosition { position: f32 },
    /// Periodic maintenance tick (~60Hz).
    Tick,
}

/// An event emitted after a command is processed.
#[derive(Debug, Clone)]
pub enum RigControlEvent {
    /// A scene transition completed or is pending.
    SceneTransitioned {
        scene_id: Option<RigSceneId>,
        result: TransitionEventData,
    },
    /// A snapshot was applied to a slot.
    SnapshotApplied {
        module_type: ModuleType,
        snapshot_id: ModuleSnapshotId,
    },
    /// A slot's state changed.
    SlotStateChanged {
        module_type: ModuleType,
        new_state: InstanceState,
    },
    /// Slot was disabled.
    SlotDisabled { module_type: ModuleType },
    /// Slot was enabled.
    SlotEnabled { module_type: ModuleType },
    /// Navigated to a different song.
    SongChanged { song_id: String, song_name: String },
    /// Navigated to a different section.
    SectionChanged {
        section_id: String,
        section_name: String,
    },
    /// Morph position changed.
    MorphPositionChanged { position: f32 },
    /// Preload completed for a section.
    PreloadReady { handle: PresetLoadHandle },
    /// An error occurred during command processing.
    Error { error: String },
}

/// Simplified transition data for events (avoids cloning full TransitionResult).
#[derive(Debug, Clone)]
pub struct TransitionEventData {
    pub completed: bool,
    pub pending_handle: Option<PresetLoadHandle>,
    pub error_count: usize,
}

impl From<&TransitionResult> for TransitionEventData {
    fn from(result: &TransitionResult) -> Self {
        Self {
            completed: result.is_completed(),
            pending_handle: match &result.outcome {
                super::rig_engine::SwitchOutcome::Pending { handle, .. } => Some(*handle),
                _ => None,
            },
            error_count: result.slot_errors.len(),
        }
    }
}

/// Service trait for rig control — wraps engine with command/event pattern.
#[allow(async_fn_in_trait)]
pub trait RigControlService: Send + Sync {
    /// Execute a command and return resulting events.
    ///
    /// Returns a vec because some commands produce multiple events
    /// (e.g. LoadPatch may emit SceneTransitioned + multiple SlotStateChanged).
    async fn execute(&self, command: RigControlCommand) -> Vec<RigControlEvent>;
}

/// In-memory mock implementation of `RigControlService` for testing.
pub struct MockRigControlService {
    /// Recorded command history (for test assertions).
    history: std::sync::Mutex<Vec<RigControlCommand>>,
}

impl Default for MockRigControlService {
    fn default() -> Self {
        Self::new()
    }
}

impl MockRigControlService {
    pub fn new() -> Self {
        Self {
            history: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Get the command history for test assertions.
    pub fn history(&self) -> Vec<RigControlCommand> {
        self.history.lock().unwrap().clone()
    }

    /// Clear command history.
    pub fn clear_history(&self) {
        self.history.lock().unwrap().clear();
    }
}

impl RigControlService for MockRigControlService {
    async fn execute(&self, command: RigControlCommand) -> Vec<RigControlEvent> {
        let events = match &command {
            RigControlCommand::LoadPatch { .. } | RigControlCommand::LoadScene { .. } => {
                vec![RigControlEvent::SceneTransitioned {
                    scene_id: None,
                    result: TransitionEventData {
                        completed: true,
                        pending_handle: None,
                        error_count: 0,
                    },
                }]
            }
            RigControlCommand::ApplySnapshot {
                module_type,
                snapshot_id,
                ..
            } => {
                vec![RigControlEvent::SnapshotApplied {
                    module_type: *module_type,
                    snapshot_id: snapshot_id.clone(),
                }]
            }
            RigControlCommand::DisableSlot { module_type } => {
                vec![RigControlEvent::SlotDisabled {
                    module_type: *module_type,
                }]
            }
            RigControlCommand::EnableSlot { module_type } => {
                vec![RigControlEvent::SlotEnabled {
                    module_type: *module_type,
                }]
            }
            RigControlCommand::SetMorphPosition { position } => {
                vec![RigControlEvent::MorphPositionChanged {
                    position: *position,
                }]
            }
            RigControlCommand::Tick => vec![],
            _ => vec![],
        };

        self.history.lock().unwrap().push(command);
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_records_history() {
        let svc = MockRigControlService::new();
        svc.execute(RigControlCommand::NextSong).await;
        svc.execute(RigControlCommand::NextSection).await;
        assert_eq!(svc.history().len(), 2);
    }

    #[tokio::test]
    async fn load_patch_emits_scene_transitioned() {
        let svc = MockRigControlService::new();
        let events = svc
            .execute(RigControlCommand::LoadPatch {
                profile_id: "p1".into(),
                patch_id: "patch1".into(),
            })
            .await;
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            RigControlEvent::SceneTransitioned { .. }
        ));
    }

    #[tokio::test]
    async fn disable_slot_emits_event() {
        let svc = MockRigControlService::new();
        let events = svc
            .execute(RigControlCommand::DisableSlot {
                module_type: ModuleType::Amp,
            })
            .await;
        assert!(matches!(events[0], RigControlEvent::SlotDisabled { .. }));
    }

    #[tokio::test]
    async fn tick_emits_nothing() {
        let svc = MockRigControlService::new();
        let events = svc.execute(RigControlCommand::Tick).await;
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn morph_emits_position_event() {
        let svc = MockRigControlService::new();
        let events = svc
            .execute(RigControlCommand::SetMorphPosition { position: 0.75 })
            .await;
        assert!(matches!(
            events[0],
            RigControlEvent::MorphPositionChanged { position } if (position - 0.75).abs() < f32::EPSILON
        ));
    }
}
