//! The manual slip: drag one hit, and the audio between it and the next
//! hit slides with the mouse — on every member of the group.
//!
//! This is the hand-edit half of drum editing (`quick-edit` in the
//! REAPER workflow). It is deliberately *not* its own write path: a slip
//! is a three-piece split plan — the audio before the hit (does not
//! move), the audio from the hit to the next hit (moves by the drag),
//! and the audio after (does not move) — handed to the same
//! [`apply_split`](crate::apply_quantize::apply_split) the quantizer
//! uses. One cut law, one pad law, one crossfade law; a quantize cut and
//! a hand cut cannot come apart.
//!
//! Spec: `drum-mode.md` r[drums.manual.slip], r[drums.manual.daw-split].

use daw::service::item::Items;
use daw::service::{ItemRef, ProjectContext, Takes};

use crate::apply_quantize::{Applied, GroupError, apply_split};
use crate::quantize::{Piece, SplitConfig};

/// The three-piece plan for one slipped hit.
///
/// Times are seconds from the item's start (take playback time), the
/// same axis a [`Piece`] uses. `hit` is the dragged transient, `next` the
/// transient after it (or the item end when the hit is the last), and
/// `delta` how far the drag moved it — positive is later.
///
/// The cut goes `pad` *before* each transient and the shift is measured
/// at the transient — the same law as the quantize planner, and for the
/// same reason: cutting on the attack clips it, and shifting the cut
/// instead of the hit flams the piece against the kit.
///
/// The drag is clamped so the moved piece cannot land before the item
/// start or swallow the following hit: `delta` is limited to the span
/// between the two cuts on either side.
// r[impl drums.manual.slip]
pub fn slip_pieces(
    hit: f64,
    next: f64,
    take_secs: f64,
    delta: f64,
    cfg: SplitConfig,
) -> Vec<Piece> {
    let pad = cfg.leading_pad_secs.max(0.0);
    let cut1 = (hit - pad).max(0.0);
    let cut2 = (next - pad).clamp(cut1, take_secs);
    let end = take_secs.max(cut2);

    // The moved piece may not slide past its neighbours' cuts.
    let delta = delta.clamp(-cut1, (end - cut2).max(0.0));

    let mut pieces = Vec::with_capacity(3);
    if cut1 > 0.0 {
        pieces.push(Piece {
            cut: 0.0,
            end: cut1,
            shift: 0.0,
            transient: None,
        });
    }
    if cut2 > cut1 {
        pieces.push(Piece {
            cut: cut1,
            end: cut2,
            shift: delta,
            transient: Some(hit),
        });
    }
    if end > cut2 {
        pieces.push(Piece {
            cut: cut2,
            end,
            shift: 0.0,
            transient: Some(next),
        });
    }
    pieces
}

/// Cut the take in two at `at`, moving nothing.
///
/// The cut lands a `leading_pad_secs` *before* the click, for the same
/// reason every other cut in this crate does: a cut exactly on a
/// transient clips the attack off it, and the click will be near a hit
/// because that is where anyone aims. The pad is the difference between
/// a split you can hear and one you cannot.
///
/// Empty when the cut would fall outside the take, or would leave a
/// piece too short to be one. Splitting past either end produces a
/// single piece, which is not a split and would rebuild every item to
/// achieve nothing; splitting a hair inside an end produces a sliver
/// that cannot even hold its own crossfade, which is worse — it is a
/// item in the timeline that the user has to find and delete.
// r[impl drums.manual.split]
pub fn split_pieces(at: f64, take_secs: f64, cfg: SplitConfig) -> Vec<Piece> {
    // A click at or beyond either end is not a split at all, before any
    // question of where the pad puts the cut.
    if !(at > 0.0 && at < take_secs) {
        return Vec::new();
    }
    let pad = cfg.leading_pad_secs.max(0.0);
    let cut = (at - pad).clamp(0.0, take_secs);
    // Both halves need room for a fade in *and* out plus something
    // between them, and 20ms regardless — shorter than any drum's decay
    // and longer than a click, so the floor never refuses a split
    // anybody meant while still refusing the slivers nobody does.
    let min_piece = (cfg.crossfade_secs * 2.0).max(0.020);
    if cut < min_piece || take_secs - cut < min_piece {
        return Vec::new();
    }
    vec![
        Piece {
            cut: 0.0,
            end: cut,
            shift: 0.0,
            transient: None,
        },
        Piece {
            cut,
            end: take_secs,
            shift: 0.0,
            transient: Some(at),
        },
    ]
}

/// Split a whole group at one time.
///
/// Every member is cut at the same place, which is the group rule: mics
/// cut at different times are no longer phase-coherent, and a kit that
/// has lost phase coherence cannot be un-lost by hand.
// r[impl drums.manual.split]
pub fn split_group<D>(
    daw: &D,
    project: ProjectContext,
    items: &[ItemRef],
    at: f64,
    take_secs: f64,
    cfg: SplitConfig,
) -> Result<Applied, GroupError>
where
    D: Items + Takes + Clone,
{
    let pieces = split_pieces(at, take_secs, cfg);
    if pieces.is_empty() {
        return Ok(Applied::default());
    }
    apply_split(daw, project, items, &pieces, cfg)
}

/// Slip one hit on a whole group.
///
/// `items` are the group's members — one item per mic, sharing a start
/// (the group rule; refused as [`GroupError::Ragged`] otherwise). The
/// cut times and the slide are identical on every member by
/// construction, which is what keeps the mics phase-coherent.
// r[impl drums.manual.slip]
pub fn slip_hit<D>(
    daw: &D,
    project: ProjectContext,
    items: &[ItemRef],
    hit: f64,
    next: f64,
    take_secs: f64,
    delta: f64,
    cfg: SplitConfig,
) -> Result<Applied, GroupError>
where
    D: Items + Takes,
{
    let pieces = slip_pieces(hit, next, take_secs, delta, cfg);
    apply_split(daw, project, items, &pieces, cfg)
}

#[cfg(test)]
mod split_tests {
    use super::*;

    const CFG: SplitConfig = SplitConfig {
        leading_pad_secs: 0.005,
        crossfade_secs: 0.005,
    };

    // r[verify drums.manual.split]
    #[test]
    fn a_split_makes_two_pieces_that_cover_the_take() {
        let p = split_pieces(2.0, 10.0, CFG);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].cut, 0.0);
        assert_eq!(p[0].end, p[1].cut, "a gap or overlap between the halves");
        assert_eq!(p[1].end, 10.0, "the second half must reach the end");
    }

    // r[verify drums.manual.split]
    #[test]
    fn a_split_moves_nothing() {
        // The difference between a split and a slip. Everything stays
        // where it was; only the item boundary is new.
        for piece in split_pieces(4.0, 10.0, CFG) {
            assert_eq!(piece.shift, 0.0);
        }
    }

    // r[verify drums.manual.split]
    #[test]
    fn the_cut_lands_before_the_click_not_on_it() {
        // Anyone splitting a drum take aims at a hit, and a cut on the
        // attack clips it.
        let p = split_pieces(2.0, 10.0, CFG);
        assert!(
            p[0].end < 2.0,
            "the cut is at {} — on or after the click, so it clips the attack",
            p[0].end
        );
        assert!((p[0].end - (2.0 - CFG.leading_pad_secs)).abs() < 1e-9);
    }

    // r[verify drums.manual.split]
    #[test]
    fn splitting_outside_the_take_is_not_a_split() {
        // One piece is the take as it already was; writing it would
        // rebuild every item to achieve nothing.
        assert!(split_pieces(0.0, 10.0, CFG).is_empty());
        assert!(split_pieces(10.0, 10.0, CFG).is_empty());
        assert!(split_pieces(11.0, 10.0, CFG).is_empty());
        // And a click inside the pad of the start resolves to the start.
        assert!(split_pieces(0.004, 10.0, CFG).is_empty());
        // A click a hair inside the end would leave a sliver, which is
        // an item the user has to find and delete rather than a split.
        assert!(split_pieces(9.999, 10.0, CFG).is_empty());
        // But a real split near the end still works.
        assert_eq!(split_pieces(9.5, 10.0, CFG).len(), 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> SplitConfig {
        SplitConfig {
            leading_pad_secs: 0.005,
            crossfade_secs: 0.005,
        }
    }

    // r[verify drums.manual.slip]
    #[test]
    fn a_slip_is_three_pieces_and_only_the_middle_moves() {
        let p = slip_pieces(1.0, 2.0, 4.0, 0.030, cfg());
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].shift, 0.0);
        assert_eq!(p[2].shift, 0.0);
        assert!((p[1].shift - 0.030).abs() < 1e-12);
        // Cuts land pad before the transients.
        assert!((p[1].cut - 0.995).abs() < 1e-12);
        assert!((p[2].cut - 1.995).abs() < 1e-12);
        // Pieces tile the take.
        assert_eq!(p[0].cut, 0.0);
        assert_eq!(p[0].end, p[1].cut);
        assert_eq!(p[1].end, p[2].cut);
        assert_eq!(p[2].end, 4.0);
    }

    #[test]
    fn the_drag_is_clamped_to_its_neighbours() {
        // Dragging further left than the lead exists is clamped to the
        // item start…
        let p = slip_pieces(0.010, 1.0, 2.0, -1.0, cfg());
        let moved = p.iter().find(|p| p.shift != 0.0).unwrap();
        assert!((moved.placed() - 0.0).abs() < 1e-12);
        // …and a hit at the very start slips the first piece.
        let p = slip_pieces(0.0, 1.0, 2.0, 0.020, cfg());
        assert_eq!(p[0].cut, 0.0);
        assert!((p[0].shift - 0.020).abs() < 1e-12);
    }
}
