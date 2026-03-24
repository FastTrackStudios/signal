//! FTS Signal Controller — CLAP plugin for per-track signal chain control.
//!
//! Sits on each REAPER track and manages:
//!
//! - **Rig setup**: Automated FX chain construction from resolved graphs
//! - **Parameter control**: Same-track `TrackFX_SetParamNormalized` calls
//!   from the audio thread (sample-accurate)
//! - **Macro modulation**: LFO, envelope, MIDI CC → FX parameter routing
//! - **Cross-track routing**: Receives `ParamWriteRequest`s over SHM from
//!   signal-extension or other controller instances, applies them locally
//!
//! # Architecture
//!
//! ```text
//! signal-desktop (UI)
//!     │
//!     │ SHM/RPC (vox)
//!     ▼
//! signal-extension (per-REAPER-instance, manages DB + resolution)
//!     │
//!     │ SHM messages (ParamWriteRequest, ResolvedGraph)
//!     ▼
//! fts-signal-controller (CLAP, per-track)
//!     │
//!     │ TrackFX_SetParamNormalized (same-track, audio-thread safe)
//!     ▼
//! Target FX plugins on this track
//! ```
//!
//! # Real-time safety
//!
//! Following the Helgobox/ReaLearn pattern:
//! - Only `TrackFX_SetParamNormalized` on the **same track** from `process()`
//! - Cross-track writes are routed via SHM to the target track's controller
//! - Lock-free ring buffer for SHM → audio thread communication
//! - No allocations in the audio callback
//!
//! # REAPER Bootstrap
//!
//! Exports `ReaperPluginEntry` for eager loading by the REAPER extension.
//! This gives the plugin direct REAPER API access and registers a ~30Hz
//! timer callback for scene switching (reading timeline MIDI items and
//! muting/unmuting sends).

// Gate each heavy module independently to bisect scan failures
#[cfg(feature = "macro-timer")]
pub mod macro_timer;
#[cfg(feature = "full")]
pub mod param_queue;
#[cfg(not(feature = "full"))]
pub mod param_queue_stub;
#[cfg(not(feature = "full"))]
pub use param_queue_stub as param_queue;
pub mod plugin;
#[cfg(feature = "reaper-init")]
pub mod reaper_bootstrap;
#[cfg(feature = "scene-timer")]
pub mod scene_timer;
#[cfg(feature = "shm-bridge")]
pub mod shm_bridge;

use fts_plugin_core::prelude::*;

// ── CLAP plugin export ──────────────────────────────────────────────

nih_export_clap!(plugin::FtsSignalController);
