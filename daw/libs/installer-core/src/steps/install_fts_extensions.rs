//! Install FTS extensions into the REAPER UserPlugins directory.
//!
//! Extensions are passed as named byte slices (bundled at build time via
//! include_bytes! or embedded by the Nix build). If no extensions are
//! provided, this step is silently skipped.

use std::path::Path;

use eyre::Context;
use tracing::info;

use crate::progress::{EventSender, InstallEvent, InstallStep};

/// A named extension binary to install.
#[derive(Clone)]
pub struct BundledExtension {
    /// Relative path under Reaper/ (e.g. "UserPlugins/fts-extensions/sync-extension")
    pub rel_path: &'static str,
    /// The binary data.
    pub data: &'static [u8],
}

/// Install all bundled FTS extensions into the REAPER directory.
pub async fn install_fts_extensions(
    extensions: &[BundledExtension],
    reaper_dir: &Path,
    tx: &EventSender,
) -> eyre::Result<()> {
    if extensions.is_empty() {
        let _ = tx
            .send(InstallEvent::StepProgress {
                step: InstallStep::InstallFtsExtensions,
                fraction: 1.0,
                message: "No FTS extensions bundled, skipping".into(),
            })
            .await;
        return Ok(());
    }

    let total = extensions.len();
    for (i, ext) in extensions.iter().enumerate() {
        let dest = reaper_dir.join(ext.rel_path);
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(&dest, ext.data)
            .await
            .wrap_err_with(|| format!("Failed to write {}", ext.rel_path))?;

        // Make executable on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o755);
            std::fs::set_permissions(&dest, perms)?;
        }

        let fraction = (i + 1) as f32 / total as f32;
        let name = ext.rel_path.rsplit('/').next().unwrap_or(ext.rel_path);
        info!("Installed FTS extension: {name}");
        let _ = tx
            .send(InstallEvent::StepProgress {
                step: InstallStep::InstallFtsExtensions,
                fraction,
                message: format!("Installed {name} ({}/{})", i + 1, total),
            })
            .await;
    }

    Ok(())
}
