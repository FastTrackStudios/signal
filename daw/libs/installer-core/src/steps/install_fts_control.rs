//! Copy FTS-Control.app into the install root.

use std::path::Path;

use eyre::Context;
use tracing::info;

use crate::progress::{EventSender, InstallEvent, InstallStep};

/// Copy FTS-Control.app from the DMG volume to the install root.
pub async fn install_fts_control(
    source: &Path,
    install_root: &Path,
    tx: &EventSender,
) -> eyre::Result<()> {
    let dest = install_root.join("FTS-Control.app");

    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::InstallFtsControl,
            fraction: 0.3,
            message: "Copying FTS Control...".into(),
        })
        .await;

    // Remove existing
    if dest.exists() {
        tokio::fs::remove_dir_all(&dest)
            .await
            .wrap_err("Failed to remove existing FTS-Control.app")?;
    }

    // cp -R for .app bundles (preserves symlinks, permissions, etc.)
    let status = tokio::process::Command::new("cp")
        .args(["-R"])
        .arg(source)
        .arg(&dest)
        .status()
        .await
        .wrap_err("Failed to copy FTS-Control.app")?;

    if !status.success() {
        eyre::bail!("cp -R failed for FTS-Control.app");
    }

    info!("Installed FTS-Control.app to {}", dest.display());
    Ok(())
}
