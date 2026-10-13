//! Runtime configuration for the FTS Input plugin.
//!
//! Loaded from `~/.config/REAPER/FTS/input.styx` (or the path supplied to
//! [`init_config_watcher`](crate::init_config_watcher)). The file is re-read
//! automatically whenever it is saved.

use facet::Facet;

use crate::InputProfile;

/// Top-level plugin config, parsed from the `.styx` config file.
#[derive(Facet, Debug, Clone)]
pub struct InputConfig {
    /// Which keyboard/mouse profile to activate.
    ///
    /// Valid values: `@FastTrackStudio`, `@Logic`, `@ProTools`
    pub profile: InputProfile,

    /// Whether the input handler starts enabled.
    ///
    /// Set to `true` if you want FTS Input active as soon as REAPER loads.
    pub enabled: bool,

    /// Consume key events that the handler processes.
    ///
    /// When `true`, keys that match a binding are swallowed so REAPER never
    /// sees them. Set to `false` for debug/passthrough mode.
    pub eat_handled_keys: bool,

    /// Write verbose input-event logging to `/tmp/reaper-fts-input.log`.
    pub debug_logging: bool,
}
