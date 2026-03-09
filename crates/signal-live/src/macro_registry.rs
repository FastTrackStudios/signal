//! Global thread-safe registry for macro parameter bindings.
//!
//! Stores the mapping from macro knob IDs to their target FX parameters
//! across the entire Signal Live session. Used by the performance tab
//! to look up which plugin parameters to drive when macro knobs change.

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};
use crate::macro_setup::MacroSetupResult;

/// Target FX parameter for a macro knob.
#[derive(Clone, Debug)]
pub struct MacroParamTarget {
    /// GUID of the track containing the target FX.
    pub track_guid: String,
    /// GUID of the target FX plugin.
    pub fx_guid: String,
    /// Concrete FX parameter index.
    pub param_index: u32,
    /// Minimum parameter value (normalized 0.0–1.0).
    pub min: f32,
    /// Maximum parameter value (normalized 0.0–1.0).
    pub max: f32,
}

/// Global macro binding registry.
/// Maps knob IDs to lists of parameter targets they drive.
/// Multiple blocks can share a macro, so each knob can drive multiple targets.
static BINDINGS: LazyLock<RwLock<HashMap<String, Vec<MacroParamTarget>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Register all bindings from a MacroSetupResult into the global registry.
///
/// Merges new bindings into existing ones (if a knob already has targets,
/// new targets are added). This allows multiple blocks to share the same macro knob.
pub fn register(result: &MacroSetupResult) {
    let mut map = BINDINGS.write().expect("lock poisoned");
    for binding in &result.bindings {
        let targets = map.entry(binding.knob_id.clone()).or_default();
        targets.push(MacroParamTarget {
            track_guid: result.track_guid.clone(),
            fx_guid: result.target_fx_guid.clone(),
            param_index: binding.param_index,
            min: binding.min,
            max: binding.max,
        });
    }
}

/// Get all parameter targets for a macro knob.
/// Returns an empty vector if the knob has no registered targets.
pub fn get_targets(knob_id: &str) -> Vec<MacroParamTarget> {
    BINDINGS.read()
        .expect("lock poisoned")
        .get(knob_id)
        .cloned()
        .unwrap_or_default()
}

/// Clear all registered bindings.
/// Call this when a new patch is activated to avoid stale bindings from
/// the previous preset.
pub fn clear() {
    BINDINGS.write().expect("lock poisoned").clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macro_setup::LiveMacroBinding;

    #[test]
    fn test_register_and_get() {
        clear(); // Start fresh

        let result = MacroSetupResult {
            track_guid: "track-1".to_string(),
            target_fx_guid: "fx-1".to_string(),
            bindings: vec![
                LiveMacroBinding {
                    knob_index: 0,
                    knob_id: "test_drive".to_string(),
                    param_index: 5,
                    min: 0.0,
                    max: 1.0,
                },
                LiveMacroBinding {
                    knob_index: 1,
                    knob_id: "test_tone".to_string(),
                    param_index: 10,
                    min: 0.0,
                    max: 1.0,
                },
            ],
        };

        register(&result);

        let drive_targets = get_targets("test_drive");
        assert_eq!(drive_targets.len(), 1);
        assert_eq!(drive_targets[0].param_index, 5);

        let tone_targets = get_targets("test_tone");
        assert_eq!(tone_targets.len(), 1);
        assert_eq!(tone_targets[0].param_index, 10);

        let missing_targets = get_targets("missing");
        assert!(missing_targets.is_empty());

        clear();
    }

    #[test]
    fn test_multiple_targets_per_knob() {
        clear();

        // First block drives "multi" to param 5
        let result1 = MacroSetupResult {
            track_guid: "track-1".to_string(),
            target_fx_guid: "fx-1".to_string(),
            bindings: vec![LiveMacroBinding {
                knob_index: 0,
                knob_id: "multi".to_string(),
                param_index: 5,
                min: 0.0,
                max: 1.0,
            }],
        };

        // Second block drives "multi" to param 12 on a different track
        let result2 = MacroSetupResult {
            track_guid: "track-2".to_string(),
            target_fx_guid: "fx-2".to_string(),
            bindings: vec![LiveMacroBinding {
                knob_index: 0,
                knob_id: "multi".to_string(),
                param_index: 12,
                min: 0.0,
                max: 1.0,
            }],
        };

        register(&result1);
        register(&result2);

        let multi_targets = get_targets("multi");
        assert_eq!(multi_targets.len(), 2);
        assert_eq!(multi_targets[0].param_index, 5);
        assert_eq!(multi_targets[1].param_index, 12);

        clear();
    }

    #[test]
    fn test_clear() {
        clear();

        let result = MacroSetupResult {
            track_guid: "track-1".to_string(),
            target_fx_guid: "fx-1".to_string(),
            bindings: vec![LiveMacroBinding {
                knob_index: 0,
                knob_id: "test_clear_knob".to_string(),
                param_index: 5,
                min: 0.0,
                max: 1.0,
            }],
        };

        register(&result);
        assert!(!get_targets("test_clear_knob").is_empty());

        clear();
        assert!(get_targets("test_clear_knob").is_empty());
    }
}
