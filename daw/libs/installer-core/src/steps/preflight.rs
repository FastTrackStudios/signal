//! Pre-flight checks — validate environment before starting the install.

use std::path::Path;

use eyre::Context;
use tracing::info;

use crate::plan::InstallPlan;
use crate::progress::{EventSender, InstallEvent, InstallStep};

/// Minimum free disk space required (500 MB).
const MIN_FREE_SPACE_BYTES: u64 = 500 * 1024 * 1024;

/// Run pre-flight checks: disk space, directory permissions, network.
/// Collects all failures and reports them together.
pub async fn run_preflight(plan: &InstallPlan, tx: &EventSender) -> eyre::Result<()> {
    let mut errors = Vec::new();

    // 1. Check disk space
    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::Preflight,
            fraction: 0.1,
            message: "Checking disk space...".into(),
        })
        .await;

    match check_disk_space(&plan.install_root) {
        Ok(free) => {
            let free_mb = free / (1024 * 1024);
            info!("Available disk space: {free_mb} MB");
        }
        Err(e) => errors.push(format!("Disk space: {e}")),
    }

    // 2. Check directory permissions
    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::Preflight,
            fraction: 0.4,
            message: "Checking directory permissions...".into(),
        })
        .await;

    if let Err(e) = check_dir_writable(&plan.install_root).await {
        errors.push(format!("Permissions: {e}"));
    }

    // 3. Check network connectivity
    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::Preflight,
            fraction: 0.7,
            message: "Checking network connectivity...".into(),
        })
        .await;

    if let Err(e) = check_network().await {
        errors.push(format!("Network: {e}"));
    }

    let _ = tx
        .send(InstallEvent::StepProgress {
            step: InstallStep::Preflight,
            fraction: 1.0,
            message: "Pre-flight checks complete".into(),
        })
        .await;

    if errors.is_empty() {
        Ok(())
    } else {
        eyre::bail!("Pre-flight checks failed:\n  - {}", errors.join("\n  - "))
    }
}

/// Check that the target filesystem has enough free space.
fn check_disk_space(install_root: &Path) -> eyre::Result<u64> {
    // Walk up to find an existing ancestor directory for statvfs.
    let check_path = existing_ancestor(install_root);

    #[cfg(unix)]
    {
        use std::ffi::CString;
        let path_c = CString::new(check_path.to_string_lossy().as_bytes())
            .wrap_err("Invalid path for statvfs")?;
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        let ret = unsafe { libc::statvfs(path_c.as_ptr(), &mut stat) };
        if ret != 0 {
            eyre::bail!("Failed to check disk space for {}", check_path.display());
        }
        let free = stat.f_bavail as u64 * stat.f_frsize as u64;
        if free < MIN_FREE_SPACE_BYTES {
            let free_mb = free / (1024 * 1024);
            let need_mb = MIN_FREE_SPACE_BYTES / (1024 * 1024);
            eyre::bail!("Not enough disk space: {free_mb} MB available, {need_mb} MB required");
        }
        Ok(free)
    }

    #[cfg(not(unix))]
    {
        let _ = check_path;
        Ok(u64::MAX) // Skip check on non-Unix
    }
}

/// Check that we can write to the install directory (or its parent).
async fn check_dir_writable(install_root: &Path) -> eyre::Result<()> {
    // Try to create the directory if it doesn't exist
    tokio::fs::create_dir_all(install_root)
        .await
        .wrap_err_with(|| format!("Cannot create directory {}", install_root.display()))?;

    // Try to write a temp file inside it
    let probe = install_root.join(".fts-install-probe");
    tokio::fs::write(&probe, b"ok")
        .await
        .wrap_err_with(|| format!("Directory {} is not writable", install_root.display()))?;
    let _ = tokio::fs::remove_file(&probe).await;

    Ok(())
}

/// Check network connectivity by hitting reaper.fm and github.com.
async fn check_network() -> eyre::Result<()> {
    let client = reqwest::Client::builder()
        .user_agent("FastTrackStudio-Installer/0.1")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .wrap_err("Failed to create HTTP client")?;

    let reaper_ok = client.head("https://www.reaper.fm/").send().await.is_ok();
    let github_ok = client.head("https://github.com/").send().await.is_ok();

    if !reaper_ok && !github_ok {
        eyre::bail!("Cannot reach reaper.fm or github.com — check your internet connection");
    } else if !reaper_ok {
        eyre::bail!("Cannot reach reaper.fm (github.com is reachable)");
    } else if !github_ok {
        eyre::bail!("Cannot reach github.com (reaper.fm is reachable)");
    }

    info!("Network connectivity OK");
    Ok(())
}

/// Walk up the path to find the first existing ancestor directory.
fn existing_ancestor(path: &Path) -> &Path {
    let mut p = path;
    while !p.exists() {
        match p.parent() {
            Some(parent) => p = parent,
            None => return path,
        }
    }
    p
}
