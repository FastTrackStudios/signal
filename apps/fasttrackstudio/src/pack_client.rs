//! Pack downloads — the client half of `signal-packs-proto`.
//!
//! Lists and fetches `.signalpack`s from a pack host (the studio engine,
//! a hosted mirror — anything serving [`signal_packs_proto::packs`])
//! over the shared engine plumbing (`remote.rs`: WebSocket or iroh p2p).
//! Downloads land in the keys packs dir with `.part` resume + sha256
//! verify, then the keys rig rescans and the pack is playable.
//!
//! All network + file work runs on this module's own small runtime and
//! reports through plain channels, so UI callers (dioxus `spawn`, any
//! surface) never need a tokio context of their own.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use signal_packs_proto::packs::PackLibraryClient;
use signal_packs_proto::{PackChunk, PackInfo};

use crate::remote::EngineTarget;

/// The hosted pack mirror — plain HTTPS with Range resume, the backup
/// when no vox pack host (p2p or LAN) is reachable.
const MIRROR_BASE_URL: &str =
    "https://fasttrackstudio.app/org/fasttrackstudios/media/packs";

/// Where a download comes from: a vox pack host (studio engine over
/// iroh/ws) or the HTTPS mirror.
#[derive(Clone)]
pub(crate) enum PackSource {
    Vox(EngineTarget),
    Mirror,
}

impl PackSource {
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Vox(t) => t.label(),
            Self::Mirror => "fasttrackstudio.app mirror".into(),
        }
    }
}

/// One download's progress stream.
#[derive(Clone, Debug)]
pub(crate) enum DownloadEvent {
    /// Bytes on disk so far (monotonic; starts at the resume point).
    Progress { done: u64, total: u64 },
    /// Paused by the user — the `.part` file stays; a new
    /// [`start_download`] resumes from it.
    Paused { done: u64, total: u64 },
    Done(PathBuf),
    Failed(String),
}

/// Internal sentinel: the transfer loop stopped because of a pause.
const PAUSED: &str = "__paused__";

/// Packs with a transfer in flight — a second concurrent download of
/// the same pack would append into the same `.part` and corrupt it
/// (observed as `hash: No such file` when the first rename won).
fn in_flight() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    static SET: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    SET.get_or_init(Default::default)
}

/// The mirror's `packs.json` shape — `PackInfo` rows under one key.
#[derive(facet::Facet)]
struct MirrorIndex {
    packs: Vec<PackInfo>,
}

/// Where downloaded keys packs land — the same dir the keys rig scans
/// (`FTS_KEYSCAPE_PACKS`, which the iOS shell points inside
/// Documents/FastTrackStudio). Desktop fallback matches the keys
/// backend's studio default, so a desktop build lists the local library.
pub(crate) fn keys_packs_dir() -> PathBuf {
    if let Ok(p) = std::env::var("FTS_KEYSCAPE_PACKS") {
        return p.into();
    }
    "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs".into()
}

/// The module's private runtime: vox pumps + file IO for every pack
/// operation, independent of whichever executor the UI runs on.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("pack-client")
            .enable_all()
            .build()
            .expect("pack-client runtime")
    })
}

/// List the host's packs. Resolves to `Err` on dial/handshake failure.
pub(crate) fn fetch_packs(
    target: EngineTarget,
) -> futures_channel::oneshot::Receiver<Result<Vec<PackInfo>, String>> {
    let (tx, rx) = futures_channel::oneshot::channel();
    runtime().spawn(async move {
        let attempt = async {
            let client: PackLibraryClient = crate::remote::establish_verbose(&target)
                .await
                .map_err(|e| format!("pack host unreachable: {e}"))?;
            client.packs().await.map_err(|e| format!("packs: {e:?}"))
        };
        // Iroh discovery + relay + hole-punch can take a while on a cold
        // path, but never forever — bound it so the UI gets a real answer.
        let result = match tokio::time::timeout(std::time::Duration::from_secs(30), attempt).await
        {
            Ok(r) => r,
            Err(_) => Err(format!("pack host timed out after 30s ({})", target.label())),
        };
        let _ = tx.send(result);
    });
    rx
}

/// List the HTTPS mirror's packs. Resolves to `Err` with the reason on
/// any network/parse failure.
pub(crate) fn fetch_mirror_packs(
) -> futures_channel::oneshot::Receiver<Result<Vec<PackInfo>, String>> {
    let (tx, rx) = futures_channel::oneshot::channel();
    runtime().spawn(async move {
        let attempt = async {
            let body = reqwest::get(format!("{MIRROR_BASE_URL}/packs.json"))
                .await
                .map_err(|e| format!("mirror: {e}"))?
                .error_for_status()
                .map_err(|e| format!("mirror: {e}"))?
                .text()
                .await
                .map_err(|e| format!("mirror read: {e}"))?;
            let index: MirrorIndex =
                facet_json::from_str(&body).map_err(|e| format!("mirror index: {e}"))?;
            Ok(index.packs)
        };
        let result = match tokio::time::timeout(std::time::Duration::from_secs(20), attempt).await
        {
            Ok(r) => r,
            Err(_) => Err("mirror timed out after 20s".into()),
        };
        let _ = tx.send(result);
    });
    rx
}

/// Download `info` from `source` into `dest_dir`, resuming any partial
/// file. Events arrive on the returned channel; the terminal event is
/// `Paused`/`Done`/`Failed`. Flip the returned cancel flag to pause —
/// the `.part` stays and a fresh `start_download` resumes.
pub(crate) fn start_download(
    source: PackSource,
    info: PackInfo,
    dest_dir: PathBuf,
) -> (
    futures_channel::mpsc::UnboundedReceiver<DownloadEvent>,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let (tx, rx) = futures_channel::mpsc::unbounded();
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel_flag = cancel.clone();
    // One transfer per pack, ever — a duplicate reports and bails
    // without touching the .part.
    if !in_flight().lock().map(|mut s| s.insert(info.name.clone())).unwrap_or(false) {
        let _ = tx.unbounded_send(DownloadEvent::Failed("already downloading".into()));
        return (rx, cancel);
    }
    runtime().spawn(async move {
        let _guard = scopeguard(info.name.clone());
        let result = match &source {
            PackSource::Vox(target) => {
                download(target, &info, &dest_dir, &tx, &cancel_flag).await
            }
            PackSource::Mirror => mirror_download(&info, &dest_dir, &tx, &cancel_flag).await,
        };
        match result {
            Ok(path) => {
                let _ = tx.unbounded_send(DownloadEvent::Done(path));
            }
            Err(e) if e == PAUSED => {
                let done = std::fs::metadata(
                    dest_dir.join(format!("{}.signalpack.part", info.name)),
                )
                .map(|m| m.len())
                .unwrap_or(0);
                let _ = tx.unbounded_send(DownloadEvent::Paused {
                    done,
                    total: info.size_bytes,
                });
            }
            Err(e) => {
                let _ = tx.unbounded_send(DownloadEvent::Failed(e));
            }
        }
    });
    (rx, cancel)
}

/// Shared download scaffolding: the destination, the `.part` resume
/// file (opened append), and how many bytes are already on disk.
struct Prepared {
    final_path: PathBuf,
    part: PathBuf,
    file: std::fs::File,
    start: u64,
}

/// `Ok(Err(path))` = already fully downloaded.
fn prepare(dest_dir: &Path, info: &PackInfo) -> Result<Result<Prepared, PathBuf>, String> {
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("create {dest_dir:?}: {e}"))?;
    let final_path = dest_dir.join(format!("{}.signalpack", info.name));
    if std::fs::metadata(&final_path).map(|m| m.len()).ok() == Some(info.size_bytes) {
        return Ok(Err(final_path));
    }
    // Resume from the .part file (truncate a stale over-long one).
    let part = dest_dir.join(format!("{}.signalpack.part", info.name));
    let mut start = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if start > info.size_bytes {
        let _ = std::fs::remove_file(&part);
        start = 0;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&part)
        .map_err(|e| format!("open {part:?}: {e}"))?;
    Ok(Ok(Prepared { final_path, part, file, start }))
}

/// Completeness + integrity gate, then the atomic rename into place.
fn finish(prep_part: &Path, final_path: &Path, info: &PackInfo, done: u64) -> Result<PathBuf, String> {
    if done != info.size_bytes {
        return Err(format!(
            "incomplete: {done} of {} bytes (rerun to resume)",
            info.size_bytes
        ));
    }
    if !info.sha256.is_empty() {
        let actual = sha256_file(prep_part).map_err(|e| format!("hash: {e}"))?;
        if actual != info.sha256 {
            let _ = std::fs::remove_file(prep_part);
            return Err("sha256 mismatch — corrupt download removed, try again".into());
        }
    }
    std::fs::rename(prep_part, final_path).map_err(|e| format!("rename: {e}"))?;
    Ok(final_path.to_path_buf())
}

async fn download(
    target: &EngineTarget,
    info: &PackInfo,
    dest_dir: &Path,
    events: &futures_channel::mpsc::UnboundedSender<DownloadEvent>,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<PathBuf, String> {
    let Prepared { final_path, part, mut file, start } = match prepare(dest_dir, info)? {
        Ok(p) => p,
        Err(done_path) => return Ok(done_path),
    };

    let client: PackLibraryClient = crate::remote::establish_verbose(target)
        .await
        .map_err(|e| format!("pack host unreachable: {e}"))?;

    let (tx, mut rx) = vox::channel::<PackChunk>();
    let read_call = client.read(info.name.clone(), info.variant.clone(), start, tx);

    let total = info.size_bytes;
    let mut done = start;
    let drain = async {
        while let Ok(Some(chunk)) = rx.recv().await {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(PAUSED.to_string());
            }
            let chunk = chunk.get();
            if chunk.offset != done {
                return Err(format!(
                    "non-contiguous chunk: got offset {} at {}",
                    chunk.offset, done
                ));
            }
            file.write_all(&chunk.bytes).map_err(|e| format!("write: {e}"))?;
            done += chunk.bytes.len() as u64;
            let _ = events.unbounded_send(DownloadEvent::Progress { done, total });
        }
        Ok(())
    };

    let (read_result, drain_result) = futures_util::join!(read_call, drain);
    if let Err(e) = &drain_result
        && e == PAUSED
    {
        return Err(PAUSED.to_string());
    }
    read_result.map_err(|e| format!("read: {e:?}"))?;
    drain_result?;
    file.flush().map_err(|e| format!("flush: {e}"))?;
    drop(file);
    finish(&part, &final_path, info, done)
}

/// Ranged HTTPS chunk size — big enough to amortise request overhead,
/// small enough that progress stays live and resume loses little.
const MIRROR_CHUNK: u64 = 16 * 1024 * 1024;

async fn mirror_download(
    info: &PackInfo,
    dest_dir: &Path,
    events: &futures_channel::mpsc::UnboundedSender<DownloadEvent>,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<PathBuf, String> {
    let Prepared { final_path, part, mut file, start } = match prepare(dest_dir, info)? {
        Ok(p) => p,
        Err(done_path) => return Ok(done_path),
    };
    let client = reqwest::Client::new();
    let url = format!("{MIRROR_BASE_URL}/{}.signalpack", info.name.replace(' ', "%20"));
    let total = info.size_bytes;
    let mut done = start;
    while done < total {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(PAUSED.to_string());
        }
        let end = (done + MIRROR_CHUNK - 1).min(total - 1);
        let resp = client
            .get(&url)
            .header(reqwest::header::RANGE, format!("bytes={done}-{end}"))
            .send()
            .await
            .map_err(|e| format!("mirror: {e}"))?;
        if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(format!("mirror: HTTP {} (expected 206)", resp.status()));
        }
        let bytes = resp.bytes().await.map_err(|e| format!("mirror read: {e}"))?;
        if bytes.is_empty() {
            return Err("mirror: empty range response".into());
        }
        file.write_all(&bytes).map_err(|e| format!("write: {e}"))?;
        done += bytes.len() as u64;
        let _ = events.unbounded_send(DownloadEvent::Progress { done, total });
    }
    file.flush().map_err(|e| format!("flush: {e}"))?;
    drop(file);
    finish(&part, &final_path, info, done)
}

/// Drop guard that releases a pack's in-flight slot however the
/// transfer task ends.
struct InFlightGuard(String);
impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if let Ok(mut s) = in_flight().lock() {
            s.remove(&self.0);
        }
    }
}
fn scopeguard(name: String) -> InFlightGuard {
    InFlightGuard(name)
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
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
