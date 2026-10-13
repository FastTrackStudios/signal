//! **Lane pool** — a realtime renderer's worker threads, so the tracks of
//! a block that depend on nothing but their own input (instrument lanes:
//! no children, no received sends) run their FX chains on several cores at
//! once. Everything after — faders, sends, folder sums, folder FX — stays
//! on the render thread in topo order.
//!
//! The render thread never waits on a lock or a wakeup: it publishes a
//! batch with one atomic store, unparks the workers (a semaphore signal,
//! no syscall when they are still spinning), and takes jobs itself like any
//! worker. It only waits for the last job running elsewhere, spinning —
//! the same wait a serial render would have spent on that job.
//!
//! Claims are one compare-exchange on a word holding the batch's epoch, the
//! next job and the job count, so a worker that wakes late for a finished
//! batch can never take a job of the next one: its claim fails on the epoch.

#![allow(unsafe_code)]

use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::{Stamp, TrackSnapshot};

/// How long a worker spins for the next batch before parking. A block is
/// ~2.7 ms at 128 frames, so a worker parks between blocks and a batch
/// pays one wakeup; spinning covers back-to-back batches cheaply.
const SPIN: Duration = Duration::from_micros(60);

/// The batch being run: the job function lives on the render thread's
/// stack for the batch, so a worker reads it only after a successful claim
/// (which the render thread outlives by waiting for every claimed job).
type JobFn = *const (dyn Fn(usize) + Sync);

struct Shared {
    /// epoch (32) | next job (16) | job count (16).
    state: AtomicU64,
    /// Jobs finished in the current batch.
    done: AtomicUsize,
    job: UnsafeCell<Option<JobFn>>,
    quit: AtomicBool,
}

// SAFETY: `job` is written only by the render thread between batches (no
// claim can succeed then) and read by a worker only after a claim that the
// render thread waits out before writing it again.
unsafe impl Sync for Shared {}
unsafe impl Send for Shared {}

const fn pack(epoch: u32, next: u16, count: u16) -> u64 {
    ((epoch as u64) << 32) | ((next as u64) << 16) | count as u64
}

const fn unpack(s: u64) -> (u32, u16, u16) {
    ((s >> 32) as u32, (s >> 16) as u16, s as u16)
}

impl Shared {
    /// Take the next job of batch `epoch`, if there is one left.
    fn claim(&self, epoch: u32) -> Option<usize> {
        let mut s = self.state.load(Ordering::Acquire);
        loop {
            let (e, next, count) = unpack(s);
            if e != epoch || next >= count {
                return None;
            }
            match self.state.compare_exchange_weak(
                s,
                pack(e, next + 1, count),
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(next as usize),
                Err(now) => s = now,
            }
        }
    }

    /// Run jobs of `epoch` until none are left.
    fn work(&self, epoch: u32) {
        while let Some(i) = self.claim(epoch) {
            // SAFETY: a claim succeeded, so the batch's job is published and
            // stays alive until this job is counted done.
            // A panic must still count the job done, or the render thread
            // waits forever; jobs catch their plugins' panics themselves.
            if let Some(f) = unsafe { *self.job.get() } {
                let _ =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe { (*f)(i) }));
            }
            self.done.fetch_add(1, Ordering::Release);
        }
    }
}

pub(crate) struct LanePool {
    shared: Arc<Shared>,
    workers: Vec<std::thread::JoinHandle<()>>,
    epoch: std::cell::Cell<u32>,
}

// SAFETY: `epoch` is touched only by `run`, which only the render thread
// calls (the renderer holds its scratch lock across it).
unsafe impl Sync for LanePool {}

impl LanePool {
    /// `threads` workers (the render thread makes one more).
    pub(crate) fn new(threads: usize) -> Self {
        let shared = Arc::new(Shared {
            state: AtomicU64::new(0),
            done: AtomicUsize::new(0),
            job: UnsafeCell::new(None),
            quit: AtomicBool::new(false),
        });
        let workers = (0..threads)
            .filter_map(|i| {
                let shared = shared.clone();
                std::thread::Builder::new()
                    .name(format!("render-lane-{i}"))
                    .spawn(move || worker(&shared))
                    .ok()
            })
            .collect();
        Self {
            shared,
            workers,
            epoch: std::cell::Cell::new(0),
        }
    }

    /// Worker threads (not counting the render thread).
    pub(crate) fn threads(&self) -> usize {
        self.workers.len()
    }

    /// Run `job(0..count)` across the workers and this thread; returns when
    /// every job has finished. Realtime-safe: no lock, no allocation.
    pub(crate) fn run(&self, count: usize, job: &(dyn Fn(usize) + Sync)) {
        let count = count.min(u16::MAX as usize);
        if count == 0 {
            return;
        }
        let epoch = self.epoch.get().wrapping_add(1);
        self.epoch.set(epoch);
        // SAFETY: no claim can succeed now (the last batch is finished), so
        // no worker reads `job`. The pointer's lifetime is erased; this call
        // outlives every use (it waits for every job below).
        unsafe {
            let f: JobFn = std::mem::transmute::<&(dyn Fn(usize) + Sync), JobFn>(job);
            *self.shared.job.get() = Some(f);
        }
        self.shared.done.store(0, Ordering::Relaxed);
        self.shared
            .state
            .store(pack(epoch, 0, count as u16), Ordering::Release);
        for w in &self.workers {
            w.thread().unpark();
        }
        self.shared.work(epoch);
        while self.shared.done.load(Ordering::Acquire) < count {
            std::hint::spin_loop();
        }
    }
}

impl Drop for LanePool {
    fn drop(&mut self) {
        self.shared.quit.store(true, Ordering::Release);
        for w in &self.workers {
            w.thread().unpark();
        }
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}

fn worker(shared: &Shared) {
    #[cfg(feature = "audio")]
    let mut seat = audio::io::rt_workers::WorkerSeat::default();
    let mut seen = unpack(shared.state.load(Ordering::Acquire)).0;
    let mut idle_since = Instant::now();
    loop {
        if shared.quit.load(Ordering::Acquire) {
            return;
        }
        let epoch = unpack(shared.state.load(Ordering::Acquire)).0;
        if epoch != seen {
            seen = epoch;
            #[cfg(feature = "audio")]
            seat.sync();
            shared.work(epoch);
            idle_since = Instant::now();
            continue;
        }
        if idle_since.elapsed() < SPIN {
            std::hint::spin_loop();
        } else {
            // An unpark between the check above and here is not lost: the
            // park returns at once.
            std::thread::park_timeout(Duration::from_millis(100));
        }
    }
}

/// Worker count for a realtime renderer: `FTS_RENDER_THREADS` if set (0
/// renders serially), else a few of the performance cores — the render
/// thread is one more, and the UI and the sample streamer need the rest.
pub(crate) fn default_threads() -> usize {
    if let Some(n) = std::env::var("FTS_RENDER_THREADS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
    {
        return n.min(15);
    }
    let cores = std::thread::available_parallelism().map_or(1, usize::from);
    (cores / 3).clamp(0, 3)
}

/// A plugin on a lane job — the map's entry, reached from a worker.
pub(super) struct PluginPtr(*mut dyn crate::plugin::PluginInstance);

impl PluginPtr {
    pub(super) fn new(p: &mut (dyn crate::plugin::PluginInstance + 'static)) -> Self {
        Self(p)
    }
}
// SAFETY: plugins are `Send`; a job's pointers are its own track's, used by
// one thread at a time while the render thread holds the map's lock.
unsafe impl Send for PluginPtr {}

/// One independent track's FX chain, to run on any render thread: its own
/// buffers and events, so jobs share nothing.
#[derive(Default)]
pub(super) struct LaneJob {
    pub(super) ti: usize,
    /// `(chain index, plugin)` of every plugin that runs, in chain order.
    pub(super) chain: Vec<(usize, PluginPtr)>,
    /// The track's bus (interleaved stereo), as an address.
    pub(super) bus: usize,
    /// The track's aux buffer ([`super::AUX_PAIRS`] pairs), as an
    /// address; 0 when nothing sends from it.
    pub(super) aux: usize,
    /// A plugin put something on an aux output.
    pub(super) aux_ran: bool,
    pub(super) in_l: Vec<f32>,
    pub(super) in_r: Vec<f32>,
    pub(super) out_l: Vec<f32>,
    pub(super) out_r: Vec<f32>,
    pub(super) midi: Vec<crate::plugin::PluginMidiEvent>,
    /// Where the block starts, seconds: what its FX automation is read at.
    pub(super) at: f64,
    /// One FX's parameters, its automation applied (reused across blocks).
    pub(super) params: Vec<(u32, f64)>,
    pub(super) note_expr: Vec<crate::plugin::PluginNoteExpression>,
    /// A plugin processed (the bus may now carry signal).
    pub(super) ran: bool,
    /// The chain index of a plugin that panicked.
    pub(super) panicked: Option<usize>,
    pub(super) us: u32,
}

impl LaneJob {
    /// Run the chain. `context` is the render thread's render frame.
    ///
    /// # Safety
    /// `bus` and every plugin pointer are live and used by no other thread
    /// for the duration.
    unsafe fn run(&mut self, t: &TrackSnapshot, frames: usize, context: Option<(u64, u64)>) {
        let started = Stamp::now();
        let _frame = (crate::plugin::RenderFrameGuard::current() != context)
            .then(|| crate::plugin::RenderFrameGuard::adopt(context));
        let _muted = crate::plugin::TrackMutedGuard::enter(t.muted);
        // SAFETY: the caller's contract.
        let bus = unsafe { std::slice::from_raw_parts_mut(self.bus as *mut f32, frames * 2) };
        for (i, PluginPtr(plugin)) in &self.chain {
            // SAFETY: the caller's contract.
            let plugin = unsafe { &mut **plugin };
            for f in 0..frames {
                self.in_l[f] = bus[f * 2];
                self.in_r[f] = bus[f * 2 + 1];
            }
            self.out_l[..frames].fill(0.0);
            self.out_r[..frames].fill(0.0);
            let fixed = t.fx_params.get(*i).map(Vec::as_slice).unwrap_or(&[]);
            let params = match t.fx_envs.get(*i) {
                Some(envs) if !envs.is_empty() => {
                    super::envelope::params_at(fixed, envs, self.at, &mut self.params);
                    self.params.as_slice()
                }
                _ => fixed,
            };
            let events = crate::plugin::PluginEvents {
                params,
                midi: &self.midi,
                note_expressions: &self.note_expr,
            };
            let (in_l, in_r) = (&self.in_l[..frames], &self.in_r[..frames]);
            let (out_l, out_r) = (&mut self.out_l[..frames], &mut self.out_r[..frames]);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                plugin.process_block(in_l, in_r, out_l, out_r, &events)
            }));
            match result {
                Ok(Ok(())) => {}
                Ok(Err(_)) => continue,
                Err(_) => {
                    self.panicked.get_or_insert(*i);
                    continue;
                }
            }
            for f in 0..frames {
                bus[f * 2] = self.out_l[f];
                bus[f * 2 + 1] = self.out_r[f];
            }
            if self.aux != 0 {
                // SAFETY: the caller's contract, as for `bus`.
                let aux = unsafe {
                    std::slice::from_raw_parts_mut(
                        self.aux as *mut f32,
                        super::AUX_PAIRS * frames * 2,
                    )
                };
                self.aux_ran |= super::add_aux(plugin, aux, frames);
            }
            self.ran = true;
        }
        self.us = started.us();
    }
}

impl LanePool {
    /// Run every job's chain across the pool; returns when all are done.
    pub(super) fn run_lanes(&self, jobs: &mut [LaneJob], tracks: &[TrackSnapshot], frames: usize) {
        let base = jobs.as_mut_ptr() as usize;
        let context = crate::plugin::RenderFrameGuard::current();
        self.run(jobs.len(), &|k| {
            // SAFETY: job `k` runs once, on one thread; its bus and plugins
            // are its own track's (a track's plugins are its own), and the
            // caller holds the scratch and the plugin map until this returns.
            let job = unsafe { &mut *(base as *mut LaneJob).add(k) };
            unsafe { job.run(&tracks[job.ti], frames, context) };
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    #[test]
    fn every_job_runs_once_per_batch() {
        let pool = LanePool::new(3);
        let hits: Vec<AtomicU32> = (0..40).map(|_| AtomicU32::new(0)).collect();
        for _ in 0..2_000 {
            pool.run(hits.len(), &|i| {
                hits[i].fetch_add(1, Ordering::Relaxed);
            });
        }
        assert!(hits.iter().all(|h| h.load(Ordering::Relaxed) == 2_000));
    }

    #[test]
    fn a_batch_sees_what_was_written_before_it_and_is_seen_after() {
        let pool = LanePool::new(2);
        let mut data = vec![0u64; 16];
        for round in 1..500u64 {
            let ptr = data.as_mut_ptr() as usize;
            pool.run(16, &move |i| {
                // SAFETY: each job its own slot.
                let slot = unsafe { &mut *(ptr as *mut u64).add(i) };
                assert_eq!(*slot, round - 1);
                *slot = round;
            });
            assert!(data.iter().all(|&v| v == round));
        }
    }
}
