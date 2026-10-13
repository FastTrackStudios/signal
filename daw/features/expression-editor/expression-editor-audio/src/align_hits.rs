//! Aligning percussion by its hits.
//!
//! [`mod@crate::align`] matches two takes frame by frame on energy, with
//! pitch as a weak cue. It works on a kit — energy is exactly what a kit
//! has — but the parts of it that make it *good* on a vocal do nothing
//! here: pairing voiced spans at zero cost matters when there are voiced
//! spans, and a drum track is unvoiced from end to end.
//!
//! What percussion has instead is a short, unambiguous list of moments:
//! the onsets. Matching two such lists is a smaller problem than warping
//! two frame sequences, and the answer is better — a hit either
//! corresponds to a hit in the reference or it does not, and there is no
//! useful sense in which the *middle* of a decay corresponds to
//! anything.
//!
//! The result is the same [`Alignment`] the frame-based path produces,
//! so it feeds the same warp markers and the same write path.

use crate::align::Alignment;
use crate::onsets::Onset;

/// How hits are matched.
#[derive(Clone, Copy, Debug)]
pub struct HitAlignConfig {
    /// Furthest a dub hit may be moved, in seconds. A promise about the
    /// result, not a hint to the search: a hit further from its
    /// reference than this is treated as having no partner rather than
    /// being dragged.
    pub max_shift_secs: f64,
    /// How much of the correction to apply, `0.0..=1.0`.
    pub strength: f64,
}

impl Default for HitAlignConfig {
    fn default() -> Self {
        Self {
            max_shift_secs: 0.05,
            strength: 1.0,
        }
    }
}

/// One dub hit and the reference hit it belongs to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pair {
    /// Dub frame.
    pub dub: usize,
    /// Reference frame it should land on.
    pub reference: f64,
}

/// Match dub hits to reference hits.
///
/// Nearest-neighbour, but each reference hit is claimed at most once and
/// the closest pairs claim first. Straight nearest-neighbour lets one
/// reference hit absorb a whole roll — every dub hit in a flam or a
/// buzz points at the same target, and the retiming collapses them on
/// top of each other. Claiming in distance order means the hit that
/// really is that reference hit wins, and the rest are left unpaired
/// (so unmoved), which is much the better failure.
pub fn pair_hits(
    reference: &[Onset],
    dub: &[Onset],
    cfg: HitAlignConfig,
    frame_rate: f64,
) -> Vec<Pair> {
    if reference.is_empty() || dub.is_empty() {
        return Vec::new();
    }
    let limit = cfg.max_shift_secs.max(0.0) * frame_rate.max(1e-9);

    // Every candidate pairing inside the limit, closest first.
    let mut candidates: Vec<(f64, usize, usize)> = Vec::new();
    for (d, dub_hit) in dub.iter().enumerate() {
        for (r, ref_hit) in reference.iter().enumerate() {
            let distance = (ref_hit.frame as f64 - dub_hit.frame as f64).abs();
            if distance <= limit {
                candidates.push((distance, d, r));
            }
        }
    }
    candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut dub_taken = vec![false; dub.len()];
    let mut ref_taken = vec![false; reference.len()];
    let mut pairs: Vec<Pair> = Vec::new();
    for (_, d, r) in candidates {
        if dub_taken[d] || ref_taken[r] {
            continue;
        }
        dub_taken[d] = true;
        ref_taken[r] = true;
        pairs.push(Pair {
            dub: dub[d].frame,
            reference: reference[r].frame as f64,
        });
    }
    // Crossings would render as a stutter: two hits swapping order means
    // the time map goes backwards between them. Dropping the later of
    // any inverted pair is enough, because pairs are already
    // distance-ranked and the survivor is the more confident match.
    pairs.sort_by_key(|p| p.dub);
    let mut monotonic: Vec<Pair> = Vec::with_capacity(pairs.len());
    for p in pairs {
        if monotonic
            .last()
            .is_some_and(|last| p.reference <= last.reference)
        {
            continue;
        }
        monotonic.push(p);
    }
    monotonic
}

/// Align a dub's hits to a reference's.
///
/// `dub_frames` is how many frames the dub's map must cover — the
/// alignment has to describe *every* frame, not only the ones with hits,
/// because the renderer walks it end to end.
pub fn align_hits(
    reference: &[Onset],
    dub: &[Onset],
    dub_frames: usize,
    frame_rate: f64,
    cfg: HitAlignConfig,
) -> Option<Alignment> {
    if dub_frames == 0 {
        return None;
    }
    let pairs = pair_hits(reference, dub, cfg, frame_rate);
    if pairs.is_empty() {
        return None;
    }

    let strength = cfg.strength.clamp(0.0, 1.0);
    // Anchors: dub frame → where it should land. Between anchors the map
    // is linear, which stretches the gap between two hits evenly. That is
    // the right behaviour for percussion — the decay of one hit and the
    // silence before the next have no features of their own to preserve,
    // so spreading the correction across them is inaudible where moving
    // it all to one end would not be.
    let mut anchors: Vec<(f64, f64)> = Vec::with_capacity(pairs.len() + 2);
    // Outside the first and last hit, hold the correction constant
    // rather than letting it decay to zero — a lead-in dragged back to
    // "no correction" would put a cymbal swell out of step with the hit
    // it swells into.
    let first = &pairs[0];
    let first_target = first.dub as f64 + (first.reference - first.dub as f64) * strength;
    anchors.push((
        0.0,
        (first_target - first.dub as f64).max(-(first.dub as f64)),
    ));

    for p in &pairs {
        let target = p.dub as f64 + (p.reference - p.dub as f64) * strength;
        anchors.push((p.dub as f64, target - p.dub as f64));
    }
    let last = pairs.last().expect("non-empty");
    let last_target = last.dub as f64 + (last.reference - last.dub as f64) * strength;
    anchors.push(((dub_frames - 1) as f64, last_target - last.dub as f64));
    anchors.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    anchors.dedup_by(|a, b| a.0 == b.0);

    let mut map = Vec::with_capacity(dub_frames);
    let mut seg = 0usize;
    for i in 0..dub_frames {
        let x = i as f64;
        while seg + 1 < anchors.len() && anchors[seg + 1].0 < x {
            seg += 1;
        }
        let (x0, d0) = anchors[seg];
        let (x1, d1) = anchors[(seg + 1).min(anchors.len() - 1)];
        let offset = if (x1 - x0).abs() < 1e-9 {
            d0
        } else {
            let t = ((x - x0) / (x1 - x0)).clamp(0.0, 1.0);
            d0 + (d1 - d0) * t
        };
        map.push((x + offset).max(0.0));
    }

    // The map must never go backwards; interpolation between monotonic
    // anchors cannot make it, but clamping at zero above can.
    for i in 1..map.len() {
        if map[i] < map[i - 1] {
            map[i] = map[i - 1];
        }
    }

    // The anchors are already known here — they are the paired hits, and
    // the two held ends. Saying so means the write path gets a marker per
    // hit rather than one per frame, exactly as it does for the frame
    // matcher, and for the same reason: the moments worth pinning on a
    // kit are the hits, and nothing in a decay corresponds to anything.
    let last_index = anchors.len().saturating_sub(1);
    let warp_anchors = anchors
        .iter()
        .enumerate()
        .map(|(i, &(x, shift))| crate::align::Anchor {
            dub: x as usize,
            // Clamped as the map is: hit pairing has no offset stage, so
            // a target before the take's start is a correction with
            // nowhere to go rather than a take that belongs earlier.
            reference: (x + shift).max(0.0),
            confidence: 1.0,
            kind: match i {
                0 => crate::align::AnchorKind::Start,
                i if i == last_index => crate::align::AnchorKind::End,
                _ => crate::align::AnchorKind::Onset,
            },
        })
        .collect();

    Some(Alignment {
        map,
        anchors: warp_anchors,
        offset: crate::align::Offset::NONE,
        frame_rate,
    })
}
