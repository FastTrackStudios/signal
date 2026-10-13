//! Where a song's files are read from when it opens: this machine's disk,
//! or memory — a browser's copy of a song that arrived from a share link.
//!
//! Opening a song is one path ([`crate::open_core`]): which file opens,
//! whether it is prepared, its project text, its chart and lyrics. That
//! path reads through a [`Folder`], so a song opens the same way from
//! either, and only where the bytes sit differs. Paths are ordinary
//! paths: a [`Memory`] folder keeps its files under a root of its own
//! (`/share-<token>/Song.RPP`), and everything that works on a path —
//! `with_extension`, `parent`, `file_stem` — works on them unchanged.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// A song's files.
pub trait Folder: Send + Sync {
    /// The whole of the file at `path`.
    ///
    /// # Errors
    ///
    /// No such file, or it could not be read.
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>>;

    /// The names directly in the directory `dir` (files and directories).
    ///
    /// # Errors
    ///
    /// No such directory.
    fn list(&self, dir: &Path) -> std::io::Result<Vec<String>>;

    /// Whether `path` is a directory here.
    fn is_dir(&self, path: &Path) -> bool;

    /// Whether a prepared song can be saved back into it (a browser's copy
    /// is read-only: it prepares in memory and keeps it there).
    fn writable(&self) -> bool;

    /// The file at `path` as text.
    ///
    /// # Errors
    ///
    /// No such file, or it is not UTF-8.
    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        String::from_utf8(self.read(path)?)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// The one file in `dir` with the extension `ext`, if there is exactly
    /// one — two would be a guess.
    fn only_with_extension(&self, dir: &Path, ext: &str) -> Option<PathBuf> {
        let mut found = self
            .list(dir)
            .ok()?
            .into_iter()
            .map(|name| dir.join(name))
            .filter(|p| {
                p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)) && !self.is_dir(p)
            });
        let one = found.next()?;
        found.next().is_none().then_some(one)
    }
}

/// This machine's disk.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, Default)]
pub struct Disk;

#[cfg(not(target_arch = "wasm32"))]
impl Folder for Disk {
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn list(&self, dir: &Path) -> std::io::Result<Vec<String>> {
        std::fs::read_dir(dir)?
            .map(|entry| entry.map(|e| e.file_name().to_string_lossy().into_owned()))
            .collect()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn writable(&self) -> bool {
        true
    }
}

/// The folder a song opens from when nothing else is said: the disk
/// natively; in a browser there is no default.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn disk() -> &'static dyn Folder {
    &Disk
}

/// Files held in memory, under paths of their own.
#[derive(Clone, Default)]
pub struct Memory {
    files: Arc<RwLock<BTreeMap<PathBuf, Arc<[u8]>>>>,
}

impl std::fmt::Debug for Memory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.files.read().map_or(0, |files| files.len());
        f.debug_struct("Memory").field("files", &count).finish()
    }
}

impl Memory {
    /// An empty folder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Put `bytes` at `path`.
    pub fn insert(&self, path: impl Into<PathBuf>, bytes: impl Into<Arc<[u8]>>) {
        if let Ok(mut files) = self.files.write() {
            files.insert(path.into(), bytes.into());
        }
    }

    /// Whether a file is at `path` (and how long).
    #[must_use]
    pub fn len_of(&self, path: &Path) -> Option<u64> {
        let files = self.files.read().ok()?;
        files
            .get(path)
            .map(|b| u64::try_from(b.len()).unwrap_or(u64::MAX))
    }
}

fn not_found(path: &Path) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("{} is not here", path.display()),
    )
}

impl Folder for Memory {
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        let files = self.files.read().map_err(|_| not_found(path))?;
        files
            .get(path)
            .map(|b| b.to_vec())
            .ok_or_else(|| not_found(path))
    }

    fn list(&self, dir: &Path) -> std::io::Result<Vec<String>> {
        let files = self.files.read().map_err(|_| not_found(dir))?;
        let mut names: Vec<String> = files
            .keys()
            .filter_map(|p| p.strip_prefix(dir).ok())
            .filter_map(|rest| rest.components().next())
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .collect();
        names.dedup();
        if names.is_empty() {
            Err(not_found(dir))
        } else {
            Ok(names)
        }
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.files
            .read()
            .is_ok_and(|files| files.keys().any(|p| p != path && p.starts_with(path)))
    }

    fn writable(&self) -> bool {
        false
    }
}

/// The REAPER project text of the `.session` directory `dir` in `folder`:
/// its manifest and its objects, read wherever they are, opened as
/// `daw_standalone::session_file` opens one.
///
/// # Errors
///
/// No manifest (or more than one), an object that is not what its name
/// says, or a manifest that does not parse.
pub fn session_text(folder: &dyn Folder, dir: &Path) -> eyre::Result<String> {
    let names = folder
        .list(dir)
        .map_err(|e| eyre::eyre!("session {}: {e}", dir.display()))?;
    let manifest = daw::standalone::session_file::choose_manifest(&names)
        .map_err(|reason| eyre::eyre!("session {}: {reason}", dir.display()))?;
    let manifest_path = dir.join(manifest);
    let text = folder.read_to_string(&manifest_path)?;
    let objects_dir = dir.join(daw::standalone::session_file::OBJECTS_DIR);
    let mut objects = Vec::new();
    for name in folder.list(&objects_dir).unwrap_or_default() {
        objects.push((name.clone(), folder.read(&objects_dir.join(&name))?));
    }
    daw::standalone::session_file::session_rpp_text_from_parts(
        &text,
        &manifest_path.to_string_lossy(),
        objects,
    )
    .map_err(|e| eyre::eyre!(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_memory_folder_lists_and_reads_like_a_directory() {
        let folder = Memory::new();
        folder.insert("/s/Song.RPP", b"rpp".to_vec());
        folder.insert("/s/Song.session/Song.session", b"manifest".to_vec());
        folder.insert("/s/Song.session/objects/sha256-aa", b"obj".to_vec());
        folder.insert("/s/Song.kf", b"chart".to_vec());
        let mut names = folder.list(Path::new("/s")).unwrap();
        names.sort();
        assert_eq!(names, ["Song.RPP", "Song.kf", "Song.session"]);
        assert!(folder.is_dir(Path::new("/s/Song.session")));
        assert!(!folder.is_dir(Path::new("/s/Song.RPP")));
        assert_eq!(folder.read(Path::new("/s/Song.kf")).unwrap(), b"chart");
        assert_eq!(
            folder.only_with_extension(Path::new("/s"), "kf"),
            Some(PathBuf::from("/s/Song.kf"))
        );
        assert_eq!(
            folder.only_with_extension(Path::new("/s"), "session"),
            None,
            "a directory is not a file"
        );
        assert!(folder.read(Path::new("/s/missing")).is_err());
    }
}
