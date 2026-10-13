//! Reads Task's `Assets/Tracks` folder directly off disk into the song
//! library the setlist builder picks from.
//!
//! Convention observed on disk (Task's `days-to-praise` vault): one folder
//! per song, named `"{Title} - {Artist}"` — no key in the folder name;
//! key (and chart lyrics/sections) come from tag data instead, a
//! `lyrics/*.json` file in the song's own folder. Sibling subfolders
//! extend as needed (`lyrics/`, `chart/`, and audio under `audio/`):
//! stems live in `audio/ogg/` (preferred — smaller, what a future player
//! would stream) or `audio/wav/`.

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct Stem {
    /// Display label, e.g. "Drums (Live)", "EG 1".
    pub label: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct LibrarySong {
    /// The song's own folder under Tracks — where its note gets written,
    /// alongside its `audio/`/`lyrics/`/`chart/`, so the whole folder is
    /// one portable, self-contained unit.
    pub folder: PathBuf,
    pub title: String,
    pub artist: String,
    /// From the song's `lyrics/*.json` tag data, if present. Not part of
    /// the folder name — see the module doc.
    pub key: Option<String>,
    pub stems: Vec<Stem>,
    /// Lyric text, built from `lyrics/*.json` sections when present.
    pub lyrics_text: Option<String>,
    /// Real keyflow chart text from `chart/*.kf`, when a chart has been
    /// transcribed for this song. `None` means "no chart yet" — the note
    /// shows a placeholder rather than treating it as an error.
    pub chart_kf: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LyricsFile {
    #[serde(default)]
    artist: Option<String>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    sections: Vec<LyricsSection>,
}

#[derive(Debug, Deserialize)]
struct LyricsSection {
    label: String,
    #[serde(default)]
    lines: Vec<String>,
}

fn parse_folder_name(name: &str) -> Option<(String, String)> {
    let (title, artist) = name.split_once(" - ")?;
    Some((title.trim().to_string(), artist.trim().to_string()))
}

/// Stem label from a stem filename: everything after the first `" - "`
/// (stems are always named `"{Title} - {Label}.ext"`). Deliberately not
/// matched against the parsed song title — the raw audio files keep an
/// older underscore-for-apostrophe convention (`"I_m"`) that no longer
/// matches the folder/title spelling, so a generic split is more robust
/// than an exact-prefix match.
fn stem_label(filename: &str) -> String {
    filename
        .split_once(" - ")
        .map(|(_, rest)| rest.to_string())
        .unwrap_or_else(|| filename.to_string())
}

fn collect_stems_from(dir: &Path, ext: &str) -> Vec<Stem> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut stems: Vec<Stem> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            if path.extension().and_then(|e| e.to_str()) != Some(ext) {
                return None;
            }
            let stem = path.file_stem()?.to_str()?.to_string();
            Some(Stem {
                label: stem_label(&stem),
                path,
            })
        })
        .collect();
    stems.sort_by(|a, b| a.label.cmp(&b.label));
    stems
}

/// Stems: prefer `audio/ogg/` (small enough to stream comfortably), then
/// `audio/wav/`. Falls back to the older, pre-`audio/`-nesting layout
/// (`ogg/`/`wav/` or bare `.wav` files directly in the song folder) for
/// any folder that hasn't been migrated yet.
fn find_stems(song_dir: &Path) -> Vec<Stem> {
    for (parent, ext) in [
        (song_dir.join("audio").join("ogg"), "ogg"),
        (song_dir.join("audio").join("wav"), "wav"),
        (song_dir.join("ogg"), "ogg"),
        (song_dir.join("wav"), "wav"),
    ] {
        if parent.is_dir() {
            let stems = collect_stems_from(&parent, ext);
            if !stems.is_empty() {
                return stems;
            }
        }
    }
    collect_stems_from(song_dir, "wav")
}

/// Read the song's `lyrics/*.json` tag data, if present.
fn find_lyrics(song_dir: &Path) -> Option<LyricsFile> {
    let lyrics_dir = song_dir.join("lyrics");
    let entries = std::fs::read_dir(&lyrics_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json")
            && let Ok(text) = std::fs::read_to_string(&path)
            && let Ok(parsed) = serde_json::from_str::<LyricsFile>(&text)
        {
            return Some(parsed);
        }
    }
    None
}

/// Read the song's real keyflow chart, if one has been transcribed to
/// `chart/*.kf`.
fn find_chart_kf(song_dir: &Path) -> Option<String> {
    let chart_dir = song_dir.join("chart");
    let entries = std::fs::read_dir(&chart_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("kf")
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            return Some(text);
        }
    }
    None
}

fn chart_text_from_sections(sections: &[LyricsSection]) -> Option<String> {
    if sections.is_empty() {
        return None;
    }
    let mut out = String::new();
    for section in sections {
        out.push_str(&section.label);
        out.push('\n');
        for line in &section.lines {
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }
    Some(out.trim_end().to_string())
}

/// Scan `tracks_dir` for song folders and build the library.
///
/// Folders that don't match the `"{Title} - {Artist}"` convention, or
/// that contain no playable stems, are skipped rather than erroring —
/// the Tracks folder is hand-maintained and will always have odd entries
/// (a song mid-import with only alternate takes, a stray file, etc.).
pub fn scan(tracks_dir: &Path) -> Vec<LibrarySong> {
    let Ok(entries) = std::fs::read_dir(tracks_dir) else {
        eprintln!("tracks directory not readable: {}", tracks_dir.display());
        return Vec::new();
    };

    let mut songs: Vec<LibrarySong> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|entry| {
            let folder_name = entry.file_name().to_string_lossy().into_owned();
            let (title, folder_artist) = parse_folder_name(&folder_name)?;
            let stems = find_stems(&entry.path());
            if stems.is_empty() {
                return None;
            }
            let lyrics = find_lyrics(&entry.path());
            let artist = lyrics
                .as_ref()
                .and_then(|l| l.artist.clone())
                .unwrap_or(folder_artist);
            let key = lyrics.as_ref().and_then(|l| l.key.clone());
            let lyrics_text = lyrics
                .as_ref()
                .and_then(|l| chart_text_from_sections(&l.sections));
            let chart_kf = find_chart_kf(&entry.path());
            Some(LibrarySong {
                folder: entry.path(),
                title,
                artist,
                key,
                lyrics_text,
                chart_kf,
                stems,
            })
        })
        .collect();

    songs.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    songs
}
