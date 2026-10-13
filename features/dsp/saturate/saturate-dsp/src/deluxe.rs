//! Deluxe — a Level-Loc-style crushing limiter with a distortion stage.
//!
//! A clean-room recreation of a well-known "Devil" limiter plug-in's
//! algorithm from black-box measurements of it: the same controls
//! (Crush, Crunch, Darkness, Release, Mix) doing the same things, aimed
//! at sounding the same rather than nulling. Each stage is a law with a
//! handful of fitted constants, listed with it below.
//!
//! What measurement showed the reference to be:
//!
//! ```text
//!  x ─ A ─ ×crush ─ ×g ─┬─ ×crunch ─ [f(·) ─ D(darkness)]×4 ─ HP 9.45 Hz ┐
//!                                 │                               ├─ mix ─ y
//!                     detector on (L+R)/2 ── g             dry ───┘
//! ```
//!
//! - `A` is the whole linear colour, *before* the curve — a coupling low
//!   shelf (zeros 10.8 Hz under poles 42 Hz) and a gentle high shelf: `H2/H1²`
//!   is flat from 100 Hz to 8 kHz, so nothing but a 9.45 Hz coupling
//!   high-pass follows the curve.
//! - Darkness is one third-order low-pass after the curve — a resonant
//!   two-pole section with a real pole above it — whose cutoff is close to
//!   linear in the knob above a ≈700 Hz floor (Q easing from ≈0.86 to ≈0.65
//!   as it closes), with a little feedthrough and a passband loss that sets
//!   in below ~1.5 kHz, like an RC network loading more as it closes. One law, fitted to 24 measured
//!   steady-state (multitone) positions at once. Its post-curve place is
//!   measured too: the detector never hears it,
//!   and it darkens the harmonics exactly as a filter after the curve
//!   would. (Single-impulse captures of it mislead: the reference's
//!   Darkness does not act as a fixed filter on an isolated spike.)
//! - `f` is a static asymmetric soft clip (rails ≈ +0.30 / −0.38),
//!   expansive at first, which is where the even harmonics come from.
//! - Crunch is gain *into the curve*, 3 dB per knob unit, after the
//!   detector tap — which is why it distorts instead of compressing.
//!   Crush is gain into the VCA along a measured 11-point taper.
//! - The detector is an *integrating* feedback detector on the stereo
//!   sum: a cap charged by a log-domain rectified (and 0.3 ms smoothed)
//!   excess of `|d|` over a
//!   threshold and discharged through a resistor, with gain reduction in
//!   dB proportional to the cap. That one mechanism gives the reference's
//!   ~10 ms overshoot after an onset, its dB-exponential release, its
//!   ≈ 15:1 static ratio, and the way it sits at the gain-reduction floor
//!   for hundreds of ms after a loud burst (the cap charged far past it).
//!   Release On discharges faster than Off. On top of it runs a fast peak
//!   path (a few-ms attack, tens-of-ms release) on the instantaneous level:
//!   the per-hit dip-and-recover that makes it pump on drums. Both were
//!   fitted on the reference's output envelope on music, not only on tones.
//! - Mix is equal-power with a unity dry path.

use core::f64::consts::{FRAC_PI_2, PI};

// ── The measured laws ────────────────────────────────────────────────────
//
// Every constant below is a parameter of the law it sits under, fitted to
// black-box measurements of the reference (the saturator-models tools).

// Crush: dB into the VCA — an audio-taper pot (log term) in series with a
// linear one: gain = POT·log10(1 + RATE·knob) + PER_UNIT·knob, 0 dB at 0.
// Within 0.6 dB of the measured taper (the reference's own steps wobble).
const CRUSH_POT_DB: f64 = 2.713_9;
const CRUSH_POT_RATE: f64 = 1.010_5;
const CRUSH_DB_PER_UNIT: f64 = 3.072;
/// Crunch: dB into the curve per knob unit.
const CRUNCH_DB_PER_UNIT: f64 = 3.0;

/// One diode of the curve's breakpoint network: it switches on at `at`
/// (curve-input units) and changes the slope by `slope` over a soft knee
/// `knee` wide.
#[derive(Clone, Copy, Debug)]
struct Diode {
    at: f64,
    slope: f64,
    knee: f64,
}

/// The curve's diodes for positive inputs: an expanding junction, then two
/// clamps; the slopes take the curve flat (rail ≈ +0.30).
const CURVE_POS: [Diode; 3] = [
    Diode { at: 0.024_427_4, slope: 0.230_417, knee: 0.013_059_3 },
    Diode { at: 0.202_496, slope: -0.669_985, knee: 0.017_875_4 },
    Diode { at: 0.308_059, slope: -0.529_683, knee: 0.004 },
];
/// …and for negative inputs, conducting later (rail ≈ −0.38).
const CURVE_NEG: [Diode; 3] = [
    Diode { at: 0.149_334, slope: 0.289_693, knee: 0.014_315_8 },
    Diode { at: 0.291_741, slope: -0.688_952, knee: 0.032_801_2 },
    Diode { at: 0.382_739, slope: -0.600_826, knee: 0.012_598_8 },
];

/// Coupling high-pass after the curve, Hz.
const POST_HP_HZ: f64 = 9.45;

// The colour filter before the VCA: a coupling low shelf (resonant zero pair
// under a pole pair) and a first-order high shelf, scaled to the measured
// gain at 1 kHz.
const PRE_LOW_ZERO_HZ: f64 = 10.7959;
const PRE_LOW_ZERO_Q: f64 = 1.058_26;
const PRE_LOW_POLE_HZ: f64 = 42.2466;
const PRE_LOW_POLE_Q: f64 = 0.530_222;
const PRE_HIGH_POLE_HZ: f64 = 12_635.2;
const PRE_HIGH_ZERO_HZ: f64 = 18_475.2;
const PRE_GAIN_1K: f64 = 1.907_73;

// Darkness: a resonant pair at fc = scale·knob^exp + floor with
// Q = q0 + q1·ln(knob / 1 kHz), a real pole at pole_ratio·fc, a signed
// feedthrough around them, and a passband loss of loss·ln(1 + (knee/knob)^exp)
// dB. Above the first measured position it fades in from the open knob.
const DARK_FC_SCALE: f64 = 0.532_242;
const DARK_FC_EXP: f64 = 1.072_92;
const DARK_FC_FLOOR_HZ: f64 = 707.144;
const DARK_Q0: f64 = 0.662_979;
const DARK_Q1: f64 = 0.068_778_3;
const DARK_POLE_RATIO: f64 = 3.042_24;
const DARK_FEEDTHROUGH: f64 = -0.001_881_65;
const DARK_LOSS_DB: f64 = 1.375_12;
const DARK_LOSS_KNEE_HZ: f64 = 1334.87;
const DARK_LOSS_EXP: f64 = 4.005_95;
const DARK_FIRST_HZ: f64 = 17_150.5;

// The integrating detector — see [`DetectorParams`] for what each one does.
const DET_OFFSET_DB: f64 = -0.126_052;
const DET_THETA_DB: f64 = -12.4385;
const DET_SMOOTH_MS: f64 = 11.2572;
const DET_POWER: f64 = 1.149_21;
const DET_SOFT: f64 = 0.002_705_82;
const DET_CHARGE_PER_S: f64 = 1.990_01e8;
const DET_CHARGE_MAX_DB_PER_MS: f64 = 184.978;
const DET_TAU_ON_MS: f64 = 120.535;
const DET_TAU_OFF_MS: f64 = 180.056;
const DET_FLOOR_DB: f64 = 49.3952;
const DET_FLOOR_KNEE_DB: f64 = 17.915;
const DET_FAST_K: f64 = 11.8737;
const DET_FAST_T_DB: f64 = -18.8279;
const DET_FAST_W_DB: f64 = 1.974_79;
const DET_FAST_ATTACK_MS: f64 = 3.946_93;
const DET_FAST_RELEASE_ON_MS: f64 = 101.201;
const DET_FAST_RELEASE_OFF_MS: f64 = 212.597;
const DET_SLOW_WEIGHT: f64 = 0.000_755_579;
const DET_SLOW_CHARGE_MS: f64 = 292.681;
const DET_SLOW_RELEASE_MS: f64 = 8864.6;
const DET_SMOOTH_ATTACK_MS: f64 = 2.028_08;
const DET_DET_FLOOR_DB: f64 = 41.3804;

/// The reference's controls, in its own units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeluxeParams {
    /// 0..=10.
    pub crush: f64,
    /// 0..=10.
    pub crunch: f64,
    /// 500..=20000 Hz (20 kHz is the open position).
    pub darkness_hz: f64,
    /// 0..=100 %.
    pub mix: f64,
    /// The Release switch: On is the faster release.
    pub release_on: bool,
}

impl Default for DeluxeParams {
    fn default() -> Self {
        Self { crush: 0.0, crunch: 0.0, darkness_hz: 20_000.0, mix: 100.0, release_on: false }
    }
}

/// Crush knob (0..=10) → dB into the VCA.
#[must_use]
pub fn crush_db(knob: f64) -> f64 {
    let k = knob.clamp(0.0, 10.0);
    CRUSH_POT_DB * libm::log10(1.0 + CRUSH_POT_RATE * k) + CRUSH_DB_PER_UNIT * k
}

/// The static curve: a diode breakpoint network.
///
/// Each polarity starts at unity slope; one junction then expands it
/// (slope ≈ 1.2) and two clamp diodes take it down to flat, each switching
/// on over a soft knee — the integral of a logistic step, a softplus. Each
/// diode's leakage below its knee (slope σ(−at/knee), value
/// knee·softplus(−at/knee) at zero) is worked out once here and taken out,
/// so both sides leave zero at exactly unity slope and the curve is smooth
/// through it. The two sides differ only in where their diodes conduct.
#[derive(Clone, Copy, Debug)]
pub struct Curve {
    pos: [Prepared; 3],
    neg: [Prepared; 3],
}

/// A diode with its leakage at zero worked out.
#[derive(Clone, Copy, Debug)]
struct Prepared {
    at: f64,
    slope: f64,
    knee: f64,
    leak_slope: f64,
    leak_value: f64,
}

impl Curve {
    #[must_use]
    pub fn new() -> Self {
        let prepare = |d: Diode| {
            let z = -d.at / d.knee;
            Prepared { at: d.at, slope: d.slope, knee: d.knee, leak_slope: 1.0 / (1.0 + libm::exp(-z)), leak_value: d.knee * softplus(z) }
        };
        Self { pos: CURVE_POS.map(prepare), neg: CURVE_NEG.map(prepare) }
    }

    /// The curve at `u` (curve-input units).
    #[inline]
    #[must_use]
    pub fn at(&self, u: f64) -> f64 {
        let a = libm::fabs(u).min(64.0);
        let side = if u >= 0.0 { &self.pos } else { &self.neg };
        let y = side.iter().fold(a, |y, d| y + d.slope * (d.knee * softplus((a - d.at) / d.knee) - d.leak_value - d.leak_slope * a));
        if u >= 0.0 { y } else { -y }
    }
}

impl Default for Curve {
    fn default() -> Self {
        Self::new()
    }
}

/// ln(1 + eˣ), without overflow.
fn softplus(x: f64) -> f64 {
    x.max(0.0) + libm::log1p(libm::exp(-libm::fabs(x)))
}

/// The static picture of the stage, for a display.
///
/// The curve as driven by Crush and Crunch, for an input `x` in −1..1 (full
/// scale), normalised to the curve's rail. The limiter's gain riding is
/// dynamics, not a curve, so it is left out — this is what one cycle of a
/// transient meets.
#[must_use]
pub fn static_curve(p: &DeluxeParams, x: f64) -> f64 {
    let gain = libm::pow(10.0, (crush_db(p.crush) + CRUNCH_DB_PER_UNIT * p.crunch.clamp(0.0, 10.0)) / 20.0);
    let curve = Curve::new();
    let rail = curve.at(1.0).abs().max(curve.at(-1.0).abs());
    curve.at(0.25 * gain * x) / rail
}

#[derive(Clone, Copy, Debug, Default)]
struct Biquad {
    c: [f64; 5],
    z1: f64,
    z2: f64,
}

impl Biquad {
    #[inline]
    fn run(&mut self, x: f64) -> f64 {
        let [b0, b1, b2, a1, a2] = self.c;
        let y = b0 * x + self.z1;
        self.z1 = b1 * x - a1 * y + self.z2;
        self.z2 = b2 * x - a2 * y;
        y
    }

    const fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// Bilinear transform of an analog second-order section
/// `(n2·s² + n1·s + n0) / (d2·s² + d1·s + d0)` with `s = c·(1 − z⁻¹)/(1 + z⁻¹)`.
fn bilinear2(n: [f64; 3], d: [f64; 3], c: f64) -> [f64; 5] {
    let c2 = c * c;
    let [n0, n1, n2] = n;
    let [d0, d1, d2] = d;
    let a0 = d2 * c2 + d1 * c + d0;
    [
        (n2 * c2 + n1 * c + n0) / a0,
        2.0 * (n0 - n2 * c2) / a0,
        (n2 * c2 - n1 * c + n0) / a0,
        2.0 * (d0 - d2 * c2) / a0,
        (d2 * c2 - d1 * c + d0) / a0,
    ]
}

/// |H(e^jω)| of a biquad at `hz`.
fn magnitude(c: [f64; 5], hz: f64, sr: f64) -> f64 {
    let w = 2.0 * PI * hz / sr;
    let (c1, s1, c2, s2) = (libm::cos(w), libm::sin(w), libm::cos(2.0 * w), libm::sin(2.0 * w));
    let [b0, b1, b2, a1, a2] = c;
    let (nr, ni) = (b0 + b1 * c1 + b2 * c2, -(b1 * s1 + b2 * s2));
    let (dr, di) = (1.0 + a1 * c1 + a2 * c2, -(a1 * s1 + a2 * s2));
    libm::sqrt((nr * nr + ni * ni) / (dr * dr + di * di).max(1e-300))
}

/// The fixed colour before the VCA, as two sections: the coupling network —
/// a low shelf whose resonant zero pair sits under its pole pair, so the
/// response falls ≈ 25 dB below 15 Hz — and a gentle first-order high shelf
/// (≈ −1 dB at 10 kHz, −3 dB at 20 kHz). Each is prewarped at its own corner;
/// the pair is scaled to the measured gain at 1 kHz.
fn colour_filter(sr: f64) -> [Biquad; 2] {
    let w = |hz: f64| 2.0 * PI * hz;
    let (wz, wp) = (w(PRE_LOW_ZERO_HZ), w(PRE_LOW_POLE_HZ));
    let low = bilinear2(
        [wz * wz, wz / PRE_LOW_ZERO_Q, 1.0],
        [wp * wp, wp / PRE_LOW_POLE_Q, 1.0],
        wp / libm::tan(PI * PRE_LOW_POLE_HZ / sr),
    );
    let fh = PRE_HIGH_POLE_HZ.min(0.45 * sr);
    let (wh, wz2) = (w(fh), w(PRE_HIGH_ZERO_HZ));
    let high = bilinear2([1.0, 1.0 / wz2, 0.0], [1.0, 1.0 / wh, 0.0], wh / libm::tan(PI * fh / sr));
    let g = PRE_GAIN_1K / (magnitude(low, 1000.0, sr) * magnitude(high, 1000.0, sr)).max(1e-30);
    let [b0, b1, b2, a1, a2] = low;
    [Biquad { c: [b0 * g, b1 * g, b2 * g, a1, a2], ..Biquad::default() }, Biquad { c: high, ..Biquad::default() }]
}

/// Per-channel state: the colour filter, Darkness, the 4× section and the
/// post-curve high-pass.
#[derive(Clone, Debug)]
struct Channel {
    pre: [Biquad; 2],
    /// Darkness: the resonant pair, then the real pole.
    dark: [Biquad; 2],
    /// The curve runs at 4× (the reference oversamples it too: a clipped
    /// 7 kHz tone's aliases sit 47–80 dB down there).
    os: crate::oversample::Oversample4,
    hp_x: f64,
    hp_y: f64,
}

impl Channel {
    fn new(sr: f64) -> Self {
        Self { pre: colour_filter(sr), dark: [Biquad::default(); 2], os: crate::oversample::Oversample4::new(), hp_x: 0.0, hp_y: 0.0 }
    }

    fn reset(&mut self) {
        self.pre.iter_mut().for_each(Biquad::reset);
        self.dark.iter_mut().for_each(Biquad::reset);
        self.os.reset();
        self.hp_x = 0.0;
        self.hp_y = 0.0;
    }

    #[inline]
    fn pre(&mut self, x: f64) -> f64 {
        self.pre.iter_mut().fold(x, |v, s| s.run(v))
    }

    /// The curve and then Darkness, both at 4×: Darkness runs before the
    /// decimator, so it keeps its analog shape up to 20 kHz (a filter at the
    /// base rate bends down into its Nyquist zero there; the reference does
    /// not). `mix` fades it in (0 = the open knob, which skips it), `gain` is
    /// its passband loss.
    #[inline]
    fn shape(&mut self, curve: &Curve, u: f64, mix: f64, gain: f64) -> f64 {
        let Self { os, dark, .. } = self;
        os.process(u, |v| {
            let y = curve.at(v);
            if mix <= 0.0 {
                return y;
            }
            let wet = dark.iter_mut().fold(y, |a, s| s.run(a)) + DARK_FEEDTHROUGH * y;
            mix * gain * wet + (1.0 - mix) * y
        })
    }

    fn set_dark(&mut self, coefs: [[f64; 5]; 2]) {
        for (s, c) in self.dark.iter_mut().zip(coefs) {
            s.c = c;
        }
    }

    #[inline]
    fn post(&mut self, w: f64, b: f64, a: f64) -> f64 {
        let y = b * (w - self.hp_x) + a * self.hp_y;
        self.hp_x = w;
        self.hp_y = y;
        y
    }
}

/// ln(10)/20: dB to natural-log units.
const DB_TO_LN: f64 = core::f64::consts::LN_10 / 20.0;

fn pole_ms(ms: f64, sample_rate: f64) -> f64 {
    if ms <= 0.0 { 0.0 } else { libm::exp(-1000.0 / (ms * sample_rate)) }
}

/// `k · ln(1 + e^(x/k))`: a smooth `max(0, x)`.
fn soft_over(x: f64, k: f64) -> f64 {
    let z = x / k;
    if z > 30.0 { x } else { k * libm::log1p(libm::exp(z)) }
}

/// The detector's constants. [`Default`] is the measured set
/// (the constants above); the fitting tools drive the engine with
/// other sets through [`Deluxe::set_detector`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectorParams {
    /// Calibration gain into the VCA, dB, on top of the Crush taper.
    pub offset_db: f64,
    /// Rectifier threshold θ, dB re full scale at the detector tap.
    pub theta_db: f64,
    /// Smoothing of the rectified level before the rectifier law, ms.
    pub smooth_ms: f64,
    /// Rectifier law: `(x^p − 1)/p` of `x = level/θ` (`p → 0` is `ln x`).
    pub power: f64,
    /// Rectifier corner softness.
    pub soft: f64,
    /// Cap charge rate (dB of gain reduction per second per unit excess)…
    pub charge_per_s: f64,
    /// …and its limit, dB per ms.
    pub charge_max_db_per_ms: f64,
    /// Cap discharge, Release On / Off, ms.
    pub tau_on_ms: f64,
    pub tau_off_ms: f64,
    /// Most gain reduction, dB, and how softly the cap approaches it.
    pub floor_db: f64,
    pub floor_knee_db: f64,
    /// Fast peak path: dB of reduction per dB over `fast_t_db`, knee width,
    /// attack, and release (On / Off).
    pub fast_k: f64,
    pub fast_t_db: f64,
    pub fast_w_db: f64,
    pub fast_attack_ms: f64,
    pub fast_release_on_ms: f64,
    pub fast_release_off_ms: f64,
    /// Release Off's slow component: weight, charge and release, ms.
    pub slow_weight: f64,
    pub slow_charge_ms: f64,
    pub slow_release_ms: f64,
    /// Attack of the level smoother, ms (`smooth_ms` is its release): the
    /// reference's detector follows peaks, so peaky material keeps charging
    /// the cap after the gain has bottomed out.
    pub smooth_attack_ms: f64,
    /// Where the detector's own gain reduction bottoms out, dB. Below the
    /// audio's floor, hot material keeps charging the cap after the audio
    /// has pinned — the longer and hotter, the longer the hold.
    pub det_floor_db: f64,
}

impl Default for DetectorParams {
    fn default() -> Self {
        Self {
            offset_db: DET_OFFSET_DB,
            theta_db: DET_THETA_DB,
            smooth_ms: DET_SMOOTH_MS,
            power: DET_POWER,
            soft: DET_SOFT,
            charge_per_s: DET_CHARGE_PER_S,
            charge_max_db_per_ms: DET_CHARGE_MAX_DB_PER_MS,
            tau_on_ms: DET_TAU_ON_MS,
            tau_off_ms: DET_TAU_OFF_MS,
            floor_db: DET_FLOOR_DB,
            floor_knee_db: DET_FLOOR_KNEE_DB,
            fast_k: DET_FAST_K,
            fast_t_db: DET_FAST_T_DB,
            fast_w_db: DET_FAST_W_DB,
            fast_attack_ms: DET_FAST_ATTACK_MS,
            fast_release_on_ms: DET_FAST_RELEASE_ON_MS,
            fast_release_off_ms: DET_FAST_RELEASE_OFF_MS,
            slow_weight: DET_SLOW_WEIGHT,
            slow_charge_ms: DET_SLOW_CHARGE_MS,
            slow_release_ms: DET_SLOW_RELEASE_MS,
            smooth_attack_ms: DET_SMOOTH_ATTACK_MS,
            det_floor_db: DET_DET_FLOOR_DB,
        }
    }
}

/// Per-sample detector coefficients derived from [`DetectorParams`] and the
/// Release switch.
#[derive(Clone, Copy, Debug, Default)]
struct DetCoefs {
    gain: f64,
    theta: f64,
    smooth: f64,
    smooth_attack: f64,
    charge: f64,
    charge_max: f64,
    rel: f64,
    fast_attack: f64,
    fast_release: f64,
    slow_weight: f64,
    slow_charge: f64,
    slow_release: f64,
    /// What the soft floors give at zero control voltage, removed so an
    /// idle limiter applies exactly no gain reduction.
    floor_idle: f64,
    det_floor_idle: f64,
}

/// The stereo Deluxe engine.
#[derive(Clone, Debug)]
pub struct Deluxe {
    sample_rate: f64,
    curve: Curve,
    ch: [Channel; 2],
    det: DetectorParams,
    c: DetCoefs,
    release_on: bool,
    crush: f64,
    crunch: f64,
    wet: f64,
    dry: f64,
    /// Darkness: fade-in (0 = open) and passband gain.
    dark_mix: f64,
    dark_gain: f64,
    hp_b: f64,
    hp_a: f64,
    /// Rectified, smoothed detector input.
    m: f64,
    /// Detector cap: gain reduction in dB.
    vc: f64,
    /// The fast peak path's gain reduction, dB.
    fast: f64,
    /// Release Off's slow component, dB.
    slow: f64,
}

impl Deluxe {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let k = libm::tan(PI * POST_HP_HZ / sample_rate);
        let mut s = Self {
            sample_rate,
            curve: Curve::new(),
            ch: [Channel::new(sample_rate), Channel::new(sample_rate)],
            det: DetectorParams::default(),
            c: DetCoefs::default(),
            release_on: false,
            crush: 1.0,
            crunch: 1.0,
            wet: 1.0,
            dry: 0.0,
            dark_mix: 0.0,
            dark_gain: 1.0,
            hp_b: 1.0 / (1.0 + k),
            hp_a: (1.0 - k) / (1.0 + k),
            m: 0.0,
            vc: 0.0,
            fast: 0.0,
            slow: 0.0,
        };
        s.set(DeluxeParams::default());
        s
    }

    #[must_use]
    pub const fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// Replace the detector's constants (the fitting tools' hook).
    pub fn set_detector(&mut self, det: DetectorParams) {
        self.det = det;
        self.refresh_detector();
    }

    fn refresh_detector(&mut self) {
        let (d, sr) = (self.det, self.sample_rate);
        let on = self.release_on;
        let leak_ratio = (1.0 - pole_ms(if on { d.tau_on_ms } else { d.tau_off_ms }, sr)) / (1.0 - pole_ms(d.tau_on_ms, sr)).max(1e-12);
        self.c = DetCoefs {
            gain: libm::pow(10.0, d.offset_db / 20.0),
            theta: libm::pow(10.0, d.theta_db / 20.0),
            smooth: 1.0 - pole_ms(d.smooth_ms, sr),
            smooth_attack: 1.0 - pole_ms(d.smooth_attack_ms, sr),
            // the Release switch changes the dynamics, not the static law: the
            // reference compresses a steady tone identically in both modes, so
            // the charge scales with the leak and the cap settles in the same
            // place either way
            charge: d.charge_per_s / sr * leak_ratio,
            charge_max: d.charge_max_db_per_ms * 1000.0 / sr,
            rel: pole_ms(if on { d.tau_on_ms } else { d.tau_off_ms }, sr),
            fast_attack: 1.0 - pole_ms(d.fast_attack_ms, sr),
            fast_release: 1.0 - pole_ms(if on { d.fast_release_on_ms } else { d.fast_release_off_ms }, sr),
            slow_weight: if on { 0.0 } else { d.slow_weight },
            slow_charge: 1.0 - pole_ms(d.slow_charge_ms, sr),
            slow_release: pole_ms(d.slow_release_ms, sr),
            floor_idle: d.floor_db - soft_over(d.floor_db, d.floor_knee_db),
            det_floor_idle: d.det_floor_db - soft_over(d.det_floor_db, d.floor_knee_db),
        };
    }

    pub fn set(&mut self, p: DeluxeParams) {
        let db = |x: f64| libm::pow(10.0, x / 20.0);
        self.crush = db(crush_db(p.crush));
        self.crunch = db(CRUNCH_DB_PER_UNIT * p.crunch.clamp(0.0, 10.0));
        let m = (p.mix / 100.0).clamp(0.0, 1.0) * FRAC_PI_2;
        self.wet = libm::sin(m);
        self.dry = libm::cos(m);
        self.release_on = p.release_on;
        self.refresh_detector();
        self.set_darkness(p.darkness_hz);
    }

    /// Darkness's law: a resonant pair at `fc = a·knob^b + f0` (≈ linear in
    /// the knob above a fixed floor — a pot in series with a resistor) with
    /// `Q = q0 + q1·ln(knob / 1 kHz)`, a real pole at `r·fc`, a small signed
    /// feedthrough around them (where the reference's response floors, near
    /// −50 dB), and a passband loss of `gA·ln(1 + (g0/knob)^gp)` dB. Above the first measured
    /// position it fades in (log-Hz) from the open knob, which is the
    /// reference the law was measured against.
    fn set_darkness(&mut self, darkness_hz: f64) {
        let hz = darkness_hz.clamp(500.0, 20_000.0);
        // designed at the 4× rate it runs at
        let sr = 4.0 * self.sample_rate;
        let fc = DARK_FC_SCALE * libm::pow(hz, DARK_FC_EXP) + DARK_FC_FLOOR_HZ;
        let q = DARK_Q0 + DARK_Q1 * libm::log(hz / 1000.0);
        let k = libm::tan(PI * fc.min(0.45 * sr) / sr);
        let n = 1.0 / (1.0 + k / q + k * k);
        let b0 = k * k * n;
        let pair = [b0, 2.0 * b0, b0, 2.0 * (k * k - 1.0) * n, (1.0 - k / q + k * k) * n];
        let k1 = libm::tan(PI * (DARK_POLE_RATIO * fc).min(0.45 * sr) / sr);
        let pole = [k1 / (1.0 + k1), k1 / (1.0 + k1), 0.0, (k1 - 1.0) / (1.0 + k1), 0.0];
        for c in &mut self.ch {
            c.set_dark([pair, pole]);
        }
        self.dark_gain = libm::pow(10.0, -DARK_LOSS_DB * libm::log1p(libm::pow(DARK_LOSS_KNEE_HZ / hz, DARK_LOSS_EXP)) / 20.0);
        self.dark_mix = (libm::log(20_000.0 / hz) / libm::log(20_000.0 / DARK_FIRST_HZ)).clamp(0.0, 1.0);
    }

    pub fn reset(&mut self) {
        for c in &mut self.ch {
            c.reset();
        }
        self.m = 0.0;
        self.vc = 0.0;
        self.fast = 0.0;
        self.slow = 0.0;
    }

    /// One stereo frame.
    pub fn process(&mut self, left: f64, right: f64) -> (f64, f64) {
        let (det, coef) = (self.det, self.c);
        let slow = coef.slow_weight * self.slow;
        let total = self.vc + self.fast + slow;
        // the gain reduction eases into its floor; zero control voltage is
        // exactly zero reduction
        let gr = det.floor_db - soft_over(det.floor_db - total, det.floor_knee_db) - coef.floor_idle;
        // 10^(−gr/20), as one exp
        let gain = libm::exp(-gr * DB_TO_LN) * self.crush * coef.gain;
        let (dm, dg) = (self.dark_mix, self.dark_gain);
        let [cl, cr] = &mut self.ch;
        let dl = gain * cl.pre(left);
        let dr = gain * cr.pre(right);
        // detector: the stereo sum, rectified and smoothed, through the
        // rectifier law against θ, integrated on a leaky cap whose voltage is
        // the gain reduction in dB (charge rate limited)
        let det_gr = det.det_floor_db - soft_over(det.det_floor_db - total, det.floor_knee_db) - coef.det_floor_idle;
        let sum = libm::fabs(0.5 * (dl + dr)) * libm::exp((gr - det_gr) * DB_TO_LN);
        self.m += if sum > self.m { coef.smooth_attack } else { coef.smooth } * (sum - self.m);
        let ln_x = libm::log(self.m / coef.theta + 1e-12);
        let q = if det.power < 1e-6 { ln_x } else { libm::expm1(det.power * ln_x) / det.power };
        let excess = det.soft * soft_over(q / det.soft, 1.0);
        self.vc = self.vc * coef.rel + (coef.charge * excess).min(coef.charge_max);
        // Release Off's slow component follows the cap up and bleeds slowly
        self.slow = if self.vc > self.slow { self.slow + coef.slow_charge * (self.vc - self.slow) } else { self.slow * coef.slow_release };
        // the fast path: a peak follower on the instantaneous level, which is
        // what makes the reference dip on every drum hit and recover between
        let level_db = libm::log(sum + 1e-9) / DB_TO_LN;
        let fast_target = det.fast_k * soft_over(level_db - det.fast_t_db, det.fast_w_db);
        let step = if fast_target > self.fast { coef.fast_attack } else { coef.fast_release };
        self.fast += step * (fast_target - self.fast);
        let (wet, dry, hb, ha, k) = (self.wet, self.dry, self.hp_b, self.hp_a, self.crunch);
        let curve = &self.curve;
        let shaped_l = cl.shape(curve, k * dl, dm, dg);
        let shaped_r = cr.shape(curve, k * dr, dm, dg);
        let yl = cl.post(shaped_l, hb, ha);
        let yr = cr.post(shaped_r, hb, ha);
        (yl * wet + left * dry, yr * wet + right * dry)
    }
}

#[cfg(test)]
mod tests {
    use super::Curve;

    #[test]
    fn curve_is_smooth_through_zero_and_flat_past_the_rails() {
        let c = Curve::new();
        let slope = |u: f64| (c.at(u + 1e-7) - c.at(u - 1e-7)) / 2e-7;
        assert!(c.at(0.0).abs() < 1e-15, "value at 0: {}", c.at(0.0));
        assert!((slope(0.0) - 1.0).abs() < 1e-6, "slope at 0: {}", slope(0.0));
        assert!(slope(2.0).abs() < 1e-3 && slope(-2.0).abs() < 1e-3);
        assert!((0.28..0.32).contains(&c.at(2.0)) && (-0.40..-0.36).contains(&c.at(-2.0)));
    }
}
