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

// ── Patches (Gig Performer "presets") ────────────────────────────────────────

/// One patch: a named scene over a rackspace's widgets.
///
/// Gig Performer calls these PRESETs. Each is a flat list of
/// `widgetId -> value`, so the widget table is what makes them legible —
/// widget 493 means nothing, `The Giant = 1.0` means the patch plays The Giant.
#[derive(Debug, Clone)]
pub struct GigPreset {
    pub name: String,
    pub rackspace: String,
    /// Resolved `(widget caption, value)` for every param the patch sets.
    pub params: Vec<(String, f32)>,
}

impl GigPreset {
    /// Value of the first param with this caption.
    pub fn get(&self, caption: &str) -> Option<f32> {
        self.params
            .iter()
            .find(|(c, _)| c == caption)
            .map(|(_, v)| *v)
    }

    /// Captions whose value reads as "on" (>= 0.5), restricted to `names`.
    /// This is how you recover which instruments a patch actually loads.
    pub fn enabled<'a>(&self, names: &[&'a str]) -> Vec<&'a str> {
        names
            .iter()
            .copied()
            .filter(|n| self.get(n).is_some_and(|v| v >= 0.5))
            .collect()
    }
}

/// One song in the gig's library.
#[derive(Debug, Clone)]
pub struct GigSong {
    pub name: String,
    pub artist: String,
    pub bpm: f32,
    pub sig_num: u32,
    pub sig_den: u32,
    /// The key as Gig Performer stores it (`rootNote` 0..11, plus a minor
    /// flag). **Frequently stale** — see [`GigSong::key_from_name`].
    pub root_note: u8,
    pub minor: bool,
    pub transpose: i32,
    /// `(part name, rackspace, variation)` in running order.
    pub parts: Vec<(String, String, String)>,
}

const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

impl GigSong {
    /// The stored key — `rootNote` rendered as a note name plus `m` for minor.
    pub fn stored_key(&self) -> String {
        format!(
            "{}{}",
            NOTE_NAMES[self.root_note as usize % 12],
            if self.minor { "m" } else { "" }
        )
    }

    /// The key from the song title's `" - <key>"` suffix, which is the field
    /// a human actually maintains.
    ///
    /// Prefer this over [`GigSong::stored_key`]: in the reference gig,
    /// `rootNote` disagreed with the title on 6 of 19 titled songs (and
    /// `transpose` was 0 throughout, so it does not explain the gap). A title
    /// says `Center - D` while `rootNote` says G.
    pub fn key_from_name(&self) -> Option<&str> {
        let suffix = self.name.rsplit_once(" - ")?.1.trim();
        let head = suffix.trim_end_matches('m');
        let ok = matches!(head.len(), 1..=2)
            && head.starts_with(|c: char| c.is_ascii_uppercase() && c <= 'G')
            && head[1..].chars().all(|c| c == '#' || c == 'b');
        ok.then_some(suffix)
    }

    /// Title with the trailing key suffix removed.
    pub fn title(&self) -> &str {
        match (self.key_from_name(), self.name.rsplit_once(" - ")) {
            (Some(_), Some((head, _))) => head.trim(),
            _ => self.name.as_str(),
        }
    }
}

/// One setlist: an ordered run of song names.
#[derive(Debug, Clone)]
pub struct GigSetlist {
    pub name: String,
    pub songs: Vec<String>,
}

/// A crude document-order walk yielding `(depth, tag, open_tag_text)` for
/// element opens, and closes as `None` tags — enough to attribute children to
/// parents without building a tree.
fn walk_tags(xml: &str, mut f: impl FnMut(&str, &str, bool)) {
    let mut rest = xml;
    while let Some(lt) = rest.find('<') {
        rest = &rest[lt..];
        let Some(gt) = rest.find('>') else { break };
        let tag = &rest[..=gt];
        let body = tag.trim_start_matches('<').trim_end_matches('>');
        if let Some(name) = body.strip_prefix('/') {
            f(name.trim(), tag, false);
        } else if !body.starts_with('?') && !body.starts_with('!') {
            let name = body
                .split([' ', '\t', '\r', '\n', '/'])
                .next()
                .unwrap_or(body);
            f(name, tag, true);
            // Self-closing elements close immediately.
            if body.ends_with('/') {
                f(name, tag, false);
            }
        }
        rest = &rest[gt + 1..];
    }
}

/// Read the patch list, with widget ids resolved to their captions.
pub fn read_presets(xml: &str) -> Vec<GigPreset> {
    // Pass 1: widget id -> caption.
    let mut captions: Vec<(String, String)> = Vec::new();
    walk_tags(xml, |name, tag, open| {
        if open && name == "WIDGET" {
            if let Some(id) = attr(tag, "id") {
                captions.push((id, attr(tag, "caption").unwrap_or_default()));
            }
        }
    });
    let caption_of = |id: &str| -> Option<&str> {
        captions
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, c)| c.trim())
    };

    // Pass 2: presets, attributed to the enclosing rackspace.
    let mut out: Vec<GigPreset> = Vec::new();
    let mut rackspace = String::new();
    let mut in_preset = false;
    walk_tags(xml, |name, tag, open| match (name, open) {
        ("GLOBALRACKSPACE" | "RACKSPACE", true) => {
            rackspace = attr(tag, "name").unwrap_or_default();
        }
        ("PRESET", true) => {
            in_preset = true;
            out.push(GigPreset {
                name: attr(tag, "name").unwrap_or_default(),
                rackspace: rackspace.clone(),
                params: Vec::new(),
            });
        }
        ("PRESET", false) => in_preset = false,
        ("PARAM", true) if in_preset => {
            let (Some(id), Some(v)) = (attr(tag, "widgetId"), attr(tag, "value")) else {
                return;
            };
            let Ok(v) = v.parse::<f32>() else { return };
            if let (Some(cap), Some(p)) = (caption_of(&id), out.last_mut()) {
                if !cap.is_empty() {
                    p.params.push((cap.to_string(), v));
                }
            }
        }
        _ => {}
    });
    out
}

/// Read the song library. Songs recur across setlists; this returns every
/// occurrence, so dedupe by name for the library view.
pub fn read_songs(xml: &str) -> Vec<GigSong> {
    let mut out: Vec<GigSong> = Vec::new();
    let num = |tag: &str, k: &str, d: f32| attr(tag, k).and_then(|v| v.parse().ok()).unwrap_or(d);
    walk_tags(xml, |name, tag, open| match (name, open) {
        ("SONG", true) => out.push(GigSong {
            name: attr(tag, "songName").unwrap_or_default(),
            artist: attr(tag, "songArtist").unwrap_or_default(),
            bpm: num(tag, "bpm", 0.0),
            sig_num: num(tag, "sigNum", 4.0) as u32,
            sig_den: num(tag, "sigDen", 4.0) as u32,
            root_note: num(tag, "rootNote", 0.0) as u8,
            minor: attr(tag, "usesMinorKey").as_deref() == Some("true"),
            transpose: num(tag, "transpose", 0.0) as i32,
            parts: Vec::new(),
        }),
        ("SONG_PART", true) => {
            if let Some(s) = out.last_mut() {
                s.parts.push((
                    attr(tag, "songPartName").unwrap_or_default(),
                    attr(tag, "rackspace").unwrap_or_default(),
                    attr(tag, "variation").unwrap_or_default(),
                ));
            }
        }
        _ => {}
    });
    out
}

/// Read the setlists, each an ordered run of song names.
pub fn read_setlists(xml: &str) -> Vec<GigSetlist> {
    let mut out: Vec<GigSetlist> = Vec::new();
    let mut depth_in_setlist = false;
    walk_tags(xml, |name, tag, open| match (name, open) {
        ("SETLIST", true) => {
            depth_in_setlist = true;
            out.push(GigSetlist {
                name: attr(tag, "name").unwrap_or_default(),
                songs: Vec::new(),
            });
        }
        ("SETLIST", false) => depth_in_setlist = false,
        ("SONG", true) if depth_in_setlist => {
            if let Some(sl) = out.last_mut() {
                sl.songs.push(attr(tag, "songName").unwrap_or_default());
            }
        }
        _ => {}
    });
    out
}
