//! Sync trait implementation for the standalone backend.
//!
//! This module implements `daw_proto::*` traits with an in-memory state
//! store. It is intended for tests and non-REAPER hosts. The state lives behind
//! a single `std::sync::Mutex` (this is non-realtime control state — std mutex
//! is fine, this is not the audio path).
//!
//! The sync state here is independent from the older async services in this
//! crate (`StandaloneTransport`, `StandaloneRegion`, etc.). Sharing the storage
//! across both API styles would be valuable but is left for a follow-up; the
//! async services have their own per-service locks and entry points and would
//! need broader surgery to converge.

mod daw;
// ext_state ported to architect::rpc — see `crate::ext_state`.
// fx_chains ported to architect::rpc — see `crate::fx_chains`.
// fx_params ported to architect::rpc — see `crate::fx_params`.
// items ported to architect::rpc — see `crate::item`.
// markers ported to architect::rpc — `impl Markers for Standalone`
// lives at `crate::marker`. The borrowed `StandaloneMarkers<'a>` view
// retired with the port.
pub(crate) mod project;
// regions ported to architect::rpc — see `crate::region`.
// routing ported to architect::rpc — see `crate::routing_sync`.
// takes ported to architect::rpc — see `crate::take`.
// tempo_map ported to architect::rpc — see `crate::tempo_map`.
// tracks ported to architect::rpc — `impl Tracks for Standalone`
// lives at `crate::track`.
// `Transport` ported to architect::rpc — `impl Transport for
// Standalone` lives at `crate::transport`.

pub use daw::{
    EnvelopeData, EnvelopeKey, FxChainKey, FxEntry, ItemEntry, ProjectState, RulerLane, Standalone,
    StandaloneState, TakeList, TrackExt,
};
