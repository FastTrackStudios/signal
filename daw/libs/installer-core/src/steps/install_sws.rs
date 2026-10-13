//! Download and install the SWS extension for REAPER.
//!
//! macOS: Downloads a .dmg from sws-extension.org pre-release builds,
//! mounts it, and copies the .dylib to UserPlugins/.
//! Linux: Downloads a .so from the same source.

use std::path::Path;
use std::time::Duration;

use eyre::Context;
use tracing::info;

use crate::progress::{EventSender, InstallEvent, InstallStep};
use crate::retry::with_retry;
use crate::steps::download_reaper::http_client;

/// SWS version + commit to download.
const SWS_VERSION: &str = "2.14.0.7";
const SWS_COMMIT: &str = "9daba634";

/// Download and install SWS extension into the REAPER UserPlugins directory.
pub async fn install_sws(reaper_dir: &Path, tx: &EventSender) -> eyre::Result<()> {
    let user_plugins = reaper_dir.join("UserPlugins");
    tokio::fs::create_dir_all(&user_plugins).await?;

    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    };

    if cfg!(target_os = "macos") {
        install_sws_macos(&user_plugins, arch, tx).await
    } else {
        // Linux SWS is typically installed via the system package manager
        // or ReaPack. Skip for now.
        let _ = tx
            .send(InstallEvent::StepProgress {
                step: InstallStep::InstallSws,
                fraction: 1.0,
                message: "SWS not available for direct install on Linux, skipping".into(),
            })
            .await;
        Ok(())
    }
}

async fn install_sws_macos(user_plugins: &Path, arch: &str, tx: &EventSender) -> eyre::Result<()> {
    let url = format!(
        "https://www.sws-extension.org/download/pre-release/sws-{SWS_VERSION}-Darwin-{arch}-{SWS_COMMIT}.dmg"
    );

    info!("Downloading SWS extension from {url}");
    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::InstallSws,
            fraction: 0.1,
            message: "Downloading SWS extension...".into(),
        })
        .await;

    let client = http_client()?;
    let download_dir = std::env::temp_dir().join("fts-installer");
    tokio::fs::create_dir_all(&download_dir).await?;
    let dmg_path = download_dir.join(format!("sws-{SWS_VERSION}-{arch}.dmg"));

    // Download with retry
    let tx2 = tx.clone();
    let url2 = url.clone();
    let dmg2 = dmg_path.clone();
    with_retry("SWS download", 3, Duration::from_secs(1), || {
        let client = client.clone();
        let url = url2.clone();
        let dmg = dmg2.clone();
        let tx = tx2.clone();
        async move {
            let response = client
                .get(&url)
                .send()
                .await
                .wrap_err("Failed to connect to sws-extension.org")?;

            if !response.status().is_success() {
                eyre::bail!("SWS download failed: HTTP {}", response.status());
            }

            let bytes = response
                .bytes()
                .await
                .wrap_err("SWS download interrupted")?;
            tokio::fs::write(&dmg, &bytes).await?;

            let _ = tx
                .send(InstallEvent::StepProgress {
                    step: InstallStep::InstallSws,
                    fraction: 0.5,
                    message: format!(
                        "Downloaded SWS ({:.1} MB)",
                        bytes.len() as f32 / 1_048_576.0
                    ),
                })
                .await;
            Ok(())
        }
    })
    .await?;

    // Mount DMG and extract .dylib
    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::InstallSws,
            fraction: 0.6,
            message: "Extracting SWS extension...".into(),
        })
        .await;

    let mount_point = tempfile::tempdir().wrap_err("Failed to create temp mount point")?;
    let mount_path = mount_point.path();

    // Pipe "Y" to auto-accept any license in the SWS DMG.
    let mut child = tokio::process::Command::new("hdiutil")
        .args([
            "attach",
            "-nobrowse",
            "-noverify",
            "-noautoopen",
            "-mountpoint",
        ])
        .arg(mount_path)
        .arg(&dmg_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .wrap_err("Failed to run hdiutil for SWS")?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        let _ = stdin.write_all(b"Y\n").await;
        drop(stdin);
    }
    let status = child.wait().await.wrap_err("hdiutil failed for SWS")?;

    if !status.success() {
        eyre::bail!("hdiutil attach failed for SWS DMG");
    }

    // Find and copy .dylib files
    let mut entries = tokio::fs::read_dir(mount_path).await?;
    let mut found = false;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.ends_with(".dylib") {
            let dest = user_plugins.join(&*name_str);
            tokio::fs::copy(entry.path(), &dest).await?;
            info!("Installed SWS: {name_str}");
            found = true;
        }
    }

    // Also check subdirectories (SWS DMG may have nested structure)
    if !found {
        let mut sub_entries = tokio::fs::read_dir(mount_path).await?;
        while let Some(entry) = sub_entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let mut inner = tokio::fs::read_dir(entry.path()).await?;
                while let Some(inner_entry) = inner.next_entry().await? {
                    let name = inner_entry.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.ends_with(".dylib") {
                        let dest = user_plugins.join(&*name_str);
                        tokio::fs::copy(inner_entry.path(), &dest).await?;
                        info!("Installed SWS: {name_str}");
                        found = true;
                    }
                }
            }
        }
    }

    // Detach
    let _ = tokio::process::Command::new("hdiutil")
        .args(["detach", "-quiet"])
        .arg(mount_path)
        .status()
        .await;

    if !found {
        eyre::bail!("No .dylib found in SWS DMG");
    }

    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::InstallSws,
            fraction: 1.0,
            message: "SWS extension installed".into(),
        })
        .await;

    Ok(())
}
