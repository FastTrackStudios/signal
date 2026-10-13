//! Subdivision trigger scheduling
//!
//! Calculates and schedules trigger positions for beat, eighth, sixteenth,
//! and triplet subdivisions within the audio buffer.
//!
//! Ported verbatim from fts-guide `audio/trigger_scheduler.rs` (only the
//! tracing target changed).

use std::collections::HashMap;
use tracing::debug;

/// Parameters needed for trigger scheduling
pub struct TriggerSchedulingParams {
    /// Current beat position (in beat units, relative to measure start)
    pub current_beat_position: f64,

    /// Current beat position in quarter notes (absolute)
    pub current_beat_position_quarter_notes: f64,

    /// Current bar start position in quarter notes
    pub current_bar_start_quarters: f64,

    /// Beats per quarter note (based on time signature denominator)
    pub beats_per_quarter: f64,

    /// Samples per beat (in the time signature's beat unit)
    pub samples_per_beat: f64,

    /// Samples per quarter note
    pub samples_per_quarter: f64,

    /// Current integer beat (floor of `current_beat_position`)
    pub current_beat_integer: i64,

    /// Buffer length in samples
    pub buffer_len: usize,

    /// Time signature numerator
    pub time_sig_num: i32,

    /// Time signature denominator
    pub time_sig_den: i32,

    /// Sample rate
    pub sample_rate: f32,

    /// Previous beat position (for calculating sample positions)
    pub previous_beat_position: f64,
}

/// Subdivision interval definitions
pub struct SubdivisionIntervals {
    /// Eighth note interval in quarter notes (always 0.5)
    pub eighth_interval: f64,

    /// Sixteenth note interval in quarter notes (always 0.25)
    pub sixteenth_interval: f64,

    /// Triplet interval in quarter notes (`beat_unit` / 3.0)
    pub triplet_interval: f64,
}

/// Trigger result: (`sample_offset`, priority)
/// Priority: 0 = Beat (highest), 1 = Eighth, 2 = Sixteenth, 3 = Triplet (lowest)
pub type TriggerResult = (usize, usize);

/// Last triggered positions for subdivisions
#[derive(Copy, Clone, Debug)]
pub struct LastTriggeredPositions {
    /// Last triggered eighth position in quarter notes
    pub eighth: f64,
    /// Last triggered sixteenth position in quarter notes
    pub sixteenth: f64,
    /// Last triggered triplet position in quarter notes
    pub triplet: f64,
}

/// Configuration for which subdivisions to trigger
#[derive(Copy, Clone, Debug)]
pub struct SubdivisionEnables {
    flags: u8,
}

impl SubdivisionEnables {
    /// Create a new subdivision enables configuration from
    /// `[beat, eighth, sixteenth, triplet]`.
    #[must_use]
    pub const fn new(flags: [bool; 4]) -> Self {
        let [beat, eighth, sixteenth, triplet] = flags;
        let mut bits = 0u8;
        if beat {
            bits |= 0x01;
        }
        if eighth {
            bits |= 0x02;
        }
        if sixteenth {
            bits |= 0x04;
        }
        if triplet {
            bits |= 0x08;
        }
        Self { flags: bits }
    }

    /// Check if beat triggers are enabled
    #[must_use]
    pub const fn beat(self) -> bool {
        self.flags & 0x01 != 0
    }

    /// Check if eighth note triggers are enabled
    #[must_use]
    pub const fn eighth(self) -> bool {
        self.flags & 0x02 != 0
    }

    /// Check if sixteenth note triggers are enabled
    #[must_use]
    pub const fn sixteenth(self) -> bool {
        self.flags & 0x04 != 0
    }

    /// Check if triplet triggers are enabled
    #[must_use]
    pub const fn triplet(self) -> bool {
        self.flags & 0x08 != 0
    }
}

/// Subdivision trigger scheduler
pub struct TriggerScheduler;

impl TriggerScheduler {
    /// Calculate subdivision intervals based on time signature
    #[must_use]
    pub fn calculate_intervals(time_sig_den: i32) -> SubdivisionIntervals {
        let eighth_interval = 0.5; // Always 0.5 quarter notes
        let sixteenth_interval = 0.25; // Always 0.25 quarter notes

        // Triplet subdivides the current beat unit into 3 parts
        let beat_unit_in_quarters = 4.0 / f64::from(time_sig_den);
        let triplet_interval = beat_unit_in_quarters / 3.0;

        SubdivisionIntervals {
            eighth_interval,
            sixteenth_interval,
            triplet_interval,
        }
    }

    /// Calculate the current beat start position in quarter notes
    #[must_use]
    pub fn calculate_current_beat_start(params: &TriggerSchedulingParams) -> f64 {
        let bar_start = params.current_bar_start_quarters;
        if bar_start.is_finite() {
            // Position in measure (quarter notes)
            let pos_in_measure_quarter = params.current_beat_position_quarter_notes - bar_start;
            // Convert to beats, then find which beat we're in, then convert back to quarters
            let pos_in_measure_beats = pos_in_measure_quarter * params.beats_per_quarter;
            let beat_number_in_measure = pos_in_measure_beats.floor();
            // Position of start of current beat in quarter notes (within measure)
            let beat_start_in_measure_quarter = beat_number_in_measure / params.beats_per_quarter;
            bar_start + beat_start_in_measure_quarter
        } else {
            // Fallback: calculate from beat position
            (params.current_beat_position / params.beats_per_quarter).floor()
                * params.beats_per_quarter
                / params.beats_per_quarter
        }
    }

    /// Calculate the next beat start position (boundary for clamping subdivisions)
    #[must_use]
    pub fn calculate_next_beat_start(
        params: &TriggerSchedulingParams,
        current_beat_start_quarter: f64,
    ) -> f64 {
        let bar_start = params.current_bar_start_quarters;
        if bar_start.is_finite() {
            let pos_in_measure_quarter = params.current_beat_position_quarter_notes - bar_start;
            let pos_in_measure_beats = pos_in_measure_quarter * params.beats_per_quarter;
            let next_beat_in_measure = pos_in_measure_beats.floor() + 1.0;
            let next_beat_start_in_measure_quarter =
                next_beat_in_measure / params.beats_per_quarter;
            bar_start + next_beat_start_in_measure_quarter
        } else {
            current_beat_start_quarter + (1.0 / params.beats_per_quarter)
        }
    }

    /// Calculate trigger sample offset within buffer
    /// Returns the sample offset where the target position occurs, or None if outside buffer
    #[must_use]
    pub fn calculate_trigger_sample(
        current_pos: f64,
        target_pos: f64,
        samples_per_beat: f64,
        buffer_len: usize,
    ) -> Option<usize> {
        let beats_until = target_pos - current_pos;
        let val = (beats_until * samples_per_beat).round();
        let samples_until = crate::cast::i64_from_f64_round(val);

        // If we've already passed the target, trigger at sample 0 (as early as possible)
        if samples_until <= 0 {
            Some(0)
        } else {
            match usize::try_from(samples_until) {
                Ok(size) if size < buffer_len => Some(size),
                _ => None,
            }
        }
    }

    /// Schedule eighth note triggers
    fn schedule_eighth_triggers(
        params: &TriggerSchedulingParams,
        intervals: &SubdivisionIntervals,
        next_eighth_quarter_clamped: f64,
        triggers: &mut Vec<TriggerResult>,
    ) {
        let quarters_until_eighth =
            next_eighth_quarter_clamped - params.current_beat_position_quarter_notes;
        let val = (quarters_until_eighth * params.samples_per_quarter).round();
        let samples_until_eighth = crate::cast::i64_from_f64_round(val);

        if samples_until_eighth >= 0 {
            if let Ok(sample) = usize::try_from(samples_until_eighth) {
                if sample < params.buffer_len {
                    debug!(
                        target: "session_guide::audio",
                        "Scheduling eighth at sample {} | time_sig={}/{}",
                        sample,
                        params.time_sig_num,
                        params.time_sig_den
                    );
                    triggers.push((sample, 1)); // Priority 1 = Eighth
                }
            }
        }
    }

    /// Schedule sixteenth note triggers
    fn schedule_sixteenth_triggers(
        params: &TriggerSchedulingParams,
        intervals: &SubdivisionIntervals,
        next_sixteenth_quarter_clamped: f64,
        triggers: &mut Vec<TriggerResult>,
    ) {
        let quarters_until_sixteenth =
            next_sixteenth_quarter_clamped - params.current_beat_position_quarter_notes;
        let val = (quarters_until_sixteenth * params.samples_per_quarter).round();
        let samples_until_sixteenth = crate::cast::i64_from_f64_round(val);

        if samples_until_sixteenth >= 0 {
            if let Ok(sample) = usize::try_from(samples_until_sixteenth) {
                if sample < params.buffer_len {
                    debug!(
                        target: "session_guide::audio",
                        "Scheduling sixteenth at sample {} | time_sig={}/{}",
                        sample,
                        params.time_sig_num,
                        params.time_sig_den
                    );
                    triggers.push((sample, 2)); // Priority 2 = Sixteenth
                }
            }
        }
    }

    /// Schedule triplet triggers
    fn schedule_triplet_triggers(
        params: &TriggerSchedulingParams,
        next_triplet_quarter: f64,
        triggers: &mut Vec<TriggerResult>,
    ) {
        let quarters_until_triplet =
            next_triplet_quarter - params.current_beat_position_quarter_notes;
        let val = (quarters_until_triplet * params.samples_per_quarter).round();
        let samples_until_triplet = crate::cast::i64_from_f64_round(val);

        // Handle negative values (target already passed) - trigger at sample 0
        let buffer_len_i64 = i64::try_from(params.buffer_len).unwrap_or(i64::MAX);
        if samples_until_triplet < buffer_len_i64
            && samples_until_triplet >= buffer_len_i64.saturating_neg()
        {
            let trigger_sample = if samples_until_triplet <= 0 {
                0 // Target already passed, trigger as early as possible
            } else {
                usize::try_from(samples_until_triplet).unwrap_or(0)
            };
            triggers.push((trigger_sample, 3)); // Priority 3 = Triplet (lowest)
        }
    }

    /// Schedule all subdivision triggers for the current buffer
    ///
    /// Returns a tuple of (triggers, `updated_last_eighth`, `updated_last_sixteenth`, `updated_last_triplet`)
    pub fn schedule_subdivision_triggers(
        params: &TriggerSchedulingParams,
        intervals: &SubdivisionIntervals,
        last_triggered: LastTriggeredPositions,
        next_beat_to_trigger: f64,
        enables: SubdivisionEnables,
    ) -> (Vec<TriggerResult>, f64, f64, f64) {
        let current_beat_start_quarter = Self::calculate_current_beat_start(params);
        let next_beat_start_quarter =
            Self::calculate_next_beat_start(params, current_beat_start_quarter);

        // Initialize last_triggered positions if needed
        let mut last_eighth = last_triggered.eighth;
        let mut last_sixteenth = last_triggered.sixteenth;
        let mut last_triplet = last_triggered.triplet;

        // Position within current beat (in quarter notes)
        let position_within_beat_quarter =
            params.current_beat_position_quarter_notes - current_beat_start_quarter;

        // Initialize eighth and sixteenth if needed
        if last_eighth < 0.0 {
            let eighth_within_beat =
                (position_within_beat_quarter / intervals.eighth_interval).floor();
            last_eighth = eighth_within_beat
                .mul_add(intervals.eighth_interval, current_beat_start_quarter)
                - intervals.eighth_interval;
        }
        if last_sixteenth < 0.0 {
            let sixteenth_within_beat =
                (position_within_beat_quarter / intervals.sixteenth_interval).floor();
            last_sixteenth = sixteenth_within_beat
                .mul_add(intervals.sixteenth_interval, current_beat_start_quarter)
                - intervals.sixteenth_interval;
        }

        // Initialize triplet if needed
        if last_triplet < 0.0 {
            let triplet_within_beat =
                (position_within_beat_quarter / intervals.triplet_interval).floor();
            last_triplet = triplet_within_beat
                .mul_add(intervals.triplet_interval, current_beat_start_quarter)
                - intervals.triplet_interval;
        } else if last_triplet < current_beat_start_quarter {
            // Reset if we're in a new beat
            let triplet_within_beat =
                (position_within_beat_quarter / intervals.triplet_interval).floor();
            last_triplet = triplet_within_beat
                .mul_add(intervals.triplet_interval, current_beat_start_quarter)
                - intervals.triplet_interval;
        }

        // Reset last_triggered if we're in a new beat
        if last_eighth < current_beat_start_quarter {
            let position_within_beat =
                params.current_beat_position_quarter_notes - current_beat_start_quarter;
            let eighth_within_beat = (position_within_beat / intervals.eighth_interval).floor();
            last_eighth = eighth_within_beat
                .mul_add(intervals.eighth_interval, current_beat_start_quarter)
                - intervals.eighth_interval;
        }

        if last_sixteenth < current_beat_start_quarter {
            let position_within_beat =
                params.current_beat_position_quarter_notes - current_beat_start_quarter;
            let sixteenth_within_beat =
                (position_within_beat / intervals.sixteenth_interval).floor();
            last_sixteenth = sixteenth_within_beat
                .mul_add(intervals.sixteenth_interval, current_beat_start_quarter)
                - intervals.sixteenth_interval;
        }

        // Calculate next trigger positions
        let next_beat = next_beat_to_trigger;

        // Triplet: clamp to not exceed next beat boundary
        let next_triplet_quarter_raw = (last_triplet / intervals.triplet_interval)
            .floor()
            .mul_add(intervals.triplet_interval, intervals.triplet_interval);
        let next_triplet_quarter = next_triplet_quarter_raw.min(next_beat_start_quarter);

        // Eighth and sixteenth: calculate relative to current beat start, clamp to beat boundary
        let next_eighth_quarter_raw = (last_eighth / intervals.eighth_interval)
            .floor()
            .mul_add(intervals.eighth_interval, intervals.eighth_interval);
        let next_sixteenth_quarter_raw = (last_sixteenth / intervals.sixteenth_interval)
            .floor()
            .mul_add(intervals.sixteenth_interval, intervals.sixteenth_interval);

        let next_eighth_quarter_clamped = next_eighth_quarter_raw.min(next_beat_start_quarter);
        let next_sixteenth_quarter_clamped =
            next_sixteenth_quarter_raw.min(next_beat_start_quarter);

        let mut triggers = Vec::new();

        // Schedule beat trigger
        let next_beat_int = if next_beat_to_trigger.is_finite() {
            crate::cast::i64_from_f64_round(next_beat_to_trigger)
        } else {
            i64::MAX
        };
        if enables.beat() && params.current_beat_integer >= next_beat_int {
            if let Some(sample) = Self::calculate_trigger_sample(
                params.current_beat_position,
                next_beat,
                params.samples_per_beat,
                params.buffer_len,
            ) {
                debug!(
                    target: "session_guide::audio",
                    "Scheduling beat at sample {} | current_beat={:.3} | next_beat={:.3} | time_sig={}/{}",
                    sample, params.current_beat_position, next_beat, params.time_sig_num, params.time_sig_den
                );
                triggers.push((sample, 0)); // Priority 0 = Beat (highest)
            }
        }

        // Schedule eighth note trigger
        if enables.eighth() {
            Self::schedule_eighth_triggers(
                params,
                intervals,
                next_eighth_quarter_clamped,
                &mut triggers,
            );
        }

        // Schedule sixteenth note trigger
        if enables.sixteenth() {
            Self::schedule_sixteenth_triggers(
                params,
                intervals,
                next_sixteenth_quarter_clamped,
                &mut triggers,
            );
        }

        // Schedule triplet trigger
        if enables.triplet() {
            Self::schedule_triplet_triggers(params, next_triplet_quarter, &mut triggers);
        }

        (triggers, last_eighth, last_sixteenth, last_triplet)
    }

    /// Apply priority mode filtering to triggers
    ///
    /// In priority mode, if multiple triggers occur at the same sample, only keep the highest priority one.
    #[must_use]
    pub fn apply_priority_mode(triggers: Vec<TriggerResult>) -> Vec<TriggerResult> {
        let mut sample_map: HashMap<usize, usize> = HashMap::new();
        for (sample, priority) in triggers {
            let entry = sample_map.entry(sample).or_insert(priority);
            *entry = (*entry).min(priority);
        }
        sample_map.into_iter().collect()
    }
}
