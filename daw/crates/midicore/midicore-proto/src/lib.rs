//! # midicore
//!
//! Ergonomic, backend-agnostic MIDI for FastTrackStudio.
//!
//! - [`event`] — typed messages ([`MidiEvent`]) and a byte codec, over tight
//!   newtypes ([`Channel`], [`U7`], [`PitchBend`]) that make illegal values
//!   unrepresentable.
//! - [`port`] — facet-typed port description / selection + the streamed
//!   [`TimedEvent`].
//! - [`service`] — the I/O surface as `#[architect::rpc]` traits, so a concrete
//!   backend is callable in-process *and* over vox RPC with no hand-written
//!   transport (feature `vox`).
//! - [`filter`] — declarative event matching.
//!
//! Concrete OS/backend adapters (midir, a daw-backed adapter, a virtual test
//! backend) live in separate crates and implement the [`service`] traits.
//!
//! Design note: the message model draws on `helgoboss-midi` — a future revision
//! will split a packed, `Copy`, real-time `ShortMessage` from the structured
//! [`MidiEvent`] (SysEx stays off the `Copy` path) and add 14-bit-CC / RPN
//! aggregators. See `README.md`.

pub mod backend;
pub mod drum_convert;
pub mod event;
pub mod filter;
pub mod number;
pub mod port;
pub mod raw;
#[cfg(feature = "vox")]
pub mod service;

pub use backend::{decode_all, BackendError, InputBackend, InputConfig, MaybeSend};
pub use drum_convert::{DrumMap, DrumMapConverter, HatThresholds};
pub use event::{DecodeError, MidiEvent, MidiKind};
pub use filter::Filter;
pub use number::{
    Channel, ControllerNumber, ControllerValue, KeyNumber, PitchBend, Pressure, ProgramNumber,
    Velocity, U14, U4, U7,
};
pub use port::{Direction, MidiIoError, PortId, PortInfo, PortSelector, TimedEvent};
pub use raw::{RawShortMessage, ShortMessage};
