//! Installation plan — validated configuration for the install process.

use std::path::PathBuf;
use std::time::Duration;

use tracing::{info, warn};

/// Fallback REAPER version if we can't fetch the latest.
pub const FALLBACK_REAPER_VERSION: &str = "7.30";

/// Default library download URL — points to the latest GitHub Release asset.
/// Update this when cutting a new library release.
pub const DEFAULT_LIBRARY_URL: &str =
    "https://github.com/FastTrackStudios/fts-library/releases/latest/download/fts-library.tar.gz";

#[derive(Clone, PartialEq)]
pub struct InstallPlan {
    /// Root installation directory (e.g. `~/Music/FastTrackStudio/`).
    pub install_root: PathBuf,
    /// Path to `FTS-Control.app` (found alongside the installer on the DMG).
    pub fts_control_source: Option<PathBuf>,
    /// URL to download the library archive (presets, FXChains, etc.).
    pub library_url: String,
    /// REAPER version to download. Resolved dynamically at install time.
    pub reaper_version: String,
}

impl InstallPlan {
    /// Create a default plan for this machine.
    pub fn default_for_machine() -> Self {
        Self {
            install_root: default_install_root(),
            fts_control_source: find_fts_control_app(),
            library_url: DEFAULT_LIBRARY_URL.to_string(),
            reaper_version: FALLBACK_REAPER_VERSION.to_string(),
        }
    }

    /// Create a plan with a custom install directory.
    pub fn with_install_dir(dir: PathBuf) -> Self {
        Self {
            install_root: dir,
            fts_control_source: find_fts_control_app(),
            library_url: DEFAULT_LIBRARY_URL.to_string(),
            reaper_version: FALLBACK_REAPER_VERSION.to_string(),
        }
    }

    /// Validate the plan before starting installation.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Check parent directory is writable
        if let Some(parent) = self.install_root.parent()
            && parent.exists()
            && std::fs::metadata(parent)
                .map(|m| m.permissions().readonly())
                .unwrap_or(true)
        {
            errors.push(format!("Directory {} is not writable", parent.display()));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// URL for downloading REAPER for this platform/arch.
    pub fn reaper_download_url(&self) -> String {
        let version_slug = self.reaper_version.replace('.', "");

        if cfg!(target_os = "macos") {
            // Universal build recommended for all modern macOS (10.15+).
            format!("https://www.reaper.fm/files/7.x/reaper{version_slug}_universal.dmg")
        } else {
            let arch = if cfg!(target_arch = "aarch64") {
                "aarch64"
            } else {
                "x86_64"
            };
            format!("https://www.reaper.fm/files/7.x/reaper{version_slug}_linux_{arch}.tar.xz")
        }
    }

    pub fn reaper_dir(&self) -> PathBuf {
        self.install_root.join("Reaper")
    }

    pub fn library_dir(&self) -> PathBuf {
        self.install_root.join("Library")
    }
}

/// Fetch the latest stable REAPER version from cockos.com.
///
/// Returns `None` on any error (timeout, parse failure, network).
/// Uses a short timeout so it doesn't block the installer.
pub async fn fetch_latest_reaper_version() -> Option<String> {
    let client = reqwest::Client::builder()
        .user_agent("FastTrackStudio-Installer/0.1")
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;

    let response = client
        .get("https://www.cockos.com/reaper/latestversion/")
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    let text = response.text().await.ok()?;
    // The endpoint returns the version on the first line, followed by
    // a URL and changelog. We only need the version number.
    let version = text.lines().next()?.trim().to_string();

    // Sanity check: should look like "7.30" or "7.66"
    if version.starts_with(|c: char| c.is_ascii_digit())
        && version.contains('.')
        && version.len() <= 10
    {
        info!("Resolved latest REAPER version: {version}");
        Some(version)
    } else {
        warn!("Unexpected version format from cockos.com: {version:?}");
        None
    }
}

fn default_install_root() -> PathBuf {
    let base = dirs_home()
        .map(|h| h.join("Music/Dev/FastTrackStudio"))
        .unwrap_or_else(|| PathBuf::from("/tmp/FastTrackStudio"));

    // Don't overwrite an existing install — find a free name.
    if !base.exists() {
        return base;
    }
    for i in 2..100 {
        let candidate = base.with_file_name(format!("FastTrackStudio-{i}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    base // fall back if somehow 99 installs exist
}

/// Look for FastTrackStudio.app adjacent to the running installer binary.
fn find_fts_control_app() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // Inside FTS Installer.app/Contents/MacOS/fts-installer
    let app_bundle = exe.parent()?.parent()?.parent()?;
    let parent = app_bundle.parent()?;

    // Try the current name first, then the legacy name.
    for name in ["FastTrackStudio.app", "FTS Control.app"] {
        let sibling = parent.join(name);
        if sibling.exists() {
            return Some(sibling);
        }
    }
    None
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
