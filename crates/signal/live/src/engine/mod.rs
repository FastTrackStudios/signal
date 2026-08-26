//! Runtime engine for gapless scene transitions and parameter morphing.
//!
//! ## Architecture
//!
//! - [`slot`] — Per-module-type instance management (Loading → Ready → Active → Tailing → Unloaded)
//! - [`target`] — Resolution targets bridging the scene resolver and diff engine
//! - [`diff`] — Pure-function diff engine computing minimal per-slot transitions
//! - [`morph`] — Parameter interpolation between two DAW snapshots
//! - [`rig_engine`] — Top-level orchestrator trait for all module slots
//! - [`commands`] — Command dispatch and event streaming
//! - [`daw_bridge`] — DAW FX chain snapshot types and capture/apply
//! - [`param_bridge`] — Signal domain Block/Graph → DawParameterSnapshot mapping
//! - [`vst_bridge`] — VST parameter bridge for bidirectional sync
//! - [`modulation`] — Real-time modulation runtime (LFO, envelope, MIDI CC → DAW params)
//! - [`error`] — Engine error types

pub mod commands;
pub mod daw_bridge;
pub mod diff;
pub mod error;
pub mod fx_binding;
pub mod gapless;
pub mod mock;
pub mod modulation;
pub mod morph;
pub mod param_bridge;
pub mod patch_applier;
pub mod rig_engine;
pub mod rig_scene_applier;
pub mod scene_applier;
pub mod slot;
pub mod snapshot_ops;
pub mod target;
pub mod vst_bridge;

// Re-export primary types for convenience.
pub use commands::{MockRigControlService, RigControlCommand, RigControlEvent, RigControlService};
pub use daw_bridge::{
    DawBridge, DawFullPreset, DawModulePreset, DawSceneSnapshot, DawStateChunk, MockDawBridge,
};
pub use diff::{compute_diff, SlotDiff};
pub use error::EngineError;
pub use fx_binding::{DiscoveredFx, DiscoveredModule, DiscoveredRig, FxRigBinding};
pub use gapless::{GaplessSwapEngine, SwapConfig, SwapResult};
pub use mock::MockRigEngine;
pub use modulation::{ModulationRuntime, ParamBinding, ParamWrite};
pub use morph::{
    DawParamValue, DawParameterSnapshot, MorphDiffEntry, MorphEngine, MorphParamChange,
};
pub use param_bridge::{
    block_to_snapshot, find_param_index, graph_state_chunks, graph_to_snapshot,
    live_params_into_block, param_name_matches, LiveParam,
};
pub use patch_applier::{DawPatchApplier, PatchApplyError};
pub use rig_engine::{
    PreloadPriority, PresetLoadHandle, PresetReadiness, RigEngine, SnapshotTween, SwitchOutcome,
    TransitionResult, TweenState,
};
pub use rig_scene_applier::{RigSceneApplier, RigSceneApplyError};
pub use scene_applier::{apply_scene_switch, SceneSwitchResult};
pub use slot::{ActivateResult, InstanceHandle, InstanceState, LoadResult, ModuleSlot};
pub use snapshot_ops::{
    capture_and_save_preset, capture_and_save_snapshot, recall_preset, recall_snapshot,
    SnapshotError,
};
pub use target::{ModuleTarget, ResolvedSlot, SlotState};
pub use vst_bridge::{MockVstBridge, ParameterSyncManager, VstParameterBridge};
