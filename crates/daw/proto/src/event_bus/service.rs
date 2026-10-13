//! `EventBus` service — single-subscribe surface for all stream domains.
//!
//! Architect `#[subscribe]` stream: the backend publishes every
//! domain's events into one `PubSub<DawEvent>` hub
//! (`EventBusStreamSource`), and subscribers receive everything.
//! Filtering is client-side — [`BusFilter`](super::BusFilter) moved
//! off the wire and into the consumer (see `daw_control::Events`),
//! matching the architect idiom of argless subscriptions.

use super::event::DawEvent;

#[architect::rpc]
pub trait EventBus {
    /// Every event from every domain, multiplexed on one channel, as
    /// it happens. Subscribers filter by variant / `project_guid`
    /// client-side.
    #[subscribe]
    fn events(&self) -> DawEvent;
}
