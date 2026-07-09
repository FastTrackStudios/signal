//! Random modulation source configuration.

use facet::Facet;
use serde::{Deserialize, Serialize};

use super::lfo::TempoDiv;

/// Configuration for a random modulation source.
///
/// Generates random values at a configurable rate with optional smoothing
/// between steps. Can be tempo-synced for rhythmic random modulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct RandomConfig {
    /// How often a new random value is generated (Hz).
    pub rate_hz: f32,
    /// Smoothing between random values (0.0 = stepped, 1.0 = fully smoothed).
    pub smoothing: f32,
    /// Modulation depth (0.0–1.0).
    pub depth: f32,
    /// Whether to sync rate to tempo.
    pub tempo_sync: bool,
    /// Tempo sync division. Only used when `tempo_sync` is true.
    #[serde(default)]
    pub sync_division: Option<TempoDiv>,
    /// Optional deterministic seed for reproducible sequences.
    #[serde(default)]
    pub seed: Option<u64>,
}

impl Default for RandomConfig {
    fn default() -> Self {
        Self {
            rate_hz: 1.0,
            smoothing: 0.3,
            depth: 1.0,
            tempo_sync: false,
            sync_division: None,
            seed: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_config_defaults() {
        let cfg = RandomConfig::default();
        assert_eq!(cfg.rate_hz, 1.0);
        assert_eq!(cfg.smoothing, 0.3);
        assert_eq!(cfg.depth, 1.0);
        assert!(!cfg.tempo_sync);
        assert!(cfg.sync_division.is_none());
        assert!(cfg.seed.is_none());
    }

    #[test]
    fn serde_round_trip() {
        let cfg = RandomConfig {
            rate_hz: 4.0,
            smoothing: 0.0,
            depth: 0.5,
            tempo_sync: true,
            sync_division: Some(TempoDiv::Eighth),
            seed: Some(42),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: RandomConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, parsed);
    }

    #[test]
    fn backward_compat_minimal_json() {
        let json = r#"{"rate_hz":1.0,"smoothing":0.3,"depth":1.0,"tempo_sync":false}"#;
        let parsed: RandomConfig = serde_json::from_str(json).unwrap();
        assert!(parsed.sync_division.is_none());
        assert!(parsed.seed.is_none());
    }
}
