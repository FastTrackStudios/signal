//! Matching the two frame sequences.
//!
//! Banded dynamic time warping. The band is the important part and it is
//! placed in three moves, each of which fixes a failure the previous one
//! leaves:
//!
//! 1. **Around the diagonal**, because the same moment of two takes of
//!    one part is at roughly the same time in both.
//! 2. **Around the diagonal scaled to the two lengths**, because two
//!    takes of a four-minute song routinely differ by a couple of percent
//!    in total length, and a band around `i` has drifted several seconds
//!    off the true path by the end.
//! 3. **Offset by the measured macro lag** ([`crate::offset`]), because a
//!    dub that came in a second late is not near the diagonal at all, and
//!    widening the band to reach it is exactly what a band exists to
//!    prevent.
//!
//! ## Steps are not free
//!
//! A plain DTW charges nothing for moving sideways, so when two frames
//! are nearly equally good it will take a step, then another, then
//! another — the "staircase", where a phrase is held still and then
//! yanked. Two rules stop it: every non-diagonal step costs
//! [`WarpConfig::slope_penalty`], and a step in the same direction as the
//! one before it costs [`WarpConfig::staircase_penalty`] times more, so a
//! run of them gets rapidly more expensive than spreading the same
//! correction out.
//!
//! Both are waived where the frame being consumed is silent. Stretching
//! a gap between phrases is genuinely free — nothing is sounding, so
//! nothing is distorted — and this is what lets the slack collect in the
//! gaps instead of inside words.
//!
//! ## Why the stretch ratio is bounded later, not here
//!
//! The usual bound on how far a warp may stray is the Itakura
//! parallelogram, which constrains the path by tying it to *both*
//! corners of the matrix. That is wrong for this problem: the path here
//! is deliberately free at both ends, so that a dub which starts late is
//! not forced to stretch its opening silence to reach the reference's
//! first frame. The ratio is instead enforced where it turns into audio,
//! in [`crate::anchors`], on the segments between the warp points that
//! actually get written. A ratio limit on a DTW cell is a limit on
//! nothing anybody hears.

use crate::features::Frame;

/// Weights on the individual feature distances.
///
/// Everything except `voicing` and `pitch` is a squared difference of two
/// `0.0..=1.0` values, and the weights are normalized to sum to one
/// before use, so a preset is a statement about *relative* importance and
/// the absolute numbers do not have to be balanced by hand.
#[derive(Clone, Copy, Debug)]
pub struct CostConfig {
    /// Broadband level. The safe cue that works on anything.
    pub level: f64,
    /// Per-band levels: sub, mid, presence, air. What separates a kick
    /// from a tom and an "sss" from an "ah".
    pub bands: [f64; 4],
    /// Rise in air-band energy. Sharp onsets.
    pub flux: f64,
    /// Rise in broadband energy. Blunt onsets.
    pub delta: f64,
    /// Zero-crossing rate. Noise against tone.
    pub zcr: f64,
    /// Pitch distance where both frames are voiced, per octave apart.
    ///
    /// Low by default and for a good reason: a harmony part is
    /// *deliberately* at a different pitch from the lead it doubles, and
    /// aligning the two must still work. Raise it only when both takes
    /// are meant to be the same notes.
    pub pitch: f64,
    /// Flat cost of matching a voiced frame to an unvoiced one.
    ///
    /// Not normalized with the rest — it is a penalty, not a
    /// measurement. On a vocal it is the single strongest cue there is,
    /// because it is what makes consonants line up with consonants.
    /// Meaningless, and therefore zero, on material with no pitch track.
    pub voicing: f64,
}

impl Default for CostConfig {
    fn default() -> Self {
        Self {
            level: 0.15,
            bands: [0.15, 0.30, 0.25, 0.15],
            flux: 0.20,
            delta: 0.15,
            zcr: 0.10,
            pitch: 0.15,
            voicing: 0.6,
        }
    }
}

impl CostConfig {
    /// Weights scaled to sum to one, so the cost of a pair is on the same
    /// scale whatever the preset.
    fn normalized(mut self) -> Self {
        let total = self.level
            + self.bands.iter().sum::<f64>()
            + self.flux
            + self.delta
            + self.zcr
            + self.pitch;
        if total <= 1e-9 {
            // Every weight zero: fall back to level alone rather than
            // returning a cost of zero for every pair, which would make
            // the path arbitrary.
            self = Self {
                level: 1.0,
                bands: [0.0; 4],
                flux: 0.0,
                delta: 0.0,
                zcr: 0.0,
                pitch: 0.0,
                voicing: self.voicing,
            };
            return self;
        }
        self.level /= total;
        for b in &mut self.bands {
            *b /= total;
        }
        self.flux /= total;
        self.delta /= total;
        self.zcr /= total;
        self.pitch /= total;
        self
    }
}

/// How the search is shaped.
#[derive(Clone, Copy, Debug)]
pub struct WarpConfig {
    /// Half-width of the search band, in seconds.
    ///
    /// This is how far the warp may move material *relative to the macro
    /// offset*, not in absolute terms — the offset has already taken care
    /// of the constant part. A second is generous for a phrase.
    pub band_secs: f64,
    /// Cost of a non-diagonal step.
    pub slope_penalty: f64,
    /// Multiplier on a step that repeats the previous step's direction.
    pub staircase_penalty: f64,
    /// What the penalties are multiplied by when the consumed frame is
    /// silent. Near zero: bending a gap is free.
    pub silence_discount: f64,
    /// Preference for not moving, per band-width of correction.
    ///
    /// Small, and it earns its place where nothing else has an opinion:
    /// silent frames pair at zero cost, so without a tiebreak the path
    /// through a gap is arbitrary and the map wanders. With no evidence,
    /// do nothing.
    pub diagonal_bias: f64,
}

impl Default for WarpConfig {
    fn default() -> Self {
        Self {
            band_secs: 0.5,
            slope_penalty: 0.02,
            staircase_penalty: 2.2,
            silence_discount: 0.15,
            diagonal_bias: 0.02,
        }
    }
}

/// Distance between two frames.
fn cost(d: Frame, r: Frame, cfg: &CostConfig) -> f64 {
    // Two silent frames are the same moment as far as anything can tell,
    // so pairing them is free and the slack collects here.
    if d.silent && r.silent {
        return 0.0;
    }
    let sq = |a: f64, b: f64| (a - b) * (a - b);
    let mut c = cfg.level * sq(d.level, r.level)
        + cfg.flux * sq(d.flux, r.flux)
        + cfg.delta * sq(d.delta, r.delta)
        + cfg.zcr * sq(d.zcr, r.zcr);
    // Spectral shape, where there is one. A silent frame's shape is the
    // shape of a noise floor, and comparing that to a sung vowel would
    // add a large and entirely meaningless distance on top of the level
    // difference that already says these two frames are not the same
    // thing.
    if !d.silent && !r.silent {
        for k in 0..4 {
            c += cfg.bands[k] * sq(d.bands[k], r.bands[k]);
        }
    }
    match (d.pitch, r.pitch) {
        // Per octave, capped: two takes an octave apart are as different
        // as pitch can usefully say, and beyond that the number would
        // swamp every other cue.
        (Some(a), Some(b)) => c += cfg.pitch * ((a - b).abs() / 12.0).min(1.0),
        // One voiced, one not: a consonant against a vowel.
        (Some(_), None) | (None, Some(_)) => c += cfg.voicing,
        (None, None) => {}
    }
    c
}

/// Step directions, and the back-pointer values that record them.
const DIAGONAL: u8 = 0;
/// Dub advanced, reference did not: the dub is being compressed.
const COMPRESS: u8 = 1;
/// Reference advanced, dub did not: the dub is being stretched.
const STRETCH: u8 = 2;

/// Match `dub` against `reference`, returning for each dub frame the
/// reference frame it belongs on.
///
/// `offset_frames` is the macro lag from [`crate::offset`], expressed in
/// frames: the reference frame that dub frame zero is expected to land
/// on. Pass `0.0` when no offset search has been run.
pub fn warp(
    dub: &[Frame],
    reference: &[Frame],
    frame_rate: f64,
    offset_frames: f64,
    warp_cfg: &WarpConfig,
    cost_cfg: &CostConfig,
) -> Vec<f64> {
    let (n, m) = (dub.len(), reference.len());
    if n == 0 || m == 0 {
        return Vec::new();
    }
    let cost_cfg = cost_cfg.normalized();
    let band = ((warp_cfg.band_secs * frame_rate).round() as usize).max(1);

    // The band's centre line: start at the macro offset, and lean toward
    // the far corner at whatever rate the two lengths imply, so a dub
    // that runs slightly long stays inside the band to the end.
    let scale = if n > 1 {
        ((m as f64 - 1.0 - offset_frames) / (n as f64 - 1.0)).clamp(0.5, 2.0)
    } else {
        1.0
    };
    let last_reference = m as isize - 1;
    let ideal = move |i: usize| (offset_frames + i as f64 * scale).round() as isize;
    // The centre held inside the reference, which is what the rolling
    // window is indexed against. A dub that begins a second before the
    // reference does has an ideal centre off the matrix entirely for its
    // first second, and a row with no cells in range leaves every later
    // row unreachable — the traceback then walks a matrix of infinities
    // and returns nonsense.
    let centre = move |i: usize| ideal(i).clamp(0, last_reference);

    let width = band * 2 + 1;
    let index = move |i: usize, j: isize| -> Option<usize> {
        let offset = j - centre(i) + band as isize;
        (offset >= 0 && (offset as usize) < width).then_some(offset as usize)
    };
    // The searchable range for a row, from the *unclamped* centre. This
    // is the part that matters: those leading frames are not merely
    // "somewhere near the start of the reference", they are all before it
    // began, and the only place they can go is its first frame. Given a
    // full band instead, the path wanders forward through the reference
    // while the dub is still silent, and then cannot come back — DTW is
    // monotonic, so an early over-advance is permanent, and the whole
    // take lands late by however far it strayed.
    let range = move |i: usize| -> (isize, isize) {
        let lo = (ideal(i) - band as isize).clamp(0, last_reference);
        let hi = (ideal(i) + band as isize).clamp(0, last_reference);
        (lo.min(hi), hi.max(lo))
    };

    let infinity = f64::INFINITY;
    // Rolling two rows: the full matrix for a five-minute take would be
    // gigabytes, and only the back-pointers are needed afterwards.
    let mut previous = vec![infinity; width];
    let mut current = vec![infinity; width];
    let mut back = vec![DIAGONAL; n * width];
    // Where the dub actually starts sounding. Zero when it starts
    // immediately, which makes the rule below reduce to a free first
    // frame — the behaviour a path with no lead-in wants anyway.
    let first_sound = dub.iter().position(|f| !f.silent).unwrap_or(0);

    for i in 0..n {
        current.iter_mut().for_each(|c| *c = infinity);
        let (lo, hi) = range(i);
        for j in lo..=hi {
            let Some(o) = index(i, j) else { continue };
            // The tiebreak is measured against *not warping* — the dub
            // where the macro offset already puts it — and not against
            // the band's centre. The centre leans with the length ratio,
            // which spreads a difference in total length evenly across
            // the take, so biasing toward it would argue for a uniform
            // compression when the truth is that the dub simply came in
            // late and the correction belongs at the front.
            //
            // Measuring against bare `i` instead, as this did while the
            // macro stage did not exist, is worse than useless once it
            // does: for a dub a second late, every correct cell is a
            // second away from `i`, so the bias pulls the path *off* the
            // offset it was just handed — far enough, on anything as
            // repetitive as a sung phrase, to settle a syllable out.
            let drift = ((j as f64) - (i as f64 + offset_frames)).abs() / band as f64;
            let c = cost(dub[i], reference[j as usize], &cost_cfg) + warp_cfg.diagonal_bias * drift;

            let previous_row = i.checked_sub(1);
            let diagonal = previous_row
                .and_then(|p| index(p, j - 1))
                .map(|p| previous[p])
                .unwrap_or(infinity);
            // Compressing consumes this dub frame; stretching consumes
            // this reference frame. Which one it is decides whose silence
            // makes the step cheap.
            let compress = previous_row
                .and_then(|row| index(row, j).map(|p| (row, p)))
                .map(|(row, p)| {
                    previous[p]
                        + step_penalty(warp_cfg, dub[i].silent, back[row * width + p] == COMPRESS)
                })
                .unwrap_or(infinity);
            let stretch = if j > lo {
                index(i, j - 1)
                    .map(|p| {
                        current[p]
                            + step_penalty(
                                warp_cfg,
                                reference[j as usize].silent,
                                back[i * width + p] == STRETCH,
                            )
                    })
                    .unwrap_or(infinity)
            } else {
                infinity
            };

            let (mut best, direction) = if diagonal <= compress && diagonal <= stretch {
                (diagonal, DIAGONAL)
            } else if compress <= stretch {
                (compress, COMPRESS)
            } else {
                (stretch, STRETCH)
            };
            // Open begin, over the dub's silent lead-in only.
            //
            // A path forced to start at the dub's first frame has to
            // account for every frame before the singer comes in, and
            // those frames have no partner — there is nothing in the
            // reference they correspond to. Charged for them, the search
            // does the rational thing and goes looking for somewhere
            // cheap to put them, and the cheapest place is the
            // reference's first *gap between phrases* — which lands the
            // dub's opening syllable on the reference's second one, an
            // alignment that is perfectly optimal and entirely wrong.
            //
            // Letting the path simply begin where the audio does removes
            // the pressure rather than penalising the symptom. It is the
            // standard open-begin DTW, restricted to the run of leading
            // silence so that it cannot be used to skip a real phrase.
            if i <= first_sound {
                best = best.min(0.0);
            }
            if best.is_finite() {
                current[o] = best + c;
                back[i * width + o] = direction;
            }
        }
        core::mem::swap(&mut previous, &mut current);
    }

    trace_back(&back, &previous, n, width, range, index)
}

fn step_penalty(cfg: &WarpConfig, consumed_is_silent: bool, repeats: bool) -> f64 {
    let mut penalty = cfg.slope_penalty;
    if repeats {
        penalty *= cfg.staircase_penalty;
    }
    if consumed_is_silent {
        penalty *= cfg.silence_discount;
    }
    penalty
}

/// Walk the back-pointers from the cheapest end cell.
fn trace_back(
    back: &[u8],
    last_row: &[f64],
    n: usize,
    width: usize,
    range: impl Fn(usize) -> (isize, isize),
    index: impl Fn(usize, isize) -> Option<usize>,
) -> Vec<f64> {
    let last = n - 1;
    let (lo, hi) = range(last);
    let mut best_j = hi;
    let mut best = f64::INFINITY;
    for j in lo..=hi {
        if let Some(o) = index(last, j).filter(|&o| last_row[o] < best) {
            best = last_row[o];
            best_j = j;
        }
    }

    let mut map = vec![0.0; n];
    let mut i = last;
    let mut j = best_j;
    loop {
        map[i] = j as f64;
        if i == 0 {
            break;
        }
        let o = index(i, j).unwrap_or(0);
        match back[i * width + o] {
            DIAGONAL => {
                i -= 1;
                j = (j - 1).max(0);
            }
            COMPRESS => i -= 1,
            _ => {
                // Several reference frames for one dub frame: the map
                // keeps the last, and the frames between are covered by
                // the interpolation between warp points.
                j = (j - 1).max(0);
            }
        }
    }
    // A time map that goes backwards renders as a stutter. Back-pointer
    // walks can produce a flat or fractionally backward step at the band
    // edges, so this is enforced rather than assumed.
    for k in 1..map.len() {
        if map[k] < map[k - 1] {
            map[k] = map[k - 1];
        }
    }
    map
}
