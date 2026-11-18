//! Envelope runtime state machine.
//!
//! Stage progression: Idle → Attack → Hold → Decay → Sustain → Release → Idle.
//! Supports three modes: Sustain (standard ADSR), `OneShot`, and Loop.

use crate::easing::EasingCurve;
use crate::sources::envelope::{EnvelopeConfig, EnvelopeMode};

/// Current stage of the envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeStage {
    /// Not active — output is 0.0.
    Idle,
    /// Ramping from 0 to 1 over `attack_s`.
    Attack,
    /// Holding at peak (1.0) for `hold_s`.
    Hold,
    /// Falling from 1.0 to `sustain` level over `decay_s`.
    Decay,
    /// Holding at `sustain` level until gate off (Sustain mode only).
    Sustain,
    /// Falling from current level to 0 over `release_s`.
    Release,
}

/// Runtime state for a single envelope instance.
#[derive(Debug, Clone)]
pub struct EnvelopeState {
    stage: EnvelopeStage,
    /// Time elapsed within the current stage (seconds).
    stage_time: f64,
    /// Current envelope amplitude [0.0, 1.0].
    level: f64,
    /// Level at the moment Release was triggered (for smooth release from any point).
    release_start_level: f64,
    /// Whether the gate is currently held.
    gate_held: bool,
}

impl EnvelopeState {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            stage: EnvelopeStage::Idle,
            stage_time: 0.0,
            level: 0.0,
            release_start_level: 0.0,
            gate_held: false,
        }
    }

    /// Current stage.
    #[must_use]
    pub const fn stage(&self) -> EnvelopeStage {
        self.stage
    }

    /// Current envelope level [0.0, 1.0].
    #[must_use]
    pub const fn level(&self) -> f64 {
        self.level
    }

    /// Whether the envelope is producing a non-zero output.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.stage != EnvelopeStage::Idle
    }

    /// Trigger the envelope (note-on / gate open).
    pub const fn gate_on(&mut self) {
        self.gate_held = true;
        self.stage = EnvelopeStage::Attack;
        self.stage_time = 0.0;
        // Don't reset level — allows retriggering during release for smooth attack
    }

    /// Release the envelope (note-off / gate close).
    pub fn gate_off(&mut self) {
        if self.stage != EnvelopeStage::Idle && self.stage != EnvelopeStage::Release {
            self.gate_held = false;
            self.release_start_level = self.level;
            self.stage = EnvelopeStage::Release;
            self.stage_time = 0.0;
        }
    }

    /// Advance the envelope by `dt` seconds. Returns the current level [0.0, 1.0].
    ///
    /// The output represents the envelope amplitude **before** depth scaling — the
    /// caller applies `depth * amount` to get the final modulation value.
    pub fn tick(&mut self, dt: f64, config: &EnvelopeConfig) -> f64 {
        if self.stage == EnvelopeStage::Idle {
            return 0.0;
        }

        self.stage_time += dt;

        match self.stage {
            EnvelopeStage::Idle => {}

            EnvelopeStage::Attack => {
                let duration = f64::from(config.attack_s);
                if duration <= 0.0 || self.stage_time >= duration {
                    self.level = 1.0;
                    self.advance_from_attack(config);
                } else {
                    let t = self.stage_time / duration;
                    self.level = apply_curve(t, config.attack_curve);
                }
            }

            EnvelopeStage::Hold => {
                let duration = f64::from(config.hold_s);
                if duration <= 0.0 || self.stage_time >= duration {
                    self.level = 1.0;
                    self.enter_stage(EnvelopeStage::Decay);
                }
                // level stays at 1.0 during hold
            }

            EnvelopeStage::Decay => {
                let duration = f64::from(config.decay_s);
                let sustain = f64::from(config.sustain);
                if duration <= 0.0 || self.stage_time >= duration {
                    self.level = sustain;
                    self.advance_from_decay(config);
                } else {
                    let t = self.stage_time / duration;
                    let eased = apply_curve(t, config.decay_curve);
                    // Decay goes from 1.0 down to sustain level
                    self.level = eased.mul_add(-(1.0 - sustain), 1.0);
                }
            }

            EnvelopeStage::Sustain => {
                // Hold at sustain level until gate_off
                self.level = f64::from(config.sustain);
            }

            EnvelopeStage::Release => {
                let duration = f64::from(config.release_s);
                if duration <= 0.0 || self.stage_time >= duration {
                    self.level = 0.0;
                    self.stage = EnvelopeStage::Idle;
                    self.stage_time = 0.0;
                } else {
                    let t = self.stage_time / duration;
                    // Release falls from release_start_level to 0
                    // Use linear for release (most natural for volume envelopes)
                    self.level = self.release_start_level * (1.0 - t);
                }
            }
        }

        self.level
    }

    /// Transition from Attack completion based on `hold_s`.
    fn advance_from_attack(&mut self, config: &EnvelopeConfig) {
        if config.hold_s > 0.0 {
            self.enter_stage(EnvelopeStage::Hold);
        } else {
            self.enter_stage(EnvelopeStage::Decay);
        }
    }

    /// Transition from Decay completion based on mode.
    const fn advance_from_decay(&mut self, config: &EnvelopeConfig) {
        match config.mode {
            EnvelopeMode::Sustain => {
                self.enter_stage(EnvelopeStage::Sustain);
            }
            EnvelopeMode::OneShot => {
                // Auto-release: go straight to Release
                self.release_start_level = self.level;
                self.enter_stage(EnvelopeStage::Release);
            }
            EnvelopeMode::Loop => {
                if self.gate_held {
                    // Restart the AD cycle
                    self.enter_stage(EnvelopeStage::Attack);
                } else {
                    // Gate was released during the cycle — release now
                    self.release_start_level = self.level;
                    self.enter_stage(EnvelopeStage::Release);
                }
            }
        }
    }

    const fn enter_stage(&mut self, stage: EnvelopeStage) {
        self.stage = stage;
        self.stage_time = 0.0;
    }
}

impl Default for EnvelopeState {
    fn default() -> Self {
        Self::new()
    }
}

/// Apply an easing curve to time `t` in [0, 1], returning [0, 1].
fn apply_curve(t: f64, curve: EasingCurve) -> f64 {
    curve.apply(t.clamp(0.0, 1.0))
}
