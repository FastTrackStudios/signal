//! Schroeder allpass delay filter.
//!
//! `y[n] = -g * x[n] + x[n-M] + g * y[n-M]`
//!
//! Passes all frequencies at unity gain but smears phase,
//! creating the diffusion that gives reverb its density.

use audiocore_dsp::delay_line::DelayLine;

pub struct Allpass {
    delay: DelayLine,
    delay_samples: usize,
    feedback: f64,
}

impl Allpass {
    pub fn new(max_delay: usize) -> Self {
        Self {
            delay: DelayLine::new(max_delay + 1),
            delay_samples: max_delay,
            feedback: 0.5,
        }
    }

    pub fn set_delay(&mut self, samples: usize) {
        self.delay_samples = samples.min(self.delay.len() - 1);
    }

    pub fn set_feedback(&mut self, g: f64) {
        self.feedback = g;
    }

    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        let delayed = self.delay.read(self.delay_samples);
        let v = input + self.feedback * delayed;
        self.delay.write(v);
        delayed - self.feedback * v
    }

    pub fn reset(&mut self) {
        self.delay.clear();
    }
}
