//! Signal domain SHM guest process.
//!
//! Connects to REAPER via daw-bridge SHM and manages signal chain state:
//! FX inference, module/block lifecycle, preset loading, and rig synchronization.
//!
//! Registers signal-domain actions with REAPER and handles their execution
//! locally when triggered. The host (daw-bridge) is domain-agnostic.
//!
//! Placed in `UserPlugins/fts-extensions/` and hot-reloaded by daw-bridge.

use daw_extension_runtime::GuestOptions;
use eyre::Result;
use signal::actions::signal_actions;
use tracing::info;

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
    daw.ext_state()
        .set("FTS_SIGNAL_EXT", "status", "ready", false)
        .await?;
    daw.ext_state()
        .set("FTS_SIGNAL_EXT", "pid", &pid.to_string(), false)
        .await?;
    info!("[signal:{pid}] Health beacon written");

    // Register signal-domain actions with REAPER.
    // Action definitions live in signal-proto — single source of truth.
    let registry = daw.action_registry();
    for def in signal_actions::definitions() {
        let cmd_name = def.id.to_command_id();
        let cmd_id = registry.register(&cmd_name, &def.description).await?;
        if cmd_id == 0 {
            tracing::warn!("[signal:{pid}] Failed to register action: {cmd_name}");
        } else {
            info!("[signal:{pid}] Registered {cmd_name} (cmd_id={cmd_id})");
        }
    }
    info!("[signal:{pid}] All signal actions registered");

    // Subscribe to action trigger events and handle them locally.
    let mut rx = registry.subscribe_actions().await?;
    info!("[signal:{pid}] Subscribed to action events");

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
                handle_action(command_name);
            }
        }
    }

    info!("[signal:{pid}] Action event stream ended");
    Ok(())
}

fn handle_action(command_name: &str) {
    // TODO: Dispatch to SignalController (rig/profile/preset operations)
    // TODO: Dispatch to signal save (capture FX state → .signal.styx)
    info!("[signal] Action triggered: {command_name}");
}
