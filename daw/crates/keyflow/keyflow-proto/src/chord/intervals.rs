//! Interval computation for chords.
//!
//! This module contains the interval-related implementation for [`Chord`],
//! including interval computation, degree queries, semitone sequences, and note generation.

use super::alteration::Alteration;
use super::definition::Chord;
use super::degree::ChordDegree;
use super::extensions::Extensions;
use crate::key::{Key, KeySpelling, SpellingMode};
use crate::primitives::{Interval, MusicalNote, RootNotation};

// ============================================================================
// Interval Computation
// ============================================================================

impl Chord {
    /// Compute intervals and semantic degrees from quality, family, extensions, and alterations.
    ///
    /// This is the core algorithm that builds the internal interval map and
    /// semantic degree list from the chord's components.
    pub(crate) fn compute_intervals(&mut self) {
        self.intervals.clear();
        self.semantic_degrees.clear();

        // Always include root
        self.intervals.insert(ChordDegree::Root, Interval::Unison);
        self.semantic_degrees.push(ChordDegree::Root);

        // Add intervals from quality (triad)
        for interval in self.quality.intervals() {
            let degree = ChordDegree::from_interval(interval);
            self.intervals.insert(degree, interval);
            if !self.semantic_degrees.contains(&degree) {
                self.semantic_degrees.push(degree);
            }
        }

        // Add seventh from family (if present)
        if let Some(family) = &self.family {
            let seventh_interval = family.seventh_interval();
            self.intervals
                .insert(ChordDegree::Seventh, seventh_interval);
            if !self.semantic_degrees.contains(&ChordDegree::Seventh) {
                self.semantic_degrees.push(ChordDegree::Seventh);
            }
        }

        // Add implied extensions for complete voicing
        self.add_implied_extensions();

        // Add explicit extensions
        for interval in self.extensions.intervals() {
            let degree = ChordDegree::from_interval(interval);
            self.intervals.insert(degree, interval);
            if !self.semantic_degrees.contains(&degree) {
                self.semantic_degrees.push(degree);
            }
        }

        // Apply alterations (override expected intervals)
        for alteration in &self.alterations {
            self.intervals
                .insert(alteration.degree, alteration.interval);
            if !self.semantic_degrees.contains(&alteration.degree) {
                self.semantic_degrees.push(alteration.degree);
            }
        }

        // Add additions
        for degree in &self.additions {
            if !self.intervals.contains_key(degree) {
                let interval = degree.to_expected_interval(self.quality);
                self.intervals.insert(*degree, interval);
            }
            if !self.semantic_degrees.contains(degree) {
                self.semantic_degrees.push(*degree);
            }
        }

        // Remove omissions
        for degree in &self.omissions {
            self.intervals.remove(degree);
            self.semantic_degrees.retain(|d| d != degree);
        }

        // Sort semantic degrees
        self.semantic_degrees.sort();
    }

    /// Add implied extensions for complete voicing.
    ///
    /// - For 11th chords, the 9th is implied
    /// - For 13th chords, the 9th and 11th are implied
    fn add_implied_extensions(&mut self) {
        let has_ninth = self.extensions.ninth.is_some();
        let has_eleventh = self.extensions.eleventh.is_some();
        let has_thirteenth = self.extensions.thirteenth.is_some();

        // Add implied 9th if we have 11th or 13th but no explicit 9th
        if (has_eleventh || has_thirteenth) && !has_ninth && self.family.is_some() {
            self.intervals.insert(ChordDegree::Ninth, Interval::Ninth);
            if !self.semantic_degrees.contains(&ChordDegree::Ninth) {
                self.semantic_degrees.push(ChordDegree::Ninth);
            }
        }

        // Add implied 11th if we have 13th but no explicit 11th
        if has_thirteenth && !has_eleventh && self.family.is_some() {
            self.intervals
                .insert(ChordDegree::Eleventh, Interval::Eleventh);
            if !self.semantic_degrees.contains(&ChordDegree::Eleventh) {
                self.semantic_degrees.push(ChordDegree::Eleventh);
            }
        }
    }
}

// ============================================================================
// Interval Queries
// ============================================================================

impl Chord {
    /// Get all intervals in this chord, sorted by semitones.
    #[must_use]
    pub fn intervals(&self) -> Vec<Interval> {
        let mut intervals: Vec<_> = self.intervals.values().copied().collect();
        intervals.sort_by_key(|i| i.semitones());
        intervals
    }

    /// Get all semantic degrees in this chord.
    #[must_use]
    pub fn semantic_degrees(&self) -> &[ChordDegree] {
        &self.semantic_degrees
    }

    /// Get the interval for a specific degree (if present).
    #[must_use]
    pub fn interval_for_degree(&self, degree: ChordDegree) -> Option<Interval> {
        self.intervals.get(&degree).copied()
    }

    /// Check if a degree is present in the chord.
    #[must_use]
    pub fn has_degree(&self, degree: ChordDegree) -> bool {
        self.intervals.contains_key(&degree)
    }
}

// ============================================================================
// Detail Level
// ============================================================================

impl Chord {
    /// Check if this chord exceeds the given detail level.
    ///
    /// Returns true if the chord has extensions beyond what the level allows.
    /// For example, a C13 chord exceeds the Sevenths level because it has
    /// 9th, 11th, and 13th extensions.
    #[must_use]
    pub fn exceeds_level(&self, level: super::detail_level::DetailLevel) -> bool {
        self.extensions.highest().is_some_and(|h| !level.allows(h))
    }

    /// Display this chord at a specific detail level.
    ///
    /// When the chord's extensions exceed the level, returns a polychord
    /// representation (e.g., "Dm/C" instead of "C13").
    #[must_use]
    pub fn display_at_level(&self, level: super::detail_level::DetailLevel) -> String {
        use super::detail_level::compute_upper_structure;

        // Check if chord exceeds the detail level
        if self.exceeds_level(level) {
            // Compute upper structure polychord
            if let Some(upper) = compute_upper_structure(self, level) {
                return format!("{}/{}", upper.chord, upper.bass);
            }
        }

        // No simplification needed, use normal display
        self.to_string()
    }
}

// ============================================================================
// Chord Modification
// ============================================================================

impl Chord {
    /// Add an alteration to the chord.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The degree to alter is not present in the chord
    /// - The degree is already altered
    pub fn add_alteration(&mut self, alteration: Alteration) -> Result<(), String> {
        // Check if the degree is already present or will be added
        if !self.has_degree(alteration.degree) {
            return Err(format!(
                "Cannot alter degree {} which is not present in the chord",
                alteration.degree
            ));
        }

        // Check for conflicting alterations
        if self
            .alterations
            .iter()
            .any(|a| a.degree == alteration.degree)
        {
            return Err(format!("Degree {} is already altered", alteration.degree));
        }

        self.alterations.push(alteration);
        self.compute_intervals();
        Ok(())
    }

    /// Add an addition (e.g., add9, add11).
    pub fn add_addition(&mut self, degree: ChordDegree) {
        if !self.additions.contains(&degree) {
            self.additions.push(degree);
            self.compute_intervals();
        }
    }

    /// Add an omission (e.g., no3, no5).
    pub fn add_omission(&mut self, degree: ChordDegree) {
        if !self.omissions.contains(&degree) {
            self.omissions.push(degree);
            self.compute_intervals();
        }
    }

    /// Set extensions.
    pub fn set_extensions(&mut self, extensions: Extensions) {
        self.extensions = extensions;
        self.compute_intervals();
    }

    /// Set the bass note (for slash chords).
    pub fn set_bass(&mut self, bass: RootNotation) {
        self.bass = Some(bass);
    }

    /// Respell the chord root according to a key context.
    ///
    /// This adjusts the root note's spelling to match the key's preferred
    /// enharmonic spelling (e.g., G# → Ab in Eb major).
    pub fn respell_root(&mut self, key_spelling: &KeySpelling, mode: SpellingMode) {
        if let Some(root_note) = self.root.resolved_note() {
            let respelled = key_spelling.respell(root_note, mode);
            self.root = RootNotation::from_note_name(respelled);
            // Also respell bass if present
            if let Some(bass) = &self.bass
                && let Some(bass_note) = bass.resolved_note()
            {
                let respelled_bass = key_spelling.respell(bass_note, mode);
                self.bass = Some(RootNotation::from_note_name(respelled_bass));
            }
            self.normalize();
        }
    }
}

// ============================================================================
// Note and Pitch Queries
// ============================================================================

impl Chord {
    /// Get the root note as a MusicalNote.
    ///
    /// For note names, this returns the note directly.
    /// For scale degrees and roman numerals, a Key is required to resolve them.
    #[must_use]
    pub fn root_note(&self, key: Option<&Key>) -> Option<MusicalNote> {
        self.root.resolve(key)
    }

    /// Get the semitone sequence for this chord.
    ///
    /// Returns a vector of semitones relative to the root, preserving octave information.
    /// This is useful for voicings where intervals can span multiple octaves.
    /// The root is always 0 (first element).
    ///
    /// # Examples
    ///
    /// - Cmaj7 = [0, 4, 7, 11] - all within first octave
    /// - C9 = [0, 4, 7, 10, 14] - ninth is in second octave
    /// - C13 = [0, 4, 7, 10, 14, 17, 21] - extends to second octave
    #[must_use]
    pub fn semitone_sequence(&self) -> Vec<u8> {
        let mut semitones: Vec<u8> = self
            .intervals
            .values()
            .map(|interval| interval.semitones())
            .collect();

        // Ensure root (0) is included and sort
        if !semitones.contains(&0) {
            semitones.push(0);
        }
        semitones.sort_unstable();
        semitones
    }

    /// Get the pitch class set for this chord (semitones within one octave).
    ///
    /// Returns a vector of semitones (0-11) relative to the root, sorted in ascending order.
    /// All intervals are reduced to the first octave. This is useful for analyzing
    /// chord quality regardless of voicing.
    #[must_use]
    pub fn pitch_classes(&self) -> Vec<u8> {
        let mut semitones: Vec<u8> = self
            .intervals
            .values()
            .map(|interval| interval.semitones() % 12)
            .collect();

        // Ensure root (0) is included and sort
        if !semitones.contains(&0) {
            semitones.push(0);
        }
        semitones.sort_unstable();
        semitones.dedup(); // Remove duplicates (e.g., if both 2 and 14 are present, we get only 2)
        semitones
    }

    /// Get all notes in the chord as MusicalNote objects.
    ///
    /// Returns a vector of MusicalNote objects representing each tone in the chord.
    /// The notes are ordered from lowest to highest (root first).
    ///
    /// For correct enharmonic spelling, a Key context should be provided.
    /// Without a key, notes will use sharp/flat based on the root note's preference.
    #[must_use]
    pub fn notes(&self, key: Option<&Key>) -> Option<Vec<MusicalNote>> {
        // Get the root note
        let root = self.root_note(key)?;

        // Build a list of (semitones, chord_degree) pairs and sort by semitones
        let mut degree_semitone_pairs: Vec<(u8, ChordDegree)> = self
            .semantic_degrees
            .iter()
            .filter_map(|&degree| {
                self.intervals
                    .get(&degree)
                    .map(|interval| (interval.semitones(), degree))
            })
            .collect();

        // Sort by semitone (ascending) - this preserves octave ordering
        degree_semitone_pairs.sort_by_key(|(semitones, _)| *semitones);

        // Generate notes using enharmonically correct spelling based on chord degrees
        let mut notes = Vec::with_capacity(degree_semitone_pairs.len());

        for (semitones, chord_degree) in degree_semitone_pairs {
            // Use semantic interval to determine correct letter name
            let semantic_interval = chord_degree.semantic_interval();

            // Generate enharmonically correct note
            let note = MusicalNote::enharmonic_from_root(&root, semitones % 12, semantic_interval);
            notes.push(note);
        }

        Some(notes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::quality::ChordQuality;

    #[test]
    fn test_basic_intervals() {
        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::new(root, ChordQuality::Major);

        assert!(chord.has_degree(ChordDegree::Root));
        assert!(chord.has_degree(ChordDegree::Third));
        assert!(chord.has_degree(ChordDegree::Fifth));
        assert!(!chord.has_degree(ChordDegree::Seventh));
    }

    #[test]
    fn test_semitone_sequence() {
        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::new(root, ChordQuality::Major);

        // C major = [0, 4, 7]
        assert_eq!(chord.semitone_sequence(), vec![0, 4, 7]);
    }

    #[test]
    fn test_pitch_classes() {
        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::new(root, ChordQuality::Major);

        assert_eq!(chord.pitch_classes(), vec![0, 4, 7]);
    }

    #[test]
    fn test_notes_basic() {
        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::new(root, ChordQuality::Major);

        let notes = chord.notes(None).unwrap();
        assert_eq!(notes.len(), 3);
        // C, E, G
    }
}
