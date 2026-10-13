//! Splitting a finished mix into the parts a mix engineer thinks in.
//!
//! Given a reference record, produce a vocal, a bass, a kick, a snare —
//! so [`signal_analyzer::elements`] can measure each one and say what the
//! record is actually doing. This crate obtains the models and drives
//! them; it does no measurement of its own.
//!
//! ## The cascade
//!
//! ```text
//!   mix ──► base (RoFormer)  ──► vocals ──► karaoke ──► lead / backing
//!                             ├─ drums  ──► DrumSep ──► kick / snare /
//!                             │                          toms / cymbals
//!                             ├─ bass
//!                             └─ other
//! ```
//!
//! Separation itself is `PyTorch`, reached through `audio-separator`;
//! `nix develop .#stems` provides that. Rust owns the model registry,
//! the ordering, and the checking — the parts where a silent mistake is
//! expensive.
//!
//! ## What this is for, and what it is not
//!
//! It runs on a handful of named reference tracks, on demand. It is not
//! a corpus stage: six-way drum separation across thousands of songs is
//! days of GPU spent on questions nobody has asked yet.
//!
//! Note also that every number downstream of this is measured on
//! *estimated* stems. Separation error partly cancels between two stems
//! measured the same way, so relationships ("the kick sits 4 dB above
//! the bass at 60 Hz") survive far better than absolutes ("the kick is
//! −12 LUFS"). Prefer the former when building anything on top.

pub mod models;

pub use models::{Arch, Asset, DRUMSEP, MANAGED, Model, Resolved};
