//! FTS Mic GUI — what the two FTS Mic plugins share above the engine.
//!
//! - [`params`]: the mic list and both parameter trees (FTS Mic, FTS Mic 180)
//! - [`view`]: the editors the plugin shells embed, on [`fts_plug_ui`]'s
//!   shared chrome

pub mod params;
pub mod view;
