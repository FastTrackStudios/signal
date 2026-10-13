//! One frame surface living in a host's strip.
//!
//! The host owns the box; the embed owns what is in it:
//!
//! 1. **Size.** [`Embed::set_size`] hands over the strip's box (logical
//!    px). The view lays itself out for it — reflow, not letterbox — at the
//!    [`Tier`] the width falls in. Values, feeds and pointer capture survive
//!    the relayout.
//! 2. **Paint, in two passes.** [`Embed::record`] paints everything that
//!    depends only on params (curves, handles, labels, the ground) — a host
//!    records it once into its own scene and replays it under the strip's
//!    transform until [`Embed::needs_record`] says otherwise. [`Embed::live`]
//!    paints what moves with the audio or with time (plots bound to feeds,
//!    meters, motion) over the top, every frame while
//!    [`Embed::animating`]. This is session's mixer model: record once,
//!    replay under a transform, meters in a live pass.
//! 3. **Pointer.** [`Embed::pointer`] takes strip-local coordinates and
//!    returns the engine calls the gesture made, plus whether the host
//!    should redraw or handle the event itself (a right click passes
//!    through to the host's context menu).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use crate::document::proto::{Node, NodeId, Scene, Transform};
use crate::surface::proto::{
    Binding, Catalog, ControlKind, EntryId, InputSource, ParamAddr, ParamValue, Source, StreamFrame,
};
use crate::surface::runtime::{
    Bounds, CurveRegistry, EngineCall, Input, Modifiers, Runtime, Surface,
};
use kurbo::Affine;

use crate::embed::canvas::{Canvas, Recording};
use crate::embed::form::{EmbedHost, SizeHints, Tier};
use crate::embed::paint::paint_filtered;
use crate::embed::text::Font;

/// Param values in engineering units (a choice's option index, a
/// toggle's 0/1), by address.
pub type Values = BTreeMap<String, f64>;

/// What a view is built for.
#[derive(Debug, Clone)]
pub struct BuildCtx {
    /// The box, logical px.
    pub size: (f64, f64),
    pub tier: Tier,
    /// Current param values (a view may lay out by them: which machine a
    /// delay is, which phases are folded).
    pub values: Values,
}

impl BuildCtx {
    #[must_use]
    pub fn new(size: (f64, f64), values: Values) -> Self {
        Self {
            size,
            tier: Tier::at(size.0),
            values,
        }
    }

    /// A value, or `default` when the catalog has no such param.
    #[must_use]
    pub fn value(&self, addr: &str, default: f64) -> f64 {
        self.values
            .get(addr)
            .copied()
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    }
}

/// Mutates one node of the frame being painted from the current values.
pub type DeriveFn = Arc<dyn Fn(&Values, &mut Node) + Send + Sync>;

/// A node property that is a function of several params — a header's
/// summary ("4 · 5.5dB"), a band handle's hue (its frequency), the shaded
/// band between a gate's two lines. Controls bind one param to one slot;
/// this is the rest, applied to the render copy after the controls, so it
/// can never drift from them.
#[derive(Clone)]
pub struct Derived {
    pub node: NodeId,
    pub apply: DeriveFn,
}

impl Derived {
    pub fn new(node: NodeId, f: impl Fn(&Values, &mut Node) + Send + Sync + 'static) -> Self {
        Self {
            node,
            apply: Arc::new(f),
        }
    }
}

impl std::fmt::Debug for Derived {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Derived")
            .field("node", &self.node)
            .finish_non_exhaustive()
    }
}

/// A view built for one box.
#[derive(Debug, Clone)]
pub struct Built {
    pub surface: Surface,
    /// The page the view is drawn from (its first child is the root frame,
    /// at the origin, the box's size).
    pub page: NodeId,
    /// Properties computed from values each frame.
    pub derived: Vec<Derived>,
    /// Params the layout itself depends on: when one changes (and no
    /// gesture is in flight) the view is built again.
    pub rebuild_on: Vec<ParamAddr>,
    /// Page px per box px: a page laid out at another size, drawn (and hit)
    /// scaled to the box. 1 for views built at the box's size.
    pub scale: f64,
}

impl Built {
    #[must_use]
    pub fn new(surface: Surface, page: NodeId) -> Self {
        Self {
            surface,
            page,
            derived: Vec::new(),
            rebuild_on: Vec::new(),
            scale: 1.0,
        }
    }
}

/// Something that lays itself out for a host's box.
pub trait EmbedView: Send {
    /// The params, feeds and actions every build binds against.
    fn catalog(&self) -> Catalog;

    /// Build the surface for `ctx.size` (logical px) at `ctx.tier`.
    fn build(&self, ctx: &BuildCtx) -> Built;

    /// The size hints the host should use.
    fn hints(&self, host: EmbedHost) -> SizeHints {
        host.hints()
    }

    /// The height this view wants at `width`, when the host lets it
    /// choose (a strip column does; REAPER does not).
    fn height_for(&self, width: f64, values: &Values) -> Option<f64> {
        let _ = (width, values);
        None
    }

    /// Curve providers to add to the runtime's builtins.
    fn curves(&self, registry: &mut CurveRegistry) {
        let _ = registry;
    }
}

/// Which mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}

/// Pointer input in strip-local logical px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pointer {
    Move {
        x: f64,
        y: f64,
    },
    Down {
        x: f64,
        y: f64,
        button: Button,
        mods: Modifiers,
    },
    Up {
        x: f64,
        y: f64,
        button: Button,
    },
    DoubleClick {
        x: f64,
        y: f64,
        button: Button,
    },
    /// Notches, positive = up/away. REAPER's 120 per step is one notch.
    Wheel {
        x: f64,
        y: f64,
        notches: f64,
        mods: Modifiers,
    },
    /// The pointer left the box (or capture was lost).
    Leave,
}

/// What a pointer event did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Handled {
    /// Calls to send to the engine.
    pub calls: Vec<EngineCall>,
    /// A control took the event.
    pub handled: bool,
    /// The event is the host's (a right click: its context menu).
    pub passthrough: bool,
    /// The embed should be repainted now (the record pass too).
    pub redraw: bool,
}

/// A surface in a host's strip. See the module docs.
pub struct Embed {
    view: Box<dyn EmbedView>,
    host: EmbedHost,
    catalog: Catalog,
    curves: CurveRegistry,
    runtime: Runtime,
    page: NodeId,
    derived: Vec<Derived>,
    rebuild_on: Vec<(ParamAddr, f64)>,
    /// Page px per box px (see [`Built::scale`]).
    scale: f64,
    size: (f64, f64),
    tier: Tier,
    /// Nodes drawn in the live pass.
    live: HashSet<NodeId>,
    /// The record pass needs repainting.
    dirty: bool,
    /// A button is down (no relayout mid-gesture).
    pressed: bool,
    /// Last frame of each feed, replayed into a rebuilt runtime.
    frames: HashMap<EntryId, StreamFrame>,
    font: Font,
    builds: u64,
}

impl std::fmt::Debug for Embed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Embed")
            .field("host", &self.host)
            .field("size", &self.size)
            .field("tier", &self.tier)
            .finish_non_exhaustive()
    }
}

impl Embed {
    /// An embed of `view` for `host`, laid out for a strip-wide box until
    /// [`Embed::set_size`] says otherwise.
    #[must_use]
    pub fn new(view: impl EmbedView + 'static, host: EmbedHost) -> Self {
        let catalog = view.catalog();
        let mut curves = crate::embed::curves::registry();
        view.curves(&mut curves);
        let hints = view.hints(host);
        let size = hints.clamp(crate::embed::form::STRIP_W, 160.0);
        let values = defaults(&catalog);
        let built = view.build(&BuildCtx::new(size, values));
        let runtime = Runtime::new(built.surface, catalog.clone()).with_curves(curves.clone());
        let mut e = Self {
            view: Box::new(view),
            host,
            catalog,
            curves,
            runtime,
            page: built.page,
            derived: built.derived,
            scale: built.scale,
            rebuild_on: Vec::new(),
            size,
            tier: Tier::at(size.0),
            live: HashSet::new(),
            dirty: true,
            pressed: false,
            frames: HashMap::new(),
            font: Font::default(),
            builds: 1,
        };
        e.rebuild_on = built.rebuild_on.into_iter().map(|a| (a, 0.0)).collect();
        e.after_build();
        e
    }

    /// Use a different face for labels.
    #[must_use]
    pub fn with_font(mut self, font: Font) -> Self {
        self.font = font;
        self.dirty = true;
        self
    }

    /// Register (or override) a curve provider — session's
    /// `eq_ui::calculate_combined_response`, say, under `eq.response`.
    pub fn register_curve(
        &mut self,
        name: &str,
        f: impl Fn(&crate::surface::runtime::CurveInputs) -> Vec<crate::surface::runtime::Series>
        + Send
        + Sync
        + 'static,
    ) {
        self.curves.register(name, f);
        *self.runtime.curves_mut() = self.curves.clone();
        self.dirty = true;
    }

    #[must_use]
    pub fn host(&self) -> EmbedHost {
        self.host
    }

    #[must_use]
    pub fn size(&self) -> (f64, f64) {
        self.size
    }

    #[must_use]
    pub fn tier(&self) -> Tier {
        self.tier
    }

    #[must_use]
    pub fn hints(&self) -> SizeHints {
        self.view.hints(self.host)
    }

    /// The view's hints for another host (REAPER asks per context).
    #[must_use]
    pub fn hints_for(&self, host: EmbedHost) -> SizeHints {
        self.view.hints(host)
    }

    /// The height the view wants at `width`, if it has a preference.
    #[must_use]
    pub fn height_for(&self, width: f64) -> Option<f64> {
        self.view.height_for(width, &self.values())
    }

    /// How many times the view has been laid out (tests: a resize to the
    /// same box does not relayout).
    #[must_use]
    pub fn builds(&self) -> u64 {
        self.builds
    }

    #[must_use]
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// The runtime, for anything the embed does not wrap. Marks the record
    /// pass dirty.
    pub fn runtime_mut(&mut self) -> &mut Runtime {
        self.dirty = true;
        &mut self.runtime
    }

    #[must_use]
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    #[must_use]
    pub fn page(&self) -> NodeId {
        self.page
    }

    /// Every param's current value, engineering units.
    #[must_use]
    pub fn values(&self) -> Values {
        self.catalog
            .params
            .iter()
            .filter_map(|p| Some((p.addr.0.clone(), self.runtime.real(&p.addr)?)))
            .collect()
    }

    /// One param's value, engineering units.
    #[must_use]
    pub fn value(&self, addr: &str) -> Option<f64> {
        self.runtime.real(&ParamAddr::new(addr))
    }

    /// Lay out for a `w × h` box (logical px). Returns whether the view
    /// was rebuilt.
    pub fn set_size(&mut self, w: f64, h: f64) -> bool {
        let (w, h) = (w.max(1.0).round(), h.max(1.0).round());
        if (w, h) == self.size {
            return false;
        }
        self.size = (w, h);
        self.tier = Tier::at(w);
        self.rebuild();
        true
    }

    fn rebuild(&mut self) {
        let values = self.runtime.values();
        let built = self.view.build(&BuildCtx::new(self.size, self.values()));
        let mut runtime =
            Runtime::new(built.surface, self.catalog.clone()).with_curves(self.curves.clone());
        runtime.apply_values(values);
        for f in self.frames.values() {
            runtime.apply_frame(f.clone(), 0.0);
        }
        self.runtime = runtime;
        self.page = built.page;
        self.derived = built.derived;
        self.scale = built.scale;
        self.rebuild_on = built.rebuild_on.into_iter().map(|a| (a, 0.0)).collect();
        self.builds += 1;
        self.after_build();
    }

    fn after_build(&mut self) {
        let scene = &self.runtime.surface().scene;
        let boxes: Vec<(NodeId, Bounds)> =
            crate::render::build_scene_layout_boxes(scene, self.page)
                .into_iter()
                .map(|(id, b)| {
                    (
                        id,
                        Bounds {
                            x: b.x,
                            y: b.y,
                            width: b.width,
                            height: b.height,
                        },
                    )
                })
                .collect();
        self.live = live_nodes(self.runtime.surface());
        self.runtime.set_boxes(boxes);
        for (addr, v) in &mut self.rebuild_on {
            *v = self.runtime.real(addr).unwrap_or(f64::NAN);
        }
        self.dirty = true;
    }

    /// Relayout if a param the layout depends on moved (never mid-gesture).
    fn settle(&mut self) {
        if self.pressed {
            return;
        }
        let moved = self.rebuild_on.iter().any(|(a, v)| {
            self.runtime
                .real(a)
                .is_some_and(|now| (now - *v).abs() > 1e-9)
        });
        if moved {
            self.rebuild();
        }
    }

    // ── engine → embed ────────────────────────────────────────────────────

    pub fn apply_values(&mut self, values: impl IntoIterator<Item = ParamValue>) {
        self.runtime.apply_values(values);
        self.dirty = true;
        self.settle();
    }

    /// A feed frame at `t` ms. Feeds only touch the live pass — unless a
    /// curve reads the feed, which moves the record pass too.
    pub fn apply_frame(&mut self, frame: StreamFrame, t: f64) {
        if self.curve_reads(&frame.id) {
            self.dirty = true;
        }
        self.frames.insert(frame.id.clone(), frame.clone());
        self.runtime.apply_frame(frame, t);
    }

    fn curve_reads(&self, id: &EntryId) -> bool {
        self.runtime
            .surface()
            .controls
            .iter()
            .any(|(n, c)| match &c.kind {
                ControlKind::Curve { inputs, .. } => {
                    !self.live.contains(n)
                        && inputs.iter().any(
                            |i| matches!(&i.source, InputSource::Stream { id: s, .. } if s == id),
                        )
                }
                _ => false,
            })
    }

    /// Flush coalesced edits (call once a frame while a gesture is live).
    pub fn poll(&mut self, t: f64) -> Vec<EngineCall> {
        let calls = self.runtime.poll(t);
        if !calls.is_empty() {
            self.dirty = true;
        }
        calls
    }

    // ── paint ─────────────────────────────────────────────────────────────

    /// Whether the record pass is stale (a param moved, the box changed).
    #[must_use]
    pub fn needs_record(&self) -> bool {
        self.dirty
    }

    /// Whether the live pass should run again soon (a feed is playing, a
    /// meter is falling, a gesture or an animation is in flight). A host
    /// that is not animating can sleep.
    #[must_use]
    pub fn animating(&self) -> bool {
        self.runtime.animating()
    }

    /// Whether this view has a live pass at all.
    #[must_use]
    pub fn has_live(&self) -> bool {
        !self.live.is_empty()
    }

    fn frame(&self, t: f64) -> Scene {
        let mut scene = self.runtime.frame(t);
        if !self.derived.is_empty() {
            let values = self.values();
            for d in &self.derived {
                if let Some(n) = scene.node_mut(d.node) {
                    (d.apply)(&values, n);
                }
            }
        }
        scene
    }

    /// Paint the params-only part into `canvas` under `at` (canvas ← strip
    /// space), and mark it clean.
    pub fn record(&mut self, canvas: &mut (impl Canvas + ?Sized), at: Affine) {
        self.settle();
        let scene = self.frame(0.0);
        let prims = crate::render::build_scene_primitives(&scene, self.page);
        let live = &self.live;
        let font = self.font.for_scene(&scene);
        paint_filtered(
            canvas,
            &prims,
            at * Affine::scale(self.scale),
            &font,
            &|id| !live.contains(&id),
        );
        self.dirty = false;
    }

    /// The record pass as a [`Recording`] in strip space (replay it under
    /// the strip's transform).
    pub fn recording(&mut self) -> Recording {
        let mut rec = Recording::new();
        self.record(&mut rec, Affine::IDENTITY);
        rec
    }

    /// Paint what moves at `t` ms over the record pass.
    pub fn live(&self, canvas: &mut (impl Canvas + ?Sized), at: Affine, t: f64) {
        if self.live.is_empty() {
            return;
        }
        let scene = self.frame(t);
        let prims = crate::render::build_scene_primitives(&scene, self.page);
        let live = &self.live;
        let font = self.font.for_scene(&scene);
        paint_filtered(
            canvas,
            &prims,
            at * Affine::scale(self.scale),
            &font,
            &|id| live.contains(&id),
        );
    }

    /// Both passes in one walk, for hosts that repaint everything every
    /// frame (a LICE bitmap).
    pub fn paint(&mut self, canvas: &mut (impl Canvas + ?Sized), at: Affine, t: f64) {
        self.settle();
        let scene = self.frame(t);
        let prims = crate::render::build_scene_primitives(&scene, self.page);
        let font = self.font.for_scene(&scene);
        crate::embed::paint::paint_primitives(
            canvas,
            &prims,
            at * Affine::scale(self.scale),
            &font,
        );
        self.dirty = false;
    }

    // ── pointer ───────────────────────────────────────────────────────────

    /// The control under a strip-local point.
    #[must_use]
    pub fn hit(&self, x: f64, y: f64) -> Option<NodeId> {
        self.runtime.hit(x / self.scale, y / self.scale)
    }

    /// Route pointer input (strip-local logical px) at `t` ms.
    pub fn pointer(&mut self, event: Pointer, t: f64) -> Handled {
        let k = 1.0 / self.scale;
        let event = match event {
            Pointer::Move { x, y } => Pointer::Move { x: x * k, y: y * k },
            Pointer::Down { x, y, button, mods } => Pointer::Down {
                x: x * k,
                y: y * k,
                button,
                mods,
            },
            Pointer::Up { x, y, button } => Pointer::Up {
                x: x * k,
                y: y * k,
                button,
            },
            Pointer::DoubleClick { x, y, button } => Pointer::DoubleClick {
                x: x * k,
                y: y * k,
                button,
            },
            Pointer::Wheel {
                x,
                y,
                notches,
                mods,
            } => Pointer::Wheel {
                x: x * k,
                y: y * k,
                notches,
                mods,
            },
            Pointer::Leave => Pointer::Leave,
        };
        let mut out = Handled::default();
        match event {
            Pointer::Down {
                button: Button::Right | Button::Middle,
                ..
            }
            | Pointer::Up {
                button: Button::Right | Button::Middle,
                ..
            }
            | Pointer::DoubleClick {
                button: Button::Right | Button::Middle,
                ..
            } => {
                out.passthrough = true;
            }
            Pointer::Down { x, y, mods, .. } => {
                let hit = self.runtime.hit(x, y);
                out.handled = hit.is_some();
                self.pressed = true;
                // A cmd/ctrl-click resets, like a double-click.
                if mods.alt
                    && let Some(id) = hit
                {
                    out.calls = self.runtime.feed(id, Input::DoubleClick, t);
                } else {
                    out.calls = self.runtime.pointer(Input::Down { x, y, t, mods }, t);
                }
            }
            Pointer::Move { x, y } => {
                out.calls = self.runtime.pointer(
                    Input::Move {
                        x,
                        y,
                        t,
                        mods: Modifiers::default(),
                    },
                    t,
                );
                out.handled = !out.calls.is_empty();
            }
            Pointer::Up { x, y, .. } => {
                out.calls = self.runtime.pointer(Input::Up { x, y, t }, t);
                out.handled = true;
                self.pressed = false;
            }
            Pointer::DoubleClick { x, y, .. } => {
                if let Some(id) = self.runtime.hit(x, y) {
                    out.calls = self.runtime.feed(id, Input::DoubleClick, t);
                    out.handled = true;
                }
            }
            Pointer::Wheel {
                x,
                y,
                notches,
                mods,
            } => {
                if let Some(id) = self.runtime.hit(x, y) {
                    out.calls = self.runtime.feed(id, Input::Wheel { notches, mods }, t);
                    out.handled = true;
                }
            }
            Pointer::Leave => {
                if self.pressed {
                    out.calls = self.runtime.pointer(Input::Cancel { t }, t);
                    self.pressed = false;
                }
            }
        }
        if !out.calls.is_empty() {
            out.redraw = true;
            self.dirty = true;
        }
        if !self.pressed {
            let before = self.builds;
            self.settle();
            out.redraw |= self.builds != before;
        }
        out
    }
}

/// Every param at its default, engineering units.
fn defaults(catalog: &Catalog) -> Values {
    let rt = Runtime::new(
        Surface {
            scene: Scene::new("defaults"),
            controls: BTreeMap::new(),
        },
        catalog.clone(),
    );
    catalog
        .params
        .iter()
        .filter_map(|p| Some((p.addr.0.clone(), rt.real(&p.addr)?)))
        .collect()
}

/// Nodes whose drawing moves with feeds or time: every control reading a
/// stream (plots, meters, gauges, stream-fed readouts and curves), and
/// every node with motion — each with its subtree.
fn live_nodes(surface: &Surface) -> HashSet<NodeId> {
    let scene: &Scene = &surface.scene;
    let mut roots: Vec<NodeId> = Vec::new();
    for (id, c) in &surface.controls {
        let streamed = matches!(
            &c.binding,
            Some(Binding {
                source: Source::Stream { .. },
                ..
            })
        ) || matches!(&c.kind, ControlKind::Plot { .. })
            || matches!(&c.kind, ControlKind::Curve { inputs, .. } if inputs.iter().any(|i| matches!(i.source, InputSource::Stream { .. })));
        if streamed {
            roots.push(*id);
            roots.extend(c.slots.iter().map(|s| NodeId(s.node)));
        }
    }
    roots.extend(
        scene
            .nodes
            .values()
            .filter(|n| !n.motion.is_empty())
            .map(|n| n.id),
    );
    let mut out = HashSet::new();
    for r in roots {
        out.extend(scene.walk(r));
    }
    out
}

// ── authored documents ───────────────────────────────────────────────────

/// An authored document shown as an embed.
///
/// The view is a page named for the host (`embed:mcp`, `embed:tcp`,
/// `embed:strip`), optionally one per breakpoint (`embed:mcp@96` is shown
/// from 96 px wide up). The widest breakpoint that fits wins; without an
/// embed page for the host, the `strip` page, then the first page. The
/// page's root frame is scaled to the host's width (its height follows),
/// which is the reflow a document can do without a runtime layout solver.
#[derive(Debug, Clone)]
pub struct DocumentView {
    pub surface: Surface,
    pub catalog: Catalog,
    pub host: EmbedHost,
}

impl DocumentView {
    #[must_use]
    pub fn new(surface: Surface, catalog: Catalog, host: EmbedHost) -> Self {
        Self {
            surface,
            catalog,
            host,
        }
    }

    /// The page shown at `width`.
    #[must_use]
    pub fn page_at(&self, width: f64) -> Option<NodeId> {
        let scene = &self.surface.scene;
        let pages = scene.pages();
        let named = |host: EmbedHost| {
            pages
                .iter()
                .filter_map(|p| {
                    let (h, bp) = crate::embed::form::parse_page_name(&scene.node(*p)?.name)?;
                    (h == host).then_some((*p, bp))
                })
                .filter(|(_, bp)| f64::from(*bp) <= width)
                .max_by_key(|(_, bp)| *bp)
                .map(|(p, _)| p)
        };
        named(self.host)
            .or_else(|| named(EmbedHost::Strip))
            .or_else(|| pages.first().copied())
    }

    fn root_size(&self, page: NodeId) -> (f64, f64) {
        let scene = &self.surface.scene;
        scene
            .node(page)
            .and_then(|p| p.children.first())
            .and_then(|r| scene.node(*r))
            .map_or((1.0, 1.0), |n| {
                (n.size.width.max(1.0), n.size.height.max(1.0))
            })
    }
}

impl EmbedView for DocumentView {
    fn catalog(&self) -> Catalog {
        self.catalog.clone()
    }

    fn build(&self, ctx: &BuildCtx) -> Built {
        let mut surface = self.surface.clone();
        let page = self.page_at(ctx.size.0).unwrap_or(surface.scene.root);
        let (w0, _) = self.root_size(page);
        // The root sits at the page origin; the embed scales the page.
        if let Some(root) = surface
            .scene
            .node(page)
            .and_then(|p| p.children.first().copied())
            && let Some(n) = surface.scene.node_mut(root)
        {
            n.transform = Transform::IDENTITY;
        }
        Built {
            scale: ctx.size.0 / w0,
            ..Built::new(surface, page)
        }
    }

    fn height_for(&self, width: f64, _values: &Values) -> Option<f64> {
        let page = self.page_at(width)?;
        let (w0, h0) = self.root_size(page);
        Some(h0 * width / w0)
    }
}
