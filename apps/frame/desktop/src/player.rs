//! One running surface against an engine, host-agnostic: the window and
//! the headless screenshot loop both drive this. The engine is the
//! in-process mock ([`Backend::Local`]) or a live one across a WebSocket
//! ([`Backend::Remote`]).

use std::collections::BTreeMap;
use std::time::Duration;

use document_proto::{NodeId, PaintKind, Scene};
use surface_memory::Memory;
use surface_proto::{Catalog, ControlKind, ParamAddr, ParamValue, Source as Binding};
use surface_runtime::sync::{Engine, EngineError, Incoming};
use surface_runtime::{Bounds, EngineCall, Input, Key, Modifiers, Runtime, Surface, sync};

use crate::input::Clicks;
use crate::remote::{Event, Remote};

/// Background when the surface's root frame has no solid fill.
const DARK: peniko::Color = peniko::Color::from_rgb8(0x0e, 0x0f, 0x12);
/// How long a scripted read of a remote engine may take.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Where the engine lives.
pub enum Backend {
    /// The in-process mock: synthetic meters, synchronous calls.
    Local(Memory),
    /// A live engine: telemetry and changes stream in, calls queue out.
    Remote(Remote),
}

impl Engine for Backend {
    async fn catalog(&self) -> Result<Catalog, EngineError> {
        match self {
            Self::Local(m) => sync::Local(m).catalog().await,
            Self::Remote(r) => r.catalog().await,
        }
    }

    async fn values(&self) -> Result<Vec<ParamValue>, EngineError> {
        match self {
            Self::Local(m) => sync::Local(m).values().await,
            Self::Remote(r) => r.values().await,
        }
    }

    async fn call(&self, call: EngineCall) -> Result<(), EngineError> {
        match self {
            Self::Local(m) => sync::Local(m).call(call).await,
            Self::Remote(r) => r.call(call).await,
        }
    }

    async fn request_feeds(&self, requests: Vec<surface_proto::FeedRequest>) -> Result<Vec<surface_proto::FeedInfo>, EngineError> {
        match self {
            Self::Local(m) => sync::Local(m).request_feeds(requests).await,
            Self::Remote(r) => r.request_feeds(requests).await,
        }
    }
}

/// A runtime laid out for one surface.
struct Built {
    rt: Runtime,
    page: NodeId,
    content: Bounds,
    background: peniko::Color,
    live: bool,
}

impl Built {
    fn new(mut rt: Runtime) -> Result<Self, String> {
        // Carets land where the painted glyphs end.
        rt.set_text_measure(std::sync::Arc::new(|data: &document_proto::TextData, text: &str| {
            render::text_advance(fonts(), &render::fonts::text_font(data), text, data.font_size, data.letter_spacing)
        }));
        let scene = &rt.surface().scene;
        let page = *scene.pages().first().ok_or("surface has no <Page>")?;
        let boxes = render::build_scene_layout_boxes(scene, page);
        let tops = scene.node(page).map(|p| p.children.clone()).unwrap_or_default();
        let content = union(tops.iter().filter_map(|id| boxes.get(id)).map(bounds));
        let background = tops.first().and_then(|id| solid_fill(scene, *id)).unwrap_or(DARK);
        let live = rt
            .surface()
            .controls
            .values()
            .any(|c| matches!(c.binding.as_ref().map(|b| &b.source), Some(Binding::Stream { .. })));
        rt.set_boxes(boxes.iter().map(|(id, b)| (*id, bounds(b))).collect::<Vec<_>>());
        Ok(Self { rt, page, content, background, live })
    }
}

pub struct Player {
    engine: Backend,
    rt: Runtime,
    page: NodeId,
    /// The page's top-level frames' union: what gets fit to the window.
    content: Bounds,
    background: peniko::Color,
    /// Any control bound to a stream (meters, readouts) → redraw continuously.
    live: bool,
    /// Last-clicked control: keyboard input goes here.
    focus: Option<NodeId>,
    clicks: Clicks,
    /// Last cursor position, surface coordinates.
    cursor: Option<(f64, f64)>,
    /// The param being dragged: its echoes (possibly stale, mid-gesture)
    /// are ignored until release so the control doesn't fight the hand.
    dragging: Option<ParamAddr>,
    /// Why the remote engine is unreachable, while it is.
    disconnected: Option<String>,
    /// The remote engine came back with a different catalog.
    catalog_changed: bool,
}

impl Player {
    /// # Errors
    /// When the surface has no page, or the engine refuses.
    pub fn new(engine: Backend, surface: Surface) -> Result<Self, String> {
        let rt = pollster::block_on(sync::connect(&engine, surface)).map_err(|e| format!("engine: {e}"))?;
        let b = Built::new(rt)?;
        Ok(Self {
            engine,
            rt: b.rt,
            page: b.page,
            content: b.content,
            background: b.background,
            live: b.live,
            focus: None,
            clicks: Clicks::default(),
            cursor: None,
            dragging: None,
            disconnected: None,
            catalog_changed: false,
        })
    }

    /// Swap in a new design in place (a reload, a live update, the next
    /// surface of a gallery), keeping the engine and the running state:
    /// every value (incl. edits still in flight), meter ballistics, and a
    /// gesture in progress on a control that is still there.
    ///
    /// # Errors
    /// When the surface has no page; the old surface stays.
    pub fn swap(&mut self, surface: Surface) -> Result<(), String> {
        if surface.scene.pages().is_empty() {
            return Err("surface has no <Page>".into());
        }
        let dragging = self.dragging.take();
        let placeholder = Runtime::new(Surface::from_scene(Scene::new("")), Catalog::default());
        let mut rt = std::mem::replace(&mut self.rt, placeholder);
        rt.swap_surface(surface);
        if let Backend::Local(engine) = &self.engine {
            // The in-process engine is the truth (other callers may have
            // changed it); only a param mid-drag keeps its optimistic value.
            let values = surface_proto::SurfaceEngine::values(engine);
            rt.apply_values(values.into_iter().filter(|v| dragging.as_ref() != Some(&v.addr)));
            rt.apply_texts(surface_proto::SurfaceEngine::texts(engine));
        }
        let b = Built::new(rt)?;
        self.install(b);
        // Focus and a drag survive when their control does.
        let alive = |id: &NodeId| self.rt.surface().controls.contains_key(id);
        self.focus = self.focus.filter(alive);
        self.dragging = dragging.filter(|addr| {
            self.rt.surface().controls.values().any(|c| matches!(c.binding.as_ref().map(|b| &b.source), Some(Binding::Param { addr: a }) if a == addr))
        });
        Ok(())
    }

    fn install(&mut self, b: Built) {
        self.rt = b.rt;
        self.page = b.page;
        self.content = b.content;
        self.background = b.background;
        self.live = b.live;
        self.dragging = None;
    }

    #[must_use]
    pub fn content(&self) -> Bounds {
        self.content
    }

    #[must_use]
    pub fn background(&self) -> peniko::Color {
        self.background
    }

    #[cfg(test)]
    #[must_use]
    pub fn runtime(&self) -> &Runtime {
        &self.rt
    }

    /// The engine's value for `addr` (for scripted checks: a remote read
    /// blocks until the calls queued before it have landed).
    #[must_use]
    pub fn engine_value(&self, addr: &ParamAddr) -> Option<f64> {
        match &self.engine {
            Backend::Local(m) => m.value(addr),
            Backend::Remote(r) => match r.fetch_values(READ_TIMEOUT) {
                Ok(values) => values.into_iter().find(|v| v.addr == *addr).map(|v| v.value),
                Err(e) => {
                    eprintln!("frame: reading {addr} from the engine: {e}");
                    None
                }
            },
        }
    }

    /// "engine disconnected …" while a remote engine is unreachable.
    #[must_use]
    pub fn engine_banner(&self) -> Option<String> {
        self.disconnected.as_ref().map(|why| format!("engine disconnected — reconnecting… ({why})"))
    }

    /// Whether the remote engine reconnected with a different catalog since
    /// the last call (a generated surface should be regenerated).
    pub fn take_catalog_changed(&mut self) -> bool {
        std::mem::take(&mut self.catalog_changed)
    }

    /// The design on stage (controls stored on their nodes).
    #[must_use]
    pub fn scene(&self) -> &Scene {
        &self.rt.surface().scene
    }

    /// The catalog the surface runs against.
    #[must_use]
    pub fn catalog(&self) -> &Catalog {
        self.rt.catalog()
    }

    /// Whether something moves without input (meters, a drag in flight,
    /// or a live engine that can push changes any time).
    #[must_use]
    pub fn animating(&self) -> bool {
        self.live || self.rt.animating() || matches!(self.engine, Backend::Remote(_))
    }

    /// Advance to `t` ms: telemetry in (synthetic, or drained from the live
    /// engine), coalesced edits out.
    pub fn tick(&mut self, t: f64) {
        self.rt.set_clock(t / 1000.0);
        match &mut self.engine {
            Backend::Local(m) => {
                // Shaped by whatever feeds were requested of the mock.
                for frame in m.frames(t / 1000.0) {
                    self.rt.apply_frame(frame, t);
                }
            }
            Backend::Remote(remote) => {
                for event in remote.drain() {
                    match event {
                        Event::Engine(Incoming::Param(v)) if self.dragging.as_ref() == Some(&v.addr) => {}
                        Event::Engine(incoming) => incoming.apply(&mut self.rt, t),
                        Event::Disconnected(why) => self.disconnected = Some(why),
                        Event::Connected { catalog, values } => {
                            self.disconnected = None;
                            if catalog != *self.rt.catalog() {
                                self.catalog_changed = true;
                                let surface = self.rt.surface().clone();
                                self.rt = Runtime::new(surface, catalog);
                                let boxes = render::build_scene_layout_boxes(&self.rt.surface().scene, self.page);
                                self.rt.set_boxes(boxes.iter().map(|(id, b)| (*id, bounds(b))).collect::<Vec<_>>());
                            }
                            self.rt.apply_values(values);
                        }
                    }
                }
            }
        }
        let calls = self.rt.poll(t);
        self.dispatch(calls);
    }

    // ── input (surface coordinates; `logical` is for click distance) ─────

    pub fn pointer_down(&mut self, at: (f64, f64), logical: (f64, f64), mods: Modifiers, t: f64) {
        self.cursor = Some(at);
        let double = self.clicks.press(logical.0, logical.1, t);
        let hit = self.rt.hit(at.0, at.1);
        if let Some(id) = hit {
            self.focus = Some(id);
        }
        self.dragging = hit.and_then(|id| self.param_of(id));
        let mut calls = self.rt.pointer(Input::Down { x: at.0, y: at.1, t, mods }, t);
        if double {
            calls.extend(self.rt.pointer(Input::DoubleClick, t));
        }
        self.dispatch(calls);
    }

    pub fn pointer_move(&mut self, at: (f64, f64), mods: Modifiers, t: f64) {
        self.cursor = Some(at);
        let calls = self.rt.pointer(Input::Move { x: at.0, y: at.1, t, mods }, t);
        self.dispatch(calls);
    }

    pub fn pointer_up(&mut self, t: f64) {
        let (x, y) = self.cursor.unwrap_or_default();
        let calls = self.rt.pointer(Input::Up { x, y, t }, t);
        self.dragging = None;
        self.dispatch(calls);
    }

    /// Capture lost (window blurred).
    pub fn cancel(&mut self, t: f64) {
        let calls = self.rt.pointer(Input::Cancel { t }, t);
        self.dragging = None;
        self.dispatch(calls);
    }

    /// Wheel over the hovered control.
    pub fn wheel(&mut self, notches: f64, mods: Modifiers, t: f64) {
        let Some(id) = self.cursor.and_then(|(x, y)| self.rt.hit(x, y)) else {
            return;
        };
        let calls = self.rt.feed(id, Input::Wheel { notches, mods }, t);
        self.dispatch(calls);
    }

    /// A key: to the text field being edited, an open menu, the
    /// selection, else the last-clicked control. False when nothing took
    /// it (the host may use it: Escape quits).
    pub fn key(&mut self, key: Key, mods: Modifiers, t: f64) -> bool {
        if let Some(calls) = self.rt.key(key, mods, t) {
            self.dispatch(calls);
            return true;
        }
        let Some(id) = self.focus else { return false };
        if matches!(key, Key::Escape | Key::Tab | Key::Char(_)) {
            return false;
        }
        let calls = self.rt.feed(id, Input::Key { key, mods }, t);
        self.dispatch(calls);
        true
    }

    /// Typed text (key characters, IME commits) into the field being
    /// edited; false when none is.
    pub fn text(&mut self, text: &str) -> bool {
        match self.rt.type_text(text) {
            Some(calls) => {
                self.dispatch(calls);
                true
            }
            None => false,
        }
    }

    /// Whether a text field is being edited (keys are text, IME is on).
    #[must_use]
    pub fn editing(&self) -> bool {
        self.rt.editing().is_some()
    }

    /// A right-click: the context menu of what's there.
    pub fn context(&mut self, at: (f64, f64), t: f64) {
        self.cursor = Some(at);
        let calls = self.rt.context_click(at.0, at.1, t);
        self.dispatch(calls);
    }

    /// The runtime (scripted checks: selection, menus, editing).
    #[must_use]
    pub fn rt(&self) -> &Runtime {
        &self.rt
    }

    fn dispatch(&mut self, calls: Vec<EngineCall>) {
        if calls.is_empty() {
            return;
        }
        let engine = match &self.engine {
            Backend::Local(engine) => engine,
            // Queued, never blocking; the engine's echo arrives via `tick`.
            Backend::Remote(remote) => return remote.send(calls),
        };
        let sent: Vec<ParamAddr> = calls
            .iter()
            .filter_map(|c| match c {
                EngineCall::SetParam { addr, .. } => Some(addr.clone()),
                _ => None,
            })
            .collect();
        if let Err(e) = pollster::block_on(sync::send(&sync::Local(engine), calls)) {
            eprintln!("frame: engine refused: {e:?}");
        }
        // Reflect what the engine actually holds (stepped params snap).
        // Only the sent params: others may have edits still coalescing.
        let echoed: Vec<ParamValue> =
            sent.into_iter().filter_map(|addr| engine.value(&addr).map(|value| ParamValue { addr, value })).collect();
        self.rt.apply_values(echoed);
    }

    // ── drawing ───────────────────────────────────────────────────────────

    /// The document at `t` ms and the page to paint, for the effects stage.
    #[must_use]
    pub fn document(&self, t: f64) -> (Scene, NodeId) {
        (self.rt.frame(t), self.page)
    }

    /// Shader nodes' live inputs this frame (feeds, params, curves).
    #[must_use]
    pub fn shader_inputs(&self) -> render::fx::FxInputs {
        self.rt.shader_inputs()
    }

    /// Every control's position, for effect shaders' `value`.
    #[must_use]
    pub fn control_positions(&self) -> std::collections::HashMap<NodeId, f64> {
        self.rt.surface().controls.iter().map(|(id, c)| (*id, self.rt.position(*id, c))).collect()
    }

    /// The first knob in paint order and its box (for scripted input).
    #[must_use]
    pub fn first_knob(&self) -> Option<(NodeId, Bounds)> {
        let s = self.rt.surface();
        let boxes = render::build_scene_layout_boxes(&s.scene, self.page);
        s.scene
            .walk(s.scene.root)
            .into_iter()
            .find(|id| s.controls.get(id).is_some_and(|c| matches!(c.kind, ControlKind::Knob { .. })))
            .and_then(|id| boxes.get(&id).map(|b| (id, bounds(b))))
    }

    /// The first handle in paint order and its hit box where its values
    /// put it (for scripted input).
    #[must_use]
    pub fn first_handle(&self) -> Option<(NodeId, Bounds)> {
        let s = self.rt.surface();
        s.scene
            .walk(s.scene.root)
            .into_iter()
            .find(|id| s.controls.get(id).is_some_and(|c| matches!(c.kind, ControlKind::Handle { .. })))
            .and_then(|id| self.rt.hit_box(id).map(|b| (id, b)))
    }

    /// Handles drawn now (in use), in paint order.
    #[must_use]
    pub fn shown_handles(&self) -> Vec<NodeId> {
        let s = self.rt.surface();
        s.scene
            .walk(s.scene.root)
            .into_iter()
            .filter(|id| s.controls.get(id).is_some_and(|c| matches!(c.kind, ControlKind::Handle { .. })) && self.rt.is_shown(*id))
            .collect()
    }

    /// The first canvas's box (a plot area you can add to).
    #[must_use]
    pub fn canvas(&self) -> Option<Bounds> {
        let s = self.rt.surface();
        let id = s.controls.iter().find(|(_, c)| matches!(c.kind, ControlKind::Canvas { .. }))?.0;
        self.rt.hit_box(*id)
    }

    /// The param a control is bound to (a handle's: its x, else its y).
    #[must_use]
    pub fn param_of(&self, id: NodeId) -> Option<ParamAddr> {
        let c = self.rt.surface().controls.get(&id)?;
        let binding = match &c.kind {
            ControlKind::Handle { x, y, .. } => x.as_ref().or(y.as_ref()),
            _ => c.binding.as_ref(),
        };
        match binding?.source.clone() {
            Binding::Param { addr } => Some(addr),
            _ => None,
        }
    }
}

/// The fonts surfaces are set in: frame's bundled book.
#[must_use]
pub fn fonts() -> &'static render::FontBook {
    render::FontBook::bundled()
}

/// A one-line red banner, `width` logical px wide, as a scene to paint
/// over the surface (so it uses the same text path as everything else).
#[must_use]
pub fn banner(message: &str, width: f64) -> Option<(Scene, NodeId)> {
    let text = serde_json::to_string(message).ok()?;
    let src = format!(
        r##"<Document><Page><Frame w={width} h=28 clip fill="#c62828e6"><Text x=10 y=6 w={w} h=16 size=12 fill="#ffffff" text={text} /></Frame></Page></Document>"##,
        w = (width - 20.0).max(0.0),
    );
    let scene = document_text::parse(&src, BTreeMap::new()).ok()?;
    let page = *scene.pages().first()?;
    Some((scene, page))
}

fn bounds(b: &render::LayoutBox) -> Bounds {
    Bounds { x: b.x, y: b.y, width: b.width, height: b.height }
}

fn union(boxes: impl Iterator<Item = Bounds>) -> Bounds {
    boxes
        .reduce(|a, b| {
            let (x, y) = (a.x.min(b.x), a.y.min(b.y));
            Bounds {
                x,
                y,
                width: (a.x + a.width).max(b.x + b.width) - x,
                height: (a.y + a.height).max(b.y + b.height) - y,
            }
        })
        .unwrap_or_default()
}

fn solid_fill(scene: &Scene, id: NodeId) -> Option<peniko::Color> {
    scene.node(id)?.fills.iter().rev().find(|p| p.visible).and_then(|p| match p.kind {
        PaintKind::Solid { color: c } => {
            let u8 = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            Some(peniko::Color::from_rgb8(u8(c.r), u8(c.g), u8(c.b)))
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::testing::Server;
    use surface_widgets::{Options, surface_for_catalog};

    fn demo() -> Player {
        let catalog = surface_memory::demo_catalog();
        Player::new(Backend::Local(Memory::demo()), surface_for_catalog(&catalog, &Options::default())).unwrap()
    }

    fn drag_first_knob(p: &mut Player, dy: f64) -> ParamAddr {
        let (id, b) = p.first_knob().unwrap();
        let addr = p.param_of(id).unwrap();
        let (cx, cy) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
        p.pointer_down((cx, cy), (cx, cy), Modifiers::default(), 0.0);
        p.pointer_move((cx, cy - dy), Modifiers::default(), 16.0);
        p.pointer_up(32.0);
        addr
    }

    #[test]
    fn content_and_background_come_from_the_root_frame() {
        let p = demo();
        let c = p.content();
        assert!(c.width > 100.0 && c.height > 100.0, "{c:?}");
        assert_eq!(p.background(), DARK, "stage-dark root fill is #0e0f12");
        assert!(p.animating(), "demo has meters");
    }

    #[test]
    fn drag_changes_the_engine_value() {
        let mut p = demo();
        let (id, _) = p.first_knob().unwrap();
        let before = p.engine_value(&p.param_of(id).unwrap()).unwrap();
        let addr = drag_first_knob(&mut p, 60.0);
        let after = p.engine_value(&addr).unwrap();
        assert!((after - before - 0.3).abs() < 1e-9, "{before} → {after}");
    }

    #[test]
    fn double_click_resets_and_keys_go_to_focus() {
        let mut p = demo();
        let (id, b) = p.first_knob().unwrap();
        let addr = p.param_of(id).unwrap();
        let default = p.engine_value(&addr).unwrap();
        let (cx, cy) = (b.x + b.width / 2.0, b.y + b.height / 2.0);
        p.pointer_down((cx, cy), (cx, cy), Modifiers::default(), 0.0);
        p.pointer_up(10.0);
        p.key(Key::PageUp, Modifiers::default(), 20.0);
        assert!(p.engine_value(&addr).unwrap() > default);
        p.pointer_down((cx, cy), (cx, cy), Modifiers::default(), 1000.0);
        p.pointer_up(1010.0);
        p.pointer_down((cx + 1.0, cy), (cx + 1.0, cy), Modifiers::default(), 1200.0);
        p.pointer_up(1210.0);
        assert!((p.engine_value(&addr).unwrap() - default).abs() < 1e-9, "double-click resets");
    }

    #[test]
    fn swap_keeps_engine_values() {
        let mut p = demo();
        let (id, _) = p.first_knob().unwrap();
        let addr = p.param_of(id).unwrap();
        pollster::block_on(sync::send(
            &p.engine,
            vec![EngineCall::SetParam { addr: addr.clone(), value: 0.9, phase: surface_proto::EditPhase::End }],
        ))
        .unwrap();
        let catalog = surface_memory::demo_catalog();
        p.swap(surface_for_catalog(&catalog, &Options::default())).unwrap();
        let (id, _) = p.first_knob().unwrap();
        let control = p.runtime().surface().controls[&id].clone();
        assert!((p.runtime().position(id, &control) - 0.9).abs() < 1e-9);
    }

    #[test]
    fn banner_parses_any_message() {
        let (scene, page) = banner("3:7: expected '>' \"quoted\" <tag> {brace}", 600.0).unwrap();
        assert_eq!(scene.node(page).map(|p| p.children.len()), Some(1));
    }

    #[test]
    fn telemetry_matches_the_catalog_streams() {
        let c = surface_memory::demo_catalog();
        let frames = feed_memory::frames(&c.streams, 1.25);
        assert_eq!(frames.len(), c.streams.len());
        assert_eq!(frames[0].values.len(), 2, "stereo out_level");
        // Every feed kind generates, at its shape.
        let all = Catalog { streams: feed_memory::demo_feeds(), ..Catalog::default() };
        for (f, info) in feed_memory::frames(&all.streams, 2.0).iter().zip(&all.streams) {
            assert_eq!(f.values.len(), info.kind.frame_len(), "{}", info.id);
        }
    }

    /// Tick until `done` (the remote side is asynchronous).
    fn tick_until(p: &mut Player, server: &Server, done: impl Fn(&Player) -> bool) {
        for i in 0..500 {
            server.engine.tick(f64::from(i) * 0.01);
            p.tick(100.0 + f64::from(i) * 10.0);
            if done(p) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("condition not reached within 5 s");
    }

    #[test]
    fn remote_engine_drives_the_player() {
        let mut server = Server::start(Memory::demo(), Some("guitar"));
        let mut remote = Remote::start(&server.url, Some("guitar")).unwrap();
        remote.wait_connected(Duration::from_secs(5)).unwrap();
        let catalog = pollster::block_on(remote.catalog()).unwrap();
        let mut p = Player::new(Backend::Remote(remote), surface_for_catalog(&catalog, &Options::default())).unwrap();
        assert!(p.animating());

        // Scripted drag lands on the served engine.
        let (id, _) = p.first_knob().unwrap();
        let before = p.engine_value(&p.param_of(id).unwrap()).unwrap();
        let addr = drag_first_knob(&mut p, 60.0);
        p.tick(40.0);
        let after = p.engine_value(&addr).unwrap();
        assert!((after - before - 0.3).abs() < 1e-9, "{before} → {after}");
        assert_eq!(server.engine.value(&addr), Some(after));

        // A change made elsewhere (another surface) streams in.
        let other = ParamAddr::new("reverb/mix");
        surface_proto::SurfaceEngine::set_param(&server.engine, other.clone(), 0.8, surface_proto::EditPhase::End).unwrap();
        tick_until(&mut p, &server, |p| p.runtime().values().iter().any(|v| v.addr == other && v.value == 0.8));

        // Losing the engine shows the banner and keeps the last state…
        server.kill();
        tick_until(&mut p, &server, |p| p.engine_banner().is_some());
        assert!(p.engine_banner().unwrap().starts_with("engine disconnected"));
        assert!(p.runtime().values().iter().any(|v| v.addr == other && v.value == 0.8), "last state kept");

        // …and a reconnect clears it.
        server.restart(Some("guitar"));
        tick_until(&mut p, &server, |p| p.engine_banner().is_none());
        assert!(!p.take_catalog_changed(), "same engine, same catalog");
    }
}
