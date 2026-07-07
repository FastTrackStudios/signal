//! **Soundsource index** — name → spec-path lookup over the local extraction.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ── Soundsource index ────────────────────────────────────────────────────────

/// Name → spec-path index over the local soundsource extraction. Multisample
/// sources are `<Name>/library.styx` dirs; one-shots are flat `<Name>.styx`.
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

    /// Scan the default extraction root (`FTS_OMNISPHERE_ROOT` override).
    pub fn scan_default() -> Self {
        let root = std::env::var("FTS_OMNISPHERE_ROOT")
            .unwrap_or_else(|_| crate::omni::OMNISPHERE_ROOT.into());
        Self::scan(Path::new(&root))
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
    pub fn find(&self, name: &str) -> Option<&Path> {
        self.by_name.get(&name.to_lowercase()).map(|p| p.as_path())
    }
}
