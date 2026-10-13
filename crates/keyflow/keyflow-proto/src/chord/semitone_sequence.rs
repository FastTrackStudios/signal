//! Chord construction from semitone sequences
//!
//! Provides utilities to analyze semitone patterns and construct appropriate Chord objects

use super::{
    Alteration, Chord, ChordDegree, ChordFamily, ChordQuality, ExtensionQuality, Extensions,
    SuspendedType,
};
use crate::primitives::note::Note;
use crate::primitives::{Interval, MusicalNote, RootNotation};
use std::collections::HashSet;

/// Semitone constants for clarity
mod semitones {
    pub const MAJOR_SECOND: u8 = 2;
    pub const MINOR_THIRD: u8 = 3;
    pub const MAJOR_THIRD: u8 = 4;
    pub const PERFECT_FOURTH: u8 = 5;
    pub const DIMINISHED_FIFTH: u8 = 6;
    pub const PERFECT_FIFTH: u8 = 7;
    pub const AUGMENTED_FIFTH: u8 = 8;
    pub const MAJOR_SIXTH: u8 = 9; // Also enharmonic with diminished 7th
    pub const MINOR_SEVENTH: u8 = 10;
    pub const MAJOR_SEVENTH: u8 = 11;
    // Extensions (absolute semitones from root, including octave)
    pub const FLAT_NINTH: u8 = 13;
    pub const NINTH: u8 = 14;
    pub const SHARP_NINTH: u8 = 15;
    pub const ELEVENTH: u8 = 17;
    pub const SHARP_ELEVENTH: u8 = 18;
    pub const FLAT_THIRTEENTH: u8 = 20;
    pub const THIRTEENTH: u8 = 21;
}

/// Error type for semitone sequence analysis
#[derive(Debug, Clone, PartialEq)]
pub enum SemitoneSequenceError {
    /// Empty semitone sequence
    EmptySequence,
    /// No root note (0) in sequence
    MissingRoot,
    /// Unrecognizable chord pattern
    UnrecognizedPattern(Vec<u8>),
    /// Ambiguous chord (multiple interpretations possible)
    AmbiguousChord(Vec<String>),
}

impl std::fmt::Display for SemitoneSequenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptySequence => write!(f, "Empty semitone sequence"),
            Self::MissingRoot => write!(f, "Semitone sequence must contain root (0)"),
            Self::UnrecognizedPattern(seq) => write!(f, "Unrecognized chord pattern: {:?}", seq),
            Self::AmbiguousChord(options) => {
                write!(f, "Ambiguous chord, possibilities: {:?}", options)
            }
        }
    }
}

impl std::error::Error for SemitoneSequenceError {}

/// Result type for semitone sequence operations
pub type Result<T> = std::result::Result<T, SemitoneSequenceError>;

/// Count how many notes in the chord match a basic triad (root, 3rd, 5th)
///
/// This is used to determine if a chord is likely an inversion.
/// The interpretation with more notes matching its triad is preferred.
fn count_triad_notes(pitch_classes: &HashSet<u8>, quality: ChordQuality) -> u32 {
    let triad_intervals = match quality {
        ChordQuality::Major => vec![0, 4, 7], // Major: root, M3, P5
        ChordQuality::Minor => vec![0, 3, 7], // Minor: root, m3, P5
        ChordQuality::Diminished => vec![0, 3, 6], // Dim: root, m3, d5
        ChordQuality::Augmented => vec![0, 4, 8], // Aug: root, M3, A5
        ChordQuality::Power => vec![0, 7],    // Power: root, P5
        ChordQuality::Suspended(SuspendedType::Second) => vec![0, 2, 7], // Sus2
        ChordQuality::Suspended(SuspendedType::Fourth) => vec![0, 5, 7], // Sus4
    };

    triad_intervals
        .iter()
        .filter(|&&interval| pitch_classes.contains(&interval))
        .count() as u32
}

/// Try to find the simplest chord interpretation by testing all pitch classes as roots
///
/// The algorithm counts how many notes in the chord match a basic triad for each
/// potential root. The interpretation with more matching triad notes wins.
/// For example, C F A has more notes matching F major (F A C = 3) than C major (C E G = 1).
///
/// Returns the best interpretation with its root offset (semitones from original root).
/// A root offset of 0 means the original root is best; non-zero means an inversion was found.
/// Calculate a complexity score for a chord interpretation
/// Lower scores are simpler (better), higher scores are more complex
///
/// This penalizes:
/// - Altered fifths (b5, #5) - very dissonant alterations
/// - Altered ninths (b9, #9) - altered chord tensions
/// - Multiple alterations
///
/// And rewards:
/// - Clean triads with natural extensions
/// - #11 as a single clean extension (common in jazz)
/// - Having a seventh (chord with a 7th is a complete harmonic unit)
fn chord_complexity_score(info: &ChordInfo, rotated_pcs: &HashSet<u8>) -> i32 {
    let mut score = 0;

    // Penalize alterations - but less harshly for altered fifths when a 7th is present
    // (e.g., C7#5 is a common chord and should be preferred over weird triads)
    let has_seventh = info.family.is_some();
    for alteration in &info.alterations {
        match alteration.degree {
            ChordDegree::Fifth => {
                // Altered fifths (b5, #5) - but less penalty when 7th is present
                if has_seventh {
                    score += 8; // Moderate penalty for altered dominant/major 7th chords
                } else {
                    score += 20; // Larger penalty for altered triads
                }
            }
            ChordDegree::Ninth => {
                // Altered ninths (b9, #9) - moderate penalty
                score += 15;
            }
            ChordDegree::Eleventh => {
                // #11 is common and clean - small penalty
                score += 5;
            }
            ChordDegree::Thirteenth => {
                // b13 is fairly common - moderate penalty
                score += 10;
            }
            _ => {}
        }
    }

    // Big bonus for having a seventh - 7th chords are complete harmonic units
    // This helps prefer C7#5 over G#add2#5
    if has_seventh {
        score -= 15;
    }

    // Bonus for having a clean perfect fifth
    if rotated_pcs.contains(&7) {
        score -= 5;
    }

    // Bonus for having a natural third
    if rotated_pcs.contains(&4) || rotated_pcs.contains(&3) {
        score -= 3;
    }

    // Penalty for missing the third (unusual voicing)
    if !rotated_pcs.contains(&4) && !rotated_pcs.contains(&3) {
        score += 10;
    }

    // Penalize excessive additions — chords like "sus2add11" or "sus4add9"
    // are almost always a wrong root interpretation. Each addition beyond
    // the first adds complexity.
    if info.additions.len() > 1 {
        score += (info.additions.len() as i32 - 1) * 12;
    }

    score
}

fn find_simplest_interpretation(
    pitch_classes: &[u8],
    original_semitones: &[u8],
) -> Result<(ChordInfo, u8)> {
    // Get unique pitch classes, ensuring 0 is present
    let pcs: HashSet<u8> = pitch_classes.iter().map(|s| s % 12).collect();

    let mut best_score: i32 = i32::MIN;
    let mut best_info: Option<ChordInfo> = None;
    let mut best_root_offset: u8 = 0;

    // Sort pitch classes with 0 first, so the original root is tried first
    // This establishes a baseline - ties go to the original root
    let mut sorted_pcs: Vec<u8> = pcs.iter().copied().collect();
    sorted_pcs.sort_by_key(|&pc| if pc == 0 { 0 } else { pc + 1 });

    // Try each pitch class as a potential root
    for potential_root in sorted_pcs {
        // Rotate semitones to make potential_root the new root (0)
        let rotated_pcs: HashSet<u8> = pcs
            .iter()
            .map(|&pc| (pc + 12 - potential_root) % 12)
            .collect();

        // Also need to handle octave-aware semitones for extension detection
        let rotated_with_octave: Vec<u8> = original_semitones
            .iter()
            .filter_map(|&s| {
                let pc = s % 12;
                if pcs.contains(&pc) {
                    // Rotate this semitone
                    let new_pc = (pc + 12 - potential_root) % 12;
                    // Preserve octave offset: if original was in second octave, keep it there
                    let octave = s / 12;
                    Some(new_pc + octave * 12)
                } else {
                    None
                }
            })
            .collect();

        // Try to analyze this rotation
        if let Ok(info) = analyze_chord_structure(&rotated_with_octave) {
            // Count how many notes match the basic triad
            let triad_count = count_triad_notes(&rotated_pcs, info.quality) as i32;
            let has_complete_triad = triad_count == 3;

            // Check if this is a 7th chord (or higher)
            let has_seventh = info.family.is_some();

            // Check for "2 chord" pattern: root(0) + fifth(7) + second(2 or 14), no third
            // sus2 chords (root + 2nd + 5th) are complete musical structures that shouldn't be inverted
            let has_two_chord_pattern = rotated_pcs.contains(&0)
                && rotated_pcs.contains(&7)
                && (rotated_pcs.contains(&2) || rotated_with_octave.contains(&14))
                && !rotated_pcs.contains(&3)
                && !rotated_pcs.contains(&4);

            // Check for 6th chord pattern: root(0) + fifth(7) + sixth(9)
            // X6 chords are complete harmonic concepts even without the 3rd
            let has_sixth_chord_pattern =
                rotated_pcs.contains(&0) && rotated_pcs.contains(&7) && rotated_pcs.contains(&9);

            // Calculate complexity score (lower = simpler = better)
            let complexity = chord_complexity_score(&info, &rotated_pcs);

            // Count alterations for determining if chord is "clean"
            let alteration_count = info.alterations.len();
            let has_harsh_alterations = info
                .alterations
                .iter()
                .any(|a| matches!(a.degree, ChordDegree::Fifth | ChordDegree::Ninth));

            // Base score: triad notes * 10, minus complexity
            // Higher scores are better
            let mut score = triad_count * 10 - complexity;

            // For the original root: prefer it if:
            // 1. It has a complete triad (3/3), OR
            // 2. It has a 7th (even with altered/missing 5th, like C7#5), OR
            // 3. It's a sus2 chord (root + 2nd + 5th with no 3rd), OR
            // 4. It's a 6th chord (root + 5th + 6th)
            // This keeps C6 as C6, C7#5 as C7#5, and Dsus2 as Dsus2
            //
            // BUT: reduce the bonus if the chord has multiple harsh alterations
            // (like b5 + b9), because that likely means there's a simpler interpretation
            if potential_root == 0
                && (has_complete_triad
                    || has_seventh
                    || has_two_chord_pattern
                    || has_sixth_chord_pattern)
            {
                // Scale the bonus based on alteration count and harshness
                // Clean chords get full bonus, altered chords get reduced bonus
                let bonus = if alteration_count >= 2 && has_harsh_alterations {
                    10 // Minimal bonus for heavily altered chords
                } else if alteration_count >= 1 && has_harsh_alterations {
                    25 // Moderate bonus for single harsh alteration
                } else {
                    50 // Full bonus for clean or mildly altered chords
                };
                score += bonus;
            }

            // Prefer the original root on ties
            let is_better = if potential_root == 0 {
                score >= best_score
            } else {
                score > best_score
            };

            if is_better {
                best_score = score;
                best_info = Some(info);
                best_root_offset = potential_root;
            }
        }
    }

    best_info
        .map(|info| (info, best_root_offset))
        .ok_or_else(|| SemitoneSequenceError::UnrecognizedPattern(pitch_classes.to_vec()))
}

/// Analyze a semitone sequence and construct a Chord
///
/// # Arguments
/// * `semitones` - A slice of semitone values (0-11 or extended for octave info)
/// * `root` - The root notation for the chord
///
/// # Examples
/// ```
/// use keyflow_proto::chord::{from_semitones, Chord};
/// use keyflow_proto::primitives::{RootNotation, MusicalNote};
///
/// let root = RootNotation::from_note_name(MusicalNote::c());
/// // Major triad: C E G
/// let chord = from_semitones(&[0, 4, 7], root.clone()).unwrap();
///
/// // Dominant 7th: C E G Bb
/// let chord = from_semitones(&[0, 4, 7, 10], root.clone()).unwrap();
///
/// // Major 9th: C E G B D (with octave)
/// let chord = from_semitones(&[0, 4, 7, 11, 14], root).unwrap();
/// ```
pub fn from_semitones(semitones: &[u8], root: RootNotation) -> Result<Chord> {
    if semitones.is_empty() {
        return Err(SemitoneSequenceError::EmptySequence);
    }

    // Normalize to pitch classes (0-11) while preserving extensions
    let pitch_classes = normalize_semitones(semitones);

    if !pitch_classes.contains(&0) {
        return Err(SemitoneSequenceError::MissingRoot);
    }

    // Find the simplest chord interpretation by trying all pitch classes as roots
    let (chord_info, root_offset) = find_simplest_interpretation(&pitch_classes, semitones)?;

    // Build the chord with the new root (if changed) and bass note (original root)
    build_chord_with_inversion(root, chord_info, semitones, root_offset)
}

/// Analyze a semitone sequence without inversion detection
///
/// This is the simpler version that always uses pitch class 0 as the root.
/// Use `from_semitones()` for automatic inversion detection.
pub fn from_semitones_no_inversion(semitones: &[u8], root: RootNotation) -> Result<Chord> {
    if semitones.is_empty() {
        return Err(SemitoneSequenceError::EmptySequence);
    }

    let pitch_classes = normalize_semitones(semitones);

    if !pitch_classes.contains(&0) {
        return Err(SemitoneSequenceError::MissingRoot);
    }

    let chord_info = analyze_chord_structure(&pitch_classes)?;
    build_chord(root, chord_info, semitones)
}

/// Normalize semitones, preserving both pitch classes and octave information
fn normalize_semitones(semitones: &[u8]) -> Vec<u8> {
    let mut normalized: Vec<u8> = semitones.to_vec();
    normalized.sort_unstable();
    normalized.dedup();
    normalized
}

/// Information extracted from analyzing a semitone pattern
#[derive(Debug, Clone)]
struct ChordInfo {
    quality: ChordQuality,
    family: Option<ChordFamily>,
    extensions: Extensions,
    alterations: Vec<Alteration>,
    /// Additions like 6th, add4, add2 (degrees added without implying extensions)
    additions: Vec<ChordDegree>,
}

/// Analyze a normalized semitone sequence to determine chord structure
fn analyze_chord_structure(semitones: &[u8]) -> Result<ChordInfo> {
    use semitones::*;

    // Get pitch classes only (mod 12)
    let pitch_classes: HashSet<u8> = semitones.iter().map(|s| s % 12).collect();

    // Extract the intervals relative to root
    let mut intervals: Vec<u8> = pitch_classes.iter().filter(|&&s| s != 0).copied().collect();
    intervals.sort_unstable();

    // Identify extensions (9, 11, 13) EARLY - these affect sus2/sus4 detection
    // The pitch class 2 could be from sus2 OR from 9th (14 % 12 = 2)
    // The pitch class 5 could be from sus4 OR from 11th (17 % 12 = 5)
    // Note: We check for the actual extension semitone here, but later we'll also
    // consider pitch class 2 as a 9th if a 7th is present (see has_implicit_ninth below)
    let has_ninth_extension = semitones.contains(&NINTH);
    let has_eleventh_extension = semitones.contains(&ELEVENTH);

    // Identify altered 9ths EARLY - they affect third detection
    // #9 (semitone 15) has pitch class 3, same as minor 3rd
    // b9 (semitone 13) has pitch class 1, no conflict
    let has_sharp_ninth = semitones.contains(&SHARP_NINTH);

    // Identify second (for sus2 and add2)
    // BUT: if semitone 14 (9th) is present, the pitch class 2 comes from that, not sus2
    let has_second_pitch_class = pitch_classes.contains(&MAJOR_SECOND);
    let has_second = has_second_pitch_class && !has_ninth_extension;

    // Identify third (if present)
    // BUT: if semitone 15 (#9) is present, the pitch class 3 comes from that, not minor 3rd
    // We need to check if the actual semitone 3 is in the original array, not just the pitch class
    let has_minor_third_pitch_class = pitch_classes.contains(&MINOR_THIRD);
    let has_minor_third = has_minor_third_pitch_class && !has_sharp_ninth;
    let has_major_third = pitch_classes.contains(&MAJOR_THIRD);

    // Identify fourth (for sus4 and add4)
    // BUT: if semitone 17 (11th) is present, the pitch class 5 comes from that, not sus4
    let has_fourth_pitch_class = pitch_classes.contains(&PERFECT_FOURTH);
    let has_fourth = has_fourth_pitch_class && !has_eleventh_extension;

    // Suspended means no third
    let has_sus2 = has_second && !has_major_third && !has_minor_third;
    let has_sus4 = has_fourth && !has_major_third && !has_minor_third;

    // Add4 means we have both a third AND a fourth, but NOT an 11th extension
    // (if there's an 11th, the 4th is from the 11th, not add4)
    let has_add4 = has_fourth && (has_major_third || has_minor_third) && !has_eleventh_extension;

    // Add2 means we have both a third AND a second, but NOT a 9th extension
    // (if there's a 9th, the 2nd pitch class is from the 9th, not add2)
    let has_add2 = has_second && (has_major_third || has_minor_third) && !has_ninth_extension;

    // Identify fifth
    let has_perfect_fifth = pitch_classes.contains(&PERFECT_FIFTH);
    let has_dim_fifth = pitch_classes.contains(&DIMINISHED_FIFTH);
    let has_aug_fifth = pitch_classes.contains(&AUGMENTED_FIFTH);

    // Identify sixth/diminished seventh (same pitch class: 9 semitones)
    // The interpretation depends on the chord quality:
    // - Diminished triad + 9 semitones = dim7 chord
    // - Major/Minor triad + 9 semitones = 6th chord
    let has_sixth_or_dim7 = pitch_classes.contains(&MAJOR_SIXTH);

    // Identify seventh
    let has_minor_seventh = pitch_classes.contains(&MINOR_SEVENTH);
    let has_major_seventh = pitch_classes.contains(&MAJOR_SEVENTH);

    // Identify remaining extensions (already have ninth, eleventh, and sharp_ninth from above)
    let has_minor_ninth = semitones.contains(&FLAT_NINTH);
    let has_ninth = has_ninth_extension; // Use the earlier variable
    // has_sharp_ninth already defined above
    let has_eleventh = has_eleventh_extension; // Use the earlier variable

    // For #11, 13th, b13: avoid double-counting octave-displaced chord tones as extensions.
    // Example: Cdim7 = [0, 3, 18, 21] — semitone 18 (pc 6) is just the dim 5th in a higher
    // octave, not #11. And semitone 21 (pc 9) is the dim 7th, not the 13th.
    //
    // Rules:
    // - #11 (pc 6): Block when dim5th is present WITHOUT a perfect 5th (pure dim chord).
    //   If both dim5th and P5 are present, the tritone IS #11.
    // - 13th (pc 9): Block when it's a diminished chord with dim7th (pc 9 = dim7).
    //   Don't block when a minor/major 7th is present (then pc 9 is a separate 6th/13th).
    // - b13 (pc 8): Block when aug5th is present (same pitch class).
    let has_sharp_eleventh =
        semitones.contains(&SHARP_ELEVENTH) && (!has_dim_fifth || has_perfect_fifth);
    let is_dim_with_dim7 = has_dim_fifth
        && has_minor_third
        && has_sixth_or_dim7
        && !has_minor_seventh
        && !has_major_seventh;
    let has_thirteenth = semitones.contains(&THIRTEENTH) && !is_dim_with_dim7;
    let has_flat_thirteenth = semitones.contains(&FLAT_THIRTEENTH) && !has_aug_fifth;

    // Check if we have seventh or extensions (affects quality inference when third is missing)
    let has_any_seventh = has_minor_seventh || has_major_seventh || has_sixth_or_dim7;

    // has_any_extension should only include pitch class 2 as an extension if there's a 7th
    // Otherwise, pitch class 2 is a sus2 or add2, not a 9th extension
    let has_implied_ninth = has_second_pitch_class && has_any_seventh;
    let has_any_extension = has_ninth_extension || has_eleventh_extension || has_implied_ninth;

    // Determine quality
    // When third is missing but we have 7th AND extensions, default to Major (not Sus/Power)
    // because voicings often omit the third, especially in extended chords
    // BUT: without a 7th, treat sus2/sus4/power chords as their natural quality
    let quality = if has_dim_fifth && has_minor_third && !has_aug_fifth && !has_perfect_fifth {
        ChordQuality::Diminished
    } else if has_aug_fifth
        && has_major_third
        && !has_perfect_fifth
        && !has_minor_seventh
        && !has_major_seventh
    {
        // Only treat as augmented if it's a pure triad without seventh
        ChordQuality::Augmented
    } else if has_minor_third {
        ChordQuality::Minor
    } else if has_major_third {
        ChordQuality::Major
    } else if has_sus2 {
        // sus2 takes priority over "7th + extension = Major" because the absence
        // of a 3rd is definitive. C7sus2 = [0, 2, 7, 10], not C9.
        ChordQuality::Suspended(SuspendedType::Second)
    } else if has_sus4 && !(has_any_seventh && has_any_extension) {
        // sus4 only when it's a simpler voicing (e.g., Bb7sus4 = [0, 5, 7, 10]).
        // When both a 7th AND other extensions are present (e.g., 9th), the 4th is
        // really an 11th in an extended chord (Bb11 = [0, 5, 7, 10, 14]).
        ChordQuality::Suspended(SuspendedType::Fourth)
    } else if has_any_seventh && has_any_extension {
        // No third detected, no sus pattern, but has BOTH seventh AND extensions.
        // Assume Major quality for voicings like Bb11 without the D.
        ChordQuality::Major
    } else if has_any_seventh {
        // Has seventh but no extensions and no sus pattern - assume Major
        ChordQuality::Major
    } else if !has_major_third && !has_minor_third && has_perfect_fifth {
        ChordQuality::Power
    } else {
        return Err(SemitoneSequenceError::UnrecognizedPattern(intervals));
    };

    // Determine if 9 semitones is a 6th or dim7 based on quality
    let is_diminished = quality == ChordQuality::Diminished;
    let has_sixth = has_sixth_or_dim7 && !is_diminished;
    let has_dim_seventh = has_sixth_or_dim7 && is_diminished;

    // Determine family (seventh type)
    // Note: 6th chords don't have a family (they're triads with added 6th)
    let family = match quality {
        ChordQuality::Major => {
            if has_major_seventh {
                Some(ChordFamily::Major7)
            } else if has_minor_seventh {
                Some(ChordFamily::Dominant7)
            } else {
                // No seventh - could be a triad or 6th chord
                None
            }
        }
        ChordQuality::Minor => {
            if has_major_seventh {
                Some(ChordFamily::MinorMajor7)
            } else if has_minor_seventh {
                Some(ChordFamily::Minor7)
            } else {
                None
            }
        }
        ChordQuality::Diminished => {
            // Prioritize half-diminished (m7b5) over fully diminished (dim7)
            // Half-diminished is more common in jazz/pop, and if minor 7th is present,
            // that's the more likely interpretation even if pitch class 9 is also present
            if has_minor_seventh {
                Some(ChordFamily::HalfDiminished)
            } else if has_dim_seventh {
                Some(ChordFamily::FullyDiminished)
            } else {
                None
            }
        }
        ChordQuality::Augmented => {
            if has_major_seventh {
                Some(ChordFamily::Major7) // Aug maj7
            } else if has_minor_seventh {
                Some(ChordFamily::Dominant7) // Aug 7
            } else {
                None
            }
        }
        _ => {
            if has_minor_seventh {
                Some(ChordFamily::Dominant7)
            } else {
                None
            }
        }
    };

    // Determine additions (6th, add4, add2, add9, add11)
    let mut additions = Vec::new();

    // 6th chord: has 6th without a 7th (C6, Cm6)
    // If there's a seventh, the 6th becomes a 13th conceptually
    if has_sixth && family.is_none() {
        additions.push(ChordDegree::Sixth);
    }

    // Re-evaluate has_add2 based on family:
    // If there's a 7th AND pitch class 2 is present, it's a 9th, not add2
    // This handles cases where the 9th is voiced in the same octave (semitone 2 instead of 14)
    // Exception: sus2 chords — the 2nd IS the sus quality, not a 9th extension.
    // C7sus2 = [0, 2, 7, 10] should be sus2 + dominant 7, not C9.
    let is_sus2 = quality == ChordQuality::Suspended(SuspendedType::Second);
    let has_implicit_ninth = has_second_pitch_class && family.is_some() && !is_sus2;
    let has_add2_final = has_add2 && !has_implicit_ninth;

    // Re-evaluate has_add4 based on family and ninth:
    // If there's a 7th or 9th AND pitch class 5 is present, it's an 11th, not add4
    // This handles Fm11 = Fm7 + 9 + 11, not Fm9add11
    let has_implicit_eleventh =
        has_fourth_pitch_class && (family.is_some() || has_implicit_ninth || has_ninth_extension);
    let has_add4_final = has_add4 && !has_implicit_eleventh;

    // Add4: has fourth AND third (not sus4) AND no 7th/9th (otherwise it's an 11th)
    if has_add4_final {
        additions.push(ChordDegree::Fourth);
    }

    // Add2: has second AND third (not sus2) AND no 7th (otherwise it's a 9th)
    if has_add2_final {
        additions.push(ChordDegree::Second);
    }

    // Determine extensions vs additions for 9th, 11th, 13th
    // Key rule: Extensions (9, 11, 13) imply a 7th. Without a 7th, they become additions.
    // For suspended chords with a 7th, extensions are still valid.
    let mut extensions = Extensions::none();
    let has_seventh = family.is_some();

    // Check for 9th extension FIRST - either explicit (semitone 14) or implicit (pitch class 2 with 7th)
    // We need this before checking 11th because Fm11 = Fm7 + 9 + 11, not Fm9add11
    let has_ninth_final = has_ninth || has_implicit_ninth;

    if has_thirteenth {
        if has_seventh {
            extensions.thirteenth = Some(ExtensionQuality::Natural);
        } else {
            additions.push(ChordDegree::Thirteenth);
        }
    }

    // Check for 11th - either explicit (semitone 17) or implicit (pitch class 5 with 7th/9th)
    let has_eleventh_final = has_eleventh || has_implicit_eleventh;

    if has_eleventh_final {
        if has_seventh || has_ninth_final {
            // 11th is an extension if we have a 7th OR 9th
            // Fm11 = Fm7 + 9 + 11, shown as "Fm11" not "Fm9add11"
            extensions.eleventh = Some(ExtensionQuality::Natural);
        } else {
            // No 7th and no 9th means this is add11
            additions.push(ChordDegree::Eleventh);
        }
    }

    if has_ninth_final {
        if has_seventh {
            // Normal 9th chord (has 7th)
            extensions.ninth = Some(ExtensionQuality::Natural);
        } else {
            // No 7th = add9 (Cadd9, Cmadd9)
            // Exception: 6/9 chords (has 6th and 9th, no 7th) - treat 9th as addition too
            additions.push(ChordDegree::Ninth);
        }
    }

    // Handle alterations
    let mut alterations = Vec::new();

    // Handle the tritone (6 semitones) - could be b5 or #11
    // If we have BOTH a tritone (6) AND a perfect fifth (7), the tritone is #11
    // If we only have the tritone without perfect fifth, it's b5
    let tritone_is_sharp11 = has_dim_fifth && has_perfect_fifth;

    // Altered fifth (only if not part of the base quality)
    // But NOT if we have a perfect 5th (then it's #11, not b5)
    if has_dim_fifth && quality != ChordQuality::Diminished && !has_perfect_fifth {
        alterations.push(Alteration {
            degree: ChordDegree::Fifth,
            interval: Interval::DiminishedFifth,
        });
    }

    if has_aug_fifth && quality != ChordQuality::Augmented {
        alterations.push(Alteration {
            degree: ChordDegree::Fifth,
            interval: Interval::AugmentedFifth,
        });
    }

    // Altered ninths
    if has_minor_ninth {
        alterations.push(Alteration {
            degree: ChordDegree::Ninth,
            interval: Interval::FlatNinth,
        });
    }

    if has_sharp_ninth {
        alterations.push(Alteration {
            degree: ChordDegree::Ninth,
            interval: Interval::SharpNinth,
        });
    }

    // Sharp eleventh - either from explicit semitone 18, OR from tritone when P5 is also present
    if has_sharp_eleventh || tritone_is_sharp11 {
        alterations.push(Alteration {
            degree: ChordDegree::Eleventh,
            interval: Interval::SharpEleventh,
        });
    }

    // Altered thirteenth
    if has_flat_thirteenth {
        alterations.push(Alteration {
            degree: ChordDegree::Thirteenth,
            interval: Interval::FlatThirteenth,
        });
    }

    Ok(ChordInfo {
        quality,
        family,
        extensions,
        alterations,
        additions,
    })
}

/// Build a Chord from the analyzed information
fn build_chord(root: RootNotation, info: ChordInfo, original_semitones: &[u8]) -> Result<Chord> {
    let mut chord = if let Some(family) = info.family {
        Chord::with_family(root, info.quality, family)
    } else {
        Chord::new(root, info.quality)
    };

    // Apply extensions
    chord.extensions = info.extensions;

    // Apply alterations
    chord.alterations = info.alterations;

    // Apply additions (6th, add4, add2)
    chord.additions = info.additions;

    // Recompute intervals with the new extensions, alterations, and additions
    chord.compute_intervals();
    chord.normalize();

    // Set origin to indicate this was created from semitones
    chord.origin = format!("from_semitones({:?})", original_semitones);

    Ok(chord)
}

/// Build a Chord from the analyzed information, with inversion support
///
/// When `root_offset` is non-zero, the chord is an inversion:
/// - The new root is `root_offset` semitones above the original
/// - The original root becomes the bass note
fn build_chord_with_inversion(
    original_root: RootNotation,
    info: ChordInfo,
    original_semitones: &[u8],
    root_offset: u8,
) -> Result<Chord> {
    if root_offset == 0 {
        // No inversion needed - use the original root
        return build_chord(original_root, info, original_semitones);
    }

    // Calculate the new root note (the actual chord root)
    let original_note = original_root
        .resolved_note()
        .ok_or_else(|| SemitoneSequenceError::UnrecognizedPattern(original_semitones.to_vec()))?;

    // Determine if we should prefer sharps or flats based on the original root
    let prefer_sharp = original_note.name().contains('#');

    // Create the new root note
    let new_root_semitone = (original_note.semitone() + root_offset) % 12;
    let new_root_note = MusicalNote::from_semitone(new_root_semitone, prefer_sharp);
    let new_root = RootNotation::from_note_name(new_root_note);

    // Build the chord with the new root
    let mut chord = if let Some(family) = info.family {
        Chord::with_family(new_root, info.quality, family)
    } else {
        Chord::new(new_root, info.quality)
    };

    // Apply extensions, alterations, additions
    chord.extensions = info.extensions;
    chord.alterations = info.alterations.clone();
    chord.additions = info.additions;

    // Set the original root as the bass note (slash chord)
    chord.bass = Some(original_root.clone());

    // Fix chord quality when the bass note conflicts with the detected quality.
    // Example: D/F — bass F is minor 3rd from D, but chord has F# (major 3rd).
    // The analysis sees both m3 and M3 and picks minor, but the m3 is just the bass note.
    let bass_from_new_root = (12 - root_offset) % 12; // interval from new root to bass
    if bass_from_new_root == 3 && chord.quality == ChordQuality::Minor {
        // Bass note is a minor 3rd from root. Check if the original semitones
        // contain a major 3rd (4 semitones from new root) — if so, switch to Major.
        let pcs: HashSet<u8> = original_semitones.iter().map(|s| s % 12).collect();
        let rotated: HashSet<u8> = pcs.iter().map(|&pc| (pc + 12 - root_offset) % 12).collect();
        if rotated.contains(&4) {
            chord.quality = ChordQuality::Major;
        }
    }

    // Note: We do NOT remove alterations that match the bass note's pitch class.
    // Example: Db7#11/G — the #11 (G) IS the bass note AND a legitimate chord extension.
    // Both roles coexist in slash chords.

    // Recompute intervals
    chord.compute_intervals();
    chord.normalize();

    // Set origin to indicate this was created from semitones with inversion
    chord.origin = format!(
        "from_semitones({:?}) -> inverted from root offset {}",
        original_semitones, root_offset
    );

    Ok(chord)
}

/// Get the chord quality name from a semitone sequence
///
/// This is a convenience function that returns just the quality name without
/// constructing a full Chord object.
///
/// # Examples
/// ```
/// use keyflow_proto::chord::quality_from_semitones;
///
/// assert_eq!(quality_from_semitones(&[0, 4, 7]), Some("Major"));
/// assert_eq!(quality_from_semitones(&[0, 3, 7]), Some("Minor"));
/// assert_eq!(quality_from_semitones(&[0, 3, 6]), Some("Diminished"));
/// assert_eq!(quality_from_semitones(&[0, 4, 8]), Some("Augmented"));
/// ```
pub fn quality_from_semitones(semitones: &[u8]) -> Option<&'static str> {
    let pitch_classes: HashSet<u8> = semitones.iter().map(|s| s % 12).collect();

    if !pitch_classes.contains(&0) {
        return None;
    }

    let has_minor_third = pitch_classes.contains(&3);
    let has_major_third = pitch_classes.contains(&4);
    let has_sus2 = pitch_classes.contains(&2);
    let has_sus4 =
        pitch_classes.contains(&5) && !pitch_classes.contains(&4) && !pitch_classes.contains(&3);
    let has_perfect_fifth = pitch_classes.contains(&7);
    let has_dim_fifth = pitch_classes.contains(&6);
    let has_aug_fifth = pitch_classes.contains(&8);

    if has_sus2 && !has_major_third && !has_minor_third {
        Some("Sus2")
    } else if has_sus4 {
        Some("Sus4")
    } else if !has_major_third && !has_minor_third && has_perfect_fifth {
        Some("Power")
    } else if has_dim_fifth && has_minor_third && !has_aug_fifth && !has_perfect_fifth {
        Some("Diminished")
    } else if has_aug_fifth && has_major_third && !has_perfect_fifth {
        Some("Augmented")
    } else if has_minor_third {
        Some("Minor")
    } else if has_major_third {
        Some("Major")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::MusicalNote;

    fn c_root() -> RootNotation {
        let c_note = MusicalNote::from_string("C").unwrap();
        RootNotation::from_note_name(c_note)
    }

    #[test]
    fn test_major_triad() {
        let chord = from_semitones(&[0, 4, 7], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Major);
        assert_eq!(chord.family, None);
        assert_eq!(chord.semitone_sequence(), vec![0, 4, 7]);
    }

    #[test]
    fn test_minor_triad() {
        let chord = from_semitones(&[0, 3, 7], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Minor);
        assert_eq!(chord.family, None);
        assert_eq!(chord.semitone_sequence(), vec![0, 3, 7]);
    }

    #[test]
    fn test_diminished_triad() {
        let chord = from_semitones(&[0, 3, 6], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Diminished);
        assert_eq!(chord.family, None);
        assert_eq!(chord.semitone_sequence(), vec![0, 3, 6]);
    }

    #[test]
    fn test_augmented_triad() {
        let chord = from_semitones(&[0, 4, 8], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Augmented);
        assert_eq!(chord.family, None);
        assert_eq!(chord.semitone_sequence(), vec![0, 4, 8]);
    }

    #[test]
    fn test_suspended_2() {
        let chord = from_semitones(&[0, 2, 7], c_root()).unwrap();
        assert_eq!(
            chord.quality,
            ChordQuality::Suspended(SuspendedType::Second)
        );
    }

    #[test]
    fn test_suspended_4() {
        let chord = from_semitones(&[0, 5, 7], c_root()).unwrap();
        assert_eq!(
            chord.quality,
            ChordQuality::Suspended(SuspendedType::Fourth)
        );
    }

    #[test]
    fn test_power_chord() {
        let chord = from_semitones(&[0, 7], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Power);
    }

    #[test]
    fn test_dominant_seventh() {
        let chord = from_semitones(&[0, 4, 7, 10], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Major);
        assert_eq!(chord.family, Some(ChordFamily::Dominant7));
        assert_eq!(chord.semitone_sequence(), vec![0, 4, 7, 10]);
    }

    #[test]
    fn test_major_seventh() {
        let chord = from_semitones(&[0, 4, 7, 11], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Major);
        assert_eq!(chord.family, Some(ChordFamily::Major7));
        assert_eq!(chord.semitone_sequence(), vec![0, 4, 7, 11]);
    }

    #[test]
    fn test_minor_seventh() {
        let chord = from_semitones(&[0, 3, 7, 10], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Minor);
        assert_eq!(chord.family, Some(ChordFamily::Minor7));
        assert_eq!(chord.semitone_sequence(), vec![0, 3, 7, 10]);
    }

    #[test]
    fn test_minor_major_seventh() {
        let chord = from_semitones(&[0, 3, 7, 11], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Minor);
        assert_eq!(chord.family, Some(ChordFamily::MinorMajor7));
        assert_eq!(chord.semitone_sequence(), vec![0, 3, 7, 11]);
    }

    #[test]
    fn test_half_diminished_seventh() {
        let chord = from_semitones(&[0, 3, 6, 10], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Diminished);
        assert_eq!(chord.family, Some(ChordFamily::HalfDiminished));
        assert_eq!(chord.semitone_sequence(), vec![0, 3, 6, 10]);
    }

    #[test]
    fn test_diminished_seventh() {
        let chord = from_semitones(&[0, 3, 6, 9], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Diminished);
        assert_eq!(chord.family, Some(ChordFamily::FullyDiminished));
        assert_eq!(chord.semitone_sequence(), vec![0, 3, 6, 9]);
    }

    #[test]
    fn test_dominant_ninth() {
        let chord = from_semitones(&[0, 4, 7, 10, 14], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Major);
        assert_eq!(chord.family, Some(ChordFamily::Dominant7));
        assert!(chord.extensions.ninth.is_some());
        assert_eq!(chord.semitone_sequence(), vec![0, 4, 7, 10, 14]);
    }

    #[test]
    fn test_major_ninth() {
        let chord = from_semitones(&[0, 4, 7, 11, 14], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Major);
        assert_eq!(chord.family, Some(ChordFamily::Major7));
        assert!(chord.extensions.ninth.is_some());
        assert_eq!(chord.semitone_sequence(), vec![0, 4, 7, 11, 14]);
    }

    #[test]
    fn test_dominant_thirteenth() {
        let chord = from_semitones(&[0, 4, 7, 10, 14, 17, 21], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Major);
        assert_eq!(chord.family, Some(ChordFamily::Dominant7));
        assert!(chord.extensions.ninth.is_some());
        assert!(chord.extensions.eleventh.is_some());
        assert!(chord.extensions.thirteenth.is_some());
    }

    #[test]
    fn test_altered_chord_b5() {
        let chord = from_semitones(&[0, 4, 6, 10], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Major);
        assert_eq!(chord.family, Some(ChordFamily::Dominant7));
        assert_eq!(chord.alterations.len(), 1);
        assert_eq!(chord.alterations[0].degree, ChordDegree::Fifth);
    }

    #[test]
    fn test_altered_chord_sharp5() {
        let chord = from_semitones(&[0, 4, 8, 10], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Major);
        assert_eq!(chord.family, Some(ChordFamily::Dominant7));
        assert_eq!(chord.alterations.len(), 1);
        assert_eq!(chord.alterations[0].degree, ChordDegree::Fifth);
    }

    #[test]
    fn test_quality_from_semitones() {
        assert_eq!(quality_from_semitones(&[0, 4, 7]), Some("Major"));
        assert_eq!(quality_from_semitones(&[0, 3, 7]), Some("Minor"));
        assert_eq!(quality_from_semitones(&[0, 3, 6]), Some("Diminished"));
        assert_eq!(quality_from_semitones(&[0, 4, 8]), Some("Augmented"));
        assert_eq!(quality_from_semitones(&[0, 7]), Some("Power"));
    }

    #[test]
    fn test_empty_sequence() {
        let result = from_semitones(&[], c_root());
        assert!(matches!(result, Err(SemitoneSequenceError::EmptySequence)));
    }

    #[test]
    fn test_missing_root() {
        let result = from_semitones(&[4, 7], c_root());
        assert!(matches!(result, Err(SemitoneSequenceError::MissingRoot)));
    }

    #[test]
    fn test_unordered_input() {
        // Should handle unordered input
        let chord = from_semitones(&[7, 0, 4], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Major);
        assert_eq!(chord.semitone_sequence(), vec![0, 4, 7]);
    }

    #[test]
    fn test_with_duplicates() {
        // Should handle duplicates
        let chord = from_semitones(&[0, 4, 4, 7, 0], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Major);
        assert_eq!(chord.semitone_sequence(), vec![0, 4, 7]);
    }

    #[test]
    fn test_round_trip_major_triad() {
        // Test: semitones -> chord -> semitones produces same result
        let original_semitones = vec![0, 4, 7];
        let chord = from_semitones(&original_semitones, c_root()).unwrap();
        let recovered_semitones = chord.semitone_sequence();
        assert_eq!(recovered_semitones, original_semitones);
    }

    #[test]
    fn test_round_trip_minor_seventh() {
        let original_semitones = vec![0, 3, 7, 10];
        let chord = from_semitones(&original_semitones, c_root()).unwrap();
        let recovered_semitones = chord.semitone_sequence();
        assert_eq!(recovered_semitones, original_semitones);
    }

    #[test]
    fn test_round_trip_dominant_ninth() {
        let original_semitones = vec![0, 4, 7, 10, 14];
        let chord = from_semitones(&original_semitones, c_root()).unwrap();
        let recovered_semitones = chord.semitone_sequence();
        assert_eq!(recovered_semitones, original_semitones);
    }

    #[test]
    fn test_round_trip_dominant_thirteenth() {
        let original_semitones = vec![0, 4, 7, 10, 14, 17, 21];
        let chord = from_semitones(&original_semitones, c_root()).unwrap();
        let recovered_semitones = chord.semitone_sequence();
        assert_eq!(recovered_semitones, original_semitones);
    }

    #[test]
    fn test_round_trip_altered_chord() {
        // C7#5: C E G# Bb
        let original_semitones = vec![0, 4, 8, 10];
        let chord = from_semitones(&original_semitones, c_root()).unwrap();
        let recovered_semitones = chord.semitone_sequence();
        assert_eq!(recovered_semitones, original_semitones);
    }

    #[test]
    fn test_round_trip_diminished_seventh() {
        let original_semitones = vec![0, 3, 6, 9];
        let chord = from_semitones(&original_semitones, c_root()).unwrap();
        let recovered_semitones = chord.semitone_sequence();
        assert_eq!(recovered_semitones, original_semitones);
    }

    #[test]
    fn test_round_trip_suspended() {
        // Csus4
        let original_semitones = vec![0, 5, 7];
        let chord = from_semitones(&original_semitones, c_root()).unwrap();
        let recovered_semitones = chord.semitone_sequence();
        assert_eq!(recovered_semitones, original_semitones);
    }

    #[test]
    fn test_round_trip_augmented() {
        // Caug (pure triad)
        let original_semitones = vec![0, 4, 8];
        let chord = from_semitones(&original_semitones, c_root()).unwrap();
        let recovered_semitones = chord.semitone_sequence();
        assert_eq!(recovered_semitones, original_semitones);
    }

    #[test]
    fn test_round_trip_with_unordered_input() {
        // Input is unordered, but output should be sorted
        let unordered_input = vec![7, 0, 10, 4];
        let expected_output = vec![0, 4, 7, 10];

        let chord = from_semitones(&unordered_input, c_root()).unwrap();
        let recovered_semitones = chord.semitone_sequence();

        // Should match the sorted version
        assert_eq!(recovered_semitones, expected_output);
    }

    // ===== 6th Chord Tests =====

    #[test]
    fn test_major_sixth_chord() {
        // C6: C E G A (0, 4, 7, 9)
        let chord = from_semitones(&[0, 4, 7, 9], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Major);
        assert_eq!(chord.family, None); // 6th chords have no family (no 7th)
        assert!(chord.additions.contains(&ChordDegree::Sixth));
        // Display should show "C6" (standard notation for major sixth)
        assert_eq!(chord.to_string(), "C6");
    }

    #[test]
    fn test_minor_sixth_chord() {
        // Cm6: C Eb G A (0, 3, 7, 9)
        let chord = from_semitones(&[0, 3, 7, 9], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Minor);
        assert_eq!(chord.family, None);
        assert!(chord.additions.contains(&ChordDegree::Sixth));
        // Display should show "Cm6"
        assert_eq!(chord.to_string(), "Cm6");
    }

    #[test]
    fn test_six_nine_chord() {
        // C6/9: C E G A D (0, 4, 7, 9, 14)
        let chord = from_semitones(&[0, 4, 7, 9, 14], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Major);
        assert!(chord.additions.contains(&ChordDegree::Sixth));
        // 9th is an addition (not extension) since there's no 7th
        assert!(chord.additions.contains(&ChordDegree::Ninth));
        // Display should show "C6/9"
        let display = chord.to_string();
        assert!(display.contains("6/9"), "Expected '6/9' in {}", display);
    }

    #[test]
    fn test_diminished_seventh_not_sixth() {
        // Cdim7: C Eb Gb Bbb (0, 3, 6, 9)
        // 9 semitones here should be dim7, NOT 6th
        let chord = from_semitones(&[0, 3, 6, 9], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Diminished);
        assert_eq!(chord.family, Some(ChordFamily::FullyDiminished));
        assert!(!chord.additions.contains(&ChordDegree::Sixth));
    }

    #[test]
    fn test_round_trip_major_sixth() {
        let original = vec![0, 4, 7, 9];
        let chord = from_semitones(&original, c_root()).unwrap();
        let recovered = chord.semitone_sequence();
        assert_eq!(recovered, original);
    }

    #[test]
    fn test_round_trip_minor_sixth() {
        let original = vec![0, 3, 7, 9];
        let chord = from_semitones(&original, c_root()).unwrap();
        let recovered = chord.semitone_sequence();
        assert_eq!(recovered, original);
    }

    // ===== Add4 Tests =====

    #[test]
    fn test_add4_major() {
        // Cadd4: C E F G (0, 4, 5, 7)
        let chord = from_semitones(&[0, 4, 5, 7], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Major);
        assert!(chord.additions.contains(&ChordDegree::Fourth));
        // Display should show "Cadd4"
        let display = chord.to_string();
        assert!(display.contains("add4"), "Expected 'add4' in {}", display);
    }

    #[test]
    fn test_add4_minor() {
        // Cmadd4: C Eb F G (0, 3, 5, 7)
        let chord = from_semitones(&[0, 3, 5, 7], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Minor);
        assert!(chord.additions.contains(&ChordDegree::Fourth));
        // Display should show "Cmadd4"
        let display = chord.to_string();
        assert!(display.contains("add4"), "Expected 'add4' in {}", display);
    }

    #[test]
    fn test_sus4_vs_add4() {
        // Csus4: C F G (0, 5, 7) - no third
        let sus4 = from_semitones(&[0, 5, 7], c_root()).unwrap();
        assert_eq!(sus4.quality, ChordQuality::Suspended(SuspendedType::Fourth));
        assert!(!sus4.additions.contains(&ChordDegree::Fourth));

        // Cadd4: C E F G (0, 4, 5, 7) - has major third
        let add4 = from_semitones(&[0, 4, 5, 7], c_root()).unwrap();
        assert_eq!(add4.quality, ChordQuality::Major);
        assert!(add4.additions.contains(&ChordDegree::Fourth));
    }

    #[test]
    fn test_round_trip_add4() {
        let original = vec![0, 4, 5, 7];
        let chord = from_semitones(&original, c_root()).unwrap();
        let recovered = chord.semitone_sequence();
        assert_eq!(recovered, original);
    }

    // ===== Add2 Tests =====

    #[test]
    fn test_add2_major() {
        // Cadd2: C D E G (0, 2, 4, 7)
        let chord = from_semitones(&[0, 2, 4, 7], c_root()).unwrap();
        assert_eq!(chord.quality, ChordQuality::Major);
        assert!(chord.additions.contains(&ChordDegree::Second));
        // Display should show "Cadd2"
        let display = chord.to_string();
        assert!(display.contains("add2"), "Expected 'add2' in {}", display);
    }

    #[test]
    fn test_sus2_vs_add2() {
        // Csus2: C D G (0, 2, 7) - no third
        let sus2 = from_semitones(&[0, 2, 7], c_root()).unwrap();
        assert_eq!(sus2.quality, ChordQuality::Suspended(SuspendedType::Second));
        assert!(!sus2.additions.contains(&ChordDegree::Second));

        // Cadd2: C D E G (0, 2, 4, 7) - has major third
        let add2 = from_semitones(&[0, 2, 4, 7], c_root()).unwrap();
        assert_eq!(add2.quality, ChordQuality::Major);
        assert!(add2.additions.contains(&ChordDegree::Second));
    }

    #[test]
    fn test_round_trip_add2() {
        let original = vec![0, 2, 4, 7];
        let chord = from_semitones(&original, c_root()).unwrap();
        let recovered = chord.semitone_sequence();
        assert_eq!(recovered, original);
    }

    #[test]
    fn test_chord_types_survey() {
        // 7sus4: C F G Bb (0, 5, 7, 10)
        let chord = from_semitones(&[0, 5, 7, 10], c_root()).unwrap();
        println!(
            "7sus4 [0,5,7,10]: {} (quality: {:?}, family: {:?})",
            chord, chord.quality, chord.family
        );

        // 9sus4: C F G Bb D (0, 5, 7, 10, 14)
        let chord = from_semitones(&[0, 5, 7, 10, 14], c_root()).unwrap();
        println!(
            "9sus4 [0,5,7,10,14]: {} (quality: {:?}, family: {:?}, ext: {:?})",
            chord, chord.quality, chord.family, chord.extensions
        );

        // add9: C E G D (0, 4, 7, 14) - triad + 9th, no 7th
        let chord = from_semitones(&[0, 4, 7, 14], c_root()).unwrap();
        println!(
            "add9 [0,4,7,14]: {} (quality: {:?}, family: {:?}, add: {:?})",
            chord, chord.quality, chord.family, chord.additions
        );

        // madd9: C Eb G D (0, 3, 7, 14)
        let chord = from_semitones(&[0, 3, 7, 14], c_root()).unwrap();
        println!(
            "madd9 [0,3,7,14]: {} (quality: {:?}, family: {:?}, add: {:?})",
            chord, chord.quality, chord.family, chord.additions
        );

        // add11: C E G F (0, 4, 7, 17)
        let chord = from_semitones(&[0, 4, 7, 17], c_root()).unwrap();
        println!("add11 [0,4,7,17]: {} (add: {:?})", chord, chord.additions);

        // madd11: C Eb G F (0, 3, 7, 17)
        let chord = from_semitones(&[0, 3, 7, 17], c_root()).unwrap();
        println!("madd11 [0,3,7,17]: {} (add: {:?})", chord, chord.additions);

        // m6/9: C Eb G A D (0, 3, 7, 9, 14)
        let chord = from_semitones(&[0, 3, 7, 9, 14], c_root()).unwrap();
        println!("m6/9 [0,3,7,9,14]: {} (add: {:?})", chord, chord.additions);

        // aug7: C E G# Bb (0, 4, 8, 10)
        let chord = from_semitones(&[0, 4, 8, 10], c_root()).unwrap();
        println!(
            "aug7 [0,4,8,10]: {} (quality: {:?}, family: {:?}, alt: {:?})",
            chord, chord.quality, chord.family, chord.alterations
        );

        // augmaj7: C E G# B (0, 4, 8, 11)
        let chord = from_semitones(&[0, 4, 8, 11], c_root()).unwrap();
        println!(
            "augmaj7 [0,4,8,11]: {} (quality: {:?}, family: {:?}, alt: {:?})",
            chord, chord.quality, chord.family, chord.alterations
        );

        // 7alt: C E Bb Db D# (0, 4, 10, 13, 15) - dominant with b9 and #9
        let chord = from_semitones(&[0, 4, 10, 13, 15], c_root()).unwrap();
        println!(
            "7alt [0,4,10,13,15]: {} (alt: {:?})",
            chord, chord.alterations
        );

        // 7#9: C E G Bb D# (0, 4, 7, 10, 15)
        let chord = from_semitones(&[0, 4, 7, 10, 15], c_root()).unwrap();
        println!(
            "7#9 [0,4,7,10,15]: {} (alt: {:?})",
            chord, chord.alterations
        );

        // 7b9: C E G Bb Db (0, 4, 7, 10, 13)
        let chord = from_semitones(&[0, 4, 7, 10, 13], c_root()).unwrap();
        println!(
            "7b9 [0,4,7,10,13]: {} (alt: {:?})",
            chord, chord.alterations
        );

        // m9: C Eb G Bb D (0, 3, 7, 10, 14)
        let chord = from_semitones(&[0, 3, 7, 10, 14], c_root()).unwrap();
        println!("m9 [0,3,7,10,14]: {}", chord);

        // maj9: C E G B D (0, 4, 7, 11, 14)
        let chord = from_semitones(&[0, 4, 7, 11, 14], c_root()).unwrap();
        println!("maj9 [0,4,7,11,14]: {}", chord);

        // m11: C Eb G Bb D F (0, 3, 7, 10, 14, 17)
        let chord = from_semitones(&[0, 3, 7, 10, 14, 17], c_root()).unwrap();
        println!("m11 [0,3,7,10,14,17]: {}", chord);

        // 13: C E G Bb D F A (0, 4, 7, 10, 14, 17, 21)
        let chord = from_semitones(&[0, 4, 7, 10, 14, 17, 21], c_root()).unwrap();
        println!("13 [0,4,7,10,14,17,21]: {}", chord);
    }

    // ===== Inversion Detection Tests =====

    #[test]
    fn test_inversion_f_over_c() {
        // C F A (0, 5, 9) - This is F major with C in the bass
        // Should be detected as F/C, not Csus4add6
        let chord = from_semitones(&[0, 5, 9], c_root()).unwrap();

        // The chord should be F major with C bass
        assert_eq!(chord.quality, ChordQuality::Major);
        assert!(chord.bass.is_some(), "Should have a bass note");
        assert_eq!(chord.bass.as_ref().unwrap().to_string(), "C");
        assert_eq!(chord.root.to_string(), "F");

        // Display should show slash chord
        let display = chord.to_string();
        assert!(display.contains("/"), "Expected slash chord: {}", display);
    }

    #[test]
    fn test_inversion_am_over_c() {
        // C E A (0, 4, 9) - This is Am with C in the bass (first inversion)
        // Should be detected as Am/C, not Cadd6
        let chord = from_semitones(&[0, 4, 9], c_root()).unwrap();

        // The chord should be Am with C bass
        assert_eq!(chord.quality, ChordQuality::Minor);
        assert!(chord.bass.is_some(), "Should have a bass note");
        assert_eq!(chord.bass.as_ref().unwrap().to_string(), "C");
        assert_eq!(chord.root.to_string(), "A");

        let display = chord.to_string();
        assert!(display.contains("/"), "Expected slash chord: {}", display);
    }

    #[test]
    fn test_inversion_ab_over_c() {
        // C Eb Ab (0, 3, 8) - This is Ab major with C in the bass
        // Should be detected as Ab/C, not some complex chord
        let chord = from_semitones(&[0, 3, 8], c_root()).unwrap();

        // The chord should be Ab major with C bass
        assert_eq!(chord.quality, ChordQuality::Major);
        assert!(chord.bass.is_some(), "Should have a bass note");
        assert_eq!(chord.bass.as_ref().unwrap().to_string(), "C");
        // Ab should be spelled as G# or Ab depending on the algorithm
        let root_str = chord.root.to_string();
        assert!(
            root_str == "Ab" || root_str == "G#",
            "Root should be Ab or G#: {}",
            root_str
        );
    }

    #[test]
    fn test_no_inversion_for_simple_chords() {
        // C E G (0, 4, 7) - Simple major triad, should NOT be inverted
        let chord = from_semitones(&[0, 4, 7], c_root()).unwrap();

        assert_eq!(chord.quality, ChordQuality::Major);
        assert!(
            chord.bass.is_none(),
            "Simple major triad should not have bass note"
        );
        assert_eq!(chord.root.to_string(), "C");
        assert_eq!(chord.to_string(), "C");
    }

    #[test]
    fn test_no_inversion_for_minor_chord() {
        // C Eb G (0, 3, 7) - Simple minor triad, should NOT be inverted
        let chord = from_semitones(&[0, 3, 7], c_root()).unwrap();

        assert_eq!(chord.quality, ChordQuality::Minor);
        assert!(
            chord.bass.is_none(),
            "Simple minor triad should not have bass note"
        );
        assert_eq!(chord.root.to_string(), "C");
    }

    #[test]
    fn test_inversion_with_octave_doubling() {
        // C F A C (0, 5, 9, 12) - F major with doubled root, C in bass
        // Should still be detected as F/C
        let chord = from_semitones(&[0, 5, 9, 12], c_root()).unwrap();

        assert_eq!(chord.quality, ChordQuality::Major);
        assert!(chord.bass.is_some(), "Should have a bass note");
        assert_eq!(chord.bass.as_ref().unwrap().to_string(), "C");
        assert_eq!(chord.root.to_string(), "F");
    }

    #[test]
    fn test_inversion_detection_survey() {
        // Survey of various inversions for debugging
        println!("\n=== Inversion Detection Survey ===");

        // F/C: C F A
        let chord = from_semitones(&[0, 5, 9], c_root()).unwrap();
        println!(
            "[0, 5, 9] C F A: {} (quality: {:?}, bass: {:?})",
            chord, chord.quality, chord.bass
        );

        // Am/C: C E A
        let chord = from_semitones(&[0, 4, 9], c_root()).unwrap();
        println!(
            "[0, 4, 9] C E A: {} (quality: {:?}, bass: {:?})",
            chord, chord.quality, chord.bass
        );

        // Ab/C: C Eb Ab
        let chord = from_semitones(&[0, 3, 8], c_root()).unwrap();
        println!(
            "[0, 3, 8] C Eb Ab: {} (quality: {:?}, bass: {:?})",
            chord, chord.quality, chord.bass
        );

        // Em/G: G B E (should be detected as Em with G bass)
        let g_root = {
            let g_note = MusicalNote::from_string("G").unwrap();
            RootNotation::from_note_name(g_note)
        };
        let chord = from_semitones(&[0, 4, 9], g_root).unwrap();
        println!(
            "[0, 4, 9] on G (G B E): {} (quality: {:?}, bass: {:?})",
            chord, chord.quality, chord.bass
        );

        // G/B: B D G
        let b_root = {
            let b_note = MusicalNote::from_string("B").unwrap();
            RootNotation::from_note_name(b_note)
        };
        let chord = from_semitones(&[0, 3, 8], b_root).unwrap();
        println!(
            "[0, 3, 8] on B (B D G): {} (quality: {:?}, bass: {:?})",
            chord, chord.quality, chord.bass
        );
    }

    #[test]
    fn test_no_inversion_function() {
        // Test the from_semitones_no_inversion function
        // C F A (0, 5, 9) - Should NOT be inverted with this function
        let chord = from_semitones_no_inversion(&[0, 5, 9], c_root()).unwrap();

        // Without inversion detection, this should be some complex chord on C
        assert!(
            chord.bass.is_none(),
            "No inversion should not have bass note"
        );
        assert_eq!(chord.root.to_string(), "C");
    }
}
