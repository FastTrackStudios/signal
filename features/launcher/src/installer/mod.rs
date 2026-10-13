//! FastTrackStudio installer core — pure installation logic with no UI dependency.
//!
//! Handles downloading REAPER, extracting DMGs, copying extensions and presets,
//! writing portable `reaper.ini`, and setting up shell PATH.

pub mod icon_gen;
pub mod plan;
pub mod profiles;
pub mod progress;
pub mod retry;
pub mod runner;
pub mod steps;

pub use plan::InstallPlan;
pub use progress::{EventSender, InstallEvent, InstallStep};
pub use runner::{InstallContext, run_all_steps};
pub use steps::install_fts_extensions::BundledExtension;
