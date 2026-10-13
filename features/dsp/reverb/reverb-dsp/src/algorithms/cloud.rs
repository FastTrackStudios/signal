//! Cloud — after Strymon `BigSky`'s Cloud, from measurements of the
//! plug-in (signal-analyzer's `bigsky_match`) and the `BigSky` manuals
//! (`spec/bigsky-classic-reference.md`).
//!
//! The manual's sketch is cascaded input diffusion ahead of a reverb
//! built from interconnected loops of allpasses and delays (Griesinger's
//! late-'70s structure); the measurements fill in the numbers:
//!
//! - **Input**: a mono chain of four allpasses whose delays add to
//!   ~183 ms. Diffusion is their *gain* — at zero each stage is a pure
//!   delay and the plug-in's impulse response is silent for 183 ms, then
//!   a single pulse identical in L and R. Its output also goes straight to
//!   both outputs (the mono early field: L/R correlation 0.99 for 20 ms).
//!   Modulation is a quadrature LFO on these stages — depth up to "2
//!   o'clock", then rate.
//! - **Tank**: [`CloudRing`] — five four-allpass stages in series, the
//!   input and each stage's output mixed into one line read by BigSky's
//!   158-tap pattern; that one pass is the ~4.5 s floor every short Decay
//!   lands on, and it recirculates every 803 ms with one decay gain.
//! - **Low End**: a static one-pole high-pass on the input, 549 Hz (−10)
//!   → 300 Hz (0) → 84 Hz (+10).
//!
//! This replaced a `CloudSeed` port (parallel comb lines): its tail
//! started decaying the moment it started, where `BigSky`'s holds level
//! for half a second, and it could not reach the plug-in's decay floor or
//! its early stereo at all.

use dsp_core::num;

use crate::algorithm::{AlgorithmParams, CLOUD_T60, CloudParams, ReverbAlgorithm, decay_to_t60};
use crate::primitives::cloud_ring::{CloudRing, EARLY_TO_TAP, InputChain};
use crate::primitives::one_pole::{Hp1, Lp1};
use audiocore_dsp::biquad::{Biquad, FilterType};

/// Level of the input chain's own output (the early field), linear:
/// BigSky's early pulse is 1.4× its first ring tap.
const EARLY_OUT: f64 = EARLY_TO_TAP * 0.25;

/// BigSky's modulation by MOD knob (0…127): how far each Diffusion allpass's
/// delay shortens at most, ms, and its LFO's rate, Hz.
const MOD_DEPTH_MS: [(f64, f64); 6] = [(0.0, 0.0), (64.0, 0.385), (80.0, 0.37), (96.0, 0.31), (112.0, 0.27), (127.0, 0.207)];
const MOD_RATE_HZ: [(f64, f64); 8] =
    [(0.0, 0.87), (72.0, 0.88), (80.0, 1.16), (88.0, 1.68), (96.0, 2.3), (104.0, 3.1), (112.0, 4.26), (127.0, 7.5)];

/// Linear interpolation in a `(x, y)` table, clamped at its ends.
fn interp<const N: usize>(table: &[(f64, f64); N], x: f64) -> f64 {
    let (Some(first), Some(last)) = (table.first(), table.last()) else { return 0.0 };
    if x <= first.0 {
        return first.1;
    }
    table.windows(2).find(|w| x <= w[1].0).map_or(last.1, |w| {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        (y1 - y0).mul_add((x - x0) / (x1 - x0), y0)
    })
}

/// One side's input stage: Low End high-pass, then the diffusion chain.
struct InputStage {
    low_cut: Hp1,
    diffuser: InputChain,
}

impl InputStage {
    fn new(sample_rate: f64) -> Self {
        let mut low_cut = Hp1::new();
        low_cut.set_freq(300.0, sample_rate);
        Self { low_cut, diffuser: InputChain::new(sample_rate) }
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        // The chain's buffers are sized for the rate it was built at.
        self.diffuser = InputChain::new(sample_rate);
        self.low_cut.set_sample_rate(sample_rate);
    }

    fn clear(&mut self) {
        self.low_cut.reset();
        self.diffuser.clear();
    }

    #[inline]
    fn tick(&mut self, x: f64) -> f64 {
        let x = self.low_cut.tick(x);
        self.diffuser.tick(x)
    }
}

/// BigSky's Tone, measured: two cascaded first-order high shelves, flat
/// at the top of the knob and deepening as it comes down (fits the
/// plug-in's third-octave response to 0.14 dB RMS, 62 Hz–16 kHz).
/// `(knob 0–127, shelf corner Hz, total depth dB)`; interpolated between.
const TONE_SHELF: [(f64, f64, f64); 6] = [
    (0.0, 1026.0, -46.0),
    (32.0, 2189.0, -18.5),
    (64.0, 3105.0, -9.5),
    (96.0, 3105.0, -3.5),
    (110.0, 3919.0, -2.0),
    (127.0, 3919.0, 0.0),
];

/// One first-order high shelf (bilinear, pre-warped at the pole).
#[derive(Clone, Copy, Default)]
struct Shelf1 {
    b0: f64,
    b1: f64,
    a1: f64,
    x1: f64,
    y1: f64,
}

impl Shelf1 {
    /// `fp`: the shelf's corner (pole); `depth_db`: its gain at the top.
    fn set(&mut self, fp: f64, depth_db: f64, sample_rate: f64) {
        let wp = core::f64::consts::TAU * fp.clamp(20.0, sample_rate * 0.45);
        let wz = wp * 10f64.powf(-depth_db / 20.0);
        let k = wp / (wp / (2.0 * sample_rate)).tan();
        let a0 = 1.0 + k / wp;
        self.b0 = (1.0 + k / wz) / a0;
        self.b1 = (1.0 - k / wz) / a0;
        self.a1 = (1.0 - k / wp) / a0;
    }

    fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }

    #[inline]
    fn tick(&mut self, x: f64) -> f64 {
        let y = self.b0.mul_add(x, self.b1.mul_add(self.x1, -self.a1 * self.y1));
        self.x1 = x;
        self.y1 = y;
        y
    }
}

/// BigSky's fixed top end: every pulse of the plug-in's response trails
/// off by ~0.245 a sample (−0.132, −0.033, −0.008 …) even at Tone 127 —
/// a one-pole low-pass at ~10.75 kHz, under the Tone shelves.
const TOP_END_HZ: f64 = 10_750.0;

/// Cloud's Tone: BigSky's, on the wet. The product's `tone` (−1…+1) spans
/// BigSky's knob with 0 at noon, which the manual calls balanced.
struct ToneStage {
    /// Two shelves per side, cascaded.
    shelves: [[Shelf1; 2]; 2],
    on: bool,
    /// The fixed top end, per side.
    top: [Lp1; 2],
}

impl ToneStage {
    fn new() -> Self {
        Self { shelves: [[Shelf1::default(); 2]; 2], on: false, top: [Lp1::new(), Lp1::new()] }
    }

    fn set(&mut self, tone: f64, sample_rate: f64) {
        // A one-pole's pole for the corner: e^(−2π·fc/fs).
        let pole = (-core::f64::consts::TAU * TOP_END_HZ / sample_rate).exp();
        for f in &mut self.top {
            f.set_coeff(pole);
        }
        let knob = (tone.clamp(-1.0, 1.0) + 1.0) * 63.5;
        let (mut fc, mut depth) = (3919.0, 0.0);
        for w in TONE_SHELF.windows(2) {
            let ((k0, f0, d0), (k1, f1, d1)) = (w[0], w[1]);
            if knob <= k1 {
                let t = ((knob - k0) / (k1 - k0)).clamp(0.0, 1.0);
                fc = f0 * (f1 / f0).powf(t);
                depth = (d1 - d0).mul_add(t, d0);
                break;
            }
        }
        self.on = depth < -0.05;
        for side in &mut self.shelves {
            for shelf in side {
                shelf.set(fc, depth / 2.0, sample_rate);
            }
        }
    }

    fn clear(&mut self) {
        for side in &mut self.shelves {
            for shelf in side {
                shelf.reset();
            }
        }
        for f in &mut self.top {
            f.reset();
        }
    }

    #[inline]
    fn tick(&mut self, l: f64, r: f64) -> (f64, f64) {
        let [tl, tr] = &mut self.top;
        let (l, r) = (tl.tick(l), tr.tick(r));
        if !self.on {
            return (l, r);
        }
        let [sl, sr] = &mut self.shelves;
        let l = sl.iter_mut().fold(l, |v, s| s.tick(v));
        let r = sr.iter_mut().fold(r, |v, s| s.tick(v));
        (l, r)
    }
}

/// Vocoder-driven additive ensemble (`BigSky` MX Cloud "Ensemble",
/// from Cloudburst).
///
/// Strymon describes Cloudburst's ensemble as continuously analyzing
/// dozens of frequency bands and generating upper harmonic partials of
/// whatever it finds — "more akin to polyphonic additive synthesis".
/// That is a channel vocoder driving oscillators, NOT a pitch tracker:
/// per-band bandpass → slow envelope follower → sine partials at 2×
/// and 3× the band center, amplitude riding the band envelope with an
/// HF rolloff. Inherently polyphonic, no tracking to glitch.
struct Ensemble {
    bands: [EnsembleBand; ENSEMBLE_BANDS],
    attack: f64,
    release: f64,
    lp: crate::primitives::one_pole::Lp1,
    sample_rate: f64,
}

/// One analysis band and the two partials it drives.
///
/// Six parallel `[_; ENSEMBLE_BANDS]` arrays before, walked by a shared index
/// through the whole of `tick`.
struct EnsembleBand {
    filter: Biquad,
    /// Swell envelope following this band's level.
    env: f64,
    /// Band centre frequency, Hz.
    center: f64,
    /// Running phase of the 2x and 3x partials, and of the detune wobble.
    phase2: f64,
    phase3: f64,
    lfo: f64,
}

/// Analysis bands: ~third-octave log spacing, 80 Hz – 6 kHz.
const ENSEMBLE_BANDS: usize = 24;

impl Ensemble {
    fn new(sample_rate: f64) -> Self {
        let mut lp = crate::primitives::one_pole::Lp1::new();
        lp.set_freq(3200.0, sample_rate);
        let mut e = Self {
            bands: core::array::from_fn(|i| EnsembleBand {
                filter: Biquad::new(),
                env: 0.0,
                center: 0.0,
                phase2: num::count_to_f64(i) * 0.041,
                phase3: num::count_to_f64(i) * 0.067,
                lfo: num::count_to_f64(i) / num::count_to_f64(ENSEMBLE_BANDS),
            }),
            attack: 0.001,
            release: 0.0005,
            lp,
            sample_rate,
        };
        e.configure(sample_rate);
        e
    }

    fn configure(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let ratio = (6000.0f64 / 80.0).powf(1.0 / (num::count_to_f64(ENSEMBLE_BANDS) - 1.0));
        let mut f = 80.0;
        for band in &mut self.bands {
            band.center = f;
            band.filter.set(FilterType::Bandpass, f, 5.3, sample_rate);
            f *= ratio;
        }
        // Slow swell per band: ~180 ms attack, ~450 ms release — the
        // layer blooms behind the note instead of doubling its attack.
        self.attack = 1.0 - (-1.0 / (0.18 * sample_rate)).exp();
        self.release = 1.0 - (-1.0 / (0.45 * sample_rate)).exp();
        self.lp.set_freq(3200.0, sample_rate);
    }

    fn reset(&mut self) {
        for band in &mut self.bands {
            band.env = 0.0;
            band.filter.reset();
        }
        self.lp.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.configure(sample_rate);
        self.reset();
    }

    /// Analyze `input` (mono) and return the ensemble layer sample.
    #[inline]
    fn tick(&mut self, input: f64) -> f64 {
        let mut sum = 0.0;
        // Captured before the loop: `bands` is borrowed mutably below, so the
        // scalars it reads cannot come off `self` inside it.
        let first_center = self.bands.first().map_or(1.0, |b| b.center);
        let (attack, release, rate) = (self.attack, self.release, self.sample_rate);
        for (i, state) in self.bands.iter_mut().enumerate() {
            let band = state.filter.tick(input, 0);
            let level = band.abs();
            let coeff = if level > state.env { attack } else { release };
            state.env += (level - state.env) * coeff;
            let env = state.env;
            if env < 1.0e-5 {
                continue; // silent band: skip the oscillators entirely
            }

            // Slow per-band detune wobble (string-machine shimmer).
            let lfo_rate = 0.12 + num::count_to_f64(i % 5) * 0.06;
            state.lfo += lfo_rate / rate;
            if state.lfo >= 1.0 {
                state.lfo -= 1.0;
            }
            let wobble = (state.lfo * std::f64::consts::TAU).sin();
            let detune = (wobble * 5.0 / 1200.0).exp2();

            // Upper partials at 2× and 3× the band center; HF rolloff
            // keeps the top registers airy instead of piercing.
            let roll = (first_center / state.center).sqrt();
            let f2 = (2.0 * state.center * detune).min(rate * 0.45);
            let f3 = (3.0 * state.center * detune).min(rate * 0.45);
            state.phase2 = (state.phase2 + f2 / rate).fract();
            state.phase3 = (state.phase3 + f3 / rate).fract();
            sum += (state.phase2 * std::f64::consts::TAU).sin() * env * roll;
            sum += (state.phase3 * std::f64::consts::TAU).sin() * env * roll * 0.5;
        }

        self.lp.tick(sum * 0.7)
    }
}

/// Cloud reverb: mono input diffusion into one tapped ring tank.
pub struct Cloud {
    input_l: InputStage,
    input_r: InputStage,
    ring: CloudRing,
    tone: ToneStage,
    /// Input crossfeed (Extra B), 0 = none … 0.4 = most.
    crossfeed: f64,
    ensemble: Ensemble,
    ensemble_level: f64,
    sample_rate: f64,
}

impl Cloud {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let mut cloud = Self {
            input_l: InputStage::new(sample_rate),
            input_r: InputStage::new(sample_rate),
            ring: CloudRing::new(sample_rate),
            tone: ToneStage::new(),
            crossfeed: 0.2,
            ensemble: Ensemble::new(sample_rate),
            ensemble_level: 0.0,
            sample_rate,
        };
        cloud.set_params(&AlgorithmParams::default());
        cloud
    }

    fn for_inputs(&mut self, mut f: impl FnMut(&mut InputStage)) {
        f(&mut self.input_l);
        f(&mut self.input_r);
    }
}

impl ReverbAlgorithm for Cloud {
    fn reset(&mut self) {
        self.input_l.clear();
        self.input_r.clear();
        self.ring.clear();
        self.tone.clear();
        self.ensemble.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.input_l.set_sample_rate(sample_rate);
        self.input_r.set_sample_rate(sample_rate);
        // The ring's buffers are sized for the rate it was built at.
        self.ring = CloudRing::new(sample_rate);
        self.ensemble.set_sample_rate(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        let sr = self.sample_rate;
        let ms = |v: f64| v * 1e-3 * sr;

        // Decay is a time (CLOUD_T60); the ring turns it into one loop
        // gain per trip, and Freeze's infinite holds the ring.
        self.ring.set_t60(decay_to_t60(params.decay, CLOUD_T60.0, CLOUD_T60.1));
        self.ring.set_size(params.size);

        // Diffusion: the gain of every allpass, input chain and tank (zero =
        // pure delay, as on BigSky).
        let d = params.diffusion.clamp(0.0, 1.0);
        // BigSky's input gain: 0.85 × the knob, exactly linear — measured
        // at seven settings from the ratio of the chain's two first pulses
        // (all stages instant: g⁴; stage 1 delayed: −(1−g²)·g³).
        let input_g = d * 0.85;
        let size = params.size;
        self.for_inputs(|s| {
            s.diffuser.set_size(size);
            s.diffuser.set_gain(input_g);
        });
        // The tank's stages run at the same 0.85 × the knob — fitted from
        // −8 to +10 against the plug-in's own output.
        self.ring.set_diffusion(input_g);

        // Modulation, the manual's two segments, measured: every Diffusion
        // allpass (input chain and stages; not the figure-8) shortens its
        // delay by up to D on an LFO. D rises to ~0.385 ms by MOD 64; past
        // ~72 the rate climbs from ~0.9 Hz to ~7.5 Hz as D eases back. Read
        // off the plug-in's early pulse (the chain alone) and its first
        // stage's own tap, tracked through a train of impulses.
        let m = params.modulation.clamp(0.0, 1.0) * 127.0;
        let depth_ms = interp(&MOD_DEPTH_MS, m);
        let rate_hz = interp(&MOD_RATE_HZ.map(|(k, r)| (k, r.ln())), m).exp();
        let amount = ms(depth_ms);
        self.for_inputs(|s| s.diffuser.set_modulation(amount, rate_hz));
        // The stages' depth is not as cleanly readable (their taps collide
        // with older pulses); 1.5× the chain's lands BigSky's spread round
        // a held 1 kHz tone (±20 % from MOD 32 to 127, against −25…−35 % at
        // 1×).
        self.ring.set_modulation(amount * 1.5, rate_hz);

        // Damping: off at zero (BigSky's Cloud decays evenly across the
        // band), otherwise a low-pass on the ring's recirculation.
        self.ring.set_damping(
            (params.damping > 0.05).then(|| 20_000.0 * 10f64.powf(-params.damping * 1.3)),
        );

        // Low End — BigSky's static one-pole high-pass, ≈ 300 − 23·LowEnd
        // Hz. The chain's `low_end` reaches us as `low_decay_mult`; undo
        // that law to get the knob back.
        let le = params.low_decay_mult;
        let knob01 = if le < 1.0 { le - 0.5 } else { (le - 1.0) / 1.2 + 0.5 };
        let low_end = knob01.clamp(0.0, 1.0).mul_add(20.0, -10.0);
        let low_cut_hz = low_end.mul_add(-23.3, 300.0).clamp(20.0, 1000.0);
        self.for_inputs(|s| s.low_cut.set_cutoff(low_cut_hz));

        self.tone.set(params.tone, sr);

        // Extra B: input crossfeed between the sides.
        self.crossfeed = params.extra_b.clamp(0.0, 1.0) * 0.4;
    }

    fn set_cloud_params(&mut self, params: &CloudParams) -> bool {
        self.ensemble_level = params.ensemble.clamp(0.0, 1.0);
        true
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        // Ensemble layer rides the reverb input (Diffusion untouched).
        let (left, right) = if self.ensemble_level > 1e-9 {
            let ens = self.ensemble.tick((left + right) * 0.5) * self.ensemble_level;
            (left + ens, right + ens)
        } else {
            (left, right)
        };
        let c = self.crossfeed;
        let in_l = left.mul_add(1.0 - c, right * c);
        let in_r = right.mul_add(1.0 - c, left * c);

        let early_l = self.input_l.tick(in_l);
        let early_r = self.input_r.tick(in_r);
        let (ring_l, ring_r) = self.ring.tick(early_l, early_r);
        self.tone.tick(EARLY_OUT.mul_add(early_l, ring_l), EARLY_OUT.mul_add(early_r, ring_r))
    }
}
