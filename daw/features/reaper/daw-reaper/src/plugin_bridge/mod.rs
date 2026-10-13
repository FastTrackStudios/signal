//! Plugin-state bridge: convert a Pro Tools plugin instance's saved state into
//! a Reaper FX block.
//!
//! Some instruments serialize their entire patch as a **format-portable blob**
//! — identical bytes whether hosted as AAX (Pro Tools) or VST3/AU (Reaper). For
//! those we transplant the blob into Reaper's plugin chunk verbatim, no
//! per-parameter mapping. This is the registry of supported plugins.
//!
//! Currently: Omnisphere (`PortableChunk`). The design keeps each plugin's
//! mapping as plain data (a [`PortableChunkTemplate`]) so the registry can
//! later be loaded from community-editable config files. See
//! `docs/pt-plugin-bridge.md`.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use dawfile_protools::types::{PluginInstanceState, PluginStateKind};

/// A captured Reaper VST3-chunk template for a plugin whose state is a
/// self-contained, portable blob. The chunk is three base64 segments; only the
/// inner state (`seg1`) and two embedded size fields vary per patch.
pub struct PortableChunkTemplate {
    /// The `<VST …>` header line (plugin identity: VST3 name, file, class GUID).
    pub vst_line: &'static str,
    /// seg0 — VST3 component header (class-ID magic + IO table), base64.
    pub seg0_b64: &'static str,
    /// seg1 header bytes between `size@0` and `size@24`, base64 (20 B).
    pub seg1_hdr_mid_b64: &'static str,
    /// seg1 header bytes after `size@24`, base64 (4 B).
    pub seg1_hdr_tail_b64: &'static str,
    /// seg1 trailer after the state payload (JUCE private data), base64.
    pub seg1_trailer_b64: &'static str,
    /// seg2 — program name block, base64.
    pub seg2_b64: &'static str,
    /// Byte offset in seg0 of the u32 (LE) seg1/state length. MUST be patched
    /// to the synthesized seg1 length or the host reads a truncated state.
    pub seg0_state_len_offset: usize,
    /// `size@0` = state_xml_len + this.
    pub size0_delta: u32,
    /// `size@24` = state_xml_len + this.
    pub size24_delta: u32,
    /// Preset name to write (`PRESETNAME`).
    pub preset_name: &'static str,
}

/// A converted FX ready to attach to a Reaper track.
pub struct ConvertedFx {
    /// The complete raw `<VST …> … >` block (header line + base64 chunk lines).
    pub raw_block: String,
    /// Preset name for the `PRESETNAME` line.
    pub preset_name: String,
}

/// Omnisphere (Spectrasonics). Hosts Keyscape / Trilian / Stylus libraries too.
/// Template captured from a real Reaper-saved Omnisphere instance.
const OMNISPHERE: PortableChunkTemplate = PortableChunkTemplate {
    vst_line: "<VST \"VST3i: Omnisphere (Spectrasonics)\" Omnisphere.vst3 0 \"\" 103502701{84E8DE5F9255222296FAE4133C935A18} \"\"",
    seg0_b64: "bVMrBu5e7f4AAAAAEgAAAAEAAAAAAAAAAgAAAAAAAAAEAAAAAAAAAAgAAAAAAAAAEAAAAAAAAAAgAAAAAAAAAEAAAAAAAAAAgAAAAAAAAAAAAQAAAAAAAAACAAAAAAAAAAQAAAAAAAAACAAAAAAAAAAQAAAAAAAAACAAAAAAAAAAQAAAAAAAAACAAAAAAAAAAAABAAAAAAAAAAIAAAAAAFLCBQABAAAAAAAAAA==",
    seg1_hdr_mid_b64: "AQAAAP/JmjsAAAAAAQAAAAAAAAA=",
    seg1_hdr_tail_b64: "AAAAAA==",
    seg1_trailer_b64: "CiAAAAAAAAAAAAAAAAAAAAAAAAAAAABKVUNFUHJpdmF0ZURhdGEAAAAAAAAAAA==",
    seg2_b64: "AFByb2dyYW0gMQAAAAAA",
    seg0_state_len_offset: 160,
    size0_delta: 62,
    size24_delta: 3,
    preset_name: "Program 1",
};

/// The registry: map a plugin-state kind to its conversion template.
fn template_for(kind: PluginStateKind) -> Option<&'static PortableChunkTemplate> {
    match kind {
        PluginStateKind::Omnisphere => Some(&OMNISPHERE),
    }
}

/// Convert a parsed PT plugin instance into a Reaper FX block, if the plugin is
/// supported. Returns `None` for unsupported plugins (leaving them un-converted
/// rather than emitting a broken FX).
pub fn convert(state: &PluginInstanceState) -> Option<ConvertedFx> {
    let tpl = template_for(state.kind)?;
    Some(ConvertedFx {
        raw_block: synth_portable_chunk(tpl, &state.state),
        preset_name: tpl.preset_name.to_string(),
    })
}

/// Build the `<VST …> … >` raw block: re-wrap the portable state blob into the
/// host's three-segment base64 chunk. (Lines carry no indentation; the FX
/// serializer re-indents them.)
fn synth_portable_chunk(tpl: &PortableChunkTemplate, xml: &[u8]) -> String {
    let mut seg0 = B64.decode(tpl.seg0_b64).unwrap();
    let hdr_mid = B64.decode(tpl.seg1_hdr_mid_b64).unwrap();
    let hdr_tail = B64.decode(tpl.seg1_hdr_tail_b64).unwrap();
    let trailer = B64.decode(tpl.seg1_trailer_b64).unwrap();
    let seg2 = B64.decode(tpl.seg2_b64).unwrap();

    let l = xml.len() as u32;
    // seg1 = [u32 size@0][mid hdr][u32 size@24][tail hdr][XML][trailer]
    let mut seg1 = Vec::with_capacity(xml.len() + 82);
    seg1.extend_from_slice(&(l + tpl.size0_delta).to_le_bytes());
    seg1.extend_from_slice(&hdr_mid);
    seg1.extend_from_slice(&(l + tpl.size24_delta).to_le_bytes());
    seg1.extend_from_slice(&hdr_tail);
    seg1.extend_from_slice(xml);
    seg1.extend_from_slice(&trailer);

    // seg0 embeds the seg1 length — patch it to ours.
    let off = tpl.seg0_state_len_offset;
    seg0[off..off + 4].copy_from_slice(&(seg1.len() as u32).to_le_bytes());

    // Each segment base64-encoded independently, wrapped at 128 chars.
    let mut out = String::from(tpl.vst_line);
    out.push('\n');
    for seg in [&seg0[..], &seg1[..], &seg2[..]] {
        let b = B64.encode(seg);
        for chunk in b.as_bytes().chunks(128) {
            out.push_str(std::str::from_utf8(chunk).unwrap());
            out.push('\n');
        }
    }
    out.push('>');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omnisphere_chunk_structure_and_size_patch() {
        let xml = b"<SynthMaster vers=\"3.0.2c\">test</SynthMaster>";
        let fx = convert(&PluginInstanceState {
            track_name: "Inst 1".into(),
            kind: PluginStateKind::Omnisphere,
            state: xml.to_vec(),
        })
        .unwrap();
        assert!(fx.raw_block.starts_with("<VST \"VST3i: Omnisphere"));
        assert!(fx.raw_block.trim_end().ends_with('>'));
        assert_eq!(fx.preset_name, "Program 1");

        // Decode seg0 (first segment, ends at its `==` padding) and confirm the
        // embedded state-length field was patched to OUR seg1 length.
        let body: String = fx
            .raw_block
            .lines()
            .skip(1)
            .take_while(|l| *l != ">")
            .collect();
        let seg0_end = body.find("==").unwrap() + 2;
        let seg0 = B64.decode(&body[..seg0_end]).unwrap();
        assert_eq!(&seg0[..8], b"mS+\x06\xee^\xed\xfe");
        let embedded = u32::from_le_bytes(seg0[160..164].try_into().unwrap()) as usize;
        let expected_seg1 = 32 + xml.len() + 46; // header + xml + trailer
        assert_eq!(embedded, expected_seg1);
    }
}
