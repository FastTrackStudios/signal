//! Envelope modulation source configuration.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::easing::EasingCurve;

/// Envelope behavior mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum EnvelopeMode {
    /// Standard ADSR — sustain holds until release trigger.
    #[default]
    Sustain,
    /// One-shot — plays through ADS then immediately releases (ignores sustain hold).
    OneShot,
    /// Looping — repeats the AD cycle while held.
    Loop,
}

/// Configuration for an envelope modulation source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct EnvelopeConfig {
    /// Attack time in seconds.
    pub attack_s: f32,
    /// Decay time in seconds.
    pub decay_s: f32,
    /// Sustain level (0.0–1.0).
    pub sustain: f32,
    /// Release time in seconds.
    pub release_s: f32,
    /// Modulation depth (0.0–1.0).
    pub depth: f32,
    /// Envelope behavior mode.
    #[serde(default)]
    pub mode: EnvelopeMode,
    /// Hold time in seconds (between attack peak and decay start).
    #[serde(default)]
    pub hold_s: f32,
    /// Attack curve shape.
    #[serde(default)]
    pub attack_curve: EasingCurve,
    /// Decay curve shape.
    #[serde(default)]
    pub decay_curve: EasingCurve,
}

impl Default for EnvelopeConfig {
    fn default() -> Self {
        Self {
            attack_s: 0.01,
            decay_s: 0.1,
            sustain: 0.7,
            release_s: 0.3,
            depth: 0.5,
            mode: EnvelopeMode::Sustain,
            hold_s: 0.0,
            attack_curve: EasingCurve::Linear,
            decay_curve: EasingCurve::Linear,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_config_defaults() {
        let env = EnvelopeConfig::default();
        assert!(env.attack_s > 0.0);
        assert!(env.sustain > 0.0);
        assert_eq!(env.mode, EnvelopeMode::Sustain);
        assert_eq!(env.hold_s, 0.0);
    }

    #[test]
    fn serde_round_trip() {
        let env = EnvelopeConfig {
            mode: EnvelopeMode::Loop,
            hold_s: 0.05,
            attack_curve: EasingCurve::EaseIn,
            decay_curve: EasingCurve::EaseOut,
            ..Default::default()
        };
        let json = serde_json::to_string(&env).unwrap();
        let parsed: EnvelopeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(env, parsed);
    }

    #[test]
    fn backward_compat_no_mode_field() {
        let json = r#"{"attack_s":0.01,"decay_s":0.1,"sustain":0.7,"release_s":0.3,"depth":0.5}"#;
        let parsed: EnvelopeConfig = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.mode, EnvelopeMode::Sustain);
        assert_eq!(parsed.hold_s, 0.0);
        assert_eq!(parsed.attack_curve, EasingCurve::Linear);
    }
}
