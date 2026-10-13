//! reaper-launcher: Typed management of REAPER ini config and instance launching.
//!
//! Provides a safe, structured interface for:
//! - Reading/writing `reaper.ini` config variables
//! - Temporarily patching settings for a launch, then restoring originals
//! - Spawning REAPER instances with the correct env vars and arguments
//!
//! Used by the wrapper `.app` launcher binaries and by `xtask setup-rigs`.

mod config;
pub mod desktop;
pub mod icon_gen;
mod ini;
mod launcher;

pub use config::{LaunchConfig, ReaperIniConfig};
pub use ini::ReaperIni;
pub use launcher::{discover_config, launch, launch_with_config, launch_with_config_and_args};
