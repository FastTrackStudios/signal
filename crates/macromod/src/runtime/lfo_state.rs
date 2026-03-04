//! LFO runtime state machine.
//!
//! Manages phase accumulation, tempo sync, retrigger, and S&H random sampling.
//! Uses xorshift64 for PRNG to avoid external dependencies.

use crate::sources::lfo::{LfoConfig, LfoWaveform, RetriggerMode};

use super::waveform::evaluate_waveform;

/// Runtime state for a single LFO instance.
#[derive(Debug, Clone)]
pub struct LfoState {
    /// Current phase position [0.0, 1.0).
    phase: f64,
    /// Held value for S&H / StepSequence waveforms (bipolar [-1, 1]).
    held_value: f64,
    /// xorshift64 PRNG state.
    rng_state: u64,
}

impl LfoState {
    /// Create a new LFO state with the given initial phase offset (degrees).
    pub fn new(phase_offset_degrees: f32) -> Self {
        Self {
            phase: (phase_offset_degrees as f64 / 360.0).fract().abs(),
            held_value: 0.0,
            rng_state: 0x5A5A_5A5A_5A5A_5A5A,
        }
    }

    /// Create from an LFO config, using its phase_offset.
    pub fn from_config(config: &LfoConfig) -> Self {
        Self::new(config.phase_offset)
    }

    /// Current phase position.
    pub fn phase(&self) -> f64 {
        self.phase
    }

    /// Current held value (for S&H / StepSequence).
    pub fn held_value(&self) -> f64 {
        self.held_value
    }

    /// Advance the LFO by `dt` seconds and return the current output value (bipolar [-1, 1]).
    ///
    /// The output is the raw waveform value **before** depth/amount scaling — the caller
    /// (processor) applies `depth * amount` to get the final modulation offset.
    pub fn tick(&mut self, dt_seconds: f64, config: &LfoConfig, bpm: f64) -> f64 {
        let freq = if config.tempo_sync {
            let division = config.sync_division.unwrap_or_default();
            // One cycle spans `division.beats()` beats.
            // BPM → beats per second = bpm / 60.
            // Frequency = beats_per_second / beats_per_cycle.
            let beats_per_cycle = division.beats() as f64;
            (bpm / 60.0) / beats_per_cycle
        } else {
            config.rate_hz as f64
        };

        let phase_delta = freq * dt_seconds;
        let old_phase = self.phase;
        self.phase += phase_delta;

        // Detect cycle boundary crossing for S&H
        let crossed_boundary = self.phase >= 1.0;
        self.phase = self.phase.fract();
        if self.phase < 0.0 {
            self.phase += 1.0;
        }

        // For S&H waveforms, sample a new random value at each cycle boundary
        if crossed_boundary
            && matches!(
                config.waveform,
                LfoWaveform::SampleAndHold | LfoWaveform::StepSequence
            )
        {
            self.held_value = self.next_random_bipolar();
        }

        // StepSequence: also quantize phase into 8 steps, re-sample when step changes
        if config.waveform == LfoWaveform::StepSequence {
            let old_step = (old_phase * 8.0) as u32;
            let new_step = (self.phase * 8.0) as u32;
            if new_step != old_step && !crossed_boundary {
                // Step changed within cycle (boundary already handled above)
                self.held_value = self.next_random_bipolar();
            }
        }

        evaluate_waveform(
            config.waveform,
            self.phase,
            config.pulse_width as f64,
            self.held_value,
        )
    }

    /// Reset phase to the configured offset. Called on note-on when retrigger is NoteOn.
    pub fn retrigger(&mut self, config: &LfoConfig) {
        if config.retrigger == RetriggerMode::NoteOn {
            self.phase = (config.phase_offset as f64 / 360.0).fract().abs();
        }
    }

    /// Force-reset phase to zero (or offset). Useful for scene changes.
    pub fn reset(&mut self, phase_offset_degrees: f32) {
        self.phase = (phase_offset_degrees as f64 / 360.0).fract().abs();
        self.held_value = 0.0;
    }

    /// xorshift64 PRNG — returns next random u64.
    fn next_random_u64(&mut self) -> u64 {
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng_state = x;
        x
    }

    /// Returns a random bipolar value in [-1.0, 1.0].
    fn next_random_bipolar(&mut self) -> f64 {
        let raw = self.next_random_u64();
        // Map u64 to [0, 1] then to [-1, 1]
        let normalized = (raw as f64) / (u64::MAX as f64);
        normalized * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::lfo::{LfoWaveform, TempoDiv};

    fn sine_config(rate_hz: f32) -> LfoConfig {
        LfoConfig {
            waveform: LfoWaveform::Sine,
            rate_hz,
            depth: 1.0,
            ..Default::default()
        }
    }

    fn tempo_sync_config(division: TempoDiv) -> LfoConfig {
        LfoConfig {
            waveform: LfoWaveform::Sine,
            tempo_sync: true,
            sync_division: Some(division),
            depth: 1.0,
            ..Default::default()
        }
    }

    #[test]
    fn phase_advances_at_correct_rate() {
        let config = sine_config(1.0); // 1 Hz
        let mut state = LfoState::from_config(&config);

        // After 0.25 seconds at 1 Hz, phase should be 0.25
        state.tick(0.25, &config, 120.0);
        assert!((state.phase() - 0.25).abs() < 1e-10);
    }

    #[test]
    fn phase_wraps_at_one() {
        let config = sine_config(1.0);
        let mut state = LfoState::from_config(&config);

        // After 1.5 seconds at 1 Hz, phase should be 0.5
        state.tick(1.5, &config, 120.0);
        assert!((state.phase() - 0.5).abs() < 1e-10);
    }

    #[test]
    fn tempo_sync_quarter_note() {
        // At 120 BPM, a quarter note = 1 beat = 0.5 seconds.
        // So frequency = (120/60) / 1.0 = 2 Hz.
        // After 0.25 seconds, phase = 2 * 0.25 = 0.5
        let config = tempo_sync_config(TempoDiv::Quarter);
        let mut state = LfoState::from_config(&config);

        state.tick(0.25, &config, 120.0);
        assert!((state.phase() - 0.5).abs() < 1e-10);
    }

    #[test]
    fn tempo_sync_whole_note() {
        // At 120 BPM, a whole note = 4 beats = 2 seconds.
        // Frequency = (120/60) / 4.0 = 0.5 Hz.
        // After 1.0 seconds, phase = 0.5
        let config = tempo_sync_config(TempoDiv::Whole);
        let mut state = LfoState::from_config(&config);

        state.tick(1.0, &config, 120.0);
        assert!((state.phase() - 0.5).abs() < 1e-10);
    }

    #[test]
    fn sine_output_at_quarter_phase() {
        let config = sine_config(1.0);
        let mut state = LfoState::from_config(&config);

        // Advance to phase 0.25 → sin(π/2) = 1.0
        let val = state.tick(0.25, &config, 120.0);
        assert!((val - 1.0).abs() < 1e-10);
    }

    #[test]
    fn retrigger_resets_phase() {
        let config = LfoConfig {
            retrigger: RetriggerMode::NoteOn,
            phase_offset: 90.0,
            ..sine_config(1.0)
        };
        let mut state = LfoState::from_config(&config);

        // Advance phase
        state.tick(0.3, &config, 120.0);
        assert!(state.phase() > 0.25);

        // Retrigger should reset to phase_offset (90° = 0.25)
        state.retrigger(&config);
        assert!((state.phase() - 0.25).abs() < 1e-10);
    }

    #[test]
    fn free_retrigger_does_nothing() {
        let config = sine_config(1.0); // retrigger = Free by default
        let mut state = LfoState::from_config(&config);

        state.tick(0.3, &config, 120.0);
        let phase_before = state.phase();
        state.retrigger(&config);
        assert_eq!(state.phase(), phase_before);
    }

    #[test]
    fn sample_and_hold_changes_on_cycle_boundary() {
        let config = LfoConfig {
            waveform: LfoWaveform::SampleAndHold,
            rate_hz: 1.0,
            depth: 1.0,
            ..Default::default()
        };
        let mut state = LfoState::from_config(&config);

        // First tick — no boundary crossed yet, held_value is initial 0.0
        let v1 = state.tick(0.5, &config, 120.0);
        assert_eq!(v1, 0.0); // initial held value

        // Cross the boundary at phase=1.0
        let v2 = state.tick(0.6, &config, 120.0);
        // Should have sampled a new random value
        assert_ne!(v2, 0.0); // extremely unlikely to be exactly 0

        let held_after = state.held_value();
        assert_eq!(v2, held_after);
    }

    #[test]
    fn step_sequence_changes_on_step_boundary() {
        let config = LfoConfig {
            waveform: LfoWaveform::StepSequence,
            rate_hz: 1.0,
            depth: 1.0,
            ..Default::default()
        };
        let mut state = LfoState::from_config(&config);

        // Collect values at each step boundary (8 steps per cycle)
        let mut values = Vec::new();
        for _ in 0..8 {
            let v = state.tick(0.125, &config, 120.0);
            values.push(v);
        }

        // Not all values should be the same (statistically near-impossible)
        let all_same = values.iter().all(|v| *v == values[0]);
        assert!(!all_same, "Step sequence should produce varying values");
    }

    #[test]
    fn phase_offset_initial() {
        let state = LfoState::new(180.0);
        assert!((state.phase() - 0.5).abs() < 1e-10);
    }

    #[test]
    fn reset_restores_phase() {
        let mut state = LfoState::new(0.0);
        let config = sine_config(1.0);
        state.tick(0.3, &config, 120.0);

        state.reset(90.0);
        assert!((state.phase() - 0.25).abs() < 1e-10);
        assert_eq!(state.held_value(), 0.0);
    }

    #[test]
    fn xorshift_produces_varying_values() {
        let mut state = LfoState::new(0.0);
        let mut values = Vec::new();
        for _ in 0..10 {
            values.push(state.next_random_bipolar());
        }
        // All values should be in [-1, 1]
        for v in &values {
            assert!((-1.0..=1.0).contains(v), "random value {v} out of range");
        }
        // At least some should differ
        let unique: std::collections::HashSet<u64> =
            values.iter().map(|v| v.to_bits()).collect();
        assert!(unique.len() > 1, "PRNG produced identical values");
    }
}
