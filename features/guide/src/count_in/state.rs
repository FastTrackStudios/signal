//! Count-in state tracking
//!
//! Manages state related to count-in playback, including which region we're counting to
//! and the current count number.

/// State tracking for count-in playback
#[derive(Debug, Clone)]
pub struct CountInState {
    /// Target region start position (in quarter notes) when counting
    pub counting_to_region: Option<f64>,

    /// Last count beat position (to track which beat we're counting)
    pub last_count_beat: f64,

    /// Last bar number when counting (to detect measure boundary crossings)
    pub last_count_bar_number: Option<i32>,

    /// Last raw beat position in count-in measure (to detect wraps)
    pub last_count_beat_raw: Option<f64>,

    /// Which count number (1-8) is currently playing (-1 if none)
    pub current_count_number: i32,

    /// Whether guide has been triggered for the current count-in period (only trigger once per count-in)
    pub guide_has_triggered: bool,
}

impl Default for CountInState {
    fn default() -> Self {
        Self {
            counting_to_region: None,
            last_count_beat: -1.0,
            last_count_bar_number: None,
            last_count_beat_raw: None,
            current_count_number: -1,
            guide_has_triggered: false,
        }
    }
}

impl CountInState {
    /// Reset all count-in state (called on measure boundaries)
    pub const fn reset(&mut self) {
        self.last_count_beat = -1.0;
        self.last_count_beat_raw = None;
        self.current_count_number = -1;
        self.last_count_bar_number = None;
        self.counting_to_region = None;
        self.guide_has_triggered = false;
    }
}
