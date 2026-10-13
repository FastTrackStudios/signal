//! The arrangement as ONE node: ruler, track panel and lanes, painted
//! rather than built.
//!
//! # Why this one surface is not a DOM
//!
//! Everything else in the window is better as components — a button
//! wants `:hover`, focus, a name a screen reader can say, and a layout
//! somebody can change without recompiling. The arrangement wants none
//! of that and pays dearly for all of it.
//!
//! Measured on the golden session at 5120x1440, the same picture drawn
//! both ways and gated pixel-for-pixel against each other (by a test
//! that went with the component lanes once they lost):
//!
//! | gesture | painted | as a component tree |
//! |---|---|---|
//! | scroll | 1.06 ms | 15.5 ms |
//! | zoom | 0.71 ms | 21.8 ms |
//! | the whole session on screen | 3.77 ms | — |
//!
//! Fifteen to thirty times, and it is not the GPU: that is 0.3–1.0 ms
//! either way. It is ten thousand nodes going through Stylo's cascade,
//! Taffy's layout, damage propagation and a per-node paint walk, every
//! frame, to produce a picture that a recorded command list replays in
//! one. No amount of tuning closes a gap of that shape — a month of it
//! took the component tree from 124 ms to 15, and 15 is still not 4.17.
//!
//! So the arrangement becomes a [`Widget`]: one element in the DOM, laid
//! out and positioned by CSS like any other, whose contents are drawn by
//! the renderer this window already had. The toolbars, the transport and
//! the rails stay components, because for them the DOM is the point.
//!
//! # What this costs, honestly
//!
//! Hit testing, keyboard focus and accessibility inside this rectangle
//! are ours to write. The first two we already wrote — the wheel is
//! handled at the winit level because Blitz dispatches no wheel event,
//! and `arrange_edit` has done item hit testing from the start. The
//! third is the real debt: a screen reader sees one element here. The
//! answer is that every piece of session state is reachable through the
//! CLI and the RPC surface, so the window is not the only way in — but
//! that is an argument, not an implementation, and it should be written
//! down as one.
//!
//! # Why it still runs in a browser
//!
//! [`Widget::paint`] returns an `anyrender::Scene` — a list of
//! commands, not a wgpu call. Whatever can replay that list can draw
//! this: the CPU backend, the WebGL-class one, and a canvas in a browser
//! tab. The escape hatch that takes a `Device` and `Queue` in
//! `can_create_surfaces` is the one that would tie us to native, and
//! this does not use it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use anyrender::{RenderContext, Scene};
use blitz_dom::node::{ComputedStyles, Widget};
use blitz_traits::events::{Modifiers, MouseEventButton, UiEvent};

use vello::kurbo::Affine;

use crate::arrangement::{Arrangement, Palette, Viewport};
use crate::mousemap::Gesture;
use crate::profile::Counts;
use crate::ruler::{self, Bars};

/// What the arrangement shares with a docked mixer — see
/// `crate::mixer_panel::Links`.
#[derive(Clone)]
pub struct MixerLinks {
    pub toggle: Rc<std::cell::Cell<bool>>,
    pub rows: Rc<RefCell<(u64, Vec<(daw_proto::Track, u32)>)>>,
    /// The mixer's edits, to apply here as well.
    pub echo: Rc<RefCell<Vec<crate::engine::Edit>>>,
    /// Keys pressed while the mixer had the focus, to handle here.
    pub keys: Rc<RefCell<Vec<crate::mixer_panel::Key>>>,
}

/// The finest grid division the ruler will draw.
const FINEST: f64 = 1.0 / 16.0;

/// `event` with its pointer position divided by `ui`: CSS pixels to the
/// units the widget lays out in.
fn in_units(event: &UiEvent, ui: f64) -> UiEvent {
    let ui = ui.max(f64::EPSILON);
    if (ui - 1.0).abs() < f64::EPSILON {
        return event.clone();
    }
    #[expect(clippy::cast_possible_truncation, reason = "a coordinate")]
    let scaled = |e: &blitz_traits::events::BlitzPointerEvent| {
        let mut e = e.clone();
        e.coords.client_x = (f64::from(e.coords.client_x) / ui) as f32;
        e.coords.client_y = (f64::from(e.coords.client_y) / ui) as f32;
        e
    };
    match event {
        UiEvent::PointerMove(e) => UiEvent::PointerMove(scaled(e)),
        UiEvent::PointerDown(e) => UiEvent::PointerDown(scaled(e)),
        UiEvent::PointerUp(e) => UiEvent::PointerUp(scaled(e)),
        UiEvent::PointerCancel(e) => UiEvent::PointerCancel(scaled(e)),
        other => other.clone(),
    }
}

/// A finger held on the lanes: its press, kept to play as a click if it
/// turns out to be a tap; where it last was; whether it has become a
/// scroll.
struct Finger {
    down: blitz_traits::events::BlitzPointerEvent,
    last: (f64, f64),
    scrolling: bool,
    /// Pressed on the track panel: a sideways swipe there expands it or
    /// folds it back to names.
    on_panel: bool,
    /// Swiped: the panel has been asked to change, and the rest of this
    /// finger's gesture is spent.
    swiped: bool,
    /// Holding the play cursor's handle: the transport follows it.
    scrubbing: bool,
    /// How fast it is going, to throw the view when it lets go.
    speed: crate::touch::Speed,
    /// A second finger, and where it was: the two are a pinch — spread
    /// across, time zooms; spread down, the rows grow; moved together,
    /// the view goes with them.
    second: Option<(blitz_traits::events::BlitzPointerId, (f64, f64))>,
}

/// The track panel's grip: a slim handle standing on the panel's edge at
/// `x`, centred on `y` — what an iPad's split view has between its panes.
/// Drawn slim; a finger finds it by a generous margin round it
/// (`GRIP_REACH`).
fn grip_rect(x: f64, y: f64) -> vello::kurbo::Rect {
    const W: f64 = 5.0;
    const H: f64 = 28.0;
    vello::kurbo::Rect::new(x - W / 2.0, y - H / 2.0, x + W / 2.0, y + H / 2.0)
}

/// How far round the grip a finger still takes it.
const GRIP_REACH: (f64, f64) = (16.0, 12.0);

/// The grip itself: a light rounded bar, quiet until it is needed.
fn paint_grip(out: &mut Scene, rect: vello::kurbo::Rect) {
    use anyrender::PaintScene as _;
    use vello::kurbo::RoundedRect;
    use vello::peniko::{Color, Fill};
    let bar = RoundedRect::from_rect(rect, rect.width() / 2.0);
    out.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0x9a, 0x9f, 0xa8, 0xc0),
        None,
        &bar,
    );
}

/// The play cursor's handle, standing on the cursor at `x` in the bars
/// lane: a pin a finger can take hold of.
fn handle_rect(x: f64) -> vello::kurbo::Rect {
    // Smaller on a slim ruler, whose bars row is: a finger still takes
    // hold of it (its hit test is a fingertip round it).
    let (w, h) = if ruler::slim() {
        (10.0, 11.0)
    } else {
        (14.0, 18.0)
    };
    let bottom = ruler::ruler_h() - 3.0;
    vello::kurbo::Rect::new(x - w / 2.0, bottom - h, x + w / 2.0, bottom)
}

/// The handle itself: a light rounded tab, pointed at its foot where it
/// meets the line.
fn paint_handle(out: &mut Scene, rect: vello::kurbo::Rect) {
    use anyrender::PaintScene as _;
    use vello::kurbo::{BezPath, RoundedRect};
    use vello::peniko::{Color, Fill};
    let face = Color::from_rgb8(0xc9, 0xcc, 0xd1);
    let body = RoundedRect::new(rect.x0, rect.y0, rect.x1, rect.y1 - 4.0, 3.0);
    out.fill(Fill::NonZero, Affine::IDENTITY, face, None, &body);
    let mut tip = BezPath::new();
    tip.move_to((rect.x0, rect.y1 - 5.0));
    tip.line_to((rect.x1, rect.y1 - 5.0));
    tip.line_to((rect.center().x, rect.y1));
    tip.close_path();
    out.fill(Fill::NonZero, Affine::IDENTITY, face, None, &tip);
}

/// How far sideways, in CSS pixels, a finger on the track panel travels
/// before it is a swipe that expands or folds it.
const SWIPE: f64 = 36.0;

/// How short a row may open on a finger's screen, to fit every row shown
/// (the folders folded) down the lanes: a name still read.
const OPEN_ROW_TOUCH: f64 = 22.0;
/// How tall a row opens at least on a compact panel: a name you can read.
const OPEN_ROW_COMPACT: f64 = 28.0;
/// How wide a second opens at least on either, in CSS pixels: a bar of a
/// song at 120 is two dozen pixels, an item a thing you can take hold of.
const OPEN_PPS: f64 = 12.0;

/// Where the view is, shared between the window and the widget.
///
/// A plain cell rather than a signal. The paint happens inside Blitz's
/// own traversal, which is not the Dioxus runtime — reading a `Signal`
/// there is a panic waiting for the first frame that takes a different
/// path. What the widget needs is four numbers, and four numbers do not
/// need a reactive graph.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct View {
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub zoom_x: f64,
    pub zoom_y: f64,
    /// Where the transport is, in seconds.
    ///
    /// The window's to know and the widget's to draw. A position is a
    /// LEVEL and not an event — see `engine::Transport` — so it is
    /// written every frame like the scroll rather than sent as a
    /// change, and a missed one is covered by the next.
    pub play_at: f64,
}

impl View {
    /// Where a freshly opened session is looked at from: the start, at
    /// zoom one, the playhead at zero.
    pub const OPENING: Self = Self {
        scroll_x: 0.0,
        scroll_y: 0.0,
        zoom_x: 1.0,
        zoom_y: 1.0,
        play_at: 0.0,
    };
}

/// Blitz's modifier set, as the mouse map's.
pub(crate) fn mods(from: Modifiers) -> crate::mousemap::Mods {
    crate::mousemap::Mods {
        shift: from.contains(Modifiers::SHIFT),
        ctrl: from.contains(Modifiers::CONTROL),
        alt: from.contains(Modifiers::ALT),
    }
}

/// How tall the strip at the top of a piece's row is where its hits are
/// taken by hand — their flags — px.
const FLAG_STRIP: f64 = 10.0;

/// How far up from a kit row's foot a press takes a hit marker to
/// stretch it, pixels: the strip its stretches are drawn in, and a little.
const FOOT_STRIP: f64 = 22.0;
/// How near a flag a press must land to take it, px.
const FLAG_REACH: f64 = 5.0;
/// How far a flag must be dragged to move its hit, px: under it, a click.
const FLAG_SLOP: f64 = 2.0;

/// A hit marker being dragged by its flag.
struct HitDrag {
    id: String,
    /// Every hit dragged: the selection the flag taken is in, else it.
    ids: Vec<String>,
    from_x: f64,
    x: f64,
    /// Ctrl held at the press: the marker alone moves — where the hit
    /// was found is corrected, the audio stays. Without it the kit moves
    /// so the hit plays where it is dropped.
    marker_only: bool,
    /// Taken by its foot: the hit moved by stretching the audio either
    /// side of it (`HITS_STRETCH`), not slipping it.
    stretch: bool,
}

/// What is under the pointer in a piece row's flag strip.
struct FlagSpot {
    /// The hit whose flag it is on, if any.
    mark: Option<String>,
    /// The pieces the row shows.
    pieces: Vec<String>,
    /// Where it is, project seconds as the song plays now.
    time: f64,
    /// On the row's foot, where a marker is taken to stretch.
    foot: bool,
}

/// The Quantize panel asked for on the Action List: the frame that
/// shows it is owed.
fn quantize_asked() -> bool {
    #[cfg(feature = "native")]
    return crate::quantize_panel::asked();
    #[cfg(not(feature = "native"))]
    false
}

/// Run a hit edit made by hand on the Action List, off the window's
/// thread — the same action a model runs — and draw the markers again.
fn run_hit_action(id: &'static str, args: String) {
    #[cfg(feature = "native")]
    if let Some(runtime) = crate::open::runtime() {
        runtime.spawn_blocking(move || {
            if let Err(e) = crate::actions::list().run_json(id, args.as_bytes()) {
                tracing::warn!(hits.action = id, hits.error = %e, "a hit edit by hand was refused");
            }
            crate::hit_overlay::refresh();
        });
    }
    #[cfg(not(feature = "native"))]
    let _ = (id, args);
}

/// Run a swipe's comp (`TRACK_COMP_FROM_LANE`) on the Action List, off the
/// window's thread — the same action a model runs, which asks for the song
/// to be read back once it is done.
fn run_comp(args: String) {
    #[cfg(feature = "native")]
    if let Some(runtime) = crate::open::runtime() {
        runtime.spawn_blocking(move || {
            let id = crate::comp::SWIPE_COMP.id;
            if let Err(e) = crate::actions::list().run_json(id, args.as_bytes()) {
                tracing::warn!(comp.action = id, comp.error = %e, "a swipe comp was refused");
            }
        });
    }
    #[cfg(not(feature = "native"))]
    let _ = args;
}

/// A knob under the pointer, mid-turn.
/// The undo step a take volume curve gesture makes.
const TAKE_LABEL: &str = "Edit item volume";

/// A point of an item's take volume curve being dragged: the item, the
/// point's index now, and where the item is on screen.
struct TakeDrag {
    item: String,
    index: usize,
    left: f64,
    top: f64,
    height: f64,
    /// The item's length, seconds: the point stays inside it.
    length: f64,
}

struct ItemTurn {
    index: usize,
    from: f64,
    was: f64,
    now: f64,
}

impl ItemTurn {
    /// Quieter or louder for every pixel the pointer has gone down or up
    /// since the press: a quarter dB a pixel, a twentieth with Ctrl.
    fn turn(&mut self, y: f64, fine: bool) {
        let per_px = if fine { 0.05 } else { 0.25 };
        self.now = (self.from - y).mul_add(per_px, self.was).clamp(-60.0, 24.0);
    }
}

struct Turn {
    spot: crate::pointer::RowSpot,
    /// Where the press landed, in the widget's own coordinates.
    from: f64,
    /// The track as it was when the press landed.
    was: daw_proto::Track,
}

/// How far a knob turns through its whole range, in pixels.
///
/// REAPER's own, so a full sweep takes about the same movement here as
/// it does there.
const KNOB_TRAVEL: f64 = 150.0;

/// A handle on that, for the window to write and the widget to read.
pub type Shared = Rc<RefCell<View>>;

/// How much the last frame drew, for the benchmark to report.
#[derive(Clone, Copy, Debug, Default)]
pub struct Drawn {
    pub replayed: u64,
    pub submitted: u64,
}

/// The arrangement, as a widget.
pub struct ArrangementWidget {
    /// The tool the panel holds and the pointer's shape — see
    /// [`crate::tool`].
    pointing: crate::tool::Shared,
    /// What the which-key popup shows, for the panel to render.
    which: crate::which_key::Shared,
    /// Zooms the keys asked for, for the panel (which owns the view) to
    /// carry out.
    zooms: crate::zoom::Requests,
    /// A song opens fitted: every track top to bottom (`z v`), then the
    /// whole song across (`z x`) — asked once, on the first paint, when
    /// the rows have their places.
    fit_on_open: bool,
    /// A finger held on the lanes, not yet a scroll or a tap
    /// ([`ArrangementWidget::fingered`]).
    finger: Option<Finger>,
    /// The view still moving after a finger threw it.
    fling: Option<crate::touch::Fling>,
    /// Ask the host for another frame, for a fling: see
    /// [`crate::touch::redraw_hook`].
    redraw: Option<Rc<dyn Fn()>>,
    /// Touch mode, as the panel was opened in.
    touch: bool,
    /// Where the play cursor's handle was last drawn, in the widget's
    /// units: what a finger takes to scrub.
    handle: Cell<Option<vello::kurbo::Rect>>,
    /// Where the track panel's grip was last drawn: a finger on it slides
    /// the panel open or shut.
    grip: Cell<Option<vello::kurbo::Rect>>,
    /// How much bigger the arrangement is drawn than laid out (touch
    /// mode's zoom, or 1), shared with the panel.
    ui: Rc<Cell<f64>>,
    /// How to plan the rows again, and the project as the engine has it
    /// (with the visibility this window has changed since): what a track
    /// shown or hidden re-plans from. `None` in a widget built without
    /// one, which then cannot show or hide anything.
    replan: Option<(crate::studio::Planner, Left)>,
    /// A page's session read back from the engine, waiting to be drawn
    /// ([`Self::resync`]).
    #[cfg(not(feature = "native"))]
    read_back: Rc<std::cell::RefCell<Option<daw_ui::studio::project::Project>>>,
    /// The rows' height at zoom 1, for the panel's scroll range, which
    /// changes when rows are shown or hidden.
    content_h: Rc<std::cell::Cell<f64>>,
    /// The docked mixer's links (`crate::mixer_panel::Links`): asking for
    /// it to toggle, the rows it shows, and the edits it made, to apply
    /// here too. `None` with no mixer beside this arrangement.
    mixer: Option<MixerLinks>,
    scene: Arrangement,
    palette: Palette,
    /// `skin::version()` when the panel was cut and the palette taken: a
    /// different one is a theme changed under it.
    skin_seen: u64,
    font: crate::text::Font,
    bars: Bars,
    grid: adaptive_grid::Adaptive,
    /// Pixels per second before the zoom — the base the view scales.
    pps: f64,
    /// What the panel's live controls are drawn from.
    ///
    /// They are NOT in the recording, and cannot be: a knob shows a
    /// value, and a value changes without the session changing. The
    /// recording holds the row — its tint, its rail, its number, its
    /// name — and the controls go over it every frame, which is what
    /// the painted window has always done.
    rows: Vec<(daw_proto::Track, u32)>,
    tracks: Vec<daw_proto::Track>,
    map: crate::plan::Rows,
    /// What row heights come from, for re-cutting the panel.
    layout: crate::layout::Layout,
    view: Shared,
    drawn: Rc<RefCell<Drawn>>,
    /// The frame-time graph, when the window asked for one.
    ///
    /// A window asks; a picture or a benchmark does not, because an
    /// overlay is a difference in the one and a cost in the other.
    stats: Option<crate::fps::Stats>,
    /// The size the last paint was given, so a hit arriving between
    /// two paints is measured against the frame the picture was drawn
    /// in and not against whatever the window has become since.
    size: (f64, f64),
    /// What the pointer is on, and what it is doing to it.
    ///
    /// Cheap precisely because the panel is a recording: a control
    /// cannot change appearance without re-cutting the row, and it does
    /// not have to. The one control under the pointer is drawn AGAIN,
    /// over the recording, in its hover cell — one control a frame
    /// instead of a panel a mouse move.
    pointer: crate::pointer::Pointer<crate::pointer::RowSpot>,
    /// Items, fades, the ruler and the time selection.
    ///
    /// The whole arrangement-editing state machine, which the direct
    /// window has had from the start — moves, trims, fades, marker
    /// drags, region edges, the zoom tool. Not reimplemented here: the
    /// widget hands it hits and paints what it says is in flight.
    editor: crate::arrange_edit::Editor,
    /// The widget's own copy of the project, which the editor moves
    /// before the engine has.
    ///
    /// The same bargain as the tracks — see
    /// [`ArrangementWidget::assume`]. An item that jumps back to where
    /// it was for two frames after you drop it reads as a failed drag.
    project: daw_ui::studio::project::Project,
    /// What is needed to cut the recording again when an edit changes
    /// what it holds.
    previews: crate::midi::Previews,
    /// The previews' generation the recording was cut at: when notes or
    /// waveforms arrive after it (a browser's), it is cut again.
    previews_seen: u64,
    /// The folds the rows were planned at ([`crate::folds::generation`]).
    folds_seen: u64,
    /// The outside edits the rows were planned at ([`edit_song`]).
    edited_seen: u64,
    /// The ruler's own furniture, for hit testing.
    sections: Vec<daw_ui::studio::project::Section>,
    markers: Vec<daw_ui::studio::project::Marker>,
    /// Whether the last event changed the picture, for
    /// `Widget::needs_redraw`. A `Cell` because that question is asked
    /// through `&self`.
    dirty: std::cell::Cell<bool>,
    /// Where the play cursor has been, for its trail.
    trail: crate::cursor::Trail,
    /// The engine's levels, for the meters in the name fields — and
    /// whether any was lit last frame, so a falling meter keeps redrawing.
    meters: Option<crate::engine::Meters>,
    meters_lit: Cell<bool>,
    /// Whether the panel is in its compact shape — shared with the
    /// toolbar that toggles it, read each paint (a change re-cuts the
    /// recorded panel, which is drawn to the shape).
    compact: Rc<Cell<bool>>,
    /// Whether the window is in Record mode, set by the panel from the
    /// window's mode: the track panel's arm is shown only then.
    recording: Rc<Cell<bool>>,
    /// The panel's Mix-mode flag: every item in its track's colour.
    mixing: Rc<Cell<bool>>,
    /// The panel's Edit-mode flag: the drum detection drawn over the kit.
    editing: Rc<Cell<bool>>,
    /// The drum detection's version last drawn.
    detection_seen: Cell<u64>,
    /// The fine waveforms' generation last drawn ([`crate::wave_detail`]).
    detail_seen: Cell<u64>,
    /// The tracks' markers and regions as last laid out
    /// ([`crate::marks_strip::version`]).
    marks_seen: Cell<u64>,
    /// The points of interest drawn ([`crate::poi::version`]).
    poi_seen: Cell<u64>,
    /// The scroll and zoom last drawn: moved by the panel without an
    /// event here (a zoom a model asked for), the picture is due again.
    view_drawn: Cell<(f64, f64, f64, f64)>,
    /// Whether the editor took the last press and has not been let go
    /// of yet.
    ///
    /// Tracked here rather than asked of the editor, because
    /// `Editor::dragging` answers a different question: it is false
    /// until a ghost exists, and the ghost is created INSIDE `moved`.
    /// Gating the moves on it means the first move never arrives, so
    /// the ghost is never made and nothing can be dragged at all — and
    /// a press on the ruler is not in its answer under any
    /// circumstances.
    holding: bool,
    /// The item under the pointer, so its fade handles are drawn —
    /// REAPER puts them in the top corners and only shows them on the
    /// item you are over.
    hovered_item: Option<usize>,
    /// An open rename, and the row it belongs to.
    ///
    /// The field draws over the name it replaces and takes the keyboard
    /// until it is committed or abandoned. Blitz sends key events to
    /// the focused node, and a pointer-down on a widget focuses it, so
    /// the double-click that opens this has already done the focusing.
    renaming: Option<crate::rename::Rename>,
    /// What a key means here.
    ///
    /// The window used to have no answer at all: `typed` served the
    /// rename field and dropped everything else, so on the blitz path
    /// Delete, Home and every bound action did nothing — the gesture
    /// reached the widget and the widget threw it away. Loaded here
    /// rather than in the binary because the EDITOR is here, and a key
    /// that acts on the arrangement has to arrive where the arrangement
    /// is. See #124.
    keys: crate::keys::Keys,
    /// The last refused edit, until it has had its say.
    ///
    /// One at a time and the newest wins: refusals arrive in answer to
    /// a gesture, and the gesture in front of you is the one you are
    /// asking about. A queue would show you the reply to a drag you had
    /// already given up on.
    notice: Option<crate::notice::Notice>,
    /// A gain staging run (Shift+G) or clear (Alt+G) that has finished
    /// off the frame: what to say about it. The next frame reads the
    /// session back — the item volumes the waveforms are drawn through.
    staged: std::sync::Arc<std::sync::Mutex<Option<&'static str>>>,
    /// The scene the rows were planned by before an overview took over,
    /// to go back to: `Some(previous)` while one is up.
    scene_before: Option<Option<&'static str>>,
    /// When and where the last click on a NAME landed, so the next one
    /// can tell whether it is the second half of a double.
    last_name: Option<(usize, web_time::Instant)>,
    /// A hit marker taken by its flag (Edit mode), mid-drag.
    hit_drag: Option<HitDrag>,
    /// The hit markers selected (Edit mode): a marquee drawn over the
    /// kit's rows, or the flag last taken — dragged together.
    hit_selected: std::collections::HashSet<String>,
    /// A marquee being drawn over the kit's rows: where it began and where
    /// the pointer is now, in the widget's coordinates.
    hit_marquee: Option<((f64, f64), (f64, f64))>,
    /// The band on the ruler last clicked and when: the first half of the
    /// double-click that selects its time.
    last_band: Option<((f64, f64), web_time::Instant)>,
    /// A swipe along a take's lane, on a row showing its lanes: the
    /// stretch comped onto the comp lane when let go ([`crate::swipe`]).
    swipe: Option<crate::swipe::Swipe>,
    /// Where and when a piece row's flag strip was last clicked with no
    /// flag under it: the first half of the double-click that places a
    /// hit.
    last_strip: Option<(f64, web_time::Instant)>,
    /// A knob being turned: where the press landed, and the track as it
    /// was at that moment.
    ///
    /// The value at PRESS, not the value now. A drag is an absolute
    /// gesture — this far up from where you grabbed it is this much
    /// louder — and reading the current value each move would compound
    /// the deltas, so a slow drag and a fast one over the same distance
    /// would land somewhere different.
    turning: Option<Turn>,
    /// An item's volume dial being turned: which item, the pointer's
    /// height at the press, the dB it was at and is at now.
    item_turn: Option<ItemTurn>,
    /// A drag in an automation lane ([`crate::automation_edit`]).
    lane_drag: Option<crate::automation_edit::LaneDrag>,
    /// What in the automation lanes is under the pointer, lit.
    lane_lit: crate::automation_draw::Lit,
    /// A point of an item's take volume curve being dragged
    /// ([`crate::automation_draw::take_lines`]).
    take_drag: Option<TakeDrag>,
    /// The item and the point of its take volume curve under the pointer.
    take_lit: Option<(usize, Option<usize>)>,
    /// The automation menu, open over the window
    /// ([`crate::automation_menu`]).
    #[cfg(feature = "native")]
    menu: Option<crate::popup::Popup<crate::automation_menu::Pick>>,
    /// What the widget wants done to the session, for the window to
    /// pick up and hand to the engine.
    ///
    /// A queue rather than a call, because a widget is a painter: it
    /// knows a click landed on the mute button of row 12, and it must
    /// not know what a mute is, how to reach the engine, or whether
    /// doing it is allowed. See `crate::engine::Edit`.
    edits: Rc<RefCell<Vec<crate::engine::Edit>>>,
    /// The live controls, recorded, and the zoom they were cut at.
    ///
    /// The single most expensive thing the paint used to do: forty rows
    /// of vector art — arms, knobs, meters, buttons, names — generated
    /// from scratch every frame, while a pan or a scroll changes none
    /// of it. Recorded per row like the panel is, so a scroll replays a
    /// different span rather than rebuilding anything.
    ///
    /// Re-cut when the vertical zoom moves, because that is what
    /// decides which tier of controls a row shows. A value changing —
    /// somebody turning a knob — needs the same, and the hook for that
    /// is [`ArrangementWidget::values_changed`].
    controls: Option<(crate::overlay::Controls, f64)>,
    /// Last frame's vertical zoom, so this one can tell whether a
    /// gesture is still in flight.
    ///
    /// Seeded from the view rather than left empty, so the FIRST frame
    /// counts as settled and takes the cut. Otherwise a window that is
    /// never touched draws live forever, and — worse — the one frame a
    /// comparison shot renders is the one frame that does not use the
    /// cache, so the gates would be checking a path the window does not
    /// take.
    was: f64,
    /// What the last paint's passes cost, in microseconds.
    ///
    /// Kept across frames so the readout has something to say on the
    /// frame it is drawn on, rather than reporting the pass timings of
    /// a frame that has not finished yet.
    spent: Passes,
}

/// What one paint spent, pass by pass.
///
/// The frame graph says a frame costs ten milliseconds; this says which
/// part of the widget it went to. Without it the only honest thing to
/// do about a slow frame is guess, and a guess costs a whole build to
/// disprove.
#[derive(Clone, Copy, Debug, Default)]
struct Passes {
    lanes: u128,
    titles: u128,
    panel: u128,
    ruler: u128,
    controls: u128,
}

impl ArrangementWidget {
    /// Build the recording once, from the session.
    ///
    /// Once, and that is the whole point: the commands for a lane do not
    /// change when the view moves over them, so a pan replays what is on
    /// screen and records nothing.
    #[must_use]
    pub fn new(
        scene: Arrangement,
        palette: Palette,
        font: crate::text::Font,
        bpm: f64,
        pps: f64,
        rows: Vec<(daw_proto::Track, u32)>,
        layout: crate::layout::Layout,
        project: daw_ui::studio::project::Project,
        previews: crate::midi::Previews,
        view: Shared,
        readout: bool,
    ) -> Self {
        let tracks: Vec<daw_proto::Track> = rows.iter().map(|(t, _)| t.clone()).collect();
        let at_rest = view.borrow().zoom_y;
        let map = crate::plan::Rows::of(&rows, &tracks);
        Self {
            pointing: crate::tool::Pointing::shared(None),
            which: crate::which_key::Shared::default(),
            zooms: crate::zoom::Requests::default(),
            fit_on_open: true,
            finger: None,
            fling: None,
            redraw: None,
            touch: false,
            handle: Cell::new(None),
            grip: Cell::new(None),
            ui: Rc::new(Cell::new(1.0)),
            replan: None,
            #[cfg(not(feature = "native"))]
            read_back: Rc::default(),
            mixer: None,
            content_h: Rc::new(std::cell::Cell::new(
                rows.iter().map(|(track, _)| layout.slot_of(track)).sum(),
            )),
            scene,
            palette,
            skin_seen: crate::skin::version(),
            font,
            bars: Bars::at(bpm),
            grid: adaptive_grid::Adaptive::default(),
            pps,
            rows,
            tracks,
            map,
            layout,
            view,
            drawn: Rc::new(RefCell::new(Drawn::default())),
            size: (0.0, 0.0),
            pointer: crate::pointer::Pointer::default(),
            editor: crate::arrange_edit::Editor::default(),
            hovered_item: None,
            holding: false,
            dirty: std::cell::Cell::new(false),
            trail: crate::cursor::Trail::default(),
            meters: crate::engine::Meters::start(),
            meters_lit: Cell::new(false),
            compact: Rc::new(Cell::new(false)),
            recording: Rc::new(Cell::new(true)),
            mixing: Rc::new(Cell::new(false)),
            editing: Rc::new(Cell::new(false)),
            detection_seen: Cell::new(0),
            detail_seen: Cell::new(0),
            marks_seen: Cell::new(0),
            poi_seen: Cell::new(0),
            view_drawn: Cell::new((0.0, 0.0, 0.0, 0.0)),
            sections: project.sections.clone(),
            markers: project.markers.clone(),
            project,
            previews_seen: previews.generation(),
            folds_seen: crate::folds::generation(),
            edited_seen: EDITED.with(Cell::get),
            previews,
            renaming: None,
            keys: crate::keys::Keys::load(),
            notice: None,
            staged: std::sync::Arc::default(),
            scene_before: None,
            last_name: None,
            hit_drag: None,
            hit_selected: std::collections::HashSet::new(),
            hit_marquee: None,
            last_band: None,
            swipe: None,
            last_strip: None,
            turning: None,
            item_turn: None,
            lane_drag: None,
            lane_lit: crate::automation_draw::Lit::default(),
            take_drag: None,
            take_lit: None,
            #[cfg(feature = "native")]
            menu: None,
            edits: Rc::new(RefCell::new(Vec::new())),
            controls: None,
            was: at_rest,
            stats: readout.then(crate::fps::Stats::new),
            spent: Passes::default(),
        }
    }

    /// The widget over an opened session, built the way every host builds
    /// it: the dark theme's palette, the embedded font, the row layout
    /// the environment asks for, and the recording cut in the panel's
    /// shape (`compact`).
    ///
    /// The app's panel ([`crate::panel::use_arrangement_panel`]) and the
    /// headless benchmark (`bin/bench`) both start here, so a number the
    /// bench prints is the cost of the recording the window draws.
    ///
    /// # Panics
    ///
    /// If the embedded font does not load — a build defect, not a
    /// runtime condition.
    #[must_use]
    pub fn for_session(
        project: &daw_ui::studio::ProjectRef,
        rows: &daw_ui::studio::RowsRef,
        previews: &crate::midi::Previews,
        compact: bool,
        view: Shared,
        readout: bool,
    ) -> Self {
        // In the theme's colours, where it names them.
        crate::skin::from_env();
        let palette = crate::skin::palette(&Palette::from_theme(&daw_ui::theming::Theme::dark()));
        let font = crate::text::Font::embedded().expect("the embedded font");
        let layout = crate::layout::Layout::from_env();
        crate::tcp::fit_names(&font, rows.as_slice(), layout);
        let scene = Arrangement::build(
            &palette,
            &font,
            project,
            rows,
            layout,
            previews,
            crate::tcp::Tcp {
                compact,
                ..crate::tcp::Tcp::FULL
            },
        );
        let bpm = scene.bpm;
        Self::new(
            scene,
            palette,
            font,
            bpm,
            crate::studio::PPS,
            rows.as_slice().to_vec(),
            layout,
            // The widget's own copy, which its editor moves before the
            // engine has — see `ArrangementWidget::project`.
            (*project.0).clone(),
            previews.clone(),
            view,
            readout,
        )
        .with_compact(Rc::new(Cell::new(compact)))
    }

    /// The recording the widget replays — for a caller that has to aim a
    /// gesture at something in it (the benchmark's slip drag).
    #[must_use]
    pub const fn scene(&self) -> &Arrangement {
        &self.scene
    }

    /// Touch mode: the view opens with rows a finger can hit.
    #[must_use]
    pub fn with_touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }

    /// How to ask the host for another frame, for a fling
    /// ([`crate::touch::redraw_hook`]).
    #[must_use]
    pub fn with_redraw(mut self, redraw: Option<Rc<dyn Fn()>>) -> Self {
        self.redraw = redraw;
        self
    }

    /// Whether the first paint fits the song (the whole of it, every row):
    /// not when the view is taken up from where it was left.
    #[must_use]
    pub fn with_fit_on_open(mut self, fit: bool) -> Self {
        self.fit_on_open = fit;
        self
    }

    /// Share the panel's touch zoom ([`crate::panel::ArrangementPanel::ui`]):
    /// laid out in CSS pixels over it, drawn at it.
    #[must_use]
    pub fn with_ui(mut self, ui: Rc<Cell<f64>>) -> Self {
        self.ui = ui;
        self
    }

    /// Share the panel's shape with whoever toggles it.
    #[must_use]
    pub fn with_compact(mut self, compact: Rc<Cell<bool>>) -> Self {
        self.compact = compact;
        self
    }

    /// Share the panel's Mix-mode flag: items in their tracks' colours.
    #[must_use]
    pub fn with_mixing(mut self, mixing: Rc<Cell<bool>>) -> Self {
        self.mixing = mixing;
        self
    }

    /// What of a view changes the picture's framing: its scroll and zoom.
    const fn framing(view: &View) -> (f64, f64, f64, f64) {
        (view.scroll_x, view.scroll_y, view.zoom_x, view.zoom_y)
    }

    /// Share the panel's Edit-mode flag: the drum detection over the kit.
    #[must_use]
    pub fn with_editing(mut self, editing: Rc<Cell<bool>>) -> Self {
        self.editing = editing;
        self
    }

    /// The band of rows (unzoomed, top to bottom) showing `tracks` — a
    /// track in a shut folder by the folder's row. `None` for none.
    fn rows_of(&self, tracks: &[String]) -> Option<(f64, f64)> {
        let row_of = |guid: &str| {
            let mut at = Some(guid.to_owned());
            while let Some(guid) = at {
                if let Some(row) = self.rows.iter().position(|(t, _)| t.guid == guid) {
                    return Some(row);
                }
                at = self
                    .project
                    .tracks
                    .iter()
                    .find(|t| t.guid == guid)
                    .and_then(|t| t.parent_guid.clone());
            }
            None
        };
        let boxes: Vec<(f64, f64)> = tracks
            .iter()
            .filter_map(|t| row_of(t))
            .filter_map(|row| self.scene.row_box(row))
            .collect();
        let top = boxes.iter().map(|b| b.0).reduce(f64::min)?;
        let bottom = boxes.iter().map(|b| b.0 + b.1).fold(top, f64::max);
        Some((top, bottom))
    }

    /// The track being edited and every folder above it, nearest first:
    /// the selected one (the first, when several are) — or, nothing
    /// selected, the kit's folder.
    fn edited_tracks(&self) -> Vec<String> {
        let chosen = self.selected_and_above();
        if !chosen.is_empty() {
            return chosen;
        }
        self.kit_rows()
            .and_then(|kit| self.rows.get(kit.start))
            .map(|(t, _)| vec![t.guid.clone()])
            .unwrap_or_default()
    }

    /// A double-click on a band on the ruler — a section, a zone, the
    /// edited track's own region in the MARKS lane, a Grid region in the
    /// GRID lane — makes its time the time selection, the edit cursor at
    /// its start: what Q quantizes and Backspace puts back. The innermost
    /// band under the pointer. The first click goes on to the editor.
    fn band_gesture(&mut self, event: &UiEvent) -> bool {
        let UiEvent::PointerDown(e) = event else {
            return false;
        };
        if e.button != MouseEventButton::Main {
            return false;
        }
        let (x, y) = (f64::from(e.coords.client_x), f64::from(e.coords.client_y));
        if y >= ruler::ruler_h() || x < self.scene.tcp.width() {
            return false;
        }
        let view = self.viewport(self.size.0, self.size.1);
        let t = self.seconds_at(x, view);
        let row = ruler::lane_at(y, 0.0);
        let mut spans: Vec<(f64, f64)> = self
            .scene
            .sections()
            .iter()
            .filter(|s| row == Some(ruler::lane_row(s.lane)) && t >= s.start && t < s.end)
            .map(|s| (s.start, s.end))
            .collect();
        let edited = self.edited_tracks();
        if row == Some(ruler::MARKS_ROW) {
            spans.extend(crate::marks_strip::spans_at(&edited, t, false));
        }
        let grid_row = ruler::ruler_h() - ruler::bars_h() - ruler::TEMPO_H;
        if self.editing.get() && row.is_none() && y >= grid_row && y < grid_row + ruler::TEMPO_H {
            spans.extend(crate::marks_strip::spans_at(&edited, t, true));
        }
        let Some(span) = spans
            .into_iter()
            .min_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)))
        else {
            return false;
        };
        let now = web_time::Instant::now();
        let again = self.last_band.is_some_and(|(was, when)| {
            (was.0 - span.0).abs() < 1e-9
                && (was.1 - span.1).abs() < 1e-9
                && now.saturating_duration_since(when) <= crate::gesture::DOUBLE
        });
        if !again {
            self.last_band = Some((span, now));
            return false;
        }
        self.last_band = None;
        self.editor.cursor = crate::cursor::Edit {
            at: span.0,
            selection: Some(crate::cursor::Span::between(span.0, span.1)),
        };
        crate::cursor::publish(self.editor.cursor);
        true
    }

    /// The selected track (the first, when several are) and every folder
    /// above it, nearest first; none with nothing selected.
    fn selected_and_above(&self) -> Vec<String> {
        let tracks = &self.project.tracks;
        let mut out = Vec::new();
        let mut at = tracks.iter().find(|t| t.selected).map(|t| t.guid.clone());
        while let Some(guid) = at {
            at = tracks
                .iter()
                .find(|t| t.guid == guid)
                .and_then(|t| t.parent_guid.clone());
            out.push(guid);
        }
        out
    }

    /// The rows of the kit: the top-level `Drums` folder's and every row
    /// under it, `None` when it is not on screen.
    fn kit_rows(&self) -> Option<std::ops::Range<usize>> {
        let first = self
            .rows
            .iter()
            .position(|(t, depth)| *depth == 0 && t.name.trim().eq_ignore_ascii_case("drums"))?;
        let len = self
            .rows
            .iter()
            .skip(first.saturating_add(1))
            .take_while(|(_, depth)| *depth > 0)
            .count();
        Some(first..first.saturating_add(len).saturating_add(1))
    }

    /// What is under `(x, y)` on a piece row, in Edit mode with the hit
    /// markers drawn: a marker — its flag, or its line anywhere down the
    /// row, which is where a hand reaches for it — or, in the flag strip
    /// along the row's top with no marker there, the strip itself (where
    /// a double-click places one). Below the strip and off every line,
    /// nothing: the press is the item's.
    fn flag_at(&self, x: f64, y: f64) -> Option<FlagSpot> {
        if !self.editing.get() {
            return None;
        }
        let hits = crate::hit_overlay::current()?;
        let kit = self.kit_rows()?;
        let view = self.viewport(self.size.0, self.size.1);
        if x < view.panel_w {
            return None;
        }
        let (ox, oy) = (
            self.scene.tcp.width() - view.scroll_x,
            ruler::ruler_h() - view.scroll_y,
        );
        let pps = view.pps.max(f64::EPSILON);
        let time = (x - ox) / pps;
        let rows = self.piece_rows(kit, &hits);
        let mut in_strip = false;
        let mut foot = false;
        let (row, _) = rows.iter().find(|(row, _)| {
            self.scene.row_box(*row).is_some_and(|(top, height)| {
                let y0 = top.mul_add(view.zoom_y, oy);
                let y1 = height.mul_add(view.zoom_y, y0);
                in_strip = y >= y0 && y <= y0 + FLAG_STRIP;
                foot = y >= y1 - FOOT_STRIP && y <= y1;
                y >= y0 && y <= y1
            })
        })?;
        let pieces: Vec<usize> = rows
            .iter()
            .filter(|(r, _)| r == row)
            .map(|(_, p)| *p)
            .collect();
        let reach = FLAG_REACH / pps;
        let mark = hits
            .between(time - reach, time + reach)
            .iter()
            .filter(|m| pieces.contains(&m.piece))
            .min_by(|a, b| (a.time - time).abs().total_cmp(&(b.time - time).abs()))
            .map(|m| m.id.clone());
        if mark.is_none() && !in_strip {
            return None;
        }
        Some(FlagSpot {
            mark,
            pieces: pieces
                .iter()
                .filter_map(|p| hits.pieces.get(*p).cloned())
                .collect(),
            time,
            foot,
        })
    }

    /// Whether `(x, y)` is on the kit's rows' lanes, editing.
    fn on_kit(&self, x: f64, y: f64) -> bool {
        if !self.editing.get() {
            return false;
        }
        let Some(kit) = self.kit_rows() else {
            return false;
        };
        let view = self.viewport(self.size.0, self.size.1);
        if x < view.panel_w || y < ruler::ruler_h() {
            return false;
        }
        let oy = ruler::ruler_h() - view.scroll_y;
        kit.into_iter().any(|row| {
            self.scene.row_box(row).is_some_and(|(top, height)| {
                let top = top.mul_add(view.zoom_y, oy);
                y >= top && y < height.mul_add(view.zoom_y, top)
            })
        })
    }

    /// The hits a marquee from `a` to `b` covers: on the kit's piece rows
    /// it crosses, between the times its sides are at. A marquee too small
    /// to be one (a click) covers none — it clears the selection.
    fn hits_within(&self, a: (f64, f64), b: (f64, f64)) -> std::collections::HashSet<String> {
        let mut out = std::collections::HashSet::new();
        if (a.0 - b.0).abs() < FLAG_SLOP && (a.1 - b.1).abs() < FLAG_SLOP {
            return out;
        }
        let (Some(hits), Some(kit)) = (crate::hit_overlay::current(), self.kit_rows()) else {
            return out;
        };
        let view = self.viewport(self.size.0, self.size.1);
        let oy = ruler::ruler_h() - view.scroll_y;
        let (t0, t1) = {
            let (x, y) = (self.seconds_at(a.0, view), self.seconds_at(b.0, view));
            (x.min(y), x.max(y))
        };
        let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
        let pieces: Vec<usize> = self
            .piece_rows(kit, &hits)
            .into_iter()
            .filter(|(row, _)| {
                self.scene.row_box(*row).is_some_and(|(top, height)| {
                    let top = top.mul_add(view.zoom_y, oy);
                    top < y1 && height.mul_add(view.zoom_y, top) > y0
                })
            })
            .map(|(_, piece)| piece)
            .collect();
        for m in hits.between(t0, t1) {
            if pieces.contains(&m.piece) {
                out.insert(m.id.clone());
            }
        }
        out
    }

    /// A hit taken by hand on its flag, in Edit mode: dragged, it moves
    /// (`HITS_MOVE`, by as much as it was dragged — the slice it is in
    /// moves whole, so a drag is the same anywhere); Alt-clicked, it is
    /// removed; Shift-clicked, checked (`HITS_MARK`); a double-click on
    /// the strip with no flag under it places one there for the row's
    /// piece, snapped to the attack (`HITS_ADD`). Every gesture is the
    /// action a model runs, so a hand and a model edit the same hits.
    /// Whether the event was the hit's.
    fn hit_gesture(&mut self, event: &UiEvent) -> bool {
        match event {
            UiEvent::PointerDown(e)
                if e.button == MouseEventButton::Main && !self.pointing.borrow().tool_active() =>
            {
                let (x, y) = (f64::from(e.coords.client_x), f64::from(e.coords.client_y));
                let Some(spot) = self.flag_at(x, y) else {
                    // Editing, the kit's items are not moved by hand: a
                    // press on its rows draws a marquee over its hits.
                    if self.on_kit(x, y) {
                        self.hit_marquee = Some(((x, y), (x, y)));
                        return true;
                    }
                    return false;
                };
                let held = mods(e.mods);
                match spot.mark {
                    Some(id) if held.alt => {
                        run_hit_action("HITS_REMOVE", format!(r#"{{"ids":["{id}"]}}"#));
                        true
                    }
                    Some(id) if held.shift => {
                        run_hit_action(
                            "HITS_MARK",
                            format!(r#"{{"ids":["{id}"],"approve":true}}"#),
                        );
                        true
                    }
                    Some(id) => {
                        // A flag in the selection takes the selection; any
                        // other, itself alone.
                        if !self.hit_selected.contains(&id) {
                            self.hit_selected = std::iter::once(id.clone()).collect();
                        }
                        let mut ids: Vec<String> = self.hit_selected.iter().cloned().collect();
                        ids.sort();
                        self.hit_drag = Some(HitDrag {
                            id,
                            ids,
                            from_x: x,
                            x,
                            marker_only: held.ctrl,
                            stretch: spot.foot && !held.ctrl,
                        });
                        true
                    }
                    None => {
                        let now = web_time::Instant::now();
                        let again = self.last_strip.is_some_and(|(at, when)| {
                            (at - x).abs() < FLAG_REACH
                                && now.saturating_duration_since(when) <= crate::gesture::DOUBLE
                        });
                        if again && let [piece] = spot.pieces.as_slice() {
                            self.last_strip = None;
                            run_hit_action(
                                "HITS_ADD",
                                format!(r#"{{"piece":"{piece}","time":{},"now":true}}"#, spot.time),
                            );
                            return true;
                        }
                        self.last_strip = Some((x, now));
                        // Anywhere else on a piece's row: a marquee.
                        self.hit_marquee = Some(((x, y), (x, y)));
                        true
                    }
                }
            }
            UiEvent::PointerMove(e) => {
                let at = (f64::from(e.coords.client_x), f64::from(e.coords.client_y));
                if let Some((_, to)) = self.hit_marquee.as_mut() {
                    *to = at;
                    return true;
                }
                let Some(drag) = self.hit_drag.as_mut() else {
                    return false;
                };
                drag.x = at.0;
                true
            }
            UiEvent::PointerUp(e) | UiEvent::PointerCancel(e) => {
                if let Some((from, _)) = self.hit_marquee.take() {
                    let to = (f64::from(e.coords.client_x), f64::from(e.coords.client_y));
                    self.hit_selected = if matches!(event, UiEvent::PointerUp(_)) {
                        self.hits_within(from, to)
                    } else {
                        std::collections::HashSet::new()
                    };
                    return true;
                }
                let Some(drag) = self.hit_drag.take() else {
                    return false;
                };
                let x = if matches!(event, UiEvent::PointerUp(_)) {
                    f64::from(e.coords.client_x)
                } else {
                    drag.from_x
                };
                if (x - drag.from_x).abs() >= FLAG_SLOP {
                    let pps = self
                        .viewport(self.size.0, self.size.1)
                        .pps
                        .max(f64::EPSILON);
                    let ms = (x - drag.from_x) / pps * 1_000.0;
                    if drag.stretch {
                        // By its foot: stretched there, its neighbours kept.
                        run_hit_action(
                            "HITS_STRETCH",
                            format!(r#"{{"id":"{}","by_ms":{ms}}}"#, drag.id),
                        );
                    } else if drag.ids.len() > 1 && !drag.marker_only {
                        // The selection moved together, the kit with it.
                        let ids = serde_json::Value::from(drag.ids);
                        run_hit_action(
                            "HITS_PLACE",
                            format!(r#"{{"ids":{ids},"by_ms":{ms},"snap":false}}"#),
                        );
                    } else {
                        let id = if drag.marker_only {
                            "HITS_MOVE"
                        } else {
                            "HITS_PLACE"
                        };
                        run_hit_action(id, format!(r#"{{"id":"{}","by_ms":{ms}}}"#, drag.id));
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// A swipe comp ([`crate::swipe`]): a press with no modifier on a lane
    /// that is not the comp, on a row showing its lanes, followed; let go,
    /// the stretch comped onto the comp lane (`TRACK_COMP_FROM_LANE`) —
    /// on a shut folder's row every track under it with lanes — behind the
    /// window, as a model's comp is, the song read back after. A modifier
    /// leaves the press to the editor (select, move), as on any row.
    fn swipe_gesture(&mut self, event: &UiEvent) -> bool {
        match event {
            UiEvent::PointerDown(e)
                if e.button == MouseEventButton::Main && !self.pointing.borrow().tool_active() =>
            {
                // A finger on the lanes scrolls them.
                let held = mods(e.mods);
                if held.shift || held.ctrl || held.alt || e.is_finger() {
                    return false;
                }
                let (x, y) = (f64::from(e.coords.client_x), f64::from(e.coords.client_y));
                self.swipe = self.swipe_at(x, y);
                self.swipe.is_some()
            }
            UiEvent::PointerMove(e) => {
                if self.swipe.is_none() {
                    return false;
                }
                let view = self.viewport(self.size.0, self.size.1);
                let at = self.seconds_at(f64::from(e.coords.client_x), view);
                if let Some(swipe) = self.swipe.as_mut() {
                    swipe.to = at;
                }
                true
            }
            UiEvent::PointerUp(e) => {
                let Some(mut swipe) = self.swipe.take() else {
                    return false;
                };
                let view = self.viewport(self.size.0, self.size.1);
                swipe.to = self.seconds_at(f64::from(e.coords.client_x), view);
                if let Some(args) = swipe.comp_args(view.pps) {
                    run_comp(args);
                }
                true
            }
            UiEvent::PointerCancel(_) => self.swipe.take().is_some(),
            _ => false,
        }
    }

    /// The swipe a press at a point starts: on a lane of a row showing its
    /// lanes, not its comp, with nothing else there first — a panel
    /// control, an automation lane, an item's edge, fade or volume dial.
    fn swipe_at(&self, x: f64, y: f64) -> Option<crate::swipe::Swipe> {
        if self.spot_at(x, y).is_some() || self.lane_hit(x, y).is_some() {
            return None;
        }
        let (crate::hit::Target::Item {
            row,
            zone: crate::arrangement::ItemZone::Body,
            ..
        }
        | crate::hit::Target::Lane { row, .. }) = self.hit(x, y)?.target
        else {
            return None;
        };
        let (track, _) = self.rows.get(row)?;
        // The lanes as drawn: what the row shows is what is swiped on.
        let laned = self.scene.lanes_shown(row)?;
        let view = self.viewport(self.size.0, self.size.1);
        let zoom = if view.zoom_y > 0.0 { view.zoom_y } else { 1.0 };
        let content_y = (y - ruler::ruler_h() + view.scroll_y) / zoom;
        let (top, height) = self.scene.row_box(row)?;
        let slots = self.scene.lane_slots(row).to_vec();
        let lane = crate::swipe::lane_at(top, height, &slots, content_y)?;
        if !crate::swipe::starts_on(laned, lane) {
            return None;
        }
        let at = self.seconds_at(x, view);
        // A track's media-edit group swiped with it; a folder's tracks are
        // the comp's own to find.
        let with = if track.is_folder {
            Vec::new()
        } else {
            crate::lanes::grouped_tracks(&self.project.tracks, &track.guid)
        };
        Some(crate::swipe::Swipe {
            row,
            track: track.guid.clone(),
            with,
            lane,
            slots,
            called: crate::lanes::called(laned, lane),
            from: at,
            to: at,
        })
    }

    /// The kit's rows and the pieces each shows (`rows`' index, the
    /// piece's in `hits`), a row once per piece: a piece's folder, Sum,
    /// mics and trigger show that piece; a folder shut over several (the
    /// toms, the whole kit) shows them all; the cymbals and rooms none.
    fn piece_rows(
        &self,
        kit: std::ops::Range<usize>,
        hits: &crate::hit_overlay::Overlay,
    ) -> Vec<(usize, usize)> {
        let parents: std::collections::HashMap<&str, &str> = self
            .project
            .tracks
            .iter()
            .filter_map(|t| Some((t.guid.as_str(), t.parent_guid.as_deref()?)))
            .collect();
        let parent_of = |t: &str| parents.get(t).copied();
        kit.filter_map(|row| Some((row, &self.rows.get(row)?.0.guid)))
            .flat_map(|(row, guid)| {
                hits.pieces_under(guid, parent_of)
                    .into_iter()
                    .map(move |piece| (row, piece))
            })
            .collect()
    }

    /// Share the panel's Record-mode flag: the arm only while recording.
    #[must_use]
    pub fn with_recording(mut self, recording: Rc<Cell<bool>>) -> Self {
        self.recording = recording;
        self
    }

    /// Share the panel's tool state and window, so a tool can take the
    /// mouse and the pointer can show what a press would do.
    #[must_use]
    pub fn with_pointing(mut self, pointing: crate::tool::Shared) -> Self {
        self.pointing = pointing;
        self
    }

    /// Share the which-key popup's state and the zoom queue with the panel.
    #[must_use]
    pub fn with_view_links(
        mut self,
        which: crate::which_key::Shared,
        zooms: crate::zoom::Requests,
    ) -> Self {
        self.which = which;
        self.zooms = zooms;
        self
    }

    /// Let the widget plan its rows again (the visibility manager), from
    /// the session's planner, and share its content height with the panel.
    #[must_use]
    pub fn with_planner(
        mut self,
        planner: crate::studio::Planner,
        content_h: Rc<std::cell::Cell<f64>>,
    ) -> Self {
        let raw = song_as_left(&planner);
        self.replan = Some((planner, raw));
        content_h.set(self.content_h.get());
        self.content_h = content_h;
        self
    }

    /// Lay the rows out as `slug` (an overview); pressed again, or the
    /// other overview's key pressed in this one's, go back to the scene
    /// before. `false` with no planner.
    /// The stretch a drum key works on: the selected folded rows' span,
    /// else the time selection.
    fn drum_span(&self) -> Option<(f64, f64)> {
        let span = self.editor.selected_folds(&self.project).iter().fold(
            None,
            |span: Option<(f64, f64)>, (_, _, a, b)| {
                Some(span.map_or((*a, *b), |(f, t)| (f.min(*a), t.max(*b))))
            },
        );
        span.or_else(|| {
            self.editor
                .cursor
                .selection
                .filter(|s| s.is_meaningful())
                .map(|s| (s.start, s.end))
        })
    }

    /// Alt+Q: put the drums back as played over the selected folded rows'
    /// span or the time selection (`DRUMS_REVERT`) — the edit undone
    /// there, the hit markers kept to edit by hand, a no-quantize zone
    /// over it — off the frame, as [`Self::quantize_drums`] does.
    /// `hold`: a no-quantize zone over it (Alt+Q), so a quantize leaves it
    /// as played; without (Backspace), Q quantizes it again — to compare.
    fn revert_drums(&self, hold: bool) -> bool {
        self.revert_span(self.drum_span(), hold)
    }

    /// The stretch the selected hit markers span, as they play now (a
    /// hair either side, so the first and last are in it); `None` with
    /// none selected.
    fn selected_hits_span(&self) -> Option<(f64, f64)> {
        /// Either side of the first and the last, seconds.
        const AROUND: f64 = 0.004;
        let hits = crate::hit_overlay::current()?;
        let selected = &self.hit_selected;
        hits.marks
            .iter()
            .filter(|m| selected.contains(&m.id))
            .fold(None, |span: Option<(f64, f64)>, m| {
                Some(span.map_or((m.time, m.time), |(a, b)| (a.min(m.time), b.max(m.time))))
            })
            .map(|(a, b)| (a - AROUND, b + AROUND))
    }

    /// [`Self::revert_drums`] over `span`.
    fn revert_span(&self, span: Option<(f64, f64)>, hold: bool) -> bool {
        let Some((from, to)) = span else {
            *self
                .staged
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                Some("select the kit's folded rows, or a stretch of time, to put back as played");
            return true;
        };
        #[cfg(feature = "native")]
        {
            let Some(runtime) = crate::open::runtime() else {
                return false;
            };
            let said = std::sync::Arc::clone(&self.staged);
            runtime.spawn_blocking(move || {
                let args = format!(r#"{{"from":{from},"to":{to},"hold":{hold}}}"#);
                let out = crate::actions::list().run_json("DRUMS_REVERT", args.as_bytes());
                let text = match &out {
                    Ok(json) if String::from_utf8_lossy(json).contains(r#""ok":false"#) => {
                        "drums put back as played — the check found a problem (DRUMS_REVIEW)"
                    }
                    Ok(_) => "drums put back as played — the hit markers are there to edit",
                    Err(e) => {
                        tracing::warn!(revert.error = %e, "drums: putting back was refused");
                        "the drums could not be put back"
                    }
                };
                *said
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(text);
            });
            true
        }
        #[cfg(not(feature = "native"))]
        {
            let _ = (from, to);
            false
        }
    }

    /// `q`: QUANTIZE the kit over the selected folded rows' span, or the
    /// time selection — from its hit markers, with the settings the
    /// Quantize panel last applied (else the sticky ones) — off the
    /// frame; the slip it queues is applied here like any, and
    /// [`Self::settle_stage`] says how its check came out. With nothing
    /// to quantize over it says so. False in a page (no engine of its own
    /// to read the markers from).
    fn quantize_drums(&self) -> bool {
        let Some((from, to)) = self.drum_span() else {
            *self
                .staged
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                Some("select the kit's folded rows, or a stretch of time, to quantize");
            return true;
        };
        #[cfg(feature = "native")]
        {
            let Some(runtime) = crate::open::runtime() else {
                return false;
            };
            let said = std::sync::Arc::clone(&self.staged);
            let last = crate::quantize_panel::last();
            runtime.spawn_blocking(move || {
                let args = crate::quantize_panel::again_over(last.as_ref(), (from, to));
                let out = crate::actions::list().run_json("QUANTIZE", args.as_bytes());
                let text = match &out {
                    Ok(json) if String::from_utf8_lossy(json).contains(r#""ok":false"#) => {
                        "drums quantized — the check found hits lost or doubled (HITS_AUDIT)"
                    }
                    Ok(_) => "drums quantized — the check is clean",
                    Err(e) => {
                        tracing::warn!(quantize.error = %e, "drums: the quantize was refused");
                        "the drums could not be quantized — find the hits first"
                    }
                };
                *said
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(text);
            });
            true
        }
        #[cfg(not(feature = "native"))]
        {
            let _ = (from, to);
            false
        }
    }

    /// Slip-edit the kit by `moves` (`(hit, shift)`, microseconds): the
    /// drums folder's folded rows, each with the moves inside its span —
    /// the whole kit cut and moved as one ([`crate::arrange_edit::Editor::slip`]).
    /// False when the drums are not folded to a row here.
    /// The kit's takes an edit works on ([`crate::drum_edit::is_edited`]):
    /// every audio track under the top-level Drums folder — whatever is
    /// folded or open here, never its Process — or, `only` named, those
    /// tracks of it.
    fn kit_items(&self, only: &[String]) -> Vec<String> {
        crate::arrange_edit::kit_items(&self.project, only)
    }

    /// Take `from..to` out of the kit's edited takes — `tracks` of it, or
    /// all of it ([`crate::arrange_edit::Editor::remove_range`]).
    fn remove_range(&mut self, tracks: &[String], (from, to): (f64, f64)) -> bool {
        let items = self.kit_items(tracks);
        if items.is_empty() {
            return false;
        }
        let mut effects = Vec::new();
        crate::arrange_edit::Editor::remove_range(
            &mut self.project,
            &items,
            (from, to),
            "Remove from the drums",
            &mut effects,
        );
        tracing::info!(
            remove.takes = items.len(),
            remove.from = from,
            remove.to = to,
            remove.edits = effects.len(),
            "drums: a stretch removed"
        );
        self.settle(effects);
        self.replan_rows();
        crate::hit_overlay::refresh();
        self.dirty.set(true);
        true
    }

    fn slip_drums(&mut self, moves: &[(i64, i64)], cut: crate::arrange_edit::SlipCut) -> bool {
        // The kit's takes: one unit, cut and moved together.
        let items = self.kit_items(&[]);
        if items.is_empty() {
            return false;
        }
        let moves = crate::arrange_edit::seconds_of(moves);
        let started = std::time::Instant::now();
        // Where in its source each take started, before it is cut: every
        // slice of it shows its own part of the take's waveform.
        let taken: std::collections::HashSet<&str> = items.iter().map(String::as_str).collect();
        let offsets: std::collections::HashMap<String, f64> = self
            .project
            .items
            .values()
            .flatten()
            .filter(|i| taken.contains(i.guid.as_str()))
            .map(|i| (i.guid.clone(), i.start_offset.as_seconds()))
            .collect();
        let mut effects = Vec::new();
        crate::arrange_edit::Editor::slip_items(
            &mut self.project,
            &items,
            &moves,
            ("Slip edit drums", cut),
            &mut effects,
        );
        // A slice drawn out over a gap plays through its markers: drawn so.
        let warps: std::collections::HashMap<&str, &[(f64, f64)]> = effects
            .iter()
            .filter_map(|e| match e {
                crate::arrange_edit::Effect::Send(crate::engine::Edit::SetStretchMarkers(
                    guid,
                    markers,
                )) => Some((guid.as_str(), markers.as_slice())),
                _ => None,
            })
            .collect();
        let slices: Vec<SliceDrawn> = self
            .project
            .items
            .values()
            .flatten()
            .filter_map(|i| {
                let take = i.guid.split("-split@").next()?;
                let offset = *offsets.get(take)?;
                Some(SliceDrawn {
                    guid: i.guid.clone(),
                    take: take.to_owned(),
                    from: i.start_offset.as_seconds() - offset,
                    offset,
                    length: i.length.as_seconds(),
                    warp: warps.get(i.guid.as_str()).map(|m| m.to_vec()),
                })
            })
            .collect();
        draw_slices(&self.previews, items.clone(), slices);
        let (kit_takes, edits, cut_ms) = (items.len(), effects.len(), started.elapsed());
        self.settle(effects);
        let drawn_at = started.elapsed();
        // The kit's row folded again from the slices, so every cut is on
        // it (in Edit mode; elsewhere the seams are joined).
        self.replan_rows();
        tracing::info!(
            slip.takes = kit_takes,
            slip.moves = moves.len(),
            slip.edits = edits,
            slip.ms = cut_ms.as_millis(),
            slip.settle_ms = drawn_at.saturating_sub(cut_ms).as_millis(),
            slip.replan_ms = started.elapsed().saturating_sub(drawn_at).as_millis(),
            "drums slip-edited"
        );
        // The hits have moved: what was detected is not where they are.
        crate::hit_overlay::refresh();
        self.dirty.set(true);
        true
    }

    /// Shift+G: gain stage the session ([`crate::gain_stage::stage_live`]);
    /// Alt+G (`clear`): every staged item back at an item volume of 1. Off
    /// the frame; [`Self::settle_stage`] reads the result back.
    fn gain_stage(&self, clear: bool) -> bool {
        let out = std::sync::Arc::clone(&self.staged);
        let work = async move {
            let Some(daw) = daw::rpc::Daw::try_get() else {
                return;
            };
            let Ok(project) = daw.current_project().await else {
                return;
            };
            let said = if clear {
                match crate::gain_stage::clear_live(&project).await {
                    Ok(cleared) => {
                        tracing::info!(stage.cleared = cleared.len(), "gain staging cleared");
                        "gain staging cleared"
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "gain staging could not be cleared");
                        "gain staging could not be cleared"
                    }
                }
            } else {
                match crate::gain_stage::stage_live(&project).await {
                    Ok(report) => {
                        tracing::info!(
                            stage.staged = report.staged.len(),
                            stage.silent = report.silent.len(),
                            stage.silent_tracks = %report.silent.join(", "),
                            stage.levels = %report
                                .staged
                                .iter()
                                .map(|s| format!("{} {:.1}→{:+.1}", s.name, s.level, s.gain_db))
                                .collect::<Vec<_>>()
                                .join(", "),
                            "gain staged"
                        );
                        "gain staged"
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "gain staging failed");
                        "gain staging failed"
                    }
                }
            };
            *out.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(said);
        };
        #[cfg(feature = "native")]
        {
            let Some(runtime) = crate::open::runtime() else {
                return false;
            };
            runtime.spawn(work);
        }
        #[cfg(not(feature = "native"))]
        wasm_bindgen_futures::spawn_local(work);
        true
    }

    /// Whether a staging run has finished and is waiting to be shown.
    fn stage_ready(&self) -> bool {
        self.staged
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    }

    /// A finished staging run, shown: the session read back (the item
    /// volumes the waveforms are drawn through) and what it did said.
    fn settle_stage(&mut self) {
        let Some(said) = self
            .staged
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        else {
            return;
        };
        self.resync();
        self.notice = Some(crate::notice::Notice::new(said, None));
        self.dirty.set(true);
    }

    fn toggle_scene(&mut self, slug: &'static str) -> bool {
        let Some((planner, raw)) = self.replan.as_mut() else {
            return false;
        };
        let now = if planner.scene == Some(slug) {
            self.scene_before.take().unwrap_or(planner.scene)
        } else {
            if self.scene_before.is_none() {
                self.scene_before = Some(planner.scene);
            }
            Some(slug)
        };
        planner.scene = now;
        let name: &'static str = now
            .and_then(dynamic_template::scenes::scene)
            .map_or("Default view", |s| s.name.as_str());
        let (project, rows) = planner.plan(&raw.borrow());
        self.notice = Some(crate::notice::Notice::new(name, None));
        self.restructure((*project.0).clone(), rows.as_slice().to_vec());
        true
    }

    /// The visibility manager: show or hide a group of tracks, then plan
    /// the rows again and tell the engine. `false` when nothing changed
    /// (no planner, or a group no track is in).
    fn visibility(&mut self, change: &crate::keys::Visibility) -> bool {
        use crate::keys::Visibility;
        let Some((planner, shared)) = self.replan.as_ref() else {
            return false;
        };
        let (planner, shared) = (planner.clone(), Rc::clone(shared));
        let mut guard = shared.borrow_mut();
        let raw = &mut *guard;
        let names: Vec<String> = raw.tracks.iter().map(|t| t.name.clone()).collect();
        let groups = match dynamic_template::visibility::groups(names) {
            Ok(groups) => groups,
            Err(e) => {
                tracing::warn!(error = %e, "visibility: the tracks could not be grouped");
                return false;
            }
        };
        let in_group = |key: &str| -> std::collections::HashSet<String> {
            groups
                .get(key)
                .map(|n| n.iter().cloned().collect())
                .unwrap_or_default()
        };
        let (targets, show): (std::collections::HashSet<String>, bool) = match change {
            Visibility::ShowAll => (raw.tracks.iter().map(|t| t.name.clone()).collect(), true),
            Visibility::HideAll => (groups.values().flatten().cloned().collect(), false),
            Visibility::Toggle(group) => {
                let targets = in_group(&dynamic_template::visibility::normalize_key(group));
                // Shown again only when all of it is hidden; any of it on
                // screen, and the toggle hides it — REAPER's rule.
                let any_shown = raw
                    .tracks
                    .iter()
                    .any(|t| targets.contains(&t.name) && t.visible_in_tcp);
                (targets, !any_shown)
            }
        };
        if targets.is_empty() {
            self.notice = Some(crate::notice::Notice::new("no tracks in that group", None));
            return true;
        }
        let mut changed = Vec::new();
        for track in &mut raw.tracks {
            if targets.contains(&track.name) && track.visible_in_tcp != show {
                track.visible_in_tcp = show;
                track.visible_in_mixer = show;
                changed.push(track.guid.clone());
            }
        }
        if changed.is_empty() {
            return false;
        }
        let (project, rows) = planner.plan(raw);
        // Let go before the rows are taken: publishing them reads it.
        drop(guard);
        for guid in changed {
            self.edits
                .borrow_mut()
                .push(crate::engine::Edit::SetVisibility(guid, show, show));
        }
        self.restructure((*project.0).clone(), rows.as_slice().to_vec());
        true
    }

    /// Read the session back from the engine and draw that: what a change
    /// made there rather than here (a toolbar insert, an edited chart laid
    /// over the song) needs. MIDI items the arrangement has not seen — a
    /// chart's new chords — get their notes read first, so they draw
    /// filled rather than empty.
    #[cfg(feature = "native")]
    fn resync(&mut self) {
        let Some(fresh) = crate::studio::fetch() else {
            tracing::warn!("the session could not be read back from the engine");
            return;
        };
        let unread = unread_midi(&fresh, &self.previews);
        if !unread.is_empty() {
            self.previews.fill_blocking(unread);
        }
        // Audio items made since the song opened (a lane's copy, a slice
        // cut elsewhere): their peaks read behind the window, each drawn
        // as it arrives, rather than left blank for good.
        // And those whose sound changed under them (leveled: a new volume
        // curve), read again in place — drawn as before until they are.
        let mut waves = unread_audio(&fresh, &self.previews);
        waves.extend(crate::midi::take_stale());
        if !waves.is_empty() {
            let previews = self.previews.clone();
            crate::wave_pool::spawn(move || previews.fill_waves_blocking(waves));
        }
        self.adopt(fresh);
    }

    /// [`Self::resync`] in a page, which cannot block on the engine: the
    /// read-back (and any new MIDI's notes) runs on the page's event loop,
    /// and a later paint adopts it.
    #[cfg(not(feature = "native"))]
    fn resync(&mut self) {
        let slot = Rc::clone(&self.read_back);
        let previews = self.previews.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let Some(fresh) = daw_ui::studio::project::fetch().await else {
                tracing::warn!("the session could not be read back from the engine");
                return;
            };
            let unread = unread_midi(&fresh, &previews);
            if !unread.is_empty() {
                previews.fill(unread).await;
            }
            // The lanes, read through the facade before the rows are
            // planned around them.
            let (lanes, takes) = crate::automation::read_remote(&fresh.tracks).await;
            crate::automation::adopt_remote(lanes, takes);
            *slot.borrow_mut() = Some(fresh);
        });
    }

    /// Draw a session read back from the engine: its rows planned again,
    /// and everything they are drawn from.
    fn adopt(&mut self, fresh: daw_ui::studio::project::Project) {
        let Some((planner, raw)) = self.replan.as_mut() else {
            return;
        };
        *raw.borrow_mut() = fresh;
        let (project, rows) = planner.plan(&raw.borrow());
        self.restructure((*project.0).clone(), rows.as_slice().to_vec());
        // The engine changed the session: a slice moved is a hit moved.
        crate::hit_overlay::refresh();
    }

    /// The rows planned again from the tracks they are planned from — a
    /// folder opened or shut.
    fn replan_rows(&mut self) {
        if let Some((planner, raw)) = self.replan.as_ref() {
            let (project, rows) = planner.plan(&raw.borrow());
            self.restructure((*project.0).clone(), rows.as_slice().to_vec());
        }
    }

    /// Take a new set of rows (tracks shown or hidden): everything the
    /// rows are drawn and hit-tested from, then the recording.
    fn restructure(
        &mut self,
        project: daw_ui::studio::project::Project,
        rows: Vec<(daw_proto::Track, u32)>,
    ) {
        crate::tcp::fit_names(&self.font, &rows, self.layout);
        self.tracks = rows.iter().map(|(t, _)| t.clone()).collect();
        self.map = crate::plan::Rows::of(&rows, &self.tracks);
        self.content_h.set(
            rows.iter()
                .map(|(track, _)| self.layout.slot_of(track))
                .sum(),
        );
        self.rows = rows;
        self.project = project;
        self.hovered_item = None;
        self.turning = None;
        self.pointer = crate::pointer::Pointer::default();
        self.recut();
        self.publish_rows();
    }

    /// Link to a docked mixer: the rows go to it (now, and on every
    /// change), its edits come back here.
    #[must_use]
    pub fn with_mixer(mut self, links: MixerLinks) -> Self {
        self.mixer = Some(links);
        self.publish_rows();
        self
    }

    /// The mixer's strips, with the generation bumped: the scene's own
    /// for the mixer ([`crate::studio::Planner::plan_strips`]) — not this
    /// panel's rows, since a scene hides and folds per surface — each a
    /// track as this panel has it where it shows one. With no planner,
    /// this panel's rows.
    fn publish_rows(&self) {
        let Some(links) = &self.mixer else { return };
        let strips = self.replan.as_ref().map_or_else(
            || self.rows.clone(),
            |(planner, raw)| {
                let here: std::collections::HashMap<&str, &daw_proto::Track> = self
                    .rows
                    .iter()
                    .map(|(track, _)| (track.guid.as_str(), track))
                    .collect();
                planner
                    .plan_strips(&raw.borrow())
                    .as_slice()
                    .iter()
                    .map(|(track, depth)| {
                        let track = here
                            .get(track.guid.as_str())
                            .map_or_else(|| track.clone(), |shown| (*shown).clone());
                        (track, *depth)
                    })
                    .collect()
            },
        );
        let mut shared = links.rows.borrow_mut();
        shared.0 = shared.0.wrapping_add(1);
        shared.1 = strips;
    }

    /// The mixer's edits since the last frame, applied here too, and the
    /// keys it passed on, handled as if they had been pressed here.
    fn echoes(&mut self) {
        let Some(links) = &self.mixer else { return };
        let echoed: Vec<crate::engine::Edit> = links.echo.borrow_mut().drain(..).collect();
        let keys: Vec<crate::mixer_panel::Key> = links.keys.borrow_mut().drain(..).collect();
        for edit in &echoed {
            self.assume(edit);
        }
        for key in &keys {
            match key {
                crate::mixer_panel::Key::Down(e) => {
                    self.typed(e);
                }
                crate::mixer_panel::Key::Up(e) => {
                    self.released(e);
                }
            }
        }
    }

    /// Tell the popup what the keyboard has pending.
    fn publish_which_key(&self) {
        *self.which.borrow_mut() = self.keys.which_key();
    }

    /// A zoom the keys asked for, in session units, for the panel.
    /// `None` when there is nothing to frame (no track selected for
    /// `z t`, say).
    fn zoom_request(&self, command: crate::zoom::Command) -> Option<crate::zoom::Request> {
        use crate::zoom::{Command, Request};
        let span = |boxes: &mut dyn Iterator<Item = (f64, f64)>| {
            boxes.fold(None, |acc: Option<(f64, f64)>, (a, b)| {
                Some(acc.map_or((a, b), |(lo, hi)| (lo.min(a), hi.max(b))))
            })
        };
        let rows_of = |rows: &mut dyn Iterator<Item = usize>| {
            span(&mut rows.filter_map(|row| self.scene.row_box(row).map(|(top, h)| (top, top + h))))
        };
        let selected_rows =
            || rows_of(&mut (0..self.rows.len()).filter(|&row| self.rows[row].0.selected));
        let selected_items = || {
            (0..self.scene.items())
                .filter_map(|i| self.scene.item(i))
                .filter(|item| self.editor.selected.contains(&item.guid))
                .collect::<Vec<_>>()
        };
        let time_selection = self
            .editor
            .cursor
            .selection
            .filter(|s| s.is_meaningful())
            .map(|s| (s.start, s.end));
        let items_time = || span(&mut selected_items().into_iter().map(|item| (item.x0, item.x1)));
        let frame = |time: Option<(f64, f64)>, rows: Option<(f64, f64)>, toggle| {
            (time.is_some() || rows.is_some()).then_some(Request::Frame { time, rows, toggle })
        };
        match command {
            Command::ToggleTracks => frame(time_selection, selected_rows(), true),
            Command::FitTracks => frame(None, rows_of(&mut (0..self.rows.len())), false),
            Command::Project => frame(Some((0.0, self.project.length_secs.max(1.0))), None, false),
            Command::Selection => frame(time_selection.or_else(items_time), None, false),
            Command::ToggleSelection => frame(time_selection.or_else(items_time), None, true),
            Command::Items => {
                let items = selected_items();
                let rows = rows_of(&mut items.iter().map(|item| item.row));
                frame(items_time(), rows, false)
            }
            Command::Back => Some(Request::Back),
            Command::Forward => Some(Request::Forward),
            Command::Step { vertical, inward } => Some(Request::Scale {
                vertical,
                by: if inward { 1.25 } else { 0.8 },
            }),
        }
    }

    /// The least zoom the opening view may have on each axis (across,
    /// down).
    ///
    /// Fitting a forty-track session into an iPad's docked arrangement
    /// made every row a hairline and every item a sliver: the whole song
    /// on screen, and none of it usable. So on a finger's screen the rows
    /// open to share the lanes' height — every row shown (the folders
    /// folded) on screen at once, the whole picture — but none under
    /// [`OPEN_ROW_TOUCH`]; on a compact panel at least
    /// [`OPEN_ROW_COMPACT`], and either way a second at least
    /// [`OPEN_PPS`] wide; the rest is a scroll away. The full DAW view on
    /// a desktop fits as it always has.
    fn opening_floor(&self) -> (f64, f64) {
        let ui = self.ui.get().max(f64::EPSILON);
        let row = if self.touch {
            // Every row down the lanes: their frame is the widget less the
            // ruler and the scrollbar, in on-screen pixels, shared by the
            // rows shown. Never under a name's height.
            let lanes = (self.size.1 - ruler::ruler_h() - crate::panel::BAR).max(0.0) * ui;
            #[expect(clippy::cast_precision_loss, reason = "a row count")]
            let shown = self.rows.len().max(1) as f64;
            (lanes / shown).max(OPEN_ROW_TOUCH)
        } else if self.compact.get() {
            OPEN_ROW_COMPACT
        } else {
            return (0.0, 0.0);
        };
        // The floors are on-screen sizes, and the view is laid out at the
        // touch zoom's fraction of them (`ui`). The row that must reach `row` is
        // the typical one, the median: a session's plan sizes its rows
        // anywhere from a sliver to a working track, and neither the
        // smallest nor the default stands for what is on screen.
        let mut heights: Vec<f64> = self
            .rows
            .iter()
            .map(|(track, _)| self.layout.height_of(track.height))
            .collect();
        heights.sort_by(f64::total_cmp);
        let typical = heights
            .get(heights.len() / 2)
            .copied()
            .unwrap_or(crate::layout::CONTROL_ROW)
            .max(1.0);
        (OPEN_PPS / ui / crate::studio::PPS, row / ui / typical)
    }

    /// What the widget wants done, for the window to drain.
    #[must_use]
    pub fn edits(&self) -> Rc<RefCell<Vec<crate::engine::Edit>>> {
        Rc::clone(&self.edits)
    }

    /// Throw the recorded controls away, because a value they draw has
    /// changed.
    ///
    /// Not worked out from the data: finding the one knob that moved
    /// would mean diffing forty tracks every frame, which costs more
    /// than the redraw it saves. Whoever changed the value knows, and
    /// says so.
    pub fn values_changed(&mut self) {
        self.controls = None;
    }

    /// What the last frame replayed and submitted.
    #[must_use]
    pub fn drawn(&self) -> Rc<RefCell<Drawn>> {
        Rc::clone(&self.drawn)
    }
}

impl ArrangementWidget {
    /// The viewport the widget is showing, at a size.
    ///
    /// The same four numbers `paint` works from, so a hit and a draw
    /// cannot disagree about where anything is.
    /// The control a turn is on taken hold of (`held`) or let go — for the
    /// automation that records while it is held.
    fn touch(&self, turn: &Turn, held: bool) {
        let which = crate::engine::Touched::of(
            turn.spot.control == crate::row::Control::Volume,
            turn.spot.control == crate::row::Control::Pan,
        );
        if let Some(which) = which {
            self.edits.borrow_mut().push(crate::engine::Edit::Touch(
                turn.was.guid.clone(),
                which,
                held,
            ));
        }
    }

    /// Open the automation menu under row `row`'s automation button.
    #[cfg(feature = "native")]
    fn open_automation_menu(&mut self, row: usize) {
        let Some((track, _)) = self.rows.get(row) else {
            return;
        };
        let view = self.viewport(self.size.0, self.size.1);
        let below = ruler::ruler_h() - view.scroll_y;
        let Some((top, height)) = self.scene.row_band(row, view) else {
            return;
        };
        let at = crate::row::automation_button(top + below, height, self.scene.tcp).map_or_else(
            || (self.scene.tcp.width() * 0.2, top + below + height),
            |r| (r.x0, r.y1 + 2.0),
        );
        let choices = crate::automation_menu::top(&track.guid);
        self.menu = Some(crate::popup::Popup::open(
            &self.font, choices, at, self.size,
        ));
        self.dirty.set(true);
    }

    /// A press while the automation menu is open: a choice taken (the menu
    /// going on to its next list, or closing), or the menu closed.
    #[cfg(feature = "native")]
    fn menu_pressed(&mut self, x: f64, y: f64) -> bool {
        let Some(menu) = self.menu.take() else {
            return false;
        };
        if let Some(next) = menu
            .take(x, y)
            .and_then(|pick| crate::automation_menu::act(&pick))
        {
            let bounds = menu.bounds();
            self.menu = Some(crate::popup::Popup::open(
                &self.font,
                next,
                (bounds.x0, bounds.y0),
                self.size,
            ));
        }
        self.dirty.set(true);
        true
    }

    /// The take volume curve under a point, if any (nothing over the
    /// ruler).
    fn take_hit(&self, x: f64, y: f64) -> Option<crate::automation_draw::TakeHit> {
        if y < ruler::ruler_h() {
            return None;
        }
        let view = self.viewport(self.size.0, self.size.1);
        let below = ruler::ruler_h() - view.scroll_y;
        crate::automation_draw::take_hit(&self.scene, view, below, x, y)
    }

    /// A press on an item's take volume curve: its point dragged (Alt:
    /// deleted), or on its line a point made there and dragged. `false`
    /// when the press is not on one.
    fn take_pressed(&mut self, x: f64, y: f64, alt: bool) -> bool {
        use crate::engine::Edit;
        let Some(hit) = self.take_hit(x, y) else {
            return false;
        };
        let Some(curve) = crate::automation::take_curve(&hit.item) else {
            return false;
        };
        let length = self
            .scene
            .item(hit.index)
            .map_or(0.0, |item| item.x1 - item.x0);
        let begin = Edit::BeginUndo(TAKE_LABEL.to_owned());
        let (points, index) = match hit.point {
            Some(k) if alt => {
                let points = crate::automation::without_point(&curve.points, k);
                crate::automation::set_take_curve(&hit.item, points.clone());
                self.edits.borrow_mut().extend([
                    begin,
                    Edit::SetTakeVolume(hit.item, points),
                    Edit::EndUndo(TAKE_LABEL.to_owned()),
                ]);
                self.dirty.set(true);
                return true;
            }
            Some(k) => (None, k),
            None => {
                let (points, k) = crate::automation::with_point(
                    &curve.points,
                    None,
                    hit.t.clamp(0.0, length),
                    hit.gain,
                    daw_proto::automation::EnvelopeShape::Linear,
                );
                (Some(points), k)
            }
        };
        let mut edits = vec![begin];
        if let Some(points) = points {
            crate::automation::set_take_curve(&hit.item, points.clone());
            edits.push(Edit::SetTakeVolume(hit.item.clone(), points));
        }
        self.edits.borrow_mut().extend(edits);
        self.take_drag = Some(TakeDrag {
            item: hit.item,
            index,
            left: hit.left,
            top: hit.top,
            height: hit.height,
            length,
        });
        self.take_lit = Some((hit.index, Some(index)));
        self.dirty.set(true);
        true
    }

    /// The take volume point being dragged, moved to `(x, y)`.
    fn take_moved(&mut self, x: f64, y: f64) {
        let view = self.viewport(self.size.0, self.size.1);
        let Some(drag) = self.take_drag.as_mut() else {
            return;
        };
        let Some(curve) = crate::automation::take_curve(&drag.item) else {
            return;
        };
        let t = ((x - drag.left) / view.pps.max(1e-9)).clamp(0.0, drag.length);
        let gain = crate::automation_draw::take_gain_at(drag.top, drag.height, y);
        let (points, at) = crate::automation::with_point(
            &curve.points,
            Some(drag.index),
            t,
            gain,
            daw_proto::automation::EnvelopeShape::Linear,
        );
        drag.index = at;
        crate::automation::set_take_curve(&drag.item, points.clone());
        self.edits
            .borrow_mut()
            .push(crate::engine::Edit::SetTakeVolume(
                drag.item.clone(),
                points,
            ));
        self.dirty.set(true);
    }

    /// The automation lane under a point, and what in it — nothing over
    /// the ruler, which covers the lanes scrolled under it.
    fn lane_hit(&self, x: f64, y: f64) -> Option<crate::automation_draw::Hit> {
        if y < ruler::ruler_h() {
            return None;
        }
        let view = self.viewport(self.size.0, self.size.1);
        let below = ruler::ruler_h() - view.scroll_y;
        crate::automation_draw::hit(&self.scene, &self.tracks, view, below, x, y)
    }

    /// What to light in the lanes for the pointer at a point.
    fn lane_lit_at(&self, x: f64, y: f64) -> crate::automation_draw::Lit {
        use crate::automation_draw::{Lit, Spot};
        let Some(hit) = self.lane_hit(x, y) else {
            return Lit::default();
        };
        let lane = Some((
            hit.placed.lane.track.clone(),
            hit.placed.lane.target.clone(),
        ));
        match hit.spot {
            Spot::Body {
                t,
                point,
                item,
                item_point,
                ..
            } => Lit {
                lane,
                point,
                item,
                item_point,
                t: Some(t),
            },
            Spot::ItemBar { item, .. } => Lit {
                lane,
                item: Some(item),
                ..Lit::default()
            },
            _ => Lit {
                lane,
                ..Lit::default()
            },
        }
    }

    fn viewport(&self, width: f64, height: f64) -> Viewport {
        let at = *self.view.borrow();
        Viewport {
            scroll_x: at.scroll_x,
            scroll_y: at.scroll_y,
            pps: self.pps * at.zoom_x,
            zoom_y: at.zoom_y,
            width,
            height,
            panel_w: self.scene.tcp.width(),
        }
    }

    /// The panel control under a point in the widget's own coordinates.
    ///
    /// The panel starts at the widget's left edge and the lanes below
    /// the ruler, so the only conversion is the ruler's height — the
    /// same offset `paint` puts in its transforms.
    fn spot_at(&self, x: f64, y: f64) -> Option<crate::pointer::RowSpot> {
        let view = self.viewport(self.size.0, self.size.1);
        let below = y - ruler::ruler_h();
        if below < 0.0 {
            return None;
        }
        self.scene
            .row_spot_at(view, &self.rows, &self.tracks, x, below + view.scroll_y)
            .map(|(row, control)| crate::pointer::RowSpot { row, control })
    }

    /// What a click on a control means, as an edit to the session.
    ///
    /// The widget knows a click landed on the mute button of row 12. It
    /// deliberately does not know what a mute IS — the queue goes to
    /// the window, which owns the engine.
    fn act(&mut self, spot: crate::pointer::RowSpot) {
        use crate::engine::Edit;
        use crate::row::Control as C;
        let Some((track, _)) = self.rows.get(spot.row) else {
            return;
        };
        let guid = track.guid.clone();
        // A double-click on a name edits it. The single click that
        // preceded it selected the track, which is what you wanted on
        // the way here anyway.
        if spot.control == C::Name {
            let now = web_time::Instant::now();
            let again = self.last_name.is_some_and(|(row, when)| {
                row == spot.row && now.saturating_duration_since(when) <= crate::gesture::DOUBLE
            });
            if again {
                self.last_name = None;
                self.renaming = Some(crate::rename::Rename::new(
                    crate::rename::Surface::Arrange,
                    spot.row,
                    crate::rename::What::Track(guid),
                    &track.name,
                ));
                return;
            }
            self.last_name = Some((spot.row, now));
        }
        let edit = match spot.control {
            C::Mute => Edit::ToggleMute(guid),
            C::Solo => Edit::ToggleSolo(guid),
            C::RecArm => Edit::ToggleArm(guid),
            C::Phase => Edit::SetPhase(guid, !track.phase_inverted),
            // The `+`, or the lane's name: every lane shown, to hear and comp
            // from — on a folder its own row's and every track's under it —
            // or only the one that plays again.
            C::LaneShow | C::LaneName => {
                let open = !crate::lanes::opened(track);
                let under =
                    crate::lanes::speaks_for(&self.tracks, track, self.scene.lane_tracks(spot.row));
                let _ =
                    crate::lanes::set_expanded(&crate::lanes::opened_by(track, &under), Some(open));
                return;
            }
            C::LanePrev | C::LaneNext => {
                // What the line speaks for: the track, or a folder's tracks
                // (shut, the ones the scene recorded under it).
                let shown: Vec<daw_proto::Track> =
                    crate::lanes::speaks_for(&self.tracks, track, self.scene.lane_tracks(spot.row))
                        .into_iter()
                        .cloned()
                        .collect();
                let Some(first) = shown.first() else { return };
                let Some(mask) = crate::lanes::stepped(
                    first,
                    self.project
                        .items
                        .get(&first.guid)
                        .map_or(&[][..], Vec::as_slice),
                    spot.control == C::LaneNext,
                ) else {
                    return;
                };
                // The lane by name, on each: the same comp on every mic of
                // a kit, and on their media-edit group while grouping is on.
                let lane = (0..64)
                    .find(|l| mask == 1_u64 << l)
                    .map(|l| {
                        crate::lanes::label(&daw_proto::Track {
                            lane_play_mask: 1_u64 << l,
                            ..first.clone()
                        })
                    })
                    .unwrap_or_default();
                let mut targets: Vec<String> = Vec::new();
                for t in &shown {
                    for g in std::iter::once(t.guid.clone())
                        .chain(crate::lanes::grouped_tracks(&self.tracks, &t.guid))
                    {
                        if !targets.contains(&g) {
                            targets.push(g);
                        }
                    }
                }
                for g in targets {
                    let Some(t) = self.tracks.iter().chain(&shown).find(|t| t.guid == g) else {
                        continue;
                    };
                    let Some(own) = crate::lanes::mask_named(t, &lane) else {
                        continue;
                    };
                    let edit = Edit::SetLanePlays(g, own);
                    self.assume(&edit);
                    self.edits.borrow_mut().push(edit);
                }
                return;
            }
            C::Name => Edit::Select(guid),
            // A fold is an edit to the VIEW and not to a track: the
            // window's folds (`crate::folds`), which plan the rows again.
            C::Folder => {
                // Shut, a click opens it; open, it shuts.
                let open = !crate::folds::shut(&self.rows, spot.row);
                crate::folds::toggle(&guid, open);
                return;
            }
            // What the track can automate, to show in lanes.
            C::Automation => {
                #[cfg(feature = "native")]
                self.open_automation_menu(spot.row);
                return;
            }
            // The rack and the routing panel are surfaces this widget
            // does not own yet. Left alone rather than guessed at.
            C::Fx | C::Routing | C::Volume | C::Pan => return,
        };
        // Shown before it is true.
        //
        // The engine is a channel and a worker thread, and the state
        // comes back through a subscription some frames later. Waiting
        // for that would make every button feel broken — a mute that
        // lights up two frames after the click reads as a missed click,
        // and the second click un-does the first. So the widget moves
        // its own copy now and the engine makes it so; when the real
        // event arrives it agrees, and if the engine refuses, the next
        // update corrects it.
        self.assume(&edit);
        self.edits.borrow_mut().push(edit);
    }

    /// Where a point in the widget falls, as the window's hit test
    /// sees it.
    ///
    /// The widget's coordinates start where the rails end, which is
    /// exactly the offset `hit::arrangement` expects to subtract — so
    /// the tested hit test is reused rather than a second one written
    /// that could disagree with it.
    fn hit(&self, x: f64, y: f64) -> Option<crate::hit::Hit> {
        let view = self.viewport(self.size.0, self.size.1);
        Some(crate::hit::arrangement(
            &self.scene,
            view,
            0,
            &self.sections,
            &self.markers,
            x + crate::rails::SIDE,
            y + crate::rails::TOP,
        ))
    }

    /// The item under a point, for the fade handles.
    fn item_under(&self, x: f64, y: f64) -> Option<usize> {
        match self.hit(x, y)?.target {
            crate::hit::Target::Item { index, .. } => Some(index),
            _ => None,
        }
    }

    /// The play cursor to the time under `x`, as a finger drags its
    /// handle.
    fn scrub(&self, x: f64) {
        let view = self.viewport(self.size.0, self.size.1);
        crate::engine::transport(crate::engine::Move::Seek, self.seconds_at(x, view));
    }

    /// The time under an x in the widget's coordinates.
    fn seconds_at(&self, x: f64, view: Viewport) -> f64 {
        ((x - self.scene.tcp.width() + view.scroll_x) / view.pps.max(f64::EPSILON)).max(0.0)
    }

    /// A key that is not going into a text field: what it is bound to,
    /// done.
    ///
    /// `true` when something happened, which is what tells the shell to
    /// draw again — a key that changed nothing must not cost a frame.
    fn acted(&mut self, event: &blitz_traits::events::BlitzKeyEvent) -> bool {
        use blitz_traits::events::Key;
        // `keyboard_types::Key` is flat: a character carries its text
        // and everything else displays as the W3C name the profile
        // writes ("Delete", "ArrowLeft"). `key_code` wants them apart.
        let named;
        let (named, text) = match &event.key {
            Key::Character(text) => (None, Some(text.as_str())),
            other => {
                named = other.to_string();
                (Some(named.as_str()), None)
            }
        };
        let held = mods(event.modifiers);
        // The window's own: which words the items wear. Not in the
        // keybind profile because REAPER has no such view to bind — and
        // ctrl+shift+K is free there, so it shadows nothing.
        if held.ctrl && held.shift && text.is_some_and(|t| t.eq_ignore_ascii_case("k")) {
            self.scene.lettering = self.scene.lettering.next();
            return true;
        }
        // And whether the ruler carries the CHORDS lane. Ctrl+shift+J is
        // as free in the profile as K.
        if held.ctrl && held.shift && text.is_some_and(|t| t.eq_ignore_ascii_case("j")) {
            ruler::show_chords(!ruler::chords_shown());
            return true;
        }
        let Some(code) = crate::keys::key_code(named, text) else {
            return false;
        };
        // Escape abandons a half-typed sequence, and does only that.
        if code == input::KeyCode::Escape && self.keys.is_pending() {
            self.keys.cancel();
            self.publish_which_key();
            return true;
        }
        // A held key repeats. Inside a sequence a repeat is not a second
        // press: holding `z` is the prefix (and the zoom tool), not `z z`.
        if event.is_auto_repeating && self.keys.is_pending() {
            return true;
        }
        let actions = self.keys.press(
            code,
            input::Modifiers {
                ctrl: held.ctrl,
                alt: held.alt,
                shift: held.shift,
                meta: false,
            },
        );
        self.publish_which_key();
        if actions.is_empty() {
            // Consumed, when it opened or extended a sequence: the popup
            // changed, and the key must not also do anything else.
            return self.keys.is_pending();
        }
        // A key is one more caller of the Action List: what it asks for
        // runs there, and comes back here to be done — the path a model's
        // action takes too.
        #[cfg(feature = "native")]
        let actions = crate::actions::through_list(actions);
        self.apply_actions(actions)
    }

    /// The window's actions carried out: what a key asked for, or what
    /// was run on the Action List (`crate::actions`). Whether any did
    /// anything.
    fn apply_actions(&mut self, actions: Vec<crate::keys::Action>) -> bool {
        let mut effects = Vec::new();
        let mut handled = false;
        let mut tracks = self.tracks.clone();
        let bpm = self.scene.bpm;
        for action in actions {
            if let crate::keys::Action::View(command) = action {
                if let Some(request) = self.zoom_request(command) {
                    self.zooms.borrow_mut().push(request);
                }
                handled = true;
                continue;
            }
            if let crate::keys::Action::Visibility(change) = &action {
                handled |= self.visibility(change);
                continue;
            }
            if let crate::keys::Action::ToggleScene(slug) = action {
                handled |= self.toggle_scene(slug);
                continue;
            }
            if action == crate::keys::Action::QuantizeDrums {
                handled |= self.quantize_drums();
                continue;
            }
            if action == crate::keys::Action::RevertDrums {
                handled |= self.revert_drums(true);
                continue;
            }
            // Editing, Backspace or Delete with hit markers selected puts
            // them back as played — the stretch from the first to the last,
            // nothing held: Q over them puts the quantize back, to compare.
            if action == crate::keys::Action::DeleteSelectedItems
                && self.editing.get()
                && let Some(span) = self.selected_hits_span()
            {
                handled |= self.revert_span(Some(span), false);
                self.hit_selected.clear();
                continue;
            }
            // Editing, Backspace with a stretch of time selected takes the
            // drum edit there off — the kit as played, nothing held — so Q
            // over the same stretch puts it back: the two to compare.
            if action == crate::keys::Action::DeleteSelectedItems
                && self.editing.get()
                && self.editor.cursor.selection.is_some()
            {
                handled |= self.revert_drums(false);
                continue;
            }
            if let crate::keys::Action::Frame { time, tracks } = &action {
                let seconds = |us: i64| num_traits::ToPrimitive::to_f64(&us).unwrap_or(0.0) / 1e6;
                self.zooms.borrow_mut().push(crate::zoom::Request::Frame {
                    time: time.map(|(from, to)| (seconds(from), seconds(to))),
                    rows: self.rows_of(tracks),
                    toggle: false,
                });
                handled = true;
                continue;
            }
            if let crate::keys::Action::RemoveRange {
                tracks,
                from_us,
                to_us,
            } = &action
            {
                let seconds = |us: i64| num_traits::ToPrimitive::to_f64(&us).unwrap_or(0.0) / 1e6;
                handled |= self.remove_range(tracks, (seconds(*from_us), seconds(*to_us)));
                continue;
            }
            if let crate::keys::Action::SlipDrums {
                moves,
                pre_roll_us,
                crossfade_us,
                stretch_over_us,
                attacks_us,
            } = &action
            {
                let seconds = |us: i64| num_traits::ToPrimitive::to_f64(&us).unwrap_or(0.0) / 1e6;
                let cut = crate::arrange_edit::SlipCut {
                    pre_roll: seconds(*pre_roll_us),
                    crossfade: seconds(*crossfade_us),
                    stretch_over: (*stretch_over_us > 0).then(|| seconds(*stretch_over_us)),
                    attacks: attacks_us.iter().map(|us| seconds(*us)).collect(),
                };
                handled |= self.slip_drums(moves, cut);
                continue;
            }
            if let crate::keys::Action::StretchDrums { edits } = &action {
                let seconds = |us: i64| num_traits::ToPrimitive::to_f64(&us).unwrap_or(0.0) / 1e6;
                let edits: Vec<(String, crate::arrange_edit::Warped)> = edits
                    .iter()
                    .map(|(guid, at, length, markers)| {
                        (
                            guid.clone(),
                            crate::arrange_edit::Warped {
                                position: seconds(*at),
                                length: seconds(*length),
                                markers: markers
                                    .iter()
                                    .map(|(p, s)| (seconds(*p), seconds(*s)))
                                    .collect(),
                            },
                        )
                    })
                    .collect();
                let mut effects = Vec::new();
                crate::arrange_edit::Editor::stretch_items(
                    &mut self.project,
                    &edits,
                    "Stretch a drum hit",
                    &mut effects,
                );
                self.settle(effects);
                handled = true;
                continue;
            }
            if let crate::keys::Action::GainStage { clear } = action {
                handled |= self.gain_stage(clear);
                continue;
            }
            if action == crate::keys::Action::ToggleMixer {
                if let Some(links) = &self.mixer {
                    links.toggle.set(true);
                    handled = true;
                }
                continue;
            }
            handled |= self.editor.key(
                action,
                &mut self.project,
                &self.scene,
                &self.rows,
                &mut tracks,
                &self.map,
                bpm,
                &mut effects,
            );
        }
        self.tracks = tracks;
        self.settle(effects);
        handled
    }

    /// What the editor asked for, carried out.
    ///
    /// `ReRecord` is the expensive one and the reason edits are
    /// predicted rather than awaited: the recording is cut again from
    /// the widget's own project, so the picture is right on the next
    /// frame instead of whenever the engine gets round to it.
    fn settle(&mut self, effects: Vec<crate::arrange_edit::Effect>) {
        use crate::arrange_edit::Effect;
        // What rows are planned from takes the edit too: planned again —
        // a mode changed, a folder opened — they show the edit, not the
        // takes before it.
        if effects.iter().any(|e| matches!(e, Effect::Send(_)))
            && let Some((_, raw)) = self.replan.as_ref()
        {
            let mut raw = raw.borrow_mut();
            raw.items.clone_from(&self.project.items);
            raw.item_count = self.project.item_count;
        }
        let mut recut = false;
        for effect in effects {
            match effect {
                Effect::Send(edit) => {
                    // A plain click on an item: the Editor follows it.
                    if let crate::engine::Edit::SelectItem(item, true) = &edit {
                        crate::editor_view::picked(item);
                    }
                    self.edits.borrow_mut().push(edit);
                }
                Effect::ReRecord => recut = true,
                // Straight to the engine, fire and forget, as the
                // painted window does: dropping this is why space did
                // nothing in the Blitz window. The play cursor needs no
                // telling — the window polls it from the engine.
                Effect::Transport(command, at) => crate::engine::transport(command, at),
                Effect::Playhead(_) => {}
                // The warn line stays: a refusal is alertable, and the
                // log is where a session nobody was watching gets read
                // back. The notice is for the person who IS watching.
                Effect::Refused { why, row } => {
                    tracing::warn!(why, "the edit was refused");
                    self.notice = Some(crate::notice::Notice::new(why, row));
                }
            }
        }
        if recut {
            self.recut();
        }
    }

    /// Cut the whole recording again, from the widget's own project.
    fn recut(&mut self) {
        let rows = daw_ui::studio::RowsRef(std::sync::Arc::new(self.rows.clone()));
        let project = daw_ui::studio::ProjectRef(std::sync::Arc::new(self.project.clone()));
        // A view setting, not the session's: an edit must not reset it.
        let lettering = self.scene.lettering;
        self.scene = Arrangement::build(
            &self.palette,
            &self.font,
            &project,
            &rows,
            self.layout,
            &self.previews,
            crate::tcp::Tcp {
                compact: self.compact.get(),
                recording: self.recording.get(),
                mixing: self.mixing.get(),
            },
        );
        self.scene.lettering = lettering;
        self.sections = self.project.sections.clone();
        self.markers = self.project.markers.clone();
        self.controls = None;
    }

    /// A key came back up: the processor's held-key state (auto-repeat,
    /// the sticky prefix), and the one decision a held `z` leaves open.
    /// Held and used as the zoom tool, it was the tool, not a prefix:
    /// the popup goes and nothing is left pending. Tapped, it stays open
    /// for the next key.
    fn released(&mut self, event: &blitz_traits::events::BlitzKeyEvent) -> bool {
        use blitz_traits::events::Key;
        let named;
        let (named, text) = match &event.key {
            Key::Character(text) => (None, Some(text.as_str())),
            other => {
                named = other.to_string();
                (Some(named.as_str()), None)
            }
        };
        let Some(code) = crate::keys::key_code(named, text) else {
            return false;
        };
        let held = mods(event.modifiers);
        let was = self.keys.which_key();
        let hide = self.keys.release(
            code.clone(),
            input::Modifiers {
                ctrl: held.ctrl,
                alt: held.alt,
                shift: held.shift,
                meta: false,
            },
        );
        let tool = std::mem::take(&mut self.pointing.borrow_mut().tool_used);
        if hide || (tool && code == input::KeyCode::Character("z".into())) {
            self.keys.cancel();
        }
        self.publish_which_key();
        was != self.keys.which_key()
    }

    /// One key, into an open rename.
    ///
    /// `false` when there is no rename open, which is how a key that is
    /// not for the field falls through to whatever else the window does
    /// with it.
    fn typed(&mut self, event: &blitz_traits::events::BlitzKeyEvent) -> bool {
        // Blitz is on `keyboard_types` 0.7, whose `Key` is flat — the
        // named keys are variants of it rather than of a `NamedKey`
        // beside it.
        use blitz_traits::events::Key;
        let Some(rename) = self.renaming.as_mut() else {
            // No field open, so the key is the session's rather than a
            // letter. While one IS open the keyboard belongs to it —
            // `d` must type a d, not delete what is selected.
            return self.acted(event);
        };
        match &event.key {
            Key::Enter => {
                let Some(rename) = self.renaming.take() else {
                    return false;
                };
                // An empty name is refused, and refusing it by closing
                // the field is kinder than leaving it open with no way
                // to tell why Enter did nothing.
                let Some(name) = rename.commit().map(str::to_owned) else {
                    return true;
                };
                if let crate::rename::What::Track(guid) = rename.what {
                    let edit = crate::engine::Edit::Rename(guid, name);
                    self.assume(&edit);
                    // A name is RECORDED chrome and not a live value,
                    // so moving the track is not enough — the row it is
                    // written into has to be cut again or the field
                    // closes on the old name.
                    self.scene.forget_panel();
                    self.edits.borrow_mut().push(edit);
                }
            }
            Key::Escape => self.renaming = None,
            Key::Backspace => rename.backspace(),
            Key::Delete => rename.delete(),
            Key::ArrowLeft => rename.left(),
            Key::ArrowRight => rename.right(),
            Key::Home => rename.home(),
            Key::End => rename.end(),
            Key::Character(text) => {
                for c in text.chars() {
                    rename.insert(c);
                }
            }
            _ => return false,
        }
        true
    }

    /// A knob, turned to wherever the pointer has got to.
    ///
    /// Measured from the press and against the value the track had
    /// THEN, so the same movement always means the same change however
    /// it was made. `fine` is the modified drag: the same distance
    /// worth a fraction as much.
    fn turn(&mut self, y: f64, fine: bool) {
        let Some(turn) = self.turning.as_ref() else {
            return;
        };
        let control = match turn.spot.control {
            crate::row::Control::Volume => crate::mcp::Control::Volume,
            crate::row::Control::Pan => crate::mcp::Control::Pan,
            _ => return,
        };
        let dy = y - turn.from;
        let dy = if fine { dy * crate::gesture::FINE } else { dy };
        let fraction = crate::gesture::drag_fraction(dy, KNOB_TRAVEL);
        let Some(edit) = crate::engine::drag(control, &turn.was.guid, &turn.was, fraction) else {
            return;
        };
        self.assume(&edit);
        self.edits.borrow_mut().push(edit);
    }

    /// An item's volume set from its dial, `db`: in the widget's own
    /// project, sent to the engine, and the picture cut again (the
    /// waveform is drawn through it).
    fn set_item_volume(&mut self, index: usize, db: f64) {
        let Some(guid) = self.scene.item(index).map(|i| i.guid.clone()) else {
            return;
        };
        let volume = 10.0_f64.powf(db / 20.0);
        for item in self.project.items.values_mut().flatten() {
            if item.guid == guid {
                item.volume = volume;
            }
        }
        self.edits
            .borrow_mut()
            .push(crate::engine::Edit::SetItemVolume(guid, volume));
        self.recut();
    }

    /// Apply an edit to the widget's own copy of the tracks.
    ///
    /// Only the ones a control can make, and only the fields a control
    /// DRAWS. This is a picture being kept honest, not a second source
    /// of truth — see [`ArrangementWidget::act`].
    fn assume(&mut self, edit: &crate::engine::Edit) {
        use crate::engine::Edit;
        // Selection is not a field on one track: selecting one
        // DESELECTS the rest, which is a walk over all of them and not
        // a change to the one named. Handled before the rest.
        if let Edit::Select(guid) | Edit::AddToSelection(guid) = edit {
            let only = matches!(edit, Edit::Select(_));
            let pick = |track: &mut daw_proto::Track| {
                if track.guid == *guid {
                    track.selected = true;
                } else if only {
                    track.selected = false;
                }
            };
            self.tracks.iter_mut().for_each(pick);
            self.rows.iter_mut().for_each(|(track, _)| pick(track));
            if let Some((_, raw)) = &self.replan {
                raw.borrow_mut().tracks.iter_mut().for_each(pick);
            }
            // A row's tint carries its selection, and a tint is
            // recorded — the same reason a rename has to forget the
            // cut.
            self.scene.forget_panel();
            self.values_changed();
            return;
        }
        // A toggle flips what it finds; the others carry the value they
        // mean. Both shapes, one walk.
        let (guid, change): (&str, &dyn Fn(&mut daw_proto::Track)) = match edit {
            Edit::ToggleMute(guid) => (guid, &|t| t.muted = !t.muted),
            Edit::ToggleSolo(guid) => (guid, &|t| t.soloed = !t.soloed),
            Edit::ToggleArm(guid) => (guid, &|t| t.armed = !t.armed),
            Edit::SetPhase(guid, on) => (guid, &|t| t.phase_inverted = *on),
            Edit::SetLanePlays(guid, mask) => (guid, &|t| t.lane_play_mask = *mask),
            Edit::SetVolume(guid, gain) => (guid, &|t| t.volume = *gain),
            Edit::SetPan(guid, pan) => (guid, &|t| t.pan = *pan),
            Edit::Rename(guid, name) => (guid, &|t| t.name.clone_from(name)),
            _ => return,
        };
        for track in self
            .tracks
            .iter_mut()
            .chain(self.rows.iter_mut().map(|(track, _)| track))
            .filter(|track| track.guid == guid)
        {
            change(track);
        }
        // And the tracks the rows are planned from: a fold or a scene
        // plans the rows again from them, and an edit only in the rows it
        // replaced — a mute, a rename — was undone by it.
        if let Some((_, raw)) = &self.replan {
            raw.borrow_mut()
                .tracks
                .iter_mut()
                .filter(|track| track.guid == guid)
                .for_each(change);
        }
        self.values_changed();
    }
}

thread_local! {
    /// Each open song as its arrangement last had it — the tracks with the
    /// mutes, renames and visibility changed in it — by the song (held
    /// weakly: a song closed is forgotten). A panel rebuilt on the way
    /// back from another view starts from this, not from the song as it
    /// opened.
    static LEFT: RefCell<Vec<(std::sync::Weak<daw_ui::studio::project::Project>, Left)>> =
        const { RefCell::new(Vec::new()) };
}

/// A song as its arrangement has it, shared by every arrangement built for
/// it ([`song_as_left`]).
type Left = Rc<RefCell<daw_ui::studio::project::Project>>;

thread_local! {
    /// How many times a song's kept copy was edited from outside an
    /// arrangement ([`edit_song`]): one showing it compares this with the
    /// value it planned at, as it does the folds.
    static EDITED: Cell<u64> = const { Cell::new(0) };
}

/// Change `planner`'s song as the arrangement has it, from outside one —
/// Mix mode muting the guide. `f` says what changed (`Some`), and an
/// arrangement showing the song plans its rows again; `None`, nothing did.
pub(crate) fn edit_song<R>(
    planner: &crate::studio::Planner,
    f: impl FnOnce(&mut daw_ui::studio::project::Project) -> Option<R>,
) -> Option<R> {
    let raw = song_as_left(planner);
    let out = f(&mut raw.borrow_mut());
    if out.is_some() {
        EDITED.with(|edited| edited.set(edited.get().wrapping_add(1)));
    }
    out
}

/// `planner`'s song as its arrangement left it, kept from now on.
fn song_as_left(planner: &crate::studio::Planner) -> Left {
    LEFT.with(|all| {
        let mut all = all.borrow_mut();
        all.retain(|(song, _)| song.strong_count() > 0);
        if let Some((_, raw)) = all
            .iter()
            .find(|(song, _)| std::ptr::eq(song.as_ptr(), std::sync::Arc::as_ptr(&planner.raw)))
        {
            return Rc::clone(raw);
        }
        let raw = Rc::new(RefCell::new((*planner.raw).clone()));
        all.push((std::sync::Arc::downgrade(&planner.raw), Rc::clone(&raw)));
        raw
    })
}

/// The rows to open `planner`'s arrangement with, when it has been shown
/// before.
///
/// Planned again from the song as it was left, with the window's folds as
/// they are now. `None` the first time — the rows the song opened with
/// are those.
#[must_use]
pub fn rows_as_left(
    planner: &crate::studio::Planner,
) -> Option<(daw_ui::studio::ProjectRef, daw_ui::studio::RowsRef)> {
    let raw = LEFT.with(|all| {
        all.borrow()
            .iter()
            .find(|(song, _)| std::ptr::eq(song.as_ptr(), std::sync::Arc::as_ptr(&planner.raw)))
            .map(|(_, raw)| Rc::clone(raw))
    })?;
    let as_left = planner.plan(&raw.borrow());
    Some(as_left)
}

/// The MIDI items of `fresh` whose notes `previews` has not read.
fn unread_midi(
    fresh: &daw_ui::studio::project::Project,
    previews: &crate::midi::Previews,
) -> Vec<(String, f64)> {
    fresh
        .items
        .values()
        .flatten()
        .filter(|item| fresh.is_midi(&item.guid) && previews.get(&item.guid).is_none())
        .map(|item| (item.guid.clone(), item.length.as_seconds()))
        .collect()
}

/// A slice cut from a take, as its waveform is drawn: which take, where
/// in it (seconds from the take's own start), how long, and the stretch
/// markers it plays through over a gap.
struct SliceDrawn {
    guid: String,
    take: String,
    from: f64,
    offset: f64,
    length: f64,
    warp: Option<Vec<(f64, f64)>>,
}

/// Each slice's waveform, cut from its take's: off the window's thread on
/// the waveform pool (`crate::wave_pool`), the takes it has not read read
/// first, shown in the bottom bar as it goes. The window draws each slice
/// once its waveform is there.
fn draw_slices(previews: &crate::midi::Previews, takes: Vec<String>, slices: Vec<SliceDrawn>) {
    let cut = |previews: &crate::midi::Previews, slices: &[SliceDrawn]| {
        let drawn: Vec<(String, crate::midi::Wave)> = slices
            .iter()
            .filter_map(|s| {
                let wave = previews.wave(&s.take)?;
                Some((
                    s.guid.clone(),
                    s.warp.as_deref().map_or_else(
                        || wave.slice(s.from, s.length),
                        |markers| wave.warped(s.offset, s.length, markers),
                    ),
                ))
            })
            .collect();
        previews.put_waves(drawn);
    };
    #[cfg(feature = "native")]
    {
        let previews = previews.clone();
        crate::wave_pool::spawn(move || {
            let job = crate::jobs::start("Slicing waveforms", slices.len());
            let unread: Vec<String> = takes
                .into_iter()
                .filter(|guid| previews.wave(guid).is_none())
                .collect();
            if !unread.is_empty() {
                previews.fill_waves_blocking(unread);
            }
            cut(&previews, &slices);
            job.advance(slices.len());
        });
    }
    #[cfg(not(feature = "native"))]
    {
        let _ = takes;
        cut(previews, &slices);
    }
}

/// The audio items whose peaks have not been read.
#[cfg(feature = "native")]
fn unread_audio(
    fresh: &daw_ui::studio::project::Project,
    previews: &crate::midi::Previews,
) -> Vec<String> {
    fresh
        .items
        .values()
        .flatten()
        .filter(|item| !fresh.is_midi(&item.guid) && previews.wave(&item.guid).is_none())
        .map(|item| item.guid.clone())
        .collect()
}

/// Whether the window has actions waiting from the Action List.
#[cfg(feature = "native")]
fn window_queued() -> bool {
    crate::actions::window_pending()
}

/// No Action List without the engine: nothing ever waits.
#[cfg(not(feature = "native"))]
const fn window_queued() -> bool {
    false
}

impl Widget for ArrangementWidget {
    /// Whether the last event changed the picture.
    ///
    /// Most moves do not: a pointer crossing the panel raises an event
    /// per pixel and changes which control it is on perhaps twice.
    fn needs_redraw(&self) -> bool {
        // A notice fading and a refused ghost going home both change the
        // picture with no event behind them, so neither can wait on
        // `dirty` — nothing is going to set it. Nor can a read-back the
        // engine asked for.
        self.dirty.get()
            || window_queued()
            || crate::hit_overlay::version() != self.detection_seen.get()
            || crate::marks_strip::version() != self.marks_seen.get()
            || crate::poi::version() != self.poi_seen.get()
            || quantize_asked()
            // Edits made and not yet sent: the panel sends them at the
            // top of a frame, and this one is owed.
            || !self.edits.borrow().is_empty()
            || crate::wave_detail::generation() != self.detail_seen.get()
            || self.view_drawn.get() != Self::framing(&self.view.borrow())
            || self.stage_ready()
            || crate::studio::resync_pending()
            || self
                .notice
                .as_ref()
                .is_some_and(crate::notice::Notice::alive)
            || self.editor.settling()
            // A trail drawing in behind a stopped cursor changes the
            // picture every frame with nothing else moving.
            || self.trail.alive(web_time::Instant::now())
            || self.meters_lit.get()
            // The toolbar changed the panel's shape: nothing else will
            // ask for the frame that re-cuts it.
            || self.scene.tcp.compact != self.compact.get()
            // …or the window changed mode: the arm comes or goes.
            || self.scene.tcp.recording != self.recording.get()
            || self.scene.tcp.mixing != self.mixing.get()
            // Other people's pointers glide and their play cursors move
            // with nothing happening here.
            || crate::ghosts::active()
            // The playhead, the meters and the follow scroll, while the
            // transport moves.
            || crate::engine::moving()
            // A throw still carrying the view, and zooms queued for the
            // next frame.
            || self.fling.is_some()
            || !self.zooms.borrow().is_empty()
            // Previews and folds that changed off the UI thread: the next
            // paint re-cuts the rows for them.
            || self.previews.generation() != self.previews_seen
            || crate::folds::generation() != self.folds_seen
            || EDITED.with(Cell::get) != self.edited_seen
    }

    fn handle_event(&mut self, event: &UiEvent) {
        // A press in the arrangement takes the keyboard from the chart
        // editor, so the space bar plays again.
        if matches!(event, UiEvent::PointerDown(_)) {
            crate::keys::set_editing(false);
        }
        self.event(event);
    }

    fn paint(
        &mut self,
        _render_ctx: &mut dyn RenderContext,
        _styles: &ComputedStyles,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Scene {
        self.paint_scene(width, height, scale)
    }
}

impl ArrangementWidget {
    /// One input event, in the widget's own coordinates: what Blitz's
    /// `Widget::handle_event` calls, and what the web host calls.
    pub fn event(&mut self, event: &UiEvent) {
        // In the units the widget lays out in: the host's CSS pixels over
        // the touch zoom.
        let scaled = in_units(event, self.ui.get());
        let event = &scaled;
        // The zoom the event is read at: what tells a razor area's edge
        // from its body (`arrange_edit::Editor::pps`).
        self.editor.pps = self.viewport(self.size.0, self.size.1).pps;
        // A hit marker's flag, in Edit mode, before the mouse map: a hand
        // on a hit is the hit's, not the item's under it.
        if self.hit_gesture(event) {
            self.dirty.set(true);
            return;
        }
        // A double-click on a band on the ruler selects its time.
        if self.band_gesture(event) {
            self.dirty.set(true);
            return;
        }
        // A swipe along a take's lane, on a row showing its lanes: its own
        // gesture, before the editor's select and move.
        if self.swipe_gesture(event) {
            self.dirty.set(true);
            return;
        }
        // What a press here would do, asked BEFORE the press is taken: a
        // razor starts as an area under the pointer, which would then
        // answer as the area rather than as the razor being drawn.
        let pressed = match event {
            UiEvent::PointerDown(e) if !self.stands_down(event) => {
                let (x, y) = (f64::from(e.coords.client_x), f64::from(e.coords.client_y));
                self.hit(x, y).map(|hit| {
                    let context = self.editor.context_at(hit);
                    crate::mousemap::resolve(context, Gesture::Drag, mods(e.mods)).action
                })
            }
            _ => None,
        };
        let mut changed =
            !self.stands_down(event) && self.fingered(event).unwrap_or_else(|| self.took(event));
        self.point(event, pressed);
        // A finger has no hover: once it lifts, nothing is under it. Left
        // as it was, the name it last touched kept the underline that
        // says "double-click to rename".
        if let UiEvent::PointerUp(e) | UiEvent::PointerCancel(e) = event
            && e.is_finger()
        {
            changed |= self.pointer.hover(None);
        }
        self.dirty.set(changed);
    }

    /// The picture at `width` x `height`: what Blitz's `Widget::paint`
    /// returns, and what the web host draws into its canvas.
    pub fn paint_scene(&mut self, width: u32, height: u32, scale: f64) -> Scene {
        // What was run on the Action List for the window, from elsewhere
        // than a key (a model, a menu): done before the picture is.
        #[cfg(feature = "native")]
        if crate::actions::window_pending() {
            // What the engine changed first (a lane set up from the Action
            // List): an action carried out on the session as it was would
            // edit takes that are no longer there.
            if crate::studio::take_resync() {
                self.resync();
            }
            let _ = self.apply_actions(crate::actions::take_window());
        }
        self.marks_seen.set(crate::marks_strip::version());
        self.poi_seen.set(crate::poi::version());
        self.settle_stage();
        self.dirty.set(false);
        let generation = self.previews.generation();
        if generation != self.previews_seen {
            self.previews_seen = generation;
            self.recut();
        }
        // A folder opened or shut — here, in the mixer, all at once: the
        // rows planned again, and the mixer given them.
        // Or the song changed from outside (Mix mode's guide mute).
        let folds = crate::folds::generation();
        let edited = EDITED.with(Cell::get);
        if folds != self.folds_seen || edited != self.edited_seen {
            self.folds_seen = folds;
            self.edited_seen = edited;
            self.replan_rows();
        }
        // Blitz hands a widget its size in device pixels and the pointer in
        // CSS pixels; the page hands both in CSS pixels (and `scale` 1).
        // Laid out in CSS pixels, so a press lands on what is drawn under
        // it, then drawn at the device's scale. Drawing device pixels as if
        // they were CSS put a 2x screen's arrangement at half size, and
        // every press twice as far along as what it seemed to hit.
        // And in touch mode, bigger again ([`crate::touch::ARRANGE_ZOOM`]):
        // laid out that much smaller, drawn that much bigger, so every
        // button, lane and name grows together and a press still lands on
        // what it hits.
        let scale = scale.max(f64::EPSILON) * self.ui.get().max(f64::EPSILON);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a widget's size"
        )]
        let (css_w, css_h) = (
            (f64::from(width) / scale).round() as u32,
            (f64::from(height) / scale).round() as u32,
        );
        // A thrown view's next step, to the panel as a scroll; the panel
        // stops it at the ends.
        if let Some(fling) = self.fling.as_mut() {
            let ((dx, dy), going) = fling.step();
            self.zooms
                .borrow_mut()
                .push(crate::zoom::Request::ScrollBy { dx, dy });
            if going {
                self.dirty.set(true);
                if let Some(redraw) = &self.redraw {
                    redraw();
                }
            } else {
                self.fling = None;
            }
        }
        let drawn = self.draw(css_w, css_h, scale);
        let scene = if (scale - 1.0).abs() < f64::EPSILON {
            drawn
        } else {
            let mut scene = Scene::new();
            anyrender::PaintScene::append_scene(&mut scene, drawn, Affine::scale(scale));
            scene
        };
        if std::mem::take(&mut self.fit_on_open) {
            // The whole song and every row, as far as they fit readably.
            let framed = |command| match self.zoom_request(command) {
                Some(crate::zoom::Request::Frame { time, rows, .. }) => (time, rows),
                _ => (None, None),
            };
            let (_, rows) = framed(crate::zoom::Command::FitTracks);
            let (time, _) = framed(crate::zoom::Command::Project);
            self.zooms.borrow_mut().push(crate::zoom::Request::Open {
                time,
                rows,
                floor: self.opening_floor(),
            });
        }
        scene
    }

    /// Whether this event is not the arrangement's to act on.
    ///
    /// A press with any button but the main one: the middle button is the
    /// panel's hand, and nothing here is bound to the others. And anything
    /// while a tool is up, which owns the mouse. A gesture already under
    /// way still finishes, though: the main button's release always comes
    /// through, or a drag begun before `z` went down would never end.
    fn stands_down(&self, event: &UiEvent) -> bool {
        let busy = self.holding || self.turning.is_some() || self.pointer.pressed().is_some();
        match event {
            UiEvent::PointerDown(e) => {
                e.button != MouseEventButton::Main || self.pointing.borrow().tool_active()
            }
            UiEvent::PointerUp(e) => e.button != MouseEventButton::Main,
            UiEvent::PointerMove(_) => !busy && self.pointing.borrow().tool_active(),
            _ => false,
        }
    }

    /// Which kind of item edge `hit` is on: a seam another item's edge
    /// sits on, which fade, or neither — what the pointer's shape needs
    /// and the mouse-map context does not say.
    fn edge_of(&self, hit: crate::hit::Hit) -> crate::tool::Edge {
        use crate::arrangement::ItemZone;
        use crate::tool::Edge;
        let crate::hit::Target::Item {
            row, index, zone, ..
        } = hit.target
        else {
            return Edge::Plain;
        };
        match zone {
            ItemZone::FadeIn => Edge::FadeIn,
            ItemZone::FadeOut => Edge::FadeOut,
            ItemZone::Body | ItemZone::Volume => Edge::Plain,
            ItemZone::LeftEdge | ItemZone::RightEdge => {
                let Some(item) = self.scene.item(index) else {
                    return Edge::Plain;
                };
                let at = if zone == ItemZone::LeftEdge {
                    item.x0
                } else {
                    item.x1
                };
                if self.scene.edges_at(row, at, &item.guid).is_empty() {
                    Edge::Plain
                } else {
                    Edge::Seam
                }
            }
        }
    }

    /// Tell [`crate::tool::Pointing`] what the pointer is over and what it
    /// is doing, and put the matching shape on the window.
    fn point(&self, event: &UiEvent, pressed: Option<crate::mousemap::Action>) {
        let e = match event {
            UiEvent::PointerMove(e) | UiEvent::PointerDown(e) | UiEvent::PointerUp(e) => e,
            UiEvent::PointerCancel(_) => {
                let mut pointing = self.pointing.borrow_mut();
                pointing.gesture = None;
                pointing.apply();
                return;
            }
            _ => return,
        };
        let (x, y) = (f64::from(e.coords.client_x), f64::from(e.coords.client_y));
        let mut edge = crate::tool::Edge::Plain;
        let over = if self.turning.is_some() {
            // A knob mid-turn keeps its shape off the knob, as it keeps
            // the pointer.
            crate::tool::Over::Knob
        } else if self.hit_drag.is_some()
            || self.flag_at(x, y).is_some_and(|spot| spot.mark.is_some())
        {
            // A hit marker is taken before the item under it, so it says
            // so before it is pressed.
            crate::tool::Over::Hit
        } else if let Some(spot) = self.spot_at(x, y) {
            if spot.control.is_continuous() {
                crate::tool::Over::Knob
            } else {
                crate::tool::Over::Button
            }
        } else {
            self.hit(x, y).map_or(crate::tool::Over::Nothing, |hit| {
                edge = self.edge_of(hit);
                crate::tool::Over::Map(self.editor.context_at(hit), edge)
            })
        };
        let mut pointing = self.pointing.borrow_mut();
        pointing.mods = mods(e.mods);
        pointing.inside = true;
        pointing.over = over;
        if !self.holding {
            pointing.gesture = None;
        } else if pressed.is_some() {
            pointing.gesture = pressed.map(|action| (action, edge));
        }
        pointing.apply();
    }

    /// Pointer events, already in the widget's own coordinates.
    ///
    /// Blitz makes them relative to the node before handing them over,
    /// which is what lets this be a hit test against the arrangement
    /// rather than against the window: nothing here has to know where
    /// in the window the arrangement was put.
    ///
    /// The answer is whether the picture changed. Most moves do not: a
    /// pointer crossing the panel raises an event per pixel and changes
    /// which control it is on perhaps twice.
    /// A finger, before the mouse's handling: on a lane, the ruler or a
    /// track's name, a press is held back — dragged past
    /// [`crate::touch::SLOP`] it scrolls the view, and let go where it
    /// landed it is the click it would have been, played then. A mouse
    /// press there moves the cursor at once and drags out a selection;
    /// a finger that did would make every scroll an edit. On an item or a
    /// control it is a press like any other, so items still drag.
    ///
    /// One finger at a time: a second one down while the first is held is
    /// ignored, rather than starting an edit under the first.
    fn fingered(&mut self, event: &UiEvent) -> Option<bool> {
        let at = |e: &blitz_traits::events::BlitzPointerEvent| {
            (f64::from(e.coords.client_x), f64::from(e.coords.client_y))
        };
        match event {
            // A finger — or, locked, the mouse: a drag that moves nothing
            // moves the view, whichever hand it is.
            UiEvent::PointerDown(e)
                if e.is_finger()
                    || (crate::options::LOCKING.get()
                        && e.button == blitz_traits::events::MouseEventButton::Main
                        && e.id == blitz_traits::events::BlitzPointerId::Mouse) =>
            {
                if let Some(finger) = self.finger.as_mut() {
                    // A second finger: the gesture is a pinch from here.
                    if finger.second.is_none() && e.id != finger.down.id && e.is_finger() {
                        finger.second = Some((e.id, at(e)));
                        finger.scrolling = true;
                        finger.scrubbing = false;
                        self.fling = None;
                    }
                    return Some(true);
                }
                let (x, y) = at(e);
                // On the play cursor's handle, give or take a fingertip:
                // a scrub.
                if self
                    .handle
                    .get()
                    .is_some_and(|h| h.inflate(10.0, 8.0).contains((x, y)))
                {
                    self.fling = None;
                    self.finger = Some(Finger {
                        down: e.clone(),
                        last: (x, y),
                        scrolling: false,
                        on_panel: false,
                        swiped: false,
                        scrubbing: true,
                        speed: crate::touch::Speed::default(),
                        second: None,
                    });
                    self.scrub(x);
                    return Some(true);
                }
                // On the track panel's grip: a swipe of the panel, whatever
                // is under it.
                if self
                    .grip
                    .get()
                    .is_some_and(|g| g.inflate(GRIP_REACH.0, GRIP_REACH.1).contains((x, y)))
                {
                    self.fling = None;
                    self.finger = Some(Finger {
                        down: e.clone(),
                        last: (x, y),
                        scrolling: false,
                        on_panel: true,
                        swiped: false,
                        scrubbing: false,
                        speed: crate::touch::Speed::default(),
                        second: None,
                    });
                    return Some(true);
                }
                // A control takes its press; a track's name is where a
                // finger swipes the panel or scrolls, and a tap there is
                // the click it would have been.
                if self
                    .spot_at(x, y)
                    .is_some_and(|spot| spot.control != crate::row::Control::Name)
                {
                    return None;
                }
                // Locked, an item is background to a finger too: the drag
                // scrolls, and a tap is still the click that selects it.
                let locked = crate::options::LOCKING.get();
                let scrolls = self.hit(x, y).is_some_and(|hit| {
                    matches!(
                        hit.target,
                        crate::hit::Target::Lane { .. }
                            | crate::hit::Target::Ruler { .. }
                            | crate::hit::Target::Track { .. }
                            | crate::hit::Target::Empty
                    ) || (locked && matches!(hit.target, crate::hit::Target::Item { .. }))
                });
                if !scrolls {
                    return None;
                }
                // A press catches a thrown view.
                self.fling = None;
                self.finger = Some(Finger {
                    down: e.clone(),
                    last: (x, y),
                    scrolling: false,
                    on_panel: x < self.scene.tcp.width(),
                    swiped: false,
                    scrubbing: false,
                    speed: crate::touch::Speed::default(),
                    second: None,
                });
                Some(true)
            }
            UiEvent::PointerMove(e) => {
                let (x, y) = at(e);
                // Two fingers down: a pinch.
                if let Some(finger) = self.finger.as_mut()
                    && let Some((second_id, second_at)) = finger.second
                    && (e.id == second_id || e.id == finger.down.id)
                {
                    let (a0, b0) = (finger.last, second_at);
                    let (a1, b1) = if e.id == second_id {
                        (a0, (x, y))
                    } else {
                        ((x, y), b0)
                    };
                    let step = crate::touch::pinch_step(a0, b0, a1, b1);
                    finger.last = a1;
                    finger.second = Some((second_id, b1));
                    self.zooms.borrow_mut().push(crate::zoom::Request::Pinch {
                        sx: step.sx,
                        sy: step.sy,
                        at: (
                            step.mid.0 - self.scene.tcp.width(),
                            step.mid.1 - ruler::ruler_h(),
                        ),
                        dx: -step.dx,
                        dy: -step.dy,
                    });
                    return Some(true);
                }
                let finger = self.finger.as_mut().filter(|f| f.down.id == e.id)?;
                if finger.scrubbing {
                    self.scrub(x);
                    return Some(true);
                }
                if finger.swiped {
                    return Some(true);
                }
                // Sideways across the track panel, before it has become a
                // scroll: a swipe — right expands the panel, left folds it
                // back to names, as Logic's does.
                let from = at(&finger.down);
                let (dx, dy) = (x - from.0, y - from.1);
                if finger.on_panel
                    && !finger.scrolling
                    && dx.abs() * self.ui.get() > SWIPE
                    && dx.abs() > dy.abs() * 2.0
                {
                    finger.swiped = true;
                    self.zooms
                        .borrow_mut()
                        .push(crate::zoom::Request::Shape { compact: dx < 0.0 });
                    return Some(true);
                }
                let from = at(&finger.down);
                // The slop is a fingertip's, in CSS pixels.
                let wandered = (x - from.0).hypot(y - from.1) * self.ui.get();
                // On the track panel, a finger going sideways is on its way
                // to a swipe: it waits for that rather than scrolling.
                let sideways = finger.on_panel && (x - from.0).abs() > (y - from.1).abs();
                if !finger.scrolling && !sideways && wandered > crate::touch::SLOP {
                    finger.scrolling = true;
                }
                if finger.scrolling {
                    finger.speed.moved((x, y));
                    let (dx, dy) = (finger.last.0 - x, finger.last.1 - y);
                    finger.last = (x, y);
                    self.zooms
                        .borrow_mut()
                        .push(crate::zoom::Request::ScrollBy { dx, dy });
                }
                Some(true)
            }
            UiEvent::PointerUp(e) => {
                // Either finger of a pinch lifted: the pinch is over, and
                // the other finger's lift is nothing.
                if self.finger.as_ref().is_some_and(|f| {
                    f.second.is_some_and(|(id, _)| id == e.id)
                        || (f.second.is_some() && f.down.id == e.id)
                }) {
                    self.finger = None;
                    return Some(true);
                }
                if self.finger.as_ref().is_none_or(|f| f.down.id != e.id) {
                    // Another finger's lift, while one is held: ignored,
                    // as its press was.
                    return self.finger.is_some().then_some(true);
                }
                let finger = self.finger.take()?;
                if finger.scrubbing || finger.swiped {
                    // Where it was let go, the cursor already is; a swipe
                    // has already asked for its panel.
                } else if finger.scrolling {
                    // Thrown: the view carries on the way the finger went.
                    let (vx, vy) = finger.speed.velocity();
                    self.fling = crate::touch::Fling::thrown((-vx, -vy));
                    if let Some(redraw) = &self.redraw {
                        redraw();
                    }
                } else {
                    // A tap: the press and the release, together.
                    self.took(&UiEvent::PointerDown(finger.down));
                    self.took(&UiEvent::PointerUp(e.clone()));
                }
                Some(true)
            }
            UiEvent::PointerCancel(e) => self.finger.take_if(|f| f.down.id == e.id).map(|_| true),
            _ => None,
        }
    }

    fn took(&mut self, event: &UiEvent) -> bool {
        let at = |e: &blitz_traits::events::BlitzPointerEvent| {
            (f64::from(e.coords.client_x), f64::from(e.coords.client_y))
        };
        match event {
            UiEvent::PointerMove(e) => {
                let (x, y) = at(e);
                // Where this peer's mouse is, for everyone else in the
                // session — in time and track, not pixels.
                {
                    let view = self.viewport(self.size.0, self.size.1);
                    let over_lanes = x >= self.scene.tcp.width();
                    let ruler = ruler::ruler_h();
                    let content_y = y - ruler + view.scroll_y;
                    // Exactly where in the row, not the row: a pointer is
                    // wherever the hand is.
                    let (track, fy) = if y < ruler {
                        (None, (y / ruler.max(1.0)).clamp(0.0, 1.0))
                    } else {
                        // Past the last track (the empty lanes below it):
                        // still relative to the music — so far below the
                        // last track, in its heights — so a window whose
                        // lanes end sooner (the mixer docked under them)
                        // puts it out of sight, not on the ruler.
                        let past_last = || {
                            let row = self.rows.len().checked_sub(1)?;
                            let (top, h) = self.scene.row_band(row, view)?;
                            let guid = self.rows.get(row)?.0.guid.clone();
                            Some((Some(guid), ((content_y - top) / h.max(1.0)).max(0.0)))
                        };
                        self.scene
                            .row_at_screen(content_y, view)
                            .and_then(|row| {
                                let (top, h) = self.scene.row_band(row, view)?;
                                let guid = self.rows.get(row)?.0.guid.clone();
                                Some((Some(guid), ((content_y - top) / h.max(1.0)).clamp(0.0, 1.0)))
                            })
                            .or_else(past_last)
                            .unwrap_or((None, 1.0))
                    };
                    if over_lanes {
                        crate::ghosts::local_pointer(Some((self.seconds_at(x, view), track, fy)));
                    } else {
                        // Over the track panel: the row, and how far across.
                        let panel = self.scene.tcp.width().max(1.0);
                        crate::ghosts::local_tcp_pointer(track, x / panel, fy);
                    }
                }
                // A point, an item or an edge in an automation lane,
                // following the hand wherever it goes.
                #[cfg(feature = "native")]
                if let Some(menu) = self.menu.as_mut() {
                    return menu.hover_at(x, y);
                }
                if self.take_drag.is_some() {
                    self.take_moved(x, y);
                    return true;
                }
                let view = self.viewport(self.size.0, self.size.1);
                if let Some(drag) = self.lane_drag.as_mut() {
                    let edits = crate::automation_edit::drag(drag, view, x, y);
                    self.edits.borrow_mut().extend(edits);
                    self.dirty.set(true);
                    return true;
                }
                // The editor owns the gesture once it has taken a
                // press: an item being dragged follows the pointer off
                // the lane it started on, which is what dragging is.
                if self.holding {
                    let view = self.viewport(self.size.0, self.size.1);
                    let seconds = self.seconds_at(x, view);
                    let bpm = self.scene.bpm;
                    // A razor is the one drag here that is a rectangle,
                    // so it is the one that needs the row as well as
                    // the time.
                    // A marquee spans heights, not rows: rows are not one
                    // height, and the box follows the pointer exactly.
                    if self.editor.marquee_in_flight().is_some() {
                        let zoom = if view.zoom_y > 0.0 { view.zoom_y } else { 1.0 };
                        let content_y = (y - ruler::ruler_h() + view.scroll_y) / zoom;
                        return self.editor.marquee_moved(seconds, content_y);
                    }
                    if self.editor.razor_in_flight().is_some() {
                        let row = self
                            .scene
                            .row_at_screen(y - ruler::ruler_h() + view.scroll_y, view)
                            .unwrap_or(0);
                        return self.editor.razor_moved(seconds, row, bpm);
                    }
                    return self
                        .editor
                        .moved(Some(seconds), view.pps, bpm, mods(e.mods));
                }
                if let Some(turn) = self.item_turn.as_mut() {
                    turn.turn(y, e.mods.contains(Modifiers::CONTROL));
                    self.dirty.set(true);
                    return true;
                }
                // A knob mid-turn keeps the pointer: the hand can leave
                // the control and the drag goes on, which is what makes
                // a fine adjustment possible at all.
                if self.turning.is_some() {
                    self.turn(y, e.mods.contains(Modifiers::CONTROL));
                    return true;
                }
                let spot = self.spot_at(x, y);
                let over = self.item_under(x, y);
                let moved = self.hovered_item != over;
                self.hovered_item = over;
                let lit = self.lane_lit_at(x, y);
                let relit = lit != self.lane_lit;
                self.lane_lit = lit;
                let take = self.take_hit(x, y).map(|h| (h.index, h.point));
                let relit = relit || take != self.take_lit;
                self.take_lit = take;
                self.pointer.hover(spot) || moved || relit
            }
            UiEvent::PointerDown(e) => {
                let (x, y) = at(e);
                // An open menu takes every press: a choice, or closing it.
                #[cfg(feature = "native")]
                if self.menu_pressed(x, y) {
                    return true;
                }
                // An automation lane takes its own presses: its header in
                // the panel's column, its curve under the items.
                if let Some(hit) = self.lane_hit(x, y) {
                    // The mode: chosen from a list, not cycled through six.
                    #[cfg(feature = "native")]
                    if hit.spot == crate::automation_draw::Spot::Mode
                        && !hit.placed.lane.kind.is_leveler()
                    {
                        let lane = &hit.placed.lane;
                        let choices = crate::automation_menu::modes(
                            &lane.track,
                            Some(&lane.target),
                            lane.mode,
                        );
                        self.menu = Some(crate::popup::Popup::open(
                            &self.font,
                            choices,
                            (x, y),
                            self.size,
                        ));
                        self.dirty.set(true);
                        return true;
                    }
                    let (drag, edits) = crate::automation_edit::press(
                        &hit,
                        y,
                        crate::automation_edit::Keys {
                            alt: e.mods.contains(Modifiers::ALT),
                            shift: e.mods.contains(Modifiers::SHIFT),
                            ctrl: e.mods.contains(Modifiers::CONTROL),
                        },
                    );
                    self.lane_drag = drag;
                    self.edits.borrow_mut().extend(edits);
                    self.lane_lit = self.lane_lit_at(x, y);
                    self.dirty.set(true);
                    return true;
                }
                // An item's take volume curve, on its point or its line.
                if self.take_pressed(x, y, e.mods.contains(Modifiers::ALT)) {
                    return true;
                }
                let spot = self.spot_at(x, y);
                self.pointer.hover(spot);
                self.pointer.press();
                // Nothing in the panel: the arrangement and the ruler
                // are the editor's.
                if spot.is_none() {
                    let hit = self.hit(x, y);
                    // An item's volume dial: turned here, not by the editor.
                    if let Some(crate::hit::Target::Item {
                        index,
                        zone: crate::arrangement::ItemZone::Volume,
                        ..
                    }) = hit.map(|h| h.target)
                        && let Some(volume) = self.scene.item(index).and_then(|i| i.volume)
                    {
                        let db = 20.0 * volume.max(1e-9).log10();
                        self.item_turn = Some(ItemTurn {
                            index,
                            from: y,
                            was: db,
                            now: db,
                        });
                        return true;
                    }
                    let mut effects = Vec::new();
                    let took = self
                        .editor
                        .press(hit, mods(e.mods), &self.scene, &mut effects);
                    self.settle(effects);
                    if took {
                        self.holding = true;
                        return true;
                    }
                    // The panel's own ground, off every control: the whole
                    // row is the track's, and a press on it picks the track
                    // as a press on its name does.
                    if let Some(crate::hit::Target::Track { row }) = hit.map(|h| h.target) {
                        self.act(crate::pointer::RowSpot {
                            row,
                            control: crate::row::Control::Name,
                        });
                        return true;
                    }
                }
                self.turning = spot
                    .filter(|spot| {
                        matches!(
                            spot.control,
                            crate::row::Control::Volume | crate::row::Control::Pan
                        )
                    })
                    .and_then(|spot| {
                        let (was, _) = self.rows.get(spot.row)?;
                        Some(Turn {
                            spot,
                            from: y,
                            was: was.clone(),
                        })
                    });
                // Taken hold of: what Touch and Latch automation record by.
                if let Some(turn) = &self.turning {
                    self.touch(turn, true);
                }
                true
            }
            UiEvent::PointerUp(e) => {
                let (x, y) = at(e);
                if self.take_drag.take().is_some() {
                    self.edits
                        .borrow_mut()
                        .push(crate::engine::Edit::EndUndo(TAKE_LABEL.to_owned()));
                    self.dirty.set(true);
                    return true;
                }
                if let Some(drag) = self.lane_drag.take() {
                    let over = self.lane_hit(x, y);
                    self.edits
                        .borrow_mut()
                        .extend(crate::automation_edit::release(&drag, over.as_ref()));
                    self.dirty.set(true);
                    return true;
                }
                // A turn ends where it ends. It has already been sent,
                // every move of it, so there is nothing to do on
                // release but stop — and NOT to treat it as a click,
                // which would re-fire the control it was turning.
                if let Some(turn) = self.item_turn.take() {
                    self.set_item_volume(turn.index, turn.now);
                    return true;
                }
                if let Some(turn) = self.turning.take() {
                    self.touch(&turn, false);
                    self.pointer.release();
                    self.pointer.hover(self.spot_at(x, y));
                    return true;
                }
                if self.holding {
                    self.holding = false;
                    let view = self.viewport(self.size.0, self.size.1);
                    let seconds = self.seconds_at(x, view);
                    let mut effects = Vec::new();
                    let mut project = core::mem::take(&mut self.project);
                    if self.editor.marquee_in_flight().is_some() {
                        self.editor.marquee_released(
                            &self.scene,
                            &view,
                            &mut project,
                            &mut effects,
                        );
                    } else {
                        self.editor.release(
                            Some(seconds),
                            mods(e.mods),
                            &mut project,
                            &mut effects,
                        );
                    }
                    self.project = project;
                    self.settle(effects);
                    return true;
                }
                // Acted on only if the release lands on the control the
                // press did. Dragging off a button and letting go is
                // how every toolkit says "no, cancel that", and the
                // pressed look is already drawn to promise it.
                let up = self.spot_at(x, y);
                if let Some(spot) = self.pointer.active().map(|(spot, _)| spot)
                    && up == Some(spot)
                {
                    self.act(spot);
                }
                self.pointer.release();
                self.pointer.hover(up);
                true
            }
            UiEvent::KeyDown(e) => self.typed(e),
            UiEvent::KeyUp(e) => self.released(e),
            UiEvent::PointerCancel(_) => {
                if let Some(drag) = self.lane_drag.take() {
                    self.edits
                        .borrow_mut()
                        .extend(crate::automation_edit::release(&drag, None));
                }
                self.holding = false;
                self.turning = None;
                self.pointer.release();
                true
            }
            _ => false,
        }
    }

    fn draw(&mut self, width: u32, height: u32, _scale: f64) -> Scene {
        self.echoes();
        // A rename open is a text field holding the keyboard: the window's
        // transport keys stand aside while it is (a space in a name).
        crate::keys::set_typing(self.renaming.is_some());
        // Where an action put the edit cursor and selection, taken up;
        // then where they are, for the toolbar's inserts.
        if let Some(asked) = crate::cursor::take_asked() {
            self.editor.cursor = asked;
        }
        crate::cursor::publish(self.editor.cursor);
        crate::ghosts::local_selection(&self.editor.selected, &self.rows);
        crate::ghosts::local_shown(&self.rows, &self.scene);
        // The engine changed the session under us (a toolbar insert, an
        // edited chart): read it back before drawing it.
        if crate::studio::take_resync() {
            self.resync();
        }
        // A page's read-back, once it has arrived.
        #[cfg(not(feature = "native"))]
        {
            let fresh = self.read_back.borrow_mut().take();
            if let Some(fresh) = fresh {
                self.adopt(fresh);
            }
        }
        // The panel's shape, if the toolbar has changed it: the rows are
        // RECORDED to it, so this re-cuts rather than re-scales. A theme
        // saved or swapped is the same: new colours, new rows.
        crate::skin::poll();
        if crate::skin::version() != self.skin_seen {
            self.skin_seen = crate::skin::version();
            self.palette =
                crate::skin::palette(&Palette::from_theme(&daw_ui::theming::Theme::dark()));
            self.recut();
        } else if self.scene.tcp.compact != self.compact.get()
            || self.scene.tcp.recording != self.recording.get()
            || self.scene.tcp.mixing != self.mixing.get()
        {
            self.recut();
        }
        let began = web_time::Instant::now();
        let at_now = *self.view.borrow();
        self.view_drawn.set(Self::framing(&at_now));
        let view = self.viewport(f64::from(width), f64::from(height));
        self.size = (view.width, view.height);

        // The panel is cut at a zoom, not scaled to one. Asking every
        // frame is free when the zoom has not moved, and on the frame
        // it has, re-cutting is the difference between a taller row and
        // a row with the lettering pulled out of shape.
        self.scene.repanel(
            &self.palette,
            &self.font,
            &self.rows,
            self.layout,
            view.zoom_y,
        );

        let mut out = Scene::new();
        // The same five calls, in the same order, as the painted window
        // — including the ruler AFTER the lanes, because the lane
        // backgrounds are opaque and would paint the grid straight out
        // of the frame.
        // Everything under the ruler starts below it. The recording is
        // in session coordinates and knows nothing about the strip over
        // the top of it, so the offset belongs in every transform that
        // places recorded content — which is what the painted window has
        // always done, and leaving it out of one of them is how the
        // controls ended up half a row above their own names.
        let below = ruler::ruler_h() - view.scroll_y;
        // Timed pass by pass, by a mark between each. Inline rather
        // than wrapped in a closure because every pass wants `&mut out`
        // and a closure that also holds it is a borrow fight for no
        // gain — five `mark()` calls say the same thing and read as the
        // list of passes they are measuring.
        let mut spent = Passes::default();
        let mut mark = web_time::Instant::now();
        let since = |mark: &mut web_time::Instant| {
            let spent = mark.elapsed().as_micros();
            *mark = web_time::Instant::now();
            spent
        };

        self.detail_seen.set(crate::wave_detail::generation());
        let lanes = self.scene.replay_lanes(
            &mut out,
            view,
            Affine::translate((self.scene.tcp.width() - view.scroll_x, below))
                * Affine::scale_non_uniform(view.pps, view.zoom_y),
        );
        spent.lanes = since(&mut mark);
        // An item being slipped, redrawn at the offset the drag has
        // reached. Immediately after the lanes and BEFORE the titles:
        // it covers the recorded item to hide the old waveform, and a
        // cover drawn after the titles takes the item's name with it —
        // which reads as the label vanishing for the length of a drag.
        crate::arrangement::slip_overlay(
            &mut out,
            &self.palette,
            &self.scene,
            view,
            (self.scene.tcp.width() - view.scroll_x, below),
            self.editor.slip_in_flight(),
        );
        crate::arrangement::titles(
            &mut out,
            &self.palette,
            &self.font,
            &self.scene,
            view,
            (self.scene.tcp.width() - view.scroll_x, below),
        );
        // What the editor has in flight, over the recorded items: the
        // fade handles on the item under the pointer and a fade being
        // dragged, then the selection's outlines and the ghost of an
        // item being moved or trimmed.
        let lanes_at = (self.scene.tcp.width() - view.scroll_x, below);
        crate::arrangement::fade_overlay(
            &mut out,
            &self.palette,
            &self.scene,
            view,
            lanes_at,
            self.hovered_item,
            self.editor.fade_in_flight(),
        );
        crate::arrangement::volume_overlay(
            &mut out,
            &self.palette,
            &self.scene,
            view,
            lanes_at,
            self.hovered_item,
            self.item_turn.as_ref().map(|t| (t.index, t.now)),
        );
        // The items' take volume curves, over them.
        crate::automation_draw::take_lines(
            &mut out,
            &self.palette,
            &self.scene,
            view,
            below,
            self.take_lit,
        );
        // A leveled vocal's leveler, summed, over each of its items.
        crate::automation_draw::leveler_lines(&mut out, &self.palette, &self.scene, view, below);
        // The automation lanes under their tracks, over the items' rows.
        let lanes_placed =
            crate::automation_draw::on_screen(&self.scene, &self.tracks, view, below);
        crate::automation_draw::bodies(
            &mut out,
            &self.palette,
            &self.font,
            &lanes_placed,
            view,
            &self.lane_lit,
        );
        crate::arrangement::selection_overlay(
            &mut out,
            &self.palette,
            &self.scene,
            view,
            lanes_at,
            &self.editor.selected,
            self.editor.ghost(),
        );
        // Every item's outline, its cuts with it; without outlines, in
        // Edit mode, the cuts on a shut folder's row — quiet under the
        // kit's hit markers, which are what is being worked on there.
        let quiet = self.editing.get() && crate::hit_overlay::current().is_some();
        // The comping's picture — each item ringed in the take it came
        // from, every cut drawn — is Record mode's, where takes are comped;
        // editing, it is clutter over the hit markers.
        let comping = self.recording.get();
        if crate::options::OUTLINED.get() && comping {
            crate::arrangement::outlines_overlay(
                &mut out,
                &self.palette,
                &self.scene,
                view,
                lanes_at,
                quiet,
            );
        } else if self.editing.get() && crate::folds::cuts_shown() {
            crate::arrangement::cuts_overlay(
                &mut out,
                &self.palette,
                &self.scene,
                view,
                lanes_at,
                &self.rows,
                &self.project,
                quiet,
            );
        }
        // Editing, where a track under a shut folder plays nothing: a gap
        // the folded row would hide.
        if self.editing.get() {
            crate::arrangement::gaps_overlay(
                &mut out,
                &self.scene,
                view,
                lanes_at,
                &self.rows,
                &self.project,
            );
        }
        // In Edit mode, the kit's hit markers, each on its own piece.
        self.detection_seen.set(crate::hit_overlay::version());
        if self.editing.get()
            && let Some(kit) = self.kit_rows()
        {
            match crate::hit_overlay::current() {
                Some(hits) => {
                    let rows = self.piece_rows(kit.clone(), &hits);
                    crate::arrangement::hits_overlay(
                        &mut out,
                        (&self.palette, &self.font),
                        &self.scene,
                        view,
                        lanes_at,
                        (kit, &rows),
                        (&hits, &self.hit_selected),
                    );
                }
                // Not read yet since the window opened.
                None => crate::hit_overlay::refresh(),
            }
            if let Some(drag) = &self.hit_drag {
                // Every hit dragged, where it will go.
                let dx = drag.x - drag.from_x;
                let marks = crate::hit_overlay::current();
                let xs: Vec<f64> = marks.map_or_else(
                    || vec![drag.x],
                    |hits| {
                        hits.marks
                            .iter()
                            .filter(|m| drag.ids.contains(&m.id))
                            .map(|m| m.time.mul_add(view.pps, lanes_at.0) + dx)
                            .collect()
                    },
                );
                for x in xs {
                    crate::arrangement::hit_drag_overlay(&mut out, &self.palette, view, x);
                }
            }
            if let Some((a, b)) = self.hit_marquee {
                crate::arrangement::hit_marquee_overlay(&mut out, &self.palette, (a, b));
            }
        }
        // The lanes' names on the rows showing them, and a swipe being made
        // along a take's lane: the stretch to comp.
        crate::swipe::labels(&mut out, &self.palette, &self.font, &self.scene, view);
        if let Some(swipe) = &self.swipe {
            crate::swipe::overlay(&mut out, &self.palette, &self.scene, view, lanes_at, swipe);
        }
        // The razor's rectangles over the items they cut, and under
        // nothing: they are the thing being aimed, so they go last of
        // the lane passes.
        crate::arrangement::razor_overlay(
            &mut out,
            &self.palette,
            &self.scene,
            view,
            lanes_at,
            &self.editor.razor,
            self.editor.razor_in_flight(),
        );
        crate::arrangement::marquee_overlay(
            &mut out,
            &self.palette,
            view,
            lanes_at,
            self.editor.marquee_in_flight(),
        );
        spent.titles = since(&mut mark);
        let panel = self
            .scene
            .replay_panel(&mut out, view, Affine::translate((0.0, below)));
        // The lanes' headers, in the panel's column under each track.
        crate::automation_draw::headers(
            &mut out,
            &self.palette,
            &self.font,
            &lanes_placed,
            view,
            &self.lane_lit,
            crate::engine::Transport::shared().map_or(0.0, |t| t.read().0),
        );
        spent.panel = since(&mut mark);
        // Editing, the grid is drawn by the edited track's Grid regions.
        let edited = self.edited_tracks();
        let grids = if self.editing.get() {
            crate::marks_strip::grid_regions(&edited)
        } else {
            Vec::new()
        };
        // A figure (a bar or less) is drawn on its own grid, its lines
        // numbered; a part's grid (a section of sixteenths) is the grid
        // drawn as ever, nothing special to point out.
        let sixteenth = self.bars.secs_per_beat / 4.0;
        let (figures, parts): (Vec<_>, Vec<_>) = grids
            .iter()
            .partition(|(a, b, _)| b - a <= sixteenth.mul_add(16.0, 1e-6));
        ruler::set_grid_regions(figures.clone());
        // Editing, the grid lines are the quantize's grid where they are:
        // the kit's default, a part's, a fill's.
        crate::grid_in_force::set(self.editing.get().then(|| {
            crate::grid_in_force::InForce::new(
                crate::drum_settings::current().grid,
                &crate::marks_strip::quantize_regions(&edited),
            )
        }));
        ruler::grid(
            &mut out,
            &self.palette,
            view,
            self.bars,
            &self.grid,
            FINEST,
            (0.0, 0.0),
        );
        ruler::ruler(
            &mut out,
            &self.palette,
            &self.font,
            view,
            (self.scene.tempo(), self.scene.song_start()),
            (0.0, 0.0),
        );
        // Editing, the TEMPO lane is the GRID lane: the grids in force.
        if self.editing.get() {
            ruler::grid_lane(
                &mut out,
                &self.palette,
                &self.font,
                view,
                (0.0, 0.0),
                (&figures, &parts),
            );
        } else {
            ruler::tempo(
                &mut out,
                &self.palette,
                &self.font,
                view,
                (0.0, 0.0),
                self.scene.tempo(),
            );
        }
        // The song's own shape over the timeline: the section bands and
        // the marker flags, and the lines they drop through the lanes.
        ruler::lanes(
            &mut out,
            &self.palette,
            &self.font,
            view,
            (0.0, 0.0),
            self.scene.sections(),
            self.scene.markers(),
        );
        // The selected track's own markers and regions, and its folders',
        // in the MARKS lane: the marks of what is being edited.
        match crate::marks_strip::current() {
            Some(_) => crate::marks_strip::paint(
                &mut out,
                (&self.palette, &self.font),
                view,
                (0.0, 0.0),
                &edited,
            ),
            None => crate::marks_strip::refresh(),
        }
        // The places marked to check, over the lane.
        crate::poi::paint(&mut out, (&self.palette, &self.font), view, (0.0, 0.0));
        ruler::chord_lane(
            &mut out,
            &self.palette,
            &self.font,
            view,
            (0.0, 0.0),
            self.scene.chart(),
            self.scene.lettering,
        );
        ruler::lane_lines(
            &mut out,
            &self.palette,
            view,
            (0.0, 0.0),
            self.scene.sections(),
            self.scene.markers(),
            view.height,
        );
        spent.ruler = since(&mut mark);
        // And the controls over the recorded rows.
        //
        // Recorded for every row, which is what makes a scroll free —
        // it replays a different span of the same cut. The cost of that
        // is that a cut is the whole panel, so while the zoom is MOVING
        // the cache would be rebuilt every frame for rows that are
        // about to change again. Measured: it took zoom-y from 5.1ms to
        // 8.5.
        //
        // So the cut is taken only once the zoom has settled — the
        // frame after it stopped moving. While it moves, the controls
        // are drawn live, exactly as they were before there was a
        // cache. A gesture is no worse and everything after it is free.
        let at = Affine::translate((0.0, below));
        let settled = (self.was - view.zoom_y).abs() < f64::EPSILON;
        self.was = view.zoom_y;
        if !settled {
            self.controls = None;
        }
        let stale = self
            .controls
            .as_ref()
            .is_none_or(|(_, zoom)| (zoom - view.zoom_y).abs() >= f64::EPSILON);
        if settled && stale {
            let recorded = crate::overlay::record_controls(
                &self.palette,
                &self.font,
                &self.scene,
                &self.rows,
                &self.tracks,
                &self.map,
                view,
            );
            self.controls = Some((recorded, view.zoom_y));
        }
        // Kept below the ruler. The ruler covers the recorded panel by
        // being drawn after it, but these come after the ruler, so a row
        // scrolled halfway under it drew its knob and buttons over the
        // ruler's lanes.
        let under_ruler = vello::kurbo::Rect::new(
            0.0,
            ruler::ruler_h(),
            view.width.max(0.0),
            view.height.max(ruler::ruler_h()),
        );
        anyrender::PaintScene::push_clip_layer(&mut out, Affine::IDENTITY, &under_ruler);
        let controls = match self.controls.as_ref() {
            Some((controls, _)) => {
                let counts = controls.replay(&mut out, &self.scene, view, at);
                // The recording is at rest, so the row the pointer is
                // on is drawn again over it. One row of forty, and only
                // while the pointer is on one.
                crate::overlay::hovered_row(
                    &mut out,
                    &self.palette,
                    &self.font,
                    &self.scene,
                    &self.rows,
                    &self.tracks,
                    &self.map,
                    view,
                    &self.pointer,
                    at,
                );
                counts
            }
            None => crate::overlay::panel_controls(
                &mut out,
                &self.palette,
                &self.font,
                &self.scene,
                &self.rows,
                &self.tracks,
                &self.map,
                view,
                &self.pointer,
                at,
            ),
        };
        // The levels, in the name fields.
        if let Some(meters) = &self.meters {
            let lit = crate::overlay::row_meters(
                &mut out,
                &self.palette,
                &self.scene,
                &self.rows,
                &self.tracks,
                &self.map,
                view,
                &meters.levels(),
                at,
            );
            self.meters_lit.set(lit);
        }
        anyrender::PaintScene::pop_layer(&mut out);
        // The automation menu, over everything.
        #[cfg(feature = "native")]
        if let Some(menu) = &self.menu {
            menu.paint(&mut out, &self.palette, &self.font);
        }
        // The edit cursor and the time selection, over the lanes and
        // up through the ruler. Drawn from the editor's own state,
        // which is what a click on the ruler moves.
        crate::cursor::paint_edit(
            &mut out,
            &self.palette,
            &self.editor.cursor,
            view,
            (0.0, 0.0),
            0.0,
            view.height,
        );
        // And the play cursor over it, which is the transport's and not
        // the editor's — the window polls it and writes it into the
        // view like the scroll.
        let now = web_time::Instant::now();
        self.trail.record(now, at_now.play_at);
        crate::cursor::paint(
            &mut out,
            {
                let look = crate::cursor::Look::default();
                look.trailing(self.trail.length(now, view.pps, look.trail))
            },
            at_now
                .play_at
                .mul_add(view.pps, self.scene.tcp.width() - view.scroll_x),
            0.0,
            view.height,
            self.scene.tcp.width(),
        );
        // A touchscreen's grip on the track panel's edge: the panel slides
        // (a swipe, or a drag of this), and this says so.
        self.grip.set(None);
        if self.ui.get() > 1.0 {
            let x = self.scene.tcp.width();
            let top = ruler::ruler_h();
            let rect = grip_rect(x, top + (view.height - top) / 2.0);
            paint_grip(&mut out, rect);
            self.grip.set(Some(rect));
        }
        // A touchscreen's handle on it, in the bars lane: a finger takes
        // the cursor by this and scrubs, where a press elsewhere on the
        // ruler is a scroll.
        self.handle.set(None);
        if self.ui.get() > 1.0 {
            let x = at_now
                .play_at
                .mul_add(view.pps, self.scene.tcp.width() - view.scroll_x);
            if x >= self.scene.tcp.width() && x <= view.width {
                let rect = handle_rect(x);
                paint_handle(&mut out, rect);
                self.handle.set(Some(rect));
            }
        }
        // Everyone else in the session, faintly: their selections,
        // cursors and mouse. Over your cursors so a peer on the same spot
        // is still seen, but drawn thin and pale so theirs never reads as
        // yours.
        crate::ghosts::paint(
            &mut out,
            &self.palette,
            &self.font,
            &self.scene,
            &self.rows,
            view,
            lanes_at,
            view.height,
        );
        // An open rename, over the name it replaces. Last of the panel
        // passes, because it is a field ON one and has to cover it.
        if let Some(rename) = self.renaming.as_ref()
            && let Some((top, height)) = self.scene.row_band(rename.row, view)
            && let Some((track, depth)) = self.rows.get(rename.row)
        {
            let shape = crate::row::Row::new(
                top,
                height,
                i32::try_from(*depth).unwrap_or(0),
                track.is_folder,
                self.scene.tcp,
            );
            if let Some(field) = shape.rect(crate::row::Control::Name) {
                crate::rename::paint(&mut out, &self.palette, &self.font, rename, field, at);
            }
        }
        // A refused edit, saying why, on the row it was refused on.
        //
        // Over every other pass because it is a reply and not part of
        // the picture: it has to be legible against whatever was
        // already there, including an open rename.
        let showing = self.notice.as_ref().and_then(|notice| {
            let alpha = notice.alpha()?;
            Some((
                notice.why,
                crate::notice::area(&self.font, &self.scene, &self.rows, notice, view),
                alpha,
            ))
        });
        match showing {
            Some((why, area, alpha)) => {
                crate::notice::paint(&mut out, &self.palette, &self.font, why, area, alpha, at);
            }
            // Dropped on the first frame after its time rather than on a
            // timer, so `needs_redraw` and the paint agree about when it
            // is gone.
            None => self.notice = None,
        }
        spent.controls = since(&mut mark);
        let _ = controls;
        self.spent = spent;

        let total = |a: Counts, b: Counts| Drawn {
            replayed: a.replayed.saturating_add(b.replayed),
            submitted: a.submitted.saturating_add(b.submitted),
        };
        let drawn = total(lanes, panel);
        *self.drawn.borrow_mut() = drawn;

        // Last, so it is over the picture rather than under it. The
        // sample is the PREVIOUS frame — this one is not finished, and
        // will not be until the shell has encoded and presented what
        // this call returns — which is the only honest number a paint
        // can read about itself.
        if let Some(stats) = self.stats.as_mut() {
            // The shell's number when there is a shell. The headless
            // renderer draws frames and presents none, so there it
            // never writes one — and a graph that stays empty in the
            // one mode that can dump a PNG of itself is a graph nobody
            // can look at. So fall back to what this paint cost, and
            // say which is being shown rather than letting the two be
            // mistaken for each other.
            let shell = blitz_traits::LAST_FRAME_MICROS.load(core::sync::atomic::Ordering::Relaxed);
            // The same frame split at the hand-off to the GPU. A window
            // that draws too much and a window that draws little and
            // waits on the compositor to take it are the same frame
            // time and want opposite fixes; these say which one this is.
            let encode =
                blitz_traits::LAST_ENCODE_MICROS.load(core::sync::atomic::Ordering::Relaxed);
            let present =
                blitz_traits::LAST_PRESENT_MICROS.load(core::sync::atomic::Ordering::Relaxed);
            let source = if shell > 0 { "frame" } else { "paint" };
            let micros = if shell > 0 {
                shell
            } else {
                u64::try_from(began.elapsed().as_micros()).unwrap_or(u64::MAX)
            };
            stats.add(micros);
            crate::fps::draw(
                &mut out,
                &self.font,
                stats,
                (view.width, view.height),
                &[
                    format!("{} of {} commands", drawn.submitted, drawn.replayed),
                    format!(
                        "lanes {:.1}  panel {:.1}  ruler {:.1}",
                        ms(spent.lanes),
                        ms(spent.panel),
                        ms(spent.ruler)
                    ),
                    format!(
                        "titles {:.1}  controls {:.1}  ({source})",
                        ms(spent.titles),
                        ms(spent.controls)
                    ),
                    format!(
                        "encode {:.1}  present {:.1}",
                        ms(u128::from(encode)),
                        ms(u128::from(present))
                    ),
                ],
            );
        }
        crate::arrangement::cut_to_screen(&mut out, f64::from(width), f64::from(height));
        out
    }
}

/// Microseconds as milliseconds, for the readout's lines.
fn ms(micros: u128) -> f64 {
    u32::try_from(micros).map_or(f64::from(u32::MAX), f64::from) / 1000.0
}

#[cfg(test)]
mod tests {
    use super::{ArrangementWidget, Shared, View};
    use crate::engine::Edit;
    use crate::row::Control as C;
    use blitz_dom::node::Widget as _;
    use blitz_traits::events::UiEvent;

    /// Four rows tall enough that every control has somewhere to be.
    /// The narrow tiers drop mute and solo on purpose, and a test that
    /// used them would be asserting they are missing.
    fn widget() -> ArrangementWidget {
        widget_named("t")
    }

    /// The fixture with its tracks' guids starting `prefix`: a test that
    /// gives a track automation lanes (a store the whole process shares)
    /// names its own, so no other test's rows move under it.
    fn widget_named(prefix: &str) -> ArrangementWidget {
        widget_with(prefix, &[])
    }

    /// The fixture with `items` — `(row, guid, start, length)` — on its
    /// tracks.
    fn widget_with(prefix: &str, items: &[(usize, &str, f64, f64)]) -> ArrangementWidget {
        let palette = crate::arrangement::Palette::from_theme(&daw_ui::theming::Theme::dark());
        let font = crate::text::Font::embedded().expect("the embedded font");
        let tracks: Vec<daw_proto::Track> = (0..4)
            .map(|i| daw_proto::Track {
                guid: format!("{prefix}{i}"),
                name: format!("Track {i}"),
                height: Some(90),
                ..daw_proto::Track::default()
            })
            .collect();
        let rows: Vec<(daw_proto::Track, u32)> = tracks.iter().cloned().map(|t| (t, 0)).collect();
        let refs = daw_ui::studio::RowsRef(std::sync::Arc::new(rows.clone()));
        let mut with = daw_ui::studio::Project {
            tracks,
            length_secs: 60.0,
            ..daw_ui::studio::Project::default()
        };
        for (row, guid, start, length) in items {
            let track = format!("{prefix}{row}");
            with.items
                .entry(track.clone())
                .or_default()
                .push(daw_proto::Item {
                    guid: (*guid).to_owned(),
                    track_guid: track,
                    position: daw_proto::PositionInSeconds::from_seconds(*start),
                    length: daw_proto::Duration::from_seconds(*length),
                    ..daw_proto::Item::default()
                });
        }
        let project = daw_ui::studio::ProjectRef(std::sync::Arc::new(with));
        let layout = crate::layout::Layout::default();
        let scene = crate::arrangement::Arrangement::build(
            &palette,
            &font,
            &project,
            &refs,
            layout,
            &crate::midi::Previews::default(),
            crate::tcp::Tcp::FULL,
        );
        let view: Shared = std::rc::Rc::new(std::cell::RefCell::new(View {
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom_x: 1.0,
            zoom_y: 1.0,
            play_at: 0.0,
        }));
        let mut widget = ArrangementWidget::new(
            scene,
            palette,
            font,
            120.0,
            100.0,
            rows,
            layout,
            daw_ui::studio::project::Project::default(),
            crate::midi::Previews::default(),
            view,
            false,
        );
        widget.size = (1600.0, 900.0);
        widget
    }

    /// A kit — Drums, its Kick folder, the kick's In — in Edit mode, with
    /// one kick hit marked at 2 s.
    fn kit_widget() -> ArrangementWidget {
        let palette = crate::arrangement::Palette::from_theme(&daw_ui::theming::Theme::dark());
        let font = crate::text::Font::embedded().expect("the embedded font");
        let track = |guid: &str, name: &str, parent: Option<&str>, folder: bool| daw_proto::Track {
            guid: guid.into(),
            name: name.into(),
            parent_guid: parent.map(Into::into),
            is_folder: folder,
            height: Some(90),
            ..daw_proto::Track::default()
        };
        let tracks = vec![
            track("d", "Drums", None, true),
            track("k", "Kick", Some("d"), true),
            track("ki", "In", Some("k"), false),
        ];
        let rows: Vec<(daw_proto::Track, u32)> = tracks.iter().cloned().zip([0, 1, 2]).collect();
        let refs = daw_ui::studio::RowsRef(std::sync::Arc::new(rows.clone()));
        let project =
            daw_ui::studio::ProjectRef(std::sync::Arc::new(daw_ui::studio::Project::default()));
        let layout = crate::layout::Layout::default();
        let scene = crate::arrangement::Arrangement::build(
            &palette,
            &font,
            &project,
            &refs,
            layout,
            &crate::midi::Previews::default(),
            crate::tcp::Tcp::FULL,
        );
        let view: Shared = std::rc::Rc::new(std::cell::RefCell::new(View {
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom_x: 1.0,
            zoom_y: 1.0,
            play_at: 0.0,
        }));
        let mut widget = ArrangementWidget::new(
            scene,
            palette,
            font,
            120.0,
            100.0,
            rows,
            layout,
            daw_ui::studio::project::Project {
                tracks,
                ..daw_ui::studio::project::Project::default()
            },
            crate::midi::Previews::default(),
            view,
            false,
        );
        widget.size = (1600.0, 900.0);
        widget.editing.set(true);
        crate::hit_overlay::publish(crate::hit_overlay::Overlay {
            pieces: vec!["Kick".into()],
            tracks: vec![vec!["ki".into()]],
            marks: vec![crate::hit_overlay::Mark {
                id: "h1".into(),
                piece: 0,
                time: 2.0,
                who: crate::hit_overlay::Who::Auto,
                ghost: false,
                doubt: false,
                edited: crate::hit_overlay::Edited::AsPlayed,
            }],
            zones: Vec::new(),
            stretches: Vec::new(),
        });
        widget
    }

    /// Where `time` is on `row`'s flag strip.
    fn flag(widget: &ArrangementWidget, row: usize, time: f64) -> (f64, f64) {
        let view = widget.viewport(widget.size.0, widget.size.1);
        let (top, _) = widget.scene.row_box(row).expect("a row");
        (
            time.mul_add(view.pps, widget.scene.tcp.width() - view.scroll_x),
            top.mul_add(view.zoom_y, crate::ruler::ruler_h() - view.scroll_y) + 3.0,
        )
    }

    /// A hit's flag, on the kick's rows, is the hit's: pressed, it is
    /// taken, dragged and let go; under the strip, or with no flag there,
    /// a press is the arrangement's as ever.
    #[test]
    fn a_hit_is_taken_by_its_flag() {
        let mut widget = kit_widget();
        let (x, y) = flag(&widget, 2, 2.0);
        widget.handle_event(&UiEvent::PointerDown(pointer(x, y)));
        assert_eq!(widget.hit_drag.as_ref().map(|d| d.id.as_str()), Some("h1"));
        widget.handle_event(&UiEvent::PointerMove(pointer(x + 30.0, y)));
        assert_eq!(widget.hit_drag.as_ref().map(|d| d.x), Some(x + 30.0));
        widget.handle_event(&UiEvent::PointerUp(pointer(x + 30.0, y)));
        assert!(widget.hit_drag.is_none(), "let go");
        // The kick's folder row shows it too.
        let (fx, fy) = flag(&widget, 1, 2.0);
        assert_eq!(
            widget.flag_at(fx, fy).and_then(|s| s.mark).as_deref(),
            Some("h1")
        );
        // Its line, down the row, is the hit's too: a hand reaches for
        // the line, not the flag over it.
        assert_eq!(
            widget.flag_at(x, y + 30.0).and_then(|s| s.mark).as_deref(),
            Some("h1"),
            "the line below the strip"
        );
        widget.handle_event(&UiEvent::PointerDown(pointer(x, y + 30.0)));
        assert_eq!(widget.hit_drag.as_ref().map(|d| d.id.as_str()), Some("h1"));
        widget.handle_event(&UiEvent::PointerUp(pointer(x, y + 30.0)));
        // Below the strip and away from any line, it is the item's.
        assert!(
            widget.flag_at(x + 200.0, y + 30.0).is_none(),
            "off the line"
        );
        let away = widget.flag_at(x + 200.0, y).expect("still the strip");
        assert!(away.mark.is_none());
        assert_eq!(away.pieces, ["Kick"]);
        // Out of Edit mode, no hit is taken.
        widget.editing.set(false);
        widget.handle_event(&UiEvent::PointerDown(pointer(x, y)));
        assert!(widget.hit_drag.is_none(), "not in Edit mode");
    }

    /// Where a control is, in the widget's own coordinates — which is
    /// what an event arrives in.
    fn at(widget: &ArrangementWidget, row: usize, control: C) -> (f64, f64) {
        let view = widget.viewport(widget.size.0, widget.size.1);
        let (top, height) = widget.scene.row_band(row, view).expect("a row");
        let (track, depth) = &widget.rows[row];
        let shape = crate::row::Row::new(
            top,
            height,
            i32::try_from(*depth).unwrap_or(0),
            track.is_folder,
            widget.scene.tcp,
        );
        let r = shape.rect(control).expect("a control with somewhere to be");
        (
            r.x0 + r.width() / 2.0,
            r.y0 + r.height() / 2.0 + crate::ruler::ruler_h(),
        )
    }

    /// A mouse at a point, with nothing else going on — which is what
    /// the widget reads: the coordinates and nothing more.
    fn pointer(x: f64, y: f64) -> blitz_traits::events::BlitzPointerEvent {
        use blitz_traits::events::{
            BlitzPointerEvent, BlitzPointerId, Modifiers, MouseEventButton, MouseEventButtons,
            Point, PointerCoords, PointerDetails,
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a coordinate inside a test window"
        )]
        let (x, y) = (x as f32, y as f32);
        BlitzPointerEvent {
            id: BlitzPointerId::Mouse,
            is_primary: true,
            coords: PointerCoords {
                page_x: x,
                page_y: y,
                screen_x: x,
                screen_y: y,
                client_x: x,
                client_y: y,
            },
            button: MouseEventButton::Main,
            buttons: MouseEventButtons::Primary,
            mods: Modifiers::default(),
            details: PointerDetails::default(),
            element: Point { x, y },
            active_pointers: std::sync::Arc::default(),
        }
    }

    /// A track with a volume lane, in the shared store, under the
    /// fixture's second row.
    fn with_volume_lane(
        prefix: &str,
        points: Vec<daw_proto::automation::EnvelopePoint>,
    ) -> ArrangementWidget {
        with_items_lane(prefix, points, Vec::new())
    }

    /// The same, with automation items on the lane.
    fn with_items_lane(
        prefix: &str,
        points: Vec<daw_proto::automation::EnvelopePoint>,
        items: Vec<(
            daw_proto::automation::AutomationItem,
            Vec<daw_proto::automation::EnvelopePoint>,
        )>,
    ) -> ArrangementWidget {
        let track = format!("{prefix}1");
        let mut lane = crate::automation::Lane {
            track: track.clone(),
            target: daw_proto::automation::EnvelopeRef::Type(
                daw_proto::automation::EnvelopeType::Volume,
            ),
            name: "Volume".into(),
            kind: crate::automation::Kind::Volume,
            height: 80.0,
            mode: daw_proto::primitives::AutomationMode::TrimRead,
            points,
            items,
            played: Vec::new(),
            rest: 1.0,
            fx_name: None,
            leveled: Vec::new(),
            choices: Vec::new(),
        };
        lane.replay();
        crate::automation::replace([(track, vec![lane])]);
        widget_named(prefix)
    }

    /// Where `seconds` and `value` are in row 1's first lane, on screen.
    fn in_lane(widget: &ArrangementWidget, seconds: f64, value: f64) -> (f64, f64) {
        let view = widget.viewport(widget.size.0, widget.size.1);
        let below = crate::ruler::ruler_h() - view.scroll_y;
        let placed = crate::automation_draw::on_screen(&widget.scene, &widget.tracks, view, below);
        let lane = placed.first().expect("the lane is on screen");
        (
            seconds.mul_add(view.pps, view.panel_w - view.scroll_x),
            lane.y_of(value),
        )
    }

    fn envelope_edit(
        widget: &ArrangementWidget,
    ) -> Option<Vec<daw_proto::automation::EnvelopePoint>> {
        widget.edits.borrow().iter().rev().find_map(|e| match e {
            crate::engine::Edit::SetEnvelope(_, _, points) => Some(points.clone()),
            _ => None,
        })
    }

    /// An item's take volume curve: a press on its line makes a point there
    /// and drags it; Alt and a press on a point deletes it.
    #[test]
    fn a_take_volume_point_is_made_dragged_and_deleted_on_its_item() {
        use daw_proto::automation::EnvelopePoint;
        let point = |t: f64, gain: f64| EnvelopePoint {
            time: daw_proto::PositionInSeconds::from_seconds(t),
            value: gain,
            ..EnvelopePoint::default()
        };
        crate::automation::put_take_curve(
            "take-item",
            crate::automation::TakeCurve {
                track: "take-t1".into(),
                points: vec![point(0.0, 1.0), point(2.0, 1.0)],
            },
        );
        let mut widget = widget_with("take-t", &[(1, "take-item", 1.0, 2.0)]);
        let view = widget.viewport(widget.size.0, widget.size.1);
        let below = crate::ruler::ruler_h() - view.scroll_y;
        let item = widget.scene.item(0).expect("the item");
        let (top, height) = widget.scene.item_box(item).expect("its box");
        let (top, height) = (top.mul_add(view.zoom_y, below), height * view.zoom_y);
        let x_at = |t: f64| (1.0 + t).mul_add(view.pps, view.panel_w - view.scroll_x);
        // On the line at 0 dB, a second in.
        let y0 = (1.0 - crate::automation::take_y(1.0)).mul_add(height - 8.0, top + 4.0);
        assert!(widget.took(&UiEvent::PointerDown(pointer(x_at(1.0), y0))));
        let made = crate::automation::take_curve("take-item")
            .expect("the curve")
            .points;
        assert_eq!(made.len(), 3, "a point made: {made:?}");
        // Dragged down to −12 dB.
        let y12 = (1.0 - crate::automation::take_y(0.251)).mul_add(height - 8.0, top + 4.0);
        assert!(widget.took(&UiEvent::PointerMove(pointer(x_at(1.0), y12))));
        assert!(widget.took(&UiEvent::PointerUp(pointer(x_at(1.0), y12))));
        let dragged = crate::automation::take_curve("take-item")
            .expect("the curve")
            .points;
        let middle = dragged.get(1).map_or(0.0, |p| p.value);
        assert!(
            20.0f64.mul_add(middle.log10(), 12.0).abs() < 0.5,
            "{dragged:?}"
        );
        assert!(widget.edits.borrow().iter().any(|e| matches!(
            e,
            crate::engine::Edit::SetTakeVolume(item, points) if item == "take-item" && points.len() == 3
        )));
        assert!(matches!(
            widget.edits.borrow().last(),
            Some(crate::engine::Edit::EndUndo(_))
        ));
        // Alt and a press on it: gone.
        let mut alt = pointer(x_at(1.0), y12);
        alt.mods = blitz_traits::events::Modifiers::ALT;
        assert!(widget.took(&UiEvent::PointerDown(alt)));
        let left = crate::automation::take_curve("take-item")
            .expect("the curve")
            .points;
        assert_eq!(left.len(), 2, "{left:?}");
    }

    /// Automation items by hand: Shift and a drag makes one over the span,
    /// its right edge resizes it, Ctrl and a drag on its bar makes another
    /// of its pool, Alt and a press on its bar deletes it.
    #[test]
    fn automation_items_are_made_resized_copied_and_deleted_by_hand() {
        use crate::engine::Edit;
        use daw_proto::automation::{AutomationItem, EnvelopePoint};
        use daw_proto::primitives::{Duration, PositionInSeconds};
        let flat = |v: f64| EnvelopePoint {
            value: v,
            ..EnvelopePoint::default()
        };
        let existing = AutomationItem {
            index: 0,
            pool_id: 4,
            position: PositionInSeconds::from_seconds(10.0),
            length: Duration::from_seconds(2.0),
            ..AutomationItem::default()
        };
        let mut widget = with_items_lane(
            "items-lane-t",
            vec![flat(1.0)],
            vec![(existing, vec![flat(0.5)])],
        );
        let shift = |x: f64, y: f64| {
            let mut e = pointer(x, y);
            e.mods = blitz_traits::events::Modifiers::SHIFT;
            e
        };
        // Shift-drag from 2 s to 5 s: a fresh item over the span, its
        // curve what played there (0 dB).
        let (x2, y) = in_lane(&widget, 2.0, 0.5);
        let (x5, _) = in_lane(&widget, 5.0, 0.5);
        assert!(widget.took(&UiEvent::PointerDown(shift(x2, y))));
        assert!(widget.took(&UiEvent::PointerMove(pointer(x5, y))));
        assert!(widget.took(&UiEvent::PointerUp(pointer(x5, y))));
        let made = widget.edits.borrow().iter().find_map(|e| match e {
            Edit::AddAutomationItem(_, _, (at, length, pool), curve) => {
                Some((*at, *length, *pool, curve.clone()))
            }
            _ => None,
        });
        let (at, length, pool, curve) = made.expect("an item made");
        assert!(
            (at - 2.0).abs() < 0.05 && (length - 3.0).abs() < 0.05 && pool == -1,
            "{at} {length} {pool}"
        );
        assert!(
            curve.iter().all(|p| (p.value - 1.0).abs() < 1e-9) && curve.len() >= 2,
            "{curve:?}"
        );
        widget.edits.borrow_mut().clear();

        // The existing item's right edge, dragged from 12 s to 13 s.
        let (x12, y_bar) = in_lane(&widget, 12.0, 1.9);
        let (x13, _) = in_lane(&widget, 13.0, 1.9);
        assert!(widget.took(&UiEvent::PointerDown(pointer(x12, y_bar + 20.0))));
        assert!(widget.took(&UiEvent::PointerMove(pointer(x13, y_bar + 20.0))));
        assert!(widget.took(&UiEvent::PointerUp(pointer(x13, y_bar + 20.0))));
        let bounds = widget.edits.borrow().iter().find_map(|e| match e {
            Edit::SetAutomationItemBounds(_, _, 0, b) => Some(*b),
            _ => None,
        });
        let (at, length, _) = bounds.expect("resized");
        assert!(
            (at - 10.0).abs() < 1e-9 && (length - 3.0).abs() < 0.05,
            "{at} {length}"
        );
        widget.edits.borrow_mut().clear();

        // Its bar, Ctrl-dragged on by 10 s: another of pool 4 at 20 s.
        let view = widget.viewport(widget.size.0, widget.size.1);
        let below = crate::ruler::ruler_h() - view.scroll_y;
        let lane = crate::automation_draw::on_screen(&widget.scene, &widget.tracks, view, below)
            .into_iter()
            .next()
            .expect("the lane");
        let bar = lane.top + 5.0;
        let (x11, _) = in_lane(&widget, 11.0, 1.0);
        let (x21, _) = in_lane(&widget, 21.0, 1.0);
        let mut ctrl = pointer(x11, bar);
        ctrl.mods = blitz_traits::events::Modifiers::CONTROL;
        assert!(widget.took(&UiEvent::PointerDown(ctrl)));
        assert!(widget.took(&UiEvent::PointerMove(pointer(x21, bar))));
        assert!(widget.took(&UiEvent::PointerUp(pointer(x21, bar))));
        let copied = widget.edits.borrow().iter().find_map(|e| match e {
            Edit::AddAutomationItem(_, _, (at, _, pool), _) => Some((*at, *pool)),
            _ => None,
        });
        let (at, pool) = copied.expect("a copy");
        assert!((at - 20.0).abs() < 0.05 && pool == 4, "{at} {pool}");
        widget.edits.borrow_mut().clear();

        // Alt and a press on its bar: deleted.
        let mut alt = pointer(x11, bar);
        alt.mods = blitz_traits::events::Modifiers::ALT;
        assert!(widget.took(&UiEvent::PointerDown(alt)));
        assert!(
            widget
                .edits
                .borrow()
                .iter()
                .any(|e| matches!(e, Edit::DeleteAutomationItem(_, _, 0)))
        );
    }

    /// A lane sits under its track: the rows below move down by it, and
    /// the track's own controls stay where they were.
    #[test]
    fn an_automation_lane_sits_under_its_track() {
        let plain = widget_named("plain-lane-t");
        let laned = with_volume_lane("sits-lane-t", Vec::new());
        let view = plain.viewport(plain.size.0, plain.size.1);
        let (a, b) = (plain.scene.row_box(2), laned.scene.row_box(2));
        let ((top, _), (moved, _)) = (a.expect("row 2"), b.expect("row 2"));
        assert!((moved - top - 80.0).abs() < 1e-6, "{top} → {moved}");
        assert_eq!(
            plain.scene.row_box(1),
            laned.scene.row_box(1),
            "the track itself"
        );
        assert!(laned.scene.lanes_box(1).is_some() && plain.scene.lanes_box(1).is_none());
        let _ = view;
    }

    /// A press on a lane's ground adds a point there and drags it; a
    /// release closes the step; Alt and a press on the point deletes it.
    // r[verify automation.lanes]
    #[test]
    fn a_point_is_drawn_dragged_and_deleted_in_its_lane() {
        let mut widget = with_volume_lane("draw-lane-t", Vec::new());
        let (x, y) = in_lane(&widget, 2.0, 0.5);
        assert!(widget.took(&UiEvent::PointerDown(pointer(x, y))));
        let added = envelope_edit(&widget).expect("a point drawn");
        let [point] = added.as_slice() else {
            panic!("one point: {added:?}");
        };
        assert!((point.time.as_seconds() - 2.0).abs() < 0.05, "{point:?}");
        assert!((point.value - 0.5).abs() < 0.05, "{point:?}");
        assert!(matches!(
            widget.edits.borrow().first(),
            Some(crate::engine::Edit::BeginUndo(_))
        ));

        let (x2, y2) = in_lane(&widget, 3.0, 1.0);
        assert!(widget.took(&UiEvent::PointerMove(pointer(x2, y2))));
        let moved = envelope_edit(&widget).expect("the point dragged");
        let [point] = moved.as_slice() else {
            panic!("still one point: {moved:?}");
        };
        assert!((point.time.as_seconds() - 3.0).abs() < 0.05, "{point:?}");
        assert!((point.value - 1.0).abs() < 0.05, "{point:?}");
        assert!(widget.took(&UiEvent::PointerUp(pointer(x2, y2))));
        assert!(matches!(
            widget.edits.borrow().last(),
            Some(crate::engine::Edit::EndUndo(_))
        ));
        // The store has it: drawn where it was let go.
        let lane = crate::automation::lanes_of("draw-lane-t1");
        assert_eq!(lane.first().map(|l| l.points.len()), Some(1));

        let mut alt = pointer(x2, y2);
        alt.mods = blitz_traits::events::Modifiers::ALT;
        assert!(widget.took(&UiEvent::PointerDown(alt)));
        assert_eq!(envelope_edit(&widget).map(|p| p.len()), Some(0), "deleted");
    }

    #[test]
    fn a_move_finds_the_control_under_it() {
        let mut widget = widget();
        let (x, y) = at(&widget, 2, C::Mute);
        assert!(
            widget.took(&UiEvent::PointerMove(pointer(x, y))),
            "moving onto a control is a change worth redrawing"
        );
        assert_eq!(
            widget
                .pointer
                .hovered()
                .map(|spot| (spot.row, spot.control)),
            Some((2, C::Mute))
        );
        // And the second move onto the SAME control is not: a pointer
        // crossing a panel raises an event per pixel, and redrawing for
        // each of them is the thing hover state exists to avoid.
        assert!(
            !widget.took(&UiEvent::PointerMove(pointer(x + 1.0, y))),
            "staying on one control changes nothing"
        );
    }

    fn button(
        mut event: blitz_traits::events::BlitzPointerEvent,
        which: blitz_traits::events::MouseEventButton,
    ) -> blitz_traits::events::BlitzPointerEvent {
        event.button = which;
        event
    }

    /// The middle button is the panel's hand; nothing here acts on it.
    #[test]
    fn a_middle_click_does_nothing_here() {
        use blitz_traits::events::MouseEventButton::Auxiliary;
        let mut widget = widget();
        let (x, y) = at(&widget, 1, C::Mute);
        widget.handle_event(&UiEvent::PointerDown(button(pointer(x, y), Auxiliary)));
        widget.handle_event(&UiEvent::PointerUp(button(pointer(x, y), Auxiliary)));
        assert!(
            widget.edits.borrow().is_empty(),
            "{:?}",
            widget.edits.borrow()
        );
        assert!(!widget.rows[1].0.muted);
    }

    /// A tool owns the mouse: a click under the zoom spring is not a click.
    #[test]
    fn a_click_under_a_tool_is_the_tools() {
        let mut widget = widget();
        widget.pointing.borrow_mut().tool = crate::tool::Tool::Zoom;
        let (x, y) = at(&widget, 1, C::Mute);
        widget.handle_event(&UiEvent::PointerDown(pointer(x, y)));
        widget.handle_event(&UiEvent::PointerUp(pointer(x, y)));
        assert!(
            widget.edits.borrow().is_empty(),
            "{:?}",
            widget.edits.borrow()
        );
        assert_eq!(
            widget.pointing.borrow().icon(),
            crate::window_cursor::Shape::Glyph(crate::window_cursor::Glyph::Editor(
                expression_editor_core::cursor::Cursor::Zoom
            ))
        );
    }

    /// A press made before the tool went up still ends: its release comes
    /// through, or the gesture would never finish.
    #[test]
    fn a_gesture_begun_before_a_tool_still_finishes() {
        let mut widget = widget();
        let (x, y) = at(&widget, 1, C::Mute);
        widget.handle_event(&UiEvent::PointerDown(pointer(x, y)));
        widget.pointing.borrow_mut().tool = crate::tool::Tool::Zoom;
        widget.handle_event(&UiEvent::PointerUp(pointer(x, y)));
        let queued = widget.edits.borrow().clone();
        assert!(
            matches!(queued.as_slice(), [Edit::ToggleMute(_)]),
            "{queued:?}"
        );
    }

    /// The pointer's shape follows what is under it.
    #[test]
    fn the_pointer_says_what_is_under_it() {
        use crate::tool::Over;
        use cursor_icon::CursorIcon;
        let mut widget = widget();
        let (x, y) = at(&widget, 1, C::Volume);
        widget.handle_event(&UiEvent::PointerMove(pointer(x, y)));
        assert_eq!(widget.pointing.borrow().over, Over::Knob);
        let icon = crate::window_cursor::Shape::Icon;
        assert_eq!(widget.pointing.borrow().icon(), icon(CursorIcon::NsResize));

        let (x, y) = at(&widget, 1, C::Mute);
        widget.handle_event(&UiEvent::PointerMove(pointer(x, y)));
        assert_eq!(widget.pointing.borrow().over, Over::Button);

        // The empty lanes, with Ctrl held: a drag there is a razor.
        let mut lane = pointer(widget.scene.tcp.width() + 400.0, y);
        lane.mods = blitz_traits::events::Modifiers::CONTROL;
        widget.handle_event(&UiEvent::PointerMove(lane));
        assert!(
            matches!(widget.pointing.borrow().over, Over::Map(..)),
            "{:?}",
            widget.pointing.borrow().over
        );
        assert_eq!(
            widget.pointing.borrow().icon(),
            crate::window_cursor::Shape::Glyph(crate::window_cursor::Glyph::Editor(
                expression_editor_core::cursor::Cursor::Razor
            ))
        );
    }

    /// `z` then `t`, with a track selected: the popup shows, then goes,
    /// and the panel is asked to frame that track's row.
    #[test]
    fn z_t_asks_the_panel_to_frame_the_selected_track() {
        let mut widget = widget();
        widget.rows[2].0.selected = true;
        widget.handle_event(&UiEvent::KeyDown(key(
            blitz_traits::events::Key::Character("z".into()),
        )));
        assert!(widget.which.borrow().is_some(), "the popup is up");
        let mut up = key(blitz_traits::events::Key::Character("z".into()));
        up.state = blitz_traits::events::KeyState::Released;
        widget.handle_event(&UiEvent::KeyUp(up));
        widget.handle_event(&UiEvent::KeyDown(key(
            blitz_traits::events::Key::Character("t".into()),
        )));
        assert!(widget.which.borrow().is_none(), "and down again");
        let (top, h) = widget.scene.row_box(2).unwrap();
        let asked = widget.zooms.borrow().clone();
        assert_eq!(
            asked,
            vec![crate::zoom::Request::Frame {
                time: None,
                rows: Some((top, top + h)),
                toggle: true,
            }]
        );
    }

    /// A held `z` used as the zoom tool closes the tree on release.
    #[test]
    fn z_used_as_the_tool_closes_the_tree_on_release() {
        let mut widget = widget();
        widget.handle_event(&UiEvent::KeyDown(key(
            blitz_traits::events::Key::Character("z".into()),
        )));
        widget.pointing.borrow_mut().tool_used = true;
        let mut up = key(blitz_traits::events::Key::Character("z".into()));
        up.state = blitz_traits::events::KeyState::Released;
        widget.handle_event(&UiEvent::KeyUp(up));
        assert!(widget.which.borrow().is_none());
        assert!(!widget.keys.is_pending());
    }

    /// The visibility manager: toggling a group hides its tracks' rows
    /// and tells the engine; toggling again brings them back.
    #[test]
    fn a_visibility_toggle_hides_the_group_and_shows_it_again() {
        use crate::keys::Visibility;
        let names = ["Kick", "Snare", "Bass", "Piano"];
        let tracks: Vec<daw_proto::Track> = names
            .iter()
            .enumerate()
            .map(|(i, name)| daw_proto::Track {
                guid: format!("g{i}"),
                name: (*name).to_owned(),
                index: u32::try_from(i).unwrap(),
                ..daw_proto::Track::default()
            })
            .collect();
        let raw = daw_ui::studio::project::Project {
            tracks: tracks.clone(),
            ..daw_ui::studio::project::Project::default()
        };
        let planner = crate::studio::Planner {
            raw: std::sync::Arc::new(raw),
            scene: None,
            kinds: std::sync::Arc::new(crate::plan::Kinds::default()),
        };
        let content_h = std::rc::Rc::new(std::cell::Cell::new(0.0));
        let mut widget = widget().with_planner(planner.clone(), std::rc::Rc::clone(&content_h));
        let (project, rows) = planner.plan(&planner.raw);
        widget.restructure((*project.0).clone(), rows.as_slice().to_vec());
        let before = content_h.get();
        let names_shown = |w: &ArrangementWidget| {
            w.rows
                .iter()
                .map(|(t, _)| t.name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names_shown(&widget), names);

        assert!(widget.visibility(&Visibility::Toggle("DRUMS".into())));
        let shown = names_shown(&widget);
        assert!(
            !shown.contains(&"Kick".to_owned()) && !shown.contains(&"Snare".to_owned()),
            "{shown:?}"
        );
        assert!(shown.contains(&"Bass".to_owned()), "{shown:?}");
        assert!(content_h.get() < before, "the scroll range shrinks");
        let hidden = widget
            .edits
            .borrow()
            .iter()
            .filter(|e| matches!(e, Edit::SetVisibility(_, false, false)))
            .count();
        assert_eq!(hidden, 2, "kick and snare, told to the engine");

        assert!(widget.visibility(&Visibility::Toggle("DRUMS".into())));
        assert_eq!(names_shown(&widget), names, "and back");

        assert!(widget.visibility(&Visibility::HideAll));
        assert!(widget.visibility(&Visibility::ShowAll));
        assert_eq!(names_shown(&widget), names);
    }

    /// An item's volume dial: found where the hit test says it is, turned
    /// up by a drag (a quarter dB a pixel), and on the release the item's
    /// volume is set — in the widget's project and sent to the engine.
    #[test]
    fn an_items_dial_turns_its_volume() {
        let mut widget = widget();
        let mut project = daw_ui::studio::project::Project::default();
        project.items.insert(
            "t0".into(),
            vec![daw_proto::Item {
                guid: "i0".into(),
                track_guid: "t0".into(),
                position: daw_proto::primitives::PositionInSeconds::from_seconds(0.0),
                length: daw_proto::primitives::Duration::from_seconds(4.0),
                volume: 10.0_f64.powf(6.0 / 20.0),
                ..daw_proto::Item::default()
            }],
        );
        let rows = widget.rows.clone();
        widget.restructure(project, rows);
        let dial = (0..1600)
            .step_by(2)
            .flat_map(|x| {
                (0..300)
                    .step_by(2)
                    .map(move |y| (f64::from(x), f64::from(y)))
            })
            .find(|&(x, y)| {
                matches!(
                    widget.hit(x, y).map(|h| h.target),
                    Some(crate::hit::Target::Item {
                        zone: crate::arrangement::ItemZone::Volume,
                        ..
                    })
                )
            })
            .expect("the item's dial");
        let (x, y) = dial;
        widget.handle_event(&UiEvent::PointerDown(pointer(x, y)));
        widget.handle_event(&UiEvent::PointerMove(pointer(x, y - 40.0)));
        widget.handle_event(&UiEvent::PointerUp(pointer(x, y - 40.0)));
        let sent = widget.edits.borrow().clone();
        let Some(Edit::SetItemVolume(guid, volume)) = sent.last() else {
            panic!("{sent:?}");
        };
        assert_eq!(guid, "i0");
        let db = 20.0 * volume.log10();
        assert!((db - 16.0).abs() < 0.01, "{db} dB");
        let item = widget.project.items["t0"][0].volume;
        assert!((item - volume).abs() < 1e-9, "the widget's own copy too");
    }

    /// A mute, solo or rename made in the arrangement outlives the rows
    /// being planned again (a folder opened or shut): it lands in the
    /// tracks they are planned from, not only in the rows a fold replaces.
    #[test]
    fn a_mute_survives_a_fold() {
        let tracks: Vec<daw_proto::Track> = ["Kick", "Bass"]
            .iter()
            .enumerate()
            .map(|(i, name)| daw_proto::Track {
                guid: format!("g{i}"),
                name: (*name).to_owned(),
                index: u32::try_from(i).unwrap(),
                ..daw_proto::Track::default()
            })
            .collect();
        let raw = daw_ui::studio::project::Project {
            tracks,
            ..daw_ui::studio::project::Project::default()
        };
        let planner = crate::studio::Planner {
            raw: std::sync::Arc::new(raw),
            scene: None,
            kinds: std::sync::Arc::new(crate::plan::Kinds::default()),
        };
        let content_h = std::rc::Rc::new(std::cell::Cell::new(0.0));
        let mut widget = widget().with_planner(planner, content_h);
        widget.replan_rows();
        widget.assume(&Edit::ToggleMute("g0".into()));
        widget.assume(&Edit::ToggleSolo("g1".into()));
        widget.assume(&Edit::Rename("g1".into(), "Bass DI".into()));
        widget.replan_rows();
        let row = |guid: &str| {
            widget
                .rows
                .iter()
                .find(|(t, _)| t.guid == guid)
                .map(|(t, _)| t.clone())
                .expect("the row")
        };
        assert!(row("g0").muted, "the mute is still lit");
        assert!(row("g1").soloed && row("g1").name == "Bass DI");
        assert!(widget.tracks.iter().any(|t| t.guid == "g0" && t.muted));
    }

    /// An arrangement built again for the same song (back from the
    /// mixer) opens as the last one left it: the folders shut since, and
    /// the mutes made in it. The first one opens as the song did.
    #[test]
    fn a_rebuilt_arrangement_keeps_the_folds_and_mutes() {
        // With the top-level folders open: what is under test is beneath them.
        crate::folds::set_tops(false);
        let track = |i: u32, name: &str, depth: i32| daw_proto::Track {
            guid: format!("g{i}"),
            name: name.to_owned(),
            index: i,
            is_folder: depth > 0,
            folder_depth: depth,
            visible_in_tcp: true,
            parent_guid: (i == 1).then(|| "g0".to_owned()),
            ..daw_proto::Track::default()
        };
        let raw = daw_ui::studio::project::Project {
            tracks: vec![
                track(0, "Drums", 1),
                track(1, "Kick", -1),
                track(2, "Bass", 0),
            ],
            ..daw_ui::studio::project::Project::default()
        };
        let planner = crate::studio::Planner {
            raw: std::sync::Arc::new(raw),
            scene: None,
            kinds: std::sync::Arc::new(crate::plan::Kinds::default()),
        };
        assert!(
            super::rows_as_left(&planner).is_none(),
            "never shown: as it opened"
        );
        let content_h = std::rc::Rc::new(std::cell::Cell::new(0.0));
        let mut first = widget().with_planner(planner.clone(), content_h);
        first.replan_rows();
        assert_eq!(first.rows.len(), 3);
        first.assume(&Edit::ToggleMute("g2".into()));
        crate::folds::toggle("g0", true);
        drop(first);

        let (_, rows) = super::rows_as_left(&planner).expect("shown before");
        let names: Vec<&str> = rows
            .as_slice()
            .iter()
            .map(|(t, _)| t.name.as_str())
            .collect();
        assert_eq!(names, ["Drums", "Bass"], "the drums still shut");
        let bass = rows
            .as_slice()
            .iter()
            .find(|(t, _)| t.guid == "g2")
            .expect("bass");
        assert!(bass.0.muted, "the bass still muted");
    }

    #[test]
    fn a_click_on_mute_asks_for_a_mute() {
        let mut widget = widget();
        let (x, y) = at(&widget, 1, C::Mute);
        widget.handle_event(&UiEvent::PointerDown(pointer(x, y)));
        widget.handle_event(&UiEvent::PointerUp(pointer(x, y)));
        let queued = widget.edits.borrow().clone();
        assert!(
            matches!(queued.as_slice(), [Edit::ToggleMute(guid)] if guid == "t1"),
            "{queued:?}"
        );
        // And the widget shows it without waiting for the engine.
        assert!(widget.rows[1].0.muted, "the row it drew from");
        assert!(widget.tracks[1].muted, "the track the controls read");
    }

    /// A track of three lanes — C1, takes 1 and 2 — its `+` open: a drag
    /// along take 2 is a swipe, followed and let go; pressed on the comp it
    /// is not, and with the lanes shut nor anywhere.
    #[test]
    fn a_drag_along_a_take_lane_is_a_swipe_and_on_the_comp_is_not() {
        let palette = crate::arrangement::Palette::from_theme(&daw_ui::theming::Theme::dark());
        let font = crate::text::Font::embedded().expect("the embedded font");
        let track = daw_proto::Track {
            guid: "vox".to_owned(),
            name: "Vox".to_owned(),
            height: Some(90),
            lane_count: 3,
            lane_play_mask: 0b1,
            lane_names: vec!["C1".to_owned(), "1".to_owned(), "2".to_owned()],
            ..daw_proto::Track::default()
        };
        let _ = crate::lanes::set_expanded(&["vox".to_owned()], Some(true));
        let rows = vec![(track.clone(), 0)];
        let refs = daw_ui::studio::RowsRef(std::sync::Arc::new(rows.clone()));
        // A take on each lane: a lane holding nothing gets no strip.
        let take = |lane: u32| daw_proto::Item {
            guid: format!("take{lane}"),
            track_guid: "vox".to_owned(),
            length: daw_proto::primitives::Duration::from_seconds(30.0),
            fixed_lane: Some(lane),
            ..daw_proto::Item::default()
        };
        let project = daw_ui::studio::ProjectRef(std::sync::Arc::new(daw_ui::studio::Project {
            items: std::iter::once(("vox".to_owned(), vec![take(0), take(1), take(2)])).collect(),
            tracks: vec![track],
            length_secs: 60.0,
            ..daw_ui::studio::Project::default()
        }));
        let layout = crate::layout::Layout::default();
        let scene = crate::arrangement::Arrangement::build(
            &palette,
            &font,
            &project,
            &refs,
            layout,
            &crate::midi::Previews::default(),
            crate::tcp::Tcp::FULL,
        );
        let view: Shared = std::rc::Rc::new(std::cell::RefCell::new(View {
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom_x: 1.0,
            zoom_y: 1.0,
            play_at: 0.0,
        }));
        let mut widget = ArrangementWidget::new(
            scene,
            palette,
            font,
            120.0,
            100.0,
            rows,
            layout,
            daw_ui::studio::project::Project::default(),
            crate::midi::Previews::default(),
            view,
            false,
        );
        widget.size = (1600.0, 900.0);
        // The middle of lane `lane`'s strip, `seconds` along.
        let place = |widget: &ArrangementWidget, lane: f64, seconds: f64| {
            let view = widget.viewport(widget.size.0, widget.size.1);
            let (top, height) = widget.scene.row_box(0).expect("the row");
            let y = (lane + 0.5).mul_add(height / 3.0, top);
            (
                seconds.mul_add(view.pps, view.panel_w - view.scroll_x),
                y.mul_add(view.zoom_y, crate::ruler::ruler_h() - view.scroll_y),
            )
        };
        let (from, y) = place(&widget, 2.0, 4.0);
        let (to, _) = place(&widget, 2.0, 6.0);
        widget.handle_event(&UiEvent::PointerDown(pointer(from, y)));
        let swipe = widget.swipe.clone().expect("a swipe along take 2");
        assert_eq!(
            (swipe.row, swipe.lane, swipe.slots.as_slice()),
            (0, 2, &[0, 1, 2][..])
        );
        assert_eq!((swipe.track.as_str(), swipe.called.as_str()), ("vox", "2"));
        widget.handle_event(&UiEvent::PointerMove(pointer(to, y)));
        let (a, b) = widget.swipe.as_ref().expect("followed").span();
        assert!((a - 4.0).abs() < 0.05 && (b - 6.0).abs() < 0.05, "{a}..{b}");
        widget.handle_event(&UiEvent::PointerUp(pointer(to, y)));
        assert!(widget.swipe.is_none(), "let go");

        let (x, y) = place(&widget, 0.0, 4.0);
        widget.handle_event(&UiEvent::PointerDown(pointer(x, y)));
        assert!(widget.swipe.is_none(), "the comp is swiped onto, not from");
        widget.handle_event(&UiEvent::PointerUp(pointer(x, y)));

        let _ = crate::lanes::set_expanded(&["vox".to_owned()], Some(false));
        widget.recut();
        let (x, y) = place(&widget, 2.0, 4.0);
        widget.handle_event(&UiEvent::PointerDown(pointer(x, y)));
        assert!(widget.swipe.is_none(), "the lanes shut: the editor's press");
    }

    #[test]
    fn dragging_off_a_button_cancels_it() {
        let mut widget = widget();
        let (x, y) = at(&widget, 1, C::Solo);
        let (ax, ay) = at(&widget, 3, C::Mute);
        widget.handle_event(&UiEvent::PointerDown(pointer(x, y)));
        widget.handle_event(&UiEvent::PointerUp(pointer(ax, ay)));
        assert!(
            widget.edits.borrow().is_empty(),
            "releasing somewhere else is how every toolkit says 'no'"
        );
        assert!(!widget.rows[1].0.soloed);
    }

    /// A bound key acts on the session, with no rename field open.
    ///
    /// The window used to drop every key that was not going into a
    /// rename: `typed` returned false the moment `renaming` was `None`,
    /// so Delete, Home and every other binding reached the widget and
    /// died there. It looked exactly like a window that was not
    /// receiving keyboard events at all, which is what made it take a
    /// day to find — see #124.
    #[test]
    fn a_bound_key_reaches_the_session() {
        let mut widget = widget();
        // Delete, which the built-in profile binds to 40697. With
        // nothing selected it has nothing to remove and still answers
        // for itself, which is the whole point: the question here is
        // whether the key ARRIVES, not what it does when it does.
        let acted = widget.took(&UiEvent::KeyDown(key(blitz_traits::events::Key::Delete)));
        assert!(acted, "a bound key did nothing at all");
    }

    /// And a key with nothing behind it still does nothing, so the
    /// window does not spend a frame on every keystroke.
    #[test]
    fn an_unbound_key_costs_nothing() {
        let mut widget = widget();
        let acted = widget.took(&UiEvent::KeyDown(key(
            blitz_traits::events::Key::Character("§".into()),
        )));
        assert!(!acted, "an unbound key asked for a frame");
    }

    /// While a rename IS open the keyboard belongs to it: `d` types a
    /// d rather than deleting what is selected.
    #[test]
    fn a_rename_keeps_the_keyboard() {
        let mut widget = widget();
        let (x, y) = at(&widget, 1, C::Name);
        widget.handle_event(&UiEvent::PointerDown(pointer(x, y)));
        widget.handle_event(&UiEvent::PointerUp(pointer(x, y)));
        widget.handle_event(&UiEvent::PointerDown(pointer(x, y)));
        widget.handle_event(&UiEvent::PointerUp(pointer(x, y)));
        assert!(
            widget.renaming.is_some(),
            "the double click did not open one"
        );
        widget.handle_event(&UiEvent::KeyDown(key(
            blitz_traits::events::Key::Character("d".into()),
        )));
        let typed = widget
            .renaming
            .as_ref()
            .map(|r| r.text().to_owned())
            .unwrap_or_default();
        assert!(
            typed.ends_with('d'),
            "the letter went to the session instead of the field: {typed:?}"
        );
    }

    fn key(named: blitz_traits::events::Key) -> blitz_traits::events::BlitzKeyEvent {
        use blitz_traits::events::{BlitzKeyEvent, Code, KeyState, Location, Modifiers};
        BlitzKeyEvent {
            key: named,
            code: Code::Unidentified,
            modifiers: Modifiers::default(),
            location: Location::Standard,
            is_auto_repeating: false,
            is_composing: false,
            state: KeyState::Pressed,
            text: None,
        }
    }

    #[test]
    fn dragging_the_pan_knob_turns_it() {
        use blitz_traits::events::Key;
        let _ = Key::Escape;
        let mut widget = widget();
        let (x, y) = at(&widget, 2, C::Pan);
        let before = widget.rows[2].0.pan;
        widget.handle_event(&UiEvent::PointerDown(pointer(x, y)));
        // Up is more, so a drag UP raises the value.
        widget.handle_event(&UiEvent::PointerMove(pointer(x, y - 30.0)));
        let after = widget.rows[2].0.pan;
        assert!(after > before, "{before} -> {after}");
        assert!(matches!(
            widget.edits.borrow().last(),
            Some(Edit::SetPan(guid, _)) if guid == "t2"
        ));

        // Measured from the PRESS, not from the last frame: a second
        // move to the same place must mean the same value, or a slow
        // drag and a fast one over the same distance end up apart.
        widget.handle_event(&UiEvent::PointerMove(pointer(x, y - 30.0)));
        assert!((widget.rows[2].0.pan - after).abs() < f64::EPSILON);

        // And the release is not also a click on the knob.
        widget.handle_event(&UiEvent::PointerUp(pointer(x, y - 30.0)));
        assert!(widget.turning.is_none());
    }

    #[test]
    fn a_double_click_on_a_name_opens_a_rename_and_enter_commits_it() {
        use blitz_traits::events::Key;
        let mut widget = widget();
        let (x, y) = at(&widget, 0, C::Name);
        let click = |w: &mut ArrangementWidget| {
            w.handle_event(&UiEvent::PointerDown(pointer(x, y)));
            w.handle_event(&UiEvent::PointerUp(pointer(x, y)));
        };
        click(&mut widget);
        assert!(
            widget.renaming.is_none(),
            "one click selects, it does not rename"
        );
        click(&mut widget);
        assert!(
            widget.renaming.is_some(),
            "the second click inside the window renames"
        );

        widget.handle_event(&UiEvent::KeyDown(key(Key::Backspace)));
        widget.handle_event(&UiEvent::KeyDown(key(Key::Character("!".into()))));
        widget.handle_event(&UiEvent::KeyDown(key(Key::Enter)));
        assert!(widget.renaming.is_none(), "Enter closes the field");
        // "Track 0" with the 0 backspaced away and a ! typed in.
        assert_eq!(widget.rows[0].0.name, "Track !");
        assert!(matches!(
            widget.edits.borrow().last(),
            Some(Edit::Rename(guid, name)) if guid == "t0" && name == "Track !"
        ));
        // The name is recorded chrome, so the row has to be cut again
        // or the field closes on the old one.
        assert!(widget.scene.panel_zoom.is_nan(), "the panel was forgotten");
    }

    #[test]
    fn escape_abandons_a_rename() {
        use blitz_traits::events::Key;
        let mut widget = widget();
        let (x, y) = at(&widget, 0, C::Name);
        for _ in 0..2 {
            widget.handle_event(&UiEvent::PointerDown(pointer(x, y)));
            widget.handle_event(&UiEvent::PointerUp(pointer(x, y)));
        }
        widget.handle_event(&UiEvent::KeyDown(key(Key::Character("z".into()))));
        widget.handle_event(&UiEvent::KeyDown(key(Key::Escape)));
        assert!(widget.renaming.is_none());
        assert_eq!(widget.rows[0].0.name, "Track 0", "unchanged");
        assert!(
            !widget
                .edits
                .borrow()
                .iter()
                .any(|edit| matches!(edit, Edit::Rename(..)))
        );
    }

    /// The lanes start where the panel ends, in either shape: a point
    /// just past the compact panel is lane, not track, and reads the
    /// same time a point the same distance past the full one does.
    #[test]
    fn the_lanes_start_where_the_compact_panel_ends() {
        use crate::hit::Target;
        let mut widget = widget();
        let (_, y) = at(&widget, 0, C::Name);
        let lane_at = |widget: &ArrangementWidget, past: f64| match widget
            .hit(widget.scene.tcp.width() + past, y)
            .expect("a hit")
            .target
        {
            Target::Lane { row, seconds, .. } => (row, seconds),
            other => panic!("{past} px past the panel is {other:?}"),
        };
        let full = lane_at(&widget, 50.0);

        widget.compact.set(true);
        widget.recut();
        assert!(widget.scene.tcp.compact, "the toggle re-cuts the panel");
        assert!(widget.scene.tcp.width() < crate::arrangement::TCP_WIDTH);
        let compact = lane_at(&widget, 50.0);
        assert_eq!(full.0, compact.0);
        assert!((full.1 - compact.1).abs() < 1e-9, "{full:?} vs {compact:?}");
        // The width the panel gave up is lane now, not panel.
        let _ = lane_at(&widget, 1.0);
    }

    #[test]
    fn a_click_outside_the_panel_asks_for_nothing() {
        let mut widget = widget();
        let past = widget.scene.tcp.width() + 200.0;
        widget.handle_event(&UiEvent::PointerDown(pointer(past, 300.0)));
        widget.handle_event(&UiEvent::PointerUp(pointer(past, 300.0)));
        assert!(widget.edits.borrow().is_empty());
        assert_eq!(widget.pointer.hovered(), None);
    }
}
