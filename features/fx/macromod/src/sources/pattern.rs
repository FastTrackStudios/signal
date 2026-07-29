//! Pattern source — a drawn multi-segment curve (ShaperBox / tiagolr
//! style MSEG), cycled by tempo or free rate.
//!
//! The point list is a serde/Facet-friendly mirror of
//! `fts_modulation::pattern::Point`; [`PatternConfig::build_pattern`]
//! converts it into the DSP engine's `Pattern` for evaluation.

use facet::Facet;
use serde::{Deserialize, Serialize};

use super::lfo::{RetriggerMode, TempoDiv};

/// One breakpoint of a drawn pattern. `x`/`y` are normalized 0..1;
/// `curve_type` indexes [`fts_modulation::CurveType`] (0 = Hold,
/// 1 = Curve, 2 = SCurve, 3 = HalfSine, 4 = Pulse, 5 = Wave,
/// 6 = Triangle, 7 = Stairs, 8 = SmoothStairs); `tension` bends the
/// segment leaving this point (−1..1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Facet)]
pub struct PatternPoint {
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub tension: f32,
    #[serde(default = "default_curve_type")]
    pub curve_type: u8,
    /// Hard-reset connected tails/state as this point is crossed.
    #[serde(default)]
    pub clear_tails: bool,
}

fn default_curve_type() -> u8 {
    1 // CurveType::Curve
}

impl PatternPoint {
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            tension: 0.0,
            curve_type: default_curve_type(),
            clear_tails: false,
        }
    }
}

/// Configuration for a drawn-pattern modulation source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct PatternConfig {
    /// Breakpoints, x-sorted (the engine re-sorts defensively).
    pub points: Vec<PatternPoint>,
    /// Cycle rate in Hz when not tempo-synced.
    pub rate_hz: f32,
    /// Modulation depth (0.0–1.0).
    pub depth: f32,
    /// Phase offset in degrees (0–360).
    #[serde(default)]
    pub phase_offset: f32,
    /// Whether to sync to tempo (BPM-based rate).
    pub tempo_sync: bool,
    /// Tempo sync division. Only used when `tempo_sync` is true.
    #[serde(default)]
    pub sync_division: Option<TempoDiv>,
    /// Retrigger behavior.
    #[serde(default)]
    pub retrigger: RetriggerMode,
    /// Global tension multiplier applied on top of per-point tension.
    #[serde(default)]
    pub tension_mult: f32,
}

impl Default for PatternConfig {
    fn default() -> Self {
        // A gentle ramp-down — the classic sidechain/gate shape.
        Self {
            points: vec![
                PatternPoint::new(0.0, 1.0),
                PatternPoint::new(0.5, 0.0),
                PatternPoint::new(1.0, 1.0),
            ],
            rate_hz: 1.0,
            depth: 0.5,
            phase_offset: 0.0,
            tempo_sync: true,
            sync_division: Some(TempoDiv::Quarter),
            retrigger: RetriggerMode::Free,
            tension_mult: 0.0,
        }
    }
}

impl PatternConfig {
    /// Convert the point list into the DSP engine's evaluator.
    pub fn build_pattern(&self) -> fts_modulation::Pattern {
        let mut pattern = fts_modulation::Pattern::new();
        pattern.set_points(
            self.points
                .iter()
                .map(|p| {
                    let mut point =
                        fts_modulation::Point::new(f64::from(p.x), f64::from(p.y));
                    point.tension = f64::from(p.tension);
                    point.curve_type = fts_modulation::CurveType::from_u8(p.curve_type);
                    point.clear_tails = p.clear_tails;
                    point
                })
                .collect(),
        );
        pattern.tension_mult = f64::from(self.tension_mult);
        pattern
    }

    /// Effective cycle frequency in Hz.
    pub fn effective_rate_hz(&self, bpm: f32) -> f32 {
        if self.tempo_sync {
            let beats = self
                .sync_division
                .map(|d| d.beats())
                .unwrap_or(1.0)
                .max(1.0e-6);
            bpm / 60.0 / beats
        } else {
            self.rate_hz
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_builds_a_valid_pattern() {
        let cfg = PatternConfig::default();
        let p = cfg.build_pattern();
        assert!((p.get_y(0.0) - 1.0).abs() < 1e-9);
        assert!(p.get_y(0.5) < 0.05);
    }

    #[test]
    fn serde_round_trips() {
        let cfg = PatternConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let back: PatternConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn tempo_sync_rate() {
        let cfg = PatternConfig {
            tempo_sync: true,
            sync_division: Some(TempoDiv::Quarter),
            ..Default::default()
        };
        assert!((cfg.effective_rate_hz(120.0) - 2.0).abs() < 1e-6);
    }
}
