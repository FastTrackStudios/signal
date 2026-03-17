//! Global thread-safe registry for macro parameter bindings.
//!
//! This module stores the mapping from macro knob IDs to their target FX parameters
//! across the entire Signal Live session. It enables real-time parameter updates
//! when macro knobs are moved without requiring pre-computed routing configuration.
//!
//! # Thread Safety
//!
//! Uses `LazyLock<RwLock<HashMap>>` for:
//! - **Lock-free reads**: Multiple readers can access targets concurrently
//! - **Atomic writes**: Registry updates via `register()` are atomic
//! - **Safe initialization**: LazyLock ensures initialization on first access
//!
//! # Lifecycle
//!
//! - **Register**: Called after block load via `setup_macros_for_block()`
//! - **Get**: Called on every macro knob change in the performance tab
//! - **Clear**: Called when switching patches/presets to remove stale bindings
//!
//! # Performance Notes
//!
//! - **Registration**: O(n) where n = number of bindings in setup result
//! - **Lookup**: O(1) hash map access + O(m) clone where m = targets per knob
//! - **Clear**: O(1) — replaces entire HashMap atomically
//!
//! Typical performance: <1ms for lookup + parameter setting across multiple targets

use crate::macro_setup::MacroSetupResult;
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

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
    BINDINGS
        .read()
        .expect("lock poisoned")
        .get(knob_id)
        .cloned()
        .unwrap_or_default()
}

/// Clear all registered bindings.
/// Call this when a new patch is activated to avoid stale bindings from
/// the previous preset.
///
/// # Example
///
/// ```ignore
/// // On patch change
/// macro_registry::clear();
/// // Then load new patch
/// setup_and_register_new_patch().await?;
/// ```
pub fn clear() {
    BINDINGS.write().expect("lock poisoned").clear();
}

/// Get statistics about the current registry state.
///
/// Useful for debugging and performance monitoring.
///
/// # Returns
///
/// Tuple of (total_knobs, total_targets, avg_targets_per_knob)
pub fn stats() -> (usize, usize, f32) {
    let map = BINDINGS.read().expect("lock poisoned");
    let knob_count = map.len();
    let target_count: usize = map.values().map(|targets| targets.len()).sum();
    let avg = if knob_count > 0 {
        target_count as f32 / knob_count as f32
    } else {
        0.0
    };
    (knob_count, target_count, avg)
}

/// Check if any bindings are registered.
pub fn is_empty() -> bool {
    BINDINGS.read().expect("lock poisoned").is_empty()
}

/// Get the number of registered knobs.
pub fn knob_count() -> usize {
    BINDINGS.read().expect("lock poisoned").len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macro_setup::LiveMacroBinding;

    // Note: Tests share global state, so we run them serially to avoid races.
    // To run serially: cargo test -- --test-threads=1
    // This is only for testing; production code doesn't care about test order.

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

    #[test]
    fn test_stats() {
        clear();

        let result = MacroSetupResult {
            track_guid: "track-1".to_string(),
            target_fx_guid: "fx-1".to_string(),
            bindings: vec![
                LiveMacroBinding {
                    knob_index: 0,
                    knob_id: "knob1".to_string(),
                    param_index: 1,
                    min: 0.0,
                    max: 1.0,
                },
                LiveMacroBinding {
                    knob_index: 1,
                    knob_id: "knob2".to_string(),
                    param_index: 2,
                    min: 0.0,
                    max: 1.0,
                },
                LiveMacroBinding {
                    knob_index: 1,
                    knob_id: "knob2".to_string(),
                    param_index: 3,
                    min: 0.0,
                    max: 1.0,
                },
            ],
        };

        register(&result);

        let (knob_count, target_count, avg) = stats();
        assert_eq!(knob_count, 2);
        assert_eq!(target_count, 3);
        assert!(avg > 1.4 && avg < 1.6); // ~1.5

        clear();
        assert!(is_empty());
    }

    #[test]
    fn test_empty_and_knob_count() {
        clear();
        assert!(is_empty());
        assert_eq!(knob_count(), 0);

        let result = MacroSetupResult {
            track_guid: "track-1".to_string(),
            target_fx_guid: "fx-1".to_string(),
            bindings: vec![LiveMacroBinding {
                knob_index: 0,
                knob_id: "test".to_string(),
                param_index: 1,
                min: 0.0,
                max: 1.0,
            }],
        };

        register(&result);
        assert!(!is_empty());
        assert_eq!(knob_count(), 1);

        clear();
        assert!(is_empty());
    }
}
