//! The seam between the painted roll and the renderer.
//!
//! This is the only native-only module in the crate, and it is
//! deliberately thin: [`crate::paint`] decides what the roll looks like,
//! and this puts the result on screen.
//!
//! ## Why the scene is built outside `paint`
//!
//! [`Widget::paint`] is called by the renderer, not by dioxus. Reading a
//! `Signal` from there would be reaching into dioxus's world from
//! outside it — the same shape as the `get_client_rect().await` that
//! panicked with "RefCell already borrowed" on the first click under
//! Blitz (#167).
//!
//! So the component builds the scene during render, where reading
//! signals is ordinary and safe, and leaves it in [`SceneSlot`]. This
//! widget only clones what it finds. That also happens to be the right
//! performance model: the recording is rebuilt when the *state* changes,
//! and replayed by the renderer every frame regardless — so a frame
//! costs a replay, not a rebuild.
//!
//! ## Why the events are not here either
//!
//! `Widget::handle_event` exists, but its coordinates are page-relative
//! and it knows nothing about where its element sits. The `<object>`
//! element is an ordinary DOM node, so the existing dioxus handlers keep
//! working on it unchanged and keep giving `element_coordinates()` — and
//! with the scale now exactly 1, those *are* document coordinates. The
//! whole of `interaction.rs` therefore ports over untouched.

use std::cell::RefCell;
use std::rc::Rc;

use anyrender::Scene;

/// Where the component leaves the scene for the widget to find.
///
/// An `Rc<RefCell<…>>` rather than a signal because the reader is the
/// renderer, which is outside dioxus's reactive world entirely. Cloned
/// into the widget at mount and kept by the component.
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
    /// `try_borrow` rather than `borrow`: the cost of losing one frame
    /// to a contended slot is a stale frame, and the cost of panicking
    /// in a paint callback is the window.
    pub fn take_scene(&self) -> Option<Scene> {
        self.0.try_borrow().ok()?.clone()
    }
}

/// Painted frames, counted where they actually happen.
///
/// The editor's meter used to time its own re-renders, which is the rate
/// dioxus rebuilds a component at — not the rate anything reaches the
/// screen. `Widget::paint` is called by the renderer once per frame, so
/// this is the first place in the surface that can honestly say "fps".
///
/// A ring of recent intervals rather than a running average: what you
/// want to see while dragging is whether frames are *arriving evenly*,
/// and a lifetime mean hides exactly that.
#[derive(Clone, Default)]
pub struct Frames(Rc<RefCell<FrameLog>>);

#[derive(Default)]
pub struct FrameLog {
    last: Option<std::time::Instant>,
    intervals: Vec<f64>,
    /// How long the roll itself took, over the same window.
    ///
    /// Reported next to the rate because the two answer different
    /// questions, and confusing them wastes days: the interval says how
    /// fast frames arrive, and this says how much of that was us. A
    /// frame budget blown with this at almost zero is being spent
    /// somewhere else entirely — the renderer, or the rest of the DOM.
    paints: Vec<f64>,
    /// Intervals between *scene rebuilds*, which is once per dioxus
    /// render of the roll.
    ///
    /// The third number the frame budget needs. Frames say how fast the
    /// screen updates and `paints` says how much of that was the roll —
    /// but a pan is driven by pointer events, and if those arrive faster
    /// than frames do then every one of them is paying for a render, a
    /// DOM update and a layout that no frame ever shows. That is
    /// invisible in the other two and is exactly the shape of a drag
    /// that stutters while an idle view is smooth.
    builds: Vec<f64>,
    last_build: Option<std::time::Instant>,
}

impl Frames {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that a frame was just painted.
    fn tick(&self) {
        let Ok(mut log) = self.0.try_borrow_mut() else {
            return;
        };
        let now = std::time::Instant::now();
        if let Some(previous) = log.last.replace(now) {
            let ms = now.duration_since(previous).as_secs_f64() * 1000.0;
            // Drop the idle gaps: the first frame after a second of
            // nothing is not a 1 fps frame, it is the start of a burst.
            if ms < 500.0 {
                log.intervals.push(ms);
                if log.intervals.len() > 60 {
                    log.intervals.remove(0);
                }
            }
        }
    }

    /// Record that the roll rebuilt its scene — one dioxus render.
    pub fn built(&self) {
        let Ok(mut log) = self.0.try_borrow_mut() else {
            return;
        };
        let now = std::time::Instant::now();
        if let Some(previous) = log.last_build.replace(now) {
            let ms = now.duration_since(previous).as_secs_f64() * 1000.0;
            if ms < 500.0 {
                log.builds.push(ms);
                if log.builds.len() > 60 {
                    log.builds.remove(0);
                }
            }
        }
    }

    /// Scene rebuilds per second over the recent window.
    ///
    /// Read against the frame rate: meaningfully higher means the
    /// surface is re-rendering for events the screen never gets to show.
    pub fn builds_per_second(&self) -> Option<f64> {
        let log = self.0.try_borrow().ok()?;
        if log.builds.is_empty() {
            return None;
        }
        let mean = log.builds.iter().sum::<f64>() / log.builds.len() as f64;
        (mean > 0.0).then(|| 1000.0 / mean)
    }

    /// Record how long the roll took to hand over its drawing.
    fn spent(&self, ms: f64) {
        let Ok(mut log) = self.0.try_borrow_mut() else {
            return;
        };
        log.paints.push(ms);
        if log.paints.len() > 60 {
            log.paints.remove(0);
        }
    }

    /// Frames per second over the recent window, once there are enough
    /// frames to divide by.
    pub fn fps(&self) -> Option<f64> {
        let log = self.0.try_borrow().ok()?;
        if log.intervals.is_empty() {
            return None;
        }
        let mean = log.intervals.iter().sum::<f64>() / log.intervals.len() as f64;
        (mean > 0.0).then(|| 1000.0 / mean)
    }

    /// Milliseconds of the average frame spent in the roll.
    pub fn paint_ms(&self) -> Option<f64> {
        let log = self.0.try_borrow().ok()?;
        if log.paints.is_empty() {
            return None;
        }
        Some(log.paints.iter().sum::<f64>() / log.paints.len() as f64)
    }
}

/// The roll's custom widget.
///
/// Holds nothing but the slot and the frame counter: everything it draws
/// was decided by the component that filled it.
pub struct RollWidget {
    slot: SceneSlot,
    frames: Frames,
}

impl RollWidget {
    pub fn new(slot: SceneSlot, frames: Frames) -> Self {
        Self { slot, frames }
    }
}

/// A widget that replays a slot and counts nothing.
///
/// The frame meter belongs to the roll: it answers "how fast is the
/// music surface arriving?". A second widget on the same counter — the
/// painted cursor, which repaints on every pointer move — would make
/// that number report the mouse instead.
pub struct SceneWidget {
    slot: SceneSlot,
}

impl SceneWidget {
    pub fn new(slot: SceneSlot) -> Self {
        Self { slot }
    }
}

/// How many times a [`SceneWidget`] has been asked to paint.
///
/// The one thing a headless test can check about a custom widget.
/// `DocumentTester::render_png` rasterizes through `blitz_paint`, which
/// composites ordinary boxes but *not* custom-widget scenes — a plain
/// `div` shows up in the PNG and an `<object>` carrying a widget does
/// not. So a pixel diff cannot tell "the widget never painted" from "the
/// rasterizer does not draw widgets", and this can: the renderer calls
/// `paint` only for a widget it has laid out and intends to draw.
pub static SCENE_PAINTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl blitz_dom::Widget for SceneWidget {
    fn paint(
        &mut self,
        _ctx: &mut dyn anyrender::RenderContext,
        _styles: &blitz_dom::node::ComputedStyles,
        _width: u32,
        _height: u32,
        _scale: f64,
    ) -> Scene {
        SCENE_PAINTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.slot.take_scene().unwrap_or_default()
    }
}

impl blitz_dom::Widget for RollWidget {
    fn paint(
        &mut self,
        _ctx: &mut dyn anyrender::RenderContext,
        _styles: &blitz_dom::node::ComputedStyles,
        _width: u32,
        _height: u32,
        _scale: f64,
    ) -> Scene {
        // `width`/`height` are the element's box, in device pixels. They
        // are deliberately unused: the scene was built in CSS pixels
        // against the same box, which the editor computes exactly
        // (`crate::sizing`), and the renderer applies the device scale
        // for us. Reading them here and scaling the drawing would
        // reintroduce the very ratio that made the svg wrong.
        self.frames.tick();
        let started = std::time::Instant::now();
        let scene = self.slot.take_scene().unwrap_or_default();
        self.frames.spent(started.elapsed().as_secs_f64() * 1000.0);
        scene
    }
}
