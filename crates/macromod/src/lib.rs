//! `macromod` — Unified macro & modulation system for parameter control over time.
//!
//! This crate provides the data model for:
//! - **Macro knobs** — assignable virtual controls with parameter bindings
//! - **Modulation routing** — LFO, envelope, MIDI CC, expression, and macro sources
//! - **Response curves** — easing + power curves for shaping control response
//! - **Parameter targeting** — unified `ParamTarget` for addressing block parameters
//!
//! Macros and modulation are unified: macros **are** modulation sources
//! (`ModulationSource::Macro`), and both use `ParamTarget` for targeting.

pub mod binding;
pub mod curation;
pub mod easing;
pub mod macro_bank;
pub mod parameter;
pub mod response;
pub mod routing;
pub mod runtime;
pub mod sources;
pub mod target;

// ─── Flat re-exports for convenience ────────────────────────────

pub use binding::MacroBinding;
pub use curation::ParamCuration;
pub use easing::{lerp_eased, EasingCurve};
pub use macro_bank::{GroupSelector, MacroBank, MacroGroup, MacroKnob};
pub use parameter::{BlockParameter, ParameterValue};
pub use response::ResponseCurve;
pub use routing::{ModulationRoute, ModulationRouteSet};
pub use sources::{
    EnvelopeConfig, EnvelopeMode, FollowerConfig, FollowerInput, LfoConfig, LfoWaveform,
    ModulationSource, RandomConfig, RetriggerMode, TempoDiv,
};
pub use target::{ModulationTarget, ParamTarget};

// Runtime engine re-exports
pub use runtime::{
    evaluate_waveform, EnvelopeStage, EnvelopeState, FollowerState, LfoState, ModulationOutput,
    ModulationProcessor, RandomState, TickContext,
};
