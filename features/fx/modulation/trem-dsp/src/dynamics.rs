//! Dynamics envelope — Tremolator-inspired envelope follower that modulates
//! tremolo rate and depth based on input level.
//!
//! Two modes: `Env` (proportional) and `Gate` (on/off threshold).

use audiocore_dsp::envelope::EnvelopeFollower;

/// Dynamics mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynMode {
    /// Proportional: mod_amount scales with how far the level is above threshold.
    Env,
    /// Gate: mod_amount is 1.0 above threshold, 0.0 below.
    Gate,
}

impl Default for DynMode {
    fn default() -> Self {
        DynMode::Env
    }
}

/// Envelope follower system that outputs rate and depth modulation amounts
/// based on input signal level relative to a threshold.
pub struct TremDynamics {
    /// Threshold in dB (-60..0).
    pub threshold_db: f64,
    /// Attack time in ms (0..5000).
    pub attack_ms: f64,
    /// Release time in ms (0..5000).
    pub release_ms: f64,
    /// Dynamics mode.
    pub mode: DynMode,
    /// Rate modulation in octaves (-4..4). Bipolar.
    pub rate_mod: f64,
    /// Depth modulation (-1..1). Bipolar.
    pub depth_mod: f64,

    /// Level envelope follower (tracks input RMS/peak).
    level_env: EnvelopeFollower,
    /// Smoothing envelope for mod_amount (attack/release).
    mod_env: EnvelopeFollower,
    /// Current smoothed modulation amount (0..1).
    mod_amount: f64,

    sample_rate: f64,
}

impl TremDynamics {
    pub fn new() -> Self {
        Self {
            threshold_db: -30.0,
            attack_ms: 10.0,
            release_ms: 200.0,
            mode: DynMode::Env,
            rate_mod: 0.0,
            depth_mod: 0.0,
            level_env: EnvelopeFollower::new(0.0),
            mod_env: EnvelopeFollower::new(0.0),
            mod_amount: 0.0,
            sample_rate: 48000.0,
        }
    }

    /// Update coefficients for a new sample rate or after parameter changes.
    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        // Level follower: fast attack, moderate release for tracking
        self.level_env.set_times_ms(2.0, 50.0, sample_rate);
        // Mod amount smoother: user-controlled attack/release
        self.mod_env
            .set_times_ms(self.attack_ms, self.release_ms, sample_rate);
    }

    /// Process one sample's input level and return `(rate_scale, depth_offset)`.
    ///
    /// - `rate_scale`: multiply the base modulation rate by this value (around 1.0).
    /// - `depth_offset`: add this to the base tremolo depth.
    ///
    /// `input_level` should be the absolute value of the input sample (or peak of L/R).
    #[inline]
    pub fn tick(&mut self, input_level: f64) -> (f64, f64) {
        // Track the input level
        let level = self.level_env.tick(input_level);

        // Convert to dB
        let level_db = if level > 1e-10 {
            20.0 * level.log10()
        } else {
            -120.0
        };

        // Compute raw mod amount based on mode
        let raw = match self.mode {
            DynMode::Env => {
                // Proportional: scale from 0 at threshold to 1 at threshold + range
                // Use a 40 dB range above threshold for full modulation
                let range = 40.0;
                ((level_db - self.threshold_db) / range).clamp(0.0, 1.0)
            }
            DynMode::Gate => {
                if level_db > self.threshold_db {
                    1.0
                } else {
                    0.0
                }
            }
        };

        // Smooth with attack/release
        self.mod_amount = self.mod_env.tick(raw);

        // Compute outputs
        let rate_scale = 2.0_f64.powf(self.rate_mod * self.mod_amount);
        let depth_offset = self.depth_mod * self.mod_amount;

        (rate_scale, depth_offset)
    }

    /// Get the current smoothed mod amount (0..1).
    #[inline]
    pub fn mod_amount(&self) -> f64 {
        self.mod_amount
    }

    /// Returns true if dynamics processing is effectively bypassed
    /// (both rate_mod and depth_mod are zero).
    #[inline]
    pub fn is_bypassed(&self) -> bool {
        self.rate_mod.abs() < 1e-10 && self.depth_mod.abs() < 1e-10
    }

    pub fn reset(&mut self) {
        self.level_env.reset(0.0);
        self.mod_env.reset(0.0);
        self.mod_amount = 0.0;
    }
}

impl Default for TremDynamics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn silence_gives_no_modulation() {
        let mut d = TremDynamics::new();
        d.threshold_db = -30.0;
        d.rate_mod = 2.0;
        d.depth_mod = 0.5;
        d.update(SR);

        // Feed silence
        for _ in 0..4800 {
            d.tick(0.0);
        }

        let (rate, depth) = d.tick(0.0);
        assert!(
            (rate - 1.0).abs() < 0.01,
            "Silence should give rate_scale ~1.0: {rate}"
        );
        assert!(
            depth.abs() < 0.01,
            "Silence should give depth_offset ~0.0: {depth}"
        );
    }

    #[test]
    fn loud_signal_gives_modulation_env_mode() {
        let mut d = TremDynamics::new();
        d.threshold_db = -60.0;
        d.rate_mod = 2.0;
        d.depth_mod = 0.5;
        d.mode = DynMode::Env;
        d.attack_ms = 1.0; // fast
        d.release_ms = 100.0;
        d.update(SR);

        // Feed loud signal (0 dB)
        for _ in 0..4800 {
            d.tick(1.0);
        }

        let (rate, depth) = d.tick(1.0);
        assert!(rate > 1.5, "Loud signal should increase rate: {rate}");
        assert!(depth > 0.3, "Loud signal should add depth: {depth}");
    }

    #[test]
    fn gate_mode_binary() {
        let mut d = TremDynamics::new();
        d.threshold_db = -20.0;
        d.rate_mod = 1.0;
        d.depth_mod = 1.0;
        d.mode = DynMode::Gate;
        d.attack_ms = 0.0; // instant
        d.release_ms = 0.0;
        d.update(SR);

        // Below threshold
        let (rate_low, depth_low) = d.tick(0.01); // ~-40 dB
        assert!(
            (rate_low - 1.0).abs() < 0.1,
            "Below threshold: rate ~1.0: {rate_low}"
        );
        assert!(
            depth_low.abs() < 0.1,
            "Below threshold: depth ~0.0: {depth_low}"
        );

        // Above threshold
        for _ in 0..480 {
            d.tick(1.0);
        }
        let (rate_high, depth_high) = d.tick(1.0); // 0 dB
        assert!(rate_high > 1.5, "Above threshold: rate > 1.5: {rate_high}");
        assert!(
            depth_high > 0.5,
            "Above threshold: depth > 0.5: {depth_high}"
        );
    }

    #[test]
    fn bypassed_when_zero_mods() {
        let mut d = TremDynamics::new();
        d.rate_mod = 0.0;
        d.depth_mod = 0.0;
        assert!(d.is_bypassed());
    }

    #[test]
    fn no_nan() {
        let mut d = TremDynamics::new();
        d.rate_mod = 4.0;
        d.depth_mod = 1.0;
        d.update(SR);

        for &level in &[0.0, 1e-20, 0.001, 0.5, 1.0, 2.0] {
            let (rate, depth) = d.tick(level);
            assert!(rate.is_finite(), "NaN rate for level {level}");
            assert!(depth.is_finite(), "NaN depth for level {level}");
        }
    }
}
