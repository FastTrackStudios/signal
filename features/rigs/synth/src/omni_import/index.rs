//! **Soundsource index** — name → spec-path lookup over the local extraction.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Root of the built `.signalpack` soundsource library (loops baked in, disk
/// streaming, tags). Mirrors the Keyscape packs root; preferred over the raw
/// extraction. Override with `FTS_OMNISPHERE_PACKS`.
pub(crate) const OMNISPHERE_PACKS_ROOT: &str =
    "/run/media/AudioHaven/Signal/Libraries/Keys/Omnisphere/Packs";

// ── Soundsource index ────────────────────────────────────────────────────────

/// Name → spec-path index over the local soundsource extraction. A built
/// `<Name>.signalpack` (preferred) wins over a multisample `<Name>/library.styx`
/// dir or a flat one-shot `<Name>.styx`.
#[derive(Debug, Default)]
pub struct SoundsourceIndex {
    by_name: HashMap<String, PathBuf>,
}

impl SoundsourceIndex {
    /// Walk `root` (e.g. `…/Omnisphere`) up to a few levels, collecting every
    /// soundsource spec keyed by lower-cased name.
    pub fn scan(root: &Path) -> Self {
        let mut idx = Self::default();
        idx.scan_dir(root, 0);
        idx
    }

    /// Scan the default extraction root (`FTS_OMNISPHERE_ROOT` override), then
    /// overlay the built `.signalpack` library (`FTS_OMNISPHERE_PACKS`) so a
    /// pack always wins over the raw styx for the same name.
    pub fn scan_default() -> Self {
        let root = std::env::var("FTS_OMNISPHERE_ROOT")
            .unwrap_or_else(|_| crate::omni::OMNISPHERE_ROOT.into());
        let mut idx = Self::default();
        idx.scan_dir(Path::new(&root), 0);
        let packs =
            std::env::var("FTS_OMNISPHERE_PACKS").unwrap_or_else(|_| OMNISPHERE_PACKS_ROOT.into());
        idx.scan_dir(Path::new(&packs), 0); // packs overwrite raw entries
        idx
    }

    fn scan_dir(&mut self, dir: &Path, depth: usize) {
        if depth > 4 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // A multisample soundsource dir: <Name>/library.styx.
                let lib = path.join("library.styx");
                if lib.exists() {
                    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                        self.by_name.insert(name.to_lowercase(), lib);
                    }
                } else {
                    self.scan_dir(&path, depth + 1);
                }
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("signalpack"))
            {
                // A built pack (preferred): <Name>.signalpack.
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    self.by_name.insert(stem.to_lowercase(), path.clone());
                }
            } else if path.extension().is_some_and(|e| e == "styx")
                && path.file_name().is_some_and(|f| f != "library.styx")
            {
                // A flat one-shot: <Name>.styx beside its FLAC.
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    self.by_name.insert(stem.to_lowercase(), path.clone());
                }
            }
        }
    }

    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// Look a soundsource up by its patch name (case-insensitive).
    ///
    /// Falls back to a normalized match when the exact name misses, because
    /// patch names and extracted folder names disagree in two systematic ways:
    ///
    /// - Omnisphere marks a multi-dynamic soundsource with a trailing `^`
    ///   (`Choir Men Ohs  ^`, sometimes double-spaced) and a patch selects one
    ///   of its layers with a ` - <dyn>` suffix (`Choir Men Ohs - mf`). Our
    ///   extraction flattens the dynamics into the one folder, so the suffix
    ///   has nothing left to select and the base name is the right target.
    /// - Whitespace runs differ between the two.
    ///
    /// Exact matches always win, so this can only rescue a lookup that would
    /// otherwise have failed outright.
    pub fn find(&self, name: &str) -> Option<&Path> {
        if let Some(p) = self.by_name.get(&name.to_lowercase()) {
            return Some(p.as_path());
        }
        let want = normalize_ss_name(name);
        if want.is_empty() {
            return None;
        }
        self.by_name
            .iter()
            .find(|(k, _)| normalize_ss_name(k) == want)
            .map(|(_, p)| p.as_path())
    }
}

/// Reduce a soundsource name to the part that identifies the *source* rather
/// than which of its dynamic layers a patch wanted.
fn normalize_ss_name(name: &str) -> String {
    let mut s = name.to_lowercase();
    // Drop the multi-dynamic marker.
    s = s.trim_end().trim_end_matches('^').trim_end().to_string();
    // Drop a trailing dynamic selector: " - mf", " - ff", " - p" …
    if let Some((head, tail)) = s.rsplit_once(" - ") {
        const DYNAMICS: [&str; 8] = ["ppp", "pp", "p", "mp", "mf", "f", "ff", "fff"];
        if DYNAMICS.contains(&tail.trim()) {
            s = head.to_string();
        }
    }
    // Collapse whitespace runs — folder names carry stray doubles.
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::normalize_ss_name;

    #[test]
    fn dynamic_suffix_and_multidynamic_marker_normalize_together() {
        // The real mismatch: the gig's Gentle Gothics asks for these names,
        // the extraction wrote those folders.
        assert_eq!(
            normalize_ss_name("Choir Men Ohs - mf"),
            normalize_ss_name("Choir Men Ohs  ^")
        );
        assert_eq!(
            normalize_ss_name("Choir Women Oos - mf"),
            normalize_ss_name("Choir Women Oos  ^")
        );
        // Every dynamic marker, not just mf.
        for dyn_ in ["ppp", "pp", "p", "mp", "mf", "f", "ff", "fff"] {
            assert_eq!(normalize_ss_name(&format!("Pad - {dyn_}")), "pad");
        }
    }

    #[test]
    fn a_hyphen_that_is_not_a_dynamic_is_left_alone() {
        // Real soundsource names contain hyphens; only a trailing dynamic
        // token may be stripped, or distinct sources would collide.
        assert_eq!(
            normalize_ss_name("OB-8 PWM Big Strings"),
            "ob-8 pwm big strings"
        );
        assert_eq!(
            normalize_ss_name("Rhodes - LA Custom"),
            "rhodes - la custom"
        );
        assert_ne!(
            normalize_ss_name("Choir Men Ohs - mf"),
            normalize_ss_name("Choir Men Ahs - mf")
        );
    }
}
