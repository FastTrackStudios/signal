//! Time position utilities for converting between time and musical positions
//!
//! This module provides functions to convert time positions (in seconds) to
//! musical positions in the format Measure:Beat:SubBeat (e.g., "3.2.25")
//! using tempo and time signature data from REAPER projects.

use crate::types::time_tempo::{TempoTimeEnvelope, TempoTimePoint};

/// Convert a time position to musical position in Measure:Beat:SubBeat format
///
/// # Arguments
/// * `time_position` - Position in seconds
/// * `tempo_points` - Vector of tempo/time signature change points
/// * `default_tempo` - Default tempo in BPM
/// * `default_time_signature` - Default time signature as (numerator, denominator)
///
/// # Returns
/// String in format "Measure.Beat.SubBeat" (e.g., "3.2.25")
pub fn time_to_beat_position(
    time_position: f64,
    tempo_points: &[TempoTimePoint],
    default_tempo: f64,
    default_time_signature: (i32, i32),
) -> String {
    let (measure, beat, subbeat) = time_to_beat_position_structured(
        time_position,
        tempo_points,
        default_tempo,
        default_time_signature,
    );

    format!("{}.{}.{:02}", measure, beat, subbeat)
}

/// Convert a time position to musical position as structured data
///
/// # Arguments
/// * `time_position` - Position in seconds
/// * `tempo_points` - Vector of tempo/time signature change points
/// * `default_tempo` - Default tempo in BPM
/// * `default_time_signature` - Default time signature as (numerator, denominator)
///
/// # Returns
/// Tuple of (measure, beat, subbeat) where subbeat is in hundredths
pub fn time_to_beat_position_structured(
    time_position: f64,
    tempo_points: &[TempoTimePoint],
    default_tempo: f64,
    default_time_signature: (i32, i32),
) -> (i32, i32, i32) {
    if tempo_points.is_empty() {
        // No tempo changes, simple calculation with effective tempo
        let tempo_ratio = default_time_signature.1 as f64 / 4.0;
        let effective_tempo = default_tempo * tempo_ratio;
        let total_beats = time_position * effective_tempo / 60.0;
        let beats_per_measure = default_time_signature.0 as f64;

        let measure = (total_beats / beats_per_measure).floor() as i32 + 1;
        let beat_in_measure = ((total_beats - 1.0) % beats_per_measure + 1.0) as i32;
        let beat_fraction =
            total_beats - (measure - 1) as f64 * beats_per_measure - (beat_in_measure - 1) as f64;
        let subbeat = (beat_fraction * 100.0).round() as i32;

        return (measure, beat_in_measure, subbeat);
    }

    // Need to account for tempo and time signature changes throughout the song
    let mut last_time = 0.0;
    let mut current_tempo = default_tempo;
    let mut current_time_sig = default_time_signature;
    let mut current_measure = 1.0; // Track measures as we go

    // Add bounds checking to prevent overflow
    let max_measures = 1000.0;

    for point in tempo_points {
        if point.position <= time_position {
            // Add beats for the time segment before this point
            let segment_duration = point.position - last_time;

            // Calculate effective tempo based on time signature
            let tempo_ratio = current_time_sig.1 as f64 / 4.0;
            let effective_tempo = current_tempo * tempo_ratio;

            let segment_beats = segment_duration * effective_tempo / 60.0;

            // Calculate how many measures this segment represents
            let beats_per_measure = current_time_sig.0 as f64;
            let segment_measures = segment_beats / beats_per_measure;
            current_measure += segment_measures;

            // Prevent overflow
            if current_measure > max_measures {
                current_measure = max_measures;
            }

            // Update for next segment
            last_time = point.position;
            current_tempo = point.tempo;
            if let Some(time_sig) = point.time_signature() {
                current_time_sig = time_sig;
            }
        } else {
            // This point is after our target time, add final segment
            let segment_duration = time_position - last_time;

            // Calculate effective tempo based on current time signature
            let tempo_ratio = current_time_sig.1 as f64 / 4.0;
            let effective_tempo = current_tempo * tempo_ratio;

            let segment_beats = segment_duration * effective_tempo / 60.0;

            // Calculate final segment measures
            let beats_per_measure = current_time_sig.0 as f64;
            let segment_measures = segment_beats / beats_per_measure;
            current_measure += segment_measures;

            // Prevent overflow
            if current_measure > max_measures {
                current_measure = max_measures;
            }

            // Calculate final position with bounds checking
            let measure = (current_measure.floor() as i32 + 1).clamp(1, 1000);
            let beat_in_measure =
                ((current_measure - current_measure.floor()) * beats_per_measure + 1.0) as i32;
            let beat_in_measure = beat_in_measure.clamp(1, beats_per_measure as i32);
            let beat_fraction = (current_measure - current_measure.floor()) * beats_per_measure;
            let subbeat = (beat_fraction * 100.0).round() as i32;
            let subbeat = subbeat.clamp(0, 99);

            return (measure, beat_in_measure, subbeat);
        }
    }

    // Add final segment if we haven't reached the target time yet
    if last_time < time_position {
        let segment_duration = time_position - last_time;

        // Calculate effective tempo based on current time signature
        let tempo_ratio = current_time_sig.1 as f64 / 4.0;
        let effective_tempo = current_tempo * tempo_ratio;

        let segment_beats = segment_duration * effective_tempo / 60.0;

        let beats_per_measure = current_time_sig.0 as f64;
        let segment_measures = segment_beats / beats_per_measure;
        current_measure += segment_measures;

        // Prevent overflow
        if current_measure > max_measures {
            current_measure = max_measures;
        }
    }

    // Calculate final position with bounds checking
    let measure = (current_measure.floor() as i32 + 1).clamp(1, 1000);
    let beat_in_measure =
        ((current_measure - current_measure.floor()) * current_time_sig.0 as f64 + 1.0) as i32;
    let beat_in_measure = beat_in_measure.clamp(1, current_time_sig.0);
    let beat_fraction = (current_measure - current_measure.floor()) * current_time_sig.0 as f64;
    let subbeat = (beat_fraction * 100.0).round() as i32;
    let subbeat = subbeat.clamp(0, 99);

    (measure, beat_in_measure, subbeat)
}

/// Convert a time position using a TempoTimeEnvelope
///
/// # Arguments
/// * `time_position` - Position in seconds
/// * `tempo_envelope` - TempoTimeEnvelope containing all tempo/time signature data
///
/// # Returns
/// String in format "Measure.Beat.SubBeat" (e.g., "3.2.25")
pub fn time_to_beat_position_with_envelope(
    time_position: f64,
    tempo_envelope: &TempoTimeEnvelope,
) -> String {
    // Use the existing musical_position_at_time method from TempoTimeEnvelope
    // which properly handles time signature changes
    let (measure, beat, fraction) = tempo_envelope.musical_position_at_time(time_position);

    // Convert fraction to subbeat (hundredths)
    let subbeat = (fraction * 100.0).round() as i32;

    format!("{}.{}.{:02}", measure, beat, subbeat)
}

/// Convert a time position using a TempoTimeEnvelope (structured version)
///
/// # Arguments
/// * `time_position` - Position in seconds
/// * `tempo_envelope` - TempoTimeEnvelope containing all tempo/time signature data
///
/// # Returns
/// Tuple of (measure, beat, subbeat) where subbeat is in hundredths
pub fn time_to_beat_position_structured_with_envelope(
    time_position: f64,
    tempo_envelope: &TempoTimeEnvelope,
) -> (i32, i32, i32) {
    // Use the existing musical_position_at_time method from TempoTimeEnvelope
    // which properly handles time signature changes
    let (measure, beat, fraction) = tempo_envelope.musical_position_at_time(time_position);

    // Convert fraction to subbeat (hundredths)
    let subbeat = (fraction * 100.0).round() as i32;

    (measure, beat, subbeat)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simple test to calculate beat positions with constant tempo (no tempo changes)
    /// This helps us understand the basic calculation before adding tempo change complexity
    fn calculate_beat_position_constant_tempo(
        time_seconds: f64,
        tempo_bpm: f64,
        time_signature: (i32, i32),
    ) -> (i32, i32, f64) {
        // Calculate total beats from start
        let total_beats = time_seconds * tempo_bpm / 60.0;

        // Calculate measure and beat within measure
        let beats_per_measure = time_signature.0 as f64;

        // Measure is 1-based
        let measure = (total_beats / beats_per_measure).floor() as i32 + 1;

        // Beat within the measure (1-based)
        // For 1.833 beats: we're in measure 1, beat 1 (since we're still in the first beat of the measure)
        // The beat number is the floor of the total beats within the measure, plus 1
        let beats_within_measure = total_beats % beats_per_measure;
        let beat_in_measure = (beats_within_measure.floor() as i32) + 1;

        // Fraction of the beat (0.0 to 1.0)
        let beat_fraction =
            (total_beats % beats_per_measure) - (total_beats % beats_per_measure).floor();

        (measure, beat_in_measure, beat_fraction)
    }

    /// Calculate beat positions with tempo and time signature changes
    /// Takes a list of tempo/time signature points: (time_seconds, tempo_bpm, time_signature)
    // r[impl beat-pos.tempo-changes]
    // r[impl beat-pos.float-snap]
    fn calculate_beat_position_with_changes(
        time_seconds: f64,
        change_points: &[(f64, f64, (i32, i32))],
        default_time_signature: (i32, i32),
    ) -> (i32, i32, f64) {
        let mut current_measure = 1;
        let mut current_beat = 1;
        let mut current_beat_fraction = 0.0;
        let mut last_time = 0.0;
        let mut current_tempo = change_points[0].1; // Start with first tempo
        let mut current_time_sig = default_time_signature;

        println!("Calculating beat position for time {:.3}s:", time_seconds);
        println!(
            "  Starting at measure {}, beat {}, fraction {:.3}",
            current_measure, current_beat, current_beat_fraction
        );

        // Process each change point to calculate beats incrementally
        let mut found_target = false;

        for &(point_time, point_tempo, point_time_sig) in change_points.iter() {
            if point_time <= time_seconds {
                // This change point is before or at our target time
                // Add beats for the segment from last_time to point_time
                let segment_duration = point_time - last_time;

                // Calculate effective tempo based on time signature
                // Convert tempo to quarter note equivalents based on the note value
                // Ratio = denominator / 4 (e.g., 8/4 = 2.0 for eighth notes, 2/4 = 0.5 for half notes)
                let tempo_ratio = current_time_sig.1 as f64 / 4.0;
                let effective_tempo = current_tempo * tempo_ratio;

                let segment_beats = segment_duration * effective_tempo / 60.0;

                println!(
                    "  Segment {:.3}s to {:.3}s: {:.3}s at {:.1} BPM ({}/{} time) = {:.1} effective BPM = {:.3} beats",
                    last_time,
                    point_time,
                    segment_duration,
                    current_tempo,
                    current_time_sig.0,
                    current_time_sig.1,
                    effective_tempo,
                    segment_beats
                );

                // Add beats to current position
                current_beat_fraction += segment_beats;

                // Snap near-integer fractions to avoid floating-point drift
                let rounded = current_beat_fraction.round();
                if (current_beat_fraction - rounded).abs() < 1e-9 {
                    current_beat_fraction = rounded;
                }

                // Handle measure/beat overflow
                let _beats_per_measure = current_time_sig.0 as f64;
                while current_beat_fraction >= 1.0 {
                    println!(
                        "      Overflow: fraction {:.15} >= 1.0, advancing beat from {} to {}",
                        current_beat_fraction,
                        current_beat,
                        current_beat + 1
                    );
                    current_beat_fraction -= 1.0;
                    current_beat += 1;
                    if current_beat > current_time_sig.0 {
                        println!(
                            "      Beat {} > {}, advancing measure from {} to {}",
                            current_beat,
                            current_time_sig.0,
                            current_measure,
                            current_measure + 1
                        );
                        current_beat = 1;
                        current_measure += 1;
                    }
                    println!(
                        "      After overflow: measure {}, beat {}, fraction {:.15}",
                        current_measure, current_beat, current_beat_fraction
                    );
                }

                println!(
                    "    After segment: measure {}, beat {}, fraction {:.3}",
                    current_measure, current_beat, current_beat_fraction
                );

                // Update for next segment
                last_time = point_time;
                current_tempo = point_tempo;
                current_time_sig = point_time_sig;
            } else {
                // This change point is after our target time
                // Add final segment from last_time to target time
                let segment_duration = time_seconds - last_time;

                // Calculate effective tempo based on current time signature
                // Convert tempo to quarter note equivalents based on the note value
                // Ratio = denominator / 4 (e.g., 8/4 = 2.0 for eighth notes, 2/4 = 0.5 for half notes)
                let tempo_ratio = current_time_sig.1 as f64 / 4.0;
                let effective_tempo = current_tempo * tempo_ratio;

                let segment_beats = segment_duration * effective_tempo / 60.0;

                println!(
                    "  Final segment {:.3}s to {:.3}s: {:.3}s at {:.1} BPM ({}/{} time) = {:.1} effective BPM = {:.3} beats",
                    last_time,
                    time_seconds,
                    segment_duration,
                    current_tempo,
                    current_time_sig.0,
                    current_time_sig.1,
                    effective_tempo,
                    segment_beats
                );

                // Add final beats to current position
                current_beat_fraction += segment_beats;

                // Snap near-integer fractions to avoid floating-point drift
                let rounded = current_beat_fraction.round();
                if (current_beat_fraction - rounded).abs() < 1e-9 {
                    current_beat_fraction = rounded;
                }

                // Handle measure/beat overflow
                let _beats_per_measure = current_time_sig.0 as f64;
                while current_beat_fraction >= 1.0 {
                    current_beat_fraction -= 1.0;
                    current_beat += 1;
                    if current_beat > current_time_sig.0 {
                        current_beat = 1;
                        current_measure += 1;
                    }
                }

                println!(
                    "    Final result: measure {}, beat {}, fraction {:.3}",
                    current_measure, current_beat, current_beat_fraction
                );
                found_target = true;
                break;
            }
        }

        // If we haven't reached the target time yet, add final segment
        if !found_target && last_time < time_seconds {
            let segment_duration = time_seconds - last_time;

            // Calculate effective tempo based on current time signature
            // Convert tempo to quarter note equivalents based on the note value
            // Ratio = denominator / 4 (e.g., 8/4 = 2.0 for eighth notes, 2/4 = 0.5 for half notes)
            let tempo_ratio = current_time_sig.1 as f64 / 4.0;
            let effective_tempo = current_tempo * tempo_ratio;

            let segment_beats = segment_duration * effective_tempo / 60.0;

            println!(
                "  Final segment {:.3}s to {:.3}s: {:.3}s at {:.1} BPM ({}/{} time) = {:.1} effective BPM = {:.3} beats",
                last_time,
                time_seconds,
                segment_duration,
                current_tempo,
                current_time_sig.0,
                current_time_sig.1,
                effective_tempo,
                segment_beats
            );

            // Add final beats to current position
            current_beat_fraction += segment_beats;

            // Snap near-integer fractions to avoid floating-point drift
            let rounded = current_beat_fraction.round();
            if (current_beat_fraction - rounded).abs() < 1e-9 {
                current_beat_fraction = rounded;
            }

            // Handle measure/beat overflow
            let _beats_per_measure = current_time_sig.0 as f64;
            while current_beat_fraction >= 1.0 {
                current_beat_fraction -= 1.0;
                current_beat += 1;
                if current_beat > current_time_sig.0 {
                    current_beat = 1;
                    current_measure += 1;
                }
            }

            println!(
                "    Final result: measure {}, beat {}, fraction {:.3}",
                current_measure, current_beat, current_beat_fraction
            );
        }

        (current_measure, current_beat, current_beat_fraction)
    }

    /// Calculate beat positions with tempo changes (backward compatibility)
    /// Takes a list of tempo points: (time_seconds, tempo_bpm)
    fn calculate_beat_position_with_tempo_changes(
        time_seconds: f64,
        tempo_points: &[(f64, f64)],
        time_signature: (i32, i32),
    ) -> (i32, i32, f64) {
        // Convert tempo points to change points with constant time signature
        let change_points: Vec<(f64, f64, (i32, i32))> = tempo_points
            .iter()
            .map(|&(time, tempo)| (time, tempo, time_signature))
            .collect();

        calculate_beat_position_with_changes(time_seconds, &change_points, time_signature)
    }

    #[test]
    fn test_constant_tempo_calculation() {
        // Test with 100 BPM, 4/4 time signature (like the first tempo envelope point)
        let tempo = 100.0;
        let time_sig = (4, 4);

        // Test cases from the first few markers (time, expected_measure, expected_beat, expected_fraction)
        let test_cases = vec![
            (0.0, (1, 1, 0.0)), // M1: 0.0s → 1.1.00
            (0.6, (1, 2, 0.0)), // M6: 0.6s → 1.2.00
            (1.2, (1, 3, 0.0)), // M2: 1.2s → 1.3.00
            (1.8, (1, 4, 0.0)), // M7: 1.8s → 1.4.00
            (2.4, (2, 1, 0.0)), // M3: 2.4s → 2.1.00 (tempo changes here)
        ];

        for (time, expected) in test_cases {
            let (measure, beat, fraction) =
                calculate_beat_position_constant_tempo(time, tempo, time_sig);
            println!(
                "Time {:.1}s: Measure {}, Beat {}, Fraction {:.3} (expected: {}.{}.{:02})",
                time,
                measure,
                beat,
                fraction,
                expected.0,
                expected.1,
                (expected.2 * 100.0) as i32
            );

            // Allow small floating point differences
            assert!((measure as f64 - expected.0 as f64).abs() < 0.1);
            assert!((beat as f64 - expected.1 as f64).abs() < 0.1);
            assert!(fraction.abs() < 0.1);
        }
    }

    #[test]
    fn test_beat_calculation_math() {
        // Let's verify the math step by step
        let tempo = 100.0; // BPM
        let time_sig = (4, 4); // 4 beats per measure

        // At 0.6 seconds with 100 BPM:
        // beats = 0.6 * 100 / 60 = 1.0 beats
        // measure = floor(1.0 / 4) + 1 = floor(0.25) + 1 = 0 + 1 = 1
        // beat_in_measure = floor(1.0 % 4) + 1 = floor(1.0) + 1 = 1 + 1 = 2
        // So 0.6s should be measure 1, beat 2

        let (measure, beat, fraction) =
            calculate_beat_position_constant_tempo(0.6, tempo, time_sig);
        println!(
            "0.6s: Measure {}, Beat {}, Fraction {:.3}",
            measure, beat, fraction
        );
        assert_eq!(measure, 1);
        assert_eq!(beat, 2);

        // At 1.2 seconds with 100 BPM:
        // beats = 1.2 * 100 / 60 = 2.0 beats
        // measure = floor(2.0 / 4) + 1 = floor(0.5) + 1 = 0 + 1 = 1
        // beat_in_measure = floor(2.0 % 4) + 1 = floor(2.0) + 1 = 2 + 1 = 3
        // So 1.2s should be measure 1, beat 3

        let (measure, beat, fraction) =
            calculate_beat_position_constant_tempo(1.2, tempo, time_sig);
        println!(
            "1.2s: Measure {}, Beat {}, Fraction {:.3}",
            measure, beat, fraction
        );
        assert_eq!(measure, 1);
        assert_eq!(beat, 3);
    }

    #[test]
    fn test_tempo_change_calculation() {
        // Test with the first tempo change: 100 BPM → 60 BPM at 2.4s
        let tempo_points = vec![
            (0.0, 100.0), // Start at 100 BPM
            (2.4, 60.0),  // Change to 60 BPM at 2.4s
        ];
        let time_sig = (4, 4);

        // Test cases around the tempo change
        let test_cases = vec![
            (2.4, (2, 1, 0.0)), // M3: 2.4s → 2.1.00 (exactly at tempo change)
            (3.4, (2, 2, 0.0)), // M8: 3.4s → 2.2.00 (after tempo change)
            (4.4, (2, 3, 0.0)), // M4: 4.4s → 2.3.00 (continuing at 60 BPM)
            (5.4, (2, 4, 0.0)), // M9: 5.4s → 2.4.00 (still at 60 BPM)
        ];

        for (time, expected) in test_cases {
            println!("\nTesting time {:.1}s:", time);
            let (measure, beat, fraction) =
                calculate_beat_position_with_tempo_changes(time, &tempo_points, time_sig);
            println!(
                "Result: Measure {}, Beat {}, Fraction {:.3} (expected: {}.{}.{:02})",
                measure,
                beat,
                fraction,
                expected.0,
                expected.1,
                (expected.2 * 100.0) as i32
            );

            // Allow small floating point differences
            assert!((measure as f64 - expected.0 as f64).abs() < 0.1);
            assert!((beat as f64 - expected.1 as f64).abs() < 0.1);
            assert!(fraction.abs() < 0.1);
        }
    }

    #[test]
    fn test_first_two_measures() {
        // Test just the first two measures (8 beats) with tempo changes: 100 BPM → 60 BPM at 2.4s
        let change_points = vec![
            (0.0, 100.0, (4, 4)), // Start at 100 BPM, 4/4 time
            (2.4, 60.0, (4, 4)),  // Change to 60 BPM at 2.4s, still 4/4
        ];
        let default_time_sig = (4, 4);

        // Test cases for the first two measures
        let test_cases = vec![
            (0.0, (1, 1, 0.0)), // M1: 0.0s → 1.1.00 (start of measure 1)
            (0.6, (1, 2, 0.0)), // M6: 0.6s → 1.2.00 (beat 2 of measure 1)
            (1.2, (1, 3, 0.0)), // M2: 1.2s → 1.3.00 (beat 3 of measure 1)
            (1.8, (1, 4, 0.0)), // M7: 1.8s → 1.4.00 (beat 4 of measure 1)
            (2.4, (2, 1, 0.0)), // M3: 2.4s → 2.1.00 (start of measure 2, tempo change)
            (3.4, (2, 2, 0.0)), // M8: 3.4s → 2.2.00 (beat 2 of measure 2)
            (4.4, (2, 3, 0.0)), // M4: 4.4s → 2.3.00 (beat 3 of measure 2)
            (5.4, (2, 4, 0.0)), // M9: 5.4s → 2.4.00 (beat 4 of measure 2)
            (6.4, (3, 1, 0.0)), // M5: 6.4s → 3.1.00 (start of measure 3)
        ];

        for (time, expected) in test_cases {
            println!("\nTesting time {:.1}s:", time);
            let (measure, beat, fraction) =
                calculate_beat_position_with_changes(time, &change_points, default_time_sig);
            println!(
                "Result: Measure {}, Beat {}, Fraction {:.3} (expected: {}.{}.{:02})",
                measure,
                beat,
                fraction,
                expected.0,
                expected.1,
                (expected.2 * 100.0) as i32
            );

            // Allow small floating point differences
            assert!((measure as f64 - expected.0 as f64).abs() < 0.1);
            assert!((beat as f64 - expected.1 as f64).abs() < 0.1);
            assert!(fraction.abs() < 0.1);
        }
    }

    #[test]
    fn test_time_signature_change_with_effective_tempo() {
        // Test with tempo and time signature changes: 100 BPM → 60 BPM at 2.4s, then 4/4 → 7/8 at 6.4s
        let change_points = vec![
            (0.0, 100.0, (4, 4)), // Start at 100 BPM, 4/4 time
            (2.4, 60.0, (4, 4)),  // Change to 60 BPM at 2.4s, still 4/4
            (6.4, 60.0, (7, 8)), // Change to 7/8 time at 6.4s, still 60 BPM (but effective tempo becomes 30 BPM)
        ];
        let default_time_sig = (4, 4);

        // Test cases around the time signature change
        let test_cases = vec![
            (6.400, (3, 1, 0.0)), // M5: 6.400s → 3.1.00 (exactly at time signature change)
            (6.900, (3, 2, 0.0)), // M10: 6.900s → 3.2.00 (after time signature change, now in 7/8)
        ];

        for (time, expected) in test_cases {
            println!("\nTesting time {:.1}s:", time);
            let (measure, beat, fraction) =
                calculate_beat_position_with_changes(time, &change_points, default_time_sig);
            println!(
                "Result: Measure {}, Beat {}, Fraction {:.3} (expected: {}.{}.{:02})",
                measure,
                beat,
                fraction,
                expected.0,
                expected.1,
                (expected.2 * 100.0) as i32
            );

            // Allow small floating point differences
            assert!((measure as f64 - expected.0 as f64).abs() < 0.1);
            assert!((beat as f64 - expected.1 as f64).abs() < 0.1);
            assert!(fraction.abs() < 0.1);
        }
    }

    #[test]
    fn test_different_time_signatures_effective_tempo() {
        // Test with different time signatures to verify the generalized tempo ratio calculation
        let test_cases = vec![
            // (time_signature, tempo_bpm, expected_effective_tempo_ratio)
            ((4, 4), 120.0, 1.0), // 4/4: 120 BPM = 120 effective BPM (ratio = 4/4 = 1.0)
            ((7, 8), 60.0, 2.0),  // 7/8: 60 BPM = 120 effective BPM (ratio = 8/4 = 2.0)
            ((3, 2), 80.0, 0.5),  // 3/2: 80 BPM = 40 effective BPM (ratio = 2/4 = 0.5)
            ((6, 8), 100.0, 2.0), // 6/8: 100 BPM = 200 effective BPM (ratio = 8/4 = 2.0)
            ((2, 2), 90.0, 0.5),  // 2/2: 90 BPM = 45 effective BPM (ratio = 2/4 = 0.5)
        ];

        for ((num, denom), tempo, expected_ratio) in test_cases {
            let tempo_ratio = denom as f64 / 4.0;
            let effective_tempo = tempo * tempo_ratio;

            println!(
                "Time signature {}/{}: {} BPM * {:.1} ratio = {:.1} effective BPM",
                num, denom, tempo, tempo_ratio, effective_tempo
            );

            assert!(
                (tempo_ratio - expected_ratio).abs() < 0.01,
                "Expected ratio {:.1} for {}/{} time, got {:.1}",
                expected_ratio,
                num,
                denom,
                tempo_ratio
            );
        }
    }

    #[test]
    fn test_comprehensive_beat_positions() {
        // Test against the comprehensive BeatPos Calculator Test data
        // Parse the actual BeatPos Calculator Test.RPP file to get tempo envelope data

        use crate::types::project::ReaperProject;
        use std::path::Path;

        // Parse the BeatPos Calculator Test.RPP file
        let rpp_path = Path::new("resources/BeatPos Calculator Test.RPP");
        let rpp_content = std::fs::read_to_string(rpp_path)
            .expect("Failed to read BeatPos Calculator Test.RPP file");

        // First parse the RPP content into an RppProject
        let rpp_project = crate::parse_rpp_file(&rpp_content).expect("Failed to parse RPP content");

        // Then create a ReaperProject from the RppProject
        let project = ReaperProject::from_rpp_project(&rpp_project)
            .expect("Failed to parse BeatPos Calculator Test.RPP file");

        let envelope = project
            .tempo_envelope
            .expect("BeatPos Calculator Test.RPP should have tempo envelope data");

        // Test all markers - the marker names contain the expected beat positions
        for marker in &project.markers_regions.all {
            if marker.is_marker() && !marker.name.is_empty() {
                // Extract expected beat position from marker name
                // Marker names are in format like "1.1", "2.1", "3.1", etc.
                let expected = &marker.name;

                let actual = time_to_beat_position_with_envelope(marker.position, &envelope);
                println!(
                    "Testing M{}: time={:.3}s, expected={}, actual={}",
                    marker.id, marker.position, expected, actual
                );

                // For now, just print the results to see the pattern
                // We'll add assertions once we understand the expected format
                if actual != *expected {
                    println!("  ❌ MISMATCH: expected {}, got {}", expected, actual);
                } else {
                    println!("  ✅ MATCH");
                }
            }
        }

        // Test a few specific cases to understand the pattern
        println!("\n=== SPECIFIC TEST CASES ===");
        let test_cases = vec![(0.6, "1.2"), (1.2, "1.3"), (1.8, "1.4"), (2.4, "2.1")];

        for (time, expected) in test_cases {
            let actual = time_to_beat_position_with_envelope(time, &envelope);
            println!(
                "Time {:.1}s: expected {}, actual {}",
                time, expected, actual
            );
        }

        // Test all regions
        for region in &project.markers_regions.regions {
            if !region.name.is_empty() {
                let actual_start = time_to_beat_position_with_envelope(region.position, &envelope);
                let actual_end =
                    time_to_beat_position_with_envelope(region.end_position.unwrap(), &envelope);
                println!(
                    "Testing R{}: start={:.3}s->{}, end={:.3}s->{}",
                    region.id,
                    region.position,
                    actual_start,
                    region.end_position.unwrap(),
                    actual_end
                );
            }
        }
    }

    #[test]
    fn test_extend_to_measure_5_1() {
        // Test extending our calculation up to measure 5.1
        // This includes the gradual tempo change that starts at 11.7s
        let change_points = vec![
            (0.0, 100.0, (4, 4)),  // Start: 100 BPM, 4/4 time
            (2.4, 60.0, (4, 4)),   // Tempo change to 60 BPM at 2.4s
            (6.4, 60.0, (7, 8)),   // Time signature change to 7/8 at 6.4s
            (9.9, 100.0, (4, 4)),  // Back to 4/4 time at 9.9s
            (11.7, 100.0, (4, 4)), // Tempo change at 11.7s (start of gradual change)
        ];

        // Test cases extending to measure 5.1
        // At 100 BPM in 4/4, each beat = 0.6s. Measure 4 starts at 9.9s.
        // beat 1: 9.9–10.5, beat 2: 10.5–11.1, beat 3: 11.1–11.7, beat 4: 11.7–12.3
        // Measure 5 starts at 12.3s.
        let test_cases = vec![
            (9.9, "4.1.00"),  // M13: 4.1 at 9.9s
            (10.2, "4.1.50"), // 0.3s into beat 1 = 0.5 fraction
            (10.8, "4.2.50"), // 0.9s = 1.5 beats → beat 2, fraction 0.5
            (11.4, "4.3.50"), // 1.5s = 2.5 beats → beat 3, fraction 0.5
            (11.7, "4.4.00"), // 1.8s = 3.0 beats → beat 4, fraction 0
            (12.3, "5.1.00"), // 2.4s = 4.0 beats → measure 5, beat 1
        ];

        for (time, expected) in test_cases {
            let (measure, beat, beat_fraction) =
                calculate_beat_position_with_changes(time, &change_points, (4, 4));
            let subbeat = (beat_fraction * 100.0).round() as i32;
            let result = format!("{}.{}.{:02}", measure, beat, subbeat);
            println!("Time {}s: expected {}, actual {}", time, expected, result);
            assert_eq!(result, expected, "Failed at time {}s", time);
        }
    }
}
