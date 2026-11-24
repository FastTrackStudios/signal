//! FTS Signal, the plugin: `signal-core` as a CLAP/VST3 (signal#26 phase 2).
//!
//! The same processor Studio compiles in, wrapped for other DAWs and as
//! Studio's hot-reloadable development copy on the desktop. See
//! [`plugin`] for the wrapper and [`sampler`] for the desktop sampler block.

pub mod config;
pub mod drum;
pub mod editor;
pub mod ext;
pub mod offline;
pub mod plugin;
pub mod sampler;
pub mod slots;
pub mod strip;

pub use plugin::FtsSignal;

use nice_plug::prelude::*;

nice_export_clap!(FtsSignal);
nice_export_vst3!(FtsSignal);
