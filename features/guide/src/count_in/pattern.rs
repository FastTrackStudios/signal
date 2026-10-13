//! Count-in pattern calculation logic
//!
//! Determines when and what to count based on measure/beat position and time signature.

use tracing::debug;

/// Calculate count number for odd time signatures split into groups ending with 4
///
/// For 9/8: splits into [5 beats] + [4 beats]
/// Returns: `beat_in_group` or None if beat is in a non-counted group
///
/// Only applies to:
/// - Odd time signatures with /16 denominator, OR
/// - Time signatures with numerator > 6
pub fn calculate_odd_time_count(
    beat_in_measure: i32,
    time_sig_num: i32,
    time_sig_den: i32,
    full_count: bool,
) -> Option<i32> {
    // Check if this time signature should use splitting logic
    let is_odd_with_16 = time_sig_num % 2 == 1 && time_sig_den == 16;
    let is_numerator_gt_6 = time_sig_num > 6;
    let should_split = is_odd_with_16 || is_numerator_gt_6;

    // Debug logging for 6/8 detection
    if time_sig_num == 6 && time_sig_den == 8 {
        debug!(
            beat_in_measure = beat_in_measure,
            time_sig_num = time_sig_num,
            time_sig_den = time_sig_den,
            is_odd_with_16 = is_odd_with_16,
            is_numerator_gt_6 = is_numerator_gt_6,
            should_split = should_split,
            "calculate_odd_time_count called for 6/8"
        );
    }

    if !should_split {
        // For 6/8 and other regular time signatures, return None to use normal counting
        if time_sig_num == 6 && time_sig_den == 8 {
            debug!(
                beat_in_measure = beat_in_measure,
                "calculate_odd_time_count returning None for 6/8 (should use normal counting)"
            );
        }
        return None; // Not eligible for splitting, handle normally
    }

    // Last group is always 4 beats
    let last_group_start = time_sig_num.saturating_sub(3); // For 9/8: beat 6

    if beat_in_measure >= last_group_start && beat_in_measure <= time_sig_num {
        // This beat is in the last group (4 beats)
        // Count 1-4 within this group
        let beat_in_group = beat_in_measure
            .saturating_sub(last_group_start)
            .saturating_add(1);
        return Some(beat_in_group);
    }

    // This beat is in an earlier group
    if !full_count {
        // If full_count is false, skip counting earlier groups
        return None;
    }

    // Calculate which group and beat within that group
    let first_group_end = last_group_start.saturating_sub(1);
    let mut current_start = 1;

    while current_start <= first_group_end {
        // Calculate group size (up to 8 beats per group, but may be smaller)
        let remaining_before_last = last_group_start.saturating_sub(current_start);
        let group_size = if remaining_before_last > 8 {
            8 // Maximum group size is 8 (max count number)
        } else {
            remaining_before_last
        };
        let current_end = current_start.saturating_add(group_size).saturating_sub(1);

        if beat_in_measure >= current_start && beat_in_measure <= current_end {
            // This beat is in the current group
            let beat_in_group = beat_in_measure
                .saturating_sub(current_start)
                .saturating_add(1);
            return Some(beat_in_group);
        }

        current_start = current_end.saturating_add(1);
    }

    None
}

/// Count-in pattern calculator
pub struct CountInPattern;

impl CountInPattern {
    /// Determine if we should count on a specific beat in an extended count-in pattern
    ///
    /// Pattern for N measures (N > 1):
    /// - Measures 1 to (N-2): Count only on beat 1 (numbered 1, 2, 3, ...)
    /// - Measure (N-1): Count on beats 1 and 3 (half measure: "1 _ 2 _")
    /// - Measure N: Full count (all beats: "1 2 3 4")
    ///
    /// Returns:
    /// - `Some(count_number)` if we should count on this beat (1-indexed count number)
    /// - None if we should not count on this beat
    #[must_use]
    pub fn should_count(
        measure_index: i32,
        beat_in_measure: i32,
        total_measures: i32,
        time_sig_num: i32,
        time_sig_den: i32,
        offset_by_one: bool,
        full_count_odd_time: bool,
    ) -> Option<i32> {
        // Special handling for time signatures with denominator 16
        if time_sig_den == 16 {
            // For /16 time signatures with 1-measure count-ins: NO counts, just guide voice
            if total_measures <= 1 {
                return None;
            }

            // For multi-measure count-ins: count once per measure at beat 1
            if beat_in_measure == 1 {
                let count_number = if offset_by_one {
                    total_measures
                        .saturating_sub(measure_index)
                        .saturating_sub(1)
                } else {
                    total_measures.saturating_sub(measure_index)
                };
                if (1..=8).contains(&count_number) {
                    return Some(count_number);
                }
            }
            return None;
        }

        // Standard time signatures (4/4, 6/8, 7/8, etc.)
        if total_measures <= 1 {
            // Check if this is an odd time signature that needs splitting
            if let Some(beat_in_group) = calculate_odd_time_count(
                beat_in_measure,
                time_sig_num,
                time_sig_den,
                full_count_odd_time,
            ) {
                return Some(beat_in_group);
            }

            // Standard 1-measure count: count all beats
            if beat_in_measure >= 1 && beat_in_measure <= time_sig_num {
                return Some(beat_in_measure);
            }
            return None;
        }

        // Extended pattern
        let measure_1_indexed = measure_index.saturating_add(1);

        if total_measures == 2 {
            // Special case: 2 measures
            if measure_1_indexed == 1 {
                // First measure: half measure pattern (beats 1 and 3 only)
                if beat_in_measure == 1 {
                    return Some(1);
                } else if beat_in_measure == 3 {
                    return Some(2);
                }
                return None;
            }
            // Second measure (last measure): full count
            if let Some(beat_in_group) = calculate_odd_time_count(
                beat_in_measure,
                time_sig_num,
                time_sig_den,
                full_count_odd_time,
            ) {
                return Some(beat_in_group);
            }

            // Standard full count
            if beat_in_measure >= 1 && beat_in_measure <= time_sig_num {
                return Some(beat_in_measure);
            }
            return None;
        } else if measure_1_indexed <= total_measures.saturating_sub(2) {
            // Measures 1 to (N-2): Count only on beat 1
            if beat_in_measure == 1 {
                return Some(measure_1_indexed);
            }
            return None;
        } else if measure_1_indexed == total_measures.saturating_sub(1) {
            // Measure (N-1): Count on beats 1 and 3
            if beat_in_measure == 1 {
                return Some(1);
            } else if beat_in_measure == 3 {
                return Some(2);
            }
            return None;
        }
        // Measure N (last measure): Full count
        if let Some(beat_in_group) = calculate_odd_time_count(
            beat_in_measure,
            time_sig_num,
            time_sig_den,
            full_count_odd_time,
        ) {
            return Some(beat_in_group);
        }

        // Standard full count
        if beat_in_measure >= 1 && beat_in_measure <= time_sig_num {
            return Some(beat_in_measure);
        }
        None
    }
}
