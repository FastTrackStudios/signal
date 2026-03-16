//! Concrete [`DawPatchApplier`] for REAPER via `daw-control`.
//!
//! Uses a folder-based multi-track approach for gapless patch switching:
//!
//! ```text
//! [F] Guitar Rig               ← folder track (audio sums here)
//!     Input: Guitar Rig        ← receives live guitar, sends to patch tracks
//!     Clean                    ← current patch (active send)
//!     Crunch                   ← previous patch (muted send, tail ringing out)
//! ```
//!
//! When switching patches, the old patch's send is muted so its
//! reverb/delay tail rings out naturally, while the new patch plays
//! immediately on a fresh child track.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use moire::sync::{Mutex, RwLock};

use daw::{Project, TrackHandle};
use daw::service::TrackRef;
use signal_live::engine::{graph_state_chunks, DawPatchApplier, DawStateChunk, PatchApplyError};
use signal_proto::plugin_block::FxRole;
use signal_proto::resolve::ResolvedGraph;

/// State for a single patch child track.
struct PatchTrackState {
    track: TrackHandle,
    name: String,
    /// True if this track came from the preload pool (return it, don't delete it).
    from_preload: bool,
}

/// A preloaded patch track: FX chain loaded, send created but muted.
struct PreloadedPatchTrack {
    track: TrackHandle,
    name: String,
}

/// The folder rig structure managed by the applier.
struct FolderRigState {
    /// The "Guitar Rig" folder track.
    folder_track: TrackHandle,
    /// The "Input: Guitar Rig" track that holds sends to patch tracks.
    input_track: TrackHandle,
    /// Currently active patch child track.
    current_patch: Option<PatchTrackState>,
    /// Previous patch (tail ringing out, muted send).
    tail_patch: Option<PatchTrackState>,
    /// FX identifier for graph_state_chunks matching.
    fx_id: String,
    /// Project handle for creating/removing child tracks.
    project: Project,
    /// Preloaded patch tracks keyed by patch name, ready for instant switching.
    preloaded_patches: HashMap<String, PreloadedPatchTrack>,
    /// True once preloading has started. When set, apply_graph will NOT fall
    /// through to the cold path for patches that aren't preloaded yet —
    /// it returns Ok(false) instead, avoiding track creation gaps.
    preloading_active: bool,
    /// GUIDs of tracks with pending delayed mutes (for reverb tail ring-out).
    /// When a track is re-activated before the delay fires, its GUID is removed
    /// from this set, cancelling the mute.
    pending_mutes: Arc<Mutex<HashSet<String>>>,
}

/// Applies resolved graphs to REAPER using a folder-based multi-track layout.
pub struct ReaperPatchApplier {
    state: RwLock<Option<FolderRigState>>,
}

impl ReaperPatchApplier {
    pub fn new() -> Self {
        Self {
            state: RwLock::new("signal.applier.state", None),
        }
    }

    /// Set up the folder rig structure.
    ///
    /// Creates (or finds) a "Guitar Rig" folder track with an "Input: Guitar Rig"
    /// child track. The input track has its parent send disabled so audio flows
    /// only through explicit sends to patch child tracks.
    pub async fn set_target(
        &self,
        project: Project,
        fx_id: impl Into<String>,
    ) -> Result<(), PatchApplyError> {
        let fx_id = fx_id.into();
        let tracks = project.tracks();
        let input_track_name = format!("Input: {fx_id}");

        // Look for existing folder or create it
        let folder_track = match tracks.by_name(&fx_id).await {
            Ok(Some(t)) => t,
            _ => {
                let t = tracks
                    .add(&fx_id, None)
                    .await
                    .map_err(|e| PatchApplyError::DawError(format!("create folder track: {e}")))?;
                t.set_folder_depth(1)
                    .await
                    .map_err(|e| PatchApplyError::DawError(format!("set folder start: {e}")))?;
                t
            }
        };

        // Look for existing input track or create it
        let input_track = match tracks.by_name(&input_track_name).await {
            Ok(Some(t)) => t,
            _ => {
                // Make sure folder track has depth +1 (is a folder)
                folder_track
                    .set_folder_depth(1)
                    .await
                    .map_err(|e| PatchApplyError::DawError(format!("set folder depth: {e}")))?;

                // Insert child after the folder track
                let folder_info = folder_track
                    .info()
                    .await
                    .map_err(|e| PatchApplyError::DawError(format!("folder info: {e}")))?;
                let input = tracks
                    .add(&input_track_name, Some(folder_info.index + 1))
                    .await
                    .map_err(|e| PatchApplyError::DawError(format!("create input track: {e}")))?;

                // REAPER automatically sets the child's depth to close the folder
                // No manual depth management needed!

                // Disable parent send — audio goes through explicit sends, not folder bus
                input.set_parent_send(false).await.map_err(|e| {
                    PatchApplyError::DawError(format!("disable parent send on input: {e}"))
                })?;
                input
            }
        };

        // Recover existing child tracks from a previous session.
        // After a hot-reload the REAPER tracks are still there but our state is empty.
        let folder_guid = folder_track.guid().to_string();
        let input_guid = input_track.guid().to_string();
        let mut recovered_preloaded = HashMap::new();
        let mut recovered_current: Option<PatchTrackState> = None;

        if let Ok(all_tracks) = tracks.all().await {
            // Get all sends from the input track to check mute status
            let input_sends = input_track.sends().all().await.unwrap_or_default();

            for track_info in &all_tracks {
                // Only look at direct children of the folder (skip folder + input)
                if track_info.parent_guid.as_deref() != Some(&folder_guid) {
                    continue;
                }
                if track_info.guid == input_guid {
                    continue;
                }

                // Find the send from input → this track
                let send_muted = input_sends
                    .iter()
                    .find(|s| s.dest_track_guid.as_deref() == Some(&track_info.guid))
                    .map(|s| s.muted)
                    .unwrap_or(true); // No send found = treat as muted/preloaded

                let handle = match tracks.by_guid(&track_info.guid).await {
                    Ok(Some(h)) => h,
                    _ => continue,
                };

                if send_muted {
                    // Muted send = preloaded patch (inactive, ready for fast-switch)
                    eprintln!(
                        "[INFO] Recovered preloaded patch '{}' from existing track",
                        track_info.name
                    );
                    recovered_preloaded.insert(
                        track_info.name.clone(),
                        PreloadedPatchTrack {
                            track: handle,
                            name: track_info.name.clone(),
                        },
                    );
                } else {
                    // Unmuted send = currently active patch
                    eprintln!(
                        "[INFO] Recovered active patch '{}' from existing track",
                        track_info.name
                    );
                    recovered_current = Some(PatchTrackState {
                        track: handle,
                        name: track_info.name.clone(),
                        from_preload: true,
                    });
                }
            }
        }

        let recovered_count =
            recovered_preloaded.len() + if recovered_current.is_some() { 1 } else { 0 };
        if recovered_count > 0 {
            eprintln!("[INFO] Recovered {recovered_count} existing patch track(s) from REAPER");
        }

        *self.state.write().await = Some(FolderRigState {
            folder_track,
            input_track,
            current_patch: recovered_current,
            tail_patch: None,
            fx_id,
            project,
            preloaded_patches: recovered_preloaded,
            preloading_active: false,
            pending_mutes: Arc::new(Mutex::new("signal.applier.pending_mutes", HashSet::new())),
        });
        Ok(())
    }

    /// Clear the target (disconnects from DAW).
    pub async fn clear_target(&self) {
        *self.state.write().await = None;
    }

    /// Configure the input track's audio input channel, record-arm it,
    /// and enable input monitoring.
    ///
    /// `channel_index` is the 0-based mono hardware input index
    /// (matching REAPER's I_RECINPUT encoding for mono inputs).
    pub async fn configure_input(&self, channel_index: u32) -> Result<(), PatchApplyError> {
        let guard = self.state.read().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| PatchApplyError::NoTarget("no folder rig configured".into()))?;

        let input_track = &state.input_track;

        // Set mono hardware input (channel_index maps directly to I_RECINPUT for mono)
        input_track
            .set_record_input(daw::service::RecordInput::Raw(channel_index as i32))
            .await
            .map_err(|e| PatchApplyError::DawError(format!("set record input: {e}")))?;

        // Record-arm the input track
        input_track
            .arm()
            .await
            .map_err(|e| PatchApplyError::DawError(format!("arm input track: {e}")))?;

        // Enable input monitoring so we hear the guitar through the FX chain
        input_track
            .set_input_monitoring(daw::service::InputMonitoringMode::Normal)
            .await
            .map_err(|e| PatchApplyError::DawError(format!("set input monitoring: {e}")))?;

        Ok(())
    }

    /// Preload a set of patches as child tracks with muted sends.
    ///
    /// Each patch gets a child track with its FX chain loaded and a muted send
    /// from the input track. Patches are loaded sequentially to avoid overwhelming
    /// REAPER. Skips patches that are already the current_patch or already preloaded.
    pub async fn preload_patches(
        &self,
        patches: Vec<(String, ResolvedGraph)>,
    ) -> Result<(), PatchApplyError> {
        // Mark preloading as active so apply_graph won't fall through to cold path
        {
            let mut guard = self.state.write().await;
            if let Some(state) = guard.as_mut() {
                state.preloading_active = true;
            }
        }
        for (name, graph) in patches {
            self.preload_single_patch(&name, &graph).await?;
        }
        Ok(())
    }

    /// Preload a single patch as a child track with a muted send.
    async fn preload_single_patch(
        &self,
        name: &str,
        graph: &ResolvedGraph,
    ) -> Result<(), PatchApplyError> {
        // Extract rfxchain data outside the lock
        let fx_id = {
            let guard = self.state.read().await;
            let state = guard
                .as_ref()
                .ok_or_else(|| PatchApplyError::NoTarget("no folder rig configured".into()))?;
            state.fx_id.clone()
        };

        let chunks = graph_state_chunks(graph, &fx_id);
        let chunk = match chunks.first() {
            Some(c) => c,
            None => return Ok(()), // No state chunks — skip silently
        };

        let rfxchain_text = String::from_utf8(chunk.chunk_data.clone())
            .map_err(|e| PatchApplyError::DawError(format!("rfxchain not UTF-8: {e}")))?
            .replace('\r', "");

        // Acquire write lock for track creation
        let mut guard = self.state.write().await;
        let state = guard
            .as_mut()
            .ok_or_else(|| PatchApplyError::NoTarget("no folder rig configured".into()))?;

        // Skip if already preloaded
        if state.preloaded_patches.contains_key(name) {
            return Ok(());
        }
        // NOTE: We intentionally do NOT skip the current patch here.
        // The current patch was loaded via the cold path (from_preload: false),
        // so when the user switches away it becomes tail_patch and is eventually
        // deleted. We need a preloaded copy in the pool so the user can fast-switch
        // back to it later.

        // Find insertion point: after the last child track in the folder
        let last_child = state
            .preloaded_patches
            .values()
            .map(|p| &p.track)
            .chain(state.current_patch.as_ref().map(|p| &p.track))
            .chain(state.tail_patch.as_ref().map(|p| &p.track))
            .last()
            .unwrap_or(&state.input_track);

        let last_info = last_child
            .info()
            .await
            .map_err(|e| PatchApplyError::DawError(format!("last child info: {e}")))?;

        // Create the child track
        let track = state
            .project
            .tracks()
            .add(name, Some(last_info.index + 1))
            .await
            .map_err(|e| PatchApplyError::DawError(format!("create preload track: {e}")))?;

        // Load FX chain
        let track_chunk = track
            .get_chunk()
            .await
            .map_err(|e| PatchApplyError::DawError(format!("get_chunk: {e}")))?;

        let new_chunk = splice_fxchain(&track_chunk, &rfxchain_text).ok_or_else(|| {
            PatchApplyError::DawError("failed to splice rfxchain into preload track".into())
        })?;

        track
            .set_chunk(new_chunk)
            .await
            .map_err(|e| PatchApplyError::DawError(format!("set_chunk: {e}")))?;

        // Rename FX using FxRole::Block convention
        rename_fx_on_track(&track, chunk, name).await;

        // Enable parent send so audio flows to folder bus
        let _ = track.set_parent_send(true).await;

        // Create send from input → preloaded track, then mute it
        state
            .input_track
            .sends()
            .add_to(track.guid())
            .await
            .map_err(|e| PatchApplyError::DawError(format!("create preload send: {e}")))?;

        // Mute the send we just created
        mute_send_to_track(&state.input_track, track.guid()).await;

        // Mute the track itself to save CPU — FX won't process until activated
        let _ = track.mute().await;

        eprintln!("[INFO] Preloaded patch '{name}'");

        state.preloaded_patches.insert(
            name.to_string(),
            PreloadedPatchTrack {
                track,
                name: name.to_string(),
            },
        );

        Ok(())
    }

    /// Returns the names of all currently preloaded patches (for UI status display).
    pub async fn preloaded_patch_names(&self) -> Vec<String> {
        let guard = self.state.read().await;
        match guard.as_ref() {
            Some(state) => {
                let mut names: Vec<String> = state.preloaded_patches.keys().cloned().collect();
                // Also include the current patch if it came from the preload pool
                if let Some(ref current) = state.current_patch {
                    if current.from_preload {
                        names.push(current.name.clone());
                    }
                }
                // Include the tail patch — it has a live track with loaded FX
                // and a muted send, so it's functionally preloaded even if it
                // was originally created via the cold path.
                if let Some(ref tail) = state.tail_patch {
                    names.push(tail.name.clone());
                }
                names
            }
            None => Vec::new(),
        }
    }

    /// Remove and delete all preloaded tracks (e.g., before switching profiles).
    pub async fn clear_preloaded(&self) {
        let mut guard = self.state.write().await;
        if let Some(state) = guard.as_mut() {
            for (_, p) in state.preloaded_patches.drain() {
                let _ = state
                    .project
                    .tracks()
                    .remove(TrackRef::Guid(p.track.guid().to_string()))
                    .await;
            }
        }
    }

    /// Capture the current patch's REAPER FX chain as raw rfxchain bytes.
    /// Returns `None` if no patch is currently active.
    pub async fn capture_current_patch(&self) -> Result<Option<Vec<u8>>, PatchApplyError> {
        let guard = self.state.read().await;
        let state = guard
            .as_ref()
            .ok_or_else(|| PatchApplyError::NoTarget("no rig configured".into()))?;

        let current = match state.current_patch.as_ref() {
            Some(p) => p,
            None => return Ok(None),
        };

        let track_chunk = current
            .track
            .get_chunk()
            .await
            .map_err(|e| PatchApplyError::DawError(format!("get_chunk: {e}")))?;

        let content = extract_fxchain_content(&track_chunk)
            .ok_or_else(|| PatchApplyError::DawError("no FXCHAIN in track chunk".into()))?;

        Ok(Some(content.into_bytes()))
    }
}

/// How long to wait before muting a demoted patch track, allowing
/// reverb/delay tails to ring out naturally.
const TAIL_MUTE_DELAY: std::time::Duration = std::time::Duration::from_secs(7);

/// Schedule a background task that mutes the track after [`TAIL_MUTE_DELAY`].
///
/// The send is already muted (no new audio reaches the track), but the track
/// itself stays unmuted so its FX chain continues processing the tail. After
/// the delay, muting the track saves CPU.
///
/// If the track is re-activated (removed from `pending_mutes`) before the delay
/// fires, the mute is cancelled.
async fn schedule_delayed_track_mute(
    track: &TrackHandle,
    pending_mutes: &Arc<Mutex<HashSet<String>>>,
) {
    let guid = track.guid().to_string();
    let track = track.clone();
    let pending = Arc::clone(pending_mutes);

    // Register this GUID as having a pending mute
    pending.lock().await.insert(guid.clone());

    tokio::spawn(async move {
        tokio::time::sleep(TAIL_MUTE_DELAY).await;
        // Only mute if the GUID is still in the pending set (not re-activated)
        let should_mute = pending.lock().await.remove(&guid);
        if should_mute {
            let _ = track.mute().await;
            eprintln!("[INFO] Delayed mute applied to track '{guid}'");
        } else {
            eprintln!("[INFO] Delayed mute cancelled for track '{guid}' (re-activated)");
        }
    });
}

/// Cancel any pending delayed mute for the given track GUID.
async fn cancel_pending_mute(guid: &str, pending_mutes: &Arc<Mutex<HashSet<String>>>) {
    pending_mutes.lock().await.remove(guid);
}

/// Find the send from `source_track` to `dest_guid` and mute it.
async fn mute_send_to_track(source_track: &TrackHandle, dest_guid: &str) -> bool {
    if let Ok(sends) = source_track.sends().all().await {
        for (i, send) in sends.iter().enumerate() {
            if send.dest_track_guid.as_deref() == Some(dest_guid) {
                if let Ok(Some(handle)) = source_track.sends().by_index(i as u32).await {
                    if handle.mute().await.is_ok() {
                        return true;
                    }
                }
                break;
            }
        }
    }
    false
}

/// Find the send from `source_track` to `dest_guid` and unmute it.
async fn unmute_send_to_track(source_track: &TrackHandle, dest_guid: &str) -> bool {
    if let Ok(sends) = source_track.sends().all().await {
        for (i, send) in sends.iter().enumerate() {
            if send.dest_track_guid.as_deref() == Some(dest_guid) {
                if let Ok(Some(handle)) = source_track.sends().by_index(i as u32).await {
                    if handle.unmute().await.is_ok() {
                        return true;
                    }
                }
                break;
            }
        }
    }
    false
}

/// Rename the first FX on a track using the `FxRole::Block` naming convention.
///
/// Produces names like `"EQ Block: Pro-Q 4 - Flat"` from the block's type,
/// plugin name, and patch name.
async fn rename_fx_on_track(track: &TrackHandle, chunk: &DawStateChunk, patch_name: &str) {
    let fx_name = format!("{} - {}", chunk.plugin_name, patch_name);
    let display_name = FxRole::Block {
        block_type: chunk.block_type,
        name: fx_name,
    }
    .display_name();

    if let Ok(Some(fx)) = track.fx_chain().by_index(0).await {
        if let Err(e) = fx.rename(&display_name).await {
            eprintln!("[WARN] Failed to rename FX to '{display_name}': {e}");
        }
    }
}

/// Replace the `<FXCHAIN ...>` section in a track chunk with new rfxchain content.
///
/// If no FXCHAIN exists, creates one. Returns the modified chunk or None if
/// the track chunk couldn't be parsed.
fn splice_fxchain(track_chunk: &str, rfxchain_content: &str) -> Option<String> {
    // Build the replacement FXCHAIN block
    let new_fxchain = format!(
        "<FXCHAIN\nSHOW 0\nLASTSEL 0\nDOCKED 0\n{}\n>",
        rfxchain_content
    );

    if let Some(fxchain_start) = track_chunk.find("<FXCHAIN") {
        // Find the matching closing `>` for the FXCHAIN block by counting
        // bracket depth (same approach as mpl's ExtractBracketsBlock)
        let after_start = &track_chunk[fxchain_start..];
        let mut depth = 0i32;
        let mut end_offset = None;
        for (i, line) in after_start.split('\n').enumerate() {
            if line.trim_start().starts_with('<') {
                depth += 1;
            }
            if line.trim() == ">" {
                depth -= 1;
                if depth == 0 {
                    // Calculate byte offset of this closing `>` line's end
                    let byte_pos: usize = after_start
                        .split('\n')
                        .take(i + 1)
                        .map(|l| l.len() + 1) // +1 for the \n
                        .sum();
                    end_offset = Some(byte_pos);
                    break;
                }
            }
        }

        let end = end_offset?;
        let fxchain_end = fxchain_start + end;

        // Replace the entire <FXCHAIN...> block
        let mut result = String::with_capacity(track_chunk.len());
        result.push_str(&track_chunk[..fxchain_start]);
        result.push_str(&new_fxchain);
        result.push('\n');
        result.push_str(&track_chunk[fxchain_end..]);
        Some(result)
    } else {
        // No FXCHAIN — insert before the track's closing `>`
        let last_close = track_chunk.rfind("\n>")?;
        let mut result = String::with_capacity(track_chunk.len() + new_fxchain.len());
        result.push_str(&track_chunk[..last_close]);
        result.push('\n');
        result.push_str(&new_fxchain);
        result.push_str(&track_chunk[last_close..]);
        Some(result)
    }
}

/// Extract the inner content of the `<FXCHAIN ...>` block from a track chunk.
///
/// This is the inverse of `splice_fxchain`: it returns the lines between
/// the FXCHAIN header (SHOW/LASTSEL/DOCKED) and the closing `>`.
/// The returned content is exactly what `splice_fxchain` expects as input.
fn extract_fxchain_content(track_chunk: &str) -> Option<String> {
    let fxchain_start = track_chunk.find("<FXCHAIN")?;
    let after_start = &track_chunk[fxchain_start..];

    // Collect lines and find the inner content boundaries using depth counting
    let lines: Vec<&str> = after_start.split('\n').collect();
    let mut depth = 0i32;
    let mut content_start_line = None;
    let mut content_end_line = None;

    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with('<') {
            depth += 1;
        }
        // The first few lines after <FXCHAIN are header lines (SHOW, LASTSEL, DOCKED).
        // Content starts after the last header line before plugin blocks.
        if i == 0 {
            // Skip the <FXCHAIN line itself
        } else if content_start_line.is_none() {
            // Skip SHOW, LASTSEL, DOCKED header lines
            let trimmed = line.trim();
            if trimmed.starts_with("SHOW ")
                || trimmed.starts_with("LASTSEL ")
                || trimmed.starts_with("DOCKED ")
            {
                continue;
            }
            content_start_line = Some(i);
        }
        if line.trim() == ">" {
            depth -= 1;
            if depth == 0 {
                content_end_line = Some(i);
                break;
            }
        }
    }

    let start = content_start_line?;
    let end = content_end_line?;

    if start >= end {
        return Some(String::new());
    }

    Some(lines[start..end].join("\n"))
}

impl DawPatchApplier for ReaperPatchApplier {
    fn apply_graph<'a>(
        &'a self,
        graph: &'a ResolvedGraph,
        patch_name: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Result<bool, PatchApplyError>> + Send + 'a>> {
        Box::pin(async move {
            let mut guard = self.state.write().await;
            let state = guard
                .as_mut()
                .ok_or_else(|| PatchApplyError::NoTarget("no folder rig configured".into()))?;

            let patch_label = patch_name.unwrap_or("Patch");

            // =================================================================
            // FAST PATH: switch to a preloaded track (mute/unmute only, <5ms)
            // =================================================================
            if let Some(preloaded) = state.preloaded_patches.remove(patch_label) {
                // Demote current patch
                if let Some(current) = state.current_patch.take() {
                    mute_send_to_track(&state.input_track, current.track.guid()).await;
                    if current.from_preload {
                        // Return to the preload pool — delay muting track so
                        // reverb/delay tail rings out (~7 seconds)
                        schedule_delayed_track_mute(&current.track, &state.pending_mutes).await;
                        state.preloaded_patches.insert(
                            current.name.clone(),
                            PreloadedPatchTrack {
                                track: current.track,
                                name: current.name,
                            },
                        );
                    } else {
                        // Tail track stays unmuted so reverb/delay tail rings out
                        state.tail_patch = Some(current);
                    }
                }

                // Cancel any pending delayed mute for this track (in case the user
                // switched back to it before the tail mute fired)
                cancel_pending_mute(preloaded.track.guid(), &state.pending_mutes).await;

                // Unmute the preloaded patch's track first (resume FX processing),
                // then unmute the send (route audio)
                let _ = preloaded.track.unmute().await;
                unmute_send_to_track(&state.input_track, preloaded.track.guid()).await;

                // Promote preloaded → current
                state.current_patch = Some(PatchTrackState {
                    track: preloaded.track,
                    name: preloaded.name,
                    from_preload: true,
                });

                eprintln!("[INFO] Fast-switched to preloaded patch '{patch_label}'");
                return Ok(true);
            }

            // =================================================================
            // GUARD: if preloading is active, don't fall through to cold path.
            // The patch is still being loaded in the background — creating a
            // new track would cause an audible gap and duplicate tracks.
            // =================================================================
            if state.preloading_active {
                eprintln!(
                    "[INFO] Patch '{patch_label}' not yet preloaded, skipping (preload in progress)"
                );
                return Ok(false);
            }

            // =================================================================
            // COLD PATH: create a new track and load FX chain (existing behavior)
            // =================================================================

            // Extract rfxchain data from the resolved graph
            let chunks = graph_state_chunks(graph, &state.fx_id);
            let chunk = chunks.first().ok_or_else(|| {
                PatchApplyError::DawError("no state chunks in resolved graph".into())
            })?;

            let rfxchain_text = String::from_utf8(chunk.chunk_data.clone())
                .map_err(|e| PatchApplyError::DawError(format!("rfxchain not UTF-8: {e}")))?
                .replace('\r', "");

            // --- 1. Clean up old tail track (from two switches ago) ---
            if let Some(tail) = state.tail_patch.take() {
                if tail.from_preload {
                    // Return preloaded track to the pool — mute send immediately,
                    // delay muting track so reverb/delay tail rings out
                    mute_send_to_track(&state.input_track, tail.track.guid()).await;
                    schedule_delayed_track_mute(&tail.track, &state.pending_mutes).await;
                    state.preloaded_patches.insert(
                        tail.name.clone(),
                        PreloadedPatchTrack {
                            track: tail.track,
                            name: tail.name,
                        },
                    );
                } else {
                    let _ = state
                        .project
                        .tracks()
                        .remove(TrackRef::Guid(tail.track.guid().to_string()))
                        .await;
                }
            }

            // --- 2. Demote current patch → tail (mute its send) ---
            if let Some(current) = state.current_patch.take() {
                if !mute_send_to_track(&state.input_track, current.track.guid()).await {
                    eprintln!(
                        "[WARN] Failed to mute send to tail track '{}', send may not exist",
                        current.name
                    );
                }
                state.tail_patch = Some(current);
            }

            // --- 3. Create new child track ---
            let last_child = if let Some(ref tail) = state.tail_patch {
                &tail.track
            } else {
                &state.input_track
            };

            let last_child_info = last_child
                .info()
                .await
                .map_err(|e| PatchApplyError::DawError(format!("last child info: {e}")))?;

            let new_track = state
                .project
                .tracks()
                .add(patch_label, Some(last_child_info.index + 1))
                .await
                .map_err(|e| PatchApplyError::DawError(format!("create child track: {e}")))?;

            // --- 4. Load rfxchain into the new track ---
            let track_chunk = new_track
                .get_chunk()
                .await
                .map_err(|e| PatchApplyError::DawError(format!("get_chunk: {e}")))?;

            let new_chunk = splice_fxchain(&track_chunk, &rfxchain_text).ok_or_else(|| {
                PatchApplyError::DawError("failed to splice rfxchain into track chunk".into())
            })?;

            new_track
                .set_chunk(new_chunk)
                .await
                .map_err(|e| PatchApplyError::DawError(format!("set_chunk: {e}")))?;

            // --- 4b. Rename FX using FxRole::Block convention ---
            rename_fx_on_track(&new_track, chunk, patch_label).await;

            // --- 5. Ensure parent send is ON ---
            let _ = new_track.set_parent_send(true).await;

            // --- 6. Create send from input → new child ---
            state
                .input_track
                .sends()
                .add_to(new_track.guid())
                .await
                .map_err(|e| {
                    PatchApplyError::DawError(format!("create send to '{patch_label}': {e}"))
                })?;

            // --- 7. Update state ---
            state.current_patch = Some(PatchTrackState {
                track: new_track,
                name: patch_label.to_string(),
                from_preload: false,
            });

            Ok(true)
        })
    }
}
