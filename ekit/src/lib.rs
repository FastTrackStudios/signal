//! **Electronic Kit rig** (#77 M3) — a pad grid over a built sample space.
//!
//! Deliberately a SEPARATE rig from the acoustic drum kit: that one emulates
//! a real kit (multi-mic pieces, bleed, MM2 mixes), this one is a pad
//! instrument over a huge one-shot library. They share the primitives —
//! `signal-space` for the pool and similarity, `SamplerRig` per-instrument
//! daw tracks for the mixer — and nothing else.
//!
//! Kit semantics follow the references (see `docs/spec/sample-space.md`):
//! each pad carries a **category**, kit generation fills unlocked pads from
//! their own categories (Atlas "New Kit"), per-pad Randomize re-rolls within
//! the category, and stepping walks the pad's similarity list — with
//! [`morph_kit`](proto::ekit::EkitRig::morph_kit) stepping every pad at once
//! (XO "Kit Similarity").

mod backend;

pub use backend::{DEFAULT_COLS, DEFAULT_ROWS, EkitBackend};
pub use signal_ekit_proto as proto;

/// MIDI note of pad 0 — GM kick, so a stock drum map lines up.
pub const BASE_NOTE: u8 = 36;
/// The rig's MIDI channel (GM drums).
pub const MIDI_CHANNEL: u8 = 9;
