//! Chord detail level configuration
//!
//! Controls the maximum complexity of chord extensions displayed.
//! When a chord exceeds the configured level, it can be represented
//! as a polychord/slash chord (e.g., C13 → Dm/C at the Sevenths level).

use crate::chord::Chord;
use crate::chord::degree::ChordDegree;
use crate::chord::quality::ChordQuality;
use crate::primitives::note::Note;
use crate::primitives::{MusicalNote, RootNotation};
use facet::Facet;

/// Controls the maximum complexity of chord extensions displayed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet, Default)]
#[repr(u8)]
pub enum DetailLevel {
    /// Show up to 7th chords. 9ths, 11ths, 13ths become polychords
    Sevenths,
    /// Show up to 9th chords. 11ths, 13ths become polychords
    Ninths,
    /// Show up to 11th chords. 13ths become polychords
    Elevenths,
    /// Show full chord names including 13ths (default)
    #[default]
    Thirteenths,
}

impl DetailLevel {
    /// Check if this level allows the given extension degree
    pub fn allows(&self, degree: ChordDegree) -> bool {
        match self {
            DetailLevel::Sevenths => !matches!(
                degree,
                ChordDegree::Ninth | ChordDegree::Eleventh | ChordDegree::Thirteenth
            ),
            DetailLevel::Ninths => {
                !matches!(degree, ChordDegree::Eleventh | ChordDegree::Thirteenth)
            }
            DetailLevel::Elevenths => !matches!(degree, ChordDegree::Thirteenth),
            DetailLevel::Thirteenths => true,
        }
    }

    /// Get all levels as an array (useful for iteration)
    pub fn all() -> [DetailLevel; 4] {
        [
            DetailLevel::Sevenths,
            DetailLevel::Ninths,
            DetailLevel::Elevenths,
            DetailLevel::Thirteenths,
        ]
    }
}

impl std::fmt::Display for DetailLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DetailLevel::Sevenths => write!(f, "7ths"),
            DetailLevel::Ninths => write!(f, "9ths"),
            DetailLevel::Elevenths => write!(f, "11ths"),
            DetailLevel::Thirteenths => write!(f, "13ths"),
        }
    }
}

/// Result of computing an upper structure chord
#[derive(Debug, Clone)]
pub struct UpperStructure {
    /// The upper structure chord (simplified from extensions)
    pub chord: Chord,
    /// The original bass note (root of the full chord)
    pub bass: RootNotation,
}

/// Compute upper structure chord from extensions above the detail level cutoff
///
/// For example, C13 at Sevenths level would compute the upper structure
/// from the 9th, 11th, and 13th (D, F, A) which forms a Dm triad.
///
/// Returns `None` if:
/// - The chord doesn't exceed the detail level
/// - There are no extensions above the cutoff
/// - The root cannot be resolved to a note
pub fn compute_upper_structure(chord: &Chord, level: DetailLevel) -> Option<UpperStructure> {
    // Check if chord exceeds the level
    let highest = chord.extensions.highest()?;
    if level.allows(highest) {
        return None; // No simplification needed
    }

    // Get the root note - we need this to calculate upper structure notes
    let root_note = chord.root.resolved_note()?;

    // Collect semitones for extensions that exceed the level
    let mut upper_semitones: Vec<u8> = Vec::new();

    if !level.allows(ChordDegree::Ninth)
        && let Some(interval) = chord.extensions.ninth_interval()
    {
        upper_semitones.push(interval.semitones());
    }
    if !level.allows(ChordDegree::Eleventh)
        && let Some(interval) = chord.extensions.eleventh_interval()
    {
        upper_semitones.push(interval.semitones());
    }
    if !level.allows(ChordDegree::Thirteenth)
        && let Some(interval) = chord.extensions.thirteenth_interval()
    {
        upper_semitones.push(interval.semitones());
    }

    if upper_semitones.is_empty() {
        return None;
    }

    // Sort to get the lowest note first (this will be the root of the upper structure)
    upper_semitones.sort();
    let upper_root_semitone = upper_semitones[0];

    // Normalize semitones relative to the upper root
    let normalized: Vec<u8> = upper_semitones
        .iter()
        .map(|s| (s - upper_root_semitone) % 12)
        .collect();

    // Identify quality from the normalized semitones
    let upper_quality = quality_from_intervals(&normalized);

    // Calculate the actual note name for the upper root
    // Use prefer_sharp based on the original root's spelling
    let prefer_sharp = root_note.name().contains('#');
    let upper_root_note = MusicalNote::from_semitone(
        (root_note.semitone() + upper_root_semitone) % 12,
        prefer_sharp,
    );
    let upper_root = RootNotation::from_note_name(upper_root_note);

    // Create simplified upper chord
    let upper_chord = Chord::new(upper_root, upper_quality);

    Some(UpperStructure {
        chord: upper_chord,
        bass: chord.root.clone(),
    })
}

/// Identify chord quality from a set of intervals (semitones from root)
fn quality_from_intervals(semitones: &[u8]) -> ChordQuality {
    let has_minor_third = semitones.contains(&3);
    let has_major_third = semitones.contains(&4);
    let has_perfect_fifth = semitones.contains(&7);
    let has_dim_fifth = semitones.contains(&6);
    let has_aug_fifth = semitones.contains(&8);

    // Check for augmented (major 3rd + augmented 5th)
    if has_major_third && has_aug_fifth {
        return ChordQuality::Augmented;
    }

    // Check for diminished (minor 3rd + diminished 5th)
    if has_minor_third && has_dim_fifth {
        return ChordQuality::Diminished;
    }

    // Check for minor (minor 3rd + perfect 5th, or just minor 3rd)
    if has_minor_third && (has_perfect_fifth || !has_major_third) {
        return ChordQuality::Minor;
    }

    // Check for major (major 3rd + perfect 5th, or just major 3rd)
    if has_major_third {
        return ChordQuality::Major;
    }

    // Default to major for ambiguous cases
    ChordQuality::Major
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::extensions::{ExtensionQuality, Extensions};
    use crate::chord::family::ChordFamily;
    use crate::primitives::MusicalNote;

    fn make_chord(root: &str, quality: ChordQuality) -> Chord {
        let note = MusicalNote::from_string(root).unwrap();
        let root = RootNotation::from_note_name(note);
        Chord::new(root, quality)
    }

    #[test]
    fn test_detail_level_allows() {
        // Sevenths level
        assert!(DetailLevel::Sevenths.allows(ChordDegree::Third));
        assert!(DetailLevel::Sevenths.allows(ChordDegree::Fifth));
        assert!(DetailLevel::Sevenths.allows(ChordDegree::Seventh));
        assert!(!DetailLevel::Sevenths.allows(ChordDegree::Ninth));
        assert!(!DetailLevel::Sevenths.allows(ChordDegree::Eleventh));
        assert!(!DetailLevel::Sevenths.allows(ChordDegree::Thirteenth));

        // Ninths level
        assert!(DetailLevel::Ninths.allows(ChordDegree::Ninth));
        assert!(!DetailLevel::Ninths.allows(ChordDegree::Eleventh));
        assert!(!DetailLevel::Ninths.allows(ChordDegree::Thirteenth));

        // Elevenths level
        assert!(DetailLevel::Elevenths.allows(ChordDegree::Eleventh));
        assert!(!DetailLevel::Elevenths.allows(ChordDegree::Thirteenth));

        // Thirteenths level allows all
        assert!(DetailLevel::Thirteenths.allows(ChordDegree::Thirteenth));
    }

    #[test]
    fn test_quality_from_intervals() {
        // Major triad: 0, 4, 7
        assert_eq!(quality_from_intervals(&[0, 4, 7]), ChordQuality::Major);

        // Minor triad: 0, 3, 7
        assert_eq!(quality_from_intervals(&[0, 3, 7]), ChordQuality::Minor);

        // Diminished: 0, 3, 6
        assert_eq!(quality_from_intervals(&[0, 3, 6]), ChordQuality::Diminished);

        // Augmented: 0, 4, 8
        assert_eq!(quality_from_intervals(&[0, 4, 8]), ChordQuality::Augmented);

        // Just minor 3rd (incomplete) -> Minor
        assert_eq!(quality_from_intervals(&[0, 3]), ChordQuality::Minor);

        // Just major 3rd (incomplete) -> Major
        assert_eq!(quality_from_intervals(&[0, 4]), ChordQuality::Major);
    }

    #[test]
    fn test_compute_upper_structure_c13() {
        // C13 = C E G Bb D F A
        // At Sevenths level, upper structure is from 9th up: D F A = Dm
        let mut chord = make_chord("C", ChordQuality::Major);
        chord.family = Some(ChordFamily::Dominant7);
        chord.extensions = Extensions::with_thirteenth(
            ExtensionQuality::Natural,
            ExtensionQuality::Natural,
            ExtensionQuality::Natural,
        );

        let upper = compute_upper_structure(&chord, DetailLevel::Sevenths);
        assert!(upper.is_some());

        let upper = upper.unwrap();
        assert_eq!(upper.bass.to_string(), "C");
        assert_eq!(upper.chord.quality, ChordQuality::Minor);
        // The root should be D (9th of C)
        assert_eq!(upper.chord.root.to_string(), "D");
    }

    #[test]
    fn test_compute_upper_structure_cmaj9() {
        // Cmaj9 = C E G B D
        // At Sevenths level, upper structure is from 9th: D alone
        // With only the 9th, we'd get just D - but this forms an incomplete chord
        let mut chord = make_chord("C", ChordQuality::Major);
        chord.family = Some(ChordFamily::Major7);
        chord.extensions = Extensions::with_ninth(ExtensionQuality::Natural);

        let upper = compute_upper_structure(&chord, DetailLevel::Sevenths);
        assert!(upper.is_some());

        let upper = upper.unwrap();
        assert_eq!(upper.bass.to_string(), "C");
        // Just the 9th gives us D as root, defaulting to major
        assert_eq!(upper.chord.root.to_string(), "D");
    }

    #[test]
    fn test_no_upper_structure_when_within_level() {
        // Dm7 at Sevenths level - no upper structure needed
        let mut chord = make_chord("D", ChordQuality::Minor);
        chord.family = Some(ChordFamily::Minor7);

        let upper = compute_upper_structure(&chord, DetailLevel::Sevenths);
        assert!(upper.is_none());
    }

    #[test]
    fn test_c11_at_sevenths() {
        // C11 = C E G Bb D F
        // At Sevenths, upper structure from 9th up: D F = incomplete, but D-F is minor 3rd
        let mut chord = make_chord("C", ChordQuality::Major);
        chord.family = Some(ChordFamily::Dominant7);
        chord.extensions =
            Extensions::with_eleventh(ExtensionQuality::Natural, ExtensionQuality::Natural);

        let upper = compute_upper_structure(&chord, DetailLevel::Sevenths);
        assert!(upper.is_some());

        let upper = upper.unwrap();
        assert_eq!(upper.bass.to_string(), "C");
        // D-F forms a minor 3rd, so Dm
        assert_eq!(upper.chord.quality, ChordQuality::Minor);
        assert_eq!(upper.chord.root.to_string(), "D");
    }

    // ===== display_at_level() Tests =====

    #[test]
    fn test_display_at_level_c13() {
        // C13 at full detail should show as C13
        let mut chord = make_chord("C", ChordQuality::Major);
        chord.family = Some(ChordFamily::Dominant7);
        chord.extensions = Extensions::with_thirteenth(
            ExtensionQuality::Natural,
            ExtensionQuality::Natural,
            ExtensionQuality::Natural,
        );
        chord.compute_intervals();
        chord.normalize();

        // At Thirteenths level, should show full chord name
        let display = chord.display_at_level(DetailLevel::Thirteenths);
        assert!(display.contains("13"), "Expected '13' in {}", display);

        // At Sevenths level, should become a polychord
        let display = chord.display_at_level(DetailLevel::Sevenths);
        assert!(
            display.contains("/"),
            "Expected slash chord at Sevenths level: {}",
            display
        );
    }

    #[test]
    fn test_display_at_level_c9() {
        // C9 should show as C9 at Ninths level, polychord at Sevenths
        let mut chord = make_chord("C", ChordQuality::Major);
        chord.family = Some(ChordFamily::Dominant7);
        chord.extensions = Extensions::with_ninth(ExtensionQuality::Natural);
        chord.compute_intervals();
        chord.normalize();

        // At Ninths level, should show C9
        let display = chord.display_at_level(DetailLevel::Ninths);
        assert!(display.contains("9"), "Expected '9' in {}", display);

        // At Sevenths level, should become a polychord
        let display = chord.display_at_level(DetailLevel::Sevenths);
        assert!(
            display.contains("/"),
            "Expected slash chord at Sevenths level: {}",
            display
        );
    }

    #[test]
    fn test_display_at_level_dm7() {
        // Dm7 should show as Dm7 at all levels (no extensions beyond 7th)
        let mut chord = make_chord("D", ChordQuality::Minor);
        chord.family = Some(ChordFamily::Minor7);
        chord.compute_intervals();
        chord.normalize();

        // Should be the same at all levels
        for level in DetailLevel::all() {
            let display = chord.display_at_level(level);
            assert!(
                !display.contains("/"),
                "Dm7 should not become slash chord at {:?}: {}",
                level,
                display
            );
        }
    }

    #[test]
    fn test_display_at_level_survey() {
        println!("\n=== Detail Level Display Survey ===");

        // C13
        let mut chord = make_chord("C", ChordQuality::Major);
        chord.family = Some(ChordFamily::Dominant7);
        chord.extensions = Extensions::with_thirteenth(
            ExtensionQuality::Natural,
            ExtensionQuality::Natural,
            ExtensionQuality::Natural,
        );
        chord.compute_intervals();
        chord.normalize();

        println!("C13:");
        for level in DetailLevel::all() {
            println!("  {:?}: {}", level, chord.display_at_level(level));
        }

        // Cmaj9
        let mut chord = make_chord("C", ChordQuality::Major);
        chord.family = Some(ChordFamily::Major7);
        chord.extensions = Extensions::with_ninth(ExtensionQuality::Natural);
        chord.compute_intervals();
        chord.normalize();

        println!("Cmaj9:");
        for level in DetailLevel::all() {
            println!("  {:?}: {}", level, chord.display_at_level(level));
        }

        // C11
        let mut chord = make_chord("C", ChordQuality::Major);
        chord.family = Some(ChordFamily::Dominant7);
        chord.extensions =
            Extensions::with_eleventh(ExtensionQuality::Natural, ExtensionQuality::Natural);
        chord.compute_intervals();
        chord.normalize();

        println!("C11:");
        for level in DetailLevel::all() {
            println!("  {:?}: {}", level, chord.display_at_level(level));
        }
    }
}
