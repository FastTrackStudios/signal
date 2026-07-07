//! The sampler trait framework — `docs/sampler-trait-design.md`.
//!
//! A clean, declarative API layered *over* the existing engine. The guiding
//! principle: **the common 90% is declarative data; the exotic 10% is a trait
//! you implement.** This module defines the contract; the existing
//! [`SampleEngine`](crate::SampleEngine) is the first working
//! [`Instrument`](traits::Instrument) implementation (see [`engine`]).
//!
//! ## Phase A status (this layer)
//! - **Real**: all primitives ([`prim`]); the declarative model types
//!   ([`model`]); [`model::VelCurve`] with full breakpoint interpolation +
//!   interval scaling; every trait signature ([`traits`]); the working
//!   [`Instrument`] impl on `SampleEngine` ([`engine::EngineInstrument`]); the
//!   [`script::ScriptPipeline`] with the [`script::KeyswitchRouter`] and
//!   [`script::VelocityCurve`] built-ins.
//! - **Trivial defaults** (trait + a usable no-op impl): [`traits::SumMixer`],
//!   [`traits::Passthrough`], [`traits::ConstModulator`], [`traits::OpenAuth`].
//! - **Phase-B stubs** (defined, behavior TODO): [`traits::Voicer`],
//!   [`traits::Loader`], and the legato/pedal/CC/humanize scripts in
//!   [`script`].
//!
//! The existing `SampleEngine` / `SamplerBank` / `SamplerInstrument` /
//! `SamplerRig` / mixer stay untouched — this is additive.

pub mod adapt;
pub mod engine;
pub mod model;
pub mod prim;
pub mod rig;
pub mod script;
pub mod traits;

// ── Curated re-exports ───────────────────────────────────────────────────

pub use prim::{
    ArticulationId, Axis, Cc, Cents, Db, Frames, GroupId, InstrumentId, Interned, Interval, MicId,
    MidiCh, Note, Seconds, U7, U14, Velocity, ZoneId,
};

pub use model::{
    Articulation, Direction, DynLayer, Group, InstrumentModel, Legato, LegatoMode, Lerp, Mic,
    Polyphony, RoundRobin, SampleSlice, Scale, Select, TransitionLayers, Trigger, Variation,
    VelCurve, VelLayer, Zone, ZoneLayers,
};

pub use traits::{
    Account, AuthError, Authorization, BusGraph, ConstModulator, Effect, Instrument, LibraryId,
    Loader, MicBlock, MicMixer, ModContext, Modulator, OpenAuth, Param, Passthrough, PerfState,
    PreloadProfile, PreloadStats, SampleRef, StereoBuf, SumMixer, VoicePool, VoiceRequest, Voicer,
};

pub use engine::EngineInstrument;

pub use adapt::{CacheLoader, CacheZoneLayers, pre_delay_curve};

pub use rig::{
    Amp, Block, BlockId, BlockRole, BlockToggle, CabDsp, Cabinet, Chain, ChainBuilder,
    ControlEvent, Controller, DawRig, ExpressionBind, Lane, Node, ParallelMix, Param as RigParam,
    ParamRef, Patch, PatchBuilder, PatchId, PatchStepper, Plugin, Profile, ProfileBuilder,
    ProfileId, ProgramChangeMap, Rig, Snapshot, SnapshotId, SnapshotSwitcher, TapTempo,
    TunerReading, block,
};

pub use script::{
    Action, CcArticulation, Humanize, KeyswitchRouter, MidiMessage, Performance, PerformanceScript,
    RoundRobinReset, ScriptPipeline, SustainPedal, TrueLegato, VelocityCurve,
};
