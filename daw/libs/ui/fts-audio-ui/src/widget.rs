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
pub struct SceneSlot(Rc<RefCell<Option<Scene>>>);

impl SceneSlot {
    pub fn new() -> Self {
        Self::default()
    }

    /// Leave a freshly built scene for the next frame.
    pub fn put(&self, scene: Scene) {
        *self.0.borrow_mut() = Some(scene);
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
        self.slot.take_scene().unwrap_or_default()
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
