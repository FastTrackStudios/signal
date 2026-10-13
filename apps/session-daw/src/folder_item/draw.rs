//! Drawing a folded picture: pieces in their colours, or two sides.
//!
//! Two arms, one per grouping, and they are not variations of each
//! other — a kit layers its pieces in one waveform, a Channel splits its
//! sides into two halves of one.

use anyrender::{PaintScene, Scene};
use expression_editor_core::kit::LaneRole;
use vello::kurbo::{Affine, BezPath, Rect};
use vello::peniko::{Color, Fill};

use super::fold::{Fold, FoldColumn, GroupBy, Side};

/// How much of the track colour the outer envelope keeps.
///
/// Dim, because it is context: it is everything the kit heard, which is
/// mostly the overheads and the rooms. They hear every hit and carry the
/// long crash decays, so drawn as a colour they swamp the pieces — the
/// prototype's "layered" variant, rejected on the evidence.
const ENVELOPE_ALPHA: f32 = 0.45;
/// How far the right side is lifted from the track colour, so the two
/// halves of a stereo waveform read as two performances rather than one
/// mirrored one.
const RIGHT_LIFT: f32 = 0.34;
/// How much of the track colour the item's body keeps — the same dim
/// fill every item in the arrangement gets under its waveform.
const BODY_ALPHA: f32 = 0.22;

/// The pieces, in the order they are painted. Kick last, so the kick
/// pattern reads on its own from across the room.
///
/// `LaneRole::Other` is deliberately absent: hats, cymbals and rooms are
/// heard, not played, and they are already in the outer envelope.
const PIECES: [LaneRole; 3] = [LaneRole::Toms, LaneRole::Snare, LaneRole::Kick];

/// Where a recorded folder item is replayed: its left edge, top, width
/// and height, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub x0: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

impl Place {
    /// The transform that takes the recorded unit box to this place.
    #[must_use]
    pub fn transform(self) -> Affine {
        Affine::translate((self.x0, self.top)) * Affine::scale_non_uniform(self.width, self.height)
    }
}

/// Replay a recorded folder item into `painter` at `at`.
///
/// The same submit the arrangement replays its own lanes with, so a
/// folder item goes through one command path with everything else.
pub fn replay(painter: &mut impl PaintScene, scene: &Scene, at: Place) {
    let transform = at.transform();
    for command in &scene.commands {
        crate::arrangement::submit_command(painter, command, transform);
    }
}

/// A colour from the `#rrggbb` string a [`LaneRole`] names.
pub(crate) fn hex(s: &str) -> Color {
    let byte = |i: usize| {
        s.get(i..i.saturating_add(2))
            .and_then(|h| u8::from_str_radix(h, 16).ok())
            .unwrap_or(0)
    };
    Color::from_rgb8(byte(1), byte(3), byte(5))
}

/// Blend `b` into `a` by `t`, keeping `a`'s alpha.
fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let [ar, ag, ab, aa] = a.components;
    let [br, bg, bb, _] = b.components;
    Color::new([
        (br - ar).mul_add(t, ar),
        (bg - ag).mul_add(t, ag),
        (bb - ab).mul_add(t, ab),
        aa,
    ])
}

/// The shade a side is drawn in: the track colour for the left, and the
/// track colour lifted towards white for the right.
#[must_use]
pub fn side_colour(track: Color, side: Side) -> Color {
    match side {
        Side::Left => track,
        Side::Right => mix(track, Color::WHITE, RIGHT_LIFT),
    }
}

/// The mirrored envelope of one slot across the columns, as one closed
/// path about `mid` with `half` of height either side.
fn envelope(
    columns: &[FoldColumn],
    pick: impl Fn(&FoldColumn) -> Option<(f32, f32)>,
    x0: f64,
    column_width: f64,
    mid: f64,
    half: f64,
) -> BezPath {
    let mut path = BezPath::new();
    let at = |i: usize| crate::num::coord(i).mul_add(column_width, x0);
    let amp = |v: f32| f64::from(v).clamp(-1.0, 1.0) * half;
    path.move_to((x0, mid));
    for (i, col) in columns.iter().enumerate() {
        let hi = pick(col).map_or(0.0, |(_, hi)| amp(hi));
        path.line_to((at(i), mid - hi));
        path.line_to((at(i.saturating_add(1)), mid - hi));
    }
    for (i, col) in columns.iter().enumerate().rev() {
        let lo = pick(col).map_or(0.0, |(lo, _)| amp(lo));
        path.line_to((at(i.saturating_add(1)), mid - lo));
        path.line_to((at(i), mid - lo));
    }
    path.close_path();
    path
}

/// Draw one folder item into `scene`, in the **unit box** — `x` from
/// `0.0` to `1.0` across the item, `y` from `0.0` to `1.0` down its row.
///
/// Recorded once per [`super::cache::PictureKey`] and replayed under
/// [`Place::transform`], which is what lets a pan, a row resize and a
/// zoom inside one bucket all reuse the picture. Everything drawn is a
/// fill, so the scale is exact — there is no stroke width to be wrong.
///
/// **Roles.** The outer envelope of everything first, in the dim track
/// colour, then the toms, the snare and the kick opaque on top of it in
/// `LaneRole::color()` — kick last. In that order the kick pattern reads
/// on its own in red, the snare's backbeat sits between it in amber, and
/// a fill is unmistakably green, which is what makes a take readable as
/// one item: the pattern, not just the loudness. The colours are the
/// expression editor's own, so the arrangement and the stack agree.
///
/// **Sides.** Two halves of one stereo waveform: the left side mirrored
/// about the upper quarter, the right about the lower, each in its own
/// shade, so a double reads as two performances in one item.
///
/// r[impl flow.drums.comping.folder-item-colours]
/// r[impl flow.guitars.folder-items]
pub fn draw(scene: &mut Scene, folded: &Fold, track: Color) {
    let columns = &folded.columns;
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        track.multiply_alpha(BODY_ALPHA),
        None,
        &Rect::new(0.0, 0.0, 1.0, 1.0),
    );
    if columns.is_empty() {
        return;
    }
    let (x0, height) = (0.0, 1.0);
    let column_width = 1.0 / crate::num::coord(columns.len());
    let fill = |scene: &mut Scene, colour: Color, path: &BezPath| {
        scene.fill(Fill::NonZero, Affine::IDENTITY, colour, None, path);
    };
    match folded.group_by {
        GroupBy::Role => {
            let mid = height / 2.0;
            let half = height / 2.0;
            let outer = envelope(columns, FoldColumn::outer, x0, column_width, mid, half);
            fill(scene, track.multiply_alpha(ENVELOPE_ALPHA), &outer);
            for role in PIECES {
                let Some(slot) = LaneRole::ALL.iter().position(|r| *r == role) else {
                    continue;
                };
                let path = envelope(
                    columns,
                    |c| c.slots.get(slot).copied().flatten(),
                    x0,
                    column_width,
                    mid,
                    half,
                );
                fill(scene, hex(role.color()), &path);
            }
        }
        GroupBy::Side => {
            let quarter = height / 4.0;
            let half = quarter;
            for side in [Side::Left, Side::Right] {
                let slot = side.slot();
                let mid = match side {
                    Side::Left => quarter,
                    Side::Right => quarter * 3.0,
                };
                let path = envelope(
                    columns,
                    |c| c.slots.get(slot).copied().flatten(),
                    x0,
                    column_width,
                    mid,
                    half,
                );
                fill(scene, side_colour(track, side), &path);
            }
        }
    }
}
