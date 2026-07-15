//! The bass rig — DI bass → NAM amp / IR chain → out on the shared Signal
//! engine, mirroring the guitar rig's audio path in a preset-first shape.
//!
//! Signal-Live framing: **rigs are presets of the one engine**. The bass rig
//! is one duplex audio path ([`signal_sampler::GuitarRig`] — the shared
//! `r[signal.soundsource.audio]` DI engine) whose library holds *presets*:
//! "Bass" and "Synth Bass" are both presets of this rig (same input, a
//! different chain), and a sampled bass is a future preset kind — not
//! another rig.
//!
//! - [`BassRigBackend`] — the headless vox-served core (`architect::rig::
//!   RigBackend` meter pump, `midicore::attach` MIDI lifecycle).
//! - [`library`] — the styx preset library (`<config>/signal/bass/`).
//! - `signal-bass-proto` — the wire contract (re-exported as [`proto`]).

pub mod backend;
pub mod library;

pub use backend::BassRigBackend;
// Re-export the wire contract so front-end/app crates get types + clients
// from one place.
pub use signal_bass_proto as proto;
