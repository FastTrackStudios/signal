//! REAPER track builder for rig structure templates.
//!
//! Converts [`RigTemplate`] and [`RackTemplate`] into live REAPER tracks
//! with proper folder hierarchy, naming prefixes, and send routing.

use daw::{Project, TrackHandle};
use signal_proto::plugin_block::TrackRole;
use signal_proto::rig_template::{EngineTemplate, RackTemplate, RigTemplate};

// ─── Instance types ──────────────────────────────────────────────

/// A materialized rig in REAPER.
pub struct RigInstance {
    pub rig_track: TrackHandle,
    pub engine_instances: Vec<EngineInstance>,
    pub fx_send_tracks: Vec<TrackHandle>,
}

/// A materialized engine in REAPER.
pub struct EngineInstance {
    pub engine_track: TrackHandle,
    pub layer_tracks: Vec<TrackHandle>,
    pub fx_send_tracks: Vec<TrackHandle>,
}

/// A materialized rack in REAPER.
pub struct RackInstance {
    pub rack_track: TrackHandle,
    pub input_tracks: Vec<TrackHandle>,
    pub rig_instances: Vec<RigInstance>,
    pub fx_send_group_tracks: Vec<Vec<TrackHandle>>,
}

// ─── Builder helpers ─────────────────────────────────────────────

/// Small sleep to let REAPER process track changes.
async fn settle() {
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}

/// Build an engine's tracks (engine folder, layers, optional FX sends folder).
///
/// `close_extra` is the number of additional folder levels to close on
/// the last track of this engine (e.g., 1 to also close the parent rig).
async fn build_engine(
    project: &Project,
    engine: &EngineTemplate,
    close_extra: i32,
) -> eyre::Result<EngineInstance> {
    let engine_name = TrackRole::Engine {
        name: engine.name.clone(),
    }
    .display_name();
    let engine_track = project.tracks().add(&engine_name, None).await?;
    engine_track.set_folder_depth(1).await?;

    let mut layer_tracks = Vec::with_capacity(engine.layers.len());
    for layer in &engine.layers {
        let layer_name = TrackRole::Layer {
            name: layer.name.clone(),
        }
        .display_name();
        let track = project.tracks().add(&layer_name, None).await?;
        // Layers are normal children (depth 0) unless this engine has no
        // sends and this is the last layer — then it closes the engine.
        layer_tracks.push(track);
    }

    let mut fx_send_tracks = Vec::new();
    if !engine.fx_sends.is_empty() {
        // FX Sends folder
        let sends_folder_name = format!("[FX Sends: {}]", engine.name);
        let sends_folder = project.tracks().add(&sends_folder_name, None).await?;
        sends_folder.set_folder_depth(1).await?;

        for (i, send) in engine.fx_sends.iter().enumerate() {
            let track = project.tracks().add(&send.name, None).await?;
            if i == engine.fx_sends.len() - 1 {
                // Last send closes: sends folder (-1) + engine folder (-1) + any extra
                track.set_folder_depth(-(2 + close_extra)).await?;
            }
            fx_send_tracks.push(track);
        }
    } else {
        // No sends — the last layer must close the engine folder + any extra
        if let Some(last) = layer_tracks.last() {
            last.set_folder_depth(-(1 + close_extra)).await?;
        }
    }

    settle().await;

    // Create sends: each layer → all engine FX send tracks
    for layer_track in &layer_tracks {
        for send_track in &fx_send_tracks {
            layer_track.sends().add_to(send_track.guid()).await?;
        }
    }

    Ok(EngineInstance {
        engine_track,
        layer_tracks,
        fx_send_tracks,
    })
}

// ─── Public API ──────────────────────────────────────────────────

/// Instantiate a rig template as REAPER tracks.
///
/// Creates the full folder hierarchy with `[R]`/`[E]`/`[L]` prefixed names,
/// FX sends sub-folders, and layer→send routing.
pub async fn instantiate_rig(
    template: &RigTemplate,
    project: &Project,
) -> eyre::Result<RigInstance> {
    let rig_name = TrackRole::Rig {
        name: template.name.clone(),
    }
    .display_name();
    let rig_track = project.tracks().add(&rig_name, None).await?;
    rig_track.set_folder_depth(1).await?;

    let has_rig_sends = !template.fx_sends.is_empty();
    let engine_count = template.engines.len();

    let mut engine_instances = Vec::with_capacity(engine_count);
    for (i, engine) in template.engines.iter().enumerate() {
        let is_last_engine = i == engine_count - 1;
        // If this is the last engine and there are no rig-level sends,
        // the engine's last track must also close the rig folder.
        let close_extra = if is_last_engine && !has_rig_sends { 1 } else { 0 };
        let instance = build_engine(project, engine, close_extra).await?;
        engine_instances.push(instance);
    }

    // Rig-level FX sends
    let mut fx_send_tracks = Vec::new();
    if has_rig_sends {
        let sends_folder_name = format!("[FX Sends: {}]", template.name);
        let sends_folder = project.tracks().add(&sends_folder_name, None).await?;
        sends_folder.set_folder_depth(1).await?;

        for (i, send) in template.fx_sends.iter().enumerate() {
            let track = project.tracks().add(&send.name, None).await?;
            if i == template.fx_sends.len() - 1 {
                // Last rig send closes: sends folder (-1) + rig folder (-1)
                track.set_folder_depth(-2).await?;
            }
            fx_send_tracks.push(track);
        }
    }

    settle().await;

    Ok(RigInstance {
        rig_track,
        engine_instances,
        fx_send_tracks,
    })
}

/// Instantiate a rack template as REAPER tracks.
///
/// Creates the rack folder, input tracks, sub-rig hierarchies,
/// and rack-level FX send groups.
pub async fn instantiate_rack(
    template: &RackTemplate,
    project: &Project,
) -> eyre::Result<RackInstance> {
    // Rack folder
    let rack_track = project.tracks().add(&template.name, None).await?;
    rack_track.set_folder_depth(1).await?;

    // Input tracks (plain children)
    let mut input_tracks = Vec::with_capacity(template.input_tracks.len());
    for name in &template.input_tracks {
        let track = project.tracks().add(name, None).await?;
        input_tracks.push(track);
    }

    // Sub-rigs — each is a nested rig inside the rack folder.
    // We need to handle folder closing carefully: the last element
    // (either last rig or last send group) closes the rack folder.
    let has_send_groups = !template.fx_send_groups.is_empty();
    let rig_count = template.rigs.len();
    let mut rig_instances = Vec::with_capacity(rig_count);

    for (i, rig_template) in template.rigs.iter().enumerate() {
        let is_last_rig = i == rig_count - 1;
        // If this is the last rig and there are no rack-level send groups,
        // the rig must also close the rack folder.
        // But we handle this by adjusting the rig's own close_extra.
        // For now, instantiate each rig as a nested sub-structure.

        let rig_name = TrackRole::Rig {
            name: rig_template.name.clone(),
        }
        .display_name();
        let rig_track = project.tracks().add(&rig_name, None).await?;
        rig_track.set_folder_depth(1).await?;

        let has_rig_sends = !rig_template.fx_sends.is_empty();
        let engine_count = rig_template.engines.len();

        let mut engine_instances = Vec::with_capacity(engine_count);
        for (ei, engine) in rig_template.engines.iter().enumerate() {
            let is_last_engine = ei == engine_count - 1;
            // close_extra for engine: if last engine and no rig sends, close rig folder too
            let mut close_extra = if is_last_engine && !has_rig_sends { 1 } else { 0 };
            // Additionally, if this is also the last rig with no rack send groups
            if is_last_engine && !has_rig_sends && is_last_rig && !has_send_groups {
                close_extra += 1; // also close rack folder
            }
            let instance = build_engine(project, engine, close_extra).await?;
            engine_instances.push(instance);
        }

        // Rig-level FX sends (for vocal rigs, the engine-level sends serve this role)
        let mut rig_fx_send_tracks = Vec::new();
        if has_rig_sends {
            let sends_folder_name = format!("[FX Sends: {}]", rig_template.name);
            let sends_folder = project.tracks().add(&sends_folder_name, None).await?;
            sends_folder.set_folder_depth(1).await?;

            for (si, send) in rig_template.fx_sends.iter().enumerate() {
                let track = project.tracks().add(&send.name, None).await?;
                if si == rig_template.fx_sends.len() - 1 {
                    // Close sends folder + rig folder
                    let mut depth = -2;
                    // If last rig and no rack send groups, also close rack
                    if is_last_rig && !has_send_groups {
                        depth -= 1;
                    }
                    track.set_folder_depth(depth).await?;
                }
                rig_fx_send_tracks.push(track);
            }
        }

        rig_instances.push(RigInstance {
            rig_track,
            engine_instances,
            fx_send_tracks: rig_fx_send_tracks,
        });
    }

    // Rack-level FX send groups
    let group_count = template.fx_send_groups.len();
    let mut fx_send_group_tracks = Vec::with_capacity(group_count);

    for (gi, group) in template.fx_send_groups.iter().enumerate() {
        let is_last_group = gi == group_count - 1;
        let group_folder_name = format!("[{}]", group.name);
        let group_folder = project.tracks().add(&group_folder_name, None).await?;
        group_folder.set_folder_depth(1).await?;

        let mut group_tracks = Vec::with_capacity(group.sends.len());
        for (si, send) in group.sends.iter().enumerate() {
            let track = project.tracks().add(&send.name, None).await?;
            if si == group.sends.len() - 1 {
                // Close group folder
                let mut depth = -1;
                if is_last_group {
                    // Also close the rack-level FX sends folder (if any) and rack folder
                    depth -= 1; // close rack folder
                }
                track.set_folder_depth(depth).await?;
            }
            group_tracks.push(track);
        }
        fx_send_group_tracks.push(group_tracks);
    }

    settle().await;

    Ok(RackInstance {
        rack_track,
        input_tracks,
        rig_instances,
        fx_send_group_tracks,
    })
}
