//! **Drag throttling** — a knob feels continuous; the wire does not have to be.
//!
//! A pointer drag fires 100+ moves a second, and in this rig every one of them
//! is a round trip: the write reaches the engine, the engine republishes the
//! whole mixer, the band re-fetches its detail and the view re-renders. One
//! second of turning a cutoff was a few hundred of those, and the UI stopped
//! keeping up — the knob appeared to freeze while it was in fact drowning.
//!
//! So sends are coalesced: the newest value goes out on a ~40 ms tick (25 a
//! second, faster than a fader move can be heard as steps) and the rest are
//! dropped, because a superseded intermediate value is worth nothing. The
//! release always sends the final position, so what the engine ends up
//! holding is exactly where the knob was let go — no throttle can lose the
//! value that matters.
//!
//! Visual position is untouched: it follows the pointer every frame, from
//! local state. Only the traffic is rationed.

use std::time::Duration;

use dioxus::prelude::*;

/// How often a held drag is allowed to send.
const TICK_MS: u64 = 40;

/// A coalescing sink for drag values.
#[derive(Clone, Copy)]
pub struct Throttled {
    /// The newest value not yet sent.
    pending: Signal<Option<f32>>,
    /// A flush loop is already running.
    inflight: Signal<bool>,
    sink: Callback<f32>,
}

impl Throttled {
    /// Offer a value. It is sent on the next tick, replacing anything queued
    /// behind it.
    pub fn queue(&self, value: f32) {
        let mut pending = self.pending;
        pending.set(Some(value));
        let mut inflight = self.inflight;
        if *inflight.peek() {
            return;
        }
        inflight.set(true);
        let me = *self;
        spawn(async move {
            loop {
                architect::platform::sleep(Duration::from_millis(TICK_MS)).await;
                let next = { me.pending.clone().write().take() };
                match next {
                    Some(v) => me.sink.call(v),
                    None => {
                        // Nothing arrived this tick: the drag has stopped, so
                        // stop ticking rather than spinning behind it.
                        me.inflight.clone().set(false);
                        break;
                    }
                }
            }
        });
    }

    /// Send whatever is queued immediately — the end of a drag, where the
    /// exact final value matters.
    pub fn flush(&self) {
        let next = { self.pending.clone().write().take() };
        if let Some(v) = next {
            self.sink.call(v);
        }
    }
}

/// Wrap a control's `on_change` in a throttle.
pub fn use_throttle(on_change: EventHandler<f32>) -> Throttled {
    let pending = use_signal(|| None::<f32>);
    let inflight = use_signal(|| false);
    let sink = use_hook(|| Callback::new(move |v: f32| on_change.call(v)));
    Throttled { pending, inflight, sink }
}
