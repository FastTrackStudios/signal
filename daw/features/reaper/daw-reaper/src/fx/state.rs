//! FX state-chunk capture/restore helpers. Bridges between REAPER's
//! per-FX `tag_chunk` API (works for VST/VST3) and the track-chunk
//! parsing fallback (required for CLAP) via `dawfile-reaper`.

use daw_proto::FxStateChunk;
use reaper_high::{FxChain, MAX_TRACK_CHUNK_SIZE, Track};
use reaper_medium::ChunkCacheHint;
use tracing::{debug, warn};

use super::tree::{container_child_fx_id, is_container_fx};
use super::util::{read_config_u32, safe_fx_call};

/// Find the byte offset of the closing `>` for an RPP block in
/// `block_text`. RPP nests with `<TAG` / standalone `>`.
pub fn find_block_end(block_text: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut offset = 0usize;

    for line in block_text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('<') {
            depth += 1;
        }
        if trimmed == ">" {
            depth -= 1;
            if depth == 0 {
                let gt_pos = line.rfind('>').unwrap();
                return Some(offset + gt_pos);
            }
        }
        offset += line.len() + 1;
    }
    None
}

/// Recursively capture FX state chunks for every plugin in the chain,
/// descending into containers. Containers themselves are skipped (their
/// state is the union of children).
pub fn capture_fx_state_recursive(
    chain: &FxChain,
    track: &Track,
    top_level_count: u32,
    chunks: &mut Vec<FxStateChunk>,
) {
    for i in 0..top_level_count {
        let fx = chain.fx_by_index_untracked(i);
        if is_container_fx(&fx) {
            let child_count = read_config_u32(&fx, "container_count");
            for c in 0..child_count {
                if let Some(child_raw) = container_child_fx_id(&fx, c) {
                    let child_fx = chain.fx_by_index_untracked(child_raw);
                    if is_container_fx(&child_fx) {
                        debug!("  skipping sub-container at child {} of container {}", c, i);
                    } else {
                        capture_single_fx(chain, track, &child_fx, child_raw, chunks);
                    }
                }
            }
        } else {
            capture_single_fx(chain, track, &fx, i, chunks);
        }
    }
}

fn capture_single_fx(
    _chain: &FxChain,
    track: &Track,
    fx: &reaper_high::Fx,
    raw_index: u32,
    chunks: &mut Vec<FxStateChunk>,
) {
    let guid = fx
        .get_or_query_guid()
        .map(|g| g.to_string_without_braces())
        .unwrap_or_default();
    let plugin_name = safe_fx_call("fx.name", None, || fx.name().to_str().to_string())
        .unwrap_or_else(|| "(unknown)".to_string());

    let chunk_str = match fx.tag_chunk() {
        Ok(tag) => Some(tag.content().to_string()),
        Err(_) => {
            debug!(
                "  FX[{}] tag_chunk failed, trying track chunk fallback",
                raw_index
            );
            get_fx_block_via_track_chunk(track, raw_index)
        }
    };

    match chunk_str {
        Some(s) => {
            debug!(
                "  FX[{}] '{}' (GUID {}) — chunk captured ({} bytes)",
                raw_index,
                plugin_name,
                guid,
                s.len()
            );
            chunks.push(FxStateChunk {
                fx_guid: guid,
                fx_index: raw_index,
                plugin_name,
                encoded_chunk: s,
            });
        }
        None => {
            warn!(
                "  FX[{}] '{}' (GUID {}) — chunk capture FAILED (both methods)",
                raw_index, plugin_name, guid
            );
        }
    }
}

/// Read an FX block out of the track chunk by index. Used as the CLAP
/// fallback when `Fx::tag_chunk()` fails.
pub fn get_fx_block_via_track_chunk(track: &Track, fx_index: u32) -> Option<String> {
    let chunk = track
        .chunk(MAX_TRACK_CHUNK_SIZE, ChunkCacheHint::NormalMode)
        .ok()?;
    let chunk_str = chunk.to_string();
    let fxchain_text = dawfile_reaper::chunk_ops::extract_fxchain_block(&chunk_str)?;
    let chain = dawfile_reaper::FxChain::parse(fxchain_text).ok()?;
    let node = chain.nodes.get(fx_index as usize)?;
    let raw = match node {
        dawfile_reaper::types::FxChainNode::Plugin(plugin) => &plugin.raw_block,
        dawfile_reaper::types::FxChainNode::Container(container) => &container.raw_block,
    };
    if raw.is_empty() {
        None
    } else {
        Some(raw.clone())
    }
}

/// Replace the raw RPP block text for a specific FX by chain index by
/// rewriting the entire track chunk. CLAP fallback for
/// `Fx::set_tag_chunk()`.
pub fn set_fx_block_via_track_chunk(
    track: &Track,
    fx_index: u32,
    new_block: &str,
) -> Result<(), String> {
    let chunk = track
        .chunk(MAX_TRACK_CHUNK_SIZE, ChunkCacheHint::NormalMode)
        .map_err(|e| format!("get chunk: {e}"))?;
    let chunk_str = chunk.to_string();
    let fxchain_text =
        dawfile_reaper::chunk_ops::extract_fxchain_block(&chunk_str).ok_or("no FXCHAIN block")?;
    let chain =
        dawfile_reaper::FxChain::parse(fxchain_text).map_err(|e| format!("parse fxchain: {e}"))?;
    let node = chain
        .nodes
        .get(fx_index as usize)
        .ok_or_else(|| format!("FX index {} out of range", fx_index))?;
    let old_block = match node {
        dawfile_reaper::types::FxChainNode::Plugin(plugin) => &plugin.raw_block,
        dawfile_reaper::types::FxChainNode::Container(container) => &container.raw_block,
    };
    if old_block.is_empty() {
        return Err("FX raw_block is empty".to_string());
    }
    let new_chunk_str = chunk_str.replace(old_block, new_block);
    let new_chunk = reaper_high::Chunk::new(new_chunk_str);
    track
        .set_chunk(new_chunk)
        .map_err(|e| format!("set chunk: {e}"))
}
