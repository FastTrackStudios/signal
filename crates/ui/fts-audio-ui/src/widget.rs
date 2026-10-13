//! The seam between a painted control and the renderer (native only).
//!
//! Spec `fx.control.painted`. A control's graphic is an
//! [`anyrender::Scene`] built by [`crate::paint`] during render, left in a
//! [`SceneSlot`], and replayed by a Blitz custom widget attached to an
//! `<object>` element. This module is the only part of the kit that knows
//! about the DOM or the renderer; everything it puts on screen was decided
//! by a portable painter.
//!
//! ## Why the scene is built outside the widget
//!
//! `Widget::paint` is called by the renderer, not by dioxus. Reading a
//! `Signal` from there reaches into dioxus's world from outside it — the
//! shape that panicked with "RefCell already borrowed" under Blitz. So the
//! component builds the scene in render (where reading signals is ordinary)
//! and the widget only clones what it finds. That is also the right
//! performance model: the recording is rebuilt when the *state* changes and
//! replayed every frame regardless.
//!
//! ## Why the events are not here
//!
//! `<object>` is an ordinary DOM node: the gesture overlay the control puts
//! over it keeps receiving `element_coordinates()` exactly as before, and with
//! nothing scaling the drawing those *are* the painted coordinates.
//!
//! Same pattern as the expression editor's roll (`roll_widget.rs`) and the
//! EQ graph painter; this is the kit-wide copy so controls do not each grow
//! their own.

use std::cell::RefCell;
use std::rc::Rc;

use anyrender::Scene;
use dioxus::prelude::*;

pub use dioxus_native_dom::CustomWidgetAttr;

/// Where a component leaves its scene for the widget to find.
///
/// `Rc<RefCell<…>>` rather than a signal because the reader is the renderer,
/// outside dioxus's reactive world. Cloned into the widget at mount, kept by
/// the component.
#[derive(Clone, Default)]
pub struct SceneSlot(Rc<RefCell<Option<Scene>>>, Rc<std::cell::Cell<bool>>);

impl SceneSlot {
    pub fn new() -> Self {
        Self::default()
    }

    /// Leave a freshly built scene for the next frame.
    pub fn put(&self, scene: Scene) {
        *self.0.borrow_mut() = Some(scene);
        // Unpainted: the host draws a frame for it (see `needs_redraw`).
        self.1.set(true);
    }

    /// What the last render left, if anything.
    ///
    /// `try_borrow`: the cost of losing one frame to a contended slot is a
    /// stale frame; the cost of panicking in a paint callback is the window.
    pub fn take_scene(&self) -> Option<Scene> {
        self.0.try_borrow().ok()?.clone()
    }
}

/// A widget that replays whatever its slot holds.
pub struct SceneWidget {
    slot: SceneSlot,
}

impl SceneWidget {
    pub fn new(slot: SceneSlot) -> Self {
        Self { slot }
    }
}

impl blitz_dom::Widget for SceneWidget {
    fn paint(
        &mut self,
        _ctx: &mut dyn anyrender::RenderContext,
        _styles: &blitz_dom::node::ComputedStyles,
        _width: u32,
        _height: u32,
        _scale: f64,
    ) -> Scene {
        // `width`/`height` are the box in device pixels and are deliberately
        // unused: the scene was built in CSS pixels against the same box and
        // the renderer applies the device scale. Scaling here would
        // reintroduce the ratio that made inline svg wrong.
        self.slot.1.set(false);
        self.slot.take_scene().unwrap_or_default()
    }

    /// A scene left since the last paint: the host redraws for it, and
    /// only for it — a component re-rendering changes no DOM when all it
    /// changed is its picture.
    fn needs_redraw(&self) -> bool {
        self.slot.1.get()
    }
}

/// The slot and the write-once widget attribute for one painted surface.
///
/// Call once per component (it is a hook). `CustomWidgetAttr` is write-once
/// — the DOM takes the widget out of it on the first mutation — so it must
/// be created exactly once and reused across renders; a fresh one per render
/// would hand the second render an empty attribute and a blank control.
///
/// ```ignore
/// let painted = use_painted();
/// painted.slot.put(paint::knob::scene(&look));
/// rsx! { object { "data": painted.widget.clone(), style: "width:56px;height:56px;display:block;" } }
/// ```
pub fn use_painted() -> Painted {
    use_hook(|| {
        let slot = SceneSlot::new();
        let widget = CustomWidgetAttr::new(SceneWidget::new(slot.clone()));
        Painted { slot, widget }
    })
}

/// See [`use_painted`].
#[derive(Clone)]
pub struct Painted {
    pub slot: SceneSlot,
    pub widget: CustomWidgetAttr,
}

/// A flag a component sets when it hands its widget something new to draw
/// (see [`Freshened`]). Cloned: the component keeps one, the widget one.
#[derive(Clone, Default)]
pub struct Fresh(Rc<std::cell::Cell<bool>>);

impl Fresh {
    /// Something new to draw: the host paints a frame for it.
    pub fn mark(&self) {
        self.0.set(true);
    }
}

/// A widget whose picture a component updates behind its back (a view in
/// an `Rc<RefCell<…>>` set during render), told so by a [`Fresh`] flag.
///
/// The host redraws when the DOM changes or a widget says it needs to.
/// Re-rendering a component that only updates its widget's view changes
/// no DOM, so without this the new view is never drawn. Wraps any widget
/// and leaves it as it is otherwise.
pub struct Freshened<W> {
    inner: W,
    fresh: Fresh,
}

impl<W> Freshened<W> {
    pub fn new(inner: W, fresh: Fresh) -> Self {
        Self { inner, fresh }
    }
}

impl<W: blitz_dom::Widget> blitz_dom::Widget for Freshened<W> {
    fn connected(&mut self) {
        self.inner.connected();
    }
    fn disconnected(&mut self) {
        self.inner.disconnected();
    }
    fn attribute_changed(&mut self, name: &str, old_value: Option<&str>, new_value: Option<&str>) {
        self.inner.attribute_changed(name, old_value, new_value);
    }
    fn can_create_surfaces(&mut self, render_ctx: &mut dyn anyrender::RenderContext) {
        self.inner.can_create_surfaces(render_ctx);
    }
    fn destroy_surfaces(&mut self) {
        self.inner.destroy_surfaces();
    }
    fn handle_event(&mut self, event: &blitz_traits::events::UiEvent) {
        self.inner.handle_event(event);
    }
    fn needs_redraw(&self) -> bool {
        self.fresh.0.get() || self.inner.needs_redraw()
    }
    fn composite_texture(&self) -> Option<anyrender::ResourceId> {
        self.inner.composite_texture()
    }
    fn paint(
        &mut self,
        render_ctx: &mut dyn anyrender::RenderContext,
        styles: &blitz_dom::node::ComputedStyles,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Scene {
        self.fresh.0.set(false);
        self.inner.paint(render_ctx, styles, width, height, scale)
    }
}

/// A widget whose scene is rendered into a texture of its own
/// ([`anyrender_vello::Rasterizer`]) rather than into the page, so the host
/// can draw it over the page as a layer: a frame where only it moved then
/// costs its own small render, not the whole page's.
///
/// For widgets that move on their own (a visualiser's decay, a sweeping
/// LFO). The inner widget paints as ever; its resources (a shader's
/// texture) register with the rasteriser. On a renderer with no wgpu
/// device it paints into the page as before.
pub struct Rasterized<W> {
    inner: W,
    raster: Option<anyrender_vello::Rasterizer>,
    /// The texture as the host knows it, and its size.
    texture: Option<(anyrender::ResourceId, (u32, u32))>,
}

impl<W> Rasterized<W> {
    pub fn new(inner: W) -> Self {
        Self { inner, raster: None, texture: None }
    }
}

impl<W: blitz_dom::Widget> blitz_dom::Widget for Rasterized<W> {
    fn connected(&mut self) {
        self.inner.connected();
    }
    fn disconnected(&mut self) {
        self.inner.disconnected();
    }
    fn attribute_changed(&mut self, name: &str, old_value: Option<&str>, new_value: Option<&str>) {
        self.inner.attribute_changed(name, old_value, new_value);
    }
    fn can_create_surfaces(&mut self, render_ctx: &mut dyn anyrender::RenderContext) {
        self.texture = None;
        self.raster = render_ctx.renderer_specific_context().and_then(anyrender_vello::Rasterizer::new);
        match self.raster.as_mut() {
            Some(raster) => self.inner.can_create_surfaces(raster),
            None => self.inner.can_create_surfaces(render_ctx),
        }
    }
    fn destroy_surfaces(&mut self) {
        self.inner.destroy_surfaces();
        self.raster = None;
        self.texture = None;
    }
    fn handle_event(&mut self, event: &blitz_traits::events::UiEvent) {
        self.inner.handle_event(event);
    }
    fn needs_redraw(&self) -> bool {
        self.inner.needs_redraw()
    }
    fn composite_texture(&self) -> Option<anyrender::ResourceId> {
        self.texture.map(|(id, _)| id)
    }
    fn paint(
        &mut self,
        render_ctx: &mut dyn anyrender::RenderContext,
        styles: &blitz_dom::node::ComputedStyles,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Scene {
        let Some(raster) = self.raster.as_mut() else {
            return self.inner.paint(render_ctx, styles, width, height, scale);
        };
        // Nothing new to draw: the texture already shows it.
        let current = self.texture.is_some_and(|(_, size)| size == (width, height));
        if !(current && !self.inner.needs_redraw()) {
            let scene = self.inner.paint(raster, styles, width, height, scale);
            let Some((texture, fresh)) = raster.render(scene, (width, height)) else {
                return Scene::new();
            };
            if fresh || !current {
                if let Some((old, _)) = self.texture.take() {
                    render_ctx.unregister_resource(old);
                }
                self.texture = render_ctx
                    .try_register_custom_resource(Box::new(texture))
                    .ok()
                    .map(|id| (id, (width, height)));
            }
        }
        let mut scene = Scene::new();
        if let Some((id, _)) = self.texture {
            use anyrender::PaintScene;
            scene.fill(
                peniko::Fill::NonZero,
                kurbo::Affine::IDENTITY,
                anyrender::Paint::Resource(peniko::ImageBrush { image: id, sampler: peniko::ImageSampler::default() }),
                None,
                &kurbo::Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            );
        }
        scene
    }
}
