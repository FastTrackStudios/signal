//! Mock implementation of the rig engine for testing.
//!
//! `MockRigEngine` provides an in-memory implementation that simulates
//! slot management without any DAW interaction. Useful for UI development
//! and unit tests.

use signal_proto::ModuleSnapshot;
use signal_proto::module_type::ModuleType;
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
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
            _next_handle: Mutex::new(1),
        }
    }

    /// Initialize slots for the given module types.
    pub fn initialize_slots(&self, module_types: &[ModuleType]) {
        let mut slots = self
            .slots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
        let mut slots = self
            .slots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
    ///
    /// # Panics
    ///
    /// Panics if the slots mutex is poisoned.
    pub fn current_target(&self, module_type: ModuleType) -> Option<ModuleTarget> {
        self.slots
            .lock()
            .unwrap()
            .get(&module_type)
            .and_then(|s| s.active_target.clone())
    }

    /// Check if a slot is disabled.
    ///
    /// # Panics
    ///
    /// Panics if the slots mutex is poisoned.
    pub fn is_slot_disabled(&self, module_type: ModuleType) -> bool {
        self.slots
            .lock()
            .unwrap()
            .get(&module_type)
            .is_none_or(|s| s.is_disabled)
    }

    fn _next_handle(&self) -> u64 {
        let mut h = self
            ._next_handle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
        let slots = self
            .slots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
        self.slots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
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
        let mut slots = self
            .slots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for slot in slots.values_mut() {
            slot.instance_state = InstanceState::Unloaded;
            slot.active_target = None;
        }
    }
}
