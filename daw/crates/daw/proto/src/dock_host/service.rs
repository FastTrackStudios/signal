//! Dock host service — register / show / hide / layout docks.
//!
//! Implemented by a host adapter (REAPER docker, standalone native
//! window, browser DOM, mock for tests). Guests/extensions don't
//! know which adapter is in use.

use super::{DockHandle, DockKind, PanelPixels, UiEventDto};

#[architect::rpc]
pub trait DockHosting {
    /// Register a dock by stable string id. If already registered,
    /// returns the existing handle.
    fn register_dock(&self, id: &str, title: &str, kind: DockKind) -> DockHandle;

    /// Idempotent. Returns `false` if already gone.
    fn unregister_dock(&self, handle: DockHandle) -> bool;

    fn show(&self, handle: DockHandle);
    fn hide(&self, handle: DockHandle);
    /// Toggle visibility — returns the new visibility state.
    fn toggle(&self, handle: DockHandle) -> bool;
    fn is_visible(&self, handle: DockHandle) -> bool;

    /// Serialize the current layout for persistence.
    fn save_layout(&self) -> Vec<u8>;

    /// Restore a previously-saved layout blob.
    fn restore_layout(&self, blob: &[u8]) -> bool;

    /// Capture the current rendered pixels of a dock panel. `None`
    /// when no live panel is mounted or first render hasn't
    /// completed.
    fn capture_panel_pixels(&self, handle: DockHandle) -> Option<PanelPixels>;

    /// Inject a synthetic UI event into a panel.
    fn inject_ui_event(&self, handle: DockHandle, event: UiEventDto) -> bool;
}
