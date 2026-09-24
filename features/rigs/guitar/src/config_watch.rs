//! Live config: noticing when a library file changed under the running rig,
//! and never writing over a change the rig has not seen.
//!
//! The rig library is plain styx files meant to be edited by hand, by a
//! script or by an LLM while the rig plays. Two things make that safe:
//!
//! - **Every file the rig reads or writes is remembered** by the hash of
//!   its content ([`note_read`], [`write_guarded`]). A file whose content is
//!   something else was changed by someone else.
//! - **A save never overwrites such a file** ([`write_guarded`]): the rig's
//!   copy is older than the file, so the file wins. The [`Watcher`] then
//!   loads the file into the running rig (see `session::hot_reload`). A file
//!   that failed to parse is treated the same way — it is someone's work
//!   with a typo in it, and the next good save of it applies.
//!
//! The watcher is a poll, not an OS watch: the rig's meter pump already
//! ticks, a stat of a dozen files twice a second costs nothing, and a poll
//! sees a change made while nothing was listening (a restart, a sleep).
//! Editors save in bursts (a temp file, a rename, a second write), so a
//! change is only reported once the file has held still for one whole poll.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use facet::Facet;

/// What the rig knows about one file.
#[derive(Clone, Debug, Default)]
struct Known {
    /// The content the rig last read or wrote — the state its memory holds.
    good: Option<blake3::Hash>,
    /// That content re-serialised: a save producing exactly this changes
    /// nothing, so it is skipped and the file keeps its hand formatting.
    canonical: Option<blake3::Hash>,
    /// Content that failed to parse — reported once, never overwritten.
    bad: Option<blake3::Hash>,
}

static KNOWN: Mutex<BTreeMap<PathBuf, Known>> = Mutex::new(BTreeMap::new());

fn known() -> std::sync::MutexGuard<'static, BTreeMap<PathBuf, Known>> {
    KNOWN.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn hash(bytes: &[u8]) -> blake3::Hash {
    blake3::hash(bytes)
}

/// A file read and parsed.
#[derive(Debug)]
pub enum Read<T> {
    /// Not there.
    Missing,
    /// Parsed.
    Ok(T),
    /// There, and does not parse — the parser's message (with its line and
    /// column when it gives them).
    Bad(String),
}

/// Parse `text` as `T`.
///
/// # Errors
///
/// The parser's message when `text` is not a `T`.
pub fn parse<T: for<'a> Facet<'a>>(text: &str) -> Result<T, String> {
    facet_styx::from_str::<T>(text).map_err(|e| match e.span {
        Some(span) => {
            let (line, col) = line_col(text, span.offset as usize);
            format!("line {line}, column {col}: {e}")
        }
        None => e.to_string(),
    })
}

/// 1-based line and column (in characters) of byte `offset` in `text`.
fn line_col(text: &str, offset: usize) -> (usize, usize) {
    let mut end = offset.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let before = text.get(..end).unwrap_or_default();
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, col)
}

/// Read `path` without telling anyone — for a save that merges with what
/// is on disk. Remembering it would make an unseen external edit look like
/// the rig's own, and the watcher would never load it.
pub fn read_quiet<T: for<'a> Facet<'a>>(path: &Path) -> Read<T> {
    match std::fs::read_to_string(path) {
        Err(_) => Read::Missing,
        Ok(text) => match parse(&text) {
            Ok(v) => Read::Ok(v),
            Err(e) => Read::Bad(e),
        },
    }
}

/// Read `path` into the rig: remembered as the content its memory now holds
/// (or, when it does not parse, as content never to overwrite).
pub fn read_tracked<T: for<'a> Facet<'a>>(path: &Path) -> Read<T> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Read::Missing;
    };
    match parse::<T>(&text) {
        Ok(v) => {
            note_read(path, &text, &v);
            Read::Ok(v)
        }
        Err(e) => {
            note_bad(path, &text);
            Read::Bad(e)
        }
    }
}

/// `text` (parsed as `value`) is what the rig now holds for `path`.
pub fn note_read<T: for<'a> Facet<'a>>(path: &Path, text: &str, value: &T) {
    let canonical = facet_styx::to_string(value).ok().map(|t| hash(t.as_bytes()));
    known().insert(
        path.to_path_buf(),
        Known {
            good: Some(hash(text.as_bytes())),
            canonical,
            bad: None,
        },
    );
}

/// `text` at `path` does not parse: never overwrite it, report it once.
pub fn note_bad(path: &Path, text: &str) {
    known().entry(path.to_path_buf()).or_default().bad = Some(hash(text.as_bytes()));
}

/// Forget `path` (it was deleted, by the rig or by hand).
pub fn forget(path: &Path) {
    known().remove(path);
}

/// What a guarded save did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrote {
    /// Written.
    Written,
    /// The file already says this — nothing written, formatting kept.
    Unchanged,
    /// The file changed since the rig read it (or does not parse): left as
    /// it is. The watcher loads it.
    KeptExternal,
    /// The write itself failed (logged).
    Failed,
}

/// Whether the content at `path` is what the rig last read or wrote — the
/// question every save asks before it writes. A file the rig has never
/// seen is its to write; one it has seen must still say what it said.
fn still_ours(path: &Path, on_disk: Option<&[u8]>) -> bool {
    let k = known().get(path).cloned();
    match (k, on_disk) {
        (_, None) | (None, Some(_)) => true,
        (Some(k), Some(bytes)) => k.good == Some(hash(bytes)),
    }
}

/// Serialise and write `value` to `path` — unless the file changed under
/// the rig since it last read or wrote it, or the file does not parse. The
/// rig's copy is then the older one, and writing it would silently undo
/// somebody's edit; the file is left alone and the watcher loads it.
pub fn write_guarded<T: for<'a> Facet<'a>>(path: &Path, value: &T) -> Wrote {
    let text = match facet_styx::to_string(value) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("rig store: serialize {} failed: {e}", path.display());
            return Wrote::Failed;
        }
    };
    write_text_guarded(path, &text)
}

/// [`write_guarded`] for text already serialised.
pub fn write_text_guarded(path: &Path, text: &str) -> Wrote {
    let on_disk = std::fs::read(path).ok();
    if !still_ours(path, on_disk.as_deref()) {
        let line = format!(
            "{}: changed outside the rig since it was loaded — the rig's copy was not written over it (the file wins)",
            display_name(path)
        );
        tracing::warn!("{line}");
        log_line(&line);
        return Wrote::KeptExternal;
    }
    let new = hash(text.as_bytes());
    let canonical = known().get(path).and_then(|k| k.canonical);
    if on_disk.as_deref().is_some_and(|b| hash(b) == new)
        || (on_disk.is_some() && canonical == Some(new))
    {
        return Wrote::Unchanged;
    }
    if let Some(dir) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            tracing::warn!("rig store: cannot create {}: {e}", dir.display());
            return Wrote::Failed;
        }
    }
    if let Err(e) = std::fs::write(path, text) {
        tracing::warn!("rig store: write {} failed: {e}", path.display());
        return Wrote::Failed;
    }
    known().insert(
        path.to_path_buf(),
        Known {
            good: Some(new),
            canonical: Some(new),
            bad: None,
        },
    );
    Wrote::Written
}

/// Write `value` to `path` whatever is there — for a file the rig alone
/// owns (`last-state.styx`).
pub fn write_owned<T: for<'a> Facet<'a>>(path: &Path, value: &T) {
    match facet_styx::to_string(value) {
        Ok(text) => {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Err(e) = std::fs::write(path, &text) {
                tracing::warn!("rig store: write {} failed: {e}", path.display());
                return;
            }
            known().insert(
                path.to_path_buf(),
                Known {
                    good: Some(hash(text.as_bytes())),
                    canonical: Some(hash(text.as_bytes())),
                    bad: None,
                },
            );
        }
        Err(e) => tracing::warn!("rig store: serialize {} failed: {e}", path.display()),
    }
}

/// `profiles/worship.styx`, `songs.styx` — a path as the log names it.
#[must_use]
pub fn display_name(path: &Path) -> String {
    let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    match path.parent().and_then(Path::file_name) {
        Some(dir) if dir == crate::library::PROFILES_DIR => format!("{}/{file}", crate::library::PROFILES_DIR),
        _ => file,
    }
}

// ── The watcher ──────────────────────────────────────────────────────────

/// A file's stat: modified time and length. `None` = not there.
type Stamp = Option<(SystemTime, u64)>;

fn stamp(path: &Path) -> Stamp {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

/// A file that changed under the rig and has held still since.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub path: PathBuf,
    /// It is gone.
    pub deleted: bool,
}

/// The library files worth watching: `*.styx` in the rig directory and in
/// `profiles/`.
fn library_files(dir: &Path) -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    for d in [dir.to_path_buf(), dir.join(crate::library::PROFILES_DIR)] {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.filter_map(Result::ok) {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "styx") && p.is_file() {
                    out.insert(p);
                }
            }
        }
    }
    out
}

/// Polls the library directory for changes the rig did not make.
#[derive(Debug, Default)]
pub struct Watcher {
    /// Every file's stamp as of its last settled look.
    seen: BTreeMap<PathBuf, Stamp>,
    /// Files whose stamp moved at the last poll: reported once a poll finds
    /// the same stamp again (the burst is over).
    moving: BTreeMap<PathBuf, Stamp>,
    primed: bool,
}

impl Watcher {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// One look at `dir`: the files that changed under the rig and have
    /// since held still. The rig's own writes are not changes; nor is a file
    /// whose bad content was already reported.
    pub fn poll(&mut self, dir: &Path) -> Vec<Change> {
        self.look(dir, false)
    }

    /// Every file that differs from what the rig holds, now — no waiting
    /// for a burst to finish (an explicit reload).
    pub fn force(&mut self, dir: &Path) -> Vec<Change> {
        self.look(dir, true)
    }

    fn look(&mut self, dir: &Path, now: bool) -> Vec<Change> {
        let mut paths = library_files(dir);
        paths.extend(self.seen.keys().filter(|p| p.starts_with(dir)).cloned());
        let mut changes = Vec::new();
        for path in paths {
            let st = stamp(&path);
            if !self.primed || now {
                // First look (or a forced one): settle everything at once;
                // anything already different from what the rig loaded is a
                // change that happened while nobody was looking.
                self.seen.insert(path.clone(), st);
                self.moving.remove(&path);
                if let Some(c) = classify(&path, !now) {
                    changes.push(c);
                }
                continue;
            }
            if self.seen.get(&path) == Some(&st) {
                self.moving.remove(&path);
                continue;
            }
            if self.moving.get(&path) == Some(&st) {
                // Held still for a whole poll: the save is done.
                self.moving.remove(&path);
                self.seen.insert(path.clone(), st);
                if st.is_none() {
                    self.seen.remove(&path);
                }
                if let Some(c) = classify(&path, false) {
                    changes.push(c);
                }
            } else {
                self.moving.insert(path, st);
            }
        }
        self.primed = true;
        changes
    }
}

/// Whether `path`'s content is news to the rig. `quiet_unknown`: a file the
/// rig has never read is not news (the first look, when nothing has read
/// `nodes.styx` yet, must not reload the world).
fn classify(path: &Path, quiet_unknown: bool) -> Option<Change> {
    let bytes = std::fs::read(path).ok();
    let k = known().get(path).cloned();
    match (bytes, k) {
        (None, None) => None,
        (None, Some(_)) => Some(Change {
            path: path.to_path_buf(),
            deleted: true,
        }),
        (Some(_), None) if quiet_unknown => None,
        (Some(_), None) => Some(Change {
            path: path.to_path_buf(),
            deleted: false,
        }),
        (Some(b), Some(k)) => {
            let h = hash(&b);
            (k.good != Some(h) && k.bad != Some(h)).then(|| Change {
                path: path.to_path_buf(),
                deleted: false,
            })
        }
    }
}

// ── The reload log, and the reload request ───────────────────────────────

/// Where each reload is logged: `SIGNAL_RELOAD_LOG` (a file), else
/// `/Volumes/dev-drive/logs/config-reload.log` when that drive is there,
/// else the temp dir.
#[must_use]
pub fn log_path() -> PathBuf {
    if let Ok(p) = std::env::var("SIGNAL_RELOAD_LOG") {
        if !p.is_empty() {
            return p.into();
        }
    }
    let drive = Path::new("/Volumes/dev-drive/logs");
    if drive.is_dir() {
        return drive.join("config-reload.log");
    }
    std::env::temp_dir().join("signal-config-reload.log")
}

/// Append one line to the reload log, stamped with the time.
pub fn log_line(text: &str) {
    use std::io::Write as _;
    let line = format!("{}  {text}\n", crate::drop_log::format_local(SystemTime::now()));
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// The file `signal rig reload` drops into the rig directory to ask a
/// running rig to re-read its config now. It holds a tag the rig puts on
/// its report lines, and the rig deletes it once done.
pub const RELOAD_REQUEST: &str = ".reload-request";

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cfg-watch-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        d
    }

    #[derive(Clone, Debug, PartialEq, Facet)]
    struct Doc {
        #[facet(default)]
        name: String,
        #[facet(default)]
        count: u32,
    }

    /// Ensure the next write gets a different mtime even on a coarse clock.
    fn tick() {
        std::thread::sleep(std::time::Duration::from_millis(15));
    }

    fn settle(w: &mut Watcher, d: &Path) -> Vec<Change> {
        let mut all = w.poll(d);
        all.extend(w.poll(d));
        all
    }

    #[test]
    fn the_rigs_own_write_is_not_a_change() {
        let d = dir("own");
        let f = d.join("songs.styx");
        std::fs::write(&f, "name \"A\"\ncount 1\n").unwrap();
        assert!(matches!(read_tracked::<Doc>(&f), Read::Ok(_)));
        let mut w = Watcher::new();
        assert!(w.poll(&d).is_empty(), "nothing changed since the load");
        tick();
        assert_eq!(write_guarded(&f, &Doc { name: "A".into(), count: 2 }), Wrote::Written);
        assert!(settle(&mut w, &d).is_empty(), "the rig wrote it, so it knows it");
    }

    #[test]
    fn an_external_edit_is_a_change() {
        let d = dir("ext");
        let f = d.join("profiles").join("worship.styx");
        std::fs::write(&f, "name \"A\"\ncount 1\n").unwrap();
        assert!(matches!(read_tracked::<Doc>(&f), Read::Ok(_)));
        let mut w = Watcher::new();
        assert!(w.poll(&d).is_empty());
        tick();
        std::fs::write(&f, "name \"A\"\ncount 9\n").unwrap();
        assert!(w.poll(&d).is_empty(), "not reported while it may still be moving");
        assert_eq!(w.poll(&d), vec![Change { path: f.clone(), deleted: false }]);
        assert!(w.poll(&d).is_empty(), "reported once");
    }

    #[test]
    fn a_burst_of_writes_is_one_change_once_it_stops() {
        let d = dir("burst");
        let f = d.join("setlists.styx");
        std::fs::write(&f, "name \"A\"\n").unwrap();
        assert!(matches!(read_tracked::<Doc>(&f), Read::Ok(_)));
        let mut w = Watcher::new();
        w.poll(&d);
        let mut reported = Vec::new();
        for i in 0..4 {
            tick();
            std::fs::write(&f, format!("name \"A\"\ncount {i}{}\n", " ".repeat(i))).unwrap();
            reported.extend(w.poll(&d));
        }
        assert!(reported.is_empty(), "nothing while the editor is still saving: {reported:?}");
        assert_eq!(w.poll(&d).len(), 1, "one change once it held still");
    }

    #[test]
    fn a_change_before_the_first_look_is_still_seen() {
        let d = dir("early");
        let f = d.join("midi.styx");
        std::fs::write(&f, "name \"A\"\n").unwrap();
        assert!(matches!(read_tracked::<Doc>(&f), Read::Ok(_)));
        std::fs::write(&f, "name \"B\"\n").unwrap();
        assert_eq!(Watcher::new().poll(&d).len(), 1);
    }

    #[test]
    fn a_save_never_overwrites_an_external_edit() {
        let d = dir("keep");
        let f = d.join("songs.styx");
        std::fs::write(&f, "name \"A\"\ncount 1\n").unwrap();
        assert!(matches!(read_tracked::<Doc>(&f), Read::Ok(_)));
        std::fs::write(&f, "name \"Mine\"\ncount 1\n").unwrap();
        assert_eq!(write_guarded(&f, &Doc { name: "A".into(), count: 5 }), Wrote::KeptExternal);
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "name \"Mine\"\ncount 1\n");
        // Once the rig has loaded it, its saves go through again.
        assert!(matches!(read_tracked::<Doc>(&f), Read::Ok(_)));
        assert_eq!(write_guarded(&f, &Doc { name: "Mine".into(), count: 5 }), Wrote::Written);
    }

    #[test]
    fn a_file_that_does_not_parse_is_never_written_over() {
        let d = dir("bad");
        let f = d.join("songs.styx");
        std::fs::write(&f, "name \"A\"\n").unwrap();
        assert!(matches!(read_tracked::<Doc>(&f), Read::Ok(_)));
        let mut w = Watcher::new();
        w.poll(&d);
        tick();
        std::fs::write(&f, "name \"A\"\ncount {{{\n").unwrap();
        let ch = settle(&mut w, &d);
        assert_eq!(ch.len(), 1);
        let Read::Bad(msg) = read_tracked::<Doc>(&f) else { panic!("it does not parse") };
        assert!(!msg.is_empty());
        assert!(msg.starts_with("line 2, column"), "the error says where: {msg}");
        assert!(settle(&mut w, &d).is_empty(), "a bad file is reported once");
        assert_eq!(write_guarded(&f, &Doc { name: "A".into(), count: 1 }), Wrote::KeptExternal);
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "name \"A\"\ncount {{{\n");
        // Fixed: the next good save of it is news again.
        tick();
        std::fs::write(&f, "name \"A\"\ncount 3\n").unwrap();
        assert_eq!(settle(&mut w, &d).len(), 1);
    }

    #[test]
    fn a_save_that_changes_nothing_keeps_the_hand_formatting() {
        let d = dir("fmt");
        let f = d.join("keymap.styx");
        let hand = "// my notes\nname   \"A\"\ncount 1\n";
        std::fs::write(&f, hand).unwrap();
        let Read::Ok(v) = read_tracked::<Doc>(&f) else { panic!() };
        assert_eq!(write_guarded(&f, &v), Wrote::Unchanged);
        assert_eq!(std::fs::read_to_string(&f).unwrap(), hand);
    }

    #[test]
    fn a_deleted_file_is_a_change() {
        let d = dir("del");
        let f = d.join("profiles").join("rock.styx");
        std::fs::write(&f, "name \"Rock\"\n").unwrap();
        assert!(matches!(read_tracked::<Doc>(&f), Read::Ok(_)));
        let mut w = Watcher::new();
        w.poll(&d);
        std::fs::remove_file(&f).unwrap();
        assert_eq!(settle(&mut w, &d), vec![Change { path: f, deleted: true }]);
    }
}
