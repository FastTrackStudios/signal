//! Serving a song folder from this machine's disk — the same for every
//! backend: given the project's file, list the folder it sits in (the
//! project first) and read ranges of what is in it, never a path out of
//! it. The standalone engine and REAPER differ only in how they find the
//! project's file.

use std::path::{Component, Path, PathBuf};

use super::{MAX_READ, SongFile};
use crate::{DawError, DawResult};

/// What is never served: the DAW's own backups, and downloads in progress.
fn skipped(name: &str) -> bool {
    name.starts_with('.') || name.eq_ignore_ascii_case("Backups") || name.ends_with(".part")
}

/// The project's file (a `.RPP`, or a `.session` folder) made absolute,
/// and the song folder it sits in.
///
/// # Errors
///
/// The project has no file, or it is in no folder.
pub fn song_folder(project_file: &str) -> DawResult<(PathBuf, PathBuf)> {
    if project_file.is_empty() {
        return Err(DawError::NotFound("the project has no file".into()));
    }
    let file = std::path::absolute(PathBuf::from(project_file))
        .unwrap_or_else(|_| PathBuf::from(project_file));
    let folder = file
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| DawError::NotFound(format!("{project_file} is in no folder")))?;
    Ok((file, folder))
}

/// Every file in the song folder of `project_file`, with its size — the
/// project itself first (a `.session` is a folder: listed with size 0, its
/// files following as usual).
///
/// # Errors
///
/// As [`song_folder`].
pub fn list(project_file: &str) -> DawResult<Vec<SongFile>> {
    let (file, folder) = song_folder(project_file)?;
    let mut files = Vec::new();
    walk(&folder, &folder, &mut files);
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let project_rel = file.strip_prefix(&folder).map(relative).unwrap_or_default();
    let project_size = std::fs::metadata(&file)
        .ok()
        .filter(std::fs::Metadata::is_file)
        .map_or(0, |m| m.len());
    files.retain(|f| f.path != project_rel);
    files.insert(
        0,
        SongFile {
            path: project_rel,
            size: project_size,
        },
    );
    Ok(files)
}

/// `len` bytes (at most [`MAX_READ`]) of `path` in the song folder of
/// `project_file`, from `start`; fewer at the end of the file.
///
/// # Errors
///
/// `path` leaves the folder, or cannot be read.
pub fn read(project_file: &str, path: &str, start: u64, len: u32) -> DawResult<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let (_, folder) = song_folder(project_file)?;
    let file = inside(&folder, path)
        .ok_or_else(|| DawError::OperationFailed(format!("{path}: outside the song")))?;
    let mut f =
        std::fs::File::open(&file).map_err(|e| DawError::NotFound(format!("{path}: {e}")))?;
    f.seek(SeekFrom::Start(start))
        .map_err(|e| DawError::OperationFailed(format!("{path}: {e}")))?;
    let mut out = vec![0u8; usize::try_from(len.min(MAX_READ)).unwrap_or(0)];
    let mut got = 0usize;
    while got < out.len() {
        match f.read(&mut out[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) => return Err(DawError::OperationFailed(format!("{path}: {e}"))),
        }
    }
    out.truncate(got);
    Ok(out)
}

fn walk(folder: &Path, at: &Path, out: &mut Vec<SongFile>) {
    let Ok(entries) = std::fs::read_dir(at) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if skipped(&name) {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            walk(folder, &path, out);
        } else if let Ok(rel) = path.strip_prefix(folder) {
            out.push(SongFile {
                path: relative(rel),
                size: meta.len(),
            });
        }
    }
}

fn relative(rel: &Path) -> String {
    rel.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// `rel` inside `folder`, or `None` if it would leave it.
fn inside(folder: &Path, rel: &str) -> Option<PathBuf> {
    let rel = Path::new(rel);
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(folder.join(rel))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_outside_the_song_is_served() {
        let folder = Path::new("/songs/Washed");
        assert_eq!(
            inside(folder, "Media/Proxies/Bass.ogg"),
            Some(folder.join("Media/Proxies/Bass.ogg"))
        );
        assert_eq!(inside(folder, "../Other/secret"), None);
        assert_eq!(inside(folder, "/etc/passwd"), None);
        assert_eq!(inside(folder, "Media/../../x"), None);
    }
}
