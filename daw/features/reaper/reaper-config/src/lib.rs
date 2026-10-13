// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// this crate predates that — burn down separately.
#![allow(dead_code, unused)]

//! # reaper-config
//!
//! Typed Rust mappings for REAPER configuration files.
//!
//! This crate provides strongly-typed structs for reading and writing REAPER's
//! `reaper.ini` preferences file. Instead of working with raw string key/value
//! pairs, you get proper Rust types with documentation.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use reaper_config::ReaperConfig;
//!
//! // Parse an existing reaper.ini
//! let ini_text = std::fs::read_to_string("reaper.ini").unwrap();
//! let config = ReaperConfig::parse(&ini_text).unwrap();
//!
//! // Access typed fields
//! println!("Undo memory: {:?} MB", config.general.undo_max_mem);
//! println!("Auto-save interval: {:?}s", config.project_save.auto_save_interval);
//! println!("Worker threads: {:?}", config.performance.work_threads);
//!
//! // Modify and write back
//! let mut config = config;
//! config.general.undo_max_mem = Some(512);
//! let output = config.serialize();
//! std::fs::write("reaper.ini", output).unwrap();
//! ```

pub mod ini;
pub mod parse;
pub mod types;

pub mod def_presets;
pub mod ext_state;
pub mod fx_folders;
pub mod menu;
pub mod mouse;
pub mod presets;
pub mod render_presets;
pub mod vst_cache;

pub use ini::IniFile;
pub use types::*;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("parse error: {0}")]
    Parse(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid value for key `{key}`: {reason}")]
    InvalidValue { key: String, reason: String },
}

pub type Result<T> = std::result::Result<T, ConfigError>;
