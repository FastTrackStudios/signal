//! Extract REAPER.app from a downloaded DMG.
//!
//! Primary: pure-Rust extraction via the `dpp` crate (DMG → HFS+/APFS → files).
//! Fallback: `hdiutil` on macOS if dpp fails.

use std::path::{Path, PathBuf};

use eyre::Context;
use tracing::info;

use crate::progress::{EventSender, InstallEvent, InstallStep};

/// Extract REAPER.app from a DMG into `reaper_dir`.
pub async fn extract_dmg(dmg_path: &Path, reaper_dir: &Path, tx: &EventSender) -> eyre::Result<()> {
    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::ExtractDmg,
            fraction: 0.1,
            message: "Opening disk image...".into(),
        })
        .await;

    // Use hdiutil on macOS — it preserves symlinks which .app bundles need.
    // (Pure-Rust dpp extraction skips symlinks, breaking the bundle.)
    #[cfg(target_os = "macos")]
    {
        return extract_with_hdiutil(dmg_path, reaper_dir, tx).await;
    }

    // On Linux, use dpp (no .app bundles, symlinks less critical)
    #[cfg(not(target_os = "macos"))]
    {
        extract_with_dpp(dmg_path, reaper_dir, tx).await
    }
}

/// Pure-Rust DMG extraction via dpp.
async fn extract_with_dpp(
    dmg_path: &Path,
    reaper_dir: &Path,
    tx: &EventSender,
) -> eyre::Result<()> {
    let dmg = dmg_path.to_path_buf();
    let dest = reaper_dir.to_path_buf();
    let tx = tx.clone();

    // dpp is sync, run in blocking thread
    tokio::task::spawn_blocking(move || -> eyre::Result<()> {
        let mut pipeline = dpp::DmgPipeline::open(&dmg).wrap_err("Failed to open DMG")?;

        let mut fs = pipeline
            .open_filesystem()
            .wrap_err("Failed to open DMG filesystem")?;

        // Find REAPER.app in the root
        let entries = fs.list_directory("/").wrap_err("Failed to list DMG root")?;

        let reaper_app = entries
            .iter()
            .find(|e| {
                e.kind == dpp::FsEntryKind::Directory
                    && e.name.contains("REAPER")
                    && e.name.ends_with(".app")
            })
            .ok_or_else(|| eyre::eyre!("REAPER.app not found in DMG root"))?;

        let app_name = &reaper_app.name;
        info!("Found {app_name} in DMG, extracting...");

        // Remove existing if present
        let app_dest = dest.join(app_name);
        if app_dest.exists() {
            std::fs::remove_dir_all(&app_dest).wrap_err("Failed to remove existing REAPER.app")?;
        }
        std::fs::create_dir_all(&app_dest)?;

        // Extract REAPER.app contents into the app_dest directory.
        // extract_path strips the base prefix, so we extract into the .app dir.
        let stats = fs
            .extract_path(&format!("/{app_name}"), &app_dest)
            .wrap_err("Failed to extract REAPER.app from DMG")?;

        info!(
            "Extracted {app_name}: {} files, {} dirs, {} bytes",
            stats.files, stats.dirs, stats.bytes
        );

        Ok(())
    })
    .await
    .wrap_err("DMG extraction task panicked")??;

    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::ExtractDmg,
            fraction: 1.0,
            message: "REAPER extracted".into(),
        })
        .await;

    Ok(())
}

/// Fallback: mount DMG with hdiutil, copy REAPER.app, detach.
#[allow(dead_code)] // kept as the hdiutil fallback path
async fn extract_with_hdiutil(
    dmg_path: &Path,
    reaper_dir: &Path,
    tx: &EventSender,
) -> eyre::Result<()> {
    let mount_point = tempfile::tempdir().wrap_err("Failed to create temp mount point")?;
    let mount_path = mount_point.path();

    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::ExtractDmg,
            fraction: 0.3,
            message: "Mounting disk image...".into(),
        })
        .await;

    // Pipe "Y" to stdin to auto-accept REAPER's license agreement in the DMG.
    let mut child = tokio::process::Command::new("hdiutil")
        .args([
            "attach",
            "-nobrowse",
            "-noverify",
            "-noautoopen",
            "-mountpoint",
        ])
        .arg(mount_path)
        .arg(dmg_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .wrap_err("Failed to run hdiutil")?;

    // Write "Y\n" to accept the license, then wait
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        let _ = stdin.write_all(b"Y\n").await;
        drop(stdin);
    }
    let status = child.wait().await.wrap_err("hdiutil failed")?;

    if !status.success() {
        eyre::bail!("hdiutil attach failed with status {status}");
    }

    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::ExtractDmg,
            fraction: 0.5,
            message: "Copying REAPER.app...".into(),
        })
        .await;

    let reaper_app_src = find_reaper_app(mount_path).await?;
    let reaper_app_dst = reaper_dir.join("REAPER.app");

    if reaper_app_dst.exists() {
        tokio::fs::remove_dir_all(&reaper_app_dst)
            .await
            .wrap_err("Failed to remove existing REAPER.app")?;
    }

    tokio::fs::create_dir_all(reaper_dir).await?;

    let cp_status = tokio::process::Command::new("cp")
        .args(["-R"])
        .arg(&reaper_app_src)
        .arg(&reaper_app_dst)
        .status()
        .await
        .wrap_err("Failed to copy REAPER.app")?;

    if !cp_status.success() {
        eyre::bail!("Failed to copy REAPER.app");
    }

    info!("Copied REAPER.app to {}", reaper_app_dst.display());

    let _ = tokio::process::Command::new("hdiutil")
        .args(["detach", "-quiet"])
        .arg(mount_path)
        .status()
        .await;

    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::ExtractDmg,
            fraction: 1.0,
            message: "REAPER extracted".into(),
        })
        .await;

    Ok(())
}

/// Find REAPER.app inside a mounted DMG volume.
#[allow(dead_code)] // used by the hdiutil fallback path
async fn find_reaper_app(mount_path: &Path) -> eyre::Result<PathBuf> {
    let mut entries = tokio::fs::read_dir(mount_path).await?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.ends_with(".app") && name_str.contains("REAPER") {
            return Ok(entry.path());
        }
    }
    eyre::bail!(
        "REAPER.app not found in mounted DMG at {}",
        mount_path.display()
    )
}
