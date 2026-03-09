//! Macro binding resolution from abstract names to concrete FX parameters.
//!
//! This module bridges the gap between Signal's abstract macro definitions
//! and REAPER's concrete FX parameter indices. When a block is loaded:
//!
//! 1. Collect bindings from the MacroBank (including sub-macros)
//! 2. Query the target FX plugin for available parameters
//! 3. Match parameter names using semantic name resolution
//! 4. Return resolved bindings ready for registration in the global registry
//!
//! # Architecture
//!
//! **Input**: `Block.macro_bank` with abstract bindings like:
//! ```ignore
//! MacroBinding {
//!     block_id: "my_eq",
//!     target: "low_freq",  // Abstract parameter name
//!     min: 20.0,
//!     max: 5000.0,
//! }
//! ```
//!
//! **Process**:
//! - Recursive knob tree traversal (collect_knob_bindings)
//! - Parameter name matching (param_name_matches from engine)
//! - Index resolution from FX parameter list
//!
//! **Output**: `MacroSetupResult` with concrete indices:
//! ```ignore
//! LiveMacroBinding {
//!     knob_id: "drive",
//!     param_index: 5,  // Concrete index on the FX
//!     min: 0.0,
//!     max: 1.0,
//! }
//! ```
//!
//! # Design Principles
//!
//! - **No side effects**: setup_macros_for_block is pure, returns result for caller to register
//! - **No JSFX/MIDI**: Direct API, no middleware injection
//! - **Fail gracefully**: Missing parameters are skipped (logged as warnings)
//! - **Recursive**: Supports nested sub-macros (children in knob tree)

use daw::{FxHandle, TrackHandle};
use macromod::{MacroBank, MacroBinding, MacroKnob};
use signal_proto::Block;

use crate::engine::param_bridge::param_name_matches;

// ─── Result types ───────────────────────────────────────────────

/// Outcome of macro setup for a loaded block.
/// Contains everything needed to register bindings in the global registry.
pub struct MacroSetupResult {
    /// GUID of the track containing the target FX.
    pub track_guid: String,
    /// GUID of the target FX plugin.
    pub target_fx_guid: String,
    /// Resolved bindings with concrete parameter indices.
    pub bindings: Vec<LiveMacroBinding>,
}

/// A single macro binding resolved to concrete FX parameter.
pub struct LiveMacroBinding {
    /// 0-based index of the macro knob in the bank.
    pub knob_index: usize,
    /// Unique ID of the knob (e.g. "drive").
    pub knob_id: String,
    /// Concrete FX parameter index on the target plugin.
    pub param_index: u32,
    /// Minimum parameter value (normalized 0.0–1.0).
    pub min: f32,
    /// Maximum parameter value (normalized 0.0–1.0).
    pub max: f32,
}

// ─── Collected binding (pre-resolution) ─────────────────────────

/// Intermediate type: a binding plus its knob context, before FX param resolution.
struct CollectedBinding<'a> {
    knob_index: usize,
    knob_id: String,
    binding: &'a MacroBinding,
}

// ─── Core entry point ──────────────────────────────────────────

/// Resolve macro bindings to concrete FX parameters for a loaded block.
///
/// Returns `None` if the block has no macro bank or no bindings.
/// Otherwise resolves bindings to parameter indices and returns
/// a MacroSetupResult that can be registered in the global registry.
///
/// No JSFX insertion, no MIDI CC assignment, no plink configuration.
pub async fn setup_macros_for_block(
    track: &TrackHandle,
    target_fx: &FxHandle,
    block: &Block,
) -> Result<Option<MacroSetupResult>, String> {
    // 1. Early return if no macro bank.
    let macro_bank = match &block.macro_bank {
        Some(bank) => bank,
        None => return Ok(None),
    };

    // 2. Collect all bindings from the knob tree.
    let collected = collect_all_bindings(macro_bank);
    if collected.is_empty() {
        return Ok(None);
    }

    // 3. Get target FX parameters for name resolution.
    let target_params = target_fx
        .parameters()
        .await
        .map_err(|e| format!("Failed to get target FX parameters: {e}"))?;

    // 4. Resolve bindings to concrete param indices.
    let mut bindings = Vec::new();

    for cb in &collected {
        let param_info = target_params
            .iter()
            .find(|p| param_name_matches(&cb.binding.target.param_id, &p.name));

        if let Some(param) = param_info {
            bindings.push(LiveMacroBinding {
                knob_index: cb.knob_index,
                knob_id: cb.knob_id.clone(),
                param_index: param.index,
                min: cb.binding.min,
                max: cb.binding.max,
            });
        }
    }

    // Return early if no bindings could be resolved.
    if bindings.is_empty() {
        return Ok(None);
    }

    Ok(Some(MacroSetupResult {
        track_guid: track.guid().to_string(),
        target_fx_guid: target_fx.guid().to_string(),
        bindings,
    }))
}

// ─── Binding collection ────────────────────────────────────────

/// Recursively collect all bindings from a MacroBank's knobs and their children.
fn collect_all_bindings(bank: &MacroBank) -> Vec<CollectedBinding<'_>> {
    let mut result = Vec::new();
    for (idx, knob) in bank.knobs.iter().enumerate() {
        collect_knob_bindings(idx, knob, &mut result);
    }
    for group in &bank.groups {
        for knob in &group.knobs {
            let idx = bank.knobs.len() + result.len();
            collect_knob_bindings(idx, knob, &mut result);
        }
    }
    result
}

/// Recursively collect bindings from a single knob and its children.
fn collect_knob_bindings<'a>(
    knob_index: usize,
    knob: &'a MacroKnob,
    out: &mut Vec<CollectedBinding<'a>>,
) {
    for binding in &knob.bindings {
        out.push(CollectedBinding {
            knob_index,
            knob_id: knob.id.clone(),
            binding,
        });
    }
    for child in &knob.children {
        collect_knob_bindings(knob_index, child, out);
    }
}


// ─── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use macromod::{MacroBank, MacroBinding, MacroKnob};

    fn make_binding(param_id: &str) -> MacroBinding {
        MacroBinding::from_ids("block1", param_id, 0.0, 1.0)
    }

    fn make_knob(id: &str, bindings: Vec<MacroBinding>) -> MacroKnob {
        let mut knob = MacroKnob::new(id, id);
        knob.bindings = bindings;
        knob
    }

    #[test]
    fn collect_bindings_flat_knobs() {
        let mut bank = MacroBank::default();
        bank.add(make_knob("drive", vec![make_binding("gain")]));
        bank.add(make_knob("tone", vec![make_binding("treble"), make_binding("bass")]));

        let collected = collect_all_bindings(&bank);
        assert_eq!(collected.len(), 3);
        assert_eq!(collected[0].knob_id, "drive");
        assert_eq!(collected[0].binding.target.param_id, "gain");
        assert_eq!(collected[1].knob_id, "tone");
        assert_eq!(collected[2].knob_id, "tone");
    }

    #[test]
    fn collect_bindings_with_children() {
        let child = make_knob("sub_drive", vec![make_binding("sub_gain")]);
        let mut parent = make_knob("drive", vec![make_binding("gain")]);
        parent.children.push(child);

        let mut bank = MacroBank::default();
        bank.add(parent);

        let collected = collect_all_bindings(&bank);
        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0].knob_index, 0);
        assert_eq!(collected[1].knob_index, 0);
        assert_eq!(collected[1].knob_id, "sub_drive");
    }

    #[test]
    fn no_macro_bank_means_empty_collection() {
        let bank = MacroBank::default();
        let collected = collect_all_bindings(&bank);
        assert!(collected.is_empty());
    }
}
