//! Signal domain SHM guest process.
//!
//! Connects to REAPER via daw-bridge SHM and manages signal chain state:
//! FX inference, module/block lifecycle, preset loading, and rig synchronization.
//!
//! Placed in `UserPlugins/fts-extensions/` and hot-reloaded by daw-bridge.

use daw_extension_runtime::GuestOptions;
use eyre::Result;
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

    // TODO: Register signal-domain actions
    // TODO: Watch for project/track changes and infer signal chains
    // TODO: Sync preset state with signal.db

    // Keep the process alive — daw-bridge will kill us on reload
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}
