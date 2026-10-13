//! Scale harmonization - generate chords from scale degrees
//!
//! This module provides utilities to:
//! - Get all notes from a scale with a given root
//! - Build chords (triads through 13ths) at each scale degree
//! - Analyze harmonic structure of any scale mode
use super::ScaleMode;
use crate::chord::{Chord, from_semitones};
use crate::primitives::{MusicalNote, Note, RootNotation};
use facet::Facet;

/// Depth of chord harmonization
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub enum HarmonizationDepth {
    /// Triads only (3 notes: 1-3-5)
    Triads,
    /// Seventh chords (4 notes: 1-3-5-7)
    Sevenths,
    /// Ninth chords (5 notes: 1-3-5-7-9)
    Ninths,
    /// Eleventh chords (6 notes: 1-3-5-7-9-11)
    Elevenths,
    /// Thirteenth chords (7 notes: 1-3-5-7-9-11-13)
    Thirteenths,
}

impl HarmonizationDepth {
    /// Get the number of notes to stack when building chords
    pub fn note_count(&self) -> usize {
        match self {
            HarmonizationDepth::Triads => 3,
            HarmonizationDepth::Sevenths => 4,
            HarmonizationDepth::Ninths => 5,
            HarmonizationDepth::Elevenths => 6,
            HarmonizationDepth::Thirteenths => 7,
        }
    }

    /// Get a display name for this depth
    pub fn name(&self) -> &'static str {
        match self {
            HarmonizationDepth::Triads => "Triads",
            HarmonizationDepth::Sevenths => "Sevenths",
            HarmonizationDepth::Ninths => "Ninths",
            HarmonizationDepth::Elevenths => "Elevenths",
            HarmonizationDepth::Thirteenths => "Thirteenths",
        }
    }
}

/// A harmonized scale with all its chords
#[derive(Debug, Clone, Facet)]
pub struct ScaleHarmonization {
    /// The scale mode used
    pub mode: ScaleMode,
    /// The root note
    pub root: MusicalNote,
    /// All notes in the scale (semitones relative to root)
    pub scale_semitones: Vec<u8>,
    /// All notes in the scale (actual note names)
    pub scale_notes: Vec<MusicalNote>,
    /// Harmonization depth
    pub depth: HarmonizationDepth,
    /// Chords at each scale degree (7 chords)
    pub chords: Vec<Chord>,
}

impl ScaleHarmonization {
    /// Create a new scale harmonization
    pub fn new(mode: ScaleMode, root: MusicalNote, depth: HarmonizationDepth) -> Self {
        let scale_semitones = mode.interval_pattern();
        let scale_notes = generate_scale_notes(&root, &scale_semitones);
        let chords = harmonize_scale(&mode, &root, depth);

        Self {
            mode,
            root,
            scale_semitones,
            scale_notes,
            depth,
            chords,
        }
    }

    /// Get chord at a specific scale degree (1-7)
    pub fn chord_at_degree(&self, degree: usize) -> Option<&Chord> {
        if degree == 0 || degree > 7 {
            return None;
        }
        self.chords.get(degree - 1)
    }

    /// Get all chord names in the scale
    pub fn chord_names(&self) -> Vec<String> {
        self.chords.iter().map(|c| format!("{}", c)).collect()
    }

    /// Print a formatted chord chart for this scale
    pub fn format_chord_chart(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!(
            "{} {} ({})\n",
            self.root.name(),
            self.mode.name(),
            self.depth.name()
        ));
        output.push_str(&"=".repeat(60));
        output.push('\n');

        for (i, chord) in self.chords.iter().enumerate() {
            let degree = i + 1;
            let scale_note = &self.scale_notes[i];
            output.push_str(&format!(
                "  {}. {} - {}\n",
                degree,
                scale_note.name(),
                chord
            ));
        }

        output
    }
}

/// Generate all notes in a scale given a root and interval pattern
///
/// This function ensures proper enharmonic spelling by using each letter name exactly once.
/// For example, in Bb major, we use Bb-C-D-Eb-F-G-A (not Bb-C-D-D#-F-G-A).
///
/// Sharp keys prefer sharps, flat keys prefer flats.
pub fn generate_scale_notes(root: &MusicalNote, interval_pattern: &[u8]) -> Vec<MusicalNote> {
    use crate::primitives::Accidental;

    // Determine if this is a sharp or flat key based on the root
    let _prefer_sharps = match root.accidental {
        Some(Accidental::Sharp) | Some(Accidental::DoubleSharp) => true,
        Some(Accidental::Flat) | Some(Accidental::DoubleFlat) => false,
        None | Some(Accidental::Natural) => {
            // Natural roots: use circle of fifths logic
            // F keys prefer flats; C, G, D, A, E, B keys prefer sharps
            matches!(root.letter, 'G' | 'D' | 'A' | 'E' | 'B' | 'C')
        }
    };

    // Start with the root's letter and proceed through the musical alphabet
    let letters = ['C', 'D', 'E', 'F', 'G', 'A', 'B'];
    let root_letter_index = letters.iter().position(|&l| l == root.letter).unwrap();

    let mut result = Vec::new();

    for (i, &semitone_offset) in interval_pattern.iter().enumerate() {
        // For the first degree, use the root note itself to preserve its spelling
        if i == 0 {
            result.push(root.clone());
            continue;
        }

        // Calculate which letter we should use (cycle through C-D-E-F-G-A-B)
        let letter_index = (root_letter_index + i) % 7;
        let expected_letter = letters[letter_index];

        // Calculate the actual semitone
        let target_semitone = (root.semitone + semitone_offset) % 12;

        // Find the accidental needed to reach the target semitone from this letter
        let letter_base_semitone = match expected_letter {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => 0, // Shouldn't happen
        };

        // Calculate how many semitones up or down from the letter's natural position
        let mut semitone_diff = (target_semitone as i8 - letter_base_semitone as i8 + 12) % 12;
        if semitone_diff > 6 {
            semitone_diff -= 12;
        }

        // Convert semitone difference to accidental, respecting the key's preference
        // If prefer_sharps is true and diff is ambiguous, prefer sharps over flats
        let accidental = match semitone_diff {
            -2 => Some(Accidental::DoubleFlat),
            -1 => Some(Accidental::Flat),
            0 => None,
            1 => Some(Accidental::Sharp),
            2 => Some(Accidental::DoubleSharp),
            _ => None, // Shouldn't happen in normal scales
        };

        result.push(MusicalNote::from_letter_and_accidental(
            expected_letter,
            accidental,
        ));
    }

    result
}

/// Generate all notes in a scale as semitones only
pub fn generate_scale_semitones(interval_pattern: &[u8]) -> Vec<u8> {
    interval_pattern.to_vec()
}

/// Harmonize a scale - build chords at each degree
///
/// # Arguments
/// * `mode` - The scale mode to harmonize
/// * `root` - The root note of the scale
/// * `depth` - How deep to stack the chords (triads, sevenths, ninths, etc.)
///
/// # Returns
/// A vector of 7 chords, one for each scale degree
pub fn harmonize_scale(
    mode: &ScaleMode,
    root: &MusicalNote,
    depth: HarmonizationDepth,
) -> Vec<Chord> {
    let scale_pattern = mode.interval_pattern();
    let note_count = depth.note_count();

    let mut chords = Vec::new();

    // Build a chord at each scale degree (1-7)
    for degree in 0..7 {
        let chord = build_chord_at_degree(&scale_pattern, root, degree, note_count);
        chords.push(chord);
    }

    chords
}

/// Build a single chord at a specific scale degree
///
/// # Arguments
/// * `scale_pattern` - The semitone intervals of the scale
/// * `root` - The root note of the scale (not the chord)
/// * `degree` - Which scale degree to build from (0-6 for degrees 1-7)
/// * `note_count` - How many notes to stack (3=triad, 4=seventh, etc.)
///
/// # Returns
/// A Chord built from the scale intervals
fn build_chord_at_degree(
    scale_pattern: &[u8],
    root: &MusicalNote,
    degree: usize,
    note_count: usize,
) -> Chord {
    // First, generate the properly spelled scale notes
    let scale_notes = generate_scale_notes(root, scale_pattern);

    // Stack notes in thirds (every other note in the scale)
    let mut chord_semitones = Vec::new();

    for i in 0..note_count {
        let scale_index = (degree + (i * 2)) % 7;
        chord_semitones.push(scale_pattern[scale_index]);
    }

    // Normalize to start from 0 (chord root)
    let chord_root_semitone = chord_semitones[0];
    let normalized_semitones: Vec<u8> = chord_semitones
        .iter()
        .map(|&s| {
            if s >= chord_root_semitone {
                s - chord_root_semitone
            } else {
                s + 12 - chord_root_semitone
            }
        })
        .collect();

    // Get the chord root from the properly spelled scale notes
    let chord_root_note = scale_notes[degree].clone();

    // Build the chord from the semitone sequence
    let chord_root_notation = RootNotation::from_note_name(chord_root_note);

    // Use our semitone-to-chord converter
    from_semitones(&normalized_semitones, chord_root_notation.clone()).unwrap_or_else(|_| {
        // Fallback: if we can't recognize the pattern, create a basic chord
        // This shouldn't happen for normal scales, but good to be safe
        use crate::chord::ChordQuality;
        Chord::new(chord_root_notation, ChordQuality::Major)
    })
}

/// Get a quick analysis of a scale's harmonization
pub fn analyze_scale_harmony(
    mode: ScaleMode,
    root: MusicalNote,
    depth: HarmonizationDepth,
) -> String {
    let harmonization = ScaleHarmonization::new(mode, root, depth);
    harmonization.format_chord_chart()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c_note() -> MusicalNote {
        MusicalNote::from_string("C").unwrap()
    }

    #[test]
    fn test_generate_scale_notes_c_major() {
        let root = c_note();
        let pattern = vec![0, 2, 4, 5, 7, 9, 11]; // C major
        let notes = generate_scale_notes(&root, &pattern);

        assert_eq!(notes.len(), 7);
        // First note should be C
        assert_eq!(notes[0].semitone, 0);
    }

    #[test]
    fn test_harmonize_c_major_triads() {
        let mode = ScaleMode::ionian();
        let root = c_note();
        let chords = harmonize_scale(&mode, &root, HarmonizationDepth::Triads);

        assert_eq!(chords.len(), 7);

        // Check the chord at degree I (should be major)
        assert_eq!(chords[0].quality, crate::chord::ChordQuality::Major);

        // Check the chord at degree ii (should be minor)
        assert_eq!(chords[1].quality, crate::chord::ChordQuality::Minor);

        // Check the chord at degree vii° (should be diminished)
        assert_eq!(chords[6].quality, crate::chord::ChordQuality::Diminished);
    }

    #[test]
    fn test_harmonize_c_major_sevenths() {
        let mode = ScaleMode::ionian();
        let root = c_note();
        let chords = harmonize_scale(&mode, &root, HarmonizationDepth::Sevenths);

        assert_eq!(chords.len(), 7);

        // Ionian (Major) scale seventh chord qualities:
        // I: Major7, ii: Minor7, iii: Minor7, IV: Major7, V: Dominant7, vi: Minor7, vii°: HalfDiminished

        // I - Cmaj7
        assert_eq!(chords[0].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[0].family, Some(crate::chord::ChordFamily::Major7));

        // ii - Dm7
        assert_eq!(chords[1].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[1].family, Some(crate::chord::ChordFamily::Minor7));

        // iii - Em7
        assert_eq!(chords[2].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[2].family, Some(crate::chord::ChordFamily::Minor7));

        // IV - Fmaj7
        assert_eq!(chords[3].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[3].family, Some(crate::chord::ChordFamily::Major7));

        // V - G7 (dominant)
        assert_eq!(chords[4].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[4].family, Some(crate::chord::ChordFamily::Dominant7));

        // vi - Am7
        assert_eq!(chords[5].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[5].family, Some(crate::chord::ChordFamily::Minor7));

        // vii° - Bm7b5 (half-diminished)
        assert_eq!(chords[6].quality, crate::chord::ChordQuality::Diminished);
        assert_eq!(
            chords[6].family,
            Some(crate::chord::ChordFamily::HalfDiminished)
        );
    }

    #[test]
    fn test_harmonize_harmonic_minor_triads() {
        let mode = ScaleMode::harmonic_minor();
        let root = c_note();
        let chords = harmonize_scale(&mode, &root, HarmonizationDepth::Triads);

        assert_eq!(chords.len(), 7);

        // i should be minor
        assert_eq!(chords[0].quality, crate::chord::ChordQuality::Minor);

        // III+ should be augmented
        assert_eq!(chords[2].quality, crate::chord::ChordQuality::Augmented);
    }

    #[test]
    fn test_scale_harmonization_struct() {
        let mode = ScaleMode::ionian();
        let root = c_note();
        let harmonization = ScaleHarmonization::new(mode, root, HarmonizationDepth::Sevenths);

        assert_eq!(harmonization.chords.len(), 7);
        assert_eq!(harmonization.scale_notes.len(), 7);
        assert_eq!(harmonization.scale_semitones.len(), 7);

        // Test chord_at_degree
        assert!(harmonization.chord_at_degree(1).is_some());
        assert!(harmonization.chord_at_degree(7).is_some());
        assert!(harmonization.chord_at_degree(0).is_none());
        assert!(harmonization.chord_at_degree(8).is_none());
    }

    #[test]
    fn test_harmonization_depth() {
        assert_eq!(HarmonizationDepth::Triads.note_count(), 3);
        assert_eq!(HarmonizationDepth::Sevenths.note_count(), 4);
        assert_eq!(HarmonizationDepth::Ninths.note_count(), 5);
        assert_eq!(HarmonizationDepth::Elevenths.note_count(), 6);
        assert_eq!(HarmonizationDepth::Thirteenths.note_count(), 7);
    }

    #[test]
    fn test_format_chord_chart() {
        let mode = ScaleMode::ionian();
        let root = c_note();
        let harmonization = ScaleHarmonization::new(mode, root, HarmonizationDepth::Triads);

        let chart = harmonization.format_chord_chart();
        assert!(chart.contains("Ionian"));
        assert!(chart.contains("Triads"));
    }

    #[test]
    fn test_all_harmonization_depths_c_major() {
        let mode = ScaleMode::ionian();
        let root = c_note();

        for depth in [
            HarmonizationDepth::Triads,
            HarmonizationDepth::Sevenths,
            HarmonizationDepth::Ninths,
            HarmonizationDepth::Elevenths,
            HarmonizationDepth::Thirteenths,
        ] {
            let harmonization = ScaleHarmonization::new(mode, root.clone(), depth);
            assert_eq!(harmonization.chords.len(), 7);

            // All chords should have been successfully created
            for chord in &harmonization.chords {
                // Just verify we got valid chords (not panicking is good enough)
                let _ = chord.semitone_sequence();
            }
        }
    }

    #[test]
    fn test_dorian_mode_harmonization() {
        let mode = ScaleMode::dorian();
        let root = c_note();
        let chords = harmonize_scale(&mode, &root, HarmonizationDepth::Sevenths);

        assert_eq!(chords.len(), 7);

        // Dorian scale seventh chord qualities:
        // i: Minor7, ii: Minor7, III: Major7, IV: Dominant7, v: Minor7, vi°: HalfDiminished, VII: Major7

        // i - Cm7
        assert_eq!(chords[0].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[0].family, Some(crate::chord::ChordFamily::Minor7));

        // ii - Dm7
        assert_eq!(chords[1].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[1].family, Some(crate::chord::ChordFamily::Minor7));

        // III - E♭maj7
        assert_eq!(chords[2].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[2].family, Some(crate::chord::ChordFamily::Major7));

        // IV - F7 (dominant)
        assert_eq!(chords[3].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[3].family, Some(crate::chord::ChordFamily::Dominant7));

        // v - Gm7
        assert_eq!(chords[4].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[4].family, Some(crate::chord::ChordFamily::Minor7));

        // vi° - Am7b5 (half-diminished)
        assert_eq!(chords[5].quality, crate::chord::ChordQuality::Diminished);
        assert_eq!(
            chords[5].family,
            Some(crate::chord::ChordFamily::HalfDiminished)
        );

        // VII - B♭maj7
        assert_eq!(chords[6].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[6].family, Some(crate::chord::ChordFamily::Major7));
    }

    #[test]
    fn test_phrygian_mode_harmonization() {
        let mode = ScaleMode::phrygian();
        let root = c_note();
        let chords = harmonize_scale(&mode, &root, HarmonizationDepth::Sevenths);

        assert_eq!(chords.len(), 7);

        // Phrygian scale seventh chord qualities:
        // i: Minor7, II: Major7, III: Dominant7, iv: Minor7, v°: HalfDiminished, VI: Major7, VII: Minor7
        assert_eq!(chords[0].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[0].family, Some(crate::chord::ChordFamily::Minor7));

        assert_eq!(chords[1].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[1].family, Some(crate::chord::ChordFamily::Major7));

        assert_eq!(chords[2].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[2].family, Some(crate::chord::ChordFamily::Dominant7));

        assert_eq!(chords[3].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[3].family, Some(crate::chord::ChordFamily::Minor7));

        assert_eq!(chords[4].quality, crate::chord::ChordQuality::Diminished);
        assert_eq!(
            chords[4].family,
            Some(crate::chord::ChordFamily::HalfDiminished)
        );

        assert_eq!(chords[5].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[5].family, Some(crate::chord::ChordFamily::Major7));

        assert_eq!(chords[6].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[6].family, Some(crate::chord::ChordFamily::Minor7));
    }

    #[test]
    fn test_lydian_mode_harmonization() {
        let mode = ScaleMode::lydian();
        let root = c_note();
        let chords = harmonize_scale(&mode, &root, HarmonizationDepth::Sevenths);

        assert_eq!(chords.len(), 7);

        // Lydian scale seventh chord qualities:
        // I: Major7, II: Dominant7, iii: Minor7, iv°: HalfDiminished, V: Major7, vi: Minor7, vii: Minor7
        assert_eq!(chords[0].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[0].family, Some(crate::chord::ChordFamily::Major7));

        assert_eq!(chords[1].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[1].family, Some(crate::chord::ChordFamily::Dominant7));

        assert_eq!(chords[2].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[2].family, Some(crate::chord::ChordFamily::Minor7));

        assert_eq!(chords[3].quality, crate::chord::ChordQuality::Diminished);
        assert_eq!(
            chords[3].family,
            Some(crate::chord::ChordFamily::HalfDiminished)
        );

        assert_eq!(chords[4].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[4].family, Some(crate::chord::ChordFamily::Major7));

        assert_eq!(chords[5].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[5].family, Some(crate::chord::ChordFamily::Minor7));

        assert_eq!(chords[6].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[6].family, Some(crate::chord::ChordFamily::Minor7));
    }

    #[test]
    fn test_mixolydian_mode_harmonization() {
        let mode = ScaleMode::mixolydian();
        let root = c_note();
        let chords = harmonize_scale(&mode, &root, HarmonizationDepth::Sevenths);

        assert_eq!(chords.len(), 7);

        // Mixolydian scale seventh chord qualities:
        // I: Dominant7, ii: Minor7, iii°: HalfDiminished, IV: Major7, v: Minor7, vi: Minor7, VII: Major7
        assert_eq!(chords[0].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[0].family, Some(crate::chord::ChordFamily::Dominant7));

        assert_eq!(chords[1].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[1].family, Some(crate::chord::ChordFamily::Minor7));

        assert_eq!(chords[2].quality, crate::chord::ChordQuality::Diminished);
        assert_eq!(
            chords[2].family,
            Some(crate::chord::ChordFamily::HalfDiminished)
        );

        assert_eq!(chords[3].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[3].family, Some(crate::chord::ChordFamily::Major7));

        assert_eq!(chords[4].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[4].family, Some(crate::chord::ChordFamily::Minor7));

        assert_eq!(chords[5].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[5].family, Some(crate::chord::ChordFamily::Minor7));

        assert_eq!(chords[6].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[6].family, Some(crate::chord::ChordFamily::Major7));
    }

    #[test]
    fn test_aeolian_mode_harmonization() {
        let mode = ScaleMode::aeolian();
        let root = c_note();
        let chords = harmonize_scale(&mode, &root, HarmonizationDepth::Sevenths);

        assert_eq!(chords.len(), 7);

        // Aeolian (Natural Minor) scale seventh chord qualities:
        // i: Minor7, ii°: HalfDiminished, III: Major7, iv: Minor7, v: Minor7, VI: Major7, VII: Dominant7
        assert_eq!(chords[0].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[0].family, Some(crate::chord::ChordFamily::Minor7));

        assert_eq!(chords[1].quality, crate::chord::ChordQuality::Diminished);
        assert_eq!(
            chords[1].family,
            Some(crate::chord::ChordFamily::HalfDiminished)
        );

        assert_eq!(chords[2].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[2].family, Some(crate::chord::ChordFamily::Major7));

        assert_eq!(chords[3].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[3].family, Some(crate::chord::ChordFamily::Minor7));

        assert_eq!(chords[4].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[4].family, Some(crate::chord::ChordFamily::Minor7));

        assert_eq!(chords[5].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[5].family, Some(crate::chord::ChordFamily::Major7));

        assert_eq!(chords[6].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[6].family, Some(crate::chord::ChordFamily::Dominant7));
    }

    #[test]
    fn test_locrian_mode_harmonization() {
        let mode = ScaleMode::locrian();
        let root = c_note();
        let chords = harmonize_scale(&mode, &root, HarmonizationDepth::Sevenths);

        assert_eq!(chords.len(), 7);

        // Locrian scale seventh chord qualities:
        // i°: HalfDiminished, II: Major7, III: Minor7, iv: Minor7, V: Major7, VI: Dominant7, VII: Minor7
        assert_eq!(chords[0].quality, crate::chord::ChordQuality::Diminished);
        assert_eq!(
            chords[0].family,
            Some(crate::chord::ChordFamily::HalfDiminished)
        );

        assert_eq!(chords[1].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[1].family, Some(crate::chord::ChordFamily::Major7));

        assert_eq!(chords[2].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[2].family, Some(crate::chord::ChordFamily::Minor7));

        assert_eq!(chords[3].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[3].family, Some(crate::chord::ChordFamily::Minor7));

        assert_eq!(chords[4].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[4].family, Some(crate::chord::ChordFamily::Major7));

        assert_eq!(chords[5].quality, crate::chord::ChordQuality::Major);
        assert_eq!(chords[5].family, Some(crate::chord::ChordFamily::Dominant7));

        assert_eq!(chords[6].quality, crate::chord::ChordQuality::Minor);
        assert_eq!(chords[6].family, Some(crate::chord::ChordFamily::Minor7));
    }

    #[test]
    fn test_analyze_scale_harmony() {
        let mode = ScaleMode::ionian();
        let root = c_note();
        let analysis = analyze_scale_harmony(mode, root.clone(), HarmonizationDepth::Triads);

        assert!(!analysis.is_empty());
        assert!(analysis.contains("Ionian"));
    }

    /// Helper function to get expected Roman numeral for a chord
    fn get_roman_numeral(
        degree: usize,
        quality: &crate::chord::ChordQuality,
        _family: Option<&crate::chord::ChordFamily>,
    ) -> String {
        let base_numeral = match degree {
            1 => "I",
            2 => "II",
            3 => "III",
            4 => "IV",
            5 => "V",
            6 => "VI",
            7 => "VII",
            _ => panic!("Invalid degree"),
        };

        let is_major = matches!(quality, crate::chord::ChordQuality::Major);
        let is_diminished = matches!(quality, crate::chord::ChordQuality::Diminished);
        let is_augmented = matches!(quality, crate::chord::ChordQuality::Augmented);

        let numeral = if is_major {
            base_numeral.to_string()
        } else {
            base_numeral.to_lowercase()
        };

        // Add symbols for altered qualities
        if is_diminished {
            format!("{}°", numeral)
        } else if is_augmented {
            format!("{}+", numeral)
        } else {
            numeral
        }
    }

    #[test]
    fn test_ionian_scale_degrees_and_notes() {
        let mode = ScaleMode::ionian();
        let root = c_note();
        let harmonization =
            ScaleHarmonization::new(mode, root.clone(), HarmonizationDepth::Sevenths);

        // Expected note names for C Ionian (C major scale)
        let expected_notes = ["C", "D", "E", "F", "G", "A", "B"];
        let expected_romans = ["I", "ii", "iii", "IV", "V", "vi", "vii°"];

        for (i, ((chord, expected_note), expected_roman)) in harmonization
            .chords
            .iter()
            .zip(expected_notes.iter())
            .zip(expected_romans.iter())
            .enumerate()
        {
            let degree = i + 1;
            let scale_note = &harmonization.scale_notes[i];

            // Test degree number
            assert_eq!(degree, i + 1, "Degree number mismatch");

            // Test note name
            assert_eq!(
                scale_note.name(),
                *expected_note,
                "Note name mismatch at degree {}: expected {}, got {}",
                degree,
                expected_note,
                scale_note.name()
            );

            // Test Roman numeral
            let actual_roman = get_roman_numeral(degree, &chord.quality, chord.family.as_ref());
            assert_eq!(
                actual_roman, *expected_roman,
                "Roman numeral mismatch at degree {}: expected {}, got {}",
                degree, expected_roman, actual_roman
            );
        }
    }

    #[test]
    fn test_dorian_scale_degrees_and_notes() {
        let mode = ScaleMode::dorian();
        let root = c_note();
        let harmonization =
            ScaleHarmonization::new(mode, root.clone(), HarmonizationDepth::Sevenths);

        // Expected note names for C Dorian
        let expected_notes = ["C", "D", "Eb", "F", "G", "A", "Bb"];
        let expected_romans = ["i", "ii", "III", "IV", "v", "vi°", "VII"];

        for (i, ((chord, expected_note), expected_roman)) in harmonization
            .chords
            .iter()
            .zip(expected_notes.iter())
            .zip(expected_romans.iter())
            .enumerate()
        {
            let degree = i + 1;
            let scale_note = &harmonization.scale_notes[i];

            assert_eq!(degree, i + 1);
            assert_eq!(
                scale_note.name(),
                *expected_note,
                "Note name mismatch at degree {}: expected {}, got {}",
                degree,
                expected_note,
                scale_note.name()
            );

            let actual_roman = get_roman_numeral(degree, &chord.quality, chord.family.as_ref());
            assert_eq!(
                actual_roman, *expected_roman,
                "Roman numeral mismatch at degree {}: expected {}, got {}",
                degree, expected_roman, actual_roman
            );
        }
    }

    #[test]
    fn test_phrygian_scale_degrees_and_notes() {
        let mode = ScaleMode::phrygian();
        let root = c_note();
        let harmonization =
            ScaleHarmonization::new(mode, root.clone(), HarmonizationDepth::Sevenths);

        // Expected note names for C Phrygian (uses all 7 letters)
        let expected_notes = ["C", "Db", "Eb", "F", "G", "Ab", "Bb"];
        // Phrygian: i(min), II(maj), III(maj/dom7), iv(min), v°(dim), VI(maj), VII(min)
        let expected_romans = ["i", "II", "III", "iv", "v°", "VI", "vii"];

        for (i, ((chord, expected_note), expected_roman)) in harmonization
            .chords
            .iter()
            .zip(expected_notes.iter())
            .zip(expected_romans.iter())
            .enumerate()
        {
            let degree = i + 1;
            let scale_note = &harmonization.scale_notes[i];

            assert_eq!(degree, i + 1);
            assert_eq!(
                scale_note.name(),
                *expected_note,
                "Note name mismatch at degree {}: expected {}, got {}",
                degree,
                expected_note,
                scale_note.name()
            );

            let actual_roman = get_roman_numeral(degree, &chord.quality, chord.family.as_ref());
            assert_eq!(
                actual_roman, *expected_roman,
                "Roman numeral mismatch at degree {}: expected {}, got {}",
                degree, expected_roman, actual_roman
            );
        }
    }

    #[test]
    fn test_lydian_scale_degrees_and_notes() {
        let mode = ScaleMode::lydian();
        let root = c_note();
        let harmonization =
            ScaleHarmonization::new(mode, root.clone(), HarmonizationDepth::Sevenths);

        // Expected note names for C Lydian (uses all 7 letters)
        let expected_notes = ["C", "D", "E", "F#", "G", "A", "B"];
        let expected_romans = ["I", "II", "iii", "iv°", "V", "vi", "vii"];

        for (i, ((chord, expected_note), expected_roman)) in harmonization
            .chords
            .iter()
            .zip(expected_notes.iter())
            .zip(expected_romans.iter())
            .enumerate()
        {
            let degree = i + 1;
            let scale_note = &harmonization.scale_notes[i];

            assert_eq!(degree, i + 1);
            assert_eq!(
                scale_note.name(),
                *expected_note,
                "Note name mismatch at degree {}: expected {}, got {}",
                degree,
                expected_note,
                scale_note.name()
            );

            let actual_roman = get_roman_numeral(degree, &chord.quality, chord.family.as_ref());
            assert_eq!(
                actual_roman, *expected_roman,
                "Roman numeral mismatch at degree {}: expected {}, got {}",
                degree, expected_roman, actual_roman
            );
        }
    }

    #[test]
    fn test_mixolydian_scale_degrees_and_notes() {
        let mode = ScaleMode::mixolydian();
        let root = c_note();
        let harmonization =
            ScaleHarmonization::new(mode, root.clone(), HarmonizationDepth::Sevenths);

        // Expected note names for C Mixolydian
        let expected_notes = ["C", "D", "E", "F", "G", "A", "Bb"];
        let expected_romans = ["I", "ii", "iii°", "IV", "v", "vi", "VII"];

        for (i, ((chord, expected_note), expected_roman)) in harmonization
            .chords
            .iter()
            .zip(expected_notes.iter())
            .zip(expected_romans.iter())
            .enumerate()
        {
            let degree = i + 1;
            let scale_note = &harmonization.scale_notes[i];

            assert_eq!(degree, i + 1);
            assert_eq!(
                scale_note.name(),
                *expected_note,
                "Note name mismatch at degree {}: expected {}, got {}",
                degree,
                expected_note,
                scale_note.name()
            );

            let actual_roman = get_roman_numeral(degree, &chord.quality, chord.family.as_ref());
            assert_eq!(
                actual_roman, *expected_roman,
                "Roman numeral mismatch at degree {}: expected {}, got {}",
                degree, expected_roman, actual_roman
            );
        }
    }

    #[test]
    fn test_aeolian_scale_degrees_and_notes() {
        let mode = ScaleMode::aeolian();
        let root = c_note();
        let harmonization =
            ScaleHarmonization::new(mode, root.clone(), HarmonizationDepth::Sevenths);

        // Expected note names for C Aeolian (C natural minor)
        let expected_notes = ["C", "D", "Eb", "F", "G", "Ab", "Bb"];
        let expected_romans = ["i", "ii°", "III", "iv", "v", "VI", "VII"];

        for (i, ((chord, expected_note), expected_roman)) in harmonization
            .chords
            .iter()
            .zip(expected_notes.iter())
            .zip(expected_romans.iter())
            .enumerate()
        {
            let degree = i + 1;
            let scale_note = &harmonization.scale_notes[i];

            assert_eq!(degree, i + 1);
            assert_eq!(
                scale_note.name(),
                *expected_note,
                "Note name mismatch at degree {}: expected {}, got {}",
                degree,
                expected_note,
                scale_note.name()
            );

            let actual_roman = get_roman_numeral(degree, &chord.quality, chord.family.as_ref());
            assert_eq!(
                actual_roman, *expected_roman,
                "Roman numeral mismatch at degree {}: expected {}, got {}",
                degree, expected_roman, actual_roman
            );
        }
    }

    #[test]
    fn test_locrian_scale_degrees_and_notes() {
        let mode = ScaleMode::locrian();
        let root = c_note();
        let harmonization =
            ScaleHarmonization::new(mode, root.clone(), HarmonizationDepth::Sevenths);

        // Expected note names for C Locrian (uses all 7 letters)
        let expected_notes = ["C", "Db", "Eb", "F", "Gb", "Ab", "Bb"];
        // Locrian: i°(dim), II(maj), III(min), iv(min), V(maj), VI(maj/dom7), VII(min)
        let expected_romans = ["i°", "II", "iii", "iv", "V", "VI", "vii"];

        for (i, ((chord, expected_note), expected_roman)) in harmonization
            .chords
            .iter()
            .zip(expected_notes.iter())
            .zip(expected_romans.iter())
            .enumerate()
        {
            let degree = i + 1;
            let scale_note = &harmonization.scale_notes[i];

            assert_eq!(degree, i + 1);
            assert_eq!(
                scale_note.name(),
                *expected_note,
                "Note name mismatch at degree {}: expected {}, got {}",
                degree,
                expected_note,
                scale_note.name()
            );

            let actual_roman = get_roman_numeral(degree, &chord.quality, chord.family.as_ref());
            assert_eq!(
                actual_roman, *expected_roman,
                "Roman numeral mismatch at degree {}: expected {}, got {}",
                degree, expected_roman, actual_roman
            );
        }
    }
}
