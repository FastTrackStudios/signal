//! Envelope runtime state machine.
//!
//! Stage progression: Idle → Attack → Hold → Decay → Sustain → Release → Idle.
//! Supports three modes: Sustain (standard ADSR), OneShot, and Loop.

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
    pub fn new() -> Self {
        Self {
            stage: EnvelopeStage::Idle,
            stage_time: 0.0,
            level: 0.0,
            release_start_level: 0.0,
            gate_held: false,
        }
    }

    /// Current stage.
    pub fn stage(&self) -> EnvelopeStage {
        self.stage
    }

    /// Current envelope level [0.0, 1.0].
    pub fn level(&self) -> f64 {
        self.level
    }

    /// Whether the envelope is producing a non-zero output.
    pub fn is_active(&self) -> bool {
        self.stage != EnvelopeStage::Idle
    }

    /// Trigger the envelope (note-on / gate open).
    pub fn gate_on(&mut self) {
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
                let duration = config.attack_s as f64;
                if duration <= 0.0 || self.stage_time >= duration {
                    self.level = 1.0;
                    self.advance_from_attack(config);
                } else {
                    let t = self.stage_time / duration;
                    self.level = apply_curve(t, config.attack_curve);
                }
            }

            EnvelopeStage::Hold => {
                let duration = config.hold_s as f64;
                if duration <= 0.0 || self.stage_time >= duration {
                    self.level = 1.0;
                    self.enter_stage(EnvelopeStage::Decay);
                }
                // level stays at 1.0 during hold
            }

            EnvelopeStage::Decay => {
                let duration = config.decay_s as f64;
                let sustain = config.sustain as f64;
                if duration <= 0.0 || self.stage_time >= duration {
                    self.level = sustain;
                    self.advance_from_decay(config);
                } else {
                    let t = self.stage_time / duration;
                    let eased = apply_curve(t, config.decay_curve);
                    // Decay goes from 1.0 down to sustain level
                    self.level = 1.0 - eased * (1.0 - sustain);
                }
            }

            EnvelopeStage::Sustain => {
                // Hold at sustain level until gate_off
                self.level = config.sustain as f64;
            }

            EnvelopeStage::Release => {
                let duration = config.release_s as f64;
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

    /// Transition from Attack completion based on hold_s.
    fn advance_from_attack(&mut self, config: &EnvelopeConfig) {
        if config.hold_s > 0.0 {
            self.enter_stage(EnvelopeStage::Hold);
        } else {
            self.enter_stage(EnvelopeStage::Decay);
        }
    }

    /// Transition from Decay completion based on mode.
    fn advance_from_decay(&mut self, config: &EnvelopeConfig) {
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

    fn enter_stage(&mut self, stage: EnvelopeStage) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fast_config() -> EnvelopeConfig {
        EnvelopeConfig {
            attack_s: 0.1,
            decay_s: 0.1,
            sustain: 0.5,
            release_s: 0.1,
            depth: 1.0,
            mode: EnvelopeMode::Sustain,
            hold_s: 0.0,
            attack_curve: EasingCurve::Linear,
            decay_curve: EasingCurve::Linear,
        }
    }

    #[test]
    fn idle_by_default() {
        let env = EnvelopeState::new();
        assert_eq!(env.stage(), EnvelopeStage::Idle);
        assert_eq!(env.level(), 0.0);
        assert!(!env.is_active());
    }

    #[test]
    fn gate_on_starts_attack() {
        let mut env = EnvelopeState::new();
        env.gate_on();
        assert_eq!(env.stage(), EnvelopeStage::Attack);
        assert!(env.is_active());
    }

    #[test]
    fn attack_ramps_to_one() {
        let config = fast_config();
        let mut env = EnvelopeState::new();
        env.gate_on();

        // Tick halfway through attack
        let level = env.tick(0.05, &config);
        assert!(level > 0.0 && level < 1.0, "mid-attack level = {level}");

        // Tick past attack end
        let level = env.tick(0.06, &config);
        // Should now be in decay, level may have started descending or be at 1.0
        assert!(level > 0.0);
    }

    #[test]
    fn full_adsr_cycle() {
        let config = fast_config(); // A=0.1, D=0.1, S=0.5, R=0.1
        let mut env = EnvelopeState::new();
        env.gate_on();

        // Attack (0.1s)
        for _ in 0..10 {
            env.tick(0.01, &config);
        }
        // Should be at or near peak
        assert!(env.level() > 0.9, "after attack: {}", env.level());

        // Decay (0.1s) → sustain level 0.5
        for _ in 0..12 {
            env.tick(0.01, &config);
        }
        assert!(
            (env.level() - 0.5).abs() < 0.1,
            "after decay: {}",
            env.level()
        );
        assert_eq!(env.stage(), EnvelopeStage::Sustain);

        // Sustain holds
        env.tick(1.0, &config);
        assert!(
            (env.level() - 0.5).abs() < 0.01,
            "sustain: {}",
            env.level()
        );

        // Release
        env.gate_off();
        assert_eq!(env.stage(), EnvelopeStage::Release);
        for _ in 0..10 {
            env.tick(0.01, &config);
        }
        assert!(
            env.level() < 0.01,
            "after release: {}",
            env.level()
        );
    }

    #[test]
    fn one_shot_auto_releases() {
        let config = EnvelopeConfig {
            attack_s: 0.01,
            decay_s: 0.01,
            sustain: 0.5,
            release_s: 0.01,
            depth: 1.0,
            mode: EnvelopeMode::OneShot,
            hold_s: 0.0,
            attack_curve: EasingCurve::Linear,
            decay_curve: EasingCurve::Linear,
        };
        let mut env = EnvelopeState::new();
        env.gate_on();

        // Run through attack + decay + auto-release
        for _ in 0..50 {
            env.tick(0.001, &config);
        }

        // Should end up idle without ever calling gate_off
        assert_eq!(env.stage(), EnvelopeStage::Idle);
        assert!(env.level() < 0.01);
    }

    #[test]
    fn loop_mode_repeats_ad_cycle() {
        let config = EnvelopeConfig {
            attack_s: 0.05,
            decay_s: 0.05,
            sustain: 0.5,
            release_s: 0.1,
            depth: 1.0,
            mode: EnvelopeMode::Loop,
            hold_s: 0.0,
            attack_curve: EasingCurve::Linear,
            decay_curve: EasingCurve::Linear,
        };
        let mut env = EnvelopeState::new();
        env.gate_on();

        // Run for enough time to complete 2+ AD cycles (each 0.1s)
        let mut peak_count = 0;
        let mut prev_level = 0.0;
        for _ in 0..300 {
            let level = env.tick(0.001, &config);
            // Detect peaks: level was rising, now falling
            if level < prev_level && prev_level > 0.9 {
                peak_count += 1;
            }
            prev_level = level;
        }

        assert!(
            peak_count >= 2,
            "loop mode should produce multiple peaks, got {peak_count}"
        );
    }

    #[test]
    fn hold_stage_delays_decay() {
        let config = EnvelopeConfig {
            attack_s: 0.01,
            hold_s: 0.1,
            decay_s: 0.01,
            sustain: 0.3,
            release_s: 0.01,
            depth: 1.0,
            mode: EnvelopeMode::Sustain,
            attack_curve: EasingCurve::Linear,
            decay_curve: EasingCurve::Linear,
        };
        let mut env = EnvelopeState::new();
        env.gate_on();

        // Past attack
        env.tick(0.02, &config);
        assert_eq!(env.stage(), EnvelopeStage::Hold);
        assert!((env.level() - 1.0).abs() < 0.01);

        // Still in hold after 50ms
        env.tick(0.05, &config);
        assert_eq!(env.stage(), EnvelopeStage::Hold);
        assert!((env.level() - 1.0).abs() < 0.01);

        // Past hold → decay
        env.tick(0.06, &config);
        assert!(
            env.stage() == EnvelopeStage::Decay || env.stage() == EnvelopeStage::Sustain
        );
    }

    #[test]
    fn easing_curves_shape_attack() {
        // EaseIn should be slower at the start than Linear
        let linear_config = fast_config();
        let ease_in_config = EnvelopeConfig {
            attack_curve: EasingCurve::EaseIn,
            ..fast_config()
        };

        let mut linear_env = EnvelopeState::new();
        let mut ease_in_env = EnvelopeState::new();
        linear_env.gate_on();
        ease_in_env.gate_on();

        // At 25% through attack
        let linear_level = linear_env.tick(0.025, &linear_config);
        let ease_in_level = ease_in_env.tick(0.025, &ease_in_config);

        assert!(
            ease_in_level < linear_level,
            "EaseIn ({ease_in_level}) should be slower than Linear ({linear_level}) at start"
        );
    }

    #[test]
    fn release_from_partial_sustain() {
        let config = fast_config();
        let mut env = EnvelopeState::new();
        env.gate_on();

        // Get to sustain
        for _ in 0..30 {
            env.tick(0.01, &config);
        }
        let sustain_level = env.level();

        // Release
        env.gate_off();
        let first_release = env.tick(0.001, &config);
        // Should start falling from sustain level, not from 1.0
        assert!(first_release < sustain_level + 0.01);
        assert!(first_release > 0.0);
    }

    #[test]
    fn gate_off_during_idle_is_noop() {
        let mut env = EnvelopeState::new();
        env.gate_off(); // should not crash or change state
        assert_eq!(env.stage(), EnvelopeStage::Idle);
    }

    #[test]
    fn retrigger_during_release() {
        let config = fast_config();
        let mut env = EnvelopeState::new();
        env.gate_on();

        // Get to sustain then release
        for _ in 0..30 {
            env.tick(0.01, &config);
        }
        env.gate_off();
        env.tick(0.05, &config); // partially through release

        // Retrigger
        env.gate_on();
        assert_eq!(env.stage(), EnvelopeStage::Attack);
    }

    #[test]
    fn zero_attack_skips_to_peak() {
        let config = EnvelopeConfig {
            attack_s: 0.0,
            ..fast_config()
        };
        let mut env = EnvelopeState::new();
        env.gate_on();

        env.tick(0.001, &config);
        // Should have jumped past attack to decay
        assert!(
            env.stage() == EnvelopeStage::Decay || env.stage() == EnvelopeStage::Sustain,
            "stage = {:?}",
            env.stage()
        );
    }
}
