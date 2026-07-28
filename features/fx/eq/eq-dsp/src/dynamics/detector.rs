//! Shared level detector: peak/RMS-mix level → dB → soft-knee window →
//! 0..1 drive → attack/release follower with punch-smooth blend.
//!
//! Used by dynamic bands (per band), the spectral engine (per bin,
//! vectorized), and the transient splitter's realtime mode.

use super::histogram::LoudnessHistogram;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectorParams {
    /// Threshold in dB (used when `auto` is off; while auto is on it
    /// acts as an OFFSET around the learned threshold).
    pub threshold_db: f64,
    /// Soft-knee width in dB (0..32).
    pub knee_db: f64,
    /// Attack time in ms (0..1000).
    pub attack_ms: f64,
    /// Release time in ms (0..5000).
    pub release_ms: f64,
    /// RMS window in ms (0 = pure peak).
    pub rms_ms: f64,
    /// Peak↔RMS blend (0 = peak, 1 = RMS).
    pub rms_mix: f64,
    /// Punch-smooth blend (0 = plain AR, 1 = peak-hold smoothed).
    pub smooth: f64,
    /// Learn the threshold from a decaying loudness histogram.
    pub auto: bool,
    /// Relative mode: detect `side_db − program_db` (band loud relative
    /// to the mix) instead of absolute side level.
    pub relative: bool,
}

impl Default for DetectorParams {
    fn default() -> Self {
        Self {
            threshold_db: -40.0,
            knee_db: 8.0,
            attack_ms: 100.0,
            release_ms: 500.0,
            rms_ms: 10.0,
            rms_mix: 0.5,
            smooth: 1.0,
            auto: false,
            relative: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Detector {
    pub params: DetectorParams,
    hist: LoudnessHistogram,
    /// RMS accumulator (one-pole mean-square — window-free, cheap).
    ms_state: f64,
    ms_coeff: f64,
    /// Program-level mean-square for relative mode.
    prog_ms_state: f64,
    /// Follower states: plain AR and the peak-hold ("punch") variant.
    ar_state: f64,
    hold_state: f64,
    attack_coeff: f64,
    release_coeff: f64,
    /// Histogram push decimation.
    push_countdown: u32,
    sample_rate: f64,
}

/// One-pole coefficient with the 1 kHz-referenced constant used across
/// the fx tree: exp(−2π·1000 / (sr·ms)).
#[inline]
fn ar_coeff(ms: f64, sample_rate: f64) -> f64 {
    if ms <= 0.0 {
        1.0
    } else {
        1.0 - (-2.0 * core::f64::consts::PI * 1000.0 / (sample_rate * ms)).exp()
    }
}

impl Detector {
    pub fn new(sample_rate: f64) -> Self {
        let mut d = Self {
            params: DetectorParams::default(),
            hist: LoudnessHistogram::new(),
            ms_state: 0.0,
            ms_coeff: 0.0,
            prog_ms_state: 0.0,
            ar_state: 0.0,
            hold_state: 0.0,
            attack_coeff: 1.0,
            release_coeff: 1.0,
            push_countdown: 0,
            sample_rate,
        };
        d.update(sample_rate);
        d
    }

    /// Recompute coefficients. Call after changing time params — never
    /// per sample.
    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.attack_coeff = ar_coeff(self.params.attack_ms.max(0.05), sample_rate);
        self.release_coeff = ar_coeff(self.params.release_ms.max(0.05), sample_rate);
        self.ms_coeff = ar_coeff(self.params.rms_ms.max(0.1), sample_rate);
    }

    /// Effective (threshold, knee) — learned values while auto is on.
    pub fn effective_threshold(&self) -> (f64, f64) {
        if self.params.auto {
            if let Some((thr, knee)) = self.hist.learned() {
                // The user's threshold knob re-centers as an offset
                // around the learned value.
                return (thr + self.params.threshold_db * 0.25, knee);
            }
        }
        (self.params.threshold_db, self.params.knee_db)
    }

    /// One sample of the side signal (and the program signal for
    /// relative mode) → drive d ∈ [0, 1].
    #[inline]
    pub fn tick(&mut self, side: f64, program: f64) -> f64 {
        // Level: blend instantaneous |x| with a one-pole RMS.
        let sq = side * side;
        if self.ms_state <= 1.0e-18 {
            self.ms_state = sq;
        }
        self.ms_state += (sq - self.ms_state) * self.ms_coeff;
        let peak = side.abs();
        let rms = self.ms_state.sqrt();
        let level = peak + (rms - peak) * self.params.rms_mix.clamp(0.0, 1.0);
        let mut level_db = 20.0 * level.max(1.0e-10).log10();

        if self.params.relative {
            let psq = program * program;
            if self.prog_ms_state <= 1.0e-18 {
                self.prog_ms_state = psq;
            }
            self.prog_ms_state += (psq - self.prog_ms_state) * self.ms_coeff;
            let prog_db = 10.0 * self.prog_ms_state.max(1.0e-20).log10();
            level_db -= prog_db;
        }

        // Auto-threshold bookkeeping at ~1 kHz cadence.
        if self.params.auto {
            if self.push_countdown == 0 {
                self.push_countdown = (self.sample_rate / 1000.0) as u32;
                self.hist.push(level_db);
            }
            self.push_countdown -= 1;
        }

        // Soft-knee window → raw drive 0..1 (squared for an S-curve).
        let (thr, knee) = self.effective_threshold();
        let knee = knee.max(0.01);
        let raw = ((level_db - (thr - knee)) / (2.0 * knee)).clamp(0.0, 1.0);
        let drive = raw * raw;

        // Plain AR follower.
        let coeff = if drive > self.ar_state {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.ar_state += (drive - self.ar_state) * coeff;

        // Punch variant: release-tracked peak hold, then attack-smoothed.
        self.hold_state = (self.hold_state - self.hold_state * self.release_coeff).max(drive);
        let punched = self.ar_state + (self.hold_state - self.ar_state) * self.attack_coeff;

        let s = self.params.smooth.clamp(0.0, 1.0);
        (self.ar_state + (punched - self.ar_state) * s).clamp(0.0, 1.0)
    }

    pub fn reset(&mut self) {
        self.ms_state = 0.0;
        self.prog_ms_state = 0.0;
        self.ar_state = 0.0;
        self.hold_state = 0.0;
        self.push_countdown = 0;
        self.hist.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn rides_up_on_loud_and_releases() {
        let mut d = Detector::new(SR);
        d.params.threshold_db = -20.0;
        d.params.attack_ms = 5.0;
        d.params.release_ms = 50.0;
        d.update(SR);
        let mut peak = 0.0f64;
        for _ in 0..4800 {
            peak = peak.max(d.tick(0.8, 0.8)); // ≈ −2 dB, well over
        }
        assert!(peak > 0.9, "loud input should drive to 1: {peak}");
        let mut out = 1.0;
        for _ in 0..48_000 {
            out = d.tick(0.0, 0.0);
        }
        assert!(out < 0.05, "should release: {out}");
    }

    #[test]
    fn below_threshold_stays_quiet() {
        let mut d = Detector::new(SR);
        d.params.threshold_db = -20.0;
        d.params.knee_db = 4.0;
        d.update(SR);
        let mut peak = 0.0f64;
        for _ in 0..9600 {
            peak = peak.max(d.tick(0.005, 0.005)); // ≈ −46 dB
        }
        assert!(peak < 0.01, "quiet input must not trigger: {peak}");
    }

    #[test]
    fn auto_threshold_adapts() {
        let mut d = Detector::new(SR);
        d.params.auto = true;
        d.params.threshold_db = 0.0; // offset knob centered
        d.update(SR);
        // Program at ≈ −18 dB for 2 s.
        for i in 0..96_000 {
            let x = 0.125 * (core::f64::consts::TAU * 300.0 * i as f64 / SR).sin();
            d.tick(x, x);
        }
        let (thr, knee) = d.effective_threshold();
        assert!(
            (-35.0..=-12.0).contains(&thr),
            "learned threshold near program level: {thr}"
        );
        assert!(knee >= 5.0, "knee floor: {knee}");
    }

    #[test]
    fn relative_mode_needs_band_dominance() {
        // Side at −20 dB inside a −6 dB program: relative level ≈ −14 dB
        // → below a −6 dB relative threshold. Same side alone (program
        // equally quiet) → relative ≈ 0 dB → over threshold.
        let run = |program: f64| -> f64 {
            let mut d = Detector::new(SR);
            d.params.relative = true;
            d.params.threshold_db = -6.0;
            d.params.attack_ms = 1.0;
            d.update(SR);
            let mut peak = 0.0f64;
            for _ in 0..9600 {
                peak = peak.max(d.tick(0.1, program));
            }
            peak
        };
        assert!(run(0.5) < 0.2, "band buried in the mix must not trigger");
        assert!(run(0.1) > 0.6, "band dominating the mix must trigger");
    }
}
