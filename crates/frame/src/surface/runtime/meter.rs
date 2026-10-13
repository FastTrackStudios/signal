//! Meter ballistics: engines publish levels at ~30 Hz; surfaces draw at
//! 60+ fps. Peaks attack instantly, fall at a fixed dB/s, and a peak-hold
//! marker lingers before falling too — all computed from explicit time so
//! the meter looks the same at any frame rate.

use crate::surface::proto::Curve;

/// dB → bar position.
#[derive(Debug, Clone, PartialEq)]
pub struct MeterScale {
    pub floor_db: f64,
    pub ceiling_db: f64,
    /// Shape over the dB span (linear-in-dB by default).
    pub curve: Curve,
}

impl Default for MeterScale {
    fn default() -> Self {
        Self {
            floor_db: -60.0,
            ceiling_db: 6.0,
            curve: Curve::Linear,
        }
    }
}

impl MeterScale {
    /// Where `db` sits, 0..1 (through the curve). A span can run either way:
    /// `floor > ceiling` reads right to left.
    #[must_use]
    pub fn position(&self, db: f64) -> f64 {
        let span = self.ceiling_db - self.floor_db;
        if span == 0.0 || db.is_nan() {
            return 0.0;
        }
        self.curve.apply((db - self.floor_db) / span)
    }
}

/// Which way a meter's reading attacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sense {
    /// A level: rests at silence, jumps *up* to a peak, falls back.
    #[default]
    Level,
    /// Gain reduction (dB ≤ 0): rests at 0 (no reduction), jumps *down* to
    /// the deepest reduction, recovers back up toward 0.
    Reduction,
}

impl Sense {
    /// What the meter reads with no signal.
    #[must_use]
    pub const fn rest_db(self) -> f64 {
        match self {
            Self::Level => SILENCE_DB,
            Self::Reduction => 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ballistics {
    /// Fall rate of the bar (recovery rate, for gain reduction).
    pub release_db_per_s: f64,
    /// How long the peak marker holds before falling.
    pub hold_ms: f64,
    pub sense: Sense,
    level_db: f64,
    level_t: f64,
    peak_db: f64,
    peak_t: f64,
}

impl Default for Ballistics {
    fn default() -> Self {
        Self::new(24.0, 1500.0)
    }
}

pub const SILENCE_DB: f64 = -120.0;

impl Ballistics {
    #[must_use]
    pub fn new(release_db_per_s: f64, hold_ms: f64) -> Self {
        Self {
            release_db_per_s,
            hold_ms,
            sense: Sense::Level,
            level_db: SILENCE_DB,
            level_t: 0.0,
            peak_db: SILENCE_DB,
            peak_t: 0.0,
        }
    }

    /// Gain-reduction ballistics: rest at 0 dB, attack toward deeper
    /// reduction, recover at `release_db_per_s`.
    #[must_use]
    pub fn reduction(release_db_per_s: f64, hold_ms: f64) -> Self {
        Self {
            sense: Sense::Reduction,
            level_db: 0.0,
            peak_db: 0.0,
            ..Self::new(release_db_per_s, hold_ms)
        }
    }

    /// Ballistics for a reading of `sense` (default rates).
    #[must_use]
    pub fn for_sense(sense: Sense) -> Self {
        match sense {
            Sense::Level => Self::default(),
            Sense::Reduction => Self::reduction(24.0, 1500.0),
        }
    }

    /// `from_db` after `since_ms` of release, toward rest.
    fn fallen(&self, from_db: f64, since_ms: f64) -> f64 {
        let d = self.release_db_per_s * since_ms.max(0.0) / 1000.0;
        match self.sense {
            Sense::Level => (from_db - d).max(SILENCE_DB),
            Sense::Reduction => (from_db + d).min(0.0),
        }
    }

    /// Whether `a` reads further from rest than `b`.
    fn beyond(&self, a: f64, b: f64) -> bool {
        match self.sense {
            Sense::Level => a >= b,
            Sense::Reduction => a <= b,
        }
    }

    /// A new measurement at time `t` (ms). Gain reduction may come either
    /// sign (`-6` or `6` both mean 6 dB of reduction).
    pub fn push(&mut self, db: f64, t: f64) {
        let db = match self.sense {
            Sense::Level => db,
            Sense::Reduction => -db.abs(),
        };
        let current = self.fallen(self.level_db, t - self.level_t);
        // Instant attack, timed release.
        self.level_db = if self.beyond(db, current) {
            db
        } else {
            current
        };
        self.level_t = t;
        let held = self.peak_hold_db(t);
        if self.beyond(db, held) {
            self.peak_db = db;
            self.peak_t = t;
        }
    }

    /// The bar's level at time `t` (ms).
    #[must_use]
    pub fn level_db(&self, t: f64) -> f64 {
        self.fallen(self.level_db, t - self.level_t)
    }

    /// The peak marker's level at time `t` (ms).
    #[must_use]
    pub fn peak_hold_db(&self, t: f64) -> f64 {
        let since = t - self.peak_t;
        if since <= self.hold_ms {
            self.peak_db
        } else {
            self.fallen(self.peak_db, since - self.hold_ms)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instant_attack_timed_release() {
        let mut b = Ballistics::new(24.0, 1000.0);
        b.push(-6.0, 0.0);
        assert_eq!(b.level_db(0.0), -6.0);
        assert!(
            (b.level_db(500.0) - -18.0).abs() < 1e-9,
            "24 dB/s for 0.5 s"
        );
        // A quieter reading doesn't pull the bar down faster than release.
        b.push(-40.0, 500.0);
        assert!((b.level_db(500.0) - -18.0).abs() < 1e-9);
        b.push(0.0, 600.0);
        assert_eq!(b.level_db(600.0), 0.0);
    }

    #[test]
    fn peak_holds_then_falls() {
        let mut b = Ballistics::new(24.0, 1000.0);
        b.push(-3.0, 0.0);
        b.push(-30.0, 100.0);
        assert_eq!(b.peak_hold_db(900.0), -3.0);
        assert!((b.peak_hold_db(1500.0) - -15.0).abs() < 1e-9);
    }

    #[test]
    fn gain_reduction_rests_at_zero_attacks_down_and_recovers_up() {
        let mut b = Ballistics::reduction(24.0, 1000.0);
        assert_eq!(b.level_db(0.0), 0.0, "no data: no reduction");
        b.push(-10.0, 0.0);
        assert_eq!(b.level_db(0.0), -10.0, "attack is instant, downward");
        // Less reduction doesn't snap back: the needle recovers at the rate.
        b.push(-2.0, 250.0);
        assert!(
            (b.level_db(250.0) - -4.0).abs() < 1e-9,
            "{}",
            b.level_db(250.0)
        );
        assert_eq!(b.level_db(5000.0), 0.0, "recovers to 0, not past it");
        // Deeper reduction lands at once; a positive reading is reduction too.
        b.push(6.0, 5000.0);
        assert_eq!(b.level_db(5000.0), -6.0);
        assert_eq!(
            b.peak_hold_db(5000.0),
            -6.0,
            "the old peak had fallen back to rest"
        );
    }

    #[test]
    fn a_reversed_span_reads_right_to_left() {
        let s = MeterScale {
            floor_db: 0.0,
            ceiling_db: -20.0,
            curve: crate::surface::proto::Curve::Linear,
        };
        assert_eq!(s.position(0.0), 0.0);
        assert!((s.position(-5.0) - 0.25).abs() < 1e-12);
    }

    #[test]
    fn frame_rate_independent() {
        let mut a = Ballistics::default();
        let mut c = Ballistics::default();
        a.push(-10.0, 0.0);
        c.push(-10.0, 0.0);
        // Sampling often vs once gives the same answer.
        for i in 0..60 {
            let _ = a.level_db(f64::from(i) * 16.0);
        }
        assert_eq!(a.level_db(960.0), c.level_db(960.0));
    }

    #[test]
    fn scale() {
        let s = MeterScale::default();
        assert_eq!(s.position(-60.0), 0.0);
        assert_eq!(s.position(6.0), 1.0);
        assert!((s.position(-27.0) - 0.5).abs() < 1e-12);
        assert_eq!(s.position(-200.0), 0.0);
    }
}
