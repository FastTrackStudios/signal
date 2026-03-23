//! Background SHM bridge — connects to daw-bridge and reads track ext_state.
//!
//! Spawns a tokio runtime on a background thread. Periodically reads the
//! `fts_signal/macro_config` ext_state from the plugin's track and updates
//! the UI state (macro names, colors, connection status).
//!
//! Also handles scene switching: when the audio thread receives a MIDI note
//! in the scene-switch range, it sets `requested_scene`. This bridge reads
//! that value and mutes/unmutes sends on the profile input track to switch
//! between scene variations.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use daw::Daw;
use daw_extension_runtime::GuestOptions;
use serde::Deserialize;

use crate::plugin::{ControllerUiState, NUM_MACROS};

/// Parsed macro configuration from track ext_state.
#[derive(Debug, Deserialize)]
struct MacroConfig {
    macros: Vec<MacroEntry>,
}

#[derive(Debug, Deserialize)]
struct MacroEntry {
    label: String,
    #[serde(default)]
    color: String,
    // Allow extra fields (id, value, bindings, children) without failing deserialization
}

/// Spawn a background thread with a tokio runtime that connects to daw-bridge
/// and polls track ext_state for macro configuration.
///
/// The `track_guid` identifies which track this plugin instance lives on,
/// so we can read the correct ext_state. If we're on the rig folder track,
/// we read our own ext_state. If we're on a child track, we walk up to
/// find the parent rig folder's config.
pub fn spawn_bridge(ui_state: Arc<ControllerUiState>) {
    std::thread::Builder::new()
        .name("fts-signal-ctrl-shm".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("[signal-ctrl] Failed to build tokio runtime: {e}");
                    return;
                }
            };
            rt.block_on(bridge_loop(ui_state));
        })
        .expect("failed to spawn SHM bridge thread");
}

/// Main loop: connect to daw-bridge, then poll ext_state for macro config.
async fn bridge_loop(ui_state: Arc<ControllerUiState>) {
    let pid = std::process::id();
    tracing::info!("[signal-ctrl:{pid}] SHM bridge starting");

    // Retry connection with backoff
    let daw = loop {
        match daw_extension_runtime::connect(GuestOptions {
            role: "signal-controller",
            ..Default::default()
        })
        .await
        {
            Ok(daw) => {
                tracing::info!("[signal-ctrl:{pid}] Connected to daw-bridge via SHM");
                ui_state.shm_connected.store(1, Ordering::Relaxed);
                break daw;
            }
            Err(e) => {
                tracing::debug!("[signal-ctrl:{pid}] SHM connect failed (retrying): {e:#}");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    };

    // Poll loop: read ext_state, update UI, and handle scene switching
    let mut last_config = String::new();
    loop {
        if let Err(e) = poll_macro_config(&daw, &ui_state, &mut last_config).await {
            tracing::debug!("[signal-ctrl] poll_macro_config error: {e:#}");
        }

        // Note: Scene switching is handled by the signal-extension process,
        // which monitors transport position and mutes/unmutes sends.
        // The controller plugin still detects MIDI notes (for future use)
        // but the extension is the authoritative scene switcher.

        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Read macro_config from ext_state on all tracks, find the one that has it,
/// and update the UI state if it changed.
async fn poll_macro_config(
    daw: &Daw,
    ui_state: &ControllerUiState,
    last_config: &mut String,
) -> eyre::Result<()> {
    let project = daw.current_project().await?;
    let tracks = project.tracks();

    // Look for any track with fts_signal/macro_config ext_state.
    // In practice, this is the rig folder track or profile folder track.
    // TODO: Once we have track GUID context from the host, read our specific track
    // and walk up to parent. For now, scan all tracks.
    let all_tracks = tracks.all().await?;
    for track_info in &all_tracks {
        let track: daw::TrackHandle = match tracks.by_guid(&track_info.guid).await? {
            Some(t) => t,
            None => continue,
        };

        let config_json: String = match track.get_ext_state("fts_signal", "macro_config").await? {
            Some(json) if !json.is_empty() => json,
            _ => continue,
        };

        // Skip if unchanged
        if config_json == *last_config {
            return Ok(());
        }

        tracing::info!("[signal-ctrl] Found macro_config on track '{}'", track_info.name);
        *last_config = config_json.clone();

        // Parse and apply
        match serde_json::from_str::<MacroConfig>(&config_json) {
            Ok(config) => {
                apply_macro_config(ui_state, &config);
                tracing::info!(
                    "[signal-ctrl] Applied macro config: {} macros",
                    config.macros.len()
                );
            }
            Err(e) => {
                tracing::warn!("[signal-ctrl] Failed to parse macro_config: {e}");
            }
        }

        return Ok(());
    }

    Ok(())
}

/// Apply parsed macro config to the UI state — update display names.
fn apply_macro_config(ui_state: &ControllerUiState, config: &MacroConfig) {
    let macros = ui_state.params.macros();
    for (i, entry) in config.macros.iter().enumerate() {
        if i >= NUM_MACROS {
            break;
        }
        // set_display_name takes &'static str, so we need to leak the string.
        // This is fine — macro configs change rarely and the leaked strings are small.
        let name: &'static str = Box::leak(entry.label.clone().into_boxed_str());
        macros[i].set_display_name(name);

        ui_state.set_macro_label(i, &entry.label);
        if !entry.color.is_empty() {
            ui_state.set_macro_color(i, &entry.color);
        }
    }

    ui_state
        .config_loaded
        .store(true, std::sync::atomic::Ordering::Relaxed);
}
