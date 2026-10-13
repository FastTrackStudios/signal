//! Copy the FTS REAPER extension into UserPlugins.

use std::path::Path;

use eyre::Context;
use tracing::info;

use crate::progress::{EventSender, InstallEvent, InstallStep};

/// Extension binary bytes, embedded at compile time by the installer app.
///
/// The installer app passes these in — installer-core doesn't embed them itself
/// so it stays testable without a built dylib.
pub async fn copy_extension(
    extension_bytes: &[u8],
    reaper_dir: &Path,
    tx: &EventSender,
) -> eyre::Result<()> {
    let plugins_dir = reaper_dir.join("UserPlugins");
    tokio::fs::create_dir_all(&plugins_dir).await?;

    // REAPER only loads UserPlugins files named `reaper_*`; cargo's `lib`
    // prefix on the built cdylib has to go.
    let ext_name = format!("reaper_fts.{}", std::env::consts::DLL_EXTENSION);

    let dest = plugins_dir.join(&ext_name);

    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::CopyExtension,
            fraction: 0.5,
            message: format!("Writing {ext_name}..."),
        })
        .await;

    tokio::fs::write(&dest, extension_bytes)
        .await
        .wrap_err_with(|| format!("Failed to write {}", dest.display()))?;

    info!("Installed extension to {}", dest.display());
    Ok(())
}
