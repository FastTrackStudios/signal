//! Full reverb delay line with feedback loop.
//!
//! Ported from CloudSeedCore DelayLine.h (MIT, Ghost Note Audio).
//! Signal flow: input + feedback → ModulatedDelay → [AllpassDiffuser]
//! → [Biquad LowShelf] → [Biquad HighShelf] → [Lp1 Cutoff] → feedback buffer → output.

use super::allpass_diffuser::AllpassDiffuser;
use super::biquad::{Biquad, FilterType};
use super::modulated_delay::ModulatedDelay;
use super::one_pole::Lp1;

pub struct ReverbLine {
    delay: ModulatedDelay,
    diffuser: AllpassDiffuser,
    low_shelf: Biquad,
    high_shelf: Biquad,
    low_pass: Lp1,
    feedback_value: f64,
    feedback_coeff: f64,
    pub diffuser_enabled: bool,
    pub low_shelf_enabled: bool,
    pub high_shelf_enabled: bool,
    pub cutoff_enabled: bool,
    pub tap_post_diffuser: bool,
}

impl ReverbLine {
    pub fn new(sample_rate: f64) -> Self {
        let mut low_shelf = Biquad::new(FilterType::LowShelf, sample_rate);
        low_shelf.set_gain_db(-20.0);
        low_shelf.frequency = 20.0;
        low_shelf.update();

        let mut high_shelf = Biquad::new(FilterType::HighShelf, sample_rate);
        high_shelf.set_gain_db(-20.0);
        high_shelf.frequency = 19000.0;
        high_shelf.update();

        let mut low_pass = Lp1::new();
        low_pass.set_cutoff(1000.0);
        low_pass.set_sample_rate(sample_rate);

        let mut diffuser = AllpassDiffuser::new_default();
        diffuser.set_sample_rate(sample_rate);
        diffuser.set_interpolation_enabled(true);

        Self {
            delay: ModulatedDelay::new(),
            diffuser,
            low_shelf,
            high_shelf,
            low_pass,
            feedback_value: 0.0,
            feedback_coeff: 0.0,
            diffuser_enabled: false,
            low_shelf_enabled: false,
            high_shelf_enabled: false,
            cutoff_enabled: false,
            tap_post_diffuser: false,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.diffuser.set_sample_rate(sr);
        self.low_pass.set_sample_rate(sr);
        self.low_shelf.set_sample_rate(sr);
        self.high_shelf.set_sample_rate(sr);
    }

    pub fn set_diffuser_seed(&mut self, seed: u64, cross_seed: f64) {
        self.diffuser.set_seed(seed);
        self.diffuser.set_cross_seed(cross_seed);
    }

    pub fn set_delay(&mut self, samples: usize) {
        self.delay.sample_delay = samples;
    }

    pub fn set_feedback(&mut self, feedback: f64) {
        self.feedback_coeff = feedback;
    }

    pub fn set_diffuser_delay(&mut self, samples: usize) {
        self.diffuser.set_delay(samples);
    }

    pub fn set_diffuser_feedback(&mut self, feedback: f64) {
        self.diffuser.set_feedback(feedback);
    }

    pub fn set_diffuser_stages(&mut self, stages: usize) {
        self.diffuser.stages = stages;
    }

    pub fn set_low_shelf_gain(&mut self, gain_db: f64) {
        self.low_shelf.set_gain_db(gain_db);
        self.low_shelf.update();
    }

    pub fn set_low_shelf_frequency(&mut self, freq: f64) {
        self.low_shelf.frequency = freq;
        self.low_shelf.update();
    }

    pub fn set_high_shelf_gain(&mut self, gain_db: f64) {
        self.high_shelf.set_gain_db(gain_db);
        self.high_shelf.update();
    }

    pub fn set_high_shelf_frequency(&mut self, freq: f64) {
        self.high_shelf.frequency = freq;
        self.high_shelf.update();
    }

    pub fn set_cutoff_frequency(&mut self, freq: f64) {
        self.low_pass.set_cutoff(freq);
    }

    pub fn set_line_mod_amount(&mut self, amount: f64) {
        self.delay.mod_amount = amount;
    }

    pub fn set_line_mod_rate(&mut self, rate: f64) {
        self.delay.mod_rate = rate;
    }

    pub fn set_diffuser_mod_amount(&mut self, amount: f64) {
        self.diffuser.set_modulation_enabled(amount > 0.0);
        self.diffuser.set_mod_amount(amount);
    }

    pub fn set_diffuser_mod_rate(&mut self, rate: f64) {
        self.diffuser.set_mod_rate(rate);
    }

    pub fn set_interpolation_enabled(&mut self, enabled: bool) {
        self.diffuser.set_interpolation_enabled(enabled);
    }

    /// Process one sample through the reverb line.
    /// Returns the output sample (tapped pre- or post-diffuser).
    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        let combined = input + self.feedback_value * self.feedback_coeff;

        let delayed = self.delay.tick(combined);

        let output_pre = delayed;

        let mut x = delayed;
        if self.diffuser_enabled {
            x = self.diffuser.tick(x);
        }

        let output_post = x;

        if self.low_shelf_enabled {
            x = self.low_shelf.tick(x);
        }
        if self.high_shelf_enabled {
            x = self.high_shelf.tick(x);
        }
        if self.cutoff_enabled {
            x = self.low_pass.tick(x);
        }

        self.feedback_value = x;

        if self.tap_post_diffuser {
            output_post
        } else {
            output_pre
        }
    }

    pub fn clear_diffuser(&mut self) {
        self.diffuser.clear();
    }

    pub fn clear(&mut self) {
        self.delay.clear();
        self.diffuser.clear();
        self.low_shelf.clear();
        self.high_shelf.clear();
        self.low_pass.reset();
        self.feedback_value = 0.0;
    }

    pub fn reset(&mut self) {
        self.clear();
    }
}
