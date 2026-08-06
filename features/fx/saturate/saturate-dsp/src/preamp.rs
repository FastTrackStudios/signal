//! Class-A preamp saturation — asymmetric even-order harmonic engine.
//!
//! The physics being modeled (behavior-level, published amplifier
//! theory): a class-A stage idles at a DC operating point (the
//! **Q point**). Shifting the Q point toward one rail makes the
//! transfer asymmetric — one half of the waveform saturates earlier
//! than the other — which is exactly what generates **even-order
//! harmonics** (2nd/4th/6th), the "vintage preamp" richness a class-AB
//! stage (symmetric, odd-only) doesn't produce. A strong DC blocker on
//! the output (the coupling transformer/capacitor in a 1073-style
//! design) keeps the bias voltage itself out of the program.
//!
//! Beyond the bias, each SIDE of the waveform gets its own transfer
//! curve — transformer knee on the positive half, op-amp hard-ish
//! clip on the negative, any combination — so asymmetry (and its even
//! harmonics) is available even at Q = 0.
//!
//! The `analysis` feature adds display helpers: the static transfer
//! curve (for the big indicative slow sine view — deliberately NOT an
//! oscilloscope) and a harmonic-spectrum probe measuring H1..Hn of the
//! current settings against an internally synthesized sine.

use crate::tanh_approx;

/// Per-side transfer curves. All pass through the origin with unity
/// small-signal slope, so `Clean`/`Clean` at Q = 0 is transparent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum SideShaper {
    /// Wire — no shaping.
    #[default]
    Clean = 0,
    /// Op-amp / class-AB rail: firm tanh limiting.
    OpAmp = 1,
    /// Single-ended triode grid: soft rational compression
    /// (`x / (1 + |x|)` family) — early, gentle, strongly 2nd-order
    /// when paired asymmetrically.
    Tube = 2,
    /// Transformer core: polynomial soft knee blending into tanh
    /// (gentle onset, stronger 3rd) — the "iron" curve.
    Transformer = 3,
    /// Diode knee: faster-onset rational fold.
    Diode = 4,
    /// Hard rail clip.
    Hard = 5,
}

impl SideShaper {
    pub fn from_index(idx: u32) -> Self {
        match idx {
            1 => SideShaper::OpAmp,
            2 => SideShaper::Tube,
            3 => SideShaper::Transformer,
            4 => SideShaper::Diode,
            5 => SideShaper::Hard,
            _ => SideShaper::Clean,
        }
    }

    /// Static transfer (stateless): input in shaper units.
    #[inline]
    pub fn shape(self, x: f32) -> f32 {
        match self {
            SideShaper::Clean => x,
            SideShaper::OpAmp => tanh_approx(x),
            SideShaper::Tube => x / (1.0 + x.abs()),
            SideShaper::Transformer => {
                let soft = x - (x * x * x) / 3.0;
                let t = (x.abs() * 0.5).clamp(0.0, 1.0);
                soft * (1.0 - t) + tanh_approx(x) * t
            }
            SideShaper::Diode => {
                // Faster knee than tanh: rational fold with 1.5x onset.
                let v = x * 1.5;
                (v / (1.0 + v * v).max(1.0)) + x * 0.2 / (1.0 + x * x)
            }
            SideShaper::Hard => x.clamp(-1.0, 1.0),
        }
    }
}

/// Output DC-blocker pole: ~10 Hz at 48 kHz — strong, like the output
/// coupling of a 1073-class design.
const DC_BLOCK_HZ: f32 = 10.0;
/// Where the tilt filter splits low from high. 700 Hz is the usual
/// emphasis hinge: high enough that it is the *top* being pushed, low
/// enough that turning it the other way reaches the body of a bass.
const TILT_HZ: f32 = 700.0;
/// The widest tilt either way. Beyond this the de-emphasis is undoing
/// so much that the stage is mostly filter.
pub const TILT_MAX_DB: f32 = 12.0;
/// Widest crossover deadband, in shaper units.
const CROSSOVER_MAX: f32 = 0.15;
pub const MAX_CHANNELS: usize = 2;

#[derive(Debug, Clone)]
pub struct ClassAPreamp {
    /// Drive into the stage (linear, 1..16 from the user's dB knob).
    pub drive: f32,
    /// Q point / operating-point bias, −1..1 (0 = centered = class-AB
    /// symmetric behavior; raise it and even harmonics spring up).
    pub q_point: f32,
    /// Positive-half transfer.
    pub positive: SideShaper,
    /// Negative-half transfer.
    pub negative: SideShaper,
    /// Bias sag (0..1): the operating point droops with program level
    /// (cathode/supply sag in a real tube stage) — louder passages get
    /// MORE asymmetry, so even harmonics bloom dynamically instead of
    /// sitting at a static level.
    pub sag: f32,
    /// Per-side onset asymmetry, −1..1. Applied as a gain INTO the
    /// shaper and taken straight back out, so the small-signal slope
    /// stays unity on both halves and only the *knee* moves. That
    /// matters: scaling one half's output would put a kink at the zero
    /// crossing (a buzz), whereas moving where each half runs out of
    /// room is what a real single-ended stage does — and is where its
    /// even harmonics come from. Positive = the top half clips first.
    pub skew: f32,
    /// Headroom, ~0.25..4. Scales where the knee sits without changing
    /// the small-signal gain: a bigger core, a higher rail, a tape
    /// biased hotter. Above 1 the stage stays clean longer.
    pub headroom: f32,
    /// Blend of the chosen shapers toward a hard rail clip, 0..1. The
    /// corner a solid-state stage has and a valve does not.
    pub knee: f32,
    /// Class-B crossover deadband, 0..1. Both halves hand over at zero
    /// and neither is conducting in between — the notch that makes an
    /// underbiased transistor stage buzz on quiet passages, and the
    /// same mechanism that gates a starved fuzz.
    pub crossover: f32,
    /// Dry/wet (1 = fully processed).
    pub mix: f32,
    /// Output trim (linear).
    pub output_gain: f32,
    // DC blocker state per channel: y[n] = x[n] − x[n−1] + R·y[n−1].
    dc_x1: [f32; MAX_CHANNELS],
    dc_y1: [f32; MAX_CHANNELS],
    dc_r: f32,
    /// Sag envelope (per channel).
    sag_env: [f32; MAX_CHANNELS],
    sag_coeff: f32,
    sag_ms: f32,
    /// Tilt: pre-emphasis into the stage, de-emphasis out of it. Which
    /// part of the spectrum drives the circuit hardest is most of what
    /// separates one saturator from another — a transformer takes its
    /// lows into the core first, a tape machine pre-emphasises the top.
    ///
    /// The emphasis is a first-order shelf built from the one-pole
    /// split at [`TILT_HZ`]; the de-emphasis is its **exact** inverse
    /// rather than a mirrored second shelf, which two shelves are not.
    /// That matters here: the whole point is that the tilt changes what
    /// the shaper sees and nothing else, so anything it leaves behind
    /// in the output is a lie about what the knob does.
    tilt_db: f32,
    // Emphasis H(z) = (b0 + b1·z⁻¹) / (1 + a1·z⁻¹).
    tilt_b0: f32,
    tilt_b1: f32,
    tilt_a1: f32,
    tilt_pre: [(f32, f32); MAX_CHANNELS],
    tilt_post: [(f32, f32); MAX_CHANNELS],
    sample_rate: f32,
}

impl ClassAPreamp {
    pub fn new(sample_rate: f32) -> Self {
        let mut p = Self {
            drive: 1.0,
            q_point: 0.0,
            positive: SideShaper::Clean,
            negative: SideShaper::Clean,
            sag: 0.0,
            skew: 0.0,
            headroom: 1.0,
            knee: 0.0,
            crossover: 0.0,
            mix: 1.0,
            output_gain: 1.0,
            dc_x1: [0.0; MAX_CHANNELS],
            dc_y1: [0.0; MAX_CHANNELS],
            dc_r: 0.0,
            sag_env: [0.0; MAX_CHANNELS],
            sag_coeff: 0.0,
            sag_ms: 30.0,
            tilt_db: 0.0,
            tilt_b0: 1.0,
            tilt_b1: 0.0,
            tilt_a1: 0.0,
            tilt_pre: [(0.0, 0.0); MAX_CHANNELS],
            tilt_post: [(0.0, 0.0); MAX_CHANNELS],
            sample_rate: 48_000.0,
        };
        p.set_sample_rate(sample_rate);
        p
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.dc_r = 1.0 - core::f32::consts::TAU * DC_BLOCK_HZ / self.sample_rate;
        let ms = self.sag_ms;
        self.set_sag_ms(ms);
        let db = self.tilt_db;
        self.set_tilt_db(db);
    }

    /// Sag ballistics in milliseconds. A tape machine's recovery is
    /// slower than a valve's cathode, and how long the stage stays bent
    /// after a transient is audibly a different machine.
    pub fn set_sag_ms(&mut self, ms: f32) {
        self.sag_ms = ms.clamp(1.0, 500.0);
        // Padé form of 1 − e^(−x) for tiny x — keeps the crate honestly
        // no_std (f32::exp only resolves here when a feature happens to
        // link std).
        let x = 1.0 / (self.sag_ms * 0.001 * self.sample_rate);
        self.sag_coeff = x / (1.0 + x);
    }

    pub fn sag_ms(&self) -> f32 {
        self.sag_ms
    }

    /// Pre-emphasis tilt in dB: positive drives the top of the spectrum
    /// into the stage harder, negative drives the bottom. The mirror
    /// image is applied on the way out, so the tilt decides *what*
    /// distorts rather than what the output sounds like.
    pub fn set_tilt_db(&mut self, db: f32) {
        self.tilt_db = db.clamp(-TILT_MAX_DB, TILT_MAX_DB);
        // The shelf, written out from the one-pole split. With
        // L(z) = a / (1 − (1−a)z⁻¹) and H = lo·L + hi·(1 − L):
        //   H(z) = (hi + (lo − hi)a − hi(1−a)z⁻¹) / (1 − (1−a)z⁻¹)
        let a = (core::f32::consts::TAU * TILT_HZ / self.sample_rate).clamp(1.0e-4, 1.0);
        let hi = crate::db_to_gain(self.tilt_db);
        let lo = crate::db_to_gain(-self.tilt_db);
        self.tilt_b0 = hi + (lo - hi) * a;
        self.tilt_b1 = -hi * (1.0 - a);
        self.tilt_a1 = -(1.0 - a);
    }

    pub fn tilt_db(&self) -> f32 {
        self.tilt_db
    }

    /// The class-B crossover deadband: neither half conducts until the
    /// signal has climbed out of it.
    #[inline]
    fn deadband(&self, x: f32) -> f32 {
        if self.crossover <= 0.0 {
            return x;
        }
        let d = self.crossover.min(1.0) * CROSSOVER_MAX;
        if x > d {
            x - d
        } else if x < -d {
            x + d
        } else {
            0.0
        }
    }

    /// One half of the wave through its own shaper, at this stage's
    /// headroom, skew and knee.
    #[inline]
    fn shape_side(&self, v: f32) -> f32 {
        let h = self.headroom.clamp(0.05, 16.0);
        // Skew as a gain in and back out: the knee moves, the slope at
        // the origin does not, so the two halves still meet smoothly.
        let g = if v >= 0.0 {
            1.0 + self.skew
        } else {
            1.0 - self.skew
        }
        .clamp(0.05, 4.0);
        let side = if v >= 0.0 { self.positive } else { self.negative };
        let scale = h / g;
        let soft = side.shape(v / scale) * scale;
        if self.knee > 0.0 {
            let hard = (v / h).clamp(-1.0, 1.0) * h;
            soft + (hard - soft) * self.knee.min(1.0)
        } else {
            soft
        }
    }

    /// The static (stateless) transfer: crossover, bias, side-split
    /// shaping. The DC the blocker removes at rest is subtracted so the
    /// curve is origin-anchored — this is what the display draws.
    ///
    /// Tilt is deliberately absent: it is a filter pair, so it has no
    /// single transfer curve to draw. Everything a panel *can* draw is
    /// here, and it is the same arithmetic `process` runs.
    #[inline]
    pub fn transfer(&self, x: f32) -> f32 {
        let v = self.deadband(x * self.drive) + self.q_point;
        self.shape_side(v) - self.shape_side(self.q_point)
    }

    /// Process one sample on channel `ch`, dry/wet mixed and trimmed.
    #[inline]
    pub fn process(&mut self, ch: usize, input: f32) -> f32 {
        let wet = self.process_wet(ch, input);
        (input + (wet - input) * self.mix) * self.output_gain
    }

    /// The stage alone: emphasis, shaping, de-emphasis — no mix, no
    /// output trim. Callers that put another stage after this one (the
    /// digital family runs [`crate::digital::DigitalStage`] on the wet
    /// path) need to mix *after* that, not here.
    #[inline]
    pub fn process_wet(&mut self, ch: usize, input: f32) -> f32 {
        let ch = ch.min(MAX_CHANNELS - 1);
        let x = self.tilt_in(ch, input);
        // Sag: program level pulls the operating point off center.
        let bias = if self.sag > 0.0 {
            let e = &mut self.sag_env[ch];
            *e += (x.abs() * self.drive - *e) * self.sag_coeff;
            self.q_point + self.sag * (*e).min(2.0) * 0.4
        } else {
            self.q_point
        };
        let shaped = self.shape_side(self.deadband(x * self.drive) + bias);
        // Output DC blocker — the bias voltage never leaves the box.
        let y = shaped - self.dc_x1[ch] + self.dc_r * self.dc_y1[ch];
        self.dc_x1[ch] = shaped;
        self.dc_y1[ch] = y;
        // Small-signal gain compensation (drive is unity-slope at the
        // origin for every curve, so 1/drive re-centers loudness).
        let wet = y / self.drive.max(1.0e-3);
        self.tilt_out(ch, wet)
    }

    /// Pre-emphasis: the shelf, applied.
    ///
    /// Run unconditionally rather than short-circuited at zero tilt —
    /// at zero the coefficients make it an identity anyway, and keeping
    /// the state warm means moving the knob (or switching profile) does
    /// not click.
    #[inline]
    fn tilt_in(&mut self, ch: usize, x: f32) -> f32 {
        let (x1, y1) = &mut self.tilt_pre[ch];
        let y = self.tilt_b0 * x + self.tilt_b1 * *x1 - self.tilt_a1 * *y1;
        *x1 = x;
        *y1 = y;
        y
    }

    /// De-emphasis: 1/H(z), so the signal comes out exactly level. What
    /// changed is only which part of it was pushed into the knee.
    #[inline]
    fn tilt_out(&mut self, ch: usize, x: f32) -> f32 {
        let (x1, y1) = &mut self.tilt_post[ch];
        let y = (x + self.tilt_a1 * *x1 - self.tilt_b1 * *y1) / self.tilt_b0;
        *x1 = x;
        *y1 = y;
        y
    }

    pub fn reset(&mut self) {
        self.dc_x1 = [0.0; MAX_CHANNELS];
        self.dc_y1 = [0.0; MAX_CHANNELS];
        self.sag_env = [0.0; MAX_CHANNELS];
        self.tilt_pre = [(0.0, 0.0); MAX_CHANNELS];
        self.tilt_post = [(0.0, 0.0); MAX_CHANNELS];
    }
}

/// Display + metering helpers (std: needs real trig for the probes).
#[cfg(feature = "analysis")]
pub mod analysis {
    extern crate std;
    use super::ClassAPreamp;

    /// Sample the static transfer curve into `out` over x ∈ [−1, 1] —
    /// the indicative display draws its big slow sine through this.
    pub fn transfer_curve(pre: &ClassAPreamp, out: &mut [(f32, f32)]) {
        let n = out.len().max(2);
        for (i, slot) in out.iter_mut().enumerate() {
            let x = -1.0 + 2.0 * i as f32 / (n - 1) as f32;
            *slot = (x, pre.transfer(x));
        }
    }

    /// Measure the harmonic spectrum of the CURRENT settings: an
    /// internally synthesized full-scale sine runs through a state
    /// clone (including the DC blocker), and Goertzel correlation
    /// reads H1..Hn. `out[k]` = linear magnitude of harmonic k+1,
    /// normalized so H1 = 1. This is the "what is the saturation
    /// actually adding" visualization — measured, not hand-waved.
    pub fn harmonic_spectrum(pre: &ClassAPreamp, out: &mut [f32]) {
        const N: usize = 8192;
        const CYCLES: usize = 64;
        let mut probe = pre.clone();
        probe.mix = 1.0;
        probe.output_gain = 1.0;
        probe.reset();
        let mut buf = [0.0f32; N];
        for (i, b) in buf.iter_mut().enumerate() {
            let ph = core::f32::consts::TAU * CYCLES as f32 * i as f32 / N as f32;
            *b = probe.process(0, ph.sin());
        }
        // Skip the DC-blocker warmup: analyze the second half.
        let seg = &buf[N / 2..];
        let mut h1 = 0.0f32;
        for (k, slot) in out.iter_mut().enumerate() {
            let f = (CYCLES * (k + 1)) as f32 / N as f32;
            let mut re = 0.0f32;
            let mut im = 0.0f32;
            for (i, &s) in seg.iter().enumerate() {
                let ph = core::f32::consts::TAU * f * i as f32;
                re += s * ph.cos();
                im += s * ph.sin();
            }
            let mag = (re * re + im * im).sqrt();
            if k == 0 {
                h1 = mag.max(1.0e-12);
            }
            *slot = mag / h1.max(1.0e-12);
        }
    }
}

#[cfg(all(test, feature = "analysis"))]
mod tests {
    extern crate std;
    use super::analysis::harmonic_spectrum;
    use super::*;

    fn db(x: f32) -> f32 {
        20.0 * x.max(1.0e-9).log10()
    }

    #[test]
    fn symmetric_drive_is_odd_only() {
        // Same shaper both sides, Q centered: class-AB behavior —
        // 3rd/5th present, 2nd/4th buried.
        let mut p = ClassAPreamp::new(48_000.0);
        p.drive = 4.0;
        p.positive = SideShaper::OpAmp;
        p.negative = SideShaper::OpAmp;
        let mut h = [0.0f32; 6];
        harmonic_spectrum(&p, &mut h);
        assert!(db(h[2]) > -40.0, "3rd harmonic present: {} dB", db(h[2]));
        assert!(
            db(h[1]) < db(h[2]) - 30.0,
            "2nd stays buried when symmetric: H2={} H3={}",
            db(h[1]),
            db(h[2])
        );
    }

    #[test]
    fn raising_the_q_point_springs_even_harmonics() {
        // The video demo: raise the Q point and 2nd/4th spring into
        // life.
        let mut p = ClassAPreamp::new(48_000.0);
        p.drive = 4.0;
        p.positive = SideShaper::OpAmp;
        p.negative = SideShaper::OpAmp;
        p.q_point = 0.5;
        let mut h = [0.0f32; 6];
        harmonic_spectrum(&p, &mut h);
        assert!(
            db(h[1]) > -30.0,
            "2nd harmonic springs up with bias: {} dB",
            db(h[1])
        );
        assert!(
            db(h[3]) > -60.0,
            "4th follows: {} dB",
            db(h[3])
        );
    }

    #[test]
    fn different_side_shapers_are_asymmetric_without_bias() {
        // Transformer up / tube down at Q = 0: still even harmonics —
        // per-side asymmetry alone does it.
        let mut p = ClassAPreamp::new(48_000.0);
        p.drive = 4.0;
        p.positive = SideShaper::Hard;
        p.negative = SideShaper::Tube;
        let mut h = [0.0f32; 4];
        harmonic_spectrum(&p, &mut h);
        assert!(
            db(h[1]) > -35.0,
            "per-side asymmetry generates 2nd: {} dB",
            db(h[1])
        );
    }

    #[test]
    fn dc_never_leaves_the_box() {
        // Big bias, hard drive: the output mean must still settle to
        // ~zero (the blocker holds the operating point inside).
        let mut p = ClassAPreamp::new(48_000.0);
        p.drive = 8.0;
        p.q_point = 0.8;
        p.positive = SideShaper::Tube;
        p.negative = SideShaper::OpAmp;
        let mut mean = 0.0f64;
        let n = 96_000;
        for i in 0..n {
            let x = (core::f32::consts::TAU * 100.0 * i as f32 / 48_000.0).sin() * 0.5;
            let y = p.process(0, x);
            if i >= n / 2 {
                mean += f64::from(y);
            }
        }
        mean /= (n / 2) as f64;
        assert!(
            mean.abs() < 1.0e-3,
            "output DC must be blocked: mean={mean:e}"
        );
    }

    /// Skew is the other route to even harmonics, and the one a Heat
    /// knob rides: same shaper on both halves, centered Q, and the
    /// asymmetry comes purely from one side reaching its knee first.
    #[test]
    fn skew_alone_springs_even_harmonics() {
        let mut p = ClassAPreamp::new(48_000.0);
        p.drive = 4.0;
        p.positive = SideShaper::OpAmp;
        p.negative = SideShaper::OpAmp;
        let mut flat = [0.0f32; 4];
        harmonic_spectrum(&p, &mut flat);
        p.skew = 0.5;
        let mut skewed = [0.0f32; 4];
        harmonic_spectrum(&p, &mut skewed);
        assert!(
            db(skewed[1]) > db(flat[1]) + 20.0,
            "skew must raise the 2nd: {} → {} dB",
            db(flat[1]),
            db(skewed[1]),
        );
    }

    /// …and it must do it without putting a kink at the zero crossing.
    /// A gain applied to one half's OUTPUT would break the slope there,
    /// which is a buzz rather than a warmth.
    #[test]
    fn skew_leaves_the_slope_continuous_at_zero() {
        let mut p = ClassAPreamp::new(48_000.0);
        p.drive = 1.0;
        p.positive = SideShaper::Tube;
        p.negative = SideShaper::OpAmp;
        p.skew = 0.8;
        let e = 1.0e-4;
        let up = (p.transfer(e) - p.transfer(0.0)) / e;
        let down = (p.transfer(0.0) - p.transfer(-e)) / e;
        assert!(
            (up - down).abs() < 1.0e-2,
            "slope jumps across zero: {up} vs {down}",
        );
    }

    /// Headroom moves the knee, not the gain. Below it the stage is the
    /// same wire it was; the difference is only how far you can push.
    #[test]
    fn headroom_moves_the_knee_and_not_the_small_signal_gain() {
        let mut tight = ClassAPreamp::new(48_000.0);
        tight.drive = 1.0;
        tight.positive = SideShaper::OpAmp;
        tight.negative = SideShaper::OpAmp;
        let mut roomy = tight.clone();
        roomy.headroom = 3.0;

        // Small signal: indistinguishable.
        assert!((tight.transfer(0.01) - roomy.transfer(0.01)).abs() < 1.0e-3);
        // Hard into it: the roomier stage is still climbing.
        assert!(
            roomy.transfer(2.0) > tight.transfer(2.0) * 1.5,
            "headroom must hold the knee off: {} vs {}",
            tight.transfer(2.0),
            roomy.transfer(2.0),
        );
    }

    /// The corner a solid-state stage has. Fully up, the knee IS a hard
    /// rail, whatever shapers the profile named — so the curve has to
    /// land exactly on one.
    #[test]
    fn a_full_knee_is_a_hard_rail_whatever_the_shapers_were() {
        let mut p = ClassAPreamp::new(48_000.0);
        p.drive = 1.0;
        p.positive = SideShaper::Tube;
        p.negative = SideShaper::Transformer;
        p.knee = 1.0;
        for i in -40..=40 {
            let x = i as f32 / 20.0;
            assert!(
                (p.transfer(x) - x.clamp(-1.0, 1.0)).abs() < 1.0e-5,
                "knee=1 must be a hard clip at {x}: {}",
                p.transfer(x),
            );
        }
    }

    /// …and on the way there it sharpens the curve, which is audible as
    /// high-order odd content the soft shaper does not make.
    #[test]
    fn knee_sharpens_toward_a_hard_clip() {
        let mut soft = ClassAPreamp::new(48_000.0);
        soft.drive = 4.0;
        soft.positive = SideShaper::Tube;
        soft.negative = SideShaper::Tube;
        let mut hard = soft.clone();
        hard.knee = 1.0;
        let (mut a, mut b) = ([0.0f32; 8], [0.0f32; 8]);
        harmonic_spectrum(&soft, &mut a);
        harmonic_spectrum(&hard, &mut b);
        assert!(
            db(b[4]) > db(a[4]) + 4.0,
            "a hard corner has more 5th: {} → {} dB",
            db(a[4]),
            db(b[4]),
        );
    }

    /// Crossover is a deadband, so quiet signal genuinely stops. That
    /// is the gating a starved supply does, and it should be audible as
    /// silence rather than as a quieter version of the input.
    #[test]
    fn crossover_gates_what_sits_inside_the_deadband() {
        let mut p = ClassAPreamp::new(48_000.0);
        p.drive = 1.0;
        p.crossover = 1.0;
        assert_eq!(p.transfer(0.05), 0.0, "inside the deadband must be silence");
        assert!(p.transfer(0.5) != 0.0, "outside it, the stage conducts");
    }

    /// Tilt decides what distorts, not what the output sounds like: a
    /// clean stage with any tilt is still a wire, because the
    /// de-emphasis is the mirror of the emphasis.
    #[test]
    fn tilt_is_undone_when_the_stage_is_clean() {
        let mut p = ClassAPreamp::new(48_000.0);
        p.set_tilt_db(9.0);
        let mut max_err = 0.0f32;
        for i in 0..48_000 {
            let x = (core::f32::consts::TAU * 1_000.0 * i as f32 / 48_000.0).sin() * 0.5;
            let y = p.process(0, x);
            if i > 24_000 {
                max_err = max_err.max((y - x).abs());
            }
        }
        assert!(max_err < 0.05, "emphasis must mirror out: {max_err}");
    }

    /// …and it does change which band drives the knee. Tilted at the
    /// top, a bass note saturates less than it did flat.
    #[test]
    fn tilt_decides_which_band_meets_the_knee() {
        let harmonics_of = |tilt: f32| {
            let mut p = ClassAPreamp::new(48_000.0);
            p.drive = 6.0;
            p.positive = SideShaper::OpAmp;
            p.negative = SideShaper::OpAmp;
            p.set_tilt_db(tilt);
            // 60 Hz — well below the 700 Hz hinge.
            let mut peak = 0.0f32;
            for i in 0..48_000 {
                let x = (core::f32::consts::TAU * 60.0 * i as f32 / 48_000.0).sin() * 0.5;
                let y = p.process(0, x);
                if i > 24_000 {
                    peak = peak.max(y.abs());
                }
            }
            peak
        };
        // Driving the TOP harder means the low note arrives at the
        // shaper quieter, so less of it is clipped away and more of it
        // survives to the output.
        assert!(
            harmonics_of(9.0) > harmonics_of(-9.0),
            "a low note must clip less when the tilt favours the top",
        );
    }

    #[test]
    fn clean_at_center_is_transparent() {
        let mut p = ClassAPreamp::new(48_000.0);
        let mut max_err = 0.0f32;
        for i in 0..48_000 {
            let x = (core::f32::consts::TAU * 440.0 * i as f32 / 48_000.0).sin() * 0.5;
            let y = p.process(0, x);
            if i > 24_000 {
                max_err = max_err.max((y - x).abs());
            }
        }
        // The DC blocker's tiny LF phase shift is the only difference
        // (≈ −39 dB at 440 Hz with a 10 Hz pole).
        assert!(max_err < 0.02, "clean settings ≈ wire: {max_err}");
    }
}
