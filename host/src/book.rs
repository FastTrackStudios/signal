//! A rig's library as one Loro doc, kept in a `.session` file.
//!
//! The rig libraries were directories of styx files; they are one doc each
//! now (docs/design/sync.md, ADR 0002), so a library syncs between devices,
//! merges what two of them changed, and keeps its history. A [`RigBook`]
//! is that doc: every library file the rig read and wrote (`songs.styx`,
//! `profiles/worship.styx`, …) is an entry under the same name, its value
//! the file's struct as the Loro codec lays it out — a field per entry, a
//! list of things with ids a keyed list — so two devices changing
//! different fields both land.
//!
//! The rig's code does not change. A book is [`register`]ed for the
//! library directory it replaces, and the typed reads and writes every rig
//! already makes (`StyxDir`, the guitar's `config_watch`) find it by path
//! ([`for_path`]) and go to the doc instead of the file. What arrives from
//! another device is reported by [`RigBook::take_changes`] under the same
//! names, so the rig's hot reload applies it the way it applied an edited
//! file.
//!
//! **Moving in** is lazy and typed: a name the doc does not have yet is
//! read once from the old file (the caller knows its type), written into
//! the doc, and the doc is the library from then on. The directory is left
//! as it was — a backup, never written again. Binary files (NAM captures,
//! a guitar's IRs) stay files until the blobs layer carries them.
//!
//! **Assets are not in the doc.** NAM captures, IRs and the like are files,
//! moved between devices by file sync; the doc says how the rig uses them
//! (`models/Fender Clean.nam` in a preset) and, in its asset index
//! ([`RigBook::note_asset`]), what each of those files is by content — a
//! BLAKE3 hash and a size — so a device can tell a capture it lacks, fetch
//! it by hash, and find it whatever it is called on disk.
//!
//! **Seeding** the shipped defaults into a library that syncs would write
//! them into the account's doc ahead of the account's own library, and
//! bring back what was deleted elsewhere. So a book seeds only when it may
//! ([`RigBook::may_seed`]): it has a history of its own on this device, or
//! nothing syncs it. Until then the defaults play from memory.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use facet::Facet;
use loro::{EventTriggerKind, LoroDoc, LoroMap};
use session_format::store::{MAIN, Store};
use signal_proto::doc::codec;

/// The root map every entry lives under.
const ROOT: &str = "library";

/// What a step written by the rig is named in the file's history.
const LABEL: &str = "Rig";

/// The entry the asset index lives under (not a library file: the rig's
/// hot reload passes it by).
const ASSETS: &str = "assets.index";

/// An asset as the book knows it: what it is, by content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetRef {
    /// BLAKE3, hex.
    pub hash: String,
    pub size: u64,
}

/// What reading an entry found.
#[derive(Debug)]
pub enum BookRead<T> {
    Ok(T),
    /// Not in the book, nor in the directory it replaced.
    Missing,
    /// There, and not this type (a newer build's, or a hand-made mistake):
    /// the caller plays its default and does not write over it.
    Bad(String),
}

/// Why a book would not open.
#[derive(Debug, thiserror::Error)]
pub enum BookError {
    #[error("the library file: {0}")]
    Store(#[from] session_format::store::StoreError),
}

/// A rig's library: one Loro doc, its `.session` file, and the directory
/// it stands for.
pub struct RigBook {
    /// The library directory the rig's paths are under.
    dir: PathBuf,
    /// The `.session` file the doc is kept in.
    file: PathBuf,
    doc: LoroDoc,
    /// The names an import changed, for the rig's hot reload.
    changes: Arc<Mutex<BTreeSet<String>>>,
    /// Bumped on every change, the rig's own included: a cache's stamp.
    generation: Arc<AtomicU64>,
    /// Whether the shipped defaults may be written in ([`Self::may_seed`]).
    seeding: AtomicBool,
    /// The names this device has moved in from the directory (kept in its
    /// `.book-moved-in`): their files are backups now, never read again —
    /// or deleting an entry would bring it straight back.
    moved: Mutex<BTreeSet<String>>,
    /// Each asset as last hashed (modified, size): not hashed again unchanged.
    hashed: Mutex<std::collections::HashMap<String, (Option<std::time::SystemTime>, u64)>>,
    /// The file's own writer, when the book keeps its file itself.
    _local: Option<loro::Subscription>,
    _root: loro::Subscription,
    /// What keeps a doc someone else owns alive ([`Self::over`]).
    _keep: Option<Box<dyn std::any::Any + Send + Sync>>,
}

impl std::fmt::Debug for RigBook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RigBook")
            .field("dir", &self.dir)
            .field("file", &self.file)
            .finish_non_exhaustive()
    }
}

fn locked<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    #[allow(
        clippy::disallowed_methods,
        reason = "library persistence: a save or an import, never the audio thread"
    )]
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl RigBook {
    /// Open (or make) the book kept in `file`, standing for the library
    /// directory `dir`. `syncs`: an account's sync carries this book, so it
    /// waits for that before writing defaults ([`Self::may_seed`]).
    ///
    /// # Errors
    ///
    /// The file would not open or load.
    pub fn open(file: &Path, dir: &Path, syncs: bool) -> Result<Arc<Self>, BookError> {
        if let Some(parent) = file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let store = Store::open(file)?;
        let doc = store.load(MAIN)?;
        let fresh = doc.oplog_vv().is_empty();
        let store = Arc::new(Mutex::new(store));
        // Every commit the rig makes goes into the file as one step.
        let local = doc.subscribe_local_update(Box::new({
            let store = Arc::clone(&store);
            move |bytes| {
                if let Err(e) = locked(&store).append_update(MAIN, LABEL, bytes) {
                    tracing::error!(rig.book = "append", "rig book: a change was not kept: {e}");
                }
                true
            }
        }));
        Ok(Self::assemble(
            dir,
            file,
            doc,
            syncs,
            fresh,
            Some(local),
            None,
        ))
    }

    /// A book over `doc`, a replica someone else keeps — the sync
    /// driver, which persists it to `file` and syncs it with the
    /// account's other devices. `keep` is held as long as the book (the
    /// driver's handle on the doc). The book only reads and writes it.
    #[must_use]
    pub fn over(
        dir: &Path,
        file: &Path,
        doc: LoroDoc,
        keep: Box<dyn std::any::Any + Send + Sync>,
    ) -> Arc<Self> {
        let fresh = doc.oplog_vv().is_empty();
        Self::assemble(dir, file, doc, true, fresh, None, Some(keep))
    }

    fn assemble(
        dir: &Path,
        file: &Path,
        doc: LoroDoc,
        syncs: bool,
        fresh: bool,
        local: Option<loro::Subscription>,
        keep: Option<Box<dyn std::any::Any + Send + Sync>>,
    ) -> Arc<Self> {
        let changes = Arc::new(Mutex::new(BTreeSet::new()));
        let generation = Arc::new(AtomicU64::new(0));
        let root = doc.subscribe_root(Arc::new({
            let changes = Arc::clone(&changes);
            let generation = Arc::clone(&generation);
            move |event| {
                generation.fetch_add(1, Ordering::Relaxed);
                if event.triggered_by != EventTriggerKind::Import {
                    return;
                }
                let mut changed = locked(&changes);
                for e in &event.events {
                    // The entry's name is the first key under the root.
                    let name = e.path.iter().find_map(|(_, index)| match index {
                        loro::Index::Key(k) if k.as_str() != ROOT => Some(k.to_string()),
                        _ => None,
                    });
                    match name {
                        Some(name) => {
                            changed.insert(name);
                        }
                        // The root itself: entries added or removed.
                        None => {
                            if let loro::event::Diff::Map(map) = &e.diff {
                                changed.extend(map.updated.keys().map(ToString::to_string));
                            }
                        }
                    }
                }
            }
        }));
        let book = Arc::new(Self {
            dir: dir.to_path_buf(),
            file: file.to_path_buf(),
            doc,
            changes,
            generation,
            seeding: AtomicBool::new(!syncs || !fresh),
            moved: Mutex::new(read_moved(dir)),
            hashed: Mutex::new(std::collections::HashMap::new()),
            _local: local,
            _keep: keep,
            _root: root,
        });
        tracing::info!(
            rig.book = %book.file.display(),
            rig.book_fresh = fresh,
            "rig book: open"
        );
        book
    }

    /// The library directory this book stands for.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The `.session` file it is kept in.
    #[must_use]
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// The doc itself — what sync shares between devices.
    #[must_use]
    pub fn doc(&self) -> &LoroDoc {
        &self.doc
    }

    fn root(&self) -> LoroMap {
        self.doc.get_map(ROOT)
    }

    /// Read `name` as a `T`: from the doc, else (once) from the file it
    /// was before the book, moved in.
    pub fn read<T: for<'a> Facet<'a>>(&self, name: &str) -> BookRead<T> {
        if let Some(value) = codec::get(&self.root(), name) {
            return match from_json::<T>(&value) {
                Ok(v) => BookRead::Ok(v),
                Err(e) => BookRead::Bad(e),
            };
        }
        if locked(&self.moved).contains(name) {
            return BookRead::Missing;
        }
        let path = self.dir.join(name);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return BookRead::Missing;
        };
        match facet_styx::from_str::<T>(&text) {
            Ok(v) => {
                if let Err(e) = self.write(name, &v) {
                    tracing::warn!(rig.book_entry = name, "rig book: moving in failed: {e}");
                } else {
                    self.note_moved(name);
                    tracing::info!(rig.book_entry = name, "rig book: moved in from its file");
                }
                BookRead::Ok(v)
            }
            Err(e) => BookRead::Bad(e.to_string()),
        }
    }

    /// Write `value` as `name`: only what differs from the doc becomes a
    /// change, so writing the same value twice records nothing.
    ///
    /// # Errors
    ///
    /// The value would not serialise, or the doc refused it.
    pub fn write<T: for<'a> Facet<'a>>(&self, name: &str, value: &T) -> Result<(), String> {
        let json = to_json(value)?;
        codec::put(&self.root(), name, &json).map_err(|e| e.to_string())?;
        self.doc.commit();
        Ok(())
    }

    /// Whether `name` is in the book.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.root().get(name).is_some()
    }

    /// Take `name` out of the book (and never move its old file back in).
    pub fn remove(&self, name: &str) {
        self.note_moved(name);
        if self.root().delete(name).is_ok() {
            self.doc.commit();
        }
    }

    /// `name`'s file is a backup from now on.
    fn note_moved(&self, name: &str) {
        let mut moved = locked(&self.moved);
        if moved.insert(name.to_owned()) {
            let text: String = moved.iter().map(|n| format!("{n}\n")).collect();
            if let Err(e) = std::fs::write(self.dir.join(MOVED), text) {
                tracing::warn!(
                    rig.book_entry = name,
                    "rig book: the moved-in list was not kept: {e}"
                );
            }
        }
    }

    /// The `.styx` names directly under `prefix` (`"profiles/"`): the
    /// book's, and the old directory's that have not moved in yet (they
    /// move in when read). Sorted, each once.
    #[must_use]
    pub fn names_with_files(&self, prefix: &str) -> Vec<String> {
        let mut names: BTreeSet<String> = self.names(prefix).into_iter().collect();
        let moved = locked(&self.moved);
        if let Ok(entries) = std::fs::read_dir(self.dir.join(prefix)) {
            names.extend(
                entries
                    .filter_map(Result::ok)
                    .filter_map(|e| e.file_name().into_string().ok())
                    .filter(|f| f.ends_with(".styx"))
                    .map(|f| format!("{prefix}{f}"))
                    .filter(|n| !moved.contains(n) && !is_device_local(n)),
            );
        }
        names.into_iter().collect()
    }

    /// The names under `prefix` (`"profiles/"`), sorted.
    #[must_use]
    pub fn names(&self, prefix: &str) -> Vec<String> {
        let mut names: Vec<String> = self
            .root()
            .keys()
            .map(|k| k.to_string())
            .filter(|k| k.starts_with(prefix))
            .collect();
        names.sort();
        names
    }

    /// Record what the asset at `rel` (a library path: `models/x.nam`) is,
    /// by content: its BLAKE3 hash and size, in the book's asset index.
    /// A file not on this device is left as the index has it; one that has
    /// not changed since it was last hashed is not hashed again.
    pub fn note_asset(&self, rel: &str) {
        let path = self.dir.join(rel);
        let Ok(meta) = std::fs::metadata(&path) else {
            return;
        };
        let stamp = (meta.modified().ok(), meta.len());
        if locked(&self.hashed).get(rel) == Some(&stamp) {
            return;
        }
        let hash = match std::fs::File::open(&path).and_then(|mut f| {
            let mut hasher = blake3::Hasher::new();
            std::io::copy(&mut f, &mut hasher)?;
            Ok(hasher.finalize())
        }) {
            Ok(h) => h.to_hex().to_string(),
            Err(e) => {
                tracing::warn!(rig.asset = rel, "rig book: an asset could not be read: {e}");
                return;
            }
        };
        let entry = serde_json::json!({ "hash": hash, "size": meta.len() });
        // Mergeable: two devices that made the index apart hold one index.
        let index = self.root().ensure_mergeable_map(ASSETS);
        match index.and_then(|index| codec::put(&index, rel, &entry)) {
            Ok(()) => {
                self.doc.commit();
                locked(&self.hashed).insert(rel.to_owned(), stamp);
            }
            Err(e) => tracing::warn!(rig.asset = rel, "rig book: the asset index refused it: {e}"),
        }
    }

    /// What the book says the asset at `rel` is.
    #[must_use]
    pub fn asset(&self, rel: &str) -> Option<AssetRef> {
        let index = codec::get(&self.root(), ASSETS)?;
        let entry = index.get(rel)?;
        Some(AssetRef {
            hash: entry.get("hash")?.as_str()?.to_owned(),
            size: entry.get("size")?.as_u64()?,
        })
    }

    /// The library path an asset with `hash` is known by, if any.
    #[must_use]
    pub fn asset_named(&self, hash: &str) -> Option<String> {
        let index = codec::get(&self.root(), ASSETS)?;
        index
            .as_object()?
            .iter()
            .find_map(|(rel, entry)| (entry.get("hash")?.as_str()? == hash).then(|| rel.clone()))
    }

    /// The names another device changed since the last call, once each.
    #[must_use]
    pub fn take_changes(&self) -> Vec<String> {
        std::mem::take(&mut *locked(&self.changes))
            .into_iter()
            .collect()
    }

    /// Moves on every change: a cache keyed on it is stale when it moved.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// Whether the shipped defaults may be written into this book: it has a
    /// history of its own here, or no account syncs it, or its first sync
    /// has landed ([`Self::allow_seeding`]).
    #[must_use]
    pub fn may_seed(&self) -> bool {
        self.seeding.load(Ordering::Acquire)
    }

    /// The account's library has arrived: what it lacks may be seeded now.
    pub fn allow_seeding(&self) {
        self.seeding.store(true, Ordering::Release);
    }
}

/// The device-local list of names moved in from the directory.
const MOVED: &str = ".book-moved-in";

fn read_moved(dir: &Path) -> BTreeSet<String> {
    std::fs::read_to_string(dir.join(MOVED))
        .map(|t| {
            t.lines()
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// A `T` as the codec's JSON.
fn to_json<T: for<'a> Facet<'a>>(value: &T) -> Result<serde_json::Value, String> {
    let text = facet_json::to_string(value).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// The codec's JSON as a `T`.
fn from_json<T: for<'a> Facet<'a>>(value: &serde_json::Value) -> Result<T, String> {
    let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
    facet_json::from_str::<T>(&text).map_err(|e| e.to_string())
}

/// Whether `name` is this device's own and never goes in a book: where
/// the rig is (`last-state.styx`, a perform rig's `cursor.styx` — moving to
/// song four on the desk must not move the iPad), and the library's own
/// bookkeeping (`.seeded-*`, `*.migrated`, `migrate/`, the `frozen/` renders).
#[must_use]
pub fn is_device_local(name: &str) -> bool {
    let file = name.rsplit('/').next().unwrap_or(name);
    matches!(file, "last-state.styx" | "cursor.styx")
        || file.starts_with('.')
        || file.ends_with(".migrated")
        || name.starts_with("migrate/")
        || name.starts_with("frozen/")
}

// ── Books in this process: whether, and which ──────────────────────────────

/// Set once, by the app that plays the rigs: books are on, and whether an
/// account's sync carries them ([`RigBook::may_seed`]).
static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// What makes a book for a library directory when something other than
/// the book keeps its doc (the sync service: [`set_provider`]).
pub type Provider = Arc<dyn Fn(&Path) -> Option<Arc<RigBook>> + Send + Sync>;

static PROVIDER: Mutex<Option<Provider>> = Mutex::new(None);

/// Let `provider` make the books from now on ([`adopt`] asks it first): the
/// sync service, once signed in, so a rig's book is the doc it syncs. A
/// directory it gives no book for gets one of its own, as before.
pub fn set_provider(provider: Provider) {
    *locked(&PROVIDER) = Some(provider);
}

/// Turn books on for this process: every rig library [`adopt`]ed from now
/// on is a book. The app calls it once, before its rigs start, and not for
/// a run that must leave no mark (design, ephemeral). `syncs`: an account
/// is signed in, so its sync carries the books.
pub fn enable(syncs: bool) {
    let _ = ENABLED.set(syncs);
}

/// The book for the rig library at `dir` — `<dir>.session` beside it,
/// opened and [`register`]ed the first time — when books are on
/// ([`enable`]); `None` leaves the library a directory.
pub fn adopt(dir: &Path) -> Option<Arc<RigBook>> {
    let syncs = *ENABLED.get()?;
    if let Some(book) = for_dir(dir) {
        return Some(book);
    }
    // Signed in, the sync service hands out the book: the doc its driver
    // keeps and syncs, persisted to the same `<dir>.session`.
    let provider = locked(&PROVIDER).clone();
    if let Some(book) = provider.and_then(|p| p(dir)) {
        register(Arc::clone(&book));
        return Some(book);
    }
    let file = dir.with_extension("session");
    match RigBook::open(&file, dir, syncs) {
        Ok(book) => {
            register(Arc::clone(&book));
            Some(book)
        }
        Err(e) => {
            tracing::error!(rig.book = %file.display(), "rig book: would not open — the library stays files: {e}");
            None
        }
    }
}

// ── The books in this process, by the directory each stands for ────────────

static BOOKS: Mutex<Vec<Arc<RigBook>>> = Mutex::new(Vec::new());

/// The book whose directory holds `path` most closely.
fn deepest<'a>(books: &'a [Arc<RigBook>], path: &Path) -> Option<&'a Arc<RigBook>> {
    books
        .iter()
        .filter(|b| path.starts_with(&b.dir))
        .max_by_key(|b| b.dir.components().count())
}

/// Put `book` in place of its directory for every read and write in this
/// process. A second book for the same directory replaces the first.
pub fn register(book: Arc<RigBook>) {
    let mut books = locked(&BOOKS);
    books.retain(|b| b.dir != book.dir);
    books.push(book);
}

/// The book standing for `dir`, if there is one.
#[must_use]
pub fn for_dir(dir: &Path) -> Option<Arc<RigBook>> {
    locked(&BOOKS).iter().find(|b| b.dir == dir).cloned()
}

/// The book whose directory `dir` is (or is under), and `dir`'s prefix in
/// it (`"profiles/"`, or `""` for the book's own directory).
#[must_use]
pub fn for_tree(dir: &Path) -> Option<(Arc<RigBook>, String)> {
    let books = locked(&BOOKS);
    // The deepest book wins: a bass library can sit inside the guitar's.
    deepest(&books, dir).and_then(|b| {
        let rel = dir.strip_prefix(&b.dir).ok()?;
        let mut prefix = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if !prefix.is_empty() {
            prefix.push('/');
        }
        Some((Arc::clone(b), prefix))
    })
}

/// The book whose directory `path` is under, and the entry's name there
/// (`profiles/worship.styx`, `/`-separated) — none for a name this device
/// keeps to itself ([`is_device_local`]), which stays a file.
#[must_use]
pub fn for_path(path: &Path) -> Option<(Arc<RigBook>, String)> {
    let books = locked(&BOOKS);
    deepest(&books, path).and_then(|b| {
        let rel = path.strip_prefix(&b.dir).ok()?;
        let name = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        (!name.is_empty() && !is_device_local(&name)).then(|| (Arc::clone(b), name))
    })
}
