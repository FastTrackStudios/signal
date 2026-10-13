//! A [`SurfaceStore`] over a folder: every `.frame` markup file (the older
//! `.fm` too) and `.frame` bundle (a zip) under the root is a surface, `*.catalog.json` files are the engine
//! catalogs they were designed for.
//!
//! ```text
//! root/
//!   comp/07-vari-mu.frame         surface `comp/07-vari-mu`
//!   comp/07-vari-mu.frame.assets/ its textures and fonts (see `document-io`)
//!   comp/07-vari-mu.meta.json     optional: author, description, tags, license, forked_from, version
//!   comp/07-vari-mu.png           optional: the preview (else rendered by the previewer, if any)
//!   comp/comp.catalog.json        a catalog (served by `catalogs()`, tags surfaces that fit it)
//! ```
//!
//! The index lives in memory, built by [`FsLibrary::rescan`]. [`FsLibrary::watch`]
//! rescans on a short interval, so an edit on disk (an agent's MCP autosave,
//! a person's editor) becomes a new version pushed to every subscriber. A
//! poll rather than OS notifications: it needs no platform backend, sees
//! atomic write-then-rename saves as one change, and a stat of a few
//! hundred files every 200 ms costs nothing. A markup surface's stamp
//! covers its `.frame.assets/` sidecar too, so replacing a texture is an edit
//! like any other. A file keeps its path when it is rewritten, so a library
//! of `.fm` files stays `.fm` while its surfaces stay markup; republishing in
//! a different format rewrites the same `.frame` path (a `.fm` moves to
//! `.frame` when it becomes a bundle). A file's content hash (markup and asset ids) decides
//! whether it changed, so a publish (which writes the file) doesn't come
//! back as a second update.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, SystemTime};

use architect::dispatch::CurrentThreadDispatcher;
use architect::{HasDispatcher, PubSub};
use document_proto::Scene;
pub use library_proto::*;
use serde::{Deserialize, Serialize};
use surface_proto::Catalog;

/// Renders a scene's thumbnail as PNG bytes (`None` = can't).
pub type Previewer = Arc<dyn Fn(&Scene) -> Option<Vec<u8>> + Send + Sync>;

/// Past versions kept per surface for `get(id, Some(version))`.
const HISTORY: usize = 16;
/// How often [`FsLibrary::watch`] looks at the folder.
pub const WATCH_INTERVAL: Duration = Duration::from_millis(200);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

type Stamp = Option<(SystemTime, u64)>;

fn stamp(path: &Path) -> Stamp {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

fn hash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// `<id>.meta.json`: what a file can't say about itself.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Sidecar {
    name: String,
    author: String,
    description: String,
    tags: Vec<String>,
    license: String,
    forked_from: Option<String>,
    version: u64,
}

impl Sidecar {
    fn is_empty(&self) -> bool {
        self.author.is_empty()
            && self.description.is_empty()
            && self.tags.is_empty()
            && self.license.is_empty()
            && self.forked_from.is_none()
    }
}

struct Entry {
    path: PathBuf,
    /// [`document_io::fingerprint`]: the file and its assets sidecar.
    stamp: Option<u64>,
    hash: u64,
    meta: SurfaceMeta,
    content: Content,
    /// Older versions, newest last.
    history: VecDeque<(u64, Content)>,
}

impl Entry {
    fn surface(&self) -> Surface {
        Surface {
            meta: self.meta.clone(),
            content: self.content.clone(),
        }
    }

    fn update(&self) -> SurfaceUpdate {
        SurfaceUpdate {
            id: self.meta.id.clone(),
            version: self.meta.version,
            surface: Some(self.surface()),
        }
    }
}

#[derive(Default)]
struct State {
    entries: BTreeMap<SurfaceId, Entry>,
    catalogs: BTreeMap<PathBuf, (Stamp, Catalog)>,
}

struct Inner {
    root: PathBuf,
    state: Mutex<State>,
    hubs: Mutex<HashMap<SurfaceId, PubSub<SurfaceUpdate>>>,
    changes: PubSub<IndexChange>,
    previewer: Option<Previewer>,
}

/// A folder of surfaces as a [`SurfaceStore`].
#[derive(Clone, HasDispatcher)]
#[dispatch(CurrentThreadDispatcher)]
pub struct FsLibrary {
    inner: Arc<Inner>,
}

/// What one scan found, published after the index lock is released.
#[derive(Default)]
struct Changes {
    updates: Vec<SurfaceUpdate>,
    index: Vec<IndexChange>,
}

impl FsLibrary {
    /// A library over `root` (scanned now).
    ///
    /// # Errors
    /// When `root` isn't a readable directory.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, LibraryError> {
        Self::open_with(root, None)
    }

    /// [`Self::open`] with a previewer that renders thumbnails of surfaces
    /// without a `.png` next to them.
    ///
    /// # Errors
    /// When `root` isn't a readable directory.
    pub fn open_with(
        root: impl Into<PathBuf>,
        previewer: Option<Previewer>,
    ) -> Result<Self, LibraryError> {
        let root = root.into();
        if !root.is_dir() {
            return Err(LibraryError::Io(format!(
                "{} is not a directory",
                root.display()
            )));
        }
        let lib = Self {
            inner: Arc::new(Inner {
                root,
                state: Mutex::new(State::default()),
                hubs: Mutex::new(HashMap::new()),
                changes: PubSub::sliding(256),
                previewer,
            }),
        };
        lib.rescan();
        Ok(lib)
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.inner.root
    }

    /// Where a surface's file is (for downloads).
    #[must_use]
    pub fn path_of(&self, id: &str) -> Option<PathBuf> {
        lock(&self.inner.state)
            .entries
            .get(id)
            .map(|e| e.path.clone())
    }

    /// An asset some surface carries (for downloads). Ids are content
    /// hashes, so any surface's copy is the asset.
    #[must_use]
    pub fn asset(&self, id: &str) -> Option<document_proto::Asset> {
        lock(&self.inner.state)
            .entries
            .values()
            .find_map(|e| e.content.assets.iter().find(|a| a.id.0 == id).cloned())
    }

    /// Look at the folder once: new, changed and removed files become
    /// versions and updates. Returns how many surfaces changed.
    pub fn rescan(&self) -> usize {
        let changes = self.scan();
        let n = changes.index.len();
        self.emit(changes);
        n
    }

    /// Rescan every [`WATCH_INTERVAL`] on a background thread until the
    /// returned handle (or the library) is dropped.
    #[must_use]
    pub fn watch(&self) -> Watcher {
        self.watch_every(WATCH_INTERVAL)
    }

    /// [`Self::watch`] at a custom interval.
    #[must_use]
    pub fn watch_every(&self, interval: Duration) -> Watcher {
        let weak: Weak<Inner> = Arc::downgrade(&self.inner);
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("library-fs-watch".into())
            .spawn(move || {
                while !flag.load(Ordering::Relaxed) {
                    std::thread::sleep(interval);
                    let Some(inner) = weak.upgrade() else { return };
                    FsLibrary { inner }.rescan();
                }
            })
            .ok();
        Watcher { stop, thread }
    }

    fn emit(&self, changes: Changes) {
        let hubs = lock(&self.inner.hubs);
        for u in changes.updates {
            if let Some(hub) = hubs.get(&u.id) {
                hub.publish(u);
            }
        }
        drop(hubs);
        for c in changes.index {
            self.inner.changes.publish(c);
        }
    }

    fn scan(&self) -> Changes {
        let mut files = Vec::new();
        walk(&self.inner.root, 0, &mut files);
        let mut state = lock(&self.inner.state);
        let catalogs_changed = self.scan_catalogs(&mut state, &files);
        let engines: Vec<(String, Signature)> = state
            .catalogs
            .values()
            .map(|(_, c)| (c.id.clone(), Signature::of_catalog(c)))
            .collect();

        // Surface files by id; a `.frame` wins over a legacy `.fm` of the
        // same name.
        let mut found: BTreeMap<SurfaceId, PathBuf> = BTreeMap::new();
        for path in &files {
            if format_of(path).is_none() {
                continue;
            }
            let Some(id) = self.id_of(path) else { continue };
            if found
                .get(&id)
                .is_none_or(|have| is_legacy(have) && !is_legacy(path))
            {
                found.insert(id, path.clone());
            }
        }

        let mut out = Changes::default();
        let gone: Vec<SurfaceId> = state
            .entries
            .keys()
            .filter(|id| !found.contains_key(*id))
            .cloned()
            .collect();
        for id in gone {
            state.entries.remove(&id);
            out.updates.push(SurfaceUpdate {
                id: id.clone(),
                version: 0,
                surface: None,
            });
            out.index.push(IndexChange { id, meta: None });
        }
        for (id, path) in found {
            let st = document_io::fingerprint(&path);
            let entry = state.entries.get_mut(&id);
            if let Some(e) = &entry
                && e.path == path
                && e.stamp == st
                && !catalogs_changed
            {
                continue;
            }
            let format = format_of(&path).unwrap_or(Format::Markup);
            // Unreadable (or a sidecar mid-write): the next scan retries.
            let Some(content) = read_content(&path, format) else {
                continue;
            };
            let h = content_hash(&content);
            if let Some(e) = entry
                && e.path == path
                && e.hash == h
            {
                e.stamp = st;
                if catalogs_changed {
                    e.meta.tags = tags(&id, &read_sidecar(&path).tags, &e.meta.fits, &engines);
                }
                continue;
            }
            let old = state.entries.remove(&id);
            let entry = self.build(&id, path, st, h, content, old, None, &engines);
            out.updates.push(entry.update());
            out.index.push(IndexChange {
                id: id.clone(),
                meta: Some(entry.meta.clone()),
            });
            state.entries.insert(id, entry);
        }
        out
    }

    /// Reload changed `*.catalog.json` files; true when any changed.
    fn scan_catalogs(&self, state: &mut State, files: &[PathBuf]) -> bool {
        let paths: BTreeSet<&PathBuf> = files.iter().filter(|p| is_catalog(p)).collect();
        let before = state.catalogs.len();
        state.catalogs.retain(|p, _| paths.contains(p));
        let mut changed = state.catalogs.len() != before;
        for path in paths {
            let st = stamp(path);
            if state.catalogs.get(path).is_some_and(|(s, _)| *s == st) {
                continue;
            }
            match std::fs::read_to_string(path)
                .map_err(|e| e.to_string())
                .and_then(|t| serde_json::from_str::<Catalog>(&t).map_err(|e| e.to_string()))
            {
                Ok(c) => {
                    state.catalogs.insert(path.clone(), (st, c));
                    changed = true;
                }
                Err(e) => eprintln!("library-fs: skipping catalog {}: {e}", path.display()),
            }
        }
        changed
    }

    /// A fresh index entry for `content` at `path`, carrying over what the
    /// previous version knew when the new content doesn't decode.
    #[allow(clippy::too_many_arguments)]
    fn build(
        &self,
        id: &str,
        path: PathBuf,
        stamp: Option<u64>,
        hash: u64,
        content: Content,
        old: Option<Entry>,
        preview: Option<Vec<u8>>,
        engines: &[(String, Signature)],
    ) -> Entry {
        let side = read_sidecar(&path);
        let decoded = content.decode();
        let (fits, name, preview) = match (&decoded, &old) {
            (Ok(scene), _) => {
                let name = if side.name.is_empty() {
                    content
                        .title()
                        .or_else(|| scene_name(scene))
                        .unwrap_or_else(|| stem(id))
                } else {
                    side.name.clone()
                };
                let preview = preview
                    .filter(|p| !p.is_empty())
                    .or_else(|| std::fs::read(path.with_extension("png")).ok())
                    .or_else(|| {
                        self.inner
                            .previewer
                            .as_ref()
                            .and_then(|render| render(scene))
                    })
                    .unwrap_or_default();
                (Signature::of_scene(scene), name, preview)
            }
            // Broken on disk (mid-edit): keep what the last good version said;
            // the content itself goes out as is and hosts reject it.
            (Err(_), Some(o)) => (
                o.meta.fits.clone(),
                o.meta.name.clone(),
                o.meta.preview_png.clone(),
            ),
            (Err(_), None) => (Signature::default(), stem(id), Vec::new()),
        };
        let (version, mut history) = match old {
            Some(o) => {
                let mut h = o.history;
                h.push_back((o.meta.version, o.content));
                while h.len() > HISTORY {
                    h.pop_front();
                }
                (o.meta.version + 1, h)
            }
            None => (side.version.max(1), VecDeque::new()),
        };
        history.retain(|(v, _)| *v < version);
        let meta = SurfaceMeta {
            id: id.to_owned(),
            name,
            author: side.author.clone(),
            description: side.description.clone(),
            tags: tags(id, &side.tags, &fits, engines),
            fits,
            preview_png: preview,
            version,
            license: side.license.clone(),
            forked_from: side.forked_from.clone(),
        };
        Entry {
            path,
            stamp,
            hash,
            meta,
            content,
            history,
        }
    }

    /// `path`'s id: relative to the root, no extension, `/`-separated.
    fn id_of(&self, path: &Path) -> Option<SurfaceId> {
        let rel = path.strip_prefix(&self.inner.root).ok()?.with_extension("");
        let parts: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        (!parts.is_empty()).then(|| parts.join("/"))
    }

    fn fresh_id(state: &State, name: &str) -> SurfaceId {
        let base: String = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        let base = if base.is_empty() {
            "surface".to_owned()
        } else {
            base
        };
        if !state.entries.contains_key(&base) {
            return base;
        }
        (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|id| !state.entries.contains_key(id))
            .unwrap_or(base)
    }
}

/// Stops a [`FsLibrary::watch`] thread when dropped.
pub struct Watcher {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn is_catalog(p: &Path) -> bool {
    p.file_name()
        .is_some_and(|n| n.to_string_lossy().ends_with(".catalog.json"))
}

/// A surface file's format: a `.frame` is a bundle when its content is a
/// zip, markup otherwise; a `.fm` is markup.
fn format_of(path: &Path) -> Option<Format> {
    document_io::is_frame_document(path).then(|| match document_io::Format::of(path) {
        Some(document_io::Format::Bundle) => Format::Frame,
        _ => Format::Markup,
    })
}

/// Whether `path` is a legacy `.fm` file.
fn is_legacy(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some(document_io::LEGACY_EXTENSION)
}

/// A surface file's content: a bundle's bytes, or markup text plus the
/// assets in its sidecar.
fn read_content(path: &Path, format: Format) -> Option<Content> {
    match format {
        Format::Frame => std::fs::read(path).ok().map(|bytes| Content {
            format,
            bytes,
            assets: Vec::new(),
        }),
        Format::Markup => match document_io::read_markup(path) {
            Ok(m) => Some(Content::markup_with(m.text, m.assets.into_values())),
            Err(e) => {
                eprintln!("library-fs: skipping {e}");
                None
            }
        },
    }
}

/// What decides whether a surface changed: its bytes and its assets' ids
/// (content hashes themselves).
fn content_hash(content: &Content) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    hash(&content.bytes).hash(&mut h);
    for a in &content.assets {
        a.id.hash(&mut h);
        a.mime.hash(&mut h);
    }
    h.finish()
}

/// Every file under `dir` (hidden dirs, `target`, `node_modules` and
/// markup assets sidecars skipped).
fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 6 {
        return;
    }
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for p in paths {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            if name != "target" && name != "node_modules" && !document_io::is_assets_dir(&p) {
                walk(&p, depth + 1, out);
            }
        } else {
            out.push(p);
        }
    }
}

fn sidecar_path(path: &Path) -> PathBuf {
    path.with_extension("meta.json")
}

fn read_sidecar(path: &Path) -> Sidecar {
    std::fs::read_to_string(sidecar_path(path))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Write-then-rename, so the watcher (or any reader) never sees half a file.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    // A dot-less tmp name would be scanned as a surface; `.tmp` isn't one.
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// The id's folder names, the sidecar's tags, and every known catalog the
/// surface fits.
fn tags(
    id: &str,
    extra: &[String],
    fits: &Signature,
    engines: &[(String, Signature)],
) -> Vec<String> {
    let mut out: Vec<String> = id.split('/').rev().skip(1).map(str::to_owned).collect();
    out.reverse();
    for t in extra {
        if !out.contains(t) {
            out.push(t.clone());
        }
    }
    if !fits.is_empty() {
        for (cid, sig) in engines {
            if fits.fits(sig) && !out.contains(cid) {
                out.push(cid.clone());
            }
        }
    }
    out
}

fn stem(id: &str) -> String {
    id.rsplit('/').next().unwrap_or(id).to_owned()
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('/')
        && id.split('/').all(|p| {
            !p.is_empty()
                && !p.starts_with('.')
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_ ".contains(c))
        })
}

impl SurfaceStore for FsLibrary {
    fn list(&self, query: ListQuery) -> Vec<SurfaceMeta> {
        lock(&self.inner.state)
            .entries
            .values()
            .filter(|e| query.matches(&e.meta))
            .map(|e| {
                let mut meta = e.meta.clone();
                if !query.previews {
                    meta.preview_png = Vec::new();
                }
                meta
            })
            .collect()
    }

    fn preview(&self, id: String) -> Vec<u8> {
        lock(&self.inner.state)
            .entries
            .get(&id)
            .map(|e| e.meta.preview_png.clone())
            .unwrap_or_default()
    }

    fn get(&self, id: String, version: Option<u64>) -> Result<Surface, LibraryError> {
        let state = lock(&self.inner.state);
        let e = state
            .entries
            .get(&id)
            .ok_or_else(|| LibraryError::NotFound(id.clone()))?;
        match version {
            None => Ok(e.surface()),
            Some(v) if v == e.meta.version => Ok(e.surface()),
            Some(v) => e
                .history
                .iter()
                .find(|(hv, _)| *hv == v)
                .map(|(_, content)| Surface {
                    meta: SurfaceMeta {
                        version: v,
                        ..e.meta.clone()
                    },
                    content: content.clone(),
                })
                .ok_or_else(|| LibraryError::NotFound(format!("{id}@{v}"))),
        }
    }

    fn publish(&self, content: Content, meta: PublishMeta) -> Result<Published, LibraryError> {
        content.decode().map_err(LibraryError::BadContent)?;
        let mut state = lock(&self.inner.state);
        let id = if meta.id.is_empty() {
            Self::fresh_id(&state, &meta.name)
        } else if valid_id(&meta.id) {
            meta.id.clone()
        } else {
            return Err(LibraryError::BadId(meta.id));
        };
        let old = state.entries.remove(&id);
        // Rewriting a surface keeps its file, in whatever format: a legacy
        // `.fm` stays `.fm` while it is markup (no duplicate beside it) and
        // moves to `.frame` when it becomes a bundle.
        let path = match &old {
            Some(o) if !is_legacy(&o.path) || content.format == Format::Markup => o.path.clone(),
            _ => self
                .inner
                .root
                .join(format!("{id}.{}", content.format.extension())),
        };
        let io = |e: std::io::Error| LibraryError::Io(format!("{}: {e}", path.display()));
        let result = (|| {
            match content.format {
                Format::Markup => {
                    let text = std::str::from_utf8(&content.bytes)
                        .map_err(|e| LibraryError::BadContent(e.to_string()))?;
                    document_io::write_markup(&path, text, &content.assets)
                        .map_err(|e| LibraryError::Io(e.to_string()))?;
                }
                Format::Frame => {
                    write_atomic(&path, &content.bytes).map_err(io)?;
                    // A markup file's assets sidecar doesn't belong to a bundle.
                    document_io::prune_assets(&path)
                        .map_err(|e| LibraryError::Io(e.to_string()))?;
                }
            }
            let version = old.as_ref().map_or(1, |o| o.meta.version + 1);
            let side = Sidecar {
                name: meta.name.clone(),
                author: meta.author.clone(),
                description: meta.description.clone(),
                tags: meta.tags.clone(),
                license: meta.license.clone(),
                forked_from: meta.forked_from.clone(),
                version,
            };
            if !side.is_empty() {
                let json = serde_json::to_vec_pretty(&side)
                    .map_err(|e| LibraryError::Io(e.to_string()))?;
                write_atomic(&sidecar_path(&path), &json).map_err(io)?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            if let Some(o) = old {
                state.entries.insert(id, o);
            }
            return Err(e);
        }
        // A format change leaves the other file (and its sidecar) behind:
        // remove it.
        if let Some(o) = &old
            && o.path != path
        {
            let _ = document_io::remove_surface(&o.path);
        }
        let engines: Vec<(String, Signature)> = state
            .catalogs
            .values()
            .map(|(_, c)| (c.id.clone(), Signature::of_catalog(c)))
            .collect();
        let (st, h) = (document_io::fingerprint(&path), content_hash(&content));
        let mut entry = self.build(
            &id,
            path,
            st,
            h,
            content,
            old,
            Some(meta.preview_png),
            &engines,
        );
        if !meta.name.is_empty() {
            entry.meta.name = meta.name;
        }
        let published = Published {
            id: id.clone(),
            version: entry.meta.version,
        };
        let changes = Changes {
            updates: vec![entry.update()],
            index: vec![IndexChange {
                id: id.clone(),
                meta: Some(entry.meta.clone()),
            }],
        };
        state.entries.insert(id, entry);
        drop(state);
        self.emit(changes);
        Ok(published)
    }

    fn catalogs(&self) -> Vec<Catalog> {
        lock(&self.inner.state)
            .catalogs
            .values()
            .map(|(_, c)| c.clone())
            .collect()
    }
}

impl SurfaceStoreStreamSource for FsLibrary {
    fn updates_attach(&self, id: String, sink: architect::EventSink<SurfaceUpdate>) {
        let hub = lock(&self.inner.hubs)
            .entry(id.clone())
            .or_insert_with(|| PubSub::sliding(8))
            .clone();
        // Buffer from now, snapshot, then open the tap: nothing is lost
        // between the snapshot and the first live update.
        let pending = hub.begin_attach(sink);
        let intro = lock(&self.inner.state).entries.get(&id).map_or_else(
            || SurfaceUpdate {
                id: id.clone(),
                version: 0,
                surface: None,
            },
            Entry::update,
        );
        hub.complete_attach(pending, Some(intro));
    }

    fn changes_hub(&self) -> &PubSub<IndexChange> {
        &self.inner.changes
    }
}

impl architect::Services for FsLibrary {
    fn layers() -> impl architect::Layer<Self> {
        architect::layers![
            library_proto::store::Service,
            library_proto::store::StreamService
        ]
    }
}

#[cfg(test)]
mod tests;
