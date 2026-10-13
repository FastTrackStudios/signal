//! Proof-of-concept: extract an Omnisphere patch from a PTX (AAX) session and
//! synthesize a Reaper VST3 chunk so Reaper/Omnisphere can load it.
//!
//! Usage: cargo run -p daw-reaper --example omni_convert_test -- <in.ptx> <out.rpp>
//!
//! Validates the "portable state blob" hypothesis end-to-end: open <out.rpp>
//! in Reaper and confirm the Omnisphere instance loads the converted patch.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;

// Omnisphere VST3 chunk template constants (captured from a real Reaper-saved
// Omnisphere instance; patch-independent framing).
const VST_LINE: &str = "<VST \"VST3i: Omnisphere (Spectrasonics)\" Omnisphere.vst3 0 \"\" 103502701{84E8DE5F9255222296FAE4133C935A18} \"\"";
const SEG0_B64: &str = "bVMrBu5e7f4AAAAAEgAAAAEAAAAAAAAAAgAAAAAAAAAEAAAAAAAAAAgAAAAAAAAAEAAAAAAAAAAgAAAAAAAAAEAAAAAAAAAAgAAAAAAAAAAAAQAAAAAAAAACAAAAAAAAAAQAAAAAAAAACAAAAAAAAAAQAAAAAAAAACAAAAAAAAAAQAAAAAAAAACAAAAAAAAAAAABAAAAAAAAAAIAAAAAAFLCBQABAAAAAAAAAA==";
const SEG1_HDR_MID_B64: &str = "AQAAAP/JmjsAAAAAAQAAAAAAAAA="; // 20 bytes between size@0 and size@24
const SEG1_HDR_TAIL_B64: &str = "AAAAAA=="; // 4 bytes after size@24
const SEG1_TRAILER_B64: &str = "CiAAAAAAAAAAAAAAAAAAAAAAAAAAAABKVUNFUHJpdmF0ZURhdGEAAAAAAAAAAA==";
const SEG2_B64: &str = "AFByb2dyYW0gMQAAAAAA";

/// Extract the first Omnisphere XML state document from decrypted PTX bytes.
/// Returns the bytes from `<SynthMaster` through `</SynthMaster>\n ` inclusive.
fn extract_omni_xml(data: &[u8], instance: usize) -> Option<&[u8]> {
    // The real root tag is `<SynthMaster vers=` — `<SynthMaster` alone also
    // matches the nested `<SynthMasterEngineParamBlock`.
    let mut from = 0;
    let mut start = None;
    for _ in 0..=instance {
        let p = find(data, b"<SynthMaster vers=", from)?;
        start = Some(p);
        from = p + 1;
    }
    let start = start?;
    // The XML document ends at the single `</SynthMaster>` close; the trailing
    // "\n " that follows belongs to the host (JUCE) trailer framing, which our
    // template supplies as a constant.
    let end = find(data, b"</SynthMaster>", start)? + b"</SynthMaster>".len();
    Some(&data[start..end])
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    (from..=hay.len().saturating_sub(needle.len())).find(|&i| &hay[i..i + needle.len()] == needle)
}

/// Build the Reaper VST3 chunk (3 base64 segments) from an Omnisphere XML blob.
fn synth_vst3_chunk(xml: &[u8]) -> String {
    let mut seg0 = B64.decode(SEG0_B64).unwrap();
    let hdr_mid = B64.decode(SEG1_HDR_MID_B64).unwrap();
    let hdr_tail = B64.decode(SEG1_HDR_TAIL_B64).unwrap();
    let trailer = B64.decode(SEG1_TRAILER_B64).unwrap();
    let seg2 = B64.decode(SEG2_B64).unwrap();

    let l = xml.len() as u32;
    // seg1 = [u32 size@0 = L+62][20B mid][u32 size@24 = L+3][4B tail][XML][46B trailer]
    let mut seg1 = Vec::with_capacity(xml.len() + 82);
    seg1.extend_from_slice(&(l + 62).to_le_bytes());
    seg1.extend_from_slice(&hdr_mid);
    seg1.extend_from_slice(&(l + 3).to_le_bytes());
    seg1.extend_from_slice(&hdr_tail);
    seg1.extend_from_slice(xml);
    seg1.extend_from_slice(&trailer);

    // CRITICAL: seg0 (the VST3 component header) embeds the seg1/state length
    // at byte offset 160. If left at the template's value, Reaper reads a
    // truncated state and the plugin loads blank/silent. Patch it to OUR
    // seg1 length.
    seg0[160..164].copy_from_slice(&(seg1.len() as u32).to_le_bytes());

    // Each segment base64-encoded independently, wrapped at 128 chars, each
    // starting on a fresh line (matches Reaper's writer).
    let mut out = String::new();
    for seg in [&seg0[..], &seg1[..], &seg2[..]] {
        let b = B64.encode(seg);
        for chunk in b.as_bytes().chunks(128) {
            out.push_str("        ");
            out.push_str(std::str::from_utf8(chunk).unwrap());
            out.push('\n');
        }
    }
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::env::args()
        .nth(1)
        .ok_or("usage: <in.ptx> <out.rpp> [instance]")?;
    let output = std::env::args()
        .nth(2)
        .ok_or("usage: <in.ptx> <out.rpp> [instance]")?;
    let instance: usize = std::env::args()
        .nth(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let raw = std::fs::read(&input)?;
    let session = dawfile_protools::parse_raw(raw).map_err(|e| format!("{e:?}"))?;
    let data = session.cursor().data();

    let xml = extract_omni_xml(data, instance).ok_or("no Omnisphere <SynthMaster> state found")?;
    eprintln!("extracted Omnisphere XML: {} bytes", xml.len());
    eprintln!(
        "  head: {}",
        String::from_utf8_lossy(&xml[..xml.len().min(60)])
    );

    let chunk = synth_vst3_chunk(xml);

    let rpp = format!(
        "<REAPER_PROJECT 0.1 \"7.0\" 0\n  TEMPO 120 4 4\n  <TRACK\n    NAME \"Omnisphere (converted)\"\n    TRACKHEIGHT 0 0 0 0 0 0\n    <FXCHAIN\n      SHOW 0\n      LASTSEL 0\n      DOCKED 0\n      {VST_LINE}\n{chunk}      >\n      PRESETNAME \"Program 1\"\n      FLOATPOS 0 0 0 0\n      FXID {{00000000-0000-0000-0000-000000000000}}\n      WAK 0 0\n    >\n  >\n>\n"
    );
    std::fs::write(&output, &rpp)?;
    eprintln!("wrote {output} ({} bytes)", rpp.len());
    Ok(())
}
