//! Internal transport engine — sample-accurate, WASM-compatible.
//!
//! This module is **not** part of the public `daw-proto` surface. It's
//! an internal layer that owns the audio-thread sample clock and
//! exposes lock-free atomics + snapshots. The `Transport` service in
//! `src/transport.rs` writes through it; the cpal/AudioWorklet callback
//! ticks it via [`TransportShared::advance`].
//!
//! Layering:
//!
//! ```text
//! daw-proto Transport (seconds + tempo + loop_region)
//!     ▲ mirror via TransportEngine::snapshot()
//!     │
//! TransportEngine ── Arc<TransportShared> ──► audio callback
//!     ▲                  (atomics; advance())
//!     │
//! transport.rs service impl
//! ```
//!
//! See module-level docs in each submodule for the algorithm
//! references (mostly distilled from Firewheel).

pub mod block;
pub mod bundle;
pub mod clock;
pub mod engine;
pub mod tempo_map;

pub use block::{BlockPlan, BlockSegment, ScheduledLocate};
pub use bundle::{TransportBundle, spawn_subscriber_pump};
pub use clock::{InstantMusical, InstantSamples, InstantSeconds, SampleClock};
pub use engine::{
    LoopRegionSamples, PlayStateRepr, TransportEngine, TransportShared, TransportSnapshot,
};
pub use tempo_map::{
    DynamicTempoMap, StaticTempoMap, TempoKeyframe, TempoMapError, build_dynamic_from_time_bpm,
};
