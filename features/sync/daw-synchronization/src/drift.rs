//! Drift correction for PeerMesh transport sync.
//!
//! Uses proportional control to adjust playback rate based on the position
//! difference between master and follower. At 5Hz heartbeat rate, the
//! semitone-nudge approach from the Link engine (designed for 30Hz) causes
//! runaway oscillation. Instead, we compute a playrate proportional to the
//! drift, which naturally converges without overshooting.
//!
//! The [`DriftCorrector`] is host-agnostic: it takes a position difference and
//! returns a [`DriftAction`] describing what the caller should do.

use tracing::debug;

// ── Rolling Average ─────────────────────────────────────────────────────────

/// Fixed-size circular buffer with interquartile-mean smoothing.
///
/// Port of ReaBlink's `RollingAverage.hpp`. Maintains a bounded window of
/// samples and returns the trimmed mean (drops the lowest and highest
/// quarters before averaging), which rejects outliers from timer jitter
/// and scheduling noise.
pub struct RollingAverage {
    values: Vec<f64>,
    max_size: usize,
    /// Write cursor — next position to overwrite in the circular buffer.
    cursor: usize,
    /// Number of values added so far (saturates at `max_size`).
    len: usize,
}

impl RollingAverage {
    /// Create a new rolling average with the given window size.
    pub fn new(size: usize) -> Self {
        assert!(size > 0, "RollingAverage size must be > 0");
        Self {
            values: vec![0.0; size],
            max_size: size,
            cursor: 0,
            len: 0,
        }
    }

    /// Push a new sample into the buffer, evicting the oldest if full.
    pub fn add(&mut self, value: f64) {
        self.values[self.cursor] = value;
        self.cursor = (self.cursor + 1) % self.max_size;
        if self.len < self.max_size {
            self.len += 1;
        }
    }

    /// Interquartile mean of the buffered samples.
    ///
    /// Sorts a copy of the live window, trims the bottom and top quarters,
    /// and averages the remaining middle half. Returns `0.0` when empty.
    pub fn average(&self) -> f64 {
        if self.len == 0 {
            return 0.0;
        }

        let mut sorted: Vec<f64> = self.values[..self.len].to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let quarter = sorted.len() / 4;
        let trimmed = &sorted[quarter..sorted.len() - quarter];
        if trimmed.is_empty() {
            // Fewer than 4 samples — just average everything.
            return sorted.iter().sum::<f64>() / sorted.len() as f64;
        }
        trimmed.iter().sum::<f64>() / trimmed.len() as f64
    }

    /// Number of samples currently in the buffer.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Reset the buffer, clearing all samples.
    pub fn reset(&mut self) {
        self.cursor = 0;
        self.len = 0;
    }
}

// ── Drift Correction ────────────────────────────────────────────────────────

/// Maximum playrate correction away from 1.0.
/// Keeps playrate in [1 - MAX, 1 + MAX] = [0.85, 1.15].
/// 15% is inaudible for tempo-nudge; 50% was perceptible during live playback.
const MAX_RATE_CORRECTION: f64 = 0.15;

/// Proportional gain — how aggressively to correct.
///
/// With gain=1.5 and 30Hz heartbeat: a 50ms drift produces 7.5% playrate
/// correction, converging in ~0.66s. Gentler than 2.0 to avoid audible pumping.
const PROPORTIONAL_GAIN: f64 = 1.5;

/// Action to take for drift correction.
#[derive(Debug, Clone, PartialEq)]
pub enum DriftAction {
    /// No correction needed — drift within tolerance and playrate is normal.
    None,
    /// Set playrate to the given value to correct drift.
    SetRate { new_playrate: f64 },
    /// Reset playrate to 1.0 (drift within tolerance, playrate was nudged).
    Reset,
    /// Hard seek to master position (drift exceeds hard-seek threshold).
    HardSeek,
}

/// Drift corrector using proportional control.
///
/// Instead of fixed semitone nudges (which compound and oscillate at 5Hz),
/// computes a playrate proportional to the drift. Larger drift = faster
/// correction, and the correction naturally decreases as positions converge.
///
/// ```text
/// playrate = 1.0 + clamp(drift * gain, -max, +max)
/// ```
pub struct DriftCorrector {
    /// Tolerance in seconds. Drift below this is considered "in sync".
    tolerance: f64,
    /// Hard seek threshold in seconds.
    hard_seek_threshold: f64,
    /// Smoothing filter for raw drift measurements (window=4, ~133ms at 30Hz).
    /// Smaller than Link's window=8 to avoid phase lag in the control loop;
    /// 4 samples is enough to reject single-tick jitter without destabilizing
    /// the proportional controller.
    drift_avg: RollingAverage,
}

impl DriftCorrector {
    /// Create a new drift corrector with default thresholds.
    ///
    /// - Tolerance: 10ms
    /// - Hard seek: 0.5s
    pub fn new() -> Self {
        Self {
            tolerance: 0.01,
            // Hard-seek when drift exceeds 1.0s. Proportional control at
            // gain=1.5 converges drifts up to ~500ms in a few seconds.
            // Above 1.0s, something is fundamentally wrong (seek, tempo
            // change, etc.) and a hard seek is more efficient.
            hard_seek_threshold: 1.0,
            drift_avg: RollingAverage::new(4),
        }
    }

    /// Create a drift corrector with custom thresholds.
    pub fn with_thresholds(tolerance: f64, hard_seek_threshold: f64) -> Self {
        Self {
            tolerance,
            hard_seek_threshold,
            drift_avg: RollingAverage::new(4),
        }
    }

    /// Feed a position difference and get the correction action.
    ///
    /// # Arguments
    /// - `drift_seconds`: `remote_position - local_position`. Positive means
    ///   local is behind the master (needs to speed up).
    /// - `current_playrate`: current playback rate (1.0 = normal speed).
    ///
    /// # Returns
    /// A [`DriftAction`] describing what the caller should do.
    pub fn correct(&mut self, drift_seconds: f64, current_playrate: f64) -> DriftAction {
        let abs_drift = drift_seconds.abs();

        // Way too far off — hard seek (check raw drift for immediate response)
        if abs_drift > self.hard_seek_threshold {
            debug!("Drift {drift_seconds:.3}s exceeds hard-seek threshold");
            self.drift_avg.reset(); // stale after seek
            return DriftAction::HardSeek;
        }

        // Feed raw measurement into the smoothing filter
        self.drift_avg.add(drift_seconds);
        let smoothed = self.drift_avg.average();
        let abs_smoothed = smoothed.abs();

        if abs_smoothed > self.tolerance {
            // Proportional correction using smoothed drift
            let correction =
                (smoothed * PROPORTIONAL_GAIN).clamp(-MAX_RATE_CORRECTION, MAX_RATE_CORRECTION);
            let new_rate = 1.0 + correction;
            debug!(
                "Drift correction: raw={drift_seconds:.4}s smoothed={smoothed:.4}s, playrate {current_playrate:.4} → {new_rate:.4}"
            );
            DriftAction::SetRate {
                new_playrate: new_rate,
            }
        } else if (current_playrate - 1.0).abs() > 0.001 {
            // Within tolerance but playrate was previously adjusted — reset
            debug!("Drift within tolerance (smoothed={smoothed:.4}s), resetting playrate");
            DriftAction::Reset
        } else {
            DriftAction::None
        }
    }

    /// Get the hard-seek threshold in seconds.
    pub fn hard_seek_threshold(&self) -> f64 {
        self.hard_seek_threshold
    }

    /// Reset the corrector state (e.g., on transport stop/start).
    pub fn reset(&mut self) {
        self.drift_avg.reset();
    }
}

impl Default for DriftCorrector {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolling_average_basic() {
        let mut ra = RollingAverage::new(4);
        assert!(ra.is_empty());
        assert_eq!(ra.average(), 0.0);

        ra.add(1.0);
        ra.add(2.0);
        ra.add(3.0);
        let avg = ra.average();
        assert!((avg - 2.0).abs() < 1e-9);
    }

    #[test]
    fn rolling_average_circular_eviction() {
        let mut ra = RollingAverage::new(3);
        ra.add(10.0);
        ra.add(20.0);
        ra.add(30.0);
        ra.add(40.0);
        assert_eq!(ra.len(), 3);
        assert!((ra.average() - 30.0).abs() < 1e-9);
    }

    #[test]
    fn rolling_average_outlier_rejection() {
        let mut ra = RollingAverage::new(8);
        ra.add(100.0);
        ra.add(10.0);
        ra.add(10.0);
        ra.add(10.0);
        ra.add(10.0);
        ra.add(10.0);
        ra.add(10.0);
        ra.add(0.001);
        assert!((ra.average() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn rolling_average_reset() {
        let mut ra = RollingAverage::new(4);
        ra.add(1.0);
        ra.add(2.0);
        assert_eq!(ra.len(), 2);
        ra.reset();
        assert!(ra.is_empty());
        assert_eq!(ra.average(), 0.0);
    }

    // ── DriftCorrector tests ────────────────────────────────────────────

    #[test]
    fn no_correction_when_in_sync() {
        let mut dc = DriftCorrector::new();
        let action = dc.correct(0.0, 1.0);
        assert_eq!(action, DriftAction::None);
    }

    #[test]
    fn proportional_rate_when_behind() {
        let mut dc = DriftCorrector::new();
        let action = dc.correct(0.1, 1.0); // 100ms behind
        match action {
            DriftAction::SetRate { new_playrate } => {
                // 1.0 + 0.1 * 1.5 = 1.15
                assert!(
                    (new_playrate - 1.15).abs() < 0.01,
                    "expected ~1.15, got {new_playrate}"
                );
            }
            other => panic!("expected SetRate, got {other:?}"),
        }
    }

    #[test]
    fn proportional_rate_when_ahead() {
        let mut dc = DriftCorrector::new();
        let action = dc.correct(-0.1, 1.0); // 100ms ahead
        match action {
            DriftAction::SetRate { new_playrate } => {
                // 1.0 + (-0.1 * 1.5) = 0.85
                assert!(
                    (new_playrate - 0.85).abs() < 0.01,
                    "expected ~0.85, got {new_playrate}"
                );
            }
            other => panic!("expected SetRate, got {other:?}"),
        }
    }

    #[test]
    fn rate_clamped_on_large_drift() {
        // Use custom thresholds so 0.3s drift is within hard-seek but large
        // enough that gain * drift exceeds MAX_RATE_CORRECTION.
        // 0.3 * 1.5 = 0.45 clamped to 0.15
        let mut dc = DriftCorrector::with_thresholds(0.01, 5.0);
        let action = dc.correct(3.0, 1.0); // 3s behind, 3.0*1.5=4.5 but clamped to 0.15
        match action {
            DriftAction::SetRate { new_playrate } => {
                // Clamped: 1.0 + 0.15 = 1.15
                assert!(
                    (new_playrate - 1.15).abs() < 0.01,
                    "expected 1.15 (clamped), got {new_playrate}"
                );
            }
            other => panic!("expected SetRate, got {other:?}"),
        }
    }

    #[test]
    fn reset_when_converged() {
        let mut dc = DriftCorrector::new();
        // Within tolerance, but playrate still adjusted
        let action = dc.correct(0.001, 1.25);
        assert_eq!(action, DriftAction::Reset);
    }

    #[test]
    fn hard_seek_on_large_drift() {
        let mut dc = DriftCorrector::new();
        let action = dc.correct(1.5, 1.0); // 1.5s off (threshold is 1.0s)
        assert_eq!(action, DriftAction::HardSeek);
    }

    #[test]
    fn convergence_simulation() {
        // Simulate a follower that starts 400ms behind the master.
        // At gain=1.5, MAX_RATE_CORRECTION=0.15, hard_seek=0.5s:
        // 400ms is under the 0.5s hard-seek threshold, so proportional
        // control handles it. Should converge in ~2.6s.
        let mut dc = DriftCorrector::new();
        let mut local_pos = 0.0;
        let mut master_pos = 0.4; // 400ms ahead (under 0.5s hard-seek threshold)
        let mut playrate = 1.0;
        let tick_interval = 0.033; // 33ms heartbeat (30Hz)

        let mut max_playrate = 1.0_f64;
        let mut min_playrate = 1.0_f64;
        let mut converged = false;

        for tick in 0..300 {
            let drift = master_pos - local_pos;
            let action = dc.correct(drift, playrate);

            match action {
                DriftAction::SetRate { new_playrate } => playrate = new_playrate,
                DriftAction::Reset => playrate = 1.0,
                DriftAction::None => {}
                DriftAction::HardSeek => {
                    local_pos = master_pos;
                    playrate = 1.0;
                }
            }

            max_playrate = max_playrate.max(playrate);
            min_playrate = min_playrate.min(playrate);

            // Advance positions
            master_pos += tick_interval; // master at 1.0x
            local_pos += tick_interval * playrate; // follower at adjusted rate

            if tick > 3 && drift.abs() < 0.01 {
                converged = true;
                println!("Converged at tick {tick}: drift={drift:.4}s, playrate={playrate:.4}");
                break;
            }
        }

        assert!(converged, "should converge within 300 ticks (~10 seconds)");
        // Playrate should stay within [0.85, 1.15] (clamped by MAX_RATE_CORRECTION=0.15)
        assert!(
            min_playrate >= 0.85,
            "playrate should not drop below 0.85, got {min_playrate}"
        );
        assert!(
            max_playrate <= 1.15,
            "playrate should not exceed 1.15, got {max_playrate}"
        );
    }

    #[test]
    fn no_oscillation() {
        // Ensure we don't oscillate around zero.
        // Start 200ms behind, track the drift direction changes.
        let mut dc = DriftCorrector::new();
        let mut local_pos = 0.0;
        let mut master_pos = 0.2;
        let mut playrate = 1.0;
        let tick_interval = 0.033;
        let mut sign_changes = 0;
        let mut prev_drift_sign = 1;

        for _ in 0..100 {
            let drift = master_pos - local_pos;
            let action = dc.correct(drift, playrate);

            match action {
                DriftAction::SetRate { new_playrate } => playrate = new_playrate,
                DriftAction::Reset => playrate = 1.0,
                _ => {}
            }

            let current_sign = if drift > 0.001 {
                1
            } else if drift < -0.001 {
                -1
            } else {
                0
            };
            if current_sign != 0 && current_sign != prev_drift_sign {
                sign_changes += 1;
            }
            if current_sign != 0 {
                prev_drift_sign = current_sign;
            }

            master_pos += tick_interval;
            local_pos += tick_interval * playrate;
        }

        // With proportional control + smoothing, there should be at most 1 sign
        // change (when it crosses zero). No oscillation.
        assert!(
            sign_changes <= 1,
            "too many drift sign changes ({sign_changes}), indicates oscillation"
        );
    }

    #[test]
    fn smoothing_rejects_jitter() {
        // Alternating +30ms/-30ms jitter around a true 50ms drift.
        // Without smoothing, playrate would oscillate between ~1.03 and ~1.12.
        // With smoothing, the interquartile mean converges to ~50ms and
        // playrate stays stable.
        let mut dc = DriftCorrector::new();
        let mut playrates = Vec::new();

        // Prime the rolling average with consistent measurements
        for i in 0..8 {
            let jitter = if i % 2 == 0 { 0.03 } else { -0.03 };
            let drift = 0.05 + jitter; // alternating 0.08 / 0.02
            let action = dc.correct(drift, 1.0);
            if let DriftAction::SetRate { new_playrate } = action {
                playrates.push(new_playrate);
            }
        }

        // After the rolling average is full, check that recent playrates
        // are stable (within a tight band around the expected value)
        let recent = &playrates[playrates.len().saturating_sub(4)..];
        let min = recent.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = recent.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let spread = max - min;
        println!("Jitter test: recent playrates = {recent:?}, spread = {spread:.6}");

        // The smoothed drift should be ~0.05, giving playrate ~1.075.
        // Spread should be very small (smoothing absorbs the jitter).
        assert!(
            spread < 0.02,
            "playrate spread {spread:.4} too large — smoothing not working"
        );
        // All rates should be in the expected range for 50ms drift
        for &r in recent {
            assert!(
                r > 1.03 && r < 1.12,
                "playrate {r:.4} outside expected range for 50ms drift"
            );
        }
    }
}
