//! Auto-update skeleton — check / download / apply. STUB ONLY for now.
//!
//! The shape is fixed here so the Settings UI and a future background
//! check can wire against it; the network/apply halves are deliberately
//! unimplemented. When they land, `libs/installer-core` already carries
//! the useful primitives: `retry` (transient-failure retry policy),
//! `progress::{EventSender, InstallEvent}` (step/percent progress
//! reporting) and download/extract steps — the downloader should reuse
//! those rather than growing its own.
//!
//! Distribution note: the standalone `apps/installer` will be repointed
//! to download *this* app only (see apps/installer/README.md); once
//! installed, fasttrackstudio keeps itself (and the engine binaries it
//! ships next to) current through this module.

//! Plugin bundle note (issue #31): the FTS plugin suite has its own
//! release asset (`fts-plugins-v*-<platform>.tar.gz`) and a working
//! manifest-driven engine in `fts-installer plugins install|uninstall|
//! list` (apps/installer/src/plugins.rs). When this updater grows its
//! network half, the Settings UI gets a Plugins section driving that
//! same flow: resolve asset by prefix → download+verify → install into
//! ~/.clap + ~/.vst3 → record MANIFEST/VERSION for update checks.

/// Release feed: codeberg releases API for the monorepo.
/// (Placeholder — releases are not published yet.)
pub const FEED_URL: &str = "https://codeberg.org/api/v1/repos/FastTrackStudios/FastTrackStudio/releases?limit=1";

/// The running app's version (workspace version at build time).
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// A downloadable release, as reported by the feed.
#[derive(Clone, Debug)]
#[allow(dead_code)]
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
    #[allow(dead_code)]
    Available(UpdateInfo),
    #[allow(dead_code, reason = "STUB — no code path produces a network failure yet; the network half is deliberately unimplemented")]
    Failed(String),
}

/// The update pipeline. One real implementation will exist
/// ([`CodebergUpdater`]); the trait keeps the UI testable.
#[allow(dead_code, reason = "STUB trait — download/apply are the not-yet-implemented network/install halves described at the top of this file")]
pub trait Updater {
    /// Query [`FEED_URL`] and compare against [`current_version`].
    fn check_for_updates(&self) -> UpdateStatus;
    /// Download the release asset to a staging path.
    fn download(&self, info: &UpdateInfo) -> Result<std::path::PathBuf, String>;
    /// Swap the staged binaries in and schedule a relaunch.
    fn apply(&self, staged: &std::path::Path) -> Result<(), String>;
}

/// Codeberg-releases updater. STUB: `check_for_updates` does not touch
/// the network yet — it reports the app as current so the Settings UI
/// has something honest to render.
pub struct CodebergUpdater;

impl Updater for CodebergUpdater {
    fn check_for_updates(&self) -> UpdateStatus {
        tracing::info!(
            "update check (stub): v{} against {FEED_URL}",
            current_version()
        );
        UpdateStatus::UpToDate
    }

    fn download(&self, _info: &UpdateInfo) -> Result<std::path::PathBuf, String> {
        Err("update download not implemented yet".into())
    }

    fn apply(&self, _staged: &std::path::Path) -> Result<(), String> {
        Err("update apply not implemented yet".into())
    }
}
