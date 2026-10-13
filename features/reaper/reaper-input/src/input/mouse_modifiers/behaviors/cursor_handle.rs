//! Cursor Handle mouse modifier behaviors

/// Cursor Handle behaviors
#[allow(unused_imports)]
use crate::define_behavior_enum;

define_behavior_enum! {
    pub enum CursorHandleBehavior {
        NoAction => (0, "No action"),
        ScrubAudio => (1, "Scrub audio"),
        JogAudio => (2, "Jog audio"),
        ScrubAudioLoopedSegment => (3, "Scrub audio (looped-segment mode)"),
        JogAudioLoopedSegment => (4, "Jog audio (looped-segment mode)"),
    }
}
