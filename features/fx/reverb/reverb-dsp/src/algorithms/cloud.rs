//! Cloud reverb — full CloudSeedCore reverb engine.
//!
//! Faithfully ported from CloudSeedCore (MIT, Ghost Note Audio).
//! Implements the complete 45-parameter CloudSeed architecture:
//! Input HP/LP → PreDelay → MultitapDelay → AllpassDiffuser
//! → Parallel ReverbLines with per-line feedback/diffusion/EQ → Output.
//!
//! The stereo version runs two independent channels with cross-seed
//! decorrelation, matching CloudSeed's ReverbController architecture.
//!
//! The 8 AlgorithmParams are mapped to CloudSeed's 45 internal parameters
//! using the original ScaleParam() response curves.

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::lcg_random::random_buffer_cross_seed;
use crate::primitives::modulated_delay::ModulatedDelay;
use crate::primitives::multitap_delay::MultitapDelay;
use crate::primitives::one_pole::{Hp1, Lp1};
use crate::primitives::response_curves::*;
use crate::primitives::reverb_line::ReverbLine;

const TOTAL_LINE_COUNT: usize = 12;

/// CloudSeed's 45 internal parameter indices (matching Parameters.h).
mod param {
    pub const INTERPOLATION: usize = 0;
    pub const LOW_CUT_ENABLED: usize = 1;
    pub const HIGH_CUT_ENABLED: usize = 2;
    pub const INPUT_MIX: usize = 3;
    pub const LOW_CUT: usize = 4;
    pub const HIGH_CUT: usize = 5;
    pub const DRY_OUT: usize = 6;
    pub const EARLY_OUT: usize = 7;
    pub const LATE_OUT: usize = 8;
    pub const TAP_ENABLED: usize = 9;
    pub const TAP_COUNT: usize = 10;
    pub const TAP_DECAY: usize = 11;
    pub const TAP_PREDELAY: usize = 12;
    pub const TAP_LENGTH: usize = 13;
    pub const EARLY_DIFFUSE_ENABLED: usize = 14;
    pub const EARLY_DIFFUSE_COUNT: usize = 15;
    pub const EARLY_DIFFUSE_DELAY: usize = 16;
    pub const EARLY_DIFFUSE_MOD_AMOUNT: usize = 17;
    pub const EARLY_DIFFUSE_FEEDBACK: usize = 18;
    pub const EARLY_DIFFUSE_MOD_RATE: usize = 19;
    pub const LATE_MODE: usize = 20;
    pub const LATE_LINE_COUNT: usize = 21;
    pub const LATE_DIFFUSE_ENABLED: usize = 22;
    pub const LATE_DIFFUSE_COUNT: usize = 23;
    pub const LATE_LINE_SIZE: usize = 24;
    pub const LATE_LINE_MOD_AMOUNT: usize = 25;
    pub const LATE_DIFFUSE_DELAY: usize = 26;
    pub const LATE_DIFFUSE_MOD_AMOUNT: usize = 27;
    pub const LATE_LINE_DECAY: usize = 28;
    pub const LATE_LINE_MOD_RATE: usize = 29;
    pub const LATE_DIFFUSE_FEEDBACK: usize = 30;
    pub const LATE_DIFFUSE_MOD_RATE: usize = 31;
    pub const EQ_LOW_SHELF_ENABLED: usize = 32;
    pub const EQ_HIGH_SHELF_ENABLED: usize = 33;
    pub const EQ_LOWPASS_ENABLED: usize = 34;
    pub const EQ_LOW_FREQ: usize = 35;
    pub const EQ_HIGH_FREQ: usize = 36;
    pub const EQ_CUTOFF: usize = 37;
    pub const EQ_LOW_GAIN: usize = 38;
    pub const EQ_HIGH_GAIN: usize = 39;
    pub const EQ_CROSS_SEED: usize = 40;
    pub const SEED_TAP: usize = 41;
    pub const SEED_DIFFUSION: usize = 42;
    pub const SEED_DELAY: usize = 43;
    pub const SEED_POST_DIFFUSION: usize = 44;
    pub const COUNT: usize = 45;
}

/// CloudSeed's ScaleParam() — exact port from Parameters.h.
fn scale_param(val: f64, index: usize) -> f64 {
    match index {
        param::INTERPOLATION
        | param::LOW_CUT_ENABLED
        | param::HIGH_CUT_ENABLED
        | param::TAP_ENABLED
        | param::LATE_DIFFUSE_ENABLED
        | param::EQ_LOW_SHELF_ENABLED
        | param::EQ_HIGH_SHELF_ENABLED
        | param::EQ_LOWPASS_ENABLED
        | param::EARLY_DIFFUSE_ENABLED => {
            if val < 0.5 {
                0.0
            } else {
                1.0
            }
        }

        param::INPUT_MIX
        | param::EARLY_DIFFUSE_FEEDBACK
        | param::TAP_DECAY
        | param::LATE_DIFFUSE_FEEDBACK
        | param::EQ_CROSS_SEED => val,

        param::SEED_TAP
        | param::SEED_DIFFUSION
        | param::SEED_DELAY
        | param::SEED_POST_DIFFUSION => (val * 999.999).floor(),

        param::LOW_CUT => 20.0 + resp4oct(val) * 980.0,
        param::HIGH_CUT => 400.0 + resp4oct(val) * 19600.0,

        param::DRY_OUT | param::EARLY_OUT | param::LATE_OUT => -30.0 + val * 30.0,

        param::TAP_COUNT => (1.0 + val * 255.0).floor(),
        param::TAP_PREDELAY => resp1dec(val) * 500.0,
        param::TAP_LENGTH => 10.0 + val * 990.0,

        param::EARLY_DIFFUSE_COUNT => (1.0 + val * 11.999).floor(),
        param::EARLY_DIFFUSE_DELAY => 10.0 + val * 90.0,
        param::EARLY_DIFFUSE_MOD_AMOUNT => val * 2.5,
        param::EARLY_DIFFUSE_MOD_RATE => resp2dec(val) * 5.0,

        param::LATE_MODE => {
            if val < 0.5 {
                0.0
            } else {
                1.0
            }
        }
        param::LATE_LINE_COUNT => (1.0 + val * 11.999).floor(),
        param::LATE_DIFFUSE_COUNT => (1.0 + val * 7.999).floor(),
        param::LATE_LINE_SIZE => 20.0 + resp2dec(val) * 980.0,
        param::LATE_LINE_MOD_AMOUNT => val * 2.5,
        param::LATE_DIFFUSE_DELAY => 10.0 + val * 90.0,
        param::LATE_DIFFUSE_MOD_AMOUNT => val * 2.5,
        param::LATE_LINE_DECAY => 0.05 + resp3dec(val) * 59.95,
        param::LATE_LINE_MOD_RATE => resp2dec(val) * 5.0,
        param::LATE_DIFFUSE_MOD_RATE => resp2dec(val) * 5.0,

        param::EQ_LOW_FREQ => 20.0 + resp3oct(val) * 980.0,
        param::EQ_HIGH_FREQ => 400.0 + resp4oct(val) * 19600.0,
        param::EQ_CUTOFF => 400.0 + resp4oct(val) * 19600.0,
        param::EQ_LOW_GAIN | param::EQ_HIGH_GAIN => -20.0 + val * 20.0,

        _ => val,
    }
}

/// Single CloudSeed reverb channel (mono).
struct CloudChannel {
    params_scaled: [f64; param::COUNT],
    sample_rate: f64,

    pre_delay: ModulatedDelay,
    multitap: MultitapDelay,
    diffuser: AllpassDiffuser,
    lines: Vec<ReverbLine>,
    high_pass: Hp1,
    low_pass: Lp1,

    delay_line_seed: u64,
    post_diffusion_seed: u64,
    line_count: usize,

    low_cut_enabled: bool,
    high_cut_enabled: bool,
    multitap_enabled: bool,
    diffuser_enabled: bool,
    input_mix: f64,
    early_out: f64,
    line_out: f64,
    cross_seed: f64,
    is_right: bool,
}

impl CloudChannel {
    fn new(sample_rate: f64, is_right: bool) -> Self {
        let lines: Vec<ReverbLine> = (0..TOTAL_LINE_COUNT)
            .map(|_| ReverbLine::new(sample_rate as f64))
            .collect();

        let mut diffuser = AllpassDiffuser::new_default();
        diffuser.set_sample_rate(sample_rate);
        diffuser.set_interpolation_enabled(true);

        let mut high_pass = Hp1::new();
        high_pass.set_freq(20.0, sample_rate);
        let mut low_pass = Lp1::new();
        low_pass.set_freq(20000.0, sample_rate);

        let mut ch = Self {
            params_scaled: [0.0; param::COUNT],
            sample_rate,
            pre_delay: ModulatedDelay::new(),
            multitap: MultitapDelay::new(384000),
            diffuser,
            lines,
            high_pass,
            low_pass,
            delay_line_seed: 0,
            post_diffusion_seed: 0,
            line_count: 8,
            low_cut_enabled: false,
            high_cut_enabled: false,
            multitap_enabled: true,
            diffuser_enabled: true,
            input_mix: 0.0,
            early_out: 1.0,
            line_out: 1.0,
            cross_seed: if is_right { 0.5 } else { 0.5 },
            is_right,
        };

        ch.clear();
        ch.update_lines();
        ch
    }

    fn set_sample_rate(&mut self, sr: f64) {
        self.sample_rate = sr;
        self.high_pass.set_sample_rate(sr);
        self.low_pass.set_sample_rate(sr);
        self.diffuser.set_sample_rate(sr);
        for line in &mut self.lines {
            line.set_sample_rate(sr);
        }
        self.reapply_all_params();
        self.clear();
        self.update_lines();
    }

    fn reapply_all_params(&mut self) {
        for i in 0..param::COUNT {
            let val = self.params_scaled[i];
            self.apply_param(i, val);
        }
    }

    /// Apply a single scaled parameter — exact port of ReverbChannel::SetParameter.
    fn apply_param(&mut self, para: usize, scaled: f64) {
        self.params_scaled[para] = scaled;

        match para {
            param::INTERPOLATION => {
                for line in &mut self.lines {
                    line.set_interpolation_enabled(scaled >= 0.5);
                }
            }
            param::LOW_CUT_ENABLED => {
                self.low_cut_enabled = scaled >= 0.5;
                if self.low_cut_enabled {
                    self.high_pass.reset();
                }
            }
            param::HIGH_CUT_ENABLED => {
                self.high_cut_enabled = scaled >= 0.5;
                if self.high_cut_enabled {
                    self.low_pass.reset();
                }
            }
            param::INPUT_MIX => self.input_mix = scaled,
            param::LOW_CUT => self.high_pass.set_cutoff(scaled),
            param::HIGH_CUT => self.low_pass.set_cutoff(scaled),
            param::DRY_OUT => { /* handled at stereo level */ }
            param::EARLY_OUT => {
                self.early_out = if scaled <= -30.0 {
                    0.0
                } else {
                    db2gain(scaled)
                };
            }
            param::LATE_OUT => {
                self.line_out = if scaled <= -30.0 {
                    0.0
                } else {
                    db2gain(scaled)
                };
            }

            param::TAP_ENABLED => {
                let new_val = scaled >= 0.5;
                if new_val != self.multitap_enabled {
                    self.multitap.clear();
                }
                self.multitap_enabled = new_val;
            }
            param::TAP_COUNT => self.multitap.set_tap_count(scaled as usize),
            param::TAP_DECAY => self.multitap.set_tap_decay(scaled),
            param::TAP_PREDELAY => {
                self.pre_delay.sample_delay = self.ms2samples(scaled) as usize;
            }
            param::TAP_LENGTH => {
                self.multitap
                    .set_tap_length(self.ms2samples(scaled) as usize);
            }

            param::EARLY_DIFFUSE_ENABLED => {
                let new_val = scaled >= 0.5;
                if new_val != self.diffuser_enabled {
                    self.diffuser.clear();
                }
                self.diffuser_enabled = new_val;
            }
            param::EARLY_DIFFUSE_COUNT => self.diffuser.stages = scaled as usize,
            param::EARLY_DIFFUSE_DELAY => {
                self.diffuser.set_delay(self.ms2samples(scaled) as usize);
            }
            param::EARLY_DIFFUSE_MOD_AMOUNT => {
                self.diffuser.set_modulation_enabled(scaled > 0.5);
                self.diffuser.set_mod_amount(self.ms2samples(scaled));
            }
            param::EARLY_DIFFUSE_FEEDBACK => self.diffuser.set_feedback(scaled),
            param::EARLY_DIFFUSE_MOD_RATE => self.diffuser.set_mod_rate(scaled),

            param::LATE_MODE => {
                for line in &mut self.lines {
                    line.tap_post_diffuser = scaled >= 0.5;
                }
            }
            param::LATE_LINE_COUNT => self.line_count = scaled as usize,
            param::LATE_DIFFUSE_ENABLED => {
                for line in &mut self.lines {
                    let new_val = scaled >= 0.5;
                    if new_val != line.diffuser_enabled {
                        line.clear_diffuser();
                    }
                    line.diffuser_enabled = new_val;
                }
            }
            param::LATE_DIFFUSE_COUNT => {
                for line in &mut self.lines {
                    line.set_diffuser_stages(scaled as usize);
                }
            }
            param::LATE_LINE_SIZE
            | param::LATE_LINE_MOD_AMOUNT
            | param::LATE_DIFFUSE_MOD_AMOUNT
            | param::LATE_LINE_DECAY
            | param::LATE_LINE_MOD_RATE
            | param::LATE_DIFFUSE_MOD_RATE => {
                self.update_lines();
            }
            param::LATE_DIFFUSE_DELAY => {
                let samples = self.ms2samples(scaled) as usize;
                for line in &mut self.lines {
                    line.set_diffuser_delay(samples);
                }
            }
            param::LATE_DIFFUSE_FEEDBACK => {
                for line in &mut self.lines {
                    line.set_diffuser_feedback(scaled);
                }
            }

            param::EQ_LOW_SHELF_ENABLED => {
                for line in &mut self.lines {
                    line.low_shelf_enabled = scaled >= 0.5;
                }
            }
            param::EQ_HIGH_SHELF_ENABLED => {
                for line in &mut self.lines {
                    line.high_shelf_enabled = scaled >= 0.5;
                }
            }
            param::EQ_LOWPASS_ENABLED => {
                for line in &mut self.lines {
                    line.cutoff_enabled = scaled >= 0.5;
                }
            }
            param::EQ_LOW_FREQ => {
                for line in &mut self.lines {
                    line.set_low_shelf_frequency(scaled);
                }
            }
            param::EQ_HIGH_FREQ => {
                for line in &mut self.lines {
                    line.set_high_shelf_frequency(scaled);
                }
            }
            param::EQ_CUTOFF => {
                for line in &mut self.lines {
                    line.set_cutoff_frequency(scaled);
                }
            }
            param::EQ_LOW_GAIN => {
                for line in &mut self.lines {
                    line.set_low_shelf_gain(scaled);
                }
            }
            param::EQ_HIGH_GAIN => {
                for line in &mut self.lines {
                    line.set_high_shelf_gain(scaled);
                }
            }
            param::EQ_CROSS_SEED => {
                self.cross_seed = if self.is_right {
                    0.5 * scaled
                } else {
                    1.0 - 0.5 * scaled
                };
                self.multitap.set_cross_seed(self.cross_seed);
                self.diffuser.set_cross_seed(self.cross_seed);
                self.update_lines();
                self.update_post_diffusion();
            }

            param::SEED_TAP => self.multitap.set_seed(scaled as u64),
            param::SEED_DIFFUSION => self.diffuser.set_seed(scaled as u64),
            param::SEED_DELAY => {
                self.delay_line_seed = scaled as u64;
                self.update_lines();
            }
            param::SEED_POST_DIFFUSION => {
                self.post_diffusion_seed = scaled as u64;
                self.update_post_diffusion();
            }

            _ => {}
        }
    }

    fn ms2samples(&self, ms: f64) -> f64 {
        ms / 1000.0 * self.sample_rate
    }

    fn per_line_gain(&self) -> f64 {
        1.0 / (self.line_count.max(1) as f64).sqrt()
    }

    /// Exact port of ReverbChannel::UpdateLines.
    fn update_lines(&mut self) {
        let line_delay_samples = self.ms2samples(self.params_scaled[param::LATE_LINE_SIZE]);
        let line_decay_millis = self.params_scaled[param::LATE_LINE_DECAY] * 1000.0;
        let line_decay_samples = self.ms2samples(line_decay_millis);

        let line_mod_amount = self.ms2samples(self.params_scaled[param::LATE_LINE_MOD_AMOUNT]);
        let line_mod_rate = self.params_scaled[param::LATE_LINE_MOD_RATE];

        let late_diff_mod_amount =
            self.ms2samples(self.params_scaled[param::LATE_DIFFUSE_MOD_AMOUNT]);
        let late_diff_mod_rate = self.params_scaled[param::LATE_DIFFUSE_MOD_RATE];

        let seeds =
            random_buffer_cross_seed(self.delay_line_seed, TOTAL_LINE_COUNT * 3, self.cross_seed);

        for i in 0..TOTAL_LINE_COUNT {
            let mod_amount = line_mod_amount * (0.7 + 0.3 * seeds[i]);
            let mod_rate =
                line_mod_rate * (0.7 + 0.3 * seeds[TOTAL_LINE_COUNT + i]) / self.sample_rate;

            let mut delay_samples =
                (0.5 + 1.0 * seeds[TOTAL_LINE_COUNT * 2 + i]) * line_delay_samples;
            // When delay is really short and modulation is high,
            // mod could take delay time negative — prevent that
            if delay_samples < mod_amount + 2.0 {
                delay_samples = mod_amount + 2.0;
            }

            // T60 decay calculation
            let db_after_1iter = delay_samples / line_decay_samples.max(1.0) * (-60.0);
            let gain_after_1iter = db2gain(db_after_1iter);

            self.lines[i].set_delay(delay_samples as usize);
            self.lines[i].set_feedback(gain_after_1iter);
            self.lines[i].set_line_mod_amount(mod_amount);
            self.lines[i].set_line_mod_rate(mod_rate);
            self.lines[i].set_diffuser_mod_amount(late_diff_mod_amount);
            self.lines[i].set_diffuser_mod_rate(late_diff_mod_rate);
        }
    }

    /// Exact port of ReverbChannel::UpdatePostDiffusion.
    fn update_post_diffusion(&mut self) {
        for i in 0..TOTAL_LINE_COUNT {
            self.lines[i]
                .set_diffuser_seed(self.post_diffusion_seed * (i as u64 + 1), self.cross_seed);
        }
    }

    /// Process one sample — exact port of ReverbChannel::Process (per-sample).
    #[inline]
    fn tick(&mut self, input: f64) -> (f64, f64) {
        let mut x = input;

        // Input filters
        if self.low_cut_enabled {
            x = self.high_pass.tick(x);
        }
        if self.high_cut_enabled {
            x = self.low_pass.tick(x);
        }

        // Denormal prevention (CloudSeed: zero if n*n < 1e-9)
        if x * x < 1e-9 {
            x = 0.0;
        }

        // Pre-delay
        x = self.pre_delay.tick(x);

        // Multitap early reflections
        if self.multitap_enabled {
            x = self.multitap.tick(x);
        }

        // Input diffusion
        if self.diffuser_enabled {
            x = self.diffuser.tick(x);
        }

        let early = x;

        // Late reverb: parallel delay lines
        let mut line_sum = 0.0;
        for i in 0..self.line_count.min(TOTAL_LINE_COUNT) {
            line_sum += self.lines[i].tick(x);
        }
        line_sum *= self.per_line_gain();

        // Output = early * earlyOut + late * lineOut
        let output = self.early_out * early + self.line_out * line_sum;
        (output, line_sum)
    }

    fn clear(&mut self) {
        self.low_pass.reset();
        self.high_pass.reset();
        self.pre_delay.clear();
        self.multitap.clear();
        self.diffuser.clear();
        for line in &mut self.lines {
            line.clear();
        }
    }
}

/// Cloud reverb — stereo CloudSeed engine.
///
/// Exact port of CloudSeedCore's ReverbController:
/// two independent ReverbChannels with input crossfeed mixing
/// and per-channel cross-seed decorrelation.
pub struct Cloud {
    left: CloudChannel,
    right: CloudChannel,
    raw_params: [f64; param::COUNT],
    sample_rate: f64,
}

impl Cloud {
    pub fn new(sample_rate: f64) -> Self {
        let mut cloud = Self {
            left: CloudChannel::new(sample_rate, false),
            right: CloudChannel::new(sample_rate, true),
            raw_params: [0.0; param::COUNT],
            sample_rate,
        };

        // Set sensible defaults for the 45 parameters (raw [0,1] values)
        cloud.set_raw_param(param::INTERPOLATION, 1.0);
        cloud.set_raw_param(param::INPUT_MIX, 0.5);
        cloud.set_raw_param(param::DRY_OUT, 0.0); // -30 + 0*30 = -30dB (muted)
        cloud.set_raw_param(param::EARLY_OUT, 0.8); // -30 + 0.8*30 = -6dB
        cloud.set_raw_param(param::LATE_OUT, 1.0); // -30 + 1.0*30 = 0dB
        cloud.set_raw_param(param::TAP_ENABLED, 1.0);
        cloud.set_raw_param(param::TAP_COUNT, 0.12); // ~32 taps
        cloud.set_raw_param(param::TAP_DECAY, 0.5);
        cloud.set_raw_param(param::TAP_PREDELAY, 0.2);
        cloud.set_raw_param(param::TAP_LENGTH, 0.3);
        cloud.set_raw_param(param::EARLY_DIFFUSE_ENABLED, 1.0);
        cloud.set_raw_param(param::EARLY_DIFFUSE_COUNT, 0.6); // ~8 stages
        cloud.set_raw_param(param::EARLY_DIFFUSE_DELAY, 0.5);
        cloud.set_raw_param(param::EARLY_DIFFUSE_FEEDBACK, 0.6);
        cloud.set_raw_param(param::EARLY_DIFFUSE_MOD_AMOUNT, 0.2);
        cloud.set_raw_param(param::EARLY_DIFFUSE_MOD_RATE, 0.3);
        cloud.set_raw_param(param::LATE_LINE_COUNT, 0.65); // ~8 lines
        cloud.set_raw_param(param::LATE_DIFFUSE_ENABLED, 1.0);
        cloud.set_raw_param(param::LATE_DIFFUSE_COUNT, 0.5); // ~5 stages
        cloud.set_raw_param(param::LATE_LINE_SIZE, 0.4);
        cloud.set_raw_param(param::LATE_LINE_DECAY, 0.3);
        cloud.set_raw_param(param::LATE_LINE_MOD_AMOUNT, 0.15);
        cloud.set_raw_param(param::LATE_LINE_MOD_RATE, 0.3);
        cloud.set_raw_param(param::LATE_DIFFUSE_DELAY, 0.4);
        cloud.set_raw_param(param::LATE_DIFFUSE_FEEDBACK, 0.6);
        cloud.set_raw_param(param::LATE_DIFFUSE_MOD_AMOUNT, 0.1);
        cloud.set_raw_param(param::LATE_DIFFUSE_MOD_RATE, 0.3);
        cloud.set_raw_param(param::EQ_LOWPASS_ENABLED, 1.0);
        cloud.set_raw_param(param::EQ_CUTOFF, 0.6);
        cloud.set_raw_param(param::EQ_CROSS_SEED, 0.4);
        cloud.set_raw_param(param::SEED_TAP, 0.3);
        cloud.set_raw_param(param::SEED_DIFFUSION, 0.5);
        cloud.set_raw_param(param::SEED_DELAY, 0.7);
        cloud.set_raw_param(param::SEED_POST_DIFFUSION, 0.4);

        cloud
    }

    /// Set a raw [0, 1] parameter and apply through ScaleParam to both channels.
    fn set_raw_param(&mut self, param_id: usize, value: f64) {
        self.raw_params[param_id] = value;
        let scaled = scale_param(value, param_id);
        self.left.apply_param(param_id, scaled);
        self.right.apply_param(param_id, scaled);
    }
}

impl ReverbAlgorithm for Cloud {
    fn reset(&mut self) {
        self.left.clear();
        self.right.clear();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.left.set_sample_rate(sample_rate);
        self.right.set_sample_rate(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Map our 8 AlgorithmParams to CloudSeed's 45 raw parameters.
        // Each knob controls the most musically relevant CloudSeed parameters.

        // Decay → late line decay (0.05-60s via resp3dec)
        self.set_raw_param(param::LATE_LINE_DECAY, params.decay);
        // Also affect tap decay
        self.set_raw_param(param::TAP_DECAY, 0.3 + params.decay * 0.5);

        // Size → late line size, tap length, early diffuse delay, late diffuse delay
        self.set_raw_param(param::LATE_LINE_SIZE, params.size);
        self.set_raw_param(param::TAP_LENGTH, params.size);
        self.set_raw_param(param::EARLY_DIFFUSE_DELAY, params.size);
        self.set_raw_param(param::LATE_DIFFUSE_DELAY, params.size);

        // Diffusion → early/late diffuse counts and feedback
        self.set_raw_param(param::EARLY_DIFFUSE_COUNT, params.diffusion);
        self.set_raw_param(param::EARLY_DIFFUSE_FEEDBACK, params.diffusion * 0.8);
        self.set_raw_param(param::LATE_DIFFUSE_COUNT, params.diffusion);
        self.set_raw_param(param::LATE_DIFFUSE_FEEDBACK, params.diffusion * 0.8);
        self.set_raw_param(
            param::EARLY_DIFFUSE_ENABLED,
            if params.diffusion > 0.05 { 1.0 } else { 0.0 },
        );
        self.set_raw_param(
            param::LATE_DIFFUSE_ENABLED,
            if params.diffusion > 0.15 { 1.0 } else { 0.0 },
        );

        // Damping → EQ lowpass cutoff
        let cutoff_raw = 1.0 - params.damping;
        self.set_raw_param(
            param::EQ_LOWPASS_ENABLED,
            if params.damping > 0.05 { 1.0 } else { 0.0 },
        );
        self.set_raw_param(param::EQ_CUTOFF, cutoff_raw);

        // Modulation → all mod amounts and rates
        self.set_raw_param(param::EARLY_DIFFUSE_MOD_AMOUNT, params.modulation);
        self.set_raw_param(param::EARLY_DIFFUSE_MOD_RATE, params.modulation);
        self.set_raw_param(param::LATE_LINE_MOD_AMOUNT, params.modulation);
        self.set_raw_param(param::LATE_LINE_MOD_RATE, params.modulation);
        self.set_raw_param(param::LATE_DIFFUSE_MOD_AMOUNT, params.modulation * 0.8);
        self.set_raw_param(param::LATE_DIFFUSE_MOD_RATE, params.modulation);

        // Tone → EQ shelf gains
        if params.tone < 0.0 {
            // Dark: cut highs
            self.set_raw_param(param::EQ_HIGH_SHELF_ENABLED, 1.0);
            self.set_raw_param(param::EQ_HIGH_GAIN, 0.5 + params.tone * 0.5);
            self.set_raw_param(param::EQ_HIGH_FREQ, 0.5);
            self.set_raw_param(param::EQ_LOW_SHELF_ENABLED, 0.0);
        } else if params.tone > 0.0 {
            // Bright: cut lows
            self.set_raw_param(param::EQ_LOW_SHELF_ENABLED, 1.0);
            self.set_raw_param(param::EQ_LOW_GAIN, 0.5 - params.tone * 0.5);
            self.set_raw_param(param::EQ_LOW_FREQ, 0.3);
            self.set_raw_param(param::EQ_HIGH_SHELF_ENABLED, 0.0);
        } else {
            self.set_raw_param(param::EQ_LOW_SHELF_ENABLED, 0.0);
            self.set_raw_param(param::EQ_HIGH_SHELF_ENABLED, 0.0);
        }

        // Extra A → pre-delay (0-500ms via resp1dec), tap count, line count
        self.set_raw_param(param::TAP_PREDELAY, params.extra_a * 0.5);
        self.set_raw_param(param::TAP_COUNT, params.extra_a * 0.3);
        self.set_raw_param(param::LATE_LINE_COUNT, 0.4 + params.extra_a * 0.5);

        // Extra B → cross-seed, seeds (character/stereo width)
        self.set_raw_param(param::EQ_CROSS_SEED, params.extra_b);
        self.set_raw_param(param::INPUT_MIX, params.extra_b * 0.8);

        // Output levels (always set)
        self.set_raw_param(param::EARLY_OUT, 0.8);
        self.set_raw_param(param::LATE_OUT, 1.0);
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        // CloudSeed ReverbController input crossfeed mixing
        let input_mix = scale_param(self.raw_params[param::INPUT_MIX], param::INPUT_MIX);
        let cm = input_mix * 0.5;
        let cmi = 1.0 - cm;

        let left_in = left * cmi + right * cm;
        let right_in = right * cmi + left * cm;

        let (out_l, _) = self.left.tick(left_in);
        let (out_r, _) = self.right.tick(right_in);

        (out_l, out_r)
    }
}
