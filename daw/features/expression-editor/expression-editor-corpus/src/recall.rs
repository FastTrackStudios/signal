//! Turning the sweep into a curve a test can assert on.
//!
//! The point of the exercise, stated in `drum-datasets.md`: assert flam
//! recall "as a curve over grace spacing rather than picking a
//! threshold. `onsets.rs` documents its conservatism as deliberate —
//! the test should record where the knee is, not demand it move."
//!
//! So this module answers one question per case — *were both strikes
//! found?* — and aggregates it into recall against spacing. A
//! regression then shows up as the knee moving, which is a statement
//! about behaviour, where "onset count changed from 143 to 141" is not.
//!
//! ## Matching detections to truth, and the two traps in it
//!
//! An onset is reported as a frame index, so its time is quantised to
//! one hop — 5.3 ms at the default 256-sample hop and 48 kHz, the same
//! order as the smallest spacing in the sweep. Worse, the quantisation
//! is not symmetric: spectral flux cannot report a change before it
//! happens, and a strike rising out of a decay takes several frames to
//! accumulate enough flux to peak. Measured against the synthetic
//! sweep, detections land **2 to 10 ms late**, never early beyond hop
//! rounding. So the tolerance window is asymmetric — see [`Tolerance`]
//! — and the lag is reported rather than hidden, because a detector
//! that is reliably 6 ms late is a fact the quantize path needs.
//!
//! The second trap is subtler and it inflated the first curve this
//! module produced. When a flam yields only *one* detection, matching
//! by nearest truth credits it to whichever strike it happens to sit
//! closer to — and at 5 ms spacing that is sometimes the ghost. The
//! curve then reported 75% ghost recall at the tightest spacing in the
//! sweep and 25% accent recall, which is precisely backwards: what was
//! detected was obviously the accent.
//!
//! So the assignment is **accent-first**, one detection per strike: the
//! accent claims its nearest unused detection, and the ghost gets what
//! is left. That encodes the thing we know independently of the data —
//! a lone detection of a loud strike and a quiet one is the loud one.

use crate::flam::{FlamCase, Side};
use expression_editor_audio::onsets::{OnsetConfig, detect, onset_seconds};

/// How generously a detection is matched to a strike, in each
/// direction.
///
/// Asymmetric on purpose — see the module docs. `early` covers hop
/// rounding, which is all that can put a detection ahead of its
/// strike. `late` covers the hop plus the frames a masked strike takes
/// to build enough flux to peak, which is the larger and more variable
/// term.
#[derive(Clone, Copy, Debug)]
pub struct Tolerance {
    pub early_secs: f64,
    pub late_secs: f64,
}

impl Default for Tolerance {
    fn default() -> Self {
        Self {
            // One hop at the default 256/48 kHz, rounded up.
            early_secs: 0.006,
            // Half again over the worst lag measured on the synthetic
            // sweep, so a modest regression in the detector's response
            // time shows as a *lag* rather than as a phantom miss.
            late_secs: 0.015,
        }
    }
}

impl Tolerance {
    /// Symmetric, for callers that want the conventional form.
    pub fn symmetric(secs: f64) -> Self {
        Self {
            early_secs: secs,
            late_secs: secs,
        }
    }

    fn accepts(&self, detected: f64, truth: f64) -> bool {
        let dt = detected - truth;
        dt >= -self.early_secs && dt <= self.late_secs
    }
}

/// A configuration that can see a flam at all.
///
/// The default [`OnsetConfig`] sets `min_spacing_secs` to 50 ms, which
/// is longer than most flams — so under defaults the answer for every
/// spacing below 50 ms is fixed before the detector looks at the audio.
/// That is a real and correct property for segmenting a take, and it
/// makes the *threshold* question unanswerable, because the spacing
/// rule gets there first. This config lowers the floor to 3 ms and
/// changes nothing else, so what the curve then measures is the
/// detector's sensitivity rather than its spacing policy.
pub fn flam_config() -> OnsetConfig {
    OnsetConfig {
        min_spacing_secs: 0.003,
        ..OnsetConfig::default()
    }
}

/// What happened to one case.
#[derive(Clone, Copy, Debug)]
pub struct CaseResult {
    pub case: FlamCase,
    /// Detection time of the accent, if one matched.
    pub accent: Option<f64>,
    /// Detection time of the ghost, if one matched. The interesting
    /// column.
    pub ghost: Option<f64>,
    /// Detections that fell inside the case's window and matched
    /// neither strike — a spurious split, usually a decay re-triggering.
    pub extra: usize,
}

impl CaseResult {
    /// Both strikes found: the case passed.
    pub fn complete(&self) -> bool {
        self.accent.is_some() && self.ghost.is_some()
    }

    /// How late the accent was reported, in ms.
    pub fn accent_lag_ms(&self) -> Option<f64> {
        self.accent.map(|d| (d - self.case.accent_secs) * 1000.0)
    }

    /// How late the ghost was reported, in ms.
    pub fn ghost_lag_ms(&self) -> Option<f64> {
        self.ghost.map(|d| (d - self.case.grace_secs) * 1000.0)
    }
}

/// Run the detector over a rendered sweep and match what it found.
pub fn measure(
    samples: &[f64],
    sample_rate: f64,
    cases: &[FlamCase],
    cfg: OnsetConfig,
    tolerance: Tolerance,
) -> Vec<CaseResult> {
    let detected: Vec<f64> = detect(samples, sample_rate, cfg)
        .iter()
        .map(|o| onset_seconds(o, sample_rate, &cfg))
        .collect();
    match_cases(&detected, cases, tolerance)
}

/// The matching half, separated so it can be tested against a list of
/// times without rendering anything.
pub fn match_cases(detected: &[f64], cases: &[FlamCase], tolerance: Tolerance) -> Vec<CaseResult> {
    let mut used = vec![false; detected.len()];
    cases
        .iter()
        .map(|case| {
            let [first, last] = case.strikes();
            let (lo, hi) = (first - tolerance.early_secs, last + tolerance.late_secs);

            // Accent first, then ghost — see the module docs. Each
            // takes its nearest detection that no one has claimed.
            let claim = |truth: f64, used: &mut Vec<bool>| -> Option<f64> {
                let best = detected
                    .iter()
                    .enumerate()
                    .filter(|&(i, &d)| !used[i] && tolerance.accepts(d, truth))
                    .min_by(|a, b| (a.1 - truth).abs().total_cmp(&(b.1 - truth).abs()))
                    .map(|(i, &d)| (i, d));
                best.map(|(i, d)| {
                    used[i] = true;
                    d
                })
            };
            let accent = claim(case.accent_secs, &mut used);
            let ghost = claim(case.grace_secs, &mut used);

            let matched = accent.is_some() as usize + ghost.is_some() as usize;
            // Everything the detector put in this case's window,
            // matched or not. Cases are laid out far enough apart that
            // a window belongs to exactly one of them, so an unmatched
            // detection in here is a spurious split rather than the
            // neighbour's business.
            let in_window = detected.iter().filter(|&&d| d >= lo && d <= hi).count();
            CaseResult {
                case: *case,
                accent,
                ghost,
                extra: in_window.saturating_sub(matched),
            }
        })
        .collect()
}

/// One row of the curve: everything measured at one spacing, on one
/// side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurvePoint {
    pub side: Side,
    pub spacing_ms: f64,
    /// How many cases (one per grace velocity) fed this row.
    pub cases: usize,
    /// Of those, how many had the ghost found.
    pub ghost_found: usize,
    /// And how many had the accent found.
    ///
    /// Not a formality. In the grace-first ordering it is the *accent*
    /// that goes missing: the grace already claimed the flux, and log
    /// compression makes a rise out of silence worth far more than a
    /// louder rise out of a decay. The detector then reports the flam
    /// as one onset at the grace's position — several milliseconds
    /// before where a musician would say the note is, which is a
    /// quantize hazard rather than a detection one.
    pub accent_found: usize,
    /// And how many had **both** — the headline number.
    ///
    /// Neither column above answers "was the flam resolved" on its
    /// own, and reading them as if they did is how the first version of
    /// this curve claimed 100% recall for cases the detector had
    /// collapsed into a single onset.
    pub both_found: usize,
}

impl CurvePoint {
    pub fn ghost_recall(&self) -> f64 {
        if self.cases == 0 {
            0.0
        } else {
            self.ghost_found as f64 / self.cases as f64
        }
    }

    pub fn accent_recall(&self) -> f64 {
        if self.cases == 0 {
            0.0
        } else {
            self.accent_found as f64 / self.cases as f64
        }
    }

    /// Both strikes resolved: the flam was actually seen as a flam.
    pub fn flam_recall(&self) -> f64 {
        if self.cases == 0 {
            0.0
        } else {
            self.both_found as f64 / self.cases as f64
        }
    }
}

/// The curve: recall against spacing, one series per ordering.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve(pub Vec<CurvePoint>);

/// Aggregate case results into the curve.
pub fn recall_curve(results: &[CaseResult]) -> Curve {
    let mut points: Vec<CurvePoint> = Vec::new();
    for r in results {
        let point = match points
            .iter_mut()
            .find(|p| p.side == r.case.side && p.spacing_ms == r.case.spacing_ms)
        {
            Some(p) => p,
            None => {
                points.push(CurvePoint {
                    side: r.case.side,
                    spacing_ms: r.case.spacing_ms,
                    cases: 0,
                    ghost_found: 0,
                    accent_found: 0,
                    both_found: 0,
                });
                points.last_mut().expect("just pushed")
            }
        };
        point.cases += 1;
        point.ghost_found += r.ghost.is_some() as usize;
        point.accent_found += r.accent.is_some() as usize;
        point.both_found += r.complete() as usize;
    }
    points.sort_by(|a, b| {
        a.side
            .as_str()
            .cmp(b.side.as_str())
            .then(a.spacing_ms.total_cmp(&b.spacing_ms))
    });
    Curve(points)
}

impl Curve {
    /// The spacing at which flam recall — *both* strikes — first
    /// reaches `target` and stays there. The knee, and the number worth
    /// quoting.
    ///
    /// "And stays there" matters: recall that touches 1.0 at 20 ms,
    /// drops at 25 and returns at 30 has its knee at 30. The first
    /// crossing would be luck.
    pub fn knee_ms(&self, side: Side, target: f64) -> Option<f64> {
        let series: Vec<&CurvePoint> = self.0.iter().filter(|p| p.side == side).collect();
        let mut knee = None;
        for p in series.iter().rev() {
            if p.flam_recall() + 1e-9 >= target {
                knee = Some(p.spacing_ms);
            } else {
                break;
            }
        }
        knee
    }

    pub fn to_csv(&self) -> String {
        let mut s = String::from("side,spacing_ms,cases,ghost_found,accent_found,both_found\n");
        for p in &self.0 {
            s.push_str(&format!(
                "{},{},{},{},{},{}\n",
                p.side.as_str(),
                p.spacing_ms,
                p.cases,
                p.ghost_found,
                p.accent_found,
                p.both_found
            ));
        }
        s
    }

    pub fn parse_csv(text: &str) -> Result<Self, String> {
        let mut points = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("side,") {
                continue;
            }
            let f: Vec<&str> = line.split(',').collect();
            if f.len() != 6 {
                return Err(format!("line {}: want 6 fields, got {}", n + 1, f.len()));
            }
            let n_of = |i: usize| -> Result<usize, String> {
                f[i].parse().map_err(|e| format!("line {}: {e}", n + 1))
            };
            points.push(CurvePoint {
                side: match f[0] {
                    "before" => Side::Before,
                    "after" => Side::After,
                    other => return Err(format!("line {}: unknown side {other:?}", n + 1)),
                },
                spacing_ms: f[1].parse().map_err(|e| format!("line {}: {e}", n + 1))?,
                cases: n_of(2)?,
                ghost_found: n_of(3)?,
                accent_found: n_of(4)?,
                both_found: n_of(5)?,
            });
        }
        Ok(Curve(points))
    }
}

/// How late the detector was, over the strikes it found.
///
/// Worth reporting on its own rather than folding into the tolerance,
/// because it is the number the quantize path cares about most: a
/// detector that finds every hit but reports each one 6 ms late moves
/// every note 6 ms late unless something subtracts it. [`gate`] exists
/// precisely because an STFT cannot do better than this — the lag here
/// is a measurement of how much better a sample-rate detector needs to
/// be, not a defect to fix in [`onsets`].
///
/// [`gate`]: expression_editor_audio::gate
/// [`onsets`]: expression_editor_audio::onsets
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lag {
    pub matched: usize,
    pub median_ms: f64,
    pub worst_ms: f64,
}

/// Lag over the accents — the strike that is reliably found, so the
/// sample is not biased by which ghosts happened to survive.
pub fn accent_lag(results: &[CaseResult]) -> Lag {
    let mut lags: Vec<f64> = results.iter().filter_map(|r| r.accent_lag_ms()).collect();
    lags.sort_by(f64::total_cmp);
    Lag {
        matched: lags.len(),
        median_ms: lags.get(lags.len() / 2).copied().unwrap_or(0.0),
        worst_ms: lags.iter().copied().fold(0.0, f64::max),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flam::FlamSweep;

    fn case(side: Side, spacing_ms: f64) -> FlamCase {
        FlamCase {
            index: 0,
            side,
            spacing_ms,
            grace_velocity: 0.25,
            grace_secs: match side {
                Side::Before => 1.0,
                Side::After => 1.0 + spacing_ms / 1000.0,
            },
            accent_secs: match side {
                Side::Before => 1.0 + spacing_ms / 1000.0,
                Side::After => 1.0,
            },
        }
    }

    #[test]
    fn one_detection_cannot_satisfy_two_strikes() {
        // The failure the one-to-one rule exists to prevent: at 5 ms
        // the two truths are inside one tolerance of the same
        // detection, and a naive matcher would score the case complete.
        let c = case(Side::Before, 5.0);
        let out = match_cases(&[1.002], &[c], Tolerance::default());
        assert_eq!(out.len(), 1);
        assert!(!out[0].complete(), "{out:?}");
        assert_eq!(
            out[0].accent.is_some() as usize + out[0].ghost.is_some() as usize,
            1
        );
    }

    #[test]
    fn a_lone_detection_is_credited_to_the_accent() {
        // The artifact that inflated the first curve this produced: at
        // 5 ms the single detection sat nearer the ghost, and nearest-
        // truth matching reported 75% ghost recall at the tightest
        // spacing in the sweep. What was detected was the loud strike.
        let c = case(Side::After, 5.0);
        let out = match_cases(&[1.003], &[c], Tolerance::default());
        assert!(out[0].accent.is_some(), "{out:?}");
        assert!(out[0].ghost.is_none(), "{out:?}");
    }

    #[test]
    fn the_ghost_still_gets_a_detection_the_accent_cannot_reach() {
        let c = case(Side::Before, 40.0);
        // On the grace at 1.000; the accent at 1.040 is far outside
        // tolerance, so it cannot claim it.
        let out = match_cases(&[1.001], &[c], Tolerance::default());
        assert!(out[0].accent.is_none(), "{out:?}");
        assert!(out[0].ghost.is_some(), "{out:?}");
    }

    #[test]
    fn a_detection_before_its_strike_is_rejected_sooner_than_one_after() {
        let c = case(Side::Before, 40.0);
        // 10 ms early: outside the 6 ms early window.
        assert!(
            match_cases(&[0.990], &[c], Tolerance::default())[0]
                .ghost
                .is_none()
        );
        // 10 ms late: inside the 15 ms late window, because flux lags.
        assert!(
            match_cases(&[1.010], &[c], Tolerance::default())[0]
                .ghost
                .is_some()
        );
    }

    #[test]
    fn detections_outside_every_window_are_ignored() {
        let c = case(Side::After, 20.0);
        let out = match_cases(&[5.0], &[c], Tolerance::default());
        assert!(!out[0].complete());
        assert_eq!(out[0].extra, 0);
    }

    #[test]
    fn a_third_detection_in_the_window_is_extra() {
        let c = case(Side::Before, 30.0);
        let out = match_cases(&[1.000, 1.030, 1.034], &[c], Tolerance::default());
        assert!(out[0].complete());
        assert_eq!(out[0].extra, 1);
    }

    #[test]
    fn lag_is_reported_signed_against_the_authored_time() {
        let c = case(Side::Before, 30.0);
        let out = match_cases(&[1.004, 1.036], &[c], Tolerance::default());
        assert!((out[0].ghost_lag_ms().expect("ghost") - 4.0).abs() < 1e-6);
        assert!((out[0].accent_lag_ms().expect("accent") - 6.0).abs() < 1e-6);
    }

    #[test]
    fn the_knee_requires_recall_to_hold() {
        let curve = Curve(vec![
            CurvePoint {
                side: Side::Before,
                spacing_ms: 10.0,
                cases: 4,
                ghost_found: 4,
                accent_found: 4,
                both_found: 4,
            },
            CurvePoint {
                side: Side::Before,
                spacing_ms: 20.0,
                cases: 4,
                ghost_found: 1,
                accent_found: 4,
                both_found: 1,
            },
            CurvePoint {
                side: Side::Before,
                spacing_ms: 30.0,
                cases: 4,
                ghost_found: 4,
                accent_found: 4,
                both_found: 4,
            },
        ]);
        // 10 ms hits the target but does not hold, so the knee is 30.
        assert_eq!(curve.knee_ms(Side::Before, 1.0), Some(30.0));
    }

    #[test]
    fn the_curve_round_trips_through_csv() {
        let sweep = FlamSweep {
            spacings_ms: vec![10.0, 40.0],
            grace_velocities: vec![0.25, 0.6],
            ..Default::default()
        };
        let rendered = sweep.render();
        let results = measure(
            &rendered.samples,
            rendered.sample_rate,
            &rendered.cases,
            flam_config(),
            Tolerance::default(),
        );
        let curve = recall_curve(&results);
        assert_eq!(Curve::parse_csv(&curve.to_csv()).expect("parses"), curve);
    }
}
