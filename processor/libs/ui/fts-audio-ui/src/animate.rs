//! Whether painted visualisers should move.
//!
//! A visualiser that animates keeps the window redrawing, whole, every
//! frame. When nothing is being processed (the audio engine is stopped)
//! there is nothing to show moving, so a host provides [`Animate`] near the
//! root and the visualisers' clocks read it through [`animating`]. With no
//! [`Animate`] provided, they move, as before.

use dioxus::prelude::*;

/// Provide this to say when visualisers may move: `true` while audio runs.
#[derive(Clone, Copy)]
pub struct Animate(pub Signal<bool>);

/// Whether this scope's visualiser may move. Subscribes the scope, so it
/// re-renders (and restarts its clock) when that changes.
#[must_use]
pub fn animating() -> bool {
    try_consume_context::<Animate>().is_none_or(|a| (a.0)())
}

/// Re-render this component every `period` while `active` and the host
/// lets visualisers move ([`animating`]), for as long as it is mounted.
///
/// A picture that moves by itself (a decaying tail, a sweeping LFO) changes
/// nothing in the DOM, so its clock has to come from outside: a thread
/// poking the runtime (`schedule_update` is safe off it). Inactive, the
/// thread stays but pokes nothing; unmounted, it ends.
#[cfg(not(target_arch = "wasm32"))]
pub fn use_frame_clock(active: bool, period: std::time::Duration) {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
    struct Clock {
        running: Arc<AtomicBool>,
        active: Arc<AtomicBool>,
    }
    impl Drop for Clock {
        fn drop(&mut self) {
            self.running.store(false, Relaxed);
        }
    }
    let active = active && animating();
    let clock = use_hook(|| {
        let updater = dioxus::dioxus_core::schedule_update();
        let running = Arc::new(AtomicBool::new(true));
        let ticking = Arc::new(AtomicBool::new(active));
        let (alive, on) = (Arc::clone(&running), Arc::clone(&ticking));
        std::thread::spawn(move || {
            while alive.load(Relaxed) {
                std::thread::sleep(period);
                if on.load(Relaxed) {
                    updater();
                }
            }
        });
        std::rc::Rc::new(Clock { running, active: ticking })
    });
    clock.active.store(active, Relaxed);
}

/// In a browser the canvas runs its own clock.
#[cfg(target_arch = "wasm32")]
pub fn use_frame_clock(_active: bool, _period: std::time::Duration) {}
