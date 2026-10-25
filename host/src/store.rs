//! Styx-directory persistence for rig libraries.
//!
//! The read / write / read-or-seed trio each rig's `library.rs` re-implemented, over one
//! directory of plain styx text files (portable, git-trackable, editable) —
//! or, when a [`crate::book::RigBook`] stands for the directory (feature
//! `book`), over that book: the rig's library as one synced Loro doc.

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

/// This device's libraries — kits, packs, rig libraries brought from a
/// studio, laid out as the studio's: `$SIGNAL_LIBRARIES_DIR`, else
/// `<config>/signal/libraries` (on the iPad, in the Files app).
#[must_use]
pub fn libraries_dir() -> PathBuf {
    std::env::var_os("SIGNAL_LIBRARIES_DIR")
        .map_or_else(|| signal_config_dir().join("libraries"), PathBuf::from)
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
    #[must_use]
    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    /// Resolve a dir-relative asset path (`models/…`, `irs/…`) to absolute;
    /// empties pass through, and so do absolute paths that exist.
    ///
    /// An absolute path that does not exist here was written on another
    /// machine (a library carried over, or a default captured on a
    /// developer's box): the same file in this dir is used when there is
    /// one — under the folder it sat in (`…/frozen/x.nam` → `frozen/x.nam`)
    /// or under `models/` — so a library moves between machines without
    /// its paths being edited.
    pub fn resolve(&self, path: &mut String) {
        if path.is_empty() {
            return;
        }
        let given = std::path::Path::new(path.as_str());
        if !given.is_absolute() {
            self.note_asset(path);
            *path = self.dir.join(given).to_string_lossy().into_owned();
            return;
        }
        if given.exists() {
            return;
        }
        let Some(file) = given.file_name() else {
            return;
        };
        let folder = given.parent().and_then(std::path::Path::file_name);
        let found = folder
            .map(|f| self.dir.join(f).join(file))
            .into_iter()
            .chain(std::iter::once(self.dir.join("models").join(file)))
            .find(|p| p.exists());
        if let Some(p) = found {
            *path = p.to_string_lossy().into_owned();
        }
    }

    /// Inverse of [`resolve`](Self::resolve) for saves: paths under the dir
    /// are stored relative, so the on-disk library stays portable — including
    /// when the dir is a symlink into the repo's default-config (live edits
    /// become committable working-tree diffs).
    pub fn relativize(&self, path: &mut String) {
        if let Ok(rel) = std::path::Path::new(path.as_str()).strip_prefix(&self.dir) {
            *path = rel.to_string_lossy().into_owned();
            self.note_asset(path);
        }
    }

    /// A library's book keeps what each asset it uses is, by content
    /// (`RigBook::note_asset`); a directory keeps nothing.
    fn note_asset(&self, rel: &str) {
        #[cfg(all(feature = "book", not(target_arch = "wasm32")))]
        if let Some(book) = crate::book::for_dir(&self.dir) {
            book.note_asset(rel);
        }
        #[cfg(not(all(feature = "book", not(target_arch = "wasm32"))))]
        let _ = rel;
    }

    /// Read + parse `file`. `None` when missing; parse failures are logged
    /// and treated as missing (the caller falls back to defaults rather than
    /// dying mid-set over a hand-edit typo).
    pub fn read<T: for<'a> Facet<'a>>(&self, file: &str) -> Option<T> {
        #[cfg(all(feature = "book", not(target_arch = "wasm32")))]
        if let Some(book) =
            crate::book::for_dir(&self.dir).filter(|_| !crate::book::is_device_local(file))
        {
            return match book.read::<T>(file) {
                crate::book::BookRead::Ok(v) => Some(v),
                crate::book::BookRead::Missing => None,
                crate::book::BookRead::Bad(e) => {
                    tracing::warn!(
                        "rig store: {file} in the book is not readable ({e}) — using defaults"
                    );
                    None
                }
            };
        }
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
        #[cfg(all(feature = "book", not(target_arch = "wasm32")))]
        if let Some(book) =
            crate::book::for_dir(&self.dir).filter(|_| !crate::book::is_device_local(file))
        {
            if let Err(e) = book.write(file, value) {
                tracing::warn!("rig store: {file} was not written to the book: {e}");
            }
            return;
        }
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
    /// A file that exists and fails to parse is never overwritten: the
    /// default plays in memory until the file is fixed.
    /// Falls back to the code-built default if the embedded text fails to
    /// parse — and persists that fallback so the directory stays complete.
    pub fn read_or_seed<T: for<'a> Facet<'a>>(
        &self,
        file: &str,
        seed: &str,
        fallback: impl FnOnce() -> T,
    ) -> T {
        #[cfg(all(feature = "book", not(target_arch = "wasm32")))]
        if let Some(book) =
            crate::book::for_dir(&self.dir).filter(|_| !crate::book::is_device_local(file))
        {
            return match book.read::<T>(file) {
                crate::book::BookRead::Ok(v) => v,
                // Somebody's entry this build cannot read: the default
                // plays, and the entry is left as it is.
                crate::book::BookRead::Bad(e) => {
                    tracing::error!(
                        "rig store: {file} in the book does not parse ({e}) — playing the default"
                    );
                    facet_styx::from_str::<T>(seed).unwrap_or_else(|_| fallback())
                }
                crate::book::BookRead::Missing => {
                    let v = facet_styx::from_str::<T>(seed).unwrap_or_else(|_| fallback());
                    // Written in only when it may be: an account's book waits
                    // for the account's library (`RigBook::may_seed`).
                    if book.may_seed() {
                        if let Err(e) = book.write(file, &v) {
                            tracing::warn!("rig store: seeding {file} into the book failed: {e}");
                        }
                    }
                    v
                }
            };
        }
        if let Some(v) = self.read(file) {
            return v;
        }
        // A file that is there but does not parse is somebody's work with a
        // typo in it. Play the default for now and leave the file alone:
        // seeding over it would throw the work away, and fixing the typo
        // should be all it takes to get it back.
        if self.dir.join(file).exists() {
            tracing::error!(
                "rig store: {file} does not parse — playing the default for now and leaving the file as it is"
            );
            return facet_styx::from_str::<T>(seed).unwrap_or_else(|_| fallback());
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
