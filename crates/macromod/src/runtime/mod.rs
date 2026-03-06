//! Runtime evaluation engine for modulation sources.
//!
//! This module provides the "brain" that makes modulation data actually modulate.
//! It contains pure computation — no async, no DAW, no I/O.
//!
//! ## Architecture
//!
//! - [`waveform`] — Pure waveform evaluation for all 7 LFO shapes
//! - [`lfo_state`] — LFO state machine with phase accumulation and tempo sync
//! - [`envelope_state`] — AHDSR envelope with Sustain/OneShot/Loop modes
//! - [`processor`] — Orchestrates all sources and produces per-target offsets

pub mod envelope_state;
pub mod lfo_state;
pub mod processor;
pub mod waveform;

pub use envelope_state::{EnvelopeStage, EnvelopeState};
pub use lfo_state::LfoState;
pub use processor::{FollowerState, ModulationOutput, ModulationProcessor, RandomState, TickContext};
pub use waveform::evaluate_waveform;
