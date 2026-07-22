//! TriggerChain — complete trigger processing chain with sidechain EQ.
//!
//! Signal flow:
//! - Audio path: Input → dry signal preserved → mix with sample playback
//! - Sidechain path: Input → HPF/LPF → detector → velocity → sampler trigger

use eq_dsp::band::Band;
use eq_dsp::FilterType;
use audiocore_dsp::{AudioConfig, Processor};

use crate::detector::{DetectMode, TriggerDetector};
use crate::sampler::{MixMode, Sampler};
use crate::velocity::{VelocityCurve, VelocityMapper};

// r[impl trigger.chain.signal-flow]
/// Complete trigger processing chain.
///
/// Signal flow:
/// - Sidechain: Input → HPF → LPF → detector → velocity extraction
/// - Audio: Input preserved, mixed with sample playback output
///
/// On each trigger event, the velocity mapper converts the detected peak
/// level to a velocity value, which selects and triggers a sample from
/// the sampler's velocity layers.
pub struct TriggerChain {
    pub detector: TriggerDetector,
    pub velocity: VelocityMapper,
    pub sampler: Sampler,

    // Sidechain filters
    sc_hpf: Band,
    sc_lpf: Band,

    // Parameters (public for direct access)
    /// Detection threshold in dB.
    pub threshold_db: f64,
    /// Release threshold ratio (0.0-1.0).
    pub release_ratio: f64,
    /// Detection confirmation time in ms.
    pub detect_time_ms: f64,
    /// Release confirmation time in ms.
    pub release_time_ms: f64,
    /// Minimum retrigger interval in ms.
    pub retrigger_ms: f64,
    /// Sidechain reactivity in ms.
    pub reactivity_ms: f64,
    /// Detection mode (Peak / RMS).
    pub detect_mode: DetectMode,
    /// Velocity dynamics (0.0-1.0).
    pub dynamics: f64,
    /// Velocity curve.
    pub velocity_curve: VelocityCurve,
    /// Mix mode (Replace / Layer / Blend).
    pub mix_mode: MixMode,
    /// Mix amount for blend mode (0.0-1.0).
    pub mix_amount: f64,
    /// Sidechain HPF frequency (0 = off).
    pub sc_hpf_freq: f64,
    /// Sidechain LPF frequency (0 = off).
    pub sc_lpf_freq: f64,
    /// Sidechain listen mode.
    pub sc_listen: bool,

    config: AudioConfig,

    /// Last trigger velocity (for metering).
    pub last_velocity: f64,
    /// Whether a trigger fired in the last process() call.
    pub triggered_this_block: bool,
}

impl TriggerChain {
    pub fn new() -> Self {
        let mut sc_hpf = Band::new();
        sc_hpf.filter_type = FilterType::Highpass;
        sc_hpf.freq_hz = 100.0;
        sc_hpf.q = 0.707;
        sc_hpf.order = 2;
        sc_hpf.enabled = false;

        let mut sc_lpf = Band::new();
        sc_lpf.filter_type = FilterType::Lowpass;
        sc_lpf.freq_hz = 10000.0;
        sc_lpf.q = 0.707;
        sc_lpf.order = 2;
        sc_lpf.enabled = false;

        Self {
            detector: TriggerDetector::new(),
            velocity: VelocityMapper::new(),
            sampler: Sampler::new(),
            sc_hpf,
            sc_lpf,
            threshold_db: -30.0,
            release_ratio: 0.5,
            detect_time_ms: 1.0,
            release_time_ms: 5.0,
            retrigger_ms: 10.0,
            reactivity_ms: 10.0,
            detect_mode: DetectMode::Peak,
            dynamics: 0.5,
            velocity_curve: VelocityCurve::Linear,
            mix_mode: MixMode::Replace,
            mix_amount: 1.0,
            sc_hpf_freq: 0.0,
            sc_lpf_freq: 0.0,
            sc_listen: false,
            config: AudioConfig {
                sample_rate: 48000.0,
                max_buffer_size: 512,
            },
            last_velocity: 0.0,
            triggered_this_block: false,
        }
    }

    /// Set sidechain HPF frequency (0 = off).
    pub fn set_sc_hpf(&mut self, freq: f64) {
        self.sc_hpf_freq = freq;
        if freq > 0.0 {
            self.sc_hpf.enabled = true;
            self.sc_hpf.freq_hz = freq;
            self.sc_hpf.update(self.config.sample_rate);
        } else {
            self.sc_hpf.enabled = false;
        }
    }

    /// Set sidechain LPF frequency (0 = off).
    pub fn set_sc_lpf(&mut self, freq: f64) {
        self.sc_lpf_freq = freq;
        if freq > 0.0 {
            self.sc_lpf.enabled = true;
            self.sc_lpf.freq_hz = freq;
            self.sc_lpf.update(self.config.sample_rate);
        } else {
            self.sc_lpf.enabled = false;
        }
    }

    /// Get the number of trigger events in the last process() call.
    /// (Check `triggered_this_block` for boolean, or this for count.)
    pub fn last_trigger_velocity(&self) -> f64 {
        self.last_velocity
    }

    /// Detection-only tick for MIDI-out shells (no sampler, no dry-path
    /// modification): run one stereo sample pair through the sidechain
    /// filters and the detector, returning `Some(velocity)` (0.0-1.0,
    /// already curve-mapped) on a new onset.
    ///
    /// This is the sidechain half of [`Processor::process`] — plugin shells
    /// that emit MIDI notes instead of playing samples drive this per sample
    /// so onsets stay sample-accurate, leaving the audio path untouched.
    #[inline]
    pub fn detect_tick(&mut self, left: f64, right: f64) -> Option<f64> {
        let mut sc_l = left;
        let mut sc_r = right;
        if self.sc_hpf.enabled {
            sc_l = self.sc_hpf.tick(sc_l, 0);
            sc_r = self.sc_hpf.tick(sc_r, 1);
        }
        if self.sc_lpf.enabled {
            sc_l = self.sc_lpf.tick(sc_l, 0);
            sc_r = self.sc_lpf.tick(sc_r, 1);
        }

        let sc_mono = (sc_l + sc_r) * 0.5;
        let detect_threshold_gain = 10.0_f64.powf(self.threshold_db / 20.0);

        if let Some(peak_level) = self.detector.tick(sc_mono) {
            let vel = self.velocity.map(peak_level, detect_threshold_gain);
            self.last_velocity = vel;
            self.triggered_this_block = true;
            Some(vel)
        } else {
            None
        }
    }

    /// Latency (in samples) of the current detection algorithm — zero for
    /// the time-domain peak envelope, FFT-sized for spectral modes.
    pub fn latency_samples(&self) -> usize {
        self.detector.latency_samples()
    }
}

impl Processor for TriggerChain {
    fn reset(&mut self) {
        self.detector.reset();
        self.sampler.reset();
        self.sc_hpf.reset();
        self.sc_lpf.reset();
        self.last_velocity = 0.0;
        self.triggered_this_block = false;
    }

    fn update(&mut self, config: AudioConfig) {
        self.config = config;

        // Update detector
        self.detector.detect_threshold_db = self.threshold_db;
        self.detector.release_ratio = self.release_ratio;
        self.detector.detect_time_ms = self.detect_time_ms;
        self.detector.release_time_ms = self.release_time_ms;
        self.detector.retrigger_ms = self.retrigger_ms;
        self.detector.reactivity_ms = self.reactivity_ms;
        self.detector.mode = self.detect_mode;
        self.detector.update(config.sample_rate);

        // Update velocity mapper
        self.velocity.dynamics = self.dynamics;
        self.velocity.curve = self.velocity_curve;

        // Update sampler
        self.sampler.set_sample_rate(config.sample_rate);
        self.sampler.mix_mode = self.mix_mode;
        self.sampler.mix_amount = self.mix_amount;

        // Update sidechain filters
        if self.sc_hpf.enabled {
            self.sc_hpf.update(config.sample_rate);
        }
        if self.sc_lpf.enabled {
            self.sc_lpf.update(config.sample_rate);
        }
    }

    fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        self.triggered_this_block = false;
        let detect_threshold_gain = 10.0_f64.powf(self.threshold_db / 20.0);

        for i in 0..left.len() {
            // Sidechain path: filter for detection
            let mut sc_l = left[i];
            let mut sc_r = right[i];

            if self.sc_hpf.enabled {
                sc_l = self.sc_hpf.tick(sc_l, 0);
                sc_r = self.sc_hpf.tick(sc_r, 1);
            }
            if self.sc_lpf.enabled {
                sc_l = self.sc_lpf.tick(sc_l, 0);
                sc_r = self.sc_lpf.tick(sc_r, 1);
            }

            // Sidechain listen mode
            if self.sc_listen {
                left[i] = sc_l;
                right[i] = sc_r;
                continue;
            }

            // Detection (mono sum)
            let sc_mono = (sc_l + sc_r) * 0.5;

            // Check for trigger
            if let Some(peak_level) = self.detector.tick(sc_mono) {
                let vel = self.velocity.map(peak_level, detect_threshold_gain);
                self.sampler.trigger(vel);
                self.last_velocity = vel;
                self.triggered_this_block = true;
            }

            // Sample playback + mix
            let (out_l, out_r) = self.sampler.tick(left[i], right[i]);
            left[i] = out_l;
            right[i] = out_r;
        }
    }
}

impl Default for TriggerChain {
    fn default() -> Self {
        Self::new()
    }
}
