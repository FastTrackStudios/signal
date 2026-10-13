//! Enums for time representation and behavior

use facet::Facet;

/// Time display mode (how time is shown in the UI)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet, Default)]
pub enum TimeMode {
    /// Display as seconds (e.g., "123.456")
    Time,
    /// Display as measures.beats.time (e.g., "4.2.75")
    #[default]
    MeasuresBeatsTime,
    /// Display as measures.beats (e.g., "4.2")
    MeasuresBeats,
    /// Display as measures.beats minimal (e.g., "4.2")
    MeasuresBeatsMinimal,
    /// Display as seconds (e.g., "123.456")
    Seconds,
    /// Display as audio samples
    Samples,
    /// Display as hours:minutes:seconds:frames (e.g., "0:02:03:15")
    HoursMinutesSecondsFrames,
    /// Display as absolute frame count
    AbsoluteFrames,
}

/// How items/regions attach to timeline (affects time stretch behavior)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet, Default)]
pub enum BeatAttachMode {
    /// Fixed to time (doesn't move with tempo changes)
    #[default]
    Time,
    /// Fixed to beats (position, length, and rate follow tempo)
    Beats,
    /// Position follows beats, but length/rate fixed to time
    BeatsPositionOnly,
}

/// How beats are calculated in time mapping
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet, Default)]
pub enum MeasureMode {
    /// Pure beat counting (ignore measure boundaries)
    #[default]
    IgnoreMeasure,
    /// Start from specific measure, stop at next tempo/time sig marker
    FromMeasureAtIndex(i32),
}

/// Time mode override (for specific contexts)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet, Default)]
pub enum TimeModeOverride {
    /// Use the project's default time mode
    #[default]
    ProjectDefault,
    /// Override with a specific time mode
    Mode(TimeMode),
}

/// MIDI event timing
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet, Default)]
pub enum SendMidiTime {
    /// Send MIDI event immediately
    #[default]
    Instantly,
    /// Send MIDI event at specific frame offset
    AtFrameOffset(u32),
}

/// Automation playback/recording mode
#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Facet)]
pub enum AutomationMode {
    /// Trim/Read mode - automation is read, trims are applied
    #[default]
    TrimRead = 0,
    /// Read mode - automation is read as-is
    Read = 1,
    /// Touch mode - writes automation only while control is touched
    Touch = 2,
    /// Write mode - continuously overwrites automation
    Write = 3,
    /// Latch mode - writes automation after first touch until stop
    Latch = 4,
    /// Latch Preview mode - like Latch but previews changes
    LatchPreview = 5,
    /// Off — bypass automation (matches REAPER's "Off" track mode).
    /// The envelope is ignored entirely during playback + render.
    Off = 6,
}
