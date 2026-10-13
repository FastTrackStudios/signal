//! The Chart panel: the song's keyflow chart, engraved and painted with a
//! live playback cursor, pannable and zoomable — the native counterpart to
//! the web chart panes.
//!
//! Session's own `apps/desktop/src/session_chart_pane.rs` and, further
//! along the same design, `task`'s
//! `crates/player-ui/src/session_chart_pane.rs` (paginated A4 pages,
//! drag-to-pan, wheel-to-zoom, auto-follow to the playhead's page) render
//! fontless SVG through a browser engine — no wgpu, no canvas — because
//! that is what a wasm build can do. Blitz is not a browser, so this
//! paints straight into the widget's own scene instead, through
//! `keyflow::engraver::renderer::ChartView`: the SAME layout engine, the
//! same [`keyflow::engraver::layout::chart::cursor::ChartCursor`] playhead
//! math, and the same embedded fonts those SVG panes call — only the last
//! step, painting, differs from theirs. `ChartView` also paints the page
//! white, behind the notation, the way sheet music reads.
//!
//! **Interaction** matches the keyflow site (a browser: a trackpad's
//! two-finger scroll pans a page, an actual mouse wheel or ctrl+scroll
//! zooms it — Safari's and Chrome's own split): a trackpad scroll (winit
//! delivers it as `PixelDelta`, smooth and on both axes at once) pans;
//! ctrl+scroll, or a plain wheel on an actual mouse (`LineDelta`, discrete
//! single-axis steps — winit's own way of telling the two apart on
//! macOS), zooms. A middle-drag pans too, for a mouse that has one.
//!
//! **The count-in offset.** The DAW's transport and the chart's own layout
//! do not share a zero: the chart's timeline starts at its first REAL
//! measure (the count-in is a header snippet at negative chart time), while
//! the transport's seconds run from the project's own start, before any
//! count-in. This panel reads the SONGSTART marker once at construction and
//! subtracts it from every transport read, so a playhead over the count-in
//! lands on the count-in header rather than nowhere — the same subtraction
//! task's pane docs as its own playhead model.
//!
//! **v1**, scoped down from task's pane: one continuous column rather than
//! paginated A4 pages, and no auto-follow yet (manual pan takes priority —
//! the two would fight without a "the user just touched it" timeout, which
//! is the thing to add before bringing auto-follow back). Bringing
//! pagination here is the next step; port `DocRender`'s page-box math from
//! task's pane rather than rewriting it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use anyrender::{RenderContext, Scene};
use blitz_dom::node::{ComputedStyles, Widget};
use dioxus::prelude::*;

use keyflow::engraver::renderer::{ChartView, points_to_px};

use crate::studio::StudioSession;

/// How far one wheel notch zooms, as a factor.
const ZOOM_STEP: f64 = 1.12;
const ZOOM_MIN: f64 = 0.2;
const ZOOM_MAX: f64 = 6.0;

/// What the widget reads (and partly writes) every paint, shared with the
/// panel's own window-level input handling — the same shape
/// `crate::widget::Shared` gives the arrangement.
struct Live {
    /// Pan, in chart points — zoom-independent, see [`ChartView::paint`].
    scroll_pt: (f64, f64),
    zoom: f64,
    /// The chart's own content size in points, and the device-pixels-per-
    /// point factor (at the CURRENT `zoom`) — both written by the widget
    /// after each paint, so the input handling can turn a screen-pixel
    /// drag or a wheel notch into points without knowing keyflow's DPI
    /// constant itself. One frame behind a size or a zoom change, which
    /// self-corrects the next frame — the same trade-off `ChartView`'s own
    /// `cursor_y_pt` makes.
    content_pt: (f64, f64),
    px_per_pt: f64,
    /// The device scale the last paint used, and the laid-out measures —
    /// what a collaborator's pointer is anchored to (see [`ChartAnchor`]).
    scale: f64,
    boxes: Vec<keyflow::engraver::renderer::view::MeasureBox>,
    /// The view was moved by hand (a pan or a zoom): a paged panel stops
    /// fitting the page and following the song until a double-click hands
    /// it back.
    manual: bool,
    /// When two fingers last pinched: their lift arrives as two clicks,
    /// which is a double-click — the fit's way back — and is not one.
    pinched_at: Option<web_time::Instant>,
    /// A page turned to by hand ([`turn`]): the panel stays on it, fitted,
    /// until the song is followed again.
    held: bool,
}

type Shared = Rc<RefCell<Live>>;

/// How much of the next page a fitted page shows beside it: about its
/// first measure.
pub const NEXT_PAGE_PEEK: f64 = 0.3;

/// The shape a fitted chart takes, width over height: a Letter page, the
/// gap between pages and [`NEXT_PAGE_PEEK`] of the next across, the
/// page's height down — what a host sizes the chart's pane to so the
/// chart fills it with nothing left over.
pub const FITTED_WIDTH_OVER_HEIGHT: f64 = (612.0 * (1.0 + NEXT_PAGE_PEEK) + 40.0) / 792.0;

/// The widget: owns the layout cache; the pan and zoom live in [`Live`].
struct ChartWidget {
    chart: std::sync::Arc<keyflow::Chart>,
    /// Stable for the session's lifetime — [`Rc::as_ptr`], not a content
    /// hash: the chart this widget holds never changes after
    /// [`StudioSession::open`], so identity is all [`ChartView`] needs to
    /// know when to keep its cached layout.
    key: u64,
    view: ChartView,
    live: Shared,
    /// The SONGSTART marker's position, in project seconds — subtracted
    /// from the transport's read to get chart time. `None` with no such
    /// marker (an unprepared session, or one opened without a chart).
    songstart: Option<f64>,
    /// One page at a time, filled to the panel and following the song
    /// ([`Live::paged`]) — the Overview's chart, and any pane too small
    /// to be read by scrolling.
    paged: bool,
    /// The page a paged panel is showing (1-indexed), kept between frames
    /// so a playhead outside the chart leaves the page where it was.
    page: u32,
    /// The last live chart taken ([`publish_live`]).
    live_seen: u64,
    /// The chart the measure boxes in [`Live`] were taken from.
    boxes_key: u64,
    /// Fingers on the page: one drags it, two pinch it (`touch::Fingers`).
    fingers: crate::touch::Fingers,
    /// Moved by a finger since the last paint: paint again.
    moved: std::cell::Cell<bool>,
    /// The last page turn taken ([`turn`]).
    turn_seen: u64,
    /// The last fit asked for that it has taken ([`set_fit`]).
    fit_seen: u64,
}

/// A collaborator's pointer over the chart, anchored to the music: the
/// measure under it, how far through that measure's box (0 at the left
/// barline, 1 at the right) and how high against its staff (0 the top
/// line, 1 the bottom) — so it lands on measure 40, beat 4 in every
/// window, whatever each has zoomed, panned or paged to.
struct ChartAnchor {
    live: Shared,
}

impl ChartAnchor {
    /// Logical pixels per chart point, and the pan.
    fn view(&self) -> Option<(f64, (f64, f64))> {
        let live = self.live.borrow();
        let k = live.px_per_pt / live.scale.max(f64::EPSILON);
        (k > 0.0).then_some((k, live.scroll_pt))
    }
}

impl crate::ghosts::Anchor for ChartAnchor {
    fn anchor(&self, x: f64, y: f64, _size: (f64, f64)) -> Option<(String, f64, f64)> {
        let (k, scroll) = self.view()?;
        let (px, py) = (scroll.0 + x / k, scroll.1 + y / k);
        let live = self.live.borrow();
        // The nearest measure — to its staff's box, from anywhere on the
        // chart (a margin, the gap between systems): a pointer in the chart
        // is always somewhere in the music, so every other view puts it in
        // the same place, whatever its zoom or page. Off the box, `u`/`v`
        // run past 0..1 and keep the offset.
        let distance = |b: &keyflow::engraver::renderer::view::MeasureBox| {
            let dx = (b.x0 - px).max(px - b.x1).max(0.0);
            let dy = (b.staff_y - py)
                .max(py - (b.staff_y + b.staff_height))
                .max(0.0);
            dx.hypot(dy)
        };
        let hit = live
            .boxes
            .iter()
            .min_by(|a, b| distance(a).total_cmp(&distance(b)))?;
        Some((
            hit.measure.to_string(),
            (px - hit.x0) / (hit.x1 - hit.x0).max(f64::EPSILON),
            (py - hit.staff_y) / hit.staff_height.max(f64::EPSILON),
        ))
    }

    fn place(&self, key: &str, u: f64, v: f64, _size: (f64, f64)) -> Option<(f64, f64)> {
        let measure: usize = key.parse().ok()?;
        let (k, scroll) = self.view()?;
        let live = self.live.borrow();
        let b = live.boxes.iter().find(|b| b.measure == measure)?;
        let (px, py) = (
            u.mul_add(b.x1 - b.x0, b.x0),
            v.mul_add(b.staff_height, b.staff_y),
        );
        Some(((px - scroll.0) * k, (py - scroll.1) * k))
    }
}

impl ChartWidget {
    /// The picture: what Blitz's `Widget::paint` returns, and what the web
    /// host draws into its canvas.
    pub fn paint_scene(&mut self, width: u32, height: u32, scale: f64) -> Scene {
        // An edited chart laid over the song: draw that one from now on.
        if let Some((chart, songstart, number)) = live_since(self.live_seen) {
            self.key = std::sync::Arc::as_ptr(&chart) as usize as u64;
            self.chart = chart;
            if songstart.is_some() {
                self.songstart = songstart;
            }
            self.live_seen = number;
        }
        // A fit picked: fitted again, wherever it was moved by hand.
        let asked = FIT_ASKED.load(std::sync::atomic::Ordering::Relaxed);
        if asked != self.fit_seen {
            self.fit_seen = asked;
            self.live.borrow_mut().manual = false;
        }
        if let Some((turned, number)) = turn_since(self.turn_seen) {
            self.turn_seen = number;
            let last = u32::try_from(self.view.pages()).unwrap_or(u32::MAX).max(1);
            let mut live = self.live.borrow_mut();
            live.manual = false;
            match turned {
                Turn::Back => {
                    self.page = self.page.saturating_sub(1).max(1);
                    live.held = true;
                }
                Turn::On => {
                    self.page = (self.page + 1).min(last);
                    live.held = true;
                }
                Turn::Follow => live.held = false,
            }
        }
        let (w, h) = (f64::from(width), f64::from(height));
        let chart_secs = self
            .songstart
            .zip(crate::engine::Transport::shared())
            .map(|(songstart, t)| t.read().0 - songstart);
        // Laid out on its first paint: before that there is no page to fit
        // to, and the fit waits for the frame after — asked for below.
        let laid_out = self.view.pages() > 0;
        if self.paged {
            self.fit_page(w, h, scale, chart_secs);
        }
        let (zoom, scroll_pt) = {
            let live = self.live.borrow();
            (live.zoom, live.scroll_pt)
        };

        let mut out = Scene::new();
        let content_pt = self.view.paint(
            &mut out,
            &self.chart,
            self.key,
            w,
            scale,
            zoom,
            scroll_pt,
            chart_secs,
        );
        if self.paged && !laid_out && self.view.pages() > 0 {
            // Laid out just now: one more frame, fitted. (The window draws
            // only when something asks — nothing else would.)
            self.moved.set(true);
        }
        let mut live = self.live.borrow_mut();
        live.content_pt = content_pt;
        live.px_per_pt = points_to_px(scale) * zoom;
        live.scale = scale;
        // The measures move only with a new layout: refreshed when the
        // chart does (or before there are any).
        if live.boxes.is_empty() || self.boxes_key != self.key {
            live.boxes = self.view.measure_boxes();
            self.boxes_key = self.key;
        }
        out
    }

    /// Put the page the playhead is on on the panel, its top-left on the
    /// panel's, with the start of the next page beside it.
    ///
    /// The zoom fits the page and [`NEXT_PAGE_PEEK`] of the next one
    /// across the width — or the page alone down the height, whichever
    /// runs out first — and the scroll is the page's own corner. A pane
    /// wider than that shows more of the next page.
    ///
    /// Before the downbeat (the count-in, or stopped at zero) that is the
    /// first page; past the chart's end, the page it ended on stays up.
    fn fit_page(&mut self, w: f64, h: f64, scale: f64, chart_secs: Option<f64>) {
        // Moved by hand: the view is where it was put.
        let held = {
            let live = self.live.borrow();
            if live.manual {
                return;
            }
            live.held
        };
        // Turned to by hand: that page, from its top, whatever is playing.
        let chart_secs = chart_secs.filter(|_| !held);
        match chart_secs {
            Some(secs) if secs < 0.0 => self.page = 1,
            Some(secs) => {
                if let Some(number) = self.view.page_number_at_time(secs) {
                    self.page = number;
                }
            }
            None => {}
        }
        let Some((x, y, page_w, page_h)) = self.view.page(self.page) else {
            return;
        };
        let per_pt = points_to_px(scale);
        if page_w <= 0.0 || page_h <= 0.0 || per_pt <= 0.0 {
            return;
        }
        // A panel taller than wide (a phone held upright), or too short for
        // a page to be read at its full height (one on its side, a tablet's
        // Performance pane), shows the page across its width — and, when
        // that is taller than the panel, follows the song down it. A whole
        // page in under seven tenths of the width is a page too small to
        // read from a music stand. (The Overview's pane is shaped for the
        // page and its peek — `FITTED_WIDTH_OVER_HEIGHT`, the page about
        // 0.73 of it — and stays across.)
        let fit_width = match fit() {
            Some(Fit::Width) => true,
            Some(Fit::Page) => false,
            None => w < h || page_w / page_h * h < w * 0.7,
        };
        SHOWN.store(
            if fit_width { Fit::Width } else { Fit::Page } as u8,
            std::sync::atomic::Ordering::Relaxed,
        );
        if fit_width {
            let zoom = w / (page_w * per_pt);
            let visible = h / (zoom * per_pt);
            let top = chart_secs
                .and_then(|secs| self.view.cursor_at_time(secs))
                .filter(|(page, ..)| *page == self.page)
                .map_or(y, |(_, cursor_y, cursor_h)| {
                    // The line being played a third of the way down, the
                    // rest of the page's next lines under it.
                    let wanted = cursor_y + cursor_h / 2.0 - visible / 3.0;
                    wanted.clamp(y, (y + page_h - visible).max(y))
                });
            let mut live = self.live.borrow_mut();
            live.zoom = zoom;
            live.scroll_pt = (x, top);
            return;
        }
        // Across: the page, and the first measure or so of the next one
        // beside it — what is coming is on screen before the page turns.
        let across = self
            .view
            .page(self.page + 1)
            .map_or(page_w, |(next_x, ..)| {
                (next_x - x) + page_w * NEXT_PAGE_PEEK
            });
        let zoom = (w / (across * per_pt)).min(h / (page_h * per_pt));
        let mut live = self.live.borrow_mut();
        live.zoom = zoom;
        live.scroll_pt = (x, y);
    }
}

impl ChartWidget {
    /// A finger on the page: dragged, the page goes with it; two, and it
    /// zooms about the point between them — the arrangement's gesture
    /// (`touch::pinch_step`), both axes alike, since a page has one zoom.
    /// Either takes the view off the fitted page, as a wheel does.
    fn touched(&mut self, event: &blitz_traits::events::UiEvent) -> bool {
        use blitz_traits::events::UiEvent;
        let at = |e: &blitz_traits::events::BlitzPointerEvent| {
            (f64::from(e.coords.client_x), f64::from(e.coords.client_y))
        };
        match event {
            UiEvent::PointerDown(e) if e.is_finger() => {
                self.fingers.down(e.id, at(e));
                false
            }
            UiEvent::PointerMove(e) if e.is_finger() => {
                let Some(step) = self.fingers.moved(e.id, at(e)) else {
                    return false;
                };
                let scale = self.live.borrow().scale.max(f64::EPSILON);
                let by = match (step.sx == 1.0, step.sy == 1.0) {
                    (true, true) => 1.0,
                    (false, true) => step.sx,
                    (true, false) => step.sy,
                    (false, false) => (step.sx * step.sy).sqrt(),
                };
                if (by - 1.0).abs() > f64::EPSILON {
                    let mut live = self.live.borrow_mut();
                    live.manual = true;
                    live.pinched_at = Some(web_time::Instant::now());
                    let was = live.px_per_pt.max(f64::EPSILON);
                    let zoom = (live.zoom * by).clamp(ZOOM_MIN, ZOOM_MAX);
                    let now = was * zoom / live.zoom.max(f64::EPSILON);
                    // The point under the fingers stays under them.
                    let (mx, my) = (step.mid.0 * scale, step.mid.1 * scale);
                    live.scroll_pt.0 += mx / was - mx / now;
                    live.scroll_pt.1 += my / was - my / now;
                    live.zoom = zoom;
                    live.px_per_pt = now;
                }
                pan_by(&self.live, step.dx * scale, step.dy * scale);
                true
            }
            UiEvent::PointerUp(e) | UiEvent::PointerCancel(e) if e.is_finger() => {
                self.fingers.up(e.id);
                false
            }
            _ => false,
        }
    }
}

impl Widget for ChartWidget {
    fn handle_event(&mut self, event: &blitz_traits::events::UiEvent) {
        if self.touched(event) {
            self.moved.set(true);
        }
    }

    fn needs_redraw(&self) -> bool {
        // The follow cursor while the transport moves; a chart that came in
        // live.
        self.moved.get()
            || turn_since(self.turn_seen).is_some()
            || FIT_ASKED.load(std::sync::atomic::Ordering::Relaxed) != self.fit_seen
            || live_since(self.live_seen).is_some()
            || crate::engine::moving()
    }

    fn paint(
        &mut self,
        _render_ctx: &mut dyn RenderContext,
        _styles: &ComputedStyles,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Scene {
        self.moved.set(false);
        self.paint_scene(width, height, scale)
    }
}

/// The panel. Reads [`StudioSession::chart`] from context; when the session
/// has none it says so rather than showing an empty page.
/// The chart widget for the session's chart, and the view state its input
/// drives. `None` without a chart (or with the fonts failing to build).
fn build(session: &StudioSession, paged: bool) -> Option<(ChartWidget, Shared)> {
    let chart = session.chart.clone()?;
    let view = match ChartView::new() {
        Ok(view) => view,
        Err(e) => {
            tracing::error!(error = %e, "chart panel: font bundle failed");
            return None;
        }
    };
    let key = std::sync::Arc::as_ptr(&chart) as usize as u64;
    let live: Shared = Rc::new(RefCell::new(Live {
        scroll_pt: (0.0, 0.0),
        zoom: 1.0,
        content_pt: (1.0, 1.0),
        px_per_pt: 1.0,
        manual: false,
        pinched_at: None,
        held: false,
        scale: 1.0,
        boxes: Vec::new(),
    }));
    // The Overview's chart (the paged one) is the `chart` pane others'
    // pointers are placed in.
    if paged {
        crate::ghosts::register_anchor(
            "chart",
            Rc::new(ChartAnchor {
                live: Rc::clone(&live),
            }),
        );
    }
    let widget = ChartWidget {
        chart,
        key,
        view,
        live: Rc::clone(&live),
        songstart: songstart_secs(&session.project),
        paged,
        page: 1,
        live_seen: 0,
        boxes_key: 0,
        fingers: crate::touch::Fingers::default(),
        moved: std::cell::Cell::new(false),
        // Turns asked for before this chart was up are not for it.
        turn_seen: turn_since(0).map_or(0, |(_, number)| number),
        fit_seen: FIT_ASKED.load(std::sync::atomic::Ordering::Relaxed),
    };
    Some((widget, live))
}

/// Move the view by a screen-pixel delta, in POINTS (so it stays
/// physically anchored to the content as the zoom changes). Takes the view
/// off the fitted page (see [`Live::manual`]).
fn pan_by(live: &Shared, dx_px: f64, dy_px: f64) {
    let mut live = live.borrow_mut();
    live.manual = true;
    let k = live.px_per_pt.max(f64::EPSILON);
    let (cw, ch) = live.content_pt;
    // A page and a half of slack past either edge — panning clean off the
    // content is normal while looking for something, and the clamp is only
    // there to stop a drag from running away into the thousands.
    live.scroll_pt.0 = (live.scroll_pt.0 - dx_px / k).clamp(-cw * 0.5, cw * 1.5);
    live.scroll_pt.1 = (live.scroll_pt.1 - dy_px / k).clamp(-ch * 0.5, ch * 1.5);
}

/// Zoom by `notches` steps (positive in).
fn zoom_by(live: &Shared, notches: f64) {
    if notches.abs() < f64::EPSILON {
        return;
    }
    let mut live = live.borrow_mut();
    live.manual = true;
    live.zoom = (live.zoom * ZOOM_STEP.powf(notches)).clamp(ZOOM_MIN, ZOOM_MAX);
}

/// No chart: a quiet line where it would be.
#[component]
fn NoChart() -> Element {
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; width:100%; height:100%; \
                    display:flex; align-items:center; justify-content:center; \
                    color:#8b9099; font-size:12px; background:#1a1b1e;",
            "No chart for this session."
        }
    }
}

/// The chart panel in a browser: the widget in a canvas; a wheel or a
/// trackpad pans, ctrl+wheel (and a pinch, which browsers deliver as one)
/// zooms, a middle-drag pans.
#[cfg(feature = "web")]
#[component]
pub fn WebChart(
    /// One page at a time, filled to the panel and following the song.
    #[props(default)]
    paged: bool,
) -> Element {
    use crate::panel::{Button, PanelEvent};
    let session: StudioSession = use_context();
    let built = use_hook(|| {
        build(&session, paged).map(|(widget, live)| {
            (
                crate::web_host::HostedRef(Rc::new(RefCell::new(widget))),
                live,
            )
        })
    });
    let Some((widget, live)) = built else {
        return rsx! { NoChart {} };
    };
    let input = use_hook(|| Rc::new(Cell::new((false, None::<(f64, f64)>))));
    let refit = Rc::clone(&live);
    let on_input = move |event: PanelEvent| {
        let (ctrl, drag) = input.get();
        match event {
            PanelEvent::Modifiers(mods) => input.set((mods.ctrl, drag)),
            PanelEvent::Button {
                button: Button::Middle,
                pressed,
            } => {
                input.set((ctrl, pressed.then_some((f64::NAN, f64::NAN))));
            }
            PanelEvent::Pointer { x, y } => {
                if let Some(from) = drag {
                    if !from.0.is_nan() {
                        pan_by(&live, x - from.0, y - from.1);
                    }
                    input.set((ctrl, Some((x, y))));
                }
            }
            PanelEvent::Wheel { dx, dy } => {
                if ctrl {
                    zoom_by(&live, dy / 40.0);
                } else {
                    pan_by(&live, dx, dy);
                }
            }
            _ => {}
        }
    };
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; width:100%; height:100%; \
                    overflow:hidden; background:#1a1b1e;",
            // Back to the whole page, following the song.
            ondoubleclick: move |_| {
                let mut live = refit.borrow_mut();
                if live.pinched_at.is_none_or(|at| at.elapsed().as_millis() > 600) {
                    live.manual = false;
                    live.held = false;
                }
            },
            crate::web_host::WidgetCanvas { widget, panel: on_input }
        }
    }
}

#[cfg(feature = "web")]
impl crate::web_host::Hosted for ChartWidget {
    fn paint(&mut self, width: u32, height: u32, scale: f64) -> Scene {
        self.paint_scene(width, height, scale)
    }
    fn event(&mut self, event: &blitz_traits::events::UiEvent) {
        self.touched(event);
    }
}

#[cfg(feature = "native")]
#[component]
pub fn Chart(
    /// One page at a time, filled to the panel and following the song.
    #[props(default)]
    paged: bool,
) -> Element {
    let session: StudioSession = use_context();
    let built = use_hook(|| {
        build(&session, paged)
            .map(|(widget, live)| (dioxus_native_dom::CustomWidgetAttr::new(widget), live))
    });
    let Some((widget, live)) = built else {
        return rsx! { NoChart {} };
    };

    // This panel's rectangle in the window — needed to turn a window-space
    // drag or wheel into a panel-local one, and to filter a middle-drag to
    // presses that started inside it. Blitz does not route the wheel
    // through the DOM at all, so this happens at the winit level, the same
    // idiom `crate::studio::Arrangement` uses for its own wheel and drag.
    let mut rect = use_signal(|| (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64));
    let mounted = use_hook(|| Rc::new(RefCell::new(None::<Rc<MountedData>>)));
    let dragging = use_hook(|| Rc::new(Cell::new(Option::<(f64, f64)>::None)));
    // The pointer's last known window position, tracked on every move —
    // not only during a drag — so a bare wheel notch can be told whether
    // it landed over this panel at all.
    let pointer = use_hook(|| Rc::new(Cell::new((0.0_f64, 0.0_f64))));
    let ctrl_held = use_hook(|| Rc::new(Cell::new(false)));
    let measured = use_hook(|| Rc::new(Cell::new(None::<web_time::Instant>)));

    let measuring = Rc::clone(&mounted);
    let drag_state = Rc::clone(&dragging);
    let tracking = Rc::clone(&pointer);
    let ctrl = Rc::clone(&ctrl_held);
    let driving = Rc::clone(&live);
    dioxus_native::use_window_event(move |event, _| {
        let r = *rect.peek();
        let inside = |p: (f64, f64)| p.0 >= r.0 && p.0 < r.0 + r.2 && p.1 >= r.1 && p.1 < r.1 + r.3;
        // Move the view by a screen-pixel delta, in POINTS (so it stays
        // physically anchored to the content as `zoom` changes) — shared by
        // a middle-drag and a trackpad's two-finger scroll.
        let pan_by = |dx_px: f64, dy_px: f64| pan_by(&driving, dx_px, dy_px);
        let zoom_by = |dy: f64| zoom_by(&driving, dy);
        match event {
            winit::event::WindowEvent::ModifiersChanged(state) => {
                ctrl.set(state.state().control_key());
            }
            winit::event::WindowEvent::PointerButton {
                state,
                button,
                position,
                ..
            } => {
                let which = match button {
                    winit::event::ButtonSource::Mouse(button) => *button,
                    _ => winit::event::MouseButton::Left,
                };
                let at = (position.x, position.y);
                if which == winit::event::MouseButton::Middle {
                    drag_state.set(state.is_pressed().then_some(at).filter(|&p| inside(p)));
                }
            }
            winit::event::WindowEvent::PointerMoved { position, .. } => {
                let at = (position.x, position.y);
                tracking.set(at);
                let Some(from) = drag_state.get() else { return };
                drag_state.set(Some(at));
                pan_by(at.0 - from.0, at.1 - from.1);
            }
            winit::event::WindowEvent::MouseWheel { delta, .. } => {
                if !inside(tracking.get()) {
                    return;
                }
                // A trackpad's two-finger scroll arrives as `PixelDelta`
                // (smooth, both axes) on macOS; an actual mouse wheel
                // arrives as `LineDelta` (discrete, one axis) — winit's own
                // way of telling the two apart, and what this uses to pick
                // pan (a trackpad scroll, the natural gesture) from zoom (a
                // real wheel, or either device with ctrl held — Safari's
                // and Chrome's own split).
                match delta {
                    // A real wheel always zooms — it has no second axis to
                    // pan with.
                    winit::event::MouseScrollDelta::LineDelta(_, y) => zoom_by(f64::from(*y)),
                    // A trackpad scroll pans, unless ctrl is held (Safari's
                    // and Chrome's own zoom modifier).
                    winit::event::MouseScrollDelta::PixelDelta(at) if ctrl.get() => {
                        zoom_by(at.y / 40.0);
                    }
                    winit::event::MouseScrollDelta::PixelDelta(at) => pan_by(at.x, at.y),
                }
            }
            winit::event::WindowEvent::RedrawRequested => {
                let stale = measured
                    .get()
                    .is_none_or(|at| at.elapsed() > std::time::Duration::from_millis(200));
                if stale && let Some(node) = measuring.borrow().clone() {
                    measured.set(Some(web_time::Instant::now()));
                    spawn(async move {
                        if let Ok(got) = node.get_client_rect().await {
                            let next =
                                (got.origin.x, got.origin.y, got.size.width, got.size.height);
                            if next != *rect.peek() {
                                rect.set(next);
                            }
                        }
                    });
                }
            }
            _ => {}
        }
    });

    rsx! {
        div {
            style: "position:absolute; top:0; left:0; width:100%; height:100%; \
                    overflow:hidden; background:#1a1b1e;",
            onmounted: move |event| {
                *mounted.borrow_mut() = Some(event.data());
            },
            // Back to the whole page, following the song.
            ondoubleclick: {
                let live = Rc::clone(&live);
                move |_| {
                    let mut live = live.borrow_mut();
                    if live.pinched_at.is_none_or(|at| at.elapsed().as_millis() > 600) {
                        live.manual = false;
                        live.held = false;
                    }
                }
            },
            object {
                style: "position:absolute; top:0; left:0; width:100%; height:100%;",
                data: widget.clone(),
            }
        }
    }
}

/// How a chart's page fits its panel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Fit {
    /// The whole page down the panel's height — zoomed out as far as that
    /// takes, the next page beside it where there is room.
    Page = 1,
    /// The page across the panel's width, zoomed in, following the song
    /// down it.
    Width = 2,
}

impl Fit {
    const fn from_u8(n: u8) -> Option<Self> {
        match n {
            1 => Some(Self::Page),
            2 => Some(Self::Width),
            _ => None,
        }
    }

    /// Its name, as the bottom bar says it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Page => "Full Page",
            Self::Width => "Full Width",
        }
    }
}

/// The fit picked (0: none — the panel's shape decides, a panel taller
/// than wide or too short for a page reading across its width).
static FIT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
/// Numbered, so each chart re-fits once for each pick.
static FIT_ASKED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// The fit the chart last drew with, picked or not.
static SHOWN: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(Fit::Page as u8);

/// The fit picked, if one was (kept between launches in the app).
#[must_use]
pub fn fit() -> Option<Fit> {
    let picked = FIT.load(std::sync::atomic::Ordering::Relaxed);
    if picked != 0 {
        return Fit::from_u8(picked);
    }
    let kept = kept_fit();
    if let Some(kept) = kept {
        FIT.store(kept as u8, std::sync::atomic::Ordering::Relaxed);
    }
    kept
}

/// The fit the chart shows now: the one picked, else the one its shape
/// chose.
#[must_use]
pub fn shown_fit() -> Fit {
    fit().unwrap_or_else(|| {
        Fit::from_u8(SHOWN.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(Fit::Page)
    })
}

/// Pick how the chart's page fits: every chart on screen re-fits to it.
pub fn set_fit(fit: Fit) {
    FIT.store(fit as u8, std::sync::atomic::Ordering::Relaxed);
    FIT_ASKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    keep_fit(fit);
}

/// Where the app keeps the fit picked.
#[cfg(feature = "native")]
fn fit_file() -> Option<std::path::PathBuf> {
    Some(dirs::data_dir()?.join("Session").join("chart-fit"))
}

#[cfg(feature = "native")]
fn kept_fit() -> Option<Fit> {
    let text = std::fs::read_to_string(fit_file()?).ok()?;
    Fit::from_u8(text.trim().parse().ok()?)
}

#[cfg(not(feature = "native"))]
const fn kept_fit() -> Option<Fit> {
    None
}

#[cfg(feature = "native")]
fn keep_fit(fit: Fit) {
    let Some(file) = fit_file() else { return };
    let written = file
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&file, (fit as u8).to_string()));
    if let Err(e) = written {
        tracing::warn!(error = %e, "chart: the fit picked could not be kept");
    }
}

#[cfg(not(feature = "native"))]
const fn keep_fit(_: Fit) {}

/// A turn of the chart's page, asked for from outside it (the bottom
/// bar's paging controls).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Turn {
    /// The page before, held there.
    Back,
    /// The page after, held there.
    On,
    /// Back to the page being played, following the song.
    Follow,
}

/// The last turn asked for, numbered, so each chart takes each once.
static TURN: std::sync::Mutex<(u64, Turn)> = std::sync::Mutex::new((0, Turn::Follow));

/// Turn the chart's page: every chart on screen takes it on its next frame.
pub fn turn(turned: Turn) {
    if let Ok(mut slot) = TURN.lock() {
        *slot = (slot.0 + 1, turned);
    }
}

/// The turn asked for since `seen`, and its number.
fn turn_since(seen: u64) -> Option<(Turn, u64)> {
    let slot = *TURN.lock().ok()?;
    (slot.0 > seen).then_some((slot.1, slot.0))
}

/// A chart laid over the song since it opened — by Organize mode's editor
/// — with the SONGSTART it moved to, for the song it belongs to. Numbered,
/// so a panel knows whether it has the latest.
struct LiveChart {
    project: String,
    chart: std::sync::Arc<keyflow::Chart>,
    songstart: Option<f64>,
    number: u64,
}

static LIVE: std::sync::Mutex<Option<LiveChart>> = std::sync::Mutex::new(None);

/// Publish an edited chart for `project`: its chart panels show it from
/// their next frame.
pub fn publish_live(project: &str, chart: std::sync::Arc<keyflow::Chart>, songstart: Option<f64>) {
    if let Ok(mut slot) = LIVE.lock() {
        let number = slot.as_ref().map_or(1, |live| live.number + 1);
        *slot = Some(LiveChart {
            project: project.to_owned(),
            chart,
            songstart,
            number,
        });
    }
}

/// The live chart for the song on screen, if newer than `seen`.
fn live_since(seen: u64) -> Option<(std::sync::Arc<keyflow::Chart>, Option<f64>, u64)> {
    #[cfg(feature = "native")]
    let current = crate::open::current_song();
    #[cfg(not(feature = "native"))]
    let current: Option<String> = None;
    let slot = LIVE.lock().ok()?;
    let live = slot.as_ref()?;
    (live.number > seen && current.as_deref() == Some(live.project.as_str())).then(|| {
        (
            std::sync::Arc::clone(&live.chart),
            live.songstart,
            live.number,
        )
    })
}

/// Where the MARKS lane's SONGSTART marker sits, in project seconds — the
/// chart's own time zero. `None` when the session has no such marker.
fn songstart_secs(project: &daw_ui::studio::project::Project) -> Option<f64> {
    project
        .markers
        .iter()
        .find(|m| m.name == "SONGSTART")
        .map(|m| m.at)
}

#[cfg(test)]
mod tests {
    use keyflow::engraver::renderer::ChartView;

    /// A chart long enough for several pages: the page the playhead is on
    /// moves with it. The pages sit side by side — every one at the same
    /// `y` — which is why the page is found by the cursor's own number;
    /// found by position, it was always the last.
    #[test]
    fn the_page_follows_the_playhead_across_pages() {
        let chart = keyflow::parse("Song - Artist\n120bpm 4/4 #C\n\nvs 160\nC G Am F x40\n")
            .expect("parses");
        let mut view = ChartView::new().expect("the fonts load");
        let mut scene = anyrender::Scene::new();
        view.paint(&mut scene, &chart, 1, 800.0, 1.0, 1.0, (0.0, 0.0), None);
        assert!(
            view.pages() > 1,
            "{} page(s): not long enough",
            view.pages()
        );

        assert_eq!(view.page_number_at_time(0.5), Some(1));
        let last = view
            .page_number_at_time(310.0)
            .expect("310 s is inside 160 bars at 120");
        assert!(last > 1, "the end of the chart is still on page {last}");

        // The cursor is on the page it says, inside it, lower down the page
        // later in the song.
        let (page, top, height) = view.cursor_at_time(0.5).expect("a cursor at 0.5 s");
        assert_eq!(page, 1);
        assert!(height > 0.0);
        let (_, page_y, _, page_h) = view.page(1).expect("page one");
        assert!(top >= page_y && top + height <= page_y + page_h);
        let (_, lower, _) = view.cursor_at_time(20.0).expect("a cursor at 20 s");
        assert!(lower > top, "20 s is further down page one than 0.5 s");

        let (first_x, first_y, ..) = view.page(1).expect("page one");
        let (last_x, last_y, ..) = view.page(last).expect("the last page");
        assert!(last_x > first_x, "pages sit side by side");
        assert!((last_y - first_y).abs() < f64::EPSILON);
    }
}
