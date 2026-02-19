//! Resolution targets and slot state for the diff engine.
//!
//! These types bridge the resolver (which walks the scene hierarchy) and the
//! diff engine (which computes per-slot transitions).

use signal_proto::module_type::ModuleType;
use signal_proto::{ModulePresetId, ModuleSnapshotId};

use super::slot::InstanceHandle;

/// The resolved target for a single module slot — what preset/snapshot to load.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleTarget {
    pub module_type: ModuleType,
    pub module_preset_id: ModulePresetId,
    /// `None` means use the preset's default snapshot.
    pub module_snapshot_id: Option<ModuleSnapshotId>,
}

/// What a slot is currently doing (input to the diff engine).
#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedSlot {
    /// Slot has an active module target loaded.
    Active(ModuleTarget),
    /// Slot is muted / bypassed.
    Disabled,
}

/// Per-slot state snapshot, used as input to `compute_diff`.
#[derive(Debug, Clone)]
pub struct SlotState {
    pub module_type: ModuleType,
    /// What's currently active in this slot (if anything).
    pub current: Option<ResolvedSlot>,
    /// Handle to the currently active instance (if any).
    pub active_handle: Option<InstanceHandle>,
    /// Whether the slot is disabled (bypassed/muted).
    pub is_disabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_target_equality() {
        let a = ModuleTarget {
            module_type: ModuleType::Amp,
            module_preset_id: ModulePresetId::new(),
            module_snapshot_id: None,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn resolved_slot_variants() {
        let target = ModuleTarget {
            module_type: ModuleType::Drive,
            module_preset_id: ModulePresetId::new(),
            module_snapshot_id: None,
        };
        let active = ResolvedSlot::Active(target);
        let disabled = ResolvedSlot::Disabled;
        assert_ne!(active, disabled);
    }
}
