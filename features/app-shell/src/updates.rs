//! The update check — check / download / apply. The check is a stub for
//! now: the shape is fixed so the Settings UI wires against it, and the
//! network and install halves are deliberately unimplemented.
//!
//! When they land, `launcher`'s installer core already carries the useful
//! primitives — retry policy, step/percent progress, download and extract —
//! and the downloader should reuse those. The FTS plugin suite ships as its
//! own release asset (`fts-plugins-v*-<platform>.tar.gz`); the same flow
//! installs it into ~/.clap and ~/.vst3.

/// Release feed: the studio repo's latest GitHub release.
pub const FEED_URL: &str = "https://api.github.com/repos/FastTrackStudios/studio/releases/latest";

/// A downloadable release, as reported by the feed.
#[derive(Clone, Debug)]
pub struct UpdateInfo {
    pub version: String,
    /// Asset download URL for this platform.
    pub asset_url: String,
    pub notes: String,
}

/// Outcome of a check.
#[derive(Clone, Debug)]
pub enum UpdateStatus {
    UpToDate,
    Available(UpdateInfo),
    Failed(String),
}

/// The update pipeline; the trait keeps the UI testable.
pub trait Updater {
    /// Query [`FEED_URL`] and compare against the running version.
    fn check_for_updates(&self) -> UpdateStatus;
    /// Download the release asset to a staging path.
    ///
    /// # Errors
    ///
    /// Not implemented yet.
    fn download(&self, info: &UpdateInfo) -> Result<std::path::PathBuf, String>;
    /// Swap the staged binaries in and schedule a relaunch.
    ///
    /// # Errors
    ///
    /// Not implemented yet.
    fn apply(&self, staged: &std::path::Path) -> Result<(), String>;
}

/// GitHub-releases updater for the app built at `version` (the app's
/// `env!("CARGO_PKG_VERSION")`). STUB: the check does not touch the
/// network yet — it reports the app as current, so the Settings UI has
/// something honest to render.
pub struct GithubUpdater {
    pub version: &'static str,
}

impl GithubUpdater {
    #[must_use]
    pub const fn new(version: &'static str) -> Self {
        Self { version }
    }
}

impl Updater for GithubUpdater {
    fn check_for_updates(&self) -> UpdateStatus {
        tracing::info!("update check (stub): v{} against {FEED_URL}", self.version);
        UpdateStatus::UpToDate
    }

    fn download(&self, _info: &UpdateInfo) -> Result<std::path::PathBuf, String> {
        Err("update download not implemented yet".into())
    }

    fn apply(&self, _staged: &std::path::Path) -> Result<(), String> {
        Err("update apply not implemented yet".into())
    }
}
