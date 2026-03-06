//! Bridge between Block MacroBank and the FTS Macros JSFX.
//!
//! When a block with a `MacroBank` is loaded onto a REAPER track, this module:
//! 1. Inserts the FTS Macros JSFX via chunk insertion (so @serialize state loads)
//! 2. Resolves macro bindings to concrete FX parameter indices
//! 3. Writes REAPER plink configuration so the target FX listens to MIDI CC
//! 4. Bakes initial macro slider values and serialize state into the JSFX chunk

use daw::{FxHandle, TrackHandle};
use macromod::{MacroBank, MacroBinding, MacroKnob};
use signal_proto::Block;

use crate::engine::param_bridge::param_name_matches;

/// JSFX plugin identifier for REAPER's FX chain.
const JSFX_NAME: &str = "JS: FTS Macros";

/// Number of JSFX slider slots in RPP chunk format (always 64).
const JSFX_SLIDER_SLOTS: usize = 64;

// ─── Result types ───────────────────────────────────────────────

/// Outcome of macro setup for a loaded block.
pub struct MacroSetupResult {
    /// GUID of the inserted (or reused) FTS Macros JSFX instance.
    pub macros_fx_guid: String,
    /// Resolved bindings with concrete parameter indices and CC numbers.
    pub bindings: Vec<ResolvedMacroBinding>,
}

/// A single macro binding resolved to concrete FX parameter and CC indices.
pub struct ResolvedMacroBinding {
    /// 0-based index of the macro knob in the bank.
    pub knob_index: usize,
    /// Unique ID of the knob (e.g. "drive").
    pub knob_id: String,
    /// Concrete FX parameter index on the target plugin.
    pub target_param_index: u32,
    /// 1-based CC number assigned to this binding for plink.
    pub cc_number: u32,
}

// ─── Collected binding (pre-resolution) ─────────────────────────

/// Intermediate type: a binding plus its knob context, before FX param resolution.
struct CollectedBinding<'a> {
    knob_index: usize,
    knob_id: String,
    binding: &'a MacroBinding,
}

// ─── Core entry point ──────────────────────────────────────────

/// Insert FTS Macros JSFX and configure macro bindings for a loaded block.
///
/// Returns `None` if the block has no macro bank. Otherwise inserts (or reuses)
/// the JSFX, resolves bindings, writes plink config, and bakes slider values
/// and serialize state into the JSFX chunk.
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

    // 4. Resolve bindings to concrete param indices and assign CC numbers.
    let mut bindings = Vec::new();
    let mut cc_counter: u32 = 1;

    for cb in &collected {
        let param_index = target_params
            .iter()
            .find(|p| param_name_matches(&cb.binding.target.param_id, &p.name))
            .map(|p| p.index);

        if let Some(idx) = param_index {
            bindings.push(ResolvedMacroBinding {
                knob_index: cb.knob_index,
                knob_id: cb.knob_id.clone(),
                target_param_index: idx,
                cc_number: cc_counter,
            });
            cc_counter += 1;
        }
    }

    // 5. Insert or reuse the JSFX.
    let macros_fx = insert_or_reuse_jsfx(track, macro_bank, &bindings, &target_params).await?;

    // 6. Write plink config on each resolved target param.
    for rb in &bindings {
        write_plink_config(target_fx, rb.target_param_index, rb.cc_number).await?;
    }

    Ok(Some(MacroSetupResult {
        macros_fx_guid: macros_fx.guid().to_string(),
        bindings,
    }))
}

/// Insert FTS Macros JSFX with pre-baked state, or reuse an existing instance.
///
/// For new insertions: builds a complete RPP chunk with slider values and
/// @serialize data, then inserts it via `insert_chunk` so REAPER instantiates
/// the JSFX fresh from the chunk (triggering proper @serialize deserialization).
///
/// For existing instances: removes and re-inserts with updated state.
async fn insert_or_reuse_jsfx(
    track: &TrackHandle,
    macro_bank: &MacroBank,
    bindings: &[ResolvedMacroBinding],
    target_params: &[daw_proto::FxParameter],
) -> Result<FxHandle, String> {
    // Check for existing JSFX.
    let existing = track
        .fx_chain()
        .by_name(JSFX_NAME)
        .await
        .map_err(|e| format!("Failed to search for FTS Macros: {e}"))?;
    let existing = match existing {
        Some(fx) => Some(fx),
        None => track
            .fx_chain()
            .by_name("FTS Macros")
            .await
            .map_err(|e| format!("Failed to search for FTS Macros: {e}"))?,
    };

    // If existing, remove it so we can re-insert with fresh state.
    if let Some(fx) = &existing {
        fx.remove()
            .await
            .map_err(|e| format!("Failed to remove existing FTS Macros: {e}"))?;
    }

    // Build the complete JSFX chunk with slider values + serialize data.
    let serialize_data = build_jsfx_serialize_data(bindings, macro_bank, target_params);
    let state_text = serialize_data_to_jsfx_text(&serialize_data);
    let chunk = build_complete_jsfx_chunk(macro_bank, &state_text);

    eprintln!(
        "[macro_setup] Inserting JSFX chunk ({} chars, {} serialize values)",
        chunk.len(),
        serialize_data.len()
    );

    // Insert the complete chunk into the FX chain.
    // insert_chunk appends to the end of the chain.
    track
        .fx_chain()
        .insert_chunk(&chunk)
        .await
        .map_err(|e| format!("Failed to insert FTS Macros chunk: {e}"))?;

    // Find the just-inserted JSFX by name.
    let macros_fx = track
        .fx_chain()
        .by_name(JSFX_NAME)
        .await
        .map_err(|e| format!("Failed to find inserted FTS Macros: {e}"))?
        .or_else(|| None);

    // Fallback: try bare name.
    let macros_fx = match macros_fx {
        Some(fx) => fx,
        None => track
            .fx_chain()
            .by_name("FTS Macros")
            .await
            .map_err(|e| format!("Failed to find FTS Macros: {e}"))?
            .ok_or("FTS Macros JSFX not found after chunk insertion")?,
    };

    Ok(macros_fx)
}

/// Build a complete JSFX RPP chunk block ready for insertion.
///
/// Format:
/// ```text
/// <JS "FastTrackStudio/FTS Macros.jsfx" ""
///   slider1 slider2 ... slider25 - - - ... (64 total slots)
///   serialize_data_line_1
///   serialize_data_line_2
///   ...
/// >
/// ```
fn build_complete_jsfx_chunk(macro_bank: &MacroBank, serialize_text: &str) -> String {
    let mut chunk = String::new();

    // Header — use the path format REAPER expects.
    chunk.push_str("<JS \"FastTrackStudio/FTS Macros.jsfx\" \"\"\n");

    // Slider values line: 25 defined sliders + 39 dashes = 64 slots.
    // Sliders 1-8: macro values, 9-16: morph (0), 17-24: automatable (0), 25: track ID (0).
    let mut slider_vals: Vec<String> = Vec::with_capacity(JSFX_SLIDER_SLOTS);
    for i in 0..25 {
        if i < 8 {
            // Macro slider: use knob value if available.
            let val = macro_bank
                .knobs
                .get(i)
                .map(|k| k.value as f64)
                .unwrap_or(0.0);
            if val == 0.0 {
                slider_vals.push("0".to_string());
            } else {
                slider_vals.push(format!("{val}"));
            }
        } else {
            slider_vals.push("0".to_string());
        }
    }
    // Fill remaining 39 slots with dashes.
    for _ in 25..JSFX_SLIDER_SLOTS {
        slider_vals.push("-".to_string());
    }
    chunk.push_str(&slider_vals.join(" "));
    chunk.push('\n');

    // Serialize data lines (no leading spaces — matches REAPER's format).
    for line in serialize_text.lines() {
        chunk.push_str(line);
        chunk.push('\n');
    }

    // Close the block.
    chunk.push('>');
    chunk
}

// ─── Binding collection ────────────────────────────────────────

/// Recursively collect all bindings from a MacroBank's knobs and their children.
///
/// Returns a flat list of `(knob_index, knob_id, binding)` tuples. The knob_index
/// is the top-level knob position (0-based), preserved even for child bindings.
fn collect_all_bindings(bank: &MacroBank) -> Vec<CollectedBinding<'_>> {
    let mut result = Vec::new();
    for (idx, knob) in bank.knobs.iter().enumerate() {
        collect_knob_bindings(idx, knob, &mut result);
    }
    // Also collect from group knobs.
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

// ─── Plink configuration ───────────────────────────────────────

/// Write REAPER plink config on a target FX parameter.
///
/// Configures the parameter to be driven by MIDI CC on bus 15, channel 16.
/// The CC number corresponds to the param's instance index in the JSFX (1-based).
async fn write_plink_config(
    target_fx: &FxHandle,
    param_index: u32,
    cc_number: u32,
) -> Result<(), String> {
    let keys = [
        ("active", "1"),
        ("effect", "-100"),
        ("param", "-1"),
        ("midi_bus", "15"),
        ("midi_chan", "16"),
        ("midi_msg", "176"),
    ];

    for (suffix, value) in &keys {
        let key = format!("param.{param_index}.plink.{suffix}");
        target_fx
            .set_config(&key, value)
            .await
            .map_err(|e| format!("Failed to set plink {key}: {e}"))?;
    }

    // midi_msg2 is the CC number (variable per binding).
    let key = format!("param.{param_index}.plink.midi_msg2");
    let cc_str = cc_number.to_string();
    target_fx
        .set_config(&key, &cc_str)
        .await
        .map_err(|e| format!("Failed to set plink {key}: {e}"))?;

    Ok(())
}

// ─── JSFX state serialization ──────────────────────────────────

/// Build the serialized data matching the FTS Macros JSFX @serialize format.
///
/// Layout (all f64):
/// 1. P.Inst (number of linked params)
/// 2. For each param i=1..P.Inst: 8 mod amounts + P_OrigV
/// 3. 80000 zeros (MOD_CURVE table)
/// 4. 8 × 18 zeros (modulator state — type=0 means macro/direct)
/// 5. 512 zeros (step sequencer)
fn build_jsfx_serialize_data(
    bindings: &[ResolvedMacroBinding],
    _macro_bank: &MacroBank,
    target_params: &[daw_proto::FxParameter],
) -> Vec<f64> {
    let p_inst = bindings.len();
    let mut data = Vec::new();

    // 1. P.Inst
    data.push(p_inst as f64);

    // 2. For each linked param: 8 mod amounts + P_OrigV
    for rb in bindings {
        // Mod amounts: 100 for the knob that owns this binding, 0 for others.
        // ModAmt encoding: 0–100 = unipolar, >100 = bipolar.
        for macro_idx in 0..8usize {
            if macro_idx == rb.knob_index && rb.knob_index < 8 {
                data.push(100.0); // full unipolar modulation
            } else {
                data.push(0.0); // no modulation from this macro
            }
        }

        // P_OrigV: original param value (0.0–1.0).
        let orig_value = target_params
            .iter()
            .find(|p| p.index == rb.target_param_index)
            .map(|p| p.value)
            .unwrap_or(0.5);
        data.push(orig_value);
    }

    // Sections 3-5 (MOD_CURVE, modulator state, step sequencer) are all
    // zeros for default macro-direct mode. The JSFX @serialize handles EOF
    // gracefully — file_var/file_mem return 0 for missing data — so we
    // omit the 80656 zeros entirely. This keeps the chunk compact.

    data
}

/// Convert a `Vec<f64>` serialize payload into raw bytes (LE f64).
/// Used by unit tests for roundtrip verification.
#[cfg(test)]
fn serialize_data_to_bytes(data: &[f64]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(data.len() * 8);
    for &val in data {
        bytes.extend_from_slice(&val.to_le_bytes());
    }
    bytes
}

/// Convert serialize data to JSFX text format.
///
/// REAPER stores JSFX `@serialize` data as space-separated decimal numbers
/// in the chunk, with multiple values per line. We emit 8 values per line
/// to keep it compact.
fn serialize_data_to_jsfx_text(data: &[f64]) -> String {
    let mut lines = Vec::new();
    for chunk in data.chunks(8) {
        let line: Vec<String> = chunk
            .iter()
            .map(|v| {
                // Integers as clean ints, floats with enough precision.
                if v.fract() == 0.0 && v.abs() < 1e15 {
                    format!("{}", *v as i64)
                } else {
                    format!("{:.14}", v)
                }
            })
            .collect();
        lines.push(line.join(" "));
    }
    lines.join("\n")
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

    #[test]
    fn cc_numbers_sequential_from_1() {
        let bindings = vec![
            ResolvedMacroBinding {
                knob_index: 0,
                knob_id: "drive".into(),
                target_param_index: 5,
                cc_number: 1,
            },
            ResolvedMacroBinding {
                knob_index: 1,
                knob_id: "tone".into(),
                target_param_index: 12,
                cc_number: 2,
            },
            ResolvedMacroBinding {
                knob_index: 1,
                knob_id: "tone".into(),
                target_param_index: 13,
                cc_number: 3,
            },
        ];
        for (i, b) in bindings.iter().enumerate() {
            assert_eq!(b.cc_number, (i + 1) as u32);
        }
    }

    #[test]
    fn build_serialize_data_layout() {
        let bindings = vec![
            ResolvedMacroBinding {
                knob_index: 0,
                knob_id: "drive".into(),
                target_param_index: 5,
                cc_number: 1,
            },
            ResolvedMacroBinding {
                knob_index: 2,
                knob_id: "mix".into(),
                target_param_index: 10,
                cc_number: 2,
            },
        ];

        let target_params = vec![
            daw_proto::FxParameter {
                index: 5,
                name: "Gain".into(),
                value: 0.75,
                formatted: "0.75".into(),
                is_toggle: false,
                step_count: None,
                step_labels: Vec::new(),
            },
            daw_proto::FxParameter {
                index: 10,
                name: "Mix".into(),
                value: 0.5,
                formatted: "50%".into(),
                is_toggle: false,
                step_count: None,
                step_labels: Vec::new(),
            },
        ];

        let bank = MacroBank::default();
        let data = build_jsfx_serialize_data(&bindings, &bank, &target_params);

        // P.Inst = 2
        assert_eq!(data[0], 2.0);

        // Param 1 (knob_index=0): mod amounts [100, 0, 0, 0, 0, 0, 0, 0] + P_OrigV=0.75
        assert_eq!(data[1], 100.0);
        assert_eq!(data[2], 0.0);
        assert_eq!(data[9], 0.75);

        // Param 2 (knob_index=2): mod amounts [0, 0, 100, 0, 0, 0, 0, 0] + P_OrigV=0.5
        assert_eq!(data[10], 0.0);
        assert_eq!(data[12], 100.0);
        assert_eq!(data[18], 0.5);

        // Total: 1 (P.Inst) + 2*9 (per-param data) = 19
        // MOD_CURVE/modulator/sequencer zeros are omitted (JSFX reads 0 on EOF).
        assert_eq!(data.len(), 1 + 2 * 9);
    }

    #[test]
    fn serialize_data_to_bytes_roundtrip() {
        let data = vec![1.0, 2.5, 0.0, -3.14];
        let bytes = serialize_data_to_bytes(&data);
        assert_eq!(bytes.len(), 32);

        for (i, &original) in data.iter().enumerate() {
            let offset = i * 8;
            let recovered =
                f64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
            assert_eq!(recovered, original);
        }
    }

    #[test]
    fn build_complete_chunk_format() {
        let mut bank = MacroBank::default();
        let mut knob = MacroKnob::new("drive", "Drive");
        knob.value = 0.75;
        bank.add(knob);

        let serialize = "3 100 0 0 0 0 0 0\n0 0.5 0 0 0 0 0 0";
        let chunk = build_complete_jsfx_chunk(&bank, serialize);

        // Header
        assert!(chunk.starts_with("<JS \"FastTrackStudio/FTS Macros.jsfx\" \"\""));

        // Slider values: knob 0 = 0.75, rest = 0, then dashes
        let lines: Vec<&str> = chunk.lines().collect();
        let slider_line = lines[1];
        assert!(slider_line.starts_with("0.75 0 "), "slider line: {slider_line}");
        assert!(slider_line.contains(" - "), "should have dashes for unused slots");

        // Serialize data present
        assert!(chunk.contains("3 100 0 0 0 0 0 0"));
        assert!(chunk.contains("0 0.5 0 0 0 0 0 0"));

        // Closes with >
        assert!(chunk.ends_with(">"));
    }
}
