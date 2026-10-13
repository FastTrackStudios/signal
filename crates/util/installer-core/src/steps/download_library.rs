//! Download and extract the FTS library archive.
//!
//! The library (presets, FXChains, TrackTemplates, catalog, signal data)
//! is hosted as a `.tar.gz` on a GitHub Release. This step downloads it
//! and extracts it directly into the install root.
//!
//! Archive layout mirrors the install structure:
//! ```text
//! Library/
//!   blocks/
//!   presets/
//!   profiles/
//!   catalog.json
//! Reaper/
//!   FXChains/
//!   TrackTemplates/
//! ```

use std::path::Path;
use std::time::Duration;

use eyre::Context;
use futures::StreamExt;
use tracing::info;

use crate::progress::{EventSender, InstallEvent, InstallStep};
use crate::retry::with_retry;
use crate::steps::download_reaper::http_client;

/// Download the library archive and extract it into `install_root`.
///
/// If `$FTS_LIBRARY_ARCHIVE` is set, uses that local file instead of downloading.
/// This is useful for local development and testing.
pub async fn download_library(
    url: &str,
    install_root: &Path,
    tx: &EventSender,
) -> eyre::Result<()> {
    // Check for local override first
    if let Ok(local_path) = std::env::var("FTS_LIBRARY_ARCHIVE") {
        let local = std::path::PathBuf::from(&local_path);
        if local.exists() {
            info!("Using local library archive: {local_path}");
            let _ = tx
                .send(InstallEvent::StepProgress {
                    step: InstallStep::DownloadLibrary,
                    fraction: 0.5,
                    message: format!("Extracting local archive: {local_path}"),
                })
                .await;
            return extract_archive(&local, install_root, tx).await;
        } else {
            tracing::warn!(
                "FTS_LIBRARY_ARCHIVE set to {local_path} but file not found, falling back to download"
            );
        }
    }

    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::DownloadLibrary,
            fraction: 0.0,
            message: "Downloading library...".into(),
        })
        .await;

    info!("Downloading library from {url}");

    let client = http_client()?;

    // Download to a temp file first
    let tmp_dir = std::env::temp_dir().join("fts-installer");
    tokio::fs::create_dir_all(&tmp_dir).await?;
    let archive_path = tmp_dir.join("fts-library.tar.gz");

    // Download with retry
    let tx2 = tx.clone();
    let url2 = url.to_string();
    let archive_path2 = archive_path.clone();
    with_retry("library download", 3, Duration::from_secs(1), || {
        let client = client.clone();
        let url = url2.clone();
        let archive_path = archive_path2.clone();
        let tx = tx2.clone();
        async move {
            let response = client
                .get(&url)
                .send()
                .await
                .wrap_err("Failed to connect to library download URL")?;

            if !response.status().is_success() {
                eyre::bail!("Library download failed: HTTP {}", response.status());
            }

            let total_size = response.content_length().unwrap_or(0);

            // Skip download if already cached and correct size
            if archive_path.exists()
                && let Ok(meta) = tokio::fs::metadata(&archive_path).await
                && total_size > 0
                && meta.len() == total_size
            {
                info!(
                    "Library archive already cached at {}",
                    archive_path.display()
                );
                let _ = tx
                    .send(InstallEvent::StepProgress {
                        step: InstallStep::DownloadLibrary,
                        fraction: 0.7,
                        message: "Using cached download...".into(),
                    })
                    .await;
                return Ok(());
            }

            let mut file = tokio::fs::File::create(&archive_path)
                .await
                .wrap_err("Failed to create temp archive file")?;

            let mut stream = response.bytes_stream();
            let mut downloaded: u64 = 0;

            use tokio::io::AsyncWriteExt;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.wrap_err("Library download interrupted")?;
                file.write_all(&chunk).await?;
                downloaded += chunk.len() as u64;

                if total_size > 0 {
                    let fraction = (downloaded as f32 / total_size as f32) * 0.7;
                    let mb = downloaded as f32 / 1_048_576.0;
                    let total_mb = total_size as f32 / 1_048_576.0;
                    let _ = tx
                        .send(InstallEvent::StepProgress {
                            step: InstallStep::DownloadLibrary,
                            fraction,
                            message: format!("Downloading... {mb:.1} / {total_mb:.1} MB"),
                        })
                        .await;
                }
            }

            file.flush().await?;
            info!("Downloaded library archive ({downloaded} bytes)");
            Ok(())
        }
    })
    .await?;

    extract_archive(&archive_path, install_root, tx).await
}

/// Extract a `.tar.gz` archive into the install root.
async fn extract_archive(
    archive_path: &Path,
    install_root: &Path,
    tx: &EventSender,
) -> eyre::Result<()> {
    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::DownloadLibrary,
            fraction: 0.75,
            message: "Extracting library...".into(),
        })
        .await;

    tokio::fs::create_dir_all(install_root).await?;

    let status = tokio::process::Command::new("tar")
        .args(["xzf"])
        .arg(archive_path)
        .arg("-C")
        .arg(install_root)
        .status()
        .await
        .wrap_err("Failed to run tar")?;

    if !status.success() {
        eyre::bail!("tar extraction failed with status {status}");
    }

    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::DownloadLibrary,
            fraction: 1.0,
            message: "Library installed".into(),
        })
        .await;

    info!("Extracted library to {}", install_root.display());
    Ok(())
}
