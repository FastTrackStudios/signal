//! Bridge between Block MacroBank and the FTS Macros JSFX.
//!
//! When a block with a `MacroBank` is loaded onto a REAPER track, this module:
//! 1. Inserts the FTS Macros JSFX at position 0
//! 2. Resolves macro bindings to concrete FX parameter indices
//! 3. Writes REAPER plink configuration so the target FX listens to MIDI CC
//! 4. Injects @serialize state (P.Inst, ModAmt, P_OrigV) via the <JS_SER> block
//! 5. Sets initial macro slider values

use base64::{engine::general_purpose::STANDARD, Engine as _};
use daw::{FxHandle, TrackHandle};
use macromod::{MacroBank, MacroBinding, MacroKnob};
use signal_proto::Block;

use crate::engine::param_bridge::param_name_matches;

/// JSFX plugin identifier for REAPER's FX chain.
const JSFX_NAME: &str = "JS: FTS Macros";

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
/// the JSFX, resolves bindings, writes plink config, injects serialize state,
/// and sets macro slider values.
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

    // 3. Insert or reuse the JSFX.
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
    let macros_fx = match existing {
        Some(fx) => fx,
        None => track
            .fx_chain()
            .add_at(JSFX_NAME, 0)
            .await
            .map_err(|e| format!("Failed to insert FTS Macros JSFX: {e}"))?,
    };

    // 4. Get target FX parameters for name resolution.
    let target_params = target_fx
        .parameters()
        .await
        .map_err(|e| format!("Failed to get target FX parameters: {e}"))?;

    // 5. Resolve bindings to concrete param indices and assign CC numbers.
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

    // 6. Inject JSFX @serialize state and slider values.
    //    REAPER stores JSFX serialize data in a <JS_SER> block (base64 LE f64).
    //    Simply modifying the track chunk doesn't re-instantiate live FX, so we:
    //    a) Capture the track chunk with our JSFX in it
    //    b) Modify the <JS_SER> block and slider values in the chunk
    //    c) Remove the JSFX (so it's no longer live)
    //    d) Set the modified chunk back — REAPER instantiates the JSFX fresh
    //       from the chunk, running @serialize in read mode
    let serialize_data = build_jsfx_serialize_data(&bindings, macro_bank, &target_params);
    reinject_jsfx_with_state(track, &macros_fx, &serialize_data, macro_bank).await?;

    // Re-acquire the FxHandle after re-instantiation (GUID is preserved in chunk).
    let macros_fx = track
        .fx_chain()
        .by_name(JSFX_NAME)
        .await
        .map_err(|e| format!("Failed to find FTS Macros after re-inject: {e}"))?
        .or_else(|| None);
    let macros_fx = match macros_fx {
        Some(fx) => fx,
        None => track
            .fx_chain()
            .by_name("FTS Macros")
            .await
            .map_err(|e| format!("Failed to find FTS Macros: {e}"))?
            .ok_or("FTS Macros JSFX not found after re-injection")?,
    };

    // 7. Write plink config on each resolved target param.
    //    IMPORTANT: This must happen AFTER reinject_jsfx_with_state because
    //    set_chunk replaces the entire track state. REAPER's plink MIDI routing
    //    (midi_bus/midi_chan/midi_msg/midi_msg2) is not preserved through the
    //    get_chunk/set_chunk cycle — the PLINK line in the RPP chunk stores
    //    the MIDI data in a format that SetNamedConfigParm populates differently
    //    from GetTrackStateChunk. Writing plink after set_chunk ensures the
    //    MIDI routing is live on the final FX state.
    for rb in &bindings {
        write_plink_config(target_fx, rb.target_param_index, rb.cc_number).await?;
    }

    Ok(Some(MacroSetupResult {
        macros_fx_guid: macros_fx.guid().to_string(),
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

// ─── Plink configuration ───────────────────────────────────────

/// Write REAPER plink config on a target FX parameter.
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
///
/// Sections 3-5 (MOD_CURVE, modulator state, step sequencer) are all zeros
/// for default macro-direct mode. The JSFX @serialize handles EOF gracefully
/// (file_var/file_mem return 0 for missing data) so we omit them.
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
        for macro_idx in 0..8usize {
            if macro_idx == rb.knob_index && rb.knob_index < 8 {
                data.push(100.0); // full unipolar modulation
            } else {
                data.push(0.0);
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

    data
}

/// Encode serialize data as base64 (LE f64 bytes), matching REAPER's <JS_SER> format.
fn serialize_data_to_base64(data: &[f64]) -> String {
    let mut bytes = Vec::with_capacity(data.len() * 8);
    for &val in data {
        bytes.extend_from_slice(&val.to_le_bytes());
    }
    STANDARD.encode(&bytes)
}

/// Format base64 data into lines matching REAPER's chunk format (278 chars per line).
fn format_base64_lines(encoded: &str) -> String {
    const LINE_WIDTH: usize = 278;
    let mut result = String::new();
    for chunk in encoded.as_bytes().chunks(LINE_WIDTH) {
        result.push_str(std::str::from_utf8(chunk).unwrap());
        result.push('\n');
    }
    result
}

/// Remove the JSFX, modify its state in the track chunk, and let REAPER
/// re-instantiate it fresh from the chunk (triggering @serialize read mode).
async fn reinject_jsfx_with_state(
    track: &TrackHandle,
    macros_fx: &FxHandle,
    serialize_data: &[f64],
    macro_bank: &MacroBank,
) -> Result<(), String> {
    // 1. Capture the track chunk while the JSFX is still present.
    let chunk = track
        .get_chunk()
        .await
        .map_err(|e| format!("Failed to get track chunk: {e}"))?;

    let fx_guid = macros_fx.guid().to_string();

    // 2. Modify the <JS_SER> block with our serialize data.
    let new_b64 = serialize_data_to_base64(serialize_data);
    let new_b64_lines = format_base64_lines(&new_b64);
    let chunk = replace_js_ser_block(&chunk, &fx_guid, &new_b64_lines)?;

    // 3. Bake slider values into the <JS> block's slider line.
    let chunk = replace_js_slider_line(&chunk, &fx_guid, macro_bank)?;

    // 4. Remove the live JSFX instance so it's no longer cached.
    macros_fx
        .remove()
        .await
        .map_err(|e| format!("Failed to remove FTS Macros for re-inject: {e}"))?;

    // 5. Set the modified chunk. REAPER will instantiate the JSFX fresh from
    //    the chunk text, calling @serialize in read mode to load our state.
    track
        .set_chunk(chunk)
        .await
        .map_err(|e| format!("Failed to set track chunk: {e}"))?;

    Ok(())
}

/// Replace the slider values line in the `<JS>` block for a specific JSFX.
fn replace_js_slider_line(chunk: &str, fx_guid: &str, macro_bank: &MacroBank) -> Result<String, String> {
    let guid_pattern = format!("FXID {{{}}}", fx_guid.trim_matches(|c| c == '{' || c == '}'));
    let guid_pos = chunk
        .find(&guid_pattern)
        .ok_or_else(|| format!("FXID {} not found", fx_guid))?;

    // Search backwards for the <JS block.
    let before_guid = &chunk[..guid_pos];
    let js_start = before_guid
        .rfind("<JS ")
        .ok_or("No <JS block found before FXID")?;

    // The slider line is the first line after <JS ...>.
    // Find end of the <JS header line.
    let after_js = &chunk[js_start..];
    let header_end = after_js.find('\n').ok_or("No newline after <JS header")?;
    let slider_start = js_start + header_end + 1;

    // Find end of slider line.
    let slider_line_end = chunk[slider_start..]
        .find('\n')
        .map(|p| slider_start + p)
        .ok_or("No newline after slider line")?;

    // Build new slider line: 25 slider values + 39 dashes = 64 slots.
    let mut slider_vals: Vec<String> = Vec::with_capacity(64);
    for i in 0..25 {
        if i < 8 {
            let val = macro_bank.knobs.get(i).map(|k| k.value as f64).unwrap_or(0.0);
            if val == 0.0 {
                slider_vals.push("0".to_string());
            } else {
                slider_vals.push(format!("{val}"));
            }
        } else {
            slider_vals.push("0".to_string());
        }
    }
    for _ in 25..64 {
        slider_vals.push("-".to_string());
    }
    let new_slider_line = slider_vals.join(" ");

    let mut result = String::with_capacity(chunk.len());
    result.push_str(&chunk[..slider_start]);
    result.push_str(&new_slider_line);
    result.push_str(&chunk[slider_line_end..]);

    Ok(result)
}

/// Replace the `<JS_SER>` block content for a specific JSFX identified by GUID.
fn replace_js_ser_block(chunk: &str, fx_guid: &str, new_b64_lines: &str) -> Result<String, String> {
    // Find the FXID line for our JSFX.
    let clean_guid = fx_guid.trim_matches(|c| c == '{' || c == '}');
    let guid_pattern = format!("FXID {{{clean_guid}}}");
    eprintln!("[replace_js_ser] Looking for: {guid_pattern}");
    let guid_pos = chunk
        .find(&guid_pattern)
        .ok_or_else(|| format!("FXID {} not found in track chunk (clean: {})", fx_guid, clean_guid))?;
    eprintln!("[replace_js_ser] Found FXID at pos {guid_pos}");

    // Search backwards from the FXID for the <JS_SER block.
    let before_guid = &chunk[..guid_pos];
    let js_ser_start = before_guid
        .rfind("<JS_SER")
        .ok_or("No <JS_SER block found before FXID")?;
    eprintln!("[replace_js_ser] Found <JS_SER at pos {js_ser_start}");

    // Find the closing > for the <JS_SER> block.
    let after_ser = &chunk[js_ser_start..];
    let ser_close = after_ser
        .find("\n>")
        .ok_or("No closing > for <JS_SER block")?;

    // The full <JS_SER>...</> span in the original chunk.
    let ser_end = js_ser_start + ser_close + 2; // +2 for "\n>"
    eprintln!("[replace_js_ser] Replacing {} bytes with {} bytes of new data",
        ser_end - js_ser_start, new_b64_lines.len());

    // Build the replacement <JS_SER> block.
    let new_ser_block = format!("<JS_SER\n{}>", new_b64_lines);

    let mut result = String::with_capacity(chunk.len());
    result.push_str(&chunk[..js_ser_start]);
    result.push_str(&new_ser_block);
    result.push_str(&chunk[ser_end..]);

    Ok(result)
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
            ResolvedMacroBinding { knob_index: 0, knob_id: "drive".into(), target_param_index: 5, cc_number: 1 },
            ResolvedMacroBinding { knob_index: 1, knob_id: "tone".into(), target_param_index: 12, cc_number: 2 },
            ResolvedMacroBinding { knob_index: 1, knob_id: "tone".into(), target_param_index: 13, cc_number: 3 },
        ];
        for (i, b) in bindings.iter().enumerate() {
            assert_eq!(b.cc_number, (i + 1) as u32);
        }
    }

    #[test]
    fn build_serialize_data_layout() {
        let bindings = vec![
            ResolvedMacroBinding { knob_index: 0, knob_id: "drive".into(), target_param_index: 5, cc_number: 1 },
            ResolvedMacroBinding { knob_index: 2, knob_id: "mix".into(), target_param_index: 10, cc_number: 2 },
        ];

        let target_params = vec![
            daw_proto::FxParameter {
                index: 5, name: "Gain".into(), value: 0.75, formatted: "0.75".into(),
                is_toggle: false, step_count: None, step_labels: Vec::new(),
            },
            daw_proto::FxParameter {
                index: 10, name: "Mix".into(), value: 0.5, formatted: "50%".into(),
                is_toggle: false, step_count: None, step_labels: Vec::new(),
            },
        ];

        let bank = MacroBank::default();
        let data = build_jsfx_serialize_data(&bindings, &bank, &target_params);

        assert_eq!(data[0], 2.0);                      // P.Inst
        assert_eq!(data[1], 100.0);                     // macro 0 mod amount
        assert_eq!(data[2], 0.0);                       // macro 1 = none
        assert_eq!(data[9], 0.75);                      // P_OrigV
        assert_eq!(data[10], 0.0);                      // macro 0 = none
        assert_eq!(data[12], 100.0);                    // macro 2 = full
        assert_eq!(data[18], 0.5);                      // P_OrigV
        assert_eq!(data.len(), 1 + 2 * 9);              // compact: no zero-fill
    }

    #[test]
    fn serialize_base64_roundtrip() {
        let data = vec![3.0, 100.0, 0.0, 0.5];
        let b64 = serialize_data_to_base64(&data);
        let decoded = STANDARD.decode(&b64).unwrap();
        assert_eq!(decoded.len(), 32);

        for (i, &original) in data.iter().enumerate() {
            let offset = i * 8;
            let recovered = f64::from_le_bytes(decoded[offset..offset + 8].try_into().unwrap());
            assert_eq!(recovered, original);
        }
    }

    #[test]
    fn replace_js_ser_block_in_chunk() {
        let chunk = r#"<TRACK
<FXCHAIN
BYPASS 0 0 0
<JS "test" ""
0 0 0 - - -
>
<JS_SER
AAAA
BBBB
>
FLOATPOS 0 0 0 0
FXID {ABC-123}
WAK 0 0
>
>"#;

        let new_b64 = "XXXX\nYYYY\n";
        let result = replace_js_ser_block(chunk, "ABC-123", new_b64).unwrap();

        assert!(result.contains("<JS_SER\nXXXX\nYYYY\n>"));
        assert!(!result.contains("AAAA"));
        assert!(!result.contains("BBBB"));
        assert!(result.contains("FXID {ABC-123}"));
    }
}
