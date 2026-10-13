//! Chord transposition logic.
//!
//! This module contains the transposition implementation for [`Chord`],
//! allowing chords to be transposed to different keys while preserving
//! their musical structure.

use super::definition::Chord;
use super::degree::ChordDegree;
use super::extensions::{ExtensionQuality, Extensions};
use super::family::ChordFamily;
use super::quality::ChordQuality;
use crate::key::Key;
use crate::primitives::{Interval, MusicalNote, RootNotation};
use std::collections::HashMap;

// ============================================================================
// Transposition
// ============================================================================

impl Chord {
    /// Transpose this chord to a new key
    ///
    /// Unified algorithm that works for all transposition scenarios:
    /// 1. **Root-only transposition** (C Major → G Major): Transposes by interval
    /// 2. **Scale-type change** (C Major → C Minor): Applies enharmonic mapping
    /// 3. **Both** (C Major → G Minor): Combines both
    ///
    /// The algorithm:
    /// 1. Get all notes in the chord
    /// 2. Transpose each note by the interval between source and target roots
    /// 3. Apply enharmonic changes based on the target scale
    ///    (e.g., E→Eb when going to C minor, A→Ab, B→Bb)
    /// 4. Recalculate chord quality from the resulting notes
    ///
    /// # Arguments
    /// * `target_key` - The key to transpose to (provides both root and scale type)
    /// * `source_key` - Optional source key for resolving scale degrees/roman numerals
    ///
    /// # Returns
    /// * `Some(Chord)` - The transposed chord
    /// * `None` - If the root cannot be resolved
    ///
    /// # Examples
    /// ```
    /// use keyflow_proto::chord::from_semitones;
    /// use keyflow_proto::primitives::{RootNotation, MusicalNote};
    /// use keyflow_proto::key::Key;
    ///
    /// // Root transposition: Cmaj7 → Gmaj7
    /// let root = RootNotation::from_note_name(MusicalNote::c());
    /// // Cmaj7 = C (0), E (4), G (7), B (11)
    /// let c_maj7 = from_semitones(&[0, 4, 7, 11], root).unwrap();
    /// let g_key = Key::major(MusicalNote::g());
    /// let g_maj7 = c_maj7.transpose_to(&g_key, None).unwrap();
    ///
    /// // Scale type change: C → Cm (C Major → C Minor)
    /// let c_root = RootNotation::from_note_name(MusicalNote::c());
    /// let c_major = from_semitones(&[0, 4, 7], c_root).unwrap();
    /// let c_key = Key::major(MusicalNote::c());
    /// let c_minor_key = Key::minor(MusicalNote::c());
    /// let c_minor_chord = c_major.transpose_to(&c_minor_key, Some(&c_key)).unwrap();
    /// ```
    pub fn transpose_to(&self, target_key: &Key, source_key: Option<&Key>) -> Option<Self> {
        // Get the current root note
        let current_root = self.root_note(source_key)?;
        let target_root = target_key.root();

        // Check if the scale mode changed - if so, we need to map scale degrees
        let source_scale_mode = source_key.map(|k| k.mode);
        let target_scale_mode = target_key.mode;
        let scale_mode_changed = source_scale_mode.is_some_and(|m| m != target_scale_mode);

        // Calculate the transposition interval
        // If scale mode changed and the chord root is a scale degree in the source key,
        // we need to find what scale degree it is and map to that degree in target key
        let interval_semitones = if let (true, Some(src_key)) = (scale_mode_changed, source_key) {
            // Check if current root is a scale degree in source key
            let mut found_scale_degree = None;
            for deg in 1..=7 {
                if let Some(scale_note) = src_key.get_scale_degree(deg)
                    && scale_note.semitone == current_root.semitone
                {
                    found_scale_degree = Some(deg);
                    break;
                }
            }

            if let Some(degree) = found_scale_degree {
                // Map to the same degree in target key
                if let Some(target_scale_note) = target_key.get_scale_degree(degree) {
                    (target_scale_note.semitone + 12 - current_root.semitone) % 12
                } else {
                    (target_root.semitone + 12 - current_root.semitone) % 12
                }
            } else {
                (target_root.semitone + 12 - current_root.semitone) % 12
            }
        } else {
            (target_root.semitone + 12 - current_root.semitone) % 12
        };

        // Step 1: Transpose all notes by the musical note interval
        // This changes E → Eb, but doesn't change chord quality
        let mut transposed_notes: Vec<MusicalNote> = Vec::new();

        for &degree in &self.semantic_degrees {
            if let Some(interval) = self.intervals.get(&degree) {
                let semantic_interval = degree.semantic_interval();
                let semitone_offset = interval.semitones();
                let new_semitone =
                    (current_root.semitone + semitone_offset + interval_semitones) % 12;

                // For the root degree
                if degree == ChordDegree::Root {
                    // Calculate the transposed root semitone
                    let transposed_root_semitone =
                        (current_root.semitone + interval_semitones) % 12;

                    // Try to find this in the target scale
                    let mut found_root = false;
                    for scale_deg in 1..=7 {
                        if let Some(scale_note) = target_key.get_scale_degree(scale_deg)
                            && scale_note.semitone == transposed_root_semitone
                        {
                            transposed_notes.push(scale_note);
                            found_root = true;
                            break;
                        }
                    }

                    if !found_root {
                        // Not in scale - use enharmonic spelling
                        let note = MusicalNote::enharmonic_from_root(
                            target_root,
                            transposed_root_semitone,
                            1,
                        );
                        transposed_notes.push(note);
                    }
                } else {
                    // For other notes, try to find in target scale first
                    let mut found_in_scale = false;
                    for scale_deg in 1..=7 {
                        if let Some(scale_note) = target_key.get_scale_degree(scale_deg)
                            && scale_note.semitone == new_semitone
                        {
                            transposed_notes.push(scale_note);
                            found_in_scale = true;
                            break;
                        }
                    }

                    if !found_in_scale {
                        // Not in scale - use enharmonic spelling
                        let note = MusicalNote::enharmonic_from_root(
                            target_root,
                            new_semitone,
                            semantic_interval,
                        );
                        transposed_notes.push(note);
                    }
                }
            }
        }

        // Step 2: If scale mode changed, apply scale transformations
        // This changes chord quality (Major → Minor, etc.)
        // We need to compare the actual ScaleMode, not just ScaleType,
        // because Ionian and Aeolian are both Diatonic but different modes
        let source_scale_mode = source_key.map(|k| k.mode);
        let target_scale_mode = target_key.mode;
        let scale_mode_changed = source_scale_mode.is_some_and(|m| m != target_scale_mode);

        if scale_mode_changed {
            let mut scale_transformed_notes: Vec<MusicalNote> = Vec::new();

            // Find where the chord root is in the target scale
            let new_chord_root = &transposed_notes[0];
            let mut chord_root_scale_degree = None;
            for deg in 1..=7 {
                if let Some(scale_note) = target_key.get_scale_degree(deg)
                    && scale_note.semitone == new_chord_root.semitone
                {
                    chord_root_scale_degree = Some(deg);
                    break;
                }
            }

            if let Some(root_deg) = chord_root_scale_degree {
                // For each chord tone, count up from the chord root's position in the scale
                // This includes extensions (9th, 11th, 13th) which we handle by:
                // 1. Flattening to get the scale degree (9 → 2, 11 → 4, 13 → 6)
                // 2. Transforming through the scale
                // 3. Preserving the octave offset

                for &degree in &self.semantic_degrees {
                    if let Some(interval) = self.intervals.get(&degree) {
                        let original_semitones = interval.semitones();
                        let octave_offset = (original_semitones / 12) * 12;

                        // Use semantic_interval which already flattens (9th→2, 11th→4, 13th→6)
                        let semantic_interval = degree.semantic_interval();

                        // Calculate which scale degree this chord tone should map to
                        let target_scale_deg = ((root_deg - 1) + (semantic_interval - 1)) % 7 + 1;

                        if let Some(scale_note) = target_key.get_scale_degree(target_scale_deg) {
                            // Calculate the new semitone value with octave preserved
                            let new_semitone_within_octave =
                                (scale_note.semitone + 12 - new_chord_root.semitone) % 12;
                            let new_total_semitones = new_semitone_within_octave + octave_offset;

                            // Create a note with the transformed pitch but preserve the name from scale
                            let mut transformed_note = scale_note;
                            // The semitone is used for sorting and interval calculation
                            // but the name (e.g., "Ab") comes from the scale
                            transformed_note.semitone = new_total_semitones;

                            scale_transformed_notes.push(transformed_note);
                        }
                    }
                }

                transposed_notes = scale_transformed_notes;
            }
            // If chord root not in scale, keep original transposed notes
        }

        // The first note is the new root
        // If the root has semitone 0 (relative), we need to find its absolute semitone
        // by looking up its name in the target key
        let root_note_for_notation = if transposed_notes[0].semitone == 0 {
            // Root is relative (semitone 0), find absolute semitone by looking up the name
            let root_name = &transposed_notes[0].name;
            let mut absolute_note = transposed_notes[0].clone();
            // Search for this note name in the target scale
            for deg in 1..=7 {
                if let Some(scale_note) = target_key.get_scale_degree(deg)
                    && scale_note.name == *root_name
                {
                    absolute_note.semitone = scale_note.semitone;
                    break;
                }
            }
            absolute_note
        } else {
            transposed_notes[0].clone()
        };
        let new_root = RootNotation::from_note_name(root_note_for_notation);

        // Recalculate quality from the transposed notes
        let new_quality = self.calculate_quality_from_notes(&transposed_notes)?;

        // Recalculate family if needed
        let new_family = if let (true, Some(family)) = (scale_mode_changed, self.family) {
            // If quality changed, we need to update the family
            if new_quality != self.quality {
                match (new_quality, family) {
                    (ChordQuality::Major, ChordFamily::Minor7) => Some(ChordFamily::Dominant7),
                    (ChordQuality::Major, ChordFamily::MinorMajor7) => Some(ChordFamily::Major7),
                    (ChordQuality::Minor, ChordFamily::Major7) => Some(ChordFamily::MinorMajor7),
                    (ChordQuality::Minor, ChordFamily::Dominant7) => Some(ChordFamily::Minor7),
                    _ => self.family,
                }
            } else {
                self.family
            }
        } else {
            self.family
        };

        // After scale transformation, recalculate extensions to be all Natural
        // because the transformed notes are now the natural scale degrees in the target scale
        let new_extensions = if scale_mode_changed && self.extensions.has_any() {
            let mut ext = Extensions::none();
            if self.extensions.ninth.is_some() {
                ext.ninth = Some(ExtensionQuality::Natural);
            }
            if self.extensions.eleventh.is_some() {
                ext.eleventh = Some(ExtensionQuality::Natural);
            }
            if self.extensions.thirteenth.is_some() {
                ext.thirteenth = Some(ExtensionQuality::Natural);
            }
            ext
        } else {
            self.extensions.clone()
        };

        // Create a new chord with transposed properties
        let mut transposed = Self {
            origin: String::new(),
            descriptor: self.descriptor.clone(),
            normalized: String::new(),
            root: new_root,
            quality: new_quality,
            family: new_family,
            extensions: new_extensions,
            alterations: self.alterations.clone(),
            additions: self.additions.clone(),
            omissions: self.omissions.clone(),
            bass: if let Some(ref bass) = self.bass {
                self.transpose_root_notation(bass, interval_semitones)
            } else {
                None
            },
            bass_vertical: self.bass_vertical,
            duration: self.duration.clone(),
            intervals: HashMap::new(),
            semantic_degrees: Vec::new(),
            tokens_consumed: 0,
        };

        // If scale was transformed, build intervals from the transformed notes
        // Otherwise, compute intervals normally
        if scale_mode_changed && !transposed_notes.is_empty() {
            // Build intervals map from the transformed notes
            let new_root_semitone = transposed_notes[0].semitone;
            for (i, &degree) in self.semantic_degrees.iter().enumerate() {
                if i < transposed_notes.len() {
                    let note_semitone = transposed_notes[i].semitone;
                    // Calculate semitone difference (preserving octave information)
                    let semitone_diff = (note_semitone + 120 - new_root_semitone) % 120;

                    // Map the semitone difference to the appropriate Interval variant
                    // For extensions (9th, 11th, 13th), we need to use the extension-specific variants
                    let interval = match (degree, semitone_diff % 12) {
                        // 9ths
                        (ChordDegree::Ninth, 1) => Some(Interval::FlatNinth),
                        (ChordDegree::Ninth, 2) => Some(Interval::Ninth),
                        (ChordDegree::Ninth, 3) => Some(Interval::SharpNinth),
                        // 11ths
                        (ChordDegree::Eleventh, 5) => Some(Interval::Eleventh),
                        (ChordDegree::Eleventh, 6) => Some(Interval::SharpEleventh),
                        // 13ths
                        (ChordDegree::Thirteenth, 8) => Some(Interval::FlatThirteenth),
                        (ChordDegree::Thirteenth, 9) => Some(Interval::Thirteenth),
                        // Everything else use from_semitones
                        _ => Interval::from_semitones(semitone_diff % 12),
                    };

                    if let Some(interval) = interval {
                        transposed.intervals.insert(degree, interval);
                    }
                }
            }
            transposed.semantic_degrees = self.semantic_degrees.clone();
        } else {
            // Recompute intervals normally
            transposed.compute_intervals();
        }

        transposed.normalize();

        Some(transposed)
    }

    /// Helper: Transpose a root notation by a semitone interval
    fn transpose_root_notation(&self, root: &RootNotation, semitones: u8) -> Option<RootNotation> {
        if let Some(note) = root.resolve(None) {
            let new_semitone = (note.semitone + semitones) % 12;
            let new_note = MusicalNote::from_semitone(new_semitone, note.name.contains('#'));
            Some(RootNotation::from_note_name(new_note))
        } else {
            None
        }
    }

    /// Helper: Calculate chord quality from a set of notes (for scale-type transposition)
    ///
    /// Examines the interval between root and third to determine quality
    fn calculate_quality_from_notes(&self, notes: &[MusicalNote]) -> Option<ChordQuality> {
        if notes.is_empty() {
            return None;
        }

        let root = &notes[0];

        // Find the third (if present)
        for note in notes.iter().skip(1) {
            let interval = (note.semitone + 12 - root.semitone) % 12;
            match interval {
                3 => return Some(ChordQuality::Minor), // Minor third
                4 => return Some(ChordQuality::Major), // Major third
                _ => continue,
            }
        }

        // No third found, keep original quality
        Some(self.quality)
    }

    /// Get the scale degrees of each chord tone in a given key
    ///
    /// Returns a vector of tuples mapping each ChordDegree to its scale degree (1-7)
    /// in the given key. This is useful for understanding the chord's function within
    /// a key (e.g., "this chord has the 3rd and 7th of the key").
    ///
    /// # Arguments
    /// * `key` - The key to analyze the chord in
    ///
    /// # Returns
    /// * `Some(Vec<(ChordDegree, u8)>)` if the chord can be analyzed in this key
    /// * `None` if the root cannot be resolved
    ///
    /// # Example
    /// ```
    /// use keyflow_proto::chord::from_semitones;
    /// use keyflow_proto::primitives::{RootNotation, MusicalNote};
    /// use keyflow_proto::key::Key;
    /// use keyflow_proto::chord::ChordDegree;
    ///
    /// // Cmaj7 in C major: C=1, E=3, G=5, B=7
    /// let root = RootNotation::from_note_name(MusicalNote::c());
    /// // Cmaj7 = C (0), E (4), G (7), B (11)
    /// let chord = from_semitones(&[0, 4, 7, 11], root).unwrap();
    /// let degrees = chord.scale_degrees(&Key::major(MusicalNote::c())).unwrap();
    /// assert_eq!(degrees[0], (ChordDegree::Root, 1));
    /// assert_eq!(degrees[1], (ChordDegree::Third, 3));
    /// ```
    pub fn scale_degrees(&self, key: &Key) -> Option<Vec<(ChordDegree, u8)>> {
        let key_root = key.root();

        // Map each chord degree to its scale degree in the key
        let mut result = Vec::with_capacity(self.semantic_degrees.len());

        // Iterate through semantic_degrees (these are the chord tones we have)
        for chord_degree in &self.semantic_degrees {
            // Get the interval for this chord degree
            if let Some(interval) = self.intervals.get(chord_degree) {
                // Get the actual note for this interval
                let root_note = self.root_note(Some(key))?;
                let note_semitone = (root_note.semitone + interval.semitones()) % 12;

                // Calculate the semitone distance from the key root
                let interval_from_key = (note_semitone + 12 - key_root.semitone) % 12;

                // Map semitones to scale degrees (1-7)
                // This is approximate - in a real implementation you'd check against the actual scale
                let scale_degree = match interval_from_key {
                    0 => 1,       // Root
                    1 | 2 => 2,   // Second (major or minor)
                    3 | 4 => 3,   // Third (minor or major)
                    5 => 4,       // Fourth
                    6 | 7 => 5,   // Fifth (dim, perfect, or aug)
                    8 | 9 => 6,   // Sixth (minor or major)
                    10 | 11 => 7, // Seventh (minor or major)
                    _ => 1,       // Fallback (shouldn't reach here)
                };

                result.push((*chord_degree, scale_degree));
            }
        }

        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transpose_to_same_key() {
        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::new(root, ChordQuality::Major);
        let c_key = Key::major(MusicalNote::c());

        let transposed = chord.transpose_to(&c_key, None).unwrap();

        assert_eq!(transposed.root.to_string(), "C");
        assert_eq!(transposed.quality, ChordQuality::Major);
    }

    #[test]
    fn test_transpose_c_to_g() {
        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::new(root, ChordQuality::Major);
        let g_key = Key::major(MusicalNote::g());

        let transposed = chord.transpose_to(&g_key, None).unwrap();

        assert_eq!(transposed.root.to_string(), "G");
        assert_eq!(transposed.quality, ChordQuality::Major);
    }

    #[test]
    fn test_scale_degrees_in_c_major() {
        let root = RootNotation::from_note_name(MusicalNote::c());
        let chord = Chord::new(root, ChordQuality::Major);
        let c_key = Key::major(MusicalNote::c());

        let degrees = chord.scale_degrees(&c_key).unwrap();

        // C major triad in C major should have: root=1, third=3, fifth=5
        assert!(
            degrees
                .iter()
                .any(|(d, s)| *d == ChordDegree::Root && *s == 1)
        );
        assert!(
            degrees
                .iter()
                .any(|(d, s)| *d == ChordDegree::Third && *s == 3)
        );
        assert!(
            degrees
                .iter()
                .any(|(d, s)| *d == ChordDegree::Fifth && *s == 5)
        );
    }
}
