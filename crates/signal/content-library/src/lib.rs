//! Content host — serves a library root over vox: every file a rig plays
//! from ([`signal_content_proto::kind_of`]), its hash, and the bundles the
//! files make.
//!
//! The `Proxy/` subtree is the `proxy` variant of the same ids, so a phone
//! asks for the small twin of the file a desktop plays in full. Hashes are
//! read from `<file>.sha256` sidecars (`<size>:<hex>`) shipped beside the
//! files; the ones this host works out go to its own cache
//! (`~/.cache/signal/content-hashes`), never into the folders it serves,
//! so a restart does not re-read a drive. The distribution set (proxies, and anything under
//! 64 MiB — engines, presets, models, config) is hashed in the background,
//! the studio's multi-terabyte lossless trees only when someone builds
//! their sidecars.
//!
//! Bundles: a **drum kit** is a `.signalpreset` with everything it plays —
//! the engines it names, the packs they name — and its library's shared
//! mixes, modules and similarity space.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use architect::{Layer, Services, layers};
use signal_content_proto::content::ContentLibrary;
use signal_content_proto::{BundleInfo, ContentChunk, ContentError, ContentInfo, kind_of};
use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _};

/// Streamed chunk size — matches the pack and media lanes.
const CHUNK_BYTES: usize = 256 * 1024;

/// Files at most this big are hashed eagerly whatever their variant.
const EAGER_HASH_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug)]
struct Entry {
    info: ContentInfo,
    path: PathBuf,
}

/// The content host. Cheap to clone (state shared).
#[derive(Clone, architect::HasDispatcher)]
pub struct ContentLibraryBackend {
    entries: Arc<Mutex<Vec<Entry>>>,
}

impl ContentLibraryBackend {
    /// Scan `root` and start the background hasher.
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self::with_roots(vec![(String::new(), root.into())])
    }

    /// Scan several roots, each under its own id prefix (`""` for the
    /// sample libraries, `Signal/guitar` for the guitar rig's library, …),
    /// so one host serves everything a device might want of it.
    pub fn with_roots(roots: Vec<(String, PathBuf)>) -> Self {
        let mut entries = Vec::new();
        for (prefix, root) in &roots {
            let found = scan(root, prefix);
            tracing::info!(root = %root.display(), prefix = %prefix, files = found.len(), "content library scanned");
            entries.extend(found);
        }
        entries.sort_by(|a, b| (&a.info.id, &a.info.variant).cmp(&(&b.info.id, &b.info.variant)));
        let backend = Self {
            entries: Arc::new(Mutex::new(entries)),
        };
        backend.spawn_hasher();
        backend
    }

    /// The service router — mount on any vox transport.
    #[must_use]
    pub fn router(&self) -> architect::LayerRouter {
        self.clone().into_router()
    }

    /// Hash everything not hashed yet, now (tests; a host that must answer
    /// with hashes before it serves).
    pub fn hash_all(&self) {
        hash_pending(&self.entries, |_| true);
    }

    fn entries(&self) -> Vec<Entry> {
        self.entries.lock().map(|e| e.clone()).unwrap_or_default()
    }

    fn find(&self, id: &str, variant: &str) -> Option<Entry> {
        self.entries
            .lock()
            .ok()?
            .iter()
            .find(|e| e.info.id == id && e.info.variant == variant)
            .cloned()
    }

    fn spawn_hasher(&self) {
        let entries = self.entries.clone();
        let _ = std::thread::Builder::new()
            .name("content-hasher".into())
            .spawn(move || {
                hash_pending(&entries, |e| {
                    e.info.variant == "proxy" || e.info.size_bytes <= EAGER_HASH_BYTES
                });
            });
    }

    /// Every bundle the host can give: drum kits, and each keys pack.
    fn all_bundles(&self) -> Vec<(BundleInfo, Vec<ContentInfo>)> {
        let mut out = self.kits();
        out.extend(self.keys_packs());
        out
    }

    /// Every keys pack (`Keys/…/*.signalpack`), a bundle of itself — one
    /// instrument a keys rig plays.
    fn keys_packs(&self) -> Vec<(BundleInfo, Vec<ContentInfo>)> {
        self.entries()
            .into_iter()
            .filter(|e| e.info.kind == "pack" && e.info.id.starts_with("Keys/"))
            .map(|e| {
                let (library, file) = e.info.id.rsplit_once('/').unwrap_or(("", &e.info.id));
                let info = BundleInfo {
                    id: format!("keys-pack:{}", e.info.id),
                    kind: "keys-pack".into(),
                    name: file.trim_end_matches(".signalpack").to_string(),
                    library: library.to_string(),
                    variant: e.info.variant.clone(),
                    size_bytes: e.info.size_bytes,
                    files: 1,
                };
                (info, vec![e.info])
            })
            .collect()
    }

    /// Every drum kit: a kit preset in a library with `Engines/` beside it.
    fn kits(&self) -> Vec<(BundleInfo, Vec<ContentInfo>)> {
        let entries = self.entries();
        entries
            .iter()
            .filter(|e| e.info.kind == "kit-preset")
            .filter_map(|preset| {
                let files = kit_files(&entries, preset)?;
                let (library, _) = preset.info.id.rsplit_once("/Presets/")?;
                let name = preset
                    .info
                    .id
                    .rsplit('/')
                    .next()?
                    .trim_end_matches(".signalpreset")
                    .to_string();
                let info = BundleInfo {
                    id: format!("drum-kit:{}", preset.info.id),
                    kind: "drum-kit".into(),
                    name,
                    library: library.to_string(),
                    variant: preset.info.variant.clone(),
                    size_bytes: files.iter().map(|f| f.size_bytes).sum(),
                    files: files.len() as u64,
                };
                Some((info, files))
            })
            .collect()
    }
}

impl ContentLibrary for ContentLibraryBackend {
    fn catalog(&self, prefix: String) -> Vec<ContentInfo> {
        self.entries()
            .into_iter()
            .filter(|e| e.info.id.starts_with(&prefix))
            .map(|e| e.info)
            .collect()
    }

    fn bundles(&self, kind: String) -> Vec<BundleInfo> {
        self.all_bundles()
            .into_iter()
            .map(|(b, _)| b)
            .filter(|b| kind.is_empty() || b.kind == kind)
            .collect()
    }

    fn bundle_files(&self, id: String, variant: String) -> Vec<ContentInfo> {
        self.all_bundles()
            .into_iter()
            .find(|(b, _)| b.id == id && b.variant == variant)
            .map(|(_, f)| f)
            .unwrap_or_default()
    }

    async fn read(
        &self,
        id: String,
        variant: String,
        start: u64,
        tx: vox::Tx<ContentChunk>,
    ) -> Result<(), ContentError> {
        let entry = self.find(&id, &variant).ok_or(ContentError::NotFound)?;
        let size = entry.info.size_bytes;
        if start > size {
            return Err(ContentError::InvalidRange(format!(
                "start {start} > size {size}"
            )));
        }
        let mut file = tokio::fs::File::open(&entry.path)
            .await
            .map_err(|e| ContentError::Io(e.to_string()))?;
        file.seek(std::io::SeekFrom::Start(start))
            .await
            .map_err(|e| ContentError::Io(e.to_string()))?;
        let mut offset = start;
        let mut buf = vec![0u8; CHUNK_BYTES];
        loop {
            let n = file
                .read(&mut buf)
                .await
                .map_err(|e| ContentError::Io(e.to_string()))?;
            if n == 0 {
                return Ok(());
            }
            if tx
                .send(ContentChunk {
                    offset,
                    bytes: buf[..n].to_vec(),
                })
                .await
                .is_err()
            {
                // The receiver went away (a paused download) — not an error.
                return Ok(());
            }
            offset += n as u64;
        }
    }
}

impl Services for ContentLibraryBackend {
    fn layers() -> impl Layer<Self> {
        layers![signal_content_proto::content::Service]
    }
}

// ── scan, hash, bundles ───────────────────────────────────────────────────

/// Every rig file under `root`, its id under `prefix`; `Proxy/…` is the
/// proxy variant of `…`.
fn scan(root: &Path, prefix: &str) -> Vec<Entry> {
    let mut entries = Vec::new();
    for f in walkdir::WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !hidden(e.file_name()))
        .flatten()
    {
        if !f.file_type().is_file() {
            continue;
        }
        let path = f.path();
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        let (variant, id) = match rel.strip_prefix("Proxy/") {
            Some(rest) => ("proxy", rest.to_string()),
            None => ("full", rel),
        };
        let id = if prefix.is_empty() {
            id
        } else {
            format!("{prefix}/{id}")
        };
        let Some(kind) = kind_of(&id) else { continue };
        let meta = f.metadata().ok();
        entries.push(Entry {
            info: ContentInfo {
                id,
                kind: kind.to_string(),
                variant: variant.to_string(),
                size_bytes: meta.as_ref().map_or(0, std::fs::Metadata::len),
                sha256: sidecar(path, meta.as_ref().map_or(0, std::fs::Metadata::len))
                    .unwrap_or_default(),
                modified: meta
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs()),
            },
            path: path.to_path_buf(),
        });
    }
    entries.sort_by(|a, b| (&a.info.id, &a.info.variant).cmp(&(&b.info.id, &b.info.variant)));
    entries
}

/// Dot files and the decode caches beside the libraries.
fn hidden(name: &std::ffi::OsStr) -> bool {
    let n = name.to_string_lossy();
    n.starts_with('.') || n == "_decoded"
}

fn sidecar_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".sha256");
    PathBuf::from(s)
}

/// Where this host keeps the hashes it works out: a cache of its own,
/// never beside the files it serves (a rig's live library, a repo's
/// defaults).
fn cache_path(path: &Path) -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    let key: String = path
        .to_string_lossy()
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c == ':' {
                '%'
            } else {
                c
            }
        })
        .collect();
    base.join("signal")
        .join("content-hashes")
        .join(format!("{key}.sha256"))
}

/// A `<size>:<hex>` hash file's hash, when it is for a file this size.
fn read_hash(file: &Path, size: u64) -> Option<String> {
    let text = std::fs::read_to_string(file).ok()?;
    let (s, hex) = text.trim().split_once(':')?;
    (s.parse::<u64>().ok() == Some(size) && !hex.is_empty()).then(|| hex.to_string())
}

/// The file's hash if one is known: a sidecar shipped beside it (the
/// studio's packs), else this host's cache.
fn sidecar(path: &Path, size: u64) -> Option<String> {
    read_hash(&sidecar_path(path), size).or_else(|| read_hash(&cache_path(path), size))
}

/// Hash the entries `which` picks that have no hash yet, into the cache.
fn hash_pending(entries: &Mutex<Vec<Entry>>, which: impl Fn(&Entry) -> bool) {
    let todo: Vec<Entry> = entries
        .lock()
        .map(|e| {
            e.iter()
                .filter(|e| e.info.sha256.is_empty() && which(e))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    for entry in todo {
        match hash_file(&entry.path) {
            Ok(hex) => {
                let cache = cache_path(&entry.path);
                if let Some(dir) = cache.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::write(cache, format!("{}:{hex}\n", entry.info.size_bytes));
                if let Ok(mut entries) = entries.lock()
                    && let Some(e) = entries.iter_mut().find(|e| e.path == entry.path)
                {
                    e.info.sha256 = hex;
                }
            }
            Err(e) => tracing::warn!(file = %entry.path.display(), "content hash failed: {e}"),
        }
    }
}

fn hash_file(path: &Path) -> std::io::Result<String> {
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// `dir/rel` with `.` and `..` folded away (`a/Presets` + `../Engines/x` →
/// `a/Engines/x`).
fn join_id(dir: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// The quoted values after `key` in a styx text (`engine "../Engines/x"`).
fn refs<'a>(text: &'a str, key: &str) -> Vec<&'a str> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix(key)?.trim_start();
            let rest = rest.strip_prefix('"')?;
            rest.split_once('"').map(|(v, _)| v)
        })
        .collect()
}

/// A kit preset's files, in its variant: itself, its engines, their packs,
/// and its library's mixes, modules and space. `None` when it names an
/// engine the host does not have (not a kit it can give).
fn kit_files(entries: &[Entry], preset: &Entry) -> Option<Vec<ContentInfo>> {
    let variant = &preset.info.variant;
    let find = |id: &str| {
        entries
            .iter()
            .find(|e| e.info.id == id && &e.info.variant == variant)
    };
    let dir = |id: &str| {
        id.rsplit_once('/')
            .map(|(d, _)| d.to_string())
            .unwrap_or_default()
    };
    let (library, _) = preset.info.id.rsplit_once("/Presets/")?;
    let text = std::fs::read_to_string(&preset.path).ok()?;
    let mut files = vec![preset.info.clone()];
    let mut push = |info: &ContentInfo, files: &mut Vec<ContentInfo>| {
        if !files.iter().any(|f| f.id == info.id) {
            files.push(info.clone());
        }
    };
    for engine in refs(&text, "engine") {
        let id = join_id(&dir(&preset.info.id), engine);
        let e = find(&id)?;
        push(&e.info, &mut files);
        let etext = std::fs::read_to_string(&e.path).ok()?;
        for pack in refs(&etext, "pack") {
            let pid = join_id(&dir(&id), pack);
            if let Some(p) = find(&pid) {
                push(&p.info, &mut files);
            } else {
                tracing::warn!(kit = %preset.info.id, pack = %pid, "content: kit names a pack the host lacks");
                return None;
            }
        }
    }
    for shared in ["Mixes/", "Modules/", "Space/"] {
        let prefix = format!("{library}/{shared}");
        for e in entries
            .iter()
            .filter(|e| &e.info.variant == variant && e.info.id.starts_with(&prefix))
        {
            push(&e.info, &mut files);
        }
    }
    Some(files)
}

#[cfg(test)]
mod tests {
    use super::{join_id, refs};

    #[test]
    fn paths_and_references_resolve() {
        assert_eq!(
            join_id("Kits/MM2/Presets", "../Engines/K.signalengine"),
            "Kits/MM2/Engines/K.signalengine"
        );
        assert_eq!(
            refs(
                "  engine \"../Engines/A.signalengine\"\n  id \"kick\"",
                "engine"
            ),
            ["../Engines/A.signalengine"]
        );
    }
}
