//! Rhythm preprocessing for chart layout.
//!
//! This module handles rhythm resolution as a preprocessing step, before engraving.
//! It moves rhythm-related computation from the engraver to the chart module,
//! ensuring the engraver only renders pre-computed data.
//!
//! # Architecture
//!
//! The chart module is responsible for:
//! - Detecting push spillbacks (chords that cross barlines)
//! - Resolving beat structures (normal vs triplet beats)
//! - Computing final chord positions within measures
//!
//! The engraver then receives this resolved data and just renders it.
//!
//! # Usage
//!
//! ```ignore
//! // After parsing, resolve rhythms for a section
//! let resolved = resolve_section_rhythms(section.measures(), use_stems);
//!
//! // The engraver uses resolved.measures[i] for each measure
//! ```

use std::collections::HashMap;

use crate::chord::PushPullBase;

use super::commands::Command;
use super::types::{ChordInstance, Measure};

// ============================================================================
// Core Types
// ============================================================================

/// A chord that pushes back from a measure into the previous measure.
///
/// When a chord has push notation (e.g., `'F/C` with triplet push), it anticipates
/// the beat and starts before its nominal position. If the push crosses a barline,
/// the chord "spills back" into the previous measure.
#[derive(Debug, Clone)]
pub struct Spillback {
    /// The chord symbol that spills back
    pub chord_symbol: String,
    /// The beat position within the affected measure (typically last beat)
    pub beat_position: usize,
    /// The push amount type (Standard, Triplet, etc.)
    pub push_base: PushPullBase,
    /// Push level (1 = eighth, 2 = sixteenth triplet, etc.)
    pub push_level: u8,
    /// Whether the chord has an accent on the pushed beat (AccentOnPush command).
    /// When true, the accent symbol should be rendered at the spillback position.
    /// This is set when the syntax is `>'C` (accent before push marker).
    pub has_accent: bool,
}

/// Describes a single beat's structure in a resolved rhythm.
#[derive(Debug, Clone)]
pub enum BeatStructure {
    /// A normal beat with one segment (e.g., quarter note)
    Normal,
    /// A triplet beat with two segments (triplet quarter + triplet eighth)
    /// Contains optional chord index for pushed chord on the triplet eighth
    Triplet {
        /// Chord index of the pushed chord (if any) that lands on the triplet eighth
        pushed_chord_idx: Option<usize>,
    },
}

impl BeatStructure {
    /// Number of segments this beat contributes
    #[must_use]
    pub const fn segment_count(&self) -> usize {
        match self {
            Self::Normal => 1,
            Self::Triplet { .. } => 2,
        }
    }

    /// Whether this beat is a triplet
    #[must_use]
    pub const fn is_triplet(&self) -> bool {
        matches!(self, Self::Triplet { .. })
    }
}

/// A resolved rhythm entry - either a chord or a rest at a specific position.
#[derive(Debug, Clone)]
pub enum ResolvedEntry {
    /// A chord at this position
    Chord {
        /// Index into the measure's chord array
        chord_idx: usize,
        /// Whether this is a pushed chord (lands on triplet eighth)
        is_pushed: bool,
    },
    /// A spillback chord from the next measure
    Spillback {
        /// The spillback chord info
        spillback: Spillback,
    },
    /// A rest at this position
    Rest,
    /// A fill slash (auto-generated)
    Fill,
}

/// Resolved rhythm data for a single measure.
///
/// This contains everything needed to render the measure's rhythm
/// without the engraver needing to detect spillbacks or compute beat structures.
#[derive(Debug, Clone)]
pub struct ResolvedRhythm {
    /// Beat structures for this measure (Normal or Triplet)
    pub beats: Vec<BeatStructure>,
    /// Total number of segments (sum of beat segment counts)
    pub segment_count: usize,
    /// Mapping from chord index to segment index
    pub chord_to_segment: Vec<usize>,
    /// Spillback chords that land in this measure (from next measure)
    pub spillbacks: Vec<Spillback>,
    /// Whether this measure uses stemmed notation (has triplet beats)
    pub use_stems: bool,
    /// Indices of triplet groups for bracket rendering: (start_segment, end_segment)
    pub triplet_groups: Vec<(usize, usize)>,
}

impl ResolvedRhythm {
    /// Create an empty resolved rhythm
    #[must_use]
    pub fn empty() -> Self {
        Self {
            beats: Vec::new(),
            segment_count: 0,
            chord_to_segment: Vec::new(),
            spillbacks: Vec::new(),
            use_stems: false,
            triplet_groups: Vec::new(),
        }
    }

    /// Check if any triplet beats are present
    #[must_use]
    pub fn has_triplets(&self) -> bool {
        self.beats.iter().any(BeatStructure::is_triplet)
    }
}

/// Resolved rhythm data for an entire section.
#[derive(Debug, Clone)]
pub struct SectionRhythms {
    /// Resolved rhythm for each measure in the section
    pub measures: Vec<ResolvedRhythm>,
}

// ============================================================================
// Spillback Detection
// ============================================================================

/// Detects if a section starts with a pushed chord that spills back.
///
/// Returns the spillback info if the first chord of the section is pushed,
/// or None if there's no cross-section spillback.
#[must_use]
pub fn detect_section_start_spillback(section_measures: &[Measure]) -> Option<Spillback> {
    let first_measure = section_measures.first()?;

    // Find the first non-placeholder chord
    for chord in &first_measure.chords {
        // Skip space/rest placeholders
        if chord.full_symbol == "s" || chord.full_symbol == "r" || chord.full_symbol.is_empty() {
            continue;
        }

        if let Some((is_push, amount)) = &chord.push_pull
            && *is_push
        {
            // Check if this chord has AccentOnPush (>' syntax - accent before push)
            let has_accent = chord
                .commands
                .iter()
                .any(|c| matches!(c, Command::AccentOnPush));
            return Some(Spillback {
                chord_symbol: chord.full_symbol.clone(),
                beat_position: 3, // Last beat of 4/4, will be adjusted based on time sig
                push_base: amount.base,
                push_level: amount.level,
                has_accent,
            });
        }

        // If we found a non-space chord without push_pull, no spillback
        break;
    }
    None
}

/// Detects push spillbacks across measure boundaries in a section.
///
/// Scans all measures for chords with push notation that cross barlines,
/// and returns a map from measure index to the spillback chords that affect it.
#[must_use]
pub fn detect_push_spillbacks(section_measures: &[Measure]) -> HashMap<usize, Vec<Spillback>> {
    let mut result: HashMap<usize, Vec<Spillback>> = HashMap::new();

    for (measure_idx, measure) in section_measures.iter().enumerate() {
        // Check if this measure starts with a pushed chord
        if let Some(first_chord) = measure.chords.first()
            && let Some((is_push, amount)) = &first_chord.push_pull
            && *is_push
        {
            // This chord pushes back - affects the PREVIOUS measure
            if measure_idx > 0 {
                let prev_measure_idx = measure_idx - 1;

                // Get the time signature of the previous measure to find last beat
                let prev_time_sig = section_measures
                    .get(prev_measure_idx)
                    .map(|m| m.time_signature)
                    .unwrap_or((4, 4));
                let last_beat = (prev_time_sig.0 as usize).saturating_sub(1);

                // Check if this chord has AccentOnPush (>' syntax - accent before push)
                let has_accent = first_chord
                    .commands
                    .iter()
                    .any(|c| matches!(c, Command::AccentOnPush));
                let spillback = Spillback {
                    chord_symbol: first_chord.full_symbol.clone(),
                    beat_position: last_beat,
                    push_base: amount.base,
                    push_level: amount.level,
                    has_accent,
                };

                result.entry(prev_measure_idx).or_default().push(spillback);
            }
        }
    }

    result
}

// ============================================================================
// Rhythm Resolution
// ============================================================================

/// Check if a chord has a triplet push
fn has_triplet_push(chord: &ChordInstance) -> bool {
    chord.push_pull.as_ref().is_some_and(|(is_push, amount)| {
        *is_push && amount.base == PushPullBase::Triplet && amount.level == 1
    })
}

/// Resolve the rhythm for a single measure.
///
/// This computes:
/// - Beat structures (normal vs triplet)
/// - Segment count
/// - Chord-to-segment mapping
/// - Triplet groups for bracket rendering
///
/// # Arguments
/// * `measure` - The measure to resolve
/// * `spillbacks` - Spillback chords from the next measure
/// * `global_use_stems` - Whether the chart uses stemmed notation
#[must_use]
pub fn resolve_measure_rhythm(
    measure: &Measure,
    spillbacks: &[Spillback],
    global_use_stems: bool,
) -> ResolvedRhythm {
    let num_beats = measure.time_signature.0 as usize;

    // Determine which beats have triplet structure
    let mut beats_with_triplets: Vec<(bool, Option<usize>)> = vec![(false, None); num_beats];

    // Check chords for triplet pushes
    let mut cumulative_beats = 0usize;
    for (chord_idx, chord) in measure.chords.iter().enumerate() {
        let is_triplet_push = has_triplet_push(chord);

        let chord_duration_beats = match &chord.rhythm {
            crate::chord::ChordRhythm::Slashes { count, .. } => *count as usize,
            _ => 1,
        };

        // Triplet push affects the PREVIOUS beat (where the anticipation lands)
        if is_triplet_push && chord_idx > 0 {
            let target_beat = cumulative_beats.saturating_sub(1);
            if target_beat < num_beats {
                beats_with_triplets[target_beat] = (true, Some(chord_idx));
            }
        }

        cumulative_beats += chord_duration_beats;
    }

    // Check spillbacks for triplet pushes
    for spillback in spillbacks {
        if spillback.push_base == PushPullBase::Triplet && spillback.push_level == 1 {
            let target_beat = spillback.beat_position;
            if target_beat < num_beats && !beats_with_triplets[target_beat].0 {
                beats_with_triplets[target_beat] = (true, None);
            }
        }
    }

    // Determine if we use stems (need triplets and global setting)
    let has_any_triplet = beats_with_triplets
        .iter()
        .any(|(is_triplet, _)| *is_triplet);
    let use_stems = global_use_stems && has_any_triplet;

    // Build beat structures and compute segment mapping
    let mut beats = Vec::with_capacity(num_beats);
    let mut segment_idx = 0;
    let mut triplet_groups = Vec::new();
    let mut chord_to_segment = Vec::new();

    for (beat_idx, (has_triplet, pushed_chord_idx)) in beats_with_triplets.iter().enumerate() {
        if *has_triplet && use_stems {
            // Triplet beat: 2 segments
            let start = segment_idx;
            beats.push(BeatStructure::Triplet {
                pushed_chord_idx: *pushed_chord_idx,
            });
            triplet_groups.push((start, start + 2));
            segment_idx += 2;
        } else {
            // Normal beat: 1 segment
            beats.push(BeatStructure::Normal);
            segment_idx += 1;
        }

        // Map chord at this beat to segment
        // (simplified - actual mapping depends on chord positions)
        if beat_idx < measure.chords.len() {
            chord_to_segment
                .push(segment_idx - beats.last().map(BeatStructure::segment_count).unwrap_or(1));
        }
    }

    // Ensure chord_to_segment has entries for all chords
    while chord_to_segment.len() < measure.chords.len() {
        chord_to_segment.push(chord_to_segment.last().copied().unwrap_or(0));
    }

    ResolvedRhythm {
        beats,
        segment_count: segment_idx,
        chord_to_segment,
        spillbacks: spillbacks.to_vec(),
        use_stems,
        triplet_groups,
    }
}

/// Resolve rhythms for all measures in a section.
///
/// This is the main entry point for rhythm resolution. It:
/// 1. Detects spillbacks across all measures
/// 2. Resolves each measure's rhythm with spillback info
///
/// # Arguments
/// * `measures` - The measures in the section
/// * `use_stems` - Whether to use stemmed notation (from chart settings)
#[must_use]
pub fn resolve_section_rhythms(measures: &[Measure], use_stems: bool) -> SectionRhythms {
    // First detect all spillbacks
    let spillback_map = detect_push_spillbacks(measures);

    // Then resolve each measure with its spillbacks
    let resolved_measures: Vec<ResolvedRhythm> = measures
        .iter()
        .enumerate()
        .map(|(idx, measure)| {
            let spillbacks = spillback_map.get(&idx).map(|v| v.as_slice()).unwrap_or(&[]);
            resolve_measure_rhythm(measure, spillbacks, use_stems)
        })
        .collect();

    SectionRhythms {
        measures: resolved_measures,
    }
}

/// Check if a measure has explicit chord rhythm (LilyPond-style notation).
///
/// Returns true if any chord in the measure has explicit duration notation
/// like `r8t Ab9_8t` instead of simple slash notation.
#[must_use]
pub fn measure_has_explicit_chord_rhythm(measure: &Measure) -> bool {
    use crate::chord::ChordRhythm;

    // Check if any chord has explicit duration (not just slashes)
    measure
        .chords
        .iter()
        .any(|chord| matches!(chord.rhythm, ChordRhythm::Explicit(_)))
        || !measure.rhythm_elements.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beat_structure_segment_count() {
        assert_eq!(BeatStructure::Normal.segment_count(), 1);
        assert_eq!(
            BeatStructure::Triplet {
                pushed_chord_idx: None
            }
            .segment_count(),
            2
        );
    }

    #[test]
    fn test_spillback_detection_empty() {
        let measures: Vec<Measure> = vec![];
        let spillbacks = detect_push_spillbacks(&measures);
        assert!(spillbacks.is_empty());
    }

    #[test]
    fn test_resolved_rhythm_has_triplets() {
        let mut rhythm = ResolvedRhythm::empty();
        assert!(!rhythm.has_triplets());

        rhythm.beats.push(BeatStructure::Triplet {
            pushed_chord_idx: None,
        });
        assert!(rhythm.has_triplets());
    }
}
