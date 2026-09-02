//! One shared hardware-MIDI input for the whole process.
//!
//! # Why this exists
//!
//! Every rig used to open its own input. With `omni` (the default — "play me
//! from whatever is plugged in") that is one OS client *per port per rig*:
//! five rigs against 23 ports is ~115 JACK/ALSA clients, all subscribed to
//! the same hardware. Measured on a live rig, that load made port
//! enumeration itself unreliable — `input_ports()` returned empty or short
//! lists while the opens were in flight, the polled hot-plug path read an
//! empty list as "everything was unplugged", tore its stream down and
//! reopened, and the churn fed itself: clients climbed 41 → 291 until the
//! engine died and the keyboard had long since gone silent.
//!
//! Rigs are separate *services*, but in a single engine process they are not
//! separate *listeners*. So the hub is the process's single MIDI input, and
//! fans every event out in-process.
//!
//! # Shape
//!
//! The hub sits on [`midicore::pipewire`], the native PipeWire backend: **one
//! graph node** (`Signal`) with **one MIDI port** that every selected device
//! is linked into. That replaced a midir/JACK arrangement whose cost was
//! structural rather than incidental —
//!
//! - midir opens one OS client per connection (its API, not our choice), so
//!   23 ports meant 23 graph nodes for one application.
//! - its `input_ports()` builds a client just to enumerate, so the hot-plug
//!   pump created and destroyed a JACK client ~2.5 times a second forever —
//!   3002 client lifecycles in ten minutes on this desk. That is the churn
//!   the paragraph above is describing.
//!
//! The native backend has neither problem: enumeration is a registry
//! roundtrip that creates nothing, and hot-plug is pushed by the registry
//! rather than polled.
//!
//! # What a subscriber's filter now means
//!
//! Because every device merges into one port, an event no longer carries the
//! device it came from. A subscriber's `filter` therefore selects **what gets
//! linked**, not what gets delivered: the hub links the union of what all
//! rigs asked for, and every rig hears everything linked. A rig that names a
//! port will also hear a port some other rig named.
//!
//! That is the deliberate cost of the single node. If per-rig device routing
//! is wanted back, the shape to reach for is `pw_filter` — one node with one
//! port per device, which keeps the tag while keeping the single box in the
//! graph (see the note at the foot of `midicore-pipewire`).

use std::sync::{Arc, Mutex, RwLock};

use midicore::pipewire::MidiInput;
use midicore_proto::{PortSelector, TimedEvent};

/// A registered listener.
struct Sub {
    id: u64,
    rig: &'static str,
    /// `None` = every device (omni). `Some(s)` = link only devices whose name
    /// contains `s`, matched case-insensitively — the same rule
    /// [`PortSelector::NameContains`] uses, so a stored port name behaves
    /// identically here.
    filter: Option<String>,
    sink: Arc<dyn Fn(TimedEvent) + Send + Sync>,
}

impl Sub {
    fn selector(&self) -> PortSelector {
        match self.filter.as_deref() {
            Some(f) if !f.is_empty() => PortSelector::NameContains(f.to_string()),
            _ => PortSelector::All,
        }
    }
}

/// Shared between the input callback and the API; read on the MIDI thread,
/// written only when a rig subscribes or unsubscribes.
type Subs = Arc<RwLock<Vec<Sub>>>;

pub struct MidiHub {
    subs: Subs,
    /// The process's single MIDI input node, opened on first subscribe.
    input: Mutex<Option<MidiInput>>,
    next_id: Mutex<u64>,
}

/// Removes its sink from the hub when dropped.
pub struct Subscription {
    hub: &'static MidiHub,
    id: u64,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let removed = match self.hub.subs.write() {
            Ok(mut subs) => {
                let before = subs.len();
                subs.retain(|s| s.id != self.id);
                before != subs.len()
            }
            Err(_) => false,
        };
        // A rig going away can narrow what needs linking; leaving its links
        // up would keep feeding events nobody wants.
        if removed {
            self.hub.apply();
        }
    }
}

static HUB: std::sync::OnceLock<MidiHub> = std::sync::OnceLock::new();

/// The process-wide hub.
pub fn hub() -> &'static MidiHub {
    HUB.get_or_init(|| MidiHub {
        subs: Arc::new(RwLock::new(Vec::new())),
        input: Mutex::new(None),
        next_id: Mutex::new(0),
    })
}

impl MidiHub {
    /// Register `sink` for `rig`. Events arrive until the returned
    /// [`Subscription`] is dropped.
    ///
    /// The first subscribe opens the node; later ones only widen what is
    /// linked. Both are cheap — no rig pays for another rig's devices.
    pub fn subscribe(
        &'static self,
        rig: &'static str,
        filter: Option<String>,
        sink: impl Fn(TimedEvent) + Send + Sync + 'static,
    ) -> Subscription {
        let id = {
            let mut n = self.next_id.lock().unwrap_or_else(|e| e.into_inner());
            *n += 1;
            *n
        };
        if let Ok(mut subs) = self.subs.write() {
            subs.retain(|s| s.rig != rig); // one live sink per rig
            subs.push(Sub {
                id,
                rig,
                filter,
                sink: Arc::new(sink),
            });
        }
        tracing::info!(midi.rig = rig, midi.sub_id = id, "midi hub: subscribed");
        self.apply();
        Subscription { hub: self, id }
    }

    /// The device ports currently linked into the hub's node.
    pub fn ports(&self) -> Vec<String> {
        self.input
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|i| i.ports())
            .unwrap_or_default()
    }

    /// Re-apply what subscribers want.
    ///
    /// Kept for callers that used to drive the polled hot-plug path. It is now
    /// only a nudge: the backend follows the registry, so devices appear and
    /// disappear without anyone asking. Returns whether the linked set moved.
    pub fn rescan(&'static self) -> bool {
        let before = self.ports();
        self.apply();
        self.ports() != before
    }

    /// Open the node if needed and link the union of every subscriber's
    /// selector.
    fn apply(&'static self) {
        let selectors: Vec<PortSelector> = match self.subs.read() {
            Ok(subs) => subs.iter().map(Sub::selector).collect(),
            Err(_) => return,
        };

        let mut input = self.input.lock().unwrap_or_else(|e| e.into_inner());
        match input.as_ref() {
            Some(existing) => existing.set_selectors(selectors),
            None => {
                if selectors.is_empty() {
                    return; // Nothing to listen for yet.
                }
                let started = std::time::Instant::now();
                match MidiInput::open(PortSelector::All, self.make_sink()) {
                    Ok(new) => {
                        new.set_selectors(selectors);
                        tracing::info!(
                            midi.node = midicore::pipewire::DEFAULT_NODE_NAME,
                            midi.elapsed_ms = started.elapsed().as_millis(),
                            "midi hub: opened"
                        );
                        *input = Some(new);
                    }
                    Err(e) => tracing::error!("midi hub: open failed: {e}"),
                }
            }
        }
    }

    /// The one sink every device feeds. Fans out to every subscriber, because
    /// a merged port cannot say which device an event came from.
    fn make_sink(&'static self) -> impl Fn(TimedEvent) + Send + 'static {
        let subs = Arc::clone(&self.subs);
        move |ev: TimedEvent| {
            let Ok(subs) = subs.read() else {
                // The lock is only written on subscribe/unsubscribe, so this
                // means a panic poisoned it — and a silent return here is a
                // rig that goes deaf for no visible reason.
                tracing::error!("midi hub: subscriber list poisoned — event dropped");
                return;
            };
            for s in subs.iter() {
                (s.sink)(ev.clone());
            }

            // Every note that enters the process, logged once at its entry
            // point. This is the question that costs hours otherwise — "did
            // the app receive it at all?" — and the hub is the single place
            // where the answer is knowable. Off unless asked for; a busy
            // controller sends hundreds of events a second.
            if tracing::enabled!(tracing::Level::DEBUG) {
                match &ev.event {
                    midicore_proto::MidiEvent::NoteOn {
                        channel,
                        key,
                        velocity,
                    } => tracing::debug!(
                        midi.kind = "note_on",
                        midi.channel = ?channel,
                        midi.note = key.get(),
                        midi.velocity = velocity.get(),
                        midi.delivered_to = subs.len(),
                        "midi in"
                    ),
                    midicore_proto::MidiEvent::NoteOff { channel, key, .. } => tracing::debug!(
                        midi.kind = "note_off",
                        midi.channel = ?channel,
                        midi.note = key.get(),
                        midi.delivered_to = subs.len(),
                        "midi in"
                    ),
                    other => tracing::trace!(
                        midi.kind = "other",
                        midi.delivered_to = subs.len(),
                        midi.event = ?other,
                        "midi in"
                    ),
                }
            }

            // A note nobody wanted is worth one warning regardless of level:
            // it is always a misconfiguration, never normal.
            if subs.is_empty() && matches!(ev.event, midicore_proto::MidiEvent::NoteOn { .. }) {
                tracing::warn!(
                    "note-on reached the hub with NO subscribers — the node is \
                     linked but no rig is listening"
                );
            }
        }
    }
}

/// A drain-style subscription: the pull shape of `midicore`'s `MidiStream`,
/// but fed by the shared hub instead of its own OS clients.
///
/// A polled pump that drains once a tick does not want a callback, and
/// rewriting it to push would mean touching its gesture and debounce logic.
/// So the hub pushes into the same bounded channel and the pump keeps
/// draining exactly as it did.
pub struct MidiDrain {
    _sub: Subscription,
    rx: crossbeam_channel::Receiver<midicore_proto::RawShortMessage>,
}

impl MidiDrain {
    /// Non-blocking drain of all pending events — the same contract as
    /// `MidiStream::drain`.
    pub fn drain(&self) -> impl Iterator<Item = midicore_proto::RawShortMessage> + '_ {
        self.rx.try_iter()
    }
}

impl MidiHub {
    /// Subscribe `rig` and receive events through a channel rather than a
    /// callback. Bounded: a stalled consumer drops events instead of growing
    /// without limit on the MIDI thread.
    pub fn subscribe_drain(&'static self, rig: &'static str, filter: Option<String>) -> MidiDrain {
        let (tx, rx) = crossbeam_channel::bounded::<midicore_proto::RawShortMessage>(256);
        let sub = self.subscribe(rig, filter, move |t| {
            if let Some(raw) = midicore_proto::RawShortMessage::from_event(&t.event) {
                let _ = tx.try_send(raw);
            }
        });
        MidiDrain { _sub: sub, rx }
    }
}

#[cfg(test)]
mod tests {
    use super::Sub;
    use midicore_proto::PortSelector;
    use std::sync::Arc;

    fn sub(filter: Option<&str>) -> Sub {
        Sub {
            id: 0,
            rig: "test",
            filter: filter.map(str::to_string),
            sink: Arc::new(|_| {}),
        }
    }

    /// An omni subscriber asks for everything; a named one narrows to its
    /// device. The backend unions these, so one omni rig links every device
    /// however many named rigs there are.
    ///
    /// The filter is carried through verbatim: it is a stored port name, and
    /// the backend matches it the way `NameContains` does, so selecting a
    /// port in the UI behaves identically through the hub.
    #[test]
    fn selector_follows_the_filter() {
        assert!(matches!(sub(None).selector(), PortSelector::All));
        assert!(matches!(sub(Some("")).selector(), PortSelector::All));
        match sub(Some("S88")).selector() {
            PortSelector::NameContains(n) => assert_eq!(n, "S88"),
            other => panic!("expected NameContains, got {other:?}"),
        }
    }
}
