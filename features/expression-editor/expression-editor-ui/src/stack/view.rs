//! Stack component and pointer/keyboard interaction.
use super::geometry::{
    CHROME_ROW_H, LaneView, RULER_TICKS_H, chrome_shelves, lanes, view_span_secs,
};
use crate::{canvas, theme};
use dioxus::prelude::*;
use dioxus_elements::input_data::MouseButton;
use expression_editor_core::Editor;
pub use expression_editor_core::drum::HitGesture;
use expression_editor_core::mouse::{Context as MouseContext, Gesture as MouseGesture};
use keyboard_types::{Key, Modifiers};

/// One slip/stretch drag: which hit is being slid, and where the
/// pointer is.
///
/// Everything the release needs is captured at the press, so a lane
/// re-layout mid-drag cannot re-target the gesture.
#[derive(Clone, Debug)]
struct SlipDrag {
    /// The dragged hit's onset, seconds.
    hit_secs: f64,
    /// The previous hit's onset — `f64::NEG_INFINITY` for the first.
    prev_secs: f64,
    /// The next hit's onset, seconds — `f64::INFINITY` for the last.
    next_secs: f64,
    /// The lane the hit lives in, by its drawn name (`"Kick"`).
    lane_name: String,
    /// Where the press landed, element x.
    x0: f64,
    /// Where the pointer is now, element x.
    x: f64,
    /// Shift as of the last pointer event — the BothStretch law.
    shift: bool,
    /// The hit line's own x at the press (gutter-relative), so the
    /// ghost draws on the line rather than under the finger.
    hit_x: f64,
    /// The lane's strip, for the ghost lines (y from the lane group's
    /// origin).
    lane_y: f64,
    lane_h: f64,
}

/// The selected hit — the unit of keyboard editing. Selection follows
/// the *time*, not a note index: a nudge moves the audio under the
/// drawing, and the next nudge must move from where the hit now is.
// r[impl drums.manual.nudge]
#[derive(Clone, Debug)]
struct SelectedHit {
    lane_name: String,
    hit_secs: f64,
    prev_secs: f64,
    next_secs: f64,
    lane_y: f64,
    lane_h: f64,
}

/// How close to a hit line a press must land to pick it up, px.
const SLIP_PICK_PX: f64 = 6.0;

// ── the gutter's mic selector ────────────────────────────────────────
//
// Shared by the renderer and the press handler, which must agree on
// this geometry exactly or clicks land beside what they aim at.
/// The chip's top, below the role eyebrow, lane-relative.
/// How far past a lane's edges a hit is still mounted, in pixels —
/// enough that a pan reveals hits that are already there.
const HIT_MARGIN: f64 = 96.0;

pub(super) const MIC_CHIP_TOP: f64 = 18.0;
/// The menu's first item top, lane-relative.
pub(super) const MIC_MENU_TOP: f64 = 36.0;
/// One row of chip or menu.
pub(super) const MIC_ITEM_H: f64 = 16.0;
/// The open menu's width — wider than the gutter, over the lane.
pub(super) const MIC_MENU_W: f64 = 120.0;

/// How much taller the lane being edited is than the rest.
/// Kept in core so the editor's auto-scroll lays lanes out exactly
/// as the renderer does; a mismatch would scroll to the wrong place.
use expression_editor_core::Editor as CoreEditor;
const ACTIVE_BOOST: f32 = CoreEditor::ACTIVE_BOOST;

/// Shortest a lane may be, in pixels.
///
/// Enough to click, because clicking a lane is how you make it the
/// active one — a lane too small to hit is a track you cannot get back
/// to.
const MIN_LANE: f32 = 22.0;

/// Bars the view pages by, and frames.
///
/// Four, because that is the phrase drummers play in and the unit a
/// take gets edited in — fix a bar of a fill and you want the three
/// around it for context, not a screen of the whole song.
const BARS_PER_PAGE: usize = 4;

/// Every track at once, on one timeline.
///
/// Read-only by design. The stack answers "which track needs work" and
/// "is this in time with that"; the roll is where the work happens.
/// Clicking a lane makes it active, which is the handover between the
/// two — and it means the one gesture the stack does take is the one
/// that gets you out of it.
#[component]
pub fn StackView(
    editor: Signal<Editor>,
    /// Called when a hand edit leaves the stack — a slip or stretch
    /// drag released, a nudge key, a hit added or thrown out. `None`
    /// disables all of them: without a writer the gestures would lie.
    // r[impl drums.manual.slip]
    #[props(default)]
    on_hit: Option<EventHandler<HitGesture>>,
    /// Whether a drag/nudge stretches (WARP) instead of slipping
    /// (SPLIT) — the quantize panel's write mode, threaded down so the
    /// hand gesture and the Apply button agree about what an edit is.
    // r[impl drums.manual.stretch]
    #[props(default)]
    warp: bool,
    /// One grid division in seconds — the nudge step, and what a
    /// double-clicked hit snaps to. `<= 0` disables nudge and snap.
    // r[impl drums.manual.nudge]
    #[props(default)]
    grid_secs: f64,
    /// The transport's position, seconds, when a host has one — the
    /// stack draws it as a playhead so "where am I" survives leaving
    /// the arrangement. `None` (a demo scene, a test) draws nothing.
    #[props(default)]
    playhead_secs: Option<Signal<f64>>,
    /// The take's fills, as `(start, end)` in seconds — drawn as bands
    /// behind the lanes so the parts a quantize will leave alone are
    /// visible before it runs, not discovered afterwards.
    // r[impl drums.fills.draw]
    #[props(default)]
    fills: Vec<(f64, f64)>,
) -> Element {
    let mut editor = editor;
    let mut zoom_from = use_signal(|| None::<expression_editor_core::Tool>);
    let mut zooming = use_signal(|| None::<super::zoom::TimeZoom>);
    // Where a middle-drag pan last was.
    let mut panning = use_signal(|| None::<(f64, f64)>);
    // An in-flight slip/stretch drag on a role lane's hit.
    let mut slipping = use_signal(|| None::<SlipDrag>);
    // The selected hit, if any — what the keys act on.
    let mut selected = use_signal(|| None::<SelectedHit>);
    // The previous press, for double-click detection: no dblclick
    // event reaches this renderer, so two presses within the window
    // and the pick radius are the gesture.
    let mut last_press = use_signal(|| None::<(std::time::Instant, f64, f64)>);
    // The lane whose mic menu is open, by layout-lane index. One at a
    // time: a second chip click moves the menu rather than stacking.
    let mut mic_menu = use_signal(|| None::<usize>);
    // Where the pointer last was, for anchoring wheel zoom. (Focus
    // needs no handling: Blitz focuses the nearest focusable ancestor
    // on pointer-down — an FTS patch in the fork.)
    let mut wheel_anchor = use_signal(|| None::<(f64, f64)>);

    // Where each render leaves the drawing, and the shaper for its
    // labels. Both kept across renders; the widget itself must be built
    // exactly once — `CustomWidgetAttr` is write-once, and rebuilding it
    // is what makes a painted surface go blank. See `crate::roll`.
    let slot = use_hook(crate::roll_widget::SceneSlot::new);
    let labels =
        use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(crate::text::Labeller::new())));
    // What the `data` attribute carries.
    //
    // Under Blitz this is the custom-widget seam: the renderer calls the
    // widget's `paint` and replays the scene the render above left in the
    // slot. A WebView has no such thing and panics on the attribute
    // outright ("Any attributes are not supported by the current
    // renderer"), so there it carries nothing and the surface is drawn by
    // the webview path instead. One rsx tree either way — the gestures,
    // the box and the layout are identical, and only the seam moves.
    #[cfg(not(feature = "webview"))]
    let widget = use_hook(|| {
        dioxus_native_dom::CustomWidgetAttr::new(crate::roll_widget::SceneWidget::new(slot.clone()))
    });
    #[cfg(feature = "webview")]
    let widget = "";

    let ed = editor.read();
    let vp = ed.viewport;
    let stack_scroll = ed.stack_scroll;
    let lanes = lanes(&ed, ACTIVE_BOOST, ed.lane_floor().max(MIN_LANE));
    let ticks = canvas::ruler(&ed);
    let chrome_rows = chrome_shelves(&ed);
    let row_of = |lane: &Option<(u32, String)>, is_region: bool| -> usize {
        let key = lane.as_ref().map(|(i, _)| *i);
        chrome_rows
            .iter()
            .position(|(i, r, _)| *i == key && *r == is_region)
            .unwrap_or(0)
    };

    // The song's sections across the ruler — clipped to the view, with
    // the label given only the room its span actually has.
    let sections: Vec<(f64, f64, String, String, usize)> = {
        let (t0, t1) = ed.camera.time_span(ed.viewport);
        ed.doc
            .regions
            .iter()
            .filter(|r| r.end > t0 && r.start < t1)
            .map(|r| {
                let x0 = ed.camera.x(r.start.max(t0)).max(0.0);
                let x1 = ed.camera.x(r.end.min(t1)).min(vp.w);
                let fit = (((x1 - x0) - 6.0) / 5.5).max(0.0) as usize;
                let label: String = r.label.chars().take(fit).collect();
                let color = r.color.clone().unwrap_or_else(|| theme::SURFACE_BAR.into());
                (x0, x1, label, color, row_of(&r.lane, true))
            })
            .collect()
    };
    // The song's markers — named *points*, unlike regions' named spans.
    // Sessions use both, and which one carries a song's structure comes
    // down to how the project was set up, so the ruler has to show
    // either. Kept as points: not every marker is a section boundary
    // ("tempo change", "back to 4/4"), so stretching each one to the
    // next would draw a structure nobody wrote.
    // r[impl drums.chrome.markers]
    let marks: Vec<(f64, String, String, usize)> = {
        let (t0, t1) = ed.camera.time_span(ed.viewport);
        let visible: Vec<&expression_editor_core::doc::Marker> = ed
            .doc
            .markers
            .iter()
            .filter(|m| m.t >= t0 && m.t <= t1)
            .collect();
        visible
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let x = ed.camera.x(m.t).clamp(0.0, vp.w);
                // Clip the label to the room before the next marker, so
                // a dense passage reads as a row of ticks with the names
                // that fit rather than a pile of overlapping words. At a
                // whole-song zoom `SOLO A`, `SOLO B` and `CH 3` land
                // within a few pixels of each other and would otherwise
                // print on top of one another.
                // Room is measured against the next marker *in the
                // same lane*: a marker on `SECTIONS` does not crowd one
                // on `SONG`, because they are drawn on different rows.
                let lane_key = m.lane.as_ref().map(|(i, _)| *i);
                let next = visible
                    .iter()
                    .skip(i + 1)
                    .find(|n| n.lane.as_ref().map(|(i, _)| *i) == lane_key)
                    .map_or(vp.w, |n| ed.camera.x(n.t).clamp(0.0, vp.w));
                let fit = (((next - x) - 5.0) / 5.0).max(0.0) as usize;
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
            .collect()
    };

    // The ruler grows a shelf at a time rather than dividing a fixed
    // band: three shelves in the 15px band today's constant allows
    // would be 5px each, which cannot hold a 9px label. One shelf is
    // sized to match the old fixed band exactly, so the common
    // single-lane project looks precisely as it did.
    let mark_row_h = CHROME_ROW_H;
    // One source for the height, shared with the pointer maths below.
    let ruler_h = CHROME_ROW_H * chrome_rows.len().max(1) as f64 + RULER_TICKS_H;

    // Fill bands, clipped to the view.
    // r[impl drums.fills.draw]
    let fill_bands: Vec<(f64, f64)> = view_span_secs(&ed)
        .map(|(v0, v1)| {
            fills
                .iter()
                .filter(|(s, e)| *e > v0 && *s < v1)
                .map(|(s, e)| {
                    let px = |t: f64| (t - v0) / (v1 - v0).max(1e-9) * vp.w;
                    (px(s.max(v0)).max(0.0), px(e.min(v1)).min(vp.w))
                })
                .collect()
        })
        .unwrap_or_default();

    // The bands as text, for the `data-fills` attribute below.
    let fill_attr = fill_bands
        .iter()
        .map(|(x0, x1)| format!("{x0:.1}:{x1:.1}"))
        .collect::<Vec<_>>()
        .join(",");

    let (view0, px_per_sec) = view_span_secs(&ed)
        .map(|(v0, v1)| {
            if (v1 - v0).abs() < 1e-9 {
                (v0, 0.0)
            } else {
                (v0, vp.w / (v1 - v0))
            }
        })
        .unwrap_or((0.0, 0.0));
    drop(ed);

    // One place turns a released drag (or a synthetic delta from a key)
    // into the outgoing gesture, so the drag, the nudge and the snap
    // cannot disagree about what an edit is in each write mode.
    let emit_move = {
        let on_hit = on_hit;
        move |s: &SelectedHit, delta: f64, both: bool| {
            let Some(h) = &on_hit else { return };
            let g = if warp {
                // r[impl drums.manual.stretch]
                HitGesture::Stretch {
                    hit: s.hit_secs,
                    prev: s.prev_secs,
                    next: s.next_secs,
                    delta,
                    both,
                }
            } else {
                // r[impl drums.manual.slip]
                HitGesture::Slip {
                    hit: s.hit_secs,
                    next: s.next_secs,
                    delta,
                }
            };
            h.call(g);
        }
    };

    // The picture, built here where reading signals is ordinary and
    // safe, and left in the slot for the renderer to replay. See
    // `super::paint` for why this is paint rather than elements.
    let stack_w = vp.w + canvas::GUTTER_W;
    let stack_h = vp.h + ruler_h;
    // One render of this surface. Read against the frame rate: more
    // rebuilds than frames means work no frame ever showed, which is
    // exactly what a high-polling-rate mouse does to a drag.
    if let Some(frames) = try_consume_context::<crate::roll_widget::Frames>() {
        frames.built();
    }

    let chrome = super::paint::StackChrome {
        chrome_rows: &chrome_rows,
        sections: &sections,
        marks: &marks,
        ticks: &ticks,
        fill_bands: &fill_bands,
        mic_menu: mic_menu(),
        ruler_h,
        chrome_row_h: CHROME_ROW_H,
        mark_row_h,
        mic_chip_top: MIC_CHIP_TOP,
        mic_menu_top: MIC_MENU_TOP,
        mic_item_h: MIC_ITEM_H,
        mic_menu_w: MIC_MENU_W,
        hit_margin: HIT_MARGIN,
        host_lanes: false,
    };
    // The same picture, drawn the way each renderer is fast at.
    //
    // Blitz replays a scene: elements cost it style and box construction
    // per node, and the stack's ~1900 of them measured 154 ms a frame
    // against 6.5 for a scene. A browser engine is the other way round —
    // it is built for these elements, and painting for it instead means
    // a rasterizer, a worker thread, a byte protocol and a canvas, all
    // to avoid what it does natively. Both read the same
    // `geometry::LaneView`s, so there is one description of where things
    // go and two of how they are put on screen.
    #[cfg(not(feature = "webview"))]
    let surface_paint = {
        slot.put(super::paint::stack_scene(
            &lanes,
            &chrome,
            stack_w,
            stack_h,
            &mut labels.borrow_mut(),
            &crate::paint::Look::default(),
        ));
        String::new()
    };
    #[cfg(feature = "webview")]
    let surface_paint = {
        let _ = (&slot, &labels);
        String::new()
    };

    // The drawing itself, where the renderer wants elements.
    // Behind the gesture surface and inert, so every handler still lands
    // on the same element it did — the `object` below keeps them, in both
    // presentations, and only what is underneath changes.
    #[cfg(feature = "webview")]
    let surface_markup = rsx! {
        svg {
            // `will-change: transform` puts the drawing on a compositing
            // layer of its own.
            //
            // Without it the playhead — which moves many times a second
            // from its own leaf component, in the overlay below — shares
            // a layer with these ~1900 elements, and every tick makes the
            // engine re-rasterize all of them. The measurement that
            // pointed here: the runtime probe shows only three components
            // rendering at all (the two playheads and the transport bar),
            // so a slow frame was never diffing or rebuilding this
            // subtree. It was painting it, again, for a line that moved
            // somewhere else.
            style: "position: absolute; left: 0; top: 0; display: block; \
                    width: {stack_w:.0}px; height: {stack_h:.0}px; \
                    pointer-events: none; will-change: transform;",
            view_box: "0 0 {stack_w:.0} {stack_h:.0}",
            preserve_aspect_ratio: "none",
            {super::markup::stack_markup(&lanes, &chrome, editor, vp)}
        }
    };
    #[cfg(not(feature = "webview"))]
    let surface_markup = rsx! {};

    // ── The overlay's numbers, all resolved before the markup ──
    //
    // Every branch here used to be an `rsx!` conditional, and they toggle
    // on exactly the events that also run hit testing: a selection
    // appears on press, a ghost on drag, a marquee on a zoom sweep. A
    // template whose node count depends on its state is what blitz-dom
    // walks a stale path through — `paint_children` keeps the id of a
    // node that has been removed, and the next pointer move unwraps a
    // `None` out of the slab. `KeyPanel` in `crate::roll` carries the
    // same note for the same reason.
    //
    // So the overlay has ONE shape: the nodes are always there and the
    // state moves them and fades them. Invisible is `opacity: 0`, not
    // absent.
    let sel = selected().filter(|_| px_per_sec > 0.0);
    let sel_on = sel.is_some();
    let sel_x = sel
        .as_ref()
        .map_or(0.0, |s| (s.hit_secs - view0) * px_per_sec);
    let sel_y = sel.as_ref().map_or(0.0, |s| s.lane_y);
    let sel_h = sel.as_ref().map_or(0.0, |s| s.lane_h);

    let slip = slipping();
    let slip_on = slip.is_some();
    let slip_x = slip.as_ref().map_or(0.0, |s| s.hit_x);
    let slip_to = slip.as_ref().map_or(0.0, |s| s.hit_x + (s.x - s.x0));
    let slip_y = slip.as_ref().map_or(0.0, |s| s.lane_y);
    let slip_h = slip.as_ref().map_or(0.0, |s| s.lane_h);

    let marquee = zooming().filter(|zoom| zoom.marquee);
    let marquee_on = marquee.is_some();
    let marquee_x = marquee
        .as_ref()
        .map_or(0.0, |z| z.origin.min(z.current) + canvas::GUTTER_W);
    let marquee_w = marquee
        .as_ref()
        .map_or(0.0, |z| (z.current - z.origin).abs());

    // The surface's pixels, where the renderer needs an element to put
    // them in. Built here rather than inline because `rsx!` takes no
    // `cfg` on an element, and resolved per build so the tree still has
    // exactly one shape at runtime.

    let zoom_cursor =
        if zoom_from().is_some() || editor.read().tool == expression_editor_core::Tool::Zoom {
            "zoom-in"
        } else {
            "pointer"
        };
    rsx! {
        div {
            // Focusable so the nudge keys have somewhere to land; the
            // roll's canvas cell does the same. Keys are bound here,
            // locally — the stacked view's bindings must not leak into
            // the roll's keymap.
            style: "position: relative; display: block; width: 100%; \
                    height: 100%; outline: none;",
            tabindex: "0",
            "data-testid": "stack-cell",
            // What this surface is currently showing, for anything
            // outside it that needs to know the view moved.
            //
            // The stack used to advertise that in its markup by
            // accident: every tick, hit and lane band was an element, so
            // a pan changed thousands of them and a test could watch the
            // DOM to see the camera move. Painted, the markup is silent
            // — which would let a benchmark record a gesture as
            // "working" while the picture never moved, the one failure
            // mode the stress harness exists to catch. So the pane says
            // so itself: time origin, scale, and the lane scroll.
            "data-view": "{view0:.4},{px_per_sec:.4},{stack_scroll:.1}",
            // And where its fill bands are, `x0:x1,...` in the surface's
            // own units, for the same reason: the wash is painted, so a
            // test that asks whether it landed over the right seconds
            // has nothing else to read.
            // r[impl drums.fills.draw]
            "data-fills": "{fill_attr}",
            onkeydown: move |e: KeyboardEvent| {
                if e.is_auto_repeating() {
                    return;
                }
                let modifiers = e.modifiers();
                if matches!(e.key(), Key::Character(ref key) if key.eq_ignore_ascii_case("z"))
                    && !modifiers.intersects(Modifiers::CONTROL | Modifiers::META | Modifiers::ALT) {
                    if zoom_from.read().is_none() {
                        let previous = editor.read().tool;
                        zoom_from.set(Some(previous));
                        editor.write().tool = expression_editor_core::Tool::Zoom;
                    }
                    e.prevent_default();
                    e.stop_propagation();
                    return;
                }
                if e.key() == Key::Escape {
                    if let Some(previous) = zoom_from.take() {
                        editor.write().tool = previous;
                    }
                    zooming.set(None);
                    slipping.set(None);
                    panning.set(None);
                }
                // Zoom keys work with nothing selected — asking for a
                // selection before you may look at something closer
                // gets the order of operations backwards.
                if let Key::Character(c) = e.key() {
                    let factor = match c.as_str() {
                        "+" | "=" => Some(1.4),
                        "-" | "_" => Some(1.0 / 1.4),
                        _ => None,
                    };
                    if let Some(f) = factor {
                        editor.write().zoom_time_at(vp.w * 0.5, f);
                        e.prevent_default();
                        return;
                    }
                    // Page the view a phrase at a time — the way drums
                    // actually get edited: frame four bars, fix them,
                    // move on. Bracket keys because they sit under the
                    // hand that is not on the mouse, and because
                    // PageUp/PageDown are a scroll on every other
                    // surface and would read as one here.
                    // r[impl drums.view.page-bars]
                    let step = match c.as_str() {
                        "]" => Some(1),
                        "[" => Some(-1),
                        _ => None,
                    };
                    if let Some(step) = step {
                        editor.write().page_bars(BARS_PER_PAGE, step);
                        e.prevent_default();
                        return;
                    }
                    // Frame the page without moving off it: the way back
                    // from a zoom that got away.
                    if c.as_str() == "\\" {
                        editor.write().frame_bars(BARS_PER_PAGE);
                        e.prevent_default();
                        return;
                    }
                }
                let Some(sel) = selected() else { return };
                let shift = e.modifiers().contains(Modifiers::SHIFT);
                match e.key() {
                    // r[impl drums.manual.nudge]
                    Key::ArrowLeft | Key::ArrowRight if on_hit.is_some() => {
                        // The division with Shift for the fine step:
                        // 1 ms, the sample-accurate trim.
                        let step = if shift { 0.001 } else { grid_secs };
                        if step <= 0.0 {
                            return;
                        }
                        let delta = if e.key() == Key::ArrowLeft { -step } else { step };
                        emit_move(&sel, delta, false);
                        let mut sel = sel;
                        sel.hit_secs += delta;
                        selected.set(Some(sel));
                    }
                    // r[impl drums.manual.add-remove]
                    Key::Delete | Key::Backspace => {
                        if let Some(h) = &on_hit {
                            h.call(HitGesture::Remove {
                                lane: sel.lane_name.clone(),
                                hit: sel.hit_secs,
                            });
                        }
                        selected.set(None);
                    }
                    _ => {}
                }
            },
            onkeyup: move |e: KeyboardEvent| {
                if matches!(e.key(), Key::Character(ref key) if key.eq_ignore_ascii_case("z")) {
                    if let Some(previous) = zoom_from.take() {
                        editor.write().tool = previous;
                    }
                    e.stop_propagation();
                }
            },
            onblur: move |_| {
                if let Some(previous) = zoom_from.take() {
                        editor.write().tool = previous;
                    }
                zooming.set(None);
                slipping.set(None);
                panning.set(None);
            },
        {surface_markup}
        object {
            "data": widget,
            // Explicit and out of flow, the same box the scene was built
            // for. A widget reports no intrinsic size, and blitz-paint
            // skips one whose box is zero. The svg this replaced carried
            // a `viewBox` with `preserveAspectRatio: none`, which
            // *stretched* the drawing if the element and the editor's
            // viewport ever disagreed; a scene cannot stretch, so a
            // disagreement now shows as unpainted margin instead of
            // silently distorted music. `sizing` is what keeps them
            // equal, and this is the thing that would reveal it failing.
            style: "position: absolute; left: 0; top: 0; display: block; \
                    width: {stack_w:.0}px; height: {stack_h:.0}px; \
                    touch-action: none; user-select: none; cursor: {zoom_cursor}; \
                    {surface_paint}",
            // No `onmounted` measure here, deliberately.
            //
            // This used to `spawn` and `await get_client_rect()` from the
            // mount handler — the re-entrancy pattern that caused #167,
            // and the one the Blitz testing notes say never to use: the
            // await re-enters the document while the mount that started
            // it is still on the stack.
            //
            // It is also unnecessary. `ExpressionEditor` already keeps
            // `ed.viewport` in step with the host's space through
            // `sizing::viewport_within`, in one effect, for whichever
            // view is showing. The stack was measuring itself only
            // because `chrome_of` charged it for a lane strip it does
            // not render, which made the shared answer wrong — so the
            // fix was to make the shared answer right rather than to
            // keep a second one.
            onpointermove: move |e: PointerEvent| {
                {
                    let c = e.data().element_coordinates();
                    wheel_anchor.set(Some((c.x, c.y)));
                }
                if let Some(mut zoom) = zooming() {
                    let c = e.data().element_coordinates();
                    zoom.update(&mut editor.write(), (c.x - canvas::GUTTER_W).clamp(0.0, vp.w), e.modifiers().contains(Modifiers::SHIFT));
                    zooming.set(Some(zoom));
                    return;
                }
                if let Some(mut s) = slipping() {
                    s.x = e.data().element_coordinates().x;
                    s.shift = e.data().modifiers().contains(Modifiers::SHIFT);
                    slipping.set(Some(s));
                    return;
                }
                let Some((lx, ly)) = panning() else { return };
                let c = e.data().element_coordinates();
                let mut ed = editor.write();
                // Time on the shared camera, and the stack's own scroll
                // for vertical — the tracks are stacked in a list, not
                // laid out on a pitch axis.
                ed.pan_px(c.x - lx, 0.0);
                ed.stack_scroll = (ed.stack_scroll - (c.y - ly)).max(0.0);
                drop(ed);
                panning.set(Some((c.x, c.y)));
            },
            onpointerup: move |_| {
                if let Some(zoom) = zooming.take() {
                    zoom.finish(&mut editor.write());
                    return;
                }
                if let Some(s) = slipping() {
                    slipping.set(None);
                    let delta = if px_per_sec > 0.0 {
                        (s.x - s.x0) / px_per_sec
                    } else {
                        0.0
                    };
                    let sel = SelectedHit {
                        lane_name: s.lane_name.clone(),
                        hit_secs: s.hit_secs,
                        prev_secs: s.prev_secs,
                        next_secs: s.next_secs,
                        lane_y: s.lane_y,
                        lane_h: s.lane_h,
                    };
                    // A press that never travelled is a click, not a
                    // drag — it *selects* the hit. Half a millisecond
                    // is below anything a hand meant.
                    if delta.abs() > 0.0005 {
                        // r[impl drums.manual.slip]
                        // r[impl drums.manual.stretch]
                        emit_move(&sel, delta, s.shift);
                        let mut sel = sel;
                        sel.hit_secs += delta;
                        selected.set(Some(sel));
                    } else {
                        selected.set(Some(sel));
                    }
                    return;
                }
                panning.set(None);
            },
            onpointerleave: move |_| {
                zooming.set(None);
                slipping.set(None);
                panning.set(None);
            },
            // The wheel, on the shared bindings (`scroll::action_for`)
            // with the stack's meanings: horizontal zoom is the shared
            // time camera, vertical scroll is the lane stack. Pitch
            // zoom has no meaning over a stack of lanes and is ignored
            // rather than remapped — a gesture that does something
            // different per view is how schemes rot.
            onwheel: move |e: WheelEvent| {
                let (dx, dy) = crate::scroll::notches(&e.delta());
                let m = e.data().modifiers();
                let mods = expression_editor_core::Mods {
                    ctrl: m.contains(Modifiers::CONTROL) || m.contains(Modifiers::META),
                    shift: m.contains(Modifiers::SHIFT),
                    alt: m.contains(Modifiers::ALT),
                };
                let Some(action) = crate::scroll::action_for(dx, dy, mods) else {
                    return;
                };
                let (ax, _) =
                    wheel_anchor().unwrap_or((canvas::GUTTER_W + vp.w * 0.5, vp.h * 0.5));
                let x = (ax - canvas::GUTTER_W).max(0.0);
                let travel = if dx.abs() > dy.abs() { dx } else { dy };
                let factor = (travel.abs() / crate::interaction::ZOOM_DIVISOR).exp();
                let zoom_in = travel < 0.0;
                let mut ed = editor.write();
                match action.as_str() {
                    "view.hscroll" => {
                        ed.pan_px(-travel * crate::interaction::PAN_GAIN, 0.0);
                    }
                    "view.vscroll" => {
                        ed.stack_scroll =
                            (ed.stack_scroll + dy * crate::interaction::PAN_GAIN).max(0.0);
                    }
                    "view.zoom_h" | "view.zoom_both" => {
                        ed.zoom_time_at(x, if zoom_in { factor } else { 1.0 / factor });
                    }
                    _ => {}
                }
                drop(ed);
                e.prevent_default();
            },
            onpointerdown: move |e: PointerEvent| {
                let c = e.data().element_coordinates();
                // Middle-drag pans, the same as it does on the roll. The
                // stack is the view with the *most* to scroll — every
                // track at once — and it was the one view with no way to
                // move around at all.
                if matches!(e.trigger_button(), Some(MouseButton::Auxiliary)) {
                    panning.set(Some((c.x, c.y)));
                    return;
                }
                if matches!(e.trigger_button(), Some(MouseButton::Primary))
                    && (zoom_from().is_some() || editor.read().tool == expression_editor_core::Tool::Zoom)
                    && c.x >= canvas::GUTTER_W {
                    zooming.set(Some(super::zoom::TimeZoom::begin(&editor.read(), (c.x - canvas::GUTTER_W).clamp(0.0, vp.w), e.modifiers().contains(Modifiers::ALT))));
                    e.prevent_default();
                    return;
                }
                // The gutter's mic selector, ahead of every other
                // gesture: an open menu owns the next press, and a
                // press on a lane's chip opens its menu instead of
                // switching lanes.
                {
                    let ed = editor.read();
                    let views = self::lanes(&ed, ACTIVE_BOOST, ed.lane_floor().max(MIN_LANE));
                    drop(ed);
                    let ly = c.y - ruler_h;
                    if let Some(open) = mic_menu() {
                        enum Pick {
                            Mic(usize),
                            Solo,
                        }
                        let picked = views.iter().find(|l| l.lane == open).and_then(|l| {
                            let row = |i: usize| l.y + MIC_MENU_TOP + i as f64 * MIC_ITEM_H;
                            if c.x < 4.0 || c.x > 4.0 + MIC_MENU_W {
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
                        mic_menu.set(None);
                        match picked {
                            Some(Pick::Mic(track)) => {
                                editor.write().switch_track(track);
                            }
                            Some(Pick::Solo) => {
                                let mut ed = editor.write();
                                if let Some(l) = ed.tracks.layout_mut().lane_mut(open) {
                                    l.solo_mic = !l.solo_mic;
                                }
                            }
                            None => {}
                        }
                        return;
                    }
                    if let Some(l) = views.iter().find(|l| {
                        l.is_role
                            && l.members.len() > 1
                            && c.x >= 4.0
                            && c.x <= canvas::GUTTER_W
                            && ly >= l.y + MIC_CHIP_TOP
                            && ly < l.y + MIC_CHIP_TOP + MIC_ITEM_H
                    }) {
                        mic_menu.set(Some(l.lane));
                        return;
                    }
                }
                // A press near a role lane's hit line picks the hit up
                // for a slip/stretch instead of switching lanes. Only
                // where a host is listening — without a writer the
                // gesture would be a lie.
                // r[impl drums.manual.slip]
                if on_hit.is_some() {
                    let ed = editor.read();
                    let views = self::lanes(&ed, ACTIVE_BOOST, ed.lane_floor().max(MIN_LANE));
                    drop(ed);
                    let ly = c.y - ruler_h;
                    let lx = c.x - canvas::GUTTER_W;
                    let mods = e.data().modifiers();
                    // What a press means comes from the map, not from
                    // this handler. These bindings used to be `if`
                    // statements here, which made the drum surface the
                    // one part of the editor that could not be rebound,
                    // could not be listed beside the roll's in the
                    // preferences, and could not be told apart from a
                    // gesture nobody had written.
                    // r[impl drums.mouse.contexts]
                    let m = expression_editor_core::tools::Mods {
                        shift: mods.contains(Modifiers::SHIFT),
                        ctrl: mods.contains(Modifiers::CONTROL),
                        alt: mods.contains(Modifiers::ALT),
                    };
                    let in_lane = |l: &LaneView| l.detects && ly >= l.y && ly < l.y + l.h;
                    let on_lane = views.iter().any(in_lane);
                    let on_marker = views
                        .iter()
                        .filter(|l| in_lane(l))
                        .any(|l| l.notes.iter().any(|n| (n.x - lx).abs() <= SLIP_PICK_PX));
                    let context = if on_marker {
                        MouseContext::Hit
                    } else {
                        MouseContext::Lane
                    };
                    let act = |g: MouseGesture| {
                        let ed = editor.read();
                        ed.mouse.resolve_for(context, g, m, ed.tool)
                    };

                    // The razor gets first refusal on a lane, the way an
                    // armed tool does on the roll. A cut is the one edit
                    // with no other gesture available, since every other
                    // one starts by grabbing a hit and a cut is for
                    // where there isn't one.
                    // r[impl drums.manual.split]
                    let razor = editor.read().tool == expression_editor_core::Tool::Razor;
                    if on_lane
                        && (razor
                            || act(MouseGesture::Click) == expression_editor_core::Action::SplitTake)
                        && let Some(on_hit) = on_hit.as_ref()
                        && let Some((v0, v1)) = view_span_secs(&editor.read())
                    {
                        let at = v0 + (lx / vp.w.max(1.0)) * (v1 - v0);
                        on_hit.call(HitGesture::Split { at });
                        e.prevent_default();
                        return;
                    }
                    // Two presses inside the window and the pick radius
                    // are a double click.
                    let now = std::time::Instant::now();
                    let double = last_press().is_some_and(|(t0, px, py)| {
                        now.duration_since(t0).as_millis() < 400
                            && (c.x - px).abs() <= SLIP_PICK_PX
                            && (c.y - py).abs() <= SLIP_PICK_PX
                    });
                    last_press.set(Some((now, c.x, c.y)));
                    let picked = views
                        .iter()
                        .filter(|l| l.detects && ly >= l.y && ly < l.y + l.h)
                        .flat_map(|l| l.notes.iter().map(move |n| (l, n)))
                        .filter(|(_, n)| (n.x - lx).abs() <= SLIP_PICK_PX)
                        .min_by(|(_, a), (_, b)| {
                            (a.x - lx).abs().total_cmp(&(b.x - lx).abs())
                        })
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
                                x0: c.x,
                                x: c.x,
                                // Which of the two move bindings the
                                // modifiers resolved to, rather than a
                                // hardcoded Shift.
                                shift: act(MouseGesture::Drag)
                                    == expression_editor_core::Action::MoveHitBothEnds,
                                hit_x: n.x,
                                lane_y: l.y,
                                lane_h: l.h,
                            }
                        });
                    if let Some(s) = picked {
                        let sel = SelectedHit {
                            lane_name: s.lane_name.clone(),
                            hit_secs: s.hit_secs,
                            prev_secs: s.prev_secs,
                            next_secs: s.next_secs,
                            lane_y: s.lane_y,
                            lane_h: s.lane_h,
                        };
                        // Double-click snaps the hit to its nearest
                        // division — the fastest way to fix one hit
                        // without opening the panel.
                        // r[impl drums.manual.nudge]
                        if double
                            && grid_secs > 0.0
                            && act(MouseGesture::DoubleClick)
                                == expression_editor_core::Action::SnapHitToGrid
                        {
                            let target = (s.hit_secs / grid_secs).round() * grid_secs;
                            let delta = target - s.hit_secs;
                            if delta.abs() > 1e-9 {
                                emit_move(&sel, delta, false);
                            }
                            let mut sel = sel;
                            sel.hit_secs = target;
                            selected.set(Some(sel));
                            return;
                        }
                        slipping.set(Some(s));
                        return;
                    }
                    // Alt+click on empty role-lane audio adds a hit
                    // where the click meant — the host refines it to
                    // the nearest attack. The hit list changes; the daw
                    // does not, until a drag or Apply.
                    // r[impl drums.manual.add-remove]
                    // r[impl drums.mouse.contexts]
                    if act(MouseGesture::Click) == expression_editor_core::Action::AddHit
                        && px_per_sec > 0.0
                    {
                        let lane = views
                            .iter()
                            .find(|l| l.detects && ly >= l.y && ly < l.y + l.h);
                        if let (Some(l), Some(h)) = (lane, &on_hit) {
                            h.call(HitGesture::Add {
                                lane: l.name.clone(),
                                at: view0 + lx / px_per_sec,
                            });
                            return;
                        }
                    }
                }
                let y = c.y - ruler_h + editor.read().stack_scroll;
                // Resolve against a snapshot: the read guard has to be
                // gone before the write below.
                let hit = {
                    let ed = editor.read();
                    let rows = ed
                        .tracks
                        .stack(ed.viewport.h as f32, ACTIVE_BOOST, ed.lane_floor().max(MIN_LANE));
                    // `row_at` resolves to a *lane*. Clicking targets a
                    // lane, never a track within one: with a vocal and
                    // its guide a few pixels apart, picking by proximity
                    // silently edits the reference you were tuning
                    // against. If the click lands on the lane you are
                    // already in, nothing changes — cycling within a
                    // lane is a key, not a click.
                    expression_editor_core::tracks::Workspace::row_at(&rows, y as f32)
                        .filter(|&lane| Some(lane) != ed.tracks.active_lane())
                        .and_then(|lane| {
                            ed.tracks
                                .lane_tracks(lane)
                                .into_iter()
                                .find(|&i| ed.tracks.track(i).is_some_and(|t| !t.hidden))
                        })
                };
                if let Some(track) = hit {
                    editor.write().switch_track(track);
                }
            },
        }

        // The moving parts, kept as elements on purpose.
        //
        // Everything static is paint, but these four change on clocks of
        // their own — a transport tick arrives many times a second, a
        // drag every pointer move — and folding them into the scene
        // would rebuild the whole picture for each one. As a handful of
        // overlay nodes they cost a style write and nothing else, which
        // is the same reason `StackPlayhead` was a leaf component
        // before any of this was painted. `pointer-events: none` so the
        // gestures still land on the surface underneath.
        svg {
            // Its own layer too, for the same reason: what moves here
            // must not cost what does not move below.
            style: "position: absolute; left: 0; top: 0; display: block; \
                    width: {stack_w:.0}px; height: {stack_h:.0}px; \
                    pointer-events: none; will-change: transform;",
            // The transport's playhead, over every lane. A leaf
            // component: position ticks arrive many times a second
            // while playing and must move one line, not re-render the
            // stack.
            if let Some(ph) = playhead_secs {
                StackPlayhead {
                    playhead: ph,
                    view0,
                    // A degenerate viewport must not change the shape of
                    // this subtree either; the line just does not move.
                    px_per_sec: px_per_sec.max(0.0),
                    height: vp.h,
                    ruler_h,
                }
            }

            // The selected hit — the unit of keyboard editing — marked
            // with a bracket at the lane's top edge so it reads against
            // the hit line without hiding it.
            // r[impl drums.manual.nudge]
            g {
                transform: "translate({canvas::GUTTER_W}, {ruler_h})",
                line {
                    x1: "{sel_x:.1}", x2: "{sel_x:.1}",
                    y1: "{sel_y:.1}", y2: "{sel_y + sel_h:.1}",
                    stroke: theme::ACCENT, stroke_width: 1,
                    opacity: if sel_on { "0.8" } else { "0" },
                }
                rect {
                    x: "{sel_x - 3.0:.1}", y: "{sel_y:.1}",
                    width: 6, height: 4,
                    fill: theme::ACCENT,
                    opacity: if sel_on { "1" } else { "0" },
                }
            }

            // The drag's ghost: the hit's origin stays dim while the
            // dragged line follows the pointer, so the gesture reads as
            // "this hit is moving there".
            // r[impl drums.manual.slip]
            // r[impl drums.manual.stretch]
            g {
                transform: "translate({canvas::GUTTER_W}, {ruler_h})",
                line {
                    x1: "{slip_x:.1}", x2: "{slip_x:.1}",
                    y1: "{slip_y:.1}", y2: "{slip_y + slip_h:.1}",
                    stroke: theme::TEXT_DIM, stroke_width: 1,
                    opacity: if slip_on { "0.5" } else { "0" },
                }
                line {
                    x1: "{slip_to:.1}", x2: "{slip_to:.1}",
                    y1: "{slip_y:.1}", y2: "{slip_y + slip_h:.1}",
                    stroke: theme::ACCENT, stroke_width: 2,
                    opacity: if slip_on { "1" } else { "0" },
                }
            }
            rect {
                x: "{marquee_x:.1}",
                y: "{ruler_h:.1}",
                width: "{marquee_w:.1}",
                height: "{vp.h:.1}",
                fill: "#60a5fa26", stroke: "#60a5fa",
                pointer_events: "none",
                opacity: if marquee_on { "1" } else { "0" },
            }
        }
        }
    }
}

/// The playhead line, isolated so a transport tick re-renders one
/// element. Rendered inside the stack's svg, in content coordinates.
#[component]
fn StackPlayhead(
    playhead: Signal<f64>,
    view0: f64,
    px_per_sec: f64,
    height: f64,
    /// Where the lanes start — the ruler grows with the number of
    /// chrome shelves, so this cannot be the old constant.
    ruler_h: f64,
) -> Element {
    let x = canvas::GUTTER_W + (playhead() - view0) * px_per_sec;
    // Off the left edge is invisible, not absent.
    //
    // This returned an empty element when the playhead scrolled out of
    // view, so the node count flipped between zero and one on an
    // ordinary pan — and a template whose shape depends on its state is
    // what leaves a dead id in blitz-dom's `paint_children` for the next
    // hit test to unwrap. Same note as `KeyPanel` in `crate::roll`, and
    // the same fix: one shape, faded.
    let visible = x >= canvas::GUTTER_W;
    let x = x.max(canvas::GUTTER_W);
    rsx! {
        line {
            x1: "{x:.1}", x2: "{x:.1}",
            y1: "{ruler_h:.1}",
            y2: "{ruler_h + height:.1}",
            stroke: "#f8fafc",
            stroke_width: 1,
            opacity: if visible { "0.7" } else { "0" },
        }
    }
}
