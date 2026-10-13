//! Editing in an automation lane: what a press starts, what a drag does
//! and what a release finishes.
//!
//! - **A press on the curve's ground** adds a point there and drags it; a
//!   press **on a point** drags it; **Alt** and a press on a point deletes
//!   it. Inside an automation item the same goes for the item's own curve
//!   (its pool's: every instance of it changes).
//! - **An item's name bar** moves the item; with **Ctrl** it makes a
//!   pooled copy, dropped where the pointer lets go (in another track's
//!   lane too); with **Alt** it deletes the item.
//! - **An item's edges** resize it (the left one keeps what plays in place).
//! - **Shift and a drag on the ground** makes an automation item over the
//!   span, its curve what the envelope played there.
//! - **A lane's bottom edge** makes it taller or shorter.
//! - **In its header**, the cross hides the lane and the mode cycles
//!   Trim/Read, Read and Off.
//! - **A vocal leveler stage's lane**: the switch in its header turns the
//!   stage on and off, a drag up or down its dial turns how far it goes
//!   (Alt and a click: back to as made), and the cross folds the four back
//!   into the sum on the items. In the ride's lane its points are dragged
//!   as any envelope's are.
//!
//! Every change is made to the window's lanes at once — the picture
//! follows the hand — and sent as an [`Edit`]; a gesture is one undo step.

use daw_proto::automation::EnvelopeRef;

use crate::arrangement::Viewport;
use crate::automation::{self, Kind};
use crate::automation_draw::{self as draw, Hit, Spot};
use crate::engine::Edit;

/// The undo step a lane gesture makes.
const LABEL: &str = "Edit automation";

/// The pool a stand-in for an item not made yet has in the window's
/// lanes (drawn "New" until the engine's own is read back).
const STANDING_IN: i32 = -2;

/// The keys held at a press.
#[derive(Clone, Copy, Debug, Default)]
pub struct Keys {
    pub alt: bool,
    pub shift: bool,
    pub ctrl: bool,
}

/// What a drag in a lane is moving.
#[derive(Clone, Debug)]
pub enum Grab {
    /// One of the envelope's own points, by its index now.
    Point(usize),
    /// A point of automation item `item`'s curve.
    ItemPoint { item: usize, index: usize },
    /// Automation item `item`, by its bar: the time pressed and where
    /// the item was; `copy`: a pooled copy of it being placed.
    Item {
        item: usize,
        from: f64,
        was: f64,
        copy: bool,
    },
    /// An automation item being made over the span from `from` to the
    /// pointer.
    Make { from: f64, to: f64 },
    /// Automation item `item`'s left or right (`right`) edge: where it
    /// was (position, length, offset).
    Bound {
        item: usize,
        right: bool,
        was: (f64, f64, f64),
    },
    /// The lane's bottom edge: the y pressed and the height it was.
    Edge { from: f64, was: f64 },
    /// A point of the vocal leveler's ride, on leveled item `item`.
    Ride { item: usize, index: usize },
    /// A leveler stage's dial: the y pressed and the amount it was at.
    Knob {
        stage: crate::leveler::Stage,
        from: f64,
        was: f64,
    },
}

/// A drag in a lane under way.
#[derive(Clone, Debug)]
pub struct LaneDrag {
    pub track: String,
    pub target: EnvelopeRef,
    pub kind: Kind,
    pub grab: Grab,
    /// The lane's top and height on screen at the press.
    top: f64,
    height: f64,
}

impl LaneDrag {
    /// The value at screen `y`, from where the lane was at the press.
    fn value_at(&self, y: f64) -> f64 {
        let placed = draw::Placed {
            row: 0,
            lane: automation::Lane {
                track: String::new(),
                target: self.target.clone(),
                name: String::new(),
                kind: self.kind,
                height: 0.0,
                mode: daw_proto::primitives::AutomationMode::TrimRead,
                points: Vec::new(),
                items: Vec::new(),
                played: Vec::new(),
                rest: 0.0,
                fx_name: None,
                leveled: Vec::new(),
                choices: Vec::new(),
            },
            top: self.top,
            height: self.height,
        };
        placed.value_at_y(y)
    }
}

/// The seconds at screen `x`.
fn seconds(view: Viewport, x: f64) -> f64 {
    ((x - view.panel_w + view.scroll_x) / view.pps.max(1e-9)).max(0.0)
}

// r[impl automation.lanes]
/// A press on `hit` at `(x, y)`: the drag it starts, if any, and the edits
/// it makes at once.
#[must_use]
pub fn press(hit: &Hit, y: f64, keys: Keys) -> (Option<LaneDrag>, Vec<Edit>) {
    let alt = keys.alt;
    let lane = &hit.placed.lane;
    let track = lane.track.clone();
    let target = lane.target.clone();
    let mut drag = LaneDrag {
        track: track.clone(),
        target: target.clone(),
        kind: lane.kind,
        grab: Grab::Point(0),
        top: hit.placed.top,
        height: hit.placed.height,
    };
    let begin = Edit::BeginUndo(LABEL.to_owned());
    match &hit.spot {
        Spot::Hide if lane.kind.is_leveler() => {
            // The four fold back into the sum on the items together.
            let mut lanes = automation::lanes_of(&track);
            lanes.retain(|l| !l.kind.is_leveler());
            automation::replace([(track.clone(), lanes)]);
            #[cfg(feature = "native")]
            if let Ok((daw, project)) = crate::actions::song() {
                crate::vocal_level::show_lane(daw, &project, &track, false);
            }
            (None, Vec::new())
        }
        Spot::Hide => {
            hide(&track, &target);
            (None, vec![Edit::ShowEnvelope(track, target, false)])
        }
        // The mode, which cycles — or in its place a leveler stage's
        // switch, which turns the stage on and off (its lanes have no mode:
        // they are the item's volume).
        Spot::Mode => {
            if let Kind::Leveler(stage) = lane.kind {
                let dial = dial_of(lane, stage);
                crate::leveler::set_dial(
                    &track,
                    stage,
                    crate::leveler::Dial {
                        on: !dial.on,
                        ..dial
                    },
                );
                dials_done(&track);
                return (None, Vec::new());
            }
            let mode = draw::next_mode(lane.mode);
            automation::update(&track, &target, |l| l.mode = mode);
            (None, vec![Edit::SetEnvelopeMode(track, target, mode)])
        }
        Spot::Knob => {
            let Kind::Leveler(stage) = lane.kind else {
                return (None, Vec::new());
            };
            let dial = dial_of(lane, stage);
            if alt {
                crate::leveler::set_dial(
                    &track,
                    stage,
                    crate::leveler::Dial {
                        amount: 1.0,
                        ..dial
                    },
                );
                dials_done(&track);
                return (None, Vec::new());
            }
            drag.grab = Grab::Knob {
                stage,
                from: y,
                was: dial.amount,
            };
            (Some(drag), Vec::new())
        }
        Spot::Header => (None, Vec::new()),
        Spot::Edge => {
            drag.grab = Grab::Edge {
                from: y,
                was: lane.height,
            };
            (Some(drag), Vec::new())
        }
        Spot::ItemBar { item, t } => bar_press(lane, drag, (*item, *t), keys),
        Spot::ItemEdge { item, right } => {
            let Some((ai, _)) = lane.items.get(*item) else {
                return (None, Vec::new());
            };
            drag.grab = Grab::Bound {
                item: *item,
                right: *right,
                was: (
                    ai.position.as_seconds(),
                    ai.length.as_seconds(),
                    ai.start_offset.as_seconds(),
                ),
            };
            (Some(drag), vec![begin])
        }
        Spot::Body {
            t,
            point: None,
            item_point: None,
            ..
        } if keys.shift && !lane.kind.is_leveler() => {
            drag.grab = Grab::Make { from: *t, to: *t };
            (Some(drag), vec![begin])
        }
        Spot::Body {
            t,
            value,
            point,
            item,
            item_point,
        } => body(lane, drag, (*t, *value), (*point, *item, *item_point), alt),
    }
}

// r[impl automation.items]
/// A press on automation item `item`'s name bar at `t` seconds: Alt
/// deletes it, Ctrl starts a pooled copy, a plain press a move.
fn bar_press(
    lane: &automation::Lane,
    mut drag: LaneDrag,
    (item, from): (usize, f64),
    keys: Keys,
) -> (Option<LaneDrag>, Vec<Edit>) {
    let (track, target) = (lane.track.clone(), lane.target.clone());
    let begin = Edit::BeginUndo(LABEL.to_owned());
    let alt = keys.alt;
    let Some((ai, _)) = lane.items.get(item) else {
        return (None, Vec::new());
    };
    if alt {
        let index = ai.index;
        let which = item;
        automation::update(&track, &target, |l| {
            if which < l.items.len() {
                l.items.remove(which);
            }
        });
        return (
            None,
            vec![
                begin,
                Edit::DeleteAutomationItem(track, target, index),
                Edit::EndUndo(LABEL.to_owned()),
            ],
        );
    }
    drag.grab = Grab::Item {
        item,
        from,
        was: ai.position.as_seconds(),
        copy: keys.ctrl,
    };
    (Some(drag), vec![begin])
}

/// A press on a lane's ground at `t` seconds and `value`, over envelope
/// point `point` or, in automation item `item`, its curve's point
/// `item_point`.
fn body(
    lane: &automation::Lane,
    mut drag: LaneDrag,
    (t, value): (f64, f64),
    (point, item, item_point): (Option<usize>, Option<usize>, Option<usize>),
    alt: bool,
) -> (Option<LaneDrag>, Vec<Edit>) {
    match lane.kind {
        Kind::Leveler(crate::leveler::Stage::Ride) => {
            return ride_press(lane, drag, (t, value), (item, item_point), alt);
        }
        // The other stages are read, not drawn: their dials say how far.
        Kind::Leveler(_) => return (None, Vec::new()),
        _ => {}
    }
    let (track, target) = (lane.track.clone(), lane.target.clone());
    let begin = Edit::BeginUndo(LABEL.to_owned());
    match (item, item_point, point) {
        (Some(i), Some(k), _) if alt => {
            let Some((ai, curve)) = lane.items.get(i) else {
                return (None, Vec::new());
            };
            let curve = automation::without_point(curve, k);
            let index = ai.index;
            set_item_curve(&track, &target, i, &curve);
            (
                None,
                vec![
                    begin,
                    Edit::SetItemCurve(track, target, index, curve),
                    Edit::EndUndo(LABEL.to_owned()),
                ],
            )
        }
        (Some(i), Some(k), _) => {
            drag.grab = Grab::ItemPoint { item: i, index: k };
            (Some(drag), vec![begin])
        }
        (Some(i), None, _) => {
            let Some((ai, curve)) = lane.items.get(i) else {
                return (None, Vec::new());
            };
            let at = draw::source_time(ai, t).max(0.0);
            let (curve, k) = automation::with_point(curve, None, at, value, lane.shape());
            let index = ai.index;
            set_item_curve(&track, &target, i, &curve);
            drag.grab = Grab::ItemPoint { item: i, index: k };
            (
                Some(drag),
                vec![begin, Edit::SetItemCurve(track, target, index, curve)],
            )
        }
        (None, _, Some(k)) if alt => {
            let points = automation::without_point(&lane.points, k);
            automation::update(&track, &target, |l| l.points.clone_from(&points));
            (
                None,
                vec![
                    begin,
                    Edit::SetEnvelope(track, target, points),
                    Edit::EndUndo(LABEL.to_owned()),
                ],
            )
        }
        (None, _, Some(k)) => {
            drag.grab = Grab::Point(k);
            (Some(drag), vec![begin])
        }
        (None, _, None) => {
            let (points, k) = automation::with_point(&lane.points, None, t, value, lane.shape());
            automation::update(&track, &target, |l| l.points.clone_from(&points));
            drag.grab = Grab::Point(k);
            (
                Some(drag),
                vec![begin, Edit::SetEnvelope(track, target, points)],
            )
        }
    }
}

/// A press in the vocal leveler's ride lane: on a ride point, it is
/// dragged (with Alt, deleted); elsewhere over an item, a ride point is
/// made there and dragged. The item's volume is written again when the
/// hand lets go ([`release`]); a deletion at once.
///
/// The ride is drawn through its dial, so a point put at a height is the
/// ride as made at that height over the dial; with the ride off or dialled
/// to nothing there is nothing to draw on.
fn ride_press(
    lane: &automation::Lane,
    mut drag: LaneDrag,
    (t, value): (f64, f64),
    (item, item_point): (Option<usize>, Option<usize>),
    alt: bool,
) -> (Option<LaneDrag>, Vec<Edit>) {
    let Some((i, leveled)) = item.and_then(|i| lane.leveled.get(i).map(|l| (i, l))) else {
        return (None, Vec::new());
    };
    let Some(scale) = ride_scale(leveled) else {
        return (None, Vec::new());
    };
    let track = lane.track.clone();
    match item_point {
        Some(k) if alt => {
            let mut ride = leveled.lanes.ride.clone();
            if k < ride.len() {
                ride.remove(k);
            }
            set_ride(&track, &leveled.item, &ride);
            relevel(&track, &leveled.item);
            (None, Vec::new())
        }
        Some(k) => {
            drag.grab = Grab::Ride { item: i, index: k };
            (Some(drag), Vec::new())
        }
        None => {
            let at = leveled.lane_time(t).max(0.0);
            let (ride, k) = with_ride_point(&leveled.lanes.ride, None, at, value / scale);
            set_ride(&track, &leveled.item, &ride);
            drag.grab = Grab::Ride { item: i, index: k };
            (Some(drag), Vec::new())
        }
    }
}

/// What the ride's dial multiplies it by, if a hand can draw on it.
fn ride_scale(leveled: &automation::Leveled) -> Option<f64> {
    let scale = leveled.stages.ride.gain();
    (scale > 0.01).then_some(scale)
}

/// `ride` with point `moving` moved to (or a new point made at) `t`
/// (lane seconds), `db` — and where it is now, the points kept in order.
fn with_ride_point(
    ride: &[level_dsp::envelope::EnvPoint],
    moving: Option<usize>,
    t: f64,
    db: f64,
) -> (Vec<level_dsp::envelope::EnvPoint>, usize) {
    let mut out = ride.to_vec();
    if let Some(i) = moving.filter(|&i| i < out.len()) {
        out.remove(i);
    }
    let at = out.partition_point(|p| p.t_s <= t);
    out.insert(at, level_dsp::envelope::EnvPoint::new(t, db));
    (out, at)
}

/// Leveled item `item`'s ride (as made) in the window, and its sum again.
fn set_ride(track: &str, item: &str, ride: &[level_dsp::envelope::EnvPoint]) {
    crate::leveler::update(track, |l| {
        if l.item == item {
            l.lanes.ride = ride.to_vec();
            l.resum();
        }
    });
}

/// Leveled item `item` as the window has it, kept and written as its
/// volume ([`crate::vocal_level::relevel`]).
fn relevel(track: &str, item: &str) {
    #[cfg(feature = "native")]
    {
        let Some(leveled) = crate::leveler::with_item(item, Clone::clone) else {
            return;
        };
        if let Ok((daw, project)) = crate::actions::song() {
            crate::vocal_level::relevel(daw, &project, track, &leveled);
            crate::studio::request_resync();
        }
    }
    #[cfg(not(feature = "native"))]
    let _ = (track, item);
}

/// A leveler stage lane's dial, as its items have it.
fn dial_of(lane: &automation::Lane, stage: crate::leveler::Stage) -> crate::leveler::Dial {
    lane.leveled
        .first()
        .map_or_else(crate::leveler::Dial::default, |l| l.stages.get(stage))
}

/// `track`'s dials as the window has them now: kept with the song, and
/// every leveled item's volume written through them.
fn dials_done(track: &str) {
    #[cfg(feature = "native")]
    if let (Some(stages), Ok((daw, project))) =
        (crate::leveler::stages_of(track), crate::actions::song())
    {
        let _rewritten = crate::vocal_level::set_stages(daw, &project, track, &stages);
        crate::studio::request_resync();
    }
    #[cfg(not(feature = "native"))]
    let _ = track;
}

/// The drag moved to `(x, y)`: the lane changed to follow it, and the edit
/// that says so.
#[must_use]
pub fn drag(drag: &mut LaneDrag, view: Viewport, x: f64, y: f64) -> Vec<Edit> {
    let zoom = if view.zoom_y > 0.0 { view.zoom_y } else { 1.0 };
    let t = seconds(view, x);
    let value = drag.value_at(y.clamp(drag.top, drag.top + drag.height));
    let lanes = automation::lanes_of(&drag.track);
    let Some(lane) = lanes
        .iter()
        .find(|l| automation::same_target(&l.target, &drag.target))
    else {
        return Vec::new();
    };
    let (track, target) = (drag.track.clone(), drag.target.clone());
    match &mut drag.grab {
        Grab::Point(index) => {
            let (points, at) =
                automation::with_point(&lane.points, Some(*index), t, value, lane.shape());
            *index = at;
            automation::update(&track, &target, |l| l.points.clone_from(&points));
            vec![Edit::SetEnvelope(track, target, points)]
        }
        Grab::ItemPoint { item, index } => {
            let Some((ai, curve)) = lane.items.get(*item) else {
                return Vec::new();
            };
            let source = draw::source_time(ai, t).max(0.0);
            let (curve, at) =
                automation::with_point(curve, Some(*index), source, value, lane.shape());
            *index = at;
            let id = ai.index;
            set_item_curve(&track, &target, *item, &curve);
            vec![Edit::SetItemCurve(track, target, id, curve)]
        }
        Grab::Item {
            item,
            from,
            was,
            copy,
        } => {
            let to = (*was + t - *from).max(0.0);
            let Some((ai, curve)) = lane.items.get(*item) else {
                return Vec::new();
            };
            if *copy {
                // The copy's stand-in, where it would land in this lane.
                let stand_in = daw_proto::automation::AutomationItem {
                    index: u32::MAX,
                    position: daw_proto::primitives::PositionInSeconds::from_seconds(to),
                    ..ai.clone()
                };
                let curve = curve.clone();
                automation::update(&track, &target, |l| {
                    l.items.retain(|(ai, _)| ai.index != u32::MAX);
                    l.items.push((stand_in, curve));
                });
                return Vec::new();
            }
            let id = ai.index;
            let which = *item;
            automation::update(&track, &target, |l| {
                if let Some((ai, _)) = l.items.get_mut(which) {
                    ai.position = daw_proto::primitives::PositionInSeconds::from_seconds(to);
                }
            });
            vec![Edit::MoveAutomationItem(track, target, id, to)]
        }
        Grab::Make { from, to } => {
            *to = t;
            let (start, end) = (from.min(t), from.max(t));
            let stand_in = daw_proto::automation::AutomationItem {
                index: u32::MAX,
                pool_id: STANDING_IN,
                position: daw_proto::primitives::PositionInSeconds::from_seconds(start),
                length: daw_proto::primitives::Duration::from_seconds(end - start),
                ..daw_proto::automation::AutomationItem::default()
            };
            let curve = curve_over(lane, start, end);
            automation::update(&track, &target, |l| {
                l.items.retain(|(ai, _)| ai.index != u32::MAX);
                l.items.push((stand_in, curve));
            });
            Vec::new()
        }
        Grab::Bound { item, right, was } => {
            let Some((ai, _)) = lane.items.get(*item) else {
                return Vec::new();
            };
            let (pos, len, off) = *was;
            let rate = if ai.play_rate > 0.0 {
                ai.play_rate
            } else {
                1.0
            };
            let shortest = 0.05;
            let (pos, len, off) = if *right {
                (pos, (t - pos).max(shortest), off)
            } else {
                // The left edge: what plays stays where it is.
                let to = t.clamp(0.0, pos + len - shortest);
                let moved = to - pos;
                (to, len - moved, moved.mul_add(rate, off).max(0.0))
            };
            let (id, which) = (ai.index, *item);
            automation::update(&track, &target, |l| {
                if let Some((ai, _)) = l.items.get_mut(which) {
                    ai.position = daw_proto::primitives::PositionInSeconds::from_seconds(pos);
                    ai.length = daw_proto::primitives::Duration::from_seconds(len);
                    ai.start_offset = daw_proto::primitives::Duration::from_seconds(off);
                }
            });
            vec![Edit::SetAutomationItemBounds(
                track,
                target,
                id,
                (pos, len, off),
            )]
        }
        Grab::Ride { item, index } => {
            let Some(leveled) = lane.leveled.get(*item) else {
                return Vec::new();
            };
            let Some(scale) = ride_scale(leveled) else {
                return Vec::new();
            };
            let at = leveled.lane_time(t).max(0.0);
            let (ride, k) = with_ride_point(&leveled.lanes.ride, Some(*index), at, value / scale);
            *index = k;
            set_ride(&track, &leveled.item, &ride);
            Vec::new()
        }
        Grab::Knob { stage, from, was } => {
            // Up turns it up; the leveled items follow at once, and their
            // volume is written when the hand lets go.
            let dial = dial_of(lane, *stage);
            crate::leveler::set_dial(
                &track,
                *stage,
                crate::leveler::Dial {
                    amount: (*from - y).mul_add(draw::KNOB_PER_PX, *was),
                    ..dial
                },
            );
            Vec::new()
        }
        Grab::Edge { from, was } => {
            let height = ((y - *from) / zoom + *was).max(automation::LANE_MIN_H);
            automation::update(&track, &target, |l| l.height = height);
            let pixels = num_traits::ToPrimitive::to_u32(&height.round()).unwrap_or(0);
            vec![Edit::SetLaneHeight(track, target, pixels)]
        }
    }
}

/// The drag let go over `over` (the lane under the pointer, if any): an
/// item made or copied there, and the undo step it opened closed.
#[must_use]
pub fn release(drag: &LaneDrag, over: Option<&Hit>) -> Vec<Edit> {
    let end = Edit::EndUndo(LABEL.to_owned());
    let lane = || {
        automation::lanes_of(&drag.track)
            .into_iter()
            .find(|l| automation::same_target(&l.target, &drag.target))
    };
    match drag.grab {
        Grab::Make { from, to } => {
            let (start, stop) = (from.min(to), from.max(to));
            if stop - start < 0.05 {
                clear_stand_in(&drag.track, &drag.target);
                return vec![end];
            }
            let curve = lane().map_or_else(Vec::new, |l| curve_over(&l, start, stop));
            vec![
                Edit::AddAutomationItem(
                    drag.track.clone(),
                    drag.target.clone(),
                    (start, stop - start, -1),
                    curve,
                ),
                end,
            ]
        }
        Grab::Item {
            item, copy: true, ..
        } => {
            let Some(source) = lane() else {
                return vec![end];
            };
            let placed = source
                .items
                .iter()
                .find(|(ai, _)| ai.index == u32::MAX)
                .cloned();
            let pool = source.items.get(item).map(|(ai, _)| ai.pool_id);
            clear_stand_in(&drag.track, &drag.target);
            let (Some((stand_in, _)), Some(pool)) = (placed, pool) else {
                return vec![end];
            };
            // Dropped on another lane (another track's, too): the copy is
            // there, at the time it was let go.
            let (track, target, at) = match over {
                Some(hit) if !hit.placed.lane.kind.is_leveler() => match hit.spot {
                    Spot::Body { .. } | Spot::ItemBar { .. } | Spot::ItemEdge { .. } => (
                        hit.placed.lane.track.clone(),
                        hit.placed.lane.target.clone(),
                        stand_in.position.as_seconds(),
                    ),
                    _ => (
                        drag.track.clone(),
                        drag.target.clone(),
                        stand_in.position.as_seconds(),
                    ),
                },
                _ => (
                    drag.track.clone(),
                    drag.target.clone(),
                    stand_in.position.as_seconds(),
                ),
            };
            vec![
                Edit::AddAutomationItem(
                    track,
                    target,
                    (at, stand_in.length.as_seconds(), pool),
                    Vec::new(),
                ),
                end,
            ]
        }
        Grab::Edge { .. } => Vec::new(),
        Grab::Ride { item, .. } => {
            if let Some(guid) = lane().and_then(|l| l.leveled.get(item).map(|x| x.item.clone())) {
                relevel(&drag.track, &guid);
            }
            Vec::new()
        }
        Grab::Knob { .. } => {
            dials_done(&drag.track);
            Vec::new()
        }
        _ => vec![Edit::EndUndo(LABEL.to_owned())],
    }
}

/// The envelope's curve over `start..end` as an automation item's own —
/// what plays there, in seconds into the item.
fn curve_over(
    lane: &automation::Lane,
    start: f64,
    end: f64,
) -> Vec<daw_proto::automation::EnvelopePoint> {
    let at = |t: f64, value: f64| daw_proto::automation::EnvelopePoint {
        time: daw_proto::primitives::PositionInSeconds::from_seconds(t - start),
        value,
        ..daw_proto::automation::EnvelopePoint::default()
    };
    let mut out = vec![at(start, lane.value_at(start))];
    out.extend(
        lane.played
            .iter()
            .filter(|p| p.time.as_seconds() > start && p.time.as_seconds() < end)
            .map(|p| daw_proto::automation::EnvelopePoint {
                time: daw_proto::primitives::PositionInSeconds::from_seconds(
                    p.time.as_seconds() - start,
                ),
                ..p.clone()
            }),
    );
    out.push(at(end, lane.value_at(end)));
    out
}

/// A stand-in item gone from the window's lanes.
fn clear_stand_in(track: &str, target: &EnvelopeRef) {
    automation::update(track, target, |l| {
        l.items.retain(|(ai, _)| ai.index != u32::MAX);
    });
}

/// Automation item `item`'s curve changed in the window's lanes — and every
/// other instance of its pool with it.
fn set_item_curve(
    track: &str,
    target: &EnvelopeRef,
    item: usize,
    curve: &[daw_proto::automation::EnvelopePoint],
) {
    automation::update(track, target, |l| {
        let Some(pool) = l.items.get(item).map(|(ai, _)| ai.pool_id) else {
            return;
        };
        for (ai, c) in &mut l.items {
            if ai.pool_id == pool {
                *c = curve.to_vec();
            }
        }
    });
}

/// A lane gone from the window's lanes (hidden).
fn hide(track: &str, target: &EnvelopeRef) {
    let mut lanes = automation::lanes_of(track);
    lanes.retain(|l| !automation::same_target(&l.target, target));
    automation::replace([(track.to_owned(), lanes)]);
}
