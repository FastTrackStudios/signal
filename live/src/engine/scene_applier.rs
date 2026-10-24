//! Scene switch applier — bridges `LoadScene` commands to DAW FX operations.
//!
//! Given current and target module enabled sets (derived from `RigScene`
//! selections), computes the diff and applies enable/disable changes via
//! [`DawBridge`]. Also applies parameter overrides as a [`DawParameterSnapshot`].
//!
//! ## Design
//!
//! The applier is a **pure function pair**:
//! - `modules_for_scene` — derives which `ModuleType`s are active in a scene
//! - [`apply_scene_switch`] — diffs two sets and drives the `DawBridge`
//!
//! This keeps the logic testable in isolation: tests inject a `MockDawBridge`
//! and verify which modules were enabled/disabled.

use signal_proto::module_type::ModuleType;
use std::collections::HashSet;

use super::daw_bridge::DawBridge;
use super::morph::DawParameterSnapshot;

// region: --- SceneSwitchResult

/// Summary of what changed during a scene switch.
#[derive(Debug, Default)]
pub struct SceneSwitchResult {
    /// Module types that were **newly enabled** this switch.
    pub enabled: Vec<ModuleType>,
    /// Module types that were **newly disabled** this switch.
    pub disabled: Vec<ModuleType>,
    /// Whether a parameter snapshot was applied.
    pub params_applied: bool,
}

impl SceneSwitchResult {
    /// Returns `true` if any module state or parameter changed.
    #[must_use]
    pub const fn has_changes(&self) -> bool {
        !self.enabled.is_empty() || !self.disabled.is_empty() || self.params_applied
    }
}

// endregion: --- SceneSwitchResult

// region: --- apply_scene_switch

/// Apply a scene switch: diff two module-enabled sets and drive the bridge.
///
/// # Arguments
///
/// - `bridge` — DAW bridge to call for enable/disable and parameter changes.
/// - `track_id` — target track identifier.
/// - `current_enabled` — modules that are active **before** the switch.
/// - `target_enabled` — modules that should be active **after** the switch.
/// - `param_snapshot` — optional parameter snapshot to apply (e.g. scene overrides).
///
/// # Behaviour
///
/// Only modules whose enabled state *changes* are touched — no-ops are skipped.
/// Parameters are applied after enable/disable so that newly-enabled modules
/// receive their correct initial parameter values.
pub fn apply_scene_switch(
    bridge: &dyn DawBridge,
    track_id: &str,
    current_enabled: &HashSet<ModuleType>,
    target_enabled: &HashSet<ModuleType>,
    param_snapshot: Option<&DawParameterSnapshot>,
) -> SceneSwitchResult {
    let mut result = SceneSwitchResult::default();

    // Modules to disable: active now but not in target.
    for &mt in current_enabled {
        if !target_enabled.contains(&mt) {
            bridge.set_module_enabled(track_id, mt, false);
            result.disabled.push(mt);
        }
    }

    // Modules to enable: in target but not currently active.
    for &mt in target_enabled {
        if !current_enabled.contains(&mt) {
            bridge.set_module_enabled(track_id, mt, true);
            result.enabled.push(mt);
        }
    }

    // Apply parameter snapshot after enable/disable so newly-enabled modules
    // receive correct initial values.
    if let Some(snapshot) = param_snapshot {
        bridge.apply_parameters(track_id, snapshot);
        result.params_applied = true;
    }

    result
}

// endregion: --- apply_scene_switch

// region: --- tests

// endregion: --- tests
