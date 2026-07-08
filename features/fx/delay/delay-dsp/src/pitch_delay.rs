//! PitchDelay — per-repeat pitch shifting with granular crossfade.
//!
//! Two read heads move through the delay buffer at `speed` rate relative
//! to write. When a read head drifts too far from the target delay, it
//! resets and a crossfade prevents clicks.

use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::smoothing::ParamSmoother;

use crate::grain::GrainReader;

// r[impl delay.pitch.shift]
// r[impl delay.pitch.granular-crossfade]
/// Pitch-shifted delay line with grain crossfading.
///
/// `speed` controls pitch: 1.0 = normal, 2.0 = octave up, 0.5 = octave down.
pub struct PitchDelay {
    /// Delay time in milliseconds.
    pub time_ms: f64,
    /// Feedback amount (0.0–1.0).
    pub feedback: f64,
    /// Playback speed ratio (1.0 = normal pitch).
    pub speed: f64,
    /// Crossfade grain size in milliseconds.
    pub grain_ms: f64,
    /// Decay EQ tilt (-1.0 = darken repeats, 0 = neutral, +1.0 = brighten).
    pub decay_tilt: f64,

    decay_eq: Biquad,
    delay: DelayLine,
    grain: GrainReader,
    dc_blocker: DcBlocker,
    feedback_sample: f64,
    sample_rate: f64,
    smoother: ParamSmoother,
}

impl PitchDelay {
    const MAX_DELAY_S: f64 = 5.0;

    pub fn new() -> Self {
        let buf_len = 48000 * 5 + 1024;
        Self {
            time_ms: 250.0,
            feedback: 0.4,
            speed: 1.0,
            grain_ms: 30.0,
            decay_tilt: 0.0,
            decay_eq: Biquad::new(),
            delay: DelayLine::new(buf_len),
            grain: GrainReader::new(),
            dc_blocker: DcBlocker::new(),
            feedback_sample: 0.0,
            sample_rate: 48000.0,
            smoother: ParamSmoother::new(0.0),
        }
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let max_len = (sample_rate * Self::MAX_DELAY_S) as usize + 1024;
        if self.delay.len() < max_len {
            self.delay = DelayLine::new(max_len);
        }
        // Decay EQ: tilt filter in feedback path
        if self.decay_tilt.abs() > 0.01 {
            if self.decay_tilt < 0.0 {
                let freq = 20000.0 * (1.0 + self.decay_tilt).max(0.05);
                self.decay_eq
                    .set(FilterType::Lowpass, freq, 0.707, sample_rate);
            } else {
                let freq = 20.0 + self.decay_tilt * 2000.0;
                self.decay_eq
                    .set(FilterType::Highpass, freq, 0.707, sample_rate);
            }
        }

        self.smoother.set_time(0.15, sample_rate);
        self.dc_blocker.set_cutoff(10.0, sample_rate);
        let target = self.time_ms * 0.001 * sample_rate;
        if self.smoother.value() == 0.0 {
            self.smoother.set_immediate(target);
            self.grain.seat(target);
        }
    }

    // r[impl delay.pitch.tick]
    /// Process one sample. Returns the pitch-shifted delayed output.
    pub fn tick(&mut self, input: f64) -> f64 {
        // Smooth delay time
        let target_delay = self.time_ms * 0.001 * self.sample_rate;
        self.smoother.set_target(target_delay);
        let smooth_delay = self.smoother.tick();

        let grain_samples = (self.grain_ms * 0.001 * self.sample_rate).max(64.0);

        let output = self
            .grain
            .tick(&self.delay, smooth_delay, grain_samples, self.speed);

        // Feedback with self-limiting
        let mut fb = output * self.feedback;
        if self.decay_tilt.abs() > 0.01 {
            fb = self.decay_eq.tick(fb, 0);
        }
        let limited_fb = if fb.abs() > 0.001 {
            fb * (3.0 - fb.abs() * 2.0).max(0.0) / 3.0
        } else {
            fb
        };
        // Grain crossfades + the nonlinear limiter can build a subsonic
        // offset over many recirculations — block it inside the loop.
        let clamped_fb = self.dc_blocker.tick(limited_fb.clamp(-1.5, 1.5));

        self.delay.write(input + clamped_fb);
        self.feedback_sample = clamped_fb;

        output
    }

    pub fn last_feedback(&self) -> f64 {
        self.feedback_sample
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.decay_eq.reset();
        self.grain.reset();
        self.dc_blocker.reset();
        self.feedback_sample = 0.0;
        self.smoother.reset(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 48000.0;

    fn make_pitch_delay() -> PitchDelay {
        let mut d = PitchDelay::new();
        d.time_ms = 100.0;
        d.feedback = 0.0;
        d.speed = 1.0;
        d.update(SR);
        d
    }

    #[test]
    fn unity_pitch_delays_signal() {
        let mut d = make_pitch_delay();

        let mut peak_pos = 0;
        let mut peak_val: f64 = 0.0;

        for i in 0..10000 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let out = d.tick(input);
            if out.abs() > peak_val {
                peak_val = out.abs();
                peak_pos = i;
            }
        }

        // At speed=1.0, offset stays at target (~4800 samples)
        assert!(
            peak_pos > 4000 && peak_pos < 6000,
            "Peak at {peak_pos}, expected near 4800"
        );
        assert!(peak_val > 0.3, "Peak should be significant: {peak_val}");
    }

    #[test]
    fn no_nan() {
        let mut d = PitchDelay::new();
        d.time_ms = 200.0;
        d.feedback = 0.6;
        d.speed = 1.5;
        d.update(SR);

        for i in 0..96000 {
            let input = (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5;
            let out = d.tick(input);
            assert!(out.is_finite(), "NaN at sample {i}");
        }
    }

    #[test]
    fn feedback_self_limits() {
        let mut d = PitchDelay::new();
        d.time_ms = 50.0;
        d.feedback = 0.99;
        d.speed = 1.0;
        d.update(SR);

        for _ in 0..480 {
            d.tick(1.0);
        }

        let mut max_out: f64 = 0.0;
        for _ in 0..96000 {
            let out = d.tick(0.0);
            max_out = max_out.max(out.abs());
        }

        assert!(max_out < 5.0, "Should self-limit: max={max_out}");
    }

    #[test]
    fn pitch_shift_changes_output() {
        // At speed != 1.0, output should differ from normal delay
        let mut d_normal = make_pitch_delay();
        let mut d_shifted = make_pitch_delay();
        d_shifted.speed = 0.5; // Octave down

        let mut out_normal = Vec::new();
        let mut out_shifted = Vec::new();

        for i in 0..9600 {
            let s = (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5;
            out_normal.push(d_normal.tick(s));
            out_shifted.push(d_shifted.tick(s));
        }

        // Outputs should differ significantly
        let diff: f64 = out_normal
            .iter()
            .zip(out_shifted.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / 9600.0;

        assert!(
            diff > 0.001,
            "Pitch shift should change output: avg_diff={diff}"
        );
    }
}
