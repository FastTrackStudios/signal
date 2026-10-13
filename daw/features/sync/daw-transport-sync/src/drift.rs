//! Drift correction: what to do about the difference between this
//! engine and the leader.
//!
//! Given this engine's latest [`AudioSnapshot`] and the leader's
//! [`Position`], both in one clock, [`DriftController::step`] answers:
//!
//! - **Locate** — far out (a start, a seek, a stall): be at the leader's
//!   place at a moment a little ahead, so the engine can land on it
//!   exactly instead of chasing it. Starting is a locate too.
//! - **Rate** — close: run a fraction of a percent fast or slow until the
//!   gap is gone. Proportional-integral: the proportional part closes a
//!   gap (a millisecond bleeds off in about a second at 0.1 % — well under
//!   anything audible, and no clicks, which a seek would make); the
//!   integral part learns the two devices' crystal mismatch (tens of ppm,
//!   constant), which a proportional loop alone would leave as a standing
//!   gap of mismatch × convergence. Clamped to ±`max_rate_deviation`.
//! - **Stop** — the leader stopped.
//! - **Hold** — nothing to do.
//!
//! Inside the deadband (a couple of samples) the rate goes back to the
//! leader's: the two sample clocks' own wander is the noise floor there,
//! and chasing it would only wobble.
//!
//! After a locate, the controller waits for a snapshot taken after it
//! landed before judging again — the engine's report of where it is lags
//! the command by a buffer or two.

use crate::{AudioSnapshot, Position};

/// The controller's tuning.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DriftConfig {
    /// A gap below this is left alone (seconds). 50 µs ≈ 2 samples at 48 kHz.
    pub deadband_seconds: f64,
    /// How long a gap takes to close by rate (seconds).
    pub convergence_seconds: f64,
    /// The furthest the rate moves from the leader's (0.01 = ±1 %).
    pub max_rate_deviation: f64,
    /// How long the integral takes to learn a standing mismatch (seconds;
    /// 0 turns it off).
    pub integral_seconds: f64,
    /// A gap above this is closed by a locate, not by rate (seconds).
    pub locate_threshold_seconds: f64,
    /// How far ahead a locate is scheduled (microseconds) — enough for the
    /// command to reach the audio thread before the moment it names.
    pub locate_lead_micros: f64,
    /// A leader position older than this is not followed (microseconds).
    pub max_position_age_micros: f64,
}

impl Default for DriftConfig {
    fn default() -> Self {
        Self {
            deadband_seconds: 50e-6,
            convergence_seconds: 1.0,
            max_rate_deviation: 0.01,
            integral_seconds: 4.0,
            locate_threshold_seconds: 0.020,
            locate_lead_micros: 60_000.0,
            max_position_age_micros: 2_000_000.0,
        }
    }
}

/// What this engine should do now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Correction {
    /// Nothing.
    Hold,
    /// Run at this rate.
    Rate(f64),
    /// Be at `position` at `at_micros` (the controller's clock), playing
    /// or not, at `rate`.
    Locate {
        at_micros: f64,
        position: f64,
        playing: bool,
        rate: f64,
    },
    /// Stop, resting at `position`.
    Stop { position: f64 },
}

/// One engine's drift controller.
#[derive(Clone, Debug)]
pub struct DriftController {
    config: DriftConfig,
    /// A locate is landing: judge nothing until a snapshot after this.
    settling_until: f64,
    /// The rate last asked for.
    rate: f64,
    /// The last gap measured (seconds; local − leader), for diagnostics.
    last_drift: Option<f64>,
    /// The learned standing correction (a rate fraction: the crystal
    /// mismatch).
    integral: f64,
    /// When the last gap was measured (the snapshot's time), µs.
    measured_at: Option<f64>,
}

impl Default for DriftController {
    fn default() -> Self {
        Self::new(DriftConfig::default())
    }
}

impl DriftController {
    #[must_use]
    pub const fn new(config: DriftConfig) -> Self {
        Self {
            config,
            settling_until: f64::NEG_INFINITY,
            rate: 1.0,
            last_drift: None,
            integral: 0.0,
            measured_at: None,
        }
    }

    #[must_use]
    pub const fn config(&self) -> &DriftConfig {
        &self.config
    }

    /// The last gap measured, seconds (this engine minus the leader) —
    /// `None` when not both playing.
    #[must_use]
    pub const fn last_drift(&self) -> Option<f64> {
        self.last_drift
    }

    /// The rate last asked for.
    #[must_use]
    pub const fn rate(&self) -> f64 {
        self.rate
    }

    /// Forget any locate in flight and any rate in force (a new leader, a
    /// new song).
    pub fn reset(&mut self) {
        self.settling_until = f64::NEG_INFINITY;
        self.rate = 1.0;
        self.last_drift = None;
        self.integral = 0.0;
        self.measured_at = None;
    }

    /// The learned standing correction — this device against the
    /// leader's, as a rate fraction (−80e-6: this one runs 80 ppm fast).
    #[must_use]
    pub const fn learned_mismatch(&self) -> f64 {
        self.integral
    }

    /// What to do, given this engine's latest snapshot and the leader's
    /// position, both stamped in the same clock, at `now_micros` in it.
    pub fn step(
        &mut self,
        local: &AudioSnapshot,
        leader: &Position,
        now_micros: f64,
    ) -> Correction {
        let c = self.config;
        if now_micros - leader.host_micros > c.max_position_age_micros {
            self.last_drift = None;
            return self.back_to(1.0);
        }
        // A locate is landing: the snapshot must be from after it.
        if local.host_micros < self.settling_until {
            return Correction::Hold;
        }
        match (leader.is_playing, local.is_playing) {
            (false, true) => {
                self.last_drift = None;
                self.measured_at = None;
                self.rate = 1.0;
                Correction::Stop {
                    position: leader.playhead_seconds,
                }
            }
            (false, false) => {
                self.last_drift = None;
                if (local.playhead_seconds - leader.playhead_seconds).abs()
                    > c.locate_threshold_seconds
                {
                    self.locate(leader, now_micros, false)
                } else {
                    self.back_to(1.0)
                }
            }
            (true, false) => {
                self.last_drift = None;
                self.locate(leader, now_micros, true)
            }
            (true, true) => {
                let drift = local.playhead_seconds - leader.at(local.host_micros);
                self.last_drift = Some(drift);
                if drift.abs() > c.locate_threshold_seconds {
                    return self.locate(leader, now_micros, true);
                }
                // The integral learns from every measurement (a new
                // snapshot only), deadband or not: the mismatch is there
                // either way.
                let since = self
                    .measured_at
                    .map_or(0.0, |at| ((local.host_micros - at) * 1e-6).max(0.0));
                if since > 0.0 && c.integral_seconds > 0.0 {
                    let learn = -drift / (c.convergence_seconds * c.integral_seconds) * since;
                    self.integral =
                        (self.integral + learn).clamp(-c.max_rate_deviation, c.max_rate_deviation);
                }
                self.measured_at = Some(local.host_micros);
                let proportional = if drift.abs() < c.deadband_seconds {
                    0.0
                } else {
                    -drift / c.convergence_seconds
                };
                let nudge = (proportional + self.integral)
                    .clamp(-c.max_rate_deviation, c.max_rate_deviation);
                self.back_to(leader.playrate * (1.0 + nudge))
            }
        }
    }

    fn back_to(&mut self, rate: f64) -> Correction {
        if (rate - self.rate).abs() > 1e-7 {
            self.rate = rate;
            Correction::Rate(rate)
        } else {
            Correction::Hold
        }
    }

    fn locate(&mut self, leader: &Position, now_micros: f64, playing: bool) -> Correction {
        let at_micros = now_micros + self.config.locate_lead_micros;
        self.settling_until = at_micros;
        // What was learned still holds (the same two devices); the time
        // base for learning starts over.
        self.measured_at = None;
        self.rate = leader.playrate;
        Correction::Locate {
            at_micros,
            position: leader.at(at_micros),
            playing,
            rate: leader.playrate,
        }
    }
}
