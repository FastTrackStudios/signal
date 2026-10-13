//! Set up shell PATH so `fts` CLI is accessible.

use std::path::{Path, PathBuf};

use eyre::Context;
use tracing::info;

use crate::progress::{EventSender, InstallEvent, InstallStep};

const GUARD_START: &str = "# >>> FastTrackStudio FTS CLI >>>";
const GUARD_END: &str = "# <<< FastTrackStudio FTS CLI <<<";

/// Append PATH export to shell rc files (~/.zshrc, ~/.bash_profile).
pub async fn setup_shell(install_root: &Path, tx: &EventSender) -> eyre::Result<()> {
    let bin_dir = install_root.join("bin");
    let export_block = format!(
        "{GUARD_START}\nexport PATH=\"{}:$PATH\"\n{GUARD_END}\n",
        bin_dir.display()
    );

    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let rc_files = [
        PathBuf::from(&home).join(".zshrc"),
        PathBuf::from(&home).join(".bash_profile"),
    ];

    for rc in &rc_files {
        let _ = tx
            .send(InstallEvent::StepProgress {
                step: InstallStep::SetupShell,
                fraction: 0.5,
                message: format!(
                    "Updating {}...",
                    rc.file_name().unwrap_or_default().to_string_lossy()
                ),
            })
            .await;

        if let Err(e) = append_if_absent(rc, &export_block).await {
            // Non-fatal — user might not have this shell
            tracing::warn!("Could not update {}: {e}", rc.display());
        }
    }

    info!("Shell PATH setup complete");
    Ok(())
}

async fn append_if_absent(rc_path: &Path, block: &str) -> eyre::Result<()> {
    let existing = match tokio::fs::read_to_string(rc_path).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).wrap_err_with(|| format!("Failed to read {}", rc_path.display())),
    };

    if existing.contains(GUARD_START) {
        info!("{} already has FTS PATH block, skipping", rc_path.display());
        return Ok(());
    }

    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(rc_path)
        .await?;

    file.write_all(b"\n").await?;
    file.write_all(block.as_bytes()).await?;
    info!("Appended PATH block to {}", rc_path.display());

    Ok(())
}
