//! Gain-reduction smoothing with reference-informed change detection.
//!
//! The active smoother keeps a short per-channel history, uses a 0.1% change
//! threshold, and smooths transitions in dB. Older exact-polynomial hypotheses
//! are kept in the research docs, not in production code.
//!
//! History buffer structure (4 doubles per channel):
//! - `hist[0]`: Most recent smoothed result (becomes hist0 in next sample)
//! - `hist[1]`: Previous smoothed result (becomes hist1)
//! - `hist[2]`: Two samples ago (becomes hist2)
//! - `hist[3]`: Three samples ago (becomes hist3)

/// Hermite cubic smoother with change detection
#[derive(Clone)]
pub struct HermiteCubicSmoother {
    /// Per-channel history: 4 most recent smoothed results [hist0, hist1, hist2, hist3]
    history: [[f64; 4]; 2],

    /// Change detection threshold (0.001 = 0.1%)
    change_threshold: f64,
}

/// Different hypotheses for what state_func does (mostly unused in current impl)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateFuncHypothesis {
    /// Hypothesis 1: state_func(x) = x (identity)
    Identity,
    /// Hypothesis 2: state_func(x) = 2.0 * x (simple scaling)
    Scale2x,
    /// Hypothesis 3: state_func returns stored gr_inst
    GrInst,
    /// Hypothesis 4: state_func = exponential smoother (IIR)
    ExponentialSmoothing,
    /// Hypothesis 5: state_func(x) = sqrt(x)
    PowerDomain,
    /// Hypothesis 6: state_func(x) = log(x)
    LogDomain,
}

impl HermiteCubicSmoother {
    pub fn new(_hypothesis: StateFuncHypothesis) -> Self {
        Self {
            history: [[1.0; 4]; 2],
            change_threshold: 0.001,
        }
    }

    /// Active smoothing algorithm:
    /// 1. Read GR history (4 most recent smoothed results)
    /// 2. Detect change: threshold = gr_inst * 0.001, compare with history
    /// 3. Route: Hermite cubic if change detected, the requested gain if steady
    /// 4. Update history for next sample
    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        gr_inst: f64,
        attack_coeff: f64,
        release_coeff: f64,
        _log_rel: f64,
        _log_atk: f64,
        _sqrt_h0: f64,
        _sqrt_h1: f64,
        ch: usize,
    ) -> f64 {
        // Step 1: Get 4-sample history for this channel
        let hist = self.history[ch];
        let hist0 = hist[0];
        let hist1 = hist[1];
        let hist2 = hist[2];
        let hist3 = hist[3];

        // Step 2: Change detection threshold
        let threshold = gr_inst * self.change_threshold;

        // Step 3: Check if ANY history value differs significantly
        let has_change = (gr_inst - hist0).abs() >= threshold
            || (gr_inst - hist1).abs() >= threshold
            || (gr_inst - hist2).abs() >= threshold
            || (gr_inst - hist3).abs() >= threshold;

        // Step 4: Route to algorithm, smoothing in the dB domain.
        //
        // The gain goes in as it is. An earlier version took its square root
        // first — `linear_to_db(sqrt(g))` is `10·log10(g)`, which is exactly
        // half the reduction — while smoothing it against a history that had
        // no square root, so the two were not even in the same domain. The
        // steady-state branch then returned the square root outright.
        //
        // Measured against FabFilter Pro-C 3 this applied almost exactly half
        // the gain reduction the plugin did: -22.20 dB against -43.79 at
        // 100:1, -19.74 against -40.33 at 8.36:1. The error scaled as
        // `N·(1 - 1/ratio)` with N ≈ 10 dB across every ratio, which is what
        // a constant halving of the dB looks like from the outside.
        let gr_instant_db = audiocore_dsp::db::linear_to_db(gr_inst.max(1e-10));
        let hist0_db = audiocore_dsp::db::linear_to_db(hist0.max(1e-10));

        let result = if has_change {
            // Transition detected: smoothly interpolate between history and current GR
            // Use attack during compression increase (more negative dB), release otherwise
            let gr_change_db = gr_instant_db - hist0_db;
            let coeff = if gr_change_db < 0.0 {
                attack_coeff // Compressing more: use attack time
            } else {
                release_coeff // Releasing: use release time
            };

            // Exponential smoothing in dB domain
            let smoothed_db = coeff * hist0_db + (1.0 - coeff) * gr_instant_db;
            audiocore_dsp::db::db_to_linear(smoothed_db)
        } else {
            // Steady state: the requested gain, unchanged.
            gr_inst
        };

        // Step 5: Shift history and add new result
        // Next sample: hist[0] (new result), hist[1] (old hist[0]), hist[2] (old hist[1]), hist[3] (old hist[2])
        self.history[ch][3] = self.history[ch][2];
        self.history[ch][2] = self.history[ch][1];
        self.history[ch][1] = self.history[ch][0];
        self.history[ch][0] = result;

        result
    }

    /// Update change detection threshold for tuning
    pub fn set_change_threshold(&mut self, threshold: f64) {
        self.change_threshold = threshold;
    }

    /// Get current change detection threshold
    pub fn get_change_threshold(&self) -> f64 {
        self.change_threshold
    }

    pub fn reset(&mut self) {
        self.history = [[1.0; 4]; 2];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steady_state_applies_the_requested_gain_not_its_square_root() {
        // The regression this exists for: the smoother returned sqrt(g) in
        // steady state, which halves the reduction in dB. -12 dB of gain
        // reduction came out as -6.
        let mut h = HermiteCubicSmoother::new(StateFuncHypothesis::Identity);
        let target = audiocore_dsp::db::db_to_linear(-12.0);
        let mut out = 0.0;
        for _ in 0..4096 {
            out = h.process(target, 0.5, 0.5, 0.0, 0.0, 0.0, 0.0, 0);
        }
        let db = audiocore_dsp::db::linear_to_db(out);
        assert!(
            (db + 12.0).abs() < 0.1,
            "asked for -12 dB, settled at {db:.2} dB"
        );
    }

    #[test]
    fn a_range_of_reductions_all_settle_where_asked() {
        for want in [-1.0, -6.0, -12.0, -24.0, -40.0] {
            let mut h = HermiteCubicSmoother::new(StateFuncHypothesis::Identity);
            let target = audiocore_dsp::db::db_to_linear(want);
            let mut out = 0.0;
            for _ in 0..8192 {
                out = h.process(target, 0.5, 0.5, 0.0, 0.0, 0.0, 0.0, 0);
            }
            let db = audiocore_dsp::db::linear_to_db(out);
            assert!((db - want).abs() < 0.2, "asked {want} dB, settled {db:.2} dB");
        }
    }

    #[test]
    fn test_hermite_identity_hypothesis() {
        let mut smoother = HermiteCubicSmoother::new(StateFuncHypothesis::Identity);

        // Test with simple values
        let gr_inst = 0.5_f64;
        let attack = 0.01_f64;
        let release = 0.05_f64;

        let log_rel = release.ln();
        let log_atk = attack.ln();
        let sqrt_h0 = 0.7_f64;
        let sqrt_h1 = 0.6_f64;

        let result = smoother.process(
            gr_inst, attack, release, log_rel, log_atk, sqrt_h0, sqrt_h1, 0,
        );

        // Should not panic and should produce a valid f64
        assert!(result.is_finite());
        assert!(result >= 0.0);
    }
}
