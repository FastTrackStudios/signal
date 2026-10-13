//! Standalone FTS Routing Project — receives stems from song projects via loopback.
//!
//! The routing project is a singleton REAPER project with a track for each
//! `RoutingChannel`. Each track is configured with:
//! - Stereo input from the corresponding hardware loopback pair
//! - Record-armed (for monitoring)
//! - Parent send disabled (each stem routes independently)
//!
//! The project can be opened/created via the `ensure_routing_project` function,
//! which checks for an existing routing project before creating one.

use daw::service::{ExtState, ProjectContext, Projects, RecordInput, Routing, TrackRef, Tracks};
use session_proto::SessionServiceError;
use session_proto::routing_project::{
    EXT_STATE_KEY_IS_ROUTING, EXT_STATE_SECTION, EXT_STATE_VALUE_TRUE, LoopbackConfig,
    ROUTING_PROJECT_FILENAME, RoutingChannel, RoutingGroup,
};
use tracing::info;

// r[impl routing.project.ensure]
/// Ensure the FTS routing project exists and is open. Returns the project GUID.
///
/// Resolution order:
/// 1. Scan open projects for `ExtState` `FTS/is_routing_project == "1"`
/// 2. Check disk at `<fts_home>/Reaper/FTS-Routing.RPP` and open if found
/// 3. Create from scratch with the canonical track hierarchy
///
/// # Errors
///
/// Returns `SessionServiceError::DawError` if opening an existing routing project fails,
/// or if creating a new routing project fails.
pub fn ensure_routing_project<D>(
    daw: &D,
    config: &LoopbackConfig,
) -> Result<String, SessionServiceError>
where
    D: Projects + ExtState + Tracks + Routing,
{
    // Step 1: Check open projects for existing routing project
    if let Some(guid) = find_open_routing_project(daw) {
        info!("Found open routing project: {guid}");
        return Ok(guid);
    }

    // Step 2: Check disk
    let routing_path = utils::paths::reaper_dir().join(ROUTING_PROJECT_FILENAME);
    if routing_path.exists() {
        info!(
            "Opening routing project from disk: {}",
            routing_path.display()
        );
        let project = daw
            .open(routing_path.to_string_lossy().as_ref())
            .ok_or_else(|| {
                SessionServiceError::DawError("Failed to open routing project".to_string())
            })?;
        return Ok(project.guid);
    }

    // Step 3: Create from scratch
    info!("Creating new routing project");
    create_routing_project(daw, config)
}

/// Scan open projects for one with the routing project `ExtState` marker.
fn find_open_routing_project<D>(daw: &D) -> Option<String>
where
    D: Projects + ExtState,
{
    for project in daw.list() {
        let ctx = ProjectContext::Project(project.guid.clone());
        let value = daw.get_project(ctx, EXT_STATE_SECTION, EXT_STATE_KEY_IS_ROUTING);
        if value.as_deref() == Some(EXT_STATE_VALUE_TRUE) {
            return Some(project.guid);
        }
    }

    None
}

// r[impl routing.project.structure]
/// Create the routing project from scratch with the canonical FTS track hierarchy.
fn create_routing_project<D>(
    daw: &D,
    config: &LoopbackConfig,
) -> Result<String, SessionServiceError>
where
    D: Projects + ExtState + Tracks + Routing,
{
    // Create a new empty project tab
    let project = daw
        .create()
        .ok_or_else(|| SessionServiceError::DawError("Failed to create project".to_string()))?;
    let ctx = ProjectContext::Project(project.guid.clone());

    // Mark as routing project via ExtState
    daw.set_project(
        ctx.clone(),
        EXT_STATE_SECTION,
        EXT_STATE_KEY_IS_ROUTING,
        EXT_STATE_VALUE_TRUE,
    )
    .map_err(|e| SessionServiceError::DawError(e.to_string()))?;

    // Create "Click + Guide" folder with children
    create_folder_with_channels(
        daw,
        &ctx,
        RoutingGroup::ClickGuide.display_name(),
        RoutingChannel::click_guide_channels(),
        config,
    )?;

    // Create "TRACKS" folder with children
    create_folder_with_channels(
        daw,
        &ctx,
        RoutingGroup::Tracks.display_name(),
        RoutingChannel::track_channels(),
        config,
    )?;

    // Save the project (will prompt Save As for first save)
    daw.save(ctx);

    let guid = project.guid;
    info!("Routing project created: {guid}");
    Ok(guid)
}

/// Create a folder track with child routing channel tracks.
fn create_folder_with_channels<D>(
    daw: &D,
    project: &ProjectContext,
    folder_name: &str,
    channels: &[RoutingChannel],
    config: &LoopbackConfig,
) -> Result<(), SessionServiceError>
where
    D: Tracks + Routing,
{
    let map_err = |e: daw::service::DawError| SessionServiceError::DawError(e.to_string());

    // Create folder track
    let folder_guid = daw
        .add(project.clone(), folder_name, None)
        .map_err(map_err)?;
    daw.set_folder_depth(project.clone(), TrackRef::guid(folder_guid), 1)
        .map_err(map_err)?;

    // Create child tracks
    let last_idx = channels.len().saturating_sub(1);
    for (i, channel) in channels.iter().enumerate() {
        let track_guid = daw
            .add(project.clone(), channel.display_name(), None)
            .map_err(map_err)?;
        let track = TrackRef::guid(track_guid);

        // Configure for stereo loopback input
        daw.set_num_channels(project.clone(), track.clone(), 2)
            .map_err(map_err)?;
        daw.set_record_input(
            project.clone(),
            track.clone(),
            RecordInput::Raw(config.recinput_value(*channel)),
        )
        .map_err(map_err)?;
        daw.set_armed(project.clone(), track.clone(), true)
            .map_err(map_err)?;
        daw.set_parent_send_enabled(project.clone(), track.clone(), false)
            .map_err(map_err)?;

        // Close the folder on the last child
        if i == last_idx {
            daw.set_folder_depth(project.clone(), track, -1)
                .map_err(map_err)?;
        }
    }

    Ok(())
}
