//! **Gig Performer `.gig` reader** — recover hosted-plugin state out of a Gig
//! Performer 5 gig file, so a rig built in GP can be read (and ported) instead
//! of transcribed off the screen.
//!
//! The file is plain XML — `GIGRACK` → `GLOBALRACKSPACE` / `RACKSPACE*` →
//! `PROCESSOR*` — and the human-facing rig map (`prop_str_nodeName`, the
//! `PLUGIN` descriptors) is already readable there. What is *not* readable is
//! the plugin state, which hides under four nested encodings:
//!
//! ```text
//! <PROCESSORSTATEZ>  "<size>.<chars>"     JUCE MemoryBlock::toBase64Encoding
//!   └─ zlib                                the "Z" in STATEZ
//!       └─ "VC2!" <VST3PluginState><IComponent>…   JUCE VST3 state wrapper
//!           └─ "<size>.<chars>"           JUCE base64 again
//!               └─ the plugin's own chunk (Omnisphere DAW3 body, Kontakt
//!                  "hsin", Arturia "22 serialization", …)
//! ```
//!
//! The outer/inner base64 is **not** RFC 4648: JUCE uses its own 64-character
//! table beginning with `.`, packs six bits at a time **LSB-first** into the
//! output bytes, and prefixes the decoded byte count and a `.`. Feeding it to
//! a standard base64 decoder yields plausible-looking garbage, which is what
//! makes this format read as encrypted when it is merely unusual.
//!
//! For Omnisphere the innermost chunk is the same `SynthMaster` Multi XML that
//! [`crate::omni_import::state`] already handles — minus the leading `DAW3` +
//! length words, because `IComponent::getState` starts at the magic. So the
//! existing patch reader takes the output of this module unchanged.

use crate::omni_import::state;

// ── JUCE MemoryBlock base64 ──────────────────────────────────────────────────

/// JUCE's `base64EncodingTable`: `.`, `A-Z`, `a-z`, `0-9`, `+`.
const JUCE_TABLE: &[u8; 64] = b".ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+";

fn juce_index(c: u8) -> Option<u32> {
    JUCE_TABLE.iter().position(|&t| t == c).map(|i| i as u32)
}

/// Decode a `"<size>.<chars>"` string written by JUCE's
/// `MemoryBlock::toBase64Encoding()`.
///
/// Six bits per character, written LSB-first into the byte buffer (JUCE's
/// `setBitRange`), with the decoded length carried in the decimal prefix
/// rather than by padding.
pub fn juce_base64_decode(s: &str) -> Result<Vec<u8>, String> {
    let s = s.trim();
    let dot = s.find('.').ok_or("no '.' length separator")?;
    let size: usize = s[..dot]
        .parse()
        .map_err(|_| format!("bad length prefix {:?}", &s[..dot]))?;
    let mut out = vec![0u8; size];
    for (i, c) in s.as_bytes()[dot + 1..].iter().enumerate() {
        let mut v = juce_index(*c).ok_or_else(|| format!("bad char {:?} at {i}", *c as char))?;
        let mut bit = i * 6;
        let mut left = 6;
        while left > 0 {
            let (byte, off) = (bit >> 3, bit & 7);
            if byte >= size {
                break;
            }
            let take = left.min(8 - off);
            let mask = ((0xffu32 >> (8 - take)) << off) as u8;
            out[byte] = (out[byte] & !mask) | (((v << off) as u8) & mask);
            v >>= take;
            bit += take;
            left -= take;
        }
    }
    Ok(out)
}

// ── The gig file ─────────────────────────────────────────────────────────────

/// One hosted plugin recovered from a gig file.
#[derive(Debug, Clone)]
pub struct GigProcessor {
    /// Rackspace the processor lives in — `"GLOBAL RACKSPACE"` or a song scene.
    pub rackspace: String,
    /// The name the user gave the block in GP (`prop_str_nodeName`), e.g.
    /// `"Omni Pads"`, `"NI Pianos"`, `"EQ: The Grandeur"`.
    pub node_name: String,
    /// Plugin as GP knows it — `"Omnisphere"`, `"Kontakt 8"`, `"Pro-Q 3"`.
    pub plugin: String,
    /// The plugin's own state chunk, fully unwrapped.
    pub state: Vec<u8>,
}

impl GigProcessor {
    /// The `SynthMaster` Multi XML, for an Omnisphere processor.
    ///
    /// Returns `None` for any other plugin — a Kontakt chunk (`hsin`) or an
    /// Arturia one (`22 serialization`) is a different format entirely.
    pub fn omni_multi_xml(&self) -> Option<String> {
        state::parse_state(&self.state).ok()
    }
}

fn attr(open_tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let at = open_tag.find(&needle)? + needle.len();
    let end = at + open_tag[at..].find('"')?;
    Some(unescape(&open_tag[at..end]))
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Read every hosted plugin out of a `.gig` file's XML.
///
/// A linear scan rather than a tree parse: gig files put the state payload in
/// element *text*, which the AmberPart parser deliberately drops, and the
/// document order (`RACKSPACE` → `PROCESSOR` → `PLUGIN` → `PROCESSORSTATEZ`)
/// makes attribution unambiguous without one.
///
/// Processors whose state fails to decode are skipped rather than fatal — a
/// gig holds dozens of them and one unreadable utility block should not cost
/// you the instruments.
pub fn read_gig(xml: &str) -> Vec<GigProcessor> {
    let mut out = Vec::new();
    let (mut rackspace, mut node, mut plugin) = (String::new(), String::new(), String::new());
    let mut rest = xml;
    while let Some(lt) = rest.find('<') {
        rest = &rest[lt..];
        let Some(gt) = rest.find('>') else { break };
        let tag = &rest[..=gt];
        if tag.starts_with("<GLOBALRACKSPACE") || tag.starts_with("<RACKSPACE") {
            rackspace = attr(tag, "name").unwrap_or_default();
        } else if tag.starts_with("<PROCESSOR ") {
            node = attr(tag, "prop_str_nodeName").unwrap_or_default();
            plugin.clear();
        } else if tag.starts_with("<PLUGIN ") {
            plugin = attr(tag, "name").unwrap_or_default();
        } else if tag.starts_with("<PROCESSORSTATEZ") {
            let body = &rest[gt + 1..];
            if let Some(close) = body.find("</PROCESSORSTATEZ>") {
                if let Ok(state) = unwrap_state(&body[..close]) {
                    out.push(GigProcessor {
                        rackspace: rackspace.clone(),
                        node_name: node.clone(),
                        plugin: plugin.clone(),
                        state,
                    });
                }
                rest = &body[close..];
                continue;
            }
        }
        rest = &rest[gt + 1..];
    }
    out
}

/// Peel the three host-side layers off a `PROCESSORSTATEZ` body, leaving the
/// plugin's own chunk.
fn unwrap_state(b64: &str) -> Result<Vec<u8>, String> {
    let compressed = juce_base64_decode(b64)?;
    let inflated = inflate(&compressed)?;
    // JUCE VST3 wrapper: "VC2!" then an XML envelope whose <IComponent> text
    // is the plugin chunk in JUCE base64 again. A non-VST3 plugin has no
    // wrapper, so an absent envelope means we are already at the chunk.
    let text = String::from_utf8_lossy(&inflated);
    let Some(open) = text.find("<IComponent>") else {
        return Ok(inflated);
    };
    let start = open + "<IComponent>".len();
    let end = text[start..]
        .find("</IComponent>")
        .ok_or("unterminated <IComponent>")?
        + start;
    juce_base64_decode(&text[start..end])
}

fn inflate(bytes: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(bytes)
        .read_to_end(&mut out)
        .map_err(|e| format!("zlib inflate failed: {e}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The forward direction of JUCE's encoding, so the decoder is tested
    /// against the algorithm it claims to invert rather than against itself.
    fn juce_base64_encode(data: &[u8]) -> String {
        let num_chars = ((data.len() * 8) + 5) / 6;
        let mut s = format!("{}.", data.len());
        for i in 0..num_chars {
            let mut v = 0u32;
            for b in 0..6 {
                let bit = i * 6 + b;
                let (byte, off) = (bit >> 3, bit & 7);
                if byte < data.len() && (data[byte] >> off) & 1 == 1 {
                    v |= 1 << b;
                }
            }
            s.push(JUCE_TABLE[v as usize] as char);
        }
        s
    }

    #[test]
    fn juce_base64_roundtrips() {
        for case in [
            &b""[..],
            b"\x00",
            b"\xff",
            b"DAW3",
            b"the quick brown fox jumps over the lazy dog",
            &[0xf7, 0xbe, 0xb8, 0xd7, 0x4d, 0xe8, 0x32, 0xae][..],
        ] {
            let enc = juce_base64_encode(case);
            assert_eq!(juce_base64_decode(&enc).unwrap(), case, "roundtrip {enc}");
        }
    }

    #[test]
    fn juce_base64_is_not_rfc4648() {
        // A real prefix from a gig file. Standard base64 would reject or
        // mis-decode it; the length prefix and '.' alphabet are the tell.
        let enc = juce_base64_encode(b"\x78\xda\x0b\x73\x36\x52");
        assert!(enc.starts_with("6."), "length-prefixed, got {enc}");
        assert!(
            juce_base64_decode("95.3o8ByYiT").is_ok(),
            "leading digits are a length, not data"
        );
    }

    #[test]
    fn juce_base64_rejects_foreign_alphabet() {
        // '/' is in RFC 4648 but not in JUCE's table — decoding must fail
        // loudly rather than silently produce a wrong byte.
        assert!(juce_base64_decode("3.aa/").is_err());
        assert!(juce_base64_decode("nolength").is_err());
    }

    #[test]
    fn read_gig_attributes_processors_to_their_rackspace() {
        let inner = juce_base64_encode(b"PLUGINCHUNK");
        let envelope =
            format!("VC2!<VST3PluginState><IComponent>{inner}</IComponent></VST3PluginState>");
        let compressed = {
            use std::io::Write;
            let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            e.write_all(envelope.as_bytes()).unwrap();
            e.finish().unwrap()
        };
        let statez = juce_base64_encode(&compressed);
        let gig = format!(
            r#"<GIGRACK>
<GLOBALRACKSPACE name="GLOBAL RACKSPACE">
  <PROCESSOR prop_str_nodeName="Omni Pads">
    <PLUGIN name="Omnisphere" manufacturer="Spectrasonics" />
    <PROCESSORSTATEZ>{statez}</PROCESSORSTATEZ>
  </PROCESSOR>
</GLOBALRACKSPACE>
<RACKSPACE name="Massive Worship">
  <PROCESSOR prop_str_nodeName="Omni Synths">
    <PLUGIN name="Omnisphere" manufacturer="Spectrasonics" />
    <PROCESSORSTATEZ>{statez}</PROCESSORSTATEZ>
  </PROCESSOR>
</RACKSPACE>
</GIGRACK>"#
        );
        let procs = read_gig(&gig);
        assert_eq!(procs.len(), 2);
        assert_eq!(procs[0].rackspace, "GLOBAL RACKSPACE");
        assert_eq!(procs[0].node_name, "Omni Pads");
        assert_eq!(procs[0].plugin, "Omnisphere");
        assert_eq!(procs[0].state, b"PLUGINCHUNK");
        assert_eq!(procs[1].rackspace, "Massive Worship");
        assert_eq!(procs[1].node_name, "Omni Synths");
    }
}
