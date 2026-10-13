//! Chord timing analysis for push/pull detection
//!
//! Analyzes detected chord positions relative to a beat grid to determine
//! if chords are pushed (early) or pulled (late) by various subdivisions.

use super::duration::{PushPullAmount, PushPullBase};
use super::midi::DetectedChord;
use facet::Facet;

/// Configuration for timing analysis
#[derive(Debug, Clone, Facet)]
pub struct TimingAnalysisConfig {
    /// PPQ resolution of the source MIDI (REAPER uses 960 by default)
    pub source_ppq: i64,
    /// Tolerance in ticks for "on the beat" detection
    /// Default: 5 ticks (very tight for quantized MIDI)
    pub on_beat_tolerance: i64,
    /// Whether to detect triplet push/pull (8th triplet, 16th triplet)
    pub detect_triplets: bool,
    /// Whether to detect standard push/pull (8th, 16th, 32nd)
    pub detect_standard: bool,
}

impl Default for TimingAnalysisConfig {
    fn default() -> Self {
        Self {
            source_ppq: 960,      // REAPER default
            on_beat_tolerance: 5, // Very tight for quantized MIDI
            detect_triplets: true,
            detect_standard: true,
        }
    }
}

/// Result of analyzing a chord's timing relative to the beat grid
#[derive(Debug, Clone)]
pub struct ChordTimingAnalysis {
    /// The original detected chord
    pub chord: DetectedChord,
    /// Which measure this chord belongs to (0-indexed)
    pub measure_index: usize,
    /// Which beat within the measure (0-indexed)
    pub beat_index: u8,
    /// Detected push/pull timing (None = on the beat)
    /// bool is true for push (early), false for pull (late)
    pub push_pull: Option<(bool, PushPullAmount)>,
    /// Exact offset from beat in source PPQ ticks
    /// Negative = early/push, positive = late/pull
    pub offset_ticks: i64,
}

/// Standard push/pull offsets at 960 PPQ (REAPER default)
pub mod offsets_960_ppq {
    /// Eighth note push (half a beat early)
    pub const EIGHTH: i64 = 480;
    /// Sixteenth note push (quarter beat early)
    pub const SIXTEENTH: i64 = 240;
    /// Thirty-second note push (eighth of a beat early)
    pub const THIRTY_SECOND: i64 = 120;
    /// Eighth note triplet push (one-third of a beat early, 960/3)
    pub const EIGHTH_TRIPLET: i64 = 320;
    /// Sixteenth note triplet push (one-sixth of a beat early, 960/6)
    pub const SIXTEENTH_TRIPLET: i64 = 160;
}

/// Analyze chord timing relative to beat grid
///
/// For each chord, determines if it lands on a beat or is pushed/pulled.
/// Uses the time signature to calculate beat positions.
///
/// # Arguments
///
/// * `chords` - Detected chords from MIDI
/// * `beats_per_measure` - Number of beats per measure (time sig numerator)
/// * `time_sig_denominator` - Time signature denominator (4 = quarter, 8 = eighth)
/// * `config` - Timing analysis configuration
///
/// # Returns
///
/// Vector of timing analysis results for each chord
pub fn analyze_chord_timing(
    chords: &[DetectedChord],
    beats_per_measure: u8,
    time_sig_denominator: u8,
    config: &TimingAnalysisConfig,
) -> Vec<ChordTimingAnalysis> {
    // Calculate ticks per beat at source PPQ
    // For 4/4: beat = quarter note = source_ppq ticks
    // For 6/8: beat = eighth note = source_ppq / 2 ticks
    let ticks_per_beat = match time_sig_denominator {
        2 => config.source_ppq * 2,  // half note beats
        4 => config.source_ppq,      // quarter note beats
        8 => config.source_ppq / 2,  // eighth note beats
        16 => config.source_ppq / 4, // sixteenth note beats
        _ => config.source_ppq,      // default to quarter
    };

    let ticks_per_measure = ticks_per_beat * beats_per_measure as i64;

    chords
        .iter()
        .map(|chord| analyze_single_chord(chord, ticks_per_measure, ticks_per_beat, config))
        .collect()
}

/// Analyze a single chord's timing
fn analyze_single_chord(
    chord: &DetectedChord,
    ticks_per_measure: i64,
    ticks_per_beat: i64,
    config: &TimingAnalysisConfig,
) -> ChordTimingAnalysis {
    // Find the measure and beat for this chord
    let measure_index = (chord.start_ppq / ticks_per_measure) as usize;
    let tick_in_measure = chord.start_ppq % ticks_per_measure;

    // Find the nearest beat. round() is correct here because push/pull
    // detection measures distance to the closest beat — a chord 480 ticks
    // before beat 2 should snap to beat 2 (push), not beat 1 (pull).
    let beat_float = tick_in_measure as f64 / ticks_per_beat as f64;
    let nearest_beat = beat_float.round() as i64;
    let beat_start_tick = nearest_beat * ticks_per_beat;

    // Calculate offset from nearest beat
    let offset = tick_in_measure - beat_start_tick;

    // Determine the beat index (handle wrap-around for pushed chords on beat 0)
    let beat_index = if offset < 0 && nearest_beat == 0 {
        // Chord is pushed from the previous measure's last beat
        // For now, just mark it as beat 0 with a push
        0
    } else {
        nearest_beat as u8
    };

    // Check if within tolerance (on the beat)
    if offset.abs() <= config.on_beat_tolerance {
        return ChordTimingAnalysis {
            chord: chord.clone(),
            measure_index,
            beat_index,
            push_pull: None,
            offset_ticks: offset,
        };
    }

    // Analyze for push (negative offset = early) or pull (positive offset = late)
    let is_push = offset < 0;
    let subdivision_offset = offset.abs();

    // Detect the subdivision level
    let push_pull = detect_subdivision(subdivision_offset, ticks_per_beat, config);

    ChordTimingAnalysis {
        chord: chord.clone(),
        measure_index,
        beat_index,
        push_pull: push_pull.map(|pp| (is_push, pp)),
        offset_ticks: offset,
    }
}

/// Detect which subdivision matches the offset
fn detect_subdivision(
    offset: i64,
    ticks_per_beat: i64,
    config: &TimingAnalysisConfig,
) -> Option<PushPullAmount> {
    let tolerance = config.on_beat_tolerance;

    // Standard subdivisions (relative to ticks_per_beat)
    let eighth = ticks_per_beat / 2;
    let sixteenth = ticks_per_beat / 4;
    let thirty_second = ticks_per_beat / 8;

    // Triplet subdivisions
    let eighth_triplet = ticks_per_beat / 3; // 1/3 of a beat
    let sixteenth_triplet = ticks_per_beat / 6; // 1/6 of a beat

    // Check triplet subdivisions first (more specific patterns)
    if config.detect_triplets {
        // Sixteenth triplet (more specific, check first)
        if (offset - sixteenth_triplet).abs() <= tolerance {
            return Some(PushPullAmount {
                level: 2,
                base: PushPullBase::Triplet,
            });
        }
        // Eighth triplet
        if (offset - eighth_triplet).abs() <= tolerance {
            return Some(PushPullAmount {
                level: 1,
                base: PushPullBase::Triplet,
            });
        }
        // Also check for 2/3 of a beat (two eighth triplets)
        let two_eighth_triplets = ticks_per_beat * 2 / 3;
        if (offset - two_eighth_triplets).abs() <= tolerance {
            // This is effectively a push by one eighth triplet from the next beat
            // We'll represent this as an eighth triplet push
            return Some(PushPullAmount {
                level: 1,
                base: PushPullBase::Triplet,
            });
        }
    }

    // Check standard subdivisions
    if config.detect_standard {
        // Thirty-second (most specific, check first)
        if (offset - thirty_second).abs() <= tolerance {
            return Some(PushPullAmount {
                level: 3,
                base: PushPullBase::Standard,
            });
        }
        // Sixteenth
        if (offset - sixteenth).abs() <= tolerance {
            return Some(PushPullAmount {
                level: 2,
                base: PushPullBase::Standard,
            });
        }
        // Eighth
        if (offset - eighth).abs() <= tolerance {
            return Some(PushPullAmount {
                level: 1,
                base: PushPullBase::Standard,
            });
        }
        // Dotted eighth (3/4 of a beat = 3 sixteenths)
        let dotted_eighth = ticks_per_beat * 3 / 4;
        if (offset - dotted_eighth).abs() <= tolerance {
            // Represent as sixteenth push/pull (closest subdivision)
            return Some(PushPullAmount {
                level: 2,
                base: PushPullBase::Standard,
            });
        }
    }

    None // No recognized subdivision
}

/// Convert REAPER PPQ (960) to engraver layout ticks (480)
#[must_use]
pub fn reaper_ppq_to_layout_ticks(reaper_ppq: i64) -> i32 {
    (reaper_ppq / 2) as i32
}

/// Check if any chords in the analysis have push/pull timing
#[must_use]
pub fn has_rhythmic_complexity(analyses: &[ChordTimingAnalysis]) -> bool {
    analyses.iter().any(|a| a.push_pull.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::{Chord, ChordQuality};
    use crate::primitives::{MusicalNote, RootNotation};

    fn make_detected_chord(start_ppq: i64, end_ppq: i64) -> DetectedChord {
        let c_note = MusicalNote::from_string("C").unwrap();
        let root = RootNotation::from_note_name(c_note);
        DetectedChord {
            chord: Chord::new(root, ChordQuality::Major),
            start_ppq,
            end_ppq,
            root_pitch: 60,
            velocity: 100, // Default velocity for test chords
        }
    }

    #[test]
    fn test_on_beat_detection() {
        let config = TimingAnalysisConfig::default();
        let chord = make_detected_chord(960, 1920); // Exactly on beat 1 (measure 1)

        let results = analyze_chord_timing(&[chord], 4, 4, &config);
        assert_eq!(results.len(), 1);
        assert!(results[0].push_pull.is_none());
        assert_eq!(results[0].beat_index, 1);
    }

    #[test]
    fn test_eighth_push_detection() {
        let config = TimingAnalysisConfig::default();
        // Chord pushed by an eighth note (480 ticks early from beat 2)
        // Beat 2 is at 1920, so pushed chord is at 1920 - 480 = 1440
        let chord = make_detected_chord(1440, 2400);

        let results = analyze_chord_timing(&[chord], 4, 4, &config);
        assert_eq!(results.len(), 1);

        let (is_push, amount) = results[0].push_pull.as_ref().unwrap();
        assert!(*is_push); // It's a push (early)
        assert_eq!(amount.level, 1); // Eighth note
        assert_eq!(amount.base, PushPullBase::Standard);
    }

    #[test]
    fn test_sixteenth_push_detection() {
        let config = TimingAnalysisConfig::default();
        // Chord pushed by a sixteenth note (240 ticks early from beat 2)
        let chord = make_detected_chord(1680, 2400); // 1920 - 240 = 1680

        let results = analyze_chord_timing(&[chord], 4, 4, &config);
        assert_eq!(results.len(), 1);

        let (is_push, amount) = results[0].push_pull.as_ref().unwrap();
        assert!(*is_push);
        assert_eq!(amount.level, 2); // Sixteenth note
        assert_eq!(amount.base, PushPullBase::Standard);
    }

    #[test]
    fn test_eighth_triplet_push_detection() {
        let config = TimingAnalysisConfig::default();
        // Chord pushed by an eighth triplet (320 ticks early from beat 2)
        // Beat 2 is at 1920, so pushed chord is at 1920 - 320 = 1600
        let chord = make_detected_chord(1600, 2400);

        let results = analyze_chord_timing(&[chord], 4, 4, &config);
        assert_eq!(results.len(), 1);

        let (is_push, amount) = results[0].push_pull.as_ref().unwrap();
        assert!(*is_push);
        assert_eq!(amount.level, 1); // Eighth note (triplet)
        assert_eq!(amount.base, PushPullBase::Triplet);
    }

    #[test]
    fn test_sixteenth_triplet_push_detection() {
        let config = TimingAnalysisConfig::default();
        // Chord pushed by a sixteenth triplet (160 ticks early from beat 2)
        let chord = make_detected_chord(1760, 2400); // 1920 - 160 = 1760

        let results = analyze_chord_timing(&[chord], 4, 4, &config);
        assert_eq!(results.len(), 1);

        let (is_push, amount) = results[0].push_pull.as_ref().unwrap();
        assert!(*is_push);
        assert_eq!(amount.level, 2); // Sixteenth (triplet)
        assert_eq!(amount.base, PushPullBase::Triplet);
    }

    #[test]
    fn test_pull_detection() {
        let config = TimingAnalysisConfig::default();
        // Chord pulled by an eighth note (480 ticks late from beat 1)
        let chord = make_detected_chord(1440, 2400); // 960 + 480 = 1440

        let results = analyze_chord_timing(&[chord], 4, 4, &config);
        assert_eq!(results.len(), 1);

        // This will be detected as a push from beat 2, not a pull from beat 1
        // because we find the nearest beat
        let (is_push, amount) = results[0].push_pull.as_ref().unwrap();
        assert!(*is_push); // Push from beat 2
        assert_eq!(amount.level, 1);
    }

    #[test]
    fn test_has_rhythmic_complexity() {
        let on_beat = ChordTimingAnalysis {
            chord: make_detected_chord(960, 1920),
            measure_index: 0,
            beat_index: 1,
            push_pull: None,
            offset_ticks: 0,
        };

        let pushed = ChordTimingAnalysis {
            chord: make_detected_chord(1600, 2400),
            measure_index: 0,
            beat_index: 2,
            push_pull: Some((true, PushPullAmount::eighth_triplet())),
            offset_ticks: -320,
        };

        assert!(!has_rhythmic_complexity(std::slice::from_ref(&on_beat)));
        assert!(has_rhythmic_complexity(&[on_beat.clone(), pushed]));
    }

    #[test]
    fn test_6_8_time_signature() {
        let config = TimingAnalysisConfig::default();
        // In 6/8, the beat is an eighth note (480 ticks at 960 PPQ)
        // Beat 1 is at 0, beat 2 at 480, beat 3 at 960, etc.
        let chord = make_detected_chord(480, 960); // Exactly on beat 2

        let results = analyze_chord_timing(&[chord], 6, 8, &config);
        assert_eq!(results.len(), 1);
        assert!(results[0].push_pull.is_none());
        assert_eq!(results[0].beat_index, 1); // 0-indexed, so beat 2 = index 1
    }
}
