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
    /// Held value for S&H / `StepSequence` waveforms (bipolar [-1, 1]).
    held_value: f64,
    /// xorshift64 PRNG state.
    rng_state: u64,
}

impl LfoState {
    /// Create a new LFO state with the given initial phase offset (degrees).
    #[must_use]
    pub fn new(phase_offset_degrees: f32) -> Self {
        Self {
            phase: (f64::from(phase_offset_degrees) / 360.0).fract().abs(),
            held_value: 0.0,
            rng_state: 0x5A5A_5A5A_5A5A_5A5A,
        }
    }

    /// Create from an LFO config, using its `phase_offset`.
    #[must_use]
    pub fn from_config(config: &LfoConfig) -> Self {
        Self::new(config.phase_offset)
    }

    /// Current phase position.
    #[must_use]
    pub const fn phase(&self) -> f64 {
        self.phase
    }

    /// Current held value (for S&H / `StepSequence`).
    #[must_use]
    pub const fn held_value(&self) -> f64 {
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
            let beats_per_cycle = f64::from(division.beats());
            (bpm / 60.0) / beats_per_cycle
        } else {
            f64::from(config.rate_hz)
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
            f64::from(config.pulse_width),
            self.held_value,
        )
    }

    /// Reset phase to the configured offset. Called on note-on when retrigger is `NoteOn`.
    pub fn retrigger(&mut self, config: &LfoConfig) {
        if config.retrigger == RetriggerMode::NoteOn {
            self.phase = (f64::from(config.phase_offset) / 360.0).fract().abs();
        }
    }

    /// Force-reset phase to zero (or offset). Useful for scene changes.
    pub fn reset(&mut self, phase_offset_degrees: f32) {
        self.phase = (f64::from(phase_offset_degrees) / 360.0).fract().abs();
        self.held_value = 0.0;
    }

    /// xorshift64 PRNG — returns next random u64.
    const fn next_random_u64(&mut self) -> u64 {
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
        normalized.mul_add(2.0, -1.0)
    }
}
