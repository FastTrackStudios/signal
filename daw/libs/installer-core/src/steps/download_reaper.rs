//! Download REAPER from reaper.fm with streaming progress and retry.

use std::path::PathBuf;
use std::time::Duration;

use eyre::Context;
use futures::StreamExt;
use tracing::info;

use crate::plan::InstallPlan;
use crate::progress::{EventSender, InstallEvent, InstallStep};
use crate::retry::with_retry;

/// Build a shared reqwest client.
///
/// Uses a browser-like User-Agent because REAPER's CloudFront CDN
/// blocks non-browser requests with 403.
pub(crate) fn http_client() -> eyre::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15")
        .timeout(Duration::from_secs(300))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .wrap_err("Failed to create HTTP client")
}

/// Download REAPER to a temp directory. Returns the path to the downloaded file.
///
/// Skips download if a file of the correct size already exists.
/// Retries up to 3 times on network errors.
pub async fn download_reaper(plan: &InstallPlan, tx: &EventSender) -> eyre::Result<PathBuf> {
    let url = plan.reaper_download_url();
    let download_dir = std::env::temp_dir().join("fts-installer");
    tokio::fs::create_dir_all(&download_dir).await?;

    let filename = url.rsplit('/').next().unwrap_or("reaper.dmg");
    let dest = download_dir.join(filename);

    info!("Downloading REAPER from {url}");

    let client = http_client()?;

    // HEAD request to get size for cache check (best-effort, no retry)
    let expected_size = client
        .head(&url)
        .send()
        .await
        .ok()
        .and_then(|r| r.content_length());

    // Skip if already downloaded and correct size
    if let Some(size) = expected_size
        && dest.exists()
        && let Ok(meta) = tokio::fs::metadata(&dest).await
        && meta.len() == size
    {
        info!("REAPER already downloaded at {}", dest.display());
        let _ = tx
            .send(InstallEvent::StepProgress {
                step: InstallStep::DownloadReaper,
                fraction: 1.0,
                message: "Already downloaded".into(),
            })
            .await;
        return Ok(dest);
    }

    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::DownloadReaper,
            fraction: 0.0,
            message: format!("Downloading {filename}..."),
        })
        .await;

    // Download with retry
    let tx2 = tx.clone();
    let url2 = url.clone();
    let dest2 = dest.clone();
    with_retry("REAPER download", 3, Duration::from_secs(1), || {
        let client = client.clone();
        let url = url2.clone();
        let dest = dest2.clone();
        let tx = tx2.clone();
        async move {
            let response = client
                .get(&url)
                .header("Referer", "https://www.reaper.fm/download.php")
                .send()
                .await
                .wrap_err("Failed to connect to reaper.fm")?;

            if !response.status().is_success() {
                eyre::bail!("Download failed: HTTP {}", response.status());
            }

            let total_size = response.content_length().unwrap_or(0);

            let mut file = tokio::fs::File::create(&dest)
                .await
                .wrap_err_with(|| format!("Failed to create {}", dest.display()))?;

            let mut stream = response.bytes_stream();
            let mut downloaded: u64 = 0;

            use tokio::io::AsyncWriteExt;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.wrap_err("Download interrupted")?;
                file.write_all(&chunk).await?;
                downloaded += chunk.len() as u64;

                if total_size > 0 {
                    let fraction = downloaded as f32 / total_size as f32;
                    let mb = downloaded as f32 / 1_048_576.0;
                    let total_mb = total_size as f32 / 1_048_576.0;
                    let _ = tx
                        .send(InstallEvent::StepProgress {
                            step: InstallStep::DownloadReaper,
                            fraction,
                            message: format!("{mb:.1} / {total_mb:.1} MB"),
                        })
                        .await;
                }
            }

            file.flush().await?;
            info!("Downloaded {} bytes to {}", downloaded, dest.display());
            Ok(())
        }
    })
    .await?;

    Ok(dest)
}
