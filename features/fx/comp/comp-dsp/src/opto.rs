//! The optical cell — an LA-2A's T4, and what makes it not a compressor with
//! a release knob.
//!
//! An LA-2A has no attack or release control because it has no attack or
//! release *setting*: an electroluminescent panel lights in proportion to the
//! signal and a photoresistor beside it changes resistance in response, and
//! how fast it does that depends on how much light it has been seeing. The
//! envelope is a property of the cell, and it moves.
//!
//! Measured on a UADx LA-2A Gray through a pulsing tone, reading the release
//! back at 63% and 90% of its recovery:
//!
//! | Peak Reduction | settled  | t63    | t90    | t90/t63 |
//! |---------------:|---------:|-------:|-------:|--------:|
//! | 0.286          |  -3.3 dB | 162 ms | 969 ms |    5.98 |
//! | 0.429          | -11.2 dB |  61 ms | 139 ms |    2.28 |
//! | 0.714          | -20.7 dB |  36 ms |  57 ms |    1.58 |
//! | 1.000          | -27.9 dB |  28 ms |  49 ms |    1.75 |
//!
//! A single decaying exponential gives `t90/t63 = ln(10) = 2.303` at every
//! depth, always. This gives **5.98** when barely working and **1.58** when
//! driven hard, so it is not one exponential with a badly chosen constant —
//! the shape itself changes with depth. Lightly compressed, the cell dawdles:
//! most of the way back in 162 ms and the last of it nearly a second later.
//! Driven hard it recovers in a third of that and far more evenly.
//!
//! Hence two releases rather than one, blended by how deep the cell is: a
//! fast path that dominates under load, and a slow tail that dominates when
//! it is barely lit. Neither alone reproduces both ends.
//!
//! ## What this structure still cannot do
//!
//! The measured `t90/t63` falls to **1.58** when the unit is driven hard.
//! Any sum of decaying exponentials has a floor of `ln(10) = 2.303` — the
//! fitted model reaches 2.32 and cannot go lower however its constants are
//! set. A ratio below that means the recovery *accelerates*: it is steeper
//! late than early, which no combination of fixed-rate poles produces.
//!
//! Physically that is the cell speeding up as it recovers, and reproducing it
//! needs a release rate that depends on the reduction still remaining rather
//! than only on how deep it was driven. That is a change of mechanism, not of
//! constants, and it is not made here — the times are matched, the deep
//! release *shape* is not, and pretending otherwise would hide the gap.
//!
//! The attack is not exponential either — `t90/t63` runs 2.88 to 9.00 across
//! the same sweep, and the time is not even monotonic in Peak Reduction
//! (14 ms lightly compressed, 3 ms in the middle, 8 ms driven hard). That is
//! the same cell seen from the other side.

/// One electro-optical attenuator.
///
/// Holds its state in dB of gain reduction, positive meaning reduction,
/// because that is the domain the cell's behaviour was measured in and the
/// domain its time constants are meaningful in.
#[derive(Debug, Clone)]
pub struct OptoCell {
    /// Current reduction, dB, positive — the blend of the two paths below,
    /// and what the cell actually applies.
    gr_db: f64,
    /// The fast path's own state. It has to be kept separately: feeding the
    /// blended output back into it couples the two paths, and a weighted sum
    /// of two exponentials that share a state is just one exponential — which
    /// is what the first version of this produced, `t90/t63 = 2.316` at every
    /// depth, the single-exponential value to three figures.
    fast_db: f64,
    /// The slow half of the release, which lags the fast one.
    tail_db: f64,
    /// How deep the cell was when it began recovering.
    ///
    /// The release character is set by how hard the panel was driven, not by
    /// where the recovery has got to. Using the instantaneous value instead
    /// makes the constants drift during the release itself, which smears the
    /// very shape difference being modelled.
    release_from_db: f64,
    sample_rate: f64,
    params: OptoParams,
}

/// The cell's measured constants.
///
/// Defaults are fitted to the UADx LA-2A Gray table above. They are not
/// universal: an LA-3A and an LA-2 use the same kind of cell with different
/// panels and different numbers, which is why this is a parameter block
/// rather than a set of literals in the code.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OptoParams {
    /// Attack time at light reduction, ms.
    pub attack_light_ms: f64,
    /// Attack time when driven hard, ms.
    pub attack_deep_ms: f64,
    /// Fast release at light reduction, ms.
    pub release_fast_light_ms: f64,
    /// Fast release when driven hard, ms.
    pub release_fast_deep_ms: f64,
    /// The slow tail, ms. Barely moves with depth; it is the panel's
    /// persistence rather than the photoresistor's.
    pub release_tail_ms: f64,
    /// How much of the release the tail carries when the cell is barely lit.
    pub tail_mix_light: f64,
    /// ...and when it is driven hard. Lower: the fast path takes over.
    pub tail_mix_deep: f64,
    /// Reduction, in dB, at which "deep" is reached. Between zero and this
    /// the constants interpolate.
    pub deep_db: f64,
}

impl Default for OptoParams {
    fn default() -> Self {
        Self {
            // Fitted to the LA-2A Gray table above by `examples/opto_fit`,
            // not chosen by hand: the tail mix and the fast constant both
            // move the 63% point in opposite directions, so turning one by
            // hand undoes the other. The fit took the error from 5.05 to
            // 0.154 and put t63 within a millisecond at the two lighter
            // depths and within 4 ms at the two deeper ones.
            attack_light_ms: 14.0,
            attack_deep_ms: 8.0,
            release_fast_light_ms: 145.5,
            release_fast_deep_ms: 31.8,
            release_tail_ms: 1682.4,
            tail_mix_light: 0.230,
            tail_mix_deep: 0.000,
            deep_db: 14.1,
        }
    }
}

impl OptoCell {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            gr_db: 0.0,
            fast_db: 0.0,
            tail_db: 0.0,
            release_from_db: 0.0,
            sample_rate: sample_rate.max(1.0),
            params: OptoParams::default(),
        }
    }

    pub fn with_params(sample_rate: f64, params: OptoParams) -> Self {
        Self { params, ..Self::new(sample_rate) }
    }

    pub fn set_params(&mut self, params: OptoParams) {
        self.params = params;
    }

    pub fn params(&self) -> OptoParams {
        self.params
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate.max(1.0);
    }

    pub fn reset(&mut self) {
        self.gr_db = 0.0;
        self.fast_db = 0.0;
        self.tail_db = 0.0;
        self.release_from_db = 0.0;
    }

    /// Current reduction in dB, positive.
    pub fn gain_reduction_db(&self) -> f64 {
        self.gr_db
    }

    /// One-pole coefficient for a time constant in milliseconds.
    fn coeff(&self, ms: f64) -> f64 {
        let samples = (ms.max(0.01) / 1000.0) * self.sample_rate;
        (-1.0 / samples.max(1.0)).exp()
    }

    /// How far into "deep" the cell was driven, 0..1.
    fn depth(&self) -> f64 {
        (self.release_from_db / self.params.deep_db.max(0.001)).clamp(0.0, 1.0)
    }

    /// Advance one sample toward `target_gr_db` (positive = reduction) and
    /// return the reduction now applied, in dB.
    pub fn process(&mut self, target_gr_db: f64) -> f64 {
        let p = self.params;
        let d = self.depth();
        let lerp = |light: f64, deep: f64| light + (deep - light) * d;

        if target_gr_db > self.gr_db {
            // Lighting up. Both paths track the target together, so a
            // recovery always begins from where the panel actually is.
            let a = self.coeff(lerp(p.attack_light_ms, p.attack_deep_ms));
            self.fast_db = target_gr_db + (self.fast_db - target_gr_db) * a;
            self.tail_db = target_gr_db + (self.tail_db - target_gr_db) * a;
            self.gr_db = self.fast_db;
            // Remember how deep it got: that is what sets the release shape.
            self.release_from_db = self.gr_db;
        } else {
            // Recovering. Two independent paths — a fast one that dominates
            // under load and a slow tail that dominates when the cell is
            // barely lit — mixed by how deep it was driven.
            let fast = self.coeff(lerp(p.release_fast_light_ms, p.release_fast_deep_ms));
            let tail = self.coeff(p.release_tail_ms);
            let mix = lerp(p.tail_mix_light, p.tail_mix_deep).clamp(0.0, 1.0);

            self.fast_db = target_gr_db + (self.fast_db - target_gr_db) * fast;
            self.tail_db = target_gr_db + (self.tail_db - target_gr_db) * tail;
            self.gr_db = self.fast_db * (1.0 - mix) + self.tail_db * mix;
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
        // The measured LA-2A: t90/t63 = 5.98 at -3.3 dB. A single exponential
        // cannot exceed 2.303, so anything near 2.3 here means the tail is
        // not doing its job.
        let mut cell = OptoCell::new(SR);
        let (t63, t90, ratio) = release_times(&mut cell, 3.3);
        assert!(ratio > 3.0, "light release should have a tail: {t63:.0}/{t90:.0} = {ratio:.2}");
    }

    #[test]
    fn a_deep_reduction_releases_faster_and_more_evenly() {
        // Measured: 28 ms and t90/t63 = 1.75 at -27.9 dB.
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
        let ratios: Vec<f64> =
            [3.3, 11.2, 20.7, 27.9].iter().map(|d| release_times(&mut cell, *d).2).collect();
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
        assert!(cell.gain_reduction_db() < 0.1, "left at {:.3} dB", cell.gain_reduction_db());
    }

    #[test]
    fn report_against_the_measured_la2a() {
        // Not an assertion of closeness — a printout of where the fitted
        // model sits against the plugin, so the gap is visible rather than
        // asserted away. Run with --nocapture.
        let measured = [(3.3, 162.0, 5.98), (11.2, 61.0, 2.28), (20.7, 36.0, 1.58), (27.9, 28.0, 1.75)];
        let mut cell = OptoCell::new(SR);
        println!("\n  depth dB   t63 plugin   t63 model   ratio plugin   ratio model");
        for (depth, want_t63, want_ratio) in measured {
            let (t63, _, ratio) = release_times(&mut cell, depth);
            println!("  {depth:>8.1}   {want_t63:>10.0}   {t63:>9.0}   {want_ratio:>12.2}   {ratio:>11.2}");
        }
    }

    #[test]
    fn a_degenerate_sample_rate_does_not_divide_by_zero() {
        let mut cell = OptoCell::new(0.0);
        let g = cell.process(12.0);
        assert!(g.is_finite());
    }
}
