//! Copy bundled presets (FXChains, TrackTemplates, Library data) into the install.

use std::path::Path;

use eyre::Context;
use tracing::info;

use crate::progress::{EventSender, InstallEvent, InstallStep};

/// A bundled file to be copied into the installation.
pub struct BundledFile {
    /// Relative path within the install root (e.g. `Reaper/FXChains/FTS-Signal/preset.RfxChain`).
    pub relative_path: String,
    /// File contents.
    pub data: Vec<u8>,
}

/// Copy all bundled preset files into the install root.
pub async fn copy_presets(
    files: &[BundledFile],
    install_root: &Path,
    tx: &EventSender,
) -> eyre::Result<()> {
    let total = files.len();
    for (i, file) in files.iter().enumerate() {
        let dest = install_root.join(&file.relative_path);

        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(&dest, &file.data)
            .await
            .wrap_err_with(|| format!("Failed to write {}", dest.display()))?;

        let fraction = (i + 1) as f32 / total as f32;
        let _ = tx
            .send(InstallEvent::StepProgress {
                step: InstallStep::DownloadLibrary,
                fraction,
                message: format!("{}/{total} files", i + 1),
            })
            .await;
    }

    info!("Copied {} preset files", total);
    Ok(())
}
