//! Renders `LibrarySong`s into Task vault notes, matching Task's existing
//! frontmatter/body conventions exactly (see the task repo's
//! `crates/ui-core/src/frontmatter.rs` — `song_front_from` for the
//! `type: song` fields this writes, `setlist_songs_from_body` for the
//! `type: setlist` body format).
//!
//! Two things this deliberately does NOT attempt, because Task's own
//! code doesn't have anywhere to put them yet (see the research recorded
//! in docs/guides/session/getting-started.md):
//!
//! - `stems:` frontmatter with `content_hash` — that's Task's
//!   content-addressed blob path, which needs each stem uploaded through
//!   an org's attachment service first. These stems live as plain files
//!   under Assets/Tracks; nothing here uploads them.
//! - `sections:`/`bpm:`/`duration_sec:` — not derivable from a folder of
//!   audio files without analyzing them. Left absent; Task's hand-rolled
//!   frontmatter reader treats a missing key as `None`, not an error. A
//!   song with a real `chart/*.kf` chart (`LibrarySong::chart_kf`) DOES
//!   carry accurate tempo/time-signature/section-length data — just not
//!   in a form Task's frontmatter reader parses; the note embeds the
//!   chart text itself instead, in a fenced `keyflow` code block.
//!
//! What this writes instead: `artist:`/`key:` (real, from the folder
//! name/tag data). The note itself is written INTO the song's own Tracks
//! folder, alongside its `audio/`/`lyrics/`/`chart/` — so there's no
//! separate pointer field to the audio needed; the whole folder (note
//! included) is one portable, self-contained unit. Wiring Task's vault
//! to actually index this folder (rather than its own internal
//! `.task/orgs/*/vault`) so `[[wikilink]]`s resolve live is future
//! cross-repo work — see the note at the end of this file.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::library::LibrarySong;

/// Render a `type: song` note body for one library song.
pub fn song_note_markdown(song: &LibrarySong) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "---");
    let _ = writeln!(out, "type: song");
    let _ = writeln!(out, "artist: {}", song.artist);
    if let Some(key) = &song.key {
        let _ = writeln!(out, "key: {key}");
    }
    let _ = writeln!(out, "stems:");
    for stem in &song.stems {
        let rel_path = stem
            .path
            .strip_prefix(&song.folder)
            .unwrap_or(&stem.path)
            .display();
        let _ = writeln!(out, "  - name: \"{}\"", stem.label);
        let _ = writeln!(out, "    path: \"{rel_path}\"");
    }
    let _ = writeln!(out, "---");
    let _ = writeln!(out, "# {}", song.title);
    let _ = writeln!(out);
    match &song.key {
        Some(key) => {
            let _ = writeln!(out, "{} — {key}", song.artist);
        }
        None => {
            let _ = writeln!(out, "{}", song.artist);
        }
    }
    match &song.chart_kf {
        Some(chart) => {
            let _ = writeln!(out);
            let _ = writeln!(out, "## Chart");
            let _ = writeln!(out);
            let _ = writeln!(out, "```keyflow");
            let _ = writeln!(out, "{}", chart.trim_end());
            let _ = writeln!(out, "```");
        }
        None => {
            let _ = writeln!(out);
            let _ = writeln!(out, "_No chart yet._");
        }
    }
    if let Some(lyrics) = &song.lyrics_text {
        let _ = writeln!(out);
        let _ = writeln!(out, "## Lyrics");
        let _ = writeln!(out);
        let _ = writeln!(out, "{lyrics}");
    }
    out
}

/// The song's note title/filename/link-target: `"{Title} - {Artist}"`,
/// matching its Tracks folder name exactly. Titled by title alone, two
/// different artists' songs with the same name would collide (and
/// silently shadow each other in Task's link resolution) — the artist
/// suffix is the same disambiguator the Tracks folder naming already
/// uses, so a `[[Holy Forever - Bethel Music]]` link in a setlist note
/// is unambiguous even if another "Holy Forever" is added later.
pub fn song_link_name(song: &LibrarySong) -> String {
    format!("{} - {}", song.title, song.artist)
}

/// Write one `.md` file per song directly INTO the song's own Tracks
/// folder (`song.folder`), named `"{Title} - {Artist}.md"` (see
/// [`song_link_name`]) so a `[[Title - Artist]]` wikilink in a setlist
/// note resolves to it. Keeping the note inside the folder — rather than
/// in a separate vault location — is what makes the folder portable:
/// copy or zip it and the note, audio, lyrics, and chart all travel
/// together.
pub fn write_song_notes(songs: &[LibrarySong]) -> std::io::Result<Vec<PathBuf>> {
    let mut written = Vec::with_capacity(songs.len());
    for song in songs {
        let path = song.folder.join(format!("{}.md", song_link_name(song)));
        std::fs::write(&path, song_note_markdown(song))?;
        written.push(path);
    }
    Ok(written)
}

/// Render a `type: setlist` note: frontmatter, then one `[[Title - Artist]]`
/// wikilink per line in performance order — the primary/composable form
/// `setlist_songs_from_body` parses (plain standalone wikilink lines),
/// so this round-trips through Task's existing note-body parser with no
/// changes needed there.
pub fn setlist_note_markdown(name: &str, song_links: &[String]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "---");
    let _ = writeln!(out, "type: setlist");
    let _ = writeln!(out, "title: {name}");
    let _ = writeln!(out, "---");
    let _ = writeln!(out, "# {name}");
    let _ = writeln!(out);
    for link in song_links {
        let _ = writeln!(out, "[[{link}]]");
    }
    out
}

pub fn write_setlist_note(
    setlists_dir: &Path,
    name: &str,
    song_links: &[String],
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(setlists_dir)?;
    let path = setlists_dir.join(format!("{name}.md"));
    std::fs::write(&path, setlist_note_markdown(name, song_links))?;
    Ok(path)
}
