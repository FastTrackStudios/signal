//! Log-mid parameter taper: a two-segment exponential mapping where a
//! chosen musical midpoint sits at exactly 50% of knob travel (e.g.
//! freq 10..1k..30k, Q 0.025..0.707..25, attack 0..100..1000 ms).

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogMidTaper {
    pub min: f64,
    pub mid: f64,
    pub max: f64,
}

impl LogMidTaper {
    pub const fn new(min: f64, mid: f64, max: f64) -> Self {
        Self { min, mid, max }
    }

    /// Normalized 0..1 → value.
    pub fn value(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        if t < 0.5 {
            self.min * (t * 2.0 * (self.mid / self.min).ln()).exp()
        } else {
            self.mid * ((t - 0.5) * 2.0 * (self.max / self.mid).ln()).exp()
        }
    }

    /// Value → normalized 0..1.
    pub fn normalized(&self, v: f64) -> f64 {
        let v = v.clamp(self.min, self.max);
        if v < self.mid {
            0.5 * (v / self.min).ln() / (self.mid / self.min).ln()
        } else {
            0.5 + 0.5 * (v / self.mid).ln() / (self.max / self.mid).ln()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midpoint_sits_at_half_travel() {
        let t = LogMidTaper::new(10.0, 1000.0, 30000.0);
        assert!((t.value(0.5) - 1000.0).abs() < 1e-9);
        assert!((t.value(0.0) - 10.0).abs() < 1e-9);
        assert!((t.value(1.0) - 30000.0).abs() < 1e-6);
        for &v in &[10.0, 55.0, 1000.0, 4200.0, 30000.0] {
            assert!((t.value(t.normalized(v)) - v).abs() / v < 1e-9);
        }
    }
}
