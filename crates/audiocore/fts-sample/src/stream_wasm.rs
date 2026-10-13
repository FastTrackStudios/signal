//! The wasm **streamer queue** — the browser's stand-in for the native
//! streamer thread pool (W13 of `crates/signal/docs/browser-keys-rig.md`).
//!
//! Same contract as the native pool in [`super::stream`]: the audio thread
//! marks which chunks it wants and returns IMMEDIATELY; decoding happens
//! somewhere else and the decoded chunks appear in memory the audio thread
//! already reads. What differs is only how "somewhere else" is spelled.
//!
//! `wasm32-unknown-unknown` has no `std::thread::spawn` even with atomics,
//! so the workers are Web Workers the PAGE spawns, each instantiating the
//! same module over the SAME shared `WebAssembly.Memory`. Because the heap
//! is shared, a worker can operate on an `Arc<StreamedSample>` the audio
//! thread created — it only needs the pointer. So the queue carries raw
//! pointers to leaked `Arc`s, and a worker reclaims each one after filling.
//!
//! Everything here is lock-free by necessity, not preference: `atomic.wait`
//! TRAPS on the audio thread (and on the main thread), so the producer side
//! may never block. Producers push into a fixed ring with a CAS.
//!
//! The PARKING is deliberately not done here. Rust's `memory_atomic_wait32`
//! /`notify` intrinsics are nightly-only (`stdarch_wasm_atomic_wait`), and
//! this crate compiles on STABLE for every native target — a `#![feature]`
//! attribute would break all of them to serve one wasm build. So the worker
//! parks in JS instead: [`wake_addr`] hands out the byte address of a word
//! in this heap, the worker calls `Atomics.wait` on it with a short timeout,
//! and then calls [`drain`]. A timed wait needs no notify at all, which also
//! means the audio thread never has to call out to JS to wake anyone — it
//! just pushes and returns.

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

use super::stream::StreamedSample;

/// Ring capacity — power of two so the index mask is a bitand. A backlog
/// this deep already means the decoders are hopelessly behind; further
/// pushes are DROPPED rather than blocking the audio thread, and the sample
/// re-requests on its next read anyway (the wanted bitmask is idempotent).
const RING: usize = 1024;
const MASK: usize = RING - 1;

/// Slots hold `Arc::into_raw` pointers (0 = empty).
static SLOTS: [AtomicUsize; RING] = [const { AtomicUsize::new(0) }; RING];
/// Producer / consumer cursors. Monotonic; indices are `& MASK`.
static HEAD: AtomicUsize = AtomicUsize::new(0);
static TAIL: AtomicUsize = AtomicUsize::new(0);
/// Futex word workers park on. Bumped on every push.
static WAKE: AtomicU32 = AtomicU32::new(0);
/// Pushes dropped because the ring was full — a decoders-are-behind signal
/// worth surfacing, not a silent failure.
static DROPPED: AtomicUsize = AtomicUsize::new(0);

/// Queue `sample` for chunk filling. **Audio-thread safe**: a CAS and a
/// notify, no allocation, no blocking, and it never fails in a way the
/// caller must handle (a full ring drops, and the request survives in the
/// sample's own wanted bitmask).
pub fn enqueue(sample: &Arc<StreamedSample>) {
    let head = HEAD.load(Ordering::Relaxed);
    let tail = TAIL.load(Ordering::Acquire);
    if head.wrapping_sub(tail) >= RING {
        DROPPED.fetch_add(1, Ordering::Relaxed);
        return;
    }
    // Claim a slot.
    let idx = match HEAD.compare_exchange_weak(
        head,
        head.wrapping_add(1),
        Ordering::AcqRel,
        Ordering::Relaxed,
    ) {
        Ok(_) => head & MASK,
        Err(_) => {
            // Contended: another producer took the index. Re-check
            // fullness before claiming — a bare fetch_add here let HEAD
            // run more than RING ahead of TAIL under contention, which
            // silently overwrites live slots.
            let head = HEAD.load(Ordering::Relaxed);
            if head.wrapping_sub(TAIL.load(Ordering::Acquire)) >= RING {
                DROPPED.fetch_add(1, Ordering::Relaxed);
                return;
            }
            HEAD.fetch_add(1, Ordering::AcqRel) & MASK
        }
    };
    let ptr = Arc::into_raw(Arc::clone(sample)) as usize;
    let prev = SLOTS[idx].swap(ptr, Ordering::Release);
    if prev != 0 {
        // Overwrote an unconsumed slot (ring wrapped past a stalled
        // worker): drop that reference rather than leak it.
        unsafe { drop(Arc::from_raw(prev as *const StreamedSample)) };
        DROPPED.fetch_add(1, Ordering::Relaxed);
    }
    wake_one();
}

/// Take one queued sample, or `None` when the ring is empty.
fn dequeue() -> Option<Arc<StreamedSample>> {
    loop {
        let tail = TAIL.load(Ordering::Relaxed);
        if tail == HEAD.load(Ordering::Acquire) {
            return None;
        }
        // Read the slot BEFORE claiming it. The producer reserves its index
        // and stores a moment later, so a slot can be legitimately empty
        // while HEAD is already past it. Advancing TAIL first (what this
        // did originally) stepped over that item permanently: nothing ever
        // revisits it, and because `StreamedSample::queued` stays true
        // until `fill` runs, that sample is never re-queued either — the
        // voice reads silence past its head for the rest of the session.
        let ptr = SLOTS[tail & MASK].load(Ordering::Acquire);
        if ptr == 0 {
            return None; // not stored yet — leave TAIL where it is
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
            continue; // another consumer took it; look again
        }
        // We own this index now; clear it.
        SLOTS[tail & MASK].store(0, Ordering::Release);
        return Some(unsafe { Arc::from_raw(ptr as *const StreamedSample) });
    }
}

/// Publish that there is work. Workers observe this word (they park on it
/// with a timeout in JS), so no notify intrinsic is needed — and the audio
/// thread stays free of any call into JS.
fn wake_one() {
    WAKE.fetch_add(1, Ordering::Release);
}

/// Byte address of the word workers park on, for `Atomics.wait` from JS.
/// Valid only within this module's shared memory.
pub fn wake_addr() -> u32 {
    WAKE.as_ptr() as u32
}

/// The word's current value — the `expected` argument for `Atomics.wait`,
/// so a worker never sleeps through work queued between its check and its
/// wait (the standard futex race guard).
pub fn wake_value() -> u32 {
    WAKE.load(Ordering::Acquire)
}

/// Drain everything currently queued, decoding each sample's wanted chunks.
/// Returns how many samples were filled. Safe to call from a worker only.
pub fn drain() -> usize {
    let mut n = 0;
    // Zone opens FIRST: a note is waiting on one of these (it is silent
    // until its zone exists), whereas a chunk fill is a voice already
    // sounding that has audio buffered ahead of it.
    while let Some(job) = dequeue_open() {
        // Decodes the head and inserts into the cache the AUDIO THREAD
        // reads — same map, shared heap, no handoff. An error means the
        // pack bytes are not there yet; the next press re-queues it.
        if job.cache.get(&job.path).is_ok() {
            OPENED.fetch_add(1, Ordering::Relaxed);
        } else {
            OPEN_FAILED.fetch_add(1, Ordering::Relaxed);
        }
        n += 1;
    }
    while let Some(sample) = dequeue() {
        sample.fill_wanted_off_thread();
        n += 1;
    }
    n
}

// ── Zone-open jobs ─────────────────────────────────────────────────────
//
// The other half of the work, and the reason this queue exists at all now:
// a note whose zone was never OPENED needs `cache.get(path)` — decode the
// head, insert it into the cache map. On the audio thread that measured
// ~26 ms per zone. `SampleCache` is `Send + Sync` and lives in this shared
// heap, so a streamer worker can do it and the audio thread SEES THE
// RESULT with no copy and no message: same map, same memory.
//
// (This is what replaces the earlier decoder-worker protocol, where a
// second wasm instance with its OWN heap decoded and shipped PCM back
// through a MessagePort — correct, but every sample crossed as a copy.)

/// One "open this zone" job: which cache, which sample.
struct OpenJob {
    cache: super::cache::SampleCache,
    path: std::path::PathBuf,
}

static OPEN_SLOTS: [AtomicUsize; RING] = [const { AtomicUsize::new(0) }; RING];
static OPEN_HEAD: AtomicUsize = AtomicUsize::new(0);
static OPEN_TAIL: AtomicUsize = AtomicUsize::new(0);
static OPENED: AtomicUsize = AtomicUsize::new(0);
/// Open jobs a worker DEQUEUED but whose `cache.get` failed. Separates
/// "the workers never ran" (both zero) from "they ran and could not do
/// the work" (failures climbing) — `OPENED` alone cannot tell them apart.
static OPEN_FAILED: AtomicUsize = AtomicUsize::new(0);

/// Queue a zone to be opened off-thread. **Audio-thread safe**: one
/// allocation for the job box and a CAS. Duplicates are harmless — the
/// second open finds it cached and returns.
pub fn enqueue_open(cache: &super::cache::SampleCache, path: &std::path::Path) {
    let head = OPEN_HEAD.load(Ordering::Relaxed);
    if head.wrapping_sub(OPEN_TAIL.load(Ordering::Acquire)) >= RING {
        DROPPED.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let job = Box::into_raw(Box::new(OpenJob {
        cache: cache.clone_handle(),
        path: path.to_path_buf(),
    })) as usize;
    let idx = OPEN_HEAD.fetch_add(1, Ordering::AcqRel) & MASK;
    let prev = OPEN_SLOTS[idx].swap(job, Ordering::Release);
    if prev != 0 {
        unsafe { drop(Box::from_raw(prev as *mut OpenJob)) };
        DROPPED.fetch_add(1, Ordering::Relaxed);
    }
    wake_one();
}

fn dequeue_open() -> Option<Box<OpenJob>> {
    loop {
        let tail = OPEN_TAIL.load(Ordering::Relaxed);
        if tail == OPEN_HEAD.load(Ordering::Acquire) {
            return None;
        }
        let probe = OPEN_SLOTS[tail & MASK].load(Ordering::Acquire);
        if probe == 0 {
            return None; // reserved but not stored yet — see `dequeue`
        }
        if OPEN_TAIL
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
        let ptr = OPEN_SLOTS[tail & MASK].load(Ordering::Acquire);
        if ptr == 0 {
            return None; // reserved but not stored yet — see `dequeue`
        }
        OPEN_SLOTS[tail & MASK].store(0, Ordering::Release);
        return Some(unsafe { Box::from_raw(ptr as *mut OpenJob) });
    }
}

/// Zones opened off-thread since boot.
pub fn opened() -> usize {
    OPENED.load(Ordering::Relaxed)
}

/// Open jobs that were dequeued but failed to open.
pub fn open_failed() -> usize {
    OPEN_FAILED.load(Ordering::Relaxed)
}

/// Open jobs still queued (distinct from the chunk-ring [`depth`]).
pub fn open_depth() -> usize {
    OPEN_HEAD
        .load(Ordering::Relaxed)
        .wrapping_sub(OPEN_TAIL.load(Ordering::Relaxed))
}

/// Samples dropped because the ring was full — decoders falling behind.
pub fn dropped() -> usize {
    DROPPED.load(Ordering::Relaxed)
}

/// Queue depth right now (diagnostic).
pub fn depth() -> usize {
    HEAD.load(Ordering::Relaxed)
        .wrapping_sub(TAIL.load(Ordering::Relaxed))
}
