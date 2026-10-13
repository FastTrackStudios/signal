//! Turning a per-frame path into the few points that get written.
//!
//! A DTW path says something about every frame, and almost all of it is
//! noise. Two takes agree about *where the syllable starts*; they have no
//! shared opinion about which millisecond of a held vowel corresponds to
//! which, and a warp that acts on the middle of a vowel is inventing
//! detail. Writing the path out frame by frame therefore does three bad
//! things at once: it produces thousands of warp points nobody can edit,
//! it bends the inside of every note by whatever the matcher happened to
//! decide, and it makes the result impossible to reason about.
//!
//! So the path is reduced to **anchors**: the moments where both takes
//! agree that something *started*, plus the two ends. Between anchors the
//! map is a straight line, which spreads each correction across the
//! sustain and the gap that follow it — where there is nothing whose
//! timing anyone can hear.
//!
//! ## The three filters
//!
//! An anchor survives only if all three hold, and each rejects a
//! different way of being wrong:
//!
//! - **Confidence.** Both takes must show an onset at the paired frames.
//!   An onset in the reference matched to the middle of a dub vowel is
//!   the matcher guessing, and acting on it moves audio for no reason.
//! - **Spacing.** Two anchors closer together than
//!   [`AnchorConfig::min_gap_secs`] describe the same event twice, and
//!   the segment between them is short enough that a small disagreement
//!   becomes a large stretch ratio.
//! - **Ratio.** The stretch between consecutive anchors must stay inside
//!   `1/S..=S`. This is the bound the warp search deliberately does not
//!   impose — here it is a statement about audio that will actually be
//!   produced, and a segment outside it will sound wrong however good its
//!   matching score was.

use crate::features::Frame;

/// What put an anchor where it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorKind {
    /// The first frame. Holds the correction back to the take's start.
    Start,
    /// A moment both takes agree began.
    Onset,
    /// The last frame. Holds the correction on to the take's end.
    End,
}

/// One point where the dub is pinned to the reference.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    /// Dub frame.
    pub dub: usize,
    /// Reference frame it should land on, after `strength` and the shift
    /// cap have been applied.
    pub reference: f64,
    /// `0.0..=1.0`. How much both takes agreed that something began here.
    pub confidence: f64,
    pub kind: AnchorKind,
}

impl Anchor {
    /// Correction in frames: positive means the dub moves later.
    pub fn shift(&self) -> f64 {
        self.reference - self.dub as f64
    }
}

/// How the path is reduced.
#[derive(Clone, Copy, Debug)]
pub struct AnchorConfig {
    /// Onset strength a frame needs before it is a candidate at all.
    pub onset_strength: f64,
    /// Agreement between the two takes an anchor needs to survive.
    pub min_confidence: f64,
    /// Closest two anchors may be, in seconds.
    pub min_gap_secs: f64,
    /// Largest stretch ratio between consecutive anchors. `1.8` means a
    /// segment may play at most 1.8× longer or shorter than it was.
    pub max_stretch_ratio: f64,
    /// Widest correction, in seconds, **beyond the global offset**.
    ///
    /// A promise about the result rather than about the search: the band
    /// is around the offset-and-length-scaled diagonal so two takes of
    /// different lengths can match at all, and a match inside that band
    /// can still be further than the user agreed to allow.
    ///
    /// The global offset is exempt because it is not a correction to the
    /// performance — it is where the take *is*. A dub that came in a
    /// second and a half late must be allowed to move a second and a
    /// half, while still being held to a few hundred milliseconds of
    /// warping once it is in the right place.
    pub max_shift_secs: f64,
    /// How much of the correction is applied. `1.0` lands exactly on the
    /// reference; less leaves some of the dub's own feel.
    pub strength: f64,
    /// Spacing of the fallback anchors, in seconds, used when the
    /// material has no onsets to find.
    pub fallback_spacing_secs: f64,
}

impl Default for AnchorConfig {
    fn default() -> Self {
        Self {
            onset_strength: 0.12,
            min_confidence: 0.20,
            min_gap_secs: 0.060,
            max_stretch_ratio: 1.8,
            max_shift_secs: 0.5,
            strength: 1.0,
            fallback_spacing_secs: 0.25,
        }
    }
}

/// How much a frame looks like the start of something.
fn onset_strength(frames: &[Frame], i: usize) -> f64 {
    frames
        .get(i)
        .map(|f| 0.6 * f.flux + 0.4 * f.delta)
        .unwrap_or(0.0)
}

/// The strongest onset within one frame either side.
///
/// The path maps a dub frame to a reference frame that may be a frame out
/// either way from rounding alone, and an onset is one or two frames
/// wide, so asking about a single frame would reject good anchors on a
/// technicality.
fn onset_strength_near(frames: &[Frame], centre: f64) -> f64 {
    let c = centre.round() as isize;
    (-1..=1)
        .filter_map(|d| usize::try_from(c + d).ok())
        .map(|i| onset_strength(frames, i))
        .fold(0.0, f64::max)
}

/// Reduce a frame-by-frame path to the points worth writing.
pub fn anchors(
    map: &[f64],
    dub: &[Frame],
    reference: &[Frame],
    frame_rate: f64,
    offset_frames: f64,
    cfg: AnchorConfig,
) -> Vec<Anchor> {
    if map.is_empty() {
        return Vec::new();
    }
    let strength = cfg.strength.clamp(0.0, 1.0);
    let cap = cfg.max_shift_secs.max(0.0) * frame_rate.max(1e-9);
    let min_gap = (cfg.min_gap_secs * frame_rate).max(1.0);

    // Corrected target for a dub frame. Strength and the cap apply to the
    // warp only — the part of the move that is a judgement about the
    // performance. The global offset passes through both untouched,
    // because it is a fact about where the take sits.
    //
    // Not clamped to zero, either: a negative target means the dub must
    // move earlier than the reference's own first frame, which is the
    // right answer for a dub recorded late. Clamping here would flatten
    // the shift near the start into a stretch, turning a rigid move into
    // a warp. Where the take's start actually lies is the host's problem.
    let target = |i: usize| -> f64 {
        let source = i as f64 + offset_frames;
        source + (map[i] - source).clamp(-cap, cap) * strength
    };

    let last = map.len() - 1;
    let mut candidates: Vec<Anchor> = vec![Anchor {
        dub: 0,
        reference: target(0),
        confidence: 1.0,
        kind: AnchorKind::Start,
    }];

    let mut previous_dub = 0usize;
    for (i, &mapped) in map.iter().enumerate().take(last).skip(1) {
        let strength_dub = onset_strength(dub, i);
        if strength_dub < cfg.onset_strength {
            continue;
        }
        // A local maximum, so one onset yields one anchor rather than one
        // per frame of its attack.
        if strength_dub < onset_strength(dub, i + 1) || strength_dub <= onset_strength(dub, i - 1) {
            continue;
        }
        let strength_reference = onset_strength_near(reference, mapped);
        // The weaker of the two: an onset only one take can see is not
        // agreement, and acting on it moves audio on one take's evidence.
        let agreement = strength_dub.min(strength_reference);
        let confidence = (agreement / cfg.onset_strength.max(1e-9)).clamp(0.0, 1.0);
        if confidence < cfg.min_confidence {
            continue;
        }
        if ((i - previous_dub) as f64) < min_gap {
            continue;
        }
        previous_dub = i;
        candidates.push(Anchor {
            dub: i,
            reference: target(i),
            confidence,
            kind: AnchorKind::Onset,
        });
    }

    if last > 0 {
        candidates.push(Anchor {
            dub: last,
            reference: target(last),
            confidence: 1.0,
            kind: AnchorKind::End,
        });
    }

    let kept = enforce_ratio(candidates, cfg.max_stretch_ratio);

    // Material with no usable onsets — a pad, a bowed note, a held vowel —
    // would otherwise get two anchors and a single straight line across
    // the whole take, throwing away everything the match found. Sample
    // the path instead: there is no moment worth pinning, but the shape
    // of the correction is still real.
    //
    // Tested on what *survives*, not on what was proposed. A sustained
    // take that opens with an attack offers exactly one candidate, the
    // ratio check drops it for spanning no reference time, and a
    // fallback that had already decided not to run leaves the take with
    // nothing between its two ends.
    if kept.len() <= 2 && last > 0 {
        return enforce_ratio(
            fallback_anchors(map, frame_rate, cfg, target),
            cfg.max_stretch_ratio,
        );
    }
    kept
}

fn fallback_anchors(
    map: &[f64],
    frame_rate: f64,
    cfg: AnchorConfig,
    target: impl Fn(usize) -> f64,
) -> Vec<Anchor> {
    let last = map.len() - 1;
    let step = ((cfg.fallback_spacing_secs * frame_rate).round() as usize).max(1);
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        out.push(Anchor {
            dub: i,
            reference: target(i),
            confidence: 0.0,
            kind: if i == 0 {
                AnchorKind::Start
            } else if i == last {
                AnchorKind::End
            } else {
                AnchorKind::Onset
            },
        });
        if i == last {
            break;
        }
        i = (i + step).min(last);
    }
    out
}

/// Drop or clamp anchors whose segment would stretch further than allowed.
///
/// Interior anchors are dropped — the segment either side of a rejected
/// anchor simply merges, which is a smooth result. The final anchor
/// cannot be dropped without changing the length of the take, so it is
/// clamped to the limit instead.
fn enforce_ratio(candidates: Vec<Anchor>, max_ratio: f64) -> Vec<Anchor> {
    let max_ratio = max_ratio.max(1.0);
    let min_ratio = 1.0 / max_ratio;
    let mut out: Vec<Anchor> = Vec::with_capacity(candidates.len());

    for (k, mut anchor) in candidates.into_iter().enumerate() {
        let Some(previous) = out.last().copied() else {
            out.push(anchor);
            continue;
        };
        let _ = k;
        let span_dub = anchor.dub as f64 - previous.dub as f64;
        let span_reference = anchor.reference - previous.reference;
        if span_dub <= 0.0 {
            continue;
        }
        let ratio = span_reference / span_dub;
        if ratio >= min_ratio && ratio <= max_ratio {
            out.push(anchor);
            continue;
        }
        if anchor.kind == AnchorKind::End {
            // Clamp: the take must still end where the map says it ends,
            // so the correction is reduced rather than abandoned.
            let clamped = ratio.clamp(min_ratio, max_ratio);
            anchor.reference = previous.reference + span_dub * clamped;
            out.push(anchor);
        }
        // Interior anchors that fail are dropped, and the next one is
        // measured from the same previous anchor.
    }
    out
}

/// Expand anchors back into a map covering every dub frame.
///
/// Straight lines between anchors; outside the first and last, the
/// correction is *held* rather than allowed to decay to zero. A lead-in
/// dragged back to "no correction" would put a breath, or the swell into
/// a cymbal, out of step with the note it leads into.
pub fn map_from_anchors(anchors: &[Anchor], frames: usize) -> Vec<f64> {
    if frames == 0 {
        return Vec::new();
    }
    if anchors.is_empty() {
        return (0..frames).map(|i| i as f64).collect();
    }

    let mut map = Vec::with_capacity(frames);
    let mut segment = 0usize;
    for i in 0..frames {
        let x = i as f64;
        while segment + 1 < anchors.len() && (anchors[segment + 1].dub as f64) < x {
            segment += 1;
        }
        let a = anchors[segment];
        let b = anchors[(segment + 1).min(anchors.len() - 1)];
        let (x0, x1) = (a.dub as f64, b.dub as f64);
        let shift = if (x1 - x0).abs() < 1e-9 {
            a.shift()
        } else {
            let t = ((x - x0) / (x1 - x0)).clamp(0.0, 1.0);
            a.shift() + (b.shift() - a.shift()) * t
        };
        map.push(x + shift);
    }

    // Interpolation between monotonic anchors cannot go backwards, but a
    // ratio clamp on the final anchor can leave one flat step.
    for i in 1..map.len() {
        if map[i] < map[i - 1] {
            map[i] = map[i - 1];
        }
    }
    map
}
