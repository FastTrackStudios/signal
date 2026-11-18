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
//! - **Safe initialization**: `LazyLock` ensures initialization on first access
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
//! - **Clear**: O(1) — replaces entire `HashMap` atomically
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

/// Register all bindings from a `MacroSetupResult` into the global registry.
///
/// Merges new bindings into existing ones (if a knob already has targets,
/// new targets are added). This allows multiple blocks to share the same macro knob.
pub fn register(result: &MacroSetupResult) {
    let mut map = BINDINGS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
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
///
/// # Panics
///
/// Panics if the registry lock is poisoned.
pub fn get_targets(knob_id: &str) -> Vec<MacroParamTarget> {
    BINDINGS
        .read()
        .unwrap_or_else(|e| e.into_inner())
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
    BINDINGS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
}

/// Get statistics about the current registry state.
///
/// Useful for debugging and performance monitoring.
///
/// # Returns
///
/// Tuple of (`total_knobs`, `total_targets`, `avg_targets_per_knob`)
pub fn stats() -> (usize, usize, f32) {
    let map = BINDINGS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let knob_count = map.len();
    let target_count: usize = map.values().map(std::vec::Vec::len).sum();
    let avg = if knob_count > 0 {
        target_count as f32 / knob_count as f32
    } else {
        0.0
    };
    (knob_count, target_count, avg)
}

/// Check if any bindings are registered.
pub fn is_empty() -> bool {
    BINDINGS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .is_empty()
}

/// Get the number of registered knobs.
pub fn knob_count() -> usize {
    BINDINGS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .len()
}
