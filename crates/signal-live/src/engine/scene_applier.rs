//! Scene switch applier — bridges `LoadScene` commands to DAW FX operations.
//!
//! Given current and target module enabled sets (derived from `RigScene`
//! selections), computes the diff and applies enable/disable changes via
//! [`DawBridge`]. Also applies parameter overrides as a [`DawParameterSnapshot`].
//!
//! ## Design
//!
//! The applier is a **pure function pair**:
//! - [`modules_for_scene`] — derives which `ModuleType`s are active in a scene
//! - [`apply_scene_switch`] — diffs two sets and drives the `DawBridge`
//!
//! This keeps the logic testable in isolation: tests inject a [`MockDawBridge`]
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
    pub fn has_changes(&self) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::daw_bridge::MockDawBridge;
    use crate::engine::morph::{DawParamValue, DawParameterSnapshot};

    fn set(types: &[ModuleType]) -> HashSet<ModuleType> {
        types.iter().copied().collect()
    }

    fn param(fx: &str, idx: u32, val: f64) -> DawParamValue {
        DawParamValue {
            fx_id: fx.into(),
            param_index: idx,
            param_name: format!("Param {idx}"),
            value: val,
        }
    }

    // --- enable/disable diffing ---

    #[test]
    fn dry_to_ambient_enables_modules() {
        let bridge = MockDawBridge::new();
        let dry = set(&[ModuleType::Amp]);
        let ambient = set(&[ModuleType::Amp, ModuleType::Modulation, ModuleType::Time, ModuleType::Motion]);

        let result = apply_scene_switch(&bridge, "track-1", &dry, &ambient, None);

        // Three modules should have been enabled.
        assert_eq!(result.enabled.len(), 3);
        assert!(result.enabled.contains(&ModuleType::Modulation));
        assert!(result.enabled.contains(&ModuleType::Time));
        assert!(result.enabled.contains(&ModuleType::Motion));
        assert!(result.disabled.is_empty());

        // Bridge should reflect the new state.
        assert!(bridge.is_module_enabled("track-1", ModuleType::Modulation));
        assert!(bridge.is_module_enabled("track-1", ModuleType::Time));
        assert!(bridge.is_module_enabled("track-1", ModuleType::Motion));
        assert!(!result.params_applied);
    }

    #[test]
    fn ambient_to_dry_disables_modules() {
        let bridge = MockDawBridge::new();
        let ambient = set(&[ModuleType::Amp, ModuleType::Modulation, ModuleType::Time, ModuleType::Motion]);
        let dry = set(&[ModuleType::Amp]);

        let result = apply_scene_switch(&bridge, "track-1", &ambient, &dry, None);

        assert_eq!(result.disabled.len(), 3);
        assert!(result.disabled.contains(&ModuleType::Modulation));
        assert!(result.disabled.contains(&ModuleType::Time));
        assert!(result.disabled.contains(&ModuleType::Motion));
        assert!(result.enabled.is_empty());

        // Bridge reflects disabled state.
        assert!(!bridge.is_module_enabled("track-1", ModuleType::Modulation));
        assert!(!bridge.is_module_enabled("track-1", ModuleType::Time));
        assert!(!bridge.is_module_enabled("track-1", ModuleType::Motion));
    }

    #[test]
    fn no_change_when_same_modules() {
        let bridge = MockDawBridge::new();
        let modules = set(&[ModuleType::Amp, ModuleType::Drive]);

        let result = apply_scene_switch(&bridge, "track-1", &modules, &modules, None);

        assert!(result.enabled.is_empty());
        assert!(result.disabled.is_empty());
        assert!(!result.has_changes());
    }

    #[test]
    fn parameter_snapshot_applied_after_enable() {
        let bridge = MockDawBridge::new();
        let current = set(&[ModuleType::Amp]);
        let target = set(&[ModuleType::Amp, ModuleType::Modulation]);
        let snapshot = DawParameterSnapshot::new(vec![param("mod-fx", 0, 0.75)]);

        let result = apply_scene_switch(&bridge, "track-1", &current, &target, Some(&snapshot));

        assert!(result.params_applied);
        // Verify params were applied to bridge.
        let captured = bridge.capture_parameters("track-1");
        assert_eq!(captured.params.len(), 1);
        assert!((captured.params[0].value - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn bypass_state_reflected_in_bridge() {
        let bridge = MockDawBridge::new();

        // Initially, all modules report enabled (default).
        assert!(bridge.is_module_enabled("track-1", ModuleType::Time));

        // Switch to a scene that disables Time.
        let before = set(&[ModuleType::Amp, ModuleType::Time]);
        let after = set(&[ModuleType::Amp]);
        apply_scene_switch(&bridge, "track-1", &before, &after, None);

        // Time should now be disabled in the bridge.
        assert!(!bridge.is_module_enabled("track-1", ModuleType::Time));
        // Amp was unaffected.
        assert!(bridge.is_module_enabled("track-1", ModuleType::Amp));
    }

    #[test]
    fn switch_only_affects_specified_track() {
        let bridge = MockDawBridge::new();
        let before = set(&[ModuleType::Amp, ModuleType::Modulation]);
        let after = set(&[ModuleType::Amp]);

        apply_scene_switch(&bridge, "track-1", &before, &after, None);

        // track-2 is unaffected — Modulation still reports default (enabled).
        assert!(bridge.is_module_enabled("track-2", ModuleType::Modulation));
        // track-1 has the change.
        assert!(!bridge.is_module_enabled("track-1", ModuleType::Modulation));
    }

    #[test]
    fn result_has_changes_when_modules_changed() {
        let bridge = MockDawBridge::new();
        let result = apply_scene_switch(
            &bridge,
            "track-1",
            &set(&[ModuleType::Amp]),
            &set(&[ModuleType::Amp, ModuleType::Drive]),
            None,
        );
        assert!(result.has_changes());
    }
}

// endregion: --- tests
