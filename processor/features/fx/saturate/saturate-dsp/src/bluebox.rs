//! Bluebox — a tube harmonics generator after a well-known boutique
//! two-tube "mix bus" processor and its plug-in.
//!
//! A voltage-domain circuit model, written from the behaviour spec
//! `saturate/spec/bluebox-circuit.md` and checked against black-box
//! measurement of the plug-in (`assets/saturator-models/tools/teamb`).
//!
//! ```text
//!  in ─┬─────────────── dry (latency-aligned) ─────────────────────────────────────────┐
//!      └► XFMR_in ► ×G_in ► ↑OS ─┬─► LP_alt ► SAT ► H_sat ► ×g_sat ─(+)─► ×(−1) ─┐       │
//!                                └──────────────────────────────────┘            │       │
//!   ┌────────────────────────────────────────────────────────────────────────────┘       │
//!   ▼                                                                                  │
//!  LP_pent ► ×g_pent ►(−)► PLATE1 ┬► HP ► LP_tri ► ×g_tri ► LIMIT ►(−)► PLATE2 ┬► HP ► ↓OS ► ×G_out ► XFMR_out
//!                      └─ Z_cal ◄─┘                                 └─ Z_air ◄─┘          out = dry + (wet−dry)·mix
//! ```
//!
//! - Everything between the interpolator and the decimator runs at the
//!   oversampled rate, in absolute volts on a 320 V supply. Every time
//!   constant is physical, so the model holds at any rate.
//! - Each tube stage is a static plate curve with cathode feedback through
//!   a first-order network (Cal on the pentode, Air on the triode), fed
//!   back from the previous oversampled sample, then a coupling capacitor.
//! - The parallel saturation tube adds to the clean signal ahead of the
//!   pentode; Sat Freq picks the band of what it makes.
//! - The engine starts at its settled operating point, so it never thumps.
//! - The two channels are independent and identical.
//!
//! The curves are evaluated in closed form per sample; nothing is tabulated.

use core::f64::consts::PI;

use crate::curve_table::{CurveTable, Left};
use crate::halfband::Oversampler;

/// Saturation Frequency.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SatFreq {
    Low,
    #[default]
    Flat,
    High,
}

/// Cal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Cal {
    Dark,
    #[default]
    Norm,
    Bright,
}

/// The reference's controls, in its own units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlueboxParams {
    /// 0..=100.
    pub pentode: f64,
    /// 0..=100.
    pub triode: f64,
    pub saturation_in: bool,
    /// 0..=100.
    pub saturation: f64,
    pub saturation_freq: SatFreq,
    pub alt_tube: bool,
    pub air_in: bool,
    /// 0..=100.
    pub air: f64,
    pub cal: Cal,
    /// −100..=100.
    pub density: f64,
    /// −12..=12 dB.
    pub input_db: f64,
    /// 0..=100 (50 is unity).
    pub output: f64,
    /// 0..=100 %.
    pub mix: f64,
}

impl Default for BlueboxParams {
    fn default() -> Self {
        Self {
            pentode: 50.0,
            triode: 50.0,
            saturation_in: false,
            saturation: 50.0,
            saturation_freq: SatFreq::Flat,
            alt_tube: false,
            air_in: false,
            air: 50.0,
            cal: Cal::Norm,
            density: 0.0,
            input_db: 0.0,
            output: 50.0,
            mix: 100.0,
        }
    }
}

// ── Knob laws ───────────────────────────────────────────────────────────

/// A knob law measured on the reference: gain linear between the nine
/// positions `0, 12.5, …, 100` (black-box: the small-signal gain against
/// each knob, `assets/saturator-models/tools/teamb/hgb_taperpl.py`, fits
/// these segments to within 0.01 dB from knob 2 up).
struct Taper([f64; 9]);

impl Taper {
    fn at(&self, u: f64) -> f64 {
        piecewise(&self.0, u)
    }
}

const TAPER_SAT: Taper = Taper([0.0, 0.007_23, 0.042_29, 0.258_7, 0.506_8, 0.712_8, 0.955_0, 0.992_0, 1.0]);
const TAPER_TRIODE: Taper = Taper([0.0, 0.007_85, 0.052_80, 0.291_0, 0.510_0, 0.738_0, 0.957_0, 0.994_0, 1.0]);
const TAPER_OUTPUT: Taper = Taper([0.0, 0.009_07, 0.060_30, 0.272_0, 0.503_0, 0.713_0, 0.946_0, 0.992_0, 1.0]);
const TAPER_PENTODE: Taper = Taper([0.0, 0.021_71, 0.071_82, 0.118_0, 0.162_0, 0.256_1, 0.583_1, 0.971_0, 1.0]);

/// Air's cathode resistance (Ω) at knob positions 0, 5, …, 100, measured on
/// the reference: its Air-In response against Air-In Off at 0.5–18 kHz,
/// fitted with this stage's network per position (`teamb/hgb_airfit.py`;
/// within 0.002 dB up to Air 75, 0.14 dB above). Linear between positions.
const AIR_RA: [f64; 21] = [10030.3, 9985.2, 9940.1, 9891.5, 9842.0, 9719.6, 9539.2, 9117.0, 8210.7, 7283.5, 6188.2, 5093.0, 4190.4, 3312.2, 2304.4, 1235.9, 555.9, 351.1, 244.0, 242.2, 240.9];

/// Air's cathode resistance at knob `u` (0..1).
fn air_ra(u: f64) -> f64 {
    piecewise(&AIR_RA, u)
}

/// Linear interpolation through `nodes` spread evenly over `u` in 0..=1.
fn piecewise(nodes: &[f64], u: f64) -> f64 {
    let last = nodes.len().saturating_sub(1);
    let pos = u.clamp(0.0, 1.0) * dsp_core::num::count_to_f64(last);
    let cell = dsp_core::num::f64_to_index(pos).min(last.saturating_sub(1));
    let frac = pos - dsp_core::num::count_to_f64(cell);
    let from = nodes.get(cell).copied().unwrap_or(0.0);
    let to = nodes.get(cell.saturating_add(1)).copied().unwrap_or(from);
    from + (to - from) * frac
}

/// Saturation's level into the main path at full knob.
const G_SAT: f64 = -1.405_5;
/// Output make-up at full knob: the spec's 0.175830 trimmed by −0.105 dB —
/// measured, the small-signal level otherwise sits 0.10 dB over the
/// reference at every frequency and every knob setting.
const G_OUT: f64 = 0.173_717;

/// Density's drive factor: linear in the knob, −100..=100.
fn density_drive(d: f64) -> f64 {
    0.251_189 + 3.729_883 * (d.clamp(-100.0, 100.0) + 100.0) / 200.0
}

// ── The curves ──────────────────────────────────────────────────────────

const SUPPLY: f64 = 320.0;

/// `ln(1 + e^(k·z)) / k`, in its overflow-safe form.
#[inline]
fn softplus(k: f64, z: f64) -> f64 {
    let a = k * z.abs();
    // past 40 the correction is below 1e-17·k⁻¹
    if a > 40.0 { z.max(0.0) } else { z.max(0.0) + libm::log1p(libm::exp(-a)) / k }
}

/// `(1 + x)^(−q)` for `x ≥ 0`.
#[inline]
fn inv_pow1p(x: f64, q: f64) -> f64 {
    libm::exp(-q * libm::log1p(x))
}

/// The saturation tube's transfer, volts in, volts out (a starved 12 V stage).
#[derive(Clone, Copy, Debug)]
struct SatCurve {
    lambda: f64,
    c: f64,
    k: f64,
    v1: f64,
    q1: f64,
    w: f64,
}

impl SatCurve {
    #[inline]
    fn at(&self, v: f64) -> f64 {
        let s = softplus(self.k, v + self.c);
        12.0 * (self.w * libm::exp(-s / self.lambda) + (1.0 - self.w) * inv_pow1p(s / self.v1, self.q1))
    }
}

const SAT_NORMAL: SatCurve = SatCurve { lambda: 11.7561, c: 1.486_46, k: 3.306_65, v1: 11.8707, q1: 0.951_370, w: 0.216_030 };
const SAT_ALT: SatCurve = SatCurve { lambda: 7.778_59, c: 1.496_82, k: 4.678_41, v1: 1.956_71, q1: 0.381_980, w: 0.596_760 };

/// The pentode's plate volts against grid–cathode volts on its load line.
#[inline]
fn plate1(v: f64) -> f64 {
    const VF: f64 = 9.142_07;
    let s = softplus(1.613_62, v + 0.726_697);
    VF + (SUPPLY - VF) * inv_pow1p(s / 3.762_15, 2.372_80)
}

/// Grid conduction: identity below ~5 V, a soft limit towards ~12.9 V.
#[inline]
fn limit(v: f64) -> f64 {
    const B: f64 = 4.680_81;
    const N: f64 = 0.834_898;
    const S: f64 = 0.008_184_34;
    let d = softplus(0.851_870, v - 9.240_66);
    if d <= 0.0 {
        return v;
    }
    let r = libm::exp(N * libm::log(d / B));
    v - d + d * libm::exp(-libm::log1p(r) / N) + S * d
}

/// The triode's plate volts against grid–cathode volts.
#[inline]
fn plate2(v: f64) -> f64 {
    const VF: f64 = 6.022_44;
    const KF: f64 = 0.096_555_5;
    const K: f64 = 0.304_75;
    let s1 = softplus(K, v + 20.5530);
    let s2 = softplus(K, v + 74.9767);
    let i1 = if s1 > 0.0 { 1.980_30 * libm::exp(1.528_16 * libm::log(s1)) } else { 0.0 };
    let i2 = if s2 > 0.0 { libm::exp(22.4634 * libm::log(s2 / 70.2491)) } else { 0.0 };
    VF + softplus(KF, (SUPPLY - VF) - i1 - i2)
}

/// Cathode feedback of the pentode at DC (the plate load's share).
const CAL_K: f64 = 0.011;
/// …and of the triode.
const AIR_K: f64 = 0.0603;

/// The plate voltage `vp` that holds `vp = plate(grid − k·(320 − vp))`.
fn operating_point(plate: impl Fn(f64) -> f64, grid: f64, k: f64) -> f64 {
    let (mut lo, mut hi) = (0.0, SUPPLY);
    // 64 halvings of 320 V reach the last bit
    for _ in 0..64 {
        let mid = 0.5 * (lo + hi);
        if plate(grid - k * (SUPPLY - mid)) > mid {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Idle plate volts of the two stages (they do not depend on any control).
fn idle() -> (f64, f64) {
    (operating_point(plate1, 0.0, CAL_K), operating_point(plate2, limit(0.0), AIR_K))
}

/// As [`idle`], on the curves as the engine evaluates them, so it settles
/// to the last bit.
fn idle_on(k: &Curves) -> (f64, f64) {
    (operating_point(|v| k.plate1(v), 0.0, CAL_K), operating_point(|v| k.plate2(v), k.limit(0.0), AIR_K))
}

fn sat_normal(v: f64) -> f64 {
    SAT_NORMAL.at(v)
}

fn sat_alt(v: f64) -> f64 {
    SAT_ALT.at(v)
}

/// The five curves, tabulated (or, for checking, in closed form).
#[derive(Clone, Debug)]
struct Curves {
    exact: bool,
    sat0: CurveTable,
    sat1: CurveTable,
    plate1: CurveTable,
    limit: CurveTable,
    plate2: CurveTable,
}

impl Curves {
    /// Fine steps over each knee, 1/4 V either side: within 3e-7 V of the
    /// closed forms everywhere (`curves_match_their_equations`).
    fn new(exact: bool) -> Self {
        let (f, c) = (1.0 / 32.0, 0.25);
        Self {
            exact,
            sat0: CurveTable::new(sat_normal, Left::Const(12.0), [-12.0, -6.0, 6.0, 300.0], [c, f, c]),
            sat1: CurveTable::new(sat_alt, Left::Const(12.0), [-10.0, -5.0, 6.0, 300.0], [c, f, c]),
            plate1: CurveTable::new(plate1, Left::Const(SUPPLY), [-25.0, -10.0, 15.0, 400.0], [c, f, c]),
            limit: CurveTable::new(limit, Left::Identity, [-45.0, -5.0, 35.0, 400.0], [c, f, c]),
            plate2: CurveTable::new(plate2, Left::Const(plate2(-150.0)), [-150.0, -45.0, 30.0, 100.0], [c, 2.0 * f, c]),
        }
    }

    #[inline]
    fn sat0(&self, v: f64) -> f64 {
        if self.exact { sat_normal(v) } else { self.sat0.eval(v) }
    }

    #[inline]
    fn sat1(&self, v: f64) -> f64 {
        if self.exact { sat_alt(v) } else { self.sat1.eval(v) }
    }

    #[inline]
    fn plate1(&self, v: f64) -> f64 {
        if self.exact { plate1(v) } else { self.plate1.eval(v) }
    }

    #[inline]
    fn limit(&self, v: f64) -> f64 {
        if self.exact { limit(v) } else { self.limit.eval(v) }
    }

    #[inline]
    fn plate2(&self, v: f64) -> f64 {
        if self.exact { plate2(v) } else { self.plate2.eval(v) }
    }
}

// ── Filters ─────────────────────────────────────────────────────────────

/// A one-pole by the trapezoidal rule: `G = g/(1+g)`, `g = tan(π·f/rate)`.
/// Returns the low-pass; the high-pass is the input minus it.
#[inline]
fn tpt(state: &mut f64, g: f64, x: f64) -> f64 {
    let v = g * (x - *state);
    let lp = v + *state;
    *state = lp + v;
    lp
}

fn tpt_gain(hz: f64, rate: f64) -> f64 {
    let g = libm::tan(PI * (hz / rate).min(0.49));
    g / (1.0 + g)
}

/// `1 − exp(−1/(τ·rate))`: an RC low-pass by impulse invariance.
fn rc_alpha(tau: f64, rate: f64) -> f64 {
    1.0 - libm::exp(-1.0 / (tau * rate))
}

/// A cathode network `k·(1 + s·R·C)/(1 + s·(R + Rk)·C)` written as
/// `k·(1 − m·HP)`, `m = Rk/(R + Rk)` and `HP` a one-pole high-pass at
/// `1/(2π(R + Rk)C)`: the DC gain is `k` whatever `m` and the corner do, so
/// moving them never steps the operating point.
fn cathode(r: f64, rk: f64, c: f64, rate: f64) -> (f64, f64) {
    let hz = 1.0 / (2.0 * PI * (r + rk) * c);
    (tpt_gain(hz, rate), rk / (r + rk))
}

/// A first-order digital section `g·(1 + b·z⁻¹)/(1 + a·z⁻¹)` whose magnitude
/// follows an analog real pole at `hz` — even one above Nyquist — across
/// the audio band (least squares on the squared magnitude, linear in the
/// section's parameters; within 0.015 dB to 20 kHz at 44.1 kHz and up).
fn pole_section(hz: f64, fs: f64) -> [f64; 3] {
    // |H|²/|H(0)|² = (1 + s·B²)/(1 + s·A²), s = tan²(πf/fs),
    // A = (1−a)/(1+a), B = (1−b)/(1+b): fit α = A², β = B² to 1/(1+(f/hz)²)
    let top = 20_000.0_f64.min(0.45 * fs);
    let (mut saa, mut sab, mut sbb, mut sya, mut syb) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for step in 1..=64_u32 {
        let freq = top * f64::from(step) / 64.0;
        let target = 1.0 / (1.0 + (freq / hz) * (freq / hz));
        let warp = libm::tan(PI * freq / fs);
        let warp2 = warp * warp;
        let (xa, xb, rhs) = (-target * warp2, warp2, target - 1.0);
        saa += xa * xa;
        sab += xa * xb;
        sbb += xb * xb;
        sya += rhs * xa;
        syb += rhs * xb;
    }
    let cross = sab * sab;
    let det = saa * sbb - cross;
    let alpha = ((sya * sbb - syb * sab) / det).max(0.0);
    let beta = ((syb * saa - sya * sab) / det).max(0.0);
    let (root_a, root_b) = (libm::sqrt(alpha), libm::sqrt(beta));
    let pole = (1.0 - root_a) / (1.0 + root_a);
    let zero = (1.0 - root_b) / (1.0 + root_b);
    [(1.0 + pole) / (1.0 + zero), zero, pole]
}

/// Core saturation, a DC-blocking high-pass, an HF roll-off; a leaky flux
/// integrator closes the loop.
#[derive(Clone, Copy, Debug, Default)]
struct Transformer {
    hp_b: f64,
    hp_a: f64,
    /// The roll-off (Q 0.5: two coincident real poles) as two identical
    /// first-order sections `[g, b, a]`.
    hf: [f64; 3],
    leak: f64,
    flux_k: f64,
    gain: f64,
    hp_x: f64,
    hp_y: f64,
    /// Section states (transposed direct form II).
    s1: f64,
    s2: f64,
    phi: f64,
}

impl Transformer {
    fn new(hp_hz: f64, rolloff_hz: f64, k_flux: f64, gain: f64, fs: f64) -> Self {
        let warp = libm::tan(PI * hp_hz / fs);
        Self {
            hp_b: 1.0 / (1.0 + warp),
            hp_a: (1.0 - warp) / (1.0 + warp),
            hf: pole_section(rolloff_hz, fs),
            leak: 1.0 - 1.0 / fs,
            flux_k: k_flux / fs,
            gain,
            ..Self::default()
        }
    }

    #[inline]
    fn run(&mut self, input: f64) -> f64 {
        let phi2 = self.phi * self.phi;
        let phi4 = phi2 * phi2;
        let core = input * (1.0 - phi4 * phi4);
        let blocked = self.hp_b * (core - self.hp_x) + self.hp_a * self.hp_y;
        self.hp_x = core;
        self.hp_y = blocked;
        let [scale, zero, pole] = self.hf;
        let mid = scale * blocked + self.s1;
        self.s1 = scale * zero * blocked - pole * mid;
        let out = scale * mid + self.s2;
        self.s2 = scale * zero * mid - pole * out;
        self.phi = (self.phi * self.leak + out * self.flux_k).clamp(-1.0, 1.0);
        out * self.gain
    }

    const fn reset(&mut self) {
        self.hp_x = 0.0;
        self.hp_y = 0.0;
        self.s1 = 0.0;
        self.s2 = 0.0;
        self.phi = 0.0;
    }
}

// ── Smoothing ───────────────────────────────────────────────────────────

/// A value gliding toward its target with a one-pole; lands exactly.
#[derive(Clone, Copy, Debug, Default)]
struct Glide {
    now: f64,
    target: f64,
}

impl Glide {
    #[inline]
    fn step(&mut self, a: f64) -> f64 {
        let d = self.target - self.now;
        self.now = if d.abs() <= 1e-12 * self.target.abs().max(1e-6) { self.target } else { self.now + a * d };
        self.now
    }

    const fn snap(&mut self) {
        self.now = self.target;
    }
}

/// Sat Freq's crossfade: Low / Flat / High weights, moving linearly from
/// wherever they were to the chosen band.
#[derive(Clone, Copy, Debug)]
struct BandFade {
    from: [f64; 3],
    to: SatFreq,
    t: f64,
}

impl BandFade {
    const fn weights(band: SatFreq) -> [f64; 3] {
        match band {
            SatFreq::Low => [1.0, 0.0, 0.0],
            SatFreq::Flat => [0.0, 1.0, 0.0],
            SatFreq::High => [0.0, 0.0, 1.0],
        }
    }

    fn retarget(&mut self, to: SatFreq) {
        if to != self.to {
            self.from = self.now();
            self.to = to;
            self.t = 0.0;
        }
    }

    fn now(&self) -> [f64; 3] {
        let mut weights = Self::weights(self.to);
        for (w, from) in weights.iter_mut().zip(self.from) {
            *w = from + (*w - from) * self.t;
        }
        weights
    }

    #[inline]
    fn step(&mut self, rate: f64) -> [f64; 3] {
        self.t = (self.t + rate).min(1.0);
        self.now()
    }

    const fn snap(&mut self) {
        self.t = 1.0;
    }
}

// ── The circuit ─────────────────────────────────────────────────────────

/// What the oversampled section needs this sample, shared by both channels.
#[derive(Clone, Copy, Debug, Default)]
struct Live {
    g_sat: f64,
    g_pent: f64,
    g_tri: f64,
    cal_g: f64,
    cal_m: f64,
    air_g: f64,
    air_m: f64,
    /// Alt Tube crossfade.
    alt: f64,
    /// Low / Flat / High weights.
    band: [f64; 3],
    /// Fixed corners.
    a_alt: f64,
    a_pent: f64,
    a_tri: f64,
    c1: f64,
    c2: f64,
    sat_hp: f64,
    sat_lp: f64,
    sat_high: f64,
    sat0_rest: f64,
    sat1_rest: f64,
}

/// One channel's tube circuit, run at the oversampled rate.
#[derive(Clone, Copy, Debug, Default)]
struct Circuit {
    s_alt: f64,
    sat_hp: f64,
    sat_lp: f64,
    sat_high: f64,
    p: f64,
    cal: f64,
    vp1: f64,
    h1: f64,
    t2: f64,
    air: f64,
    vp2: f64,
    h2: f64,
}

impl Circuit {
    /// At rest on the operating point.
    fn settled(vp1: f64, vp2: f64) -> Self {
        Self { cal: SUPPLY - vp1, vp1, air: SUPPLY - vp2, vp2, ..Self::default() }
    }

    #[inline]
    fn run(&mut self, live: &Live, curves: &Curves, input: f64) -> f64 {
        // parallel saturation tube, summed with the clean signal, inverted
        let grid = if live.g_sat == 0.0 {
            self.s_alt = 0.0;
            self.sat_hp = 0.0;
            self.sat_lp = 0.0;
            self.sat_high = 0.0;
            -input
        } else {
            self.s_alt += live.a_alt * (input - self.s_alt);
            let rc = self.s_alt;
            let made = if live.alt <= 0.0 {
                curves.sat0(rc) - live.sat0_rest
            } else if live.alt >= 1.0 {
                curves.sat1(rc) - live.sat1_rest
            } else {
                (1.0 - live.alt) * (curves.sat0(rc) - live.sat0_rest) + live.alt * (curves.sat1(rc) - live.sat1_rest)
            };
            let flat = made - tpt(&mut self.sat_hp, live.sat_hp, made);
            let low = tpt(&mut self.sat_lp, live.sat_lp, flat);
            let high = made - tpt(&mut self.sat_high, live.sat_high, made);
            let [wl, wf, wh] = live.band;
            -(input + live.g_sat * (wl * low + wf * flat + wh * high))
        };
        // stage 1: pentode, Cal network in its cathode
        self.p += live.a_pent * (grid - self.p);
        let e1 = SUPPLY - self.vp1;
        let vk1 = CAL_K * (e1 - live.cal_m * (e1 - tpt(&mut self.cal, live.cal_g, e1)));
        let vp1 = curves.plate1(live.g_pent * self.p - vk1);
        self.h1 = live.c1 * (self.h1 + vp1 - self.vp1);
        self.vp1 = vp1;
        // stage 2: triode, Air network in its cathode; the grid limits first
        self.t2 += live.a_tri * (self.h1 - self.t2);
        let e2 = SUPPLY - self.vp2;
        let vk2 = AIR_K * (e2 - live.air_m * (e2 - tpt(&mut self.air, live.air_g, e2)));
        let vp2 = curves.plate2(curves.limit(live.g_tri * self.t2) - vk2);
        self.h2 = live.c2 * (self.h2 + vp2 - self.vp2);
        self.vp2 = vp2;
        self.h2
    }
}

/// The base-rate gains for one frame, shared by both channels.
#[derive(Clone, Copy, Debug)]
struct Frame {
    g_in: f64,
    g_out: f64,
    wet: f64,
    latency: usize,
}

/// Longest dry delay (base-rate samples) the oversampler can need.
const DRY_LEN: usize = 64;

#[derive(Clone, Copy, Debug)]
struct Channel {
    input: Transformer,
    output: Transformer,
    os: Oversampler,
    circuit: Circuit,
    dry: [f64; DRY_LEN],
    dry_pos: usize,
}

/// The stereo Bluebox engine.
#[derive(Clone, Debug)]
pub struct Bluebox {
    sample_rate: f64,
    os_rate: f64,
    latency: usize,
    idle: (f64, f64),
    ch: [Channel; 2],
    curves: Curves,
    live: Live,
    /// Per-sample one-pole for the ~50 ms glides, and the crossfade steps.
    glide_a: f64,
    alt_step: f64,
    band_step: f64,
    g_in: Glide,
    g_sat: Glide,
    g_pent: Glide,
    g_tri: Glide,
    g_out: Glide,
    wet: Glide,
    cal_g: Glide,
    cal_m: Glide,
    air_g: Glide,
    air_m: Glide,
    alt: Glide,
    band: BandFade,
    primed: bool,
}

impl Bluebox {
    /// The engine at `sample_rate`, its curves tabulated (≈160 KB, built
    /// here — call off the audio thread).
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        Self::build(sample_rate, false)
    }

    /// As [`Self::new`], evaluating every curve in closed form per sample
    /// (slow; the reference the tables are checked against).
    #[must_use]
    pub fn new_exact(sample_rate: f64) -> Self {
        Self::build(sample_rate, true)
    }

    fn build(sample_rate: f64, exact: bool) -> Self {
        let fs = sample_rate.max(1.0);
        let factor = if fs < 50_000.0 {
            4
        } else if fs < 100_000.0 {
            2
        } else {
            1
        };
        let os = Oversampler::new(factor);
        let fo = fs * dsp_core::num::count_to_f64(factor);
        let curves = Curves::new(exact);
        let idle = idle_on(&curves);
        let channel = Channel {
            input: Transformer::new(0.37, 97_000.0, 83.20, -3.0876, fs),
            output: Transformer::new(0.65, 75_000.0, 5.4785, 0.25, fs),
            os,
            circuit: Circuit::settled(idle.0, idle.1),
            dry: [0.0; DRY_LEN],
            dry_pos: 0,
        };
        let coupling = |rc: f64| rc / (rc + 1.0 / fo);
        let live = Live {
            a_tri: rc_alpha(7.6305e-6, fo),
            c1: coupling(0.09),
            c2: coupling(0.18),
            sat_hp: tpt_gain(3.379, fo),
            sat_lp: tpt_gain(1711.3, fo),
            sat_high: tpt_gain(5198.6, fo),
            band: [0.0, 1.0, 0.0],
            ..Live::default()
        };
        let mut s = Self {
            sample_rate: fs,
            os_rate: fo,
            latency: os.latency(),
            idle,
            ch: [channel, channel],
            live: Live { sat0_rest: curves.sat0(0.0), sat1_rest: curves.sat1(0.0), ..live },
            curves,
            glide_a: 1.0 - libm::exp(-1.0 / (0.05 * fs)),
            alt_step: 1.0 / (0.05 * fs),
            band_step: 1.0 / (0.15 * fs),
            g_in: Glide::default(),
            g_sat: Glide::default(),
            g_pent: Glide::default(),
            g_tri: Glide::default(),
            g_out: Glide::default(),
            wet: Glide::default(),
            cal_g: Glide::default(),
            cal_m: Glide::default(),
            air_g: Glide::default(),
            air_m: Glide::default(),
            alt: Glide::default(),
            band: BandFade { from: [0.0, 1.0, 0.0], to: SatFreq::Flat, t: 1.0 },
            primed: false,
        };
        s.set(BlueboxParams::default());
        s.primed = false;
        s
    }

    #[must_use]
    pub const fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// The wet path's delay in samples; the dry path is delayed to match.
    /// Report it to the host.
    #[must_use]
    pub const fn latency(&self) -> usize {
        self.latency
    }

    /// Take new settings. Gains and the cathode networks glide (~50 ms),
    /// Alt Tube crossfades, Sat Freq crossfades over 0.15 s; the very first
    /// call after construction lands on them at once.
    pub fn set(&mut self, p: BlueboxParams) {
        let fo = self.os_rate;
        let unit = |v: f64| (v / 100.0).clamp(0.0, 1.0);
        let d = density_drive(p.density);
        self.g_in.target = libm::pow(10.0, p.input_db.clamp(-12.0, 12.0) / 20.0) * d;
        self.g_sat.target = if p.saturation_in { G_SAT * TAPER_SAT.at(unit(p.saturation)) } else { 0.0 };
        let pent = TAPER_PENTODE.at(unit(p.pentode));
        self.g_pent.target = 0.72 * pent;
        self.g_tri.target = 0.90 * TAPER_TRIODE.at(unit(p.triode));
        self.g_out.target = G_OUT * TAPER_OUTPUT.at(unit(p.output)) / d.min(3.162_28);
        self.wet.target = unit(p.mix);
        let r_cal = match p.cal {
            Cal::Dark => 5_000.0,
            Cal::Norm => 1_000.0,
            Cal::Bright => 200.0,
        };
        (self.cal_g.target, self.cal_m.target) = cathode(r_cal, 600.0, 33e-9, fo);
        let ra = air_ra(unit(p.air));
        let (air_g, air_m) = cathode(ra, 2_200.0, 10e-9, fo);
        self.air_g.target = air_g;
        self.air_m.target = if p.air_in { air_m } else { 0.0 };
        self.alt.target = if p.alt_tube { 1.0 } else { 0.0 };
        self.band.retarget(p.saturation_freq);
        // the corners switch without gliding
        self.live.a_alt = rc_alpha(if p.alt_tube { 8.9817e-6 } else { 15.288e-6 }, fo);
        self.live.a_pent = rc_alpha(1.410_17e-6 - 1.015_32e-6 * pent, fo);
        if !self.primed {
            self.primed = true;
            self.snap();
        }
    }

    fn snap(&mut self) {
        for g in [
            &mut self.g_in,
            &mut self.g_sat,
            &mut self.g_pent,
            &mut self.g_tri,
            &mut self.g_out,
            &mut self.wet,
            &mut self.cal_g,
            &mut self.cal_m,
            &mut self.air_g,
            &mut self.air_m,
            &mut self.alt,
        ] {
            g.snap();
        }
        self.band.snap();
    }

    /// Back to the settled operating point, settings landed.
    pub fn reset(&mut self) {
        self.snap();
        let (vp1, vp2) = self.idle;
        for c in &mut self.ch {
            c.input.reset();
            c.output.reset();
            c.os.reset();
            c.circuit = Circuit::settled(vp1, vp2);
            c.dry = [0.0; DRY_LEN];
            c.dry_pos = 0;
        }
    }

    /// One stereo frame.
    pub fn process(&mut self, left: f64, right: f64) -> (f64, f64) {
        let a = self.glide_a;
        let g_in = self.g_in.step(a);
        let g_out = self.g_out.step(a);
        let wet = self.wet.step(a);
        self.live.g_sat = self.g_sat.step(a);
        self.live.g_pent = self.g_pent.step(a);
        self.live.g_tri = self.g_tri.step(a);
        self.live.cal_g = self.cal_g.step(a);
        self.live.cal_m = self.cal_m.step(a);
        self.live.air_g = self.air_g.step(a);
        self.live.air_m = self.air_m.step(a);
        let alt = &mut self.alt;
        alt.now = if alt.target > alt.now { (alt.now + self.alt_step).min(alt.target) } else { (alt.now - self.alt_step).max(alt.target) };
        self.live.alt = alt.now;
        self.live.band = self.band.step(self.band_step);
        let live = self.live;
        let latency = self.latency;
        let curves = &self.curves;
        let [l, r] = &mut self.ch;
        let frame = Frame { g_in, g_out, wet, latency };
        (l.run(&live, curves, frame, left), r.run(&live, curves, frame, right))
    }
}

impl Channel {
    #[inline]
    fn run(&mut self, live: &Live, curves: &Curves, frame: Frame, input: f64) -> f64 {
        // the dry path, delayed by the wet path's latency
        let next = self.dry_pos.saturating_add(1);
        self.dry_pos = if next >= DRY_LEN { 0 } else { next };
        if let Some(slot) = self.dry.get_mut(self.dry_pos) {
            *slot = input;
        }
        let back = frame.latency.min(DRY_LEN.saturating_sub(1));
        let read = self.dry_pos.checked_sub(back).unwrap_or_else(|| self.dry_pos.saturating_add(DRY_LEN).saturating_sub(back));
        let dry = self.dry.get(read).copied().unwrap_or(0.0);
        let volts = self.input.run(input) * frame.g_in;
        let circuit = &mut self.circuit;
        let out = self.os.process(volts, |v| circuit.run(live, curves, v));
        let wet = self.output.run(out * frame.g_out);
        dry + (wet - dry) * frame.wet
    }
}

/// The static picture of the two tube stages, for a display.
///
/// For an input `x` in −1..1 (full scale): each stage's plate swing about
/// its operating point with its cathode network at DC, normalised to the
/// swing at full scale. Not real-time; it solves each stage per point.
#[must_use]
pub fn static_curve(params: &BlueboxParams, input: f64) -> f64 {
    let unit = |v: f64| (v / 100.0).clamp(0.0, 1.0);
    let g_in = libm::pow(10.0, params.input_db.clamp(-12.0, 12.0) / 20.0) * density_drive(params.density);
    let g_pent = 0.72 * TAPER_PENTODE.at(unit(params.pentode));
    let g_tri = 0.90 * TAPER_TRIODE.at(unit(params.triode));
    let (vp1, vp2) = idle();
    let stages = |level: f64| {
        let grid = 3.0876 * g_in * level;
        let swing1 = operating_point(plate1, g_pent * grid, CAL_K) - vp1;
        operating_point(plate2, limit(g_tri * swing1), AIR_K) - vp2
    };
    let swing = stages(1.0).abs().max(stages(-1.0).abs()).max(1e-9);
    stages(input) / swing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operating_points_are_the_specified_ones() {
        let (vp1, vp2) = idle();
        assert!((vp1 - 251.6).abs() < 0.2, "Vp1 {vp1}");
        assert!((CAL_K * (SUPPLY - vp1) - 0.75).abs() < 0.01);
        assert!((vp2 - 208.9).abs() < 0.2, "Vp2 {vp2}");
        assert!((AIR_K * (SUPPLY - vp2) - 6.70).abs() < 0.02);
    }

    #[test]
    fn tapers_run_zero_to_one() {
        let laws: [&dyn Fn(f64) -> f64; 5] = [&|u| TAPER_SAT.at(u), &|u| TAPER_TRIODE.at(u), &|u| TAPER_OUTPUT.at(u), &|u| TAPER_PENTODE.at(u), &|u| (10_030.3 - air_ra(u)) / (10_030.3 - 240.9)];
        for t in laws {
            assert!(t(0.0).abs() < 1e-12);
            assert!((t(1.0) - 1.0).abs() < 1e-12);
            assert!((TAPER_TRIODE.at(0.25) - 0.0528).abs() < 1e-12);
            let mut last = 0.0;
            for i in 1..=100 {
                let v = t(f64::from(i) / 100.0);
                assert!(v >= last);
                last = v;
            }
        }
    }

    #[test]
    fn limit_is_identity_for_small_drive() {
        for v in [-20.0, -5.0, 0.0, 2.0] {
            assert!((limit(v) - v).abs() < 1e-3, "{v} -> {}", limit(v));
        }
        assert!(limit(30.0) < 14.0);
    }
}
