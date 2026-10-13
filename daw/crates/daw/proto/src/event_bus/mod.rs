//! Cross-domain event bus.
//!
//! See [`EventBus`] for the service trait and [`DawEvent`] /
//! [`BusFilter`] for the wire shapes. Per-domain `Tracks::subscribe`
//! etc. remain — the bus is the convenience layer for consumers
//! (OSC/MIDI bridges, inspectors) that want everything on one channel.

mod event;
mod service;

pub use event::{BusFilter, DawEvent};
pub use service::*;

// vox::service is applied to `EventBus` directly (all-async shape), so
// the emitted client/dispatcher/descriptor have no `Rpc` infix.
// Architect emits the `Rpc`-suffixed aliases for serve/layer plumbing.
