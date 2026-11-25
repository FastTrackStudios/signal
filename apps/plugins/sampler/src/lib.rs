//! Signal Sampler — CLAP instrument plugin (document mode, see
//! `docs/plan/document-mode.md`).
//!
//! The sampler sitting directly on a track as a self-sufficient CLAP. This
//! crate is the thin host shell; all playback logic lives in `sampler`
//! ([`sampler::DocumentPlayer`]):
//!
//! - **Document playback**: while the host transport is rolling and a
//!   [`Schedule`](sampler::Schedule) is loaded, the track's notes play
//!   ahead of the transport — each legato transition and short pre-fired
//!   by its own lead, so it is heard on its grid tick — with zero latency.
//!   Starting anywhere is the full render at that point.
//! - **Where the document comes from**: the host's track timeline
//!   (`com.fasttrackstudio.host-timeline/1`, [`ext`]: Studio), or, in a
//!   host without it, a `.mid` or JSON file ([`doc_watch`]).
//! - **Live input**: with the host's playback claim, what arrives as MIDI
//!   is live and plays beside the document; with no document or the
//!   transport stopped, the plugin is a plain live instrument (live
//!   auto-divisi allocator + `low_latency` legato gates).
//!
//! Patch + document intake are documented on [`config`] and [`doc_watch`].

pub mod config;
pub mod doc_watch;
pub mod ext;
pub mod plugin;
pub mod reference;

use plugin_core::prelude::*;

nice_export_clap!(plugin::SignalSamplerClap);
