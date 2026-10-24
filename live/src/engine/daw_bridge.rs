//! DAW FX chain snapshot types and capture/apply trait.
//!
//! Defines the data structures for capturing and restoring DAW plugin state:
//! - `DawStateChunk` — raw RPP/FX chain state data for a single plugin
//! - `DawSceneSnapshot` — combined parameter values + state chunks for a scene
//! - `DawFullPreset` — complete rig snapshot for A/B comparison
//! - `DawModulePreset` — per-module snapshot for saving/loading
//!
//! The `DawBridge` trait abstracts DAW-specific operations so the engine
//! can capture/apply state without coupling to a specific DAW API.

use super::morph::{DawParamValue, DawParameterSnapshot};
use serde::{Deserialize, Serialize};
use signal_proto::BlockType;
use std::collections::HashMap;

/// Raw state chunk data for a single FX plugin (RPP format in REAPER).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DawStateChunk {
    /// FX identifier (e.g. plugin GUID string).
    pub fx_id: String,
    /// Plugin name (for display).
    pub plugin_name: String,
    /// Functional category of the block (e.g. Eq, Drive, Reverb).
    pub block_type: BlockType,
    /// Raw state data (base64-encoded RPP chunk or equivalent).
    pub chunk_data: Vec<u8>,
}

/// A complete scene snapshot combining parameter values and state chunks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DawSceneSnapshot {
    /// Parameter values (lightweight, diffable, morphable).
    pub params: DawParameterSnapshot,
    /// Full state chunks (heavyweight, for exact recall).
    pub chunks: Vec<DawStateChunk>,
}

impl DawSceneSnapshot {
    #[must_use]
    pub const fn new(params: DawParameterSnapshot, chunks: Vec<DawStateChunk>) -> Self {
        Self { params, chunks }
    }

    #[must_use]
    pub const fn params_only(params: DawParameterSnapshot) -> Self {
        Self {
            params,
            chunks: Vec::new(),
        }
    }
}

/// A per-module snapshot (subset of the full rig).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DawModulePreset {
    /// Module type identifier.
    pub module_type: String,
    /// Human-readable name.
    pub name: String,
    /// Parameter snapshot for this module.
    pub params: DawParameterSnapshot,
    /// State chunks for this module's FX.
    pub chunks: Vec<DawStateChunk>,
}

/// A complete rig preset (all modules, for A/B comparison and saving).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DawFullPreset {
    /// Human-readable name.
    pub name: String,
    /// Scene snapshots keyed by scene ID.
    pub scenes: HashMap<String, DawSceneSnapshot>,
    /// Per-module presets.
    pub modules: Vec<DawModulePreset>,
}

/// Result of diffing two parameter snapshots.
#[derive(Debug, Clone)]
pub struct ParamDiff {
    pub fx_id: String,
    pub param_index: u32,
    pub param_name: String,
    pub value_a: f64,
    pub value_b: f64,
}

/// Diff two parameter snapshots to find which parameters differ.
#[must_use]
pub fn diff_parameter_snapshots(
    a: &DawParameterSnapshot,
    b: &DawParameterSnapshot,
) -> Vec<ParamDiff> {
    let b_lookup: HashMap<(&str, u32), &DawParamValue> = b
        .params
        .iter()
        .map(|p| ((p.fx_id.as_str(), p.param_index), p))
        .collect();

    let mut diffs = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for pa in &a.params {
        let key = (pa.fx_id.as_str(), pa.param_index);
        seen.insert((pa.fx_id.clone(), pa.param_index));

        let value_b = b_lookup.get(&key).map_or(0.0, |pb| pb.value);

        if (pa.value - value_b).abs() > f64::EPSILON {
            diffs.push(ParamDiff {
                fx_id: pa.fx_id.clone(),
                param_index: pa.param_index,
                param_name: pa.param_name.clone(),
                value_a: pa.value,
                value_b,
            });
        }
    }

    // Params only in B.
    for pb in &b.params {
        if !seen.contains(&(pb.fx_id.clone(), pb.param_index)) && pb.value.abs() > f64::EPSILON {
            diffs.push(ParamDiff {
                fx_id: pb.fx_id.clone(),
                param_index: pb.param_index,
                param_name: pb.param_name.clone(),
                value_a: 0.0,
                value_b: pb.value,
            });
        }
    }

    diffs
}

/// Remap FX GUIDs in a snapshot (for cross-track application).
pub fn remap_snapshot_guids(
    snapshot: &mut DawParameterSnapshot,
    guid_map: &HashMap<String, String>,
) {
    for param in &mut snapshot.params {
        if let Some(new_guid) = guid_map.get(&param.fx_id) {
            param.fx_id = new_guid.clone();
        }
    }
}

/// Remap FX GUIDs in state chunks (for cross-track application).
pub fn remap_chunk_guids(chunks: &mut [DawStateChunk], guid_map: &HashMap<String, String>) {
    for chunk in chunks.iter_mut() {
        if let Some(new_guid) = guid_map.get(&chunk.fx_id) {
            chunk.fx_id = new_guid.clone();
        }
    }
}

/// Trait abstracting DAW-specific FX chain operations.
///
/// Implementors provide the actual DAW API calls (e.g. REAPER's `GetFXChunk`,
/// `TrackFX_GetParam`, etc.). The engine calls these through the trait.
pub trait DawBridge: Send + Sync {
    /// Capture parameter values from the current FX chain.
    fn capture_parameters(&self, track_id: &str) -> DawParameterSnapshot;

    /// Capture full state chunks from the current FX chain.
    fn capture_state_chunks(&self, track_id: &str) -> Vec<DawStateChunk>;

    /// Apply parameter values to the FX chain.
    fn apply_parameters(&self, track_id: &str, snapshot: &DawParameterSnapshot);

    /// Apply state chunks to the FX chain.
    fn apply_state_chunks(&self, track_id: &str, chunks: &[DawStateChunk]);

    /// Build a GUID remapping table between two tracks' FX chains.
    fn build_guid_map(&self, source_track: &str, target_track: &str) -> HashMap<String, String>;

    /// Enable or disable a module by type (in REAPER: bypass the container FX).
    ///
    /// `enabled = true` means the module is active; `false` means bypassed/disabled.
    fn set_module_enabled(
        &self,
        track_id: &str,
        module_type: signal_proto::module_type::ModuleType,
        enabled: bool,
    );

    /// Query whether a module is currently enabled for a track.
    fn is_module_enabled(
        &self,
        track_id: &str,
        module_type: signal_proto::module_type::ModuleType,
    ) -> bool;
}

/// Mock DAW bridge for testing.
pub struct MockDawBridge {
    /// Stored snapshots keyed by track ID.
    snapshots: std::sync::Mutex<HashMap<String, DawParameterSnapshot>>,
    /// Enabled/disabled state per (`track_id`, `module_type`).
    ///
    /// Absent key = enabled (default). Explicit `false` = disabled.
    enabled_modules:
        std::sync::Mutex<HashMap<(String, signal_proto::module_type::ModuleType), bool>>,
}

impl Default for MockDawBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl MockDawBridge {
    #[must_use]
    pub fn new() -> Self {
        Self {
            snapshots: std::sync::Mutex::new(HashMap::new()),
            enabled_modules: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Pre-load a snapshot for a track (for test setup).
    pub fn set_snapshot(&self, track_id: &str, snapshot: DawParameterSnapshot) {
        if let Ok(mut s) = self.snapshots.lock() {
            s.insert(track_id.to_string(), snapshot);
        }
    }
}

impl DawBridge for MockDawBridge {
    fn capture_parameters(&self, track_id: &str) -> DawParameterSnapshot {
        self.snapshots
            .lock()
            .unwrap()
            .get(track_id)
            .cloned()
            .unwrap_or_default()
    }

    fn capture_state_chunks(&self, _track_id: &str) -> Vec<DawStateChunk> {
        Vec::new()
    }

    fn apply_parameters(&self, track_id: &str, snapshot: &DawParameterSnapshot) {
        self.snapshots
            .lock()
            .unwrap()
            .insert(track_id.to_string(), snapshot.clone());
    }

    fn apply_state_chunks(&self, _track_id: &str, _chunks: &[DawStateChunk]) {
        // Mock: no-op.
    }

    fn build_guid_map(&self, _source: &str, _target: &str) -> HashMap<String, String> {
        HashMap::new()
    }

    fn set_module_enabled(
        &self,
        track_id: &str,
        module_type: signal_proto::module_type::ModuleType,
        enabled: bool,
    ) {
        self.enabled_modules
            .lock()
            .unwrap()
            .insert((track_id.to_string(), module_type), enabled);
    }

    fn is_module_enabled(
        &self,
        track_id: &str,
        module_type: signal_proto::module_type::ModuleType,
    ) -> bool {
        self.enabled_modules
            .lock()
            .unwrap()
            .get(&(track_id.to_string(), module_type))
            .copied()
            .unwrap_or(true) // default: enabled
    }
}
