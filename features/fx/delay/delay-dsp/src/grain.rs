//! Dual-head grain reader for pitch-shifted delay reads.
//!
//! Shared by `PitchDelay` and `ShimmerDelay`: two read heads drift through
//! the delay buffer at `1 - speed` samples per sample, centered on the
//! target delay time, crossfading when a head strays more than a grain
//! from the target. Unlike `audiocore_dsp::grain_pitch::GrainPitchShifter`
//! (which owns its buffer and shifts "now"), this reads from an external
//! delay line at an arbitrary delay offset.

use audiocore_dsp::delay_line::DelayLine;

pub(crate) struct GrainReader {
    offset_a: f64,
    offset_b: f64,
    /// Crossfade position (0 = head B primary, 1 = head A primary).
    crossfade: f64,
    grain_phase: bool,
}

impl GrainReader {
    pub fn new() -> Self {
        Self {
            offset_a: 0.0,
            offset_b: 0.0,
            crossfade: 1.0,
            grain_phase: true,
        }
    }

    /// Seat both heads at the target delay (used at init / after reset).
    pub fn seat(&mut self, target_delay: f64) {
        self.offset_a = target_delay;
        self.offset_b = target_delay;
    }

    /// Advance one sample and read the pitch-shifted signal from `delay`.
    ///
    /// `speed` is the pitch ratio (2.0 = octave up), `target_delay` the
    /// smoothed delay in samples, `grain_samples` the crossfade window.
    #[inline]
    pub fn tick(
        &mut self,
        delay: &DelayLine,
        target_delay: f64,
        grain_samples: f64,
        speed: f64,
    ) -> f64 {
        let max_read = delay.len() as f64 - 4.0;

        // Both read heads drift at `speed` rate:
        // speed=1.0 → offset constant (normal delay);
        // speed=2.0 → offset shrinks (reading faster = pitch up);
        // speed=0.5 → offset grows (reading slower = pitch down).
        self.offset_a += 1.0 - speed;
        self.offset_b += 1.0 - speed;

        // Re-seat a head that left the valid window entirely.
        for o in [&mut self.offset_a, &mut self.offset_b] {
            if *o < 1.0 || *o > max_read || (*o - target_delay).abs() > grain_samples {
                *o = target_delay;
            }
        }

        let sample_a = delay.read_cubic(self.offset_a.clamp(1.0, max_read));
        let sample_b = delay.read_cubic(self.offset_b.clamp(1.0, max_read));

        // Crossfade between grains: fade fully to one head, then re-seat
        // the silent head once the audible one has strayed half a grain.
        let fade_rate = 1.0 / grain_samples;
        if self.grain_phase {
            self.crossfade = (self.crossfade + fade_rate).min(1.0);
            if self.crossfade >= 1.0
                && (self.offset_a - target_delay).abs() > grain_samples * 0.5
            {
                self.offset_b = target_delay;
                self.grain_phase = false;
            }
        } else {
            self.crossfade = (self.crossfade - fade_rate).max(0.0);
            if self.crossfade <= 0.0
                && (self.offset_b - target_delay).abs() > grain_samples * 0.5
            {
                self.offset_a = target_delay;
                self.grain_phase = true;
            }
        }

        sample_a * self.crossfade + sample_b * (1.0 - self.crossfade)
    }

    pub fn reset(&mut self) {
        self.offset_a = 0.0;
        self.offset_b = 0.0;
        self.crossfade = 1.0;
        self.grain_phase = true;
    }
}
