//! Engine error types for slot and rig operations.

use signal_proto::{ModulePresetId, ModuleSnapshotId};
use std::fmt;

use super::slot::InstanceState;

/// Errors arising from slot or rig engine operations.
#[derive(Debug, Clone)]
pub enum EngineError {
    /// A requested plugin/FX was not found in the DAW.
    PluginNotFound { plugin_id: String },
    /// The slot for a module type hasn't been initialized yet.
    SlotNotInitialized { module_type: signal_proto::module_type::ModuleType },
    /// A state transition was attempted from an unexpected state.
    InvalidState {
        expected: InstanceState,
        actual: InstanceState,
    },
    /// Plugin load exceeded the timeout.
    LoadTimeout {
        module_preset_id: ModulePresetId,
        timeout_ms: u64,
    },
    /// Module preset not found in storage.
    ModulePresetNotFound { module_preset_id: ModulePresetId },
    /// Snapshot not found within a module preset.
    SnapshotNotFound { snapshot_id: ModuleSnapshotId },
    /// Generic backend/DAW error.
    Backend(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PluginNotFound { plugin_id } => {
                write!(f, "plugin not found: {plugin_id}")
            }
            Self::SlotNotInitialized { module_type } => {
                write!(f, "slot not initialized: {module_type:?}")
            }
            Self::InvalidState { expected, actual } => {
                write!(f, "invalid state: expected {expected:?}, got {actual:?}")
            }
            Self::LoadTimeout {
                module_preset_id,
                timeout_ms,
            } => {
                write!(f, "load timeout for {module_preset_id} after {timeout_ms}ms")
            }
            Self::ModulePresetNotFound { module_preset_id } => {
                write!(f, "module preset not found: {module_preset_id}")
            }
            Self::SnapshotNotFound { snapshot_id } => {
                write!(f, "snapshot not found: {snapshot_id}")
            }
            Self::Backend(msg) => write!(f, "backend error: {msg}"),
        }
    }
}

impl std::error::Error for EngineError {}
