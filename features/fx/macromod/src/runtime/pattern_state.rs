//! Pattern runtime state — phase accumulation over a built
//! `fts_modulation::Pattern`.
//!
//! The evaluator is rebuilt only when the config's point list actually
//! changes (cheap hash compare), so `tick` stays allocation-free at
//! control rate.

use crate::sources::pattern::PatternConfig;
use crate::sources::lfo::RetriggerMode;

/// Runtime state for a single Pattern source instance.
#[derive(Debug, Clone)]
pub struct PatternState {
    phase: f64,
    pattern: fts_modulation::Pattern,
    /// Fingerprint of the points the evaluator was built from.
    built_from: u64,
}

fn config_fingerprint(config: &PatternConfig) -> u64 {
    // FNV-1a over the point fields; control-rate cheap, collision odds
    // irrelevant (a miss only costs a rebuild).
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |v: u64| {
        h ^= v;
        h = h.wrapping_mul(0x1000_0000_01b3);
    };
    for p in &config.points {
        eat(p.x.to_bits() as u64);
        eat(p.y.to_bits() as u64);
        eat(p.tension.to_bits() as u64);
        eat(u64::from(p.curve_type));
        eat(u64::from(p.clear_tails));
    }
    eat(config.tension_mult.to_bits() as u64);
    h
}

impl PatternState {
    pub fn from_config(config: &PatternConfig) -> Self {
        Self {
            phase: (f64::from(config.phase_offset) / 360.0).fract().abs(),
            pattern: config.build_pattern(),
            built_from: config_fingerprint(config),
        }
    }

    pub fn phase(&self) -> f64 {
        self.phase
    }

    /// Advance and evaluate. Returns the pattern value, unipolar [0, 1].
    pub fn tick(&mut self, dt: f64, config: &PatternConfig, bpm: f64) -> f64 {
        let fp = config_fingerprint(config);
        if fp != self.built_from {
            self.pattern = config.build_pattern();
            self.built_from = fp;
        }
        let rate = f64::from(config.effective_rate_hz(bpm as f32));
        self.phase = (self.phase + dt * rate).rem_euclid(1.0);
        self.pattern.get_y(self.phase)
    }

    /// Note-on retrigger: reset the phase when the config asks for it.
    pub fn retrigger(&mut self, config: &PatternConfig) {
        if config.retrigger == RetriggerMode::NoteOn {
            self.phase = (f64::from(config.phase_offset) / 360.0).fract().abs();
        }
    }

    pub fn reset(&mut self, phase_offset_degrees: f32) {
        self.phase = (f64::from(phase_offset_degrees) / 360.0).fract().abs();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_the_drawn_shape() {
        let config = PatternConfig::default(); // dip at x = 0.5
        let mut s = PatternState::from_config(&config);
        let mut min_v = f64::MAX;
        let mut max_v = f64::MIN;
        // 1 Hz effective at 60 bpm quarters; 1 s of 10 ms ticks = one cycle.
        for _ in 0..100 {
            let v = s.tick(0.01, &config, 60.0);
            min_v = min_v.min(v);
            max_v = max_v.max(v);
        }
        assert!(max_v > 0.9, "peak of the drawn shape: {max_v}");
        assert!(min_v < 0.1, "dip of the drawn shape: {min_v}");
    }

    #[test]
    fn rebuilds_only_on_change() {
        let mut config = PatternConfig::default();
        let mut s = PatternState::from_config(&config);
        let before = s.tick(0.0, &config, 120.0);
        // Flatten the pattern: value at the dip should now read high.
        for p in &mut config.points {
            p.y = 1.0;
        }
        s.phase = 0.5;
        let after = s.tick(0.0, &config, 120.0);
        assert!(before <= 1.0 && after > 0.9, "evaluator should rebuild: {after}");
    }
}
