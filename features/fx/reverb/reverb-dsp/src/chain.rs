//! Reverb chain — top-level processor with algorithm dispatch,
//! pre/post processing, mix, width, freeze, output EQ, ducker, saturation.

use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::{AudioConfig, Processor};
use crossbeam_channel::{Receiver, TryRecvError};

use crate::algorithm::{
    AlgorithmParams, AlgorithmType, ConvolutionModParams, IrSlot, ReverbAlgorithm,
};
use crate::algorithms;
use crate::ir::engine::ProcessedIr;
use crate::ir::prepared::PreparedIrPair;
use audiocore_dsp::envelope::EnvelopeFollower;
use audiocore_dsp::smoothing::ParamSmoother;

use crate::primitives::saturation::Saturator;
use crate::primitives::tilt_eq::TiltEq;

/// Full reverb processing chain.
///
/// Signal flow:
///   Input → Input HP/LP → Pre-Delay → Algorithm (with freeze override)
///        → Wet Saturation → Output Low/High-Cut → Tilt EQ
///        → Width → Ducker → Mix
pub struct ReverbChain {
    // Algorithm
    algorithm: Box<dyn ReverbAlgorithm>,
    algorithm_type: AlgorithmType,
    variant: usize,

    // Pre-delay (up to 500ms)
    predelay: DelayLine,
    predelay_samples: usize,

    // Input conditioning
    input_hp: Biquad,
    input_lp: Biquad,

    // Wet-bus saturation
    sat_l: Saturator,
    sat_r: Saturator,

    // Output EQ
    output_hp: Biquad,
    output_lp: Biquad,
    tilt_l: TiltEq,
    tilt_r: TiltEq,

    // Ducker (sidechain on dry input → gain reduction on wet)
    duck_env: EnvelopeFollower,
    duck_gain: f64,   // smoothed gain reduction (1.0 = no duck, 0.0 = full duck)
    duck_smooth: f64, // sample-rate-correct one-pole coeff for duck_gain

    // Per-sample smoothing of the zipper-prone global controls.
    mix_smoother: ParamSmoother,
    width_smoother: ParamSmoother,
    pan_smoother: ParamSmoother,
    trem_depth_smoother: ParamSmoother,

    // Wet-tremolo oscillator phase (radians).
    trem_phase: f64,

    // Coefficient-level params, ramped at sub-block rate (every
    // SMOOTH_BLOCK samples) so automation doesn't step filter/feedback
    // coefficients audibly. decay/damping re-enter the algorithm via
    // set_params (allocation-free for every algorithm); tilt/saturation
    // re-tune the output stage.
    decay_smoother: ParamSmoother,
    damping_smoother: ParamSmoother,
    tilt_smoother: ParamSmoother,
    sat_smoother: ParamSmoother,

    // Algorithm params
    pub params: AlgorithmParams,

    /// Convolution modulation options (motion / mod sources / IR morph).
    /// Only consumed while the Convolution algorithm is active; the
    /// smoothing (ramp on `update_params`, snap on `update`) lives
    /// inside the algorithm.
    pub conv_mod: ConvolutionModParams,

    // Global controls
    /// Pre-delay in milliseconds (0-500).
    pub predelay_ms: f64,
    /// Dry/wet mix (0.0 = fully dry, 1.0 = fully wet).
    pub mix: f64,
    /// Stereo width (0.0 = mono, 1.0 = normal, 2.0 = extra wide).
    pub width: f64,
    /// Wet-output pan (-1 left .. +1 right, 0 = centered). Equal-power,
    /// unity at center; a mid setting weights the field rather than
    /// muting the far side (BigSky MX per-slot pan).
    pub pan: f64,
    /// Wet tremolo rate in Hz (Flint-style trem on the reverb output;
    /// covers BigSky NonLinear Chop / TimeLine MX Reverb-machine trem).
    pub trem_rate_hz: f64,
    /// Wet tremolo depth (0 = off, 1 = full amplitude modulation).
    pub trem_depth: f64,
    /// Input highpass frequency in Hz (20 = off).
    pub input_hp_freq: f64,
    /// Input lowpass frequency in Hz (20000 = off).
    pub input_lp_freq: f64,
    /// Output highpass frequency in Hz (20 = off).
    pub output_hp_freq: f64,
    /// Output lowpass frequency in Hz (20000 = off).
    pub output_lp_freq: f64,
    /// Output tilt in dB (-12 = dark .. +12 = bright).
    pub output_tilt_db: f64,
    /// Output tilt pivot frequency in Hz.
    pub output_tilt_pivot: f64,
    /// Wet saturation amount (0.0 = clean, 1.0 = heavy).
    pub saturation: f64,
    /// Ducking depth (0.0 = no ducking, 1.0 = silence wet when dry plays).
    pub duck_amount: f64,
    /// Ducker threshold (linear, 0.0 ≈ -inf dB, 1.0 = full-scale).
    pub duck_threshold: f64,
    /// Ducker attack in ms.
    pub duck_attack_ms: f64,
    /// Ducker release in ms.
    pub duck_release_ms: f64,
    /// Freeze / infinite hold (kills new input, forces max feedback).
    pub freeze: bool,

    /// Receiver for processed IRs from a background loader. Drained at
    /// the top of each `process()` call; the most recent IR wins.
    /// Triggers an FFT precompute on the audio thread — fine for
    /// occasional swaps, not for sweeping.
    ir_swap_rx: Option<Receiver<ProcessedIr>>,

    /// Receiver for fully pre-FFT'd IRs. Audio-thread-safe — no FFT,
    /// just buffer moves. Preferred over `ir_swap_rx` when the plugin
    /// can do the precompute on a worker thread.
    prepared_ir_rx: Option<Receiver<PreparedIrPair>>,

    sample_rate: f64,
}

/// Sub-block length for coefficient-level parameter ramping. Smoothers
/// advance per-sample; coefficients are refreshed at this granularity
/// (0.67 ms at 48 kHz) while a ramp is in motion.
const SMOOTH_BLOCK: usize = 32;

impl ReverbChain {
    pub fn new() -> Self {
        let sample_rate = 48000.0;
        let max_predelay = (sample_rate * 0.5) as usize; // 500ms

        Self {
            algorithm: algorithms::create(AlgorithmType::Room, 0, sample_rate),
            algorithm_type: AlgorithmType::Room,
            variant: 0,
            predelay: DelayLine::new(max_predelay + 1),
            predelay_samples: 0,
            input_hp: Biquad::new(),
            input_lp: Biquad::new(),
            sat_l: Saturator::new(),
            sat_r: Saturator::new(),
            output_hp: Biquad::new(),
            output_lp: Biquad::new(),
            tilt_l: TiltEq::new(sample_rate),
            tilt_r: TiltEq::new(sample_rate),
            duck_env: EnvelopeFollower::new(0.0),
            duck_gain: 1.0,
            duck_smooth: EnvelopeFollower::coeff(0.001, sample_rate),
            mix_smoother: {
                let mut s = ParamSmoother::new(0.5);
                s.set_time_ms(5.0, sample_rate);
                s.set_epsilon(1e-4);
                s
            },
            width_smoother: {
                let mut s = ParamSmoother::new(1.0);
                s.set_time_ms(5.0, sample_rate);
                s.set_epsilon(1e-4);
                s
            },
            pan_smoother: {
                let mut s = ParamSmoother::new(0.0);
                s.set_time_ms(5.0, sample_rate);
                s.set_epsilon(1e-4);
                s
            },
            trem_depth_smoother: {
                let mut s = ParamSmoother::new(0.0);
                s.set_time_ms(10.0, sample_rate);
                s.set_epsilon(1e-4);
                s
            },
            trem_phase: 0.0,
            decay_smoother: {
                // Matches AlgorithmParams::default().decay
                let mut s = ParamSmoother::new(0.5);
                s.set_time_ms(30.0, sample_rate);
                s.set_epsilon(1e-4);
                s
            },
            damping_smoother: {
                // Matches AlgorithmParams::default().damping
                let mut s = ParamSmoother::new(0.3);
                s.set_time_ms(30.0, sample_rate);
                s.set_epsilon(1e-4);
                s
            },
            tilt_smoother: {
                let mut s = ParamSmoother::new(0.0);
                s.set_time_ms(30.0, sample_rate);
                s.set_epsilon(1e-3);
                s
            },
            sat_smoother: {
                let mut s = ParamSmoother::new(0.0);
                s.set_time_ms(10.0, sample_rate);
                s.set_epsilon(1e-4);
                s
            },
            params: AlgorithmParams::default(),
            conv_mod: ConvolutionModParams::default(),
            predelay_ms: 0.0,
            mix: 0.5,
            width: 1.0,
            pan: 0.0,
            trem_rate_hz: 4.0,
            trem_depth: 0.0,
            input_hp_freq: 20.0,
            input_lp_freq: 20000.0,
            output_hp_freq: 20.0,
            output_lp_freq: 20000.0,
            output_tilt_db: 0.0,
            output_tilt_pivot: 700.0,
            saturation: 0.0,
            duck_amount: 0.0,
            duck_threshold: 0.1,
            duck_attack_ms: 5.0,
            duck_release_ms: 120.0,
            freeze: false,
            ir_swap_rx: None,
            prepared_ir_rx: None,
            sample_rate,
        }
    }

    /// Attach a background IR loader's result channel. Whenever a new
    /// [`ProcessedIr`] arrives, it's pushed into the active algorithm
    /// via [`ReverbAlgorithm::try_load_ir`]. Algorithms that don't
    /// support IRs silently ignore it.
    pub fn set_ir_swap_receiver(&mut self, rx: Receiver<ProcessedIr>) {
        self.ir_swap_rx = Some(rx);
    }

    /// Attach a background loader's prepared-IR channel. Pre-FFT'd
    /// pairs arriving here are swapped in on the audio thread with no
    /// FFT cost — preferred over [`Self::set_ir_swap_receiver`] for
    /// glitch-free runtime IR changes.
    pub fn set_prepared_ir_receiver(&mut self, rx: Receiver<PreparedIrPair>) {
        self.prepared_ir_rx = Some(rx);
    }

    /// Synchronously load a stereo IR into the active algorithm.
    /// Returns `true` if the algorithm accepted it (i.e. Convolution).
    pub fn load_convolution_ir(&mut self, left: &[f64], right: &[f64]) -> bool {
        self.algorithm.try_load_ir(left, right)
    }

    /// Slot-addressed synchronous IR load (slot B feeds the morph's
    /// second convolver). Returns `true` if accepted.
    pub fn load_convolution_ir_slot(
        &mut self,
        left: &[f64],
        right: &[f64],
        slot: IrSlot,
    ) -> bool {
        self.algorithm.try_load_ir_slot(left, right, slot)
    }

    /// Synchronously swap in a pre-FFT'd IR pair. No FFT on the audio
    /// thread. Returns `true` if accepted.
    pub fn load_prepared_ir(&mut self, pair: PreparedIrPair) -> bool {
        self.algorithm.try_load_prepared_ir(pair)
    }

    /// Slot-addressed pre-FFT'd IR swap. Returns `true` if accepted.
    pub fn load_prepared_ir_slot(&mut self, pair: PreparedIrPair, slot: IrSlot) -> bool {
        self.algorithm.try_load_prepared_ir_slot(pair, slot)
    }

    /// Does the current algorithm accept IRs?
    pub fn algorithm_supports_ir(&self) -> bool {
        self.algorithm.supports_ir_loading()
    }

    /// Switch to a different algorithm type and/or variant. Resets algorithm state.
    pub fn set_algorithm(&mut self, algo: AlgorithmType) {
        self.set_algorithm_variant(algo, self.variant);
    }

    /// Switch to a specific algorithm type and variant.
    pub fn set_algorithm_variant(&mut self, algo: AlgorithmType, variant: usize) {
        let variant = variant.min(algo.variant_count().saturating_sub(1));
        if algo != self.algorithm_type || variant != self.variant {
            self.algorithm_type = algo;
            self.variant = variant;
            self.algorithm = algorithms::create(algo, variant, self.sample_rate);
            // Fresh algorithm state — land on the target params directly
            // instead of ramping from wherever the old algorithm was.
            let p = self.effective_params();
            self.decay_smoother.set_immediate(p.decay);
            self.damping_smoother.set_immediate(p.damping);
            self.algorithm.set_params(&p);
            // Fresh algorithm — convolution mod options land instantly.
            self.algorithm.set_conv_mod_params(&self.conv_mod, true);
        }
    }

    /// Set just the variant for the current algorithm type.
    pub fn set_variant(&mut self, variant: usize) {
        self.set_algorithm_variant(self.algorithm_type, variant);
    }

    /// Get the current algorithm type.
    pub fn algorithm_type(&self) -> AlgorithmType {
        self.algorithm_type
    }

    /// Get the current variant index.
    pub fn variant(&self) -> usize {
        self.variant
    }

    /// Compute params that get sent to the algorithm. Freeze forces decay=1.0
    /// and bumps damping toward neutral so the tail sustains.
    fn effective_params(&self) -> AlgorithmParams {
        let mut p = self.params;
        if self.freeze {
            p.decay = 1.0;
        }
        p
    }

    /// Update all algorithm parameters.
    ///
    /// This is the automation path: decay, damping, tilt, and saturation
    /// become smoother *targets* and ramp inside `process` (30/30/30/10 ms)
    /// instead of stepping coefficients instantly. Everything else (size,
    /// diffusion, modulation, tone, ...) still applies immediately.
    /// For an instant snap (preset load), call `update()` instead.
    pub fn update_params(&mut self) {
        let mut p = self.effective_params();
        self.decay_smoother.set_target(p.decay);
        self.damping_smoother.set_target(p.damping);
        // Push non-smoothed params now, holding decay/damping at their
        // current ramp position so this call never steps them.
        p.decay = self.decay_smoother.value();
        p.damping = self.damping_smoother.value();
        self.algorithm.set_params(&p);

        self.sat_smoother.set_target(self.saturation);
        self.tilt_smoother.set_target(self.output_tilt_db);
        self.tilt_l.set_pivot(self.output_tilt_pivot);
        self.tilt_r.set_pivot(self.output_tilt_pivot);
        self.duck_env
            .set_times_ms(self.duck_attack_ms, self.duck_release_ms, self.sample_rate);
        // Convolution mod options ride the automation path too (the
        // algorithm ramps its own smoothers). No-op outside Convolution.
        self.algorithm.set_conv_mod_params(&self.conv_mod, false);
    }
}

impl Processor for ReverbChain {
    fn reset(&mut self) {
        self.algorithm.reset();
        self.predelay.clear();
        self.input_hp.reset();
        self.input_lp.reset();
        self.output_hp.reset();
        self.output_lp.reset();
        self.tilt_l.reset();
        self.tilt_r.reset();
        self.duck_env.reset(0.0);
        self.duck_gain = 1.0;
        self.trem_phase = 0.0;
    }

    fn update(&mut self, config: AudioConfig) {
        self.sample_rate = config.sample_rate;

        let max_predelay = (config.sample_rate * 0.5) as usize;
        self.predelay = DelayLine::new(max_predelay + 1);
        self.predelay_samples = (self.predelay_ms * 0.001 * config.sample_rate) as usize;

        self.input_hp.set(
            FilterType::Highpass,
            self.input_hp_freq.max(20.0),
            0.707,
            config.sample_rate,
        );
        self.input_lp.set(
            FilterType::Lowpass,
            self.input_lp_freq.min(20000.0),
            0.707,
            config.sample_rate,
        );
        self.output_hp.set(
            FilterType::Highpass,
            self.output_hp_freq.max(20.0),
            0.707,
            config.sample_rate,
        );
        self.output_lp.set(
            FilterType::Lowpass,
            self.output_lp_freq.min(20000.0),
            0.707,
            config.sample_rate,
        );
        self.tilt_l.set_sample_rate(config.sample_rate);
        self.tilt_r.set_sample_rate(config.sample_rate);
        self.tilt_l.set_pivot(self.output_tilt_pivot);
        self.tilt_l.set_tilt_db(self.output_tilt_db);
        self.tilt_r.set_pivot(self.output_tilt_pivot);
        self.tilt_r.set_tilt_db(self.output_tilt_db);

        self.duck_env.set_times_ms(
            self.duck_attack_ms,
            self.duck_release_ms,
            config.sample_rate,
        );
        // ~1ms duck-gain smoothing, independent of sample rate (the old
        // hard-coded 0.25 step made duck timing vary with the SR).
        self.duck_smooth = EnvelopeFollower::coeff(0.001, config.sample_rate);
        self.mix_smoother.set_time_ms(5.0, config.sample_rate);
        self.width_smoother.set_time_ms(5.0, config.sample_rate);
        self.pan_smoother.set_time_ms(5.0, config.sample_rate);
        self.trem_depth_smoother.set_time_ms(10.0, config.sample_rate);
        self.decay_smoother.set_time_ms(30.0, config.sample_rate);
        self.damping_smoother.set_time_ms(30.0, config.sample_rate);
        self.tilt_smoother.set_time_ms(30.0, config.sample_rate);
        self.sat_smoother.set_time_ms(10.0, config.sample_rate);
        // update() is a reconfiguration point, not an automation path —
        // land on the new values instantly instead of ramping.
        self.mix_smoother.set_immediate(self.mix);
        self.width_smoother.set_immediate(self.width);
        self.pan_smoother.set_immediate(self.pan);
        self.trem_depth_smoother.set_immediate(self.trem_depth);
        let p = self.effective_params();
        self.decay_smoother.set_immediate(p.decay);
        self.damping_smoother.set_immediate(p.damping);
        self.tilt_smoother.set_immediate(self.output_tilt_db);
        self.sat_smoother.set_immediate(self.saturation);

        self.sat_l.set_drive(self.saturation);
        self.sat_r.set_drive(self.saturation);

        self.algorithm.set_sample_rate(config.sample_rate);
        self.algorithm.set_params(&p);
        // Reconfiguration point — convolution mod options snap.
        self.algorithm.set_conv_mod_params(&self.conv_mod, true);
    }

    fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        let n = left.len().min(right.len());

        // Prepared (pre-FFT'd) IRs — preferred path. No FFT cost here.
        if let Some(rx) = self.prepared_ir_rx.as_ref() {
            let mut latest: Option<PreparedIrPair> = None;
            loop {
                match rx.try_recv() {
                    Ok(pair) => latest = Some(pair),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.prepared_ir_rx = None;
                        break;
                    }
                }
            }
            if let Some(pair) = latest {
                self.algorithm.try_load_prepared_ir(pair);
            }
        }

        // Drain pending raw f64 IRs — last-one-wins per slot so
        // back-to-back loads don't each trigger an FFT precompute.
        // Latency is one audio block. This path DOES run FFTs on the
        // audio thread.
        if let Some(rx) = self.ir_swap_rx.as_ref() {
            let mut latest_a: Option<ProcessedIr> = None;
            let mut latest_b: Option<ProcessedIr> = None;
            loop {
                match rx.try_recv() {
                    Ok(ir) => match ir.slot {
                        IrSlot::A => latest_a = Some(ir),
                        IrSlot::B => latest_b = Some(ir),
                    },
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.ir_swap_rx = None;
                        break;
                    }
                }
            }
            for ir in [latest_a, latest_b].into_iter().flatten() {
                self.algorithm.try_load_ir_slot(&ir.left, &ir.right, ir.slot);
            }
        }

        self.predelay_samples = (self.predelay_ms * 0.001 * self.sample_rate) as usize;
        let duck_thresh = self.duck_threshold.max(1.0e-6);
        self.mix_smoother.set_target(self.mix);
        self.width_smoother.set_target(self.width);
        self.pan_smoother.set_target(self.pan);
        self.trem_depth_smoother.set_target(self.trem_depth);
        let trem_inc = 2.0 * std::f64::consts::PI * self.trem_rate_hz.clamp(0.1, 12.0)
            / self.sample_rate;

        let mut block_start = 0;
        while block_start < n {
            let block_end = (block_start + SMOOTH_BLOCK).min(n);

            // Refresh coefficient-level params only while a ramp is in
            // motion; settled smoothers cost one comparison per sub-block.
            if !self.decay_smoother.is_settled() || !self.damping_smoother.is_settled() {
                let mut p = self.effective_params();
                p.decay = self.decay_smoother.value();
                p.damping = self.damping_smoother.value();
                self.algorithm.set_params(&p);
            }
            if !self.tilt_smoother.is_settled() {
                let tilt = self.tilt_smoother.value();
                self.tilt_l.set_tilt_db(tilt);
                self.tilt_r.set_tilt_db(tilt);
            }
            if !self.sat_smoother.is_settled() {
                let drive = self.sat_smoother.value();
                self.sat_l.set_drive(drive);
                self.sat_r.set_drive(drive);
            }

            for i in block_start..block_end {
                // Advance the coefficient ramps per-sample so their rate is
                // independent of buffer/sub-block size.
                self.decay_smoother.tick();
                self.damping_smoother.tick();
                self.tilt_smoother.tick();
                self.sat_smoother.tick();
                let dry_l = left[i];
                let dry_r = right[i];
                let mix = self.mix_smoother.tick();
                let width = self.width_smoother.tick();

                // Sidechain envelope from dry sum (rectified — the follower is
                // domain-agnostic and needs |x|).
                let env = self.duck_env.tick((dry_l + dry_r).abs());
                let over = (env / duck_thresh - 1.0).max(0.0);
                let target_duck = 1.0 - (over.min(1.0) * self.duck_amount);
                // 1-pole smooth toward target_duck (independent of attack/release
                // — env already shapes the rate).
                self.duck_gain = self.duck_smooth * (self.duck_gain - target_duck) + target_duck;

                // Input filtering
                let filt_l = self.input_lp.tick(self.input_hp.tick(dry_l, 0), 0);
                let filt_r = self.input_lp.tick(self.input_hp.tick(dry_r, 1), 1);

                // Freeze: kill input to algorithm but keep feedback running
                let (alg_in_l, alg_in_r) = if self.freeze {
                    (0.0, 0.0)
                } else if self.predelay_samples > 0 {
                    self.predelay.write(filt_l);
                    let delayed = self.predelay.read(self.predelay_samples);
                    (delayed, filt_r)
                } else {
                    (filt_l, filt_r)
                };

                // Algorithm
                let (mut wet_l, mut wet_r) = self.algorithm.tick(alg_in_l, alg_in_r);

                // Wet saturation (gate on the ramped drive, not the raw param,
                // so a saturation fade-out stays engaged until it lands on 0)
                if self.sat_l.drive() > 0.0 {
                    wet_l = self.sat_l.tick(wet_l);
                    wet_r = self.sat_r.tick(wet_r);
                }

                // Output band-shaping
                wet_l = self.output_hp.tick(wet_l, 0);
                wet_l = self.output_lp.tick(wet_l, 0);
                wet_r = self.output_hp.tick(wet_r, 1);
                wet_r = self.output_lp.tick(wet_r, 1);

                // Tilt EQ (gate on the ramped tilt for the same reason)
                if self.tilt_smoother.value().abs() > 0.01 {
                    wet_l = self.tilt_l.tick(wet_l);
                    wet_r = self.tilt_r.tick(wet_r);
                }

                // Width (mid-side)
                let (mut final_l, mut final_r) = if (width - 1.0).abs() > 0.001 {
                    let mid = (wet_l + wet_r) * 0.5;
                    let side = (wet_l - wet_r) * 0.5;
                    (mid + side * width, mid - side * width)
                } else {
                    (wet_l, wet_r)
                };

                // Wet pan (equal-power, unity at center — same law as
                // delay's pan_gains; a mid setting weights the field,
                // leaving decay audible on the far side).
                let pan = self.pan_smoother.tick();
                if pan.abs() > 1e-6 {
                    let theta = (pan.clamp(-1.0, 1.0) + 1.0) * std::f64::consts::FRAC_PI_4;
                    final_l *= std::f64::consts::SQRT_2 * theta.cos();
                    final_r *= std::f64::consts::SQRT_2 * theta.sin();
                }

                // Wet tremolo (sine, wet path only). Phase advances only
                // while the ramped depth is non-zero, so depth 0 stays
                // bit-identical and re-engaging starts at full gain.
                let trem_depth = self.trem_depth_smoother.tick();
                if trem_depth > 1e-6 {
                    let g = 1.0 - trem_depth * 0.5 * (1.0 - self.trem_phase.cos());
                    final_l *= g;
                    final_r *= g;
                    self.trem_phase += trem_inc;
                    if self.trem_phase > 2.0 * std::f64::consts::PI {
                        self.trem_phase -= 2.0 * std::f64::consts::PI;
                    }
                }

                // Ducker
                if self.duck_amount > 0.0 {
                    final_l *= self.duck_gain;
                    final_r *= self.duck_gain;
                }

                // Mix
                left[i] = dry_l * (1.0 - mix) + final_l * mix;
                right[i] = dry_r * (1.0 - mix) + final_r * mix;
            }

            block_start = block_end;
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

    #[test]
    fn dry_wet_mix() {
        let mut c = ReverbChain::new();
        c.mix = 0.0;
        c.update(config());

        let mut l = vec![0.5; 512];
        let mut r = vec![0.5; 512];
        c.process(&mut l, &mut r);

        assert!((l[0] - 0.5).abs() < 1e-10, "Dry pass-through");
    }

    #[test]
    fn wet_produces_output() {
        let mut c = ReverbChain::new();
        c.mix = 1.0;
        c.update(config());

        let n = 4800;
        let mut l: Vec<f64> = (0..n).map(|i| if i < 10 { 1.0 } else { 0.0 }).collect();
        let mut r = l.clone();

        c.process(&mut l, &mut r);

        let late_energy: f64 = l[100..].iter().map(|x| x * x).sum();
        assert!(
            late_energy > 0.001,
            "Reverb should produce a tail: {late_energy}"
        );
    }

    #[test]
    fn all_algorithms_no_nan() {
        for &algo in AlgorithmType::ALL {
            for variant in 0..algo.variant_count() {
                let mut c = ReverbChain::new();
                c.set_algorithm_variant(algo, variant);
                c.mix = 1.0;
                c.update(config());

                let n = 4800;
                let mut l: Vec<f64> = (0..n)
                    .map(|i| (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5)
                    .collect();
                let mut r = l.clone();

                c.process(&mut l, &mut r);

                let vname = algo.variant_name(variant);
                for (i, (&lv, &rv)) in l.iter().zip(r.iter()).enumerate() {
                    assert!(
                        lv.is_finite(),
                        "{} {}: L NaN/Inf at sample {i}: {lv}",
                        algo.name(),
                        vname,
                    );
                    assert!(
                        rv.is_finite(),
                        "{} {}: R NaN/Inf at sample {i}: {rv}",
                        algo.name(),
                        vname,
                    );
                }
            }
        }
    }

    #[test]
    fn algorithm_switching() {
        let mut c = ReverbChain::new();
        c.update(config());

        for &algo in AlgorithmType::ALL {
            for variant in 0..algo.variant_count() {
                c.set_algorithm_variant(algo, variant);
                assert_eq!(c.algorithm_type(), algo);
                assert_eq!(c.variant(), variant);

                let mut l = vec![0.1; 128];
                let mut r = vec![0.1; 128];
                c.process(&mut l, &mut r);
            }
        }
    }

    #[test]
    fn predelay_delays_signal() {
        let mut c = ReverbChain::new();
        c.mix = 1.0;
        c.predelay_ms = 10.0;
        c.update(config());

        let n = 2400;
        let mut l: Vec<f64> = (0..n).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();
        let mut r = l.clone();

        c.process(&mut l, &mut r);

        let early_energy: f64 = l[..400].iter().map(|x| x * x).sum();
        let late_energy: f64 = l[400..].iter().map(|x| x * x).sum();

        assert!(
            late_energy > early_energy,
            "Predelay should shift energy later: early={early_energy}, late={late_energy}"
        );
    }

    #[test]
    fn freeze_sustains_after_input_stops() {
        let mut c = ReverbChain::new();
        c.mix = 1.0;
        c.set_algorithm(AlgorithmType::Hall);
        c.update(config());

        // Excite for ~50ms, then freeze with input=0
        let mut l: Vec<f64> = (0..2400)
            .map(|i| (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5)
            .collect();
        let mut r = l.clone();
        c.process(&mut l, &mut r);

        c.freeze = true;
        c.update_params();

        // Now feed silence for 2 seconds, then check tail still has energy.
        let mut l2 = vec![0.0; (SR as usize) * 2];
        let mut r2 = l2.clone();
        c.process(&mut l2, &mut r2);

        let tail_energy: f64 = l2[(l2.len() - 1000)..].iter().map(|x| x * x).sum();
        assert!(
            tail_energy > 1e-6,
            "Freeze should sustain tail: {tail_energy}"
        );
    }

    #[test]
    fn saturation_affects_output() {
        let mut c = ReverbChain::new();
        c.mix = 1.0;
        c.update(config());

        let make_input = || -> (Vec<f64>, Vec<f64>) {
            let l: Vec<f64> = (0..2400).map(|i| if i < 50 { 1.0 } else { 0.0 }).collect();
            let r = l.clone();
            (l, r)
        };

        let (mut l1, mut r1) = make_input();
        c.process(&mut l1, &mut r1);

        let mut c2 = ReverbChain::new();
        c2.mix = 1.0;
        c2.saturation = 1.0;
        c2.update(config());
        let (mut l2, mut r2) = make_input();
        c2.process(&mut l2, &mut r2);

        // Some sample should differ.
        let diff: f64 = l1.iter().zip(l2.iter()).map(|(a, b)| (a - b).abs()).sum();
        assert!(diff > 1e-3, "Saturation should change output: {diff}");
    }

    #[test]
    fn ducker_reduces_wet_when_dry_plays() {
        let mut c = ReverbChain::new();
        c.mix = 1.0;
        c.duck_amount = 1.0;
        c.duck_threshold = 0.05;
        c.duck_attack_ms = 1.0;
        c.duck_release_ms = 50.0;
        c.update(config());

        // Loud sustained input → ducker should pull wet output down.
        // mix=1.0 means output = wet alone, so |output| measures wet level.
        let n = 9600;
        let mut l: Vec<f64> = (0..n)
            .map(|i| (2.0 * PI * 200.0 * i as f64 / SR).sin() * 0.6)
            .collect();
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        let ducked: f64 = l[4800..].iter().map(|x| x * x).sum();

        let mut c2 = ReverbChain::new();
        c2.mix = 1.0;
        c2.duck_amount = 0.0;
        c2.update(config());
        let mut l2: Vec<f64> = (0..n)
            .map(|i| (2.0 * PI * 200.0 * i as f64 / SR).sin() * 0.6)
            .collect();
        let mut r2 = l2.clone();
        c2.process(&mut l2, &mut r2);
        let undccked: f64 = l2[4800..].iter().map(|x| x * x).sum();

        assert!(
            ducked < undccked,
            "Ducking should reduce wet level: ducked={ducked}, no_duck={undccked}"
        );
    }

    #[test]
    fn ir_hot_swap_changes_convolution_output() {
        use crate::ir::engine::ProcessedIr;
        use crossbeam_channel::unbounded;

        let mut c = ReverbChain::new();
        c.set_algorithm(AlgorithmType::Convolution);
        c.mix = 1.0;
        c.update(config());

        // Hammer the chain with a known input, get baseline (default synth IR).
        let make_input = || -> (Vec<f64>, Vec<f64>) {
            let l: Vec<f64> = (0..4800).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();
            (l.clone(), l)
        };
        let (mut l1, mut r1) = make_input();
        c.process(&mut l1, &mut r1);
        let baseline_sum: f64 = l1.iter().map(|x| x.abs()).sum();

        // Wire up swap channel and shove a custom IR (single positive spike).
        let (tx, rx) = unbounded::<ProcessedIr>();
        c.set_ir_swap_receiver(rx);
        let ir_l = vec![0.5; 1000];
        let ir_r = vec![0.5; 1000];
        tx.send(ProcessedIr {
            left: ir_l,
            right: ir_r,
            sample_rate: SR,
            source_frames: 1000,
            source_channels: 2,
            slot: IrSlot::A,
        })
        .unwrap();

        let (mut l2, mut r2) = make_input();
        c.process(&mut l2, &mut r2);
        let swapped_sum: f64 = l2.iter().map(|x| x.abs()).sum();

        // Different IR → different total energy in the response.
        assert!(
            (baseline_sum - swapped_sum).abs() > 1e-3,
            "Hot-swap should change Convolution output: baseline={baseline_sum} swapped={swapped_sum}"
        );
    }

    #[test]
    fn prepared_ir_hot_swap_no_fft_on_audio_thread() {
        use crate::ir::prepared::PreparedIrPair;
        use crossbeam_channel::unbounded;

        let mut c = ReverbChain::new();
        c.set_algorithm(AlgorithmType::Convolution);
        c.mix = 1.0;
        c.update(config());

        // Baseline: default synthetic IR, click input.
        let make_input = || -> (Vec<f64>, Vec<f64>) {
            let l: Vec<f64> = (0..4800).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();
            (l.clone(), l)
        };
        let (mut l1, mut r1) = make_input();
        c.process(&mut l1, &mut r1);
        let baseline_energy: f64 = l1.iter().map(|x| x * x).sum();

        // Build a prepared IR off-thread (test thread substituting for
        // the worker) and ship it via the prepared channel.
        let (tx, rx) = unbounded::<PreparedIrPair>();
        c.set_prepared_ir_receiver(rx);

        let custom_l: Vec<f64> = (0..2000).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();
        let custom_r = custom_l.clone();
        let pair = PreparedIrPair::build(&custom_l, &custom_r);
        // Make sure the partition count is non-trivial.
        assert!(pair.left.num_partitions() >= 2);
        tx.send(pair).unwrap();

        let (mut l2, mut r2) = make_input();
        c.process(&mut l2, &mut r2);
        let swapped_energy: f64 = l2.iter().map(|x| x * x).sum();

        assert!(
            (baseline_energy - swapped_energy).abs() > 1e-6,
            "Prepared swap should change output: base={baseline_energy} swap={swapped_energy}"
        );
    }

    #[test]
    fn ir_loader_supports_check() {
        let mut c = ReverbChain::new();
        c.set_algorithm(AlgorithmType::Hall);
        c.update(config());
        assert!(!c.algorithm_supports_ir());

        c.set_algorithm(AlgorithmType::Convolution);
        assert!(c.algorithm_supports_ir());
    }

    /// Max adjacent-sample delta over a slice — a proxy for clicks.
    fn max_step(x: &[f64]) -> f64 {
        x.windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f64::max)
    }

    /// Drive a chain with a steady sine, step `mutate` mid-stream via
    /// update_params, and assert the post-step region has no adjacent-
    /// sample jump grossly larger than the pre-step region's natural
    /// slope (i.e. the ramp, not the step, reaches the audio).
    fn assert_step_click_free(
        freq: f64,
        setup: impl Fn(&mut ReverbChain),
        mutate: impl Fn(&mut ReverbChain),
    ) {
        let mut c = ReverbChain::new();
        c.set_algorithm(AlgorithmType::Hall);
        c.mix = 1.0;
        setup(&mut c);
        c.update(config());

        let n = 9600;
        let sine = |off: usize| -> Vec<f64> {
            (0..n)
                .map(|i| (2.0 * PI * freq * (off + i) as f64 / SR).sin() * 0.5)
                .collect()
        };

        let mut l1 = sine(0);
        let mut r1 = l1.clone();
        c.process(&mut l1, &mut r1);
        // Natural signal slope, measured after the reverb has built up.
        let before = max_step(&l1[4800..]);

        mutate(&mut c);
        c.update_params();

        let mut l2 = sine(n);
        let mut r2 = l2.clone();
        c.process(&mut l2, &mut r2);
        // The 30 ms ramp lives inside the first ~1440 samples + margin.
        let after = max_step(&l2[..2400]);

        for &v in l2.iter() {
            assert!(v.is_finite());
        }
        assert!(
            after < before * 3.0 + 0.02,
            "param step should be ramped, not stepped: before={before}, after={after}"
        );
    }

    #[test]
    fn decay_damping_automation_click_free() {
        assert_step_click_free(
            200.0,
            |c| {
                c.params.decay = 0.2;
                c.params.damping = 0.8;
            },
            |c| {
                c.params.decay = 0.95;
                c.params.damping = 0.05;
            },
        );
    }

    #[test]
    fn saturation_automation_click_free() {
        assert_step_click_free(200.0, |_| {}, |c| c.saturation = 1.0);
    }

    #[test]
    fn tilt_automation_click_free() {
        assert_step_click_free(
            2000.0,
            |c| c.output_tilt_db = -12.0,
            |c| c.output_tilt_db = 12.0,
        );
    }

    #[test]
    fn decay_ramp_reaches_target() {
        // After stepping decay up, the new decay must actually take
        // effect (ramp completes): the tail outlives the old setting.
        let tail_energy = |decay: f64, automate: bool| -> f64 {
            let mut c = ReverbChain::new();
            c.set_algorithm(AlgorithmType::Hall);
            c.mix = 1.0;
            c.params.decay = if automate { 0.2 } else { decay };
            c.update(config());
            if automate {
                c.params.decay = decay;
                c.update_params(); // ramped path
            }
            let n = (SR as usize) * 2;
            let mut l: Vec<f64> = (0..n)
                .map(|i| {
                    if i < 4800 {
                        (2.0 * PI * 300.0 * i as f64 / SR).sin() * 0.5
                    } else {
                        0.0
                    }
                })
                .collect();
            let mut r = l.clone();
            c.process(&mut l, &mut r);
            l[(n - 9600)..].iter().map(|x| x * x).sum::<f64>()
        };
        let automated = tail_energy(0.95, true);
        let short = tail_energy(0.2, false);
        assert!(
            automated > short * 10.0,
            "ramped decay change must reach the algorithm: automated={automated}, short={short}"
        );
    }

    #[test]
    fn multi_band_decay_changes_low_energy() {
        // Excite a long sine at 100 Hz on a Hall reverb. Compare tail
        // energy with low_decay_mult = 0.3 (kills lows) vs 1.0.
        let make = |low_mult: f64| -> f64 {
            let mut c = ReverbChain::new();
            c.set_algorithm(AlgorithmType::Hall);
            c.mix = 1.0;
            c.params.decay = 0.9;
            c.params.low_decay_mult = low_mult;
            c.params.high_decay_mult = 1.0;
            c.params.band_crossover_hz = 400.0;
            c.update(config());
            let n = (SR as usize) * 2;
            let mut l: Vec<f64> = (0..n)
                .map(|i| (2.0 * PI * 100.0 * i as f64 / SR).sin() * 0.4)
                .collect();
            let mut r = l.clone();
            c.process(&mut l, &mut r);
            // Measure tail after input stops contributing (start from end)
            l[(n - 4800)..].iter().map(|x| x * x).sum::<f64>()
        };
        let low_kept = make(1.0);
        let low_cut = make(0.3);
        assert!(
            low_cut < low_kept,
            "Lower low_decay_mult should reduce LF tail energy: kept={low_kept}, cut={low_cut}"
        );
    }

    #[test]
    fn pan_zero_is_bit_identical() {
        let render = |pan: f64| -> Vec<f64> {
            let mut c = ReverbChain::new();
            c.mix = 1.0;
            c.pan = pan;
            c.update(config());
            let mut l: Vec<f64> = (0..9600).map(|i| if i < 8 { 1.0 } else { 0.0 }).collect();
            let mut r = l.clone();
            c.process(&mut l, &mut r);
            l
        };
        let base = render(0.0);
        // Re-render with the field left at its default (never set).
        let mut c = ReverbChain::new();
        c.mix = 1.0;
        c.update(config());
        let mut l: Vec<f64> = (0..9600).map(|i| if i < 8 { 1.0 } else { 0.0 }).collect();
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        for (a, b) in base.iter().zip(l.iter()) {
            assert!((a - b).abs() < 1e-15, "pan=0 must be bit-identical");
        }
    }

    #[test]
    fn pan_weights_field_but_keeps_far_side() {
        // Mid-left pan: left louder than right, but the right still
        // carries decay (weighted, not muted).
        let mut c = ReverbChain::new();
        c.set_algorithm(AlgorithmType::Hall);
        c.mix = 1.0;
        c.pan = -0.5;
        c.update(config());
        let n = 48000;
        let mut l: Vec<f64> = (0..n).map(|i| if i < 8 { 1.0 } else { 0.0 }).collect();
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        let el: f64 = l.iter().map(|x| x * x).sum();
        let er: f64 = r.iter().map(|x| x * x).sum();
        assert!(el > er * 1.5, "left must be weighted: L={el}, R={er}");
        assert!(
            er > el * 0.05,
            "right must still carry decay at mid pan: L={el}, R={er}"
        );

        // Full left: right side essentially gone.
        let mut c2 = ReverbChain::new();
        c2.set_algorithm(AlgorithmType::Hall);
        c2.mix = 1.0;
        c2.pan = -1.0;
        c2.update(config());
        let mut l2: Vec<f64> = (0..n).map(|i| if i < 8 { 1.0 } else { 0.0 }).collect();
        let mut r2 = l2.clone();
        c2.process(&mut l2, &mut r2);
        let el2: f64 = l2.iter().map(|x| x * x).sum();
        let er2: f64 = r2.iter().map(|x| x * x).sum();
        assert!(
            er2 < el2 * 1e-6,
            "full-left pan should mute the right wet: L={el2}, R={er2}"
        );
    }

    #[test]
    fn trem_zero_is_bit_identical_and_dry_untouched() {
        let render = |depth: f64, mix: f64| -> Vec<f64> {
            let mut c = ReverbChain::new();
            c.mix = mix;
            c.trem_depth = depth;
            c.trem_rate_hz = 6.0;
            c.update(config());
            let mut l: Vec<f64> = (0..9600)
                .map(|i| (2.0 * PI * 220.0 * i as f64 / SR).sin() * 0.5)
                .collect();
            let mut r = l.clone();
            c.process(&mut l, &mut r);
            l
        };
        // depth 0 == default, bit-identical.
        let off = render(0.0, 1.0);
        let base = render(-0.0, 1.0);
        for (a, b) in off.iter().zip(base.iter()) {
            assert!((a - b).abs() < 1e-15);
        }
        // mix 0 (all dry): trem must not touch the output at all.
        let dry_trem = render(1.0, 0.0);
        let dry_plain = render(0.0, 0.0);
        for (a, b) in dry_trem.iter().zip(dry_plain.iter()) {
            assert!((a - b).abs() < 1e-12, "trem must be wet-only");
        }
    }

    #[test]
    fn trem_modulates_wet_at_set_rate() {
        // Full-wet steady sine through a frozen-ish long hall: the wet
        // envelope should dip periodically at ~4 Hz (12000 samples).
        let mut c = ReverbChain::new();
        c.set_algorithm(AlgorithmType::Hall);
        c.mix = 1.0;
        c.params.decay = 0.9;
        c.trem_depth = 1.0;
        c.trem_rate_hz = 4.0;
        c.update(config());

        let n = 96000;
        let mut l: Vec<f64> = (0..n)
            .map(|i| (2.0 * PI * 220.0 * i as f64 / SR).sin() * 0.5)
            .collect();
        let mut r = l.clone();
        c.process(&mut l, &mut r);

        // RMS in 1000-sample windows over the steady-state region.
        let win = 1000;
        let rms: Vec<f64> = (24000..n - win)
            .step_by(win)
            .map(|s| (l[s..s + win].iter().map(|x| x * x).sum::<f64>() / win as f64).sqrt())
            .collect();
        let max = rms.iter().cloned().fold(0.0f64, f64::max);
        let min = rms.iter().cloned().fold(f64::MAX, f64::min);
        assert!(
            min < max * 0.6,
            "trem at depth 1 should visibly modulate the wet envelope: min={min}, max={max}"
        );
    }

    #[test]
    fn output_tilt_changes_spectrum() {
        let mut c = ReverbChain::new();
        c.mix = 1.0;
        c.output_tilt_db = -12.0; // dark
        c.update(config());
        let mut l: Vec<f64> = (0..4800)
            .map(|i| (2.0 * PI * 8000.0 * i as f64 / SR).sin() * 0.5)
            .collect();
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        let dark_hf: f64 = l.iter().map(|x| x * x).sum();

        let mut c2 = ReverbChain::new();
        c2.mix = 1.0;
        c2.output_tilt_db = 12.0; // bright
        c2.update(config());
        let mut l2: Vec<f64> = (0..4800)
            .map(|i| (2.0 * PI * 8000.0 * i as f64 / SR).sin() * 0.5)
            .collect();
        let mut r2 = l2.clone();
        c2.process(&mut l2, &mut r2);
        let bright_hf: f64 = l2.iter().map(|x| x * x).sum();

        assert!(
            bright_hf > dark_hf,
            "Bright tilt should keep more HF energy than dark: bright={bright_hf}, dark={dark_hf}"
        );
    }
}
