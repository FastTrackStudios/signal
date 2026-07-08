//! Delay chain — composes delay engines, ducking, and diffusion
//! into a full stereo processor.

use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::smoothing::ParamSmoother;
use audiocore_dsp::{AudioConfig, Processor};

use crate::engine::{DelayEngine, DelayStyle};
use crate::modulation::{Diffuser, DuckingFollower};

// r[impl delay.chain.signal-flow]
/// Stereo mode for the delay chain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StereoMode {
    /// Independent L/R delays.
    Stereo,
    /// Cross-feed: L output feeds R input and vice versa.
    PingPong,
    /// Mono delay duplicated to both channels.
    Mono,
}

/// RE-201–style tape head configuration.
///
/// Controls which of the 3 playback heads are active.
/// Head 1 is at base delay time, Head 2 at 1.94×, Head 3 at 2.85×
/// (matching RE-201 physical head spacing).
///
/// Modes match the Neural DSP Archetype: Mateus Asato Echo module.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HeadMode {
    /// Head 3 only — single repetition at longest interval.
    Mode1,
    /// Heads 1 + 3 — dual repetition (short + long).
    Mode2,
    /// Heads 2 + 3 — syncopated dual repetition.
    Mode3,
    /// All three heads — three-repetition pattern.
    Mode4,
}

/// Full stereo delay processing chain.
///
/// Signal flow: Input → InputLevel → Diffusion(loop) → Stereo Routing →
/// Engine → Diffusion(post) → Accent → Duck → LR Offset → Width → OutputLevel → Mix
pub struct DelayChain {
    // Delay engines
    pub delay_l: DelayEngine,
    pub delay_r: DelayEngine,

    // Global parameters
    /// Dry/wet mix (0.0 = fully dry, 1.0 = fully wet).
    pub mix: f64,
    /// Stereo mode.
    pub stereo_mode: StereoMode,
    /// Stereo width (0.0 = mono, 1.0 = normal, 2.0 = extra wide).
    pub width: f64,
    /// Tape head configuration (RE-201 style, only applies to Tape style).
    pub head_mode: HeadMode,
    /// Enable diffusion.
    pub diffusion_enabled: bool,
    /// Diffusion size (0.0–1.0).
    pub diffusion_size: f64,
    /// Diffusion smear / feedback (0.0–1.0).
    pub diffusion_smear: f64,
    /// Enable ducking.
    pub ducking_enabled: bool,
    /// Ping-pong feedback (used when stereo_mode == PingPong).
    pub pingpong_feedback: f64,

    // --- New parameters ---
    /// Accent: alternating repeat volume. Bipolar -1.0 to 1.0, 0.0 = off.
    /// Positive emphasizes odd repeats, negative emphasizes even repeats.
    pub accent: f64,

    /// Groove: shuffle/swing. Bipolar -1.0 to 1.0, 0.0 = off.
    /// Negative = shuffle, positive = swing.
    pub groove: f64,

    /// Feel: draggin'/rushin'. Bipolar -1.0 to 1.0, 0.0 = neutral.
    /// Negative = draggin' (adds pre-delay), positive = rushin' (subtracts pre-delay).
    pub feel: f64,
    /// Maximum feel offset in ms.
    pub feel_max_ms: f64,

    /// Prime numbers: anti-resonance toggle.
    /// Multiplies delay time by (1.0 + 0.0013) to prevent comb filtering.
    pub prime_numbers: bool,

    /// L/R offset in ms (0.0–25.0). Delays the R channel for stereo spread.
    pub lr_offset_ms: f64,

    /// Diffusion placement: false = post (default), true = loop (before delay).
    pub diffusion_in_loop: bool,

    /// Input level: scales signal going INTO the delay (0.0–2.0, default 1.0).
    pub input_level: f64,
    /// Output level: scales the wet signal before mixing (0.0–2.0, default 1.0).
    pub output_level: f64,

    // Internal
    diffuser_l: Diffuser,
    diffuser_r: Diffuser,
    pub ducker: DuckingFollower,
    sample_rate: f64,

    // Accent internal state
    accent_phase_l: f64,
    accent_phase_r: f64,
    accent_flip_l: bool,
    accent_flip_r: bool,

    // Groove internal state
    groove_phase_l: f64,
    groove_phase_r: f64,

    // LR offset internal state
    lr_offset_delay: DelayLine,
    lr_offset_smoother: ParamSmoother,

    // Per-sample parameter smoothing (kills zipper noise on automation)
    mix_smoother: ParamSmoother,
    width_smoother: ParamSmoother,
    input_level_smoother: ParamSmoother,
    output_level_smoother: ParamSmoother,
    pingpong_fb_smoother: ParamSmoother,

    // The ping-pong cross-feed recirculates through both engines — strip
    // DC inside that outer loop too.
    pingpong_dc_l: DcBlocker,
    pingpong_dc_r: DcBlocker,
}

impl DelayChain {
    pub fn new() -> Self {
        let mut lr_smoother = ParamSmoother::new(8.0);
        lr_smoother.set_time_ms(5.0, 48000.0);
        lr_smoother.set_epsilon(0.01);

        Self {
            delay_l: DelayEngine::new(),
            delay_r: DelayEngine::new(),
            mix: 0.5,
            stereo_mode: StereoMode::Stereo,
            width: 1.0,
            head_mode: HeadMode::Mode1,
            diffusion_enabled: false,
            diffusion_size: 0.5,
            diffusion_smear: 0.5,
            ducking_enabled: false,
            pingpong_feedback: 0.5,

            accent: 0.0,
            groove: 0.0,
            feel: 0.0,
            feel_max_ms: 50.0,
            prime_numbers: false,
            lr_offset_ms: 8.0,
            diffusion_in_loop: false,
            input_level: 1.0,
            output_level: 1.0,

            diffuser_l: Diffuser::new(48000.0, false),
            diffuser_r: Diffuser::new(48000.0, true),
            ducker: DuckingFollower::new(),
            sample_rate: 48000.0,

            accent_phase_l: 0.0,
            accent_phase_r: 0.0,
            accent_flip_l: false,
            accent_flip_r: false,

            groove_phase_l: 0.0,
            groove_phase_r: 0.0,

            // 25ms at 48kHz = 1200 samples, add margin
            lr_offset_delay: DelayLine::new(2048),
            lr_offset_smoother: lr_smoother,

            mix_smoother: ParamSmoother::new(0.5),
            width_smoother: ParamSmoother::new(1.0),
            input_level_smoother: ParamSmoother::new(1.0),
            output_level_smoother: ParamSmoother::new(1.0),
            pingpong_fb_smoother: ParamSmoother::new(0.5),

            pingpong_dc_l: DcBlocker::new(),
            pingpong_dc_r: DcBlocker::new(),
        }
    }

    /// Switch both engines to a new delay style.
    pub fn set_style(&mut self, style: DelayStyle) {
        self.delay_l.set_style(style);
        self.delay_r.set_style(style);
    }
}

impl Processor for DelayChain {
    fn reset(&mut self) {
        self.delay_l.reset();
        self.delay_r.reset();
        self.diffuser_l.reset();
        self.diffuser_r.reset();
        self.ducker.reset();

        self.accent_phase_l = 0.0;
        self.accent_phase_r = 0.0;
        self.accent_flip_l = false;
        self.accent_flip_r = false;

        self.groove_phase_l = 0.0;
        self.groove_phase_r = 0.0;

        self.lr_offset_delay.clear();
        self.lr_offset_smoother.reset(self.lr_offset_ms);

        self.mix_smoother.reset(self.mix);
        self.width_smoother.reset(self.width);
        self.input_level_smoother.reset(self.input_level);
        self.output_level_smoother.reset(self.output_level);
        self.pingpong_fb_smoother.reset(self.pingpong_feedback);
        self.pingpong_dc_l.reset();
        self.pingpong_dc_r.reset();
    }

    fn update(&mut self, config: AudioConfig) {
        self.sample_rate = config.sample_rate;

        // Configure tape heads based on head mode (only applies to Tape style)
        let (h1, h2, h3) = match self.head_mode {
            HeadMode::Mode1 => (false, false, true), // Head 3 only
            HeadMode::Mode2 => (true, false, true),  // Head 1 + 3
            HeadMode::Mode3 => (false, true, true),  // Head 2 + 3
            HeadMode::Mode4 => (true, true, true),   // All heads
        };
        self.delay_l.head1_enabled = h1;
        self.delay_l.head2_enabled = h2;
        self.delay_l.head3_enabled = h3;
        self.delay_r.head1_enabled = h1;
        self.delay_r.head2_enabled = h2;
        self.delay_r.head3_enabled = h3;

        self.delay_l.update(config.sample_rate);
        self.delay_r.update(config.sample_rate);

        // Diffuser::update resizes its buffers itself if the sample rate
        // grew — no per-update reallocation.
        self.diffuser_l.size = self.diffusion_size;
        self.diffuser_l.smear = self.diffusion_smear;
        self.diffuser_r.size = self.diffusion_size;
        self.diffuser_r.smear = self.diffusion_smear;
        self.diffuser_l.update(config.sample_rate, false);
        self.diffuser_r.update(config.sample_rate, true);

        self.ducker.set_sample_rate(config.sample_rate);
        self.ducker.update_coeffs();

        // Per-sample smoothed params: ~5ms
        for s in [
            &mut self.mix_smoother,
            &mut self.width_smoother,
            &mut self.input_level_smoother,
            &mut self.output_level_smoother,
            &mut self.pingpong_fb_smoother,
        ] {
            s.set_time_ms(5.0, config.sample_rate);
            s.set_epsilon(1e-4);
        }
        self.pingpong_dc_l.set_cutoff(10.0, config.sample_rate);
        self.pingpong_dc_r.set_cutoff(10.0, config.sample_rate);

        // Update LR offset smoother and delay line
        self.lr_offset_smoother.set_time_ms(5.0, config.sample_rate);
        self.lr_offset_smoother.set_target(self.lr_offset_ms);

        // Ensure delay line is large enough for max offset at this sample rate
        let max_offset_samples = (25.0 * config.sample_rate / 1000.0) as usize + 64;
        if self.lr_offset_delay.len() < max_offset_samples {
            self.lr_offset_delay = DelayLine::new(max_offset_samples);
        }
    }

    // r[impl delay.chain.process]
    fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        let n = left.len().min(right.len());

        // Cache delay times in samples for accent/groove phase tracking
        let delay_time_samples_l = self.delay_l.time_ms * self.sample_rate / 1000.0;
        let delay_time_samples_r = self.delay_r.time_ms * self.sample_rate / 1000.0;

        // Base (user) delay times; per-sample modulation is passed via
        // tick_at without touching the public params.
        let base_time_l = self.delay_l.time_ms;
        let base_time_r = self.delay_r.time_ms;

        for i in 0..n {
            let dry_l = left[i];
            let dry_r = right[i];

            // --- Smoothed global params ---
            self.mix_smoother.set_target(self.mix);
            let mix = self.mix_smoother.tick();
            self.width_smoother.set_target(self.width);
            let width = self.width_smoother.tick();
            self.input_level_smoother.set_target(self.input_level);
            let input_level = self.input_level_smoother.tick();
            self.output_level_smoother.set_target(self.output_level);
            let output_level = self.output_level_smoother.tick();
            self.pingpong_fb_smoother.set_target(self.pingpong_feedback);
            let pingpong_feedback = self.pingpong_fb_smoother.tick();

            // --- Input level ---
            let scaled_l = dry_l * input_level;
            let scaled_r = dry_r * input_level;

            // --- Diffusion (loop mode: applied to input before delay) ---
            let (diff_in_l, diff_in_r) = if self.diffusion_in_loop && self.diffusion_enabled {
                (
                    self.diffuser_l.tick(scaled_l),
                    self.diffuser_r.tick(scaled_r),
                )
            } else {
                (scaled_l, scaled_r)
            };

            // Compute delay input based on stereo mode
            let (in_l, in_r) = match self.stereo_mode {
                StereoMode::Stereo => (diff_in_l, diff_in_r),
                StereoMode::Mono => {
                    let mono = (diff_in_l + diff_in_r) * 0.5;
                    (mono, mono)
                }
                StereoMode::PingPong => {
                    // In ping-pong, we feed mono to L and cross-feed from
                    // outputs; DC-block the recirculating cross-feed.
                    let mono = (diff_in_l + diff_in_r) * 0.5;
                    let fb_r = self.pingpong_dc_r.tick(self.delay_r.last_feedback());
                    let fb_l = self.pingpong_dc_l.tick(self.delay_l.last_feedback());
                    (mono + fb_r * pingpong_feedback, fb_l * pingpong_feedback)
                }
            };

            // --- Feel: offset delay time ---
            let feel_offset = self.feel * self.feel_max_ms;
            let mut time_l = base_time_l + feel_offset;
            let mut time_r = base_time_r + feel_offset;

            // --- Prime numbers: anti-resonance ---
            // 0.13% detune pushes repeats off exact integer-ratio spacing
            // so stacked delays don't comb-filter against each other.
            if self.prime_numbers {
                time_l *= 1.0 + 0.0013;
                time_r *= 1.0 + 0.0013;
            }

            // --- Groove: modulate delay time based on phase ---
            if self.groove.abs() > 1e-6 && delay_time_samples_l > 1.0 {
                // L channel groove
                if self.groove_phase_l > delay_time_samples_l * 0.5 {
                    time_l += self.groove * time_l * 0.333;
                }
                // R channel groove
                if self.groove_phase_r > delay_time_samples_r * 0.5 {
                    time_r += self.groove * time_r * 0.333;
                }
            }

            // Clamp to positive
            time_l = time_l.max(0.1);
            time_r = time_r.max(0.1);

            let mut wet_l = self.delay_l.tick_at(in_l, 0, time_l);
            let mut wet_r = self.delay_r.tick_at(in_r, 1, time_r);

            // --- Diffusion (post mode: applied after delay output) ---
            if !self.diffusion_in_loop && self.diffusion_enabled {
                wet_l = self.diffuser_l.tick(wet_l);
                wet_r = self.diffuser_r.tick(wet_r);
            }

            // --- Accent: alternating repeat volume ---
            if self.accent.abs() > 1e-6 && delay_time_samples_l > 1.0 {
                // L channel accent
                let accent_gain_l = 1.0 + self.accent * if self.accent_flip_l { 1.0 } else { -1.0 };
                wet_l *= accent_gain_l;

                // R channel accent
                let accent_gain_r = 1.0 + self.accent * if self.accent_flip_r { 1.0 } else { -1.0 };
                wet_r *= accent_gain_r;

                // Advance accent phases
                self.accent_phase_l += 1.0;
                if self.accent_phase_l >= delay_time_samples_l {
                    self.accent_phase_l -= delay_time_samples_l;
                    self.accent_flip_l = !self.accent_flip_l;
                }
                self.accent_phase_r += 1.0;
                if self.accent_phase_r >= delay_time_samples_r {
                    self.accent_phase_r -= delay_time_samples_r;
                    self.accent_flip_r = !self.accent_flip_r;
                }
            }

            // --- Groove phase tracking ---
            if delay_time_samples_l > 1.0 {
                self.groove_phase_l += 1.0;
                if self.groove_phase_l >= delay_time_samples_l {
                    self.groove_phase_l -= delay_time_samples_l;
                }
            }
            if delay_time_samples_r > 1.0 {
                self.groove_phase_r += 1.0;
                if self.groove_phase_r >= delay_time_samples_r {
                    self.groove_phase_r -= delay_time_samples_r;
                }
            }

            // Ducking
            if self.ducking_enabled {
                let input_level = (dry_l.abs() + dry_r.abs()) * 0.5;
                let duck_gain = self.ducker.tick(input_level);
                wet_l *= duck_gain;
                wet_r *= duck_gain;
            }

            // --- LR offset: delay the R channel ---
            if width > 0.001 {
                self.lr_offset_smoother.set_target(self.lr_offset_ms);
                let offset_ms = self.lr_offset_smoother.tick();
                let offset_samples = offset_ms * self.sample_rate / 1000.0;
                self.lr_offset_delay.write(wet_r);
                if offset_samples > 0.5 {
                    wet_r = self.lr_offset_delay.read_cubic(offset_samples);
                }
            }

            // Stereo width (mid-side)
            if (width - 1.0).abs() > 0.001 {
                let mid = (wet_l + wet_r) * 0.5;
                let side = (wet_l - wet_r) * 0.5;
                wet_l = mid + side * width;
                wet_r = mid - side * width;
            }

            // --- Output level ---
            wet_l *= output_level;
            wet_r *= output_level;

            // Mix dry/wet
            left[i] = dry_l * (1.0 - mix) + wet_l * mix;
            right[i] = dry_r * (1.0 - mix) + wet_r * mix;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 48000.0;

    fn config() -> AudioConfig {
        AudioConfig {
            sample_rate: SR,
            max_buffer_size: 512,
        }
    }

    fn make_chain() -> DelayChain {
        let mut c = DelayChain::new();
        c.delay_l.time_ms = 100.0;
        c.delay_r.time_ms = 100.0;
        c.delay_l.feedback = 0.0;
        c.delay_r.feedback = 0.0;
        c.mix = 1.0;
        c.update(config());
        c
    }

    #[test]
    fn dry_wet_mix() {
        let mut c = make_chain();
        c.mix = 0.0; // Fully dry
        c.reset(); // Snap smoothers to the new params

        let mut l = vec![0.5; 512];
        let mut r = vec![0.5; 512];
        c.process(&mut l, &mut r);

        // Should be unchanged
        assert!((l[0] - 0.5).abs() < 1e-10);
    }

    #[test]
    fn mix_change_is_smooth() {
        let mut c = make_chain();
        c.mix = 1.0;
        c.reset();

        // Steady dry signal, delay produces nothing yet (first 100ms)
        let mut l = vec![0.8; 1024];
        let mut r = vec![0.8; 1024];
        c.process(&mut l, &mut r);

        // Step mix hard from 1.0 to 0.0 mid-stream — output must ramp,
        // not jump, sample to sample.
        c.mix = 0.0;
        let mut l2 = vec![0.8; 4096];
        let mut r2 = vec![0.8; 4096];
        c.process(&mut l2, &mut r2);

        let mut prev = l[1023];
        let mut max_jump: f64 = 0.0;
        for &v in &l2 {
            max_jump = max_jump.max((v - prev).abs());
            prev = v;
        }
        // Unsoothed, the wet→dry step would jump by up to 0.8 in one
        // sample; smoothed it moves in small increments.
        assert!(
            max_jump < 0.05,
            "Mix step should be smoothed: max_jump={max_jump}"
        );
        // And it must actually settle to fully dry.
        assert!((l2[4095] - 0.8).abs() < 1e-6, "Should settle dry: {}", l2[4095]);
    }

    #[test]
    fn stereo_mode_works() {
        let mut c = make_chain();
        c.stereo_mode = StereoMode::Mono;
        c.delay_l.time_ms = 50.0;
        c.delay_r.time_ms = 50.0;
        c.update(config());

        // Send signal only to left
        let n = 9600;
        let mut l: Vec<f64> = (0..n).map(|i| if i < 100 { 1.0 } else { 0.0 }).collect();
        let mut r = vec![0.0; n];

        c.process(&mut l, &mut r);

        // In mono mode, right should also get output
        let r_energy: f64 = r.iter().map(|x| x * x).sum();
        assert!(r_energy > 0.01, "Mono mode should output to both channels");
    }

    #[test]
    fn no_nan_full_chain() {
        let mut c = DelayChain::new();
        c.delay_l.time_ms = 200.0;
        c.delay_r.time_ms = 250.0;
        c.delay_l.feedback = 0.6;
        c.delay_r.feedback = 0.6;
        c.delay_l.drive = 0.5;
        c.delay_r.drive = 0.5;
        c.delay_l.hicut_freq = 5000.0;
        c.delay_r.hicut_freq = 5000.0;
        c.delay_l.wow_depth = 0.3;
        c.delay_r.wow_depth = 0.3;
        c.delay_l.flutter_depth = 0.3;
        c.delay_r.flutter_depth = 0.3;
        c.diffusion_enabled = true;
        c.ducking_enabled = true;
        c.ducker.amount = 0.5;
        c.ducker.threshold = 0.1;
        c.mix = 0.5;
        c.update(config());

        let n = 48000;
        let mut l: Vec<f64> = (0..n)
            .map(|i| (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5)
            .collect();
        let mut r = l.clone();

        c.process(&mut l, &mut r);

        for (i, (&lv, &rv)) in l.iter().zip(r.iter()).enumerate() {
            assert!(lv.is_finite(), "L NaN at {i}");
            assert!(rv.is_finite(), "R NaN at {i}");
        }
    }

    #[test]
    fn ping_pong_produces_output_both_channels() {
        let mut c = DelayChain::new();
        c.delay_l.time_ms = 50.0;
        c.delay_r.time_ms = 50.0;
        c.delay_l.feedback = 0.3;
        c.delay_r.feedback = 0.3;
        c.stereo_mode = StereoMode::PingPong;
        c.pingpong_feedback = 0.6;
        c.mix = 1.0;
        c.update(config());

        // Send a burst in left channel
        let n = 19200;
        let mut l: Vec<f64> = (0..n).map(|i| if i < 100 { 0.8 } else { 0.0 }).collect();
        let mut r = vec![0.0; n];

        c.process(&mut l, &mut r);

        // Both channels should have output (ping-pong cross-feeds)
        let l_energy: f64 = l.iter().map(|x| x * x).sum();
        let r_energy: f64 = r.iter().map(|x| x * x).sum();

        assert!(l_energy > 0.01, "L should have output: {l_energy}");
        assert!(
            r_energy > 0.001,
            "R should have output from ping-pong cross-feed: {r_energy}"
        );
    }

    #[test]
    fn diffusion_smears_impulse() {
        let mut c_clean = make_chain();
        c_clean.diffusion_enabled = false;
        c_clean.update(config());

        let mut c_diff = make_chain();
        c_diff.diffusion_enabled = true;
        c_diff.diffusion_smear = 0.7;
        c_diff.diffusion_size = 0.5;
        c_diff.update(config());

        let n = 19200;
        let impulse: Vec<f64> = (0..n).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();

        let mut l_clean = impulse.clone();
        let mut r_clean = impulse.clone();
        c_clean.process(&mut l_clean, &mut r_clean);

        let mut l_diff = impulse.clone();
        let mut r_diff = impulse.clone();
        c_diff.process(&mut l_diff, &mut r_diff);

        // Count how many samples are above threshold — diffusion should spread energy
        let count_clean = l_clean.iter().filter(|x| x.abs() > 0.01).count();
        let count_diff = l_diff.iter().filter(|x| x.abs() > 0.01).count();

        assert!(
            count_diff > count_clean,
            "Diffusion should spread the impulse: clean={count_clean}, diff={count_diff}"
        );
    }

    #[test]
    fn width_control() {
        let mut c = make_chain();
        c.delay_l.time_ms = 50.0;
        c.delay_r.time_ms = 75.0; // Different times for L/R
        c.width = 2.0; // Extra wide
        c.update(config());

        let n = 9600;
        let mut l: Vec<f64> = (0..n)
            .map(|i| (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5)
            .collect();
        let mut r = l.clone();

        c.process(&mut l, &mut r);

        // With different delay times and width>1, L and R should differ
        let diff: f64 = l
            .iter()
            .zip(r.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / n as f64;
        assert!(
            diff > 0.001,
            "Width should create stereo difference: avg_diff={diff}"
        );
    }

    #[test]
    fn style_switching_works() {
        for i in 0..DelayStyle::COUNT {
            let style = DelayStyle::from_index(i);
            let mut c = DelayChain::new();
            c.set_style(style);
            c.delay_l.time_ms = 30.0;
            c.delay_r.time_ms = 30.0;
            c.delay_l.feedback = 0.3;
            c.delay_r.feedback = 0.3;
            c.head_mode = HeadMode::Mode4;
            c.mix = 1.0;
            c.update(config());

            let n = 48000;
            let mut l: Vec<f64> = (0..n).map(|s| if s < 100 { 0.8 } else { 0.0 }).collect();
            let mut r = l.clone();

            c.process(&mut l, &mut r);

            let energy: f64 = l.iter().map(|x| x * x).sum();
            assert!(
                energy > 0.001,
                "{:?} style should produce output in chain: energy={energy}",
                style
            );
        }
    }

    #[test]
    fn lr_offset_creates_stereo_difference() {
        let mut c = make_chain();
        c.delay_l.time_ms = 50.0;
        c.delay_r.time_ms = 50.0;
        c.lr_offset_ms = 10.0; // 10ms offset on R channel
        c.width = 1.0;
        c.mix = 1.0;
        c.update(config());

        let n = 19200;
        let mut l: Vec<f64> = (0..n).map(|i| if i < 100 { 0.8 } else { 0.0 }).collect();
        let mut r = l.clone();

        c.process(&mut l, &mut r);

        // Find the sample index of peak output in each channel (after the delay)
        let l_peak_idx = l
            .iter()
            .enumerate()
            .skip(2000) // skip past initial transient
            .max_by(|(_, a), (_, b)| a.abs().partial_cmp(&b.abs()).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        let r_peak_idx = r
            .iter()
            .enumerate()
            .skip(2000)
            .max_by(|(_, a), (_, b)| a.abs().partial_cmp(&b.abs()).unwrap())
            .map(|(i, _)| i)
            .unwrap();

        // The R channel peak should be offset from L channel peak
        // 10ms at 48kHz = 480 samples
        let idx_diff = (r_peak_idx as i64 - l_peak_idx as i64).unsigned_abs();

        // Also verify L and R differ in content
        let sample_diff: f64 = l
            .iter()
            .zip(r.iter())
            .skip(2000)
            .take(4800)
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>();

        assert!(
            sample_diff > 0.01 || idx_diff > 100,
            "LR offset should make channels differ: sample_diff={sample_diff}, idx_diff={idx_diff}"
        );
    }
}
