//! The songs worked on lately, and each song's alternatives.
//!
//! **Recent** is a list of songs (and setlists) in the order they were
//! last opened, kept beside the app's other memories
//! (`~/.local/share/Session/recent.tsv`): a line each, newest first —
//! when, the song, and which of its versions was open.
//!
//! **Alternatives** are a song's named checkpoints, as Logic keeps them: the
//! song as it stood when the checkpoint was made ("Organized", before the
//! mix), a branch of its project file (`Unbreakable.session/Organized`).
//! Making one does not open it: the song stays what is open, under its own
//! name. Any alternative opens as itself later, and saving it then writes
//! it.
//!
//! A song is known by its **base** — its folder and name without an
//! extension (`…/unbreakable/Unbreakable`) — so its `.RPP`, its `.session`
//! and its alternatives are one song.

use std::path::{Path, PathBuf};

/// How many songs the list keeps.
const KEPT: usize = 30;

/// A song or setlist opened lately.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recent {
    /// The song's base ([`base_of`]), or a setlist's path.
    pub base: PathBuf,
    /// The version that was open: the song's own file or an alternative.
    pub opened: PathBuf,
    /// When, seconds since the epoch.
    pub at: u64,
}

impl Recent {
    /// The song's name.
    #[must_use]
    pub fn name(&self) -> String {
        self.base
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
    }

    /// Whether it is a song (that can have alternatives) rather than a
    /// setlist.
    #[must_use]
    pub fn is_song(&self) -> bool {
        main_of(&self.base).is_some()
    }

    /// The alternative that was open, if it was one.
    #[must_use]
    pub fn opened_alternative(&self) -> Option<String> {
        alternative_name(&self.opened)
    }
}

/// One of a song's alternatives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alternative {
    pub name: String,
    pub path: PathBuf,
    /// When it was last saved, seconds since the epoch.
    pub saved: u64,
}

/// A song's base from any of its versions: `X.RPP`, `X.session` or
/// `X.session/Name` all give `…/X`. `None` for what is not a
/// song file (a setlist, a folder).
#[must_use]
pub fn base_of(path: &Path) -> Option<PathBuf> {
    // An alternative in a project file: `X.session/Organized`.
    #[cfg(feature = "native")]
    if let Some((file, _)) = crate::project_file::file_and_branch(path) {
        return Some(file.with_extension(""));
    }
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    if ext != "rpp" && ext != "session" {
        return None;
    }
    Some(path.with_extension(""))
}

/// The song's own version: its `.session` once it has one, else its
/// `.RPP`. `None` if neither is there.
#[must_use]
pub fn main_of(base: &Path) -> Option<PathBuf> {
    let session = base.with_extension("session");
    if session.is_file() {
        return Some(session);
    }
    ["RPP", "rpp"]
        .iter()
        .map(|ext| base.with_extension(ext))
        .find(|p| p.is_file())
}

/// The alternative `path` is, by name, if it is one.
#[must_use]
pub fn alternative_name(path: &Path) -> Option<String> {
    #[cfg(feature = "native")]
    if let Some((_, branch)) = crate::project_file::file_and_branch(path) {
        return (branch != dawfile_store::MAIN).then_some(branch);
    }
    let _ = path;
    None
}

/// A song's alternatives, the newest first.
#[must_use]
pub fn alternatives(base: &Path) -> Vec<Alternative> {
    let mut found: Vec<Alternative> = Vec::new();
    // The project file's branches, but the song's own.
    #[cfg(feature = "native")]
    {
        let file = base.with_extension("session");
        if crate::project_file::is_project_file(&file) {
            found.extend(
                crate::project_file::branches(&file)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|b| b.name != dawfile_store::MAIN)
                    .map(|b| Alternative {
                        path: file.join(&b.name),
                        name: b.name,
                        saved: u64::try_from(b.at).unwrap_or(0),
                    }),
            );
        }
    }
    found.sort_by(|a, b| b.saved.cmp(&a.saved).then_with(|| a.name.cmp(&b.name)));
    found
}

/// The songs and setlists opened lately, newest first — those still there.
#[must_use]
pub fn recent() -> Vec<Recent> {
    read().into_iter().filter(|r| r.opened.exists()).collect()
}

/// Note that `path` (a song's version, or a setlist) was just opened: to
/// the top of the list, once.
pub fn opened(path: &Path) {
    let Some(file) = list_file() else { return };
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let base = base_of(&path).unwrap_or_else(|| path.clone());
    let mut list = read();
    list.retain(|r| r.base != base);
    list.insert(
        0,
        Recent {
            base,
            opened: path,
            at: now(),
        },
    );
    list.truncate(KEPT);
    let text = list.iter().fold(String::new(), |mut text, r| {
        use std::fmt::Write as _;
        let _ = writeln!(
            text,
            "{}\t{}\t{}",
            r.at,
            r.base.display(),
            r.opened.display()
        );
        text
    });
    let written = file
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| {
            let part = file.with_extension("tsv.part");
            std::fs::write(&part, text).and_then(|()| std::fs::rename(&part, &file))
        });
    if let Err(e) = written {
        // Not fatal: the list is a convenience.
        tracing::warn!(error = %e, "could not note the song as opened");
    }
}

fn list_file() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("Session").join("recent.tsv"))
}

fn read() -> Vec<Recent> {
    let Some(text) = list_file().and_then(|f| std::fs::read_to_string(f).ok()) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let at = parts.next()?.parse().ok()?;
            let base = PathBuf::from(parts.next()?);
            let opened = PathBuf::from(parts.next()?);
            Some(Recent { base, opened, at })
        })
        .collect()
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Each open song's fingerprint as it was opened or last saved
/// (`daw::standalone::digest::fingerprint`), by project: what it is held
/// up to for "unsaved changes".
static SAVED: std::sync::Mutex<Option<std::collections::HashMap<String, u64>>> =
    std::sync::Mutex::new(None);

/// The song `project` is saved as it is now (or was just opened so).
pub fn mark_saved(daw: &daw::standalone::Standalone, project: &str) {
    if let Some(print) = daw::standalone::digest::fingerprint(daw, project) {
        SAVED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_or_insert_with(std::collections::HashMap::new)
            .insert(project.to_owned(), print);
    }
}

/// Whether the song `project` has changed since it was opened or last
/// saved; `None` if that is not known (opened some other way).
#[must_use]
pub fn unsaved(daw: &daw::standalone::Standalone, project: &str) -> Option<bool> {
    let saved = SAVED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()?
        .get(project)
        .copied()?;
    Some(daw::standalone::digest::fingerprint(daw, project)? != saved)
}

/// What to open once the start screen is back: a version picked from
/// inside a song (the top bar's Versions) goes back out of the song and
/// in again as that version.
static NEXT: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// Open `path` as soon as the start screen is shown ([`take_next`]).
pub fn open_next(path: PathBuf) {
    *NEXT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(path);
}

/// Whether an open was asked for from outside the window (the Action
/// List's `SESSION_OPEN`): the window goes back out of the song to take it.
static BACK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Open `path` from outside the window: as [`open_next`], and the window
/// asked to go back out of the song for it ([`take_back`]).
pub fn open_from_outside(path: PathBuf) {
    open_next(path);
    BACK.store(true, std::sync::atomic::Ordering::Release);
}

/// Ask the window to go back out of the song, to the start screen — for
/// what the start screen then opens itself (a new song,
/// `crate::project_new`).
pub fn go_back() {
    BACK.store(true, std::sync::atomic::Ordering::Release);
}

/// Whether the window should go back out of the song now — once.
pub fn take_back() -> bool {
    BACK.swap(false, std::sync::atomic::Ordering::AcqRel)
}

/// What was asked to be opened next, once.
#[must_use]
pub fn take_next() -> Option<PathBuf> {
    NEXT.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
}

/// "just now", "5 min ago", "3 h ago", "2 days ago" — when `at` was.
#[must_use]
pub fn ago(at: u64) -> String {
    let secs = now().saturating_sub(at);
    match secs {
        0..60 => "just now".to_owned(),
        60..3_600 => format!("{} min ago", secs / 60),
        3_600..86_400 => format!("{} h ago", secs / 3_600),
        86_400..172_800 => "yesterday".to_owned(),
        _ => format!("{} days ago", secs / 86_400),
    }
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use super::*;

    /// A song's `.RPP`, its project file and an alternative in it are one
    /// song; the alternative is known by its branch's name.
    #[test]
    fn every_version_of_a_song_is_the_song() {
        let tmp = tempfile::tempdir().expect("a folder");
        let base = tmp.path().join("Unbreakable");
        assert_eq!(base_of(&base.with_extension("RPP")), Some(base.clone()));
        let file = base.with_extension("session");
        dawfile_store::Store::open(&file).expect("a project file");
        assert_eq!(base_of(&file), Some(base.clone()));
        let organized = file.join("Organized");
        assert_eq!(base_of(&organized), Some(base));
        assert_eq!(alternative_name(&organized).as_deref(), Some("Organized"));
        assert_eq!(
            alternative_name(&file),
            None,
            "the song is not an alternative"
        );
        assert_eq!(base_of(Path::new("/sets/Friday.setlist")), None);
    }

    /// A song's own version is its project file once it has one, else its
    /// `.RPP`; its alternatives are the file's branches, newest first.
    #[test]
    fn a_songs_alternatives_and_its_own_version() {
        let tmp = tempfile::tempdir().expect("a folder");
        let base = tmp.path().join("Song");
        std::fs::write(base.with_extension("RPP"), "<REAPER_PROJECT\n>\n").expect("an rpp");
        assert_eq!(main_of(&base), Some(base.with_extension("RPP")));
        let file = base.with_extension("session");
        let store = dawfile_store::Store::open(&file).expect("a project file");
        assert_eq!(main_of(&base), Some(file.clone()));
        let doc = session::sync::loro::LoroDoc::new();
        for name in ["Organized", "Mix A"] {
            store.fork(name, dawfile_store::MAIN, &doc).expect("forked");
            std::thread::sleep(std::time::Duration::from_millis(1_100));
        }
        let found = alternatives(&base);
        let names: Vec<&str> = found.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Mix A", "Organized"]);
        assert_eq!(found[0].path, file.join("Mix A"));
        drop(store);
    }
}
