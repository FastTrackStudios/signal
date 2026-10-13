//! Home page data layer: the two on-disk track libraries + the setlist
//! notes saved under them.
//!
//! Reuses `session-vault-sync` (built for the `session-vault-sync` /
//! `session-player` CLIs) rather than re-implementing scanning or note
//! writing here — this module is just "aggregate multiple roots" +
//! "list/read/write setlist notes" glue on top of it.

use std::path::{Path, PathBuf};

use session_vault_sync::library::{self, LibrarySong};
use session_vault_sync::setlist::{parse_setlist_links, resolve_song};
use session_vault_sync::vault::{song_link_name, write_setlist_note};

/// Named track-library roots. Each root's songs live under
/// `root/Tracks`; its setlist notes are saved under `root/Setlists`
/// (created on first save). A setlist may freely mix songs from more
/// than one root — resolution always searches the combined library.
pub const LIBRARY_ROOTS: &[(&str, &str)] = &[
    ("FastTrackAudio", "/home/cody/Task/Assets/fasttrackaudio"),
    ("Days to Praise", "/home/cody/Task/Assets/days-to-praise"),
];

fn tracks_dir(root: &str) -> PathBuf {
    Path::new(root).join("Tracks")
}

fn setlists_dir(root: &str) -> PathBuf {
    Path::new(root).join("Setlists")
}

/// One song, tagged with which library root it came from (for display
/// grouping and for defaulting a new setlist's save destination).
#[derive(Clone)]
pub struct LibraryEntry {
    pub library: &'static str,
    pub song: LibrarySong,
}

/// Scan every configured root's `Tracks/` folder and merge the results,
/// sorted by title. Cheap enough to re-run on demand (folder scans, no
/// audio decode) — call it fresh whenever Home is shown or a setlist is
/// saved, rather than caching.
pub fn scan_libraries() -> Vec<LibraryEntry> {
    let mut all: Vec<LibraryEntry> = LIBRARY_ROOTS
        .iter()
        .flat_map(|(label, root)| {
            library::scan(&tracks_dir(root))
                .into_iter()
                .map(move |song| LibraryEntry {
                    library: label,
                    song,
                })
        })
        .collect();
    all.sort_by(|a, b| {
        a.song
            .title
            .to_lowercase()
            .cmp(&b.song.title.to_lowercase())
    });
    all
}

/// A saved setlist note, without its songs resolved yet (cheap to list).
#[derive(Clone)]
pub struct SetlistSummary {
    pub path: PathBuf,
    pub title: String,
    pub library: &'static str,
}

/// Pull `title:` out of a note's frontmatter, but only for notes whose
/// frontmatter says `type: setlist` — anything else in the folder (a
/// song note that happens to live elsewhere, a stray file) is skipped.
fn read_setlist_title(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut is_setlist = false;
    let mut title = None;
    for line in lines.by_ref().take(10) {
        let line = line.trim();
        if line == "---" {
            break;
        }
        if let Some(rest) = line.strip_prefix("type:") {
            is_setlist = rest.trim() == "setlist";
        } else if let Some(rest) = line.strip_prefix("title:") {
            title = Some(rest.trim().to_string());
        }
    }
    if is_setlist { title } else { None }
}

/// List every setlist note found under any configured root's
/// `Setlists/` folder.
pub fn list_setlists() -> Vec<SetlistSummary> {
    let mut out = Vec::new();
    for (label, root) in LIBRARY_ROOTS {
        let dir = setlists_dir(root);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            if let Some(title) = read_setlist_title(&path) {
                out.push(SetlistSummary {
                    path,
                    title,
                    library: label,
                });
            }
        }
    }
    out.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    out
}

/// Read a setlist note's `[[wikilink]]` songs and resolve each against
/// the combined library. Unresolved links (a song removed from disk, a
/// typo) are returned separately rather than failing the whole load.
pub fn read_setlist_songs(
    path: &Path,
    all: &[LibraryEntry],
) -> std::io::Result<(Vec<LibrarySong>, Vec<String>)> {
    let text = std::fs::read_to_string(path)?;
    let known: Vec<LibrarySong> = all.iter().map(|e| e.song.clone()).collect();
    let mut songs = Vec::new();
    let mut warnings = Vec::new();
    for link in parse_setlist_links(&text) {
        match resolve_song(&known, &link) {
            Ok(song) => songs.push(song.clone()),
            Err(e) => warnings.push(e),
        }
    }
    Ok((songs, warnings))
}

/// The default `Setlists/` destination for a new setlist: whichever
/// root most of the given songs' libraries agree on (ties broken by
/// `LIBRARY_ROOTS` order), falling back to the first configured root
/// when `songs` is empty.
pub fn default_destination(songs: &[LibraryEntry]) -> PathBuf {
    let winner = LIBRARY_ROOTS
        .iter()
        .max_by_key(|(label, _)| songs.iter().filter(|e| e.library == *label).count())
        .unwrap_or(&LIBRARY_ROOTS[0]);
    setlists_dir(winner.1)
}

/// Save a new setlist note: one `[[Title - Artist]]` link per song, in
/// order, written into `destination_dir` (created if needed).
pub fn create_setlist(
    name: &str,
    songs: &[LibrarySong],
    destination_dir: &Path,
) -> std::io::Result<PathBuf> {
    let links: Vec<String> = songs.iter().map(song_link_name).collect();
    write_setlist_note(destination_dir, name, &links)
}
