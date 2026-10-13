//! REAPER's plugin-state framing inside a `.rpp` block.
//!
//! Neither format is documented, so both layouts below were read off real
//! files written by REAPER 7 — `~/.config/REAPER/TrackTemplates/Pro C.RTrackTemplate`
//! and `FTSComp Test.RfxChain` — and the field meanings checked against the
//! values they actually carry (the declared chunk length matches the decoded
//! body to the byte; the channel masks are `1, 2` for a stereo plugin).
//!
//! ## VST3
//!
//! A `<VST>` block's children are **three separate base64 streams**, not one.
//! That matters: they cannot be concatenated as text, because a stream that
//! does not land on a 3-byte boundary carries `=` padding in the middle of
//! the run. The header stream is always one line; the trailer is always the
//! last line; the body is everything between.
//!
//! ```text
//! header  (60 bytes for a 2-in/2-out plugin)
//!   u32     REAPER's plugin id hash        (repeated in the header line)
//!   u32     0xFEED5EEE                     (0xFEED5EEF for VST2)
//!   u32     input channel count  n
//!   u64[n]  input channel masks            (1, 2 for stereo)
//!   u32     output channel count  m
//!   u64[m]  output channel masks
//!   u32     body length
//!   u32     1
//!   u32     0x0000FFFF
//! body
//!   u32     IComponent state length  c
//!   u32     1
//!   u8[c]   IComponent state                (what the plugin's getState wrote)
//!   u32     IEditController state length    (0 when absent)
//!   u32     0
//! trailer
//!   6 zero bytes
//! ```
//!
//! ## CLAP
//!
//! Far simpler, and the reason the converter writes CLAP: a `<STATE>` child
//! block whose base64 is **exactly** the bytes `clap_plugin_state.save`
//! produced, with no framing of REAPER's own at all. No length, no id hash,
//! no class UID — REAPER resolves the plugin by the CLAP id string in the
//! header line, which we know at compile time.
//!
//! Pro-C 3's state starts at its own `FFBS` magic with nothing in front of
//! it, which is what settles this: the `u64` length that opens an FTS
//! plugin's `<STATE>` is nih-plug's, written by its own serializer, and
//! belongs to the state builder rather than to this layer.

use base64::Engine;

const B64: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::STANDARD;

/// REAPER wraps base64 payloads at this many characters per line.
const WRAP: usize = 128;

/// The magic in a VST3 chunk header's second word.
pub const VST3_MAGIC: u32 = 0xFEED_5EEE;
/// The same slot for a VST2 chunk.
pub const VST2_MAGIC: u32 = 0xFEED_5EEF;

#[derive(Debug, thiserror::Error)]
pub enum ChunkError {
    #[error("base64: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("chunk is too short to be {0}")]
    TooShort(&'static str),
    #[error("not a VST3 chunk (magic {0:#010x})")]
    NotVst3(u32),
    #[error("declared length {declared} does not match the {actual} bytes present")]
    LengthMismatch { declared: usize, actual: usize },
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, ChunkError> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
        .ok_or(ChunkError::TooShort("a length field"))
}

/// The header stream of a VST3 `<VST>` block, kept whole so a rewritten block
/// can reuse the routing fields it does not understand.
#[derive(Debug, Clone)]
pub struct Vst3Header {
    pub raw: Vec<u8>,
}

impl Vst3Header {
    /// Where the body length lives: the third word from the end.
    fn body_len_offset(&self) -> usize {
        self.raw.len().saturating_sub(12)
    }

    #[must_use]
    pub fn plugin_id(&self) -> u32 {
        u32_at(&self.raw, 0).unwrap_or(0)
    }

    #[must_use]
    pub fn magic(&self) -> u32 {
        u32_at(&self.raw, 4).unwrap_or(0)
    }

    #[must_use]
    pub fn body_len(&self) -> usize {
        u32_at(&self.raw, self.body_len_offset()).unwrap_or(0) as usize
    }

    fn set_body_len(&mut self, len: usize) {
        let at = self.body_len_offset();
        self.raw[at..at + 4].copy_from_slice(&(len as u32).to_le_bytes());
    }
}

/// A decoded VST3 `<VST>` block.
#[derive(Debug, Clone)]
pub struct Vst3Chunk {
    pub header: Vst3Header,
    /// The plugin's own `IComponent` state — the bytes to hand to a host, and
    /// the bytes a preset file's contents have to become.
    pub component: Vec<u8>,
    /// The `IEditController` state, usually empty.
    pub controller: Vec<u8>,
    pub trailer: Vec<u8>,
}

/// Decode the base64 lines of a `<VST>` block.
///
/// # Errors
///
/// Returns an error if the base64 cannot be decoded, if the magic number is incorrect,
/// or if the declared length does not match the actual data.
pub fn decode_vst3(lines: &[String]) -> Result<Vst3Chunk, ChunkError> {
    if lines.len() < 2 {
        return Err(ChunkError::TooShort("a VST3 chunk"));
    }
    let header = Vst3Header {
        raw: B64.decode(lines[0].trim())?,
    };
    if header.magic() != VST3_MAGIC {
        return Err(ChunkError::NotVst3(header.magic()));
    }
    let trailer = B64.decode(lines[lines.len() - 1].trim())?;
    let body = B64.decode(lines[1..lines.len() - 1].concat())?;
    if body.len() != header.body_len() {
        return Err(ChunkError::LengthMismatch {
            declared: header.body_len(),
            actual: body.len(),
        });
    }

    let comp_len = u32_at(&body, 0)? as usize;
    let comp_at = 8;
    let component = body
        .get(comp_at..comp_at + comp_len)
        .ok_or(ChunkError::TooShort("the component state"))?
        .to_vec();
    // The controller state is optional, and REAPER pads the tail with a
    // second zero word. Read it when it is there and shrug when it is not —
    // a plugin that keeps nothing in its controller is the common case.
    let ctrl_at = comp_at + comp_len;
    let controller = match u32_at(&body, ctrl_at) {
        Ok(n) if n > 0 => body
            .get(ctrl_at + 8..ctrl_at + 8 + n as usize)
            .unwrap_or_default()
            .to_vec(),
        _ => Vec::new(),
    };

    Ok(Vst3Chunk {
        header,
        component,
        controller,
        trailer,
    })
}

/// Re-encode a VST3 chunk, with the body length in the header corrected to
/// whatever the component state now weighs.
#[must_use]
pub fn encode_vst3(chunk: &Vst3Chunk) -> Vec<String> {
    let mut body = Vec::with_capacity(16 + chunk.component.len() + chunk.controller.len());
    body.extend_from_slice(&(chunk.component.len() as u32).to_le_bytes());
    body.extend_from_slice(&1u32.to_le_bytes());
    body.extend_from_slice(&chunk.component);
    body.extend_from_slice(&(chunk.controller.len() as u32).to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&chunk.controller);

    let mut header = chunk.header.clone();
    header.set_body_len(body.len());

    let mut lines = vec![B64.encode(&header.raw)];
    lines.extend(wrap(&B64.encode(&body)));
    lines.push(B64.encode(&chunk.trailer));
    lines
}

/// Decode a CLAP `<STATE>` block's base64 into the bytes the plugin saved.
///
/// # Errors
///
/// Returns an error if the base64 cannot be decoded.
pub fn decode_clap_state(lines: &[String]) -> Result<Vec<u8>, ChunkError> {
    Ok(B64.decode(lines.concat())?)
}

/// Encode plugin-saved bytes as a CLAP `<STATE>` body.
#[must_use]
pub fn encode_clap_state(state: &[u8]) -> Vec<String> {
    wrap(&B64.encode(state))
}

fn wrap(s: &str) -> Vec<String> {
    s.as_bytes()
        .chunks(WRAP)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpp::{Document, Node};

    /// The fixture is a real REAPER 7 track template: a `FabFilter` Pro-C 3
    /// CLAP between two JUCE VST3s. It is checked in because the framing
    /// below was read off it — a regression here means we have started
    /// guessing again.
    const FIXTURE: &str = include_str!("../../tests/fixtures/pro-c.RTrackTemplate");

    fn blocks(token: &str) -> Vec<crate::rpp::Block> {
        Document::parse(FIXTURE)
            .walk()
            .into_iter()
            .filter(|b| b.block.token() == token)
            .map(|b| b.block.clone())
            .collect()
    }

    #[test]
    fn a_project_file_round_trips_byte_for_byte() {
        let doc = Document::parse(FIXTURE);
        assert_eq!(doc.render(), FIXTURE);
    }

    #[test]
    fn the_vst3_header_is_the_layout_we_measured() {
        let b = &blocks("VST")[0];
        let chunk = decode_vst3(&b.base64_lines()).expect("decode");
        assert_eq!(chunk.header.raw.len(), 60);
        assert_eq!(chunk.header.magic(), VST3_MAGIC);
        // The id in the header's first word is the one the header *line*
        // repeats in decimal — that is what ties the two together.
        assert_eq!(chunk.header.plugin_id(), 635_903_665);
        assert_eq!(chunk.header.body_len(), 634);
        assert_eq!(chunk.component.len(), 618);
        assert!(chunk.controller.is_empty());
        assert_eq!(chunk.trailer, vec![0u8; 6]);
        // A JUCE plugin's VST3 state starts with JUCE's own chunk magic.
        assert_eq!(&chunk.component[..4], b"VC2!");
    }

    #[test]
    fn re_encoding_an_untouched_vst3_chunk_reproduces_it() {
        let b = &blocks("VST")[0];
        let original = b.base64_lines();
        let chunk = decode_vst3(&original).expect("decode");
        assert_eq!(encode_vst3(&chunk), original);
    }

    #[test]
    fn a_clap_state_is_the_plugins_own_bytes() {
        let clap = &blocks("CLAP")[0];
        let state_block = clap.child_block("STATE").expect("<STATE>");
        let state = decode_clap_state(&state_block.base64_lines()).expect("decode");
        // Pro-C 3 writes FabFilter's binary state format, and it starts at
        // byte zero — REAPER prepends nothing.
        assert_eq!(&state[..4], b"FFBS");
        assert_eq!(
            encode_clap_state(&state),
            state_block.base64_lines(),
            "re-encoding an untouched CLAP state reproduces it"
        );
    }

    #[test]
    fn quoting_survives_a_header_line() {
        use crate::rpp::{quote, split_fields, unquote};
        let line = r#"    <VST "VST3: Delta Expose (AP Mastering)" "Delta Expose.vst3" 0 "" 635903665{AB} """#;
        let f = split_fields(line);
        assert_eq!(f[0], "<VST");
        assert_eq!(unquote(&f[1]), "VST3: Delta Expose (AP Mastering)");
        assert_eq!(unquote(&f[4]), "");
        assert_eq!(quote(r#"say "hi""#), r#"'say "hi"'"#);
    }

    #[test]
    fn walk_reports_where_a_block_lives() {
        let doc = Document::parse(FIXTURE);
        let clap = doc
            .walk()
            .into_iter()
            .find(|b| b.block.token() == "CLAP")
            .expect("a CLAP block");
        assert_eq!(clap.path, vec!["TRACK", "FXCHAIN"]);
    }

    #[test]
    fn base64_lines_skips_nested_blocks() {
        let clap = &blocks("CLAP")[0];
        // `CFG …` is a leaf line and `<STATE>` a block; only the former shows
        // up, which is why the CLAP path has to descend into `<STATE>`.
        assert!(clap.base64_lines().iter().all(|l| l.starts_with("CFG")));
    }

    #[test]
    fn an_unrecognised_chunk_is_refused_rather_than_mangled() {
        let lines = vec!["AAAA".to_string(), "AAAA".to_string()];
        assert!(matches!(
            decode_vst3(&lines),
            Err(ChunkError::NotVst3(_) | ChunkError::TooShort(_))
        ));
    }

    #[test]
    fn node_is_either_a_line_or_a_block() {
        let doc = Document::parse("A 1\n<B\n  C 2\n>\n");
        assert!(matches!(doc.nodes[0], Node::Line(_)));
        assert!(matches!(doc.nodes[1], Node::Block(_)));
        assert_eq!(doc.render(), "A 1\n<B\n  C 2\n>\n");
    }
}
