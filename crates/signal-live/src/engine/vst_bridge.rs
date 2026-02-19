//! VST parameter bridge for bidirectional sync between internal state and DAW plugins.
//!
//! The `VstParameterBridge` trait defines how parameter values flow between
//! the signal engine and DAW plugin instances. `ParameterSyncManager` coordinates
//! the sync, handling debouncing and conflict resolution.

use std::collections::HashMap;

/// Direction of a parameter sync operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDirection {
    /// Push internal state → DAW plugin.
    ToPlugin,
    /// Pull DAW plugin state → internal.
    FromPlugin,
}

/// A single parameter value to sync.
#[derive(Debug, Clone)]
pub struct ParameterSync {
    /// FX identifier in the DAW.
    pub fx_id: String,
    /// Parameter index within the FX.
    pub param_index: u32,
    /// The value to set (0.0–1.0 normalized).
    pub value: f64,
    /// Direction of sync.
    pub direction: SyncDirection,
}

/// Trait for bidirectional VST parameter access.
///
/// Implementors provide DAW-specific parameter get/set operations.
/// The sync manager uses this to keep internal state and plugin state in sync.
pub trait VstParameterBridge: Send + Sync {
    /// Read a parameter value from a DAW plugin.
    fn get_parameter(&self, fx_id: &str, param_index: u32) -> Option<f64>;

    /// Write a parameter value to a DAW plugin.
    fn set_parameter(&self, fx_id: &str, param_index: u32, value: f64);

    /// Read all parameter values from a DAW plugin.
    fn get_all_parameters(&self, fx_id: &str) -> Vec<(u32, f64)>;

    /// Get the number of parameters for an FX.
    fn parameter_count(&self, fx_id: &str) -> u32;

    /// Get a parameter's display name.
    fn parameter_name(&self, fx_id: &str, param_index: u32) -> Option<String>;
}

/// Coordinates parameter sync between internal engine state and DAW plugins.
///
/// Tracks which parameters have been modified from each side and resolves
/// conflicts (latest write wins).
pub struct ParameterSyncManager {
    /// Last known parameter values (fx_id, param_index) → value.
    known_values: HashMap<(String, u32), f64>,
    /// Parameters that need to be pushed to the DAW.
    pending_to_plugin: Vec<ParameterSync>,
    /// Parameters that were updated from the DAW (for UI notification).
    updated_from_plugin: Vec<ParameterSync>,
}

impl Default for ParameterSyncManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ParameterSyncManager {
    pub fn new() -> Self {
        Self {
            known_values: HashMap::new(),
            pending_to_plugin: Vec::new(),
            updated_from_plugin: Vec::new(),
        }
    }

    /// Queue a parameter change to push to the DAW plugin.
    pub fn queue_to_plugin(&mut self, fx_id: &str, param_index: u32, value: f64) {
        self.known_values
            .insert((fx_id.to_string(), param_index), value);
        self.pending_to_plugin.push(ParameterSync {
            fx_id: fx_id.to_string(),
            param_index,
            value,
            direction: SyncDirection::ToPlugin,
        });
    }

    /// Sync with the DAW: push pending changes, pull DAW-side changes.
    ///
    /// Returns parameters that were updated from the DAW side (for UI update).
    pub fn sync(&mut self, bridge: &dyn VstParameterBridge) -> Vec<ParameterSync> {
        // Push pending changes to plugin.
        for sync in self.pending_to_plugin.drain(..) {
            bridge.set_parameter(&sync.fx_id, sync.param_index, sync.value);
        }

        // Pull DAW-side changes.
        self.updated_from_plugin.clear();
        let known_fxs: Vec<String> = self
            .known_values
            .keys()
            .map(|(fx, _)| fx.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        for fx_id in &known_fxs {
            let daw_values = bridge.get_all_parameters(fx_id);
            for (idx, daw_value) in daw_values {
                let key = (fx_id.clone(), idx);
                let known = self.known_values.get(&key).copied();

                if let Some(known_val) = known {
                    if (daw_value - known_val).abs() > 1e-6 {
                        // DAW changed the value — update our tracking.
                        self.known_values.insert(key, daw_value);
                        self.updated_from_plugin.push(ParameterSync {
                            fx_id: fx_id.clone(),
                            param_index: idx,
                            value: daw_value,
                            direction: SyncDirection::FromPlugin,
                        });
                    }
                }
            }
        }

        self.updated_from_plugin.clone()
    }

    /// Get the last known value for a parameter.
    pub fn known_value(&self, fx_id: &str, param_index: u32) -> Option<f64> {
        self.known_values
            .get(&(fx_id.to_string(), param_index))
            .copied()
    }

    /// Clear all tracked state.
    pub fn reset(&mut self) {
        self.known_values.clear();
        self.pending_to_plugin.clear();
        self.updated_from_plugin.clear();
    }
}

/// Mock VST bridge for testing.
pub struct MockVstBridge {
    params: std::sync::Mutex<HashMap<(String, u32), f64>>,
}

impl Default for MockVstBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl MockVstBridge {
    pub fn new() -> Self {
        Self {
            params: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Simulate the DAW changing a parameter externally.
    pub fn simulate_daw_change(&self, fx_id: &str, param_index: u32, value: f64) {
        self.params
            .lock()
            .unwrap()
            .insert((fx_id.to_string(), param_index), value);
    }
}

impl VstParameterBridge for MockVstBridge {
    fn get_parameter(&self, fx_id: &str, param_index: u32) -> Option<f64> {
        self.params
            .lock()
            .unwrap()
            .get(&(fx_id.to_string(), param_index))
            .copied()
    }

    fn set_parameter(&self, fx_id: &str, param_index: u32, value: f64) {
        self.params
            .lock()
            .unwrap()
            .insert((fx_id.to_string(), param_index), value);
    }

    fn get_all_parameters(&self, fx_id: &str) -> Vec<(u32, f64)> {
        self.params
            .lock()
            .unwrap()
            .iter()
            .filter(|((fid, _), _)| fid == fx_id)
            .map(|((_, idx), val)| (*idx, *val))
            .collect()
    }

    fn parameter_count(&self, fx_id: &str) -> u32 {
        self.params
            .lock()
            .unwrap()
            .keys()
            .filter(|(fid, _)| fid == fx_id)
            .count() as u32
    }

    fn parameter_name(&self, _fx_id: &str, param_index: u32) -> Option<String> {
        Some(format!("Param {param_index}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_and_sync_pushes_to_plugin() {
        let bridge = MockVstBridge::new();
        let mut manager = ParameterSyncManager::new();

        manager.queue_to_plugin("fx1", 0, 0.75);
        let updates = manager.sync(&bridge);

        // Should have pushed to DAW.
        assert_eq!(bridge.get_parameter("fx1", 0), Some(0.75));
        // No DAW-side changes to report.
        assert!(updates.is_empty());
    }

    #[test]
    fn sync_detects_daw_changes() {
        let bridge = MockVstBridge::new();
        let mut manager = ParameterSyncManager::new();

        // Set up initial known value.
        manager.queue_to_plugin("fx1", 0, 0.5);
        manager.sync(&bridge);

        // Simulate DAW changing the value.
        bridge.simulate_daw_change("fx1", 0, 0.9);
        let updates = manager.sync(&bridge);

        assert_eq!(updates.len(), 1);
        assert!((updates[0].value - 0.9).abs() < f64::EPSILON);
        assert_eq!(updates[0].direction, SyncDirection::FromPlugin);
    }

    #[test]
    fn known_value_tracking() {
        let mut manager = ParameterSyncManager::new();
        assert!(manager.known_value("fx1", 0).is_none());

        manager.queue_to_plugin("fx1", 0, 0.3);
        assert_eq!(manager.known_value("fx1", 0), Some(0.3));

        manager.reset();
        assert!(manager.known_value("fx1", 0).is_none());
    }

    #[test]
    fn no_false_positive_on_unchanged() {
        let bridge = MockVstBridge::new();
        let mut manager = ParameterSyncManager::new();

        manager.queue_to_plugin("fx1", 0, 0.5);
        manager.sync(&bridge);

        // Sync again with no changes.
        let updates = manager.sync(&bridge);
        assert!(updates.is_empty());
    }
}
