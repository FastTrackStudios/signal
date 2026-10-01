//! **Lane health** — per lane, the notes cut short and why: voices the CPU
//! guard faded (and whether the load or the voice count asked for it), notes
//! a polyphony limit stole, the most voices it held. For a rig's health log:
//! "a note ended early" is only fixable once it says which lane and which
//! mechanism.
//!
//! The audio thread counts without a lock or an allocation: the lane's
//! instrument points [`CURRENT`] at its counters for the duration of its
//! block, and the voice pools deep inside the render tree bump whatever it
//! points at. The counters are registered (control side, once per lane
//! instrument) so a reader can walk them.

use std::cell::Cell;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex, Weak};

/// One lane's counters since the last [`LaneHealth::take`].
#[derive(Debug, Default)]
pub struct LaneHealth {
    pub name: String,
    /// Voices the CPU guard faded.
    faded: AtomicU64,
    /// Blocks the guard shed in because the rig ran hot for a while…
    shed_by_load: AtomicU64,
    /// …or because the rig held more voices than its budget.
    shed_by_count: AtomicU64,
    /// Blocks the guard cut a held note in (a real overrun, or far more
    /// voices than the deadline holds).
    cut_blocks: AtomicU64,
    /// Notes a polyphony limit stole.
    stolen: AtomicU64,
    /// The most voices this lane held in one block.
    peak_voices: AtomicU32,
}

/// A window's reading of one lane.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaneReport {
    pub name: String,
    pub faded: u64,
    pub shed_by_load: u64,
    pub shed_by_count: u64,
    pub cut_blocks: u64,
    pub stolen: u64,
    pub peak_voices: u32,
}

impl LaneReport {
    /// Anything cut short in the window.
    #[must_use]
    pub fn cut_anything(&self) -> bool {
        self.faded > 0 || self.stolen > 0
    }
}

impl LaneHealth {
    /// Counters for lane `name`, registered for [`reports`].
    #[must_use]
    pub fn register(name: &str) -> Arc<Self> {
        let h = Arc::new(Self {
            name: name.to_string(),
            ..Self::default()
        });
        let mut all = REGISTRY.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        all.retain(|w| w.strong_count() > 0);
        all.push(Arc::downgrade(&h));
        h
    }

    /// The guard shed this block, for this reason.
    pub fn shed(&self, by_load: bool) {
        if by_load {
            self.shed_by_load.fetch_add(1, Relaxed);
        } else {
            self.shed_by_count.fetch_add(1, Relaxed);
        }
    }

    /// The guard cut a held note this block.
    pub fn cut(&self) {
        self.cut_blocks.fetch_add(1, Relaxed);
    }

    /// The lane held `voices` this block.
    pub fn voices(&self, voices: u32) {
        self.peak_voices.fetch_max(voices, Relaxed);
    }

    /// This window's counts, and start the next.
    pub fn take(&self) -> LaneReport {
        LaneReport {
            name: self.name.clone(),
            faded: self.faded.swap(0, Relaxed),
            shed_by_load: self.shed_by_load.swap(0, Relaxed),
            shed_by_count: self.shed_by_count.swap(0, Relaxed),
            cut_blocks: self.cut_blocks.swap(0, Relaxed),
            stolen: self.stolen.swap(0, Relaxed),
            peak_voices: self.peak_voices.swap(0, Relaxed),
        }
    }
}

static REGISTRY: Mutex<Vec<Weak<LaneHealth>>> = Mutex::new(Vec::new());

/// Every live lane's window, taken (control side).
#[must_use]
pub fn reports() -> Vec<LaneReport> {
    let all = REGISTRY.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    all.iter().filter_map(Weak::upgrade).map(|h| h.take()).collect()
}

thread_local! {
    /// The lane the audio thread is rendering, if a lane instrument set it.
    static CURRENT: Cell<*const LaneHealth> = const { Cell::new(std::ptr::null()) };
}

/// While alive, voice pools on this thread count into `health`.
pub struct Scope(());

impl Scope {
    /// Point this thread's counting at `health` until the scope drops. The
    /// caller keeps `health` alive for the scope (it holds the `Arc`).
    #[must_use]
    pub fn enter(health: &LaneHealth) -> Self {
        CURRENT.with(|c| c.set(std::ptr::from_ref(health)));
        Self(())
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        CURRENT.with(|c| c.set(std::ptr::null()));
    }
}

fn with_current(f: impl FnOnce(&LaneHealth)) {
    CURRENT.with(|c| {
        // SAFETY: set only by `Scope::enter` from a reference the caller
        // keeps alive for the scope, and cleared when the scope drops.
        if let Some(h) = unsafe { c.get().as_ref() } {
            f(h);
        }
    });
}

/// `n` voices faded by the guard, in the current lane.
pub fn faded(n: usize) {
    if n > 0 {
        with_current(|h| {
            h.faded.fetch_add(n as u64, Relaxed);
        });
    }
}

/// A note stolen at a polyphony limit, in the current lane.
pub fn stolen() {
    with_current(|h| {
        h.stolen.fetch_add(1, Relaxed);
    });
}
