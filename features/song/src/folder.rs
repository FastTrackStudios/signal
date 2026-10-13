//! Song-folder (de)serialization: read/write a [`Song`] to a self-contained
//! folder of plaintext + attachment references.
//!
//! Layout (see `docs/song-folder-format.md` for the full spec):
//!
//! ```text
//! <song-root>/
//!   song.md                         # index: id, title, tags, default + arrangement list
//!   arrangements/
//!     <arr-dir>/
//!       arrangement.md              # full arrangement record (key, chartRef, parts, attachments)
//! ```
//!
//! `song.md` is the index (ordering, the default pointer, and per-arrangement
//! metadata incl. key + folder). Each `arrangement.md` is the authoritative
//! full record for its arrangement — the index's `key` is a convenience
//! mirror, and reads take the value from `arrangement.md`.

use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::key::Key;
use crate::model::{Arrangement, ArrangementId, Song, SongId};

const SONG_FILE: &str = "song.md";
const ARRANGEMENTS_DIR: &str = "arrangements";
const ARRANGEMENT_FILE: &str = "arrangement.md";

/// Error reading a song from a folder.
#[derive(Debug, Error)]
pub enum ReadError {
    #[error("io: {0}")]
    Io(String),
    #[error("missing frontmatter in {0}")]
    NoFrontmatter(String),
    #[error("yaml in {file}: {source}")]
    Yaml {
        file: String,
        #[source]
        source: serde_yaml::Error,
    },
    #[error("song references arrangement dir `{0}` but it is missing or unreadable")]
    MissingArrangement(String),
}

/// Error writing a song to a folder.
#[derive(Debug, Error)]
pub enum WriteError {
    #[error("io: {0}")]
    Io(String),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

// ── song.md index shape ───────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SongIndex {
    id: SongId,
    title: String,
    #[serde(default)]
    tags: Vec<String>,
    default_arrangement: ArrangementId,
    arrangements: Vec<IndexEntry>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexEntry {
    id: ArrangementId,
    name: String,
    /// Folder under `arrangements/` holding this arrangement's record.
    dir: String,
    /// Convenience mirror of the arrangement's key (authoritative copy
    /// lives in the arrangement record).
    key: Key,
}

// ── public API ────────────────────────────────────────────────────────

/// Write `song` to a folder rooted at `root`, creating it (and the
/// `arrangements/` subfolders) as needed.
///
/// # Errors
///
/// Returns [`WriteError`] if a directory or file cannot be written, or if
/// an arrangement/index fails to serialize.
pub fn to_folder(song: &Song, root: &Path) -> Result<(), WriteError> {
    std::fs::create_dir_all(root).map_err(|e| WriteError::Io(e.to_string()))?;
    let arr_root = root.join(ARRANGEMENTS_DIR);
    std::fs::create_dir_all(&arr_root).map_err(|e| WriteError::Io(e.to_string()))?;

    // Assign a stable, unique dir per arrangement.
    let mut entries = Vec::with_capacity(song.arrangements.len());
    let mut used: Vec<String> = Vec::new();
    for arr in &song.arrangements {
        let dir = unique_dir(&arr.name, arr.id, &used);
        used.push(dir.clone());

        let arr_dir = arr_root.join(&dir);
        std::fs::create_dir_all(&arr_dir).map_err(|e| WriteError::Io(e.to_string()))?;
        let yaml = serde_yaml::to_string(arr)?;
        std::fs::write(arr_dir.join(ARRANGEMENT_FILE), frontmatter(&yaml))
            .map_err(|e| WriteError::Io(e.to_string()))?;

        entries.push(IndexEntry {
            id: arr.id,
            name: arr.name.clone(),
            dir,
            key: arr.key,
        });
    }

    let index = SongIndex {
        id: song.id,
        title: song.title.clone(),
        tags: song.tags.clone(),
        default_arrangement: song.default_arrangement,
        arrangements: entries,
    };
    let yaml = serde_yaml::to_string(&index)?;
    std::fs::write(root.join(SONG_FILE), frontmatter(&yaml))
        .map_err(|e| WriteError::Io(e.to_string()))?;
    Ok(())
}

/// Read a [`Song`] back from a folder written by [`to_folder`].
///
/// Arrangements are returned in the order listed in `song.md`; each is read
/// from its `arrangement.md` (the authoritative record).
///
/// # Errors
///
/// Returns [`ReadError`] if `song.md` or an arrangement file is missing,
/// unreadable, missing its frontmatter fence, or fails to parse as YAML.
pub fn from_folder(root: &Path) -> Result<Song, ReadError> {
    let song_path = root.join(SONG_FILE);
    let raw = std::fs::read_to_string(&song_path).map_err(|e| ReadError::Io(e.to_string()))?;
    let fm = split_frontmatter(&raw)
        .ok_or_else(|| ReadError::NoFrontmatter(song_path.display().to_string()))?;
    let index: SongIndex = serde_yaml::from_str(fm).map_err(|e| ReadError::Yaml {
        file: song_path.display().to_string(),
        source: e,
    })?;

    let arr_root = root.join(ARRANGEMENTS_DIR);
    let mut arrangements = Vec::with_capacity(index.arrangements.len());
    for entry in &index.arrangements {
        let path = arr_root.join(&entry.dir).join(ARRANGEMENT_FILE);
        let raw = std::fs::read_to_string(&path)
            .map_err(|_| ReadError::MissingArrangement(entry.dir.clone()))?;
        let fm = split_frontmatter(&raw)
            .ok_or_else(|| ReadError::NoFrontmatter(path.display().to_string()))?;
        let arr: Arrangement = serde_yaml::from_str(fm).map_err(|e| ReadError::Yaml {
            file: path.display().to_string(),
            source: e,
        })?;
        arrangements.push(arr);
    }

    Ok(Song {
        id: index.id,
        title: index.title,
        tags: index.tags,
        default_arrangement: index.default_arrangement,
        arrangements,
    })
}

// ── helpers ───────────────────────────────────────────────────────────

/// Wrap serialized YAML in a `---` frontmatter fence (markdown-page form,
/// matching the vault-page convention).
fn frontmatter(yaml: &str) -> String {
    format!("---\n{yaml}---\n")
}

/// Split `---\n...\n---\n` frontmatter off the front of a markdown string,
/// returning the YAML body.
fn split_frontmatter(src: &str) -> Option<&str> {
    let rest = src.strip_prefix("---\n")?;
    let end = rest.find("\n---")?;
    // `end` sits on the `\n` that opens the closing fence, an ASCII byte,
    // so `..=end` is always a valid UTF-8 boundary.
    rest.get(..=end) // keep the trailing newline for yaml
}

/// Slugify a name into a folder-safe token.
fn slug(name: &str) -> String {
    let s: String = name
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let cleaned = s
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if cleaned.is_empty() {
        "arrangement".to_string()
    } else {
        cleaned
    }
}

/// A dir slug that is unique among `used`; disambiguates collisions with a
/// short id suffix so two arrangements named "Default" don't clobber.
fn unique_dir(name: &str, id: ArrangementId, used: &[String]) -> String {
    let base = slug(name);
    if !used.contains(&base) {
        return base;
    }
    let short = id.simple().to_string();
    let short = short.get(..8.min(short.len())).unwrap_or(&short);
    let candidate = format!("{base}-{short}");
    if !used.contains(&candidate) {
        return candidate;
    }
    // Extremely unlikely; fall back to the full id.
    format!("{base}-{}", id.simple())
}
