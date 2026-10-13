//! Download and install the ReaPack extension for REAPER.
//!
//! Downloads the pre-built .dylib (macOS) or .so (Linux) from GitHub releases
//! and copies it to UserPlugins/.

use std::path::Path;
use std::time::Duration;

use eyre::Context;
use tracing::info;

use crate::progress::{EventSender, InstallEvent, InstallStep};
use crate::retry::with_retry;
use crate::steps::download_reaper::http_client;

/// ReaPack version to download.
const REAPACK_VERSION: &str = "1.2.6";

/// Download and install ReaPack into the REAPER UserPlugins directory.
pub async fn install_reapack(reaper_dir: &Path, tx: &EventSender) -> eyre::Result<()> {
    let user_plugins = reaper_dir.join("UserPlugins");
    tokio::fs::create_dir_all(&user_plugins).await?;

    let (arch, ext) = if cfg!(target_os = "macos") {
        let arch = if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            "x86_64"
        };
        (arch, "dylib")
    } else {
        let arch = if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else {
            "x86_64"
        };
        (arch, "so")
    };

    let filename = format!("reaper_reapack-{arch}.{ext}");
    let url = format!(
        "https://github.com/cfillion/reapack/releases/download/v{REAPACK_VERSION}/{filename}"
    );

    info!("Downloading ReaPack from {url}");
    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::InstallReaPack,
            fraction: 0.1,
            message: "Downloading ReaPack...".into(),
        })
        .await;

    let client = http_client()?;
    let dest = user_plugins.join(&filename);

    // Download with retry
    let tx2 = tx.clone();
    let url2 = url.clone();
    let dest2 = dest.clone();
    with_retry("ReaPack download", 3, Duration::from_secs(1), || {
        let client = client.clone();
        let url = url2.clone();
        let dest = dest2.clone();
        let tx = tx2.clone();
        async move {
            let response = client
                .get(&url)
                .send()
                .await
                .wrap_err("Failed to connect to GitHub for ReaPack")?;

            if !response.status().is_success() {
                eyre::bail!("ReaPack download failed: HTTP {}", response.status());
            }

            let bytes = response
                .bytes()
                .await
                .wrap_err("ReaPack download interrupted")?;
            tokio::fs::write(&dest, &bytes).await?;

            let _ = tx
                .send(InstallEvent::StepProgress {
                    step: InstallStep::InstallReaPack,
                    fraction: 0.9,
                    message: format!(
                        "Downloaded ReaPack ({:.1} MB)",
                        bytes.len() as f32 / 1_048_576.0
                    ),
                })
                .await;
            Ok(())
        }
    })
    .await?;

    info!("Installed ReaPack: {filename} → {}", dest.display());

    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::InstallReaPack,
            fraction: 1.0,
            message: "ReaPack installed".into(),
        })
        .await;

    Ok(())
}
