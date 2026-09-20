//! What the delay and the reverb are doing, painted.
//!
//! These two panels were knobs and a number. A delay's character is its
//! taps — where they land against the beat, how fast they give up, whether
//! they walk across the stereo field — and none of that is legible from
//! `TIME 0.42 · FB 0.28`. A reverb's is its decay: how dense the early
//! reflections are and how long the tail takes to fall away.
//!
//! # Two painters, one widget
//!
//! [`Widget::paint`] records into an `anyrender::Scene` — a command list, not
//! a wgpu call — so whatever can replay it can draw this: the GPU backend, the
//! CPU one, and a canvas in a browser. That is the **fallback**, and it is not
//! a poor relation: vello rasterises it on the GPU with real gradients and
//! blurs.
//!
//! The **shader** path is the escape hatch the trait describes: take the
//! device in [`Widget::can_create_surfaces`], render WGSL into a texture, and
//! hand the scene that texture's `ResourceId`. `try_register_custom_resource`
//! answers `Unimplemented` on a renderer that cannot do it, which is what
//! makes the choice safe to make per-frame rather than per-build — WebGPU in a
//! browser takes the shader path, WebGL-class and CPU backends fall back, and
//! neither needs to know which it is.
//!
//! # Not a signal, a cell
//!
//! Paint happens inside Blitz's traversal, which is not the Dioxus runtime;
//! reading a `Signal` there is a panic waiting for the first frame that takes
//! a different path. What these need is a handful of numbers, and numbers can
//! live in a `RefCell` the component writes and the widget reads.

use std::cell::RefCell;
use std::rc::Rc;

use anyrender::{PaintScene, RenderContext, Scene};
use blitz_dom::node::{ComputedStyles, Widget};

use vello::kurbo::{Affine, BezPath, Circle, Line, Point, Rect, Stroke};
use vello::peniko::{Color, ColorStop, Fill, Gradient};

/// One delay tap: when it lands, how loud, and where it sits in the field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tap {
    /// Seconds after the dry hit.
    pub at: f32,
    /// Linear level, 0..=1.
    pub level: f32,
    /// −1 hard left, +1 hard right.
    pub pan: f32,
}

/// What the delay panel draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DelayView {
    pub taps: Vec<Tap>,
    /// Seconds the window shows — the tail's own length, so a long delay is
    /// not drawn as a wall of taps against the left edge.
    pub window: f32,
    /// Wet/dry, for how much of the panel the taps are allowed to own.
    pub mix: f32,
    /// Engaged; a bypassed delay draws its shape unlit rather than vanishing,
    /// because "off" and "not configured" must not look the same.
    pub on: bool,
}

/// What the reverb panel draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReverbView {
    /// RT60 in seconds — the tail's length.
    pub decay: f32,
    /// 0..=1: how quickly early reflections thicken into a wash.
    pub density: f32,
    /// Pre-delay in seconds, before anything arrives.
    pub predelay: f32,
    pub mix: f32,
    pub on: bool,
}

/// The numbers a widget reads, written by the component that owns it.
pub type Shared<T> = Rc<RefCell<T>>;

/// The delay's taps, painted.
pub struct DelayWidget {
    view: Shared<DelayView>,
    gpu: Option<GpuSeam>,
}

/// The reverb's decay, painted.
pub struct ReverbWidget {
    view: Shared<ReverbView>,
    gpu: Option<GpuSeam>,
}

/// Where the WGSL path will attach.
///
/// Held rather than used: [`Widget::can_create_surfaces`] is the only place a
/// widget is handed a device, and it is called once, before the first paint.
/// Capturing it here means the shader path can land without touching the
/// component, the layout, or the fallback.
struct GpuSeam {
    /// The renderer's own context, type-erased — a wgpu `Device`/`Queue` pair
    /// on the vello backends, something else elsewhere, nothing at all on the
    /// CPU one.
    _ctx: Box<dyn std::any::Any>,
}

impl DelayWidget {
    #[must_use]
    pub fn new(view: Shared<DelayView>) -> Self {
        Self { view, gpu: None }
    }
}

impl ReverbWidget {
    #[must_use]
    pub fn new(view: Shared<ReverbView>) -> Self {
        Self { view, gpu: None }
    }
}

impl Widget for DelayWidget {
    fn can_create_surfaces(&mut self, render_ctx: &mut dyn RenderContext) {
        self.gpu = render_ctx
            .renderer_specific_context()
            .map(|ctx| GpuSeam { _ctx: ctx });
    }

    fn paint(
        &mut self,
        _render_ctx: &mut dyn RenderContext,
        _styles: &ComputedStyles,
        width: u32,
        height: u32,
        _scale: f64,
    ) -> Scene {
        let mut scene = Scene::new();
        let (w, h) = (f64::from(width), f64::from(height));
        if w < 2.0 || h < 2.0 {
            return scene;
        }
        paint_delay(&mut scene, &self.view.borrow(), w, h);
        scene
    }
}

impl Widget for ReverbWidget {
    fn can_create_surfaces(&mut self, render_ctx: &mut dyn RenderContext) {
        self.gpu = render_ctx
            .renderer_specific_context()
            .map(|ctx| GpuSeam { _ctx: ctx });
    }

    fn paint(
        &mut self,
        _render_ctx: &mut dyn RenderContext,
        _styles: &ComputedStyles,
        width: u32,
        height: u32,
        _scale: f64,
    ) -> Scene {
        let mut scene = Scene::new();
        let (w, h) = (f64::from(width), f64::from(height));
        if w < 2.0 || h < 2.0 {
            return scene;
        }
        paint_reverb(&mut scene, &self.view.borrow(), w, h);
        scene
    }
}

// ── The fallback painters ───────────────────────────────────────────────────

/// Teal for signal, amber for feedback, dimmed when bypassed.
fn palette(on: bool) -> (Color, Color) {
    if on {
        (
            Color::from_rgba8(34, 211, 238, 255),
            Color::from_rgba8(245, 158, 11, 255),
        )
    } else {
        (
            Color::from_rgba8(63, 63, 70, 255),
            Color::from_rgba8(63, 63, 70, 255),
        )
    }
}

fn with_alpha(c: Color, a: f32) -> Color {
    c.multiply_alpha(a.clamp(0.0, 1.0))
}

/// The taps, on a time axis, with the tail they imply.
pub fn paint_delay(scene: &mut Scene, view: &DelayView, w: f64, h: f64) {
    let (signal, accent) = palette(view.on);
    let mid = h * 0.5;
    let window = f64::from(view.window.max(0.05));

    // A ground that darkens to the right: time running out.
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &Gradient::new_linear(Point::new(0.0, 0.0), Point::new(w, 0.0)).with_stops([
            ColorStop::from((0.0, with_alpha(signal, 0.10))),
            ColorStop::from((1.0, Color::from_rgba8(0, 0, 0, 0))),
        ]),
        None,
        &Rect::new(0.0, 0.0, w, h),
    );

    // The centre line — the stereo axis the taps hang off.
    scene.stroke(
        &Stroke::new(1.0),
        Affine::IDENTITY,
        with_alpha(signal, 0.22),
        None,
        &Line::new(Point::new(0.0, mid), Point::new(w, mid)),
    );

    // The dry hit, at zero.
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        with_alpha(signal, 0.9),
        None,
        &Rect::new(0.0, mid - h * 0.42, 2.0, mid + h * 0.42),
    );

    // The envelope the taps decay along, as a filled curve — the shape of the
    // tail, which is the thing a number cannot show.
    if view.taps.len() > 1 {
        let mut env = BezPath::new();
        env.move_to((0.0, mid));
        for tap in &view.taps {
            let x = f64::from(tap.at) / window * w;
            let y = mid - f64::from(tap.level) * h * 0.42;
            env.line_to((x, y));
        }
        env.line_to((w, mid));
        for tap in view.taps.iter().rev() {
            let x = f64::from(tap.at) / window * w;
            let y = mid + f64::from(tap.level) * h * 0.42;
            env.line_to((x, y));
        }
        env.close_path();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &Gradient::new_linear(Point::new(0.0, 0.0), Point::new(w, 0.0)).with_stops([
                ColorStop::from((0.0, with_alpha(accent, 0.34))),
                ColorStop::from((1.0, with_alpha(accent, 0.02))),
            ]),
            None,
            &env,
        );
    }

    // Each tap: an impulse whose height is its level and whose offset from the
    // centre is its pan, with a head bright enough to count at a glance.
    for tap in &view.taps {
        let x = f64::from(tap.at) / window * w;
        if x > w {
            continue;
        }
        let level = f64::from(tap.level).clamp(0.0, 1.0);
        let reach = level * h * 0.42;
        let pan = f64::from(tap.pan).clamp(-1.0, 1.0);
        let y = mid - pan * h * 0.16;
        scene.stroke(
            &Stroke::new(2.0),
            Affine::IDENTITY,
            with_alpha(signal, 0.35 + 0.65 * level as f32),
            None,
            &Line::new(Point::new(x, y - reach), Point::new(x, y + reach)),
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            with_alpha(signal, 0.5 + 0.5 * level as f32),
            None,
            &Circle::new(Point::new(x, y), 1.5 + 2.0 * level),
        );
    }
}

/// The reverb's decay: pre-delay, early reflections, then the tail.
pub fn paint_reverb(scene: &mut Scene, view: &ReverbView, w: f64, h: f64) {
    let (signal, accent) = palette(view.on);
    let decay = f64::from(view.decay.max(0.05));
    // Show the whole tail plus a little air, so a long reverb is not clipped
    // at the right edge and a short one is not lost against it.
    let window = (decay * 1.15).max(0.2);
    let pre = f64::from(view.predelay) / window * w;
    let density = f64::from(view.density.clamp(0.0, 1.0));

    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &Gradient::new_linear(Point::new(0.0, 0.0), Point::new(0.0, h)).with_stops([
            ColorStop::from((0.0, with_alpha(signal, 0.12))),
            ColorStop::from((1.0, Color::from_rgba8(0, 0, 0, 0))),
        ]),
        None,
        &Rect::new(0.0, 0.0, w, h),
    );

    // The decay envelope, exponential to −60 dB across the tail.
    let mut env = BezPath::new();
    env.move_to((pre, h));
    let steps = 96;
    for i in 0..=steps {
        let t = f64::from(i) / f64::from(steps);
        let x = pre + t * (w - pre);
        let secs = t * window;
        let amp = (-6.908 * secs / decay).exp();
        env.line_to((x, h - amp * h * 0.88));
    }
    env.line_to((w, h));
    env.close_path();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &Gradient::new_linear(Point::new(0.0, h), Point::new(0.0, 0.0)).with_stops([
            ColorStop::from((0.0, with_alpha(signal, 0.06))),
            ColorStop::from((1.0, with_alpha(signal, 0.46))),
        ]),
        None,
        &env,
    );

    // Early reflections: discrete arrivals thickening with density, each one
    // sitting on the envelope so the two read as one event.
    let reflections = 6 + (density * 26.0) as usize;
    for i in 0..reflections {
        let t = f64::from(i as u32) / reflections as f64;
        // Cluster toward the start — reflections arrive fast, then blur.
        let at = t.powf(0.55) * 0.45;
        let x = pre + at * (w - pre);
        let secs = at * window;
        let amp = (-6.908 * secs / decay).exp();
        let top = h - amp * h * 0.88;
        scene.stroke(
            &Stroke::new(1.0),
            Affine::IDENTITY,
            with_alpha(accent, (0.5 * (1.0 - t as f32).max(0.06))),
            None,
            &Line::new(Point::new(x, top), Point::new(x, h)),
        );
    }

    // Pre-delay: the silence before any of it, marked rather than implied.
    if pre > 1.0 {
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            with_alpha(signal, 0.07),
            None,
            &Rect::new(0.0, 0.0, pre, h),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taps(n: usize) -> Vec<Tap> {
        (0..n)
            .map(|i| Tap {
                at: 0.4 * (i as f32 + 1.0),
                level: 0.8f32.powi(i as i32 + 1),
                pan: if i % 2 == 0 { -0.7 } else { 0.7 },
            })
            .collect()
    }

    /// Painting produces a scene rather than panicking, at the sizes a panel
    /// actually takes — including the degenerate ones a flex row hands out
    /// mid-layout.
    #[test]
    fn it_paints_at_every_size() {
        let view = DelayView {
            taps: taps(6),
            window: 2.0,
            mix: 0.3,
            on: true,
        };
        for (w, h) in [(2.0, 2.0), (120.0, 40.0), (1280.0, 300.0)] {
            let mut scene = Scene::new();
            paint_delay(&mut scene, &view, w, h);
        }
        let rev = ReverbView {
            decay: 2.4,
            density: 0.6,
            predelay: 0.02,
            mix: 0.3,
            on: true,
        };
        for (w, h) in [(2.0, 2.0), (120.0, 40.0), (1280.0, 300.0)] {
            let mut scene = Scene::new();
            paint_reverb(&mut scene, &rev, w, h);
        }
    }

    /// A reverb with no decay set still has a window, so the envelope maths
    /// cannot divide by zero or run off the panel.
    #[test]
    fn a_zero_reverb_still_has_a_window() {
        let mut scene = Scene::new();
        paint_reverb(&mut scene, &ReverbView::default(), 200.0, 80.0);
    }

    /// A bypassed block draws unlit rather than empty: "off" and "not
    /// configured" must not look the same.
    #[test]
    fn bypassed_is_not_blank() {
        let (on_signal, _) = palette(true);
        let (off_signal, _) = palette(false);
        assert_ne!(on_signal, off_signal);
    }
}

// ── Mounting ────────────────────────────────────────────────────────────────

use dioxus::prelude::*;

/// A delay lane, painted by [`DelayWidget`].
///
/// A component per lane because `CustomWidgetAttr` is write-once: the widget
/// is built in a hook that runs once, and every later render pushes numbers
/// through the shared cell instead of rebuilding it. Rebuilding would hand
/// Blitz a second widget for the same node and lose the first one's state.
#[component]
pub fn DelayViz(taps: Vec<(f32, f32, bool)>, win_ms: f32, on: bool) -> Element {
    let view: Shared<DelayView> = use_hook(|| Rc::new(RefCell::new(DelayView::default())));
    let attr = use_hook(|| {
        dioxus_native_dom::CustomWidgetAttr::new(DelayWidget::new(Rc::clone(&view)))
    });

    // Milliseconds in, seconds out: the widget speaks in the units a tail is
    // measured in, and the panel happens to hold the other.
    *view.borrow_mut() = DelayView {
        taps: taps
            .iter()
            .map(|(t, amp, up)| Tap {
                at: t / 1000.0,
                level: *amp,
                pan: if *up { -0.8 } else { 0.8 },
            })
            .collect(),
        window: win_ms / 1000.0,
        mix: 1.0,
        on,
    };

    rsx! {
        object {
            "data": attr,
            style: "position:absolute; top:0; left:0; right:0; bottom:0; \
                    width:100%; height:100%; display:block; pointer-events:none;",
        }
    }
}

/// A reverb, painted by [`ReverbWidget`].
#[component]
pub fn ReverbViz(decay: f32, density: f32, predelay: f32, mix: f32, on: bool) -> Element {
    let view: Shared<ReverbView> = use_hook(|| Rc::new(RefCell::new(ReverbView::default())));
    let attr = use_hook(|| {
        dioxus_native_dom::CustomWidgetAttr::new(ReverbWidget::new(Rc::clone(&view)))
    });

    *view.borrow_mut() = ReverbView {
        decay,
        density,
        predelay,
        mix,
        on,
    };

    rsx! {
        object {
            "data": attr,
            style: "position:absolute; top:0; left:0; right:0; bottom:0; \
                    width:100%; height:100%; display:block; pointer-events:none;",
        }
    }
}
