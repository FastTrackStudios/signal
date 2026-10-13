//! Close-ups: any component, full screen.
//!
//! A strip's EQ is a thumbnail of an editor; a close-up is the editor.
//! One way in and out for everything that can be looked at closer — a
//! rack's EQ, its compressor, and whatever joins them — so the gesture is
//! learnt once:
//!
//! - **In**: the expand mark at the right of a panel's header, from any
//!   pointer. A widget asks ([`Closeups::ask`]) and the host that owns it
//!   carries the ask to the layer the next frame, since a painted widget
//!   cannot set a signal.
//! - **Out**: the back button, Escape, or a swipe down on the header.
//!
//! What is shown is a [`Closeup`]; each kind draws itself in the layer
//! ([`CloseupLayer`]). A rack panel is the rack's own drawing handed the
//! whole screen (`tone::Folded::zoomed`) and edited through the same
//! grips — the same settings the strip shows, so an edit here is an
//! edit there.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use anyrender::Scene;
use blitz_traits::events::{BlitzPointerEvent, BlitzPointerId, MouseEventButton, UiEvent};
use dioxus::prelude::*;
use eq_ui::eq_graph_interaction::Mods;
use vello::kurbo::Affine;

use crate::tone::{Folded, Grip, Panel, Shared, Which};

/// What a close-up is of.
#[derive(Clone, PartialEq, Debug)]
pub enum Closeup {
    /// One panel of a track's rack — its EQ, its compressor, its
    /// saturation.
    Rack {
        guid: String,
        /// The track's name, for the title.
        name: String,
        which: Which,
    },
}

impl Closeup {
    /// The track's name, for the header — what the tabs beside it are
    /// the parts of.
    #[must_use]
    pub fn subject(&self) -> &str {
        match self {
            Self::Rack { name, .. } => name,
        }
    }
}

/// The close-up showing, and where a widget asks for one: a context the
/// shell provides, with the racks' settings every rack view shares.
#[derive(Clone)]
pub struct Closeups {
    pub current: Signal<Option<Closeup>>,
    asks: Rc<RefCell<Option<Closeup>>>,
    /// The racks' settings: the mixer's strips and a close-up of one
    /// panel edit the same ones.
    pub tone: Rc<Shared>,
}

impl Closeups {
    #[must_use]
    pub fn new() -> Self {
        Self {
            current: Signal::new(None),
            asks: Rc::default(),
            tone: Rc::default(),
        }
    }

    /// A widget's ask for a close-up, carried to the layer by
    /// [`Closeups::take_ask`].
    pub fn ask(&self, closeup: Closeup) {
        *self.asks.borrow_mut() = Some(closeup);
    }

    /// Show what a widget asked for, if it asked: called once a frame by
    /// the host of a widget that can ask.
    pub fn take_ask(&self) {
        let asked = self.asks.borrow_mut().take();
        if let Some(closeup) = asked {
            let mut current = self.current;
            current.set(Some(closeup));
        }
    }
}

impl Default for Closeups {
    fn default() -> Self {
        Self::new()
    }
}

/// The modifiers a pointer held, as the rack's editing reads them: Alt,
/// Shift, and Ctrl or Command.
#[must_use]
pub fn mods_of(e: &BlitzPointerEvent) -> Mods {
    use blitz_traits::events::Modifiers;
    Mods::new(
        e.mods.contains(Modifiers::ALT),
        e.mods.contains(Modifiers::SHIFT),
        e.mods.contains(Modifiers::CONTROL) || e.mods.contains(Modifiers::META),
    )
}

/// How far down a finger swipes the header before the close-up closes.
const SWIPE_CLOSE: f64 = 60.0;

/// The close-up layer: over the whole window while something is shown,
/// nothing otherwise. Mounted once, by the shell, which says whether the
/// window is on its side: then the tabs are a column down the left,
/// where a wide screen has room, rather than a row taking height from
/// a short one.
#[component]
pub fn CloseupLayer(landscape: bool) -> Element {
    let closeups = try_use_context::<Closeups>();
    let mut swipe = use_signal(|| None::<f64>);
    #[cfg(feature = "native")]
    {
        let closeups = closeups.clone();
        dioxus_native::use_window_event(move |event, _| {
            if let winit::event::WindowEvent::KeyboardInput { event, .. } = event
                && event.state.is_pressed()
                && event.logical_key
                    == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape)
                && let Some(closeups) = &closeups
            {
                let mut current = closeups.current;
                if current.peek().is_some() {
                    current.set(None);
                }
            }
        });
    }
    let Some(closeups) = closeups else {
        return rsx! {};
    };
    let mut current = closeups.current;
    let Some(shown) = current() else {
        return rsx! {};
    };
    let subject = shown.subject().to_owned();
    // The close-ups beside this one: a rack's other panels, as tabs, so
    // going from its EQ to its compressor is one tap rather than out
    // and back in.
    let tabs: Vec<(Closeup, &'static str, bool)> = match &shown {
        Closeup::Rack { guid, name, which } => {
            let store = closeups.tone.store.borrow();
            let chain = crate::tone::panels_for(session::mix_phases::MixPhase::Tone);
            let panels = store.get(guid).map_or(chain, |tone| tone.panels(chain));
            panels
                .iter()
                .filter(|w| !w.name().is_empty())
                .map(|&w| {
                    let to = Closeup::Rack {
                        guid: guid.clone(),
                        name: name.clone(),
                        which: w,
                    };
                    (to, w.name(), w == *which)
                })
                .collect()
        }
    };
    let tab_style = move |on: bool| {
        let shape = if landscape {
            "flex:none; height:40px; width:100%; padding:0 12px; text-align:left; \
             display:flex; align-items:center;"
        } else {
            "flex:none; height:30px; padding:0 12px;"
        };
        let look = if on {
            "border:1px solid #3b82f6; background:#1e3a5f; color:#e5e7eb; font-weight:650;"
        } else {
            "border:1px solid #2a2c31; background:#1c1e22; color:#9ca3af; font-weight:600;"
        };
        format!(
            "{shape} {look} border-radius:7px; font-family:inherit; font-size:12px; \
             letter-spacing:0.04em; white-space:nowrap;"
        )
    };
    let tab_buttons = tabs.into_iter().map(move |(to, label, on)| {
        rsx! {
            button {
                key: "{label}",
                style: tab_style(on),
                onclick: move |_| current.set(Some(to.clone())),
                "{label}"
            }
        }
    });
    let (row_tabs, column_tabs) = if landscape {
        (None, Some(tab_buttons))
    } else {
        (Some(tab_buttons), None)
    };
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; width:100vw; height:100vh; z-index:300; \
                    display:flex; flex-direction:column; background:#0f1012; color:#e5e7eb; \
                    font-family:system-ui, sans-serif;",
            // The header: the way back, what this is, and a swipe down on
            // it closes too.
            div {
                style: "flex:none; height:48px; display:flex; align-items:center; gap:12px; \
                        padding:0 12px; background:#17181b; border-bottom:1px solid #2a2c31;",
                onpointerdown: move |e| swipe.set(Some(e.data().client_coordinates().y)),
                onpointermove: move |e| {
                    if let Some(from) = swipe() && e.data().client_coordinates().y - from > SWIPE_CLOSE {
                        swipe.set(None);
                        current.set(None);
                    }
                },
                onpointerup: move |_| swipe.set(None),
                onpointercancel: move |_| swipe.set(None),
                button {
                    style: "height:34px; display:flex; align-items:center; gap:6px; padding:0 12px 0 8px; \
                            border-radius:8px; border:1px solid #2a2c31; background:#1c1e22; \
                            color:#e5e7eb; font-family:inherit; font-size:14px; cursor:pointer;",
                    onclick: move |_| current.set(None),
                    lucide_dioxus::ChevronLeft { size: 18, color: "currentColor" }
                    "Back"
                }
                span {
                    style: "flex:none; font-size:15px; font-weight:650; white-space:nowrap;",
                    "{subject}"
                }
                if let Some(tabs) = row_tabs {
                    div {
                        style: "flex:1; min-width:0; display:flex; gap:4px; overflow-x:auto; \
                                scrollbar-width:none;",
                        {tabs}
                    }
                }
            }
            div {
                style: "flex:1; min-height:0; display:flex; flex-direction:row;",
                if let Some(tabs) = column_tabs {
                    div {
                        style: "flex:none; width:150px; height:100%; display:flex; flex-direction:column; \
                                gap:4px; padding:10px 8px; overflow-y:auto; background:#141518; \
                                border-right:1px solid #2a2c31;",
                        {tabs}
                    }
                }
                div {
                    style: "position:relative; flex:1; min-width:0; height:100%;",
                    match shown {
                        Closeup::Rack { guid, which, .. } => rsx! {
                            RackCloseup { key: "{guid}-{which:?}", guid, which }
                        },
                    }
                }
            }
            // A swipe begun on the header goes on below it, where a
            // pointer's moves would reach the component rather than the
            // header: while one is under way, a sheet over the component
            // follows it instead.
            if swipe().is_some() {
                div {
                    style: "position:absolute; left:0; right:0; top:48px; bottom:0; z-index:1;",
                    onpointermove: move |e| {
                        if let Some(from) = swipe() && e.data().client_coordinates().y - from > SWIPE_CLOSE {
                            swipe.set(None);
                            current.set(None);
                        }
                    },
                    onpointerup: move |_| swipe.set(None),
                    onpointercancel: move |_| swipe.set(None),
                }
            }
        }
    }
}

/// One rack panel, the whole layer.
#[component]
fn RackCloseup(guid: String, which: Which) -> Element {
    let closeups: Closeups = use_context();
    let touch = crate::touch::use_touch();
    #[cfg(feature = "native")]
    {
        let widget = use_hook(|| {
            dioxus_native_dom::CustomWidgetAttr::new(RackWidget::new(
                Rc::clone(&closeups.tone),
                guid.clone(),
                which,
                touch,
            ))
        });
        rsx! {
            object {
                style: "position:absolute; left:0; top:0; width:100%; height:100%;",
                data: widget,
            }
        }
    }
    #[cfg(all(feature = "web", not(feature = "native")))]
    {
        let widget = use_hook(|| {
            crate::web_host::HostedRef(Rc::new(RefCell::new(RackWidget::new(
                Rc::clone(&closeups.tone),
                guid.clone(),
                which,
                touch,
            ))))
        });
        rsx! {
            crate::web_host::WidgetCanvas { widget, hidden: false }
        }
    }
    #[cfg(not(any(feature = "native", feature = "web")))]
    {
        let _ = (closeups, guid, which, touch);
        rsx! {}
    }
}

/// How far the panel stands in from the layer's edges.
const INSET: f64 = 16.0;

/// How much bigger a close-up draws its panel than it lays it out: the
/// panel's type, nodes and lines are a strip's, and at a screen's width
/// they would be a thumbnail's labels on a poster. Laid out at a width
/// of about this many pixels, then drawn to fill.
const LAYOUT_W: f64 = 520.0;

/// The most a close-up zooms: past it the panel is all type.
const ZOOM_MAX: f64 = 2.2;

/// A rack panel as a widget of its own: drawn to fill it, edited through
/// the same grips a strip's is.
struct RackWidget {
    tone: Rc<Shared>,
    guid: String,
    which: Which,
    palette: crate::arrangement::Palette,
    font: crate::text::Font,
    /// Where the panel was last laid out, in its own (zoomed) units.
    panel: Cell<Panel>,
    /// How much bigger it was drawn than laid out.
    zoom: Cell<f64>,
    /// The least it zooms: under a finger, never smaller than the strip
    /// it came from.
    zoom_floor: f64,
    /// The grip a pointer holds, and where it was.
    hold: Option<(BlitzPointerId, Grip, (f64, f64))>,
    /// The last grip pressed, and when: a second press soon after puts
    /// it back to its default.
    pressed: Option<(Grip, web_time::Instant)>,
    /// The grip under a mouse, lit.
    hover: Option<Grip>,
    dirty: Cell<bool>,
}

impl RackWidget {
    fn new(tone: Rc<Shared>, guid: String, which: Which, touch: bool) -> Self {
        let theme = daw_ui::theming::Theme::dark();
        Self {
            tone,
            guid,
            which,
            palette: crate::arrangement::Palette::from_theme(&theme),
            font: crate::text::Font::embedded().expect("the embedded font"),
            panel: Cell::new(Panel {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            }),
            zoom: Cell::new(1.0),
            zoom_floor: if touch { crate::touch::ZOOM } else { 1.0 },
            hold: None,
            pressed: None,
            hover: None,
            dirty: Cell::new(false),
        }
    }

    fn grip_at(&self, x: f64, y: f64) -> Option<Grip> {
        let store = self.tone.store.borrow();
        let tone = store.get(&self.guid)?;
        crate::tone::grip_at(
            &[self.which],
            tone,
            self.panel.get(),
            Folded::zoomed(),
            x,
            y,
        )
    }

    fn event(&mut self, event: &UiEvent) {
        // In the panel's own units: CSS pixels over the close-up's zoom.
        let zoom = self.zoom.get().max(f64::EPSILON);
        let at = |e: &BlitzPointerEvent| {
            (
                f64::from(e.coords.client_x) / zoom,
                f64::from(e.coords.client_y) / zoom,
            )
        };
        let changed = match event {
            UiEvent::PointerDown(e) if e.button == MouseEventButton::Main => {
                let (x, y) = at(e);
                self.press(e.id, x, y, mods_of(e))
            }
            UiEvent::PointerMove(e) => {
                let (x, y) = at(e);
                if let Some((id, grip, last)) = self.hold
                    && id == e.id
                {
                    self.hold = Some((id, grip, (x, y)));
                    let panel = self.panel.get();
                    if let Some(tone) = self.tone.store.borrow_mut().edit(&self.guid) {
                        crate::tone::drag(
                            tone,
                            grip,
                            &[self.which],
                            panel,
                            Folded::zoomed(),
                            mods_of(e),
                            x - last.0,
                            y - last.1,
                        );
                    }
                    self.tone.edited();
                    true
                } else if !e.is_finger() {
                    let over = self.grip_at(x, y);
                    let lit = over != self.hover;
                    self.hover = over;
                    lit
                } else {
                    false
                }
            }
            UiEvent::PointerUp(e) | UiEvent::PointerCancel(e) => {
                let held = self.hold.take_if(|(id, ..)| *id == e.id).is_some();
                if e.is_finger() {
                    self.hover = None;
                }
                held
            }
            _ => false,
        };
        self.dirty.set(changed);
    }

    /// A press: a switch flips, a second press on a grip resets it, a
    /// modified press on an EQ band bypasses or reshapes it, and any
    /// other grip is taken hold of.
    fn press(&mut self, id: BlitzPointerId, x: f64, y: f64, mods: Mods) -> bool {
        let Some(grip) = self.grip_at(x, y) else {
            return false;
        };
        let now = web_time::Instant::now();
        let again = self
            .pressed
            .is_some_and(|(g, at)| g == grip && now.duration_since(at) < crate::gesture::DOUBLE);
        self.pressed = Some((grip, now));
        let mut store = self.tone.store.borrow_mut();
        let Some(tone) = store.edit(&self.guid) else {
            return false;
        };
        if grip.is_switch() {
            crate::tone::toggle(tone, grip);
        } else if again {
            crate::tone::reset(tone, grip);
        } else if !matches!(grip, Grip::Band(which, index)
            if crate::tone::dot_click(tone, which, index, mods))
        {
            drop(store);
            self.hold = Some((id, grip, (x, y)));
            return true;
        }
        drop(store);
        self.tone.edited();
        true
    }

    fn paint_scene(&mut self, width: u32, height: u32, scale: f64) -> Scene {
        self.dirty.set(false);
        let scale = scale.max(f64::EPSILON);
        let (css_w, css_h) = (f64::from(width) / scale, f64::from(height) / scale);
        let zoom = (css_w.min(css_h * 1.4) / LAYOUT_W).clamp(self.zoom_floor, ZOOM_MAX);
        self.zoom.set(zoom);
        let (w, h) = (css_w / zoom, css_h / zoom);
        let panel = Panel {
            x: INSET,
            y: INSET,
            width: (w - INSET * 2.0).max(0.0),
            height: (h - INSET * 2.0).max(0.0),
        };
        self.panel.set(panel);
        let mut drawn = Scene::new();
        let lit = self.hold.map(|(_, grip, _)| grip).or(self.hover);
        {
            let store = self.tone.store.borrow();
            if let Some(tone) = store.get(&self.guid) {
                crate::tone::draw(
                    &mut drawn,
                    &self.palette,
                    &self.font,
                    tone,
                    &crate::live::Meters::default(),
                    &[self.which],
                    panel,
                    Folded::zoomed(),
                    lit,
                    None,
                );
            }
        }
        let mut out = Scene::new();
        anyrender::PaintScene::append_scene(&mut out, drawn, Affine::scale(scale * zoom));
        out
    }
}

#[cfg(feature = "native")]
impl blitz_dom::node::Widget for RackWidget {
    fn handle_event(&mut self, event: &UiEvent) {
        self.event(event);
    }

    fn needs_redraw(&self) -> bool {
        // Its meters and the playhead, while the transport moves.
        self.dirty.get() || crate::engine::moving()
    }

    fn paint(
        &mut self,
        _render_ctx: &mut dyn anyrender::RenderContext,
        _styles: &blitz_dom::node::ComputedStyles,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Scene {
        self.paint_scene(width, height, scale)
    }
}

#[cfg(feature = "web")]
impl crate::web_host::Hosted for RackWidget {
    fn paint(&mut self, width: u32, height: u32, scale: f64) -> Scene {
        self.paint_scene(width, height, scale)
    }
    fn event(&mut self, event: &UiEvent) {
        RackWidget::event(self, event);
    }
}
