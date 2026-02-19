//! Mock implementation of the rig engine for testing.
//!
//! `MockRigEngine` provides an in-memory implementation that simulates
//! slot management without any DAW interaction. Useful for UI development
//! and unit tests.

use signal_proto::module_type::ModuleType;
use signal_proto::ModuleSnapshot;
use std::collections::HashMap;
use std::sync::Mutex;

use super::error::EngineError;
use super::rig_engine::{PresetLoadHandle, PresetReadiness, RigEngine, TransitionResult};
use super::slot::InstanceState;
use super::target::ModuleTarget;

/// Tracks state for a single mock slot.
#[derive(Debug)]
struct MockSlotState {
    _module_type: ModuleType,
    active_target: Option<ModuleTarget>,
    instance_state: InstanceState,
    is_disabled: bool,
}

/// In-memory rig engine for testing.
///
/// Simulates slot management without DAW interaction. All transitions
/// complete immediately (no async loading delays).
#[derive(Debug)]
pub struct MockRigEngine {
    slots: Mutex<HashMap<ModuleType, MockSlotState>>,
    _next_handle: Mutex<u64>,
}

impl Default for MockRigEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MockRigEngine {
    pub fn new() -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
            _next_handle: Mutex::new(1),
        }
    }

    /// Initialize slots for the given module types.
    pub fn initialize_slots(&self, module_types: &[ModuleType]) {
        let mut slots = self.slots.lock().unwrap();
        for &mt in module_types {
            slots.entry(mt).or_insert_with(|| MockSlotState {
                _module_type: mt,
                active_target: None,
                instance_state: InstanceState::Unloaded,
                is_disabled: false,
            });
        }
    }

    /// Load a scene by applying a set of targets to slots.
    ///
    /// Missing slots are auto-created. Slots not in targets are disabled.
    pub fn load_scene_targets(
        &self,
        targets: HashMap<ModuleType, ModuleTarget>,
    ) -> TransitionResult {
        let mut slots = self.slots.lock().unwrap();
        let errors = Vec::new();

        // Disable slots not present in new targets.
        for (mt, slot) in slots.iter_mut() {
            if !targets.contains_key(mt) && !slot.is_disabled {
                slot.is_disabled = true;
                slot.instance_state = InstanceState::Unloaded;
            }
        }

        // Activate or load targets.
        for (mt, target) in targets {
            let slot = slots.entry(mt).or_insert_with(|| MockSlotState {
                _module_type: mt,
                active_target: None,
                instance_state: InstanceState::Unloaded,
                is_disabled: false,
            });

            // Check if we need to change anything.
            let needs_load = match &slot.active_target {
                Some(current) => current.module_preset_id != target.module_preset_id,
                None => true,
            };

            if needs_load {
                slot.active_target = Some(target);
                slot.instance_state = InstanceState::Active;
                slot.is_disabled = false;
            } else if slot.is_disabled {
                slot.is_disabled = false;
                slot.instance_state = InstanceState::Active;
            }
        }

        if errors.is_empty() {
            TransitionResult::completed()
        } else {
            TransitionResult {
                outcome: super::rig_engine::SwitchOutcome::Completed,
                slot_errors: errors,
            }
        }
    }

    /// Get the current target for a module type.
    pub fn current_target(&self, module_type: ModuleType) -> Option<ModuleTarget> {
        self.slots
            .lock()
            .unwrap()
            .get(&module_type)
            .and_then(|s| s.active_target.clone())
    }

    /// Check if a slot is disabled.
    pub fn is_slot_disabled(&self, module_type: ModuleType) -> bool {
        self.slots
            .lock()
            .unwrap()
            .get(&module_type)
            .map(|s| s.is_disabled)
            .unwrap_or(true)
    }

    fn _next_handle(&self) -> u64 {
        let mut h = self._next_handle.lock().unwrap();
        let val = *h;
        *h += 1;
        val
    }
}

impl RigEngine for MockRigEngine {
    async fn apply_snapshot(
        &self,
        module_type: ModuleType,
        _snapshot: &ModuleSnapshot,
    ) -> Result<(), EngineError> {
        let slots = self.slots.lock().unwrap();
        if slots.contains_key(&module_type) {
            Ok(())
        } else {
            Err(EngineError::SlotNotInitialized { module_type })
        }
    }

    fn check_readiness(&self, _handle: PresetLoadHandle) -> PresetReadiness {
        // Mock: everything loads instantly.
        PresetReadiness::Ready
    }

    async fn wait_ready(&self, _handle: PresetLoadHandle) {
        // Mock: already ready.
    }

    fn slot_count(&self) -> usize {
        self.slots.lock().unwrap().len()
    }

    fn active_module_types(&self) -> Vec<ModuleType> {
        self.slots
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, s)| s.instance_state == InstanceState::Active && !s.is_disabled)
            .map(|(&mt, _)| mt)
            .collect()
    }

    async fn tick(&self) {
        // Mock: no preload queue or tails to clean up.
    }

    async fn shutdown(&self) {
        let mut slots = self.slots.lock().unwrap();
        for slot in slots.values_mut() {
            slot.instance_state = InstanceState::Unloaded;
            slot.active_target = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_proto::ModulePresetId;

    fn make_target(mt: ModuleType) -> ModuleTarget {
        ModuleTarget {
            module_type: mt,
            module_preset_id: ModulePresetId::new(),
            module_snapshot_id: None,
        }
    }

    #[test]
    fn initialize_creates_slots() {
        let engine = MockRigEngine::new();
        engine.initialize_slots(&[ModuleType::Amp, ModuleType::Drive]);
        assert_eq!(engine.slot_count(), 2);
    }

    #[test]
    fn load_scene_activates_targets() {
        let engine = MockRigEngine::new();
        let mut targets = HashMap::new();
        targets.insert(ModuleType::Amp, make_target(ModuleType::Amp));
        targets.insert(ModuleType::Drive, make_target(ModuleType::Drive));

        let result = engine.load_scene_targets(targets);
        assert!(result.is_completed());

        let active = engine.active_module_types();
        assert_eq!(active.len(), 2);
        assert!(!engine.is_slot_disabled(ModuleType::Amp));
    }

    #[test]
    fn load_scene_disables_removed_slots() {
        let engine = MockRigEngine::new();

        // First scene: Amp + Drive
        let mut targets1 = HashMap::new();
        targets1.insert(ModuleType::Amp, make_target(ModuleType::Amp));
        targets1.insert(ModuleType::Drive, make_target(ModuleType::Drive));
        engine.load_scene_targets(targets1);

        // Second scene: Amp only (Drive should be disabled)
        let mut targets2 = HashMap::new();
        targets2.insert(ModuleType::Amp, make_target(ModuleType::Amp));
        engine.load_scene_targets(targets2);

        assert!(!engine.is_slot_disabled(ModuleType::Amp));
        assert!(engine.is_slot_disabled(ModuleType::Drive));
    }

    #[tokio::test]
    async fn apply_snapshot_to_missing_slot_errors() {
        let engine = MockRigEngine::new();
        let snapshot = signal_proto::ModuleSnapshot::new(
            signal_proto::ModuleSnapshotId::new(),
            "test",
            signal_proto::Module::from_blocks(vec![]),
        );
        let result = engine
            .apply_snapshot(ModuleType::Amp, &snapshot)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn shutdown_unloads_all() {
        let engine = MockRigEngine::new();
        let mut targets = HashMap::new();
        targets.insert(ModuleType::Amp, make_target(ModuleType::Amp));
        engine.load_scene_targets(targets);
        assert_eq!(engine.active_module_types().len(), 1);

        engine.shutdown().await;
        assert_eq!(engine.active_module_types().len(), 0);
    }

    #[test]
    fn check_readiness_always_ready() {
        let engine = MockRigEngine::new();
        assert_eq!(
            engine.check_readiness(PresetLoadHandle(1)),
            PresetReadiness::Ready
        );
    }
}
