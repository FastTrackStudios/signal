//! Versioning a REAPER configuration.
//!
//! A REAPER resource directory is ~360 MB, and almost none of it is
//! worth keeping: 200 MB is scratch state, 45 MB is REAPER's own bundled
//! data, and `Scripts/` is nearly a thousand files that **ReaPack
//! downloaded**. Versioning those would be committing someone else's
//! artifacts.
//!
//! What actually defines "your REAPER" is about 350 KB:
//!
//! - the ini files that hold keybindings, toolbars, mouse modifiers, FX
//!   tags and folders;
//! - `reapack.ini` + `ReaPack/registry.db` — the *manifest* of installed
//!   packages, from which ReaPack can restore all 994 scripts;
//! - scripts and JSFX you wrote yourself;
//! - a filtered `reaper.ini`.
//!
//! ## Why `reaper.ini` is filtered rather than copied
//!
//! It mixes genuine preferences (MIDI editor behaviour, defaults, theme
//! choice) with things that are true of *one machine on one day*: the
//! audio device, window and dock geometry, recent files, plugin-scan
//! caches. Copying it whole drags one machine's sound card onto another;
//! not versioning it at all means preferences never follow. So keys are
//! filtered by prefix, and the machine-specific ones stay put.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Files and directories that carry the configuration.
///
/// Relative to the REAPER resource directory. Directories are copied
/// recursively, subject to [`EXCLUDED_DIRS`].
pub const TRACKED: &[&str] = &[
    // Input: the three that define how REAPER feels.
    "reaper-kb.ini",    // keybindings + custom actions
    "reaper-menu.ini",  // toolbars and menus
    "reaper-mouse.ini", // mouse modifiers
    // Organisation.
    "reaper-fxtags.ini",
    "reaper-fxfolders.ini",
    "reaper-screensets.ini",
    "reaper-themeconfig.ini",
    // Extensions that keep their own config.
    "S&M.ini",
    "reapack.ini",
    "ReaPack/registry.db", // the package manifest — not the cache
    // Authored content only — see AUTHORED. `Scripts/` and `Effects/`
    // are NOT tracked wholesale.
    "MenuSets",
    "TrackTemplates",
    "ProjectTemplates",
    "Configurations",
];

/// Our own scripts and JSFX, inside directories ReaPack also populates.
///
/// This is an **allowlist, not a blocklist**, and that is the whole
/// point: ReaPack installs into a directory named after each package's
/// author — Cockos, sockmonkey72, STEMwerk, sstillwell, Liteon, and so
/// on — so a blocklist would have to enumerate every author on ReaPack
/// and would silently start vendoring someone else's repository the
/// first time you installed a new package. An allowlist can only ever
/// be too small, which is a mistake you notice.
///
/// A prefix here is tracked recursively. Extend it when you add a
/// directory of your own.
/// Placeholder standing in for the resource directory in versioned
/// paths.
///
/// REAPER writes absolute paths for things like the active theme
/// (`lastthemefn5=/home/cody/fts-dev/ColorThemes/…`). Keeping those
/// verbatim would break on any machine whose resource directory differs
/// — which is every other machine — so they are rewritten to this token
/// on export and back to the real directory on apply.
pub const RESOURCES_TOKEN: &str = "$REAPER_RESOURCES";

pub const AUTHORED: &[&str] = &[
    "Scripts/FTS",
    "Effects/FTS",
    "Scripts/FastTrackStudio",
    "Effects/FastTrackStudio",
];

/// Subdirectories never versioned, matched anywhere in a tracked tree.
pub const EXCLUDED_DIRS: &[&str] = &["__pycache__", ".git", "cache"];

/// File patterns never versioned.
///
/// `Default_*.ReaperTheme*` are REAPER's own stock themes — ~25 MB
/// each, and every install already has them. Any *other* theme in
/// `ColorThemes/` is one you added, so it travels.
pub const EXCLUDED_FILES: &[&str] = &[".bak", ".log", ".dat", ".tmp", "~", ".pyc", ".DS_Store"];

/// Filename prefixes never versioned.
pub const EXCLUDED_PREFIXES: &[&str] = &["Default_"];

/// `reaper.ini` keys that describe a machine rather than a preference.
///
/// Prefix-matched, case-insensitively. Anything starting with one of
/// these is dropped on export.
pub const MACHINE_KEYS: &[&str] = &[
    // Hardware and drivers.
    "audiodev",
    "audio_",
    "midiins",
    "midiouts",
    "alsa",
    "jack",
    "device",
    // Window, dock and screen geometry.
    "wnd_",
    "dock_",
    "docker",
    "leftpanewid",
    "trackview",
    "mixwnd",
    "toolbarwnd",
    // Session history.
    "recent",
    "lastproject",
    "lastcwd",
    "lasttrackfx",
    // Caches and scan results.
    "vstpath",
    "clappath",
    "lv2path",
    "fxcache",
    "reascript_lastdir",
];

/// A file to copy, source → destination-relative path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Entry {
    pub relative: PathBuf,
}

/// Whether a path should be versioned.
pub fn is_tracked(relative: &Path) -> bool {
    let s = relative.to_string_lossy();
    // An exactly-named file always wins over a directory exclusion:
    // `ReaPack/` is excluded wholesale (it is mostly cache), but
    // `ReaPack/registry.db` is the manifest and the whole point.
    if TRACKED.iter().any(|t| relative == Path::new(t)) {
        return true;
    }
    if EXCLUDED_FILES.iter().any(|p| s.ends_with(p)) {
        return false;
    }
    let name = relative
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if EXCLUDED_PREFIXES.iter().any(|p| name.starts_with(p)) {
        return false;
    }
    let excluded_dir = relative
        .components()
        .any(|c| EXCLUDED_DIRS.contains(&c.as_os_str().to_string_lossy().as_ref()));
    if excluded_dir {
        return false;
    }
    if AUTHORED.iter().any(|a| relative.starts_with(Path::new(a))) {
        return true;
    }
    TRACKED.iter().any(|t| {
        let t = Path::new(t);
        relative == t || relative.starts_with(t)
    })
}

/// Every versionable file under a REAPER resource directory.
pub fn collect(resources: &Path) -> Vec<Entry> {
    let mut out = BTreeSet::new();
    for tracked in TRACKED.iter().chain(AUTHORED.iter()) {
        let path = resources.join(tracked);
        if path.is_file() {
            out.insert(Entry {
                relative: PathBuf::from(tracked),
            });
        } else if path.is_dir() {
            walk(resources, &path, &mut out);
        }
    }
    out.into_iter().collect()
}

fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<Entry>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        if !is_tracked(relative) {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, out);
        } else if path.is_file() {
            out.insert(Entry {
                relative: relative.to_path_buf(),
            });
        }
    }
}

/// The theme files a `reaper.ini` refers to, relative to the resource
/// directory.
///
/// Only the theme actually in use travels: `ColorThemes/` also holds
/// REAPER's two stock themes at ~25 MB each, which every install
/// already has.
pub fn referenced_themes(ini: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for line in ini.lines() {
        let t = line.trim_start().to_ascii_lowercase();
        if !t.starts_with("lastthemefn") {
            continue;
        }
        let Some(value) = line.split_once('=').map(|(_, v)| v.trim()) else {
            continue;
        };
        // Either an absolute path under the resource dir, or already
        // tokenised from a previous export.
        let tail = value
            .rsplit_once("ColorThemes/")
            .map(|(_, name)| name)
            .unwrap_or(value);
        if !tail.is_empty() {
            out.push(PathBuf::from("ColorThemes").join(tail));
        }
    }
    out
}

/// Replace an absolute resource-dir prefix with [`RESOURCES_TOKEN`].
pub fn tokenise_paths(contents: &str, resources: &Path) -> String {
    contents.replace(&resources.to_string_lossy().to_string(), RESOURCES_TOKEN)
}

/// Expand [`RESOURCES_TOKEN`] back to a real resource directory.
pub fn expand_paths(contents: &str, resources: &Path) -> String {
    contents.replace(RESOURCES_TOKEN, &resources.to_string_lossy())
}

/// Strip machine-specific keys from a `reaper.ini`.
///
/// Sections are preserved even when they end up empty, so a diff
/// against a later export stays readable.
pub fn filter_reaper_ini(contents: &str) -> String {
    let mut out = String::with_capacity(contents.len());
    for line in contents.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') || trimmed.is_empty() || trimmed.starts_with(';') {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let key = trimmed
            .split('=')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if MACHINE_KEYS.iter().any(|m| key.starts_with(m)) {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Copy the versionable parts of `resources` into `repo`.
///
/// Returns the files written. `reaper.ini` is filtered on the way.
pub fn export(resources: &Path, repo: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    for entry in collect(resources) {
        let src = resources.join(&entry.relative);
        let dst = repo.join(&entry.relative);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&src, &dst)?;
        written.push(entry.relative);
    }

    // reaper.ini is handled separately because it is transformed, not
    // copied — versioning it verbatim would carry one machine's audio
    // device and window layout to every other machine.
    let ini = resources.join("reaper.ini");
    if ini.is_file() {
        let contents = std::fs::read_to_string(&ini)?;
        // Only the theme in use — `ColorThemes/` also holds REAPER's
        // stock themes at ~25 MB each, which every install already has.
        for theme in referenced_themes(&contents) {
            let src = resources.join(&theme);
            if src.is_file() {
                let dst = repo.join(&theme);
                if let Some(parent) = dst.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&src, &dst)?;
                written.push(theme);
            }
        }
        let filtered = tokenise_paths(&filter_reaper_ini(&contents), resources);
        std::fs::write(repo.join("reaper.ini"), filtered)?;
        written.push(PathBuf::from("reaper.ini"));
    }
    Ok(written)
}

/// Copy the versioned themes back.
///
/// `ColorThemes/` is never swept on export — it holds REAPER's stock
/// themes and, if a theme has been unzipped, thousands of images. Only
/// what `reaper.ini` points at is exported, so on the way back the
/// whole (small) directory can simply be copied.
fn apply_themes(repo: &Path, resources: &Path, written: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let src_dir = repo.join("ColorThemes");
    if !src_dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&src_dir)?.flatten() {
        if !entry.path().is_file() {
            continue;
        }
        let rel = PathBuf::from("ColorThemes").join(entry.file_name());
        let dst = resources.join(&rel);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(entry.path(), &dst)?;
        written.push(rel);
    }
    Ok(())
}

/// Copy versioned config from `repo` into a REAPER resource directory.
///
/// `reaper.ini` is **merged**, not replaced: the destination keeps its
/// own machine keys and takes the repo's preferences. Overwriting it
/// would reset the audio device every time the config was applied.
pub fn apply(repo: &Path, resources: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    for entry in collect(repo) {
        let src = repo.join(&entry.relative);
        let dst = resources.join(&entry.relative);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&src, &dst)?;
        written.push(entry.relative);
    }

    apply_themes(repo, resources, &mut written)?;

    let repo_ini = repo.join("reaper.ini");
    if repo_ini.is_file() {
        // Paths come back pointing at *this* machine's resource dir.
        let incoming = expand_paths(&std::fs::read_to_string(&repo_ini)?, resources);
        let target = resources.join("reaper.ini");
        let existing = std::fs::read_to_string(&target).unwrap_or_default();
        std::fs::write(&target, merge_reaper_ini(&existing, &incoming))?;
        written.push(PathBuf::from("reaper.ini"));
    }
    Ok(written)
}

/// Merge preferences into an existing `reaper.ini`, keeping its machine
/// keys.
pub fn merge_reaper_ini(existing: &str, incoming: &str) -> String {
    // Keys the incoming file sets, so the existing value is replaced
    // rather than duplicated.
    let incoming_keys: BTreeSet<String> = incoming
        .lines()
        .filter_map(|l| {
            let t = l.trim_start();
            if t.starts_with('[') || t.is_empty() || t.starts_with(';') {
                return None;
            }
            Some(t.split('=').next()?.trim().to_ascii_lowercase())
        })
        .collect();

    let mut out = String::new();
    for line in existing.lines() {
        let t = line.trim_start();
        if t.starts_with('[') || t.is_empty() || t.starts_with(';') {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let key = t
            .split('=')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        // Machine keys always survive; anything the repo also sets is
        // dropped here and re-added from the repo below.
        if MACHINE_KEYS.iter().any(|m| key.starts_with(m)) || !incoming_keys.contains(&key) {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(incoming);
    out
}

/// Files that differ between a resource dir and the repo.
pub fn diff(resources: &Path, repo: &Path) -> Vec<PathBuf> {
    let mut changed = Vec::new();
    for entry in collect(resources) {
        let a = resources.join(&entry.relative);
        let b = repo.join(&entry.relative);
        let same = match (std::fs::read(&a), std::fs::read(&b)) {
            (Ok(x), Ok(y)) => x == y,
            _ => false,
        };
        if !same {
            changed.push(entry.relative);
        }
    }
    // Filtered comparison, or every export would look like a change.
    let ini = resources.join("reaper.ini");
    if let (Ok(live), Ok(stored)) = (
        std::fs::read_to_string(&ini),
        std::fs::read_to_string(repo.join("reaper.ini")),
    ) {
        if filter_reaper_ini(&live) != stored {
            changed.push(PathBuf::from("reaper.ini"));
        }
    }
    changed
}
