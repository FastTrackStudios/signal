//! Content transfer client — fetch what a host offers
//! ([`signal_content_proto::content::ContentLibrary`]) onto this device.
//!
//! Every file lands at `<dest root>/<id>`, so the device's library has the
//! host's layout and a downloaded kit plays where it lands. Transfers are
//! resumable (`<file>.part`, opened append), verified (sha256 when the
//! host has one, then a `<file>.sha256` sidecar beside it so a later
//! bundle knows it has the file without reading it again), atomic (rename
//! into place), and one at a time per file — a second transfer of the
//! same file would append into the same `.part`.
//!
//! A **bundle** downloads its files in turn, skipping those already here
//! with the same hash: kits sharing a library's packs fetch them once.
//!
//! All network and file work runs on this crate's own small runtime and
//! reports through channels, so a UI never needs a Tokio context.

use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use signal_content_proto::content::ContentLibraryClient;
use signal_content_proto::{BundleInfo, ContentChunk, ContentInfo};

/// A transfer's progress.
#[derive(Clone, Debug, PartialEq)]
pub enum TransferEvent {
    /// Starting file `index` of `count` (a single file is 0 of 1).
    File {
        id: String,
        index: u64,
        count: u64,
    },
    /// Bytes here so far, of the whole transfer.
    Progress {
        done: u64,
        total: u64,
    },
    /// Paused: the `.part` stays; starting again resumes.
    Paused {
        done: u64,
        total: u64,
    },
    /// Everything is here: the file, or the bundle's library folder.
    Done(PathBuf),
    Failed(String),
}

type Events = futures_channel::mpsc::UnboundedSender<TransferEvent>;

/// The paused sentinel inside the transfer loop.
const PAUSED: &str = "__paused__";

/// The crate's runtime: vox pumps and file IO, whatever executor the
/// caller runs on.
#[must_use]
pub fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("content-client")
            .enable_all()
            .build()
            .unwrap_or_else(|e| panic!("content-client runtime: {e}"))
    })
}

/// Where `info` lives under `dest_root`.
///
/// # Errors
/// When the host's id is not a plain relative path (see [`under_root`]):
/// a hostile or buggy host must not place files outside `dest_root`.
pub fn local_path(dest_root: &Path, info: &ContentInfo) -> Result<PathBuf, String> {
    under_root(dest_root, &info.id)
}

/// `rel`, a host-supplied `/`-separated path, joined under `root`.
///
/// # Errors
/// Unless every segment is a plain name: an empty (`a//b`, a leading
/// `/`), `.`, `..` or absolute/prefixed segment is refused, so the result
/// always stays inside `root`.
fn under_root(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let mut path = root.to_path_buf();
    for seg in rel.split('/') {
        let mut parts = Path::new(seg).components();
        match (parts.next(), parts.next()) {
            (Some(Component::Normal(name)), None) => path.push(name),
            _ => {
                return Err(format!(
                    "refusing the host's path {rel:?}: not a plain relative path"
                ));
            }
        }
    }
    Ok(path)
}

/// Whether `info` is already here, whole: the right size, and (when the
/// host has a hash) the sidecar a finished transfer left says the same.
/// A path the host may not use is never here.
#[must_use]
pub fn have(dest_root: &Path, info: &ContentInfo) -> bool {
    let Ok(path) = local_path(dest_root, info) else {
        return false;
    };
    if std::fs::metadata(&path).map(|m| m.len()).ok() != Some(info.size_bytes) {
        return false;
    }
    info.sha256.is_empty() || sidecar(&path).is_some_and(|h| h == info.sha256)
}

// ── one transfer per file ──

fn in_flight() -> &'static std::sync::Mutex<std::collections::HashSet<PathBuf>> {
    static SET: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<PathBuf>>> =
        std::sync::OnceLock::new();
    SET.get_or_init(Default::default)
}

/// Holds a file's in-flight slot; released however the transfer ends.
struct Slot(PathBuf);
impl Slot {
    fn take(path: &Path) -> Option<Self> {
        in_flight()
            .lock()
            .ok()?
            .insert(path.to_path_buf())
            .then(|| Self(path.to_path_buf()))
    }
}
impl Drop for Slot {
    fn drop(&mut self) {
        if let Ok(mut s) = in_flight().lock() {
            s.remove(&self.0);
        }
    }
}

// ── the public transfers ──

/// Download one file from `client` into `dest_root`. Events arrive on the
/// returned channel, ending in `Paused`/`Done`/`Failed`; flip the flag to
/// pause.
pub fn download_file(
    client: ContentLibraryClient,
    info: ContentInfo,
    dest_root: PathBuf,
) -> (
    futures_channel::mpsc::UnboundedReceiver<TransferEvent>,
    Arc<AtomicBool>,
) {
    let (tx, rx) = futures_channel::mpsc::unbounded();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    runtime().spawn(async move {
        let _ = tx.unbounded_send(TransferEvent::File {
            id: info.id.clone(),
            index: 0,
            count: 1,
        });
        let total = info.size_bytes;
        let result = fetch(&client, &info, &dest_root, 0, total, &tx, &flag).await;
        let _ = tx.unbounded_send(match result {
            Ok(path) => TransferEvent::Done(path),
            Err(e) if e == PAUSED => {
                let done = local_path(&dest_root, &info).map_or(0, |p| part_len(&p));
                TransferEvent::Paused { done, total }
            }
            Err(e) => TransferEvent::Failed(e),
        });
    });
    (rx, cancel)
}

/// Download a bundle — every file it lists that is not already here — into
/// `dest_root`. Ends with `Done(<dest_root>/<library>)`.
pub fn download_bundle(
    client: ContentLibraryClient,
    bundle: BundleInfo,
    dest_root: PathBuf,
) -> (
    futures_channel::mpsc::UnboundedReceiver<TransferEvent>,
    Arc<AtomicBool>,
) {
    let (tx, rx) = futures_channel::mpsc::unbounded();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    runtime().spawn(async move {
        let result = async {
            let library = under_root(&dest_root, &bundle.library)?;
            let files = client
                .bundle_files(bundle.id.clone(), bundle.variant.clone())
                .await
                .map_err(|e| format!("bundle files: {e:?}"))?;
            if files.is_empty() {
                return Err(format!("the host has no {} {:?}", bundle.kind, bundle.name));
            }
            let total: u64 = files.iter().map(|f| f.size_bytes).sum();
            let count = files.len() as u64;
            let mut done = 0u64;
            for (index, info) in files.iter().enumerate() {
                let _ = tx.unbounded_send(TransferEvent::File {
                    id: info.id.clone(),
                    index: index as u64,
                    count,
                });
                if !have(&dest_root, info) {
                    fetch(&client, info, &dest_root, done, total, &tx, &flag).await?;
                }
                done += info.size_bytes;
                let _ = tx.unbounded_send(TransferEvent::Progress { done, total });
            }
            Ok(library)
        }
        .await;
        let _ = tx.unbounded_send(match result {
            Ok(path) => TransferEvent::Done(path),
            Err(e) if e == PAUSED => TransferEvent::Paused {
                done: 0,
                total: bundle.size_bytes,
            },
            Err(e) => TransferEvent::Failed(e),
        });
    });
    (rx, cancel)
}

/// Fetch one file into place. `base` is what the transfer had before it
/// (a bundle's earlier files), so progress counts the whole.
async fn fetch(
    client: &ContentLibraryClient,
    info: &ContentInfo,
    dest_root: &Path,
    base: u64,
    total: u64,
    events: &Events,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let final_path = local_path(dest_root, info)?;
    if have(dest_root, info) {
        return Ok(final_path);
    }
    let Some(_slot) = Slot::take(&final_path) else {
        return Err(format!("{} is already downloading", info.id));
    };
    let (part, mut file, start) = prepare(&final_path, info.size_bytes)?;
    let (tx, mut rx) = vox::channel::<ContentChunk>();
    let read = client.read(info.id.clone(), info.variant.clone(), start, tx);
    let mut done = start;
    let drain = async {
        while let Ok(Some(chunk)) = rx.recv().await {
            if cancel.load(Ordering::Relaxed) {
                return Err(PAUSED.to_string());
            }
            let chunk = chunk.get();
            if chunk.offset != done {
                return Err(format!(
                    "non-contiguous chunk: offset {} at {done}",
                    chunk.offset
                ));
            }
            file.write_all(&chunk.bytes)
                .map_err(|e| format!("write: {e}"))?;
            done += chunk.bytes.len() as u64;
            let _ = events.unbounded_send(TransferEvent::Progress {
                done: base + done,
                total,
            });
        }
        Ok(())
    };
    let (read_result, drain_result) = futures_util::join!(read, drain);
    if matches!(&drain_result, Err(e) if e == PAUSED) {
        return Err(PAUSED.to_string());
    }
    read_result.map_err(|e| format!("read {}: {e:?}", info.id))?;
    drain_result?;
    file.flush().map_err(|e| format!("flush: {e}"))?;
    drop(file);
    finish(&part, &final_path, info.size_bytes, &info.sha256, done)
}

/// Download `url` (plain HTTPS, Range resume) as `info` into `dest_root` —
/// the mirror path.
#[cfg(feature = "mirror")]
pub fn download_url(
    url: String,
    info: ContentInfo,
    dest_root: PathBuf,
) -> (
    futures_channel::mpsc::UnboundedReceiver<TransferEvent>,
    Arc<AtomicBool>,
) {
    let (tx, rx) = futures_channel::mpsc::unbounded();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    runtime().spawn(async move {
        let result = mirror_fetch(&url, &info, &dest_root, &tx, &flag).await;
        let _ = tx.unbounded_send(match result {
            Ok(path) => TransferEvent::Done(path),
            Err(e) if e == PAUSED => {
                let done = local_path(&dest_root, &info).map_or(0, |p| part_len(&p));
                TransferEvent::Paused {
                    done,
                    total: info.size_bytes,
                }
            }
            Err(e) => TransferEvent::Failed(e),
        });
    });
    (rx, cancel)
}

#[cfg(feature = "mirror")]
async fn mirror_fetch(
    url: &str,
    info: &ContentInfo,
    dest_root: &Path,
    events: &Events,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    /// Big enough to amortise a request, small enough that resume loses little.
    const RANGE: u64 = 16 * 1024 * 1024;
    let final_path = local_path(dest_root, info)?;
    if have(dest_root, info) {
        return Ok(final_path);
    }
    let Some(_slot) = Slot::take(&final_path) else {
        return Err(format!("{} is already downloading", info.id));
    };
    let (part, mut file, start) = prepare(&final_path, info.size_bytes)?;
    let http = reqwest::Client::new();
    let total = info.size_bytes;
    let mut done = start;
    while done < total {
        if cancel.load(Ordering::Relaxed) {
            return Err(PAUSED.to_string());
        }
        let end = (done + RANGE - 1).min(total - 1);
        let resp = http
            .get(url)
            .header(reqwest::header::RANGE, format!("bytes={done}-{end}"))
            .send()
            .await
            .map_err(|e| format!("mirror: {e}"))?;
        if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(format!("mirror: HTTP {} (expected 206)", resp.status()));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("mirror read: {e}"))?;
        if bytes.is_empty() {
            return Err("mirror: empty range".into());
        }
        file.write_all(&bytes).map_err(|e| format!("write: {e}"))?;
        done += bytes.len() as u64;
        let _ = events.unbounded_send(TransferEvent::Progress { done, total });
    }
    file.flush().map_err(|e| format!("flush: {e}"))?;
    drop(file);
    finish(&part, &final_path, info.size_bytes, &info.sha256, done)
}

// ── the file on disk ──

fn part_path(final_path: &Path) -> PathBuf {
    let mut s = final_path.as_os_str().to_os_string();
    s.push(".part");
    PathBuf::from(s)
}

fn sidecar_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".sha256");
    PathBuf::from(s)
}

/// The hash a finished transfer left beside `path`, if the size matches.
fn sidecar(path: &Path) -> Option<String> {
    let size = std::fs::metadata(path).ok()?.len();
    let text = std::fs::read_to_string(sidecar_path(path)).ok()?;
    let (s, hex) = text.trim().split_once(':')?;
    (s.parse::<u64>().ok() == Some(size)).then(|| hex.to_string())
}

fn part_len(final_path: &Path) -> u64 {
    std::fs::metadata(part_path(final_path))
        .map(|m| m.len())
        .unwrap_or(0)
}

/// The `.part` to append to, and how much of it is there (a stale
/// over-long one starts again).
fn prepare(final_path: &Path, size: u64) -> Result<(PathBuf, std::fs::File, u64), String> {
    if let Some(dir) = final_path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    }
    let part = part_path(final_path);
    let mut start = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if start > size {
        let _ = std::fs::remove_file(&part);
        start = 0;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&part)
        .map_err(|e| format!("open {}: {e}", part.display()))?;
    Ok((part, file, start))
}

/// Whole and the right hash, then into place with its sidecar.
fn finish(
    part: &Path,
    final_path: &Path,
    size: u64,
    sha256: &str,
    done: u64,
) -> Result<PathBuf, String> {
    if done != size {
        return Err(format!(
            "incomplete: {done} of {size} bytes (start again to resume)"
        ));
    }
    let actual = hash_file(part).map_err(|e| format!("hash: {e}"))?;
    if !sha256.is_empty() && actual != sha256 {
        let _ = std::fs::remove_file(part);
        return Err("sha256 mismatch — the corrupt download was removed; try again".into());
    }
    std::fs::rename(part, final_path).map_err(|e| format!("rename: {e}"))?;
    let _ = std::fs::write(sidecar_path(final_path), format!("{size}:{actual}\n"));
    Ok(final_path.to_path_buf())
}

fn hash_file(path: &Path) -> std::io::Result<String> {
    use sha2::Digest as _;
    use std::io::Read as _;
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
