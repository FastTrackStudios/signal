//! Shared level detector: peak/RMS-mix level → dB → soft-knee window →
//! 0..1 drive → attack/release follower with punch-smooth blend.
//!
//! Used by dynamic bands (per band), the spectral engine (per bin,
//! vectorized), and the transient splitter's realtime mode.

use super::histogram::LoudnessHistogram;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectorParams {
    /// Threshold in dB (used when `auto` is off; while auto is on it
    /// acts as an OFFSET around the learned threshold).
    pub threshold_db: f64,
    /// Soft-knee width in dB (0..32).
    pub knee_db: f64,
    /// How many dB over threshold correspond to full drive.
    ///
    /// Set this to the band's dynamic range and the band moves **one dB per dB
    /// over threshold, capped at the range** — which is what Pro-Q does,
    /// measured against the plugin: a -12 dB band at a -30 dB threshold sits
    /// at -5.99 dB when fed -24 dBFS and -11.99 dB at -18, and a -24 dB band
    /// reaches its cap 24 dB over rather than 12.
    ///
    /// Left at the default the window is a fixed span and the curve's slope
    /// scales with the range instead, so a deep band races to its cap: ours
    /// hit -22.9 dB where the plugin was at -12.2, a 10.7 dB miss that grew
    /// with the range.
    pub span_db: f64,
    /// Attack time in ms (0..1000).
    pub attack_ms: f64,
    /// Release time in ms (0..5000).
    pub release_ms: f64,
    /// RMS window in ms (0 = pure peak).
    pub rms_ms: f64,
    /// Peak↔RMS blend (0 = peak, 1 = RMS).
    pub rms_mix: f64,
    /// Punch-smooth blend (0 = plain AR, 1 = peak-hold smoothed).
    pub smooth: f64,
    /// Learn the threshold from a decaying loudness histogram.
    pub auto: bool,
    /// With `auto`, follow the programme instead of using the fixed level
    /// Pro-Q's Auto sits at.
    ///
    /// Off by default because it does not match the plugin — see
    /// [`AUTO_THRESHOLD_DB`]. Kept because a threshold that tracks the
    /// programme is a genuinely different and useful behaviour, and the
    /// histogram that drives it is built and tested; it simply is not what
    /// "Auto" means in a translated Pro-Q preset.
    pub adaptive: bool,
    /// Relative mode: detect `side_db − program_db` (band loud relative
    /// to the mix) instead of absolute side level.
    pub relative: bool,
}

/// The level at which an auto-threshold band reaches its full range, in dBFS.
///
/// Pro-Q's Auto does not sit at one threshold — it sits wherever the band has
/// to start so that it is **fully engaged by -40 dBFS**, which makes the
/// threshold `-40 - range`. Read off three measured curves, all of which
/// arrive at full range at the same input level despite very different ranges:
///
/// ```text
///   range  9.6 dB -> threshold -49.4   (-40 -  9.6 = -49.6)
///   range 12.0 dB -> threshold -51.9   (-40 - 12.0 = -52.0)
///   range 19.5 dB -> threshold -59.5   (-40 - 19.5 = -59.5)
/// ```
///
/// A single fixed threshold fits the middle case and misses the other two by
/// up to 7 dB, which is what a first reading of the -12 dB curve alone
/// suggested.
///
/// It is blended with the programme-tracking threshold rather than acting as a
/// floor under it — a floor measured better in isolation and worse across the
/// library; see `effective_threshold`.
///
/// Measured with a steady tone. Whether Auto drifts on real programme material
/// over longer spans is not established here; what is established is that it
/// does not follow a level sweep.
pub const AUTO_FULL_RANGE_AT_DB: f64 = -40.0;

/// How far under the learned programme level the settled threshold sits, in dB.
///
/// The three constants below are one model with one job: say where a band on
/// Auto comes to rest, and how long it takes to get there. They were fitted
/// **together**, against nine recorded plugin trajectories — three ranges by
/// three programme levels, each run from a cold start and left for forty
/// seconds — by `eq_auto_fit`, scoring the worst error over every second of
/// every run rather than the mean. Changing one of them alone will not hold.
///
/// What the plugin actually does, settled (0 dB = fully engaged):
///
/// ```text
///            -30 dBFS   -18.8   -9.0
///  range  4.5   -1.16    -0.32  -0.32
///  range  9.0   -1.54    -0.41  -0.41
///  range 18.0   -1.56    -0.44  -0.44
/// ```
///
/// Two things fall out of that table. The resting shortfall is **almost
/// independent of the range** — an engagement *fraction*, which is what this
/// used to be, would have spread the -18.8 column over 0.32..1.28 instead of
/// 0.32..0.44. And it **is** a function of the absolute level: 11 dB quieter
/// programme costs the band about a decibel of engagement, so the threshold
/// does not simply ride the material. Hence a headroom rather than a
/// fraction, and a tracking weight below 1.
///
/// The headroom only bites on deep bands: it is subtracted from the range and
/// floored at zero, so anything at or under 6 dB of range rests with its
/// threshold on the learned median.
///
/// **Fitted across level, not at one level.** It was 7 for a long time, which
/// is the optimum if the library is only ever measured on noise at
/// -18.8 dBFS. Measured at -30 as well, that setting is the worst of the
/// three — the whole library, presets at 2 dB or worse:
///
/// ```text
///   headroom   -18.8 dBFS   -30 dBFS   under 1 dB (both)
///        5.0        6           11           240
///        6.0        4           17           238
///        7.0        4           23           237
/// ```
///
/// 6 keeps everything 7 had at the reference level and takes six presets out
/// of the tail at -30. 5 is better still on both totals and costs four presets
/// at the reference, which is where the "95% under 1 dB" bar is read; if that
/// bar is ever retired in favour of a multi-level one, 5 is the answer.
pub const AUTO_HEADROOM_DB: f64 = 6.0;

/// How much of the settled threshold follows the programme, 0..1.
///
/// The remainder is anchored to [`AUTO_FULL_RANGE_AT_DB`]. That absolute
/// anchor is what gives the model its level dependence — a threshold that
/// tracked the programme completely would hold the same engagement at
/// -30 dBFS as at -9, and the plugin plainly does not.
pub const AUTO_TRACKING: f64 = 0.78;

/// How long a band on Auto takes to walk onto its settled threshold, as a
/// time constant in seconds.
///
/// The plugin does not arrive at its resting engagement — it walks there. One
/// band on unchanging noise, read once a second from a cold start, moved for
/// about five seconds and then held to the second decimal for the next
/// thirty-five:
///
/// ```text
///   1 s   2 s   3 s   4 s   5 s ... 40 s
///  -1.38 -1.19 -0.94 -0.61 -0.41    -0.41
/// ```
///
/// Nothing in a decaying histogram produces that on its own: fed a constant
/// level every observation lands in the same bin, so the percentile is right
/// from the first push and the band would be at its resting point inside a
/// block. The walk has to be modelled, and it is — as a threshold that starts
/// [`AUTO_COLD_OFFSET_DB`] high and decays onto the learned one.
///
/// This is why the library harness warms up for eight seconds. Anything read
/// before then is a reading of this ramp.
pub const AUTO_SETTLE_S: f64 = 3.0;

/// How far above its settled value a cold band's threshold starts, in dB.
///
/// Paired with [`AUTO_SETTLE_S`]. The sign matters and cost a round trip to
/// find: the plugin walks **up** into its engagement. Ramping from the
/// absolute fallback — the obvious thing, and what was tried first — moves the
/// band the wrong way, because that fallback is the *more* engaged of the two,
/// and it made the trajectory error worse than doing nothing (1.4 dB against
/// 1.1).
pub const AUTO_COLD_OFFSET_DB: f64 = 4.0;

impl Default for DetectorParams {
    fn default() -> Self {
        Self {
            threshold_db: -40.0,
            knee_db: 8.0,
            span_db: 12.0,
            attack_ms: 100.0,
            release_ms: 500.0,
            rms_ms: 10.0,
            rms_mix: 0.5,
            smooth: 1.0,
            auto: false,
            adaptive: false,
            relative: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Detector {
    pub params: DetectorParams,
    hist: LoudnessHistogram,
    /// RMS accumulator (one-pole mean-square — window-free, cheap).
    ms_state: f64,
    ms_coeff: f64,
    /// Program-level mean-square for relative mode.
    prog_ms_state: f64,
    /// Follower states: plain AR and the peak-hold ("punch") variant.
    ar_state: f64,
    hold_state: f64,
    attack_coeff: f64,
    release_coeff: f64,
    /// Histogram push decimation.
    push_countdown: u32,
    /// How far the learned threshold has taken over from the cold-start
    /// fallback, 0..1 — see [`AUTO_SETTLE_S`].
    auto_conf: f64,
    auto_conf_coeff: f64,
    sample_rate: f64,
}

/// One-pole coefficient with the 1 kHz-referenced constant used across
/// the fx tree: exp(−2π·1000 / (sr·ms)).
#[inline]
/// One-pole coefficient for a time constant of `ms`.
///
/// The time-constant convention: after `ms` the follower has covered
/// `1 - 1/e` of the distance. It used to carry a `2 * PI` factor — the
/// cutoff-frequency convention — which made every ballistic **2 pi times
/// faster than the number next to it**, so a 300 ms release actually released
/// in 48. Measured against the plugin, that let every dynamic band recover
/// between transients and under-reduce on any material that is not a steady
/// tone.
fn ar_coeff(ms: f64, sample_rate: f64) -> f64 {
    if ms <= 0.0 {
        1.0
    } else {
        1.0 - (-1000.0 / (sample_rate * ms)).exp()
    }
}

impl Detector {
    pub fn new(sample_rate: f64) -> Self {
        let mut d = Self {
            params: DetectorParams::default(),
            hist: LoudnessHistogram::new(),
            ms_state: 0.0,
            ms_coeff: 0.0,
            prog_ms_state: 0.0,
            ar_state: 0.0,
            hold_state: 0.0,
            attack_coeff: 1.0,
            release_coeff: 1.0,
            push_countdown: 0,
            auto_conf: 0.0,
            auto_conf_coeff: 0.0,
            sample_rate,
        };
        d.update(sample_rate);
        d
    }

    /// Recompute coefficients. Call after changing time params — never
    /// per sample.
    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.attack_coeff = ar_coeff(self.params.attack_ms.max(0.05), sample_rate);
        self.release_coeff = ar_coeff(self.params.release_ms.max(0.05), sample_rate);
        self.ms_coeff = ar_coeff(self.params.rms_ms.max(0.1), sample_rate);
        // The histogram is pushed at ~1 kHz, so the handover advances once per
        // push rather than once per sample.
        self.auto_conf_coeff = 1.0 - (-1.0 / (1000.0 * AUTO_SETTLE_S)).exp();
    }

    /// Effective (threshold, knee) — learned values while auto is on.
    pub fn effective_threshold(&self) -> (f64, f64) {
        if self.params.auto {
            if self.params.adaptive {
                if let Some((thr, _)) = self.hist.learned() {
                    // The knob re-centres as an offset around the learned
                    // value. The knee stays tied to the range, as it is
                    // everywhere else.
                    let tracking = thr - (self.params.span_db - AUTO_HEADROOM_DB).max(0.0);
                    let absolute = AUTO_FULL_RANGE_AT_DB - self.params.span_db;
                    // A weighted blend of the two, not a maximum.
                    //
                    // A maximum is the tidier story — above the anchor the
                    // band holds its engagement, below it gives range up — and
                    // it fits two isolated probes better: the nine recorded
                    // trajectories go from 0.62 dB worst to 0.37, and a 20 Hz
                    // high shelf on noise stops climbing with level the way
                    // the plugin's does. It measures WORSE across the library,
                    // which is the only test that counts here: 35 presets
                    // improved, 69 got worse, and the median went 0.52 dB to
                    // 0.72. The maximum collapses the engagement to nothing
                    // once the programme drops under the anchor, where the
                    // plugin only gives up a decibel or so, and most of the
                    // library's dynamic bands are band-limited enough to sit
                    // near that edge.
                    let learned =
                        AUTO_TRACKING * tracking + (1.0 - AUTO_TRACKING) * absolute;
                    // A cold band sits a little SHY of where it will end up —
                    // the plugin walks up into its engagement, it does not
                    // fall back into it. So the handover is a threshold that
                    // starts high and decays onto the learned one, not a
                    // crossfade from the absolute fallback: that fallback is
                    // the more engaged of the two and ramping from it moves
                    // the band the wrong way.
                    let blended = learned + (1.0 - self.auto_conf) * AUTO_COLD_OFFSET_DB;
                    return (
                        blended + self.params.threshold_db * 0.25,
                        self.params.knee_db,
                    );
                }
            }
            // Measured, not learned. Pro-Q's Auto behaves as a **fixed
            // absolute threshold**: swept in level with a steady tone it
            // produces an ordinary static curve starting near -52 dBFS, and
            // the same curve at 200 Hz, 1 kHz and 8 kHz — so it is neither
            // tracking the programme nor derived from the band's bandwidth.
            //
            // A learned threshold cannot reproduce that. Tracking the
            // programme means the band always sees itself as sitting at its
            // own threshold, so it applies roughly the same reduction whatever
            // it is fed: ours sat at a flat -3 dB from -60 to -3 dBFS where
            // the plugin ran the full 12. **299 of the 528 dynamic bands in
            // the factory library are on Auto**, so this is most of them.
            //
            // The knob stays an offset around it, which is how Pro-Q's reads.
            return (
                AUTO_FULL_RANGE_AT_DB - self.params.span_db + self.params.threshold_db * 0.25,
                self.params.knee_db,
            );
        }
        (self.params.threshold_db, self.params.knee_db)
    }

    /// One sample of the side signal (and the program signal for
    /// relative mode) → drive d ∈ [0, 1].
    #[inline]
    pub fn tick(&mut self, side: f64, program: f64) -> f64 {
        // Level: blend instantaneous |x| with a one-pole RMS.
        let sq = side * side;
        if self.ms_state <= 1.0e-18 {
            self.ms_state = sq;
        }
        self.ms_state += (sq - self.ms_state) * self.ms_coeff;
        let peak = side.abs();
        // Calibrate the RMS arm so a full-scale sine reads full scale, the
        // convention every level-dependent control is written against. Raw RMS
        // sits 3.01 dB under a sine's peak, so an uncalibrated detector reads
        // low and the threshold lands somewhere else than where it is marked:
        // measured against Pro-Q, our whole curve sat about 3.5 dB shy of its
        // at every input level, uniformly.
        const RMS_TO_PEAK: f64 = std::f64::consts::SQRT_2;
        let rms = self.ms_state.sqrt() * RMS_TO_PEAK;
        let level = peak + (rms - peak) * self.params.rms_mix.clamp(0.0, 1.0);
        let mut level_db = 20.0 * level.max(1.0e-10).log10();

        if self.params.relative {
            let psq = program * program;
            if self.prog_ms_state <= 1.0e-18 {
                self.prog_ms_state = psq;
            }
            self.prog_ms_state += (psq - self.prog_ms_state) * self.ms_coeff;
            let prog_db = 10.0 * self.prog_ms_state.max(1.0e-20).log10();
            level_db -= prog_db;
        }

        // Auto-threshold bookkeeping at ~1 kHz cadence. Only while something
        // reads it — a fixed threshold needs no histogram, and this runs on
        // the audio thread.
        if self.params.auto && self.params.adaptive {
            if self.push_countdown == 0 {
                self.push_countdown = (self.sample_rate / 1000.0) as u32;
                self.hist.push(level_db);
                self.auto_conf += (1.0 - self.auto_conf) * self.auto_conf_coeff;
            }
            self.push_countdown -= 1;
        }

        // dB over threshold, through a soft knee, scaled by the span.
        //
        // The drive stays 0..1 because that is what the band crossfades with,
        // but what it now *means* is "how far over threshold, as a fraction of
        // the span" — so `drive * range` is decibels over threshold clamped to
        // the range, one for one. The previous form divided by the knee width,
        // which made the slope a function of the range.
        let (thr, knee) = self.effective_threshold();
        let knee = knee.max(0.01);
        let over = level_db - thr;
        let shaped = if over <= -knee {
            0.0
        } else if over >= knee {
            over
        } else {
            // Standard quadratic knee: continuous in value and slope at both
            // corners, so a signal drifting across the threshold does not
            // step.
            (over + knee) * (over + knee) / (4.0 * knee)
        };
        let drive = (shaped / self.params.span_db.max(0.5)).clamp(0.0, 1.0);

        // Plain AR follower.
        let coeff = if drive > self.ar_state {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.ar_state += (drive - self.ar_state) * coeff;

        // Punch variant: release-tracked peak hold, then attack-smoothed.
        self.hold_state = (self.hold_state - self.hold_state * self.release_coeff).max(drive);
        let punched = self.ar_state + (self.hold_state - self.ar_state) * self.attack_coeff;

        let s = self.params.smooth.clamp(0.0, 1.0);
        (self.ar_state + (punched - self.ar_state) * s).clamp(0.0, 1.0)
    }

    pub fn reset(&mut self) {
        self.ms_state = 0.0;
        self.prog_ms_state = 0.0;
        self.ar_state = 0.0;
        self.hold_state = 0.0;
        self.push_countdown = 0;
        self.auto_conf = 0.0;
        self.hist.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn rides_up_on_loud_and_releases() {
        let mut d = Detector::new(SR);
        d.params.threshold_db = -20.0;
        d.params.attack_ms = 5.0;
        d.params.release_ms = 50.0;
        d.update(SR);
        let mut peak = 0.0f64;
        for _ in 0..4800 {
            peak = peak.max(d.tick(0.8, 0.8)); // ≈ −2 dB, well over
        }
        assert!(peak > 0.9, "loud input should drive to 1: {peak}");
        let mut out = 1.0;
        for _ in 0..48_000 {
            out = d.tick(0.0, 0.0);
        }
        assert!(out < 0.05, "should release: {out}");
    }

    #[test]
    fn below_threshold_stays_quiet() {
        let mut d = Detector::new(SR);
        d.params.threshold_db = -20.0;
        d.params.knee_db = 4.0;
        d.update(SR);
        let mut peak = 0.0f64;
        for _ in 0..9600 {
            peak = peak.max(d.tick(0.005, 0.005)); // ≈ −46 dB
        }
        assert!(peak < 0.01, "quiet input must not trigger: {peak}");
    }

    /// Auto follows the programme, settling partway into the band's travel.
    ///
    /// This has been round the houses. It first asserted that Auto learns a
    /// threshold near the programme level; that was replaced with a fixed
    /// threshold when a level sweep appeared to show Auto ignoring the
    /// programme entirely. **That sweep was wrong** — the plugin's Auto adapts
    /// over about seven seconds, and every reading in it was taken while the
    /// threshold was still moving, which is also why the same configuration
    /// measured twice in one run disagreed by 7 dB.
    ///
    /// Given time to settle, Auto plainly tracks: on unchanging noise the
    /// applied gain walks for seconds and then holds, partway to the band's
    /// target rather than at it. So the learned threshold is back, sitting
    /// [`AUTO_HEADROOM_DB`] under the level the band hears.
    ///
    /// [`AUTO_FULL_RANGE_AT_DB`] remains for bands with no learned value yet —
    /// the first moments after a reset, before the histogram has anything in
    /// it.
    #[test]
    fn auto_follows_the_programme() {
        let mut d = Detector::new(SR);
        d.params.auto = true;
        d.params.adaptive = true;
        d.params.threshold_db = 0.0; // offset knob centred
        d.params.span_db = 12.0;
        d.update(SR);
        // Ten seconds — the plugin's own Auto takes about seven to settle, and
        // the histogram here needs comparable time to fill.
        for i in 0..480_000 {
            let x = 0.125 * (core::f64::consts::TAU * 300.0 * i as f64 / SR).sin();
            d.tick(x, x);
        }
        // The programme here is a sine at about -18 dBFS, and the threshold
        // has to end up below it by most of the band's range.
        let (thr, _) = d.effective_threshold();
        assert!(
            (-45.0..=-20.0).contains(&thr),
            "auto should settle under the programme, got {thr}",
        );
        // And it must be the programme it followed, not a constant.
        let mut quiet = Detector::new(SR);
        quiet.params.auto = true;
        quiet.params.adaptive = true;
        quiet.params.span_db = d.params.span_db;
        quiet.update(SR);
        for i in 0..480_000 {
            let x = 0.004 * (core::f64::consts::TAU * 300.0 * i as f64 / SR).sin();
            quiet.tick(x, x);
        }
        let (quiet_thr, _) = quiet.effective_threshold();
        assert!(
            quiet_thr < thr - 10.0,
            "a quieter programme must learn a lower threshold: {quiet_thr} \
             against {thr}",
        );
    }

    /// Turning the tracking off leaves the fallback threshold.
    #[test]
    fn auto_without_tracking_uses_the_fallback() {
        let mut d = Detector::new(SR);
        d.params.auto = true;
        d.params.adaptive = false;
        d.params.span_db = 12.0;
        d.params.threshold_db = 0.0;
        d.update(SR);
        for i in 0..96_000 {
            let x = 0.125 * (core::f64::consts::TAU * 300.0 * i as f64 / SR).sin();
            d.tick(x, x);
        }
        let (thr, _) = d.effective_threshold();
        let want = AUTO_FULL_RANGE_AT_DB - d.params.span_db;
        assert!(
            (thr - want).abs() < 0.01,
            "without tracking auto should sit at {want}, got {thr}",
        );
    }

    /// Relative mode triggers on how far the band stands out of the mix.
    ///
    /// The numbers here changed when `drive` stopped being a squared window
    /// and became "decibels over threshold as a fraction of `span_db`" — a
    /// band 6 dB over a 12 dB span now reports 0.5, where the old squared
    /// form reported 0.77. What is being tested is unchanged: buried in the
    /// mix it must not move at all, dominating it must.
    #[test]
    fn relative_mode_needs_band_dominance() {
        // Side at -20 dB inside a -6 dB program: relative level ~ -14 dB
        // -> below a -6 dB relative threshold. Same side alone (program
        // equally quiet) -> relative ~ 0 dB -> 6 dB over threshold.
        let run = |program: f64| -> f64 {
            let mut d = Detector::new(SR);
            d.params.relative = true;
            d.params.threshold_db = -6.0;
            d.params.attack_ms = 1.0;
            d.params.span_db = 12.0;
            d.update(SR);
            let mut peak = 0.0f64;
            for _ in 0..9600 {
                peak = peak.max(d.tick(0.1, program));
            }
            peak
        };
        let buried = run(0.5);
        let dominant = run(0.1);
        assert!(buried < 0.05, "band buried in the mix must not trigger ({buried})");
        // 6 dB over threshold across a 12 dB span is half travel.
        assert!(
            (dominant - 0.5).abs() < 0.15,
            "band dominating the mix should sit near half travel ({dominant})",
        );
    }

    /// A time constant means what it says.
    ///
    /// `ar_coeff` used to carry the cutoff-frequency convention's `2 * PI`,
    /// which made every ballistic 2 pi times faster than the millisecond
    /// figure beside it — a 300 ms release actually released in 48. On a
    /// steady tone that is invisible, because the follower settles either way;
    /// on programme material it is the difference between a band that stays
    /// engaged and one that recovers between transients. Measured against the
    /// plugin the error was 11.7 dB mid-release.
    #[test]
    fn a_release_time_is_a_time_constant() {
        let mut d = Detector::new(SR);
        d.params.threshold_db = -30.0;
        d.params.span_db = 12.0;
        d.params.knee_db = 6.0;
        d.params.attack_ms = 1.0;
        d.params.release_ms = 300.0;
        d.params.rms_mix = 1.0;
        d.update(SR);

        // Drive it to the top, then let go.
        for i in 0..(SR as usize) {
            let x = 0.5 * (core::f64::consts::TAU * 1000.0 * i as f64 / SR).sin();
            d.tick(x, x);
        }
        let peak = d.tick(0.0, 0.0);
        assert!(peak > 0.9, "the detector should be fully driven ({peak})");

        // Time to fall to 1/e of that.
        let mut fell_at = None;
        for i in 0..(SR as usize) {
            if d.tick(0.0, 0.0) < peak * std::f64::consts::E.recip() {
                fell_at = Some(i as f64 * 1000.0 / SR);
                break;
            }
        }
        let tau = fell_at.expect("the detector must release");
        assert!(
            (tau - 300.0).abs() < 60.0,
            "a 300 ms release should take about 300 ms, took {tau:.0}",
        );
    }
}
