//! Window geometry data types.

use crate::ScreensetRect;
use facet::Facet;

/// Which window the geometry op targets.
///
/// - `Focused` — whatever window currently has keyboard focus.
/// - `Main` — REAPER's main window.
#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Facet)]
#[facet(rename_all = "snake_case")]
pub enum WindowTarget {
    #[default]
    Focused,
    Main,
}

/// Result of a geometry op. `applied` is `false` when the target
/// wasn't resolvable (e.g. nothing focused, or SWELL unavailable).
/// The `rect` reflects the window state *after* the op when applied.
#[derive(Debug, Clone, Default, Facet)]
pub struct WindowGeometryResult {
    pub applied: bool,
    pub rect: Option<ScreensetRect>,
    /// Empty on success; populated when the op couldn't run.
    pub error: String,
}
