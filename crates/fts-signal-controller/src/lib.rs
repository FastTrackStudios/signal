//! FTS Signal Controller — CLAP plugin for per-track signal chain control.
//!
//! Uses `daw::init()` / `daw::get()` for DAW access — fully DAW-agnostic.
//! No direct reaper-rs dependency.
//!
//! # Timer callbacks
//!
//! - **scene_timer**: reads timeline MIDI items, mutes/unmutes child tracks
//! - **macro_timer**: reads macro values + mapping config from P_EXT, drives FX params

pub mod macro_timer;
pub mod param_queue;
pub mod plugin;
pub mod scene_timer;
pub mod shm_bridge;

use fts_plugin_core::prelude::*;

nih_export_clap!(plugin::FtsSignalController);
