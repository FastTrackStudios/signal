//! Signal domain SHM guest process.
//!
//! Connects to REAPER via daw-bridge SHM and manages signal chain state:
//! FX inference, module/block lifecycle, preset loading, and rig synchronization.
//!
//! Registers signal-domain actions with REAPER and handles their execution
//! locally when triggered. The host (daw-bridge) is domain-agnostic.
//!
//! Placed in `UserPlugins/fts-extensions/` and hot-reloaded by daw-bridge.

mod demo_profile;
mod demo_rig;
mod demo_setlist;
mod place_switch;
mod scene_midi;

use daw::Daw;
use daw_extension_runtime::GuestOptions;
use eyre::Result;
use signal::actions::signal_actions;
use tracing::{debug, error, info};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(run())
}

async fn run() -> Result<()> {
    let pid = std::process::id();
    info!("[signal:{pid}] Signal extension starting");

    let daw = daw_extension_runtime::connect(GuestOptions {
        role: "signal",
        ..Default::default()
    })
    .await?;

    info!("[signal:{pid}] Connected to REAPER via SHM");

    // Signal that we're alive — tests read this to verify the extension connected
    match daw
        .ext_state()
        .set("FTS_SIGNAL_EXT", "status", "ready", false)
        .await
    {
        Ok(_) => info!("[signal:{pid}] Health beacon: status=ready"),
        Err(e) => error!("[signal:{pid}] Failed to set health beacon: {e:#}"),
    }
    match daw
        .ext_state()
        .set("FTS_SIGNAL_EXT", "pid", &pid.to_string(), false)
        .await
    {
        Ok(_) => {}
        Err(e) => error!("[signal:{pid}] Failed to set pid beacon: {e:#}"),
    }

    // Register signal-domain actions with REAPER.
    // Action definitions live in signal-proto — single source of truth.
    info!("[signal:{pid}] Registering signal actions...");
    let registry = daw.action_registry();
    let defs = signal_actions::definitions();
    let total = defs.len();
    info!("[signal:{pid}] {total} action definitions to register");
    let mut registered = 0usize;
    for def in &defs {
        let cmd_name = def.id.to_command_id();
        match registry.register(&cmd_name, &def.display_name()).await {
            Ok(cmd_id) => {
                if cmd_id == 0 {
                    tracing::warn!("[signal:{pid}] Action returned cmd_id=0: {cmd_name}");
                } else {
                    registered += 1;
                }
            }
            Err(e) => {
                error!("[signal:{pid}] Failed to register action {cmd_name}: {e:#}");
            }
        }
    }
    info!("[signal:{pid}] Registered {registered}/{total} signal actions");

    // Subscribe to action trigger events and handle them locally.
    let mut rx = registry.subscribe_actions().await?;
    info!("[signal:{pid}] Subscribed to action events — entering event loop");

    // TODO: Move signal_bridge.rs from reaper-extension into this process:
    //   - Initialize SQLite-backed SignalController
    //   - Wire appliers: ReaperPatchApplier + RigSceneManager
    //   - Expose 11 signal services (Block, Layer, Engine, Rig, Profile,
    //     Song, Setlist, Browser, Resolve, SceneTemplate, Rack)
    // TODO: Move signal_save.rs from reaper-extension into this process:
    //   - Capture track/FX state as Signal presets
    //   - Write .signal.styx sidecar files
    // TODO: Watch for project/track changes and infer signal chains
    // TODO: Sync preset state with signal.db

    // Event loop — handle action triggers from REAPER
    while let Ok(Some(event)) = rx.recv().await {
        match &*event {
            daw::service::ActionEvent::Triggered { command_name } => {
                handle_action(&daw, command_name).await;
            }
        }
    }

    info!("[signal:{pid}] Action event stream ended");
    Ok(())
}

async fn handle_action(daw: &Daw, command_name: &str) {
    info!("[signal] Action triggered: {command_name}");

    // Resolve action ID from the command name format "FTS_SIGNAL_xxx"
    match command_name {
        cmd if cmd.ends_with("DEV_LOAD_DEMO_GUITAR_RIG") => {
            info!("[signal] Loading demo guitar rig...");
            if let Err(e) = demo_rig::load_demo_guitar_rig(daw).await {
                error!("[signal] Failed to load demo guitar rig: {e:#}");
            }
        }
        cmd if cmd.ends_with("DEV_LOAD_DEMO_GUITAR_PROFILE") => {
            info!("[signal] Loading demo guitar profile...");
            if let Err(e) = demo_profile::load_demo_profile(daw).await {
                error!("[signal] Failed to load demo guitar profile: {e:#}");
            }
        }
        cmd if cmd.ends_with("DEV_GENERATE_SCENE_MIDI_ITEMS") => {
            info!("[signal] Generating scene MIDI items...");
            if let Err(e) = scene_midi::generate_scene_midi_items(daw).await {
                error!("[signal] Failed to generate scene MIDI items: {e:#}");
            }
        }
        cmd if cmd.ends_with("DEV_LOAD_DEMO_SETLIST") => {
            info!("[signal] Loading demo setlist...");
            if let Err(e) = demo_setlist::load_demo_setlist(daw).await {
                error!("[signal] Failed to load demo setlist: {e:#}");
            }
        }
        cmd if cmd.ends_with("PLACE_SECTION_SWITCH") => {
            info!("[signal] Placing section switch...");
            if let Err(e) = place_switch::place_section_switch(daw).await {
                error!("[signal] Failed to place section switch: {e:#}");
            }
        }
        cmd if cmd.ends_with("PLACE_SONG_SWITCH") => {
            info!("[signal] Placing song switch...");
            if let Err(e) = place_switch::place_song_switch(daw).await {
                error!("[signal] Failed to place song switch: {e:#}");
            }
        }
        cmd if cmd.ends_with("PLACE_SCENE_SWITCH") => {
            info!("[signal] Placing scene switch...");
            if let Err(e) = place_switch::place_scene_switch(daw).await {
                error!("[signal] Failed to place scene switch: {e:#}");
            }
        }
        _ => {
            // TODO: Dispatch to SignalController (rig/profile/preset operations)
            debug!("[signal] Unhandled action: {command_name}");
        }
    }
}
