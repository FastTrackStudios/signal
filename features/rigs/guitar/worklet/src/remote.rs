//! A block whose processing runs on another thread — a NAM model on a Web
//! Worker, reached over shared memory.
//!
//! The render thread cannot run six A2 models: one costs about half a
//! 128-frame quantum in wasm. So a model the render thread cannot afford runs
//! on a worker, and a [`RemoteBlock`] stands in its slot. The block talks to
//! the worker through a [`Link`] — on the web a `SharedArrayBuffer` with an
//! input, an output and two sequence counters; natively (tests) a thread.
//!
//! Two ways to wait, per block ([`Lag`]):
//!
//! - [`Lag::Zero`] — post this quantum's input and wait for its output.
//!   Adds no latency, and buys no time unless something else runs meanwhile:
//!   that is Amp R, whose input (the dry guitar) is known when Amp L starts,
//!   so the host [`prefetch`](RemoteBlock::prefetch)es it there and the two
//!   amps run at the same time.
//! - [`Lag::One`] — return the output of the *previous* quantum, then post
//!   this one. The worker gets a whole quantum to run in, concurrently with
//!   everything else, for one quantum (2.7 ms at 48 kHz) of latency.
//!
//! A worker that misses its deadline never stalls the render thread: the
//! wait is bounded, the block plays silence for that quantum, and the miss
//! is counted so the host can re-plan (a slimmer model, or a lag).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

/// How many parameter changes one post can carry. More in one quantum is
/// not a thing a player does; the rest wait for the next quantum.
pub const MAX_POSTED_PARAMS: usize = 8;

/// The far end of a [`RemoteBlock`].
pub trait Link: Send {
    /// Hand the worker one quantum of input (and any parameter changes).
    fn post(&mut self, in_l: &[f32], in_r: &[f32], params: &[(u32, f64)]);
    /// Whether the output for the last post has arrived.
    fn ready(&self) -> bool;
    /// Copy the last post's output out. Only meaningful after [`ready`](Self::ready).
    fn take(&mut self, out_l: &mut [f32], out_r: &mut [f32]);
    /// Wait for [`ready`](Self::ready), at most about `budget_us`
    /// microseconds. Returns whether it arrived.
    fn wait(&self, budget_us: u32) -> bool;
}

/// See the module docs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lag {
    Zero,
    One,
}

/// Counters a host reads to decide whether its plan is keeping up.
#[derive(Debug, Default)]
pub struct RemoteStats {
    /// Quanta whose output did not arrive in time.
    pub misses: AtomicU32,
    /// Quanta served.
    pub quanta: AtomicU32,
}

struct Inner {
    link: Box<dyn Link>,
    /// A post is outstanding for this quantum (prefetched, or lag-one's).
    posted: bool,
    /// Parameter changes waiting for the next post.
    params: Vec<(u32, f64)>,
}

/// A slot whose block runs on a worker. See the module docs.
pub struct RemoteBlock {
    name: String,
    lag: Lag,
    inner: Arc<Mutex<Inner>>,
    stats: Arc<RemoteStats>,
    /// How long a zero-lag wait may spin before the quantum is given up.
    budget_us: u32,
    latency: u32,
}

/// A handle that posts a [`RemoteBlock`]'s input early (see
/// [`RemoteBlock::prefetch`]).
#[derive(Clone)]
pub struct Prefetch(Arc<Mutex<Inner>>);

impl Prefetch {
    /// Post `in_l`/`in_r` now; the block's own `process_block` then only
    /// collects. A no-op if something is already outstanding.
    pub fn post(&self, in_l: &[f32], in_r: &[f32]) {
        let Ok(mut s) = self.0.try_lock() else { return };
        if s.posted {
            return;
        }
        let s = &mut *s;
        s.link.post(in_l, in_r, &s.params);
        s.params.clear();
        s.posted = true;
    }
}

/// Keeps a bypassed [`RemoteBlock`] warm (see [`RemoteBlock::standby`]).
#[derive(Clone)]
pub struct Standby(Arc<Mutex<Inner>>);

impl Standby {
    /// Hand the worker this quantum's input if it is idle; never wait. The
    /// output is not needed — only the model's state, so that switching it
    /// on is seamless. A busy worker skips the quantum, which a bypassed
    /// model can afford: the audio thread must never spin for it (browsers
    /// render quanta in bursts, so "a quantum later" can be well under a
    /// quantum of wall time).
    pub fn feed(&self, in_l: &[f32], in_r: &[f32]) {
        let Ok(mut s) = self.0.try_lock() else { return };
        if s.posted && !s.link.ready() {
            return;
        }
        let s = &mut *s;
        s.link.post(in_l, in_r, &s.params);
        s.params.clear();
        s.posted = true;
    }
}

impl RemoteBlock {
    /// `quantum` is the host's block size, so a lag-one block can report its
    /// latency in frames.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        link: Box<dyn Link>,
        lag: Lag,
        budget_us: u32,
        quantum: u32,
    ) -> Self {
        Self {
            name: name.into(),
            lag,
            inner: Arc::new(Mutex::new(Inner {
                link,
                posted: false,
                params: Vec::with_capacity(MAX_POSTED_PARAMS),
            })),
            stats: Arc::new(RemoteStats::default()),
            budget_us,
            latency: if lag == Lag::One { quantum } else { 0 },
        }
    }

    #[must_use]
    pub fn lag(&self) -> Lag {
        self.lag
    }

    #[must_use]
    pub fn stats(&self) -> Arc<RemoteStats> {
        self.stats.clone()
    }

    /// For a lag-one block: the handle that keeps it warm while bypassed.
    #[must_use]
    pub fn standby(&self) -> Option<Standby> {
        (self.lag == Lag::One).then(|| Standby(self.inner.clone()))
    }

    /// Only a zero-lag block can be prefetched: a lag-one block already
    /// has a whole quantum.
    #[must_use]
    pub fn prefetch(&self) -> Option<Prefetch> {
        (self.lag == Lag::Zero).then(|| Prefetch(self.inner.clone()))
    }
}

fn silence(out_l: &mut [f32], out_r: &mut [f32]) {
    out_l.fill(0.0);
    out_r.fill(0.0);
}

impl PluginInstance for RemoteBlock {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "signal.rig.remote".into(),
            name: self.name.clone(),
            vendor: "FastTrackStudio".into(),
            version: String::new(),
            format: PluginFormat::Clap,
        }
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        Vec::new()
    }
    fn param_value(&mut self, _: u32) -> Option<f64> {
        None
    }
    fn value_to_text(&mut self, _: u32, _: f64) -> Option<String> {
        None
    }
    fn text_to_value(&mut self, _: u32, _: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        self.latency
    }
    fn prepare(&mut self, _: f64, _: u32) -> Result<(), PluginError> {
        Ok(())
    }
    fn is_prepared(&self) -> bool {
        true
    }
    fn deactivate(&mut self) {}

    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        let Ok(mut guard) = self.inner.try_lock() else {
            silence(out_l, out_r);
            return Ok(());
        };
        let s = &mut *guard;
        for &p in events.params {
            if s.params.len() < MAX_POSTED_PARAMS {
                s.params.push(p);
            }
        }
        self.stats.quanta.fetch_add(1, Ordering::Relaxed);
        match self.lag {
            Lag::Zero => {
                if !s.posted {
                    s.link.post(in_l, in_r, &s.params);
                    s.params.clear();
                }
                s.posted = false;
                if s.link.wait(self.budget_us) {
                    s.link.take(out_l, out_r);
                } else {
                    self.stats.misses.fetch_add(1, Ordering::Relaxed);
                    silence(out_l, out_r);
                }
            }
            Lag::One => {
                if s.posted {
                    // The worker had a whole quantum; waiting here is only
                    // for a worker that is late, and stays bounded.
                    if s.link.wait(self.budget_us) {
                        s.link.take(out_l, out_r);
                    } else {
                        self.stats.misses.fetch_add(1, Ordering::Relaxed);
                        silence(out_l, out_r);
                    }
                } else {
                    silence(out_l, out_r);
                }
                s.link.post(in_l, in_r, &s.params);
                s.params.clear();
                s.posted = true;
            }
        }
        Ok(())
    }
}

/// A [`Link`] to a thread in the same process — the native stand-in for a
/// Web Worker, so the protocol is tested without a browser.
#[cfg(not(target_arch = "wasm32"))]
pub mod thread {
    use super::Link;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    use signal_plugin_host::{PluginEvents, PluginInstance};

    struct Shared {
        seq_in: AtomicU32,
        seq_out: AtomicU32,
        stop: AtomicBool,
        io: Mutex<Io>,
    }

    #[derive(Default)]
    struct Io {
        in_l: Vec<f32>,
        in_r: Vec<f32>,
        out_l: Vec<f32>,
        out_r: Vec<f32>,
        params: Vec<(u32, f64)>,
    }

    pub struct ThreadLink {
        shared: Arc<Shared>,
        seq: u32,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl ThreadLink {
        /// Run `block` on its own thread; `delay` stands in for a slow model.
        #[must_use]
        pub fn spawn(mut block: Box<dyn PluginInstance>, delay: std::time::Duration) -> Self {
            let shared = Arc::new(Shared {
                seq_in: AtomicU32::new(0),
                seq_out: AtomicU32::new(0),
                stop: AtomicBool::new(false),
                io: Mutex::new(Io::default()),
            });
            let s = shared.clone();
            let thread = std::thread::spawn(move || {
                let mut done = 0;
                while !s.stop.load(Ordering::Acquire) {
                    let seq = s.seq_in.load(Ordering::Acquire);
                    if seq == done {
                        std::thread::yield_now();
                        continue;
                    }
                    std::thread::sleep(delay);
                    let mut io = s.io.lock().unwrap();
                    let io = &mut *io;
                    let n = io.in_l.len();
                    io.out_l.resize(n, 0.0);
                    io.out_r.resize(n, 0.0);
                    let params = std::mem::take(&mut io.params);
                    let events = PluginEvents {
                        params: &params,
                        ..PluginEvents::default()
                    };
                    let _ = block.process_block(
                        &io.in_l,
                        &io.in_r,
                        &mut io.out_l,
                        &mut io.out_r,
                        &events,
                    );
                    done = seq;
                    s.seq_out.store(seq, Ordering::Release);
                }
            });
            Self {
                shared,
                seq: 0,
                thread: Some(thread),
            }
        }
    }

    impl Drop for ThreadLink {
        fn drop(&mut self) {
            self.shared.stop.store(true, Ordering::Release);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    impl Link for ThreadLink {
        fn post(&mut self, in_l: &[f32], in_r: &[f32], params: &[(u32, f64)]) {
            {
                let mut io = self.shared.io.lock().unwrap();
                io.in_l.clear();
                io.in_l.extend_from_slice(in_l);
                io.in_r.clear();
                io.in_r.extend_from_slice(in_r);
                io.params.extend_from_slice(params);
            }
            self.seq = self.seq.wrapping_add(1);
            self.shared.seq_in.store(self.seq, Ordering::Release);
        }
        fn ready(&self) -> bool {
            self.shared.seq_out.load(Ordering::Acquire) == self.seq
        }
        fn take(&mut self, out_l: &mut [f32], out_r: &mut [f32]) {
            let io = self.shared.io.lock().unwrap();
            let n = out_l.len().min(io.out_l.len());
            out_l[..n].copy_from_slice(&io.out_l[..n]);
            out_r[..n].copy_from_slice(&io.out_r[..n]);
        }
        fn wait(&self, budget_us: u32) -> bool {
            let until =
                std::time::Instant::now() + std::time::Duration::from_micros(u64::from(budget_us));
            while !self.ready() {
                if std::time::Instant::now() >= until {
                    return false;
                }
                std::hint::spin_loop();
            }
            true
        }
    }
}
