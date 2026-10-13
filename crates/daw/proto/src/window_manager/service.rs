//! WindowManager service — named workspace layouts.
//!
//! Layouts are stored in REAPER's native `reaper-screensets.ini` so that
//! REAPER's own load action can drive the actual window/dock manipulation.
//! This service is the typed façade for naming, listing, applying, and
//! tracking those layouts from Rust.

use super::{WindowLayout, WindowLayoutOptions, WindowLayoutResult, WindowLayoutSummary};

#[architect::rpc]
pub trait WindowManager {
    /// Apply a named layout. Implementations look up the layout by `name`
    /// in `reaper-screensets.ini` and fire REAPER's native `Screenset:
    /// Load #N` action for the matching slot.
    fn apply_layout(&self, name: String, options: WindowLayoutOptions) -> WindowLayoutResult;

    /// All named layouts currently stored in `reaper-screensets.ini`.
    fn list_layouts(&self) -> Vec<WindowLayoutSummary>;

    /// Return the most recently applied layout, if any. Tracked Rust-side
    /// (REAPER doesn't expose "current screenset" through its public API).
    fn current_layout(&self) -> Option<WindowLayoutSummary>;

    /// Fetch a full layout definition by name.
    fn get_layout(&self, name: String) -> Option<WindowLayout>;

    /// Persist a layout definition. The toolbar-placement metadata lives
    /// in our own store; the `[ssetN]` body is whatever REAPER had at
    /// capture time (host fires "Screenset: Save #N" alongside this call
    /// when the user wants to snapshot live state).
    fn save_layout(&self, layout: WindowLayout) -> WindowLayoutResult;

    /// Remove a layout by name. Frees the underlying REAPER screenset slot
    /// for reuse.
    fn delete_layout(&self, name: String) -> WindowLayoutResult;
}
