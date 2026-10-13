//! The values the UI owns while a finger is on them.
//!
//! A fader cannot wait for the engine. Round-tripping every pixel of a drag
//! through a service — over a WebSocket, in the browser's case — puts the
//! cap behind the pointer by however long the trip takes, and the user feels
//! it as the control being loose. So the in-flight value belongs to the UI:
//! the drag writes a **draft**, the control renders the draft, and the
//! engine is told separately.
//!
//! That inverts the usual direction and buys two problems, both handled
//! here.
//!
//! **The engine must not be shouted at.** A 60fps drag is 60 writes a
//! second, which is invisible in-process and wasteful over a socket. Drafts
//! are therefore *coalesced*: [`Drafts::take_dirty`] hands out the latest
//! value per track and nothing in between, and a sync loop calls it on a
//! timer. The trailing edge matters more than the rate — whatever the timing,
//! the last value a user chose is the one that must land, which is why a
//! draft stays dirty until it has actually been taken rather than being
//! dropped when the drag ends.
//!
//! **The engine echoes.** Set a volume and the backend sends `VolumeChanged`
//! back, including for your own write. Applying that mid-drag drops the cap
//! back to wherever the engine had got to, and the fader fights the finger —
//! two steps forward, one back, for the length of the gesture. While a draft
//! exists the store therefore *ignores* inbound volume for that track
//! ([`TrackStore::apply`][crate::controls::TrackStore::apply] consults
//! [`Drafts::holds`]), and the draft is retired a beat after the last write
//! lands, by which time the echo of that write has been and gone.
//!
//! Nothing here knows what a fader is. A draft is "a value the UI is holding
//! for this track", and volume is simply the first control that needs one.

use std::collections::HashMap;

use crate::prelude::*;

/// One value the UI is holding, and how far through its life it is.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Draft {
    value: f64,
    /// Changed since the last flush — the engine has not been told yet.
    dirty: bool,
    /// The gesture is over. The draft outlives it by one retire pass, so
    /// the echo of the final write lands while it is still suppressed.
    released: bool,
}

/// Which of a track's continuous values a draft is for.
///
/// One map keyed by both, rather than a map per field: a control holding a
/// value is the same mechanism whatever the value is, and the sync loop
/// should not grow a branch each time a control learns to drag.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Held {
    Volume,
    Pan,
}

/// Every value the UI is currently holding, by track guid and field.
#[derive(Clone, Copy, PartialEq, Default)]
pub struct Drafts {
    held: Signal<HashMap<(String, Held), Draft>>,
}

impl Drafts {
    /// Must be called inside a component scope — it owns a Signal.
    pub fn new() -> Self {
        Self {
            held: Signal::new(HashMap::new()),
        }
    }

    /// The value to render for this track's `what`, if the UI is holding one.
    pub fn get(&self, guid: &str, what: Held) -> Option<f64> {
        self.held
            .read()
            .get(&(guid.to_string(), what))
            .map(|d| d.value)
    }

    /// Is this value being held — and so should an inbound event for it be
    /// ignored?
    pub fn holds(&self, guid: &str, what: Held) -> bool {
        self.held.read().contains_key(&(guid.to_string(), what))
    }

    /// The value to render for this track's volume, if held.
    pub fn volume(&self, guid: &str) -> Option<f64> {
        self.get(guid, Held::Volume)
    }

    /// The value to render for this track's pan, if held.
    pub fn pan(&self, guid: &str) -> Option<f64> {
        self.get(guid, Held::Pan)
    }

    /// Record where the user has just dragged to.
    ///
    /// Re-entering a released draft revives it: a user who grabs the cap
    /// again before the retire pass is still dragging, and dropping the
    /// suppression under them would let the previous write's echo through
    /// mid-gesture.
    pub fn set(&mut self, guid: &str, what: Held, value: f64) {
        self.held
            .write()
            .entry((guid.to_string(), what))
            .and_modify(|d| {
                d.value = value;
                d.dirty = true;
                d.released = false;
            })
            .or_insert(Draft {
                value,
                dirty: true,
                released: false,
            });
    }

    /// Record where the user has just dragged this track's volume to.
    pub fn set_volume(&mut self, guid: &str, value: f64) {
        self.set(guid, Held::Volume, value);
    }

    /// Record where the user has just dragged this track's pan to.
    pub fn set_pan(&mut self, guid: &str, value: f64) {
        self.set(guid, Held::Pan, value);
    }

    /// The gesture ended. The draft stays — and stays suppressing — until
    /// its last value has been written and retired.
    pub fn release(&mut self, guid: &str, what: Held) {
        if let Some(d) = self.held.write().get_mut(&(guid.to_string(), what)) {
            d.released = true;
        }
    }

    pub fn release_volume(&mut self, guid: &str) {
        self.release(guid, Held::Volume);
    }

    pub fn release_pan(&mut self, guid: &str) {
        self.release(guid, Held::Pan);
    }

    /// Every value the engine has not been told about yet, newest only.
    ///
    /// Coalescing lives here rather than in the caller: a drag that moved
    /// forty times between two flushes yields one write, of the value the
    /// user actually stopped on.
    pub fn take_dirty(&mut self) -> Vec<(String, Held, f64)> {
        let mut out = Vec::new();
        for ((guid, what), d) in self.held.write().iter_mut() {
            if d.dirty {
                d.dirty = false;
                out.push((guid.clone(), *what, d.value));
            }
        }
        out
    }

    /// Drop the drafts whose gesture is over and whose last value has been
    /// written.
    ///
    /// Called one pass *after* the flush that cleaned them, so the window
    /// covers the round trip of that final write. Once a draft is gone the
    /// track reads from the store again, and a volume change made anywhere
    /// else reaches the control normally.
    pub fn retire(&mut self) {
        self.held.write().retain(|_, d| !(d.released && !d.dirty));
    }

    /// How many values the UI is holding. Zero means every control on
    /// screen is showing the engine's own state.
    pub fn count(&self) -> usize {
        self.held.read().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Signal needs a runtime *and* an owning scope, and these tests
    /// have no component of their own — so they borrow the root's.
    fn with_runtime(f: impl FnOnce(Drafts)) {
        let mut dom = VirtualDom::new(|| rsx! { div {} });
        dom.rebuild_in_place();
        dom.in_runtime(|| {
            f(Drafts {
                held: Signal::new_in_scope(HashMap::new(), ScopeId::ROOT),
            })
        });
    }

    #[test]
    fn a_drag_is_written_once_at_the_value_it_ended_on() {
        with_runtime(|mut drafts| {
            for v in [0.1, 0.2, 0.3, 0.4] {
                drafts.set_volume("T1", v);
            }
            assert_eq!(
                drafts.take_dirty(),
                vec![("T1".to_string(), Held::Volume, 0.4)]
            );
            // Nothing new since: the engine is not told the same value twice.
            assert!(drafts.take_dirty().is_empty());
        });
    }

    /// The trailing edge. Whatever the flush timing, the value the user
    /// stopped on has to reach the engine — a draft released before it was
    /// ever taken is the case that loses it.
    #[test]
    fn the_last_value_lands_even_if_the_drag_ends_between_flushes() {
        with_runtime(|mut drafts| {
            drafts.set_volume("T1", 0.8);
            drafts.release_volume("T1");
            drafts.retire();
            assert_eq!(
                drafts.take_dirty(),
                vec![("T1".to_string(), Held::Volume, 0.8)],
                "the final value was dropped with the gesture"
            );
        });
    }

    #[test]
    fn a_held_track_suppresses_its_echo_until_a_beat_after_the_drag() {
        with_runtime(|mut drafts| {
            drafts.set_volume("T1", 0.5);
            assert!(drafts.holds("T1", Held::Volume));
            assert!(
                !drafts.holds("T2", Held::Volume),
                "another track is not suppressed"
            );

            drafts.release_volume("T1");
            // Still held: the final write has not gone out yet.
            drafts.retire();
            assert!(drafts.holds("T1", Held::Volume));

            // Flushed, then one more pass — now the echo window closes.
            drafts.take_dirty();
            drafts.retire();
            assert!(!drafts.holds("T1", Held::Volume));
            assert_eq!(drafts.count(), 0);
        });
    }

    #[test]
    fn grabbing_the_cap_again_revives_the_draft() {
        with_runtime(|mut drafts| {
            drafts.set_volume("T1", 0.5);
            drafts.release_volume("T1");
            drafts.take_dirty();
            // Second gesture starts before the retire pass.
            drafts.set_volume("T1", 0.9);
            drafts.retire();
            assert!(
                drafts.holds("T1", Held::Volume),
                "suppression dropped mid-gesture"
            );
            assert_eq!(drafts.volume("T1"), Some(0.9));
        });
    }
}
