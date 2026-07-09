//! Signal Sampler — CLAP instrument plugin (document mode phase 2, see
//! `docs/plan/document-mode.md`).
//!
//! The production shape from the plan's "Deployment target": the sampler
//! sitting directly on a REAPER track as a self-sufficient CLAP. This crate
//! is the thin host shell; all playback logic lives in `signal-sampler`:
//!
//! - **Document playback**: while the host transport is rolling and a
//!   [`Schedule`](signal_sampler::Schedule) is loaded, process() delegates to
//!   [`signal_sampler::RealtimeScheduler`] — the offline document walker
//!   driven by the CLAP transport (prefires ahead of legato ticks,
//!   seek/loop/stop discontinuity handling, late-start degradation).
//! - **StrictLive**: otherwise incoming CLAP MIDI goes down the normal live
//!   path (live auto-divisi allocator + `low_latency` legato gates), the
//!   zero-added-latency policy.
//!
//! Patch + document intake are documented on [`config`] and [`doc_watch`].

pub mod config;
pub mod doc_watch;
pub mod plugin;

use audiocore_core::prelude::*;

nice_export_clap!(plugin::SignalSamplerClap);
