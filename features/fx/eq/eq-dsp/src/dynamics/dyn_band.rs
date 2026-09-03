//! One dynamic EQ band: SVF filter + side-chain detector +
//! base/target gain crossfade.
//!
//! `gain_db(t) = base + d(t)·(target − base)` — the detector drive
//! `d ∈ [0,1]` breathes the band between its two drawn curves.
//! `target − base` is the Pro-Q-style bipolar "dynamic range".

use super::detector::Detector;

/// The band's Q as the state-variable filter wants it.
///
/// The static designs read Q on Pro-Q's convention, where a displayed 1.0 is
/// Butterworth — filter Q of `1/sqrt(2)`. The SVF takes the filter Q directly,
/// so handing it the displayed value builds a band about 1.4x too narrow.
/// Measured against the plugin with a band pinned at full range: at half an
/// octave off centre Pro-Q was at -5.69 dB and this filter at -3.96, while
/// both agreed exactly at the centre — the signature of a width error rather
/// than a gain one, and one that only shows on a frequency sweep because a
/// level sweep sits at the centre where the two agree.
///
/// **A shelf needs the square root of it.** The two topologies read a shelf's
/// Q differently, and a single scalar only lines them up at Q 1: at 0.05 the
/// SVF sat near the midpoint across the whole band where the cascade completed
/// its transition, and at 4 it overshot about twice as far. Fitting the SVF
/// against the static design — which already matches the plugin — over a
/// nine-to-one span of Q gives `0.707 * sqrt(q)` to within 6%, and holds the
/// worst error under a quarter of a decibel. **109 of the 527 dynamic bands
/// in the factory library sit outside Q 0.3..3**, so the extremes are not a
/// corner case.
#[inline]
fn svf_q(display_q: f64, shape: DynShape) -> f64 {
    let q = display_q.max(1.0e-6);
    match shape {
        DynShape::Bell => q * std::f64::consts::FRAC_1_SQRT_2,
        DynShape::LowShelf | DynShape::HighShelf => q.sqrt() * std::f64::consts::FRAC_1_SQRT_2,
    }
}
use super::svf::{Svf, SvfShape};
use crate::band::Placement;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynShape {
    Bell,
    LowShelf,
    HighShelf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideMode {
    /// Trigger on the band's own frequency region (bandpass at the
    /// band's freq/Q) — Pro-Q "Band" / ZL "Side Link".
    BandLinked,
    /// Trigger on a custom range set by `side_lo_hz..side_hi_hz`.
    Free,
    /// Trigger on the unfiltered side signal.
    Wide,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DynBandParams {
    pub shape: DynShape,
    pub freq_hz: f64,
    pub q: f64,
    /// Static (base) gain in dB.
    pub base_gain_db: f64,
    /// Dynamic range in dB: target = base + range. Negative =
    /// compress-when-loud, positive = expand-when-loud.
    pub range_db: f64,
    pub side_mode: SideMode,
    pub side_lo_hz: f64,
    pub side_hi_hz: f64,
    /// Which part of the stereo image the band (and its detector)
    /// works on — Mid lets you duck just the center, Side just the
    /// width, etc.
    pub placement: Placement,
    pub enabled: bool,
    /// Run the detector and report a gain, but leave the audio alone.
    ///
    /// For the shapes the SVF cannot build — Pro-Q's Flat Tilt is the one the
    /// factory library uses, 15 bands of it — the dynamics are applied by
    /// modulating the *static* design's gain instead. The band still needs a
    /// detector, side filter and ballistics; it just must not filter twice.
    pub modulate_only: bool,
}

impl Default for DynBandParams {
    fn default() -> Self {
        Self {
            shape: DynShape::Bell,
            freq_hz: 1000.0,
            q: 1.0,
            base_gain_db: 0.0,
            range_db: 0.0,
            side_mode: SideMode::BandLinked,
            side_lo_hz: 20.0,
            side_hi_hz: 20000.0,
            placement: Placement::Stereo,
            enabled: true,
            modulate_only: false,
        }
    }
}

/// Per-sample dB smoothing time for the applied gain (declicks the
/// detector edges without adding another follower stage).
const GAIN_SMOOTH_MS: f64 = 2.0;

/// How much wider than the band itself the band-linked detector listens.
///
/// Fitted against the plugin's effective noise bandwidth over Q 0.2 to 8 (the
/// table in [`DynBand::update`]), on the worst error across the whole sweep:
///
/// ```text
///   width    Q 0.2   0.5      1      2      4      8
///     2.0    -1.31  -2.93  -3.22  -2.83  -1.19   0.41
///     3.0     0.76  -1.20  -1.71  -1.57  -0.53   0.63
///     4.2     0.86  +0.1x  -0.4x  -0.4x   0.3x   0.9x   <- chosen
///     5.0     0.86   0.94   0.37   0.17   0.84   1.22
/// ```
///
/// One would make the trigger region exactly the band's own -3 dB width; the
/// plugin listens four times wider than that.
const SIDE_WIDTH: f64 = 4.2;

#[derive(Debug, Clone)]
pub struct DynBand {
    pub params: DynBandParams,
    pub detector: Detector,
    filter: Svf,
    /// Side-chain band filter (mono — detector sees the mono sum).
    side_bp: Svf,
    side_hp: Svf,
    side_lp: Svf,
    /// Second pair, cascaded — the trigger region is fourth-order.
    ///
    /// A single Butterworth pair's skirts overlap once the band is narrow, so
    /// its effective noise bandwidth stops shrinking: measured, ours floored
    /// at -15.4 dB from Q 4 upward while the plugin kept going to -17.3 at
    /// Q 8. Doubling the order pulls the skirts in and lets the region follow
    /// the band.
    side_hp2: Svf,
    side_lp2: Svf,
    /// Undoes the side filter's own loss at the frequency it is watching.
    side_makeup: f64,
    applied_gain_db: f64,
    gain_smooth_coeff: f64,
    sample_rate: f64,
}

impl DynBand {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let mut b = Self {
            params: DynBandParams::default(),
            detector: Detector::new(sample_rate),
            filter: Svf::new(sample_rate),
            side_bp: Svf::new(sample_rate),
            side_hp: Svf::new(sample_rate),
            side_lp: Svf::new(sample_rate),
            side_hp2: Svf::new(sample_rate),
            side_lp2: Svf::new(sample_rate),
            side_makeup: 1.0,
            applied_gain_db: 0.0,
            gain_smooth_coeff: 0.0,
            sample_rate,
        };
        b.update(sample_rate);
        b
    }

    /// Recompute filters + coefficients after parameter changes.
    /// Never call per sample.
    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        // The detector's span is the band's range, so the drive it reports is
        // decibels over threshold rather than a fraction of a fixed window —
        // see `DetectorParams::span_db`. Without this the curve's slope grows
        // with the range and a deep band races to its cap.
        let range = self.params.range_db.abs().max(0.5);
        self.detector.params.span_db = range;
        // The knee widens with the range, which is what the plugin does. Read
        // off two measured curves: at its threshold a -12 dB band sits at
        // -1.33 dB and a -24 dB band at -3.85, and a quadratic knee of
        // `range / 2` puts them at -1.5 and -3.0 — where a fixed knee has the
        // deep band arriving far too late and the shallow one too early.
        self.detector.params.knee_db = range * 0.5;
        // Pure (calibrated) RMS, not a peak blend.
        //
        // A band-limited detector's whole job is to report the energy in its
        // own slice of the spectrum, and RMS is the only estimator that scales
        // with bandwidth the way energy does. Blending in an instantaneous
        // peak breaks that: measured against the plugin, widening a band from
        // Q 4 to Q 0.5 raised Pro-Q's detected level by ~7 dB — close to the
        // 9 dB the eightfold bandwidth implies — while the blend moved ours by
        // 1.5. A sine is unaffected either way, since the calibration puts
        // RMS at the peak, which is why the tone curves matched while the
        // noise curves did not.
        self.detector.params.rms_mix = 1.0;
        self.gain_smooth_coeff = 1.0 - (-1.0 / (GAIN_SMOOTH_MS * 0.001 * sample_rate)).exp();
        let shape = match self.params.shape {
            DynShape::Bell => SvfShape::Bell,
            DynShape::LowShelf => SvfShape::LowShelf,
            DynShape::HighShelf => SvfShape::HighShelf,
        };
        self.filter.set_sample_rate(sample_rate);
        self.filter.set(
            shape,
            self.params.freq_hz,
            svf_q(self.params.q, self.params.shape),
            self.applied_gain_db,
        );
        self.side_bp.set_sample_rate(sample_rate);
        // Band-linked trigger: bandpass-ish selectivity via a bell-Q
        // pair of cut filters is overkill — a resonant band-limited
        // path from HP+LP at the band edges tracks Pro-Q's "band
        // limited according to the band's frequency range".
        let (lo, hi) = match self.params.side_mode {
            SideMode::BandLinked => {
                // The detector's width, measured rather than assumed.
                //
                // A tone at the band's centre and broadband noise at the same
                // RMS differ only in spectrum, so the gap between the levels
                // at which each takes the band to half its range **is** the
                // detector's effective noise bandwidth. Read off both engines
                // (`eq_detector_probe`), with the old arithmetic placement at
                // the band's own Q:
                //
                // ```text
                //     Q     0.2    0.5      1      2      4      8
                //  Pro-Q   -4.5   -6.9   -9.4  -12.2  -15.3  -17.3
                //   ours   -9.5  -11.8  -13.5  -15.0  -15.4  -15.4
                // ```
                //
                // Two errors in that. Ours was far too narrow everywhere below
                // Q 4 — five decibels of missing noise energy at Q 0.2 — and
                // it stopped narrowing at all above Q 4, because the edges
                // were placed **arithmetically**: `f ± bw/2` puts the lower
                // edge below zero at low Q (it was clamped to 10 Hz) and
                // collapses the pair onto the centre at high Q, where the two
                // Butterworth skirts then overlap into one wide response.
                //
                // Placed geometrically the pair is symmetric on a log axis at
                // every Q, which is what a constant-Q trigger region is.
                let qq = svf_q(self.params.q, DynShape::Bell).max(0.02);
                let alpha = SIDE_WIDTH / (2.0 * qq);
                let ratio = (1.0 + alpha * alpha).sqrt() + alpha;
                match self.params.shape {
                    // A shelf listens to everything it shelves, not to a band
                    // around its corner. Measured with a 1 kHz high shelf on a
                    // -12 dB range and a fixed threshold, swept with a tone:
                    //
                    // ```text
                    //      Hz    500    1000    2000    4000    8000   16000
                    //   Pro-Q  -0.90   -6.00  -11.10  -11.94  -12.00  -12.00
                    //   banded -0.89   -6.00  -11.12  -11.94  -10.98   -0.34
                    // ```
                    //
                    // A tone four octaves above the corner drives the plugin's
                    // band to its cap and left ours untouched. It matters most
                    // where the corner is far out of the way: on "Drumbus -
                    // Mud Control and Magic" a 20 Hz high shelf with an 8 dB
                    // range applied 7.7 dB in the plugin and 2.1 here, because
                    // a band around 20 Hz hears almost nothing.
                    DynShape::HighShelf => {
                        ((self.params.freq_hz / ratio).max(10.0), sample_rate * 0.45)
                    }
                    DynShape::LowShelf => {
                        (10.0, (self.params.freq_hz * ratio).min(sample_rate * 0.45))
                    }
                    DynShape::Bell => (
                        (self.params.freq_hz / ratio).max(10.0),
                        (self.params.freq_hz * ratio).min(sample_rate * 0.45),
                    ),
                }
            }
            SideMode::Free => (self.params.side_lo_hz, self.params.side_hi_hz),
            SideMode::Wide => (0.0, 0.0),
        };
        self.side_hp2.set_sample_rate(sample_rate);
        self.side_lp2.set_sample_rate(sample_rate);
        if lo > 0.0 {
            self.side_hp.set(SvfShape::Highpass, lo, 0.707, 0.0);
            self.side_lp.set(SvfShape::Lowpass, hi, 0.707, 0.0);
            self.side_hp2.set(SvfShape::Highpass, lo, 0.707, 0.0);
            self.side_lp2.set(SvfShape::Lowpass, hi, 0.707, 0.0);
            // The side filter attenuates the very thing it is listening for.
            // A band-linked detector at freq/Q brackets the band with a
            // Butterworth pair whose skirts are already ~1 dB down at the
            // centre, so a tone sitting exactly on the band reads quieter than
            // it is and the threshold triggers late. Undo that at the watched
            // frequency, so the number on the threshold means the level of the
            // thing being watched.
            let watch = match self.params.side_mode {
                SideMode::Free => (lo * hi).sqrt(),
                _ => self.params.freq_hz,
            };
            // Second-order Butterworth magnitudes at `watch`.
            let highpass_ratio = watch / lo.max(1.0);
            let lowpass_ratio = watch / hi.max(1.0);
            let hp = highpass_ratio * highpass_ratio / (1.0 + highpass_ratio.powi(4)).sqrt();
            let lp = 1.0 / (1.0 + lowpass_ratio.powi(4)).sqrt();
            // Squared, because each shape is applied twice.
            self.side_makeup = 1.0 / (hp * hp * lp * lp).max(1.0e-4);
        } else {
            self.side_makeup = 1.0;
        }
        self.detector.update(sample_rate);
    }

    /// Current live gain in dB (for metering / the yellow bar).
    #[must_use]
    pub const fn live_gain_db(&self) -> f64 {
        self.applied_gain_db
    }

    /// Process one stereo sample in place. `side` is the external
    /// side-chain sample (mono); pass the input's mono sum when no
    /// external side-chain is routed.
    /// Advance the detector and the applied gain without touching the audio.
    ///
    /// The gain lands in [`Self::live_gain_db`], which is what a
    /// `modulate_only` band's owner reads to drive its static design.
    #[inline]
    pub fn observe(&mut self, left: f64, right: f64, side: f64) {
        let component = match self.params.placement {
            Placement::Stereo => side,
            Placement::Left => left,
            Placement::Right => right,
            Placement::Mid => 0.5 * (left + right),
            Placement::Side => 0.5 * (left - right),
        };
        let filtered_side = if self.params.side_mode == SideMode::Wide {
            side
        } else {
            let hp = self.side_hp2.tick(0, self.side_hp.tick(0, component));
            let lp = self.side_lp2.tick(0, self.side_lp.tick(0, hp));
            lp * self.side_makeup
        };
        let d = self.detector.tick(filtered_side, side);
        let target = self.params.base_gain_db + d * self.params.range_db;
        self.applied_gain_db += (target - self.applied_gain_db) * self.gain_smooth_coeff;
    }

    #[inline]
    pub fn tick(&mut self, left: &mut f64, right: &mut f64, side: f64) {
        if !self.params.enabled {
            return;
        }
        if self.params.modulate_only {
            self.observe(*left, *right, side);
            return;
        }
        // The stereo component this band works on — the detector
        // listens to the SAME component (a Side band triggers on side
        // energy, a Mid band on center energy), except in Wide/external
        // configurations where the caller-provided signal wins.
        let component = match self.params.placement {
            Placement::Stereo => side,
            Placement::Left => *left,
            Placement::Right => *right,
            Placement::Mid => 0.5 * (*left + *right),
            Placement::Side => 0.5 * (*left - *right),
        };
        // Side path: band-limit, detect.
        let filtered_side = if self.params.side_mode == SideMode::Wide {
            side
        } else {
            let hp = self.side_hp2.tick(0, self.side_hp.tick(0, component));
            let lp = self.side_lp2.tick(0, self.side_lp.tick(0, hp));
            lp * self.side_makeup
        };
        let d = self.detector.tick(filtered_side, side);

        // Base → target crossfade, smoothed, cheap gain-only retune.
        let target = self.params.base_gain_db + d * self.params.range_db;
        self.applied_gain_db += (target - self.applied_gain_db) * self.gain_smooth_coeff;
        self.filter.set_gain_db(self.applied_gain_db);

        match self.params.placement {
            Placement::Stereo => {
                *left = self.filter.tick(0, *left);
                *right = self.filter.tick(1, *right);
            }
            Placement::Left => *left = self.filter.tick(0, *left),
            Placement::Right => *right = self.filter.tick(1, *right),
            Placement::Mid => {
                let m = 0.5 * (*left + *right);
                let s = 0.5 * (*left - *right);
                let m = self.filter.tick(0, m);
                *left = m + s;
                *right = m - s;
            }
            Placement::Side => {
                let m = 0.5 * (*left + *right);
                let s = 0.5 * (*left - *right);
                let s = self.filter.tick(0, s);
                *left = m + s;
                *right = m - s;
            }
        }
    }

    pub fn reset(&mut self) {
        self.filter.reset();
        self.side_bp.reset();
        self.side_hp.reset();
        self.side_lp.reset();
        self.side_hp2.reset();
        self.side_lp2.reset();
        self.detector.reset();
        self.applied_gain_db = self.params.base_gain_db;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    /// RMS of a window of samples.
    fn rms(buf: &[f64]) -> f64 {
        let len_f = f64::from(u32::try_from(buf.len()).unwrap_or(1));
        (buf.iter().map(|x| x * x).sum::<f64>() / len_f).sqrt()
    }

    #[test]
    fn compresses_its_band_when_loud() {
        // −12 dB dynamic range bell at 1 kHz: a loud 1 kHz sine should
        // be attenuated once the detector rides up; a quiet one passes.
        let run = |amp: f64| -> f64 {
            let mut b = DynBand::new(SR);
            b.params.range_db = -12.0;
            b.params.freq_hz = 1000.0;
            b.params.q = 1.0;
            b.detector.params.threshold_db = -20.0;
            b.detector.params.attack_ms = 2.0;
            b.detector.params.rms_mix = 1.0;
            b.update(SR);
            let n = 48_000;
            let mut out = vec![0.0; n];
            for (i, output) in out.iter_mut().enumerate().take(n) {
                let mut l = amp * (core::f64::consts::TAU * 1000.0 * (i as u32 as f64) / SR).sin();
                let mut r = l;
                let side = l;
                b.tick(&mut l, &mut r, side);
                *output = l;
            }
            20.0 * (rms(&out[n / 2..]) / (amp / 2.0f64.sqrt())).log10()
        };
        let loud = run(0.5); // ≈ −6 dB, over threshold
        let quiet = run(0.005); // ≈ −46 dB, under
        assert!(loud < -8.0, "loud sine should be pulled down: {loud}");
        assert!(quiet.abs() < 1.0, "quiet sine passes at base gain: {quiet}");
    }

    #[test]
    fn out_of_band_content_does_not_trigger() {
        // Band-linked side filter at 4 kHz: loud 200 Hz content must
        // not duck the 4 kHz band.
        let mut b = DynBand::new(SR);
        b.params.range_db = -12.0;
        b.params.freq_hz = 4000.0;
        b.params.q = 2.0;
        b.detector.params.threshold_db = -20.0;
        b.update(SR);
        let mut min_gain = 0.0f64;
        for i in 0..48_000 {
            let mut l = 0.7 * (core::f64::consts::TAU * 200.0 * i as f64 / SR).sin();
            let mut r = l;
            let side = l;
            b.tick(&mut l, &mut r, side);
            min_gain = min_gain.min(b.live_gain_db());
        }
        assert!(
            min_gain > -2.0,
            "out-of-band bass must not trigger the band: {min_gain}"
        );
    }

    /// A settled Auto band rests just shy of its target, and rests lower on
    /// quieter programme.
    ///
    /// The numbers are the plugin's, measured from a cold start on unchanging
    /// noise and left for forty seconds (0 dB = fully engaged):
    ///
    /// ```text
    ///            -30 dBFS   -18.8   -9.0
    ///  range  4.5   -1.16    -0.32  -0.32
    ///  range  9.0   -1.54    -0.41  -0.41
    ///  range 18.0   -1.56    -0.44  -0.44
    /// ```
    ///
    /// Two properties out of that table are worth pinning: the band does not
    /// arrive at its target, and it gives up engagement when the programme
    /// gets quieter. An engagement *fraction* — what this used to be — has
    /// the first and not the second, and a purely programme-tracking
    /// threshold has neither.
    #[test]
    fn auto_settles_shy_of_target_and_lower_on_quiet_programme() {
        // A +range band based at -range, so full engagement is 0 dB of gain
        // and the settled value reads as the shortfall directly.
        let settle = |level: f64, range: f64| -> f64 {
            let mut b = DynBand::new(SR);
            b.params.range_db = range;
            b.params.base_gain_db = -range;
            b.params.freq_hz = 1000.0;
            b.params.q = 1.0;
            b.detector.params.auto = true;
            b.detector.params.adaptive = true;
            b.detector.params.threshold_db = 0.0;
            b.update(SR);
            // Ten seconds — past the handover, which has a three-second time
            // constant.
            let mut rng = 0x51DE_0042u64;
            for _ in 0..(10 * SR as usize) {
                rng = rng.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                let u = ((rng >> 33) as f64 / (1u64 << 31) as f64) - 1.0;
                let mut l = level * u * 3.0f64.sqrt();
                let mut r = l;
                let side = l;
                b.tick(&mut l, &mut r, side);
            }
            b.live_gain_db()
        };

        let loud = 10.0f64.powf(-18.8 / 20.0);
        let quiet = 10.0f64.powf(-30.0 / 20.0);
        for range in [4.5f64, 9.0, 18.0] {
            let hot = settle(loud, range);
            assert!(
                (-4.0..-0.05).contains(&hot),
                "range {range}: a settled auto band should rest just under its \
                 target, got {hot:.2} dB",
            );
            let cold = settle(quiet, range);
            assert!(
                cold < hot - 0.2,
                "range {range}: quieter programme must engage LESS \
                 ({cold:.2} against {hot:.2})",
            );
        }
    }

    /// The handover is a ramp, not a step.
    ///
    /// The plugin walks into its engagement over about five seconds; nothing
    /// in a decaying histogram does that on its own, so it is modelled
    /// explicitly and this is the guard that it still happens.
    #[test]
    fn auto_walks_into_its_engagement() {
        let mut b = DynBand::new(SR);
        b.params.range_db = 9.0;
        b.params.base_gain_db = -9.0;
        b.detector.params.auto = true;
        b.detector.params.adaptive = true;
        b.detector.params.threshold_db = 0.0;
        b.update(SR);
        let level = 10.0f64.powf(-18.8 / 20.0);
        let mut rng = 0x51DE_0042u64;
        let at = |b: &mut DynBand, seconds: usize, rng: &mut u64| {
            for _ in 0..(seconds * SR as usize) {
                *rng = rng.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                let u = ((*rng >> 33) as f64 / (1u64 << 31) as f64) - 1.0;
                let mut l = level * u * 3.0f64.sqrt();
                let mut r = l;
                let side = l;
                b.tick(&mut l, &mut r, side);
            }
            b.live_gain_db()
        };
        let one = at(&mut b, 1, &mut rng);
        let ten = at(&mut b, 9, &mut rng);
        assert!(
            ten > one + 0.3,
            "the band should engage further as it settles ({one:.2} -> {ten:.2})",
        );
    }

    #[test]
    fn expansion_rides_up() {
        let mut b = DynBand::new(SR);
        b.params.range_db = 9.0; // expand when loud
        b.detector.params.threshold_db = -20.0;
        b.detector.params.attack_ms = 2.0;
        b.update(SR);
        let mut max_gain = 0.0f64;
        for i in 0..48_000 {
            let mut l = 0.5 * (core::f64::consts::TAU * 1000.0 * i as f64 / SR).sin();
            let mut r = l;
            let side = l;
            b.tick(&mut l, &mut r, side);
            max_gain = max_gain.max(b.live_gain_db());
        }
        assert!(
            max_gain > 7.0,
            "positive range should boost when loud: {max_gain}"
        );
    }
}
