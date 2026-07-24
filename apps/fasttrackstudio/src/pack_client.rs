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

/// One download's progress stream.
#[derive(Clone, Debug)]
pub(crate) enum DownloadEvent {
    /// Bytes on disk so far (monotonic; starts at the resume point).
    Progress { done: u64, total: u64 },
    Done(PathBuf),
    Failed(String),
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

/// Download `info` into `dest_dir`, resuming any partial file. Events
/// arrive on the returned channel; the terminal event is `Done`/`Failed`.
pub(crate) fn start_download(
    target: EngineTarget,
    info: PackInfo,
    dest_dir: PathBuf,
) -> futures_channel::mpsc::UnboundedReceiver<DownloadEvent> {
    let (tx, rx) = futures_channel::mpsc::unbounded();
    runtime().spawn(async move {
        match download(&target, &info, &dest_dir, &tx).await {
            Ok(path) => {
                let _ = tx.unbounded_send(DownloadEvent::Done(path));
            }
            Err(e) => {
                let _ = tx.unbounded_send(DownloadEvent::Failed(e));
            }
        }
    });
    rx
}

async fn download(
    target: &EngineTarget,
    info: &PackInfo,
    dest_dir: &Path,
    events: &futures_channel::mpsc::UnboundedSender<DownloadEvent>,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("create {dest_dir:?}: {e}"))?;
    let final_path = dest_dir.join(format!("{}.signalpack", info.name));
    if std::fs::metadata(&final_path).map(|m| m.len()).ok() == Some(info.size_bytes) {
        return Ok(final_path); // already downloaded
    }

    // Resume from the .part file (truncate a stale over-long one).
    let part = dest_dir.join(format!("{}.signalpack.part", info.name));
    let mut start = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if start > info.size_bytes {
        let _ = std::fs::remove_file(&part);
        start = 0;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&part)
        .map_err(|e| format!("open {part:?}: {e}"))?;

    let client: PackLibraryClient = crate::remote::establish_verbose(target)
        .await
        .map_err(|e| format!("pack host unreachable: {e}"))?;

    let (tx, mut rx) = vox::channel::<PackChunk>();
    let read_call = client.read(info.name.clone(), info.variant.clone(), start, tx);

    let total = info.size_bytes;
    let mut done = start;
    let drain = async {
        while let Ok(Some(chunk)) = rx.recv().await {
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
    read_result.map_err(|e| format!("read: {e:?}"))?;
    drain_result?;
    file.flush().map_err(|e| format!("flush: {e}"))?;
    drop(file);

    if done != total {
        return Err(format!("incomplete: {done} of {total} bytes (rerun to resume)"));
    }
    if !info.sha256.is_empty() {
        let actual = sha256_file(&part).map_err(|e| format!("hash: {e}"))?;
        if actual != info.sha256 {
            let _ = std::fs::remove_file(&part);
            return Err("sha256 mismatch — corrupt download removed, try again".into());
        }
    }
    std::fs::rename(&part, &final_path).map_err(|e| format!("rename: {e}"))?;
    Ok(final_path)
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
