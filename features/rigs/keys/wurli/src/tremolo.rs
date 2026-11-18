//! Wurlitzer 200A tremolo — feature-toggled between circuit (default) and behavioral oscillator.
//!
//! Default: melange-generated Twin-T oscillator circuit (real waveform shape).
//! `--features legacy-tremolo`: behavioral sine LFO (for A/B testing).
//!
//! Both share the same CdS LDR model (LED drive → asymmetric envelope → power-law R).

#[cfg(feature = "legacy-tremolo")]
use std::f64::consts::PI;

#[cfg(not(feature = "legacy-tremolo"))]
use crate::gen_tremolo;

/// CdS LDR parameters (shared between behavioral and circuit paths).
const ATTACK_TAU: f64 = 0.003;
const RELEASE_TAU: f64 = 0.050;
/// LDR bright-phase floor. Set to match the preamp's documented
/// "tremolo bright" calibration point (19 kΩ total shunt → 12.61 dB gain).
/// Shunt is `R_SHUNT_SERIES + r_ldr`, so r_ldr_min = 19_000 − 680 ≈ 18 320.
/// Previously 50 Ω (datasheet CdS min) — but shunt values below about
/// 1 kΩ clamp at the preamp's `.runtime R 1k 1Meg` floor and waste drive.
/// Keeping the bright phase at the preamp's documented bright point gives
/// monotonic depth→swing behavior and hits the 6.1 dB gain-range target.
const R_LDR_MIN: f64 = 18_320.0;
const R_LDR_MAX: f64 = 1_000_000.0;
const GAMMA: f64 = 1.1;

/// Fixed series resistance in the LDR shunt path from fb_junction to GND:
/// R-??? (680 Ω on LG-1 pin 5) in series with the LDR itself. The 50 kΩ
/// VIBRATO pot + 18 kΩ are in the LED drive path (controlling LED current
/// and hence brightness), **not** in this shunt path — see `set_depth`.
/// Pre-Apr-2026 versions double-counted the pot into both paths, which
/// flattened the depth→swing curve at high settings (100 % actually milder
/// than 75 %). See `memory/known-issues.md` for the full write-up.
const R_SHUNT_SERIES: f64 = 680.0;

/// Twin-T oscillator output voltage range (from ngspice/melange validation).
#[cfg(not(feature = "legacy-tremolo"))]
const V_OUT_MIN: f64 = 0.70;
#[cfg(not(feature = "legacy-tremolo"))]
const V_OUT_MAX: f64 = 10.95;

pub struct Tremolo {
    // --- Oscillator state ---
    /// Behavioral: LFO phase
    #[cfg(feature = "legacy-tremolo")]
    phase: f64,
    #[cfg(feature = "legacy-tremolo")]
    phase_inc: f64,

    /// Circuit: Twin-T oscillator state
    #[cfg(not(feature = "legacy-tremolo"))]
    osc_state: gen_tremolo::CircuitState,

    // --- Shared LDR model ---
    depth: f64,
    r_ldr: f64,
    ldr_envelope: f64,
    ldr_attack: f64,
    ldr_release: f64,
    r_ldr_max: f64,
    gamma: f64,
    ln_r_max: f64,
    ln_min_minus_max: f64,
}

/// Fixed oscillator rate for the legacy behavioral LFO (Hz).
#[cfg(feature = "legacy-tremolo")]
const LEGACY_RATE_HZ: f64 = 5.63;

impl Tremolo {
    pub fn new(depth: f64, sample_rate: f64) -> Self {
        Self {
            #[cfg(feature = "legacy-tremolo")]
            phase: 0.0,
            #[cfg(feature = "legacy-tremolo")]
            phase_inc: 2.0 * PI * LEGACY_RATE_HZ / sample_rate,

            #[cfg(not(feature = "legacy-tremolo"))]
            osc_state: {
                let mut s = gen_tremolo::CircuitState::default();
                if (sample_rate - gen_tremolo::SAMPLE_RATE).abs() > 0.5 {
                    s.set_sample_rate(sample_rate);
                }
                // Settle oscillator to reach steady-state amplitude
                for _ in 0..(sample_rate * 2.0) as usize {
                    gen_tremolo::process_sample(0.0, &mut s);
                }
                s
            },

            depth,
            r_ldr: R_LDR_MAX,
            ldr_envelope: 0.0,
            ldr_attack: (-1.0 / (ATTACK_TAU * sample_rate)).exp(),
            ldr_release: (-1.0 / (RELEASE_TAU * sample_rate)).exp(),
            r_ldr_max: R_LDR_MAX,
            gamma: GAMMA,
            ln_r_max: R_LDR_MAX.ln(),
            ln_min_minus_max: R_LDR_MIN.ln() - R_LDR_MAX.ln(),
        }
    }

    pub fn set_depth(&mut self, depth: f64) {
        self.depth = depth.clamp(0.0, 1.0);
    }

    pub fn process(&mut self) -> f64 {
        // Step 1: Get oscillator drive (0..1)
        let led_drive = self.oscillator_drive() * self.depth;

        // Step 2: CdS LDR envelope (asymmetric attack/release)
        let coeff = if led_drive > self.ldr_envelope {
            self.ldr_attack
        } else {
            self.ldr_release
        };
        self.ldr_envelope = led_drive + coeff * (self.ldr_envelope - led_drive);

        // Step 3: CdS power-law resistance
        let drive = self.ldr_envelope.clamp(0.0, 1.0);
        if drive < 1e-6 {
            self.r_ldr = self.r_ldr_max;
        } else {
            let log_r = self.ln_r_max + self.ln_min_minus_max * drive.powf(self.gamma);
            self.r_ldr = log_r.exp();
        }

        R_SHUNT_SERIES + self.r_ldr
    }

    /// Get the oscillator's LED drive signal (0..1).
    #[cfg(feature = "legacy-tremolo")]
    fn oscillator_drive(&mut self) -> f64 {
        let lfo = self.phase.sin();
        self.phase += self.phase_inc;
        if self.phase >= 2.0 * PI {
            self.phase -= 2.0 * PI;
        }
        lfo.max(0.0) // half-wave rectify
    }

    #[cfg(not(feature = "legacy-tremolo"))]
    fn oscillator_drive(&mut self) -> f64 {
        let v_out = gen_tremolo::process_sample(0.0, &mut self.osc_state)[0];
        // Map collector voltage to LED drive: low V = bright LED = high drive
        ((V_OUT_MAX - v_out) / (V_OUT_MAX - V_OUT_MIN)).clamp(0.0, 1.0)
    }

    pub fn current_resistance(&self) -> f64 {
        R_SHUNT_SERIES + self.r_ldr
    }

    pub fn reset(&mut self) {
        #[cfg(feature = "legacy-tremolo")]
        {
            self.phase = 0.0;
        }
        #[cfg(not(feature = "legacy-tremolo"))]
        {
            self.osc_state.reset();
        }
        self.ldr_envelope = 0.0;
        self.r_ldr = self.r_ldr_max;
    }
}
