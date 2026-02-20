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

use std::future::Future;
use std::pin::Pin;
use tokio::sync::RwLock;

use daw_control::{Project, TrackHandle};
use daw_proto::TrackRef;
use signal_live::engine::{graph_state_chunks, DawPatchApplier, PatchApplyError};
use signal_proto::resolve::ResolvedGraph;

/// State for a single patch child track.
struct PatchTrackState {
    track: TrackHandle,
    #[allow(dead_code)]
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
}

/// Applies resolved graphs to REAPER using a folder-based multi-track layout.
pub struct ReaperPatchApplier {
    state: RwLock<Option<FolderRigState>>,
}

impl ReaperPatchApplier {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(None),
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

        // Look for existing folder or create it
        let folder_track = match tracks.by_name("Guitar Rig").await {
            Ok(Some(t)) => t,
            _ => {
                let t = tracks
                    .add("Guitar Rig", None)
                    .await
                    .map_err(|e| PatchApplyError::DawError(format!("create folder track: {e}")))?;
                t.set_folder_depth(1)
                    .await
                    .map_err(|e| PatchApplyError::DawError(format!("set folder start: {e}")))?;
                t
            }
        };

        // Look for existing input track or create it
        let input_track = match tracks.by_name("Input: Guitar Rig").await {
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
                    .add("Input: Guitar Rig", Some(folder_info.index + 1))
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

        *self.state.write().await = Some(FolderRigState {
            folder_track,
            input_track,
            current_patch: None,
            tail_patch: None,
            fx_id,
            project,
        });
        Ok(())
    }

    /// Clear the target (disconnects from DAW).
    pub async fn clear_target(&self) {
        *self.state.write().await = None;
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

            // Extract rfxchain data from the resolved graph
            let chunks = graph_state_chunks(graph, &state.fx_id);
            let chunk = chunks.first().ok_or_else(|| {
                PatchApplyError::DawError("no state chunks in resolved graph".into())
            })?;

            let rfxchain_text = String::from_utf8(chunk.chunk_data.clone())
                .map_err(|e| PatchApplyError::DawError(format!("rfxchain not UTF-8: {e}")))?
                .replace('\r', "");

            let patch_label = patch_name.unwrap_or("Patch");

            // --- 1. Clean up old tail track (from two switches ago) ---
            if let Some(tail) = state.tail_patch.take() {
                // Mute all sends TO this track from input before removing
                let _ = state
                    .project
                    .tracks()
                    .remove(TrackRef::Guid(tail.track.guid().to_string()))
                    .await;
            }

            // --- 2. Demote current patch → tail (mute its send) ---
            if let Some(current) = state.current_patch.take() {
                // Find and mute the send from input → this track
                let mut muted = false;
                if let Ok(sends) = state.input_track.sends().all().await {
                    for (i, send) in sends.iter().enumerate() {
                        if send.dest_track_guid.as_deref() == Some(current.track.guid()) {
                            if let Ok(Some(handle)) =
                                state.input_track.sends().by_index(i as u32).await
                            {
                                if let Ok(()) = handle.mute().await {
                                    muted = true;
                                }
                            }
                            break;
                        }
                    }
                }

                // If we couldn't mute an existing send, the send might not exist — create it
                if !muted {
                    eprintln!(
                        "[WARN] Failed to mute send to tail track '{}', send may not exist",
                        current.name
                    );
                    // Try to create the send if it's missing
                    if let Err(e) = state.input_track.sends().add_to(current.track.guid()).await {
                        eprintln!("[ERROR] Failed to create missing send to tail: {}", e);
                    } else if let Ok(sends) = state.input_track.sends().all().await {
                        // Mute the newly created send
                        for (i, send) in sends.iter().enumerate() {
                            if send.dest_track_guid.as_deref() == Some(current.track.guid()) {
                                if let Ok(Some(handle)) =
                                    state.input_track.sends().by_index(i as u32).await
                                {
                                    let _ = handle.mute().await;
                                }
                                break;
                            }
                        }
                    }
                }

                state.tail_patch = Some(current);
            }

            // --- 3. Create new child track ---
            // Insert after the last existing child. REAPER automatically manages folder depth.
            let last_child = if let Some(ref tail) = state.tail_patch {
                &tail.track
            } else {
                &state.input_track
            };

            let last_child_info = last_child
                .info()
                .await
                .map_err(|e| PatchApplyError::DawError(format!("last child info: {e}")))?;

            // Insert after the last existing child
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

            // --- 5. Disable parent send on new child ---
            let _ = new_track.set_parent_send(false).await;

            // --- 6. Create send from input → new child ---
            let send_result = state.input_track.sends().add_to(new_track.guid()).await;

            if let Err(e) = send_result {
                eprintln!(
                    "[ERROR] Failed to create send from input to '{}': {}",
                    patch_label, e
                );
                return Err(PatchApplyError::DawError(format!("create send: {e}")));
            }

            // Verify the send was created
            if let Ok(sends) = state.input_track.sends().all().await {
                let found = sends
                    .iter()
                    .any(|s| s.dest_track_guid.as_deref() == Some(new_track.guid()));
                if !found {
                    eprintln!(
                        "[ERROR] Send to '{}' not found after creation!",
                        patch_label
                    );
                } else {
                    eprintln!("[INFO] Send to '{}' created successfully", patch_label);
                }
            }

            // --- 7. Rename folder track to current patch ---
            let _ = state.folder_track.rename(patch_label).await;

            // --- 8. Update state ---
            state.current_patch = Some(PatchTrackState {
                track: new_track,
                name: patch_label.to_string(),
            });

            Ok(true)
        })
    }
}
