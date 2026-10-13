//! Write a minimal portable reaper.ini.

use std::path::Path;

use eyre::Context;
use tracing::info;

use crate::progress::{EventSender, InstallEvent, InstallStep};

/// Write `reaper.ini` to make the REAPER instance portable.
pub async fn write_reaper_ini(reaper_dir: &Path, tx: &EventSender) -> eyre::Result<()> {
    let ini_path = reaper_dir.join("reaper.ini");

    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::WriteReaperIni,
            fraction: 0.5,
            message: "Writing reaper.ini...".into(),
        })
        .await;

    // Minimal ini that makes REAPER treat this directory as its resource root.
    let content = "[REAPER]\n\
                   lastproject=\n";

    tokio::fs::create_dir_all(reaper_dir).await?;
    tokio::fs::write(&ini_path, content)
        .await
        .wrap_err_with(|| format!("Failed to write {}", ini_path.display()))?;

    info!("Wrote portable reaper.ini at {}", ini_path.display());
    Ok(())
}
