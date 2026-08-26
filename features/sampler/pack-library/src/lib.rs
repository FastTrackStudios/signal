//! Headless pack-library host — serves a `.signalpack` tree over vox.
//!
//! Scans a library root for built packs (`Proxy/` subtree = the Ogg
//! streaming variant, everything else = lossless Full), hashes them in
//! the background (sha256 sidecars, so restarts are instant), and
//! implements [`signal_packs_proto::packs::PackLibrary`]: list + ranged
//! chunk streaming. Mount [`PackLibraryBackend::router`] on any vox
//! transport — the engine's WebSocket router and its iroh p2p endpoint
//! both serve it unchanged.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use std::collections::HashMap;

use architect::{layers, Layer, Services};
use signal_packs_proto::packs::PackLibrary;
use signal_packs_proto::{PackChunk, PackError, PackInfo, PackRange, PackSegment};
use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _};

/// Default library root scanned when `FTS_PACK_LIBRARY` is unset.
const LIBRARY_ROOT: &str = "/run/media/AudioHaven/Signal/Libraries";

/// Streamed chunk size — matches the task-server media lane.
const CHUNK_BYTES: usize = 256 * 1024;

#[derive(Clone)]
struct Entry {
    info: PackInfo,
    path: PathBuf,
}

/// The pack-library backend handle. Cheap to clone (state shared).
#[derive(Clone, architect::HasDispatcher)]
pub struct PackLibraryBackend {
    entries: Arc<Mutex<Vec<Entry>>>,
    /// Memoized range-streaming plans (facet-json of `Vec<PackSegment>`),
    /// keyed `(name, variant)` — planning re-opens the pack (header +
    /// index), so a piano's plan is computed once per process, not once
    /// per client.
    plans: Arc<Mutex<HashMap<(String, String), String>>>,
}

impl Default for PackLibraryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl PackLibraryBackend {
    /// Scan `FTS_PACK_LIBRARY` (default the studio library root) and
    /// start the background hasher.
    pub fn new() -> Self {
        let root = std::env::var("FTS_PACK_LIBRARY").unwrap_or_else(|_| LIBRARY_ROOT.into());
        Self::with_root(root)
    }

    /// Scan an explicit library root.
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let entries = scan(&root);
        tracing::info!(root = %root.display(), packs = entries.len(), "pack library scanned");
        let backend = Self {
            entries: Arc::new(Mutex::new(entries)),
            plans: Arc::new(Mutex::new(HashMap::new())),
        };
        backend.spawn_hasher();
        backend
    }

    /// The composed service router — mount on any vox transport.
    pub fn router(&self) -> architect::LayerRouter {
        self.clone().into_router()
    }

    fn find(&self, name: &str, variant: &str) -> Option<Entry> {
        let entries = self.entries.lock().ok()?;
        entries
            .iter()
            .find(|e| e.info.name == name && e.info.variant == variant)
            .cloned()
    }

    /// Hash entries without a sha, sidecar-cached (`<pack>.sha256`
    /// holding `<size>:<hex>`; recomputed when the size changed).
    ///
    /// Eager hashing covers only the **proxy** variants — the
    /// distribution set, a few GB. Full packs (the studio's multi-TB
    /// lossless trees) stay `<pending>` until someone builds their
    /// sidecars out-of-band; clients skip verification for an empty
    /// sha rather than forcing the host to read every drive it owns.
    fn spawn_hasher(&self) {
        let entries = self.entries.clone();
        let _ = std::thread::Builder::new()
            .name("pack-hasher".into())
            .spawn(move || {
                let todo: Vec<Entry> = entries
                    .lock()
                    .map(|e| {
                        e.iter()
                            .filter(|e| e.info.sha256.is_empty() && e.info.variant == "proxy")
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default();
                for entry in todo {
                    match sidecar_or_compute(&entry.path, entry.info.size_bytes) {
                        Ok(hex) => {
                            if let Ok(mut entries) = entries.lock() {
                                if let Some(e) = entries.iter_mut().find(|e| {
                                    e.info.name == entry.info.name
                                        && e.info.variant == entry.info.variant
                                }) {
                                    e.info.sha256 = hex;
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(pack = %entry.path.display(), "pack hash failed: {e}")
                        }
                    }
                }
            });
    }
}

impl PackLibrary for PackLibraryBackend {
    fn packs(&self) -> Vec<PackInfo> {
        self.entries
            .lock()
            .map(|e| e.iter().map(|e| e.info.clone()).collect())
            .unwrap_or_default()
    }

    async fn read(
        &self,
        name: String,
        variant: String,
        start: u64,
        tx: vox::Tx<PackChunk>,
    ) -> Result<(), PackError> {
        // Virtual names (see the trait docs) — the browser's route to the
        // W7 plan/range operations over the one proven wire signature.
        if let Some(pname) = name.strip_prefix("plan:") {
            return self.pack_plan(pname.to_string(), variant, start, tx).await;
        }
        if let Some(rest) = name.strip_prefix("range:") {
            let (range, pname) = rest.split_once(':').ok_or_else(|| {
                PackError::InvalidRange("virtual name is not range:<start>+<len>:<name>".into())
            })?;
            return self
                .read_range(pname.to_string(), variant, range.to_string(), tx)
                .await;
        }
        let entry = self.find(&name, &variant).ok_or(PackError::NotFound)?;
        let size = entry.info.size_bytes;
        if start > size {
            return Err(PackError::InvalidRange(format!(
                "start {start} > size {size}"
            )));
        }
        let mut file = tokio::fs::File::open(&entry.path)
            .await
            .map_err(|e| PackError::Io(e.to_string()))?;
        file.seek(std::io::SeekFrom::Start(start))
            .await
            .map_err(|e| PackError::Io(e.to_string()))?;
        let mut offset = start;
        let mut buf = vec![0u8; CHUNK_BYTES];
        loop {
            let n = file
                .read(&mut buf)
                .await
                .map_err(|e| PackError::Io(e.to_string()))?;
            if n == 0 {
                return Ok(());
            }
            if tx
                .send(PackChunk {
                    offset,
                    bytes: buf[..n].to_vec(),
                })
                .await
                .is_err()
            {
                // Receiver went away (download cancelled) — not an error.
                return Ok(());
            }
            offset += n as u64;
        }
    }

    async fn read_range(
        &self,
        name: String,
        variant: String,
        range: String,
        tx: vox::Tx<PackChunk>,
    ) -> Result<(), PackError> {
        let range: PackRange = range.parse().map_err(PackError::InvalidRange)?;
        // Wide event: enrich architect's per-RPC span so a slow or failing
        // range fetch names the pack and the bytes asked for.
        architect_telemetry::wide::set("pack.name", name.clone());
        architect_telemetry::wide::set("pack.variant", variant.clone());
        architect_telemetry::wide::set("pack.range_start", range.start as i64);
        architect_telemetry::wide::set("pack.range_len", range.len as i64);
        let entry = self.find(&name, &variant).ok_or(PackError::NotFound)?;
        let size = entry.info.size_bytes;
        let end = range
            .start
            .checked_add(range.len)
            .filter(|&e| e <= size)
            .ok_or_else(|| {
                PackError::InvalidRange(format!(
                    "range {}+{} exceeds size {size}",
                    range.start, range.len
                ))
            })?;
        let mut file = tokio::fs::File::open(&entry.path)
            .await
            .map_err(|e| PackError::Io(e.to_string()))?;
        file.seek(std::io::SeekFrom::Start(range.start))
            .await
            .map_err(|e| PackError::Io(e.to_string()))?;
        let mut offset = range.start;
        let mut buf = vec![0u8; CHUNK_BYTES];
        while offset < end {
            let want =
                usize::try_from((end - offset).min(CHUNK_BYTES as u64)).expect("chunk fits usize");
            let n = file
                .read(&mut buf[..want])
                .await
                .map_err(|e| PackError::Io(e.to_string()))?;
            if n == 0 {
                return Err(PackError::Io(format!(
                    "pack truncated at {offset} (wanted {end})"
                )));
            }
            if tx
                .send(PackChunk {
                    offset,
                    bytes: buf[..n].to_vec(),
                })
                .await
                .is_err()
            {
                // Receiver went away (fetch cancelled) — not an error.
                return Ok(());
            }
            offset += n as u64;
        }
        Ok(())
    }

    /// Stream the prioritized plan (facet-json of `Vec<PackSegment>`,
    /// UTF-8 bytes chunked like a pack read — see the trait docs for why
    /// it is a byte stream). Planning itself is a header + index read (a
    /// few MB even for a multi-GB pack), memoized per pack, and run off
    /// the async executor.
    async fn pack_plan(
        &self,
        name: String,
        variant: String,
        start: u64,
        tx: vox::Tx<PackChunk>,
    ) -> Result<(), PackError> {
        // Wide event: name the pack on architect's per-RPC span, and record
        // whether the plan came from the memo or was computed fresh.
        architect_telemetry::wide::set("pack.name", name.clone());
        architect_telemetry::wide::set("pack.variant", variant.clone());
        let entry = self.find(&name, &variant).ok_or(PackError::NotFound)?;
        let total = entry.info.size_bytes;
        let key = (name, variant);
        let cached = self.plans.lock().ok().and_then(|p| p.get(&key).cloned());
        architect_telemetry::wide::set("pack.plan_cached", cached.is_some());
        let json = match cached {
            Some(json) => json,
            None => {
                let path = entry.path.clone();
                let planned = tokio::task::spawn_blocking(move || {
                    signal_sampler::pack_plan::plan_pack_file(&path)
                })
                .await
                .map_err(|e| PackError::Io(e.to_string()))?
                .map_err(|e| PackError::Io(e.to_string()))?;
                if planned.total != total {
                    return Err(PackError::Io(format!(
                        "pack size changed under the library ({} planned vs {total} listed)",
                        planned.total
                    )));
                }
                let segments: Vec<PackSegment> = planned
                    .segments
                    .into_iter()
                    .map(|s| PackSegment {
                        start: s.start,
                        len: s.len,
                        rank: u64::from(s.rank),
                        label: s.label,
                    })
                    .collect();
                let json = facet_json::to_string(&segments)
                    .map_err(|e| PackError::Io(format!("plan json: {e}")))?;
                if let Ok(mut plans) = self.plans.lock() {
                    plans.insert(key, json.clone());
                }
                json
            }
        };
        let bytes = json.as_bytes();
        let len = bytes.len() as u64;
        if start > len {
            return Err(PackError::InvalidRange(format!(
                "start {start} > plan size {len}"
            )));
        }
        let mut offset = start;
        while offset < len {
            let end = (offset + CHUNK_BYTES as u64).min(len);
            let chunk = bytes[offset as usize..end as usize].to_vec();
            if tx
                .send(PackChunk {
                    offset,
                    bytes: chunk,
                })
                .await
                .is_err()
            {
                return Ok(()); // receiver went away — not an error
            }
            offset = end;
        }
        Ok(())
    }
}

impl Services for PackLibraryBackend {
    fn layers() -> impl Layer<Self> {
        layers![signal_packs_proto::packs::Service]
    }
}

// ── scan + hash helpers ─────────────────────────────────────────────────────

/// Walk `root` for `*.signalpack`; the `Proxy/` subtree is the "proxy"
/// variant with the prefix stripped from the category, so a proxy pack
/// and its lossless twin share a category and differ only by variant.
fn scan(root: &Path) -> Vec<Entry> {
    let mut entries = Vec::new();
    for f in walkdir::WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .flatten()
    {
        let path = f.path();
        if !path.is_file()
            || path
                .extension()
                .is_none_or(|e| !e.eq_ignore_ascii_case("signalpack"))
        {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(rel_dir) = path.parent().and_then(|p| p.strip_prefix(root).ok()) else {
            continue;
        };
        let rel = rel_dir.to_string_lossy().replace('\\', "/");
        let (variant, category) = match rel.strip_prefix("Proxy/") {
            Some(rest) => ("proxy", rest.to_string()),
            None if rel == "Proxy" => ("proxy", String::new()),
            None => ("full", rel),
        };
        let size_bytes = f.metadata().map(|m| m.len()).unwrap_or(0);
        entries.push(Entry {
            info: PackInfo {
                name: name.to_string(),
                category,
                variant: variant.to_string(),
                size_bytes,
                sha256: String::new(),
            },
            path: path.to_path_buf(),
        });
    }
    entries.sort_by(|a, b| {
        (&a.info.category, &a.info.name, &a.info.variant).cmp(&(
            &b.info.category,
            &b.info.name,
            &b.info.variant,
        ))
    });
    entries
}

/// Read the `<pack>.sha256` sidecar (format `<size>:<hex>`), else hash
/// the file and write one.
fn sidecar_or_compute(path: &Path, size: u64) -> std::io::Result<String> {
    let sidecar = path.with_extension("signalpack.sha256");
    if let Ok(text) = std::fs::read_to_string(&sidecar) {
        if let Some((s, hex)) = text.trim().split_once(':') {
            if s.parse::<u64>().ok() == Some(size) && !hex.is_empty() {
                return Ok(hex.to_string());
            }
        }
    }
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
    let hex = format!("{:x}", hasher.finalize());
    let _ = std::fs::write(&sidecar, format!("{size}:{hex}\n"));
    Ok(hex)
}
