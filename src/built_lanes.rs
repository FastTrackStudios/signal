//! Lane instruments compiled OFF the audio thread, handed over by pointer.
//!
//! Building a lane's instrument — compiling the tree, opening its zones —
//! is the last expensive thing the browser rig does on the audio thread
//! (measured 62 ms per pack attach, ~23 render quanta). It cannot move to
//! another thread as a whole, because `KeysRig` is `!Send` on wasm: it
//! holds a vox sink that is single-threaded there.
//!
//! But the expensive PART is `RenderNode`, and that IS `Send`. So a worker
//! compiles the tree and enqueues it here; the audio thread picks it up and
//! calls [`KeysInstrument::begin_swap`](crate::KeysInstrument::begin_swap),
//! which is two moves. Because the wasm heap is shared, the `Box` a worker
//! allocated is simply valid on the audio thread — nothing is serialized
//! and nothing is copied.
//!
//! The same mechanism is what makes a PATCH CHANGE gapless: swapping a
//! patch is exactly "build a tree, then install it", and `begin_swap` keeps
//! the outgoing tree sounding until its voices finish.

use core::sync::atomic::{AtomicUsize, Ordering};

use crate::node_render::{GainCells, RenderNode};

/// One compiled lane waiting to be installed.
pub struct BuiltLane {
    /// Which layer it belongs to (matches `LaneLayer::name`).
    pub layer: String,
    /// The compiled tree, ready to render.
    pub render: RenderNode,
    /// Its live fader cells (the mixer's handles on the new tree).
    pub cells: GainCells,
}

/// Small ring: a handful of lanes can be in flight, never a backlog. Past
/// the cap the oldest is dropped — a stale compile is worth nothing, and
/// the producer must never block (it may be a worker the audio thread is
/// waiting on).
const RING: usize = 32;
const MASK: usize = RING - 1;

static SLOTS: [AtomicUsize; RING] = [const { AtomicUsize::new(0) }; RING];
static HEAD: AtomicUsize = AtomicUsize::new(0);
static TAIL: AtomicUsize = AtomicUsize::new(0);
static BUILT: AtomicUsize = AtomicUsize::new(0);
static INSTALLED: AtomicUsize = AtomicUsize::new(0);

/// Hand a compiled lane to the audio thread. Worker side.
pub fn publish(lane: BuiltLane) {
    let ptr = Box::into_raw(Box::new(lane)) as usize;
    let idx = HEAD.fetch_add(1, Ordering::AcqRel) & MASK;
    let prev = SLOTS[idx].swap(ptr, Ordering::Release);
    if prev != 0 {
        // Ring wrapped past an uncollected slot: drop that compile rather
        // than leak it.
        unsafe { drop(Box::from_raw(prev as *mut BuiltLane)) };
    }
    BUILT.fetch_add(1, Ordering::Relaxed);
}

/// Take the next compiled lane, if any. **Audio-thread safe**: an atomic
/// load and a pointer move, no allocation, no lock.
pub fn take() -> Option<Box<BuiltLane>> {
    loop {
        let tail = TAIL.load(Ordering::Relaxed);
        if tail == HEAD.load(Ordering::Acquire) {
            return None;
        }
        if TAIL
            .compare_exchange_weak(
                tail,
                tail.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_err()
        {
            continue;
        }
        let ptr = SLOTS[tail & MASK].swap(0, Ordering::AcqRel);
        if ptr == 0 {
            continue; // producer claimed the index but has not stored yet
        }
        INSTALLED.fetch_add(1, Ordering::Relaxed);
        return Some(unsafe { Box::from_raw(ptr as *mut BuiltLane) });
    }
}

// ── The reaper ─────────────────────────────────────────────────────────
//
// The audio thread must never FREE, for the same reason it must never
// allocate: `dlmalloc` in a threaded wasm build is guarded by a lock in
// the shared heap, and the worklet thread runs at REAL-TIME priority while
// the workers do not. An RT thread spinning on a lock held by a
// normal-priority worker is priority inversion, and on Linux there is no
// priority inheritance to rescue it — the renderer simply stops. This is
// the documented way people get graph-swapping wrong (see
// browser-keys-rig.md W15).
//
// So a retired tree is not dropped on the audio thread. It goes back into
// the very `Box` its replacement arrived in — reusing that allocation, so
// the swap neither allocates nor frees — and a worker drops it later.

static REAP_SLOTS: [AtomicUsize; RING] = [const { AtomicUsize::new(0) }; RING];
static REAP_HEAD: AtomicUsize = AtomicUsize::new(0);
static REAP_TAIL: AtomicUsize = AtomicUsize::new(0);
static REAPED: AtomicUsize = AtomicUsize::new(0);

/// Hand a retired lane back for a worker to drop. **Audio-thread safe**:
/// a pointer store, no allocation and no free.
pub fn retire(lane: Box<BuiltLane>) {
    let ptr = Box::into_raw(lane) as usize;
    let idx = REAP_HEAD.fetch_add(1, Ordering::AcqRel) & MASK;
    let prev = REAP_SLOTS[idx].swap(ptr, Ordering::Release);
    if prev != 0 {
        // Ring wrapped past an uncollected slot. Dropping HERE would be the
        // very thing this exists to avoid, so leak it instead — bounded by
        // the ring size, and only reachable if the reaper stopped running.
        REAP_SLOTS[idx].store(prev, Ordering::Release);
    }
}

/// Drop everything retired. **Workers only** — this frees.
pub fn reap() -> usize {
    let mut n = 0;
    loop {
        let tail = REAP_TAIL.load(Ordering::Relaxed);
        if tail == REAP_HEAD.load(Ordering::Acquire) {
            return n;
        }
        if REAP_TAIL
            .compare_exchange_weak(
                tail,
                tail.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_err()
        {
            continue;
        }
        let ptr = REAP_SLOTS[tail & MASK].swap(0, Ordering::AcqRel);
        if ptr == 0 {
            continue;
        }
        // The actual free, on a normal-priority thread.
        unsafe { drop(Box::from_raw(ptr as *mut BuiltLane)) };
        REAPED.fetch_add(1, Ordering::Relaxed);
        n += 1;
    }
}

/// Lanes dropped off the audio thread.
pub fn reaped() -> usize {
    REAPED.load(Ordering::Relaxed)
}

/// Whether anything is waiting — checked per quantum, so it must stay a
/// single atomic load.
#[inline]
pub fn has_pending() -> bool {
    HEAD.load(Ordering::Acquire) != TAIL.load(Ordering::Acquire)
}

/// Lanes compiled off-thread / installed on the audio thread, since boot.
pub fn built() -> usize {
    BUILT.load(Ordering::Relaxed)
}

pub fn installed() -> usize {
    INSTALLED.load(Ordering::Relaxed)
}
