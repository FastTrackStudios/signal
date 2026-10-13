//! Extract per-instance plugin state blobs from the session.
//!
//! Some instruments serialize their entire patch as a self-contained,
//! host-portable blob (identical bytes in AAX / VST3 / AU). For those, the
//! converter can transplant the blob into Reaper's plugin chunk verbatim — no
//! per-parameter mapping. This pass locates such blobs and tags each with its
//! owning track so the converter can re-emit it on the matching Reaper track.
//!
//! ## Omnisphere (`<SynthMaster …>`)
//!
//! Pro Tools stores an instrument track's plugin state in a per-track
//! container block `0x2621`:
//! - the **track name** is a length-prefixed string at payload `+18`
//!   (u32 little-endian length, then the bytes);
//! - the Omnisphere state is the XML document from `<SynthMaster vers=` to the
//!   single `</SynthMaster>` close (note: `<SynthMaster` alone also matches the
//!   nested `<SynthMasterEngineParamBlock`, so key on `<SynthMaster vers=`).

use crate::block::Block;
use crate::cursor::Cursor;
use crate::types::{PluginInstanceState, PluginStateKind};

const TRACK_PLUGIN_CONTAINER: u16 = 0x2621;
const OMNI_ROOT: &[u8] = b"<SynthMaster vers=";
const OMNI_CLOSE: &[u8] = b"</SynthMaster>";

/// Scan the block tree for per-instance plugin states.
pub fn parse_plugin_states(blocks: &[Block], cursor: &Cursor<'_>) -> Vec<PluginInstanceState> {
    let data = cursor.data();
    let mut containers = Vec::new();
    collect(blocks, TRACK_PLUGIN_CONTAINER, &mut containers);

    let mut out = Vec::new();
    for c in containers {
        // Container payload starts 2 bytes after the content-type field.
        let payload = c.offset + 2;
        let Some(track_name) = read_len_prefixed_name(data, payload + 18) else {
            continue;
        };
        // Bound the search to this container.
        let end = (c.offset + 2 + c.block_size.saturating_sub(2) as usize).min(data.len());
        let body = &data[c.offset..end];
        if let Some(state) = extract_between(body, OMNI_ROOT, OMNI_CLOSE) {
            out.push(PluginInstanceState {
                track_name,
                kind: PluginStateKind::Omnisphere,
                state,
            });
        }
    }
    out
}

/// Read a `u32`-length-prefixed UTF-8 name at `at`, validating it is a sane
/// printable track name.
fn read_len_prefixed_name(data: &[u8], at: usize) -> Option<String> {
    if at + 4 > data.len() {
        return None;
    }
    let len = u32::from_le_bytes(data[at..at + 4].try_into().ok()?) as usize;
    if len == 0 || len > 128 || at + 4 + len > data.len() {
        return None;
    }
    let bytes = &data[at + 4..at + 4 + len];
    if !bytes.iter().all(|&b| (0x20..0x7f).contains(&b)) {
        return None;
    }
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// Return the bytes from `open` through the end of the first `close` after it.
fn extract_between(hay: &[u8], open: &[u8], close: &[u8]) -> Option<Vec<u8>> {
    let start = find(hay, open, 0)?;
    let close_at = find(hay, close, start)? + close.len();
    Some(hay[start..close_at].to_vec())
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    (from..=hay.len() - needle.len()).find(|&i| &hay[i..i + needle.len()] == needle)
}

fn collect<'a>(blocks: &'a [Block], ct_raw: u16, out: &mut Vec<&'a Block>) {
    for b in blocks {
        if b.content_type_raw == ct_raw {
            out.push(b);
        }
        collect(&b.children, ct_raw, out);
    }
}
