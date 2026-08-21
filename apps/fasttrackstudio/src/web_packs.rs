//! Browser pack streaming — the wasm mirror of `pack_client.rs`.
//!
//! Streams `.signalpack`s from the engine's `PackLibrary` (the SAME `/vox`
//! target `remote.rs` resolves), caching the bytes in OPFS
//! (`packs/<name>.<variant>.signalpack`, `.part` while in flight) with a
//! download LEDGER in IndexedDB `{name, variant, expected_size, sha256,
//! bytes, state}`. Both survive refresh: a later boot resumes via
//! `read(start = committed bytes)` and a verified local file attaches with
//! no network at all.
//!
//! OPFS `createWritable` is atomic-on-close (a mid-stream refresh discards
//! the uncommitted tail), so the writer COMMITS every [`FLUSH_BYTES`]:
//! close, reopen with `keepExistingData`, seek to the end, update the
//! ledger. Resume granularity is therefore the flush interval, never the
//! whole file.

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::{Array, Reflect, Uint8Array};
use signal_packs_proto::PackChunk;
use signal_packs_proto::packs::PackLibraryClient;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    FileSystemCreateWritableOptions, FileSystemDirectoryHandle, FileSystemFileHandle,
    FileSystemGetDirectoryOptions, FileSystemGetFileOptions, FileSystemWritableFileStream,
};

use crate::remote::EngineTarget;

/// Commit interval — how much streamed data a refresh can lose.
const FLUSH_BYTES: u64 = 16 * 1024 * 1024;

/// Packs at or under this stream whole-file (the W6 path — they finish in
/// seconds); larger packs go PROGRESSIVE: plan + ranged segments, playable
/// after rank 0.
pub(crate) const PROGRESSIVE_THRESHOLD: u64 = 32 * 1024 * 1024;

/// Progress callbacks out of [`ensure_pack`].
#[derive(Clone, Copy, Debug)]
pub(crate) enum PackEvent {
    /// Committed bytes on disk so far (monotonic; starts at the resume
    /// point).
    Progress { bytes: u64, total: u64 },
    /// Transfer complete — sha256 verify running.
    Verifying,
}

/// What [`ensure_pack`] needs to know about a pack — the engine's
/// `PackLibrary` row (name/variant/size/sha).
#[derive(Clone, Debug, Default)]
pub(crate) struct PackWant {
    pub name: String,
    pub variant: String,
    pub size_bytes: u64,
    /// Hex sha256; empty = skip verification.
    pub sha256: String,
}

fn js_str(e: JsValue) -> String {
    e.as_string().unwrap_or_else(|| format!("{e:?}"))
}

// ── IndexedDB (the download ledger + small kv cache) ───────────────────────

/// One pack's ledger row, as facet-json under key `pack:<name>.<variant>`.
#[derive(Clone, Debug, Default, facet::Facet)]
struct LedgerRow {
    name: String,
    variant: String,
    expected_size: u64,
    sha256: String,
    /// Committed bytes in the `.part` (or the whole file once ready).
    bytes: u64,
    /// `streaming` | `ready`.
    state: String,
}

async fn await_idb(req: web_sys::IdbRequest) -> Result<JsValue, String> {
    let (tx, rx) = futures_channel::oneshot::channel::<Result<JsValue, String>>();
    let tx = Rc::new(RefCell::new(Some(tx)));
    let (t1, r1) = (tx.clone(), req.clone());
    let ok = Closure::<dyn FnMut()>::new(move || {
        if let Some(t) = t1.borrow_mut().take() {
            let _ = t.send(r1.result().map_err(js_str));
        }
    });
    let (t2, r2) = (tx.clone(), req.clone());
    let err = Closure::<dyn FnMut()>::new(move || {
        if let Some(t) = t2.borrow_mut().take() {
            let _ = t.send(Err(format!("idb request failed: {:?}", r2.error())));
        }
    });
    req.set_onsuccess(Some(ok.as_ref().unchecked_ref()));
    req.set_onerror(Some(err.as_ref().unchecked_ref()));
    let out = rx.await.map_err(|_| "idb: reply dropped".to_string())?;
    drop((ok, err));
    out
}

/// Open (creating on first use) the app's small kv database.
async fn idb_open() -> Result<web_sys::IdbDatabase, String> {
    let factory = web_sys::window()
        .and_then(|w| w.indexed_db().ok().flatten())
        .ok_or("IndexedDB unavailable")?;
    let req = factory
        .open_with_u32("fts-keys-rig", 1)
        .map_err(js_str)?;
    let up_req = req.clone();
    let upgrade = Closure::<dyn FnMut()>::new(move || {
        if let Ok(db) = up_req.result()
            && let Ok(db) = db.dyn_into::<web_sys::IdbDatabase>()
        {
            let _ = db.create_object_store("kv");
        }
    });
    req.set_onupgradeneeded(Some(upgrade.as_ref().unchecked_ref()));
    let db = await_idb(req.into()).await?;
    drop(upgrade);
    db.dyn_into::<web_sys::IdbDatabase>()
        .map_err(|_| "idb: not a database".to_string())
}

/// Read a string value from the kv store.
pub(crate) async fn idb_get(key: &str) -> Result<Option<String>, String> {
    let db = idb_open().await?;
    let tx = db.transaction_with_str("kv").map_err(js_str)?;
    let store = tx.object_store("kv").map_err(js_str)?;
    let req = store.get(&JsValue::from_str(key)).map_err(js_str)?;
    let v = await_idb(req).await?;
    Ok(v.as_string())
}

/// Write a string value into the kv store.
pub(crate) async fn idb_put(key: &str, value: &str) -> Result<(), String> {
    let db = idb_open().await?;
    let tx = db
        .transaction_with_str_and_mode("kv", web_sys::IdbTransactionMode::Readwrite)
        .map_err(js_str)?;
    let store = tx.object_store("kv").map_err(js_str)?;
    let req = store
        .put_with_key(&JsValue::from_str(value), &JsValue::from_str(key))
        .map_err(js_str)?;
    await_idb(req).await?;
    Ok(())
}

async fn idb_delete(key: &str) -> Result<(), String> {
    let db = idb_open().await?;
    let tx = db
        .transaction_with_str_and_mode("kv", web_sys::IdbTransactionMode::Readwrite)
        .map_err(js_str)?;
    let store = tx.object_store("kv").map_err(js_str)?;
    let req = store.delete(&JsValue::from_str(key)).map_err(js_str)?;
    await_idb(req).await?;
    Ok(())
}

fn ledger_key(name: &str, variant: &str) -> String {
    format!("pack:{name}.{variant}")
}

async fn ledger_get(name: &str, variant: &str) -> Option<LedgerRow> {
    let json = idb_get(&ledger_key(name, variant)).await.ok()??;
    facet_json::from_str(&json).ok()
}

async fn ledger_put(row: &LedgerRow) {
    if let Ok(json) = facet_json::to_string(row) {
        let _ = idb_put(&ledger_key(&row.name, &row.variant), &json).await;
    }
}

// ── OPFS (the pack bytes) ──────────────────────────────────────────────────

/// Ask the browser to keep our origin's storage (once per page).
pub(crate) async fn request_persist() {
    thread_local! {
        static ASKED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if ASKED.with(|a| a.replace(true)) {
        return;
    }
    if let Some(w) = web_sys::window()
        && let Ok(p) = w.navigator().storage().persist()
    {
        let _ = JsFuture::from(p).await;
    }
}

/// The OPFS `packs/` directory, created on first use.
async fn packs_dir() -> Result<FileSystemDirectoryHandle, String> {
    let storage = web_sys::window()
        .ok_or("no window")?
        .navigator()
        .storage();
    let root = JsFuture::from(storage.get_directory())
        .await
        .map_err(|e| format!("OPFS unavailable: {}", js_str(e)))?;
    let root: FileSystemDirectoryHandle = root
        .dyn_into()
        .map_err(|_| "OPFS root is not a directory".to_string())?;
    let opts = FileSystemGetDirectoryOptions::new();
    opts.set_create(true);
    let dir = JsFuture::from(root.get_directory_handle_with_options("packs", &opts))
        .await
        .map_err(js_str)?;
    dir.dyn_into()
        .map_err(|_| "packs/ is not a directory".to_string())
}

async fn file_handle(
    dir: &FileSystemDirectoryHandle,
    name: &str,
    create: bool,
) -> Result<FileSystemFileHandle, String> {
    let opts = FileSystemGetFileOptions::new();
    opts.set_create(create);
    let h = JsFuture::from(dir.get_file_handle_with_options(name, &opts))
        .await
        .map_err(js_str)?;
    h.dyn_into().map_err(|_| "not a file".to_string())
}

async fn file_of(handle: &FileSystemFileHandle) -> Result<web_sys::File, String> {
    JsFuture::from(handle.get_file())
        .await
        .map_err(js_str)?
        .dyn_into()
        .map_err(|_| "not a File".to_string())
}

/// Size of `name` in the packs dir, `None` when absent.
async fn file_size(dir: &FileSystemDirectoryHandle, name: &str) -> Option<u64> {
    let handle = file_handle(dir, name, false).await.ok()?;
    let file = file_of(&handle).await.ok()?;
    Some(file.size() as u64)
}

async fn remove_file(dir: &FileSystemDirectoryHandle, name: &str) {
    let _ = JsFuture::from(dir.remove_entry(name)).await;
}

/// The file's full bytes as a transferable `ArrayBuffer` (what the worklet's
/// `attach_pack` takes).
async fn read_bytes(
    dir: &FileSystemDirectoryHandle,
    name: &str,
) -> Result<js_sys::ArrayBuffer, String> {
    let handle = file_handle(dir, name, false).await?;
    let file = file_of(&handle).await?;
    JsFuture::from(file.array_buffer())
        .await
        .map_err(js_str)?
        .dyn_into()
        .map_err(|_| "not an ArrayBuffer".to_string())
}

/// Streaming sha256 of an OPFS file (8 MB slices — never the whole piano in
/// one Rust-side buffer).
async fn sha256_file(dir: &FileSystemDirectoryHandle, name: &str) -> Result<String, String> {
    use sha2::Digest as _;
    let handle = file_handle(dir, name, false).await?;
    let file = file_of(&handle).await?;
    let total = file.size();
    let mut hasher = sha2::Sha256::new();
    let step = 8.0 * 1024.0 * 1024.0;
    let mut at = 0.0;
    while at < total {
        let end = (at + step).min(total);
        let slice = file.slice_with_f64_and_f64(at, end).map_err(js_str)?;
        let buf = JsFuture::from(slice.array_buffer()).await.map_err(js_str)?;
        hasher.update(Uint8Array::new(&buf).to_vec());
        at = end;
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// An append writer over a `.part` file that commits every [`FLUSH_BYTES`]
/// (OPFS writables only persist on close — see the module docs).
struct PartWriter {
    dir: FileSystemDirectoryHandle,
    name: String,
    stream: FileSystemWritableFileStream,
    committed: u64,
    pending: u64,
}

impl PartWriter {
    async fn open(dir: &FileSystemDirectoryHandle, name: &str, at: u64) -> Result<Self, String> {
        let stream = Self::stream_at(dir, name, at).await?;
        Ok(PartWriter {
            dir: dir.clone(),
            name: name.to_string(),
            stream,
            committed: at,
            pending: 0,
        })
    }

    async fn stream_at(
        dir: &FileSystemDirectoryHandle,
        name: &str,
        at: u64,
    ) -> Result<FileSystemWritableFileStream, String> {
        let handle = file_handle(dir, name, true).await?;
        let opts = FileSystemCreateWritableOptions::new();
        opts.set_keep_existing_data(true);
        let stream = JsFuture::from(handle.create_writable_with_options(&opts))
            .await
            .map_err(js_str)?;
        let stream: FileSystemWritableFileStream =
            stream.dyn_into().map_err(|_| "not a writable".to_string())?;
        JsFuture::from(stream.seek_with_f64(at as f64).map_err(js_str)?)
            .await
            .map_err(js_str)?;
        Ok(stream)
    }

    async fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        let p = self
            .stream
            .write_with_u8_array(bytes)
            .map_err(js_str)?;
        JsFuture::from(p).await.map_err(js_str)?;
        self.pending += bytes.len() as u64;
        if self.pending >= FLUSH_BYTES {
            self.commit().await?;
        }
        Ok(())
    }

    /// Close (persisting everything written) and reopen at the new end.
    async fn commit(&mut self) -> Result<(), String> {
        JsFuture::from(self.stream.close()).await.map_err(js_str)?;
        self.committed += self.pending;
        self.pending = 0;
        self.stream = Self::stream_at(&self.dir, &self.name, self.committed).await?;
        Ok(())
    }

    /// Final close.
    async fn finish(mut self) -> Result<u64, String> {
        JsFuture::from(self.stream.close()).await.map_err(js_str)?;
        self.committed += self.pending;
        Ok(self.committed)
    }
}

/// Rename `from` → `to` in the packs dir: `FileSystemHandle.move()` where
/// the browser has it (Chromium), else copy + delete.
async fn rename(
    dir: &FileSystemDirectoryHandle,
    from: &str,
    to: &str,
) -> Result<(), String> {
    let handle = file_handle(dir, from, false).await?;
    if let Ok(mv) = Reflect::get(handle.as_ref(), &JsValue::from_str("move"))
        && let Some(mv) = mv.dyn_ref::<js_sys::Function>()
        && let Ok(p) = mv.call1(handle.as_ref(), &JsValue::from_str(to))
        && let Ok(p) = p.dyn_into::<js_sys::Promise>()
        && JsFuture::from(p).await.is_ok()
    {
        return Ok(());
    }
    // Fallback: copy through memory, then delete the part.
    let bytes = read_bytes(dir, from).await?;
    let dest = file_handle(dir, to, true).await?;
    let stream = JsFuture::from(dest.create_writable())
        .await
        .map_err(js_str)?;
    let stream: FileSystemWritableFileStream =
        stream.dyn_into().map_err(|_| "not a writable".to_string())?;
    JsFuture::from(
        stream
            .write_with_buffer_source(&bytes)
            .map_err(js_str)?,
    )
    .await
    .map_err(js_str)?;
    JsFuture::from(stream.close()).await.map_err(js_str)?;
    remove_file(dir, from).await;
    Ok(())
}

// ── The public flow ────────────────────────────────────────────────────────

fn final_name(want: &PackWant) -> String {
    format!("{}.{}.signalpack", want.name, want.variant)
}

/// A verified, already-downloaded pack's bytes — `None` when the cache
/// doesn't hold it (stream with [`ensure_pack`] instead). OPFS-first boot
/// path: no network touched.
pub(crate) async fn cached_pack(name: &str, variant: &str) -> Option<js_sys::ArrayBuffer> {
    let row = ledger_get(name, variant).await?;
    if row.state != "ready" {
        return None;
    }
    let dir = packs_dir().await.ok()?;
    let file = format!("{name}.{variant}.signalpack");
    let size = file_size(&dir, &file).await?;
    if row.expected_size != 0 && size != row.expected_size {
        return None;
    }
    read_bytes(&dir, &file).await.ok()
}

/// Read a cached pack from OPFS **directly into a `SharedArrayBuffer`**,
/// in chunks, so the whole pack never exists twice.
///
/// The obvious version — read the file to an ArrayBuffer, then copy it into
/// a SAB — holds BOTH buffers at peak. With the Worship set (~2.4 GB) that
/// doubles to ~4.8 GB and the tab freezes; measured 3.4 GB RSS before the
/// renderer stopped responding. Reading slice-by-slice into the SAB keeps
/// peak at the pack itself plus one chunk.
///
/// `None` when the page has no `SharedArrayBuffer` (not cross-origin
/// isolated) or the pack is not cached ready — callers fall back to the
/// ordinary transfer path.
pub(crate) async fn cached_pack_shared(name: &str, variant: &str) -> Option<JsValue> {
    let ctor = js_sys::Reflect::get(&js_sys::global(), &"SharedArrayBuffer".into()).ok()?;
    let ctor = ctor.dyn_into::<js_sys::Function>().ok()?;

    let row = ledger_get(name, variant).await?;
    if row.state != "ready" {
        return None;
    }
    let dir = packs_dir().await.ok()?;
    let name_on_disk = format!("{name}.{variant}.signalpack");
    let size = file_size(&dir, &name_on_disk).await?;
    if row.expected_size != 0 && size != row.expected_size {
        return None;
    }

    let sab = js_sys::Reflect::construct(&ctor, &Array::of1(&(size as f64).into())).ok()?;
    let dst = Uint8Array::new(&sab);
    let handle = file_handle(&dir, &name_on_disk, false).await.ok()?;
    let file: web_sys::File = JsFuture::from(handle.get_file()).await.ok()?.dyn_into().ok()?;

    // 32 MB at a time: big enough that the per-slice overhead vanishes,
    // small enough that the transient copy is irrelevant next to the pack.
    const CHUNK: f64 = 32.0 * 1024.0 * 1024.0;
    let mut at = 0f64;
    let total = size as f64;
    while at < total {
        let end = (at + CHUNK).min(total);
        let slice = file.slice_with_f64_and_f64(at, end).ok()?;
        let buf = JsFuture::from(slice.array_buffer()).await.ok()?;
        let src = Uint8Array::new(&buf);
        dst.set(&src, at as u32);
        at = end;
    }
    Some(sab)
}

/// Stream (or resume) `want` into OPFS, verify, and return the full bytes.
/// Progress lands on `on` as committed bytes; the terminal state is the
/// returned `Result`.
pub(crate) async fn ensure_pack(
    target: &EngineTarget,
    want: &PackWant,
    on: &dyn Fn(PackEvent),
) -> Result<js_sys::ArrayBuffer, String> {
    request_persist().await;
    let dir = packs_dir().await?;
    let file = final_name(want);
    let part = format!("{file}.part");
    let total = want.size_bytes;

    // Already fully there (e.g. a parallel tab finished it).
    if let Some(bytes) = cached_pack(&want.name, &want.variant).await {
        on(PackEvent::Progress { bytes: total, total });
        return Ok(bytes);
    }

    // Resume point: the committed .part on disk is the truth.
    let mut start = file_size(&dir, &part).await.unwrap_or(0);
    if start > total {
        remove_file(&dir, &part).await;
        start = 0;
    }
    let mut row = LedgerRow {
        name: want.name.clone(),
        variant: want.variant.clone(),
        expected_size: total,
        sha256: want.sha256.clone(),
        bytes: start,
        state: "streaming".into(),
    };
    ledger_put(&row).await;
    on(PackEvent::Progress { bytes: start, total });

    if start < total {
        let client: PackLibraryClient = crate::remote::establish_verbose(target)
            .await
            .map_err(|e| format!("pack host unreachable: {e}"))?;
        let mut writer = PartWriter::open(&dir, &part, start).await?;
        let (tx, mut rx) = vox::channel::<PackChunk>();
        let read_call = client.read(want.name.clone(), want.variant.clone(), start, tx);
        let mut done = start;
        let drain = async {
            while let Ok(Some(chunk)) = rx.recv().await {
                let chunk = chunk.get();
                if chunk.offset != done {
                    return Err(format!(
                        "non-contiguous chunk: got offset {} at {done}",
                        chunk.offset
                    ));
                }
                writer.write(&chunk.bytes).await?;
                done += chunk.bytes.len() as u64;
                on(PackEvent::Progress { bytes: writer.committed, total });
            }
            Ok(())
        };
        let (read_result, drain_result) = futures_util::join!(read_call, drain);
        drain_result?;
        read_result.map_err(|e| format!("read: {e:?}"))?;
        let landed = writer.finish().await?;
        row.bytes = landed;
        ledger_put(&row).await;
        if landed != total {
            return Err(format!(
                "incomplete: {landed} of {total} bytes (reload to resume)"
            ));
        }
        on(PackEvent::Progress { bytes: landed, total });
    }

    // Integrity gate, then the rename into place.
    if !want.sha256.is_empty() {
        on(PackEvent::Verifying);
        let actual = sha256_file(&dir, &part).await?;
        if actual != want.sha256 {
            remove_file(&dir, &part).await;
            row.bytes = 0;
            ledger_put(&row).await;
            return Err("sha256 mismatch — corrupt download removed, retry".into());
        }
    }
    rename(&dir, &part, &file).await?;
    row.bytes = total;
    row.state = "ready".into();
    ledger_put(&row).await;
    read_bytes(&dir, &file).await
}

/// Drop a pack from the cache (bytes + ledger). In-flight `.part`s and
/// progressive `.sparse` files (with their plan ledgers) go too.
pub(crate) async fn delete_pack(name: &str, variant: &str) -> Result<(), String> {
    let dir = packs_dir().await?;
    let file = format!("{name}.{variant}.signalpack");
    remove_file(&dir, &file).await;
    remove_file(&dir, &format!("{file}.part")).await;
    remove_file(&dir, &format!("{file}.sparse")).await;
    let _ = idb_delete(&plan_key(name, variant)).await;
    idb_delete(&ledger_key(name, variant)).await
}

// ── Progressive (W7): plan ledger + sparse OPFS file + ranged fetch ────────

/// One plan segment as persisted in the plan ledger (label dropped — it is
/// diagnostics-only and a piano plan has thousands of rows).
#[derive(Clone, Copy, Debug, Default, facet::Facet)]
pub(crate) struct SegRow {
    pub start: u64,
    pub len: u64,
    pub rank: u32,
}

/// The progressive download ledger for one pack, under key
/// `plan:<name>.<variant>`: the fetched plan plus which segments are
/// COMMITTED in the `.sparse` OPFS file. Refresh-resume = re-send the
/// received segments to the worklet from OPFS, keep fetching the rest.
#[derive(Clone, Debug, Default, facet::Facet)]
pub(crate) struct PlanLedger {
    pub name: String,
    pub variant: String,
    pub total: u64,
    pub sha256: String,
    pub segments: Vec<SegRow>,
    /// Parallel to `segments`: committed to OPFS.
    pub received: Vec<bool>,
}

impl PlanLedger {
    pub(crate) fn received_count(&self) -> usize {
        self.received.iter().filter(|r| **r).count()
    }

    /// Whether every rank-0 segment (the pack open set) is committed.
    pub(crate) fn rank0_complete(&self) -> bool {
        self.segments
            .iter()
            .zip(&self.received)
            .all(|(s, r)| s.rank != 0 || *r)
    }

    /// Same plan as `other`? (shape check for resume — a rebuilt pack
    /// invalidates the sparse cache).
    pub(crate) fn matches(&self, total: u64, segments: &[SegRow]) -> bool {
        self.total == total
            && self.segments.len() == segments.len()
            && self
                .segments
                .iter()
                .zip(segments)
                .all(|(a, b)| a.start == b.start && a.len == b.len)
    }
}

fn plan_key(name: &str, variant: &str) -> String {
    format!("plan:{name}.{variant}")
}

pub(crate) async fn plan_ledger_get(name: &str, variant: &str) -> Option<PlanLedger> {
    let json = idb_get(&plan_key(name, variant)).await.ok()??;
    facet_json::from_str(&json).ok()
}

pub(crate) async fn plan_ledger_put(ledger: &PlanLedger) {
    if let Ok(json) = facet_json::to_string(ledger) {
        let _ = idb_put(&plan_key(&ledger.name, &ledger.variant), &json).await;
    }
}

pub(crate) async fn plan_ledger_delete(name: &str, variant: &str) {
    let _ = idb_delete(&plan_key(name, variant)).await;
}

/// The pack's sparse OPFS file name (true offsets; becomes the final file
/// by rename once complete + verified).
pub(crate) fn sparse_name(name: &str, variant: &str) -> String {
    format!("{name}.{variant}.signalpack.sparse")
}

/// Drop a stale `.sparse` file (plan changed / fresh start).
pub(crate) async fn discard_sparse(name: &str, variant: &str) {
    if let Ok(dir) = packs_dir().await {
        remove_file(&dir, &sparse_name(name, variant)).await;
    }
}

/// Read `[start, start+len)` of an OPFS packs file into a Vec.
pub(crate) async fn read_file_slice(
    file_name: &str,
    start: u64,
    len: u64,
) -> Result<Vec<u8>, String> {
    let dir = packs_dir().await?;
    let handle = file_handle(&dir, file_name, false).await?;
    let file = file_of(&handle).await?;
    let slice = file
        .slice_with_f64_and_f64(start as f64, (start + len) as f64)
        .map_err(js_str)?;
    let buf = JsFuture::from(slice.array_buffer()).await.map_err(js_str)?;
    Ok(Uint8Array::new(&buf).to_vec())
}

/// A random-access writer over the `.sparse` file. Same OPFS semantics as
/// [`PartWriter`] (writables only persist on close), so it commits every
/// [`FLUSH_BYTES`]; only COMMITTED segments may be marked received in the
/// plan ledger.
pub(crate) struct SparseWriter {
    dir: FileSystemDirectoryHandle,
    name: String,
    stream: FileSystemWritableFileStream,
    pending: u64,
}

impl SparseWriter {
    pub(crate) async fn open(name: &str, variant: &str) -> Result<Self, String> {
        let dir = packs_dir().await?;
        let file = sparse_name(name, variant);
        let stream = Self::stream_of(&dir, &file).await?;
        Ok(SparseWriter { dir, name: file, stream, pending: 0 })
    }

    async fn stream_of(
        dir: &FileSystemDirectoryHandle,
        name: &str,
    ) -> Result<FileSystemWritableFileStream, String> {
        let handle = file_handle(dir, name, true).await?;
        let opts = FileSystemCreateWritableOptions::new();
        opts.set_keep_existing_data(true);
        let stream = JsFuture::from(handle.create_writable_with_options(&opts))
            .await
            .map_err(js_str)?;
        stream.dyn_into().map_err(|_| "not a writable".to_string())
    }

    /// Write `bytes` at absolute offset `start`. Returns `true` when this
    /// write triggered a commit (safe point for ledger updates).
    pub(crate) async fn write_at(&mut self, start: u64, bytes: &[u8]) -> Result<bool, String> {
        JsFuture::from(self.stream.seek_with_f64(start as f64).map_err(js_str)?)
            .await
            .map_err(js_str)?;
        let p = self.stream.write_with_u8_array(bytes).map_err(js_str)?;
        JsFuture::from(p).await.map_err(js_str)?;
        self.pending += bytes.len() as u64;
        if self.pending >= FLUSH_BYTES {
            self.commit().await?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Close (persisting everything written) and reopen.
    pub(crate) async fn commit(&mut self) -> Result<(), String> {
        JsFuture::from(self.stream.close()).await.map_err(js_str)?;
        self.pending = 0;
        self.stream = Self::stream_of(&self.dir, &self.name).await?;
        Ok(())
    }

    /// Final close.
    pub(crate) async fn finish(self) -> Result<(), String> {
        JsFuture::from(self.stream.close()).await.map_err(js_str)?;
        Ok(())
    }
}

/// Fetch one plan for `(name, variant)` from the pack host. `total` is
/// the listing's `size_bytes` — an honest plan's segments sum to exactly
/// that (the tiling invariant), which doubles as the staleness check.
pub(crate) async fn fetch_pack_plan(
    target: &EngineTarget,
    name: &str,
    variant: &str,
    total: u64,
) -> Result<Vec<signal_packs_proto::PackSegment>, String> {
    let client: PackLibraryClient = crate::remote::establish_verbose(target)
        .await
        .map_err(|e| format!("pack host unreachable: {e}"))?;
    // The plan streams as chunked JSON bytes over the VIRTUAL-NAME `read`
    // (see the proto docs): `read` is the one method signature the wasm
    // client provably drives — every dedicated new method (any reply
    // shape, sync or async) fails the browser-side phon schema compat
    // with "writer and reader schema kinds differ" while working
    // natively.
    let (tx, mut rx) = vox::channel::<PackChunk>();
    let call = client.read(format!("plan:{name}"), variant.to_string(), 0, tx);
    let mut json_bytes: Vec<u8> = Vec::new();
    let drain = async {
        let mut at = 0u64;
        while let Ok(Some(chunk)) = rx.recv().await {
            let chunk = chunk.get();
            if chunk.offset != at {
                return Err(format!("plan chunk gap at {} (expected {at})", chunk.offset));
            }
            json_bytes.extend_from_slice(&chunk.bytes);
            at += chunk.bytes.len() as u64;
        }
        Ok(())
    };
    let (call_result, drain_result) = futures_util::join!(call, drain);
    drain_result?;
    call_result.map_err(|e| format!("pack_plan: {e:?}"))?;
    let json = String::from_utf8(json_bytes).map_err(|e| format!("plan utf8: {e}"))?;
    if json.trim().is_empty() {
        return Err("pack host returned no plan for this pack".into());
    }
    let segments: Vec<signal_packs_proto::PackSegment> =
        facet_json::from_str(&json).map_err(|e| format!("pack_plan json: {e}"))?;
    if segments.is_empty() {
        return Err("pack host returned no plan for this pack".into());
    }
    let covered: u64 = segments.iter().map(|s| s.len).sum();
    if covered != total {
        return Err(format!(
            "plan covers {covered} bytes but the listing says {total} — stale listing?"
        ));
    }
    Ok(segments)
}

/// Fetch exactly `[start, start+len)` of a pack over an ESTABLISHED
/// client (callers reuse one connection across a fill session).
pub(crate) async fn read_range_to_vec(
    client: &PackLibraryClient,
    name: &str,
    variant: &str,
    start: u64,
    len: u64,
) -> Result<Vec<u8>, String> {
    // Virtual-name `read` (see `fetch_pack_plan` for why not the
    // dedicated `read_range` method from wasm).
    let (tx, mut rx) = vox::channel::<PackChunk>();
    let call = client.read(
        format!("range:{}:{name}", signal_packs_proto::PackRange { start, len }),
        variant.to_string(),
        0,
        tx,
    );
    let mut out = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
    let drain = async {
        let mut at = start;
        while let Ok(Some(chunk)) = rx.recv().await {
            let chunk = chunk.get();
            if chunk.offset != at {
                return Err(format!("non-contiguous chunk at {} (expected {at})", chunk.offset));
            }
            out.extend_from_slice(&chunk.bytes);
            at += chunk.bytes.len() as u64;
        }
        Ok(())
    };
    let (call_result, drain_result) = futures_util::join!(call, drain);
    drain_result?;
    call_result.map_err(|e| format!("read_range: {e:?}"))?;
    if out.len() as u64 != len {
        return Err(format!("short range: {} of {len} bytes", out.len()));
    }
    Ok(out)
}

/// Finalize a completed progressive download: verify the whole-file
/// sha256 (when known), rename `.sparse` → the final pack name, write the
/// ready [`LedgerRow`], and drop the plan ledger. On sha mismatch the
/// sparse file and plan ledger are removed so a retry restarts clean.
pub(crate) async fn finalize_progressive(
    name: &str,
    variant: &str,
    total: u64,
    sha256: &str,
) -> Result<(), String> {
    let dir = packs_dir().await?;
    let sparse = sparse_name(name, variant);
    let size = file_size(&dir, &sparse).await.unwrap_or(0);
    if size != total {
        return Err(format!("sparse file is {size} bytes, expected {total}"));
    }
    if !sha256.is_empty() {
        let actual = sha256_file(&dir, &sparse).await?;
        if actual != sha256 {
            remove_file(&dir, &sparse).await;
            plan_ledger_delete(name, variant).await;
            return Err("sha256 mismatch — corrupt download removed, retry".into());
        }
    }
    let file = format!("{name}.{variant}.signalpack");
    rename(&dir, &sparse, &file).await?;
    ledger_put(&LedgerRow {
        name: name.to_string(),
        variant: variant.to_string(),
        expected_size: total,
        sha256: sha256.to_string(),
        bytes: total,
        state: "ready".into(),
    })
    .await;
    plan_ledger_delete(name, variant).await;
    Ok(())
}
