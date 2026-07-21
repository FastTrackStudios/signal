//! Onset/transient detection with confirmation window and retrigger prevention.
//!
//! Based on LSP Trigger's four-state machine architecture:
//! - Off: waiting for level to exceed detect threshold
//! - Detect: confirmation window — level must stay above threshold
//! - On: triggered, waiting for level to drop below release threshold
//! - Release: confirmation window — level must stay below release threshold
//!
//! This two-stage confirmation approach eliminates false triggers from
//! transient spikes while also preventing premature release from brief dips.

use audiocore_dsp::db::{db_to_linear, linear_to_db};
use audiocore_dsp::envelope::EnvelopeFollower;

use crate::spectral_flux::{FluxMode, SpectralFluxDetector, DEFAULT_FFT_SIZE, DEFAULT_HOP_SIZE};

/// Sidechain detection mode — how the raw input is converted to a level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DetectMode {
    /// Peak envelope follower (fast attack, slower release).
    Peak,
    /// RMS envelope (windowed).
    Rms,
}

/// Detection algorithm selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DetectAlgorithm {
    /// Time-domain envelope following (lowest latency, suitable for live).
    PeakEnvelope,
    /// Spectral flux onset detection (FFT-based).
    SpectralFlux,
    /// SuperFlux with vibrato suppression (FFT-based, highest accuracy).
    SuperFlux,
    /// High frequency content — best for percussive onsets.
    Hfc,
    /// Complex domain — catches soft/tonal onsets.
    ComplexDomain,
    /// Rectified complex domain — best single classical ODF.
    RectifiedComplexDomain,
    /// Modified KL divergence — sensitive to quiet onsets.
    ModifiedKl,
}

/// Detector state machine states.
#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    /// Idle — waiting for signal to exceed detect threshold.
    Off,
    /// Signal exceeded threshold — confirmation countdown running.
    /// If level drops before countdown expires, return to Off.
    Detect,
    /// Trigger confirmed — active until signal drops below release threshold.
    On,
    /// Signal dropped below release threshold — release countdown running.
    /// If level rises before countdown expires, return to On.
    Release,
}

// r[impl trigger.detector.transient]
// r[impl trigger.detector.retrigger-prevent]
// r[impl trigger.detector.sensitivity]
/// Onset detector with confirmation windows and retrigger prevention.
///
/// Converts a continuous audio level into discrete trigger events
/// (on/off transitions) using a state machine with hysteresis.
pub struct TriggerDetector {
    state: State,

    /// Smoothed sidechain level (linear amplitude).
    level: f64,

    /// Peak level captured during the detect phase, used for velocity.
    peak_level: f64,

    // Envelope follower coefficients
    attack_coeff: f64,
    release_coeff: f64,

    // RMS state
    rms_sum: f64,
    rms_count: usize,
    rms_window: usize,

    // Confirmation counters
    detect_counter: u32,
    release_counter: u32,

    // Cached sample counts
    detect_samples: u32,
    release_samples: u32,
    retrigger_samples: u32,

    /// Samples since last trigger (for retrigger prevention).
    since_last_trigger: u32,

    // Parameters
    /// Detection threshold in dB.
    pub detect_threshold_db: f64,
    /// Release threshold as a fraction of detect threshold (0.0-1.0).
    /// Lower = more hysteresis. Default 0.5 = release at half the detect level.
    pub release_ratio: f64,
    /// Detection confirmation time in ms (0-50).
    pub detect_time_ms: f64,
    /// Release confirmation time in ms (0-50).
    pub release_time_ms: f64,
    /// Minimum retrigger interval in ms (1-200).
    pub retrigger_ms: f64,
    /// Sidechain reactivity in ms (envelope smoothing time, 0-250).
    pub reactivity_ms: f64,
    /// Detection mode.
    pub mode: DetectMode,
    /// Detection algorithm.
    pub algorithm: DetectAlgorithm,

    // Spectral flux detector (used in SpectralFlux/SuperFlux modes)
    spectral: Option<SpectralFluxDetector>,
    /// Threshold delta for adaptive peak picking in spectral modes.
    pub spectral_threshold_delta: f64,
    /// Pending ODF value from spectral flux (converted to level for state machine).
    spectral_level: f64,

    sample_rate: f64,
}

impl TriggerDetector {
    pub fn new() -> Self {
        Self {
            state: State::Off,
            level: 0.0,
            peak_level: 0.0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            rms_sum: 0.0,
            rms_count: 0,
            rms_window: 480,
            detect_counter: 0,
            release_counter: 0,
            detect_samples: 0,
            release_samples: 0,
            retrigger_samples: 48,
            since_last_trigger: u32::MAX,
            detect_threshold_db: -30.0,
            release_ratio: 0.5,
            detect_time_ms: 1.0,
            release_time_ms: 5.0,
            retrigger_ms: 10.0,
            reactivity_ms: 10.0,
            mode: DetectMode::Peak,
            algorithm: DetectAlgorithm::PeakEnvelope,
            spectral: None,
            spectral_threshold_delta: 0.5,
            spectral_level: 0.0,
            sample_rate: 48000.0,
        }
    }

    /// Update internal coefficients after parameter changes.
    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;

        // Envelope follower: fast attack (1ms), release based on reactivity
        let attack_ms = 0.5_f64.min(self.reactivity_ms);
        self.attack_coeff =
            EnvelopeFollower::coeff((attack_ms * 0.001).max(1.0 / sample_rate), sample_rate);
        self.release_coeff = EnvelopeFollower::coeff(
            (self.reactivity_ms * 0.001).max(1.0 / sample_rate),
            sample_rate,
        );

        // RMS window size
        self.rms_window = (self.reactivity_ms * 0.001 * sample_rate).max(1.0) as usize;

        // Confirmation counters
        self.detect_samples = (self.detect_time_ms * 0.001 * sample_rate) as u32;
        self.release_samples = (self.release_time_ms * 0.001 * sample_rate) as u32;
        self.retrigger_samples = (self.retrigger_ms * 0.001 * sample_rate).max(1.0) as u32;

        // Initialize or update spectral detector if needed
        let flux_mode = match self.algorithm {
            DetectAlgorithm::PeakEnvelope => None,
            DetectAlgorithm::SpectralFlux => Some(FluxMode::SpectralFlux),
            DetectAlgorithm::SuperFlux => Some(FluxMode::SuperFlux),
            DetectAlgorithm::Hfc => Some(FluxMode::Hfc),
            DetectAlgorithm::ComplexDomain => Some(FluxMode::ComplexDomain),
            DetectAlgorithm::RectifiedComplexDomain => Some(FluxMode::RectifiedComplexDomain),
            DetectAlgorithm::ModifiedKl => Some(FluxMode::ModifiedKl),
        };

        match flux_mode {
            Some(mode) => match self.spectral.as_mut() {
                Some(spectral) if spectral.mode == mode => {
                    spectral.update(sample_rate);
                }
                _ => {
                    self.spectral = Some(SpectralFluxDetector::new(
                        mode,
                        DEFAULT_FFT_SIZE,
                        DEFAULT_HOP_SIZE,
                        sample_rate,
                    ));
                }
            },
            None => {
                self.spectral = None;
            }
        }
    }

    /// Feed one sample and return whether a trigger event occurred.
    ///
    /// Returns `Some(peak_level)` on a new trigger-on event, `None` otherwise.
    /// Use [`is_active`] to check ongoing trigger state.
    #[inline]
    pub fn tick(&mut self, sample: f64) -> Option<f64> {
        // In spectral modes, feed the spectral detector and use its ODF
        // as the "level" for the state machine
        if let Some(ref mut spectral) = self.spectral {
            if let Some(odf) = spectral.tick(sample) {
                // Convert ODF to a level the state machine can threshold.
                // The ODF is unbounded, so we use the peak picker's adaptive
                // threshold to determine if this is a trigger-worthy onset.
                // We scale the ODF into a pseudo-amplitude that the existing
                // threshold comparison can work with.
                self.spectral_level = odf;

                if spectral.is_peak(odf, self.spectral_threshold_delta) {
                    // Peak detected — set level high to trigger the state machine
                    self.level = 1.0; // max level to ensure threshold crossing
                    self.peak_level = odf; // store raw ODF as "peak" for velocity
                } else {
                    // No peak — decay the level
                    self.level *= 0.5;
                }
            }

            // Retrigger prevention
            if self.since_last_trigger < u32::MAX {
                self.since_last_trigger = self.since_last_trigger.saturating_add(1);
            }

            let detect_level = db_to_linear(self.detect_threshold_db);
            let release_level = detect_level * self.release_ratio;

            return self.run_state_machine(detect_level, release_level);
        }

        // Time-domain mode: update envelope level
        let input_abs = sample.abs();
        self.update_level(input_abs);

        // Retrigger prevention
        if self.since_last_trigger < u32::MAX {
            self.since_last_trigger = self.since_last_trigger.saturating_add(1);
        }

        let detect_level = db_to_linear(self.detect_threshold_db);
        let release_level = detect_level * self.release_ratio;

        self.run_state_machine(detect_level, release_level)
    }

    /// Run the four-state trigger state machine.
    #[inline]
    fn run_state_machine(&mut self, detect_level: f64, release_level: f64) -> Option<f64> {
        let mut triggered = None;

        match self.state {
            State::Off => {
                if self.level >= detect_level && self.since_last_trigger >= self.retrigger_samples {
                    if self.detect_samples == 0 {
                        // No confirmation needed — trigger immediately
                        self.state = State::On;
                        self.peak_level = self.level;
                        self.since_last_trigger = 0;
                        triggered = Some(self.peak_level);
                    } else {
                        self.state = State::Detect;
                        self.detect_counter = self.detect_samples;
                        self.peak_level = self.level;
                    }
                }
            }
            State::Detect => {
                // Track peak during confirmation
                if self.level > self.peak_level {
                    self.peak_level = self.level;
                }
                if self.level < detect_level {
                    // Level dropped — false alarm
                    self.state = State::Off;
                } else if self.detect_counter == 0 {
                    // Confirmation complete — trigger!
                    self.state = State::On;
                    self.since_last_trigger = 0;
                    triggered = Some(self.peak_level);
                } else {
                    self.detect_counter -= 1;
                }
            }
            State::On => {
                if self.level < release_level {
                    if self.release_samples == 0 {
                        self.state = State::Off;
                    } else {
                        self.state = State::Release;
                        self.release_counter = self.release_samples;
                    }
                }
            }
            State::Release => {
                if self.level >= release_level {
                    // Level came back — stay active
                    self.state = State::On;
                } else if self.release_counter == 0 {
                    // Release confirmed
                    self.state = State::Off;
                } else {
                    self.release_counter -= 1;
                }
            }
        }

        triggered
    }

    /// Whether the trigger is currently active (On or Detect states).
    pub fn is_active(&self) -> bool {
        matches!(self.state, State::On | State::Detect)
    }

    /// Get the current smoothed level (linear).
    pub fn level(&self) -> f64 {
        self.level
    }

    /// Get the current smoothed level in dB.
    pub fn level_db(&self) -> f64 {
        linear_to_db(self.level)
    }

    /// Returns the latency in samples introduced by the current algorithm.
    /// PeakEnvelope has zero latency; spectral modes have FFT latency.
    pub fn latency_samples(&self) -> usize {
        match &self.spectral {
            Some(s) => s.latency_samples(),
            None => 0,
        }
    }

    pub fn reset(&mut self) {
        self.state = State::Off;
        self.level = 0.0;
        self.peak_level = 0.0;
        self.rms_sum = 0.0;
        self.rms_count = 0;
        self.detect_counter = 0;
        self.release_counter = 0;
        self.since_last_trigger = u32::MAX;
        self.spectral_level = 0.0;
        if let Some(ref mut s) = self.spectral {
            s.reset();
        }
    }

    /// Update the internal level based on detection mode.
    #[inline]
    fn update_level(&mut self, input_abs: f64) {
        match self.mode {
            DetectMode::Peak => {
                // Asymmetric peak follower
                let coeff = if input_abs > self.level {
                    self.attack_coeff
                } else {
                    self.release_coeff
                };
                self.level = coeff * (self.level - input_abs) + input_abs;
            }
            DetectMode::Rms => {
                // Running RMS approximation
                self.rms_sum += input_abs * input_abs;
                self.rms_count += 1;
                if self.rms_count >= self.rms_window {
                    self.level = (self.rms_sum / self.rms_count as f64).sqrt();
                    self.rms_sum = 0.0;
                    self.rms_count = 0;
                }
            }
        }
    }
}

impl Default for TriggerDetector {
    fn default() -> Self {
        Self::new()
    }
}
