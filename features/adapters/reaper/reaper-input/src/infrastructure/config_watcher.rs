//! Hot-reloadable config file watcher.
//!
//! Polls the config file's mtime once per second (called from the 30 Hz timer
//! callback) and re-parses with `facet_styx` when the file changes.

use std::collections::HashMap;
use std::fs::FileType;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use tracing::{info, warn};

use crate::config::InputConfig;

/// Watches a `.styx` config file and returns a new [`InputConfig`] whenever it
/// is modified.
pub struct ConfigWatcher {
    path: PathBuf,
    last_modified: Option<SystemTime>,
    last_checked: Instant,
}

const CHECK_INTERVAL: Duration = Duration::from_secs(1);

/// Default config embedded at compile time so we can seed a missing file.
const DEFAULT_CONFIG: &str = include_str!("../../config/input.styx");

impl ConfigWatcher {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            last_modified: None,
            // Force a check+load on the very first call to check_and_reload.
            last_checked: Instant::now() - CHECK_INTERVAL - Duration::from_secs(1),
        }
    }

    /// Write the bundled default config if the file doesn't exist or can't be parsed.
    ///
    /// An unparseable file (e.g. stale format from an older version) is replaced
    /// with the current default so the plugin always starts with a valid config.
    pub fn ensure_default_config(&self) {
        let needs_write = if self.path.exists() {
            // Check whether the existing file is still parseable.
            let ok = std::fs::read_to_string(&self.path)
                .ok()
                .and_then(|s| facet_styx::from_str::<InputConfig>(&s).ok())
                .is_some();
            if !ok {
                warn!(
                    "Config file {:?} is unparseable — rewriting with default",
                    self.path
                );
            }
            !ok
        } else {
            true
        };

        if !needs_write {
            return;
        }

        if let Some(parent) = self.path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            warn!("Failed to create config dir {:?}: {e}", parent);
            return;
        }
        match std::fs::write(&self.path, DEFAULT_CONFIG) {
            Ok(()) => info!("Wrote default config to {:?}", self.path),
            Err(e) => warn!("Failed to write default config to {:?}: {e}", self.path),
        }
    }

    /// Parse the config file immediately and return the result.
    ///
    /// Does not update the mtime cache — use this for the initial load before
    /// entering the polling loop.
    pub fn load(&mut self) -> Option<InputConfig> {
        let cfg = self.parse_file();
        // Seed the mtime so the first poll doesn't redundantly reload.
        self.last_modified = std::fs::metadata(&self.path)
            .and_then(|m| m.modified())
            .ok();
        self.last_checked = Instant::now();
        cfg
    }

    /// Check whether the file has changed and, if so, reload it.
    ///
    /// Returns `Some(config)` on a successful reload, `None` if nothing changed
    /// or if it is too soon since the last check.
    pub fn check_and_reload(&mut self) -> Option<InputConfig> {
        if self.last_checked.elapsed() < CHECK_INTERVAL {
            return None;
        }
        self.last_checked = Instant::now();

        let modified = std::fs::metadata(&self.path)
            .and_then(|m| m.modified())
            .ok()?;

        if Some(modified) == self.last_modified {
            return None;
        }
        self.last_modified = Some(modified);

        let cfg = self.parse_file();
        if cfg.is_some() {
            info!("Config reloaded from {:?}", self.path);
        }
        cfg
    }

    fn parse_file(&self) -> Option<InputConfig> {
        let contents = match std::fs::read_to_string(&self.path) {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to read config {:?}: {e}", self.path);
                return None;
            }
        };

        match facet_styx::from_str::<InputConfig>(&contents) {
            Ok(cfg) => Some(cfg),
            Err(e) => {
                warn!("Failed to parse config {:?}: {e}", self.path);
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// KeybindWatcher
// ---------------------------------------------------------------------------

/// Watches a keybind config directory tree for changes and triggers reloads.
///
/// Polls all `*.styx` files under the directory (following symlinks) once per
/// second. When any file's mtime changes, signals that `load_from_dir` should
/// be re-run to pick up the new bindings.
pub struct KeybindWatcher {
    dir: PathBuf,
    last_checked: Instant,
    file_mtimes: HashMap<PathBuf, SystemTime>,
}

impl KeybindWatcher {
    pub fn new(dir: PathBuf) -> Self {
        let mut watcher = Self {
            dir,
            last_checked: Instant::now(),
            file_mtimes: HashMap::new(),
        };
        watcher.file_mtimes = watcher.scan_mtimes();
        info!(
            path = %watcher.dir.display(),
            file_count = watcher.file_mtimes.len(),
            "Config watcher seeded"
        );
        watcher
    }

    /// Check if any `.styx` file in the directory tree has changed.
    ///
    /// Returns `true` if a change was detected (caller should reload).
    /// Seeds at construction; if the directory appears later, the first files
    /// discovered trigger one reload.
    pub fn check(&mut self) -> bool {
        if self.last_checked.elapsed() < CHECK_INTERVAL {
            return false;
        }
        self.last_checked = Instant::now();

        let current = self.scan_mtimes();

        // If the watcher was created before the directory existed, seed once
        // when files appear. After normal construction, `new` already seeded.
        if self.file_mtimes.is_empty() {
            self.file_mtimes = current;
            let changed = !self.file_mtimes.is_empty();
            if changed {
                info!(
                    path = %self.dir.display(),
                    file_count = self.file_mtimes.len(),
                    "Config watcher initial files discovered"
                );
            }
            return changed;
        }

        if current == self.file_mtimes {
            return false;
        }
        log_mtime_changes(&self.file_mtimes, &current);
        self.file_mtimes = current;
        true
    }

    /// The directory being watched.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn scan_mtimes(&self) -> HashMap<PathBuf, SystemTime> {
        let mut mtimes = HashMap::new();
        scan_dir_mtimes(&self.dir, &mut mtimes);
        mtimes
    }
}

fn log_mtime_changes(before: &HashMap<PathBuf, SystemTime>, after: &HashMap<PathBuf, SystemTime>) {
    for path in after.keys().filter(|path| !before.contains_key(*path)) {
        info!(path = %path.display(), "Config file added");
    }
    for path in before.keys().filter(|path| !after.contains_key(*path)) {
        info!(path = %path.display(), "Config file removed");
    }
    for (path, mtime) in after {
        if before.get(path).is_some_and(|old| old != mtime) {
            info!(path = %path.display(), "Config file modified");
        }
    }
}

fn scan_dir_mtimes(dir: &Path, mtimes: &mut HashMap<PathBuf, SystemTime>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(link_meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        let file_type = link_meta.file_type();
        record_symlink_mtime(&path, file_type, &link_meta, mtimes);

        // Follow symlinks so symlinked config dirs and files are traversed
        // correctly, while still tracking the link entry above.
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };

        if meta.is_dir() {
            scan_dir_mtimes(&path, mtimes);
        } else if path.extension().and_then(|s| s.to_str()) == Some("styx")
            && let Ok(mtime) = meta.modified()
        {
            mtimes.insert(path, mtime);
        }
    }
}

fn record_symlink_mtime(
    path: &Path,
    file_type: FileType,
    meta: &std::fs::Metadata,
    mtimes: &mut HashMap<PathBuf, SystemTime>,
) {
    if !file_type.is_symlink() {
        return;
    }

    let Ok(mtime) = meta.modified() else {
        return;
    };

    let mut key = path.to_path_buf();
    key.as_mut_os_string().push("::symlink");
    mtimes.insert(key, mtime);
}
