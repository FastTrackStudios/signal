//! Pattern runtime state — phase accumulation over a built
//! `dsp_core::modulation::Pattern`.
//!
//! The evaluator is rebuilt only when the config's point list actually
//! changes (cheap hash compare), so `tick` stays allocation-free at
//! control rate.

use crate::sources::lfo::RetriggerMode;
use crate::sources::pattern::PatternConfig;

/// Runtime state for a single Pattern source instance.
#[derive(Debug, Clone)]
pub struct PatternState {
    phase: f64,
    pattern: dsp_core::modulation::Pattern,
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
        eat(u64::from(p.x.to_bits()));
        eat(u64::from(p.y.to_bits()));
        eat(u64::from(p.tension.to_bits()));
        eat(u64::from(p.curve_type));
        eat(u64::from(p.clear_tails));
    }
    eat(u64::from(config.tension_mult.to_bits()));
    h
}

impl PatternState {
    #[must_use]
    pub fn from_config(config: &PatternConfig) -> Self {
        Self {
            phase: (f64::from(config.phase_offset) / 360.0).fract().abs(),
            pattern: config.build_pattern(),
            built_from: config_fingerprint(config),
        }
    }

    #[must_use]
    pub const fn phase(&self) -> f64 {
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
        self.phase = dt.mul_add(rate, self.phase).rem_euclid(1.0);
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
