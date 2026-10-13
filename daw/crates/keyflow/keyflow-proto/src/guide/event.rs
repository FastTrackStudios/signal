//! Guide event types — the output of click/count-in/section-cue scheduling.

use crate::SectionType;
use facet::Facet;

/// A guide event produced by the scheduling algorithms.
#[repr(u8)]
#[derive(Debug, Clone, Facet)]
pub enum GuideEvent {
    /// A click subdivision event.
    Click(ClickEvent),
    /// A count-in number event (e.g., "1", "2", "3", "4").
    Count(CountEvent),
    /// A section cue announcement event (e.g., "Verse", "Chorus").
    SectionCue(SectionCueEvent),
}

/// The type of click subdivision.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub enum ClickType {
    /// Regular beat click.
    Beat,
    /// Eighth note subdivision.
    Eighth,
    /// Sixteenth note subdivision.
    Sixteenth,
    /// Triplet subdivision.
    Triplet,
    /// Measure accent (beat 1).
    Accent,
}

/// A click track event at a specific position.
#[derive(Debug, Clone, Facet)]
pub struct ClickEvent {
    /// The subdivision type.
    pub click_type: ClickType,
    /// Position in quarter notes from the start of the region.
    pub position_quarters: f64,
    /// MIDI note number for this click type.
    pub midi_note: u8,
    /// MIDI velocity (0–127).
    pub velocity: u8,
}

/// A count-in number event (spoken "1", "2", etc.).
#[derive(Debug, Clone, Facet)]
pub struct CountEvent {
    /// The count number (1–8).
    pub count_number: u8,
    /// Position in quarter notes from the start of the region.
    pub position_quarters: f64,
    /// MIDI note number for this count.
    pub midi_note: u8,
}

/// A section cue announcement event.
#[derive(Debug, Clone, Facet)]
pub struct SectionCueEvent {
    /// The section type being announced.
    pub section_type: SectionType,
    /// Optional section number (e.g., Verse 1, Verse 2).
    pub section_number: Option<u32>,
    /// Position in quarter notes from the start of the region.
    pub position_quarters: f64,
    /// MIDI note number for this section type.
    pub midi_note: u8,
}
