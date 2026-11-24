//! Signal's one processor (signal#26).
//!
//! Every host runs this crate: Studio's engine links it (desktop, iPhone,
//! iPad, the browser's AudioWorklet), and the Signal CLAP
//! (`apps/plugins/signal`) wraps it for other DAWs and as Studio's
//! hot-reloadable development copy. No plugin format, no UI, no I/O.
//!
//! ```text
//!  SignalState (one blob: the preset)        SignalHost (the host's side)
//!        │  build (control thread)                 │ files, actions, hits,
//!        ▼                                         │ tempo and transport
//!  Graph: lanes of blocks  ──install──▶  Processor (audio thread)
//!        in channels ─▶ lane chain ─▶ out channels, MIDI in, host params
//! ```
//!
//! - [`state`]: the preset and instance state, one serialisable blob.
//! - [`block`]: a chain's block, `ChainBlock { id, kind, params, state,
//!   backend }`: compiled in (`Linked`) by default, or its own CLAP (or a
//!   third-party one), with one parameter schema for both, so macros and
//!   modulation work the same on either; where it can run ([`platform`])
//!   and what became of it when built (active, fallback, bypassed).
//! - [`null_test`]: the harness that renders a block compiled in and as
//!   its CLAP with the same macro automation, and says whether they null.
//! - [`graph`]: a state built into blocks; [`engine`]: the audio side and
//!   the lock-free handoff from the control side.
//! - [`session`]: one Signal session shared by the instances on a kit's
//!   tracks (the per-track mode), beside the single multi-channel mode.
//! - [`host`]: [`SignalHost`](host::SignalHost), what the core asks of
//!   whoever runs it.
//!
//! The crate is `no_std` + `alloc`: allocation happens when a graph is
//! built, never in [`engine::Processor::process`].

#![no_std]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod block;
pub mod blocks;
pub mod engine;
pub mod graph;
pub mod host;
pub mod kit;
pub mod midi;
#[cfg(feature = "std")]
pub mod null_test;
pub mod param;
pub mod platform;
pub mod rigs;
pub mod session;
pub mod state;
mod util;
pub mod wav;

pub use block::{
    Backend, BlockFactory, BlockKind, BlockProcessor, BlockStatus, BuiltinFactory, ChainBlock,
    HostedPlugin,
};
pub use engine::{Controller, Processor, processor_pair};
pub use graph::{BuildOptions, BuildReport, Graph};
pub use host::{NoHost, SignalHost};
pub use midi::MidiEvent;
pub use platform::{Platform, Platforms};
pub use state::SignalState;

/// The most channels a Signal instance takes in or gives out: a kit's
/// mics in, its stems out.
pub const MAX_CHANNELS: usize = 32;

/// The longest piece a block is handed at once. A host block longer than
/// this is processed in pieces.
pub const MAX_BLOCK: usize = 1024;
