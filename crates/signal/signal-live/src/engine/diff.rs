//! Slot diff engine — computes minimal per-slot transitions.
//!
//! Pure function: takes current slot states + new resolved targets → produces
//! a `Vec<SlotDiff>` describing what each slot needs to do.

use signal_proto::module_type::ModuleType;
use signal_proto::{ModulePresetId, ModuleSnapshotId};
use std::collections::HashMap;

use super::slot::InstanceHandle;
use super::target::{ModuleTarget, ResolvedSlot, SlotState};

/// A single slot's required transition.
#[derive(Debug, Clone)]
pub enum SlotDiff {
    /// Load a new plugin instance and activate it.
    LoadAndActivate {
        module_type: ModuleType,
        target: ModuleTarget,
    },
    /// Activate an already-preloaded instance (skip loading).
    Activate {
        module_type: ModuleType,
        handle: InstanceHandle,
    },
    /// Same preset, different snapshot — parameter changes only (no instance switch).
    ApplySnapshot {
        module_type: ModuleType,
        module_preset_id: ModulePresetId,
        snapshot_id: ModuleSnapshotId,
    },
    /// Mute all output for this slot.
    Disable { module_type: ModuleType },
    /// Un-mute a previously disabled slot.
    Enable { module_type: ModuleType },
    /// No change needed.
    NoChange { module_type: ModuleType },
}

impl SlotDiff {
    pub fn module_type(&self) -> ModuleType {
        match self {
            Self::LoadAndActivate { module_type, .. }
            | Self::Activate { module_type, .. }
            | Self::ApplySnapshot { module_type, .. }
            | Self::Disable { module_type }
            | Self::Enable { module_type }
            | Self::NoChange { module_type } => *module_type,
        }
    }
}

/// Compute the minimal set of transitions to move from current state to target state.
///
/// # Arguments
///
/// - `current_states` — current per-slot state (what's loaded/active now)
/// - `new_targets` — the resolved target for each module type in the new scene
/// - `preload_lookup` — returns a handle if a target is already preloaded and Ready
pub fn compute_diff(
    current_states: &[SlotState],
    new_targets: &HashMap<ModuleType, ResolvedSlot>,
    preload_lookup: &dyn Fn(&ModuleTarget) -> Option<InstanceHandle>,
) -> Vec<SlotDiff> {
    let mut diffs = Vec::new();

    // Process slots that exist in current state.
    for slot in current_states {
        let mt = slot.module_type;

        match new_targets.get(&mt) {
            // Target exists for this slot.
            Some(ResolvedSlot::Active(new_target)) => {
                diffs.push(diff_slot(slot, new_target, preload_lookup));
            }

            // Target is explicitly disabled.
            Some(ResolvedSlot::Disabled) => {
                if !slot.is_disabled {
                    diffs.push(SlotDiff::Disable { module_type: mt });
                } else {
                    diffs.push(SlotDiff::NoChange { module_type: mt });
                }
            }

            // Module type removed from new targets — disable it.
            None => {
                if slot.current.is_some() && !slot.is_disabled {
                    diffs.push(SlotDiff::Disable { module_type: mt });
                } else {
                    diffs.push(SlotDiff::NoChange { module_type: mt });
                }
            }
        }
    }

    // Process new targets that don't have an existing slot.
    let existing_types: std::collections::HashSet<ModuleType> =
        current_states.iter().map(|s| s.module_type).collect();

    for (mt, resolved) in new_targets {
        if existing_types.contains(mt) {
            continue;
        }
        match resolved {
            ResolvedSlot::Active(target) => {
                if let Some(handle) = preload_lookup(target) {
                    diffs.push(SlotDiff::Activate {
                        module_type: *mt,
                        handle,
                    });
                } else {
                    diffs.push(SlotDiff::LoadAndActivate {
                        module_type: *mt,
                        target: target.clone(),
                    });
                }
            }
            ResolvedSlot::Disabled => {
                diffs.push(SlotDiff::Disable { module_type: *mt });
            }
        }
    }

    diffs
}

/// Diff a single slot against its new target.
fn diff_slot(
    slot: &SlotState,
    new_target: &ModuleTarget,
    preload_lookup: &dyn Fn(&ModuleTarget) -> Option<InstanceHandle>,
) -> SlotDiff {
    let mt = slot.module_type;

    // Was disabled, now has a target → need to load.
    if slot.is_disabled {
        if let Some(handle) = preload_lookup(new_target) {
            return SlotDiff::Activate {
                module_type: mt,
                handle,
            };
        }
        return SlotDiff::LoadAndActivate {
            module_type: mt,
            target: new_target.clone(),
        };
    }

    match &slot.current {
        Some(ResolvedSlot::Active(current_target)) => {
            // Same preset?
            if current_target.module_preset_id == new_target.module_preset_id {
                // Same snapshot → no change.
                if current_target.module_snapshot_id == new_target.module_snapshot_id {
                    return SlotDiff::NoChange { module_type: mt };
                }
                // Different snapshot → apply parameters only.
                if let Some(snap_id) = &new_target.module_snapshot_id {
                    return SlotDiff::ApplySnapshot {
                        module_type: mt,
                        module_preset_id: new_target.module_preset_id.clone(),
                        snapshot_id: snap_id.clone(),
                    };
                }
                // Target has no snapshot (use default) — treat as parameter apply.
                return SlotDiff::NoChange { module_type: mt };
            }

            // Different preset — check preload cache.
            if let Some(handle) = preload_lookup(new_target) {
                SlotDiff::Activate {
                    module_type: mt,
                    handle,
                }
            } else {
                SlotDiff::LoadAndActivate {
                    module_type: mt,
                    target: new_target.clone(),
                }
            }
        }
        Some(ResolvedSlot::Disabled) | None => {
            // Not currently active — need to load.
            if let Some(handle) = preload_lookup(new_target) {
                SlotDiff::Activate {
                    module_type: mt,
                    handle,
                }
            } else {
                SlotDiff::LoadAndActivate {
                    module_type: mt,
                    target: new_target.clone(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_target(mt: ModuleType) -> ModuleTarget {
        ModuleTarget {
            module_type: mt,
            module_preset_id: ModulePresetId::new(),
            module_snapshot_id: None,
        }
    }

    fn no_preload(_: &ModuleTarget) -> Option<InstanceHandle> {
        None
    }

    #[test]
    fn no_change_when_same_target() {
        let target = make_target(ModuleType::Amp);
        let slot = SlotState {
            module_type: ModuleType::Amp,
            current: Some(ResolvedSlot::Active(target.clone())),
            active_handle: Some(InstanceHandle::new(1)),
            is_disabled: false,
        };

        let mut targets = HashMap::new();
        targets.insert(ModuleType::Amp, ResolvedSlot::Active(target));

        let diffs = compute_diff(&[slot], &targets, &no_preload);
        assert_eq!(diffs.len(), 1);
        assert!(matches!(diffs[0], SlotDiff::NoChange { .. }));
    }

    #[test]
    fn load_and_activate_when_different_preset() {
        let old = make_target(ModuleType::Amp);
        let new = make_target(ModuleType::Amp);
        assert_ne!(old.module_preset_id, new.module_preset_id);

        let slot = SlotState {
            module_type: ModuleType::Amp,
            current: Some(ResolvedSlot::Active(old)),
            active_handle: Some(InstanceHandle::new(1)),
            is_disabled: false,
        };

        let mut targets = HashMap::new();
        targets.insert(ModuleType::Amp, ResolvedSlot::Active(new));

        let diffs = compute_diff(&[slot], &targets, &no_preload);
        assert_eq!(diffs.len(), 1);
        assert!(matches!(diffs[0], SlotDiff::LoadAndActivate { .. }));
    }

    #[test]
    fn apply_snapshot_when_same_preset_different_snapshot() {
        let preset_id = ModulePresetId::new();
        let snap_a = ModuleSnapshotId::new();
        let snap_b = ModuleSnapshotId::new();

        let old = ModuleTarget {
            module_type: ModuleType::Amp,
            module_preset_id: preset_id.clone(),
            module_snapshot_id: Some(snap_a),
        };
        let new = ModuleTarget {
            module_type: ModuleType::Amp,
            module_preset_id: preset_id,
            module_snapshot_id: Some(snap_b),
        };

        let slot = SlotState {
            module_type: ModuleType::Amp,
            current: Some(ResolvedSlot::Active(old)),
            active_handle: Some(InstanceHandle::new(1)),
            is_disabled: false,
        };

        let mut targets = HashMap::new();
        targets.insert(ModuleType::Amp, ResolvedSlot::Active(new));

        let diffs = compute_diff(&[slot], &targets, &no_preload);
        assert_eq!(diffs.len(), 1);
        assert!(matches!(diffs[0], SlotDiff::ApplySnapshot { .. }));
    }

    #[test]
    fn disable_when_target_removed() {
        let target = make_target(ModuleType::Drive);
        let slot = SlotState {
            module_type: ModuleType::Drive,
            current: Some(ResolvedSlot::Active(target)),
            active_handle: Some(InstanceHandle::new(1)),
            is_disabled: false,
        };

        let targets = HashMap::new(); // empty — Drive removed

        let diffs = compute_diff(&[slot], &targets, &no_preload);
        assert_eq!(diffs.len(), 1);
        assert!(matches!(diffs[0], SlotDiff::Disable { .. }));
    }

    #[test]
    fn activate_preloaded_instance() {
        let target = make_target(ModuleType::Amp);
        let preloaded_handle = InstanceHandle::new(99);
        let target_clone = target.clone();

        let slot = SlotState {
            module_type: ModuleType::Amp,
            current: None,
            active_handle: None,
            is_disabled: false,
        };

        let mut targets = HashMap::new();
        targets.insert(ModuleType::Amp, ResolvedSlot::Active(target));

        let lookup = move |t: &ModuleTarget| -> Option<InstanceHandle> {
            if t.module_preset_id == target_clone.module_preset_id {
                Some(preloaded_handle)
            } else {
                None
            }
        };

        let diffs = compute_diff(&[slot], &targets, &lookup);
        assert_eq!(diffs.len(), 1);
        assert!(matches!(diffs[0], SlotDiff::Activate { handle, .. } if handle == preloaded_handle));
    }

    #[test]
    fn new_slot_from_empty() {
        let target = make_target(ModuleType::Time);

        let mut targets = HashMap::new();
        targets.insert(ModuleType::Time, ResolvedSlot::Active(target));

        // No current slots at all.
        let diffs = compute_diff(&[], &targets, &no_preload);
        assert_eq!(diffs.len(), 1);
        assert!(matches!(diffs[0], SlotDiff::LoadAndActivate { .. }));
    }
}
