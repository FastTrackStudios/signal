//! The stack's gestures, as a state machine any host can drive.
//!
//! What the Dioxus `StackView` kept in signals lives here in fields,
//! and what it did in event handlers is a method per event: a press,
//! a move, a release, a wheel, a key. Every method takes the editor it
//! acts on and answers whether anything changed; the edits a host has
//! to carry out on the audio — a slip, a stretch, a split, a hit added
//! or removed — come back as [`HitGesture`]s rather than being applied,
//! because the editor's document is the hit *list* and the audio is
//! the host's.
//!
//! Coordinates are the stack's own: `(0, 0)` at the top-left of the
//! box, the gutter in the first [`canvas::GUTTER_W`] pixels and the
//! ruler in the first `ruler_h`. The same space `paint::stack_scene`
//! draws in, so a host translates once.

use std::time::Instant;

use anyrender::{PaintScene, Scene};
use expression_editor_core::drum::HitGesture;
use expression_editor_core::mouse::{Action, Context as MouseContext, Gesture as MouseGesture};
use expression_editor_core::tools::Mods;
use expression_editor_core::{Editor, Tool};
use kurbo::{Affine, Line, Rect};
use peniko::Fill;

use super::geometry::{
    CHROME_ROW_H, LaneView, RULER_TICKS_H, chrome_shelves, lanes, view_span_secs,
};
use super::paint::{StackChrome, stack_scene};
use super::zoom::TimeZoom;
use crate::canvas::{self, Tick};
use crate::interaction::{PAN_GAIN, ZOOM_DIVISOR};
use crate::num;
use crate::paint::{Look, stroke_of, with_alpha};
use crate::text::Labeller;
use crate::theme;

/// How near a hit line a press picks it up, in pixels.
const SLIP_PICK_PX: f64 = 6.0;
/// How far past a lane's edges a hit is still drawn.
const HIT_MARGIN: f64 = 96.0;
/// The gutter's mic chip and menu.
pub const MIC_CHIP_TOP: f64 = 18.0;
pub const MIC_MENU_TOP: f64 = 36.0;
pub const MIC_ITEM_H: f64 = 16.0;
pub const MIC_MENU_W: f64 = 120.0;
/// The least a lane can be squeezed to.
const MIN_LANE: f32 = 22.0;
/// How many bars a page turn moves.
const BARS_PER_PAGE: usize = 4;
/// Two presses inside this many milliseconds are a double-click.
const DOUBLE_MS: u128 = 400;

/// One slip/stretch drag: which hit is being slid, and where.
#[derive(Clone, Debug)]
struct SlipDrag {
    hit_secs: f64,
    prev_secs: f64,
    next_secs: f64,
    lane_name: String,
    x0: f64,
    x: f64,
    /// Both ends move — the modifiers resolved to the both-ends
    /// binding.
    both: bool,
    hit_x: f64,
    lane_y: f64,
    lane_h: f64,
}

/// The selected hit — the unit of keyboard editing.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectedHit {
    pub lane_name: String,
    pub hit_secs: f64,
    pub prev_secs: f64,
    pub next_secs: f64,
    lane_y: f64,
    lane_h: f64,
}

/// The stack's interaction state.
#[derive(Default)]
// r[impl flow.drums.editing.stack]
pub struct Stack {
    /// The tool before `z` sprang the zoom tool.
    zoom_from: Option<Tool>,
    zooming: Option<TimeZoom>,
    /// Where a middle-drag pan last was.
    panning: Option<(f64, f64)>,
    slipping: Option<SlipDrag>,
    pub selected: Option<SelectedHit>,
    /// The previous press, for double-click detection.
    last_press: Option<(Instant, f64, f64)>,
    /// The lane whose mic menu is open, by layout-lane index.
    pub mic_menu: Option<usize>,
    /// Where the pointer last was, for anchoring wheel zoom.
    wheel_anchor: Option<(f64, f64)>,
    /// Whether a drag or nudge stretches (warps) rather than slips.
    pub warp: bool,
    /// Whether a host is listening for hit gestures. Without one, a
    /// press on a hit switches lanes rather than picking it up — the
    /// gesture would be a lie.
    pub editable: bool,
    /// Fill regions, in seconds, drawn behind the lanes.
    pub fills: Vec<(f64, f64)>,
    /// Whether the host draws each lane's header — its name and the
    /// armed rail — from [`Stack::lane_views`] (a themed host does).
    /// The mic chip, the sub-lane names and the hand mark stay here:
    /// they answer this stack's clicks, so this stack draws them.
    pub host_lanes: bool,
}

/// What a cached [`Stack::scene`] was built for. Equal keys, same
/// picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewKey {
    t0: u64,
    units_per_px: u64,
    stack_scroll: u64,
    viewport: (u64, u64),
    active: usize,
    tracks: usize,
    notes: usize,
    mic_menu: Option<usize>,
    fills: usize,
    host_lanes: bool,
}

/// The chrome the ruler is drawn from, resolved once per frame and
/// shared with the hit tests.
struct ChromeData {
    chrome_rows: Vec<(Option<u32>, bool, String)>,
    sections: Vec<(f64, f64, String, String, usize)>,
    marks: Vec<(f64, String, String, usize)>,
    ticks: Vec<Tick>,
    fill_bands: Vec<(f64, f64)>,
    ruler_h: f64,
}

impl Stack {
    /// A stack a host will write edits through.
    #[must_use]
    pub fn editable() -> Self {
        Self {
            editable: true,
            ..Self::default()
        }
    }

    /// The ruler's height for `ed` — the shelves plus the tick strip.
    #[must_use]
    pub fn ruler_h(ed: &Editor) -> f64 {
        CHROME_ROW_H.mul_add(num::coord(chrome_shelves(ed).len().max(1)), RULER_TICKS_H)
    }

    /// The lanes as [`Stack::scene`] lays them out, in the stack's lane
    /// space: y from under the ruler ([`Stack::ruler_h`]).
    #[must_use]
    pub fn lane_views(ed: &Editor) -> Vec<LaneView> {
        Self::lanes_of(ed)
    }

    fn lanes_of(ed: &Editor) -> Vec<LaneView> {
        lanes(ed, Editor::ACTIVE_BOOST, ed.lane_floor().max(MIN_LANE))
    }

    fn chrome_data(&self, ed: &Editor) -> ChromeData {
        let vp = ed.viewport;
        let ticks = canvas::ruler(ed);
        let chrome_rows = chrome_shelves(ed);
        let row_of = |lane: &Option<(u32, String)>, is_region: bool| -> usize {
            let key = lane.as_ref().map(|(i, _)| *i);
            chrome_rows
                .iter()
                .position(|(i, r, _)| *i == key && *r == is_region)
                .unwrap_or(0)
        };
        let (t0, t1) = ed.camera.time_span(vp);
        // The song's sections across the ruler — clipped to the view,
        // with the label given only the room its span actually has.
        let sections: Vec<(f64, f64, String, String, usize)> = ed
            .doc
            .regions
            .iter()
            .filter(|r| r.end > t0 && r.start < t1)
            .map(|r| {
                let x0 = ed.camera.x(r.start.max(t0)).max(0.0);
                let x1 = ed.camera.x(r.end.min(t1)).min(vp.w);
                let fit = num::index(((x1 - x0) - 6.0) / 5.5);
                let label: String = r.label.chars().take(fit).collect();
                let color = r.color.clone().unwrap_or_else(|| theme::SURFACE_BAR.into());
                (x0, x1, label, color, row_of(&r.lane, true))
            })
            .collect();
        // The song's markers — named points. Each label is clipped to
        // the room before the next marker in the same lane.
        let visible: Vec<&expression_editor_core::doc::Marker> = ed
            .doc
            .markers
            .iter()
            .filter(|m| m.t >= t0 && m.t <= t1)
            .collect();
        let marks: Vec<(f64, String, String, usize)> = visible
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let x = ed.camera.x(m.t).clamp(0.0, vp.w);
                let lane_key = m.lane.as_ref().map(|(i, _)| *i);
                let next = visible
                    .iter()
                    .skip(i.saturating_add(1))
                    .find(|n| n.lane.as_ref().map(|(i, _)| *i) == lane_key)
                    .map_or(vp.w, |n| ed.camera.x(n.t).clamp(0.0, vp.w));
                let fit = num::index(((next - x) - 5.0) / 5.0);
                let label: String = m
                    .label
                    .clone()
                    .unwrap_or_default()
                    .chars()
                    .take(fit)
                    .collect();
                (
                    x,
                    label,
                    m.color.clone().unwrap_or_else(|| theme::TEXT_DIM.into()),
                    row_of(&m.lane, false),
                )
            })
            .collect();
        let ruler_h = CHROME_ROW_H.mul_add(num::coord(chrome_rows.len().max(1)), RULER_TICKS_H);
        let fill_bands: Vec<(f64, f64)> = view_span_secs(ed)
            .map(|(v0, v1)| {
                self.fills
                    .iter()
                    .filter(|(s, e)| *e > v0 && *s < v1)
                    .map(|(s, e)| {
                        let px = |t: f64| (t - v0) / (v1 - v0).max(1e-9) * vp.w;
                        (px(s.max(v0)).max(0.0), px(e.min(v1)).min(vp.w))
                    })
                    .collect()
            })
            .unwrap_or_default();
        ChromeData {
            chrome_rows,
            sections,
            marks,
            ticks,
            fill_bands,
            ruler_h,
        }
    }

    /// The picture that only changes with the camera, the layout or
    /// the document: the lanes, their waveforms and hits, the ruler.
    ///
    /// A host repainting every frame caches this and replays it until
    /// [`Stack::view_key`] changes, drawing [`Stack::overlays`] fresh
    /// on top — a transport tick or a drag then costs a few lines, not
    /// a song's worth of markers.
    pub fn scene(&self, ed: &Editor, labels: &mut Labeller, look: &Look) -> Scene {
        let vp = ed.viewport;
        let views = Self::lanes_of(ed);
        let chrome = self.chrome_data(ed);
        let stack_chrome = StackChrome {
            chrome_rows: &chrome.chrome_rows,
            sections: &chrome.sections,
            marks: &chrome.marks,
            ticks: &chrome.ticks,
            fill_bands: &chrome.fill_bands,
            mic_menu: self.mic_menu,
            ruler_h: chrome.ruler_h,
            chrome_row_h: CHROME_ROW_H,
            mark_row_h: CHROME_ROW_H,
            mic_chip_top: MIC_CHIP_TOP,
            mic_menu_top: MIC_MENU_TOP,
            mic_item_h: MIC_ITEM_H,
            mic_menu_w: MIC_MENU_W,
            hit_margin: HIT_MARGIN,
            host_lanes: self.host_lanes,
        };
        let w = vp.w + canvas::GUTTER_W;
        let h = vp.h + chrome.ruler_h;
        stack_scene(&views, &stack_chrome, w, h, labels, look)
    }

    /// What decides whether [`Stack::scene`] has to be rebuilt: the
    /// camera, the lane scroll, the box, which track is active, the
    /// mic menu, and the document's size. A host that edits the
    /// document through anything but this stack should also drop its
    /// cache when it does.
    #[must_use]
    pub fn view_key(&self, ed: &Editor) -> ViewKey {
        ViewKey {
            t0: ed.camera.t0.to_bits(),
            units_per_px: ed.camera.units_per_px.to_bits(),
            stack_scroll: ed.stack_scroll.to_bits(),
            viewport: (ed.viewport.w.to_bits(), ed.viewport.h.to_bits()),
            active: ed.tracks.active(),
            tracks: ed.tracks.len(),
            notes: ed.doc.notes.len(),
            mic_menu: self.mic_menu,
            fills: self.fills.len(),
            host_lanes: self.host_lanes,
        }
    }

    /// The moving parts, over the scene: the playhead, the selected
    /// hit's bracket, a drag's ghost and a zoom marquee.
    #[must_use]
    pub fn overlays(&self, ed: &Editor, playhead_secs: Option<f64>, look: &Look) -> Scene {
        let vp = ed.viewport;
        let ruler_h = Self::ruler_h(ed);
        let (view0, px) =
            view_span_secs(ed).map_or((0.0, 0.0), |(v0, v1)| (v0, vp.w / (v1 - v0).max(1e-9)));
        let mut scene = Scene::new();
        let at = Affine::translate((canvas::GUTTER_W, ruler_h));
        if let Some(ph) = playhead_secs.filter(|_| px > 0.0) {
            let x = (ph - view0) * px;
            if x >= 0.0 {
                scene.stroke(
                    &stroke_of(1.0),
                    at,
                    with_alpha(look.playhead, 0.7),
                    None,
                    &Line::new((x, 0.0), (x, vp.h)),
                );
            }
        }
        if let Some(sel) = self.selected.as_ref().filter(|_| px > 0.0) {
            let x = (sel.hit_secs - view0) * px;
            scene.stroke(
                &stroke_of(1.0),
                at,
                with_alpha(look.accent, 0.8),
                None,
                &Line::new((x, sel.lane_y), (x, sel.lane_y + sel.lane_h)),
            );
            scene.fill(
                Fill::NonZero,
                at,
                look.accent,
                None,
                &Rect::new(x - 3.0, sel.lane_y, x + 3.0, sel.lane_y + 4.0),
            );
        }
        if let Some(slip) = &self.slipping {
            let to = slip.hit_x + (slip.x - slip.x0);
            scene.stroke(
                &stroke_of(1.0),
                at,
                with_alpha(look.text_dim, 0.5),
                None,
                &Line::new(
                    (slip.hit_x, slip.lane_y),
                    (slip.hit_x, slip.lane_y + slip.lane_h),
                ),
            );
            scene.stroke(
                &stroke_of(2.0),
                at,
                look.accent,
                None,
                &Line::new((to, slip.lane_y), (to, slip.lane_y + slip.lane_h)),
            );
        }
        if let Some(zoom) = self.zooming.filter(|z| z.marquee) {
            let x0 = zoom.origin.min(zoom.current);
            let r = Rect::new(x0, 0.0, x0 + (zoom.current - zoom.origin).abs(), vp.h);
            scene.fill(Fill::NonZero, at, with_alpha(look.accent, 0.15), None, &r);
            scene.stroke(&stroke_of(1.0), at, look.accent, None, &r);
        }
        scene
    }

    /// Whether the gesture in flight moves the camera — a pan or a
    /// zoom — as opposed to a slip, whose ghost is an overlay.
    #[must_use]
    pub const fn moves_camera(&self) -> bool {
        self.zooming.is_some() || self.panning.is_some()
    }

    /// Whether a gesture is in flight.
    #[must_use]
    pub const fn dragging(&self) -> bool {
        self.zooming.is_some() || self.panning.is_some() || self.slipping.is_some()
    }

    /// Drop every gesture in flight — Escape, or the pointer leaving.
    pub fn cancel(&mut self, ed: &mut Editor) {
        if let Some(previous) = self.zoom_from.take() {
            ed.tool = previous;
        }
        self.zooming = None;
        self.slipping = None;
        self.panning = None;
    }

    /// The grid's step in seconds — what a nudge and a snap use.
    fn grid_secs(ed: &Editor) -> f64 {
        let ups = ed.doc.time_base.units_per_second(ed.bpm);
        if ups.abs() < 1e-9 {
            0.0
        } else {
            ed.grid_step() / ups
        }
    }

    /// Turn a delta on the selected hit into the gesture a host
    /// applies — a slip, or a stretch when warping.
    fn emit_move(&self, sel: &SelectedHit, delta: f64, both: bool, out: &mut Vec<HitGesture>) {
        if !self.editable {
            return;
        }
        out.push(if self.warp {
            HitGesture::Stretch {
                hit: sel.hit_secs,
                prev: sel.prev_secs,
                next: sel.next_secs,
                delta,
                both,
            }
        } else {
            HitGesture::Slip {
                hit: sel.hit_secs,
                next: sel.next_secs,
                delta,
            }
        });
    }

    /// A button press. `button` is 0 left, 1 middle, 2 right.
    pub fn press(
        &mut self,
        ed: &mut Editor,
        x: f64,
        y: f64,
        button: u16,
        mods: Mods,
        out: &mut Vec<HitGesture>,
    ) -> bool {
        let vp = ed.viewport;
        let ruler_h = Self::ruler_h(ed);
        // Middle-drag pans, the same as it does on the roll.
        if button == 1 {
            self.panning = Some((x, y));
            return true;
        }
        if button != 0 {
            return false;
        }
        if (self.zoom_from.is_some() || ed.tool == Tool::Zoom) && x >= canvas::GUTTER_W {
            self.zooming = Some(TimeZoom::begin(
                ed,
                (x - canvas::GUTTER_W).clamp(0.0, vp.w),
                mods.alt,
            ));
            return true;
        }
        let views = Self::lanes_of(ed);
        let ly = y - ruler_h;
        // The gutter's mic selector, ahead of every other gesture: an
        // open menu owns the next press, and a press on a lane's chip
        // opens its menu instead of switching lanes.
        if let Some(open) = self.mic_menu.take() {
            enum Pick {
                Mic(usize),
                Solo,
            }
            let picked = views.iter().find(|l| l.lane == open).and_then(|l| {
                let row = |i: usize| num::coord(i).mul_add(MIC_ITEM_H, l.y + MIC_MENU_TOP);
                if !(4.0..=4.0 + MIC_MENU_W).contains(&x) {
                    return None;
                }
                for (i, (ti, ..)) in l.members.iter().enumerate() {
                    if ly >= row(i) && ly < row(i) + MIC_ITEM_H {
                        return Some(Pick::Mic(*ti));
                    }
                }
                let solo_row = row(l.members.len());
                (ly >= solo_row && ly < solo_row + MIC_ITEM_H).then_some(Pick::Solo)
            });
            match picked {
                Some(Pick::Mic(track)) => {
                    ed.switch_track(track);
                }
                Some(Pick::Solo) => {
                    if let Some(l) = ed.tracks.layout_mut().lane_mut(open) {
                        l.solo_mic = !l.solo_mic;
                    }
                }
                None => {}
            }
            return true;
        }
        if let Some(l) = views.iter().find(|l| {
            l.is_role
                && l.members.len() > 1
                && (4.0..=canvas::GUTTER_W).contains(&x)
                && ly >= l.y + MIC_CHIP_TOP
                && ly < l.y + MIC_CHIP_TOP + MIC_ITEM_H
        }) {
            self.mic_menu = Some(l.lane);
            return true;
        }
        // A press near a role lane's hit line picks the hit up for a
        // slip/stretch instead of switching lanes.
        if self.editable && self.press_on_hits(ed, &views, (x, y), ruler_h, mods, out) {
            return true;
        }
        // Otherwise the press selects the lane under it. Clicking
        // targets a lane, never a track within one.
        let sy = ly + ed.stack_scroll;
        let rows = ed.tracks.stack(
            num::sample(vp.h / 4096.0) * 4096.0,
            Editor::ACTIVE_BOOST,
            ed.lane_floor().max(MIN_LANE),
        );
        let hit = expression_editor_core::tracks::Workspace::row_at(
            &rows,
            num::sample(sy / 4096.0) * 4096.0,
        )
        .filter(|&lane| Some(lane) != ed.tracks.active_lane())
        .and_then(|lane| {
            ed.tracks
                .lane_tracks(lane)
                .into_iter()
                .find(|&i| ed.tracks.track(i).is_some_and(|t| !t.hidden))
        });
        if let Some(track) = hit {
            ed.switch_track(track);
            return true;
        }
        false
    }

    /// The editing half of a press: a cut, a pick-up, a snap, an add.
    /// `true` when the press was one of those.
    fn press_on_hits(
        &mut self,
        ed: &Editor,
        views: &[LaneView],
        at: (f64, f64),
        ruler_h: f64,
        mods: Mods,
        out: &mut Vec<HitGesture>,
    ) -> bool {
        let (x, y) = at;
        let vp = ed.viewport;
        let ly = y - ruler_h;
        let lx = x - canvas::GUTTER_W;
        let in_lane = |l: &LaneView| l.detects && ly >= l.y && ly < l.y + l.h;
        let on_lane = views.iter().any(in_lane);
        let on_marker = views
            .iter()
            .filter(|l| in_lane(l))
            .any(|l| l.notes.iter().any(|n| (n.x - lx).abs() <= SLIP_PICK_PX));
        // What a press means comes from the map, not from this
        // handler, so the drum surface can be rebound like the roll.
        let context = if on_marker {
            MouseContext::Hit
        } else {
            MouseContext::Lane
        };
        let act = |g: MouseGesture| ed.mouse.resolve_for(context, g, mods, ed.tool);
        let (view_start, px_per_sec) =
            view_span_secs(ed).map_or((0.0, 0.0), |(v0, v1)| (v0, vp.w / (v1 - v0).max(1e-9)));

        // The razor gets first refusal on a lane, the way an armed tool
        // does on the roll.
        let razor = ed.tool == Tool::Razor;
        if on_lane
            && (razor || act(MouseGesture::Click) == Action::SplitTake)
            && let Some((v0, v1)) = view_span_secs(ed)
        {
            let at = (lx / vp.w.max(1.0)).mul_add(v1 - v0, v0);
            out.push(HitGesture::Split { at });
            return true;
        }
        // Two presses inside the window and the pick radius are a
        // double click.
        let now = Instant::now();
        let double = self.last_press.is_some_and(|(t0, px, py)| {
            now.duration_since(t0).as_millis() < DOUBLE_MS
                && (x - px).abs() <= SLIP_PICK_PX
                && (y - py).abs() <= SLIP_PICK_PX
        });
        self.last_press = Some((now, x, y));
        let both = act(MouseGesture::Drag) == Action::MoveHitBothEnds;
        let picked = pick_hit(views, lx, ly, x, both);
        if let Some(s) = picked {
            let sel = SelectedHit {
                lane_name: s.lane_name.clone(),
                hit_secs: s.hit_secs,
                prev_secs: s.prev_secs,
                next_secs: s.next_secs,
                lane_y: s.lane_y,
                lane_h: s.lane_h,
            };
            let grid_secs = Self::grid_secs(ed);
            // Double-click snaps the hit to its nearest division.
            if double && grid_secs > 0.0 && act(MouseGesture::DoubleClick) == Action::SnapHitToGrid
            {
                let target = (s.hit_secs / grid_secs).round() * grid_secs;
                let delta = target - s.hit_secs;
                if delta.abs() > 1e-9 {
                    self.emit_move(&sel, delta, false, out);
                }
                let mut sel = sel;
                sel.hit_secs = target;
                self.selected = Some(sel);
                return true;
            }
            self.slipping = Some(s);
            return true;
        }
        // A click on empty role-lane audio that the map says adds a hit
        // adds one where the click meant.
        if act(MouseGesture::Click) == Action::AddHit
            && px_per_sec > 0.0
            && let Some(l) = views.iter().find(|l| in_lane(l))
        {
            out.push(HitGesture::Add {
                lane: l.name.clone(),
                at: view_start + lx / px_per_sec,
            });
            return true;
        }
        false
    }

    /// The pointer moved. `true` when the picture changed.
    pub fn moved(&mut self, ed: &mut Editor, x: f64, y: f64, mods: Mods) -> bool {
        self.wheel_anchor = Some((x, y));
        let vp = ed.viewport;
        if let Some(mut zoom) = self.zooming {
            zoom.update(ed, (x - canvas::GUTTER_W).clamp(0.0, vp.w), mods.shift);
            self.zooming = Some(zoom);
            return true;
        }
        if let Some(s) = self.slipping.as_mut() {
            s.x = x;
            let both = ed
                .mouse
                .resolve_for(MouseContext::Hit, MouseGesture::Drag, mods, ed.tool)
                == Action::MoveHitBothEnds;
            s.both = both;
            return true;
        }
        let Some((lx, ly)) = self.panning else {
            return false;
        };
        // Time on the shared camera, and the stack's own scroll for
        // vertical — the tracks are stacked in a list, not laid out on
        // a pitch axis.
        ed.pan_px(x - lx, 0.0);
        ed.stack_scroll = (ed.stack_scroll - (y - ly)).max(0.0);
        self.panning = Some((x, y));
        true
    }

    /// The button came up. `true` when a gesture ended.
    pub fn release(&mut self, ed: &mut Editor, out: &mut Vec<HitGesture>) -> bool {
        if let Some(zoom) = self.zooming.take() {
            zoom.finish(ed);
            return true;
        }
        if let Some(s) = self.slipping.take() {
            let px_per_sec =
                view_span_secs(ed).map_or(0.0, |(v0, v1)| ed.viewport.w / (v1 - v0).max(1e-9));
            let delta = if px_per_sec > 0.0 {
                (s.x - s.x0) / px_per_sec
            } else {
                0.0
            };
            let mut sel = SelectedHit {
                lane_name: s.lane_name.clone(),
                hit_secs: s.hit_secs,
                prev_secs: s.prev_secs,
                next_secs: s.next_secs,
                lane_y: s.lane_y,
                lane_h: s.lane_h,
            };
            // A press that never travelled is a click, not a drag — it
            // selects the hit. Half a millisecond is below anything a
            // hand meant.
            if delta.abs() > 0.0005 {
                self.emit_move(&sel, delta, s.both, out);
                sel.hit_secs += delta;
            }
            self.selected = Some(sel);
            return true;
        }
        self.panning.take().is_some()
    }

    /// The wheel, in notches, on the shared bindings with the stack's
    /// meanings: horizontal zoom is the shared time camera, vertical
    /// scroll is the lane stack.
    pub fn wheel(&mut self, ed: &mut Editor, dx: f64, dy: f64, mods: Mods) -> bool {
        let Some(action) = crate::scroll::action_for(dx, dy, mods) else {
            return false;
        };
        let vp = ed.viewport;
        let (ax, _) = self
            .wheel_anchor
            .unwrap_or_else(|| (vp.w.mul_add(0.5, canvas::GUTTER_W), vp.h * 0.5));
        let x = (ax - canvas::GUTTER_W).max(0.0);
        let travel = if dx.abs() > dy.abs() { dx } else { dy };
        let factor = (travel.abs() / ZOOM_DIVISOR).exp();
        let zoom_in = travel < 0.0;
        match action.as_str() {
            "view.hscroll" => ed.pan_px(-travel * PAN_GAIN, 0.0),
            "view.vscroll" => {
                ed.stack_scroll = dy.mul_add(PAN_GAIN, ed.stack_scroll).max(0.0);
            }
            "view.zoom_h" | "view.zoom_both" => {
                ed.zoom_time_at(x, if zoom_in { factor } else { 1.0 / factor });
            }
            _ => return false,
        }
        true
    }

    /// A key, by its browser-style name. `true` when the stack took it.
    pub fn key(
        &mut self,
        ed: &mut Editor,
        key: &str,
        mods: Mods,
        out: &mut Vec<HitGesture>,
    ) -> bool {
        let vp = ed.viewport;
        if key.eq_ignore_ascii_case("z") && !mods.ctrl && !mods.alt {
            if self.zoom_from.is_none() {
                self.zoom_from = Some(ed.tool);
                ed.tool = Tool::Zoom;
            }
            return true;
        }
        if key == "Escape" {
            self.cancel(ed);
            return true;
        }
        // Zoom and paging work with nothing selected.
        let factor = match key {
            "+" | "=" => Some(1.4),
            "-" | "_" => Some(1.0 / 1.4),
            _ => None,
        };
        if let Some(f) = factor {
            ed.zoom_time_at(vp.w * 0.5, f);
            return true;
        }
        match key {
            "]" => return ed.page_bars(BARS_PER_PAGE, 1),
            "[" => return ed.page_bars(BARS_PER_PAGE, -1),
            "\\" => return ed.frame_bars(BARS_PER_PAGE),
            _ => {}
        }
        let Some(sel) = self.selected.clone() else {
            return false;
        };
        match key {
            "ArrowLeft" | "ArrowRight" if self.editable => {
                // The division, with Shift for the fine step: 1 ms.
                let step = if mods.shift {
                    0.001
                } else {
                    Self::grid_secs(ed)
                };
                if step <= 0.0 {
                    return false;
                }
                let delta = if key == "ArrowLeft" { -step } else { step };
                self.emit_move(&sel, delta, false, out);
                let mut sel = sel;
                sel.hit_secs += delta;
                self.selected = Some(sel);
                true
            }
            "Delete" | "Backspace" => {
                if self.editable {
                    out.push(HitGesture::Remove {
                        lane: sel.lane_name,
                        hit: sel.hit_secs,
                    });
                }
                self.selected = None;
                true
            }
            _ => false,
        }
    }

    /// A key came up: `z` lets the zoom tool go.
    pub const fn key_up(&mut self, ed: &mut Editor, key: &str) -> bool {
        if key.eq_ignore_ascii_case("z")
            && let Some(previous) = self.zoom_from.take()
        {
            ed.tool = previous;
            return true;
        }
        false
    }
}

/// The hit nearest a press on a role lane, picked up for a drag.
fn pick_hit(views: &[LaneView], lx: f64, ly: f64, x: f64, both: bool) -> Option<SlipDrag> {
    views
        .iter()
        .filter(|l| l.detects && ly >= l.y && ly < l.y + l.h)
        .flat_map(|l| l.notes.iter().map(move |n| (l, n)))
        .filter(|(_, n)| (n.x - lx).abs() <= SLIP_PICK_PX)
        .min_by(|(_, a), (_, b)| (a.x - lx).abs().total_cmp(&(b.x - lx).abs()))
        .map(|(l, n)| {
            let next = l
                .notes
                .iter()
                .map(|m| m.at_secs)
                .filter(|&t| t > n.at_secs + 1e-9)
                .fold(f64::INFINITY, f64::min);
            let prev = l
                .notes
                .iter()
                .map(|m| m.at_secs)
                .filter(|&t| t < n.at_secs - 1e-9)
                .fold(f64::NEG_INFINITY, f64::max);
            SlipDrag {
                hit_secs: n.at_secs,
                prev_secs: prev,
                next_secs: next,
                lane_name: l.name.clone(),
                x0: x,
                x,
                both,
                hit_x: n.x,
                lane_y: l.y,
                lane_h: l.h,
            }
        })
}

/// Apply a hit gesture to the editor's own document — the hit list —
/// for a host with no audio to write to.
///
/// The real host slips and stretches audio and re-detects; this only
/// keeps the picture honest: a slipped hit moves, a removed one goes,
/// an added one appears, a split cuts nothing it can see. What a host
/// does with the audio is its own business, and this is what it does
/// with the list beforehand.
// r[impl flow.drums.editing.hands]
pub fn apply_to_document(ed: &mut Editor, gesture: &HitGesture) -> bool {
    use expression_editor_core::doc::NoteId;
    use expression_editor_core::kit::LaneRole;
    let ups = ed.doc.time_base.units_per_second(ed.bpm);
    let lane_track = |ed: &Editor, lane: &str| -> Option<usize> {
        let role = LaneRole::ALL.into_iter().find(|r| r.label() == lane)?;
        ed.tracks
            .role_members(role)
            .iter()
            .filter_map(|g| ed.tracks.index_of_guid(g))
            .find(|&i| ed.tracks.track(i).is_some_and(|t| !t.hidden))
    };
    match gesture {
        HitGesture::Slip { hit, delta, .. } | HitGesture::Stretch { hit, delta, .. } => {
            let target = hit * ups;
            let tol = 0.015 * ups;
            let mut moved = false;
            for n in &mut ed.doc.notes {
                if (n.start - target).abs() <= tol {
                    let len = n.end - n.start;
                    n.start += delta * ups;
                    n.end = n.start + len;
                    moved = true;
                }
            }
            moved
        }
        HitGesture::Add { lane, at } => {
            let Some(track) = lane_track(ed, lane) else {
                return false;
            };
            ed.switch_track(track);
            let start = at * ups;
            let end = ed
                .doc
                .notes
                .iter()
                .map(|n| n.start)
                .filter(|&s| s > start + 1e-6)
                .fold(ed.doc.end, f64::min);
            let id = NoteId(
                ed.doc
                    .notes
                    .iter()
                    .map(|n| n.id.0)
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1),
            );
            let mut note = expression_editor_core::Note::new(id, start, end.max(start + 1.0), 1);
            note.velocity = 1.0;
            ed.doc.push(note);
            true
        }
        HitGesture::Remove { lane, hit } => {
            let Some(track) = lane_track(ed, lane) else {
                return false;
            };
            ed.switch_track(track);
            let target = hit * ups;
            let tol = 0.015 * ups;
            let before = ed.doc.notes.len();
            ed.doc.notes.retain(|n| (n.start - target).abs() > tol);
            ed.doc.notes.len() != before
        }
        HitGesture::Split { .. } => false,
    }
}
