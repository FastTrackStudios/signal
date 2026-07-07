//! Reverb chain — top-level processor with algorithm dispatch,
//! pre/post processing, mix, width, freeze, output EQ, ducker, saturation.

use crossbeam_channel::{Receiver, TryRecvError};
use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::{AudioConfig, Processor};

use crate::algorithm::{AlgorithmParams, AlgorithmType, ReverbAlgorithm};
use crate::algorithms;
use crate::ir::engine::ProcessedIr;
use crate::ir::prepared::PreparedIrPair;
use crate::primitives::envelope_follower::EnvelopeFollower;
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
    duck_gain: f64, // smoothed gain reduction (1.0 = no duck, 0.0 = full duck)

    // Algorithm params
    pub params: AlgorithmParams,

    // Global controls
    /// Pre-delay in milliseconds (0-500).
    pub predelay_ms: f64,
    /// Dry/wet mix (0.0 = fully dry, 1.0 = fully wet).
    pub mix: f64,
    /// Stereo width (0.0 = mono, 1.0 = normal, 2.0 = extra wide).
    pub width: f64,
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
            duck_env: EnvelopeFollower::new(sample_rate),
            duck_gain: 1.0,
            params: AlgorithmParams::default(),
            predelay_ms: 0.0,
            mix: 0.5,
            width: 1.0,
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

    /// Synchronously swap in a pre-FFT'd IR pair. No FFT on the audio
    /// thread. Returns `true` if accepted.
    pub fn load_prepared_ir(&mut self, pair: PreparedIrPair) -> bool {
        self.algorithm.try_load_prepared_ir(pair)
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
            self.algorithm.set_params(&self.effective_params());
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
    pub fn update_params(&mut self) {
        self.algorithm.set_params(&self.effective_params());
        self.sat_l.set_drive(self.saturation);
        self.sat_r.set_drive(self.saturation);
        self.tilt_l.set_pivot(self.output_tilt_pivot);
        self.tilt_l.set_tilt_db(self.output_tilt_db);
        self.tilt_r.set_pivot(self.output_tilt_pivot);
        self.tilt_r.set_tilt_db(self.output_tilt_db);
        self.duck_env
            .set_times(self.duck_attack_ms, self.duck_release_ms);
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
        self.duck_env.reset();
        self.duck_gain = 1.0;
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

        self.duck_env.set_sample_rate(config.sample_rate);
        self.duck_env.set_times(self.duck_attack_ms, self.duck_release_ms);

        self.sat_l.set_drive(self.saturation);
        self.sat_r.set_drive(self.saturation);

        self.algorithm.set_sample_rate(config.sample_rate);
        self.algorithm.set_params(&self.effective_params());
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

        // Drain pending raw f64 IRs — last-one-wins so back-to-back
        // loads don't each trigger an FFT precompute. Latency is one
        // audio block. This path DOES run FFTs on the audio thread.
        if let Some(rx) = self.ir_swap_rx.as_ref() {
            let mut latest: Option<ProcessedIr> = None;
            loop {
                match rx.try_recv() {
                    Ok(ir) => latest = Some(ir),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.ir_swap_rx = None;
                        break;
                    }
                }
            }
            if let Some(ir) = latest {
                self.algorithm.try_load_ir(&ir.left, &ir.right);
            }
        }

        self.predelay_samples = (self.predelay_ms * 0.001 * self.sample_rate) as usize;
        let duck_thresh = self.duck_threshold.max(1.0e-6);

        for i in 0..n {
            let dry_l = left[i];
            let dry_r = right[i];

            // Sidechain envelope from dry sum
            let env = self.duck_env.tick(dry_l + dry_r);
            let over = (env / duck_thresh - 1.0).max(0.0);
            let target_duck = 1.0 - (over.min(1.0) * self.duck_amount);
            // 1-pole smooth toward target_duck (independent of attack/release
            // — env already shapes the rate).
            self.duck_gain += (target_duck - self.duck_gain) * 0.25;

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

            // Wet saturation
            if self.saturation > 0.0 {
                wet_l = self.sat_l.tick(wet_l);
                wet_r = self.sat_r.tick(wet_r);
            }

            // Output band-shaping
            wet_l = self.output_hp.tick(wet_l, 0);
            wet_l = self.output_lp.tick(wet_l, 0);
            wet_r = self.output_hp.tick(wet_r, 1);
            wet_r = self.output_lp.tick(wet_r, 1);

            // Tilt EQ
            if self.output_tilt_db.abs() > 0.01 {
                wet_l = self.tilt_l.tick(wet_l);
                wet_r = self.tilt_r.tick(wet_r);
            }

            // Width (mid-side)
            let (mut final_l, mut final_r) = if (self.width - 1.0).abs() > 0.001 {
                let mid = (wet_l + wet_r) * 0.5;
                let side = (wet_l - wet_r) * 0.5;
                (mid + side * self.width, mid - side * self.width)
            } else {
                (wet_l, wet_r)
            };

            // Ducker
            if self.duck_amount > 0.0 {
                final_l *= self.duck_gain;
                final_r *= self.duck_gain;
            }

            // Mix
            left[i] = dry_l * (1.0 - self.mix) + final_l * self.mix;
            right[i] = dry_r * (1.0 - self.mix) + final_r * self.mix;
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
