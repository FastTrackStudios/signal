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
