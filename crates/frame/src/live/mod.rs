//! A frame surface running live inside a host's renderer.
//!
//! [`LiveSurface`] is what a host with a GPU reaches for: it loads a page
//! from `.frame` markup (plus its catalog), watches both files and swaps a new
//! design in place when either changes — values and a drag in progress
//! survive — runs the surface's controls, and renders through the effects
//! stage on the host's own wgpu device into a texture the host composites.
//!
//! Coordinates in are the host's element pixels (logical); the surface is
//! fitted into the element uniformly and centred, like the desktop player
//! fits it into its window.
//!
//! Edits come out as [`EngineCall`]s ([`LiveSurface::take_calls`]); with no
//! engine answering, each edit is echoed back so the control follows the
//! hand. A host with an engine feeds the engine's values in with
//! [`LiveSurface::apply_values`].

#[cfg(feature = "anyrender-0-11")]
mod vectors;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::document::proto::{NodeId, PaintKind, Scene};
use crate::surface::proto::{Catalog, ParamAddr, ParamValue, Source as Binding, TextValue};
use crate::surface::runtime::{Bounds, EngineCall, Input, Runtime, Surface};
use vello::wgpu;
use vello::{AaConfig, Renderer, RendererOptions};

pub use crate::render::fx::Mode;
pub use crate::surface::runtime::EngineCall as Call;
pub use crate::surface::runtime::Modifiers;

/// How often to look at the files.
const POLL: Duration = Duration::from_millis(250);
/// Max gap and travel between two presses of a double-click.
const DOUBLE_CLICK_MS: f64 = 350.0;
const DOUBLE_CLICK_PX: f64 = 4.0;
/// Under a page with no solid fill: nothing — the host's panel shows
/// through (the texture is straight alpha).
const DARK: peniko::Color = peniko::Color::from_rgba8(0, 0, 0, 0);

/// Fit of the surface's content into the element: surface → element px.
#[derive(Debug, Clone, Copy, PartialEq)]
struct View {
    scale: f64,
    /// The vertical scale where it differs (a stretched picture).
    scale_y: f64,
    tx: f64,
    ty: f64,
}

impl View {
    fn fit(content: Bounds, width: f64, height: f64) -> Self {
        if content.width <= 0.0 || content.height <= 0.0 || width <= 0.0 || height <= 0.0 {
            return Self {
                scale: 1.0,
                scale_y: 1.0,
                tx: -content.x,
                ty: -content.y,
            };
        }
        let scale = (width / content.width).min(height / content.height);
        Self {
            scale,
            scale_y: scale,
            tx: (width - content.width * scale) / 2.0 - content.x * scale,
            ty: (height - content.height * scale) / 2.0 - content.y * scale,
        }
    }

    /// Stretched to fill `width × height` exactly (a picture, not a unit).
    fn fill(content: Bounds, width: f64, height: f64) -> Self {
        if content.width <= 0.0 || content.height <= 0.0 || width <= 0.0 || height <= 0.0 {
            return Self::fit(content, width, height);
        }
        let (sx, sy) = (width / content.width, height / content.height);
        Self {
            scale: sx,
            scale_y: sy,
            tx: -content.x * sx,
            ty: -content.y * sy,
        }
    }

    fn to_surface(self, x: f64, y: f64) -> (f64, f64) {
        ((x - self.tx) / self.scale, (y - self.ty) / self.scale_y)
    }

    fn affine(self) -> kurbo::Affine {
        kurbo::Affine::new([self.scale, 0.0, 0.0, self.scale_y, self.tx, self.ty])
    }
}

thread_local! {
    /// One vello renderer per device for every surface on this thread: its
    /// pipelines and buffers are the heavy part, and a host drawing twenty
    /// faces needs them once, not twenty times.
    static SHARED: std::cell::RefCell<Option<(wgpu::Device, Shared)>> = const { std::cell::RefCell::new(None) };
}

/// A device's renderer, and its effects stages' shared pipelines (each
/// effect compiled once, not once a face).
type Shared = (
    std::rc::Rc<std::cell::RefCell<Renderer>>,
    crate::render::fx::SharedPasses,
);

/// This thread's [`Shared`] for `device`, made on first use.
fn shared_renderer(device: &wgpu::Device) -> Result<Shared, String> {
    SHARED.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some((d, (r, passes))) = slot.as_ref()
            && d == device
        {
            return Ok((std::rc::Rc::clone(r), passes.clone()));
        }
        let renderer = Renderer::new(
            device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: [AaConfig::Area].into_iter().collect(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )
        .map_err(|e| format!("renderer: {e}"))?;
        let r = std::rc::Rc::new(std::cell::RefCell::new(renderer));
        let passes = crate::render::fx::SharedPasses::new(device);
        *slot = Some((device.clone(), (std::rc::Rc::clone(&r), passes.clone())));
        Ok((r, passes))
    })
}

/// The GPU half, on the host's device.
struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: std::rc::Rc<std::cell::RefCell<Renderer>>,
    /// The next frame checks vello's buffers (the first, and after a
    /// resize or a reload); the rest render straight through.
    check_next: bool,
    /// A deferred check found an overflow: checks wait (and band) from
    /// then on, since the surface will overflow again at that size.
    banded: bool,
    stage: crate::render::fx::Stage,
    /// The host's texture, which the frame is copied into.
    target: Option<(wgpu::Texture, (u32, u32))>,
}

/// One surface, live.
pub struct LiveSurface {
    markup: PathBuf,
    catalog_path: Option<PathBuf>,
    stamps: (Option<u64>, Option<u64>),
    next_check: Instant,
    rt: Runtime,
    page: NodeId,
    content: Bounds,
    background: peniko::Color,
    mode: Mode,
    gpu: Option<Gpu>,
    born: Instant,
    /// Element px → surface, as of the last render (logical px).
    view: View,
    cursor: Option<(f64, f64)>,
    last_press: Option<(f64, f64, f64)>,
    dragging: Option<ParamAddr>,
    calls: Vec<EngineCall>,
    /// Something changed since the last render.
    dirty: bool,
    time_dependent: bool,
    /// When the last render began, to pace clock-driven redraws.
    rendered_at: Option<std::time::Instant>,
    /// The pace of clock-driven renders.
    clock_frame: std::time::Duration,
    /// Whether motion may run on its own (see [`LiveSurface::set_animate`]).
    animate: bool,
    /// Held still, motion still runs until then after a change, so a
    /// needle's spring finishes its move rather than freezing mid-way.
    settle_until: Option<std::time::Instant>,
    error: Option<String>,
    /// Fill the element exactly rather than fit it (pictures).
    stretch: bool,
    /// Lay the face out for the element's proportions (see
    /// [`LiveSurface::set_responsive`]).
    responsive: bool,
    /// The face as drawn, to lay out afresh for each width.
    design: Option<Surface>,
    /// The width it is laid out at now (its drawn width until a
    /// responsive render changes it).
    laid_width: f64,
}

/// How often a face that only moves with the clock is rendered unless the
/// host says otherwise ([`LiveSurface::set_max_fps`]): 30 fps.
pub const CLOCK_FRAME: std::time::Duration = std::time::Duration::from_millis(33);

/// How long motion keeps running after a change on a face held still.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(600);

fn stamp(path: &Path) -> Option<u64> {
    crate::document::io::fingerprint(path)
}

fn load(markup: &Path, catalog: Option<&Path>) -> Result<(Surface, Catalog), String> {
    let scene = crate::document::io::load_surface(markup)
        .map_err(|e| format!("{}: {e}", markup.display()))?;
    let catalog = match catalog {
        Some(p) => {
            let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
            serde_json::from_str::<Catalog>(&text).map_err(|e| format!("{}: {e}", p.display()))?
        }
        None => Catalog::default(),
    };
    Ok((Surface::from_scene(scene), catalog))
}

fn bounds(b: &crate::render::LayoutBox) -> Bounds {
    Bounds {
        x: b.x,
        y: b.y,
        width: b.width,
        height: b.height,
    }
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
    scene
        .node(id)?
        .fills
        .iter()
        .rev()
        .find(|p| p.visible)
        .and_then(|p| match p.kind {
            PaintKind::Solid { color: c } => {
                let u8 = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                Some(peniko::Color::from_rgb8(u8(c.r), u8(c.g), u8(c.b)))
            }
            _ => None,
        })
}

impl LiveSurface {
    /// Open `markup` (a `.frame` file with its `.frame.assets/`) against the
    /// catalog at `catalog` (its params' ranges and defaults).
    ///
    /// # Errors
    /// When either file cannot be read, or the surface has no page.
    pub fn open(markup: impl Into<PathBuf>, catalog: Option<PathBuf>) -> Result<Self, String> {
        let markup = markup.into();
        let (surface, cat) = load(&markup, catalog.as_deref())?;
        let stamps = (stamp(&markup), catalog.as_deref().and_then(stamp));
        let mut me = Self {
            markup,
            catalog_path: catalog,
            stamps,
            next_check: Instant::now() + POLL,
            rt: Runtime::new(Surface::from_scene(Scene::new("")), Catalog::default()),
            page: NodeId::default(),
            content: Bounds::default(),
            background: DARK,
            mode: Mode::Gpu,
            gpu: None,
            born: Instant::now(),
            view: View {
                scale: 1.0,
                scale_y: 1.0,
                tx: 0.0,
                ty: 0.0,
            },
            stretch: false,
            responsive: false,
            design: Some(surface.clone()),
            laid_width: 0.0,
            cursor: None,
            last_press: None,
            dragging: None,
            calls: Vec::new(),
            dirty: true,
            time_dependent: false,
            rendered_at: None,
            clock_frame: CLOCK_FRAME,
            animate: true,
            settle_until: None,
            error: None,
        };
        me.install(Runtime::new(surface, cat))?;
        Ok(me)
    }

    /// Lay out a runtime and make it the one running.
    fn install(&mut self, mut rt: Runtime) -> Result<(), String> {
        rt.set_text_measure(std::sync::Arc::new(
            |data: &crate::document::proto::TextData, text: &str| {
                crate::render::text_advance(
                    crate::render::FontBook::bundled(),
                    &crate::render::fonts::text_font(data),
                    text,
                    data.font_size,
                    data.letter_spacing,
                )
            },
        ));
        let scene = &rt.surface().scene;
        let page = *scene.pages().first().ok_or("surface has no <Page>")?;
        let boxes = crate::render::build_scene_layout_boxes(scene, page);
        let tops = scene
            .node(page)
            .map(|p| p.children.clone())
            .unwrap_or_default();
        self.content = union(tops.iter().filter_map(|id| boxes.get(id)).map(bounds));
        self.background = tops
            .first()
            .and_then(|id| solid_fill(scene, *id))
            .unwrap_or(DARK);
        rt.set_boxes(
            boxes
                .iter()
                .map(|(id, b)| (*id, bounds(b)))
                .collect::<Vec<_>>(),
        );
        self.page = page;
        self.rt = rt;
        self.dirty = true;
        Ok(())
    }

    /// Stretch to fill the element exactly instead of fitting it whole and
    /// centred — for a picture (a filter's response, a visualiser), never
    /// a unit whose proportions are its look.
    pub fn set_stretch(&mut self, stretch: bool) {
        self.stretch = stretch;
        self.dirty = true;
    }

    /// Lay the face out for the element's proportions instead of fitting
    /// it as drawn: kept at the element's height, it is made as wide as
    /// the element (within 0.6–2.4× its drawn width), its parts following
    /// their [`ResizeX`](crate::document::proto::ResizeX) — windows and grounds
    /// stretch, knobs and labels keep their size and spread or close up.
    pub fn set_responsive(&mut self, responsive: bool) {
        self.responsive = responsive;
        self.dirty = true;
    }

    /// Relay the design for an element `width × height`, when its
    /// proportions call for another width than the one laid out.
    fn respond(&mut self, width: u32, height: u32) {
        let Some(design) = self.design.as_ref() else {
            return;
        };
        let scene = &design.scene;
        let Some(page) = scene.pages().first().copied() else {
            return;
        };
        let Some(root) = scene.node(page).and_then(|p| p.children.first().copied()) else {
            return;
        };
        let Some((dw, dh)) = scene.node(root).map(|n| (n.size.width, n.size.height)) else {
            return;
        };
        if dw <= 0.0 || dh <= 0.0 || width == 0 || height == 0 {
            return;
        }
        let want = (dh * f64::from(width) / f64::from(height)).clamp(dw * 0.6, dw * 2.4);
        let now = if self.laid_width > 0.0 {
            self.laid_width
        } else {
            dw
        };
        if (want - now).abs() <= dw * 0.005 {
            return;
        }
        let mut surface = design.clone();
        surface.scene.resize_width(root, want);
        let placeholder = Runtime::new(Surface::from_scene(Scene::new("")), Catalog::default());
        let mut rt = std::mem::replace(&mut self.rt, placeholder);
        rt.swap_surface(surface);
        if self.install(rt).is_ok() {
            self.laid_width = want;
        }
    }

    /// The effects mode (GPU by default).
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.dirty = true;
    }

    /// The surface's content size (its page's frames), surface units.
    #[must_use]
    pub fn content_size(&self) -> (f64, f64) {
        (self.content.width, self.content.height)
    }

    /// Why the last reload failed, while it stays failed.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Look at the files (at most every 250 ms) and swap in a changed
    /// design, keeping values and a drag on a control that is still there.
    /// True when something was swapped in.
    pub fn poll_reload(&mut self) -> bool {
        let now = Instant::now();
        if now < self.next_check {
            return false;
        }
        self.next_check = now + POLL;
        let stamps = (
            stamp(&self.markup),
            self.catalog_path.as_deref().and_then(stamp),
        );
        if stamps == self.stamps || stamps.0.is_none() {
            return false;
        }
        self.stamps = stamps;
        match load(&self.markup, self.catalog_path.as_deref()) {
            Ok((surface, catalog)) => {
                if surface.scene.pages().is_empty() {
                    self.error = Some("surface has no <Page>".into());
                    return false;
                }
                self.design = Some(surface.clone());
                self.laid_width = 0.0;
                let placeholder =
                    Runtime::new(Surface::from_scene(Scene::new("")), Catalog::default());
                let mut rt = std::mem::replace(&mut self.rt, placeholder);
                if catalog != *rt.catalog() {
                    // New params (a regenerated page): start from the old
                    // values where the addresses carry over.
                    let values = rt.values();
                    rt = Runtime::new(surface, catalog);
                    rt.apply_values(values);
                } else {
                    rt.swap_surface(surface);
                }
                match self.install(rt) {
                    Ok(()) => {
                        self.error = None;
                        if let Some(g) = self.gpu.as_mut() {
                            g.check_next = true;
                        }
                        true
                    }
                    Err(e) => {
                        self.error = Some(e);
                        false
                    }
                }
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }

    /// Values from the engine (normalized 0..1). A param being dragged
    /// keeps the hand's value.
    pub fn apply_values(&mut self, values: impl IntoIterator<Item = ParamValue>) {
        let dragging = self.dragging.clone();
        self.rt.apply_values(
            values
                .into_iter()
                .filter(|v| dragging.as_ref() != Some(&v.addr)),
        );
        self.dirty = true;
    }

    /// Text values from the engine, by address (a preset's name on its
    /// nameplate): whatever control is bound to the text param shows it.
    pub fn apply_texts(&mut self, texts: impl IntoIterator<Item = (String, String)>) {
        self.rt
            .apply_texts(texts.into_iter().map(|(addr, text)| TextValue {
                addr: ParamAddr::new(addr),
                text,
            }));
        self.dirty = true;
    }

    /// The calls the surface made since the last take (param edits,
    /// actions), for the host to send.
    pub fn take_calls(&mut self) -> Vec<EngineCall> {
        std::mem::take(&mut self.calls)
    }

    /// Values from the engine in their own units (Hz, dB, 0..1…), by
    /// address; each is normalized through the surface catalog's range.
    /// Addresses the catalog does not know are ignored.
    pub fn apply_real(&mut self, values: impl IntoIterator<Item = (String, f64)>) {
        let normalized: Vec<ParamValue> = values
            .into_iter()
            .filter_map(|(addr, real)| {
                let addr = ParamAddr::new(addr);
                let info = self.rt.catalog().param(&addr)?;
                Some(ParamValue {
                    value: info.range.to_normalized(real),
                    addr,
                })
            })
            .collect();
        self.apply_values(normalized);
    }

    /// Live readings from the engine, by stream id (`<ns>/spectrum`, a
    /// level's `<ns>/in`, a gain reduction's `<ns>/gr`, a trace's
    /// `<ns>/in_history`): the plots, meters and lamps bound to them move.
    /// A history the catalog calls a series is taken whole; any other
    /// reading is pushed onto its plot's ring.
    pub fn apply_stream(&mut self, id: &str, values: Vec<f64>) {
        let t = self.t();
        self.rt.apply_frame(
            crate::surface::proto::StreamFrame {
                id: crate::surface::proto::EntryId::new(id),
                values,
            },
            t,
        );
        self.dirty = true;
    }

    /// The param edits made since the last take, as (address, value in the
    /// param's own units) — the catalog's range turns the surface's
    /// normalized value back into Hz, dB, 0..1…
    pub fn take_edits(&mut self) -> Vec<(String, f64)> {
        let calls = std::mem::take(&mut self.calls);
        calls
            .into_iter()
            .filter_map(|c| match c {
                EngineCall::SetParam { addr, value, .. } => {
                    let real = self
                        .rt
                        .catalog()
                        .param(&addr)
                        .map_or(value, |p| p.range.to_real(value));
                    Some((addr.to_string(), real))
                }
                _ => None,
            })
            .collect()
    }

    /// The params the surface's catalog knows, by address.
    #[must_use]
    pub fn addresses(&self) -> Vec<String> {
        self.rt
            .catalog()
            .params
            .iter()
            .map(|p| p.addr.to_string())
            .collect()
    }

    /// Pace clock-driven renders to at most `fps` a second (the display's
    /// rate, say); 0 leaves them unpaced.
    pub fn set_max_fps(&mut self, fps: u32) {
        self.clock_frame = if fps == 0 {
            std::time::Duration::ZERO
        } else {
            std::time::Duration::from_secs_f64(1.0 / f64::from(fps))
        };
    }

    /// Let the face move on its own (LFOs, springs settling, clock-driven
    /// shaders) or hold it still: a host with nothing running (its audio
    /// stopped) holds it, and the face redraws only when something changes.
    /// A control in the hand still moves.
    pub fn set_animate(&mut self, animate: bool) {
        if animate != self.animate {
            self.animate = animate;
            self.dirty = true;
        }
    }

    /// Whether the face has something new to show or is moving: what a
    /// host asks to decide whether to draw another frame at all. Unlike
    /// [`Self::needs_redraw`] it ignores the clock pacing — a face whose
    /// LFO is between frames is still moving.
    #[must_use]
    pub fn is_moving(&self) -> bool {
        let moving = self.animate
            || self
                .settle_until
                .is_some_and(|at| std::time::Instant::now() < at);
        self.dirty || (moving && (self.time_dependent || self.rt.animating()))
    }

    /// Whether the host should render again: something changed (an edit,
    /// the pointer), or the face moves by itself — a shader reading the
    /// clock, a motion running — and a frame's time (its
    /// [max fps](Self::set_max_fps)) has passed. What moves by itself (an
    /// LFO lamp, a reverb's wavefront, a scope) is paced, not re-rendered
    /// at whatever rate the window happens to redraw; what the hand does is
    /// not.
    #[must_use]
    pub fn needs_redraw(&self) -> bool {
        let due = self
            .rendered_at
            .is_none_or(|at| at.elapsed() >= self.clock_frame);
        let moving = self.animate
            || self
                .settle_until
                .is_some_and(|at| std::time::Instant::now() < at);
        // A deferred check still out is read by the next render.
        let checking = self.gpu.as_ref().is_some_and(|g| g.stage.checking());
        self.dirty || checking || (moving && due && (self.time_dependent || self.rt.animating()))
    }

    fn t(&self) -> f64 {
        self.born.elapsed().as_secs_f64() * 1000.0
    }

    fn dispatch(&mut self, calls: Vec<EngineCall>) {
        if calls.is_empty() {
            return;
        }
        // No engine answers here: echo the edits so the controls follow.
        let echoed: Vec<ParamValue> = calls
            .iter()
            .filter_map(|c| match c {
                EngineCall::SetParam { addr, value, .. } => Some(ParamValue {
                    addr: addr.clone(),
                    value: *value,
                }),
                _ => None,
            })
            .collect();
        self.rt.apply_values(echoed);
        self.calls.extend(calls);
        self.dirty = true;
    }

    fn param_of(&self, id: NodeId) -> Option<ParamAddr> {
        let c = self.rt.surface().controls.get(&id)?;
        match c.binding.as_ref().map(|b| &b.source) {
            Some(Binding::Param { addr }) => Some(addr.clone()),
            _ => None,
        }
    }

    // ── input, in element px (logical) ──────────────────────────────────

    pub fn pointer_down(&mut self, x: f64, y: f64, mods: Modifiers) {
        let t = self.t();
        let at = self.view.to_surface(x, y);
        self.cursor = Some(at);
        let double = self.last_press.is_some_and(|(lx, ly, lt)| {
            t - lt <= DOUBLE_CLICK_MS && (x - lx).hypot(y - ly) <= DOUBLE_CLICK_PX
        });
        self.last_press = if double { None } else { Some((x, y, t)) };
        let hit = self.rt.hit(at.0, at.1);
        self.dragging = hit.and_then(|id| self.param_of(id));
        let mut calls = self.rt.pointer(
            Input::Down {
                x: at.0,
                y: at.1,
                t,
                mods,
            },
            t,
        );
        if double {
            calls.extend(self.rt.pointer(Input::DoubleClick, t));
        }
        self.dispatch(calls);
        self.dirty = true;
    }

    pub fn pointer_move(&mut self, x: f64, y: f64, mods: Modifiers) {
        let t = self.t();
        let at = self.view.to_surface(x, y);
        self.cursor = Some(at);
        let calls = self.rt.pointer(
            Input::Move {
                x: at.0,
                y: at.1,
                t,
                mods,
            },
            t,
        );
        if self.dragging.is_some() || !calls.is_empty() {
            self.dirty = true;
        }
        self.dispatch(calls);
    }

    pub fn pointer_up(&mut self) {
        let t = self.t();
        let (x, y) = self.cursor.unwrap_or_default();
        let calls = self.rt.pointer(Input::Up { x, y, t }, t);
        self.dragging = None;
        self.dispatch(calls);
        self.dirty = true;
    }

    pub fn cancel(&mut self) {
        let t = self.t();
        let calls = self.rt.pointer(Input::Cancel { t }, t);
        self.dragging = None;
        self.dispatch(calls);
    }

    /// Wheel over the control under the cursor (notches: + is up).
    pub fn wheel(&mut self, x: f64, y: f64, notches: f64, mods: Modifiers) {
        let t = self.t();
        let at = self.view.to_surface(x, y);
        self.cursor = Some(at);
        let Some(id) = self.rt.hit(at.0, at.1) else {
            return;
        };
        let calls = self.rt.feed(id, Input::Wheel { notches, mods }, t);
        self.dispatch(calls);
    }

    // ── drawing ─────────────────────────────────────────────────────────

    /// Take the host's device (once it has one).
    ///
    /// # Errors
    /// When vello cannot build its pipelines on it.
    pub fn attach_gpu(&mut self, device: wgpu::Device, queue: wgpu::Queue) -> Result<(), String> {
        let (renderer, passes) = shared_renderer(&device)?;
        let stage = crate::render::fx::Stage::with_passes(&passes);
        self.gpu = Some(Gpu {
            device,
            queue,
            renderer,
            check_next: true,
            banded: false,
            stage,
            target: None,
        });
        self.dirty = true;
        Ok(())
    }

    /// The host's device went away (drop the textures).
    pub fn detach_gpu(&mut self) {
        self.gpu = None;
    }

    /// Whether a device is attached.
    #[must_use]
    pub fn has_gpu(&self) -> bool {
        self.gpu.is_some()
    }

    /// Paint the surface's vectors at `width × height` physical px
    /// (`scale` physical px per element px) into `scene`, a host's anyrender
    /// 0.11 scene — the path for a host whose renderer gives no Vello GPU to
    /// [`render`](Self::render) with (vello-hybrid on the iOS simulator,
    /// whose Metal has no indirect execution). The same frame as `render`,
    /// painted by the same painter; the GPU effects stage is left out (a
    /// glow is drawn as a soft copy, as frame's fallback mode draws it).
    #[cfg(feature = "anyrender-0-11")]
    pub fn paint_vectors<S: anyrender_0_11::PaintScene>(
        &mut self,
        scene: &mut S,
        width: u32,
        height: u32,
        scale: f64,
    ) {
        if self.responsive {
            self.respond(width, height);
        }
        self.rendered_at = Some(std::time::Instant::now());
        if self.dirty {
            self.settle_until = self.rendered_at.map(|at| at + SETTLE);
        }
        let t = self.t();
        let calls = self.rt.poll(t);
        self.dispatch(calls);
        self.rt.set_clock(t / 1000.0);
        let (w, h) = (width.max(1), height.max(1));
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let physical = if self.stretch {
            View::fill(self.content, f64::from(w), f64::from(h))
        } else {
            View::fit(self.content, f64::from(w), f64::from(h))
        };
        self.view = View {
            scale: physical.scale / scale,
            scale_y: physical.scale_y / scale,
            tx: physical.tx / scale,
            ty: physical.ty / scale,
        };
        let doc = self.rt.frame(t);
        let mut target = vectors::AnyRender011(scene);
        let bounds = kurbo::Rect::new(0.0, 0.0, f64::from(w), f64::from(h));
        crate::render::PaintTarget::fill(
            &mut target,
            peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            self.background,
            None,
            &bounds,
        );
        let prims = crate::render::build_scene_primitives_with(
            &doc,
            self.page,
            crate::render::CollectOptions {
                emission_glow: true,
            },
        );
        crate::render::paint_primitives_into_scene_with(
            &mut target,
            &prims,
            physical.affine(),
            Some(crate::render::FontBook::bundled()),
        );
        self.time_dependent = false;
        self.dirty = false;
    }

    /// Paint the surface's vectors fitted to a `width × height` box placed
    /// by `transform` (the box's units to the scene's, as an affine's six
    /// coefficients) into a host's scene, with nothing behind it — a face
    /// drawn over a host's own picture, as one block of its grid, its
    /// transparent margin showing what is under it. The same frame as
    /// [`paint_vectors`](Self::paint_vectors), by the same painter.
    #[cfg(feature = "anyrender-0-11")]
    pub fn paint_vectors_at<S: anyrender_0_11::PaintScene>(
        &mut self,
        scene: &mut S,
        width: f64,
        height: f64,
        transform: [f64; 6],
    ) {
        self.rendered_at = Some(std::time::Instant::now());
        if self.dirty {
            self.settle_until = self.rendered_at.map(|at| at + SETTLE);
        }
        let t = self.t();
        let calls = self.rt.poll(t);
        self.dispatch(calls);
        self.rt.set_clock(t / 1000.0);
        let view = View::fit(self.content, width.max(1.0), height.max(1.0));
        self.view = view;
        let doc = self.rt.frame(t);
        let mut target = vectors::AnyRender011(scene);
        let prims = crate::render::build_scene_primitives_with(
            &doc,
            self.page,
            crate::render::CollectOptions {
                emission_glow: true,
            },
        );
        crate::render::paint_primitives_into_scene_with(
            &mut target,
            &prims,
            kurbo::Affine::new(transform) * view.affine(),
            Some(crate::render::FontBook::bundled()),
        );
        self.time_dependent = false;
        self.dirty = false;
    }

    /// Render at `width × height` physical px (`scale` physical px per
    /// element px) into the texture this keeps for the host, and return
    /// it with whether it was (re)made — a new texture must be registered
    /// again. `Rgba8Unorm`, straight alpha, usable as a texture binding and
    /// a copy source.
    ///
    /// # Errors
    /// With no device attached, or when vello fails.
    pub fn render(
        &mut self,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Result<(wgpu::Texture, bool), String> {
        if self.responsive {
            self.respond(width, height);
        }
        self.rendered_at = Some(std::time::Instant::now());
        if self.dirty {
            self.settle_until = self.rendered_at.map(|at| at + SETTLE);
        }
        let t = self.t();
        let calls = self.rt.poll(t);
        self.dispatch(calls);
        self.rt.set_clock(t / 1000.0);
        let (w, h) = (width.max(1), height.max(1));
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let physical = if self.stretch {
            View::fill(self.content, f64::from(w), f64::from(h))
        } else {
            View::fit(self.content, f64::from(w), f64::from(h))
        };
        self.view = View {
            scale: physical.scale / scale,
            scale_y: physical.scale_y / scale,
            tx: physical.tx / scale,
            ty: physical.ty / scale,
        };
        let doc = self.rt.frame(t);
        let values = self
            .rt
            .surface()
            .controls
            .iter()
            .map(|(id, c)| (*id, self.rt.position(*id, c)))
            .collect();
        let inputs = self.rt.shader_inputs();
        let gpu = self.gpu.as_mut().ok_or("no GPU attached")?;
        // The last check, deferred so a first frame does not wait on the
        // GPU: an overflow drew nothing, so this frame renders again,
        // checked and waited for, which bands it.
        if gpu.stage.check(&gpu.device) == crate::render::fx::Check::Overflowed {
            gpu.banded = true;
            gpu.check_next = true;
        }
        let frame = crate::render::fx::Frame {
            transform: physical.affine(),
            base: self.background,
            fonts: Some(crate::render::FontBook::bundled()),
            mode: self.mode,
            time_s: t / 1000.0,
            values: Some(&values),
            inputs: Some(&inputs),
            overlay: None,
            antialiasing: AaConfig::Area,
            checked: gpu.check_next || gpu.target.as_ref().is_none_or(|(_, size)| *size != (w, h)),
            deferred: !gpu.banded,
            ..crate::render::fx::Frame::new(&doc, self.page, w, h)
        };
        let rendered = gpu.stage.render(
            &gpu.device,
            &gpu.queue,
            &mut gpu.renderer.borrow_mut(),
            &frame,
        )?;
        gpu.check_next = false;
        self.time_dependent = gpu.stage.time_dependent();
        let fresh = !gpu.target.as_ref().is_some_and(|(_, size)| *size == (w, h));
        if fresh {
            let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("frame-live target"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            gpu.target = Some((texture, (w, h)));
        }
        let (target, _) = gpu.target.as_ref().expect("target made above");
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame-live copy"),
            });
        encoder.copy_texture_to_texture(
            rendered.as_image_copy(),
            target.as_image_copy(),
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        gpu.queue.submit(Some(encoder.finish()));
        self.dirty = false;
        Ok((target.clone(), fresh))
    }
}

/// Where a design lives: `<dir>/<name>.frame` with `<dir>/<plugin>.catalog.json`
/// beside it, as frame's plugin-faces generator writes them (an older
/// `<name>.fm` is used when there is no `.frame`).
#[must_use]
pub fn face_paths(dir: &Path, name: &str) -> (PathBuf, Option<PathBuf>) {
    let markup = dir.join(format!("{name}.{}", crate::document::io::EXTENSION));
    let legacy = dir.join(format!("{name}.{}", crate::document::io::LEGACY_EXTENSION));
    let markup = if !markup.exists() && legacy.exists() {
        legacy
    } else {
        markup
    };
    let catalog = std::fs::read_dir(dir).ok().and_then(|rd| {
        rd.filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.to_string_lossy().ends_with(".catalog.json"))
    });
    (markup, catalog)
}
