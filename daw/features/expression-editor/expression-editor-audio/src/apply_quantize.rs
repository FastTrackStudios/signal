//! Writing a quantize plan to the host.
//!
//! Both modes, and the group rule that makes multi-mic editing work.
//!
//! **One detection drives every track.** Transients are found on a
//! single trigger track — normally the closest mic — and the resulting
//! edit is applied identically to every item in the group. Detecting per
//! track and editing per track is the thing that must not happen: two
//! mics would get cut a few samples apart and the source smears at every
//! join. That is the whole reason drum editing is done by splitting
//! rather than by warping, and it is undone completely by detecting
//! twice.
//!
//! The constraint that follows is real: items in a group must share a
//! start time, or "the same cut" means different audio on each. This
//! refuses rather than guessing.
//!
//! Nothing here needs a `split_item` in the facade. A split is a
//! duplicate with its position, length and start offset set, and those
//! four calls already exist on both backends — so this works headlessly
//! on standalone and in REAPER from the same code.

use daw::service::item::Items;
use daw::service::{
    Duration, FadeShape, ItemRef, PositionInSeconds, ProjectContext, StretchMarkers,
    StretchTakeRef, TakeRef, Takes,
};

use crate::align::Alignment;
use crate::quantize::{Piece, SplitConfig};
use crate::retime::TakePlacement;

/// What a group edit did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Applied {
    /// Items created, across every track.
    pub pieces: usize,
    /// Tracks edited.
    pub items: usize,
}

/// Why a group could not be edited.
#[derive(Clone, Debug, PartialEq)]
pub enum GroupError {
    /// No items to edit.
    Empty,
    /// Items do not share a start time. Reported with the worst
    /// disagreement so a caller can say how far out they are rather than
    /// only that they are.
    Ragged { spread_secs: f64 },
    /// An item vanished between planning and writing.
    Missing,
    /// The host refused a write.
    ///
    /// Reported rather than swallowed: a half-applied group is worse
    /// than a refused one, and a caller that is told "12 pieces" while
    /// three of the writes failed has no way to find out.
    Write { what: &'static str, detail: String },
}

impl GroupError {
    fn write(what: &'static str, e: impl core::fmt::Debug) -> Self {
        GroupError::Write {
            what,
            detail: format!("{e:?}"),
        }
    }
}

/// Largest start-time disagreement a group may have, in seconds.
///
/// Not zero: items nudged by a hair, or placed at a position that does
/// not land on a sample boundary, are still the same take. A millisecond
/// is well under the shortest crossfade and far below anything audible
/// as a phase error.
const START_TOLERANCE: f64 = 0.001;

/// Check a group shares a start, and return it.
pub fn group_start<D: Items>(
    daw: &D,
    project: ProjectContext,
    items: &[ItemRef],
) -> Result<f64, GroupError> {
    if items.is_empty() {
        return Err(GroupError::Empty);
    }
    let mut starts = Vec::with_capacity(items.len());
    for item in items {
        let info = daw
            .get_item(project.clone(), item.clone())
            .ok_or(GroupError::Missing)?;
        starts.push(info.position.as_seconds());
    }
    let lo = starts.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = starts.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if hi - lo > START_TOLERANCE {
        return Err(GroupError::Ragged {
            spread_secs: hi - lo,
        });
    }
    Ok(lo)
}

/// Cut and move every item in the group by one plan.
///
/// The first piece reuses the original item and the rest are duplicates,
/// so an undo in the host is one step per track rather than one per
/// piece, and a group that is already correct is not rebuilt.
// r[impl drums.group.kit]
pub fn apply_split<D>(
    daw: &D,
    project: ProjectContext,
    items: &[ItemRef],
    pieces: &[Piece],
    cfg: SplitConfig,
) -> Result<Applied, GroupError>
where
    D: Items + Takes,
{
    let start = group_start(daw, project.clone(), items)?;
    let pieces: Vec<&Piece> = pieces.iter().filter(|p| !p.is_empty()).collect();
    if pieces.is_empty() {
        return Ok(Applied::default());
    }
    let xfade = cfg.crossfade_secs.max(0.0);

    let mut applied = Applied {
        pieces: 0,
        items: 0,
    };
    for item in items {
        // The original take's start offset is where this item already
        // reads its source from; every cut is relative to that, not to
        // zero. An item trimmed at its left edge would otherwise have
        // every piece read from the wrong place.
        let base_offset = daw
            .get_take(project.clone(), item.clone(), TakeRef::Active)
            .map(|t| t.start_offset.as_seconds())
            .unwrap_or(0.0);

        for (i, piece) in pieces.iter().enumerate() {
            let target = if i == 0 {
                item.clone()
            } else {
                let guid = daw
                    .duplicate_item(project.clone(), item.clone())
                    .ok_or(GroupError::Missing)?;
                ItemRef::Guid(guid)
            };

            // Each piece is extended by a crossfade's worth so it
            // overlaps the piece after it. Without the overlap there is
            // nothing to fade *between*: moving pieces leaves a gap or a
            // butt join at every cut, and a butt join is a click.
            let is_last = i + 1 == pieces.len();
            let length = piece.len() + if is_last { 0.0 } else { xfade };

            daw.set_position(
                project.clone(),
                target.clone(),
                PositionInSeconds::from_seconds(start + piece.placed()),
            )
            .map_err(|e| GroupError::write("position", e))?;
            daw.set_length(
                project.clone(),
                target.clone(),
                Duration::from_seconds(length),
            )
            .map_err(|e| GroupError::write("length", e))?;
            daw.set_start_offset(
                project.clone(),
                target.clone(),
                TakeRef::Active,
                Duration::from_seconds(base_offset + piece.cut),
            )
            .map_err(|e| GroupError::write("start offset", e))?;

            // The fade in is on the *later* piece and the fade out on
            // the earlier, so the pair crosses. Equal-power rather than
            // linear: two correlated pieces summed through linear fades
            // dip by 3 dB in the middle, which on a kick is an audible
            // hole at every edit.
            if xfade > 0.0 {
                if i > 0 {
                    daw.set_fade_in(
                        project.clone(),
                        target.clone(),
                        Duration::from_seconds(xfade),
                        FadeShape::FastStart,
                    )
                    .map_err(|e| GroupError::write("fade in", e))?;
                }
                if !is_last {
                    daw.set_fade_out(
                        project.clone(),
                        target.clone(),
                        Duration::from_seconds(xfade),
                        FadeShape::FastEnd,
                    )
                    .map_err(|e| GroupError::write("fade out", e))?;
                }
            }
            applied.pieces += 1;
        }
        applied.items += 1;
    }
    Ok(applied)
}

/// Warp every item in the group by one plan.
///
/// The WARP half of the group write: the plan's alignment becomes one
/// stretch-marker map, written to every member take — identical in
/// project time, adjusted per take for its own start offset and play
/// rate. Offered for a group even though SPLIT is the drum default,
/// because a user who accepts the phase cost is entitled to; refused
/// only where SPLIT is refused, on a ragged start.
///
/// `applied.pieces` counts markers written per item.
// r[impl drums.quantize.apply]
pub fn apply_warp<D>(
    daw: &D,
    project: ProjectContext,
    items: &[ItemRef],
    alignment: &Alignment,
) -> Result<Applied, GroupError>
where
    D: Items + Takes + StretchMarkers,
{
    group_start(daw, project.clone(), items)?;
    let mut applied = Applied::default();
    for item in items {
        let info = daw
            .get_item(project.clone(), item.clone())
            .ok_or(GroupError::Missing)?;
        let take = daw
            .get_take(project.clone(), item.clone(), TakeRef::Active)
            .ok_or(GroupError::Missing)?;
        let placement = TakePlacement {
            item_position: info.position.as_seconds(),
            start_offset: take.start_offset.as_seconds(),
            play_rate: if take.play_rate > 0.0 {
                take.play_rate
            } else {
                1.0
            },
            frame_rate: alignment.frame_rate,
        };
        let markers = crate::retime::alignment_markers(alignment, placement);
        let location = StretchTakeRef::new(project.clone(), item.clone(), TakeRef::Active);
        daw.set_stretch_markers(location, markers.clone())
            .map_err(|e| GroupError::write("stretch markers", e))?;
        applied.pieces += markers.len();
        applied.items += 1;
    }
    Ok(applied)
}
