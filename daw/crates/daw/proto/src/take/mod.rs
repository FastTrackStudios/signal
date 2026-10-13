//! Take service — canonical home for the `Takes` trait + architect
//! re-exports. Take *data* types (`Take`, `TakeRef`, `TakeMarker`, …)
//! live under `crate::item::take` for now; this module owns the
//! service surface so `take::serve(Reaper)` mounts cleanly alongside
//! the rest of the architect::rpc family.

pub mod service;

pub use service::*;
