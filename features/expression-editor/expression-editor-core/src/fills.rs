//! Finding the fills in a drum take.
//!
//! A fill is where the drummer stops keeping time and plays something,
//! and it is the one part of a take that must not be quantized like the
//! rest of it. Groove wants to sit on the grid; a fill is often played
//! across the grid on purpose — a triplet run, a drag into the
//! downbeat, a rushed crescendo — and flattening it onto sixteenths
//! takes the performance out. So the editor needs to know where the
//! fills are before it quantizes anything, in order to leave them alone
//! or give them settings of their own.
//!
//! # What separates a fill from the groove
//!
//! Two things, and neither alone is enough.
//!
//! **Toms.** A rock groove is kick, snare and hats; the toms sit unused
//! for whole songs and then carry the fill. Tom activity is therefore
//! the single strongest signal, and it is now available per drum
//! (`kit::detection_units`) rather than as one summed "a tom was hit".
//!
//! **Density.** Not every fill uses toms — a snare roll or a stretch of
//! kick sixteenths is a fill too — so a bar that is simply much busier
//! than the song's habit counts as well.
//!
//! # Against the song's own habit, never a fixed number
//!
//! "More than six tom hits in a bar" is a rule that works on one song.
//! A track with a tom-driven groove would be marked as one continuous
//! fill, and a sparse ballad's one real fill would be missed. So every
//! bar is scored against the **median** bar of that same take.
//!
//! The median, not the mean, and the median absolute deviation, not the
//! standard deviation: fills are precisely the outliers being looked
//! for, and an average is dragged towards whatever it is meant to be
//! detecting. On a song with four fills in sixty bars a mean tom count
//! is pulled up by the very bars that should stand out, and the
//! threshold quietly rises to hide them.
//!
//! Spec: `features/expression-editor/spec/drum-mode.md` (`drums.fills.*`).

use crate::kit::LaneRole;

/// How a bar has to behave before it counts as a fill.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillConfig {
    /// How far above the take's own median a bar must score.
    ///
    /// In units of the take's spread — see [`excess`] — so it means the
    /// same thing on a busy song and a sparse one.
    ///
    /// Chosen by `--example calibrate`, which sweeps it against the
    /// album and scores each value on how much more often its fills sit
    /// at a section boundary than a bar picked at random does.
    ///
    /// That score keeps rising as the threshold does — 2.26x here,
    /// 2.54x at 7.0, 2.81x at 10.0 — but it is a precision proxy with
    /// no recall term, so it rewards reporting fewer and safer fills
    /// until barely any are left (four a song at 10.0). The argmax is
    /// the wrong thing to take.
    ///
    /// The tie is broken on the cost of each mistake, which is not
    /// symmetric. A fill that is missed gets quantized like groove and
    /// the performance in it is flattened — the damage this whole
    /// module exists to prevent. A bar wrongly called a fill is merely
    /// left alone. So the threshold sits where the fill count is still
    /// musically plausible, about ten a song, rather than where the
    /// proxy peaks.
    pub threshold: f64,
    /// How much a bar's overall busyness counts next to its tom
    /// activity. Toms lead because a groove rarely uses them; density
    /// is the corroborating voice that catches a tomless fill.
    pub density_weight: f64,
    /// Bars of groove that may sit inside one fill without splitting it
    /// in two. A fill that breathes for a beat is still one fill.
    pub join_gap_bars: usize,
    /// Transient-detector sensitivity for counting bar activity.
    ///
    /// Not the quantize panel's. The panel *moves* every hit it
    /// reports, so a false one damages the take and it is tuned for
    /// precision; counting how busy a bar was wants recall, where a
    /// missed hit is the costly error and a spurious one is noise the
    /// median absorbs.
    ///
    /// Lives here beside [`FillConfig::threshold`] because the two are
    /// coupled: more sensitive detection raises every bar's score, so a
    /// threshold tuned at one sensitivity is wrong at another. They have
    /// to be chosen together, which is what `--example calibrate` does.
    pub detect_sensitivity: f64,
    /// Find hits with the hybrid detector (spectral flux against a
    /// moving median, refined by the envelope) rather than the envelope
    /// gate, ignoring [`FillConfig::detect_sensitivity`].
    ///
    /// The two are a tie for *placing* hits, which is what the gate is
    /// tuned for. Counting them is a different question: the flux stage
    /// scores each frame against its own neighbourhood, so it needs no
    /// sensitivity set per song, and a bar count wants exactly that
    /// kind of robustness rather than millisecond precision.
    pub hybrid_detect: bool,
}

impl Default for FillConfig {
    fn default() -> Self {
        Self {
            threshold: 5.0,
            density_weight: 0.5,
            join_gap_bars: 0,
            // The detector's calibrated optimum against the drum MIDI,
            // under the standard one-to-one matching rule at a ±25ms
            // window: F1 peaks here at 0.345 and falls either side
            // (0.293 at 0.80, 0.286 at 0.98).
            //
            // Arrived at twice over, which is the reassuring part. It
            // was first settled empirically because fill detection
            // wants *contrast* between bars rather than raw recall —
            // pushing higher adds marginal hits to groove and fill
            // alike, lifting the median as much as the outliers, and at
            // 0.95 two of the six album projects lost every
            // section-aligned fill. The onset-detection optimum turned
            // out to be the same place.
            detect_sensitivity: 0.9,
            hybrid_detect: false,
        }
    }
}

/// A stretch of the take that plays as a fill.
#[derive(Clone, Debug, PartialEq)]
pub struct Fill {
    /// Seconds, from the start of the first bar it covers.
    pub start: f64,
    /// Seconds, to the end of the last bar it covers.
    pub end: f64,
    /// Bar indices covered, as `[first, last]` inclusive — what a UI
    /// needs to say "bars 15–16" rather than a pair of timestamps.
    pub bars: (usize, usize),
    /// The strongest bar score inside it, for ranking when a take has
    /// more fills than the user wants to look at.
    pub score: f64,
}

/// How far `x` sits above `median`, in units of the sample's own spread.
///
/// The spread is floored at one hit so a take whose bars are all
/// identical — every bar exactly zero toms, which is the common case —
/// does not divide by zero. There the score reads directly as "this
/// many hits more than a normal bar", which is the right thing for it
/// to mean when the song has no variation to measure against.
fn excess(x: f64, median: f64, spread: f64) -> f64 {
    (x - median) / spread.max(1.0)
}

/// The median of `xs`, which is left unsorted.
fn median(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut v = xs.to_vec();
    v.sort_by(f64::total_cmp);
    let mid = v.len() / 2;
    if v.len() % 2 == 0 {
        (v[mid - 1] + v[mid]) / 2.0
    } else {
        v[mid]
    }
}

/// Median absolute deviation — the robust answer to "how much do these
/// vary", unmoved by the outliers this module exists to find.
fn mad(xs: &[f64], med: f64) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let devs: Vec<f64> = xs.iter().map(|x| (x - med).abs()).collect();
    median(&devs)
}

/// Score every bar for how much it behaves like a fill.
///
/// `bars` are bar start times in seconds with a final entry for the end
/// of the last bar, so `n` bars need `n + 1` boundaries. The caller owns
/// the tempo map: a take with a time-signature change has uneven bars
/// and only the host knows where they fall.
///
/// Returned scores line up with the bars, one each.
pub fn bar_scores(bars: &[f64], hits: &[(f64, LaneRole)], cfg: &FillConfig) -> Vec<f64> {
    if bars.len() < 2 {
        return Vec::new();
    }
    let n = bars.len() - 1;
    let mut toms = vec![0.0f64; n];
    let mut total = vec![0.0f64; n];
    for &(at, role) in hits {
        // `partition_point` puts a hit in the bar it starts in; a hit
        // exactly on a boundary belongs to the bar it opens.
        let i = bars.partition_point(|&b| b <= at);
        if i == 0 || i > n {
            continue;
        }
        let bar = i - 1;
        total[bar] += 1.0;
        if role == LaneRole::Toms {
            toms[bar] += 1.0;
        }
    }

    let (tm, tt) = (median(&toms), median(&total));
    let (ts, ds) = (mad(&toms, tm), mad(&total, tt));
    (0..n)
        .map(|i| {
            let tom = excess(toms[i], tm, ts);
            let dense = excess(total[i], tt, ds);
            // Only *busier* than normal reads as a fill. A bar quieter
            // than the median is a break, not a fill, and letting it
            // score negatively would drag down a genuine tom run that
            // happens to drop the kick.
            tom.max(0.0) + cfg.density_weight * dense.max(0.0)
        })
        .collect()
}

/// The fills in a take: runs of bars scoring above the threshold.
///
/// See [`bar_scores`] for the meaning of `bars`.
// r[impl drums.fills.detect]
pub fn detect_fills(bars: &[f64], hits: &[(f64, LaneRole)], cfg: &FillConfig) -> Vec<Fill> {
    let scores = bar_scores(bars, hits, cfg);
    let mut out: Vec<Fill> = Vec::new();
    for (i, &s) in scores.iter().enumerate() {
        if s < cfg.threshold {
            continue;
        }
        // Extend the fill in progress if this bar is near enough to it,
        // otherwise start a new one.
        match out.last_mut() {
            Some(f) if i <= f.bars.1 + cfg.join_gap_bars + 1 => {
                f.bars.1 = i;
                f.end = bars[i + 1];
                f.score = f.score.max(s);
            }
            _ => out.push(Fill {
                start: bars[i],
                end: bars[i + 1],
                bars: (i, i),
                score: s,
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bar boundaries for `n` bars of `len` seconds.
    fn bars(n: usize, len: f64) -> Vec<f64> {
        (0..=n).map(|i| i as f64 * len).collect()
    }

    /// A plain rock bar: kick and snare, no toms.
    fn groove(at: f64, out: &mut Vec<(f64, LaneRole)>) {
        for (i, role) in [
            LaneRole::Kick,
            LaneRole::Snare,
            LaneRole::Kick,
            LaneRole::Snare,
        ]
        .into_iter()
        .enumerate()
        {
            out.push((at + i as f64 * 0.5, role));
        }
    }

    /// A tom fill: a run across the kit.
    fn tom_fill(at: f64, out: &mut Vec<(f64, LaneRole)>) {
        for i in 0..8 {
            out.push((at + i as f64 * 0.25, LaneRole::Toms));
        }
    }

    /// Sixteen bars of groove with a tom fill in the bar `fill_at`.
    fn take_with_a_fill(fill_at: usize) -> (Vec<f64>, Vec<(f64, LaneRole)>) {
        let mut hits = Vec::new();
        for bar in 0..16 {
            let at = bar as f64 * 2.0;
            if bar == fill_at {
                tom_fill(at, &mut hits);
            } else {
                groove(at, &mut hits);
            }
        }
        (bars(16, 2.0), hits)
    }

    // r[verify drums.fills.detect]
    #[test]
    fn a_tom_fill_stands_out_from_the_groove() {
        let (b, h) = take_with_a_fill(7);
        let fills = detect_fills(&b, &h, &FillConfig::default());
        assert_eq!(fills.len(), 1, "expected exactly the one fill: {fills:?}");
        assert_eq!(fills[0].bars, (7, 7));
        assert_eq!(fills[0].start, 14.0);
        assert_eq!(fills[0].end, 16.0);
    }

    // r[verify drums.fills.detect]
    #[test]
    fn a_take_that_is_all_groove_has_no_fills() {
        // The detector must be able to say "none". Scoring against the
        // take's own median means a uniform take has nothing above it,
        // where a fixed threshold would either fire on every bar or
        // never fire at all.
        let mut hits = Vec::new();
        for bar in 0..16 {
            groove(bar as f64 * 2.0, &mut hits);
        }
        assert!(detect_fills(&bars(16, 2.0), &hits, &FillConfig::default()).is_empty());
    }

    // r[verify drums.fills.detect]
    #[test]
    fn a_tom_groove_is_not_one_long_fill() {
        // The reason the baseline is the take's own median. A song
        // played on the toms throughout has a high tom count in *every*
        // bar, so no bar is unusual and none of it is a fill. A fixed
        // "more than N toms" rule would mark the whole song.
        let mut hits = Vec::new();
        for bar in 0..16 {
            let at = bar as f64 * 2.0;
            for i in 0..8 {
                hits.push((at + i as f64 * 0.25, LaneRole::Toms));
            }
        }
        assert!(
            detect_fills(&bars(16, 2.0), &hits, &FillConfig::default()).is_empty(),
            "a tom-driven groove is a groove"
        );
    }

    // r[verify drums.fills.detect]
    #[test]
    fn a_snare_roll_counts_even_with_no_toms() {
        // Density is the corroborating signal: not every fill reaches
        // for the toms.
        let mut hits = Vec::new();
        for bar in 0..16 {
            let at = bar as f64 * 2.0;
            if bar == 11 {
                for i in 0..16 {
                    hits.push((at + i as f64 * 0.125, LaneRole::Snare));
                }
            } else {
                groove(at, &mut hits);
            }
        }
        let fills = detect_fills(&bars(16, 2.0), &hits, &FillConfig::default());
        assert_eq!(fills.len(), 1, "the roll is a fill: {fills:?}");
        assert_eq!(fills[0].bars, (11, 11));
    }

    // r[verify drums.fills.detect]
    #[test]
    fn two_adjacent_fill_bars_are_one_fill() {
        let mut hits = Vec::new();
        for bar in 0..16 {
            let at = bar as f64 * 2.0;
            if bar == 6 || bar == 7 {
                tom_fill(at, &mut hits);
            } else {
                groove(at, &mut hits);
            }
        }
        let fills = detect_fills(&bars(16, 2.0), &hits, &FillConfig::default());
        assert_eq!(fills.len(), 1, "a two-bar fill is one fill: {fills:?}");
        assert_eq!(fills[0].bars, (6, 7));
        assert_eq!(fills[0].end, 16.0);
    }

    // r[verify drums.fills.detect]
    #[test]
    fn separate_fills_stay_separate() {
        let mut hits = Vec::new();
        for bar in 0..16 {
            let at = bar as f64 * 2.0;
            if bar == 3 || bar == 11 {
                tom_fill(at, &mut hits);
            } else {
                groove(at, &mut hits);
            }
        }
        let fills = detect_fills(&bars(16, 2.0), &hits, &FillConfig::default());
        assert_eq!(fills.len(), 2, "{fills:?}");
        assert_eq!(fills[0].bars, (3, 3));
        assert_eq!(fills[1].bars, (11, 11));
    }

    // r[verify drums.fills.detect]
    #[test]
    fn a_quiet_bar_is_a_break_not_a_fill() {
        // A bar with *fewer* hits than usual is the drummer dropping
        // out. Letting density score negatively both ways would call it
        // a fill on the strength of being unusual.
        let mut hits = Vec::new();
        for bar in 0..16 {
            let at = bar as f64 * 2.0;
            if bar == 9 {
                hits.push((at, LaneRole::Kick));
            } else {
                groove(at, &mut hits);
            }
        }
        assert!(detect_fills(&bars(16, 2.0), &hits, &FillConfig::default()).is_empty());
    }

    #[test]
    fn uneven_bars_are_the_callers_to_describe() {
        // A time-signature change makes bars different lengths, and the
        // host owns the tempo map. Passing boundaries rather than a bar
        // length is what lets `set in stone` — 6/8 with a 7/4 section —
        // be scored at all.
        let b = vec![0.0, 2.0, 4.0, 7.5, 9.5];
        let mut hits = Vec::new();
        for at in [0.0, 2.0, 9.5] {
            groove(at, &mut hits);
        }
        tom_fill(4.0, &mut hits);
        let fills = detect_fills(&b, &hits, &FillConfig::default());
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].start, 4.0);
        assert_eq!(fills[0].end, 7.5, "the long bar keeps its own length");
    }
}
