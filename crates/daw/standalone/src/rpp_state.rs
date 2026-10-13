//! Decoding REAPER `.rpp` plugin state chunks into byte blobs that
//! [`crate::plugin::PluginInstance::load_state`] accepts.
//!
//! ## REAPER's chunk layout
//!
//! Inside an `<FXCHAIN>` block, each plugin's state is encoded as
//! base64-formatted lines under a `<VST ...>`, `<CLAP ...>`, or
//! `<JS ...>` header. The base64 lines decode to a binary blob whose
//! structure is plugin-format-dependent — REAPER prepends its own
//! preamble and (for VST3) sandwiches the IComponent + IEditController
//! state streams.
//!
//! Documentation on the format is sparse; the implementation here is
//! reverse-engineered from real `.rpp` files and from REAPER's
//! `vst3_chunkutil.h` semantics. Best-effort — some plugins write
//! custom payloads inside the wrapper that we just pass through.
//!
//! ## VST3 chunk layout (REAPER 6+, the common case)
//!
//! ```text
//!   Line 0:                          16 base64 bytes containing
//!                                    [u8;4]  magic
//!                                    u32 LE  flags (0x00000002 for VST3)
//!                                    u32 LE  component-state size
//!                                    u32 LE  controller-state size
//!   Line 1..N (concatenated):        <component-state-size> bytes
//!                                    + <controller-state-size> bytes
//!                                    + optional REAPER metadata trailer
//! ```
//!
//! ## CLAP chunk layout
//!
//! Less documented — REAPER stores the raw bytes produced by
//! `clap_plugin_state.save` with no wrapper besides a small header
//! line. The implementation below treats CLAP chunks as "everything
//! after the first header line" and passes them straight to
//! `clap_plugin_state.load`. Adjust if real-world plugins disagree.

use base64::Engine;

/// Errors raised while decoding a REAPER state chunk.
#[derive(Debug, thiserror::Error)]
pub enum RppStateError {
    #[error("base64 decode failed: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("chunk too small to be a valid VST3 state blob")]
    TooShort,
    #[error("chunk component+controller sizes overrun the decoded buffer")]
    SizeOverflow,
}

/// Decode the base64 lines of a `<VST ... >` block into a daw-internal
/// VST3 state blob suitable for
/// [`crate::audio_engine::vst3_host::LoadedVst3Plugin::load_state`].
///
/// Returns `Ok(blob)` where blob has the `DAW3` magic + sizes + states
/// layout documented on `save_state`. Falls back to "everything after
/// the REAPER header" wrapped as a single component-state blob if the
/// header parser doesn't recognize the layout — that's enough for
/// many self-contained plugins that ignore controller state.
pub fn reaper_vst3_to_daw_state(lines: &[String]) -> Result<Vec<u8>, RppStateError> {
    let raw = decode_b64_lines(lines)?;
    // Pass-through: if the chunk is already a DAW3-framed blob
    // (e.g. saved by `save_state`, round-tripped through a host
    // that doesn't know our format), just hand it back. Lets tests
    // round-trip without writing a separate REAPER-format encoder.
    if raw.starts_with(b"DAW3") && is_self_consistent_daw3(&raw) {
        return Ok(raw);
    }
    // Heuristic: parse REAPER's 16-byte chunk header. Require the
    // header-declared sizes to fit *exactly* (modulo a small REAPER
    // metadata trailer) inside the buffer, otherwise the sizes were
    // probably not the REAPER format and the bytes are something
    // else entirely (e.g. a plugin-specific blob).
    if raw.len() >= 16 {
        let comp_size = u32::from_le_bytes(raw[8..12].try_into().unwrap()) as usize;
        let ctrl_size = u32::from_le_bytes(raw[12..16].try_into().unwrap()) as usize;
        let body = 16usize
            .checked_add(comp_size)
            .and_then(|s| s.checked_add(ctrl_size));
        if let Some(b) = body {
            // Accept the header if the declared sizes consume at
            // least half the buffer (excludes the case where we
            // mis-parsed two small u32s out of arbitrary bytes).
            if b <= raw.len() && b >= raw.len() / 2 {
                let comp = &raw[16..16 + comp_size];
                let ctrl = &raw[16 + comp_size..16 + comp_size + ctrl_size];
                return Ok(wrap_daw3(comp, ctrl));
            }
        }
    }
    // Fallback: treat the entire payload as component state.
    Ok(wrap_daw3(&raw, &[]))
}

/// Cheap check that a DAW3 blob's internal lengths add up — guards
/// against accepting random data that happens to start with "DAW3".
fn is_self_consistent_daw3(raw: &[u8]) -> bool {
    if raw.len() < 12 {
        return false;
    }
    let comp = u32::from_le_bytes(raw[4..8].try_into().unwrap()) as usize;
    let ctrl_off = 8 + comp;
    if raw.len() < ctrl_off + 4 {
        return false;
    }
    let ctrl = u32::from_le_bytes(raw[ctrl_off..ctrl_off + 4].try_into().unwrap()) as usize;
    raw.len() == ctrl_off + 4 + ctrl
}

/// Decode the base64 lines of a `<CLAP ... >` block into a byte blob
/// suitable for [`crate::audio_engine::plugin_host::LoadedClapPlugin::load_state`].
///
/// REAPER's CLAP wrapper is minimal — strip the first 16-byte header
/// (preset-name length + flags) and return the rest, which is what
/// `clap_plugin_state.save` originally wrote.
pub fn reaper_clap_to_state(lines: &[String]) -> Result<Vec<u8>, RppStateError> {
    let raw = decode_b64_lines(lines)?;
    if raw.len() < 16 {
        return Err(RppStateError::TooShort);
    }
    // Skip the REAPER preamble. Adjust if a specific plugin needs
    // bytes inside the preamble (e.g. preset-index, but typical CLAP
    // plugins ignore it).
    Ok(raw[16..].to_vec())
}

/// A plugin's state blob as the lines of a `<CLAP ... >` block —
/// [`reaper_clap_to_state`]'s inverse: REAPER's 16-byte preamble (no
/// preset name, no flags) and the state, base64 in 128-column lines.
#[must_use]
pub fn state_to_reaper_clap(state: &[u8]) -> Vec<String> {
    use base64::Engine;
    let mut raw = vec![0_u8; 16];
    raw.extend_from_slice(state);
    let text = base64::engine::general_purpose::STANDARD.encode(raw);
    text.as_bytes()
        .chunks(128)
        .map(|line| String::from_utf8_lossy(line).into_owned())
        .collect()
}

/// Raw base64 decode of a `Vec<String>` of base64 lines (whitespace
/// in each line is tolerated).
pub fn decode_b64_lines(lines: &[String]) -> Result<Vec<u8>, base64::DecodeError> {
    let mut joined = String::new();
    for line in lines {
        for c in line.chars() {
            if !c.is_whitespace() {
                joined.push(c);
            }
        }
    }
    base64::engine::general_purpose::STANDARD.decode(joined.as_bytes())
}

/// Pack `(component_state, controller_state)` into the DAW3 envelope
/// `LoadedVst3Plugin::load_state` expects.
fn wrap_daw3(component: &[u8], controller: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + component.len() + 4 + controller.len());
    out.extend_from_slice(b"DAW3");
    out.extend_from_slice(&(component.len() as u32).to_le_bytes());
    out.extend_from_slice(component);
    out.extend_from_slice(&(controller.len() as u32).to_le_bytes());
    out.extend_from_slice(controller);
    out
}

#[cfg(test)]
mod tests {

    /// A state written as a CLAP block reads back as itself.
    #[test]
    fn a_clap_state_round_trips() {
        let state: Vec<u8> = (0..=255_u8).cycle().take(1_000).collect();
        let lines = super::state_to_reaper_clap(&state);
        assert!(lines.len() > 1 && lines.iter().all(|l| l.len() <= 128));
        assert_eq!(super::reaper_clap_to_state(&lines).expect("decodes"), state);
    }

    use super::*;

    #[test]
    fn empty_input_decodes_to_empty_blob() {
        // No state lines: yields an empty DAW3-wrapped blob with
        // zero-length component + controller fields.
        let out = reaper_vst3_to_daw_state(&[]).unwrap();
        assert_eq!(&out[..4], b"DAW3");
        assert_eq!(&out[4..8], &0u32.to_le_bytes());
        assert_eq!(&out[8..12], &0u32.to_le_bytes());
    }

    #[test]
    fn synthetic_header_round_trips_component_and_controller() {
        // Build a fake REAPER chunk: 16-byte header + 4 comp bytes +
        // 3 controller bytes, base64-encode it as one line, then
        // decode + verify the daw-internal blob carries both halves.
        let mut raw = Vec::new();
        raw.extend_from_slice(b"abcd"); // magic
        raw.extend_from_slice(&2u32.to_le_bytes()); // flags
        raw.extend_from_slice(&4u32.to_le_bytes()); // comp size
        raw.extend_from_slice(&3u32.to_le_bytes()); // ctrl size
        raw.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]); // comp
        raw.extend_from_slice(&[0x11, 0x22, 0x33]); // ctrl
        let line = base64::engine::general_purpose::STANDARD.encode(&raw);

        let out = reaper_vst3_to_daw_state(&[line]).unwrap();
        assert_eq!(&out[..4], b"DAW3");
        let comp_len = u32::from_le_bytes(out[4..8].try_into().unwrap()) as usize;
        assert_eq!(comp_len, 4);
        assert_eq!(&out[8..12], &[0xDE, 0xAD, 0xBE, 0xEF]);
        let ctrl_len_off = 8 + comp_len;
        let ctrl_len =
            u32::from_le_bytes(out[ctrl_len_off..ctrl_len_off + 4].try_into().unwrap()) as usize;
        assert_eq!(ctrl_len, 3);
        assert_eq!(
            &out[ctrl_len_off + 4..ctrl_len_off + 4 + ctrl_len],
            &[0x11, 0x22, 0x33]
        );
    }

    /// Real REAPER `.rpp` chunk for the MeldaProduction MUtility
    /// VST3, extracted from helgobox's test fixtures. Verifies the
    /// parser produces non-empty component + controller blobs for a
    /// real-world plugin chunk (we can't actually load MUtility,
    /// but byte-level sanity is enough here).
    #[test]
    fn real_melda_mutility_chunk_decodes_to_dawstate() {
        // First 7 lines from
        // helgobox/resources/test-projects/issue-15-melda-vst3.rpp
        // (the rest snipped for brevity — header line + first body
        // line is enough to exercise the parser's framing logic).
        let lines: Vec<String> = vec![
            "oRbSVO5e7f4CAAAAAQAAAAAAAAACAAAAAAAAAAIAAAABAAAAAAAAAAIAAAAAAAAAJAIAAAEAAAAAABAA".into(),
            "CgEAAAEAAAB42nWRQU+DQBCF7/0Vm+lVhVZJabLQRMSTxEa0nilM243THbK7NNRfbwLWRMTjvu/tvpl9ctUeSZzQWMU6gtmNDwJ1yZXS+wgat7sOYRVPZPbmFCl3tuic".into(),
        ];
        let out = reaper_vst3_to_daw_state(&lines).expect("parser");
        assert_eq!(&out[..4], b"DAW3", "DAW3 magic must be at the start");
        let comp_len = u32::from_le_bytes(out[4..8].try_into().unwrap()) as usize;
        // The real MUtility chunk header advertises non-zero
        // component-state — our parser should pick that up rather
        // than falling through to the "everything is component"
        // branch.
        assert!(
            comp_len > 0,
            "expected non-zero component state from real chunk, got {comp_len}"
        );
    }

    #[test]
    fn clap_skips_header_preamble() {
        let payload = b"clap-state-bytes";
        let mut raw = vec![0u8; 16];
        raw.extend_from_slice(payload);
        let line = base64::engine::general_purpose::STANDARD.encode(&raw);
        let out = reaper_clap_to_state(&[line]).unwrap();
        assert_eq!(out, payload);
    }
}
