//! Chord-to-syllable alignment
//!
//! Maps chord instances to lyric syllables based on their timing positions.
//! This enables:
//! - Synced lyric playback (each syllable knows its chord change)
//! - Lyric slides (visualize syllable duration until next chord)
//! - MIDI generation with syllable-level precision
//! - Interactive chord charts with syllable selection

use facet::Facet;

use super::lyrics::LyricSyllable;
use super::types::ChordInstance;
use crate::time::{AbsolutePosition, MusicalDuration};

/// A chord assigned to a specific syllable with duration information
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct ChordSyllableMapping {
    /// Which chord instance this mapping refers to
    pub chord_index: usize,

    /// Which syllable this chord is assigned to (index in the lyric line)
    pub syllable_index: usize,

    /// Position where this chord starts (measure + beat)
    pub chord_position: AbsolutePosition,

    /// Duration until the next chord change
    pub duration_until_next_chord: MusicalDuration,

    /// Attachment point relative to the syllable
    /// e.g., "at_start" means chord changes right at syllable start
    pub attachment: ChordAttachmentType,
}

/// How a chord attaches to a syllable in time
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum ChordAttachmentType {
    /// Chord starts exactly at syllable start
    AtSyllableStart,

    /// Chord starts before syllable (mid-word or upbeat)
    BeforeSyllable,

    /// Chord starts after syllable begins (mid-syllable or upbeat)
    AfterSyllableStart,

    /// Chord spans multiple syllables (no intermediate change)
    SpansMultipleSyllables,
}

/// Complete chord-syllable alignment for a section
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct SectionAlignment {
    /// All chord-syllable mappings in order
    pub mappings: Vec<ChordSyllableMapping>,

    /// Reference to all chords in the section (for index lookups)
    pub chord_count: usize,

    /// Reference to all syllables in the section (for index lookups)
    pub syllable_count: usize,
}

impl SectionAlignment {
    /// Create a new alignment
    pub fn new(chord_count: usize, syllable_count: usize) -> Self {
        Self {
            mappings: Vec::new(),
            chord_count,
            syllable_count,
        }
    }

    /// Add a chord-syllable mapping
    pub fn add_mapping(&mut self, mapping: ChordSyllableMapping) {
        self.mappings.push(mapping);
    }

    /// Get all chords that apply to a specific syllable
    pub fn chords_for_syllable(&self, syllable_index: usize) -> Vec<&ChordSyllableMapping> {
        self.mappings
            .iter()
            .filter(|m| m.syllable_index == syllable_index)
            .collect()
    }

    /// Get all syllables covered by a specific chord
    pub fn syllables_for_chord(&self, chord_index: usize) -> Vec<&ChordSyllableMapping> {
        self.mappings
            .iter()
            .filter(|m| m.chord_index == chord_index)
            .collect()
    }

    /// Get the chord that covers a given time position
    pub fn chord_at_position(&self, position: &AbsolutePosition) -> Option<&ChordSyllableMapping> {
        self.mappings.iter().find(|m| &m.chord_position == position)
    }

    /// Sort mappings by chord position for sequential processing
    pub fn sort_by_position(&mut self) {
        self.mappings.sort_by(|a, b| {
            ChordSyllableAligner::compare_positions(&a.chord_position, &b.chord_position)
        });
    }
}

/// Algorithm to align chords and syllables based on timing
pub struct ChordSyllableAligner;

impl ChordSyllableAligner {
    /// Align chords to syllables for a section
    ///
    /// # Algorithm
    ///
    /// 1. Collect all chord instances with their time positions
    /// 2. Collect all syllables (assign them timing based on evenly distributing them
    ///    across the measures, or use explicit timing if available)
    /// 3. For each syllable, find which chord covers its time position
    /// 4. Determine attachment type based on exact alignment
    /// 5. Calculate duration until next chord change
    ///
    /// # Returns
    ///
    /// A `SectionAlignment` mapping each chord to its syllables
    pub fn align(
        chords: &[ChordInstance],
        syllables: &[LyricSyllable],
    ) -> Result<SectionAlignment, String> {
        if chords.is_empty() {
            return Err("No chords to align".to_string());
        }
        if syllables.is_empty() {
            return Err("No syllables to align".to_string());
        }

        let mut alignment = SectionAlignment::new(chords.len(), syllables.len());

        // Assign timing to syllables if not explicit
        let syllables_with_timing = Self::assign_syllable_timing(syllables, chords)?;

        // For each syllable, find the active chord and create mapping
        for (syl_idx, syllable) in syllables_with_timing.iter().enumerate() {
            // Find the chord that covers this syllable's time
            if let Some(chord_idx) = Self::find_active_chord(
                &syllable.position,
                chords,
                syl_idx < syllables_with_timing.len() - 1,
            ) {
                let chord = &chords[chord_idx];

                // Determine attachment type
                let attachment = if syllable.position == chord.position {
                    ChordAttachmentType::AtSyllableStart
                } else if Self::compare_positions(&syllable.position, &chord.position).is_lt() {
                    ChordAttachmentType::BeforeSyllable
                } else {
                    ChordAttachmentType::AfterSyllableStart
                };

                // Calculate duration until next chord
                let next_chord_position = Self::next_chord_position(chord_idx, chords);
                let duration_until_next = if let Some(next_pos) = next_chord_position {
                    Self::calculate_duration(&syllable.position, &next_pos)?
                } else {
                    chord.duration.clone()
                };

                let mapping = ChordSyllableMapping {
                    chord_index: chord_idx,
                    syllable_index: syl_idx,
                    chord_position: chord.position.clone(),
                    duration_until_next_chord: duration_until_next,
                    attachment,
                };

                alignment.add_mapping(mapping);
            }
        }

        alignment.sort_by_position();
        Ok(alignment)
    }

    /// Assign timing to syllables based on even distribution or explicit timing
    fn assign_syllable_timing(
        syllables: &[LyricSyllable],
        chords: &[ChordInstance],
    ) -> Result<Vec<SyllableWithTiming>, String> {
        let result: Vec<SyllableWithTiming> = syllables
            .iter()
            .enumerate()
            .map(|(idx, syl)| {
                // If syllable has explicit timing, use it
                let position = if syl.beat > 0.0 || syl.measure_index > 0 {
                    AbsolutePosition::new(
                        MusicalDuration::new(syl.measure_index as i32, syl.beat as i32, 0),
                        0,
                    )
                } else {
                    // Otherwise, distribute syllables evenly across first chord duration
                    let chord_duration_beats =
                        chords.first().map(|c| c.duration.beat.max(1)).unwrap_or(4);
                    let syllable_offset =
                        (idx as i32 * chord_duration_beats) / syllables.len() as i32;
                    AbsolutePosition::new(MusicalDuration::new(0, syllable_offset, 0), 0)
                };

                SyllableWithTiming { position }
            })
            .collect();

        Ok(result)
    }

    /// Find which chord covers a given time position
    fn find_active_chord(
        position: &AbsolutePosition,
        chords: &[ChordInstance],
        _not_last: bool,
    ) -> Option<usize> {
        // Find the chord at or before this position
        let mut active_chord_idx = None;

        for (idx, chord) in chords.iter().enumerate() {
            if Self::compare_positions(&chord.position, position).is_le() {
                active_chord_idx = Some(idx);
            } else {
                break; // Chords should be sorted by position
            }
        }

        active_chord_idx
    }

    /// Find the next chord after a given chord index
    fn next_chord_position(chord_idx: usize, chords: &[ChordInstance]) -> Option<AbsolutePosition> {
        chords.get(chord_idx + 1).map(|c| c.position.clone())
    }

    /// Calculate musical duration between two absolute positions
    fn calculate_duration(
        from: &AbsolutePosition,
        to: &AbsolutePosition,
    ) -> Result<MusicalDuration, String> {
        // This is a simplified version; actual implementation would need proper duration calculation
        let measure_diff = to
            .total_duration
            .measure
            .saturating_sub(from.total_duration.measure);
        let beat_diff = to
            .total_duration
            .beat
            .saturating_sub(from.total_duration.beat);

        Ok(MusicalDuration {
            measure: measure_diff,
            beat: beat_diff,
            subdivision: 0,
        })
    }

    fn compare_positions(a: &AbsolutePosition, b: &AbsolutePosition) -> std::cmp::Ordering {
        (
            a.section_index,
            a.total_duration.measure,
            a.total_duration.beat,
            a.total_duration.subdivision,
        )
            .cmp(&(
                b.section_index,
                b.total_duration.measure,
                b.total_duration.beat,
                b.total_duration.subdivision,
            ))
    }
}

/// Internal: Syllable with computed timing
#[derive(Debug, Clone)]
struct SyllableWithTiming {
    position: AbsolutePosition,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_section_alignment_creation() {
        let alignment = SectionAlignment::new(4, 12);
        assert_eq!(alignment.chord_count, 4);
        assert_eq!(alignment.syllable_count, 12);
        assert_eq!(alignment.mappings.len(), 0);
    }

    #[test]
    fn test_chords_for_syllable() {
        let mut alignment = SectionAlignment::new(2, 4);

        alignment.add_mapping(ChordSyllableMapping {
            chord_index: 0,
            syllable_index: 0,
            chord_position: AbsolutePosition::new(MusicalDuration::new(0, 0, 0), 0),
            duration_until_next_chord: MusicalDuration::new(0, 2, 0),
            attachment: ChordAttachmentType::AtSyllableStart,
        });

        alignment.add_mapping(ChordSyllableMapping {
            chord_index: 1,
            syllable_index: 2,
            chord_position: AbsolutePosition::new(MusicalDuration::new(0, 2, 0), 0),
            duration_until_next_chord: MusicalDuration::new(0, 2, 0),
            attachment: ChordAttachmentType::AtSyllableStart,
        });

        assert_eq!(alignment.chords_for_syllable(0).len(), 1);
        assert_eq!(alignment.chords_for_syllable(2).len(), 1);
        assert_eq!(alignment.chords_for_syllable(3).len(), 0);
    }
}
