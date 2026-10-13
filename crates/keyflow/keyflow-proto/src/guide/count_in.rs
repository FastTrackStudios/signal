//! Count-in state tracking for real-time playback.
//!
//! Ported from the legacy FTS Guide plugin's `count_in/state.rs`.

use facet::Facet;

/// Mutable state for tracking count-in progress during playback.
///
/// This tracks where we are in a count-in period so we can determine
/// which count number to play and whether the guide cue has already fired.
#[derive(Debug, Clone, Default, Facet)]
pub struct CountInState {
    /// Target section start position in quarter notes.
    /// `Some` when a count-in is active, `None` when idle.
    pub counting_to_position: Option<f64>,

    /// The last beat position (quarter notes) at which a count was triggered.
    /// Used to avoid double-triggering on the same beat.
    pub last_count_beat: f64,

    /// The last bar number at which counting occurred.
    /// Used to detect measure boundary crossings.
    pub last_count_bar_number: Option<i32>,

    /// Current count number (1–8), or 0 if no count is active.
    pub current_count_number: i32,

    /// Whether the section guide cue has already been triggered
    /// for the current count-in period.
    pub guide_has_triggered: bool,
}

impl CountInState {
    /// Reset all state for a new count-in period or when stopping.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
