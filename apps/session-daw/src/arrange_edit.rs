//! Editing in the arrangement, as a state machine the window drives.
//!
//! The window gets winit events; this gets what they meant — a hit,
//! a time under the pointer, the keys held, a bound action — and
//! answers with what changed here and what has to happen elsewhere
//! ([`Effect`]: an edit for the engine, a re-record, a transport
//! move). Nothing in here knows a window, which is what makes a
//! gesture testable: a test builds a project, records it, presses on
//! an item, moves, releases, and reads the effects — the same code the
//! window runs, without a display.
//!
//! The window's copy of the project is edited here too (`&mut
//! Project`), so a prediction and the edit it predicts are one place.

use std::collections::{HashMap, HashSet};

use daw_proto::Track;
use daw_ui::studio::project::Project;
use razor::RazorArea;

use crate::arrangement::{Arrangement, Fades, ItemZone, Viewport};
use crate::cursor;
use crate::engine::{Edit, Move};
use crate::hit::{Hit, Target};
use crate::keys::Action;
use crate::mousemap::{self, Gesture, Mods};

/// The zoom tool's drag: the expression editor's gesture, on the
/// arrangement.
///
/// Hold `z`, press, and drag — sideways zooms time, up zooms the
/// rows, both exponential and both anchored so what was under the
/// press stays under it. Alt sweeps a box to frame instead.
///
/// The gains are the roll's: two hundred pixels of travel is one
/// e-fold, eight hundred with Shift for the fine control.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomTool {
    /// The drag itself, in window pixels — the Editor's too
    /// (`view_gestures::ZoomDrag`): how far a drag zooms, which way, and
    /// when a sweep is still a click.
    drag: view_gestures::ZoomDrag,
    /// The second under the press, and the content row under it at a
    /// vertical zoom of one.
    anchor_t: f64,
    anchor_y: f64,
    base_pps: f64,
    base_zoom_y: f64,
}

/// Where the lanes start in the window: the rails' corner plus the
/// panel's width and the ruler's height.
pub type LanesOrigin = (f64, f64);

/// The least and most the arrangement zooms to, in pixels a second
/// and in row scale.
pub const PPS_RANGE: (f64, f64) = (2.0, 48_000.0 * 20.0);
pub const ZOOM_Y_RANGE: (f64, f64) = (0.1, 40.0);

/// What an interaction asks the window to do beyond the picture.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// To the engine.
    Send(Edit),
    /// The recording is stale: what it holds changed.
    ReRecord,
    /// The transport, with a time where one is meant.
    Transport(Move, f64),
    /// The window's playhead, told where the transport went.
    Playhead(f64),
    /// The edit could not honestly be made, and this is why.
    ///
    /// Carried rather than swallowed: an edit that silently does nothing
    /// reads as a broken window. The only case so far is a folded row
    /// whose takes do not line up under the hand — see
    /// [`daw_ui::studio::folded::spread`].
    ///
    /// `row` is the folded folder the refusal is about, by track guid,
    /// so the window can put the sentence on the row it belongs to
    /// rather than in a corner the reader has to carry it back from.
    /// `None` where the refusal was not about a folded row — nothing
    /// produces one today, and the window falls back to the top of the
    /// lanes rather than dropping the message.
    Refused {
        why: &'static str,
        row: Option<String>,
    },
}

/// An item taken hold of by its body or an edge.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemPress {
    pub index: usize,
    pub guid: String,
    pub zone: ItemZone,
    /// The item's span when it was taken, in seconds.
    pub x0: f64,
    pub x1: f64,
    /// The time under the pointer when it was taken.
    pub from: f64,
    /// Where it would land, once the press has become a drag.
    pub ghost: Option<(f64, f64)>,
    /// Whether the drop lands on the grid, as the mouse map resolved
    /// it when the press landed.
    pub snap: bool,
    /// The slip this drag is changing, when it is a slip: how far into
    /// the source the item started at the press. `None` for every other
    /// drag, which is what tells the release which edit it is making.
    pub slip: Option<f64>,
    /// Where the slip has got to, once the pointer has moved.
    pub slipped: Option<f64>,
    /// Whether the drop leaves the original behind.
    ///
    /// Resolved at the press like `snap`, and for the same reason: a
    /// modifier let go of halfway through must not change what the drag
    /// is. You would be holding a copy and dropping a move.
    pub copy: bool,
    /// The other edges that were sitting exactly where this one was,
    /// and which of their own edges it is.
    ///
    /// Worked out at PRESS and not at release, because by release the
    /// edge has moved and nothing is sitting there any more. See
    /// [`Arrangement::edges_at`] for why a seam moves as one thing.
    pub joined: Vec<(String, ItemZone)>,
}

/// A fade taken hold of by its handle.
#[derive(Clone, Debug, PartialEq)]
pub struct FadeDrag {
    pub index: usize,
    pub guid: String,
    pub zone: ItemZone,
    pub x0: f64,
    pub x1: f64,
    pub fades: Fades,
}

/// A marquee across empty lanes: the Editor's marquee over notes, over
/// items (`view_gestures`). Kept in session units — seconds across, and
/// height from the top of the lanes, since rows are not one height — and
/// judged a click by its travel on screen.
#[derive(Clone, Debug)]
struct MarqueeDrag {
    from: (f64, f64),
    to: (f64, f64),
    mode: view_gestures::MarqueeMode,
    /// What was selected at the press, which the mode combines with.
    base: HashSet<String>,
}

/// How near an area's edge, on screen, a press takes the edge.
const RAZOR_EDGE_PX: f64 = 6.0;
/// The narrowest an edge drag leaves an area, in seconds.
const RAZOR_MIN_SECS: f64 = 0.01;

/// The arrangement's editing state.
#[derive(Debug, Default)]
pub struct Editor {
    /// The zoom tool's drag, while `z` is held and the button is down.
    pub zoom: Option<ZoomTool>,
    /// Pixels a second, as the window last drew: what tells a razor
    /// area's edge from its body. Zero until drawn — no edges then.
    pub pps: f64,
    /// A marquee across empty lanes, while the button is down.
    marquee: Option<MarqueeDrag>,
    /// The selected items, by guid.
    pub selected: HashSet<String>,
    /// The edit cursor and the time selection.
    pub cursor: cursor::Edit,
    /// A time selection being dragged out, from this time.
    time_drag: Option<f64>,
    item_press: Option<ItemPress>,
    fade_drag: Option<FadeDrag>,
    /// A press on the ruler, until it becomes a drag or a click.
    ruler_press: Option<RulerPress>,
    /// What is selected on the ruler, so a delete knows what to take.
    pub ruler_selection: Option<crate::ruler::On>,
    /// The span of the band under the pointer, set by the window when
    /// it hit-tests, so a drag knows what it is moving from. The hit
    /// map knows WHICH band; only the window has the list to ask how
    /// wide it is.
    pub ruler_span: Option<(f64, f64)>,
    /// A refused drag on its way back to where it started.
    snapback: Option<Snapback>,
    /// The razor areas currently drawn.
    ///
    /// Public because the paint passes read it and the window owns no
    /// copy: an area is editor state the way the selection is, and two
    /// copies of it would be two answers to "what is the razor over".
    pub razor: razor::RazorSet,
    /// A razor area being drawn or moved.
    razor_drag: Option<RazorDrag>,
}

/// A razor area under the hand: the Editor's razor gesture
/// (`razor::RazorGesture` — drawing, moving, resizing an edge), with what
/// only the arrangement adds to it.
#[derive(Clone, Debug)]
struct RazorDrag {
    gesture: razor::RazorGesture,
    /// Whether the edges land on the grid — resolved at the press and
    /// obeyed for the rest of the gesture, like every other drag here.
    snap: bool,
    /// What the press would have meant if it never becomes a drag.
    ///
    /// Ctrl on an item's body is a razor when the pointer moves and a
    /// toggle of the selection when it does not, and a press cannot yet
    /// tell which. The item press solves the same problem by growing a
    /// ghost; a razor cannot, because it takes the press before the
    /// item does — so it carries the other reading instead and performs
    /// it on release if the drag never happened.
    on_click: Option<ClickInstead>,
}

/// What a razor press meant, if it turned out to be a click.
#[derive(Clone, Debug)]
enum ClickInstead {
    /// Select this item — alone, or added to what is selected.
    Item { guid: String, add: bool },
    /// Put the edit cursor here, which is what a click on empty lane
    /// has always meant.
    Cursor(f64),
}

/// The ghost of a refused drag, returning.
///
/// A refused edit used to end by the ghost blinking out, which from the
/// hand is indistinguishable from the press never having been taken —
/// and a window that appears to ignore the mouse is the thing the
/// notice exists to stop. So the ghost travels back to where the item
/// actually is: the gesture was seen, considered, and undone, and all
/// three are legible in one movement.
///
/// Short on purpose. This is punctuation on a gesture, not an
/// animation to watch; anything slower would be in the way of the next
/// attempt, which is usually immediate.
#[derive(Clone, Copy, Debug)]
struct Snapback {
    index: usize,
    /// Where the drag was refused.
    from: (f64, f64),
    /// And where the item has been all along.
    to: (f64, f64),
    started: web_time::Instant,
}

/// How long a refused ghost takes to get back.
const SNAPBACK: f64 = 0.16;

impl Snapback {
    /// How far back it has come, 0..1, or `None` once it is home.
    fn progress(&self) -> Option<f64> {
        let t = self.started.elapsed().as_secs_f64() / SNAPBACK;
        (t < 1.0).then(|| {
            // Eased out: fast away from the refusal, settling into the
            // place it is going, which is the shape a thing springing
            // back has.
            let left = 1.0 - t;
            1.0 - left * left
        })
    }

    /// The span to draw, on the way.
    fn span(&self) -> Option<(usize, f64, f64)> {
        let t = self.progress()?;
        let lerp = |a: f64, b: f64| (b - a).mul_add(t, a);
        Some((
            self.index,
            lerp(self.from.0, self.to.0),
            lerp(self.from.1, self.to.1),
        ))
    }
}

/// A press on the ruler, before it has decided what it is.
///
/// Holds where it started and what it landed on, and grows a `ghost`
/// once the pointer has moved far enough to be a drag. Until then it is
/// a click, and a click on the ruler means what it has always meant:
/// put the cursor there.
#[derive(Clone, Debug)]
struct RulerPress {
    on: crate::ruler::On,
    /// Where the press was, in seconds.
    from: f64,
    /// The band or mark as it stood, so a drag is relative to where it
    /// WAS and not to where the last frame left it.
    was: (f64, f64),
    /// Where it would land, once the press has become a drag.
    ghost: Option<(f64, f64)>,
    /// Whether it lands on the grid, as the mouse map resolved it when
    /// the press landed — see `ItemPress::snap`.
    snap: bool,
    /// The other region edges and markers that were sitting exactly
    /// where this one was.
    ///
    /// Worked out at PRESS for the reason the item side works its own
    /// out then: by release the edge has moved and nothing is sitting
    /// there any more. Empty for a BODY drag — a region moved bodily
    /// takes its neighbours' boundaries nowhere, and one that dragged
    /// them would make a timeline impossible to rearrange.
    joined: Vec<crate::ruler::MarkEdge>,
}

impl Editor {
    /// The mouse-map context a press on `hit` would be asked about: the
    /// hit's own, unless a razor area already drawn is under it, which
    /// answers for itself (the same rule [`Self::press`] follows).
    #[must_use]
    pub fn context_at(&self, hit: Hit) -> input_config_proto::MouseModifierContext {
        let Some((at, row)) = Self::where_in_lanes(hit.target) else {
            return hit.context;
        };
        match self.razor.at(at, row) {
            // Within a few pixels of either end: the edge, as the
            // Editor's razor has one.
            Some((_, area)) if self.on_razor_edge(area, at) => mousemap::RAZOR_EDGE,
            Some(_) => mousemap::RAZOR_AREA,
            None => hit.context,
        }
    }

    /// Whether `at` is within reach of either edge of `area`, on screen.
    fn on_razor_edge(&self, area: RazorArea, at: f64) -> bool {
        self.pps > 0.0
            && ((at - area.t0).abs() * self.pps <= RAZOR_EDGE_PX
                || (at - area.t1).abs() * self.pps <= RAZOR_EDGE_PX)
    }

    /// The pointer went down on `hit`, with `keys` held. `true` if the
    /// press was taken here.
    pub fn press(
        &mut self,
        hit: Option<Hit>,
        keys: Mods,
        scene: &Arrangement,
        effects: &mut Vec<Effect>,
    ) -> bool {
        // A new press ends any refused ghost still travelling: `ghost`
        // answers for one drag, and the one being started now is it.
        self.snapback = None;
        let Some(hit) = hit else { return false };
        if self.razor_pressed(hit, keys, scene) {
            return true;
        }
        match hit.target {
            Target::Item {
                index,
                zone,
                seconds,
                ..
            } => {
                let bound = mousemap::resolve(hit.context, Gesture::Drag, keys);
                let action = bound.action;
                let Some(item) = scene.item(index) else {
                    return false;
                };
                match action {
                    // A fade is not taken hold of while locked.
                    mousemap::Action::FadeIn | mousemap::Action::FadeShape
                        if crate::options::LOCKING.get() =>
                    {
                        false
                    }
                    mousemap::Action::FadeIn | mousemap::Action::FadeShape => {
                        self.fade_drag = Some(FadeDrag {
                            index,
                            guid: item.guid.clone(),
                            zone,
                            x0: item.x0,
                            x1: item.x1,
                            fades: item.fades,
                        });
                        true
                    }
                    mousemap::Action::MoveItem
                    | mousemap::Action::CopyItem
                    | mousemap::Action::SlipItem
                    | mousemap::Action::TrimLeft
                    | mousemap::Action::TrimRight => {
                        // A trim takes the seam with it; a move does
                        // not — sliding an item along its lane is
                        // meant to leave its neighbours alone.
                        let joined = match zone {
                            ItemZone::LeftEdge => scene.edges_at(item.row, item.x0, &item.guid),
                            ItemZone::RightEdge => scene.edges_at(item.row, item.x1, &item.guid),
                            _ => Vec::new(),
                        };
                        self.item_press = Some(ItemPress {
                            index,
                            guid: item.guid.clone(),
                            zone,
                            x0: item.x0,
                            x1: item.x1,
                            from: seconds,
                            ghost: None,
                            joined,
                            // Resolved at the PRESS and obeyed for the
                            // rest of the gesture. A modifier let go of
                            // halfway through a drag must not change
                            // what the drag is — you would be holding
                            // one thing and dropping another.
                            snap: bound.snap,
                            copy: action == mousemap::Action::CopyItem,
                            slip: (action == mousemap::Action::SlipItem).then_some(item.slip),
                            slipped: None,
                        });
                        true
                    }
                    _ => false,
                }
            }
            Target::Ruler { seconds, on } => {
                use crate::ruler::{On, Zone};
                // What the press took hold of is remembered whatever it
                // was, because a press on the ruler is not yet a
                // gesture: it becomes a drag if the pointer moves, and
                // stays a click if it does not.
                self.ruler_selection =
                    matches!(on, On::Marker { .. } | On::Region { .. }).then_some(on);
                let was = match on {
                    On::Marker { .. } => (seconds, seconds),
                    On::Region { .. } => self.ruler_span.unwrap_or((seconds, seconds)),
                    On::Lane { .. } | On::Bars => (seconds, seconds),
                };
                // What else is on this moment. A boundary is shared:
                // the region that ends here, the one that starts here,
                // and any marker written on it are one thing as far as
                // the hand is concerned.
                let joined = match on {
                    On::Region {
                        zone: Zone::Body, ..
                    }
                    | On::Lane { .. }
                    | On::Bars => Vec::new(),
                    On::Region { zone, .. } => {
                        let edge = match zone {
                            Zone::Start => was.0,
                            // `Body` is unreachable — matched above.
                            Zone::Body | Zone::End => was.1,
                        };
                        scene.marks_at(edge, Some(on))
                    }
                    On::Marker { .. } => scene.marks_at(seconds, Some(on)),
                };
                self.ruler_press = Some(RulerPress {
                    on,
                    from: seconds,
                    was,
                    ghost: None,
                    snap: mousemap::resolve(hit.context, Gesture::Drag, keys).snap,
                    joined,
                });
                // The bars are the timeline, so a press there still
                // means what a press on a timeline has always meant.
                // The lanes do not: a press in a lane is the start of
                // making or moving something, and moving the cursor as
                // well would seek every time you reached for a band.
                if matches!(on, On::Bars) {
                    self.time_drag = Some(seconds);
                    self.cursor.click(seconds);
                    effects.push(Effect::Transport(Move::Seek, seconds));
                }
                true
            }
            Target::Lane { seconds, y, .. } => {
                // A drag across empty lanes is a marquee; until it moves
                // it is the click below, decided on release.
                let mode = match mousemap::resolve(hit.context, Gesture::Drag, keys).action {
                    mousemap::Action::MarqueeSelect => Some(view_gestures::MarqueeMode::Replace),
                    mousemap::Action::MarqueeAdd => Some(view_gestures::MarqueeMode::Add),
                    _ => None,
                };
                if let Some(mode) = mode {
                    self.marquee = Some(MarqueeDrag {
                        from: (seconds, y),
                        to: (seconds, y),
                        mode,
                        base: self.selected.clone(),
                    });
                    return true;
                }
                if mousemap::resolve(hit.context, Gesture::Click, keys).action
                    != mousemap::Action::SetEditCursor
                {
                    return false;
                }
                // A press moves the cursor at once — the click is what
                // you meant — and the transport goes there, because the
                // edit cursor and the play position are one thing until
                // a time selection separates them.
                self.time_drag = Some(seconds);
                self.cursor.click(seconds);
                effects.push(Effect::Transport(Move::Seek, seconds));
                true
            }
            _ => false,
        }
    }

    /// Write the edit each joined mark needs to land on `to`.
    ///
    /// A region takes both bounds whatever moved, so the bound that did
    /// not move is the one captured at the press — see
    /// [`crate::ruler::MarkEdge`].
    fn carry_joined(joined: &[crate::ruler::MarkEdge], to: f64, effects: &mut Vec<Effect>) {
        use crate::ruler::MarkEdge;
        for edge in joined {
            let edit = match *edge {
                MarkEdge::RegionStart { id, end } => {
                    Edit::SetRegionBounds(String::new(), id, to, end)
                }
                MarkEdge::RegionEnd { id, start } => {
                    Edit::SetRegionBounds(String::new(), id, start, to)
                }
                MarkEdge::Marker { id } => Edit::MoveMarker(String::new(), id, to),
            };
            effects.push(Effect::Send(edit));
        }
    }

    /// The razor's half of a press: an area grabbed, or a new one
    /// started.
    ///
    /// Before the rest of `press` and not inside it, because an area
    /// drawn over a take sits OVER that take: the pointer is on both,
    /// and the one that was put there deliberately wins. Answering
    /// `true` means the gesture has been taken and nothing else should
    /// look at it.
    fn razor_pressed(&mut self, hit: Hit, keys: Mods, scene: &Arrangement) -> bool {
        let Some((at, row)) = Self::where_in_lanes(hit.target) else {
            return false;
        };
        // An area already under the pointer answers for itself: its body
        // moves it, its edge resizes it.
        if let Some((index, area)) = self.razor.at(at, row) {
            let bound = mousemap::resolve(self.context_at(hit), Gesture::Drag, keys);
            let gesture = match bound.action {
                mousemap::Action::MoveRazorArea => razor::RazorGesture::grab(index, area, at, row),
                mousemap::Action::ResizeRazorArea => razor::RazorGesture::edge(
                    index,
                    area,
                    razor::RazorGesture::nearer_start(area, at),
                    RAZOR_MIN_SECS,
                ),
                _ => return false,
            };
            self.razor_drag = Some(RazorDrag {
                gesture,
                snap: bound.snap,
                // A click on an area takes it out, which the gesture's
                // release does rather than this.
                on_click: None,
            });
            return true;
        }
        let bound = mousemap::resolve(hit.context, Gesture::Drag, keys);
        if bound.action != mousemap::Action::RazorArea {
            return false;
        }
        // Zero-width until the pointer moves. An area that sprang into
        // existence a beat wide on mousedown would be a click that
        // edits, and a razor is never that.
        self.razor_drag = Some(RazorDrag {
            gesture: razor::RazorGesture::draw(at, row),
            snap: bound.snap,
            on_click: Self::click_instead(hit, at, keys, scene),
        });
        true
    }

    /// What the press would have been without the drag.
    fn click_instead(hit: Hit, at: f64, keys: Mods, scene: &Arrangement) -> Option<ClickInstead> {
        let action = mousemap::resolve(hit.context, Gesture::Click, keys).action;
        match (hit.target, action) {
            (
                Target::Item { index, .. },
                mousemap::Action::SelectItem | mousemap::Action::ToggleItemSelection,
            ) => scene.item(index).map(|item| ClickInstead::Item {
                guid: item.guid.clone(),
                add: action == mousemap::Action::ToggleItemSelection,
            }),
            (Target::Lane { .. }, mousemap::Action::SetEditCursor) => {
                Some(ClickInstead::Cursor(at))
            }
            _ => None,
        }
    }

    /// The row and time a hit landed on, for the two targets that have
    /// both. `None` anywhere a razor cannot be.
    fn where_in_lanes(target: Target) -> Option<(f64, i32)> {
        let (row, seconds) = match target {
            Target::Lane { row, seconds, .. } | Target::Item { row, seconds, .. } => (row, seconds),
            _ => return None,
        };
        Some((seconds, i32::try_from(row).unwrap_or(i32::MAX)))
    }

    /// A razor drag following the pointer. `true` when the picture
    /// changed.
    ///
    /// Takes the row as well as the time, which is why it is not folded
    /// into [`Self::moved`]: every other drag here is along one axis
    /// and a razor is the only one that is a rectangle.
    pub fn razor_moved(&mut self, at: f64, row: usize, bpm: f64) -> bool {
        let Some(drag) = self.razor_drag.as_mut() else {
            return false;
        };
        let row = i32::try_from(row).unwrap_or(i32::MAX);
        let beat = 60.0 / bpm.max(1.0);
        let on_grid = drag.snap;
        let snapped = |t: f64| {
            if on_grid {
                (t / beat).round() * beat
            } else {
                t
            }
        };
        drag.gesture.moved(at, row, snapped)
    }

    /// The marquee follows the pointer, to `seconds` and the height `y`
    /// (session units from the top of the lanes). `true` when it moved.
    pub fn marquee_moved(&mut self, seconds: f64, y: f64) -> bool {
        let Some(drag) = self.marquee.as_mut() else {
            return false;
        };
        let was = drag.to;
        drag.to = (seconds, y);
        drag.to != was
    }

    /// The marquee being swept, `(from, to)` in seconds and session-unit
    /// heights, while one is.
    #[must_use]
    pub fn marquee_in_flight(&self) -> Option<((f64, f64), (f64, f64))> {
        self.marquee.as_ref().map(|m| (m.from, m.to))
    }

    /// The marquee let go: the items its box touches, combined with the
    /// selection at the press by its mode (`view_gestures::combine`). A
    /// box that never went anywhere on screen is a click on empty lanes:
    /// the cursor goes there, and with Replace the selection clears.
    pub fn marquee_released(
        &mut self,
        scene: &Arrangement,
        view: &Viewport,
        project: &mut Project,
        effects: &mut Vec<Effect>,
    ) -> bool {
        let Some(drag) = self.marquee.take() else {
            return false;
        };
        let zoom_y = if view.zoom_y > 0.0 { view.zoom_y } else { 1.0 };
        let travel = (drag.to.0 - drag.from.0)
            .abs()
            .mul_add(view.pps, (drag.to.1 - drag.from.1).abs() * zoom_y);
        let click = travel <= view_gestures::SLOP;
        let span = view_gestures::Span::new(drag.from, drag.to);
        let hits = scene.item_boxes().iter().filter_map(|item| {
            let (top, height) = scene.item_box(item)?;
            span.touches(item.x0, item.x1, top, top + height)
                .then(|| item.guid.clone())
        });
        let chosen = view_gestures::combine(drag.mode, &drag.base, hits, click);
        // Nothing to say when nothing changed — a click on empty lanes
        // with nothing selected is only the cursor.
        if chosen != self.selected {
            self.set_selection(&chosen, project, effects);
        }
        if click {
            self.cursor.click(drag.from.0);
            effects.push(Effect::Transport(Move::Seek, drag.from.0));
        }
        true
    }

    /// Make `chosen` the selection — rows' folded items standing for the
    /// mics under them, as a click on one does — and tell the engine.
    fn set_selection(
        &mut self,
        chosen: &HashSet<String>,
        project: &mut Project,
        effects: &mut Vec<Effect>,
    ) {
        let real: HashSet<String> = chosen
            .iter()
            .flat_map(|guid| targets(project, guid, false, effects).unwrap_or_default())
            .collect();
        self.selected.clone_from(chosen);
        for item in project.items.values_mut().flatten() {
            item.selected = real.contains(&item.guid) || chosen.contains(&item.guid);
        }
        let from = effects.len();
        effects.push(Effect::Send(Edit::DeselectAllItems(String::new())));
        let mut ordered: Vec<&String> = real.iter().collect();
        ordered.sort();
        for guid in ordered {
            effects.push(Effect::Send(Edit::SelectItem(guid.clone(), false)));
        }
        selection_as_one_step(from, effects);
    }

    /// Whether a razor drag is in flight, and the area it would leave.
    #[must_use]
    pub fn razor_in_flight(&self) -> Option<RazorArea> {
        self.razor_drag.as_ref().map(|drag| drag.gesture.area)
    }

    /// The razor drag let go of. `true` when the set changed: an area
    /// drawn, moved or resized, or a grab that went nowhere — a click,
    /// which takes the area out.
    fn razor_released(&mut self) -> bool {
        let Some(drag) = self.razor_drag.take() else {
            return false;
        };
        drag.gesture.release(&mut self.razor) != razor::Released::Nothing
    }

    /// The pointer moved to the time `at` (if it is over the timeline).
    /// `true` when the picture changed.
    pub fn moved(&mut self, at: Option<f64>, pps: f64, bpm: f64, keys: Mods) -> bool {
        let Some(at) = at else { return false };
        if let Some(press) = self.item_press.as_mut() {
            // Locked: the press still selects when it lets go, but a drag
            // moves nothing (on a touchscreen the finger layer has made it
            // a scroll before it gets here — see `widget::fingered`).
            if crate::options::LOCKING.get() {
                return false;
            }
            let dx = at - press.from;
            let moved = press.ghost.is_some() || (dx * pps).abs() > crate::gesture::SLOP;
            if !moved {
                return false;
            }
            let beat = 60.0 / bpm.max(1.0);
            let on_grid = press.snap;
            let snap = |t: f64| {
                if on_grid {
                    (t / beat).round() * beat
                } else {
                    t
                }
            };
            // A slip does not move the item, so its ghost is the item's
            // own span: what changes is which part of the source is
            // under it, and that is carried in `slip` rather than here.
            // Dragging the contents right shows EARLIER source under the
            // left edge, so the offset goes down as `dx` goes up.
            press.ghost = Some(match press.zone {
                _ if press.slip.is_some() => (press.x0, press.x1),
                ItemZone::LeftEdge => (snap(press.x0 + dx).clamp(0.0, press.x1 - 0.01), press.x1),
                ItemZone::RightEdge => (press.x0, snap(press.x1 + dx).max(press.x0 + 0.01)),
                _ => {
                    let x0 = snap(press.x0 + dx).max(0.0);
                    (x0, x0 + (press.x1 - press.x0))
                }
            });
            if let Some(base) = press.slip {
                // Clamped at the source's start: there is nothing before
                // the beginning of a file to show.
                press.slipped = Some(snap(base - dx).max(0.0));
            }
            return true;
        }
        // The ruler's bars sweep a time selection — the Editor's ruler
        // too. Live, so it is drawn as it is dragged.
        if let (Some(from), Some(true)) = (
            self.time_drag,
            self.ruler_press
                .as_ref()
                .map(|p| matches!(p.on, crate::ruler::On::Bars)),
        ) {
            self.cursor.drag(from, at);
        }
        if let Some(press) = self.ruler_press.as_mut() {
            use crate::ruler::{On, Zone};
            let dx = at - press.from;
            let moved = press.ghost.is_some() || (dx * pps).abs() > crate::gesture::SLOP;
            if !moved {
                return false;
            }
            let beat = 60.0 / bpm.max(1.0);
            // Off the grid means "exactly here", the same as it does
            // for an item, and it is the map that says so. A section
            // boundary that is a hair off the bar is one REAPER will
            // draw a hair off the bar forever.
            let on_grid = press.snap;
            let snap = |t: f64| {
                if on_grid {
                    ((t / beat).round() * beat).max(0.0)
                } else {
                    t.max(0.0)
                }
            };
            let (was0, was1) = press.was;
            press.ghost = Some(match press.on {
                On::Marker { .. } => {
                    let to = snap(was0 + dx);
                    (to, to)
                }
                On::Region { zone, .. } => match zone {
                    Zone::Start => (snap(was0 + dx).min(was1), was1),
                    Zone::End => (was0, snap(was1 + dx).max(was0)),
                    Zone::Body => {
                        let from = snap(was0 + dx);
                        (from, from + (was1 - was0))
                    }
                },
                // Drawing a new band out of empty lane: from where the
                // press landed to wherever the pointer is now.
                On::Lane { .. } | On::Bars => (snap(press.from), snap(at)),
            });
            return true;
        }
        if let Some(drag) = self.fade_drag.as_mut() {
            let span = (drag.x1 - drag.x0).max(0.0);
            match drag.zone {
                ItemZone::FadeIn => drag.fades.fade_in = (at - drag.x0).clamp(0.0, span),
                ItemZone::FadeOut => drag.fades.fade_out = (drag.x1 - at).clamp(0.0, span),
                _ => {}
            }
            return true;
        }
        if let Some(from) = self.time_drag {
            self.cursor.drag(from, at);
            return true;
        }
        false
    }

    /// The pointer came up at the time `at`.
    pub fn release(
        &mut self,
        at: Option<f64>,
        keys: Mods,
        project: &mut Project,
        effects: &mut Vec<Effect>,
    ) -> bool {
        // The razor took the press, so it answers for the release —
        // but only as a razor if it actually drew something. An area
        // still zero-wide means the press never became a drag, and then
        // the gesture was the click the press was also standing in for.
        if let Some(drag) = self.razor_drag.take() {
            if drag.gesture.grab != razor::Grab::Draw || !drag.gesture.area.is_empty() {
                self.razor_drag = Some(drag);
                return self.razor_released();
            }
            return match drag.on_click {
                Some(ClickInstead::Item { guid, add }) => {
                    self.select(&guid, !add, project, effects);
                    true
                }
                Some(ClickInstead::Cursor(at)) => {
                    self.cursor.click(at);
                    effects.push(Effect::Transport(Move::Seek, at));
                    true
                }
                None => false,
            };
        }
        if let Some(press) = self.ruler_press.take() {
            // A drag across the bars is a time selection; a click is just
            // the cursor, which the press already placed.
            if let Some(from) = self.time_drag.take()
                && matches!(press.on, crate::ruler::On::Bars)
                && let Some(to) = at
            {
                self.cursor.drag(from, to);
            }
            use crate::ruler::{MARKS_ROW, On, Zone, lane_of};
            match (press.on, press.ghost) {
                // A drag that never became one. On a mark or a band
                // that is a selection, which the press already made;
                // on empty lane it is a request for a new one, at a
                // single point.
                (On::Lane { row }, None) => {
                    let edit = if row == MARKS_ROW {
                        Edit::AddMarker(String::new(), press.from, lane_of(row))
                    } else {
                        // A region needs a length to exist. One bar,
                        // so a click makes something you can see and
                        // then drag, rather than something invisible.
                        let bar = 4.0 * 60.0 / 120.0;
                        Edit::AddRegion(String::new(), press.from, press.from + bar, lane_of(row))
                    };
                    effects.push(Effect::Send(edit));
                }
                (On::Lane { row }, Some((from, to))) => {
                    let edit = if row == MARKS_ROW {
                        // Dragging in the marks lane still makes one
                        // mark: a marker is a position, and there is
                        // no second end for the drag to have set.
                        Edit::AddMarker(String::new(), from, lane_of(row))
                    } else {
                        Edit::AddRegion(String::new(), from, to, lane_of(row))
                    };
                    effects.push(Effect::Send(edit));
                }
                (On::Marker { id }, Some((to, _))) => {
                    effects.push(Effect::Send(Edit::MoveMarker(String::new(), id, to)));
                    Self::carry_joined(&press.joined, to, effects);
                }
                (On::Region { id, zone }, Some((from, to))) => {
                    // Every zone sets both bounds, because REAPER's
                    // setter takes both — the zone decided which of
                    // them the drag was allowed to move.
                    effects.push(Effect::Send(Edit::SetRegionBounds(
                        String::new(),
                        id,
                        from,
                        to,
                    )));
                    // And the boundary goes as one thing. Which end
                    // moved is which end the neighbours follow; a body
                    // drag has no neighbours to follow it, and
                    // `joined` is empty there.
                    let moved = match zone {
                        Zone::Start => from,
                        Zone::Body | Zone::End => to,
                    };
                    Self::carry_joined(&press.joined, moved, effects);
                }
                (On::Marker { .. } | On::Region { .. }, None) | (On::Bars, _) => {}
            }
            return true;
        }
        if let Some(press) = self.item_press.take() {
            match press.ghost {
                None => self.select(&press.guid, !keys.ctrl, project, effects),
                Some((x0, x1)) => {
                    // On a folded row this lands on every mic under the
                    // item, because on screen they are one item. The
                    // span carries the same position and length as each
                    // of them — that is what `Span::whole` guarantees
                    // and why a ragged one is refused instead.
                    // A slip changes nothing about where the item IS, so
                    // it is neither a move nor a trim and shares no code
                    // with either. Its own branch, first, because both
                    // of the others would otherwise move an item the
                    // gesture was explicitly not moving.
                    if let Some(to) = press.slipped {
                        let made = spread_item_slip(project, &press.guid, to, effects);
                        if made {
                            effects.push(Effect::ReRecord);
                        } else {
                            self.snapback = Some(Snapback {
                                index: press.index,
                                from: (x0, x1),
                                to: (press.x0, press.x1),
                                started: web_time::Instant::now(),
                            });
                        }
                        return true;
                    }
                    let trimming = matches!(press.zone, ItemZone::LeftEdge | ItemZone::RightEdge);
                    // A copy leaves the original where it is, so it is
                    // not an edit to the item at all — it is a new one.
                    // Separate from the move for that reason rather
                    // than as a flag through it: nothing about the
                    // original changes, including the edges joined to
                    // it, which is why this returns early.
                    if press.copy && !trimming {
                        let made = spread_item_copy(project, &press.guid, x0, effects);
                        if !made {
                            self.snapback = Some(Snapback {
                                index: press.index,
                                from: (x0, x1),
                                to: (press.x0, press.x1),
                                started: web_time::Instant::now(),
                            });
                        } else {
                            effects.push(Effect::ReRecord);
                        }
                        return true;
                    }
                    let made = spread_item_edit(
                        project,
                        &press.guid,
                        effects,
                        |guid| {
                            if trimming {
                                Edit::TrimItem(guid.to_owned(), x0, x1 - x0)
                            } else {
                                Edit::MoveItem(guid.to_owned(), x0)
                            }
                        },
                        |item| {
                            item.position =
                                daw_proto::primitives::PositionInSeconds::from_seconds(x0);
                            item.length = daw_proto::primitives::Duration::from_seconds(x1 - x0);
                        },
                    );
                    // Refused: the ghost goes back where the item is,
                    // rather than blinking out as if the press had
                    // never been taken. The notice says why; this says
                    // that something was attempted at all.
                    if !made {
                        self.snapback = Some(Snapback {
                            index: press.index,
                            from: (x0, x1),
                            to: (press.x0, press.x1),
                            started: web_time::Instant::now(),
                        });
                    }
                    // And every edge that was sitting on the one just
                    // moved goes with it, or a trim opens a gap where
                    // two items were butted together.
                    let mut moved_any = made;
                    if trimming {
                        let to = if matches!(press.zone, ItemZone::LeftEdge) {
                            x0
                        } else {
                            x1
                        };
                        for (guid, edge) in &press.joined {
                            let start = matches!(edge, ItemZone::LeftEdge);
                            // Read the span before the walk, because
                            // the walk holds `project.items` mutably
                            // and the edit has to name the whole span:
                            // there is no "move this edge" edit, only
                            // "this item is now here, this long".
                            let Some((was0, was1)) = project
                                .items
                                .values()
                                .flatten()
                                .find(|item| item.guid == *guid)
                                .map(|item| {
                                    let at = item.position.as_seconds();
                                    (at, at + item.length.as_seconds())
                                })
                            else {
                                continue;
                            };
                            let (a, b) = if start { (to, was1) } else { (was0, to) };
                            let (a, b) = (a.min(b), a.max(b));
                            moved_any |= spread_item_edit(
                                project,
                                guid,
                                effects,
                                |guid| Edit::TrimItem(guid.to_owned(), a, b - a),
                                |item| {
                                    item.position =
                                        daw_proto::primitives::PositionInSeconds::from_seconds(a);
                                    item.length =
                                        daw_proto::primitives::Duration::from_seconds(b - a);
                                },
                            );
                        }
                    }
                    if moved_any {
                        effects.push(Effect::ReRecord);
                    }
                }
            }
            return true;
        }
        if let Some(drag) = self.fade_drag.take() {
            let edit = match drag.zone {
                ItemZone::FadeIn => {
                    Edit::SetFadeIn(drag.guid.clone(), drag.fades.fade_in, drag.fades.in_shape)
                }
                ItemZone::FadeOut => {
                    Edit::SetFadeOut(drag.guid.clone(), drag.fades.fade_out, drag.fades.out_shape)
                }
                _ => return true,
            };
            edit_item(project, &drag.guid, |item| {
                item.fade_in_length =
                    daw_proto::primitives::Duration::from_seconds(drag.fades.fade_in);
                item.fade_out_length =
                    daw_proto::primitives::Duration::from_seconds(drag.fades.fade_out);
            });
            effects.push(Effect::Send(edit));
            effects.push(Effect::ReRecord);
            return true;
        }
        if let Some(from) = self.time_drag.take() {
            // A drag across the timeline is a time selection; a click
            // is just the cursor. `Edit::drag` decides which, so a
            // twitch does not leave a four-millisecond selection.
            if let Some(to) = at {
                self.cursor.drag(from, to);
            }
            return true;
        }
        false
    }

    /// The zoom tool takes hold at a window point. `true` when it did
    /// — it only takes a press on the lanes.
    pub fn zoom_press(
        &mut self,
        at: (f64, f64),
        view: &Viewport,
        lanes: LanesOrigin,
        keys: Mods,
    ) -> bool {
        let (x, y) = at;
        if x < lanes.0 || y < lanes.1 {
            return false;
        }
        let zoom_y = if view.zoom_y > 0.0 { view.zoom_y } else { 1.0 };
        self.zoom = Some(ZoomTool {
            drag: view_gestures::ZoomDrag::press(at, keys.alt),
            anchor_t: (x - lanes.0 + view.scroll_x) / view.pps.max(f64::EPSILON),
            anchor_y: (y - lanes.1 + view.scroll_y) / zoom_y,
            base_pps: view.pps,
            base_zoom_y: zoom_y,
        });
        true
    }

    /// The zoom tool's drag: the view the arrangement should now show,
    /// or `None` when no zoom is in flight or a sweep is still being
    /// drawn. The scroll comes back unclamped; the window clamps it to
    /// its spans as it does every scroll.
    pub fn zoom_move(
        &mut self,
        at: (f64, f64),
        view: &Viewport,
        lanes: LanesOrigin,
        keys: Mods,
    ) -> Option<Viewport> {
        let z = self.zoom.as_mut()?;
        z.drag.moved(at);
        let (time, rows) = z.drag.factors(keys.shift)?;
        let pps = (z.base_pps * time).clamp(PPS_RANGE.0, PPS_RANGE.1);
        let zoom_y = (z.base_zoom_y * rows).clamp(ZOOM_Y_RANGE.0, ZOOM_Y_RANGE.1);
        // Put what was under the press back under it, on both axes.
        let origin = z.drag.origin;
        let scroll_x = z.anchor_t.mul_add(pps, lanes.0 - origin.0);
        let scroll_y = z.anchor_y.mul_add(zoom_y, lanes.1 - origin.1);
        Some(Viewport {
            scroll_x,
            scroll_y,
            pps,
            zoom_y,
            width: view.width,
            height: view.height,
            panel_w: view.panel_w,
        })
    }

    /// The zoom tool lets go. An Alt sweep frames its box now; the
    /// continuous drag already zoomed on the way. Returns the view to
    /// show, if the release changes it.
    pub fn zoom_release(&mut self, view: &Viewport, lanes: LanesOrigin) -> Option<Viewport> {
        let z = self.zoom.take()?;
        // A sweep that went somewhere; a click frames nothing.
        let ((x0, y0), (x1, y1)) = z.drag.release()?;
        let zoom_y = if view.zoom_y > 0.0 { view.zoom_y } else { 1.0 };
        let t0 = (x0 - lanes.0 + view.scroll_x) / view.pps.max(f64::EPSILON);
        let t1 = (x1 - lanes.0 + view.scroll_x) / view.pps.max(f64::EPSILON);
        let r0 = (y0 - lanes.1 + view.scroll_y) / zoom_y;
        let r1 = (y1 - lanes.1 + view.scroll_y) / zoom_y;
        let lanes_w = (view.width - (lanes.0 - crate::rails::SIDE)).max(1.0);
        let lanes_h = (view.height - (lanes.1 - crate::rails::TOP)).max(1.0);
        let pps = (lanes_w / (t1 - t0).max(1e-6)).clamp(PPS_RANGE.0, PPS_RANGE.1);
        let zoom = (lanes_h / (r1 - r0).max(1e-6)).clamp(ZOOM_Y_RANGE.0, ZOOM_Y_RANGE.1);
        Some(Viewport {
            scroll_x: t0 * pps,
            scroll_y: r0 * zoom,
            pps,
            zoom_y: zoom,
            width: view.width,
            height: view.height,
            panel_w: view.panel_w,
        })
    }

    /// The Alt sweep's box, in window pixels, while one is being drawn.
    #[must_use]
    pub fn zoom_marquee(&self) -> Option<((f64, f64), (f64, f64))> {
        self.zoom.and_then(|z| z.drag.sweep_box())
    }

    /// Whether a drag is in flight — what the edge autoscroll asks.
    #[must_use]
    pub fn dragging(&self) -> bool {
        self.item_press.as_ref().is_some_and(|p| p.ghost.is_some())
            || self.fade_drag.is_some()
            || self.time_drag.is_some()
            || self.zoom.is_some()
    }

    /// The ghost of an item being moved or trimmed: its index and span.
    ///
    /// Also the ghost of a refused one going home — the press is gone by
    /// then, so a caller asking "what is in flight" would otherwise be
    /// told nothing while something is still moving on screen.
    #[must_use]
    pub fn ghost(&self) -> Option<(usize, f64, f64)> {
        if let Some(back) = self.snapback.as_ref().and_then(Snapback::span) {
            return Some(back);
        }
        self.item_press
            .as_ref()
            // A slip has nothing to ghost. The item lands exactly where
            // it is, so the ghost would be an outline and a pale wash
            // over the item's own span — saying nothing, and dimming
            // the one thing the gesture exists to let you read.
            .filter(|p| p.slipped.is_none())
            .and_then(|p| p.ghost.map(|(x0, x1)| (p.index, x0, x1)))
    }

    /// Whether a refused ghost is still travelling, so the window knows
    /// to keep asking for frames.
    #[must_use]
    pub fn settling(&self) -> bool {
        self.snapback
            .as_ref()
            .is_some_and(|back| back.progress().is_some())
    }

    /// The item being slipped and the offset the drag has reached.
    ///
    /// Separate from [`Self::ghost`] because a slip has no ghost worth
    /// drawing — the item does not move, so its span is its own and the
    /// thing that changes is inside it.
    #[must_use]
    pub fn slip_in_flight(&self) -> Option<(usize, f64)> {
        let press = self.item_press.as_ref()?;
        Some((press.index, press.slipped?))
    }

    /// The fade in flight: its item and where the fades are now.
    #[must_use]
    pub fn fade_in_flight(&self) -> Option<(usize, Fades)> {
        self.fade_drag.as_ref().map(|d| (d.index, d.fades))
    }

    /// Select an item — alone, or added — here and in the engine.
    pub fn select(
        &mut self,
        guid: &str,
        exclusive: bool,
        project: &mut Project,
        effects: &mut Vec<Effect>,
    ) {
        if exclusive {
            self.selected.clear();
        }
        // The folded guid is what the window holds — it is what a later
        // delete or drag will be aimed at, and what the row draws as
        // selected. The ENGINE is told about the mics, because those are
        // the items it has. Selecting a fragment is allowed: every mic
        // sounding under it is unambiguous, and refusing a click would
        // make the row feel broken rather than careful.
        let real = targets(project, guid, false, effects).unwrap_or_default();
        // The modified click TOGGLES. It used to only ever add, which
        // made building a selection up a piece at a time a one-way
        // gesture: the way to correct an over-click was to start again.
        // Exclusive is not a toggle — a plain click on the selected
        // item means "just this one", not "none".
        if !exclusive && self.selected.contains(guid) {
            self.selected.remove(guid);
            let selected = &self.selected;
            for item in project.items.values_mut().flatten() {
                item.selected = selected.contains(&item.guid);
            }
            for target in real {
                effects.push(Effect::Send(Edit::DeselectItem(target)));
            }
            return;
        }
        self.selected.insert(guid.to_owned());
        let selected = &self.selected;
        for item in project.items.values_mut().flatten() {
            item.selected = selected.contains(&item.guid) || real.contains(&item.guid);
        }
        let mut first = exclusive;
        for target in real {
            effects.push(Effect::Send(Edit::SelectItem(target, first)));
            // Only the first replaces the selection; the rest join it,
            // or the row would end up with one mic selected.
            first = false;
        }
    }

    /// A bound key. `false` when this cannot do it, so the key falls
    /// through to what the window binds itself.
    #[expect(
        clippy::too_many_arguments,
        reason = "everything a key can touch, passed rather than owned"
    )]
    pub fn key(
        &mut self,
        action: Action,
        project: &mut Project,
        scene: &Arrangement,
        rows: &[(Track, u32)],
        tracks: &mut [Track],
        row_to_track: &crate::plan::Rows,
        bpm: f64,
        effects: &mut Vec<Effect>,
    ) -> bool {
        match action {
            // The window has these when it can (`keys::use_window_transport_keys`),
            // so the space bar works whatever has the focus; acting on it
            // here too would play and stop in one press.
            Action::PlayStop | Action::PlayPause if crate::keys::window_has_transport() => {}
            Action::PlayStop | Action::PlayPause => {
                effects.push(Effect::Transport(Move::PlayStop, 0.0))
            }
            Action::GoToStart => {
                effects.push(Effect::Transport(Move::Home, 0.0));
                effects.push(Effect::Playhead(0.0));
                self.cursor.click(0.0);
            }
            Action::SplitAtCursor => self.split_at(self.cursor.at, project, effects),
            Action::CompTake(take) => return self.comp_razors(take, project, rows, effects),
            Action::DeleteSelectedItems => {
                // A razor area first of all. It is the most deliberate
                // thing on screen — you drew a rectangle and it is
                // still there — so while one exists it is what the key
                // is about, and the areas go with their contents
                // because an empty rectangle left behind would be a
                // second press waiting to happen.
                if !self.razor.is_empty() {
                    self.razor_delete(scene, project, effects);
                    self.razor.clear();
                    return true;
                }
                // The ruler next, and exclusively: a mark or a band
                // taken hold of is what the key means, and deleting
                // items as well would take away a selection the user
                // had stopped looking at.
                if let Some(on) = self.ruler_selection.take() {
                    use crate::ruler::On;
                    match on {
                        On::Marker { id } => {
                            effects.push(Effect::Send(Edit::RemoveMarker(String::new(), id)));
                        }
                        On::Region { id, .. } => {
                            effects.push(Effect::Send(Edit::RemoveRegion(String::new(), id)));
                        }
                        On::Lane { .. } | On::Bars => {}
                    }
                    return true;
                }
                let doomed: Vec<String> = self.selected.drain().collect();
                if doomed.is_empty() {
                    return true;
                }
                // A folded item is a view of the mics under it, so
                // deleting one deletes them. Resolved before anything is
                // removed, so a refusal leaves the selection's real
                // items alone rather than half-deleting the row.
                let mut real: Vec<String> = Vec::with_capacity(doomed.len());
                for guid in doomed {
                    let Some(targets) = targets(project, &guid, true, effects) else {
                        continue;
                    };
                    real.extend(targets);
                }
                if real.is_empty() {
                    return true;
                }
                for lane in project.items.values_mut() {
                    lane.retain(|i| !real.contains(&i.guid));
                }
                project.item_count = project.items.values().map(Vec::len).sum();
                let from = effects.len();
                for guid in real {
                    effects.push(Effect::Send(Edit::DeleteItem(guid)));
                }
                as_one_step("Delete items", from, effects);
                effects.push(Effect::ReRecord);
            }
            Action::SelectAllItems => {
                self.selected.clear();
                for item in project.items.values_mut().flatten() {
                    item.selected = true;
                    self.selected.insert(item.guid.clone());
                }
                effects.push(Effect::Send(Edit::SelectAllItems(String::new())));
            }
            Action::ClearSelection => {
                self.cursor.selection = None;
                self.selected.clear();
                for item in project.items.values_mut().flatten() {
                    item.selected = false;
                }
                effects.push(Effect::Send(Edit::DeselectAllItems(String::new())));
            }
            Action::CursorBar(by) | Action::CursorBeat(by) => {
                let step = if matches!(action, Action::CursorBar(_)) {
                    240.0
                } else {
                    60.0
                } / bpm.max(1.0);
                // To the grid line in that direction — from a cursor
                // between lines, the next line, not a step past it.
                let at = self.cursor.at / step;
                let to = if by > 0 {
                    (at + 1e-6).floor() + 1.0
                } else {
                    (at - 1e-6).ceil() - 1.0
                };
                self.cursor.at = (to * step).max(0.0);
            }
            Action::TrackStep { by, extend } => {
                let current = rows.iter().position(|(t, _)| t.selected);
                let next = current.map_or(0, |i| {
                    i.saturating_add_signed(isize::try_from(by).unwrap_or(0))
                });
                let next = next.min(rows.len().saturating_sub(1));
                let Some((track, _)) = rows.get(next) else {
                    return true;
                };
                let guid = track.guid.clone();
                if let Some(index) = row_to_track.index(next) {
                    if !extend {
                        for t in tracks.iter_mut() {
                            t.selected = false;
                        }
                    }
                    if let Some(t) = tracks.get_mut(index) {
                        t.selected = true;
                    }
                }
                effects.push(Effect::Send(if extend {
                    Edit::AddToSelection(guid)
                } else {
                    Edit::Select(guid)
                }));
                effects.push(Effect::ReRecord);
            }
            Action::Marker(by) => {
                let at = self.cursor.at;
                let mut times: Vec<f64> = scene.markers().iter().map(|m| m.at).collect();
                times.extend(scene.sections().iter().map(|s| s.start));
                times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let to = if by > 0 {
                    times.into_iter().find(|t| *t > at + 1e-3)
                } else {
                    times.into_iter().rev().find(|t| *t < at - 1e-3)
                };
                if let Some(to) = to {
                    self.cursor.click(to);
                    effects.push(Effect::Transport(Move::Seek, to));
                }
            }
            // The record key: the transport's record, which runs
            // `TRANSPORT_RECORD` (`crate::engine::transport`).
            Action::ToggleRecord => effects.push(Effect::Transport(Move::ToggleRecord, 0.0)),
            // The view's, not the session's: the widget sends these to
            // the panel before the editor is asked.
            Action::View(_)
            | Action::Visibility(_)
            | Action::ToggleScene(_)
            | Action::QuantizeDrums
            | Action::RevertDrums
            | Action::QuantizePanel
            | Action::SlipDrums { .. }
            | Action::Frame { .. }
            | Action::RemoveRange { .. }
            | Action::GainStage { .. }
            | Action::ToggleMixer
            | Action::ToggleEditor
            | Action::ToggleFx
            | Action::ToggleBrowser
            | Action::FxFull
            | Action::BarStep(_)
            | Action::PoiStep(_)
            | Action::PoiToggle
            | Action::Mode(_)
            | Action::Save
            | Action::Undo
            | Action::Redo => {
                return false;
            }
            Action::Unbound(id) => {
                tracing::info!(ui.action = %id, "bound in the profile, not built here yet");
                return false;
            }
        }
        true
    }

    /// The real items an area covers, with its edges made real first.
    ///
    /// Carving before answering is what makes every razor operation
    /// exact: after it, "the items in the area" is a set with no
    /// partial overlaps left to reason about, so the caller never has
    /// to decide what a half-covered take means. It is the same order
    /// `expression_editor_core::razor::carve` uses over notes, for the
    /// same reason.
    ///
    /// The guids that come back are the session's own. A folded row's
    /// items are views of the mics under them, so they go through
    /// [`targets`] like every other edit — which also means a ragged
    /// row refuses here, with the reason travelling the way it does
    /// everywhere else.
    fn razor_carve(
        &mut self,
        area: RazorArea,
        scene: &Arrangement,
        project: &mut Project,
        effects: &mut Vec<Effect>,
    ) -> Vec<String> {
        // Which LANES the area covers, worked out once.
        //
        // Once because `targets` is also where a ragged row refuses,
        // and asking twice would put the same refusal on screen twice.
        // Lanes rather than items because the halves a carve is about
        // to make do not exist yet: a lane is a row that will still
        // mean the same thing afterwards, and an item is not.
        let mut lanes: HashSet<String> = HashSet::new();
        for item in Self::boxes_in(scene, area)
            .map(|item| item.guid.clone())
            .collect::<Vec<_>>()
        {
            let Some(reals) = targets(project, &item, true, effects) else {
                continue;
            };
            for real in reals {
                if let Some(lane) = project
                    .items
                    .iter()
                    .find(|(_, items)| items.iter().any(|i| i.guid == real))
                    .map(|(lane, _)| lane.clone())
                {
                    lanes.insert(lane);
                }
            }
        }
        if lanes.is_empty() {
            return Vec::new();
        }

        // Both edges. Splitting at `t0` moves nothing at `t1`, so the
        // order is only about reading clearly.
        for edge in [area.t0, area.t1] {
            let crossing: HashSet<String> = lanes
                .iter()
                .filter_map(|lane| project.items.get(lane))
                .flatten()
                .filter(|item| {
                    let start = item.position.as_seconds();
                    let end = start + item.length.as_seconds();
                    start + 1e-3 < edge && end - 1e-3 > edge
                })
                .map(|item| item.guid.clone())
                .collect();
            Self::split_items(project, edge, &crossing, effects);
        }

        // And what is left inside, read from the project because that
        // is where the new halves are.
        lanes
            .iter()
            .filter_map(|lane| project.items.get(lane))
            .flatten()
            .filter(|item| {
                let start = item.position.as_seconds();
                let end = start + item.length.as_seconds();
                start >= area.t0 - 1e-3 && end <= area.t1 + 1e-3
            })
            .map(|item| item.guid.clone())
            .collect()
    }

    /// The scene's boxes the area touches.
    fn boxes_in(
        scene: &Arrangement,
        area: RazorArea,
    ) -> impl Iterator<Item = &crate::arrangement::ItemBox> {
        scene.item_boxes().iter().filter(move |item| {
            area.touches(
                i32::try_from(item.row).unwrap_or(i32::MAX),
                item.x0,
                item.x1,
            )
        })
    }

    /// Clear an area: carve its edges, then delete what is inside.
    ///
    /// `true` when anything went. The carve happens whether or not
    /// something is deleted, which is deliberate — an area over the
    /// middle of a take leaves that take in three pieces and the middle
    /// one gone, and the two survivors are the edges the area asked for.
    pub fn razor_delete(
        &mut self,
        scene: &Arrangement,
        project: &mut Project,
        effects: &mut Vec<Effect>,
    ) -> bool {
        let areas = self.razor.areas.clone();
        // The bracket spans every area, not one each: pressing Delete
        // with three areas drawn is one gesture, and an undo that took
        // back the third of them would leave a session nobody asked
        // for.
        let from = effects.len();
        let mut went = false;
        for area in areas {
            let doomed = self.razor_carve(area, scene, project, effects);
            if doomed.is_empty() {
                continue;
            }
            for guid in &doomed {
                effects.push(Effect::Send(Edit::DeleteItem(guid.clone())));
            }
            let gone: HashSet<&String> = doomed.iter().collect();
            for lane in project.items.values_mut() {
                lane.retain(|item| !gone.contains(&item.guid));
            }
            went = true;
        }
        if went {
            project.item_count = project.items.values().map(Vec::len).sum();
            as_one_step("Delete razor contents", from, effects);
            effects.push(Effect::ReRecord);
        }
        went
    }

    /// Split every item in `mine` that `at` falls inside.
    ///
    /// The half that [`Self::split_at`] and the razor both need. The
    /// naming rule — `{guid}-split@{t}` — is the same in both because
    /// the engine has to recognise the right-hand half whichever
    /// gesture made it.
    fn split_items(
        project: &mut Project,
        at: f64,
        mine: &HashSet<String>,
        effects: &mut Vec<Effect>,
    ) -> bool {
        let splits = cut(project, at, mine);
        if splits.is_empty() {
            return false;
        }
        let from = effects.len();
        for edit in splits {
            effects.push(Effect::Send(edit));
        }
        as_one_step("Split items", from, effects);
        effects.push(Effect::ReRecord);
        true
    }

    /// The folded rows selected, each as `(row, folder, from, to)`: what
    /// a quantize reads the hits of.
    #[must_use]
    pub fn selected_folds(&self, project: &Project) -> Vec<(String, String, f64, f64)> {
        project
            .folds
            .iter()
            .flat_map(|(folder, fold)| {
                fold.items
                    .iter()
                    .filter(|i| self.selected.contains(&i.guid))
                    .map(|i| {
                        let start = i.position.as_seconds();
                        (
                            i.guid.clone(),
                            folder.clone(),
                            start,
                            start + i.length.as_seconds(),
                        )
                    })
            })
            .collect()
    }

    /// Slip-edit the kit under each of `rows` (a folded row and its
    /// moves, `(hit, shift)` in seconds): every mic cut a moment before
    /// each hit, then each slice moved by its hit's shift — on every mic
    /// at once, so the kit stays in phase with itself; the seams closed
    /// with short crossfades, a gap filled from the slice's own source
    /// (`close_seams`). No audio is stretched. One undo step, `label`.
    pub fn slip(
        project: &mut Project,
        rows: &[(String, Vec<(f64, f64)>)],
        label: &str,
        effects: &mut Vec<Effect>,
    ) {
        let from = effects.len();
        for (row, moves) in rows {
            let Some(mics) = targets(project, row, true, effects) else {
                continue;
            };
            slip_takes(project, &mics, moves, SlipCut::default(), effects);
        }
        if effects.len() > from {
            project.item_count = project.items.values().map(Vec::len).sum();
            as_one_step(label, from, effects);
            effects.push(Effect::ReRecord);
        }
    }

    /// [`Self::slip`] on these items — every take of a kit, however it is
    /// shown — as one: one undo step, `label`.
    pub fn slip_items(
        project: &mut Project,
        items: &[String],
        moves: &[(f64, f64)],
        (label, cut): (&str, SlipCut),
        effects: &mut Vec<Effect>,
    ) {
        let from = effects.len();
        slip_takes(project, items, moves, cut, effects);
        if effects.len() > from {
            project.item_count = project.items.values().map(Vec::len).sum();
            as_one_step(label, from, effects);
            effects.push(Effect::ReRecord);
        }
    }
}

/// The fade across each edge a removal leaves, seconds: enough that the
/// cut does not click.
pub const REMOVE_FADE: f64 = 0.002;

/// A piece left past a removal shorter than this, beside a longer one
/// starting where it does, is a crossfade's tail, seconds.
const REMOVE_TAIL: f64 = 0.010;

impl Editor {
    /// Remove `from..to` from the takes `items` (item guids): each cut at
    /// both ends, what lies between deleted, a short fade on each edge
    /// left — a stray hit taken out of a trigger, or a hit nobody meant
    /// out of the whole kit. One undo step.
    pub fn remove_range(
        project: &mut Project,
        items: &[String],
        (from, to): (f64, f64),
        label: &str,
        effects: &mut Vec<Effect>,
    ) {
        use daw_proto::primitives::Duration;
        if to <= from {
            return;
        }
        let start = effects.len();
        // A set, not a scan: a slip-edited kit is tens of thousands of
        // takes, and every item of the song is asked about.
        let takes: HashSet<&str> = items.iter().map(String::as_str).collect();
        let pieces = |project: &Project| -> HashSet<String> {
            project
                .items
                .values()
                .flatten()
                .filter(|i| of_mics(&i.guid, &takes))
                .map(|i| i.guid.clone())
                .collect()
        };
        let mine = pieces(project);
        effects.extend(cut(project, from, &mine).into_iter().map(Effect::Send));
        let mine = pieces(project);
        effects.extend(cut(project, to, &mine).into_iter().map(Effect::Send));
        let mine = pieces(project);
        let shape = daw_proto::item::FadeShape::default();
        for lane in project.items.values_mut() {
            let before = lane.len();
            // Where two pieces start at the far edge, the shorter is the
            // tail of a slice that reached over its seam by the
            // crossfade: it goes too, or it plays on over the next.
            let starting: Vec<(String, f64)> = lane
                .iter()
                .filter(|i| mine.contains(&i.guid) && (i.position.as_seconds() - to).abs() < 1e-6)
                .map(|i| (i.guid.clone(), i.length.as_seconds()))
                .collect();
            let longest = starting.iter().map(|s| s.1).fold(0.0, f64::max);
            let tails: HashSet<String> = starting
                .into_iter()
                .filter(|(_, length)| *length < REMOVE_TAIL && *length < longest)
                .map(|(guid, _)| guid)
                .collect();
            lane.retain(|i| {
                let (a, b) = (
                    i.position.as_seconds(),
                    i.position.as_seconds() + i.length.as_seconds(),
                );
                let inside = (mine.contains(&i.guid) && a >= from - 1e-6 && b <= to + 1e-6)
                    || tails.contains(&i.guid);
                if inside {
                    effects.push(Effect::Send(Edit::DeleteItem(i.guid.clone())));
                }
                !inside
            });
            if lane.len() == before {
                continue;
            }
            for item in lane.iter_mut().filter(|i| mine.contains(&i.guid)) {
                let (a, b) = (
                    item.position.as_seconds(),
                    item.position.as_seconds() + item.length.as_seconds(),
                );
                if (b - from).abs() < 1e-6 {
                    item.fade_out_length = Duration::from_seconds(REMOVE_FADE);
                    effects.push(Effect::Send(Edit::SetFadeOut(
                        item.guid.clone(),
                        REMOVE_FADE,
                        shape,
                    )));
                }
                if (a - to).abs() < 1e-6 {
                    item.fade_in_length = Duration::from_seconds(REMOVE_FADE);
                    effects.push(Effect::Send(Edit::SetFadeIn(
                        item.guid.clone(),
                        REMOVE_FADE,
                        shape,
                    )));
                }
            }
        }
        project.item_count = project.items.values().map(Vec::len).sum();
        if effects.len() > start {
            as_one_step(label, start, effects);
            effects.push(Effect::ReRecord);
        }
    }
}

/// The slip itself, on `mics` (item guids — a kit's takes) by `moves`:
/// every take cut a moment before each hit, each slice moved by its
/// hit's shift, the seams closed.
///
/// One pass, whatever the size: the cuts sorted once, each take cut at
/// every cut inside it left to right, each lane sorted once after. A
/// piece is named after the take it is cut from and where
/// (`{take}-split@{t}`), never after the piece before — names that grew
/// with every cut were what made a whole song's slip take a minute.
fn slip_takes(
    project: &mut Project,
    mics: &[String],
    moves: &[(f64, f64)],
    cut: SlipCut,
    effects: &mut Vec<Effect>,
) {
    use daw_proto::primitives::{Duration, PositionInSeconds};
    let mic_set: HashSet<&str> = mics.iter().map(String::as_str).collect();
    let (mut start, mut end) = (f64::INFINITY, f64::NEG_INFINITY);
    for item in project.items.values().flatten() {
        if mic_set.contains(item.guid.as_str()) {
            let at = item.position.as_seconds();
            start = start.min(at);
            end = end.max(at + item.length.as_seconds());
        }
    }
    if !start.is_finite() {
        return;
    }
    // Every cut first, in the take's own time, then every move. Cut and
    // move hit by hit and each move shifts the audio the next cut is
    // placed on: the error runs down the take.
    let cut_lead = cut.pre_roll;
    let mut cuts: Vec<(f64, f64)> = moves
        .iter()
        .map(|&(hit, shift)| (hit - cut.pre_roll, shift))
        .filter(|(at, _)| *at > start + cut.pre_roll && *at < end - cut.pre_roll)
        .collect();
    // A cut just after where a slice already starts is that slice's hit:
    // a second pass over an edited kit moves the slices it has, rather
    // than cutting a sliver off each beside its seam. Just after, because
    // a slice moved later reached back over its gap — its start is up to
    // its shift before its hit.
    let mut starts: Vec<f64> = project
        .items
        .values()
        .flatten()
        // The set, not a scan of every mic for every item: on a
        // slip-edited kit that was 25 000 × 25 000 and froze a hit's drag
        // for seconds.
        .filter(|i| of_mics(&i.guid, &mic_set))
        .map(|i| i.position.as_seconds())
        .collect();
    starts.sort_by(f64::total_cmp);
    // The cuts that are seams already: a move, never a split — the slice
    // before reaches over its seam by the crossfade, and must not be cut
    // there.
    let mut seams: Vec<f64> = Vec::new();
    // Each seam's slice and where its hit is in it: what the seams know
    // not to reach back past.
    let mut seam_hits: Vec<(f64, f64)> = Vec::new();
    for cut in &mut cuts {
        let at = starts.partition_point(|s| *s <= cut.0 + SEAM_SNAP);
        if let Some(seam) = at
            .checked_sub(1)
            .and_then(|i| starts.get(i))
            .copied()
            .filter(|s| cut.0 - s <= SEAM_REACH)
        {
            seam_hits.push((seam, cut.0 + cut_lead - seam));
            cut.0 = seam;
            seams.push(seam);
        }
    }
    seams.sort_by(f64::total_cmp);
    let is_seam = |at: f64| {
        let i = seams.partition_point(|s| *s < at - 1e-9);
        seams.get(i).is_some_and(|s| (s - at).abs() <= 1e-9)
    };
    cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
    cuts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9);
    for lane in project.items.values_mut() {
        if !lane.iter().any(|i| mic_set.contains(i.guid.as_str())) {
            continue;
        }
        let mut added: Vec<daw_proto::Item> = Vec::new();
        for item in lane
            .iter_mut()
            .filter(|i| mic_set.contains(i.guid.as_str()))
        {
            let take = item.guid.clone();
            let take_start = item.position.as_seconds();
            let take_end = take_start + item.length.as_seconds();
            let from = cuts.partition_point(|c| c.0 <= take_start + 1e-3);
            let mut pieces: Vec<daw_proto::Item> = Vec::new();
            let mut last = item.clone();
            for &(at, _) in cuts
                .iter()
                .skip(from)
                .take_while(|c| c.0 < take_end - 1e-3)
                .filter(|c| !is_seam(c.0))
            {
                let left_start = last.position.as_seconds();
                let mut right = last.clone();
                right.guid = format!("{take}-split@{at:.3}");
                right.position = PositionInSeconds::from_seconds(at);
                right.length = Duration::from_seconds(take_end - at);
                right.fade_in_length = Duration::from_seconds(0.0);
                // As far into the source as the cut is into the piece: the
                // right half plays on from where the left stopped.
                right.start_offset =
                    Duration::from_seconds(last.start_offset.as_seconds() + (at - left_start));
                // Its hit is the lead into it: where the next edit's
                // seams know not to reach back past.
                right.snap_offset = Duration::from_seconds(cut.pre_roll);
                last.length = Duration::from_seconds(at - left_start);
                last.fade_out_length = Duration::from_seconds(0.0);
                effects.push(Effect::Send(Edit::SplitItem(
                    last.guid.clone(),
                    at,
                    right.guid.clone(),
                )));
                effects.push(Effect::Send(Edit::SetSnapOffset(
                    right.guid.clone(),
                    cut.pre_roll,
                )));
                pieces.push(std::mem::replace(&mut last, right));
            }
            pieces.push(last);
            let mut pieces = pieces.into_iter();
            if let Some(first) = pieces.next() {
                *item = first;
            }
            added.extend(pieces);
        }
        lane.extend(added);
        lane.sort_by(|a, b| a.position.as_seconds().total_cmp(&b.position.as_seconds()));
    }
    project.item_count = project.items.values().map(Vec::len).sum();
    // Each piece that starts at a cut, moved by its hit's shift.
    let shift_at = |pos: f64| {
        let at = cuts.partition_point(|c| c.0 < pos - 1e-6);
        cuts.get(at)
            .filter(|c| (c.0 - pos).abs() < 1e-6)
            .map(|c| c.1)
    };
    // Where every slice of the kit is before the moves, and where in its
    // source it starts: a hit's time then is a place in its source.
    let was_at: HashMap<String, (f64, f64)> = project
        .items
        .values()
        .flatten()
        .filter(|i| of_mics(&i.guid, &mic_set))
        .map(|i| {
            (
                i.guid.clone(),
                (i.position.as_seconds(), i.start_offset.as_seconds()),
            )
        })
        .collect();
    let snap_at = |pos: f64| {
        seam_hits
            .iter()
            .find(|(seam, _)| (seam - pos).abs() < 1e-6)
            .map(|(_, into)| *into)
    };
    for item in project.items.values_mut().flatten() {
        let pos = item.position.as_seconds();
        if !of_mics(&item.guid, &mic_set) {
            continue;
        }
        // A slice already there that a hit is in: where.
        if let Some(into) = snap_at(pos)
            && item.snap_offset.as_seconds() <= 0.0
        {
            item.snap_offset = Duration::from_seconds(into);
            effects.push(Effect::Send(Edit::SetSnapOffset(item.guid.clone(), into)));
        }
        if let Some(shift) = shift_at(pos)
            && shift.abs() >= 1e-4
        {
            let landing = pos + shift;
            item.position = PositionInSeconds::from_seconds(landing);
            effects.push(Effect::Send(Edit::MoveItem(item.guid.clone(), landing)));
        }
    }
    // Every seam the moves touched, a slice either side of each.
    let touched = cuts
        .iter()
        .filter(|c| c.1.abs() >= 1e-4)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), c| {
            (lo.min(c.0), hi.max(c.0))
        });
    if touched.0.is_finite() {
        let window = (touched.0 - SEAM_WINDOW, touched.1 + SEAM_WINDOW);
        close_seams(project, (&mic_set, &was_at), window, &cut, effects);
    }
}

/// How far either side of an edit's moves its seams are closed, seconds:
/// past the slice before the first and the one after the last.
const SEAM_WINDOW: f64 = 1.0;

/// The comp lane of a track with fixed lanes: the first named `C…` (REAPER
/// 7's `C1`) or `COMP` (the kit's, `drum_lanes::COMP`), else the first
/// lane that plays. `None` for a track without lanes.
#[must_use]
pub fn comp_lane(track: &daw_proto::Track) -> Option<u32> {
    if track.lane_count == 0 {
        return None;
    }
    let named = track.lane_names.iter().position(|name| {
        name.trim().eq_ignore_ascii_case("COMP")
            || name
                .strip_prefix('C')
                .is_some_and(|rest| rest.is_empty() || rest.chars().all(|c| c.is_ascii_digit()))
    });
    named.and_then(|lane| u32::try_from(lane).ok()).or_else(|| {
        (0..track.lane_count.min(64)).find(|lane| track.lane_play_mask & (1_u64 << lane) != 0)
    })
}

/// The lane take `number` is on: the lane named for it (`"2"`), else lane
/// `number` itself.
fn take_lane(track: &daw_proto::Track, number: u32) -> u32 {
    let name = number.to_string();
    track
        .lane_names
        .iter()
        .position(|lane| lane.trim() == name)
        .and_then(|lane| u32::try_from(lane).ok())
        .unwrap_or(number)
}

/// Every track under the folder `folder`, however deep — or, for a track
/// that is not a folder, the track itself.
fn under(project: &Project, folder: &str) -> Vec<String> {
    if project
        .tracks
        .iter()
        .any(|t| t.guid == folder && !t.is_folder)
    {
        return vec![folder.to_owned()];
    }
    let mut inside: HashSet<String> = HashSet::from([folder.to_owned()]);
    let mut out = Vec::new();
    // Tracks come parent before child, so one walk finds every level.
    for track in &project.tracks {
        if track
            .parent_guid
            .as_deref()
            .is_some_and(|parent| inside.contains(parent))
        {
            inside.insert(track.guid.clone());
            out.push(track.guid.clone());
        }
    }
    out
}

impl Editor {
    /// Swipe-comp a folder: every track under `folder` (or `folder` itself,
    /// a track with lanes) takes take `take`'s audio — the lane named
    /// `take` ("1", "2"), else lane `take` — for `from..to` onto its comp lane
    /// ([`comp_lane`]) — the whole kit from one take at once, so its mics
    /// stay one performance, in phase. What the comp lane held there is
    /// cut away; what it held either side stays. The pieces meet with a
    /// short crossfade. One undo step.
    pub fn comp(
        project: &mut Project,
        folder: &str,
        take: u32,
        from: f64,
        to: f64,
        effects: &mut Vec<Effect>,
    ) {
        const JOIN: f64 = 0.005;
        if to <= from {
            return;
        }
        let start = effects.len();
        let secs = daw_proto::primitives::Duration::from_seconds;
        let at = daw_proto::primitives::PositionInSeconds::from_seconds;
        for guid in under(project, folder) {
            let Some(track) = project.tracks.iter().find(|t| t.guid == guid).cloned() else {
                continue;
            };
            let take = take_lane(&track, take);
            let Some(comp) = comp_lane(&track).filter(|comp| *comp != take) else {
                continue;
            };
            let on = |item: &daw_proto::Item, lane: u32| {
                let start = item.position.as_seconds();
                let end = start + item.length.as_seconds();
                item.fixed_lane == Some(lane) && end > from && start < to
            };
            // The comp lane cleared over the range: cut at its edges, and
            // the pieces inside gone. Gathered again for each edge: the
            // first cut's right half is a new item the second must find.
            for edge in [from, to] {
                let comped: HashSet<String> = project
                    .items
                    .get(&guid)
                    .into_iter()
                    .flatten()
                    .filter(|item| on(item, comp))
                    .map(|item| item.guid.clone())
                    .collect();
                effects.extend(cut(project, edge, &comped).into_iter().map(Effect::Send));
            }
            if let Some(lane) = project.items.get_mut(&guid) {
                lane.retain(|item| {
                    let inside = on(item, comp)
                        && item.position.as_seconds() >= from - 1e-6
                        && item.position.as_seconds() + item.length.as_seconds() <= to + 1e-6;
                    if inside {
                        effects.push(Effect::Send(Edit::DeleteItem(item.guid.clone())));
                    }
                    !inside
                });
            }
            // The take's audio over the range, onto the comp lane.
            let sources: Vec<daw_proto::Item> = project
                .items
                .get(&guid)
                .into_iter()
                .flatten()
                .filter(|item| on(item, take))
                .cloned()
                .collect();
            for source in sources {
                let begin = source.position.as_seconds();
                let piece_from = from.max(begin);
                let piece_to = to.min(begin + source.length.as_seconds());
                let mut piece = source.clone();
                piece.guid = format!("{}-comp@{piece_from:.3}", source.guid);
                piece.fixed_lane = Some(comp);
                piece.position = at(piece_from);
                piece.length = secs(piece_to - piece_from);
                piece.start_offset = secs(source.start_offset.as_seconds() + (piece_from - begin));
                piece.fade_in_length = secs(JOIN);
                piece.fade_out_length = secs(JOIN);
                piece.selected = false;
                effects.extend(
                    [
                        Edit::CopyItem(source.guid.clone(), begin, piece.guid.clone()),
                        Edit::SetItemLane(piece.guid.clone(), comp),
                        Edit::TrimItem(piece.guid.clone(), piece_from, piece_to - piece_from),
                        Edit::SlipItem(piece.guid.clone(), piece.start_offset.as_seconds()),
                        Edit::SetFadeIn(piece.guid.clone(), JOIN, piece.fade_in_shape),
                        Edit::SetFadeOut(piece.guid.clone(), JOIN, piece.fade_out_shape),
                    ]
                    .into_iter()
                    .map(Effect::Send),
                );
                if let Some(lane) = project.items.get_mut(&guid) {
                    lane.push(piece);
                    lane.sort_by(|a, b| {
                        a.position.as_seconds().total_cmp(&b.position.as_seconds())
                    });
                }
            }
        }
        if effects.len() > start {
            project.item_count = project.items.values().map(Vec::len).sum();
            as_one_step("Comp", start, effects);
            effects.push(Effect::ReRecord);
        }
    }
    /// Ctrl+N: comp every razor area from take N ([`Self::comp`]) — on a
    /// folded row, the whole folder; on a track with lanes, that track.
    /// The areas go once used. `false` with no razor drawn.
    pub fn comp_razors(
        &mut self,
        take: u32,
        project: &mut Project,
        rows: &[(Track, u32)],
        effects: &mut Vec<Effect>,
    ) -> bool {
        if self.razor.is_empty() {
            return false;
        }
        let areas = self.razor.areas.clone();
        for area in &areas {
            for row in area.row_lo..=area.row_hi {
                let Some((track, _)) = usize::try_from(row).ok().and_then(|row| rows.get(row))
                else {
                    continue;
                };
                let guid = track.guid.clone();
                Self::comp(project, &guid, take, area.t0, area.t1, effects);
            }
        }
        self.razor.clear();
        true
    }
}

/// Whether `guid` is one of `mics` or a piece a cut made of one — at any
/// depth (`{mic}-split@{t}`, `{mic}-split@{t}-split@{u}`).
fn of_mics(guid: &str, mics: &HashSet<&str>) -> bool {
    mics.contains(guid)
        || guid
            .match_indices("-split@")
            .any(|(at, _)| guid.get(..at).is_some_and(|base| mics.contains(base)))
}

/// Where a slice's hit is in its source, seconds: its snap offset into
/// it, which a cut sets to the lead before the hit — a slice cut before
/// snap offsets were kept is taken to have the lead.
fn attack_of(piece: &daw_proto::Item, lead: f64) -> f64 {
    let snap = piece.snap_offset.as_seconds();
    let into = if snap > 0.0 {
        snap
    } else if piece.guid.contains("-split@") {
        lead
    } else {
        0.0
    };
    piece.start_offset.as_seconds() + into
}

/// How far apart two slices' sources may be at their seam and still be
/// one recording, seconds: more than any shift a quantize makes.
const SAME_TAKE: f64 = 0.25;

// r[impl quantize.seams]
/// The second half of a quantize: the seams between the kit's slices
/// closed, track by track, from `from` to `to` — every slice there,
/// whichever edit cut it — with a short crossfade across each while
/// auto-crossfade is on. Where a slice moved earlier it runs over the one
/// before, which is trimmed to meet it. Where it moved later it left a
/// gap, which it fills itself — its start pulled back into its own
/// source, the ring of the hit before — rather than the slice before
/// running on into it: that one would play on past its cut into this
/// hit's attack, a flam. Never back past the hit before (its snap
/// offset says where): what is left, the slice before covers up to this
/// hit's attack.
fn close_seams(
    project: &mut Project,
    (mics, was_at): (&HashSet<&str>, &HashMap<String, (f64, f64)>),
    (from, to): (f64, f64),
    cut: &SlipCut,
    effects: &mut Vec<Effect>,
) {
    let fade = if crate::options::AUTO_CROSSFADE.get() {
        cut.crossfade
    } else {
        0.0
    };
    for lane in project.items.values_mut() {
        let mut by_lane: std::collections::BTreeMap<Option<u32>, Vec<&mut daw_proto::Item>> =
            std::collections::BTreeMap::new();
        for item in lane.iter_mut() {
            let at = item.position.as_seconds();
            let end = at + item.length.as_seconds();
            if of_mics(&item.guid, mics) && end > from && at < to {
                by_lane.entry(item.fixed_lane).or_default().push(item);
            }
        }
        for pieces in by_lane.values_mut() {
            pieces.sort_by(|a, b| a.position.as_seconds().total_cmp(&b.position.as_seconds()));
            seams(pieces, (cut, fade, was_at), effects);
        }
    }
}

/// [`close_seams`] on one lane's slices, by position.
fn seams(
    pieces: &mut [&mut daw_proto::Item],
    (cut, fade, was_at): (&SlipCut, f64, &HashMap<String, (f64, f64)>),
    effects: &mut Vec<Effect>,
) {
    use daw_proto::primitives::{Duration, PositionInSeconds};
    // Where each slice started in its source before any reached back.
    let cut_at: Vec<f64> = pieces.iter().map(|p| p.start_offset.as_seconds()).collect();
    let mut prev_end: Option<f64> = None;
    for k in 0..pieces.len() {
        let (before, rest) = pieces.split_at_mut(k);
        let Some(piece) = rest.first_mut() else {
            continue;
        };
        let start = piece.position.as_seconds();
        // A long gap, the last resort: the slice before drawn out over it.
        if let (Some(ends), Some(over), Some(prev)) =
            (prev_end, cut.stretch_over, before.last_mut())
            && start - ends > over
            && let Some(markers) = draw_out(prev, start + fade)
        {
            effects.push(Effect::Send(Edit::TrimItem(
                prev.guid.clone(),
                prev.position.as_seconds(),
                prev.length.as_seconds(),
            )));
            effects.push(Effect::Send(Edit::SetStretchMarkers(
                prev.guid.clone(),
                markers,
            )));
            prev_end = Some(prev.position.as_seconds() + prev.length.as_seconds());
        }
        // The slice before, when it is the same recording running on.
        let was_cut = cut_at.get(k).copied().unwrap_or_default();
        let joined = before.last().is_some_and(|prev| {
            (prev.start_offset.as_seconds() + prev.length.as_seconds() - was_cut).abs() <= SAME_TAKE
        });
        // Every seam overlaps by the crossfade: two slices that only meet
        // — let alone leave a gap — fade one out to nothing as the other
        // fades in from it, and the kit drops out at the seam.
        if let Some(ends) = prev_end
            && start > ends - fade + 1e-9
        {
            // A gap: this slice reaches back to meet the one before — but
            // never into the hit before it. That one's attack is a lead
            // into the slice before; reached past, it plays twice (a
            // snare 64 ms before a kick sent 58 ms later did).
            // And never over any hit at all in its own source before its
            // own — one the slip did not cut on (a flam's grace riding in
            // its stroke's slice) is as much an attack to play twice.
            let uncut = was_at.get(&piece.guid).and_then(|&(at, from)| {
                let own = at + (attack_of(piece, cut.pre_roll) - from);
                let i = cut.attacks.partition_point(|t| *t < own - SEAM_GUARD);
                let latest = i.checked_sub(1).and_then(|i| cut.attacks.get(i))?;
                // A hit the slip did not cut on is often this moment's own
                // (another drum heard a few ms sooner): past its first
                // instant only.
                Some(from + (latest - at) + SEAM_GUARD)
            });
            let floor = before
                .last()
                .filter(|_| joined)
                .map(|prev| attack_of(prev, cut.pre_roll) + RING_GUARD)
                .into_iter()
                .chain(uncut)
                .reduce(f64::max)
                .filter(|f| *f < piece.start_offset.as_seconds());
            let room = if joined {
                floor.map_or(0.0, |f| piece.start_offset.as_seconds() - f)
            } else {
                0.0
            };
            let reach = (start - ends + fade).min(room).max(0.0);
            if reach > 0.0 {
                let (at, length) = (start - reach, piece.length.as_seconds() + reach);
                let offset = (piece.start_offset.as_seconds() - reach).max(0.0);
                let snap = attack_of(piece, cut.pre_roll) - offset;
                piece.position = PositionInSeconds::from_seconds(at);
                piece.length = Duration::from_seconds(length);
                piece.start_offset = Duration::from_seconds(offset);
                piece.snap_offset = Duration::from_seconds(snap);
                effects.push(Effect::Send(Edit::TrimItem(piece.guid.clone(), at, length)));
                effects.push(Effect::Send(Edit::SlipItem(piece.guid.clone(), offset)));
                effects.push(Effect::Send(Edit::SetSnapOffset(piece.guid.clone(), snap)));
            }
            // What reaching back could not cover — a hit too soon after
            // the one before — the slice before covers, running on into
            // this hit's lead-in: its source goes on where this slice's
            // was cut, and it stops short of the attack.
            let left = piece.position.as_seconds() - (ends - fade);
            if left > 1e-6
                && joined
                && let Some(prev) = before.last_mut()
            {
                let source_end = prev.start_offset.as_seconds() + prev.length.as_seconds();
                // This slice's hit: the lead into it — or, a slice with no
                // hit in it (a take's own start, no cut, no snap), none:
                // the slice before may run on over all of it.
                let hit_free =
                    piece.snap_offset.as_seconds() <= 0.0 && !piece.guid.contains("-split@");
                let attack = if hit_free {
                    piece.start_offset.as_seconds() + piece.length.as_seconds()
                } else {
                    was_cut + cut.pre_roll
                };
                let more = left.min(attack - SEAM_GUARD - source_end).max(0.0);
                if more > 0.0 {
                    let length = prev.length.as_seconds() + more;
                    prev.length = Duration::from_seconds(length);
                    effects.push(Effect::Send(Edit::TrimItem(
                        prev.guid.clone(),
                        prev.position.as_seconds(),
                        length,
                    )));
                }
            }
            // Still short — two hits a gap apart, each side slipped as far
            // as it can without playing an attack twice — the slice before
            // drawn out over what is left, its decay slowed: never below
            // [`STRETCH_LEAST_RATE`] near its hit (slower, the overheads'
            // high end went with it — the engine stretches by rate), and
            // where that cannot reach, slower only in the space between
            // hits, its far tail ([`draw_out_between`]).
            if let Some(prev) = before.last_mut() {
                let meets = piece.position.as_seconds() + fade;
                let ends = prev.position.as_seconds() + prev.length.as_seconds();
                let drawn = if meets - ends > 1e-6 {
                    draw_out_to(prev, meets, STRETCH_LEAST_RATE)
                        .or_else(|| draw_out_between(prev, meets))
                } else {
                    None
                };
                if let Some(markers) = drawn {
                    effects.push(Effect::Send(Edit::TrimItem(
                        prev.guid.clone(),
                        prev.position.as_seconds(),
                        prev.length.as_seconds(),
                    )));
                    effects.push(Effect::Send(Edit::SetStretchMarkers(
                        prev.guid.clone(),
                        markers,
                    )));
                }
            }
        }
        if k > 0 && fade > 0.0 {
            piece.fade_in_length = Duration::from_seconds(fade);
            effects.push(Effect::Send(Edit::SetFadeIn(
                piece.guid.clone(),
                fade,
                daw_proto::item::FadeShape::default(),
            )));
        }
        prev_end = Some(piece.position.as_seconds() + piece.length.as_seconds());
    }
    trim_overlaps(pieces, fade, effects);
}

/// The overlaps of `pieces` (by position), from the end back: each slice
/// that runs past the next one's start plus the crossfade stops there,
/// and fades out across it.
fn trim_overlaps(pieces: &mut [&mut daw_proto::Item], fade: f64, effects: &mut Vec<Effect>) {
    use daw_proto::primitives::Duration;
    let mut next_start: Option<f64> = None;
    for piece in pieces.iter_mut().rev() {
        let start = piece.position.as_seconds();
        if let Some(next) = next_start {
            let length = next + fade - start;
            if length > 0.0 && length < piece.length.as_seconds() - 1e-6 {
                piece.length = Duration::from_seconds(length);
                effects.push(Effect::Send(Edit::TrimItem(
                    piece.guid.clone(),
                    start,
                    length,
                )));
            }
            if fade > 0.0 {
                piece.fade_out_length = Duration::from_seconds(fade);
                effects.push(Effect::Send(Edit::SetFadeOut(
                    piece.guid.clone(),
                    fade,
                    daw_proto::item::FadeShape::default(),
                )));
            }
        }
        next_start = Some(start);
    }
}

/// The kit's takes an edit works on ([`crate::drum_edit::is_edited`]):
/// every audio track under the top-level Drums folder — never its Process
/// — or, `only` named, those tracks of it.
#[must_use]
pub fn kit_items(project: &Project, only: &[String]) -> Vec<String> {
    let Some(drums) = project
        .tracks
        .iter()
        .find(|t| {
            t.parent_guid.is_none() && t.is_folder && t.name.trim().eq_ignore_ascii_case("drums")
        })
        .map(|t| t.guid.clone())
    else {
        return Vec::new();
    };
    let mut inside: std::collections::HashSet<String> = std::collections::HashSet::from([drums]);
    let mut kit: Vec<&daw_proto::Track> = Vec::new();
    for track in &project.tracks {
        let Some(parent) = track.parent_guid.as_deref() else {
            continue;
        };
        if !inside.contains(parent) || track.name.trim().eq_ignore_ascii_case("process") {
            continue;
        }
        inside.insert(track.guid.clone());
        // A piece's Sample track plays notes made from the hits
        // (`crate::drum_samples`): they follow the edit as notes, written
        // where the hits go — cut and moved as audio too, they moved twice.
        let derived = track.name.trim().to_ascii_lowercase().ends_with("sample");
        if !track.is_folder && !derived && (only.is_empty() || only.contains(&track.guid)) {
            kit.push(track);
        }
    }
    kit.iter()
        .flat_map(|track| {
            project
                .items
                .get(&track.guid)
                .into_iter()
                .flatten()
                .filter(|item| crate::drum_edit::is_edited(track, item))
                .map(|item| item.guid.clone())
        })
        .collect()
}

/// A slip's moves as the Action List queues them (`(hit, shift)`, µs) in
/// seconds.
#[must_use]
pub fn seconds_of(moves: &[(i64, i64)]) -> Vec<(f64, f64)> {
    let seconds = |us: i64| num_traits::ToPrimitive::to_f64(&us).unwrap_or(0.0) / 1_000_000.0;
    moves
        .iter()
        .map(|&(hit, shift)| (seconds(hit), seconds(shift)))
        .collect()
}

/// How much of a slice's start a stretch leaves as it was, seconds: its
/// attack — the part of a hit an ear times it by.
pub const STRETCH_KEEP: f64 = 0.030;

/// The slowest a stretched decay plays, as a rate: slower, the pitch it
/// takes on (the engine stretches by rate) is heard — a slice slowed to
/// a quarter of its speed took the overheads' high end with it.
pub const STRETCH_LEAST_RATE: f64 = 0.85;

/// How much of a slice's decay after its attack ([`STRETCH_KEEP`]) is
/// still the hit, seconds of its take: drawn out no slower than
/// [`STRETCH_LEAST_RATE`].
pub const STRETCH_BODY: f64 = 0.030;

/// The slowest the space between hits — a slice's far tail, past its
/// hit — is drawn out, as a rate: past this there is too little left to
/// draw, and the gap stays.
pub const TAIL_LEAST_RATE: f64 = 0.1;

/// `item` drawn out to end at `to` (or as near as it can), where
/// [`STRETCH_LEAST_RATE`] cannot reach all the way: its attack as it was,
/// the hit's body after it ([`STRETCH_BODY`], at most half the decay) at
/// that rate, and only the far tail — the space before the next hit —
/// slowed past it, as slow as it takes down to [`TAIL_LEAST_RATE`]. The
/// markers that say so; `None`, and the item left, when it has no decay
/// to draw.
fn draw_out_between(item: &mut daw_proto::Item, to: f64) -> Option<Vec<(f64, f64)>> {
    use daw_proto::primitives::Duration;
    let length = item.length.as_seconds();
    let keep = STRETCH_KEEP.min(length / 2.0);
    let decay = length - keep;
    let body = STRETCH_BODY.min(decay / 2.0);
    let tail = decay - body;
    let body_played = body / STRETCH_LEAST_RATE;
    let farthest = keep + body_played + tail / TAIL_LEAST_RATE;
    let longer = (to - item.position.as_seconds()).min(farthest);
    if tail <= 0.0 || longer <= keep + body_played + tail {
        return None;
    }
    let offset = item.start_offset.as_seconds();
    item.length = Duration::from_seconds(longer);
    Some(vec![
        (0.0, offset),
        (keep, offset + keep),
        (keep + body_played, offset + keep + body),
        (longer, offset + length),
    ])
}

/// `item` drawn out to end at `to`: its first [`STRETCH_KEEP`] as it was,
/// the rest slowed to reach — the markers that say so, `(take time,
/// source time)`, and the item lengthened. `None`, and the item left, when
/// that would slow it below [`STRETCH_LEAST_RATE`].
fn draw_out(item: &mut daw_proto::Item, to: f64) -> Option<Vec<(f64, f64)>> {
    draw_out_to(item, to, STRETCH_LEAST_RATE)
}

/// [`draw_out`], slowing it no further than `least` of its rate.
fn draw_out_to(item: &mut daw_proto::Item, to: f64, least: f64) -> Option<Vec<(f64, f64)>> {
    use daw_proto::primitives::Duration;
    let length = item.length.as_seconds();
    let longer = to - item.position.as_seconds();
    let keep = STRETCH_KEEP.min(length / 2.0);
    let decay = length - keep;
    // A hair under `least` is `least`: drawn to exactly as far as the
    // rate reaches ([`farthest_drawn`]), the division lands just below.
    if longer <= length || decay <= 0.0 || decay / (longer - keep) < least - 1e-9 {
        return None;
    }
    let offset = item.start_offset.as_seconds();
    item.length = Duration::from_seconds(longer);
    Some(vec![
        (0.0, offset),
        (keep, offset + keep),
        (longer, offset + length),
    ])
}

/// How near a hit's attack another counts as before it, seconds.
const SEAM_GUARD: f64 = 0.002;

/// How far after the hit before it a slice reaching back over a gap must
/// still start, seconds: past its attack's body, the loud first of it,
/// not just its first instant. Two kicks played 40 ms apart and sent a
/// sixteenth apart, the second's slice reached back to 2 ms after the
/// first — and played the first kick again between them (Unbreakable
/// 73.1); the snare's slice after it played the second kick again. What
/// is left of the gap the slice before draws out over.
const RING_GUARD: f64 = STRETCH_KEEP;

/// The crossfade across a quantize's seams, with auto-crossfade on.
pub const QUANTIZE_CROSSFADE: f64 = 0.005;

/// How near a slice's start a cut is taken to be that start, seconds:
/// this much before it…
pub const SEAM_SNAP: f64 = 0.004;

/// …or this much after — a slice starts its lead before its hit, and as
/// much again as it reached back over a gap (a guided slip moves 30 ms
/// at most).
pub const SEAM_REACH: f64 = 0.035;

/// Where a slip cuts and how its seams are joined — Perfect Timing!'s
/// Lead and Xfade, in seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct SlipCut {
    /// How far before a hit the cut is ([`QUANTIZE_PRE_ROLL`]).
    pub pre_roll: f64,
    /// The crossfade across each seam, with auto-crossfade on
    /// ([`QUANTIZE_CROSSFADE`]).
    pub crossfade: f64,
    /// A gap longer than this (seconds) is filled by drawing the slice
    /// before it out — its decay stretched, its attack kept — rather than
    /// by the slice after reaching back into audio it already played.
    /// `None`: never; stretching is the last resort.
    pub stretch_over: Option<f64>,
    /// Every hit of the kit where it plays before the slip, by time — the
    /// ones it cuts on and the ones it does not (a flam's grace riding in
    /// its stroke's slice, a ghost, bleed): no slice reaches back over
    /// any of them. Empty: only the cut hits are known.
    pub attacks: std::sync::Arc<[f64]>,
}

impl Default for SlipCut {
    fn default() -> Self {
        Self {
            pre_roll: QUANTIZE_PRE_ROLL,
            crossfade: QUANTIZE_CROSSFADE,
            stretch_over: None,
            attacks: std::sync::Arc::from(Vec::new()),
        }
    }
}

/// How far before a hit a quantize cuts: the stick's attack is in the
/// slice it moves, not left behind in the one before.
pub const QUANTIZE_PRE_ROLL: f64 = 0.005;

/// Cut every item in `mine` that `at` falls inside: the item keeps the
/// left half and a new one — named `{guid}-split@{t}`, the name the
/// engine remembers it by — takes the right. The edits that say so.
fn cut(project: &mut Project, at: f64, mine: &HashSet<String>) -> Vec<Edit> {
    let mut splits = Vec::new();
    for lane in project.items.values_mut() {
        let mut halves = Vec::new();
        for item in lane.iter_mut() {
            let start = item.position.as_seconds();
            let end = start + item.length.as_seconds();
            if !mine.contains(&item.guid) || at <= start + 1e-3 || at >= end - 1e-3 {
                continue;
            }
            let mut right = item.clone();
            right.guid = format!("{}-split@{at:.3}", item.guid);
            right.position = daw_proto::primitives::PositionInSeconds::from_seconds(at);
            right.length = daw_proto::primitives::Duration::from_seconds(end - at);
            right.fade_in_length = daw_proto::primitives::Duration::from_seconds(0.0);
            // As far into the source as the cut is into the item: the
            // right half plays on from where the left stopped.
            right.start_offset = daw_proto::primitives::Duration::from_seconds(
                item.start_offset.as_seconds() + (at - start),
            );
            item.length = daw_proto::primitives::Duration::from_seconds(at - start);
            item.fade_out_length = daw_proto::primitives::Duration::from_seconds(0.0);
            splits.push(Edit::SplitItem(item.guid.clone(), at, right.guid.clone()));
            halves.push(right);
        }
        lane.extend(halves);
        lane.sort_by(|a, b| {
            a.position
                .as_seconds()
                .partial_cmp(&b.position.as_seconds())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    project.item_count = project.items.values().map(Vec::len).sum();
    splits
}

impl Editor {
    /// Split at a time: the selected items that contain it, or every
    /// item that does when nothing is selected — REAPER's rule for the
    /// split key. Each becomes two here, and the engine is told.
    pub fn split_at(&mut self, at: f64, project: &mut Project, effects: &mut Vec<Effect>) {
        // Which real items the selection's folded rows stand for. Worked
        // out before the walk, because the walk holds `project.items`
        // mutably and the folds are read from the same project.
        let folded: HashSet<String> = self
            .selected
            .iter()
            .filter_map(|guid| match project.spread(guid, true) {
                daw_ui::studio::folded::Spread::Children(items) => Some(items),
                // A ragged row cannot be split on this row's edges, and
                // the reason travels the way every other refusal does.
                daw_ui::studio::folded::Spread::Refused(why) => {
                    effects.push(refused(guid, why));
                    None
                }
                daw_ui::studio::folded::Spread::Direct => None,
            })
            .flatten()
            .collect();
        // REAPER's rule for the split key: the selected items that
        // contain the time, or every item that does when nothing is
        // selected. A mic is "mine" when it is selected itself or when
        // the folded row over it is — splitting a folded item splits
        // every mic under it, which is what the row says it is.
        let mine: HashSet<String> = project
            .items
            .values()
            .flatten()
            .filter(|item| {
                self.selected.is_empty()
                    || self.selected.contains(&item.guid)
                    || folded.contains(&item.guid)
            })
            .map(|item| item.guid.clone())
            .collect();
        Self::split_items(project, at, &mine, effects);
    }
}

/// Which real items an edit aimed at `guid` has to be made on.
///
/// The one gate between a gesture and the session. An item on a folded
/// row is not an item the session has — it is a view of the mics under
/// it — so every edit passes through here and comes out addressed to
/// things that exist. A `folded:` guid can then never reach the engine,
/// which is an invariant rather than a discipline: this is the only
/// place a target is chosen.
///
/// A refusal, addressed to the row it is about.
///
/// The folded guid carries its folder in it, which is exactly the row
/// the message wants to sit on — so the address is derived here rather
/// than threaded down from whatever gesture started this.
fn refused(guid: &str, why: &'static str) -> Effect {
    Effect::Refused {
        why,
        row: daw_ui::studio::folded::parse_guid(guid).map(|(folder, _)| folder.to_owned()),
    }
}

/// `None` means the edit was refused and the reason has been pushed.
fn targets(
    project: &Project,
    guid: &str,
    destructive: bool,
    effects: &mut Vec<Effect>,
) -> Option<Vec<String>> {
    use daw_ui::studio::folded::Spread;
    match project.spread(guid, destructive) {
        Spread::Direct => Some(grouped(project, guid)),
        Spread::Children(items) => Some(items),
        Spread::Refused(why) => {
            effects.push(refused(guid, why));
            None
        }
    }
}

/// `guid` and the items its track's group edits with it.
///
/// While grouping is on: on every track following the media-edit group
/// the track leads, the item in the same place — same start, same length,
/// same lane. A kit's edit lands on every mic at once, as one take.
#[must_use]
pub fn grouped(project: &Project, guid: &str) -> Vec<String> {
    /// How near two items' edges must be to be the same place.
    const SAME: f64 = 1e-3;
    let mut out = vec![guid.to_owned()];
    if !crate::options::GROUPING.get() {
        return out;
    }
    let Some((track, item)) = project
        .items
        .iter()
        .find_map(|(track, items)| items.iter().find(|i| i.guid == guid).map(|i| (track, i)))
    else {
        return out;
    };
    let Some(lead) = project
        .tracks
        .iter()
        .find(|t| &t.guid == track)
        .map(|t| t.grouping.media_edit_lead)
        .filter(|lead| *lead != 0)
    else {
        return out;
    };
    let (at, length) = (item.position.as_seconds(), item.length.as_seconds());
    for other in project
        .tracks
        .iter()
        .filter(|t| &t.guid != track && t.grouping.media_edit_follow & lead != 0)
    {
        out.extend(
            project
                .items
                .get(&other.guid)
                .into_iter()
                .flatten()
                .filter(|i| {
                    i.fixed_lane == item.fixed_lane
                        && (i.position.as_seconds() - at).abs() < SAME
                        && (i.length.as_seconds() - length).abs() < SAME
                })
                .map(|i| i.guid.clone()),
        );
    }
    out
}

/// Bracket the sends added since `from` so the undo history sees them
/// as one step — one gesture, one undo, however many edits it made.
///
/// Even a single edit: the engine records a step only for a bracket
/// (`Edit::BeginUndo`/`EndUndo`), so an edit left bare could not be
/// undone at all — moving one item, then Ctrl+Z, did nothing. None when
/// nothing was sent: a step that undoes nothing is a stop in the history
/// the hand never made.
///
/// This is what #114 meant by "undo is one action, not N": a drag on a
/// folded row moves three mics, and an undo that took back one of them
/// would leave the take out of phase with itself.
fn as_one_step(label: &str, from: usize, effects: &mut Vec<Effect>) {
    bracket(label, from, effects, 1);
}

/// A selection as one step when it touches several items: selecting is
/// not an edit, and is not undone on its own (REAPER's default).
fn selection_as_one_step(from: usize, effects: &mut Vec<Effect>) {
    bracket("Select items", from, effects, 2);
}

/// Bracket the sends since `from` when there are at least `least`.
fn bracket(label: &str, from: usize, effects: &mut Vec<Effect>, least: usize) {
    let sends = effects.get(from..).map_or(0, |added| {
        added
            .iter()
            .filter(|effect| matches!(effect, Effect::Send(_)))
            .count()
    });
    if sends < least {
        return;
    }
    effects.insert(from, Effect::Send(Edit::BeginUndo(label.to_owned())));
    effects.push(Effect::Send(Edit::EndUndo(label.to_owned())));
}

/// Slip an item's contents to `to`, on every real item it stands for.
///
/// Through [`targets`] like every other edit: a slip on a folded row
/// slips every mic under it, because on screen they are one take and
/// slipping some of them would put the kit out of phase with itself —
/// which is the one thing a multi-mic fold exists to prevent.
fn spread_item_slip(project: &mut Project, guid: &str, to: f64, effects: &mut Vec<Effect>) -> bool {
    let Some(targets) = targets(project, guid, true, effects) else {
        return false;
    };
    let wanted: std::collections::HashSet<&String> = targets.iter().collect();
    let from = effects.len();
    let mut made = false;
    for item in project.items.values_mut().flatten() {
        if !wanted.contains(&item.guid) {
            continue;
        }
        // Predicted locally so the waveform is right on the next frame
        // rather than whenever the session answers — the same bargain
        // every other edit here makes.
        item.start_offset = daw_proto::primitives::Duration::from_seconds(to);
        effects.push(Effect::Send(Edit::SlipItem(item.guid.clone(), to)));
        made = true;
    }
    as_one_step("Slip item contents", from, effects);
    made
}

/// Copy an item to `at`, on every real item it stands for.
///
/// Through [`targets`] like every other edit, so a copy of a folded row
/// copies every mic under it and a ragged one refuses — the row says it
/// is one take, and a copy that took only some of the mics would make
/// that a lie.
///
/// The copies are predicted into the window's own project with invented
/// guids, the way a split's right-hand half is. The engine makes its
/// own; the next read replaces both.
fn spread_item_copy(project: &mut Project, guid: &str, at: f64, effects: &mut Vec<Effect>) -> bool {
    let Some(targets) = targets(project, guid, true, effects) else {
        return false;
    };
    let wanted: std::collections::HashSet<&String> = targets.iter().collect();
    let from = effects.len();
    let mut made = Vec::new();
    for lane in project.items.values_mut() {
        let mut copies = Vec::new();
        for item in lane.iter() {
            if !wanted.contains(&item.guid) {
                continue;
            }
            let mut copy = item.clone();
            copy.guid = format!("{}-copy@{at:.3}", item.guid);
            copy.position = daw_proto::primitives::PositionInSeconds::from_seconds(at);
            copy.selected = false;
            made.push(Edit::CopyItem(item.guid.clone(), at, copy.guid.clone()));
            copies.push(copy);
        }
        if copies.is_empty() {
            continue;
        }
        lane.extend(copies);
        lane.sort_by(|a, b| {
            a.position
                .as_seconds()
                .partial_cmp(&b.position.as_seconds())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    if made.is_empty() {
        return false;
    }
    project.item_count = project.items.values().map(Vec::len).sum();
    for edit in made {
        effects.push(Effect::Send(edit));
    }
    as_one_step("Copy item", from, effects);
    true
}

/// Make one item edit, on every item it really belongs to.
///
/// Predicts locally and sends, in that order and for the same targets,
/// so the window and the session cannot disagree about what a drag did.
/// Every target in one call is one gesture, and the sends are bracketed
/// so the history sees one step — see [`as_one_step`]. The verb this
/// once said the engine lacked has been there all along:
/// `Projects::begin_undo_block`.
fn spread_item_edit(
    project: &mut Project,
    guid: &str,
    effects: &mut Vec<Effect>,
    make: impl Fn(&str) -> Edit,
    change: impl Fn(&mut daw_proto::Item),
) -> bool {
    let Some(targets) = targets(project, guid, true, effects) else {
        return false;
    };
    let from = effects.len();
    for target in targets {
        edit_item(project, &target, &change);
        effects.push(Effect::Send(make(&target)));
    }
    as_one_step("Edit item", from, effects);
    true
}

/// Change one item in the window's copy of the project.
fn edit_item(project: &mut Project, guid: &str, change: impl FnOnce(&mut daw_proto::Item)) {
    if let Some(item) = project
        .items
        .values_mut()
        .flatten()
        .find(|i| i.guid == guid)
    {
        change(item);
    }
}

#[cfg(test)]
mod zoom_tests {
    use super::*;

    fn view() -> Viewport {
        Viewport {
            scroll_x: 100.0,
            scroll_y: 40.0,
            pps: 40.0,
            zoom_y: 1.0,
            width: 2000.0,
            height: 1000.0,
            panel_w: crate::arrangement::TCP_WIDTH,
        }
    }
    const LANES: LanesOrigin = (387.0, 103.0);

    #[test]
    fn a_sideways_drag_zooms_time_about_the_press() {
        let mut ed = Editor::default();
        let v = view();
        let at = (887.0, 400.0);
        assert!(ed.zoom_press(at, &v, LANES, Mods::default()));
        let t_under = (at.0 - LANES.0 + v.scroll_x) / v.pps;
        let next = ed
            .zoom_move((1087.0, 400.0), &v, LANES, Mods::default())
            .expect("a zoom");
        assert!(
            (next.pps / v.pps - std::f64::consts::E).abs() < 1e-9,
            "200px is one e-fold"
        );
        assert!(
            (next.zoom_y - 1.0).abs() < 1e-12,
            "no vertical travel, no vertical zoom"
        );
        // The second under the press is still under it.
        let t_after = (at.0 - LANES.0 + next.scroll_x) / next.pps;
        assert!((t_after - t_under).abs() < 1e-9);
    }

    #[test]
    fn an_upward_drag_zooms_the_rows() {
        let mut ed = Editor::default();
        let v = view();
        let at = (887.0, 400.0);
        ed.zoom_press(at, &v, LANES, Mods::default());
        let row_under = (at.1 - LANES.1 + v.scroll_y) / v.zoom_y;
        let next = ed
            .zoom_move((887.0, 200.0), &v, LANES, Mods::default())
            .expect("a zoom");
        assert!((next.zoom_y - std::f64::consts::E).abs() < 1e-9);
        assert!((next.pps - v.pps).abs() < 1e-12);
        let row_after = (at.1 - LANES.1 + next.scroll_y) / next.zoom_y;
        assert!((row_after - row_under).abs() < 1e-9);
    }

    #[test]
    fn shift_is_the_fine_control_and_the_press_must_be_on_the_lanes() {
        let mut ed = Editor::default();
        let v = view();
        assert!(
            !ed.zoom_press((10.0, 400.0), &v, LANES, Mods::default()),
            "the panel is not the lanes"
        );
        ed.zoom_press((887.0, 400.0), &v, LANES, Mods::default());
        let shift = Mods {
            shift: true,
            ..Mods::default()
        };
        let next = ed
            .zoom_move((1087.0, 400.0), &v, LANES, shift)
            .expect("a zoom");
        assert!((next.pps / v.pps - (0.25f64).exp()).abs() < 1e-9);
        assert!(
            ed.zoom_release(&v, LANES).is_none(),
            "a drag zoomed on the way; the release adds nothing"
        );
        assert!(!ed.dragging());
    }

    #[test]
    fn an_alt_sweep_frames_its_box_on_release() {
        let mut ed = Editor::default();
        let v = view();
        let alt = Mods {
            alt: true,
            ..Mods::default()
        };
        ed.zoom_press((587.0, 203.0), &v, LANES, alt);
        assert!(
            ed.zoom_move((987.0, 403.0), &v, LANES, alt).is_none(),
            "a sweep moves nothing until release"
        );
        assert_eq!(ed.zoom_marquee(), Some(((587.0, 203.0), (987.0, 403.0))));
        let next = ed.zoom_release(&v, LANES).expect("the box framed");
        // Four hundred pixels of sweep at 40 px/s is ten seconds; the
        // lanes are 2000 - 343 wide, so the frame is ~165 px/s.
        let lanes_w = v.width - crate::arrangement::TCP_WIDTH;
        assert!((next.pps - lanes_w / 10.0).abs() < 1e-6);
        // And the sweep's left edge is the new left edge.
        let t0 = (587.0 - LANES.0 + v.scroll_x) / v.pps;
        assert!((next.scroll_x / next.pps - t0).abs() < 1e-9);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrangement::{Palette, TCP_WIDTH, Viewport};
    use crate::ruler::ruler_h;
    use daw_proto::primitives::{Duration, PositionInSeconds};
    use daw_ui::studio::{ProjectRef, RowsRef};

    const PPS: f64 = 100.0;

    /// A hit the slip did not cut on — a flam's grace riding in its
    /// stroke's slice — is never played twice: the slice after reaching
    /// back over a gap stops short of it.
    /// r[verify quantize.seams]
    #[test]
    fn no_seam_reaches_back_over_a_grace() {
        let (lead, fade) = (0.005, 0.005);
        // A tom at 1.0 and its grace at 1.03 in one slice; a kick at 1.1
        // sent 75 ms later — reaching back over the gap would start it at
        // 1.015 in its source, before the grace.
        let cut = SlipCut {
            pre_roll: lead,
            crossfade: fade,
            stretch_over: None,
            attacks: std::sync::Arc::from(vec![1.0, 1.03, 1.1]),
        };
        let mut tom = slice("t-split@0.995", 0.995, 0.995, 0.1, lead);
        let mut kick = slice("t-split@1.095", 1.17, 1.095, 0.5, lead);
        let was_at = HashMap::from([
            (tom.guid.clone(), (0.995, 0.995)),
            (kick.guid.clone(), (1.095, 1.095)),
        ]);
        let mut pieces = vec![&mut tom, &mut kick];
        let mut effects = Vec::new();
        seams(&mut pieces, (&cut, fade, &was_at), &mut effects);
        let [_, kick] = pieces.as_slice() else {
            panic!("two slices");
        };
        assert!(
            kick.start_offset.as_seconds() > 1.03,
            "the kick's slice reaches back to {:.4}, over the grace at 1.03",
            kick.start_offset.as_seconds()
        );
    }

    /// A slice of one recording: `at` on the timeline, `from` in the
    /// source, `length` long, its hit `lead` in.
    fn slice(guid: &str, at: f64, from: f64, length: f64, lead: f64) -> daw_proto::Item {
        daw_proto::Item {
            guid: guid.to_owned(),
            position: PositionInSeconds::from_seconds(at),
            length: Duration::from_seconds(length),
            start_offset: Duration::from_seconds(from),
            snap_offset: Duration::from_seconds(lead),
            ..daw_proto::Item::default()
        }
    }

    /// The seams of slices that only meet, or are a hair apart, or apart
    /// by more than the crossfade, all overlap by the crossfade after —
    /// no slice reaching back past the hit before its own.
    /// r[verify quantize.seams]
    #[test]
    fn every_seam_overlaps_by_the_crossfade() {
        let (lead, fade) = (0.005, 0.005);
        let cut = SlipCut {
            pre_roll: lead,
            crossfade: fade,
            stretch_over: None,
            attacks: std::sync::Arc::from(Vec::new()),
        };
        // Four slices of one take, cut 5 ms before hits at 1.0, 1.5, 2.0
        // and 2.04: the second left where it was, the third moved 2 ms
        // later, the fourth (a hit 40 ms after the third) 30 ms later.
        let mut a = slice("t", 0.0, 0.0, 0.995, 0.0);
        let mut b = slice("t-split@0.995", 0.995, 0.995, 0.5, lead);
        let mut c = slice("t-split@1.495", 1.497, 1.495, 0.5, lead);
        let mut d = slice("t-split@1.995", 2.025, 1.995, 0.5, lead);
        // The slices' ends as cut: each runs to the next one's cut.
        b.length = Duration::from_seconds(0.5);
        c.length = Duration::from_seconds(0.5);
        let mut pieces = vec![&mut a, &mut b, &mut c, &mut d];
        let mut effects = Vec::new();
        seams(&mut pieces, (&cut, fade, &HashMap::new()), &mut effects);
        for pair in pieces.windows(2) {
            let [prev, next] = pair else { continue };
            let ends = prev.position.as_seconds() + prev.length.as_seconds();
            let starts = next.position.as_seconds();
            assert!(
                starts <= ends - fade + 1e-9,
                "{} starts at {starts:.4}, {} ends at {ends:.4}: no crossfade",
                next.guid,
                prev.guid
            );
            // The previous hit's attack, in this slice's source, is never
            // played again.
            let attack = prev.start_offset.as_seconds() + prev.snap_offset.as_seconds();
            assert!(
                next.start_offset.as_seconds() > attack,
                "{} reaches back over the hit before",
                next.guid
            );
        }
    }

    /// Two tracks, three items: a project small enough to reason about
    /// and big enough to split, move and select across.
    pub(super) fn project() -> Project {
        let track = |guid: &str, index: u32| Track {
            guid: guid.to_owned(),
            name: guid.to_uppercase(),
            index,
            ..Track::default()
        };
        let item = |guid: &str, track: &str, at: f64, len: f64| daw_proto::Item {
            guid: guid.to_owned(),
            track_guid: track.to_owned(),
            position: PositionInSeconds::from_seconds(at),
            length: Duration::from_seconds(len),
            fade_in_length: Duration::from_seconds(0.5),
            fade_out_length: Duration::from_seconds(1.0),
            ..daw_proto::Item::default()
        };
        let mut items = std::collections::HashMap::new();
        items.insert(
            "kick".to_owned(),
            vec![item("k1", "kick", 2.0, 4.0), item("k2", "kick", 10.0, 4.0)],
        );
        items.insert("snare".to_owned(), vec![item("s1", "snare", 4.0, 8.0)]);
        Project {
            tracks: vec![track("kick", 0), track("snare", 1)],
            items,
            bpm: 120.0,
            length_secs: 60.0,
            item_count: 3,
            ..Project::default()
        }
    }

    /// A track group's media edit carries an edit to the item in the same
    /// place on every grouped track — and only while grouping is on.
    #[test]
    fn a_grouped_tracks_item_in_the_same_place_is_edited_with_it() {
        let mut p = project();
        for t in &mut p.tracks {
            t.grouping.media_edit_lead = 1;
            t.grouping.media_edit_follow = 1;
        }
        let mut s2 = p.items["kick"][0].clone();
        s2.guid = "s2".to_owned();
        s2.track_guid = "snare".to_owned();
        p.items.get_mut("snare").expect("the snare").push(s2);
        assert_eq!(super::grouped(&p, "k1"), vec!["k1", "s2"]);
        assert_eq!(super::grouped(&p, "k2"), vec!["k2"], "nothing in its place");
        crate::options::GROUPING.set(false);
        let off = super::grouped(&p, "k1");
        crate::options::GROUPING.set(true);
        assert_eq!(off, vec!["k1"], "grouping off");
    }

    /// The same project with the kick shut: its two mics fold onto its
    /// own row, and the mics' rows are gone.
    ///
    /// The mics agree, because they are one performance — which is the
    /// case the fold exists for and the one a drag has to get right.
    fn folded_project() -> Project {
        let mut project = project();
        let mic = |guid: &str, at: f64, len: f64| daw_proto::Item {
            guid: guid.to_owned(),
            track_guid: "in".to_owned(),
            position: PositionInSeconds::from_seconds(at),
            length: Duration::from_seconds(len),
            ..daw_proto::Item::default()
        };
        // Two mics under the kick, landing on the same bars.
        project.items.insert(
            "in".to_owned(),
            vec![mic("in1", 2.0, 4.0), mic("in2", 10.0, 4.0)],
        );
        let mut out1 = mic("out1", 2.0, 4.0);
        let mut out2 = mic("out2", 10.0, 4.0);
        out1.track_guid = "out".to_owned();
        out2.track_guid = "out".to_owned();
        project.items.insert("out".to_owned(), vec![out1, out2]);
        // And nothing of the kick's own, so its row is only the fold.
        project.items.insert("kick".to_owned(), Vec::new());
        let folder = project.tracks[0].clone();
        let lanes: Vec<&[daw_proto::Item]> =
            vec![&project.items["in"][..], &project.items["out"][..]];
        let spans = daw_ui::studio::folded::spans(&lanes);
        let items = daw_ui::studio::folded::lane(&folder, &spans);
        project.folds.insert(
            folder.guid.clone(),
            daw_ui::studio::folded::Fold { items, spans },
        );
        project.item_count = project.items.values().map(Vec::len).sum();
        project
    }

    struct Stage {
        editor: Editor,
        project: Project,
        scene: Arrangement,
        rows: Vec<(Track, u32)>,
    }

    impl Stage {
        fn folded() -> Self {
            let project = folded_project();
            let rows: Vec<(Track, u32)> = project.tracks.iter().cloned().map(|t| (t, 0)).collect();
            Self {
                scene: record(&project, &rows),
                editor: Editor::default(),
                project,
                rows,
            }
        }

        fn new() -> Self {
            let project = project();
            let rows: Vec<(Track, u32)> = project.tracks.iter().cloned().map(|t| (t, 0)).collect();
            Self {
                scene: record(&project, &rows),
                editor: Editor::default(),
                project,
                rows,
            }
        }

        fn view(&self) -> Viewport {
            Viewport {
                scroll_x: 0.0,
                scroll_y: 0.0,
                panel_w: crate::arrangement::TCP_WIDTH,
                pps: PPS,
                zoom_y: 1.0,
                width: 2000.0,
                height: 1000.0,
            }
        }

        /// A window point over `seconds` on `row`, `dy` into the row.
        fn point(&self, row: usize, seconds: f64, dy: f64) -> (f64, f64) {
            let (top, _) = self.scene.row_box(row).expect("a row");
            (
                crate::rails::SIDE + TCP_WIDTH + seconds * PPS,
                crate::rails::TOP + ruler_h() + top + dy,
            )
        }

        fn hit(&self, x: f64, y: f64) -> Hit {
            crate::hit::arrangement(&self.scene, self.view(), 0, &[], &[], x, y)
        }

        fn at(&self, x: f64) -> f64 {
            (x - crate::rails::SIDE - TCP_WIDTH) / PPS
        }

        fn press(&mut self, x: f64, y: f64, keys: Mods) -> (bool, Vec<Effect>) {
            let mut effects = Vec::new();
            let taken = self
                .editor
                .press(Some(self.hit(x, y)), keys, &self.scene, &mut effects);
            (taken, effects)
        }

        fn drag_to(&mut self, x: f64, keys: Mods) -> bool {
            self.editor
                .moved(Some(self.at(x)), PPS, self.project.bpm, keys)
        }

        fn release(&mut self, x: f64, keys: Mods) -> Vec<Effect> {
            let mut effects = Vec::new();
            self.editor
                .release(Some(self.at(x)), keys, &mut self.project, &mut effects);
            effects
        }

        fn key(&mut self, action: Action) -> (bool, Vec<Effect>) {
            let mut effects = Vec::new();
            let map = crate::plan::Rows::of(&self.rows, &self.project.tracks);
            let mut tracks = self.project.tracks.clone();
            let bpm = self.project.bpm;
            let handled = self.editor.key(
                action,
                &mut self.project,
                &self.scene,
                &self.rows,
                &mut tracks,
                &map,
                bpm,
                &mut effects,
            );
            self.project.tracks = tracks;
            (handled, effects)
        }

        fn item(&self, guid: &str) -> &daw_proto::Item {
            self.project
                .items
                .values()
                .flatten()
                .find(|i| i.guid == guid)
                .expect(guid)
        }

        fn rerecord(&mut self) {
            self.scene = record(&self.project, &self.rows);
        }

        /// Draw a razor from one point to another, the way a hand
        /// would: ctrl-press, move, release.
        fn razor(&mut self, from: (usize, f64), to: (usize, f64)) {
            let ctrl = Mods {
                ctrl: true,
                ..Mods::default()
            };
            let (x, y) = self.point(from.0, from.1, 15.0);
            assert!(self.press(x, y, ctrl).0, "the press was not taken at all");
            assert!(
                self.editor.razor_in_flight().is_some(),
                "the press at {from:?} started {:?}, not a razor",
                self.hit(x, y).context
            );
            self.editor.razor_moved(to.1, to.0, self.project.bpm);
            let mut effects = Vec::new();
            self.editor
                .release(Some(to.1), ctrl, &mut self.project, &mut effects);
        }

        /// Carve the first area, the way the delete key would before
        /// it removed anything.
        fn carve(&mut self) -> Vec<String> {
            let area = self.editor.razor.areas[0];
            let mut effects = Vec::new();
            let Self {
                editor,
                scene,
                project,
                ..
            } = self;
            editor.razor_carve(area, scene, project, &mut effects)
        }

        /// The spans on a lane, rounded, for comparing against what an
        /// edit was meant to leave.
        fn spans(&self, lane: &str) -> Vec<(f64, f64)> {
            let mut out: Vec<(f64, f64)> = self.project.items[lane]
                .iter()
                .map(|i| {
                    let at = i.position.as_seconds();
                    (
                        (at * 1000.0).round() / 1000.0,
                        ((at + i.length.as_seconds()) * 1000.0).round() / 1000.0,
                    )
                })
                .collect();
            out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            out
        }
    }

    pub(super) fn record(project: &Project, rows: &[(Track, u32)]) -> Arrangement {
        let palette = Palette::from_theme(&daw_ui::theming::Theme::dark());
        let font = crate::text::Font::embedded().expect("the embedded font");
        Arrangement::build(
            &palette,
            &font,
            &ProjectRef(std::sync::Arc::new(project.clone())),
            &RowsRef(std::sync::Arc::new(rows.to_vec())),
            crate::layout::Layout::default(),
            &crate::midi::Previews::default(),
            crate::tcp::Tcp::FULL,
        )
    }

    /// The edits, without the undo brackets.
    ///
    /// For the tests that are about WHAT the session was told, which is
    /// a different question from how the history groups it — that one
    /// has its own tests, and they would be worth nothing if every
    /// other test quietly asserted the grouping too.
    pub(super) fn content(effects: &[Effect]) -> Vec<&Edit> {
        sends(effects)
            .into_iter()
            .filter(|edit| !matches!(edit, Edit::BeginUndo(_) | Edit::EndUndo(_)))
            .collect()
    }

    pub(super) fn sends(effects: &[Effect]) -> Vec<&Edit> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::Send(edit) => Some(edit),
                _ => None,
            })
            .collect()
    }

    /// The edits sent, without the undo step around them: what a gesture
    /// did, which is what most tests are about.
    pub(super) fn edits(effects: &[Effect]) -> Vec<&Edit> {
        sends(effects)
            .into_iter()
            .filter(|e| !matches!(e, Edit::BeginUndo(_) | Edit::EndUndo(_)))
            .collect()
    }

    /// Two items butted together share a boundary, and it moves as one
    /// thing.
    ///
    /// Grabbing the seam and moving only one of them opens a gap or an
    /// overlap, which is never what the hand meant — what it is on is
    /// the join.
    #[test]
    fn trimming_a_seam_moves_both_items() {
        let mut rig = Stage::new();
        // Butt the kick's second item up against the first: k1 runs
        // 2..6, so k2 starts where it ends.
        for item in rig.project.items.get_mut("kick").expect("the lane") {
            if item.guid == "k2" {
                item.position = PositionInSeconds::from_seconds(6.0);
            }
        }
        rig.rerecord();

        // Take hold of k1's right edge, which is the seam, and pull it
        // left by a second.
        let (x, y) = rig.point(0, 6.0, 20.0);
        let (taken, _) = rig.press(x, y, Mods::default());
        assert!(taken, "the seam is a hit");
        let to = crate::rails::SIDE + TCP_WIDTH + 5.0 * PPS;
        rig.drag_to(to, Mods::default());
        rig.release(to, Mods::default());

        let k1 = rig.item("k1");
        let k2 = rig.item("k2");
        let end = k1.position.as_seconds() + k1.length.as_seconds();
        assert!((end - 5.0).abs() < 0.01, "k1 ends at {end}");
        assert!(
            (k2.position.as_seconds() - 5.0).abs() < 0.01,
            "k2 starts at {}",
            k2.position.as_seconds()
        );
        // And they are still touching, which is the whole point.
        assert!((k2.position.as_seconds() - end).abs() < 0.01);
    }

    /// A MOVE is not a trim: sliding an item along its lane leaves its
    /// neighbours where they were.
    #[test]
    fn moving_an_item_leaves_the_one_it_touches_alone() {
        let mut rig = Stage::new();
        for item in rig.project.items.get_mut("kick").expect("the lane") {
            if item.guid == "k2" {
                item.position = PositionInSeconds::from_seconds(6.0);
            }
        }
        rig.rerecord();

        // The middle of k1, well inside its body.
        let (x, y) = rig.point(0, 4.0, 20.0);
        rig.press(x, y, Mods::default());
        let to = crate::rails::SIDE + TCP_WIDTH + 3.0 * PPS;
        rig.drag_to(to, Mods::default());
        rig.release(to, Mods::default());

        assert!(
            (rig.item("k2").position.as_seconds() - 6.0).abs() < 0.01,
            "k2 moved when only k1 was dragged"
        );
    }

    /// The point of the whole thing: a drag on a folded row moves every
    /// mic under it, and the engine is never told about the row.
    #[test]
    fn a_drag_on_a_folded_row_moves_every_mic() {
        let mut s = Stage::folded();
        let (x, y) = s.point(0, 3.0, 15.0);
        assert!(
            s.press(x, y, Mods::default()).0,
            "the folded item is takeable"
        );
        assert!(s.drag_to(x + 1.3 * PPS, Mods::default()));
        let effects = s.release(x + 1.3 * PPS, Mods::default());
        let sent = content(&effects);
        assert_eq!(
            sent,
            vec![
                &Edit::MoveItem("in1".into(), 3.5),
                &Edit::MoveItem("out1".into(), 3.5),
            ],
            "the mics did not both move"
        );
        assert!(
            sent.iter().all(|e| !e.guid().starts_with("folded:")),
            "a view coordinate was sent to the engine: {sent:?}"
        );
        // And the window's own copy agrees, so the next fold lands where
        // the drag left it rather than snapping back.
        assert!((s.item("in1").position.as_seconds() - 3.5).abs() < 1e-9);
        assert!((s.item("out1").position.as_seconds() - 3.5).abs() < 1e-9);
        assert!(effects.contains(&Effect::ReRecord));
    }

    /// Trimming a folded row's edge trims every mic to the same
    /// boundary — which is the only reading that leaves the take intact.
    #[test]
    fn a_trim_on_a_folded_row_trims_every_mic() {
        let mut s = Stage::folded();
        // The right edge of the first folded item, which spans 2..6.
        let (x, y) = s.point(0, 6.0, 15.0);
        s.press(x - 2.0, y, Mods::default());
        s.drag_to(x - 2.0 + 1.0 * PPS, Mods::default());
        let effects = s.release(x - 2.0 + 1.0 * PPS, Mods::default());
        let sent = content(&effects);
        assert_eq!(sent.len(), 2, "{sent:?}");
        assert!(
            sent.iter()
                .all(|e| matches!(e, Edit::TrimItem(g, ..) if g == "in1" || g == "out1")),
            "{sent:?}"
        );
    }

    /// Deleting a folded item deletes the mics under it, and the row
    /// goes with them because there is nothing left to fold.
    #[test]
    fn deleting_a_folded_item_deletes_its_mics() {
        let mut s = Stage::folded();
        let (x, y) = s.point(0, 3.0, 15.0);
        s.press(x, y, Mods::default());
        s.release(x, Mods::default());
        let (_, effects) = s.key(Action::DeleteSelectedItems);
        let sent = content(&effects);
        assert_eq!(
            sent,
            vec![
                &Edit::DeleteItem("in1".into()),
                &Edit::DeleteItem("out1".into()),
            ],
            "{sent:?}"
        );
        assert!(!s.project.items["in"].iter().any(|i| i.guid == "in1"));
        assert!(!s.project.items["out"].iter().any(|i| i.guid == "out1"));
    }

    /// A ragged row refuses an edit that would change it, and says why
    /// rather than doing nothing quietly.
    #[test]
    fn a_ragged_folded_row_refuses_a_drag() {
        let mut s = Stage::folded();
        // Punch one mic in, so the fold gains a fragment at the front.
        s.project.items.get_mut("out").expect("the out mic")[0].position =
            PositionInSeconds::from_seconds(3.0);
        let folder = s.project.tracks[0].clone();
        let lanes: Vec<&[daw_proto::Item]> =
            vec![&s.project.items["in"][..], &s.project.items["out"][..]];
        let spans = daw_ui::studio::folded::spans(&lanes);
        let items = daw_ui::studio::folded::lane(&folder, &spans);
        s.project.folds.insert(
            folder.guid.clone(),
            daw_ui::studio::folded::Fold { items, spans },
        );
        s.scene = record(&s.project, &s.rows);

        let (x, y) = s.point(0, 2.5, 15.0);
        s.press(x, y, Mods::default());
        s.drag_to(x + 1.3 * PPS, Mods::default());
        let effects = s.release(x + 1.3 * PPS, Mods::default());
        assert!(sends(&effects).is_empty(), "a fragment was dragged anyway");
        let (why, row) = effects
            .iter()
            .find_map(|e| match e {
                Effect::Refused { why, row } => Some((*why, row.clone())),
                _ => None,
            })
            .expect("it refused without saying why");
        // The sentence, not a generic failure: the reason is the only
        // part of a refusal worth reading, and it names what to do
        // instead.
        assert!(
            why.contains("do not line up") && why.contains("open the folder"),
            "the refusal lost its reason: {why}"
        );
        // And the row it is about, so the window can put it there
        // rather than in a corner the reader has to carry it back from.
        assert_eq!(
            row.as_deref(),
            Some(folder.guid.as_str()),
            "the refusal did not say which row it was about"
        );
    }

    /// A refused drag sends the ghost back to the item rather than
    /// letting it blink out, which from the hand is indistinguishable
    /// from the press never having been taken.
    #[test]
    fn a_refused_drag_sends_its_ghost_home() {
        let mut s = Stage::folded();
        s.project.items.get_mut("out").expect("the out mic")[0].position =
            PositionInSeconds::from_seconds(3.0);
        let folder = s.project.tracks[0].clone();
        let lanes: Vec<&[daw_proto::Item]> =
            vec![&s.project.items["in"][..], &s.project.items["out"][..]];
        let spans = daw_ui::studio::folded::spans(&lanes);
        let items = daw_ui::studio::folded::lane(&folder, &spans);
        s.project.folds.insert(
            folder.guid.clone(),
            daw_ui::studio::folded::Fold { items, spans },
        );
        s.scene = record(&s.project, &s.rows);

        let (x, y) = s.point(0, 2.5, 15.0);
        s.press(x, y, Mods::default());
        s.drag_to(x + 1.3 * PPS, Mods::default());
        let dragged = s.editor.ghost().expect("a drag draws a ghost");
        s.release(x + 1.3 * PPS, Mods::default());

        assert!(
            s.editor.settling(),
            "the refused ghost vanished instead of going back"
        );
        let (index, x0, _) = s.editor.ghost().expect("the ghost is still travelling");
        assert_eq!(index, dragged.0, "it came back as a different item");
        assert!(
            x0 < dragged.1,
            "it should be on its way back, not still out at {x0}"
        );

        // A new press is a new gesture, and takes the ghost with it.
        let (x2, y2) = s.point(0, 2.5, 15.0);
        s.press(x2, y2, Mods::default());
        assert!(!s.editor.settling(), "a press left the old ghost in flight");
    }

    /// A ctrl-drag over the lanes draws an area and leaves it in the
    /// set. The plain drag it shares a shape with still selects time.
    /// A drag across empty lanes is the Editor's marquee, over items: it
    /// selects what its box touches; Shift adds; a click clears and moves
    /// the cursor.
    #[test]
    fn a_marquee_selects_the_items_its_box_touches() {
        let mut s = Stage::new();
        let shift = Mods {
            shift: true,
            ..Mods::default()
        };
        let sweep = |s: &mut Stage, from: (usize, f64), to: (usize, f64), keys: Mods| {
            let (x, y) = s.point(from.0, from.1, 10.0);
            assert!(s.press(x, y, keys).0, "the empty lane takes the press");
            let (top, _) = s.scene.row_box(to.0).expect("a row");
            s.editor.marquee_moved(to.1, top + 10.0);
            let view = s.view();
            let mut effects = Vec::new();
            s.editor
                .marquee_released(&s.scene, &view, &mut s.project, &mut effects);
            effects
        };
        let selected = |s: &Stage| {
            let mut got: Vec<String> = s.editor.selected.iter().cloned().collect();
            got.sort();
            got
        };
        // From before k1 on the kick to past k1's end on the snare: k1 and
        // s1, not k2 (it starts at 10).
        let effects = sweep(&mut s, (0, 1.0), (1, 7.0), Mods::default());
        assert_eq!(selected(&s), ["k1", "s1"]);
        assert!(sends(&effects).contains(&&Edit::SelectItem("k1".into(), false)));
        assert!(s.item("s1").selected && !s.item("k2").selected);
        // Shift adds k2.
        sweep(&mut s, (0, 9.0), (0, 11.0), shift);
        assert_eq!(selected(&s), ["k1", "k2", "s1"]);
        // A click on empty lane: nothing selected, the cursor there.
        let effects = sweep(&mut s, (0, 8.0), (0, 8.0), Mods::default());
        assert!(selected(&s).is_empty(), "a click clears");
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Transport(Move::Seek, at) if (at - 8.0).abs() < 1e-9)),
            "and moves the cursor"
        );
    }

    /// A razor area's edge resizes it, as the Editor's does: the edge
    /// under the press moves, the other stays; its body still moves it.
    #[test]
    fn a_razor_areas_edge_resizes_it() {
        let mut s = Stage::new();
        s.razor((0, 2.0), (0, 4.0));
        assert_eq!(s.editor.razor.areas, vec![RazorArea::new(2.0, 4.0, 0, 0)]);
        s.editor.pps = PPS;
        // On the end edge: the resize glyph's context, then a drag.
        let (x, y) = s.point(0, 4.0, 15.0);
        assert_eq!(s.editor.context_at(s.hit(x, y)), mousemap::RAZOR_EDGE);
        assert!(s.press(x, y, Mods::default()).0);
        s.editor.razor_moved(6.0, 0, s.project.bpm);
        let mut effects = Vec::new();
        s.editor
            .release(Some(6.0), Mods::default(), &mut s.project, &mut effects);
        assert_eq!(s.editor.razor.areas, vec![RazorArea::new(2.0, 6.0, 0, 0)]);
        // Mid-body is still the area, which a drag moves.
        let (x, y) = s.point(0, 4.0, 15.0);
        assert_eq!(s.editor.context_at(s.hit(x, y)), mousemap::RAZOR_AREA);
    }

    #[test]
    fn a_ctrl_drag_draws_a_razor_area_and_a_plain_one_does_not() {
        let mut s = Stage::new();
        s.razor((0, 3.0), (0, 7.0));
        assert_eq!(s.editor.razor.areas.len(), 1, "no area was left behind");
        let area = s.editor.razor.areas[0];
        assert!((area.t0 - 3.0).abs() < 1e-6 && (area.t1 - 7.0).abs() < 1e-6);
        assert_eq!((area.row_lo, area.row_hi), (0, 0));

        // And the unmodified drag is still a time selection.
        let mut t = Stage::new();
        let (x, y) = t.point(0, 20.0, 15.0);
        t.press(x, y, Mods::default());
        t.drag_to(x + 2.0 * PPS, Mods::default());
        t.release(x + 2.0 * PPS, Mods::default());
        assert!(
            t.editor.razor.is_empty(),
            "a plain drag drew a razor: {:?}",
            t.editor.razor.areas
        );
    }

    /// Areas over the same rows merge into one; areas over different
    /// rows stay apart. Merging is what stops an interior seam being
    /// sliced twice.
    #[test]
    fn areas_merge_on_the_same_rows_and_not_across_them() {
        let mut s = Stage::new();
        s.razor((0, 2.0), (0, 5.0));
        // Drawn backwards, from 7 to 4: the press has to land OUTSIDE
        // the first area or it grabs it and moves it instead, which is
        // the behaviour `a_press_inside_an_area_moves_it` holds.
        s.razor((0, 7.0), (0, 4.0));
        assert_eq!(
            s.editor.razor.areas.len(),
            1,
            "two overlapping areas on one row stayed two: {:?}",
            s.editor.razor.areas
        );
        let merged = s.editor.razor.areas[0];
        assert!(
            (merged.t0 - 2.0).abs() < 1e-6 && (merged.t1 - 7.0).abs() < 1e-6,
            "the merge did not cover both: {merged:?}"
        );

        s.razor((1, 4.0), (1, 8.0));
        assert_eq!(
            s.editor.razor.areas.len(),
            2,
            "an area on another row was merged into this one"
        );
    }

    /// The defining property: an area SLICES at its edges. A razor over
    /// the middle of a take leaves three pieces, and the middle one is
    /// exactly the rectangle.
    #[test]
    fn an_area_slices_the_items_at_both_its_edges() {
        let mut s = Stage::new();
        // The kick's first item runs 2..6. Cut 3..5 out of the middle.
        s.razor((0, 3.0), (0, 5.0));
        let inside = s.carve();
        assert_eq!(
            s.spans("kick"),
            vec![(2.0, 3.0), (3.0, 5.0), (5.0, 6.0), (10.0, 14.0)],
            "the edges did not become real boundaries"
        );
        assert_eq!(
            inside.len(),
            1,
            "the middle piece is what is inside: {inside:?}"
        );
        // No partial overlaps left: everything the area touches is now
        // either wholly inside it or wholly outside.
        for (start, end) in s.spans("kick") {
            let straddles =
                start < 3.0 - 1e-6 && end > 3.0 + 1e-6 || start < 5.0 - 1e-6 && end > 5.0 + 1e-6;
            assert!(!straddles, "({start}, {end}) still straddles an edge");
        }
    }

    /// Deleting an area's contents removes exactly what the rectangle
    /// covered — the middle of the take, and neither shoulder.
    #[test]
    fn deleting_an_area_removes_exactly_what_it_covered() {
        let mut s = Stage::new();
        s.razor((0, 3.0), (0, 5.0));
        let (handled, effects) = s.key(Action::DeleteSelectedItems);
        assert!(handled);
        assert_eq!(
            s.spans("kick"),
            vec![(2.0, 3.0), (5.0, 6.0), (10.0, 14.0)],
            "the rectangle took more or less than it covered"
        );
        // The snare is on another row and the area never reached it.
        assert_eq!(
            s.spans("snare"),
            vec![(4.0, 12.0)],
            "a row outside the area lost something"
        );
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Send(Edit::DeleteItem(_)))),
            "the engine was never told: {effects:?}"
        );
        assert!(s.editor.razor.is_empty(), "the area outlived its contents");
    }

    /// An area already on screen is grabbed and moved, rather than a
    /// new one being started on top of it.
    #[test]
    fn a_press_inside_an_area_moves_it() {
        let mut s = Stage::new();
        s.razor((0, 3.0), (0, 5.0));
        let (x, y) = s.point(0, 4.0, 15.0);
        assert!(
            s.press(x, y, Mods::default()).0,
            "the area did not take the press"
        );
        s.editor.razor_moved(6.0, 0, s.project.bpm);
        let mut effects = Vec::new();
        s.editor
            .release(Some(6.0), Mods::default(), &mut s.project, &mut effects);
        assert_eq!(
            s.editor.razor.areas.len(),
            1,
            "moving an area left two behind"
        );
        let moved = s.editor.razor.areas[0];
        assert!(
            (moved.t0 - 5.0).abs() < 1e-6 && (moved.t1 - 7.0).abs() < 1e-6,
            "the area did not travel with the pointer: {moved:?}"
        );
    }

    /// A click inside an area takes that one out and leaves the rest,
    /// which is the only way to be rid of one without losing them all.
    #[test]
    fn a_click_inside_an_area_removes_it() {
        let mut s = Stage::new();
        s.razor((0, 2.5), (0, 5.0));
        s.razor((1, 8.0), (1, 9.0));
        assert_eq!(s.editor.razor.areas.len(), 2);

        let (x, y) = s.point(0, 4.0, 15.0);
        assert!(
            s.press(x, y, Mods::default()).0,
            "the area did not take the press"
        );
        // No move: press and release in the same place.
        let mut effects = Vec::new();
        s.editor
            .release(Some(4.0), Mods::default(), &mut s.project, &mut effects);
        assert_eq!(
            s.editor.razor.areas.len(),
            1,
            "the click did not take the area out: {:?}",
            s.editor.razor.areas
        );
        let left = s.editor.razor.areas[0];
        assert_eq!(
            (left.row_lo, left.row_hi),
            (1, 1),
            "it took the wrong one out"
        );
    }

    /// A click on an item's body selects it, alone; with Ctrl it is
    /// added. The project's own flags follow.
    #[test]
    fn a_click_selects_and_ctrl_adds() {
        let mut s = Stage::new();
        let (x, y) = s.point(0, 4.0, 15.0);
        assert!(s.press(x, y, Mods::default()).0, "the body is taken");
        let effects = s.release(x, Mods::default());
        assert_eq!(sends(&effects), vec![&Edit::SelectItem("k1".into(), true)]);
        assert!(s.item("k1").selected && !s.item("s1").selected);
        let (x2, y2) = s.point(1, 8.0, 15.0);
        let ctrl = Mods {
            ctrl: true,
            ..Mods::default()
        };
        s.press(x2, y2, ctrl);
        s.release(x2, ctrl);
        assert!(s.item("k1").selected && s.item("s1").selected);
        assert_eq!(s.editor.selected.len(), 2);
    }

    /// The modified click TOGGLES: a second one takes the item back
    /// out. Building a selection up a piece at a time is no use if the
    /// only way to correct an over-click is to start again.
    #[test]
    fn a_modified_click_takes_an_item_back_out() {
        let mut s = Stage::new();
        let ctrl = Mods {
            ctrl: true,
            ..Mods::default()
        };
        let (x, y) = s.point(0, 4.0, 15.0);
        s.press(x, y, ctrl);
        s.release(x, ctrl);
        let (x2, y2) = s.point(1, 8.0, 15.0);
        s.press(x2, y2, ctrl);
        s.release(x2, ctrl);
        assert_eq!(s.editor.selected.len(), 2, "both should be in");
        assert!(s.item("k1").selected && s.item("s1").selected);

        // And the same click again takes the first one out, leaving
        // the second where it was.
        s.press(x, y, ctrl);
        let effects = s.release(x, ctrl);
        assert_eq!(
            s.editor.selected.iter().collect::<Vec<_>>(),
            vec![&"s1".to_owned()],
            "the second click did not take it out: {:?}",
            s.editor.selected
        );
        assert!(!s.item("k1").selected, "the window still draws it selected");
        assert!(s.item("s1").selected, "it took the wrong one out");
        assert_eq!(
            sends(&effects),
            vec![&Edit::DeselectItem("k1".into())],
            "the engine was told the wrong thing: {effects:?}"
        );
    }

    /// A PLAIN click on an already-selected item still means "just this
    /// one", not "none" — exclusive is not a toggle.
    #[test]
    fn a_plain_click_on_a_selected_item_keeps_it() {
        let mut s = Stage::new();
        let (x, y) = s.point(0, 4.0, 15.0);
        s.press(x, y, Mods::default());
        s.release(x, Mods::default());
        s.press(x, y, Mods::default());
        s.release(x, Mods::default());
        assert!(
            s.item("k1").selected,
            "a plain click deselected what it landed on"
        );
        assert_eq!(s.editor.selected.len(), 1);
    }

    /// Alt-drag leaves the original where it is and drops a copy.
    #[test]
    fn alt_drag_copies_the_item_instead_of_moving_it() {
        let mut s = Stage::new();
        let alt = Mods {
            alt: true,
            ..Mods::default()
        };
        let (x, y) = s.point(0, 3.0, 15.0);
        assert!(s.press(x, y, alt).0, "the body did not take the press");
        assert!(s.drag_to(x + 4.0 * PPS, alt));
        let effects = s.release(x + 4.0 * PPS, alt);

        // k1 runs 2..6; dragged 4 s to the right it lands on 6.0.
        assert_eq!(
            s.spans("kick"),
            vec![(2.0, 6.0), (6.0, 10.0), (10.0, 14.0)],
            "the original did not stay put, or the copy landed wrong"
        );
        assert!(
            sends(&effects)
                .iter()
                .any(|e| matches!(e, Edit::CopyItem(g, at, _)
                    if g == "k1" && (*at - 6.0).abs() < 1e-9)),
            "the engine was not told to copy: {effects:?}"
        );
        assert!(
            !sends(&effects)
                .iter()
                .any(|e| matches!(e, Edit::MoveItem(..))),
            "a copy moved the original as well: {effects:?}"
        );
    }

    /// And the negative control: without Alt the same drag still MOVES.
    #[test]
    fn a_plain_drag_still_moves_rather_than_copying() {
        let mut s = Stage::new();
        let (x, y) = s.point(0, 3.0, 15.0);
        s.press(x, y, Mods::default());
        s.drag_to(x + 4.0 * PPS, Mods::default());
        let effects = s.release(x + 4.0 * PPS, Mods::default());
        assert_eq!(
            s.spans("kick"),
            vec![(6.0, 10.0), (10.0, 14.0)],
            "a plain drag left a copy behind"
        );
        assert!(
            sends(&effects)
                .iter()
                .any(|e| matches!(e, Edit::MoveItem(g, _) if g == "k1")),
            "{effects:?}"
        );
    }

    /// A copy of a folded row copies every mic under it. The row says
    /// it is one take, and a copy that took only some of the mics would
    /// make that a lie.
    #[test]
    fn copying_a_folded_row_copies_every_mic() {
        let mut s = Stage::folded();
        let alt = Mods {
            alt: true,
            ..Mods::default()
        };
        let (x, y) = s.point(0, 3.0, 15.0);
        assert!(s.press(x, y, alt).0);
        assert!(s.drag_to(x + 4.0 * PPS, alt));
        let effects = s.release(x + 4.0 * PPS, alt);
        // Sorted: the lanes come out of a map, and which mic is copied
        // first is not a thing this test is about.
        let mut copied: Vec<&String> = sends(&effects)
            .iter()
            .filter_map(|e| match e {
                Edit::CopyItem(g, ..) => Some(g),
                _ => None,
            })
            .collect();
        copied.sort();
        assert_eq!(
            copied,
            vec![&"in1".to_owned(), &"out1".to_owned()],
            "the mics did not both get copied: {copied:?}"
        );
        assert!(
            copied.iter().all(|g| !g.starts_with("folded:")),
            "a view coordinate reached the engine: {copied:?}"
        );
    }

    /// Ctrl+Alt-drag slips the contents: the item does not move, and
    /// the offset into its source does.
    #[test]
    fn ctrl_alt_drag_slips_the_contents_and_leaves_the_item() {
        let mut s = Stage::new();
        let slip = Mods {
            ctrl: true,
            alt: true,
            ..Mods::default()
        };
        let (x, y) = s.point(0, 3.0, 15.0);
        assert!(s.press(x, y, slip).0, "the body did not take the press");
        // Half a second to the right, off the grid so the number is the
        // drag's own rather than the beat's.
        let fine = Mods {
            shift: true,
            ..slip
        };
        s.press(x, y, fine);
        assert!(s.drag_to(x + 0.5 * PPS, fine));
        let effects = s.release(x + 0.5 * PPS, fine);

        // The item is exactly where it was.
        assert_eq!(
            s.spans("kick"),
            vec![(2.0, 6.0), (10.0, 14.0)],
            "a slip moved the item"
        );
        // Dragging the contents right shows EARLIER source under the
        // left edge, so an item that started at the source's beginning
        // clamps there rather than going negative.
        let sent = sends(&effects);
        assert!(
            sent.iter()
                .any(|e| matches!(e, Edit::SlipItem(g, at) if g == "k1" && *at == 0.0)),
            "no slip reached the engine: {sent:?}"
        );
        assert!(
            !sent.iter().any(|e| matches!(e, Edit::MoveItem(..))),
            "a slip moved the item as well: {sent:?}"
        );
    }

    /// Dragging the contents LEFT moves further into the source, and
    /// the window's own copy follows so the waveform is right on the
    /// next frame.
    #[test]
    fn dragging_the_contents_left_goes_further_into_the_source() {
        let mut s = Stage::new();
        let fine = Mods {
            ctrl: true,
            alt: true,
            shift: true,
        };
        let (x, y) = s.point(0, 4.0, 15.0);
        s.press(x, y, fine);
        assert!(s.drag_to(x - 0.75 * PPS, fine));
        s.release(x - 0.75 * PPS, fine);
        let slipped = s.item("k1").start_offset.as_seconds();
        assert!(
            (slipped - 0.75).abs() < 1e-6,
            "the offset went the wrong way or the wrong distance: {slipped}"
        );
        assert!(
            (s.item("k1").position.as_seconds() - 2.0).abs() < 1e-9,
            "the item moved"
        );
    }

    /// A slip on a folded row slips every mic under it. Slipping some
    /// of them would put the kit out of phase with itself, which is the
    /// one thing a multi-mic fold exists to prevent.
    /// A quantize on a folded row cuts every mic before each hit and
    /// moves each slice onto the grid together — one undo step, the
    /// other take untouched, and each slice still playing its own audio.
    /// A second slip over an edited kit moves the slices it has: a cut
    /// that lands beside a seam is that seam — no sliver is cut off
    /// beside it — and the slice there moves by the new shift.
    #[test]
    fn a_second_slip_moves_the_slices_it_has() {
        let mut project = folded_project();
        let takes = vec!["in1".to_owned(), "out1".to_owned()];
        let mut first = Vec::new();
        Editor::slip_items(
            &mut project,
            &takes,
            &[(3.0, 0.010)],
            ("Slip", SlipCut::default()),
            &mut first,
        );
        let seam = 3.0 - QUANTIZE_PRE_ROLL + 0.010;
        let pieces = |project: &Project| -> Vec<String> {
            project
                .items
                .values()
                .flatten()
                .filter(|i| i.guid.starts_with("in1") || i.guid.starts_with("out1"))
                .map(|i| i.guid.clone())
                .collect()
        };
        let after_first = pieces(&project);
        assert_eq!(after_first.len(), 4, "two mics, one cut: {after_first:?}");
        // Where the moved slice starts now: before its hit by the lead and
        // by the gap it reached back over.
        let was = project
            .items
            .values()
            .flatten()
            .find(|i| i.guid.starts_with("in1-split"))
            .map(|i| i.position.as_seconds())
            .expect("the slice");
        assert!(was < seam && seam - was <= SEAM_REACH, "{was} for {seam}");
        // The hit is found again a hair after where it was put.
        let mut second = Vec::new();
        Editor::slip_items(
            &mut project,
            &after_first,
            &[(seam + QUANTIZE_PRE_ROLL + 0.0003, -0.002)],
            ("Slip", SlipCut::default()),
            &mut second,
        );
        assert_eq!(pieces(&project).len(), 4, "no sliver beside the seam");
        assert!(
            !sends(&second)
                .iter()
                .any(|e| matches!(e, Edit::SplitItem(..))),
            "nothing cut again"
        );
        let moved: Vec<f64> = sends(&second)
            .iter()
            .filter_map(|e| match e {
                Edit::MoveItem(_, at) => Some(*at),
                _ => None,
            })
            .collect();
        assert_eq!(moved.len(), 2, "each mic's slice: {moved:?}");
        assert!(
            moved.iter().all(|at| (at - (was - 0.002)).abs() < 1e-9),
            "{moved:?}"
        );
    }

    /// A stretch removed from a take: cut at both ends, the middle gone, a
    /// fade on each edge left — and the take's other mic untouched when
    /// only one is named.
    #[test]
    fn a_range_is_removed_from_the_takes_named() {
        let mut project = folded_project();
        let mut effects = Vec::new();
        Editor::remove_range(
            &mut project,
            &["in1".to_owned()],
            (3.0, 3.03),
            "Remove",
            &mut effects,
        );
        let sent = sends(&effects);
        assert_eq!(
            sent.iter()
                .filter(|e| matches!(e, Edit::SplitItem(..)))
                .count(),
            2
        );
        assert_eq!(
            sent.iter()
                .filter(|e| matches!(e, Edit::DeleteItem(..)))
                .count(),
            1
        );
        let spans: Vec<(f64, f64)> = project.items["in"]
            .iter()
            .filter(|i| i.guid.starts_with("in1"))
            .map(|i| {
                (
                    i.position.as_seconds(),
                    i.position.as_seconds() + i.length.as_seconds(),
                )
            })
            .collect();
        assert_eq!(spans.len(), 2, "{spans:?}");
        assert!(
            spans
                .iter()
                .all(|&(a, b)| b <= 3.0 + 1e-9 || a >= 3.03 - 1e-9),
            "{spans:?}"
        );
        assert!(
            sent.iter().any(
                |e| matches!(e, Edit::SetFadeOut(_, f, _) if (*f - REMOVE_FADE).abs() < 1e-12)
            )
        );
        assert!(
            sent.iter()
                .any(|e| matches!(e, Edit::SetFadeIn(_, f, _) if (*f - REMOVE_FADE).abs() < 1e-12))
        );
        assert_eq!(project.items["out"].len(), 2, "the other mic untouched");

        // Two slices crossfaded over their seam: the removal across the
        // seam leaves no tail of the first playing on over the second.
        let mut project = folded_project();
        let lane = project.items.get_mut("in").expect("the lane");
        lane.retain(|i| i.guid != "in1");
        lane.push(daw_proto::Item {
            guid: "a".to_owned(),
            track_guid: "in".to_owned(),
            position: PositionInSeconds::from_seconds(2.0),
            length: Duration::from_seconds(1.005),
            ..daw_proto::Item::default()
        });
        lane.push(daw_proto::Item {
            guid: "b".to_owned(),
            track_guid: "in".to_owned(),
            position: PositionInSeconds::from_seconds(3.0),
            length: Duration::from_seconds(1.0),
            ..daw_proto::Item::default()
        });
        let mut effects = Vec::new();
        Editor::remove_range(
            &mut project,
            &["a".to_owned(), "b".to_owned()],
            (2.99, 3.003),
            "Remove",
            &mut effects,
        );
        let starts_at_edge = project.items["in"]
            .iter()
            .filter(|i| (i.position.as_seconds() - 3.003).abs() < 1e-6)
            .count();
        assert_eq!(starts_at_edge, 1, "{:?}", project.items["in"]);
    }

    /// Past the stretch threshold a gap is filled by drawing the slice
    /// before it out — its attack as it was, its decay slowed to reach —
    /// and the slice after it stays where it was put, reaching back over
    /// nothing.
    #[test]
    fn a_long_gap_is_stretched_over_when_asked() {
        let mut project = folded_project();
        let mut effects = Vec::new();
        Editor::slip_items(
            &mut project,
            &["in1".to_owned()],
            &[(3.0, 0.020)],
            (
                "Slip",
                SlipCut {
                    stretch_over: Some(0.010),
                    ..SlipCut::default()
                },
            ),
            &mut effects,
        );
        let sent = sends(&effects);
        let markers = sent
            .iter()
            .find_map(|e| match e {
                Edit::SetStretchMarkers(g, m) if g == "in1" => Some(m.clone()),
                _ => None,
            })
            .expect("the slice before stretched");
        let keep = markers.get(1).copied().expect("the attack kept");
        assert!((keep.0 - STRETCH_KEEP).abs() < 1e-9 && (keep.1 - STRETCH_KEEP).abs() < 1e-9);
        let last = markers.last().copied().expect("the end");
        assert!(
            (last.1 - (2.995 - 2.0)).abs() < 1e-9,
            "the source runs to the cut: {markers:?}"
        );
        assert!(
            !sent
                .iter()
                .any(|e| matches!(e, Edit::SlipItem(g, _) if g.starts_with("in1-split"))),
            "the slice after does not reach back"
        );
        // Without the setting the slice after reaches back, as before.
        let mut project = folded_project();
        let mut effects = Vec::new();
        Editor::slip_items(
            &mut project,
            &["in1".to_owned()],
            &[(3.0, 0.020)],
            ("Slip", SlipCut::default()),
            &mut effects,
        );
        assert!(
            !sends(&effects)
                .iter()
                .any(|e| matches!(e, Edit::SetStretchMarkers(..)))
        );
    }

    /// A slice too short to slow that far is not stretched.
    /// Too far for 0.85 all the way: the attack as played, the hit's body
    /// at 0.85, only the far tail slower — and it reaches.
    #[test]
    fn only_the_space_between_hits_is_drawn_out_slower() {
        let mut item = daw_proto::Item {
            position: PositionInSeconds::from_seconds(1.0),
            length: Duration::from_seconds(0.08),
            start_offset: Duration::from_seconds(5.0),
            ..daw_proto::Item::default()
        };
        let markers = super::draw_out_between(&mut item, 1.2).expect("drawn out");
        assert!((item.length.as_seconds() - 0.2).abs() < 1e-9, "it reaches");
        let rates: Vec<f64> = markers
            .windows(2)
            .map(|p| (p[1].1 - p[0].1) / (p[1].0 - p[0].0))
            .collect();
        assert!(
            (rates[0] - 1.0).abs() < 1e-9,
            "the attack as played: {rates:?}"
        );
        assert!(
            (rates[1] - super::STRETCH_LEAST_RATE).abs() < 1e-9,
            "the hit's body no slower than the floor: {rates:?}"
        );
        assert!(
            rates[2] < super::STRETCH_LEAST_RATE && rates[2] >= super::TAIL_LEAST_RATE,
            "only the tail slower: {rates:?}"
        );
        // And the take ends where it did, every marker in order.
        assert!((markers.last().map_or(0.0, |m| m.1) - 5.08).abs() < 1e-9);
    }

    #[test]
    fn a_short_slice_is_not_drawn_out_past_its_rate() {
        let mut item = daw_proto::Item {
            position: PositionInSeconds::from_seconds(1.0),
            length: Duration::from_seconds(0.08),
            ..daw_proto::Item::default()
        };
        assert!(
            super::draw_out(&mut item, 1.12).is_none(),
            "50 ms over 90: too slow"
        );
        assert!(
            (item.length.as_seconds() - 0.08).abs() < 1e-12,
            "and left as it was"
        );
        // 50 ms of decay over 58: 0.86 of its speed, within the floor.
        assert!(super::draw_out(&mut item, 1.088).is_some());
    }

    /// Unbreakable bar 63: two strokes 15 ms apart in the take, sent 93
    /// ms apart. The slice after reaches back only as far as the stroke
    /// before allows, the slice before cannot run on (its take passes the
    /// stroke after), and what is left — 24 ms — is too much to draw out
    /// gently. It is drawn out slower rather than left a 26 ms dropout.
    #[test]
    fn strokes_pulled_apart_leave_no_gap() {
        let slice = |guid: &str, at: f64, length: f64, offset: f64| daw_proto::Item {
            guid: guid.to_owned(),
            position: PositionInSeconds::from_seconds(at),
            length: Duration::from_seconds(length),
            start_offset: Duration::from_seconds(offset),
            ..daw_proto::Item::default()
        };
        let mut before = slice("a-split@1", 94.674_771, 0.066_958, 96.251_459);
        let mut after = slice("a-split@2", 94.768_313, 0.101_687, 96.266_188);
        let cut = SlipCut {
            pre_roll: 0.005,
            crossfade: 0.005,
            ..SlipCut::default()
        };
        let mut effects = Vec::new();
        super::seams(
            &mut [&mut before, &mut after],
            (&cut, 0.005, &std::collections::HashMap::new()),
            &mut effects,
        );
        // Met across the crossfade: no gap, only the far tail slowed past
        // the floor.
        let ends = before.position.as_seconds() + before.length.as_seconds();
        assert!(
            ends >= after.position.as_seconds() + 0.005 - 1e-9,
            "the slice before meets the one after: ends {ends:.4}, the next starts {:.4}",
            after.position.as_seconds()
        );
        assert!(slowest_sent(&effects) >= super::TAIL_LEAST_RATE - 1e-9);
    }

    /// Two kicks played 40 ms apart, sent a sixteenth (94 ms) apart: the
    /// second's slice reaches back no further than past the first's
    /// attack — reached back to just after it, it played the first kick
    /// again between the two (Unbreakable 73.1) — and the first's tail is
    /// drawn out over the rest, no gap.
    #[test]
    fn a_double_pulled_apart_never_plays_its_first_stroke_twice() {
        let slice = |guid: &str, at: f64, length: f64, offset: f64| daw_proto::Item {
            guid: guid.to_owned(),
            position: PositionInSeconds::from_seconds(at),
            length: Duration::from_seconds(length),
            start_offset: Duration::from_seconds(offset),
            snap_offset: Duration::from_seconds(0.005),
            ..daw_proto::Item::default()
        };
        // Cut 5 ms before each kick (at 10.000 and 10.040 in the take).
        let mut first = slice("k-split@1", 19.995, 0.040, 9.995);
        let mut second = slice("k-split@2", 20.089, 0.100, 10.035);
        let cut = SlipCut {
            pre_roll: 0.005,
            crossfade: 0.005,
            ..SlipCut::default()
        };
        let mut effects = Vec::new();
        super::seams(
            &mut [&mut first, &mut second],
            (&cut, 0.005, &std::collections::HashMap::new()),
            &mut effects,
        );
        let from = second.start_offset.as_seconds();
        assert!(
            // Past the loud first 25 ms of the first kick, at least.
            from >= 10.025,
            "the second kick's slice reaches back to {from:.4}, into the first kick at 10.000"
        );
        // The first drawn out to meet the second: no gap.
        let ends = first.position.as_seconds() + first.length.as_seconds();
        assert!(
            ends >= second.position.as_seconds() + 0.005 - 1e-9,
            "the first meets the second: ends {ends:.4}, the second starts {:.4}",
            second.position.as_seconds()
        );
        assert!(slowest_sent(&effects) >= super::TAIL_LEAST_RATE - 1e-9);
    }

    /// The slowest rate any stretch in `effects` plays at (1 with none).
    fn slowest_sent(effects: &[Effect]) -> f64 {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::Send(Edit::SetStretchMarkers(_, markers)) => Some(markers),
                _ => None,
            })
            .flat_map(|markers| {
                markers.windows(2).filter_map(|pair| {
                    let (&(ta, sa), &(tb, sb)) = (pair.first()?, pair.get(1)?);
                    (tb > ta).then(|| (sb - sa) / (tb - ta))
                })
            })
            .fold(1.0, f64::min)
    }

    #[test]
    fn a_selected_folded_row_names_its_kit_and_span() {
        let project = folded_project();
        let row = project.folds.values().next().expect("the fold").items[0]
            .guid
            .clone();
        // The selection names the folder whose kit is read, and the span.
        let mut editor = Editor::default();
        editor.selected.insert(row.clone());
        let folds = editor.selected_folds(&project);
        assert_eq!(folds.len(), 1);
        assert_eq!((folds[0].2, folds[0].3), (2.0, 6.0));
        assert_eq!(folds[0].1, project.tracks[0].guid);
    }

    #[test]
    fn slipping_a_folded_row_slips_every_mic() {
        let mut s = Stage::folded();
        let fine = Mods {
            ctrl: true,
            alt: true,
            shift: true,
        };
        let (x, y) = s.point(0, 3.0, 15.0);
        assert!(s.press(x, y, fine).0);
        assert!(s.drag_to(x - 0.5 * PPS, fine));
        let effects = s.release(x - 0.5 * PPS, fine);
        let mut slipped: Vec<&String> = sends(&effects)
            .iter()
            .filter_map(|e| match e {
                Edit::SlipItem(g, _) => Some(g),
                _ => None,
            })
            .collect();
        slipped.sort();
        assert_eq!(
            slipped,
            vec![&"in1".to_owned(), &"out1".to_owned()],
            "the mics did not both slip: {slipped:?}"
        );
    }

    /// A slip in flight is reported so the window can draw it, and it
    /// is reported as an OFFSET rather than as a span: the item does
    /// not move, so a ghost of its span would say nothing.
    #[test]
    fn a_slip_in_flight_reports_its_offset_not_a_span() {
        let mut s = Stage::new();
        let fine = Mods {
            ctrl: true,
            alt: true,
            shift: true,
        };
        let (x, y) = s.point(0, 4.0, 15.0);
        s.press(x, y, fine);
        assert_eq!(
            s.editor.slip_in_flight(),
            None,
            "a press that has not moved is not a slip yet"
        );
        assert!(s.drag_to(x - 0.6 * PPS, fine));
        let (index, offset) = s.editor.slip_in_flight().expect("the slip is in flight");
        assert_eq!(index, 0, "it named the wrong item");
        assert!(
            (offset - 0.6).abs() < 1e-6,
            "the offset does not follow the pointer: {offset}"
        );
        // And no ghost, because the item lands where it already is: an
        // outline and a pale wash over its own span would say nothing
        // and dim the waveform the gesture exists to let you read.
        assert_eq!(
            s.editor.ghost(),
            None,
            "a slip drew a ghost over the item it is not moving"
        );
        // What was previewed is what is committed. The live pass draws
        // `slip_in_flight` and the release writes `press.slipped`; if
        // those two could differ the preview would be a lie, and the
        // picture would jump when the button came up.
        let previewed = offset;
        s.release(x - 0.6 * PPS, fine);
        let committed = s.item("k1").start_offset.as_seconds();
        assert!(
            (committed - previewed).abs() < 1e-9,
            "the preview showed {previewed} and the release wrote {committed}"
        );
        // Nothing else in flight claims it.
        assert_eq!(s.editor.slip_in_flight(), None, "it outlived the gesture");
    }

    /// A drag on a folded row is ONE undo step, however many mics it
    /// reaches. An undo that took back a third of it would leave the
    /// take out of phase with itself.
    #[test]
    fn a_folded_drag_is_one_undo_step() {
        let mut s = Stage::folded();
        let (x, y) = s.point(0, 3.0, 15.0);
        s.press(x, y, Mods::default());
        s.drag_to(x + 1.3 * PPS, Mods::default());
        let effects = s.release(x + 1.3 * PPS, Mods::default());
        let sent = sends(&effects);
        // Open, two moves, close — in that order and nothing outside.
        assert!(
            matches!(
                sent.as_slice(),
                [
                    Edit::BeginUndo(open),
                    Edit::MoveItem(a, _),
                    Edit::MoveItem(b, _),
                    Edit::EndUndo(close),
                ] if open == close && a == "in1" && b == "out1"
            ),
            "the mics did not move inside one step: {sent:?}"
        );
    }

    /// And a drag on an ordinary row is a step too: the engine records
    /// steps only for brackets, so a bare move could not be undone.
    #[test]
    fn a_plain_drag_is_one_undo_step() {
        let mut s = Stage::new();
        let (x, y) = s.point(0, 3.0, 15.0);
        s.press(x, y, Mods::default());
        s.drag_to(x + 1.3 * PPS, Mods::default());
        let effects = s.release(x + 1.3 * PPS, Mods::default());
        let sent = sends(&effects);
        assert!(
            matches!(sent.first(), Some(Edit::BeginUndo(_)))
                && matches!(sent.last(), Some(Edit::EndUndo(_))),
            "a single move is undone as one step: {sent:?}"
        );
    }

    /// Deleting a folded item is one step too, and so is a razor delete
    /// that reaches more than one item.
    #[test]
    fn deleting_across_several_items_is_one_step() {
        let mut s = Stage::folded();
        let (x, y) = s.point(0, 3.0, 15.0);
        s.press(x, y, Mods::default());
        s.release(x, Mods::default());
        let (_, effects) = s.key(Action::DeleteSelectedItems);
        let sent = sends(&effects);
        let opens = sent
            .iter()
            .filter(|e| matches!(e, Edit::BeginUndo(_)))
            .count();
        let closes = sent
            .iter()
            .filter(|e| matches!(e, Edit::EndUndo(_)))
            .count();
        assert_eq!((opens, closes), (1, 1), "not one step: {sent:?}");
        assert!(
            matches!(sent.first(), Some(Edit::BeginUndo(_))),
            "the step opened after the first delete: {sent:?}"
        );
        assert!(
            matches!(sent.last(), Some(Edit::EndUndo(_))),
            "the step closed before the last delete: {sent:?}"
        );
    }

    /// A drag on the body moves the item, snapped to the beat, with a
    /// ghost on the way and one edit and one re-record at the end.
    #[test]
    fn a_body_drag_moves_the_item_to_the_beat() {
        let mut s = Stage::new();
        let (x, y) = s.point(0, 3.0, 15.0);
        s.press(x, y, Mods::default());
        // 1.3 s to the right: the item at 2.0 lands on 3.5 (a beat is
        // 0.5 s at 120 bpm), not 3.3.
        assert!(s.drag_to(x + 1.3 * PPS, Mods::default()));
        assert_eq!(s.editor.ghost(), Some((0, 3.5, 7.5)));
        assert!(s.editor.dragging());
        let effects = s.release(x + 1.3 * PPS, Mods::default());
        assert_eq!(edits(&effects), vec![&Edit::MoveItem("k1".into(), 3.5)]);
        assert!(effects.contains(&Effect::ReRecord));
        assert!((s.item("k1").position.as_seconds() - 3.5).abs() < 1e-9);
        assert!(!s.editor.dragging());
        // Shift: no snap.
        let shift = Mods {
            shift: true,
            ..Mods::default()
        };
        s.rerecord();
        let (x, y) = s.point(0, 5.0, 15.0);
        s.press(x, y, shift);
        s.drag_to(x + 0.13 * PPS, shift);
        let effects = s.release(x + 0.13 * PPS, shift);
        let Some(Edit::MoveItem(_, to)) = edits(&effects).first().copied() else {
            panic!("a move")
        };
        assert!((to - 3.63).abs() < 1e-6);
    }

    /// The edges trim: the left edge moves the start and keeps the end,
    /// the right edge the other way round.
    #[test]
    fn the_edges_trim() {
        let mut s = Stage::new();
        let (x, y) = s.point(1, 4.0, 15.0);
        let hit = s.hit(x + 1.0, y);
        assert!(
            matches!(
                hit.target,
                Target::Item {
                    zone: ItemZone::LeftEdge,
                    ..
                }
            ),
            "{hit:?}"
        );
        s.press(x + 1.0, y, Mods::default());
        s.drag_to(x + 1.0 + 2.0 * PPS, Mods::default());
        let effects = s.release(x + 1.0 + 2.0 * PPS, Mods::default());
        assert_eq!(
            edits(&effects),
            vec![&Edit::TrimItem("s1".into(), 6.0, 6.0)]
        );
        s.rerecord();
        let (x, y) = s.point(1, 12.0, 15.0);
        s.press(x - 1.0, y, Mods::default());
        s.drag_to(x - 1.0 - 1.0 * PPS, Mods::default());
        let effects = s.release(x - 1.0 - 1.0 * PPS, Mods::default());
        assert_eq!(
            edits(&effects),
            vec![&Edit::TrimItem("s1".into(), 6.0, 5.0)]
        );
    }

    /// A fade handle drags the fade's length, clamped to the item.
    #[test]
    fn a_fade_handle_drags_the_fade() {
        let mut s = Stage::new();
        // k1 starts at 2.0 with a 0.5 s fade-in: its handle is at 2.5,
        // in the band along the top.
        let (x, y) = s.point(0, 2.5, 4.0);
        let hit = s.hit(x, y);
        assert!(
            matches!(
                hit.target,
                Target::Item {
                    zone: ItemZone::FadeIn,
                    ..
                }
            ),
            "{hit:?}"
        );
        assert!(s.press(x, y, Mods::default()).0);
        s.drag_to(x + 1.0 * PPS, Mods::default());
        assert!((s.editor.fade_in_flight().expect("in flight").1.fade_in - 1.5).abs() < 1e-9);
        // Past the end of the item it stops at the item.
        s.drag_to(x + 20.0 * PPS, Mods::default());
        let effects = s.release(x + 20.0 * PPS, Mods::default());
        let Some(Edit::SetFadeIn(guid, len, _)) = sends(&effects).first().copied() else {
            panic!("a fade")
        };
        assert_eq!(guid, "k1");
        assert!((len - 4.0).abs() < 1e-9);
        assert!((s.item("k1").fade_in_length.as_seconds() - 4.0).abs() < 1e-9);
    }

    /// The split key: nothing selected splits every item under the
    /// cursor; a selection splits only itself. The halves meet at the
    /// cursor and the engine hears about each.
    #[test]
    fn split_at_cursor_follows_the_selection() {
        let mut s = Stage::new();
        s.editor.cursor.click(5.0);
        let (handled, effects) = s.key(Action::SplitAtCursor);
        assert!(handled);
        assert_eq!(content(&effects).len(), 2, "k1 and s1 both contain 5.0");
        assert_eq!(s.project.item_count, 5);
        let k1 = s.item("k1");
        assert!((k1.position.as_seconds() + k1.length.as_seconds() - 5.0).abs() < 1e-9);
        let right = s.item("k1-split@5.000");
        assert!((right.position.as_seconds() - 5.0).abs() < 1e-9);
        assert!((right.length.as_seconds() - 1.0).abs() < 1e-9);
        // With a selection, only it.
        let mut s = Stage::new();
        let mut effects = Vec::new();
        s.editor.select("s1", true, &mut s.project, &mut effects);
        s.editor.cursor.click(5.0);
        let (_, effects) = s.key(Action::SplitAtCursor);
        assert_eq!(content(&effects).len(), 1);
        assert_eq!(s.item("k1").length.as_seconds(), 4.0);
    }

    /// Delete takes the selection out of the project and tells the
    /// engine per item; select-all and escape are their opposites.
    #[test]
    fn delete_select_all_and_escape() {
        let mut s = Stage::new();
        let (handled, effects) = s.key(Action::SelectAllItems);
        assert!(handled);
        assert_eq!(s.editor.selected.len(), 3);
        assert!(effects.contains(&Effect::Send(Edit::SelectAllItems(String::new()))));
        let (_, effects) = s.key(Action::ClearSelection);
        assert!(s.editor.selected.is_empty());
        assert!(effects.contains(&Effect::Send(Edit::DeselectAllItems(String::new()))));
        let mut effects = Vec::new();
        s.editor.select("k2", true, &mut s.project, &mut effects);
        let (_, effects) = s.key(Action::DeleteSelectedItems);
        assert_eq!(edits(&effects), vec![&Edit::DeleteItem("k2".into())]);
        assert_eq!(s.project.item_count, 2);
        assert!(s.project.items["kick"].iter().all(|i| i.guid != "k2"));
    }

    /// The cursor keys step to grid lines, and a click on the lanes
    /// or the ruler places the cursor and seeks; a drag selects time.
    #[test]
    fn the_cursor_moves_by_the_grid_and_the_pointer() {
        let mut s = Stage::new();
        s.editor.cursor.click(1.3);
        s.key(Action::CursorBar(1));
        assert!(
            (s.editor.cursor.at - 2.0).abs() < 1e-9,
            "next bar line at 120 bpm"
        );
        s.key(Action::CursorBeat(-1));
        assert!((s.editor.cursor.at - 1.5).abs() < 1e-9);
        s.key(Action::GoToStart);
        assert_eq!(s.editor.cursor.at, 0.0);
        // An empty lane: a click places the cursor and seeks — on the
        // release, since until then it may be a marquee.
        let (x, y) = s.point(0, 20.0, 15.0);
        let (taken, _) = s.press(x, y, Mods::default());
        assert!(taken);
        let view = s.view();
        let mut effects = Vec::new();
        s.editor
            .marquee_released(&s.scene, &view, &mut s.project, &mut effects);
        assert_eq!(effects, vec![Effect::Transport(Move::Seek, 20.0)]);
        assert!((s.editor.cursor.at - 20.0).abs() < 1e-9);
        // The ruler's bars: a drag selects time, as the Editor's ruler.
        let hit = Hit {
            target: Target::Ruler {
                seconds: 20.0,
                on: crate::ruler::On::Bars,
            },
            context: input_config_proto::MouseModifierContext::Ruler,
        };
        let mut effects = Vec::new();
        s.editor
            .press(Some(hit), Mods::default(), &s.scene, &mut effects);
        s.drag_to(x + 3.0 * PPS, Mods::default());
        s.release(x + 3.0 * PPS, Mods::default());
        let span = s.editor.cursor.selection.expect("a time selection");
        assert!((span.start - 20.0).abs() < 1e-9 && (span.end - 23.0).abs() < 1e-9);
    }

    /// Track selection steps down the rows and extends with Shift.
    #[test]
    fn track_steps_walk_the_rows() {
        let mut s = Stage::new();
        let (_, effects) = s.key(Action::TrackStep {
            by: 1,
            extend: false,
        });
        assert_eq!(
            sends(&effects),
            vec![&Edit::Select("kick".into())],
            "from nothing, the first row"
        );
        s.rows[0].0.selected = true;
        let (_, effects) = s.key(Action::TrackStep {
            by: 1,
            extend: true,
        });
        assert_eq!(sends(&effects), vec![&Edit::AddToSelection("snare".into())]);
    }

    /// A key the window cannot do is not handled, so it falls through.
    #[test]
    fn an_unbound_action_falls_through() {
        let mut s = Stage::new();
        let (handled, effects) = s.key(Action::Unbound("40059".into()));
        assert!(!handled && effects.is_empty());
    }
}

#[cfg(test)]
mod ruler_tests {
    use super::{Edit, Editor, Effect};
    use crate::mousemap::Mods;
    use crate::ruler::{MARKS_ROW, On, SECTIONS_ROW, Zone, lane_of};

    /// The press, the drag, the release — the whole gesture, without a
    /// window.
    fn gesture(
        editor: &mut Editor,
        on: On,
        from: f64,
        was: (f64, f64),
        to: Option<f64>,
    ) -> Vec<Edit> {
        gesture_on(&super::tests::project(), editor, on, from, was, to)
    }

    /// The same, over a project the caller has furnished — with regions
    /// and markers on it, for the gestures that are about what else is
    /// at the same moment.
    fn gesture_on(
        project: &super::Project,
        editor: &mut Editor,
        on: On,
        from: f64,
        was: (f64, f64),
        to: Option<f64>,
    ) -> Vec<Edit> {
        use crate::hit::{Hit, Target};
        use input_config_proto::MouseModifierContext as Context;
        let mut effects = Vec::new();
        editor.ruler_span = Some(was);
        let hit = Hit {
            target: Target::Ruler { seconds: from, on },
            context: Context::Ruler,
        };
        let rows: Vec<(super::Track, u32)> =
            project.tracks.iter().cloned().map(|t| (t, 0)).collect();
        let scene = super::tests::record(project, &rows);
        editor.press(Some(hit), Mods::default(), &scene, &mut effects);
        if let Some(to) = to {
            // A pixel scale coarse enough that any move clears the slop.
            editor.moved(Some(to), 100.0, 120.0, Mods::default());
        }
        let mut project = project.clone();
        effects.clear();
        editor.release(to, Mods::default(), &mut project, &mut effects);
        super::tests::sends(&effects).into_iter().cloned().collect()
    }

    /// Two regions written to be contiguous — the verse ends exactly
    /// where the chorus begins — with a marker on the boundary, and a
    /// third region nowhere near it.
    fn abutting() -> super::Project {
        use daw_ui::studio::project::{Marker, Section};
        let band = |id: u32, name: &str, start: f64, end: f64| Section {
            id,
            start,
            end,
            name: name.to_owned(),
            color: None,
            lane: 0,
        };
        let mut project = super::tests::project();
        project.sections = vec![
            band(1, "Verse", 0.0, 8.0),
            band(2, "Chorus", 8.0, 16.0),
            band(3, "Outro", 24.0, 32.0),
        ];
        project.markers = vec![
            Marker {
                at: 8.0,
                name: "drop".to_owned(),
                color: None,
                idx: 7,
                lane: 0,
            },
            Marker {
                at: 20.0,
                name: "elsewhere".to_owned(),
                color: None,
                idx: 8,
                lane: 0,
            },
        ];
        project
    }

    /// The lane decides what a press makes. That is the whole reason
    /// there is no tool to pick: the ruler already says what you meant.
    #[test]
    fn the_lane_decides_what_gets_made() {
        let mut editor = Editor::default();
        let made = gesture(
            &mut editor,
            On::Lane { row: MARKS_ROW },
            4.0,
            (4.0, 4.0),
            None,
        );
        assert!(
            matches!(made.as_slice(), [Edit::AddMarker(_, at, lane)]
                if (*at - 4.0).abs() < 1e-9 && *lane == lane_of(MARKS_ROW)),
            "the marks lane should make a marker, got {made:?}"
        );

        let mut editor = Editor::default();
        let made = gesture(
            &mut editor,
            On::Lane { row: SECTIONS_ROW },
            4.0,
            (4.0, 4.0),
            None,
        );
        assert!(
            matches!(made.as_slice(), [Edit::AddRegion(_, from, to, lane)]
                if (*from - 4.0).abs() < 1e-9 && *to > *from && *lane == lane_of(SECTIONS_ROW)),
            "the sections lane should make a region with a length, got {made:?}"
        );
    }

    /// A boundary is one thing. Dragging the verse's end takes the
    /// chorus's start and the marker written on it along.
    #[test]
    fn a_shared_boundary_moves_as_one() {
        let project = abutting();
        let mut editor = Editor::default();
        let made = gesture_on(
            &project,
            &mut editor,
            On::Region {
                id: 1,
                zone: Zone::End,
            },
            8.0,
            (0.0, 8.0),
            Some(10.0),
        );
        // The verse itself.
        assert!(
            made.iter()
                .any(|e| matches!(e, Edit::SetRegionBounds(_, 1, from, to)
                if from.abs() < 1e-9 && (*to - 10.0).abs() < 1e-9)),
            "the dragged end did not move: {made:?}"
        );
        // The chorus's start, keeping its own end.
        assert!(
            made.iter()
                .any(|e| matches!(e, Edit::SetRegionBounds(_, 2, from, to)
                if (*from - 10.0).abs() < 1e-9 && (*to - 16.0).abs() < 1e-9)),
            "the abutting region was left behind, opening a gap: {made:?}"
        );
        // And the marker on the boundary.
        assert!(
            made.iter().any(|e| matches!(e, Edit::MoveMarker(_, 7, at)
                if (*at - 10.0).abs() < 1e-9)),
            "the marker on the boundary stayed put: {made:?}"
        );
        // Nothing else moved. The outro and the far marker are not on
        // this moment and must not have heard about it.
        assert!(
            !made.iter().any(|e| matches!(
                e,
                Edit::SetRegionBounds(_, 3, ..) | Edit::MoveMarker(_, 8, _)
            )),
            "a mark nowhere near the boundary was dragged: {made:?}"
        );
    }

    /// The same from the other side: dragging the chorus's START takes
    /// the verse's end with it.
    #[test]
    fn the_boundary_carries_from_either_side() {
        let project = abutting();
        let mut editor = Editor::default();
        let made = gesture_on(
            &project,
            &mut editor,
            On::Region {
                id: 2,
                zone: Zone::Start,
            },
            8.0,
            (8.0, 16.0),
            Some(6.0),
        );
        assert!(
            made.iter()
                .any(|e| matches!(e, Edit::SetRegionBounds(_, 1, from, to)
                if from.abs() < 1e-9 && (*to - 6.0).abs() < 1e-9)),
            "the region ending on the boundary did not follow: {made:?}"
        );
        assert!(
            made.iter().any(|e| matches!(e, Edit::MoveMarker(_, 7, at)
                if (*at - 6.0).abs() < 1e-9)),
            "the marker did not follow: {made:?}"
        );
    }

    /// Dragging the MARKER carries the boundary too — it is the same
    /// moment whichever of the things on it the hand took hold of.
    #[test]
    fn dragging_the_marker_carries_the_boundary() {
        let project = abutting();
        let mut editor = Editor::default();
        let made = gesture_on(
            &project,
            &mut editor,
            On::Marker { id: 7 },
            8.0,
            (8.0, 8.0),
            Some(12.0),
        );
        assert!(
            made.iter().any(|e| matches!(e, Edit::MoveMarker(_, 7, at)
                if (*at - 12.0).abs() < 1e-9)),
            "the marker did not move: {made:?}"
        );
        assert!(
            made.iter()
                .any(|e| matches!(e, Edit::SetRegionBounds(_, 1, _, to)
                if (*to - 12.0).abs() < 1e-9)),
            "the verse's end stayed behind: {made:?}"
        );
        assert!(
            made.iter()
                .any(|e| matches!(e, Edit::SetRegionBounds(_, 2, from, _)
                if (*from - 12.0).abs() < 1e-9)),
            "the chorus's start stayed behind: {made:?}"
        );
    }

    /// The negative one, and the one that catches this being written
    /// too broadly: a region moved BODILY takes nothing with it.
    ///
    /// A body drag that dragged its neighbours' boundaries would make a
    /// timeline impossible to rearrange — every move would smear the
    /// sections either side of it into the gap.
    #[test]
    fn a_body_drag_moves_only_its_own_region() {
        let project = abutting();
        let mut editor = Editor::default();
        let made = gesture_on(
            &project,
            &mut editor,
            On::Region {
                id: 1,
                zone: Zone::Body,
            },
            4.0,
            (0.0, 8.0),
            Some(6.0),
        );
        assert_eq!(
            made.len(),
            1,
            "a body drag touched something other than its own region: {made:?}"
        );
        assert!(
            matches!(made.as_slice(), [Edit::SetRegionBounds(_, 1, ..)]),
            "{made:?}"
        );
    }

    /// Dragging a band's end moves that end and leaves the other.
    ///
    /// The negative control is the other end: a resize that moved both
    /// is a move, and the user asked for a resize.
    #[test]
    fn dragging_an_end_leaves_the_other_alone() {
        let mut editor = Editor::default();
        let made = gesture(
            &mut editor,
            On::Region {
                id: 7,
                zone: Zone::End,
            },
            16.0,
            (8.0, 16.0),
            Some(24.0),
        );
        let [Edit::SetRegionBounds(_, id, from, to)] = made.as_slice() else {
            panic!("expected one bounds edit, got {made:?}");
        };
        assert_eq!(*id, 7);
        assert!((*from - 8.0).abs() < 1e-9, "the start moved to {from}");
        assert!((*to - 24.0).abs() < 1e-9, "the end went to {to}");
    }

    /// Dragging a band's body moves both ends and keeps its length.
    #[test]
    fn dragging_a_body_keeps_the_length() {
        let mut editor = Editor::default();
        let made = gesture(
            &mut editor,
            On::Region {
                id: 7,
                zone: Zone::Body,
            },
            10.0,
            (8.0, 16.0),
            Some(18.0),
        );
        let [Edit::SetRegionBounds(_, _, from, to)] = made.as_slice() else {
            panic!("expected one bounds edit, got {made:?}");
        };
        assert!(
            ((to - from) - 8.0).abs() < 1e-9,
            "the band changed length: {from}..{to}"
        );
        assert!(
            (*from - 16.0).abs() < 1e-9,
            "it moved by the drag, to {from}"
        );
    }

    /// A marker has one end, so a drag moves the only thing it has.
    #[test]
    fn dragging_a_marker_moves_it() {
        let mut editor = Editor::default();
        let made = gesture(
            &mut editor,
            On::Marker { id: 3 },
            8.0,
            (8.0, 8.0),
            Some(12.0),
        );
        assert!(
            matches!(made.as_slice(), [Edit::MoveMarker(_, 3, at)] if (*at - 12.0).abs() < 1e-9),
            "got {made:?}"
        );
    }

    /// A press on a mark or a band that never moved is a selection,
    /// not an edit. Anything else would mean you could not point at
    /// something without changing it.
    #[test]
    fn a_press_that_never_moved_changes_nothing() {
        let mut editor = Editor::default();
        let made = gesture(&mut editor, On::Marker { id: 3 }, 8.0, (8.0, 8.0), None);
        assert!(made.is_empty(), "a click should not edit: {made:?}");
        assert_eq!(editor.ruler_selection, Some(On::Marker { id: 3 }));
    }

    /// Delete takes what the ruler has hold of, and nothing else.
    #[test]
    fn delete_takes_the_ruler_selection_first() {
        let mut editor = Editor::default();
        editor.ruler_selection = Some(On::Region {
            id: 9,
            zone: Zone::Body,
        });
        editor.selected.insert("an-item".into());
        let mut project = super::tests::project();
        let rows: Vec<(super::Track, u32)> =
            project.tracks.iter().cloned().map(|t| (t, 0)).collect();
        let scene = super::tests::record(&project, &rows);
        let mut tracks: Vec<super::Track> = project.tracks.clone();
        let row_to_track = crate::plan::Rows::of(&[], &tracks);
        let mut effects = Vec::new();
        editor.key(
            crate::keys::Action::DeleteSelectedItems,
            &mut project,
            &scene,
            &rows,
            &mut tracks,
            &row_to_track,
            120.0,
            &mut effects,
        );
        let made: Vec<&Edit> = super::tests::sends(&effects);
        assert!(
            matches!(made.as_slice(), [Edit::RemoveRegion(_, 9)]),
            "got {made:?}"
        );
        assert!(
            editor.selected.contains("an-item"),
            "the item selection was taken as well"
        );
    }
}

#[cfg(test)]
mod comp_tests {
    use super::*;

    fn item(guid: &str, track: &str, lane: u32, from: f64, to: f64) -> daw_proto::Item {
        daw_proto::Item {
            guid: guid.into(),
            track_guid: track.into(),
            fixed_lane: Some(lane),
            position: daw_proto::primitives::PositionInSeconds::from_seconds(from),
            length: daw_proto::primitives::Duration::from_seconds(to - from),
            ..daw_proto::Item::default()
        }
    }

    /// A swipe on take 2 over 4–6 s comps the whole kit: on every mic the
    /// comp lane keeps take 1 either side and takes take 2's audio, from
    /// 4 s into it, in between — and a track outside the folder is left.
    #[test]
    fn a_swipe_comps_every_mic_of_the_folder_from_one_take() {
        let lanes = |guid: &str, parent: Option<&str>| daw_proto::Track {
            guid: guid.into(),
            name: guid.into(),
            parent_guid: parent.map(Into::into),
            is_folder: parent.is_none(),
            lane_count: if parent.is_some() { 3 } else { 0 },
            lane_play_mask: 0b1,
            lane_names: vec!["C1".into(), "1".into(), "2".into()],
            ..daw_proto::Track::default()
        };
        let mut project = Project {
            tracks: vec![
                lanes("drums", None),
                lanes("in", Some("drums")),
                lanes("top", Some("drums")),
                lanes("bass", None),
            ],
            ..Project::default()
        };
        for mic in ["in", "top"] {
            project.items.insert(
                mic.into(),
                vec![
                    item(&format!("{mic}-c"), mic, 0, 0.0, 10.0),
                    item(&format!("{mic}-1"), mic, 1, 0.0, 10.0),
                    item(&format!("{mic}-2"), mic, 2, 0.0, 10.0),
                ],
            );
        }
        let mut effects = Vec::new();
        Editor::comp(&mut project, "drums", 2, 4.0, 6.0, &mut effects);
        for mic in ["in", "top"] {
            let comp: Vec<(f64, f64, f64)> = project.items[mic]
                .iter()
                .filter(|i| i.fixed_lane == Some(0))
                .map(|i| {
                    let start = i.position.as_seconds();
                    (
                        start,
                        start + i.length.as_seconds(),
                        i.start_offset.as_seconds(),
                    )
                })
                .collect();
            assert_eq!(comp.len(), 3, "{mic}: {comp:?}");
            assert_eq!((comp[0].0, comp[0].1), (0.0, 4.0), "take 1 before");
            assert_eq!(comp[1], (4.0, 6.0, 4.0), "take 2, 4 s into it");
            assert_eq!((comp[2].0, comp[2].1), (6.0, 10.0), "take 1 after");
            assert_eq!(
                project.items[mic]
                    .iter()
                    .filter(|i| i.fixed_lane == Some(2))
                    .count(),
                1,
                "the take itself stays"
            );
        }
        let sent: Vec<&Edit> = effects
            .iter()
            .filter_map(|e| match e {
                Effect::Send(edit) => Some(edit),
                _ => None,
            })
            .collect();
        assert!(sent.iter().any(|e| matches!(e, Edit::SetItemLane(_, 0))));
        assert!(matches!(effects.last(), Some(Effect::ReRecord)));
    }

    /// Ctrl+N on a razor over a folded row comps the folder, and the
    /// razor goes; with no razor, the key is not taken.
    #[test]
    fn a_razor_over_a_folded_row_comps_its_folder() {
        let track = |guid: &str, parent: Option<&str>| daw_proto::Track {
            guid: guid.into(),
            parent_guid: parent.map(Into::into),
            is_folder: parent.is_none(),
            lane_count: if parent.is_some() { 3 } else { 0 },
            lane_play_mask: 0b1,
            lane_names: vec!["C1".into(), "1".into(), "2".into()],
            ..daw_proto::Track::default()
        };
        let mut project = Project {
            tracks: vec![track("drums", None), track("in", Some("drums"))],
            ..Project::default()
        };
        project.items.insert(
            "in".into(),
            vec![
                item("c", "in", 0, 0.0, 10.0),
                item("t1", "in", 1, 0.0, 10.0),
            ],
        );
        // The folder shut: its row is the only one.
        let rows = vec![(project.tracks[0].clone(), 0)];
        let mut editor = Editor::default();
        let mut effects = Vec::new();
        assert!(
            !editor.comp_razors(1, &mut project, &rows, &mut effects),
            "no razor"
        );
        editor.razor.add(razor::RazorArea::new(2.0, 3.0, 0, 0));
        assert!(editor.comp_razors(1, &mut project, &rows, &mut effects));
        assert!(editor.razor.is_empty(), "the razor used");
        assert!(project.items["in"].iter().any(|i| i.fixed_lane == Some(0)
            && (i.position.as_seconds() - 2.0).abs() < 1e-9
            && (i.length.as_seconds() - 1.0).abs() < 1e-9));
    }

    #[test]
    fn the_comp_lane_is_c1_else_the_one_playing() {
        let mut track = daw_proto::Track {
            lane_count: 3,
            lane_play_mask: 0b100,
            lane_names: vec!["1".into(), "C1".into(), "2".into()],
            ..daw_proto::Track::default()
        };
        assert_eq!(comp_lane(&track), Some(1));
        track.lane_names = vec!["A".into(), "B".into(), "C".into()];
        assert_eq!(comp_lane(&track), Some(2), "C alone counts: C1's family");
        track.lane_names = vec!["1".into(), "COMP".into(), "EDIT".into()];
        assert_eq!(comp_lane(&track), Some(1), "the kit's comp, EDIT playing");
        track.lane_names.clear();
        assert_eq!(comp_lane(&track), Some(2), "the playing lane");
        assert_eq!(comp_lane(&daw_proto::Track::default()), None);
    }
}
