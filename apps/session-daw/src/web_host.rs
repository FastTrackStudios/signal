//! The panels in a browser: dioxus-web, with each widget in a `<canvas>`.
//!
//! dioxus-web has no custom-widget mechanism of its own (that is
//! dioxus-native's `CustomWidgetAttr`), so this is one: a canvas per
//! widget, drawn every animation frame with vello_hybrid's WebGL2 renderer
//! (keyflow-ui's `wasm-graphics` pattern), and the canvas's DOM events
//! turned into the same `UiEvent`s Blitz hands a widget. The widgets
//! themselves (the arrangement, the mixer, the chart) are unchanged: they
//! paint a scene and take events, and do not know which host they are in.
//!
//! Sizes: a widget paints at the canvas's CSS size (events arrive in CSS
//! pixels, so the hit test agrees with the picture), and the scene is
//! scaled by `devicePixelRatio` into the canvas's backing store, so it is
//! sharp on a high-density display.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::str::FromStr as _;

use anyrender::PaintScene as _;
use blitz_traits::events::{
    BlitzKeyEvent, BlitzPointerEvent, BlitzPointerId, Code, Key, KeyState, Location, Modifiers,
    MouseEventButton, MouseEventButtons, PointerCoords, PointerDetails, UiEvent,
};
use dioxus::prelude::*;
use vello::kurbo::Affine;
use wasm_bindgen::JsCast as _;
use wasm_bindgen::closure::Closure;

use crate::panel::{Button, PanelEvent, WHEEL_LINE};

/// What a canvas hosts: something that paints a scene and takes events.
pub trait Hosted {
    fn paint(&mut self, width: u32, height: u32, scale: f64) -> anyrender::Scene;
    fn event(&mut self, event: &UiEvent);
}

impl Hosted for crate::widget::ArrangementWidget {
    fn paint(&mut self, width: u32, height: u32, scale: f64) -> anyrender::Scene {
        self.paint_scene(width, height, scale)
    }
    fn event(&mut self, event: &UiEvent) {
        crate::widget::ArrangementWidget::event(self, event);
    }
}

/// A hosted widget, shared between its canvas and whoever built it.
#[derive(Clone)]
pub struct HostedRef(pub Rc<RefCell<dyn Hosted>>);

impl PartialEq for HostedRef {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Where a canvas's node goes once mounted, for whoever gives it the
/// keyboard (the panel's `focus_node`).
#[derive(Clone)]
pub struct FocusSlot(pub Rc<RefCell<Option<Rc<MountedData>>>>);

impl FocusSlot {
    fn set(&self, node: Rc<MountedData>) {
        *self.0.borrow_mut() = Some(node);
    }
}

impl PartialEq for FocusSlot {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// A canvas element slot, equal by identity.
#[derive(Clone)]
pub struct ElementSlot(pub Rc<RefCell<Option<web_sys::HtmlElement>>>);

impl PartialEq for ElementSlot {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// vello_hybrid's WebGL2 renderer over one canvas.
struct Gpu {
    renderer: vello_hybrid::WebGlRenderer,
    resources: vello_hybrid::Resources,
    scene: vello_hybrid::Scene,
    images: rustc_hash::FxHashMap<u64, vello_common::paint::ImageId>,
    size: (u32, u32),
}

impl Gpu {
    fn new(canvas: &web_sys::HtmlCanvasElement, width: u32, height: u32) -> Self {
        Self {
            renderer: vello_hybrid::WebGlRenderer::new(canvas),
            resources: vello_hybrid::Resources::new(),
            scene: Self::scene_at(width, height),
            images: rustc_hash::FxHashMap::default(),
            size: (width, height),
        }
    }

    fn scene_at(width: u32, height: u32) -> vello_hybrid::Scene {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a canvas is well under 65k px"
        )]
        let (w, h) = (
            width.min(u32::from(u16::MAX)) as u16,
            height.min(u32::from(u16::MAX)) as u16,
        );
        vello_hybrid::Scene::new_with(w, h, vello_hybrid::RenderSettings::default())
    }

    fn resize(&mut self, width: u32, height: u32) {
        if self.size != (width, height) {
            self.size = (width, height);
            self.scene = Self::scene_at(width, height);
        }
    }

    /// Draw `scene` (in CSS pixels) scaled by `dpr` into the canvas.
    fn draw(&mut self, scene: anyrender::Scene, dpr: f64) {
        {
            let images = anyrender_vello_hybrid::WebGlImageManager::new(
                &mut self.renderer,
                &mut self.resources,
                &mut self.images,
            );
            let mut painter =
                anyrender_vello_hybrid::WebGlScenePainter::new(&mut self.scene, images);
            painter.append_scene(scene, Affine::scale(dpr));
        }
        let size = vello_hybrid::RenderSize {
            width: self.size.0,
            height: self.size.1,
        };
        if let Err(e) = self
            .renderer
            .render(&self.scene, &mut self.resources, &size)
        {
            tracing::error!(error = ?e, "canvas render failed");
        }
        self.scene.reset();
    }
}

/// The DOM's modifier keys, as Blitz's.
fn modifiers(m: dioxus::prelude::Modifiers) -> Modifiers {
    let mut out = Modifiers::empty();
    if m.shift() {
        out |= Modifiers::SHIFT;
    }
    if m.ctrl() {
        out |= Modifiers::CONTROL;
    }
    if m.alt() {
        out |= Modifiers::ALT;
    }
    if m.meta() {
        out |= Modifiers::META;
    }
    out
}

fn mods(m: dioxus::prelude::Modifiers) -> crate::mousemap::Mods {
    crate::mousemap::Mods {
        shift: m.shift(),
        ctrl: m.ctrl(),
        alt: m.alt(),
    }
}

/// A DOM pointer event, as the widget reads one: coordinates relative to
/// the canvas (Blitz makes them relative to the node the same way).
fn pointer(e: &PointerData) -> BlitzPointerEvent {
    #[expect(clippy::cast_possible_truncation, reason = "a coordinate on a page")]
    let (x, y) = {
        let at = e.element_coordinates();
        (at.x as f32, at.y as f32)
    };
    let button = match e.trigger_button() {
        Some(dioxus::html::input_data::MouseButton::Auxiliary) => MouseEventButton::Auxiliary,
        Some(dioxus::html::input_data::MouseButton::Secondary) => MouseEventButton::Secondary,
        _ => MouseEventButton::Main,
    };
    // Which pointer, so a widget can tell fingers apart (two faders at
    // once) and a finger from a mouse (touch gestures).
    let id = match e.pointer_type().as_str() {
        "touch" => BlitzPointerId::Finger(u64::from(e.pointer_id().unsigned_abs())),
        "pen" => BlitzPointerId::Pen,
        _ => BlitzPointerId::Mouse,
    };
    let mut buttons = MouseEventButtons::empty();
    for held in e.held_buttons() {
        buttons |= match held {
            dioxus::html::input_data::MouseButton::Primary => MouseEventButtons::Primary,
            dioxus::html::input_data::MouseButton::Secondary => MouseEventButtons::Secondary,
            dioxus::html::input_data::MouseButton::Auxiliary => MouseEventButtons::Auxiliary,
            dioxus::html::input_data::MouseButton::Fourth => MouseEventButtons::Fourth,
            dioxus::html::input_data::MouseButton::Fifth => MouseEventButtons::Fifth,
            dioxus::html::input_data::MouseButton::Unknown => MouseEventButtons::empty(),
        };
    }
    BlitzPointerEvent {
        id,
        is_primary: e.is_primary(),
        coords: PointerCoords {
            page_x: x,
            page_y: y,
            screen_x: x,
            screen_y: y,
            client_x: x,
            client_y: y,
        },
        button,
        buttons,
        mods: modifiers(e.modifiers()),
        details: PointerDetails::default(),
        element: blitz_traits::events::Point { x, y },
        active_pointers: std::sync::Arc::default(),
    }
}

/// A DOM key event, as the widget reads one: by its W3C names (the DOM
/// and Blitz are on different `keyboard-types` versions).
fn key(e: &KeyboardData, state: KeyState) -> BlitzKeyEvent {
    BlitzKeyEvent {
        key: Key::from_str(&e.key().to_string()).unwrap_or(Key::Unidentified),
        code: Code::from_str(&e.code().to_string()).unwrap_or(Code::Unidentified),
        modifiers: modifiers(e.modifiers()),
        location: Location::Standard,
        is_auto_repeating: e.is_auto_repeating(),
        is_composing: false,
        state,
        text: None,
    }
}

fn panel_button(e: &PointerData) -> Button {
    match e.trigger_button() {
        Some(dioxus::html::input_data::MouseButton::Primary) => Button::Left,
        Some(dioxus::html::input_data::MouseButton::Auxiliary) => Button::Middle,
        _ => Button::Other,
    }
}

/// One widget in a canvas: painted every animation frame, fed the canvas's
/// events. `panel`, when given, hears the panel-level events too (the
/// zoom spring, the hand, the wheel), and `frame` runs once a frame before
/// the paint.
#[component]
pub fn WidgetCanvas(
    widget: HostedRef,
    panel: Option<EventHandler<PanelEvent>>,
    frame: Option<EventHandler<()>>,
    /// Where the canvas's node goes, for whoever gives it the focus.
    focus: Option<FocusSlot>,
    /// Where the canvas element goes, for the pointer shape.
    element: Option<ElementSlot>,
    /// Out of sight: painted once, so it is ready, and not again until
    /// shown.
    #[props(default)]
    hidden: bool,
) -> Element {
    let alive = use_hook(|| Rc::new(Cell::new(true)));
    let hiding = use_hook(|| Rc::new(Cell::new(hidden)));
    hiding.set(hidden);
    {
        let alive = Rc::clone(&alive);
        use_drop(move || alive.set(false));
    }
    let to_panel = move |event: PanelEvent| {
        if let Some(panel) = panel {
            panel.call(event);
        }
    };
    let (w1, w2, w3, w4, w5, w6) = (
        widget.clone(),
        widget.clone(),
        widget.clone(),
        widget.clone(),
        widget.clone(),
        widget.clone(),
    );
    rsx! {
        canvas {
            style: "position:absolute; left:0; top:0; width:100%; height:100%; \
                    display:block; outline:none; touch-action:none;",
            tabindex: "0",
            onmounted: move |event| {
                let data = event.data();
                // Only the canvas that reads the keyboard takes the focus.
                let takes_focus = focus.is_some();
                if let Some(slot) = &focus {
                    slot.set(Rc::clone(&data));
                }
                let Some(canvas) = data
                    .downcast::<web_sys::Element>()
                    .and_then(|el| el.clone().dyn_into::<web_sys::HtmlCanvasElement>().ok())
                else {
                    tracing::error!("the widget's node is not a canvas");
                    return;
                };
                if let Some(slot) = &element {
                    *slot.0.borrow_mut() = Some(canvas.clone().unchecked_into());
                }
                if takes_focus {
                    let _ = canvas.focus();
                }
                start_painting(canvas, widget.clone(), frame, Rc::clone(&alive), Rc::clone(&hiding));
            },
            onpointermove: move |e| {
                to_panel(PanelEvent::Pointer { x: e.client_coordinates().x, y: e.client_coordinates().y });
                w1.0.borrow_mut().event(&UiEvent::PointerMove(pointer(&e)));
            },
            onpointerdown: move |e| {
                // Keep the pointer through a drag that leaves the canvas.
                if let Some(target) = e.data().downcast::<web_sys::PointerEvent>()
                    .and_then(|p| p.target())
                    .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                {
                    let _ = target.set_pointer_capture(e.pointer_id());
                }
                to_panel(PanelEvent::Button { button: panel_button(&e), pressed: true });
                w2.0.borrow_mut().event(&UiEvent::PointerDown(pointer(&e)));
            },
            onpointerup: move |e| {
                to_panel(PanelEvent::Button { button: panel_button(&e), pressed: false });
                w3.0.borrow_mut().event(&UiEvent::PointerUp(pointer(&e)));
            },
            // The browser took the pointer back (a system gesture, the
            // page scrolling): what it was doing ends here.
            onpointercancel: move |e| {
                w6.0.borrow_mut().event(&UiEvent::PointerCancel(pointer(&e)));
            },
            oncontextmenu: move |e| e.prevent_default(),
            onwheel: move |e| {
                e.prevent_default();
                let delta = e.delta();
                let (dx, dy) = match delta {
                    dioxus::html::geometry::WheelDelta::Pixels(v) => (v.x, v.y),
                    dioxus::html::geometry::WheelDelta::Lines(v) => (v.x * WHEEL_LINE, v.y * WHEEL_LINE),
                    dioxus::html::geometry::WheelDelta::Pages(v) => (v.x * 400.0, v.y * 400.0),
                };
                // The DOM's positive delta scrolls down; the panel's (winit's)
                // positive delta moves the content down.
                to_panel(PanelEvent::Modifiers(mods(e.modifiers())));
                to_panel(PanelEvent::Wheel { dx: -dx, dy: -dy });
            },
            onkeydown: move |e| {
                if e.code() == dioxus::prelude::Code::KeyZ {
                    crate::tool::set_zoom_held(true);
                    to_panel(PanelEvent::ZKey { pressed: true, repeat: e.is_auto_repeating() });
                }
                to_panel(PanelEvent::Modifiers(mods(e.modifiers())));
                // The page must not scroll on space or the arrows, nor take
                // a shortcut the session means.
                if !matches!(e.key(), dioxus::prelude::Key::Tab) {
                    e.prevent_default();
                }
                w4.0.borrow_mut().event(&UiEvent::KeyDown(key(&e, KeyState::Pressed)));
            },
            onkeyup: move |e| {
                if e.code() == dioxus::prelude::Code::KeyZ {
                    crate::tool::set_zoom_held(false);
                    to_panel(PanelEvent::ZKey { pressed: false, repeat: false });
                }
                to_panel(PanelEvent::Modifiers(mods(e.modifiers())));
                w5.0.borrow_mut().event(&UiEvent::KeyUp(key(&e, KeyState::Released)));
            },
        }
    }
}

/// The animation-frame loop for one canvas: size the backing store to the
/// canvas's CSS size times `devicePixelRatio`, run `frame`, paint.
fn start_painting(
    canvas: web_sys::HtmlCanvasElement,
    widget: HostedRef,
    frame: Option<EventHandler<()>>,
    alive: Rc<Cell<bool>>,
    hidden: Rc<Cell<bool>>,
) {
    let painted = Cell::new(false);
    let runtime = dioxus::dioxus_core::Runtime::current();
    let scope = dioxus::dioxus_core::current_scope_id();
    let gpu = Rc::new(RefCell::new(None::<Gpu>));
    let tick: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let again = Rc::clone(&tick);
    *tick.borrow_mut() = Some(Closure::new(move || {
        if !alive.get() {
            // Unmounted: stop, and let the closure go.
            again.borrow_mut().take();
            return;
        }
        let Some(window) = web_sys::window() else {
            return;
        };
        let dpr = window.device_pixel_ratio().max(1.0);
        let rect = canvas.get_bounding_client_rect();
        let (css_w, css_h) = (rect.width().max(1.0), rect.height().max(1.0));
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a canvas size"
        )]
        let (px_w, px_h) = ((css_w * dpr).round() as u32, (css_h * dpr).round() as u32);
        if canvas.width() != px_w || canvas.height() != px_h {
            canvas.set_width(px_w);
            canvas.set_height(px_h);
        }
        if let Some(frame) = frame {
            runtime.in_scope(scope, || frame.call(()));
        }
        // Hidden: painted once to be ready, then left until shown.
        if hidden.get() && painted.replace(true) {
            if let Some(next) = again.borrow().as_ref() {
                let _ = window.request_animation_frame(next.as_ref().unchecked_ref());
            }
            return;
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a canvas size"
        )]
        let scene = widget.0.borrow_mut().paint(css_w as u32, css_h as u32, 1.0);
        let mut gpu = gpu.borrow_mut();
        let gpu = gpu.get_or_insert_with(|| Gpu::new(&canvas, px_w, px_h));
        gpu.resize(px_w, px_h);
        gpu.draw(scene, dpr);
        if let Some(next) = again.borrow().as_ref() {
            let _ = window.request_animation_frame(next.as_ref().unchecked_ref());
        }
    }));
    if let (Some(window), Some(first)) = (web_sys::window(), tick.borrow().as_ref()) {
        let _ = window.request_animation_frame(first.as_ref().unchecked_ref());
    }
}

/// The arrangement panel in a browser: [`crate::panel`]'s behaviour, the
/// widget in a canvas, the shared chrome over it.
#[component]
pub fn WebArrangement(engine: crate::web_engine::EngineRef) -> Element {
    let element = use_hook(|| Rc::new(RefCell::new(None::<web_sys::HtmlElement>)));
    let sink: crate::tool::Sink = {
        let element = Rc::clone(&element);
        // A browser canvas takes no painted cursor here: each glyph's
        // nearest CSS shape.
        Rc::new(move |shape: crate::window_cursor::Shape| {
            if let Some(el) = element.borrow().as_ref() {
                let _ = el.style().set_property("cursor", shape.icon().name());
            }
        })
    };
    let to_engine: crate::panel::Engine = {
        let engine = engine.clone();
        Rc::new(move |edit| engine.send(edit))
    };
    let (panel, widget) = crate::panel::use_arrangement_panel(Some(sink), to_engine, |w| {
        HostedRef(Rc::new(RefCell::new(w)))
    });
    let colors = daw_ui::studio::lanes::Colors::from_theme(&daw_ui::theming::Theme::dark());
    let surface = colors.surface.clone();
    let mounted = Rc::clone(&panel.mounted);
    let (on_panel, on_frame) = (panel.clone(), panel.clone());
    let focus = FocusSlot(Rc::clone(&panel.focus_node));
    let playhead = engine.clone();
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; right:0; bottom:0; overflow:hidden; \
                    background:{surface};",
            onmounted: move |event| {
                *mounted.borrow_mut() = Some(event.data());
            },
            WidgetCanvas {
                widget,
                panel: move |e| on_panel.handle(e),
                frame: move |()| on_frame.handle(PanelEvent::Frame { play_at: playhead.position() }),
                focus,
                element: Some(ElementSlot(Rc::clone(&element))),
            }
            crate::panel::PanelChrome { panel: panel.clone() }
        }
    }
}

/// The web demo: open the page's songs (see
/// [`crate::web_engine::WebSource`]) and show them.
#[component]
pub fn WebDemo(
    source: crate::web_engine::WebSource,
    /// The guide sample library's share link: the click, count and cues.
    guide: Option<String>,
) -> Element {
    use crate::web_engine::Progress;
    // What the loading screen says: each step of the open, and why it is
    // trying again when it is.
    let progress = use_signal(|| Progress::Joining { retry: None });
    let mut opened = use_resource(move || {
        let (source, guide) = (source.clone(), guide.clone());
        async move {
            crate::web_engine::open(&source, guide.as_deref(), move |step| {
                let mut progress = progress;
                progress.set(step);
            })
            .await
            .map_err(|e| e.to_string())
        }
    });
    let state = opened.read();
    match &*state {
        None => rsx! { crate::loading::Loading { progress: progress() } },
        Some(Err(e)) => rsx! {
            crate::loading::LoadingFrame {
                headline: "The session did not open".to_owned(),
                detail: e.clone(),
                failed: true,
                button {
                    style: "margin-top:8px; height:32px; padding:0 16px; border-radius:8px; border:none; \
                            background:#3aa0ff; color:#0b0c0e; font-weight:600; font-size:13px; cursor:pointer;",
                    onclick: move |_| opened.restart(),
                    "Try again"
                }
            }
        },
        Some(Ok((engine, setlist))) => rsx! {
            DemoView { engine: engine.clone(), setlist: setlist.clone() }
        },
    }
}

/// The opened demo: the app's frame — the views, the setlist, the
/// transport and the mode — over whichever song is up.
#[component]
fn DemoView(engine: crate::web_engine::EngineRef, setlist: crate::setlist::Setlist) -> Element {
    use crate::compact::{CompactShell, PhoneView};
    use crate::shell::{SAFE_AREA, TopBar, View};
    use session::modes::Mode;

    // Audio starts on the page's first press or key: the only place a
    // browser allows it.
    use_hook(crate::web_audio::unlock_on_first_gesture);
    let view = use_signal(|| View::Daw);
    let mode = use_signal(|| Mode::Live);
    use_context_provider(|| mode);
    // The lyrics' Audience / Performer and layer, held across songs.
    use_context_provider(crate::lyrics_panel::LyricsChoice::new);
    // The songs — a signal from here on, which the tabs read and a pick
    // writes.
    let mut setlist = use_context_provider(|| Signal::new(setlist));
    // Each song in the mode it was last worked in.
    crate::song_modes::use_song_modes(setlist, mode);
    // In Mix mode, the guide folder muted.
    crate::mix_guide::use_mix_guide(setlist, mode);
    // The set's other songs, as they open behind the one on screen: each
    // takes its tab's place.
    use_future(move || async move {
        let Some(mut arrivals) = crate::web_engine::take_arrivals() else {
            return;
        };
        while let Some(arrival) = arrivals.recv().await {
            setlist.write().arrive(arrival);
        }
    });
    // Playing together, a song someone else picked is picked here too.
    crate::collab_bar::use_follow_song(setlist);
    // How the song on screen is heard (its reference, by default), and
    // whether a change to the mix just asked for its stems.
    let mut listening = use_signal(crate::web_engine::listening);
    let mut asking = use_signal(|| false);
    use_future(move || async move {
        use crate::web_engine::Notice;
        let mut notices = crate::web_engine::notices();
        listening.set(crate::web_engine::listening());
        while let Some(notice) = notices.recv().await {
            match notice {
                Notice::Listening(now) => listening.set(now),
                Notice::Blocked => asking.set(true),
            }
        }
    });
    // The page's shape: a phone's (or a window that small) takes the
    // small-screen layout, the same panels rearranged.
    let viewport = use_viewport_size();
    use_context_provider(|| crate::shell::WindowSize(viewport));
    let form = use_viewport_form(viewport);
    use_context_provider(|| form);
    let landscape = move || {
        let (w, h) = viewport();
        w > h
    };
    let phone_view = use_signal(|| PhoneView::Chart);
    // A song picked, from the tabs or the navigator: that song is current,
    // and the audio moves to it. Where the one it replaces had got to is
    // kept on its tab.
    let pick = move |index: usize| {
        let at = crate::engine::Transport::shared().map_or(0.0, |t| t.read().0);
        let picked = setlist
            .write()
            .pick(index, at)
            .map(|song| song.project.clone());
        if let Some(project) = picked {
            crate::open::switch_song(&project);
            // Playing together, everyone goes with it.
            crate::collab::transport_pressed();
        }
    };
    // The record view's song menu picks the same way.
    use_context_provider(|| crate::record_view::PickSong(Callback::new(pick)));
    // Touch mode: on where the page's pointer is a finger.
    use_context_provider(crate::touch::Touch::detect);
    // What is shown full screen, if anything, and the racks' settings
    // every view of them shares (`crate::closeup`).
    use_context_provider(crate::closeup::Closeups::new);
    crate::shell::use_pins();
    let current = setlist.read().current().cloned();
    if form().compact() {
        return rsx! {
            CompactShell {
                form: form(),
                view: phone_view,
                on_pick: pick,
                // Who is here and where the sound comes from: More's.
                more: rsx! {
                    div {
                        style: "display:flex; align-items:center; gap:8px; flex-wrap:wrap;",
                        crate::collab_bar::CollabBar {}
                        ListeningBadge { listening: listening(), asking }
                        crate::shell::AudioBadge { density: crate::shell::Density::Full }
                    }
                },
                body: rsx! {
                    if let Some(song) = current {
                        PhoneViews {
                            key: "{song.project}",
                            session: song.session.clone(),
                            engine: engine.clone(),
                            view: phone_view,
                        }
                    }
                },
            }
            crate::closeup::CloseupLayer { landscape: landscape() }
            if asking() {
                LoadMultitracks { listening: listening(), asking }
            }
        };
    }
    rsx! {
        // Inset from a phone's or a tablet's safe areas, which the page's
        // background (the bars' colour) fills — see `index.html`.
        div {
            style: "{SAFE_AREA}",
        div {
            style: "flex:1; min-width:0; min-height:0; position:relative; display:flex; \
                    flex-direction:column; background:#0f1012; color:#e5e7eb; \
                    font-family:system-ui, sans-serif;",
            TopBar {
                // Once, not per song: the live set this page is in — who is
                // here, together.
                badges: rsx! {
                    crate::collab_bar::CollabBar {}
                    ListeningBadge { listening: listening(), asking }
                },
                on_pick: pick,
            }

            // The navigator down the left when it is open, and the views
            // beside it.
            div {
                style: "flex:1; min-height:0; display:flex;",
                crate::shell::NavigatorColumn { on_pick: pick }
                div {
                    style: "position:relative; flex:1; min-width:0; display:flex; flex-direction:column;",
                    if let Some(song) = current.clone() {
                        // Keyed by the song: picking another remounts every
                        // panel on that song's session rather than patching
                        // the last one's.
                        SongViews { key: "{song.project}", session: song.session.clone(), engine: engine.clone(), view }
                    }
                }
            }
            // The views, across the foot of the page, and each view's own
            // controls beside them — the arrangement's the transport, which
            // reads the song it drives, so it is mounted per song.
            crate::shell::BottomBar {
                view,
                mode,
                width: Some(viewport().0).filter(|w| *w > 0.0),
                transport: rsx! {
                    if let Some(song) = current {
                        WithSong {
                            key: "{song.project}",
                            session: song.session.clone(),
                            crate::transport_bar::WebTransportBar { big: true }
                        }
                    }
                },
            }
            // Whatever is zoomed into, over all of it.
            crate::closeup::CloseupLayer { landscape: landscape() }
            if asking() {
                LoadMultitracks { listening: listening(), asking }
            }
        }
        }
    }
}

/// The page's size in CSS pixels, measured now and a few times a second
/// after — a phone turned on its side, a window resized.
fn use_viewport_size() -> Signal<(f64, f64)> {
    fn measure() -> (f64, f64) {
        web_sys::window().map_or((0.0, 0.0), |w| {
            let px = |v: Result<wasm_bindgen::JsValue, _>| {
                v.ok().and_then(|v| v.as_f64()).unwrap_or(0.0)
            };
            (px(w.inner_width()), px(w.inner_height()))
        })
    }
    let mut size = use_signal(measure);
    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(250).await;
            let now = measure();
            if *size.peek() != now {
                size.set(now);
            }
        }
    });
    size
}

/// The page's shape, from its size.
fn use_viewport_form(size: Signal<(f64, f64)>) -> Signal<crate::compact::Form> {
    let of = move || {
        let (w, h) = size();
        if w <= 0.0 {
            crate::compact::Form::Wide
        } else {
            crate::compact::Form::of(w, h)
        }
    };
    let mut form = use_signal(of);
    use_effect(move || {
        let now = of();
        if *form.peek() != now {
            form.set(now);
        }
    });
    form
}

/// The small-screen layout's views but Control (which the shell draws): the
/// same panels as the wide layout's, over one song.
#[component]
fn PhoneViews(
    session: crate::studio::StudioSession,
    engine: crate::web_engine::EngineRef,
    view: Signal<crate::compact::PhoneView>,
) -> Element {
    use crate::compact::PhoneView;
    use_context_provider(|| session);
    match view() {
        // The shell's own pages (`compact::CompactShell`).
        PhoneView::Control | PhoneView::More | PhoneView::Setup | PhoneView::Editor => rsx! {},
        PhoneView::Chart => rsx! { crate::chart_panel::WebChart { paged: true } },
        PhoneView::Lyrics => rsx! { crate::lyrics_panel::LyricsPanel {} },
        // Keyed apart: the same component in the same place would otherwise
        // be kept and handed the other view's props.
        PhoneView::Arrangement => {
            rsx! { crate::mixer_panel::WebDawPanels { key: "{view():?}", engine: engine.clone() } }
        }
        PhoneView::Mixer => {
            rsx! { crate::mixer_panel::WebDawPanels { key: "{view():?}", engine: engine.clone(), mixer_only: true } }
        }
    }
}

/// How the song on screen is heard, beside who is here: its reference (a
/// press offers the multitracks), the stems arriving, or nothing — by its
/// stems is how a song is simply played.
#[component]
fn ListeningBadge(listening: crate::web_engine::Listening, asking: Signal<bool>) -> Element {
    use crate::shell::{DIM, RULE};
    use crate::web_engine::Listening;
    let (label, title) = match listening {
        Listening::Reference { .. } => (
            "Reference mix",
            "You are hearing this song's reference mix. Load the multitracks to change the mix.",
        ),
        Listening::Loading => (
            "Loading multitracks…",
            "The stems take over once they are here.",
        ),
        Listening::Stems => return rsx! {},
    };
    rsx! {
        button {
            title: "{title}",
            style: "height:28px; box-sizing:border-box; flex:none; margin-left:8px; padding:0 10px; \
                    display:flex; align-items:center; gap:6px; border-radius:14px; \
                    border:1px solid {RULE}; background:#0f1012; color:{DIM}; \
                    font-size:12px; font-weight:600; cursor:pointer; white-space:nowrap;",
            onmousedown: move |event| event.stop_propagation(),
            onclick: move |_| {
                if matches!(listening, Listening::Reference { .. }) {
                    asking.set(true);
                }
            },
            span { style: "width:7px; height:7px; border-radius:4px; background:#e3b341; flex:none;" }
            "{label}"
        }
    }
}

/// The offer a change to the mix makes while the song is heard by its
/// reference: load every stem (what it costs, said), or keep listening.
#[component]
fn LoadMultitracks(listening: crate::web_engine::Listening, asking: Signal<bool>) -> Element {
    use crate::shell::{ACCENT, BAR_BG, DIM, RULE, TEXT};
    use crate::web_engine::Listening;
    let size = match listening {
        Listening::Reference { stems_mb } => format!("about {stems_mb} MB for this song"),
        _ => "the song's stems".to_owned(),
    };
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; width:100vw; height:100vh; z-index:60; \
                    display:flex; align-items:center; justify-content:center; \
                    background:rgba(0,0,0,0.55);",
            onclick: move |_| asking.set(false),
            div {
                style: "width:min(380px, calc(100vw - 32px)); box-sizing:border-box; padding:20px; \
                        display:flex; flex-direction:column; gap:12px; background:{BAR_BG}; \
                        border:1px solid {RULE}; border-radius:14px; color:{TEXT}; \
                        box-shadow:0 16px 40px rgba(0,0,0,0.6);",
                onclick: move |event| event.stop_propagation(),
                div { style: "font-size:15px; font-weight:700;", "Editing the mix needs the multitracks" }
                div {
                    style: "font-size:13px; line-height:1.5; color:{DIM};",
                    "You are hearing the reference mix — one stream, so the set plays without \
                     downloading every track. To change volumes, pans or mutes, load all the \
                     tracks: {size}, and the rest as you open them. The click and guide are \
                     yours to change either way."
                }
                div {
                    style: "display:flex; gap:8px; justify-content:flex-end; margin-top:4px;",
                    button {
                        style: "height:32px; padding:0 14px; border-radius:8px; border:1px solid {RULE}; \
                                background:transparent; color:{TEXT}; font-size:13px; cursor:pointer;",
                        onclick: move |_| asking.set(false),
                        "Keep listening"
                    }
                    button {
                        style: "height:32px; padding:0 14px; border-radius:8px; border:none; \
                                background:{ACCENT}; color:#0b0c0e; font-size:13px; font-weight:650; \
                                cursor:pointer;",
                        onclick: move |_| {
                            asking.set(false);
                            crate::web_engine::load_multitracks();
                        },
                        "Load multitracks"
                    }
                }
            }
        }
    }
}

/// Whatever it holds, over one song: that song's session, as context.
#[component]
fn WithSong(session: crate::studio::StudioSession, children: Element) -> Element {
    use_context_provider(|| session);
    children
}

/// The views, over one song: its session is what every panel below reads.
#[component]
fn SongViews(
    session: crate::studio::StudioSession,
    engine: crate::web_engine::EngineRef,
    view: Signal<crate::shell::View>,
) -> Element {
    use crate::shell::{OverviewLayout, View};
    use_context_provider(|| session);
    let mode: Signal<session::modes::Mode> = use_context();
    rsx! {
        crate::shell::PinnedProgress { view: view() }
        div {
            style: "position:relative; flex:1; min-height:0;",
            match view() {
                View::Setup => rsx! { crate::setup::SetupView {} },
                View::Performance => rsx! {
                    if mode() == session::modes::Mode::Record {
                        crate::record_view::RecordView {}
                    } else {
                        WebPerformance {}
                    }
                },
                View::Daw => rsx! { crate::mixer_panel::WebDawPanels { engine: engine.clone() } },
                View::Chart => rsx! { crate::chart_panel::WebChart { paged: true } },
                View::Lyrics => rsx! { crate::lyrics_panel::LyricsPanel {} },
                View::Editor => rsx! { crate::editor_view::EditorPanel {} },
                View::Mixer => rsx! {
                    crate::mixer_panel::WebDawPanels { engine: engine.clone(), mixer_only: true }
                },
                View::Fx => rsx! { crate::fx_view::FxPanel {} },
                View::Overview => rsx! {
                    OverviewLayout {
                        progress: rsx! { crate::progress::ProgressBar {} },
                        chart: rsx! { crate::chart_panel::WebChart { paged: true } },
                        // The song's synced lyrics under its chart, as on the
                        // desktop — from its Lyrics track.
                        under_chart: rsx! { crate::lyrics_panel::LyricsPanel {} },
                        panels: rsx! {
                            crate::mixer_panel::WebDawPanels {
                                engine: engine.clone(),
                                docked: true,
                            }
                        },
                    }
                },
            }
        }
        crate::shell::PinnedTransport { view: view() }
    }
}

/// The performance view: the progress, the chart, the transport buttons.
#[component]
fn WebPerformance() -> Element {
    rsx! {
        div {
            style: "position:absolute; top:0; left:0; right:0; bottom:0; display:flex; \
                    flex-direction:column; gap:16px; padding:16px;",
            crate::progress::ProgressBar {}
            div {
                style: "position:relative; flex:1; min-height:0; border-radius:8px; \
                        overflow:hidden; border:1px solid #2a2c31;",
                crate::chart_panel::WebChart { paged: true }
            }
            crate::progress::TransportButtons {}
        }
    }
}
