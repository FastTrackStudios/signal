//! Compressor DSP engine.
//!
//! Algorithm:
//! 1. Peak/RMS blended detection
//! 2. Gain curve: threshold/ratio/knee with range limiting
//! 3. Reference-informed smoothing with change detection:
//!    - Coefficients: attack_coeff, release_coeff, other_coeff
//!    - Change detection: 0.1% threshold
//! 4. Apply upward/expander stages, output gain, character drive, ceiling, and mix

pub mod biquad;
pub mod chain;
pub mod detector;
pub mod gain_curve;
pub mod hermite;
pub mod multiband;
pub mod smoother;
pub mod styles;

pub use biquad::{design_highpass_biquad, design_lowpass_biquad, BiquadFilter};
pub use chain::CompChain;
pub use detector::Detector;
pub use gain_curve::GainCurve;
pub use hermite::{HermiteCubicSmoother, StateFuncHypothesis};
pub use multiband::{CompressionBand, MultiBandCompressor};
pub use smoother::GainReductionSmoother;
pub use styles::{CompressionStyle, StyleCoefficients};

const PARAM_SMOOTHING_MS: f64 = 10.0;

/// Compressor core used by the plugin chain.
pub struct ProC3Compressor {
    detector: Detector,
    gain_curve: GainCurve,
    hermite_smoother: HermiteCubicSmoother,
    sample_rate: f64,
    last_gr_db: [f64; 2],
    last_gr_linear: [f64; 2],
    hold_remaining: [usize; 2],
    auto_makeup_db: [f64; 2],
    crest_peak_power: f64,
    crest_rms_power: f64,
    adaptive_attack_ms: f64,
    adaptive_release_ms: f64,
    smoothed_threshold_db: f64,
    smoothed_ratio: f64,
    smoothed_knee_db: f64,
    smoothed_input_gain_db: f64,
    smoothed_output_gain_db: f64,
    smoothed_fold: f64,
    smoothed_drive: f64,
    smoothed_detector_rms_mix: f64,
    smoothed_expander_threshold_db: f64,
    smoothed_expander_ratio: f64,
    smoothed_upward_threshold_db: f64,
    smoothed_upward_ratio: f64,
    smoothed_ceiling: f64,
    expander_gain_db: [f64; 2],
    upward_gain_db: [f64; 2],
    drive_previous_input: [f64; 2],
    bright_lowpass: [f64; 2],

    // Core parameters
    pub threshold_db: f64,
    pub ratio: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
    pub knee_db: f64,
    pub style: i32,

    // I/O parameters
    pub input_gain_db: f64,
    pub output_gain_db: f64,
    pub fold: f64,
    pub range_db: f64,
    pub expander_threshold_db: f64,
    pub expander_ratio: f64,
    pub upward_threshold_db: f64,
    pub upward_ratio: f64,
    pub drive: f64,
    pub character_mode: i32,

    // Parameters owned by the surrounding chain or auxiliary processing paths.
    pub hold_ms: f64,
    pub auto_makeup: bool,
    pub feedback: f64,
    pub channel_link: f64,
    pub detector_rms_mix: f64,
    pub inertia: f64,
    pub inertia_decay: f64,
    pub ceiling: f64,
}

impl ProC3Compressor {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            detector: Detector::new(),
            gain_curve: GainCurve::new(sample_rate),
            hermite_smoother: HermiteCubicSmoother::new(StateFuncHypothesis::Identity),
            sample_rate,
            last_gr_db: [0.0; 2],
            last_gr_linear: [1.0; 2],
            hold_remaining: [0; 2],
            auto_makeup_db: [0.0; 2],
            crest_peak_power: 0.0,
            crest_rms_power: 0.0,
            adaptive_attack_ms: 10.0,
            adaptive_release_ms: 50.0,
            smoothed_threshold_db: -20.0,
            smoothed_ratio: 4.0,
            smoothed_knee_db: 2.0,
            smoothed_input_gain_db: 0.0,
            smoothed_output_gain_db: 0.0,
            smoothed_fold: 1.0,
            smoothed_drive: 0.0,
            smoothed_detector_rms_mix: 0.0,
            smoothed_expander_threshold_db: -80.0,
            smoothed_expander_ratio: 1.0,
            smoothed_upward_threshold_db: -60.0,
            smoothed_upward_ratio: 1.0,
            smoothed_ceiling: 0.0,
            expander_gain_db: [0.0; 2],
            upward_gain_db: [0.0; 2],
            drive_previous_input: [0.0; 2],
            bright_lowpass: [0.0; 2],
            threshold_db: -20.0,
            ratio: 4.0,
            attack_ms: 10.0,
            release_ms: 50.0,
            knee_db: 2.0,
            style: 0,
            input_gain_db: 0.0,
            output_gain_db: 0.0,
            fold: 1.0,
            range_db: 60.0,
            expander_threshold_db: -80.0,
            expander_ratio: 1.0,
            upward_threshold_db: -60.0,
            upward_ratio: 1.0,
            drive: 0.0,
            character_mode: 0,
            hold_ms: 0.0,
            auto_makeup: false,
            feedback: 0.0,
            channel_link: 1.0,
            detector_rms_mix: 0.0,
            inertia: 0.0,
            inertia_decay: 0.0,
            ceiling: 0.0,
        }
    }

    /// Process a mono sample through the compressor core.
    pub fn process(&mut self, input: f64, channel: usize) -> f64 {
        // Step 0: Apply input gain
        self.smoothed_input_gain_db =
            self.smooth_parameter(self.smoothed_input_gain_db, self.input_gain_db);
        let input_linear = input * audiocore_dsp::db::db_to_linear(self.smoothed_input_gain_db);
        self.smoothed_detector_rms_mix =
            self.smooth_parameter(self.smoothed_detector_rms_mix, self.detector_rms_mix);

        // Step 1: detect level
        let level_db = self
            .detector
            .detect_level_with_rms_mix(input_linear.abs(), self.smoothed_detector_rms_mix);

        self.process_with_level(input_linear, level_db, channel)
    }

    /// Process a sample with an externally computed sidechain level.
    ///
    /// This lets the chain share a linked/filtered stereo detector while keeping
    /// per-channel gain smoothing and metering state.
    pub fn process_with_level(&mut self, input_linear: f64, level_db: f64, channel: usize) -> f64 {
        let channel = channel.min(1);
        self.smooth_auxiliary_params();

        // Step 2: COMPUTE GAIN REDUCTION
        // Apply threshold/ratio/knee in log domain
        self.smooth_gain_computer_params();
        self.update_program_dependent_ballistics(level_db);
        let mut gr_instant = self.gain_curve.compute_gr(level_db);

        // Hold freezes release for a short window after gain reduction deepens.
        // This keeps short gaps from pumping the detector while leaving attacks
        // fully responsive.
        if gr_instant < self.last_gr_linear[channel] {
            self.hold_remaining[channel] = self.hold_samples();
        } else if self.hold_remaining[channel] > 0 {
            gr_instant = self.last_gr_linear[channel];
            self.hold_remaining[channel] -= 1;
        }

        // Step 3: SMOOTH GAIN REDUCTION WITH HERMITE CUBIC
        // Reference-informed smoothing:
        // - Use attack/release coefficients
        // - Compare gr_inst with prior GR history values
        // - Detect change with a small relative threshold
        let log_rel = self.gain_curve.release_coeff.ln();
        let log_atk = self.gain_curve.attack_coeff.ln();
        let sqrt_h0 = gr_instant.sqrt();
        let sqrt_h1 = (gr_instant * 0.9).sqrt(); // Approximate for h1

        let gr_smoothed = self.hermite_smoother.process(
            gr_instant,
            self.gain_curve.attack_coeff,
            self.gain_curve.release_coeff,
            log_rel,
            log_atk,
            sqrt_h0,
            sqrt_h1,
            channel,
        );

        // Step 4: APPLY TO AUDIO
        let mut output = input_linear * gr_smoothed;
        output *= audiocore_dsp::db::db_to_linear(self.process_upward(level_db, channel));
        output *= audiocore_dsp::db::db_to_linear(self.process_expander(level_db, channel));

        // Step 5: OUTPUT GAIN
        let gr_db = -audiocore_dsp::db::linear_to_db(gr_smoothed.max(1e-10)).min(0.0);
        self.auto_makeup_db[channel] = if self.auto_makeup {
            // Use half of the current reduction for conservative gain matching.
            // Full compensation tends to over-brighten and overload transients.
            0.995 * self.auto_makeup_db[channel] + 0.005 * (gr_db * 0.5).min(24.0)
        } else {
            0.995 * self.auto_makeup_db[channel]
        };
        self.smoothed_output_gain_db =
            self.smooth_parameter(self.smoothed_output_gain_db, self.output_gain_db);
        let output_gain =
            audiocore_dsp::db::db_to_linear(self.smoothed_output_gain_db + self.auto_makeup_db[channel]);
        output *= output_gain;
        output = self.apply_drive(output, channel);

        // Step 6: SOFT CEILING (optional)
        if self.smoothed_ceiling > 0.0 {
            output = (output / self.smoothed_ceiling).tanh() * self.smoothed_ceiling;
        }

        // Step 7: PARALLEL COMPRESSION (fold parameter)
        let compressed = output;
        self.smoothed_fold = self.smooth_parameter(self.smoothed_fold, self.fold);
        output = compressed * self.smoothed_fold + input_linear * (1.0 - self.smoothed_fold);

        // Track GR for metering
        self.last_gr_linear[channel] = gr_smoothed;
        self.last_gr_db[channel] = gr_db;

        output
    }

    /// Update to new sample rate
    pub fn update(&mut self, sample_rate: f64) {
        if (sample_rate - self.sample_rate).abs() > 0.1 {
            self.sample_rate = sample_rate;
            self.detector.update_sample_rate(sample_rate);
            self.gain_curve = GainCurve::new(sample_rate);
            self.hermite_smoother.reset();
            // Re-apply current parameters
            self.set_threshold(self.threshold_db);
            self.set_ratio(self.ratio);
            self.set_knee(self.knee_db);
            self.set_attack_ms(self.attack_ms);
            self.set_release_ms(self.release_ms);
        }
    }

    /// Get current gain reduction in dB
    pub fn gain_reduction_db(&self) -> f64 {
        self.last_gr_db[0].max(self.last_gr_db[1])
    }

    /// Set threshold in dB
    pub fn set_threshold(&mut self, threshold_db: f64) {
        self.threshold_db = threshold_db;
        self.gain_curve.threshold_db = threshold_db;
    }

    /// Set ratio (e.g., 4.0 = 4:1)
    pub fn set_ratio(&mut self, ratio: f64) {
        self.ratio = ratio;
        self.gain_curve.ratio = ratio;
    }

    /// Set knee width in dB
    pub fn set_knee(&mut self, knee_db: f64) {
        self.knee_db = knee_db;
        self.gain_curve.knee_db = knee_db;
    }

    /// Set maximum gain-reduction range in dB.
    pub fn set_range_db(&mut self, range_db: f64) {
        self.range_db = range_db.clamp(0.0, 120.0);
        self.gain_curve.range_db = self.range_db;
    }

    fn hold_samples(&self) -> usize {
        (self.hold_ms.max(0.0) * self.sample_rate / 1000.0).round() as usize
    }

    fn smooth_parameter(&self, current: f64, target: f64) -> f64 {
        if (current - target).abs() <= 1e-12 {
            return target;
        }

        let samples = (self.sample_rate * PARAM_SMOOTHING_MS / 1000.0).max(1.0);
        let coeff = 1.0 - (-1.0 / samples).exp();
        current + (target - current) * coeff
    }

    fn smooth_gain_computer_params(&mut self) {
        self.smoothed_threshold_db =
            self.smooth_parameter(self.smoothed_threshold_db, self.threshold_db);
        self.smoothed_ratio = self.smooth_parameter(self.smoothed_ratio, self.ratio);
        self.smoothed_knee_db = self.smooth_parameter(self.smoothed_knee_db, self.knee_db);

        self.gain_curve.threshold_db = self.smoothed_threshold_db;
        self.gain_curve.ratio = self.smoothed_ratio;
        self.gain_curve.knee_db = self.smoothed_knee_db;
    }

    fn smooth_auxiliary_params(&mut self) {
        self.smoothed_expander_threshold_db = self.smooth_parameter(
            self.smoothed_expander_threshold_db,
            self.expander_threshold_db,
        );
        self.smoothed_expander_ratio =
            self.smooth_parameter(self.smoothed_expander_ratio, self.expander_ratio);
        self.smoothed_upward_threshold_db =
            self.smooth_parameter(self.smoothed_upward_threshold_db, self.upward_threshold_db);
        self.smoothed_upward_ratio =
            self.smooth_parameter(self.smoothed_upward_ratio, self.upward_ratio);
        self.smoothed_ceiling = self.smooth_ceiling_parameter(self.smoothed_ceiling, self.ceiling);
    }

    fn smooth_ceiling_parameter(&self, current: f64, target: f64) -> f64 {
        if target <= 1e-6 {
            return 0.0;
        }
        if current <= 1e-6 {
            return target;
        }
        self.smooth_parameter(current, target)
    }

    fn process_expander(&mut self, level_db: f64, channel: usize) -> f64 {
        let ratio = self.smoothed_expander_ratio.clamp(1.0, 20.0);
        let target_db = if ratio <= 1.0001 || level_db >= self.smoothed_expander_threshold_db {
            0.0
        } else {
            let below_threshold = self.smoothed_expander_threshold_db - level_db;
            (below_threshold * (1.0 - ratio)).max(-120.0)
        };

        let moving_deeper = target_db < self.expander_gain_db[channel];
        let time_ms = if moving_deeper {
            self.attack_ms.max(0.1)
        } else {
            self.release_ms.max(1.0)
        };
        let samples = (self.sample_rate * time_ms / 1000.0).max(1.0);
        let coeff = 1.0 - (-1.0 / samples).exp();
        self.expander_gain_db[channel] += (target_db - self.expander_gain_db[channel]) * coeff;
        self.expander_gain_db[channel]
    }

    fn process_upward(&mut self, level_db: f64, channel: usize) -> f64 {
        let ratio = self.smoothed_upward_ratio.clamp(1.0, 20.0);
        let target_db = if ratio <= 1.0001 || level_db >= self.smoothed_upward_threshold_db {
            0.0
        } else {
            let below_threshold = self.smoothed_upward_threshold_db - level_db;
            (below_threshold * (1.0 - 1.0 / ratio)).min(36.0)
        };

        let moving_louder = target_db > self.upward_gain_db[channel];
        let time_ms = if moving_louder {
            self.attack_ms.max(0.1)
        } else {
            self.release_ms.max(1.0)
        };
        let samples = (self.sample_rate * time_ms / 1000.0).max(1.0);
        let coeff = 1.0 - (-1.0 / samples).exp();
        self.upward_gain_db[channel] += (target_db - self.upward_gain_db[channel]) * coeff;
        self.upward_gain_db[channel]
    }

    fn apply_drive(&mut self, sample: f64, channel: usize) -> f64 {
        self.smoothed_drive = self.smooth_parameter(self.smoothed_drive, self.drive);
        let drive = self.smoothed_drive.clamp(0.0, 1.0);
        if drive <= 1e-6 {
            self.drive_previous_input[channel] = sample;
            return sample;
        }

        let pre_gain = 1.0 + drive * 11.0;
        let mode = self.character_mode.clamp(0, 6);
        if mode == 3 {
            return self.apply_bright_drive(sample, pre_gain, channel);
        }

        let previous = self.drive_previous_input[channel];
        self.drive_previous_input[channel] = sample;

        let normalization = Self::drive_transfer_raw(pre_gain, mode).abs().max(1e-9);
        let delta = sample - previous;

        if delta.abs() <= 1e-8 {
            return Self::drive_transfer(sample, pre_gain, normalization, mode);
        }

        (Self::drive_antiderivative(sample, pre_gain, normalization, mode)
            - Self::drive_antiderivative(previous, pre_gain, normalization, mode))
            / delta
    }

    fn apply_bright_drive(&mut self, sample: f64, pre_gain: f64, channel: usize) -> f64 {
        let cutoff_hz = 8_000.0;
        let coeff = 1.0 - (-2.0 * std::f64::consts::PI * cutoff_hz / self.sample_rate).exp();
        self.bright_lowpass[channel] += (sample - self.bright_lowpass[channel]) * coeff;

        let low = self.bright_lowpass[channel];
        let high = sample - low;
        let previous_high = self.drive_previous_input[channel];
        self.drive_previous_input[channel] = high;

        let normalization = Self::drive_transfer_raw(pre_gain, 0).abs().max(1e-9);
        let delta = high - previous_high;
        let saturated_high = if delta.abs() <= 1e-8 {
            Self::drive_transfer(high, pre_gain, normalization, 0)
        } else {
            (Self::drive_antiderivative(high, pre_gain, normalization, 0)
                - Self::drive_antiderivative(previous_high, pre_gain, normalization, 0))
                / delta
        };

        low + saturated_high
    }

    fn drive_transfer(sample: f64, pre_gain: f64, normalization: f64, mode: i32) -> f64 {
        Self::drive_transfer_raw(sample * pre_gain, mode) / normalization
    }

    fn drive_transfer_raw(x: f64, mode: i32) -> f64 {
        match mode {
            1 => x.atan(),
            2 => x / (1.0 + x.abs()),
            4 => cubic_drive_raw(x),
            5 => x.clamp(-1.0, 1.0),
            6 if x < 0.0 => 0.75 * x.tanh(),
            6 => x.tanh(),
            _ => x.tanh(),
        }
    }

    fn drive_antiderivative(sample: f64, pre_gain: f64, normalization: f64, mode: i32) -> f64 {
        let x = sample * pre_gain;
        let integral = match mode {
            1 => x * x.atan() - 0.5 * (1.0 + x * x).ln(),
            2 if x >= 0.0 => x - (1.0 + x).ln(),
            2 => -x - (1.0 - x).ln(),
            4 => cubic_drive_antiderivative_raw(x),
            5 => hard_clip_antiderivative_raw(x),
            6 if x < 0.0 => 0.75 * stable_log_cosh(x),
            6 => stable_log_cosh(x),
            _ => stable_log_cosh(x),
        };
        integral / (pre_gain * normalization)
    }

    fn update_program_dependent_ballistics(&mut self, level_db: f64) {
        let amount = self.inertia.abs().clamp(0.0, 1.0);
        if amount <= 1e-6 {
            return;
        }

        let amp = audiocore_dsp::db::db_to_linear(level_db).clamp(0.0, 16.0);
        let power = amp * amp;
        let coeff = (-1.0 / (self.sample_rate * 0.2)).exp();
        self.crest_peak_power = power.max(coeff * self.crest_peak_power + (1.0 - coeff) * power);
        self.crest_rms_power = coeff * self.crest_rms_power + (1.0 - coeff) * power;

        let crest = (self.crest_peak_power / self.crest_rms_power.max(1e-12)).clamp(1.0, 64.0);
        let transient = ((crest.sqrt() - 1.0) / 7.0).clamp(0.0, 1.0);

        let target_attack = self.attack_ms * (1.0 - amount * 0.85 * transient).clamp(0.05, 2.0);
        let release_shape = (1.0 + amount * (1.5 * transient - 0.4)).clamp(0.25, 4.0);
        let style_shape =
            Self::style_auto_release_multiplier(CompressionStyle::from_id(self.style), transient);
        let target_release = self.release_ms * (release_shape * style_shape).clamp(0.1, 6.0);

        let smoothing = self.inertia_decay.clamp(0.0, 0.999);
        self.adaptive_attack_ms =
            smoothing * self.adaptive_attack_ms + (1.0 - smoothing) * target_attack;
        self.adaptive_release_ms =
            smoothing * self.adaptive_release_ms + (1.0 - smoothing) * target_release;

        self.gain_curve
            .set_time_constants_ms(self.adaptive_attack_ms, self.adaptive_release_ms);
    }

    fn style_auto_release_multiplier(style: CompressionStyle, transient: f64) -> f64 {
        let transient = transient.clamp(0.0, 1.0);
        let sustained = 1.0 - transient;
        match style {
            CompressionStyle::Fet => 1.0 - 0.45 * transient,
            CompressionStyle::Vca => 1.0 - 0.2 * transient + 0.15 * sustained,
            CompressionStyle::Optical => 1.0 + 0.35 * transient + 0.9 * sustained,
            _ => 1.0,
        }
        .clamp(0.25, 2.5)
    }

    /// Set attack time in milliseconds
    pub fn set_attack_ms(&mut self, attack_ms: f64) {
        // CRITICAL: Only reset Hermite smoother if value ACTUALLY changed!
        // We're called every sample from sync_params, so must check before reset
        if (self.attack_ms - attack_ms).abs() > 1e-6 {
            self.hermite_smoother.reset();
        }
        self.attack_ms = attack_ms;
        self.adaptive_attack_ms = attack_ms.max(0.1);
        self.gain_curve.set_attack_ms(attack_ms);
    }

    /// Set release time in milliseconds
    pub fn set_release_ms(&mut self, release_ms: f64) {
        // CRITICAL: Only reset Hermite smoother if value ACTUALLY changed!
        // We're called every sample from sync_params, so must check before reset
        if (self.release_ms - release_ms).abs() > 1e-6 {
            self.hermite_smoother.reset();
        }
        self.release_ms = release_ms;
        self.adaptive_release_ms = release_ms.max(0.1);
        self.gain_curve.set_release_ms(release_ms);
    }

    /// Set attack time in seconds (for compatibility)
    pub fn set_attack(&mut self, attack_s: f64) {
        self.set_attack_ms(attack_s * 1000.0);
    }

    /// Set release time in seconds (for compatibility)
    pub fn set_release(&mut self, release_s: f64) {
        self.set_release_ms(release_s * 1000.0);
    }

    /// Set parallel compression fold parameter (0-1)
    pub fn set_fold(&mut self, fold: f64) {
        self.fold = fold.clamp(0.0, 1.0);
    }

    /// Set compression style (affects attack/release response)
    pub fn set_style(&mut self, style_id: i32) {
        let style = CompressionStyle::from_id(style_id);
        self.style = style.id();
        self.gain_curve.set_style(style);
    }

    /// Reset internal state
    pub fn reset(&mut self) {
        self.detector.reset();
        self.hermite_smoother.reset();
        self.last_gr_db = [0.0; 2];
        self.last_gr_linear = [1.0; 2];
        self.hold_remaining = [0; 2];
        self.auto_makeup_db = [0.0; 2];
        self.crest_peak_power = 0.0;
        self.crest_rms_power = 0.0;
        self.adaptive_attack_ms = self.attack_ms;
        self.adaptive_release_ms = self.release_ms;
        self.smoothed_threshold_db = self.threshold_db;
        self.smoothed_ratio = self.ratio;
        self.smoothed_knee_db = self.knee_db;
        self.smoothed_input_gain_db = self.input_gain_db;
        self.smoothed_output_gain_db = self.output_gain_db;
        self.smoothed_fold = self.fold;
        self.smoothed_drive = self.drive;
        self.smoothed_detector_rms_mix = self.detector_rms_mix;
        self.smoothed_expander_threshold_db = self.expander_threshold_db;
        self.smoothed_expander_ratio = self.expander_ratio;
        self.smoothed_upward_threshold_db = self.upward_threshold_db;
        self.smoothed_upward_ratio = self.upward_ratio;
        self.smoothed_ceiling = self.ceiling;
        self.expander_gain_db = [0.0; 2];
        self.upward_gain_db = [0.0; 2];
        self.drive_previous_input = [0.0; 2];
        self.bright_lowpass = [0.0; 2];
    }
}

fn stable_log_cosh(x: f64) -> f64 {
    let abs_x = x.abs();
    if abs_x > 20.0 {
        abs_x - std::f64::consts::LN_2
    } else {
        x.cosh().ln()
    }
}

fn cubic_drive_raw(x: f64) -> f64 {
    if x <= -1.0 {
        -2.0 / 3.0
    } else if x >= 1.0 {
        2.0 / 3.0
    } else {
        x - x.powi(3) / 3.0
    }
}

fn cubic_drive_antiderivative_raw(x: f64) -> f64 {
    if x <= -1.0 {
        (-2.0 / 3.0) * x - 0.25
    } else if x >= 1.0 {
        (2.0 / 3.0) * x - 0.25
    } else {
        0.5 * x * x - x.powi(4) / 12.0
    }
}

fn hard_clip_antiderivative_raw(x: f64) -> f64 {
    if x <= -1.0 {
        -x - 0.5
    } else if x >= 1.0 {
        x - 0.5
    } else {
        0.5 * x * x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quiet_signal_passes_through() {
        let mut comp = ProC3Compressor::new(48000.0);
        comp.set_threshold(0.0); // 0 dB threshold

        let quiet_input = 0.001; // Very quiet
        let output = comp.process(quiet_input, 0);

        // Quiet signal should pass through mostly unchanged
        assert!((output - quiet_input).abs() < 0.0001);
    }

    #[test]
    fn test_loud_signal_is_compressed() {
        let mut comp = ProC3Compressor::new(48000.0);
        comp.set_threshold(-18.0); // -18 dB threshold
        comp.set_ratio(4.0); // 4:1 ratio

        let loud_input = 0.5; // ~-6 dB
        let output = comp.process(loud_input, 0);

        // Loud signal should be compressed (reduced in amplitude)
        assert!(output.abs() < loud_input.abs());
    }

    #[test]
    fn test_hold_delays_release() {
        let mut no_hold = ProC3Compressor::new(48000.0);
        let mut with_hold = ProC3Compressor::new(48000.0);

        for comp in [&mut no_hold, &mut with_hold] {
            comp.set_threshold(-30.0);
            comp.set_ratio(8.0);
            comp.set_attack_ms(0.1);
            comp.set_release_ms(0.1);
        }
        with_hold.hold_ms = 50.0;

        for _ in 0..200 {
            no_hold.process(0.9, 0);
            with_hold.process(0.9, 0);
        }

        for _ in 0..200 {
            no_hold.process(0.001, 0);
            with_hold.process(0.001, 0);
        }

        assert!(
            with_hold.gain_reduction_db() > no_hold.gain_reduction_db(),
            "hold should keep more gain reduction during early release"
        );
    }

    #[test]
    fn test_auto_makeup_adds_bounded_gain() {
        let mut manual = ProC3Compressor::new(48000.0);
        let mut auto = ProC3Compressor::new(48000.0);

        for comp in [&mut manual, &mut auto] {
            comp.set_threshold(-30.0);
            comp.set_ratio(8.0);
            comp.set_attack_ms(0.1);
            comp.set_release_ms(50.0);
        }
        auto.auto_makeup = true;

        let mut manual_out = 0.0;
        let mut auto_out = 0.0;
        for _ in 0..2_000 {
            manual_out = manual.process(0.5, 0).abs();
            auto_out = auto.process(0.5, 0).abs();
        }

        assert!(
            auto_out > manual_out,
            "auto makeup should raise compressed output level"
        );
        assert!(
            auto.auto_makeup_db[0] <= 24.0,
            "auto makeup should remain bounded"
        );
    }

    #[test]
    fn test_inertia_adapts_ballistics_to_transients() {
        let mut comp = ProC3Compressor::new(48000.0);
        comp.set_attack_ms(20.0);
        comp.set_release_ms(100.0);
        comp.inertia = 1.0;
        comp.inertia_decay = 0.0;

        for n in 0..500 {
            let sample = if n % 50 == 0 { 1.0 } else { 0.001 };
            comp.process(sample, 0);
        }

        assert!(
            comp.adaptive_attack_ms < comp.attack_ms,
            "transient-rich material should shorten effective attack"
        );
        assert!(
            comp.adaptive_release_ms > comp.release_ms * 0.5,
            "adaptive release should remain musically bounded"
        );
    }

    #[test]
    fn test_zero_inertia_leaves_manual_ballistics() {
        let mut comp = ProC3Compressor::new(48000.0);
        comp.set_attack_ms(20.0);
        comp.set_release_ms(100.0);
        comp.inertia = 0.0;

        for n in 0..500 {
            let sample = if n % 50 == 0 { 1.0 } else { 0.001 };
            comp.process(sample, 0);
        }

        assert!((comp.adaptive_attack_ms - 20.0).abs() < 1e-9);
        assert!((comp.adaptive_release_ms - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_style_specific_auto_release_curves_diverge() {
        let mut releases = Vec::new();
        for style in [0, 1, 2, 3] {
            let mut comp = ProC3Compressor::new(48000.0);
            comp.set_attack_ms(20.0);
            comp.set_release_ms(100.0);
            comp.set_style(style);
            comp.inertia = 1.0;
            comp.inertia_decay = 0.0;

            for n in 0..500 {
                let sample = if n % 50 == 0 { 1.0 } else { 0.001 };
                comp.process(sample, 0);
            }
            releases.push(comp.adaptive_release_ms);
        }

        assert!(
            releases[1] < releases[0],
            "FET auto-release should recover faster than Clean on transient-rich material: {releases:?}"
        );
        assert!(
            releases[3] > releases[0],
            "Optical auto-release should hold longer than Clean on transient-rich material: {releases:?}"
        );
        assert!(
            (releases[2] - releases[0]).abs() > 1.0,
            "VCA auto-release should be distinct from Clean: {releases:?}"
        );
    }

    #[test]
    fn test_output_gain_automation_is_smoothed() {
        let mut comp = ProC3Compressor::new(48000.0);
        comp.set_threshold(0.0);

        for _ in 0..100 {
            comp.process(0.1, 0);
        }

        comp.output_gain_db = 24.0;
        let first = comp.process(0.1, 0).abs();
        let immediate = 0.1 * audiocore_dsp::db::db_to_linear(24.0);

        assert!(
            first < immediate * 0.25,
            "output gain should ramp instead of jumping to the target"
        );

        let mut settled = first;
        for _ in 0..5_000 {
            settled = comp.process(0.1, 0).abs();
        }

        assert!(
            (settled - immediate).abs() < immediate * 0.05,
            "output gain should settle near the target"
        );
    }

    #[test]
    fn test_gain_computer_automation_is_smoothed() {
        let mut comp = ProC3Compressor::new(48000.0);
        comp.set_threshold(0.0);
        comp.set_ratio(1.0);
        comp.set_knee(0.0);
        comp.reset();

        comp.set_threshold(-60.0);
        comp.set_ratio(20.0);
        comp.set_knee(72.0);
        comp.process(0.5, 0);

        assert!(
            comp.smoothed_threshold_db > -1.0,
            "threshold automation should ramp instead of jumping to the target"
        );
        assert!(
            comp.smoothed_ratio < 1.1,
            "ratio automation should ramp instead of jumping to the target"
        );
        assert!(
            comp.smoothed_knee_db < 1.0,
            "knee automation should ramp instead of jumping to the target"
        );

        for _ in 0..5_000 {
            comp.process(0.5, 0);
        }

        assert!((comp.smoothed_threshold_db + 60.0).abs() < 0.5);
        assert!((comp.smoothed_ratio - 20.0).abs() < 0.2);
        assert!((comp.smoothed_knee_db - 72.0).abs() < 0.6);
    }

    #[test]
    fn test_expander_reduces_signal_below_threshold() {
        let mut bypassed = ProC3Compressor::new(48000.0);
        let mut expanded = ProC3Compressor::new(48000.0);

        for comp in [&mut bypassed, &mut expanded] {
            comp.set_threshold(0.0);
            comp.set_attack_ms(0.1);
            comp.set_release_ms(10.0);
        }
        expanded.expander_threshold_db = -40.0;
        expanded.expander_ratio = 4.0;

        let mut bypassed_out = 0.0;
        let mut expanded_out = 0.0;
        for _ in 0..2_000 {
            bypassed_out = bypassed.process(0.001, 0).abs();
            expanded_out = expanded.process(0.001, 0).abs();
        }

        assert!(
            expanded_out < bypassed_out * 0.25,
            "expander should attenuate material below its threshold"
        );
    }

    #[test]
    fn test_upward_compression_lifts_signal_below_threshold() {
        let mut bypassed = ProC3Compressor::new(48000.0);
        let mut upward = ProC3Compressor::new(48000.0);

        for comp in [&mut bypassed, &mut upward] {
            comp.set_threshold(0.0);
            comp.set_attack_ms(0.1);
            comp.set_release_ms(10.0);
        }
        upward.upward_threshold_db = -40.0;
        upward.upward_ratio = 4.0;

        let mut bypassed_out = 0.0;
        let mut upward_out = 0.0;
        for _ in 0..2_000 {
            bypassed_out = bypassed.process(0.001, 0).abs();
            upward_out = upward.process(0.001, 0).abs();
        }

        assert!(
            upward_out > bypassed_out * 2.0,
            "upward compression should lift material below its threshold"
        );
    }

    #[test]
    fn test_drive_soft_clips_large_signal() {
        let mut clean = ProC3Compressor::new(48000.0);
        let mut driven = ProC3Compressor::new(48000.0);

        for comp in [&mut clean, &mut driven] {
            comp.set_threshold(0.0);
            comp.ceiling = 0.0;
        }
        driven.drive = 1.0;

        let clean_out = clean.process(2.0, 0).abs();
        let driven_out = driven.process(2.0, 0).abs();

        assert!(
            driven_out < clean_out,
            "drive should soft-clip large signals"
        );
        assert!(driven_out <= 1.05, "normalized drive should remain bounded");
    }

    #[test]
    fn test_drive_adaa_reduces_nyquist_alias_energy() {
        let mut comp = ProC3Compressor::new(48000.0);
        comp.drive = 1.0;
        comp.smoothed_drive = 1.0;

        let pre_gain = 1.0 + comp.drive * 11.0;
        let normalization = ProC3Compressor::drive_transfer_raw(pre_gain, 0)
            .abs()
            .max(1e-9);
        let memoryless = ProC3Compressor::drive_transfer(0.9, pre_gain, normalization, 0).abs()
            + ProC3Compressor::drive_transfer(-0.9, pre_gain, normalization, 0).abs();

        let mut adaa = 0.0;
        for n in 0..32 {
            let sample = if n % 2 == 0 { 0.9 } else { -0.9 };
            adaa = comp.apply_drive(sample, 0).abs();
        }

        assert!(
            adaa < memoryless * 0.1,
            "ADAA drive should strongly suppress alternating Nyquist input"
        );
    }

    #[test]
    fn test_character_modes_produce_distinct_saturation_curves() {
        let mut signatures = Vec::new();
        for mode in 0..=6 {
            let mut comp = ProC3Compressor::new(48000.0);
            comp.set_threshold(0.0);
            comp.drive = 1.0;
            comp.smoothed_drive = 1.0;
            comp.character_mode = mode;

            let mut positive = 0.0;
            for _ in 0..64 {
                positive = comp.apply_drive(0.05, 0);
            }
            comp.reset();
            comp.drive = 1.0;
            comp.smoothed_drive = 1.0;
            comp.character_mode = mode;

            let mut negative = 0.0;
            for _ in 0..64 {
                negative = comp.apply_drive(-0.05, 0);
            }
            signatures.push((positive, negative));
        }

        for pair in signatures.windows(2) {
            let distance = (pair[0].0 - pair[1].0).abs() + (pair[0].1 - pair[1].1).abs();
            assert!(
                distance > 1e-3,
                "adjacent character modes should produce distinct drive signatures: {pair:?}"
            );
        }
        assert!(
            signatures
                .iter()
                .all(|(positive, negative)| positive.is_finite() && negative.is_finite()),
            "all character modes should remain numerically stable"
        );
    }

    #[test]
    fn test_bright_character_preserves_low_frequency_body() {
        let mut full_band = ProC3Compressor::new(48000.0);
        let mut bright = ProC3Compressor::new(48000.0);

        for comp in [&mut full_band, &mut bright] {
            comp.drive = 1.0;
            comp.smoothed_drive = 1.0;
        }
        full_band.character_mode = 0;
        bright.character_mode = 3;

        let low = 0.8;
        let mut full_low = 0.0;
        let mut bright_low = 0.0;
        for _ in 0..4_800 {
            full_low = full_band.apply_drive(low, 0);
            bright_low = bright.apply_drive(low, 0);
        }

        assert!(
            (bright_low - low).abs() < (full_low - low).abs(),
            "bright mode should leave steady low-frequency body cleaner than full-band drive: bright={bright_low}, full={full_low}, input={low}"
        );
    }

    #[test]
    fn test_drive_automation_is_smoothed() {
        let mut comp = ProC3Compressor::new(48000.0);
        comp.set_threshold(0.0);

        for _ in 0..100 {
            comp.process(0.5, 0);
        }

        comp.drive = 1.0;
        comp.process(0.5, 0);

        assert!(
            comp.smoothed_drive < 0.01,
            "drive automation should ramp instead of jumping to the target"
        );

        for _ in 0..5_000 {
            comp.process(0.5, 0);
        }

        assert!(
            (comp.smoothed_drive - 1.0).abs() < 0.01,
            "drive automation should settle near the target"
        );
    }

    #[test]
    fn test_auxiliary_dynamics_automation_is_smoothed() {
        let mut comp = ProC3Compressor::new(48000.0);
        comp.reset();

        comp.detector_rms_mix = 1.0;
        comp.expander_threshold_db = -10.0;
        comp.expander_ratio = 20.0;
        comp.upward_threshold_db = -10.0;
        comp.upward_ratio = 20.0;
        comp.process(0.001, 0);

        assert!(
            comp.smoothed_detector_rms_mix < 0.01,
            "detector blend automation should ramp instead of jumping to the target"
        );
        assert!(
            comp.smoothed_expander_threshold_db < -79.0,
            "expander threshold automation should ramp from its previous value"
        );
        assert!(
            comp.smoothed_expander_ratio < 1.1,
            "expander ratio automation should ramp from its previous value"
        );
        assert!(
            comp.smoothed_upward_threshold_db < -59.0,
            "upward threshold automation should ramp from its previous value"
        );
        assert!(
            comp.smoothed_upward_ratio < 1.1,
            "upward ratio automation should ramp from its previous value"
        );

        for _ in 0..5_000 {
            comp.process(0.001, 0);
        }

        assert!((comp.smoothed_detector_rms_mix - 1.0).abs() < 0.01);
        assert!((comp.smoothed_expander_threshold_db + 10.0).abs() < 0.6);
        assert!((comp.smoothed_expander_ratio - 20.0).abs() < 0.2);
        assert!((comp.smoothed_upward_threshold_db + 10.0).abs() < 0.5);
        assert!((comp.smoothed_upward_ratio - 20.0).abs() < 0.2);
    }

    #[test]
    fn test_ceiling_automation_initializes_then_smooths() {
        let mut comp = ProC3Compressor::new(48000.0);
        comp.set_threshold(0.0);

        comp.ceiling = 1.0;
        comp.process(0.5, 0);
        assert_eq!(
            comp.smoothed_ceiling, 1.0,
            "enabling the ceiling should not ramp through near-zero values"
        );

        comp.ceiling = 0.1;
        comp.process(0.5, 0);
        assert!(
            comp.smoothed_ceiling > 0.99,
            "positive ceiling automation should ramp instead of jumping downward"
        );

        for _ in 0..5_000 {
            comp.process(0.5, 0);
        }

        assert!(
            (comp.smoothed_ceiling - 0.1).abs() < 0.01,
            "ceiling automation should settle near the target"
        );
    }
}
