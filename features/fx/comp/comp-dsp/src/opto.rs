//! An optical gain element fitted against the LA-2A Gray release captures.
//!
//! The reference stimulus falls from -6 to -20 dBFS; it is not a release into
//! silence. Captures contain applied gain including makeup, not gain reduction.
//! The extracted fixture and fitting script retain these distinctions.
//!
//! Release follows a stretched exponential. Its shape depends on the reduction
//! at release onset: a long tail under light loading, and an increasing recovery
//! rate under heavy loading. The remaining reduction determines the effective
//! position along that curve, so moving targets stay continuous. This is an
//! empirical gain-element model, not a simulation of the T4's physical circuit.

/// One measured release operating point. Shape 1 is an ordinary exponential;
/// values below 1 have a long tail; values above 1 accelerate recovery.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReleasePoint {
    pub depth_db: f64,
    pub time_ms: f64,
    pub shape: f64,
}

// Fitted to the normalized second-cycle curves at 1 kHz. The depth subtracts
// applied gain from the measured idle makeup, rather than negating output gain.
const GRAY_RELEASE: &[ReleasePoint] = &[
    ReleasePoint {
        depth_db: 8.258_824,
        time_ms: 180.690_587,
        shape: 0.554_508,
    },
    ReleasePoint {
        depth_db: 16.094_118,
        time_ms: 61.080_788,
        shape: 0.979_537,
    },
    ReleasePoint {
        depth_db: 25.623_529,
        time_ms: 33.680_980,
        shape: 1.391_615,
    },
    ReleasePoint {
        depth_db: 32.823_529,
        time_ms: 26.779_560,
        shape: 1.269_979,
    },
];

/// Immutable calibration shared by instances. Custom cells can supply a static
/// measured release curve without introducing callback allocation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OptoParams {
    pub attack_light_ms: f64,
    pub attack_deep_ms: f64,
    pub deep_db: f64,
    pub release: &'static [ReleasePoint],
}
impl Default for OptoParams {
    fn default() -> Self {
        Self {
            attack_light_ms: 14.0,
            attack_deep_ms: 8.0,
            deep_db: 32.823_529,
            release: GRAY_RELEASE,
        }
    }
}

/// Stream history for one optical attenuator, in positive dB of reduction.
#[derive(Debug, Clone)]
pub struct OptoCell {
    gr_db: f64,
    release_from_db: f64,
    sample_rate: f64,
    params: OptoParams,
}
impl OptoCell {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        Self {
            gr_db: 0.0,
            release_from_db: 0.0,
            sample_rate: sample_rate.max(1.0),
            params: OptoParams::default(),
        }
    }
    /// # Errors
    /// Rejects empty, unordered or non-finite calibration and nonpositive time constants.
    pub fn with_params(sample_rate: f64, params: OptoParams) -> Result<Self, crate::Error> {
        let mut cell = Self::new(sample_rate);
        cell.set_params(params)?;
        Ok(cell)
    }
    /// # Errors
    /// Invalid calibration is rejected before modifying the cell.
    pub fn set_params(&mut self, params: OptoParams) -> Result<(), crate::Error> {
        let positive = |x: f64| x.is_finite() && x > 0.0;
        if !positive(params.attack_light_ms)
            || !positive(params.attack_deep_ms)
            || !positive(params.deep_db)
            || params.release.is_empty()
            || !params
                .release
                .iter()
                .all(|p| positive(p.depth_db) && positive(p.time_ms) && positive(p.shape))
            || !params
                .release
                .windows(2)
                .all(|p| matches!(p,[a,b] if a.depth_db<b.depth_db))
        {
            return Err(crate::Error::InvalidControls);
        }
        self.params = params;
        Ok(())
    }
    #[must_use]
    pub const fn params(&self) -> OptoParams {
        self.params
    }
    pub const fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(1.0);
    }
    pub const fn reset(&mut self) {
        self.gr_db = 0.0;
        self.release_from_db = 0.0;
    }
    #[must_use]
    pub const fn gain_reduction_db(&self) -> f64 {
        self.gr_db
    }
    fn release_law(&self) -> (f64, f64) {
        let Some(first) = self.params.release.first() else {
            return (100.0, 1.0);
        };
        let depth = self.release_from_db;
        if depth <= first.depth_db {
            return (first.time_ms, first.shape);
        }
        for pair in self.params.release.windows(2) {
            let [a, b] = pair else {
                continue;
            };
            if depth <= b.depth_db {
                let mix = (depth - a.depth_db) / (b.depth_db - a.depth_db);
                return (
                    (b.time_ms - a.time_ms).mul_add(mix, a.time_ms),
                    (b.shape - a.shape).mul_add(mix, a.shape),
                );
            }
        }
        self.params
            .release
            .last()
            .map_or((100.0, 1.0), |p| (p.time_ms, p.shape))
    }
    /// Advance toward a nonnegative target reduction. The curve is continuous
    /// when a recovering signal starts compressing again or its target moves.
    pub fn process(&mut self, target_gr_db: f64) -> f64 {
        let target = target_gr_db.max(0.0);
        if target > self.gr_db {
            let depth = (self.gr_db / self.params.deep_db).clamp(0.0, 1.0);
            let ms = (self.params.attack_deep_ms - self.params.attack_light_ms)
                .mul_add(depth, self.params.attack_light_ms);
            let a = (-1000.0 / (ms * self.sample_rate)).exp();
            self.gr_db = (self.gr_db - target).mul_add(a, target);
            self.release_from_db = self.gr_db;
        } else if self.gr_db - target > 1e-12 {
            let (ms, shape) = self.release_law();
            let span = (self.release_from_db - target).max(self.gr_db - target);
            let remaining = ((self.gr_db - target) / span).clamp(1e-15, 1.0);
            let elapsed = (-remaining.ln()).powf(shape.recip());
            let next = (-(elapsed + 1000.0 / (ms * self.sample_rate)).powf(shape)).exp();
            self.gr_db = span.mul_add(next, target);
        } else {
            self.gr_db = target;
        }
        self.gr_db
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    /// Drive the cell to `depth` dB, then release it and report the times to
    /// 63% and 90% of the recovery — the same two numbers the plugin was
    /// measured with.
    fn release_times(cell: &mut OptoCell, depth: f64) -> (f64, f64, f64) {
        cell.reset();
        for _ in 0..(SR as usize) {
            cell.process(depth);
        }
        let from = cell.gain_reduction_db();
        let mut t63 = None;
        let mut t90 = None;
        let n = (SR * 8.0) as usize;
        for i in 0..n {
            let g = cell.process(0.0);
            let travelled = (from - g) / from.max(1e-9);
            let ms = i as f64 * 1000.0 / SR;
            if t63.is_none() && travelled >= 0.63 {
                t63 = Some(ms);
            }
            if t90.is_none() && travelled >= 0.90 {
                t90 = Some(ms);
                break;
            }
        }
        let a = t63.unwrap_or(f64::NAN);
        let b = t90.unwrap_or(f64::NAN);
        (a, b, b / a)
    }

    #[test]
    fn a_light_reduction_releases_with_a_long_tail() {
        // A light release into silence must retain a long tail. Actual
        // capture parity is tested separately with its nonzero quiet level.
        let mut cell = OptoCell::new(SR);
        let (t63, t90, ratio) = release_times(&mut cell, 3.3);
        assert!(
            ratio > 3.0,
            "light release should have a tail: {t63:.0}/{t90:.0} = {ratio:.2}"
        );
    }

    #[test]
    fn a_deep_reduction_releases_faster_and_more_evenly() {
        // A deep release into silence must recover faster than a light one.
        let mut cell = OptoCell::new(SR);
        let (deep_t63, _, deep_ratio) = release_times(&mut cell, 27.9);
        let (light_t63, _, light_ratio) = release_times(&mut cell, 3.3);
        assert!(
            deep_t63 < light_t63,
            "driven hard should recover sooner: {deep_t63:.0} vs {light_t63:.0} ms"
        );
        assert!(
            deep_ratio < light_ratio,
            "and more evenly: {deep_ratio:.2} vs {light_ratio:.2}"
        );
    }

    #[test]
    fn the_shape_changes_with_depth_which_one_exponential_cannot_do() {
        // The whole reason this type exists. A single time constant gives the
        // same ratio at every depth; this must not.
        let mut cell = OptoCell::new(SR);
        let ratios: Vec<f64> = [3.3, 11.2, 20.7, 27.9]
            .iter()
            .map(|d| release_times(&mut cell, *d).2)
            .collect();
        let spread = ratios.iter().cloned().fold(f64::MIN, f64::max)
            - ratios.iter().cloned().fold(f64::MAX, f64::min);
        assert!(spread > 1.5, "shape barely moved across depth: {ratios:?}");
    }

    #[test]
    fn it_settles_where_it_is_asked_to() {
        let mut cell = OptoCell::new(SR);
        for target in [0.0, 1.0, 6.0, 18.0, 30.0] {
            cell.reset();
            for _ in 0..(SR as usize * 2) {
                cell.process(target);
            }
            assert!(
                (cell.gain_reduction_db() - target).abs() < 0.1,
                "asked {target} dB, settled {:.2}",
                cell.gain_reduction_db()
            );
        }
    }

    #[test]
    fn it_returns_to_zero_given_long_enough() {
        let mut cell = OptoCell::new(SR);
        for _ in 0..(SR as usize) {
            cell.process(20.0);
        }
        for _ in 0..(SR as usize * 8) {
            cell.process(0.0);
        }
        assert!(
            cell.gain_reduction_db() < 0.1,
            "left at {:.3} dB",
            cell.gain_reduction_db()
        );
    }

    #[test]
    fn a_degenerate_sample_rate_does_not_divide_by_zero() {
        let mut cell = OptoCell::new(0.0);
        let g = cell.process(12.0);
        assert!(g.is_finite());
    }
}
