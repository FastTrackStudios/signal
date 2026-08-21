//! Page-side handle to the keys rig's DECODER WORKER (W12 of
//! crates/signal/docs/browser-keys-rig.md).
//!
//! The AudioWorkletGlobalScope is single-threaded — its message handlers
//! run on the audio rendering thread — so the worklet must NEVER decode.
//! The decoder worker is a second instance of the same
//! `signal-keys-worklet` wasm module in a plain module Worker: it mirrors
//! the worklet's lane program and pack attachments, decodes samples
//! off-thread (reading pack bytes from the SAME OPFS files `web_packs`
//! maintains), and ships raw PCM straight to the worklet over a
//! `MessageChannel` the page wires up at boot (`warmPort` — the page main
//! thread is not in the warm loop).
//!
//! The page keeps ONE live handle in a thread-local (like the backend):
//! the boot sequence replaces it on a latency-hint re-boot (terminating
//! the old worker), and the attach/fill paths reach it from wherever they
//! run. Ranges the worker cannot read from OPFS yet surface through
//! [`drain_net_misses`] and feed the same bump-the-network-queue path as
//! the worklet's own read misses.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use js_sys::{Array, Object, Reflect};
use wasm_bindgen::JsCast as _;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;

use crate::web_keys_rig::{WORKLET_GLUE_URL, WORKLET_WASM_URL};

const WORKER_URL: &str = "/worklet/keys_decoder_worker.js";

fn js_str(e: JsValue) -> String {
    e.as_string().unwrap_or_else(|| format!("{e:?}"))
}

type Pending = Rc<RefCell<HashMap<u32, futures_channel::oneshot::Sender<JsValue>>>>;

/// The live decoder worker + the replyTo/value RPC over its own port.
#[derive(Clone)]
pub(crate) struct DecoderWorker {
    worker: web_sys::Worker,
    pending: Pending,
    next_id: Rc<Cell<u32>>,
    _onmessage: Rc<Closure<dyn FnMut(web_sys::MessageEvent)>>,
}

thread_local! {
    /// The one live worker (replaced on a re-boot; see [`install`]).
    static DECODER: RefCell<Option<DecoderWorker>> = const { RefCell::new(None) };
    /// Ranges the worker needed but OPFS could not serve: `(pack id,
    /// start, len)` — drained by the progressive fill loop, which bumps
    /// them to the network queue front exactly like worklet read misses.
    static NET_MISSES: RefCell<Vec<(u32, u64, u64)>> = const { RefCell::new(Vec::new()) };
}

/// The live worker handle, if one is booted.
pub(crate) fn current() -> Option<DecoderWorker> {
    DECODER.with(|d| d.borrow().clone())
}

/// Drain the worker's un-serveable ranges (see [`NET_MISSES`]).
pub(crate) fn drain_net_misses() -> Vec<(u32, u64, u64)> {
    NET_MISSES.with(|m| m.borrow_mut().drain(..).collect())
}

/// Boot a fresh decoder worker, terminating any previous one, and return
/// the `MessagePort` the WORKLET's init must carry as `warmPort` (the
/// other end goes to the worker here). `sample_rate` follows the
/// AudioContext so both wasm instances resolve identical lane programs.
pub(crate) fn install(sample_rate: f64) -> Result<web_sys::MessagePort, String> {
    // Terminate the previous worker first — its warmPort partner died
    // with the old worklet, and a zombie worker would keep decoding.
    DECODER.with(|d| {
        if let Some(old) = d.borrow_mut().take() {
            old.worker.terminate();
        }
    });

    let channel = web_sys::MessageChannel::new().map_err(|e| js_str(e))?;
    let worklet_port = channel.port1();
    let worker_port = channel.port2();

    let opts = web_sys::WorkerOptions::new();
    opts.set_type(web_sys::WorkerType::Module);
    let worker = web_sys::Worker::new_with_options(WORKER_URL, &opts)
        .map_err(|e| format!("decoder worker: {}", js_str(e)))?;

    let pending: Pending = Rc::new(RefCell::new(HashMap::new()));
    let p2 = pending.clone();
    let onmessage = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
        move |ev: web_sys::MessageEvent| {
            let data = ev.data();
            if let Ok(reply_to) = Reflect::get(&data, &"replyTo".into())
                && let Some(id) = reply_to.as_f64()
            {
                if let Some(tx) = p2.borrow_mut().remove(&(id as u32)) {
                    let value =
                        Reflect::get(&data, &"value".into()).unwrap_or(JsValue::UNDEFINED);
                    let _ = tx.send(value);
                }
                return;
            }
            let kind = Reflect::get(&data, &"kind".into())
                .ok()
                .and_then(|k| k.as_string())
                .unwrap_or_default();
            match kind.as_str() {
                "net_miss" => {
                    let id = Reflect::get(&data, &"id".into())
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0) as u32;
                    if let Ok(ranges) =
                        Reflect::get(&data, &"ranges".into()).map(|v| Array::from(&v))
                    {
                        NET_MISSES.with(|m| {
                            let mut m = m.borrow_mut();
                            for r in ranges.iter() {
                                let start = Reflect::get(&r, &"start".into())
                                    .ok()
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0) as u64;
                                let len = Reflect::get(&r, &"len".into())
                                    .ok()
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0) as u64;
                                if m.len() < 256 {
                                    m.push((id, start, len));
                                }
                            }
                        });
                    }
                }
                "error" => {
                    let err = Reflect::get(&data, &"error".into())
                        .ok()
                        .and_then(|v| v.as_string())
                        .unwrap_or_default();
                    let during = Reflect::get(&data, &"during".into())
                        .ok()
                        .and_then(|v| v.as_string())
                        .unwrap_or_default();
                    tracing::warn!("keys decoder worker error during {during}: {err}");
                }
                _ => {}
            }
        },
    );
    worker.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

    let handle = DecoderWorker {
        worker: worker.clone(),
        pending,
        next_id: Rc::new(Cell::new(1)),
        _onmessage: Rc::new(onmessage),
    };

    // init — the worker fetches its own wasm (workers can fetch; the
    // worklet scope cannot) and serializes every later message behind it.
    let init = Object::new();
    let set = |o: &Object, k: &str, v: &JsValue| {
        let _ = Reflect::set(o, &k.into(), v);
    };
    set(&init, "kind", &"init".into());
    set(&init, "glueUrl", &WORKLET_GLUE_URL.into());
    set(&init, "wasmUrl", &WORKLET_WASM_URL.into());
    set(&init, "sampleRate", &sample_rate.into());
    set(&init, "warmPort", worker_port.as_ref());
    worker
        .post_message_with_transfer(&init, &Array::of1(worker_port.as_ref()))
        .map_err(|e| format!("decoder init post: {}", js_str(e)))?;

    DECODER.with(|d| *d.borrow_mut() = Some(handle));
    Ok(worklet_port)
}

impl DecoderWorker {
    fn obj(kind: &str) -> Object {
        let o = Object::new();
        let _ = Reflect::set(&o, &"kind".into(), &kind.into());
        o
    }

    fn set(o: &Object, key: &str, v: &JsValue) {
        let _ = Reflect::set(o, &key.into(), v);
    }

    fn fire(&self, o: &Object) {
        let _ = self.worker.post_message(o);
    }

    async fn rpc(&self, o: Object) -> Result<JsValue, String> {
        use futures_util::FutureExt as _;
        let id = self.next_id.get();
        self.next_id.set(id.wrapping_add(1));
        let (tx, rx) = futures_channel::oneshot::channel();
        self.pending.borrow_mut().insert(id, tx);
        Self::set(&o, "replyTo", &id.into());
        self.worker
            .post_message(&o)
            .map_err(|e| format!("decoder post: {}", js_str(e)))?;
        // A worker that failed to load never replies — the rig must not
        // hang on it (it still PLAYS, just without off-thread warms).
        futures_util::select! {
            v = rx.fuse() => v.map_err(|_| "decoder reply dropped".to_string()),
            _ = architect::platform::sleep(std::time::Duration::from_secs(60)).fuse() => {
                self.pending.borrow_mut().remove(&id);
                Err("decoder worker unresponsive (60 s)".to_string())
            }
        }
    }

    /// Mirror `open_lanes` (same program JSON the worklet got).
    pub(crate) async fn open_lanes(&self, program_json: &str) -> Result<(), String> {
        let o = Self::obj("open_lanes");
        Self::set(&o, "program", &program_json.into());
        let v = self.rpc(o).await?;
        if v.as_f64().is_some() {
            Ok(())
        } else {
            Err(format!("decoder open_lanes: {}", js_str(v)))
        }
    }

    /// Mirror `reload_lanes` after a pack attach.
    pub(crate) async fn reload_lanes(&self) {
        let _ = self.rpc(Self::obj("reload_lanes")).await;
    }

    /// A fully-cached / fully-streamed pack: the worker reads any range of
    /// its OPFS file. `id` is page-allocated (never the processor's own
    /// whole-buffer counter).
    pub(crate) async fn attach_pack(&self, key: &str, id: u32, opfs: &str, len: u64) {
        let o = Self::obj("attach_pack");
        Self::set(&o, "key", &key.into());
        Self::set(&o, "id", &id.into());
        Self::set(&o, "opfs", &opfs.into());
        Self::set(&o, "len", &JsValue::from_f64(len as f64));
        let _ = self.rpc(o).await;
    }

    /// A progressive pack's sparse OPFS file: only `ranges` (committed
    /// `[start, len]` pairs) are readable — a hole reads as zeros, which
    /// must never reach a decode.
    pub(crate) async fn attach_pack_progressive(
        &self,
        key: &str,
        id: u32,
        opfs: &str,
        len: u64,
        ranges: &[(u64, u64)],
    ) {
        let o = Self::obj("attach_pack_progressive");
        Self::set(&o, "key", &key.into());
        Self::set(&o, "id", &id.into());
        Self::set(&o, "opfs", &opfs.into());
        Self::set(&o, "len", &JsValue::from_f64(len as f64));
        Self::set(&o, "ranges", &ranges_array(ranges).into());
        let _ = self.rpc(o).await;
    }

    /// Refresh a progressive pack's committed ranges (after an OPFS
    /// commit) — unblocks decodes that were waiting on those bytes.
    pub(crate) fn pack_ranges(&self, id: u32, ranges: &[(u64, u64)]) {
        let o = Self::obj("pack_ranges");
        Self::set(&o, "id", &id.into());
        Self::set(&o, "ranges", &ranges_array(ranges).into());
        self.fire(&o);
    }

    /// (Re)start the background coverage fill, middle-out from `center`.
    ///
    /// NOT called during boot. Since a streamed (ogg proxy) sample counts as
    /// non-resident, coverage means FULLY decoding every sample of every
    /// lane — gigabytes of work that saturates the decoder, thrashes the
    /// decoded-PCM budget (shed → re-warm → shed), and competes with the
    /// pack streaming the player is waiting on. On-demand warms cover what
    /// is actually played, which is what a live rig needs. Kept for a future
    /// idle-time prefetch, which must be paced and budget-aware.
    #[allow(dead_code)]
    pub(crate) fn coverage(&self, center: u8) {
        let o = Self::obj("coverage");
        Self::set(&o, "center", &u32::from(center).into());
        self.fire(&o);
    }
}

fn ranges_array(ranges: &[(u64, u64)]) -> Array {
    let arr = Array::new();
    for (start, len) in ranges {
        arr.push(&Array::of2(
            &JsValue::from_f64(*start as f64),
            &JsValue::from_f64(*len as f64),
        ));
    }
    arr
}
