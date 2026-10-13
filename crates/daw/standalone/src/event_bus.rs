//! Event-bus stream source for `Standalone`.
//!
//! The cross-domain bus is an architect `#[subscribe]` stream served
//! from one process-wide `PubSub<DawEvent>` hub. Per-domain publish
//! helpers (`publish_track_events`, `publish_marker_event`, …) wrap
//! their events in [`DawEvent`] and publish here alongside their own
//! domain hub; transport state + position ticks are bridged in by the
//! per-project pump spawned in `Standalone::transport_engine_for`.
//! Subscribers receive everything and filter client-side (the old
//! `BusFilter` parameter moved into `daw_control::Events`).

use crate::Standalone;
use daw_proto::event_bus::{DawEvent, EventBus, EventBusStreamSource};

// The base `EventBus` trait is empty after the `#[subscribe]` port —
// only the stream sibling carries surface. The impl keeps `Standalone`
// eligible for any code that bounds on the trait.
impl EventBus for Standalone {}

impl EventBusStreamSource for Standalone {
    /// The first frame every subscriber gets, so a client can tell when
    /// it is actually attached.
    ///
    /// `subscribe` returns before the hub has the sink, and there is no
    /// replay, so anything published in that gap went to nobody — a
    /// consumer that subscribed and immediately caused an event could
    /// wait forever for it. The marker rides at the front of the new
    /// subscriber's mailbox, ahead of anything published in between.
    fn events_intro(&self) -> Option<DawEvent> {
        Some(DawEvent::Attached)
    }

    fn events_hub(&self) -> &architect::PubSub<DawEvent> {
        &self.bus_events
    }
}
