//! Gain-reduction smoothing with reference-informed change detection.
//!
//! The active smoother keeps a short per-channel history, uses a 0.1% change
//! threshold, and smooths transitions in dB. Older exact-polynomial hypotheses
//! are kept in the research docs, not in production code.
//!
//! History buffer structure (4 doubles per channel), newest first.

use dsp_core::{Channel, PerChannel};

/// Smoothed results kept per channel.
const DEPTH: usize = 4;

/// The last [`DEPTH`] smoothed results for one channel, newest first.
///
/// Destructuring rather than indexing: the array has a fixed size, so the
/// pattern is irrefutable and there is no bounds check to fail — which is both
/// the honest shape and the reason nothing here can panic on an audio callback.
#[derive(Debug, Clone, Copy)]
struct History([f64; DEPTH]);

impl History {
    /// Newest to oldest.
    const fn parts(self) -> (f64, f64, f64, f64) {
        let [newest, second, third, oldest] = self.0;
        (newest, second, third, oldest)
    }

    /// Shift in a new result, dropping the oldest.
    const fn push(&mut self, value: f64) {
        let [newest, second, third, _dropped] = self.0;
        self.0 = [value, newest, second, third];
    }
}

impl Default for History {
    /// Unity, not zero: these are linear gains, and a zeroed history would
    /// mute the first samples after a reset.
    fn default() -> Self {
        Self([1.0; DEPTH])
    }
}

/// Hermite cubic smoother with change detection
#[derive(Clone)]
pub struct HermiteCubicSmoother {
    history: PerChannel<History>,

    /// Change detection threshold (0.001 = 0.1%)
    change_threshold: f64,
}

/// Different hypotheses for what `state_func` does (mostly unused in current impl)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateFuncHypothesis {
    /// Hypothesis 1: `state_func(x)` = x (identity)
    Identity,
    /// Hypothesis 2: `state_func(x)` = 2.0 * x (simple scaling)
    Scale2x,
    /// Hypothesis 3: `state_func` returns stored `gr_inst`
    GrInst,
    /// Hypothesis 4: `state_func` = exponential smoother (IIR)
    ExponentialSmoothing,
    /// Hypothesis 5: `state_func(x)` = sqrt(x)
    PowerDomain,
    /// Hypothesis 6: `state_func(x)` = log(x)
    LogDomain,
}

impl HermiteCubicSmoother {
    #[must_use]
    pub fn new(_hypothesis: StateFuncHypothesis) -> Self {
        Self {
            history: PerChannel::filled(History::default()),
            change_threshold: 0.001,
        }
    }

    /// Active smoothing algorithm:
    /// 1. Read GR history (4 most recent smoothed results)
    /// 2. Detect change: threshold = `gr_inst` * 0.001, compare with history
    /// 3. Route: Hermite cubic if change detected, `sqrt(gr_inst)` if steady state
    /// 4. Update history for next sample
    ///
    /// The log/sqrt arguments the caller used to pass are gone: all four were
    /// ignored by the body, so computing them per sample was pure cost, and
    /// carrying them made the signature look like it modelled something.
    pub fn process(
        &mut self,
        gr_inst: f64,
        attack_coeff: f64,
        release_coeff: f64,
        ch: Channel,
    ) -> f64 {
        // Step 1: Get the history for this channel
        let (hist0, hist1, hist2, hist3) = self.history[ch].parts();

        // Step 2: Change detection threshold
        let threshold = gr_inst * self.change_threshold;

        // Step 3: Check if ANY history value differs significantly
        let has_change = (gr_inst - hist0).abs() >= threshold
            || (gr_inst - hist1).abs() >= threshold
            || (gr_inst - hist2).abs() >= threshold
            || (gr_inst - hist3).abs() >= threshold;

        // Step 4: Route to algorithm
        // Smooth in dB domain for better frequency response matching
        let gr_instant_sqrt = gr_inst.sqrt();
        let gr_instant_db = audiocore_dsp::db::linear_to_db(gr_instant_sqrt.max(1e-10));
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
            // Steady state: just return sqrt
            gr_instant_sqrt
        };

        // Step 5: shift the history and add the new result
        self.history[ch].push(result);

        result
    }

    /// Update change detection threshold for tuning
    pub fn set_change_threshold(&mut self, threshold: f64) {
        self.change_threshold = threshold;
    }

    /// Get current change detection threshold
    #[must_use]
    pub fn get_change_threshold(&self) -> f64 {
        self.change_threshold
    }

    pub fn reset(&mut self) {
        self.history.fill(History::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hermite_identity_hypothesis() {
        let mut smoother = HermiteCubicSmoother::new(StateFuncHypothesis::Identity);

        // Test with simple values
        let gr_inst = 0.5_f64;
        let attack = 0.01_f64;
        let release = 0.05_f64;

        let result = smoother.process(gr_inst, attack, release, Channel::LEFT);

        // Should not panic and should produce a valid f64
        assert!(result.is_finite());
        assert!(result >= 0.0);
    }
}
