//! Cloud's tank: allpass stages ahead of a figure-8 of two allpass loops.
//!
//! Built to the structure measured from Strymon BigSky's Cloud (the plug-in,
//! with signal-analyzer's `bigsky_match`), not to `CloudSeed`'s parallel
//! combs. Every number here was read off the plug-in's impulse response at
//! Diffusion −10 (every Diffusion allpass a pure delay) and −8/−6, and the
//! passes were pulled apart by rendering it at eight Decay settings: the
//! response is exactly `P1 + g·P2 + g²·P3 + …` in one per-Decay gain `g`
//! (residual 2e-7), so each pass is known on its own.
//!
//! - **Stages.** Five four-allpass stages in series (A, B, C, B, B); the
//!   input and each stage's output are mixed 1, 1, 1, 1, 0.8, 0.7 into the
//!   figure-8's input, and each stage's output has a stereo tap of its own.
//!   (They commute with everything after them: BigSky's every pass shows
//!   the same five copies.)
//! - **A Dattorro figure-8.** Two halves; each is a fixed allpass (g 0.75,
//!   217.6 ms in A, 165.3 ms in B), a tapped line, a decay gain, a second
//!   allpass (g −0.5, 535.5 / 362.8 ms, tapped inside as Dattorro's are), a
//!   second tapped line, a decay gain — into the OTHER half. The input
//!   enters ahead of each half's first allpass and at the start of its
//!   second line; that second line's taps are the plain pulses of the first
//!   pass. Each decay edge carries a gentle one-pole low-pass (pole 0.13 at
//!   48 kHz) — what makes every later pass a touch darker.
//! - **One pass is the floor.** At Decay 1000 `g ≈ 0`: what is left is one
//!   pass through both halves, ~5 s of sparse pulses falling ~13 dB/s —
//!   BigSky's 4.5 s floor, by construction.
//!
//! A model of exactly this fits the plug-in's first four passes to 0.8–2.4 %
//! of their energy; figure-8 against each half looping on itself was the
//! deciding test (pass 3: 29 % against 78 % before the internal taps and the
//! loop filter were in).
//!
//! Allocates in [`CloudRing::new`] only; `tick` is allocation-free.

use dsp_core::num;

use super::one_pole::Lp1;
use audiocore_dsp::delay_line::DelayLine;
use audiocore_dsp::denormal::flush;

/// The tank's allpass stages in series, `(delay ms, gain re Diffusion's)`
/// each: A, B, C, B, B.
const STAGES: [[(f64, f64); 4]; 5] = [
    [(26.04, 0.85), (13.34, 1.0), (10.42, 1.0), (35.60, 0.85)],
    [(19.06, 1.0), (14.90, 1.0), (37.23, 1.0), (50.90, 1.0)],
    [(25.44, 1.0), (18.60, 1.0), (9.52, 1.0), (7.44, 1.0)],
    [(19.06, 1.0), (14.90, 1.0), (37.23, 1.0), (50.90, 1.0)],
    [(19.06, 1.0), (14.90, 1.0), (37.23, 1.0), (50.90, 1.0)],
];
/// How much of the input and of each stage's output the pattern line gets.
const JUNCTION_MIX: [f64; 6] = [1.0, 1.0, 1.0, 1.0, 0.8, 0.7];
/// Each stage's own tap, `(left, right)` re the first pattern tap.
const JUNCTION_OUT: [(f64, f64); 5] = [(-1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-0.801, 0.801), (0.705, 0.701)];
/// Overall tap level (the chain's wet calibration sets the final level).
const TAP_GAIN: f64 = 0.25;
/// BigSky's mono early pulse against its first ring tap: 0.1856 / 0.1328.
pub const EARLY_TO_TAP: f64 = 1.4;
/// The figure-8's decay gain `g` for each decay time (T20 of the whole
/// response): BigSky's own, measured — its Decay knob's T20s (bigsky_match's
/// sweep) against the `g` its second pass scales by at the same knob (Decay
/// 1000 … 40000; 50000 is 1). `(T20 s, g)`, interpolated in log time.
const DECAY_G: [(f64, f64); 12] = [
    (4.50, 0.000_13),
    (4.53, 0.0156),
    (4.77, 0.0783),
    (5.60, 0.1714),
    (6.78, 0.2702),
    (8.15, 0.3624),
    (11.39, 0.5140),
    (19.83, 0.7063),
    (30.60, 0.8119),
    (57.16, 0.9141),
    (75.19, 0.9595),
    (93.12, 0.9834),
];
/// The two decay edges' gains at `g` = 1: back into the other half's first
/// allpass, and on into a half's second allpass (fitted, with the taps,
/// against BigSky's first four passes).
const EDGE_GAIN: f64 = 1.0396;
const EDGE_GAIN_INNER: f64 = 0.8916;
/// The decay edges' one-pole low-pass, Hz (its pole is 0.183 at 48 kHz).
const EDGE_LP_HZ: f64 = 12_970.0;
/// BigSky's floor: the decay of its first pass alone.
const FLOOR_T60: f64 = 4.5;
/// The largest size scale the buffers are sized for.
const MAX_SCALE: f64 = 1.6;
/// Headroom for modulation, samples.
const MOD_HEADROOM: f64 = 64.0;

/// A Schroeder allpass with a slowly modulated, linearly interpolated
/// delay.
struct RingAp {
    line: DelayLine,
    base_ms: f64,
    len: f64,
    g: f64,
    depth: f64,
    phase: f64,
    inc: f64,
}

impl RingAp {
    fn new(base_ms: f64, sample_rate: f64, phase: f64) -> Self {
        let cap = (base_ms * 1e-3 * sample_rate).mul_add(MAX_SCALE, MOD_HEADROOM + 8.0);
        Self {
            line: DelayLine::new(num::f64_to_index(cap)),
            base_ms,
            len: base_ms * 1e-3 * sample_rate,
            g: 0.0,
            depth: 0.0,
            phase,
            inc: 0.0,
        }
    }

    #[inline]
    fn tick(&mut self, x: f64) -> f64 {
        let mut d = self.len;
        if self.depth > 0.0 {
            self.phase += self.inc;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
            }
            // One-sided, as BigSky's: the delay only ever shortens, by up
            // to `depth` (its pulses all arrive early, never late).
            d -= 0.5 * self.depth * (1.0 - (self.phase * core::f64::consts::TAU).cos());
        }
        let cap = num::count_to_f64(self.line.len()) - 4.0;
        // Cubic: a linear read is a gentle low-pass, and the ring applies
        // it on every pass — the top octave died early.
        let delayed = self.line.read_cubic(d.clamp(1.0, cap));
        // Schroeder's sign, as BigSky's: −g on the instant path (its
        // first-order pulses come out negative against the main one).
        let v = self.g.mul_add(delayed, x);
        self.line.write(flush(v));
        (-self.g).mul_add(v, delayed)
    }
}

/// Cloud's input chain, the same on both sides (the plug-in's early field
/// is mono): four allpasses in series, 22.35 / 28.60 / 55.85 / 76.35 ms —
/// each leaves a faint pulse either side of the chain's 183 ms main pulse,
/// and the series alone accounts for the second-order pulses (the ±6.25 ms
/// pair is 28.60 − 22.35: one stage passing twice, another instantly). The
/// first two run at the Diffusion gain, the long two at 0.85 × it.
///
/// Five shorter delays (14.90, 13.34, 10.42, 9.52, 7.44 ms) once looked
/// nested in here; they are the tank's stages, seen through the chain.
const INPUT_STAGES: [(f64, f64); 4] = [(22.35, 1.0), (28.60, 1.0), (55.85, 0.85), (76.35, 0.85)];

/// The input diffusion: four allpasses, quadrature-modulated.
pub struct InputChain {
    stages: [RingAp; 4],
    sample_rate: f64,
}

impl InputChain {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        // In phase at the start, a touch apart in rate: BigSky's chain delay
        // swings as one LFO slowly beating against itself.
        Self {
            stages: core::array::from_fn(|i| RingAp::new(INPUT_STAGES.get(i).map_or(20.0, |s| s.0), sample_rate, 0.0)),
            sample_rate,
        }
    }

    /// Size 0.5 is BigSky's chain; it scales with the ring. Whole samples:
    /// a cubic read at a fractional delay is a gentle low-pass.
    pub fn set_size(&mut self, size: f64) {
        let scale = (size.clamp(0.0, 1.0) + 0.5).min(MAX_SCALE);
        let k = 1e-3 * self.sample_rate * scale;
        for ap in &mut self.stages {
            ap.len = (ap.base_ms * k).round();
        }
    }

    /// The allpasses' gain: 0.85 × Diffusion. At zero each is a pure delay.
    pub fn set_gain(&mut self, g: f64) {
        let g = g.clamp(0.0, 0.9);
        for (ap, s) in self.stages.iter_mut().zip(INPUT_STAGES.iter()) {
            ap.g = g * s.1;
        }
    }

    /// LFO depth (samples) and rate (Hz); each stage a touch off the rate.
    pub fn set_modulation(&mut self, depth: f64, rate_hz: f64) {
        let sr = self.sample_rate;
        for (i, ap) in self.stages.iter_mut().enumerate() {
            ap.depth = depth;
            ap.inc = rate_hz * 0.04f64.mul_add(num::count_to_f64(i), 0.94) / sr;
        }
    }

    pub fn clear(&mut self) {
        for ap in &mut self.stages {
            ap.line.clear();
        }
    }

    #[inline]
    pub fn tick(&mut self, x: f64) -> f64 {
        self.stages.iter_mut().fold(x, |v, ap| ap.tick(v))
    }
}

/// The figure-8's elements at size 0.5, ms: per half, the fixed allpass
/// (delay, g), its line's length to the decay edge, the second allpass
/// (delay, g) and the second line's length to the other half.
struct Half {
    first: (f64, f64),
    line1_ms: f64,
    second: (f64, f64),
    line2_ms: f64,
}
const HALVES: [Half; 2] = [
    Half { first: (217.604, 0.75), line1_ms: 850.104, second: (535.521, -0.5), line2_ms: 762.625 },
    Half { first: (165.312, 0.75), line1_ms: 897.812, second: (362.813, -0.5), line2_ms: 749.688 },
];
/// Where a tap reads.
#[derive(Clone, Copy)]
enum Node {
    /// A half's first line (after its fixed allpass).
    Line1(usize),
    /// Inside a half's second allpass.
    Inside(usize),
    /// A half's second line (after its second allpass, where the input
    /// also enters).
    Line2(usize),
}
/// The output taps, `(node, ms, right?, gain re a plain pulse)`.
const TAPS: [(Node, f64, bool, f64); 16] = [
    (Node::Line1(0), 53.646, true, 1.0126),
    (Node::Line1(0), 425.521, false, -1.0123),
    (Node::Line1(0), 599.896, true, 1.0128),
    (Node::Line1(1), 71.146, false, 1.0130),
    (Node::Line1(1), 401.146, true, -1.0118),
    (Node::Line1(1), 731.354, false, 1.0142),
    (Node::Line2(0), 24.479, false, -0.9976),
    (Node::Line2(0), 49.438, true, -0.9974),
    (Node::Line2(0), 402.396, true, 1.0004),
    (Node::Line2(1), 74.438, false, -0.9974),
    (Node::Line2(1), 214.896, true, -0.9966),
    (Node::Line2(1), 538.854, false, 0.9969),
    (Node::Inside(0), 67.604, false, -0.7454),
    (Node::Inside(0), 385.521, true, -0.7459),
    (Node::Inside(1), 247.604, false, -0.7449),
    (Node::Inside(1), 37.604, true, -0.7460),
];

struct Tap {
    node: Node,
    pos: usize,
    right: bool,
    /// The measured gain, and the gain in use (reshaped below the floor).
    base: f64,
    gain: f64,
    /// When the pulse comes on the first pass, seconds.
    t_s: f64,
}

struct HalfState {
    first: RingAp,
    line1: DelayLine,
    line1_len: usize,
    lp1: Lp1,
    second: RingAp,
    line2: DelayLine,
    line2_len: usize,
    lp2: Lp1,
}

/// The tank. Feed it the (mono-diffused) left and right inputs; it returns
/// the tapped stereo output.
pub struct CloudRing {
    stages: [[RingAp; 4]; 5],
    halves: [HalfState; 2],
    taps: [Tap; 16],
    /// The mix into the figure-8 and each stage's own tap, as in use
    /// (tilted below the floor like the taps), and when each stage's output
    /// comes, seconds.
    mix: [f64; 6],
    out: [(f64, f64); 5],
    junction_s: [f64; 6],
    sample_rate: f64,
    scale: f64,
    t60: f64,
    /// The decay edges' gains in use.
    edge: f64,
    edge_inner: f64,
    edge_lp: bool,
    damp_hz: f64,
}

impl CloudRing {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let stages = core::array::from_fn(|k| {
            core::array::from_fn(|j| {
                let ms = STAGES.get(k).and_then(|s| s.get(j)).map_or(10.0, |a| a.0);
                RingAp::new(ms, sample_rate, 0.0)
            })
        });
        let line = |ms: f64| DelayLine::new(num::f64_to_index((ms * 1e-3 * sample_rate).mul_add(MAX_SCALE, 64.0)));
        let halves = core::array::from_fn(|h| {
            let spec = HALVES.get(h).unwrap_or(&HALVES[0]);
            HalfState {
                first: RingAp::new(spec.first.0, sample_rate, 0.3 * num::count_to_f64(h)),
                line1: line(spec.line1_ms),
                line1_len: 1,
                lp1: Lp1::new(),
                second: RingAp::new(spec.second.0, sample_rate, 0.3 * num::count_to_f64(h) + 0.5),
                line2: line(spec.line2_ms),
                line2_len: 1,
                lp2: Lp1::new(),
            }
        });
        let taps = core::array::from_fn(|i| {
            let (node, ms, right, gain) = TAPS.get(i).copied().unwrap_or((Node::Line1(0), 1.0, false, 0.0));
            Tap { node, pos: 1, right, base: gain * TAP_GAIN, gain: gain * TAP_GAIN, t_s: ms * 1e-3 }
        });
        let mut ring = Self {
            stages,
            halves,
            taps,
            mix: JUNCTION_MIX,
            out: JUNCTION_OUT,
            junction_s: [0.0; 6],
            sample_rate,
            scale: 1.0,
            t60: 4.5,
            edge: 0.0,
            edge_inner: 0.0,
            edge_lp: true,
            damp_hz: EDGE_LP_HZ,
        };
        ring.set_damping(None);
        ring.set_size(0.5);
        ring
    }

    /// Size 0.5 is BigSky's tank; 0 → 0.5×, 1 → 1.5×.
    pub fn set_size(&mut self, size: f64) {
        self.scale = (size.clamp(0.0, 1.0) + 0.5).min(MAX_SCALE);
        let k = self.sample_rate * 1e-3 * self.scale;
        // Whole samples (see `InputChain::set_size`); the stages' outputs
        // come where their allpasses add up to.
        let mut at = 0.0;
        for (s, stage) in self.stages.iter_mut().enumerate() {
            for ap in stage.iter_mut() {
                ap.len = (ap.base_ms * k).round();
                at += ap.len;
            }
            if let Some(t) = self.junction_s.get_mut(s + 1) {
                *t = at / self.sample_rate;
            }
        }
        for (half, spec) in self.halves.iter_mut().zip(HALVES.iter()) {
            half.first.len = (spec.first.0 * k).round();
            half.second.len = (spec.second.0 * k).round();
            half.line1_len = num::f64_to_index((spec.line1_ms * k).round()).clamp(2, half.line1.len().saturating_sub(2));
            half.line2_len = num::f64_to_index((spec.line2_ms * k).round()).clamp(2, half.line2.len().saturating_sub(2));
        }
        for (tap, spec) in self.taps.iter_mut().zip(TAPS.iter()) {
            tap.pos = num::f64_to_index((spec.1 * k).round()).max(1);
            tap.t_s = spec.1 * 1e-3 * self.scale;
        }
        self.update_gains();
    }

    /// Decay time, seconds (≥ 1e5 holds the loop: Infinite).
    pub fn set_t60(&mut self, t60_s: f64) {
        self.t60 = t60_s.max(0.05);
        self.update_gains();
    }

    /// The stages' allpass gain: 0.85 × Diffusion (each allpass at its own
    /// share of it). The figure-8's allpasses are fixed.
    pub fn set_diffusion(&mut self, g: f64) {
        let g = g.clamp(0.0, 0.85);
        for (stage, spec) in self.stages.iter_mut().zip(STAGES.iter()) {
            for (ap, s) in stage.iter_mut().zip(spec.iter()) {
                ap.g = g * s.1;
            }
        }
        // `RingAp` puts −g on the instant path; the measured gains are
        // the other sign's.
        for (half, spec) in self.halves.iter_mut().zip(HALVES.iter()) {
            half.first.g = -spec.first.1;
            half.second.g = -spec.second.1;
        }
    }

    /// Modulation of the stages' allpasses, as the input chain's: `depth`
    /// in samples, `rate_hz` the chain's rate. BigSky's stages run ~10 %
    /// faster than its chain; each allpass a little off the mean. The
    /// figure-8 is not modulated (its pulses move only with the chain).
    pub fn set_modulation(&mut self, depth: f64, rate_hz: f64) {
        let sr = self.sample_rate;
        for (i, ap) in self.stages.iter_mut().flatten().enumerate() {
            ap.depth = depth;
            ap.inc = rate_hz * 1.1 * 0.005f64.mul_add(num::count_to_f64(i), 0.95) / sr;
        }
    }

    /// Extra damping on the decay edges: a lower cutoff for their low-pass,
    /// or BigSky's own.
    pub fn set_damping(&mut self, cutoff_hz: Option<f64>) {
        self.damp_hz = cutoff_hz.map_or(EDGE_LP_HZ, |fc| fc.clamp(200.0, EDGE_LP_HZ));
        let a = (-core::f64::consts::TAU * self.damp_hz / self.sample_rate).exp();
        for half in &mut self.halves {
            half.lp1.set_coeff(a);
            half.lp2.set_coeff(a);
        }
    }

    fn update_gains(&mut self) {
        let t = self.t60;
        let first = DECAY_G[0];
        let last = DECAY_G[DECAY_G.len() - 1];
        let infinite = t >= 1.0e5;
        let g = if infinite {
            1.0
        } else if t <= first.0 {
            first.1
        } else if t >= last.0 {
            // Past the table, hold the last decade's rate: g scales as the
            // fourth root of the per-cycle loss, which goes as 1/T.
            last.1.powf(last.0 / t)
        } else {
            DECAY_G.windows(2).find(|w| t <= w[1].0).map_or(last.1, |w| {
                let ((t0, g0), (t1, g1)) = (w[0], w[1]);
                let f = (t / t0).ln() / (t1 / t0).ln();
                (g1 - g0).mul_add(f, g0)
            })
        };
        // A resized tank keeps the same decay rate: a cycle's loss goes
        // with its length.
        let g = g.powf(self.scale);
        if infinite {
            // Lossless: the edges at unity and their low-pass off.
            self.edge = 1.0;
            self.edge_inner = 1.0;
            self.edge_lp = false;
        } else {
            self.edge = EDGE_GAIN * g;
            self.edge_inner = EDGE_GAIN_INNER * g;
            self.edge_lp = true;
        }
        // Shorter than BigSky's floor (which it cannot go): tilt the first
        // pass itself down to the time asked for.
        let tilt = if t < FLOOR_T60 { 1.0 / t - 1.0 / FLOOR_T60 } else { 0.0 };
        let fall = |t_s: f64| 10f64.powf(-3.0 * t_s * tilt);
        for tap in &mut self.taps {
            tap.gain = tap.base * fall(tap.t_s);
        }
        for (k, m) in self.mix.iter_mut().enumerate() {
            let t_s = self.junction_s.get(k).copied().unwrap_or(0.0);
            *m = JUNCTION_MIX.get(k).copied().unwrap_or(0.0) * fall(t_s);
        }
        for (k, o) in self.out.iter_mut().enumerate() {
            let t_s = self.junction_s.get(k + 1).copied().unwrap_or(0.0);
            let (l, r) = JUNCTION_OUT.get(k).copied().unwrap_or((0.0, 0.0));
            *o = (l * TAP_GAIN * fall(t_s), r * TAP_GAIN * fall(t_s));
        }
    }

    pub fn clear(&mut self) {
        for ap in self.stages.iter_mut().flatten() {
            ap.line.clear();
        }
        for half in &mut self.halves {
            half.first.line.clear();
            half.second.line.clear();
            half.line1.clear();
            half.line2.clear();
            half.lp1.reset();
            half.lp2.reset();
        }
    }

    /// One sample: both inputs enter the stages together.
    #[inline]
    pub fn tick(&mut self, in_l: f64, in_r: f64) -> (f64, f64) {
        let x0 = 0.5 * (in_l + in_r);
        let (mut out_l, mut out_r) = (0.0, 0.0);
        let mut v = x0;
        let mut x = x0 * self.mix[0];
        for ((stage, m), o) in self.stages.iter_mut().zip(self.mix.iter().skip(1)).zip(self.out.iter()) {
            v = stage.iter_mut().fold(v, |s, ap| ap.tick(s));
            x = m.mul_add(v, x);
            out_l = o.0.mul_add(v, out_l);
            out_r = o.1.mul_add(v, out_r);
        }
        // The figure-8. Reads first: every line still ends a sample back, so
        // `read(k)` is k samples ago.
        let lp = self.edge_lp;
        let [a, b] = &mut self.halves;
        let a_back = a.line2.read(a.line2_len);
        let b_back = b.line2.read(b.line2_len);
        let a_on = a.line1.read(a.line1_len);
        let b_on = b.line1.read(b.line1_len);
        let edge = |f: &mut Lp1, s: f64, gain: f64| gain * if lp { f.tick(s) } else { s };
        // Each half's first allpass hears the input and the OTHER half's end.
        let into_a = x + edge(&mut b.lp2, b_back, self.edge);
        let into_b = x + edge(&mut a.lp2, a_back, self.edge);
        for (half, into, on) in [(&mut *a, into_a, a_on), (&mut *b, into_b, b_on)] {
            let y1 = half.first.tick(into);
            half.line1.write(flush(y1));
            let y2 = half.second.tick(edge(&mut half.lp1, on, self.edge_inner));
            half.line2.write(flush(y2 + x));
        }
        // `read(1)` is the sample just written: a delay of k is read(k + 1).
        for tap in &self.taps {
            let s = match tap.node {
                Node::Line1(h) => self.halves.get(h).map_or(0.0, |hs| hs.line1.read(tap.pos + 1)),
                Node::Line2(h) => self.halves.get(h).map_or(0.0, |hs| hs.line2.read(tap.pos + 1)),
                Node::Inside(h) => self.halves.get(h).map_or(0.0, |hs| hs.second.line.read(tap.pos + 1)),
            } * tap.gain;
            if tap.right {
                out_r += s;
            } else {
                out_l += s;
            }
        }
        (out_l, out_r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse_energy(ring: &mut CloudRing, seconds: f64) -> Vec<f64> {
        // Energy per 100 ms window.
        let n = num::f64_to_index(seconds * 48_000.0);
        let mut out = Vec::new();
        let mut acc = 0.0;
        for i in 0..n {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let (l, r) = ring.tick(x, x);
            acc += l * l + r * r;
            if (i + 1) % 4800 == 0 {
                out.push(acc);
                acc = 0.0;
            }
        }
        out
    }

    /// Nothing recirculates before one trip: the output up to ~800 ms is
    /// the same whatever the decay, as on BigSky.
    #[test]
    fn decay_only_acts_after_one_trip() {
        let mut a = CloudRing::new(48_000.0);
        let mut b = CloudRing::new(48_000.0);
        // Both above BigSky's floor: below it the first pass itself is
        // tilted shorter, by design.
        a.set_t60(60.0);
        b.set_t60(6.0);
        a.set_diffusion(0.5);
        b.set_diffusion(0.5);
        let ea = impulse_energy(&mut a, 1.5);
        let eb = impulse_energy(&mut b, 1.5);
        // 7 windows = 700 ms, inside one trip (803 ms).
        for k in 0..7 {
            assert!((ea[k] - eb[k]).abs() <= 1e-9 * ea[k].max(1e-30), "window {k} differs before a trip");
        }
        assert!(eb[14] < ea[14] * 0.9, "the shorter decay is quieter after a trip");
    }

    /// A long decay rings long and a short one dies — and Infinite holds.
    #[test]
    fn decay_is_bounded_and_ordered() {
        for t60 in [1.0, 4.5, 20.0] {
            let mut r = CloudRing::new(48_000.0);
            r.set_t60(t60);
            r.set_diffusion(0.6);
            let e = impulse_energy(&mut r, 8.0);
            assert!(e.iter().all(|v| v.is_finite()));
            let early: f64 = e[2..8].iter().sum();
            let late: f64 = e[70..80].iter().sum();
            assert!(late < early, "T60 {t60}: decays");
        }
        let mut inf = CloudRing::new(48_000.0);
        inf.set_t60(1.0e6);
        inf.set_diffusion(0.6);
        let e = impulse_energy(&mut inf, 20.0);
        assert!(e.iter().all(|v| v.is_finite() && *v < 100.0), "Infinite stays bounded");
    }
}
