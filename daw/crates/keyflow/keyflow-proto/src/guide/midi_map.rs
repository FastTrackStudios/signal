//! MIDI note constants and mappings for click, count, and section guide events.
//!
//! Ported from the legacy FTS Guide plugin's `midi/notes.rs`.

use crate::SectionType;

// ── Click subdivision MIDI notes ─────────────────────────────────────────────

/// Measure accent (beat 1) — C4
pub const CLICK_ACCENT: u8 = 60;
/// Regular beat — C#4
pub const CLICK_BEAT: u8 = 61;
/// Eighth note subdivision — D4
pub const CLICK_EIGHTH: u8 = 62;
/// Sixteenth note subdivision — D#4
pub const CLICK_SIXTEENTH: u8 = 63;
/// Triplet subdivision — F4
pub const CLICK_TRIPLET: u8 = 65;

// ── Count number MIDI notes (1–8) ───────────────────────────────────────────

/// Count "1" — C5
pub const COUNT_1: u8 = 72;
/// Count "2" — C#5
pub const COUNT_2: u8 = 73;
/// Count "3" — D5
pub const COUNT_3: u8 = 74;
/// Count "4" — D#5
pub const COUNT_4: u8 = 75;
/// Count "5" — E5
pub const COUNT_5: u8 = 76;
/// Count "6" — F5
pub const COUNT_6: u8 = 77;
/// Count "7" — F#5
pub const COUNT_7: u8 = 78;
/// Count "8" — G5
pub const COUNT_8: u8 = 79;

/// All count MIDI notes indexed by count number (0-indexed: index 0 = count "1").
pub const COUNT_NOTES: [u8; 8] = [
    COUNT_1, COUNT_2, COUNT_3, COUNT_4, COUNT_5, COUNT_6, COUNT_7, COUNT_8,
];

/// Get the MIDI note for a count number (1–8).
///
/// Returns `None` if the count number is out of range.
pub fn count_midi_note(count_number: u8) -> Option<u8> {
    if (1..=8).contains(&count_number) {
        Some(COUNT_NOTES[(count_number - 1) as usize])
    } else {
        None
    }
}

// ── Section type MIDI notes ─────────────────────────────────────────────────

/// Map a `SectionType` to its MIDI note for guide cue triggering.
///
/// Returns `None` for section types that don't have a guide cue mapping.
/// Custom section types are matched by name against the legacy string-based mapping.
pub fn section_type_midi_note(section_type: &SectionType) -> Option<u8> {
    match section_type {
        SectionType::Verse => Some(84),        // C6
        SectionType::Chorus => Some(85),       // C#6
        SectionType::Bridge => Some(86),       // D6
        SectionType::Intro => Some(87),        // D#6
        SectionType::Outro => Some(88),        // E6
        SectionType::Instrumental => Some(89), // F6
        SectionType::Pre(inner) => match inner.as_ref() {
            SectionType::Chorus => Some(90), // F#6 — Pre-Chorus
            _ => None,
        },
        SectionType::Post(inner) => match inner.as_ref() {
            SectionType::Chorus => Some(91), // G6 — Post-Chorus
            _ => None,
        },
        SectionType::Breakdown => Some(92),   // G#6
        SectionType::Interlude => Some(93),   // A6
        SectionType::Solo => Some(96),        // C7
        SectionType::Vamp => Some(97),        // C#7
        SectionType::Refrain => Some(99),     // D#7 — recurring hook
        SectionType::Tag => Some(94),         // A#6 — the guide's "Tag" sample
        SectionType::Turnaround => Some(101), // F7 — transitional link
        SectionType::Custom(name) => custom_section_midi_note(name),
        SectionType::CountIn | SectionType::Opening | SectionType::End | SectionType::Hits => None,
    }
}

/// Map custom section type names to MIDI notes.
///
/// These correspond to section types that exist in the legacy string-based system
/// but don't have dedicated `SectionType` enum variants.
fn custom_section_midi_note(name: &str) -> Option<u8> {
    // Case-insensitive matching
    match name.to_lowercase().as_str() {
        "tag" => Some(94),          // A#6
        "ending" => Some(95),       // B6
        "turnaround" => Some(98),   // D7
        "refrain" => Some(99),      // D#7
        "rap" => Some(100),         // E7
        "acapella" => Some(101),    // F7
        "exhortation" => Some(102), // F#7
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_section_type_midi_notes() {
        assert_eq!(section_type_midi_note(&SectionType::Verse), Some(84));
        assert_eq!(section_type_midi_note(&SectionType::Chorus), Some(85));
        assert_eq!(section_type_midi_note(&SectionType::Bridge), Some(86));
        assert_eq!(section_type_midi_note(&SectionType::Intro), Some(87));
        assert_eq!(section_type_midi_note(&SectionType::Outro), Some(88));
        assert_eq!(section_type_midi_note(&SectionType::Instrumental), Some(89));
        assert_eq!(
            section_type_midi_note(&SectionType::Pre(Box::new(SectionType::Chorus))),
            Some(90)
        );
        assert_eq!(
            section_type_midi_note(&SectionType::Post(Box::new(SectionType::Chorus))),
            Some(91)
        );
        assert_eq!(section_type_midi_note(&SectionType::Breakdown), Some(92));
        assert_eq!(section_type_midi_note(&SectionType::Interlude), Some(93));
        assert_eq!(section_type_midi_note(&SectionType::Solo), Some(96));
        assert_eq!(section_type_midi_note(&SectionType::Vamp), Some(97));
    }

    #[test]
    fn test_section_type_midi_note_returns_none_for_unsupported() {
        assert_eq!(section_type_midi_note(&SectionType::CountIn), None);
        assert_eq!(section_type_midi_note(&SectionType::End), None);
        assert_eq!(section_type_midi_note(&SectionType::Hits), None);
        // Unknown custom types return None
        assert_eq!(
            section_type_midi_note(&SectionType::Custom("Unknown".to_string())),
            None
        );
        // Pre-Verse has no dedicated note
        assert_eq!(
            section_type_midi_note(&SectionType::Pre(Box::new(SectionType::Verse))),
            None
        );
    }

    #[test]
    fn test_custom_section_type_midi_notes() {
        assert_eq!(
            section_type_midi_note(&SectionType::Custom("Tag".to_string())),
            Some(94)
        );
        assert_eq!(
            section_type_midi_note(&SectionType::Custom("Ending".to_string())),
            Some(95)
        );
        assert_eq!(
            section_type_midi_note(&SectionType::Custom("Turnaround".to_string())),
            Some(98)
        );
        assert_eq!(
            section_type_midi_note(&SectionType::Custom("Refrain".to_string())),
            Some(99)
        );
        assert_eq!(
            section_type_midi_note(&SectionType::Custom("Rap".to_string())),
            Some(100)
        );
        assert_eq!(
            section_type_midi_note(&SectionType::Custom("Acapella".to_string())),
            Some(101)
        );
        assert_eq!(
            section_type_midi_note(&SectionType::Custom("Exhortation".to_string())),
            Some(102)
        );
        // Case insensitive
        assert_eq!(
            section_type_midi_note(&SectionType::Custom("TAG".to_string())),
            Some(94)
        );
        assert_eq!(
            section_type_midi_note(&SectionType::Custom("turnaround".to_string())),
            Some(98)
        );
    }

    #[test]
    fn test_count_midi_note() {
        assert_eq!(count_midi_note(1), Some(72));
        assert_eq!(count_midi_note(8), Some(79));
        assert_eq!(count_midi_note(0), None);
        assert_eq!(count_midi_note(9), None);
    }
}
