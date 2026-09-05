//! Spectral dynamics engine — per-bin resonance suppression /
//! isolation (ANINA / soothe class, and the per-band "Pro-Q Spectral"
//! behavior when driven with a band mask).
//!
//! STFT (Hann analysis + synthesis, 4× overlap) → per-bin level in dB
//! → compare against a **spectrally smoothed reference** of the same
//! spectrum (relative mode: a bin only triggers when it sticks out of
//! its own spectral neighborhood — that's what makes it a resonance
//! suppressor rather than an EQ) or an absolute threshold → per-bin
//! gain reduction with attack/release smoothing in time → gains applied
//! to both channels' spectra → overlap-add resynthesis.
//!
//! Clean-room design from published behavior only (see
//! `spec/eq-suite-plan.md`): Density sharpens per-bin selectivity,
//! Tilt applies +3 dB/oct to the trigger spectrum (pink-noise
//! normalization, Pro-Q 4's published default), Freeze locks the gain
//! curve, Gate skips near-silent bins, Delta morphs the output from
//! suppression to isolation (listen to what's being removed).

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpectralParams {
    /// Reduction depth (0..1): scales the per-bin gain reduction.
    pub amount: f64,
    /// Selectivity (0..1): sharpens the over-threshold mask; low =
    /// broad gentle ranges, high = narrow surgical notches.
    pub density: f64,
    /// +3 dB/oct trigger tilt (highs trigger slightly more).
    pub tilt: bool,
    /// Per-bin attack/release in ms.
    pub attack_ms: f64,
    pub release_ms: f64,
    /// Work range: bins outside are untouched.
    pub lo_hz: f64,
    pub hi_hz: f64,
    /// Bins below this absolute level never trigger (keeps the noise
    /// floor from being "suppressed").
    pub gate_db: f64,
    /// Relative mode (default): trigger on bin − smoothed-neighborhood.
    /// Absolute mode: trigger on bin level vs `threshold_db`.
    pub relative: bool,
    /// Threshold: dB **over the smoothed reference** in relative mode
    /// (0 = any prominence triggers; 6 = only strong resonances);
    /// absolute bin dB threshold otherwise.
    pub threshold_db: f64,
    /// Lock the current gain curve (ANINA Freeze).
    pub freeze: bool,
    /// Output morph: 0 = suppression, 1 = isolation (the removed part).
    pub delta: f64,
}

impl Default for SpectralParams {
    fn default() -> Self {
        Self {
            amount: 0.5,
            density: 0.5,
            tilt: true,
            attack_ms: 3.0,
            release_ms: 80.0,
            lo_hz: 80.0,
            hi_hz: 16000.0,
            gate_db: -70.0,
            relative: true,
            threshold_db: 4.0,
            freeze: false,
            delta: 0.0,
        }
    }
}

/// How far a bin's reduction reaches into its neighbours, **in octaves**, at
/// the two ends of the Density knob.
///
/// Measured with an 18 dB spectral band on a resonance planted in noise and
/// left eight seconds to settle, read as a fraction of the band's full depth:
///
/// ```text
///   offset      -0.166   -0.089   +0.084   +0.149   +0.166   +0.25  octaves
///   density 0     0.65     0.76     0.90     0.83     0.80    0.00
///   density 25    0.21     0.78     0.90     0.84     0.34    0.00
///   density 50    0.00     0.81     0.91     0.06     0.00    0.00
/// ```
///
/// **In octaves, not hertz.** Read in hertz the same measurements look wildly
/// asymmetric — at density 25 a bin 109 Hz above the resonance comes down
/// 15.07 dB and one 109 Hz below it 3.81 — and no symmetric width can fit
/// that. The asymmetry is entirely the log scale: -109 Hz is 0.166 octaves
/// out and +109 Hz only 0.149, and at 0.166 either side the two readings are
/// 3.81 and 6.15. A constant-Q neighbourhood, which is also what an analysis
/// with a fixed bin count gives you when you think in musical intervals.
///
/// The reach shrinks with Density — 0.24 octaves at 0, 0.20 at 25, 0.17 at 50
/// — and inside it the profile is graded, not flat. The old spread was a
/// max-hold in hertz: flat to the edge of its window and then a cliff, which
/// put the error exactly at the edges (5.6 dB at +160 Hz, where ours had
/// recovered and the plugin had not).
///
/// These are wider than they were before the reduction took the band's own
/// curve. With a rectangular region the spread had to supply all of the
/// narrowing; now the curve supplies most of it and the spread only adds what
/// Density asks for on top, so fitting them together moved the reach out and
/// sharpened the taper:
///
/// ```text
///   floor  range  exp   density 0    25    50
///    0.09   0.15  6.0       2.86    4.74  3.74   <- chosen
///    0.09   0.15  3.0       6.06    6.81  2.75
///    0.12   0.13  6.0       2.57    6.40  8.32
///    0.08   0.17  8.0       2.66    6.60  4.56
/// ```
const SPREAD_FLOOR_OCT: f64 = 0.09;
const SPREAD_RANGE_OCT: f64 = 0.15;

/// The shape of the grade, as a fraction of full depth against a fraction of
/// the reach.
///
/// `1 - x^6` — flat for most of the reach and then falling away steeply, which
/// is what the plugin traces: at Density 25 a bin 0.149 octaves out is at full
/// depth and one 0.166 octaves out is at 0.42 of it. A gentler `1 - x^3` gives
/// up depth far too early and measured 6.1 dB shallow at Density 0.
const SPREAD_TAPER_EXP: i32 = 6;

#[inline]
fn spread_taper(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    1.0 - x.powi(SPREAD_TAPER_EXP)
}

/// Spectral Tilt: how much a bin's trigger is weighted per octave, and about
/// which frequency.
///
/// Twenty spectral bands across ten factory presets switch Tilt on, and it is
/// not a small correction: on "Djent Bass Punch and Cut" it is the difference
/// between the plugin applying nothing and applying its full 11 dB range.
/// Measured as the change Tilt makes to the applied reduction, on a wide bell
/// at 1 kHz with a -24 dB range:
///
/// ```text
///     Hz    125    250    500   1000   2000   4000   8000
///  Pro-Q  +1.76  +2.48  +2.38  +1.86  -0.38  -0.55  -0.53
/// ```
///
/// Less reduction low, slightly more high, crossing over at about 1.4 kHz —
/// not at 1 kHz, which is where the pivot used to sit and which left the low
/// end a decibel short.
///
/// It does not explain everything. On a **high shelf** at Density 100 the
/// plugin applies essentially nothing with Tilt on (-0.76 dB of an 11 dB
/// range at 3.5 kHz) where the same band with Tilt off applies -6.21; no
/// weighting that adds reduction above the pivot can do that, and it is the
/// opposite sign to the bell above. That case is still 10 dB out and is what
/// keeps "Djent Bass Punch and Cut" in the tail.
const SPECTRAL_TILT_DB_PER_OCT: f64 = 3.0;
const SPECTRAL_TILT_PIVOT_HZ: f64 = 1400.0;

/// Spectral-neighborhood smoothing width in octaves (each side) for the
/// relative reference — roughly a third-octave view of "expected" level.
const SMOOTH_OCTAVES: f64 = 0.33;

/// The shape a spectral band's reduction takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpectralShape {
    Bell,
    LowShelf,
    HighShelf,
}

/// How far under the region's learned level its Auto threshold sits, as a
/// deduction from the band's range, in dB.
///
/// A spectral band on Auto settles a **fixed** number of decibels short of its
/// target, not a fraction of it. Measured on flat noise at Density 0, reading
/// the depth at the band's own frequency:
///
/// ```text
///   range   -6.0   -12.0   -24.0
///   depth   -3.18   -8.85  -20.96
///   short    2.82    3.15    3.04
/// ```
///
/// Three decibels short, three times over. The constant is not 3 because the
/// threshold is compared against a level estimate that still sits above the
/// bin's long-term average — see [`SPECTRAL_LEVEL_MS`] — and the spreading
/// pass then takes the loudest of a neighbourhood. Fitted against the table
/// above, the difference is worth about five decibels: 7.7 puts all three
/// within 0.15 dB.
///
/// (A -36 dB range comes up 9 dB short rather than 3. Nothing in the factory
/// library asks for one, and that looks like a floor rather than a threshold,
/// so it is not modelled.)
pub const SPECTRAL_HEADROOM_DB: f64 = 7.7;

/// Extra headroom for a shelf, in dB.
///
/// A shelf's curve is flat over a wide stretch, so the spreading pass — which
/// takes the loudest target in a neighbourhood a fixed fraction of an octave
/// wide — has far more bins to choose from than it does under a bell's peak,
/// and more of them the higher the shelf sits. Measured, our shelf came out
/// about a decibel and a half deeper than our own bell at the same range.
/// Smoothing the analysis in time (see [`SPECTRAL_LEVEL_MS`]) halved that; the
/// rest is bought here.
pub const SPECTRAL_SHELF_HEADROOM_DB: f64 = 1.4;

/// How long a spectral band's Auto threshold takes to learn, in seconds.
pub const SPECTRAL_SETTLE_S: f64 = 3.0;

/// Time constant of the per-bin level the threshold is compared against, in
/// milliseconds.
///
/// The trigger cannot read a bin's *instantaneous* magnitude. On noise those
/// fluctuate several decibels frame to frame, and the spreading pass then
/// takes the loudest of a neighbourhood — so the reduction comes out deeper
/// than the threshold asks for, and **worse at high frequencies**, where a
/// neighbourhood a fixed fraction of an octave wide covers far more bins. A
/// high shelf measured 2.4 dB deeper than a bell of the same range for that
/// reason alone.
///
/// Smoothing the analysis first is what a level detector does anyway, and it
/// makes the headroom constant mean nearly the same thing at 300 Hz and at
/// 15 kHz. 50 ms left a shelf a decibel deeper than a bell of the same range;
/// 100 halves that, and going further trades it for depth that has to be
/// bought back through the headroom anyway.
pub const SPECTRAL_LEVEL_MS: f64 = 100.0;

/// How the reduction's width relates to the band's drawn Q.
///
/// The reduction is **narrower than the band's own curve**. Measured on flat
/// noise at Density 0 with Auto, at three Qs, and fitted as an analog peaking
/// magnitude, the shape that comes out has twice the drawn Q:
///
/// ```text
///   f/f0     0.5    0.71    0.84    1.0    1.19    1.41    2.0
///   Q 0.341  0.48   0.70    0.91    1.00   0.90    0.76    0.52   (of full depth)
///   Q 1.0    0.19   0.36    0.64    1.00   0.64    0.40    0.21
///   Q 3.0     —     0.11    0.33    1.00   0.33    0.15    0.02
/// ```
///
/// Reading the drawn Q straight into the shape puts the Q 0.341 case at 0.88
/// of full depth half an octave out where the plugin is at 0.48.
const SPECTRAL_Q_SCALE: f64 = 2.0;

/// How steeply a spectral shelf's reduction transitions, in slope per unit Q.
const SPECTRAL_SHELF_SLOPE: f64 = 6.4;
/// How far above the band's frequency the transition's half-way point sits,
/// as `1 + SPECTRAL_SHELF_CORNER / (q + 0.25)` times it.
const SPECTRAL_SHELF_CORNER: f64 = 0.53;

/// One band-shaped spectral region (Pro-Q "Spectral" band).
///
/// A spectral band is **the band's own curve with a per-bin gain**, not a
/// rectangular window over a frequency range. That distinction is the whole
/// behaviour: fed flat noise, a spectral band on Auto pulls its entire curve
/// down — a bell-shaped 20 dB cut with a -24 dB range — where a prominence
/// detector finds nothing to react to and does nothing at all. That was worth
/// 5.7 dB on "Kick - Bad PZM Rescue" alone, and 42 of the 171 factory presets
/// carry a spectral band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpectralRegion {
    /// The band's frequency and Q — the reduction takes this shape.
    pub freq_hz: f64,
    pub q: f64,
    pub shape: SpectralShape,
    /// The band's dynamic range in dB; the ceiling on the reduction.
    pub max_depth_db: f64,
    /// Absolute per-bin threshold in dBFS. Ignored while `auto` is set.
    pub threshold_db: f64,
    /// Learn the threshold from the region's own programme level.
    pub auto: bool,
    /// Per-bin selectivity 0..1 for **this** region.
    ///
    /// Pro-Q sets Density per band, not per instance, and the factory library
    /// uses 25 distinct values across its 74 spectral bands — so a single
    /// engine-wide figure cannot represent a preset that pairs a broad
    /// de-esser with a surgical notch, which is the normal case.
    pub density: f64,
    /// Pink-noise tilt on this region's trigger, so highs are judged against
    /// a -3 dB/oct expectation rather than a flat one.
    pub tilt: bool,
}

/// The reduction's shape, normalised to 1 at the band's own frequency.
///
/// An analog peaking magnitude at [`SPECTRAL_Q_SCALE`] times the drawn Q, with
/// the depth setting how far the skirts reach — a deep cut is relatively
/// narrower than a shallow one, which is what the plugin measures like.
fn region_envelope(r: &SpectralRegion, hz: f64) -> f64 {
    if hz <= 0.0 || r.freq_hz <= 0.0 {
        return 0.0;
    }
    let ratio = hz / r.freq_hz;
    match r.shape {
        SpectralShape::Bell => {
            let depth = r.max_depth_db.abs().max(0.1);
            let a = 10.0f64.powf(-depth / 40.0);
            let qs = (r.q * SPECTRAL_Q_SCALE).max(0.02);
            let u = 1.0 / ratio - ratio;
            let num = a * u + (a / qs) * (a / qs);
            let den = a * u + 1.0 / (a * qs) / (a * qs);
            let db = 10.0 * (num / den).max(1.0e-30).log10();
            (db / -depth).clamp(0.0, 1.0)
        }
        // A spectral shelf's transition is far steeper than the shelf the
        // band draws, and its half-way point sits ABOVE the band's frequency,
        // by a margin that closes as Q rises. Measured at three Qs with a
        // -10 dB range on flat noise, as a fraction of full depth:
        //
        // ```text
        //   f/f0      0.84   1.00   1.19   1.41   2.00   2.83   3.36
        //   Q 0.2     0.06   0.26   0.37   0.42   0.66   0.92   1.00
        //   Q 0.689   0.02   0.12   0.29   0.43   0.76   0.99   1.00
        //   Q 2.0     0.00   0.05   0.41   0.73   0.99   1.00   1.00
        // ```
        //
        // The band's own shelf would read 0.5 at f0 at every Q. Fitted, the
        // corner sits at `1 + 0.53/(q + 0.25)` times the band's frequency and
        // the transition falls at `6.4 * q` poles' worth of slope.
        SpectralShape::HighShelf | SpectralShape::LowShelf => {
            let q = r.q.max(0.02);
            let slope = (SPECTRAL_SHELF_SLOPE * q).clamp(0.5, 40.0);
            let corner = 1.0 + SPECTRAL_SHELF_CORNER / (q + 0.25);
            let x = if matches!(r.shape, SpectralShape::HighShelf) {
                corner / ratio
            } else {
                ratio * corner
            };
            1.0 / (1.0 + x.powf(slope))
        }
    }
}

pub struct SpectralEngine {
    pub params: SpectralParams,
    /// When non-empty, band regions REPLACE the global lo/hi/amount/
    /// threshold: a bin is processed by the strongest matching region.
    regions: Vec<SpectralRegion>,
    /// Each region's normalised curve, one value per bin. Rebuilt whenever the
    /// regions change; a peaking magnitude per bin per frame would be far too
    /// much arithmetic for the audio thread.
    region_env: Vec<Vec<f64>>,
    /// The long-term level of each BIN, in dBFS — what an Auto threshold is
    /// measured against.
    ///
    /// Per bin, not per region. A region-wide average is what a band-limited
    /// detector would give, and it fails the moment another band reshapes the
    /// spectrum underneath it: on "Kick - Bad PZM Rescue" a +27 dB expanding
    /// bell at 5 kHz sits inside a spectral band's skirt at 1273 Hz, dragged
    /// the region's average up by 20 dB, and switched the spectral reduction
    /// off entirely — the plugin still applied 11.8 dB there.
    learned_db: Vec<f64>,
    learned_coeff: f64,
    /// Which region owns each bin, so the envelope can be applied after the
    /// spreading pass.
    bin_owner: Vec<usize>,
    /// Each region's curve-weighted mean reduction, in dB — what the Auto Gain
    /// compensation needs to know about a band it cannot see in the static
    /// chain.
    region_reduction_db: Vec<f64>,
    fft: Arc<dyn RealToComplex<f64>>,
    ifft: Arc<dyn ComplexToReal<f64>>,
    block: usize,
    hop: usize,
    window: Vec<f64>,
    /// Per-bin reduction before it is spread across frequency, and the width
    /// each bin's own region asks for.
    target_db: Vec<f64>,
    /// Each bin's reach, in octaves, from the region it belongs to.
    spread_oct: Vec<f64>,
    /// log2 of each bin's centre frequency — the spreading pass measures
    /// distance in octaves, and a `log2` per bin per frame is not something
    /// to do on the audio thread.
    bin_log2: Vec<f64>,
    /// Reusable buffer for the spreading pass — no allocation on the audio
    /// thread.
    scratch_db: Vec<f64>,
    /// Input rings (per channel) + pending fill.
    in_buf: [Vec<f64>; 2],
    /// Overlap-add accumulators (per channel).
    ola: [Vec<f64>; 2],
    /// Ready output queue (per channel).
    out_buf: [Vec<f64>; 2],
    fill: usize,
    // Scratch (preallocated).
    frame: Vec<f64>,
    spec: [Vec<Complex<f64>>; 2],
    mag_db: Vec<f64>,
    /// `mag_db` smoothed in time — see [`SPECTRAL_LEVEL_MS`].
    level_db: Vec<f64>,
    level_coeff: f64,
    ref_db: Vec<f64>,
    gr_db: Vec<f64>,
    gain: Vec<f64>,
    attack_coeff: f64,
    release_coeff: f64,
    sample_rate: f64,
    primed: bool,
}

impl SpectralEngine {
    /// `block` must be a power of two (512 / 1024 / 2048).
    #[must_use]
    pub fn new(sample_rate: f64, block: usize) -> Self {
        let mut planner = RealFftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(block);
        let ifft = planner.plan_fft_inverse(block);
        let hop = block / 4;
        let window: Vec<f64> = (0..block)
            .map(|i| {
                let t = i as f64 / block as f64;
                0.5 - 0.5 * (core::f64::consts::TAU * t).cos()
            })
            .collect();
        let bins = block / 2 + 1;
        let mut e = Self {
            params: SpectralParams::default(),
            regions: Vec::with_capacity(24),
            region_env: Vec::new(),
            learned_db: vec![-120.0; bins],
            learned_coeff: 1.0,
            bin_owner: vec![usize::MAX; bins],
            region_reduction_db: Vec::new(),
            fft,
            ifft,
            block,
            hop,
            window,
            in_buf: [vec![0.0; block], vec![0.0; block]],
            ola: [vec![0.0; block], vec![0.0; block]],
            out_buf: [Vec::with_capacity(block * 2), Vec::with_capacity(block * 2)],
            fill: 0,
            frame: vec![0.0; block],
            spec: [
                vec![Complex::new(0.0, 0.0); bins],
                vec![Complex::new(0.0, 0.0); bins],
            ],
            mag_db: vec![-120.0; bins],
            level_db: vec![-120.0; bins],
            level_coeff: 1.0,
            ref_db: vec![-120.0; bins],
            gr_db: vec![0.0; bins],
            target_db: vec![0.0; bins],
            spread_oct: vec![0.0; bins],
            bin_log2: vec![0.0; bins],
            scratch_db: vec![0.0; bins],
            gain: vec![1.0; bins],
            attack_coeff: 1.0,
            release_coeff: 1.0,
            sample_rate,
            primed: false,
        };
        e.update(sample_rate);
        e
    }

    /// Latency in samples (one analysis block minus the sample that
    /// completes it — verified by impulse: a spike at t returns at
    /// t + block − 1).
    #[must_use]
    pub fn latency(&self) -> usize {
        self.block - 1
    }

    /// Replace the band-region set (preallocated capacity 24 — no
    /// audio-thread allocation up to 24 regions).
    pub fn set_regions(&mut self, regions: &[SpectralRegion]) {
        let changed = self.regions.len() != regions.len()
            || self.regions.iter().zip(regions).any(|(a, b)| a != b);
        self.regions.clear();
        for r in regions.iter().take(self.regions.capacity()) {
            self.regions.push(*r);
        }
        if !changed {
            return;
        }
        // Rebuild the per-region curves. Not the audio thread: this runs when
        // a parameter moves.
        let bins = self.mag_db.len();
        let bin_hz = self.sample_rate / self.block as f64;
        self.region_env.clear();
        for r in &self.regions {
            let mut env = Vec::with_capacity(bins);
            for i in 0..bins {
                env.push(region_envelope(r, i as f64 * bin_hz));
            }
            self.region_env.push(env);
        }
        self.region_reduction_db.clear();
        self.region_reduction_db.resize(self.regions.len(), 0.0);
    }

    /// Widen each bin's reduction into its neighbourhood.
    ///
    /// Two max-holds, forward and back, each running for as many bins as that
    /// bin's own width asks for. The result is flat across the neighbourhood
    /// and untouched outside it, which is what the plugin measures like — a
    /// resonance takes its immediate surroundings with it at low Density and
    /// nothing but itself at high Density.
    ///
    /// The width is read from the bin the reduction *came from*, so two
    /// regions with different densities in one instance each spread by their
    /// own amount.
    fn spread_targets(&mut self, bins: usize) {
        if bins == 0 {
            return;
        }

        let mut spread = core::mem::take(&mut self.scratch_db);
        spread.clear();
        spread.resize(bins, 0.0);

        // Forward, then back, taking whichever side reaches further into each
        // bin. Each pass carries the last dominating reduction outward,
        // tapering it over that bin's own reach — so two regions with
        // different densities in one instance each spread by their own amount
        // and with their own profile.
        let (mut peak, mut reach, mut from) = (0.0f64, 1.0f64, f64::NEG_INFINITY);
        for i in 0..bins {
            let t = self.target_db.get(i).copied().unwrap_or(0.0);
            let dist = self.bin_log2[i] - from;
            let carried = if dist < reach { peak * spread_taper(dist / reach) } else { 0.0 };
            if t >= carried {
                peak = t;
                reach = self.spread_oct[i].max(1.0e-6);
                from = self.bin_log2[i];
                spread[i] = t;
            } else {
                spread[i] = carried;
            }
        }

        let (mut peak, mut reach, mut from) = (0.0f64, 1.0f64, f64::INFINITY);
        for i in (0..bins).rev() {
            let t = self.target_db[i];
            let dist = from - self.bin_log2[i];
            let carried = if dist < reach { peak * spread_taper(dist / reach) } else { 0.0 };
            if t >= carried {
                peak = t;
                reach = self.spread_oct[i].max(1.0e-6);
                from = self.bin_log2[i];
            } else if carried > spread[i] {
                spread[i] = carried;
            }
        }

        self.target_db[..bins].copy_from_slice(&spread[..bins]);
        self.scratch_db = spread;
    }

    /// The mean reduction each region is applying right now, in dB (positive).
    #[must_use]
    pub fn region_reduction_db(&self, region: usize) -> f64 {
        self.region_reduction_db.get(region).copied().unwrap_or(0.0)
    }

    #[must_use]
    pub fn has_regions(&self) -> bool {
        !self.regions.is_empty()
    }

    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let bin_hz = sample_rate / self.block as f64;
        for (i, slot) in self.bin_log2.iter_mut().enumerate() {
            *slot = (i as f64 * bin_hz).max(1.0).log2();
        }
        // Frame-rate ballistics: coefficients per HOP, not per sample.
        let hop_s = self.hop as f64 / sample_rate;
        let c = |ms: f64| -> f64 {
            if ms <= 0.0 {
                1.0
            } else {
                1.0 - (-hop_s / (ms * 0.001)).exp()
            }
        };
        self.level_coeff = c(SPECTRAL_LEVEL_MS);
        self.learned_coeff = c(SPECTRAL_SETTLE_S * 1000.0);
        self.attack_coeff = c(self.params.attack_ms);
        self.release_coeff = c(self.params.release_ms);
    }

    /// Process one stereo sample; returns the (delayed) processed pair.
    #[inline]
    pub fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        let pos = self.fill;
        self.in_buf[0][pos] = left;
        self.in_buf[1][pos] = right;
        self.fill += 1;
        if self.fill == self.block {
            self.process_frame();
            // Slide the input ring left by one hop.
            for ch in 0..2 {
                self.in_buf[ch].copy_within(self.hop.., 0);
            }
            self.fill = self.block - self.hop;
        }
        let (l, r) = if self.out_buf[0].is_empty() {
            (0.0, 0.0)
        } else {
            (self.out_buf[0].remove(0), self.out_buf[1].remove(0))
        };
        (l, r)
    }

    fn process_frame(&mut self) {
        let bins = self.mag_db.len();
        // Forward FFT both channels (windowed).
        for ch in 0..2 {
            for i in 0..self.block {
                self.frame[i] = self.in_buf[ch][i] * self.window[i];
            }
            let _ = self.fft.process(&mut self.frame, &mut self.spec[ch]);
        }

        if !self.params.freeze {
            // Trigger spectrum: average channel magnitude in dB.
            //
            // The tilt is NOT folded in here any more. It used to be applied
            // to the analysis spectrum, which made it one setting for the
            // whole instance; it is now applied to each bin's prominence
            // below, where the region that owns the bin decides. Adding it in
            // both places would count it twice.
            let bin_hz = self.sample_rate / self.block as f64;
            for i in 0..bins {
                let m = 0.5 * (self.spec[0][i].norm() + self.spec[1][i].norm())
                    / (self.block as f64 * 0.25);
                self.mag_db[i] = 20.0 * m.max(1.0e-10).log10();
                // The trigger reads a smoothed level, not the instantaneous
                // bin — see `SPECTRAL_LEVEL_MS`.
                if self.level_db[i] <= -119.0 {
                    self.level_db[i] = self.mag_db[i];
                } else {
                    self.level_db[i] += (self.mag_db[i] - self.level_db[i]) * self.level_coeff;
                }
            }

            // Smoothed spectral reference: two-pass (up + down) one-pole
            // across bins with an octave-proportional coefficient —
            // cheap constant-Q-ish neighborhood average.
            let mut acc = self.mag_db[0];
            for i in 0..bins {
                let f = (i.max(1)) as f64 * bin_hz;
                let neighbors = f * (2.0f64.powf(SMOOTH_OCTAVES) - 1.0) / bin_hz;
                let c = 1.0 / (1.0 + neighbors.max(1.0));
                acc += (self.mag_db[i] - acc) * c;
                self.ref_db[i] = acc;
            }
            let mut acc = self.mag_db[bins - 1];
            for i in (0..bins).rev() {
                let f = (i.max(1)) as f64 * bin_hz;
                let neighbors = f * (2.0f64.powf(SMOOTH_OCTAVES) - 1.0) / bin_hz;
                let c = 1.0 / (1.0 + neighbors.max(1.0));
                acc += (self.mag_db[i] - acc) * c;
                self.ref_db[i] = 0.5 * (self.ref_db[i] + acc);
            }

            // Each bin's long-term level, which is what an Auto threshold
            // learns from.
            for i in 0..bins {
                if self.learned_db[i] <= -119.0 {
                    self.learned_db[i] = self.level_db[i];
                } else {
                    self.learned_db[i] +=
                        (self.level_db[i] - self.learned_db[i]) * self.learned_coeff;
                }
            }

            // Per-bin target gain reduction.
            let global_sharp = 1.0 + self.params.density.clamp(0.0, 1.0) * 3.0;
            for i in 0..bins {
                let f = i as f64 * bin_hz;
                // Region mode: the region whose curve reaches furthest into
                // this bin owns it. Global mode uses the engine params, where
                // `depth` is a 0..1 scale rather than a dB ceiling.
                let mut owner = usize::MAX;
                let (in_range, depth, thr, density_here, tilt) = if self.regions.is_empty() {
                    (
                        f >= self.params.lo_hz && f <= self.params.hi_hz,
                        self.params.amount,
                        self.params.threshold_db,
                        self.params.density,
                        self.params.tilt,
                    )
                } else {
                    let mut best = 0.0f64;
                    for (ri, r) in self.regions.iter().enumerate() {
                        let reach = self.region_env[ri][i] * r.max_depth_db;
                        if reach > best {
                            best = reach;
                            owner = ri;
                        }
                    }
                    match self.regions.get(owner) {
                        // The threshold is ABSOLUTE, not a prominence over a
                        // smoothed neighbourhood. A spectral band on Auto pulls
                        // its whole curve down on flat noise, which nothing
                        // driven by prominence can do — measured, a -24 dB
                        // range takes flat noise down 20.96 dB at the band's
                        // own frequency.
                        Some(r) if best > 1.0e-4 => (
                            true,
                            r.max_depth_db,
                            if r.auto {
                                let extra = if matches!(r.shape, SpectralShape::Bell) {
                                    0.0
                                } else {
                                    SPECTRAL_SHELF_HEADROOM_DB
                                };
                                self.learned_db[i]
                                    - (r.max_depth_db - SPECTRAL_HEADROOM_DB - extra)
                            } else {
                                r.threshold_db
                            },
                            r.density,
                            r.tilt,
                        ),
                        _ => {
                            owner = usize::MAX;
                            (false, 0.0, 0.0, 0.0, false)
                        }
                    }
                };
                self.bin_owner[i] = owner;
                let gated = self.level_db[i] < self.params.gate_db;
                let tilt_db = if tilt && f > 1.0 {
                    SPECTRAL_TILT_DB_PER_OCT * (f / SPECTRAL_TILT_PIVOT_HZ).log2()
                } else {
                    0.0
                };
                let over = if self.regions.is_empty() && self.params.relative {
                    self.mag_db[i] - self.ref_db[i] - thr + tilt_db
                } else {
                    self.level_db[i] - thr + tilt_db
                };
                let target = if in_range && !gated && over > 0.0 {
                    if self.regions.is_empty() {
                        (over * global_sharp * depth.clamp(0.0, 1.0)).min(24.0)
                    } else {
                        // One dB of reduction per dB over threshold, capped at
                        // the band's range — the same law the whole-band
                        // detector runs on.
                        over.min(depth)
                    }
                } else {
                    0.0
                };
                self.target_db[i] = target;
                let d = if self.regions.is_empty() {
                    self.params.density
                } else {
                    density_here
                };
                self.spread_oct[i] =
                    SPREAD_FLOOR_OCT + (1.0 - d.clamp(0.0, 1.0)) * SPREAD_RANGE_OCT;
            }

            self.spread_targets(bins);

            // The reduction takes the band's shape. Until here `target_db` is
            // how hard the bin triggered; the curve says how much of that the
            // band actually applies there.
            if !self.regions.is_empty() {
                for i in 0..bins {
                    let owner = self.bin_owner[i];
                    self.target_db[i] *= match self.region_env.get(owner) {
                        Some(env) => env[i],
                        None => 0.0,
                    };
                }
            }

            for i in 0..bins {
                let target = self.target_db[i];
                // Frame-rate attack/release per bin.
                let c = if target > self.gr_db[i] {
                    self.attack_coeff
                } else {
                    self.release_coeff
                };
                self.gr_db[i] += (target - self.gr_db[i]) * c;
                self.gain[i] = 10.0f64.powf(-self.gr_db[i] / 20.0);
            }

            // Each region's mean reduction, weighted by its own curve.
            for r in 0..self.regions.len() {
                let env = &self.region_env[r];
                let (mut num, mut den) = (0.0f64, 0.0f64);
                for i in 0..bins {
                    if env[i] > 1.0e-3 && self.bin_owner[i] == r {
                        num += env[i] * self.gr_db[i];
                        den += env[i];
                    }
                }
                self.region_reduction_db[r] = if den > 0.0 { num / den } else { 0.0 };
            }
        }

        // Apply gains; delta morphs suppressed ↔ removed.
        let delta = self.params.delta.clamp(0.0, 1.0);
        for ch in 0..2 {
            for i in 0..bins {
                let g_keep = self.gain[i];
                let g = g_keep * (1.0 - delta) + (1.0 - g_keep) * delta;
                self.spec[ch][i] *= g;
            }
            let mut spec = self.spec[ch].clone();
            let _ = self.ifft.process(&mut spec, &mut self.frame);
            // Overlap-add with synthesis window; Hann² at 75% overlap
            // sums to 1.5·block, folded into the normalization.
            let norm = 1.0 / (self.block as f64 * 1.5);
            for i in 0..self.block {
                self.ola[ch][i] += self.frame[i] * self.window[i] * norm;
            }
            // Emit one hop of finished samples.
            for i in 0..self.hop {
                self.out_buf[ch].push(self.ola[ch][i]);
            }
            self.ola[ch].copy_within(self.hop.., 0);
            for i in (self.block - self.hop)..self.block {
                self.ola[ch][i] = 0.0;
            }
        }
        self.primed = true;
    }

    pub fn reset(&mut self) {
        for ch in 0..2 {
            self.in_buf[ch].fill(0.0);
            self.ola[ch].fill(0.0);
            self.out_buf[ch].clear();
        }
        self.fill = 0;
        self.gr_db.fill(0.0);
        self.gain.fill(1.0);
        self.primed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    /// Deterministic noise.
    fn noise(seed: &mut u64) -> f64 {
        *seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((*seed >> 33) as f64 / (1u64 << 31) as f64) - 1.0
    }

    /// Band energy of a buffer via Goertzel-ish correlation.
    fn tone_energy(buf: &[f64], freq: f64) -> f64 {
        let mut re = 0.0;
        let mut im = 0.0;
        for (i, &x) in buf.iter().enumerate() {
            let ph = core::f64::consts::TAU * freq * i as f64 / SR;
            re += x * ph.cos();
            im += x * ph.sin();
        }
        (re * re + im * im) / buf.len() as f64
    }

    #[test]
    fn suppresses_a_resonance_but_not_the_bed() {
        let mut e = SpectralEngine::new(SR, 1024);
        e.params.amount = 1.0;
        e.params.threshold_db = 3.0;
        e.params.attack_ms = 1.0;
        e.update(SR);
        let n = 96_000;
        let mut seed = 7u64;
        let mut out = vec![0.0; n];
        let mut inp = vec![0.0; n];
        for i in 0..n {
            // Noise bed at low level + screaming 2 kHz resonance.
            let x = 0.02 * noise(&mut seed)
                + 0.5 * (core::f64::consts::TAU * 2000.0 * i as f64 / SR).sin();
            inp[i] = x;
            let (l, _) = e.tick(x, x);
            out[i] = l;
        }
        let late_in = &inp[n / 2..];
        let late_out = &out[n / 2..];
        let res_in = tone_energy(late_in, 2000.0);
        let res_out = tone_energy(late_out, 2000.0);
        let red_db = 10.0 * (res_out / res_in).log10();
        assert!(
            red_db < -6.0,
            "resonance should be pulled down: {red_db:.1} dB"
        );
    }

    #[test]
    fn amount_zero_is_transparent() {
        let mut e = SpectralEngine::new(SR, 1024);
        e.params.amount = 0.0;
        e.update(SR);
        let n = 48_000;
        let lat = e.latency();
        let mut seed = 3u64;
        let mut inp = vec![0.0; n];
        let mut out = vec![0.0; n];
        for i in 0..n {
            let x = 0.3 * noise(&mut seed);
            inp[i] = x;
            let (l, _) = e.tick(x, x);
            out[i] = l;
        }
        // Compare aligned by latency, skip warmup.
        let mut err = 0.0;
        let mut sig = 0.0;
        for i in 8000..(n - lat) {
            let d = out[i + lat] - inp[i];
            err += d * d;
            sig += inp[i] * inp[i];
        }
        let err_db = 10.0 * (err / sig).log10();
        assert!(
            err_db < -30.0,
            "amount 0 should be near-null: {err_db:.1} dB"
        );
    }

    #[test]
    fn delta_isolates_the_removed_part() {
        // suppressed + isolated must reconstruct the processed-off
        // signal: out(δ=0) + out(δ=1) = passthrough (per-frame linear).
        let run = |delta: f64| -> Vec<f64> {
            let mut e = SpectralEngine::new(SR, 1024);
            e.params.amount = 1.0;
            e.params.delta = delta;
            e.params.attack_ms = 1.0;
            e.update(SR);
            let mut seed = 11u64;
            let n = 48_000;
            let mut out = vec![0.0; n];
            for i in 0..n {
                let x = 0.02 * noise(&mut seed)
                    + 0.4 * (core::f64::consts::TAU * 3000.0 * i as f64 / SR).sin();
                let (l, _) = e.tick(x, x);
                out[i] = l;
            }
            out
        };
        let kept = run(0.0);
        let removed = run(1.0);
        // The removed part must carry the resonance far more than the
        // kept part carries it.
        let res_kept = tone_energy(&kept[24_000..], 3000.0);
        let res_removed = tone_energy(&removed[24_000..], 3000.0);
        assert!(
            res_removed > res_kept * 2.0,
            "delta output should isolate the resonance: kept={res_kept:.5} removed={res_removed:.5}"
        );
    }
}
