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
    /// Dry/wet (1 = fully processed).
    pub mix: f32,
    /// Output trim (linear).
    pub output_gain: f32,
    // DC blocker state per channel: y[n] = x[n] − x[n−1] + R·y[n−1].
    dc_x1: [f32; MAX_CHANNELS],
    dc_y1: [f32; MAX_CHANNELS],
    dc_r: f32,
    /// Sag envelope (per channel, ~30 ms ballistics).
    sag_env: [f32; MAX_CHANNELS],
    sag_coeff: f32,
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
            mix: 1.0,
            output_gain: 1.0,
            dc_x1: [0.0; MAX_CHANNELS],
            dc_y1: [0.0; MAX_CHANNELS],
            dc_r: 0.0,
            sag_env: [0.0; MAX_CHANNELS],
            sag_coeff: 0.0,
            sample_rate: 48_000.0,
        };
        p.set_sample_rate(sample_rate);
        p
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.dc_r = 1.0 - core::f32::consts::TAU * DC_BLOCK_HZ / self.sample_rate;
        // ~30 ms sag ballistics. Padé form of 1 − e^(−x) for tiny x —
        // keeps the crate honestly no_std (f32::exp only resolves here
        // when a feature happens to link std).
        let x = 1.0 / (0.030 * self.sample_rate);
        self.sag_coeff = x / (1.0 + x);
    }

    /// The static (stateless) transfer: bias, side-split shaping. The
    /// DC the blocker removes at rest is subtracted so the curve is
    /// origin-anchored — this is what the display draws.
    #[inline]
    pub fn transfer(&self, x: f32) -> f32 {
        let v = x * self.drive + self.q_point;
        let shaped = if v >= 0.0 {
            self.positive.shape(v)
        } else {
            self.negative.shape(v)
        };
        let rest = if self.q_point >= 0.0 {
            self.positive.shape(self.q_point)
        } else {
            self.negative.shape(self.q_point)
        };
        shaped - rest
    }

    /// Process one sample on channel `ch`.
    #[inline]
    pub fn process(&mut self, ch: usize, input: f32) -> f32 {
        // Sag: program level pulls the operating point off center.
        let bias = if self.sag > 0.0 {
            let e = &mut self.sag_env[ch];
            *e += (input.abs() * self.drive - *e) * self.sag_coeff;
            self.q_point + self.sag * (*e).min(2.0) * 0.4
        } else {
            self.q_point
        };
        let shaped = {
            let v = input * self.drive + bias;
            if v >= 0.0 {
                self.positive.shape(v)
            } else {
                self.negative.shape(v)
            }
        };
        // Output DC blocker — the bias voltage never leaves the box.
        let y = shaped - self.dc_x1[ch] + self.dc_r * self.dc_y1[ch];
        self.dc_x1[ch] = shaped;
        self.dc_y1[ch] = y;
        // Small-signal gain compensation (drive is unity-slope at the
        // origin for every curve, so 1/drive re-centers loudness).
        let wet = y / self.drive.max(1.0e-3);
        (input + (wet - input) * self.mix) * self.output_gain
    }

    pub fn reset(&mut self) {
        self.dc_x1 = [0.0; MAX_CHANNELS];
        self.dc_y1 = [0.0; MAX_CHANNELS];
        self.sag_env = [0.0; MAX_CHANNELS];
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
