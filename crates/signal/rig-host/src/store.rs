//! Styx-directory persistence for rig libraries — the read / write /
//! read-or-seed trio each rig's `library.rs` re-implemented, over one
//! directory of plain styx text files (portable, git-trackable, editable).

use std::path::PathBuf;

use facet::Facet;

/// Signal's user config directory: `$XDG_CONFIG_HOME/signal`, falling back to
/// `$HOME/.config/signal`, then `./signal`.
pub fn signal_config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("signal")
}

/// A directory of styx files, one persisted struct per file.
#[derive(Clone, Debug)]
pub struct StyxDir {
    dir: PathBuf,
}

impl StyxDir {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The backing directory.
    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    /// Resolve a dir-relative asset path (`models/…`, `irs/…`) to absolute;
    /// absolute paths and empties pass through.
    pub fn resolve(&self, path: &mut String) {
        if !path.is_empty() && !std::path::Path::new(path.as_str()).is_absolute() {
            *path = self.dir.join(path.as_str()).to_string_lossy().into_owned();
        }
    }

    /// Inverse of [`resolve`](Self::resolve) for saves: paths under the dir
    /// are stored relative, so the on-disk library stays portable — including
    /// when the dir is a symlink into the repo's default-config (live edits
    /// become committable working-tree diffs).
    pub fn relativize(&self, path: &mut String) {
        if let Ok(rel) = std::path::Path::new(path.as_str()).strip_prefix(&self.dir) {
            *path = rel.to_string_lossy().into_owned();
        }
    }

    /// Read + parse `file`. `None` when missing; parse failures are logged
    /// and treated as missing (the caller falls back to defaults rather than
    /// dying mid-set over a hand-edit typo).
    pub fn read<T: for<'a> Facet<'a>>(&self, file: &str) -> Option<T> {
        let path = self.dir.join(file);
        let text = std::fs::read_to_string(&path).ok()?;
        match facet_styx::from_str::<T>(&text) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!("rig store: {file} failed to parse ({e}) — using defaults");
                None
            }
        }
    }

    /// Serialize + write `value` to `file`, creating the directory. Failures
    /// are logged, never fatal (persistence must not take the rig down).
    pub fn write<T: for<'a> Facet<'a>>(&self, file: &str, value: &T) {
        if let Err(e) = std::fs::create_dir_all(&self.dir) {
            tracing::warn!("rig store: cannot create {}: {e}", self.dir.display());
            return;
        }
        match facet_styx::to_string(value) {
            Ok(text) => {
                if let Err(e) = std::fs::write(self.dir.join(file), text) {
                    tracing::warn!("rig store: write {file} failed: {e}");
                }
            }
            Err(e) => tracing::warn!("rig store: serialize {file} failed: {e}"),
        }
    }

    /// Read `file`, seeding it from the embedded default text when missing
    /// (written verbatim so the on-disk copy matches the repo snapshot).
    /// Falls back to the code-built default if the embedded text fails to
    /// parse — and persists that fallback so the directory stays complete.
    pub fn read_or_seed<T: for<'a> Facet<'a>>(
        &self,
        file: &str,
        seed: &str,
        fallback: impl FnOnce() -> T,
    ) -> T {
        if let Some(v) = self.read(file) {
            return v;
        }
        if let Err(e) = std::fs::create_dir_all(&self.dir) {
            tracing::warn!("rig store: cannot create {}: {e}", self.dir.display());
        } else if let Err(e) = std::fs::write(self.dir.join(file), seed) {
            tracing::warn!("rig store: seed {file} failed: {e}");
        } else {
            tracing::info!("rig store: seeded {file} from the in-repo default");
        }
        match facet_styx::from_str::<T>(seed) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("rig store: embedded default {file} failed to parse ({e})");
                let v = fallback();
                self.write(file, &v);
                v
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[derive(Clone, Debug, PartialEq, Facet)]
    struct Demo {
        #[facet(default)]
        name: String,
        #[facet(default)]
        count: u32,
    }

    fn tmp_store(tag: &str) -> StyxDir {
        StyxDir::new(std::env::temp_dir().join(format!("fts-rig-store-{tag}-{}", std::process::id())))
    }

    #[test]
    fn write_then_read_roundtrips() {
        let store = tmp_store("rt");
        let v = Demo {
            name: "Lead".into(),
            count: 3,
        };
        store.write("demo.styx", &v);
        assert_eq!(store.read::<Demo>("demo.styx"), Some(v));
    }

    #[test]
    fn read_or_seed_writes_the_embedded_default() {
        let store = tmp_store("seed");
        let _ = std::fs::remove_file(store.dir().join("seeded.styx"));
        let v: Demo = store.read_or_seed("seeded.styx", "name \"A\"\ncount 7\n", || Demo {
            name: "fallback".into(),
            count: 0,
        });
        assert_eq!(v.count, 7);
        assert!(store.dir().join("seeded.styx").exists());
    }

    #[test]
    fn missing_file_reads_none() {
        let store = tmp_store("miss");
        assert_eq!(store.read::<Demo>("nope.styx"), None);
    }
}
