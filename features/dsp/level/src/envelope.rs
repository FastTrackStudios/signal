//! Turning the vocal chain's gain decisions into editable automation.
//!
//! The chain applies gain internally and emits nothing, so its analysis
//! is good and invisible: you cannot see what the gate decided, disagree
//! with it, or keep the ride while bypassing the gate. This module is
//! the output side — each stage emits a **sparse, shaped curve** that
//! sums with the others into the item's one volume envelope.
//!
//! ## Why decimation is load-bearing, not an optimisation
//!
//! Analysis is block-rate: the classifier's default block is 10 ms, so
//! 100 points per second per curve. A five-minute vocal is ~30,000
//! points per curve and ~120,000 across four — roughly 2.4 MB per item
//! as text. Without decimation the curves cannot live in the item at
//! all. It therefore happens **at generation**, not at storage.
//!
//! ## One analysis pass, four independent curves
//!
//! The stages share the classifier, block classes and adaptive silence
//! floor, and never share gain curves. That is what makes their
//! independence affordable: the rider still knows where silence *is*, so
//! it does not ride room tone up between lines, while remaining ignorant
//! of what the gate decided. One pass rather than four also matters
//! across a whole song.

use crate::classify::{adaptive_silence_db, BlockClass, Classifier, ClassifyConfig};

/// One breakpoint on a generated envelope.
///
/// dB rather than linear, because that is the space the ear, the
/// tolerance and the composite sum all work in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvPoint {
    /// Seconds from the start of the item.
    pub t_s: f64,
    /// Gain in dB. 0 is unity.
    pub db: f64,
    /// Whether the curve holds this value until the next point rather
    /// than ramping to it.
    ///
    /// A gate is on/off with an attack and a release; forcing that
    /// through linear interpolation is what makes a decimated curve need
    /// far more points than the shape it describes.
    pub hold: bool,
}

impl EnvPoint {
    pub fn new(t_s: f64, db: f64) -> Self {
        Self {
            t_s,
            db,
            hold: false,
        }
    }
}

/// What every stage reads, computed once.
pub struct Analysis {
    /// Class of each block, in order.
    pub classes: Vec<BlockClass>,
    /// RMS of each block, dBFS.
    pub rms_db: Vec<f64>,
    /// The adaptive silence floor for this material.
    pub silence_db: f64,
    /// Samples per block.
    pub block_len: usize,
    pub sample_rate: f64,
}

impl Analysis {
    /// Seconds at the centre of a block.
    pub fn time_of(&self, block: usize) -> f64 {
        (block as f64 + 0.5) * self.block_len as f64 / self.sample_rate.max(1.0)
    }

    pub fn blocks(&self) -> usize {
        self.classes.len()
    }

    /// Whether block `i` is singing: tonal, and above the silence floor.
    /// What the ride reads, and nothing else — a breath, an "ess" or room
    /// tone never sets how loud a phrase is.
    pub fn is_sung(&self, i: usize) -> bool {
        self.classes.get(i) == Some(&BlockClass::Tonal)
            && self.rms_db.get(i).is_some_and(|db| *db > self.silence_db)
    }
}

/// Run the one shared pass over an item's audio.
pub fn analyze(samples: &[f64], sample_rate: f64, cfg: ClassifyConfig) -> Analysis {
    let mut classifier = Classifier::new(sample_rate, cfg);
    let block_len = classifier.block_samples().max(1);

    let mut features = Vec::new();
    let mut rms_db = Vec::new();
    let mut rms_lin = Vec::new();
    for chunk in samples.chunks(block_len) {
        let f = classifier.analyze_block(chunk);
        rms_db.push(f.rms_db);
        rms_lin.push(f.rms);
        features.push(f);
    }

    let silence_db = adaptive_silence_db(&rms_lin);
    let classes = features
        .iter()
        .map(|f| classifier.classify(f, silence_db))
        .collect();

    Analysis {
        classes,
        rms_db,
        silence_db,
        block_len,
        sample_rate,
    }
}

/// Reduce a dense per-block series to breakpoints.
///
/// Ramer–Douglas–Peucker in dB space, with `forced` indices kept
/// regardless. Pure RDP rounds a gate's sharp edge; pure event-driven
/// placement misses a ride that drifts across a held note with no event
/// to hang a point on. Seeding RDP with the event boundaries gets both.
pub fn decimate(dense: &[EnvPoint], tolerance_db: f64, forced: &[usize]) -> Vec<EnvPoint> {
    if dense.len() <= 2 {
        return dense.to_vec();
    }
    let mut keep = vec![false; dense.len()];
    keep[0] = true;
    keep[dense.len() - 1] = true;
    for &i in forced {
        if i < keep.len() {
            keep[i] = true;
        }
    }

    // Split at every forced point so RDP never smooths across one.
    let anchors: Vec<usize> = (0..dense.len()).filter(|&i| keep[i]).collect();
    for pair in anchors.windows(2) {
        rdp(dense, pair[0], pair[1], tolerance_db.max(1e-9), &mut keep);
    }

    dense
        .iter()
        .zip(keep.iter())
        .filter_map(|(p, &k)| k.then_some(*p))
        .collect()
}

fn rdp(pts: &[EnvPoint], lo: usize, hi: usize, tol: f64, keep: &mut [bool]) {
    if hi <= lo + 1 {
        return;
    }
    let (a, b) = (pts[lo], pts[hi]);
    let span = b.t_s - a.t_s;

    let mut worst = 0.0_f64;
    let mut worst_i = lo;
    for (i, p) in pts.iter().enumerate().take(hi).skip(lo + 1) {
        let projected = if span.abs() < 1e-12 {
            a.db
        } else {
            a.db + (b.db - a.db) * ((p.t_s - a.t_s) / span)
        };
        let err = (p.db - projected).abs();
        if err > worst {
            worst = err;
            worst_i = i;
        }
    }
    if worst > tol {
        keep[worst_i] = true;
        rdp(pts, lo, worst_i, tol, keep);
        rdp(pts, worst_i, hi, tol, keep);
    }
}

/// Block indices where the gate's decision changes.
///
/// Forced into the decimation so an attack and a release survive it —
/// the edges are the whole shape of a gate.
fn transitions(open: &[bool]) -> Vec<usize> {
    // Both sides of each edge, so the step between them is real rather
    // than an artefact of where the blocks fell.
    edges(open)
}

/// Generate the gate's envelope from a shared analysis pass.
///
/// Emits the gain the gate *would* apply, per block, then decimates.
/// The stage's own smoothing is reproduced here rather than re-running
/// the sample-rate gate, because an envelope is read at block rate and
/// the difference is inaudible against the tolerance.
pub fn gate_envelope(
    analysis: &Analysis,
    threshold_db: f64,
    floor_db: f64,
    tolerance_db: f64,
) -> Vec<EnvPoint> {
    if analysis.blocks() == 0 {
        return Vec::new();
    }
    let open: Vec<bool> = analysis
        .rms_db
        .iter()
        .map(|&db| db >= threshold_db)
        .collect();

    let dense: Vec<EnvPoint> = open
        .iter()
        .enumerate()
        .map(|(i, &is_open)| {
            let db = if is_open { 0.0 } else { floor_db };
            EnvPoint {
                t_s: analysis.time_of(i),
                db,
                // A gate holds its state and steps; it does not ramp
                // between blocks.
                hold: true,
            }
        })
        .collect();

    decimate(&dense, tolerance_db, &transitions(&open))
}

/// A span of blocks with one gain, rather than a point curve.
///
/// Sibilance is a **discrete event**: two orders of magnitude fewer
/// entries than a curve at block rate, directly editable ("this ess,
/// 3 dB"), and the natural thing to hand a plugin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GainSpan {
    pub from_s: f64,
    pub to_s: f64,
    /// Gain in dB across the span. Negative reduces.
    pub db: f64,
}

/// Generate the breath envelope.
///
/// A breath is quiet, and not bright enough to be an "ess" — the two are
/// separated by the classifier rather than by re-running a detector, so
/// this stage and the sibilance stage cannot disagree about which is
/// which.
pub fn breath_envelope(
    analysis: &Analysis,
    reduction_db: f64,
    max_level_db: f64,
    min_level_db: f64,
    tolerance_db: f64,
) -> Vec<EnvPoint> {
    if analysis.blocks() == 0 {
        return Vec::new();
    }
    let ducked: Vec<bool> = analysis
        .rms_db
        .iter()
        .zip(&analysis.classes)
        .map(|(&db, &class)| class != BlockClass::Tonal && db < max_level_db && db > min_level_db)
        .collect();

    let dense: Vec<EnvPoint> = ducked
        .iter()
        .enumerate()
        .map(|(i, &d)| EnvPoint {
            t_s: analysis.time_of(i),
            db: if d { -reduction_db.abs() } else { 0.0 },
            // Breaths duck in and out over a slew, so they ramp.
            hold: false,
        })
        .collect();

    decimate(&dense, tolerance_db, &edges(&ducked))
}

/// Detect sibilant spans and the gain to apply over each.
///
/// Spans rather than a point curve, and **not** a split-band de-esser: a
/// volume envelope is broadband, so folding a band-split reduction into
/// it would duck the vowel body along with the "ess". True multiband
/// de-essing stays available by feeding the item into a plugin — a
/// different tool, rather than an envelope pretending to be one.
pub fn sibilance_spans(analysis: &Analysis, reduction_db: f64) -> Vec<GainSpan> {
    let mut spans: Vec<GainSpan> = Vec::new();
    let mut run: Option<usize> = None;

    for i in 0..analysis.blocks() {
        let sibilant = analysis.classes[i] == BlockClass::Consonant
            && analysis.rms_db[i] > analysis.silence_db;
        match (sibilant, run) {
            (true, None) => run = Some(i),
            (false, Some(start)) => {
                spans.push(GainSpan {
                    from_s: analysis.time_of(start),
                    to_s: analysis.time_of(i),
                    db: -reduction_db.abs(),
                });
                run = None;
            }
            _ => {}
        }
    }
    if let Some(start) = run {
        spans.push(GainSpan {
            from_s: analysis.time_of(start),
            to_s: analysis.time_of(analysis.blocks().saturating_sub(1)),
            db: -reduction_db.abs(),
        });
    }
    spans
}

/// Generate the ride envelope: how loud each phrase should be.
///
/// Rides **tonal** blocks toward the target and holds elsewhere. It
/// consults the shared silence floor, so it does not ride room tone up
/// between lines — but it never consults the gate's *output*, which is
/// what keeps the two independently bypassable.
pub fn ride_envelope(
    analysis: &Analysis,
    target_db: f64,
    amount: f64,
    max_gain_db: f64,
    max_cut_db: f64,
    tolerance_db: f64,
) -> Vec<EnvPoint> {
    if analysis.blocks() == 0 {
        return Vec::new();
    }
    let mut held = 0.0_f64;
    let dense: Vec<EnvPoint> = (0..analysis.blocks())
        .map(|i| {
            let ridable = analysis.classes[i] == BlockClass::Tonal
                && analysis.rms_db[i] > analysis.silence_db;
            if ridable {
                let correction = (target_db - analysis.rms_db[i]) * amount.clamp(0.0, 1.0);
                held = correction.clamp(-max_cut_db.abs(), max_gain_db.abs());
            }
            // Between phrases the ride holds its last value rather than
            // returning to unity, so a phrase does not swell at its edges.
            EnvPoint {
                t_s: analysis.time_of(i),
                db: held,
                hold: false,
            }
        })
        .collect();

    decimate(&dense, tolerance_db, &[])
}

/// How loud the phrase around each block is sung, dB: the power mean of
/// the sung blocks ([`Analysis::is_sung`]) within `window_s` centred on
/// it; `None` with none that near.
///
/// A ride read off single 10 ms blocks evens out every syllable — a
/// compressor, not a ride. Read off the phrase, it lifts a quiet line
/// and leaves the shape of the words inside it alone.
pub fn phrase_levels(analysis: &Analysis, window_s: f64) -> Vec<Option<f64>> {
    let n = analysis.blocks();
    let block_s = analysis.block_len as f64 / analysis.sample_rate.max(1.0);
    let half = ((window_s * 0.5) / block_s.max(1e-9)).round().max(0.0) as usize;
    // Prefix sums of the sung blocks' power, and of how many there are.
    let mut power = vec![0.0_f64; n + 1];
    let mut count = vec![0_usize; n + 1];
    for i in 0..n {
        let sung = analysis.is_sung(i);
        power[i + 1] = power[i]
            + if sung {
                10f64.powf(analysis.rms_db[i] / 10.0)
            } else {
                0.0
            };
        count[i + 1] = count[i] + usize::from(sung);
    }
    (0..n)
        .map(|i| {
            let (from, to) = (i.saturating_sub(half), (i + half + 1).min(n));
            let k = count[to] - count[from];
            (k > 0).then(|| 10.0 * ((power[to] - power[from]) / k as f64).log10())
        })
        .collect()
}

/// Ride each phrase toward `target_db`: on a sung block, the gain that
/// brings its phrase level (`levels`, from [`phrase_levels`]) `amount`
/// of the way to the target, within the range; between phrases the last
/// gain held, so a line does not swell at its edges.
///
/// The target comes from outside — one level for a whole song, so a
/// quiet verse is brought up to where the choruses are sung, not only
/// evened against itself.
pub fn phrase_ride(
    analysis: &Analysis,
    levels: &[Option<f64>],
    target_db: f64,
    amount: f64,
    (max_gain_db, max_cut_db): (f64, f64),
    tolerance_db: f64,
) -> Vec<EnvPoint> {
    let mut held = 0.0_f64;
    let dense: Vec<EnvPoint> = (0..analysis.blocks())
        .map(|i| {
            if let Some(level) = levels
                .get(i)
                .copied()
                .flatten()
                .filter(|_| analysis.is_sung(i))
            {
                held = ((target_db - level) * amount.clamp(0.0, 1.0))
                    .clamp(-max_cut_db.abs(), max_gain_db.abs());
            }
            EnvPoint {
                t_s: analysis.time_of(i),
                db: held,
                hold: false,
            }
        })
        .collect();
    decimate(&dense, tolerance_db, &[])
}

/// The floor the composite is clamped to, in dB.
///
/// A fully closed gate is -inf dB and a user can drag a curve to zero by
/// hand, but a take volume envelope needs a real number that every
/// consumer downstream can handle. -96 dB is inaudible.
pub const COMPOSITE_FLOOR_DB: f64 = -96.0;

/// Which of the four are contributing.
///
/// Bypass is a **mix decision**, not a display toggle: the composite is
/// rewritten without the stage, so it changes what you hear.
///
/// The bypassed curve is *retained*, not discarded, so un-bypassing
/// restores it exactly rather than regenerating it.
///
/// One consequence held deliberately: the four are generated from a
/// shared analysis pass but stored independently, so bypassing one
/// leaves the others as they were generated. That is a known
/// approximation, accepted because it only bites when you bypass —
/// which is a deliberate act with audible consequences — and because
/// regenerating downstream curves on a toggle would fight the
/// stored-curve model and make bypass expensive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bypass {
    pub gate: bool,
    pub breath: bool,
    pub sibilance: bool,
    pub ride: bool,
}

impl Default for Bypass {
    /// Nothing bypassed.
    fn default() -> Self {
        Self {
            gate: false,
            breath: false,
            sibilance: false,
            ride: false,
        }
    }
}

impl Bypass {
    pub fn all_bypassed(&self) -> bool {
        self.gate && self.breath && self.sibilance && self.ride
    }
}

/// The four contributions, ready to sum.
#[derive(Clone, Debug, Default)]
pub struct Contributions {
    pub gate: Vec<EnvPoint>,
    pub breath: Vec<EnvPoint>,
    pub sibilance: Vec<GainSpan>,
    pub ride: Vec<EnvPoint>,
    /// Which of them are switched off.
    pub bypass: Bypass,
}

impl Contributions {
    fn gate_pts(&self) -> &[EnvPoint] {
        if self.bypass.gate {
            &[]
        } else {
            &self.gate
        }
    }
    fn breath_pts(&self) -> &[EnvPoint] {
        if self.bypass.breath {
            &[]
        } else {
            &self.breath
        }
    }
    fn ride_pts(&self) -> &[EnvPoint] {
        if self.bypass.ride {
            &[]
        } else {
            &self.ride
        }
    }
    fn sibilance_spans(&self) -> &[GainSpan] {
        if self.bypass.sibilance {
            &[]
        } else {
            &self.sibilance
        }
    }
}

/// Value of a point curve at `t`, honouring `hold`.
fn value_at(points: &[EnvPoint], t: f64) -> f64 {
    if points.is_empty() {
        return 0.0;
    }
    // The first point after `t`: a held point's value starts AT its own
    // time, so at a gate's edge this is the gate as it is from there on.
    match points.iter().position(|p| p.t_s > t) {
        None => points[points.len() - 1].db,
        Some(0) => points[0].db,
        Some(i) => {
            let (a, b) = (points[i - 1], points[i]);
            if a.hold {
                // A gate holds its value until the next point, which is
                // the discontinuity that makes it cheap to represent.
                return a.db;
            }
            let span = b.t_s - a.t_s;
            if span.abs() < 1e-12 {
                b.db
            } else {
                a.db + (b.db - a.db) * ((t - a.t_s) / span)
            }
        }
    }
}

/// Gain the sibilance spans apply at `t`.
fn span_value_at(spans: &[GainSpan], t: f64) -> f64 {
    spans
        .iter()
        .find(|s| t >= s.from_s && t < s.to_s)
        .map(|s| s.db)
        .unwrap_or(0.0)
}

/// Sum the four into the item's one volume envelope.
///
/// **The sum in dB**, which is identical to multiplying the linear
/// gains — so the composite is exact rather than an approximation and
/// needs no reconciliation with what the realtime chain would do.
///
/// Every breakpoint from any contribution becomes a breakpoint of the
/// result, because that is where the sum can change slope. Bypassing a
/// stage is simply leaving it out of `Contributions`, which is why
/// bypass is a mix decision rather than a display toggle.
pub fn composite(parts: &Contributions) -> Vec<EnvPoint> {
    // A span is a *step*, but the composite's points ramp. Without a
    // point immediately before each edge carrying the pre-edge value,
    // the ramp into the edge starts at the previous breakpoint and the
    // reduction bleeds backwards across the whole gap — audibly ducking
    // the vowel before the "ess", which is the exact failure that made
    // sibilance a span instead of a band-split in the first place.
    const EDGE: f64 = 1e-4;
    let mut times: Vec<f64> = Vec::new();
    for p in parts
        .gate_pts()
        .iter()
        .chain(parts.breath_pts())
        .chain(parts.ride_pts())
    {
        // A held point (the gate's) is a step too: the value before it
        // just before it, or the sum ramps from there across the whole
        // gap to the next breakpoint — a gate opening over seconds.
        if p.hold {
            times.push((p.t_s - EDGE).max(0.0));
        }
        times.push(p.t_s);
    }
    for s in parts.sibilance_spans() {
        times.push((s.from_s - EDGE).max(0.0));
        times.push(s.from_s);
        times.push((s.to_s - EDGE).max(0.0));
        times.push(s.to_s);
    }
    if times.is_empty() {
        return Vec::new();
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-9);

    times
        .into_iter()
        .map(|t| {
            let sum = value_at(parts.gate_pts(), t)
                + value_at(parts.breath_pts(), t)
                + span_value_at(parts.sibilance_spans(), t)
                + value_at(parts.ride_pts(), t);
            EnvPoint {
                t_s: t,
                db: sum.max(COMPOSITE_FLOOR_DB),
                // The result ramps: only the gate steps, and its step is
                // already carried by a pair of points at the edge.
                hold: false,
            }
        })
        .collect()
}

/// Block indices either side of every change in a boolean series.
fn edges(flags: &[bool]) -> Vec<usize> {
    let mut out = Vec::new();
    for i in 1..flags.len() {
        if flags[i] != flags[i - 1] {
            out.push(i - 1);
            out.push(i);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A gate is a step in the sum, not a ramp to the next breakpoint:
    /// opened at 1.0 s with nothing else changing until a ride point at
    /// 3.0 s, the sum is open (the ride's level) from 1.0 s on, and closed
    /// up to it.
    #[test]
    fn a_gate_opens_as_a_step_in_the_sum() {
        let held = |t_s: f64, db: f64| EnvPoint {
            t_s,
            db,
            hold: true,
        };
        let parts = Contributions {
            gate: vec![held(0.0, -30.0), held(1.0, 0.0)],
            ride: vec![EnvPoint::new(0.0, 2.0), EnvPoint::new(3.0, 2.0)],
            ..Contributions::default()
        };
        let sum = composite(&parts);
        let at = |t: f64| value_at(&sum, t);
        assert!(
            (at(0.999) - (-28.0)).abs() < 1e-6,
            "closed before: {}",
            at(0.999)
        );
        assert!(
            (at(1.0) - 2.0).abs() < 1e-6,
            "open at the edge: {}",
            at(1.0)
        );
        assert!((at(2.0) - 2.0).abs() < 1e-6, "and after it: {}", at(2.0));
    }

    fn ramp(n: usize, from: f64, to: f64) -> Vec<EnvPoint> {
        (0..n)
            .map(|i| {
                let x = i as f64 / (n - 1) as f64;
                EnvPoint::new(i as f64 * 0.01, from + (to - from) * x)
            })
            .collect()
    }

    /// An analysis of `levels` (dB), every block sung, 10 ms blocks.
    fn sung(levels: &[f64]) -> Analysis {
        Analysis {
            classes: vec![BlockClass::Tonal; levels.len()],
            rms_db: levels.to_vec(),
            silence_db: -70.0,
            block_len: 480,
            sample_rate: 48_000.0,
        }
    }

    #[test]
    fn a_quiet_verse_is_brought_up_to_the_chorus() {
        // Two seconds of verse at -32 dB, two of chorus at -20.
        let mut levels = vec![-32.0; 200];
        levels.extend(vec![-20.0; 200]);
        let a = sung(&levels);
        let curve = phrase_ride(&a, &phrase_levels(&a, 0.5), -20.0, 1.0, (12.0, 12.0), 0.25);
        let at = |t: f64| value_at(&curve, t);
        assert!(
            (at(1.0) - 12.0).abs() < 0.3,
            "the verse up 12 dB: {}",
            at(1.0)
        );
        assert!(at(3.0).abs() < 0.3, "the chorus as it is: {}", at(3.0));
    }

    #[test]
    fn a_phrase_keeps_the_shape_of_its_words() {
        // A word 6 dB down inside a steady line, 60 ms long: the phrase
        // level barely moves, so the ride leaves the dip where it is.
        let mut levels = vec![-20.0; 200];
        for l in &mut levels[100..106] {
            *l = -26.0;
        }
        let a = sung(&levels);
        let curve = phrase_ride(&a, &phrase_levels(&a, 0.5), -20.0, 1.0, (12.0, 12.0), 0.25);
        assert!(
            value_at(&curve, 1.03) < 1.0,
            "the dip not filled in: {}",
            value_at(&curve, 1.03)
        );
    }

    #[test]
    fn silence_sets_no_phrase_level() {
        let mut a = sung(&[-20.0; 100]);
        for c in &mut a.classes[..] {
            *c = BlockClass::Silence;
        }
        assert!(phrase_levels(&a, 0.5).iter().all(Option::is_none));
    }

    #[test]
    fn a_straight_line_reduces_to_its_endpoints() {
        let got = decimate(&ramp(100, -6.0, 0.0), 0.25, &[]);
        assert_eq!(got.len(), 2, "nothing in between says anything new");
    }

    #[test]
    fn a_slow_drift_across_a_held_note_is_not_flattened() {
        // 6 dB over the span, with no event anywhere: pure event-driven
        // placement would emit nothing between the ends.
        let dense = ramp(200, 0.0, -6.0);
        let got = decimate(&dense, 0.25, &[]);
        assert!(got.len() >= 2);
        // The reconstruction must stay within tolerance everywhere.
        for p in &dense {
            let approx = sample_at(&got, p.t_s);
            assert!(
                (approx - p.db).abs() <= 0.25 + 1e-9,
                "drift lost at {}: {} vs {}",
                p.t_s,
                approx,
                p.db
            );
        }
    }

    #[test]
    fn a_forced_point_survives_decimation() {
        let dense = ramp(100, 0.0, 0.0);
        let got = decimate(&dense, 0.25, &[50]);
        assert!(
            got.iter().any(|p| (p.t_s - dense[50].t_s).abs() < 1e-9),
            "a boundary the classifier found is not RDP's to discard"
        );
    }

    #[test]
    fn a_tighter_tolerance_yields_more_points_never_fewer() {
        let dense = ramp(200, 0.0, -12.0);
        let coarse = decimate(&dense, 1.0, &[]).len();
        let fine = decimate(&dense, 0.05, &[]).len();
        assert!(fine >= coarse, "{fine} < {coarse}");
    }

    #[test]
    fn a_gate_edge_keeps_both_sides() {
        let mut dense: Vec<EnvPoint> = Vec::new();
        let mut open = vec![];
        for i in 0..100 {
            let is_open = i >= 50;
            open.push(is_open);
            dense.push(EnvPoint {
                t_s: i as f64 * 0.01,
                db: if is_open { 0.0 } else { -60.0 },
                hold: true,
            });
        }
        let got = decimate(&dense, 0.25, &transitions(&open));
        let has = |t: f64| got.iter().any(|p| (p.t_s - t).abs() < 1e-9);
        assert!(has(0.49), "the last closed block");
        assert!(has(0.50), "and the first open one");
    }

    fn sample_at(pts: &[EnvPoint], t: f64) -> f64 {
        match pts.iter().position(|p| p.t_s >= t) {
            None => pts.last().map(|p| p.db).unwrap_or(0.0),
            Some(0) => pts[0].db,
            Some(i) => {
                let (a, b) = (pts[i - 1], pts[i]);
                let span = b.t_s - a.t_s;
                if span.abs() < 1e-12 {
                    b.db
                } else {
                    a.db + (b.db - a.db) * ((t - a.t_s) / span)
                }
            }
        }
    }
}

// ── Generation policy ────────────────────────────────────────────────

/// Everything that changes what the four curves come out as.
///
/// Hashed to decide whether a cached set is still valid. Deliberately
/// *not* including the audio: an item's samples do not change under it,
/// and hashing megabytes on every open would cost more than the
/// analysis it is trying to avoid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GenerationConfig {
    pub gate_threshold_db: f64,
    pub gate_floor_db: f64,
    pub breath_reduction_db: f64,
    pub breath_max_level_db: f64,
    pub breath_min_level_db: f64,
    pub sibilance_reduction_db: f64,
    pub ride_target_db: f64,
    pub ride_amount: f64,
    pub ride_max_gain_db: f64,
    pub ride_max_cut_db: f64,
    /// The decimation knob — the one number worth exposing if curves
    /// ever do get too large.
    pub tolerance_db: f64,
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            gate_threshold_db: -40.0,
            gate_floor_db: -60.0,
            breath_reduction_db: 12.0,
            breath_max_level_db: -30.0,
            breath_min_level_db: -70.0,
            sibilance_reduction_db: 4.0,
            ride_target_db: -18.0,
            ride_amount: 1.0,
            ride_max_gain_db: 6.0,
            ride_max_cut_db: 6.0,
            tolerance_db: 0.25,
        }
    }
}

impl GenerationConfig {
    /// A stable digest of the settings.
    ///
    /// Hand-rolled rather than `Hash`, because these are floats and
    /// `f64` is deliberately not `Hash`. Bit patterns are the right
    /// comparison here: two configs that differ in the last bit produce
    /// different curves, so they should miss the cache.
    pub fn digest(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |v: f64| {
            for b in v.to_bits().to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x1000_0000_01b3);
            }
        };
        eat(self.gate_threshold_db);
        eat(self.gate_floor_db);
        eat(self.breath_reduction_db);
        eat(self.breath_max_level_db);
        eat(self.breath_min_level_db);
        eat(self.sibilance_reduction_db);
        eat(self.ride_target_db);
        eat(self.ride_amount);
        eat(self.ride_max_gain_db);
        eat(self.ride_max_cut_db);
        eat(self.tolerance_db);
        h
    }
}

/// Generate all four curves from one analysis pass.
pub fn generate(analysis: &Analysis, cfg: &GenerationConfig) -> Contributions {
    Contributions {
        gate: gate_envelope(
            analysis,
            cfg.gate_threshold_db,
            cfg.gate_floor_db,
            cfg.tolerance_db,
        ),
        breath: breath_envelope(
            analysis,
            cfg.breath_reduction_db,
            cfg.breath_max_level_db,
            cfg.breath_min_level_db,
            cfg.tolerance_db,
        ),
        sibilance: sibilance_spans(analysis, cfg.sibilance_reduction_db),
        ride: ride_envelope(
            analysis,
            cfg.ride_target_db,
            cfg.ride_amount,
            cfg.ride_max_gain_db,
            cfg.ride_max_cut_db,
            cfg.tolerance_db,
        ),
        bypass: Bypass::default(),
    }
}

/// Curves plus the config digest they were generated from.
///
/// Generation is **on demand** — the first time the level editor is
/// opened on an item — not on project load. Opening a 40-track project
/// would otherwise run 40 analysis passes for envelopes nobody will look
/// at.
#[derive(Clone, Debug)]
pub struct Cached {
    pub digest: u64,
    pub curves: Contributions,
}

impl Cached {
    /// Whether this cache still answers for `cfg`.
    ///
    /// Changing a threshold re-runs generation. **Nothing else does** —
    /// which is what composes with "only corrections persist": a
    /// regenerated curve nobody edited need not be written at all.
    pub fn is_valid_for(&self, cfg: &GenerationConfig) -> bool {
        self.digest == cfg.digest()
    }

    pub fn generate(analysis: &Analysis, cfg: &GenerationConfig) -> Self {
        Self {
            digest: cfg.digest(),
            curves: generate(analysis, cfg),
        }
    }
}

/// The config an item starts from: its own, else its track\'s, else the
/// default.
///
/// Setting a breath threshold once for a vocal track and having it apply
/// to thirty comps is the difference between usable on a real session
/// and not — and it is the same cascade #191 uses for mode, not a second
/// mechanism. The item owns its curves outright; only the config it
/// *starts* from is inherited.
pub fn config_for(
    item: Option<GenerationConfig>,
    track_default: Option<GenerationConfig>,
) -> GenerationConfig {
    item.or(track_default).unwrap_or_default()
}

// ── Absorbing an external edit ───────────────────────────────────────

/// Fold an externally-edited composite back into the ride.
///
/// The take volume envelope is an ordinary DAW envelope and users can
/// drag it, at which point it no longer equals the sum of the four.
///
/// The delta goes into the **ride**, and only the ride: someone dragging
/// a volume envelope meant "this bit louder", which is exactly what the
/// ride expresses. Spreading it across all four would put gain into the
/// gate and sibilance curves at times when neither is doing anything.
///
/// Doing this keeps the invariant *the four sum to the composite* true
/// at all times, so there is never a stale state and never a re-derive
/// prompt — a mode with its own UI for a situation that has an exact
/// answer. Locking the composite was rejected outright: it would stop
/// DAW users doing the most ordinary thing in their DAW to an item we
/// touched.
///
/// The absorbed delta is a **user edit**, so it persists like any other.
///
/// Idempotent: absorbing a composite that already matches changes
/// nothing, so a reload without a further external edit is a no-op.
pub fn absorb_into_ride(parts: &mut Contributions, edited: &[EnvPoint]) {
    if edited.is_empty() {
        return;
    }
    let ours = composite(parts);

    // Every time either side has an opinion about, so the correction is
    // sampled wherever it could differ.
    let mut times: Vec<f64> = ours.iter().map(|p| p.t_s).collect();
    times.extend(edited.iter().map(|p| p.t_s));
    times.extend(parts.ride.iter().map(|p| p.t_s));
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-9);

    let ride_now: Vec<EnvPoint> = parts.ride.clone();
    parts.ride = times
        .into_iter()
        .map(|t| {
            let delta = value_at(edited, t) - value_at(&ours, t);
            EnvPoint {
                t_s: t,
                db: value_at(&ride_now, t) + delta,
                hold: false,
            }
        })
        .collect();
}
