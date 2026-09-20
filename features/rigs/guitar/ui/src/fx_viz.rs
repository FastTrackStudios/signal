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
    /// Seconds per beat. The whole point of the picture: a tap that lands on a
    /// gridline is a tap in time, and a quarter-note delay is one you can see
    /// is a quarter note without reading a number.
    pub beat: f32,
    /// What the division is called ("1/4", "1/8."), if the block is locked to
    /// one. Drawn once rather than per tap.
    pub division: String,
    /// Seconds since the panel appeared — the animation's clock.
    ///
    /// Read from the widget's own `Instant` rather than pushed in by the
    /// component: a repaint can happen for reasons the component knows
    /// nothing about, and an animation that only advances when a prop
    /// changes is an animation that stutters.
    pub time: f32,
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
    /// Seconds per beat — the tail is measured against the tempo, so "two
    /// bars of reverb" is a thing the picture can say.
    pub beat: f32,
    /// Seconds since the panel appeared. See [`DelayView::time`].
    pub time: f32,
}

/// The numbers a widget reads, written by the component that owns it.
pub type Shared<T> = Rc<RefCell<T>>;

/// The delay's taps, painted.
pub struct DelayWidget {
    view: Shared<DelayView>,
    gpu: Option<GpuSeam>,
    born: std::time::Instant,
}

/// The reverb's decay, painted.
pub struct ReverbWidget {
    view: Shared<ReverbView>,
    gpu: Option<GpuSeam>,
    born: std::time::Instant,
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
        Self {
            view,
            gpu: None,
            born: std::time::Instant::now(),
        }
    }
}

impl ReverbWidget {
    #[must_use]
    pub fn new(view: Shared<ReverbView>) -> Self {
        Self {
            view,
            gpu: None,
            born: std::time::Instant::now(),
        }
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
        let mut view = self.view.borrow().clone();
        view.time = self.born.elapsed().as_secs_f32();
        paint_delay(&mut scene, &view, w, h);
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
        let mut view = self.view.borrow().clone();
        view.time = self.born.elapsed().as_secs_f32();
        paint_reverb(&mut scene, &view, w, h);
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

/// The beat grid: a line per beat across `window` seconds, the downbeat of
/// every bar brighter.
///
/// Drawn under everything else, because it is the ruler the rest is read
/// against — a tap sitting exactly on a line is the whole message.
fn paint_beats(scene: &mut Scene, w: f64, h: f64, window: f64, beat: f64, lit: bool) {
    if beat <= 0.0 || window <= 0.0 {
        return;
    }
    let beats = (window / beat).ceil() as usize;
    // A grid denser than this stops being a ruler and becomes a texture.
    if beats > 64 {
        return;
    }
    for i in 1..=beats {
        let t = beat * i as f64;
        if t > window {
            break;
        }
        let x = t / window * w;
        let bar = i % 4 == 0;
        let alpha = if !lit {
            0.06
        } else if bar {
            0.30
        } else {
            0.13
        };
        scene.stroke(
            &Stroke::new(if bar { 1.5 } else { 1.0 }),
            Affine::IDENTITY,
            Color::from_rgba8(148, 163, 184, 255).multiply_alpha(alpha),
            None,
            &Line::new(Point::new(x, 0.0), Point::new(x, h)),
        );
    }
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

    paint_beats(scene, w, h, window, f64::from(view.beat), view.on);

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

    // The division's NAME is not drawn here. Text in a scene needs a font
    // handle the widget does not have, and an empty chip is worse than no
    // chip — the panel's own "1/4" selector is inches away, and what this
    // picture adds is that the taps sit on the grid, which needs no caption.

    // The playhead: a pulse crossing the window once per cycle, so the panel
    // keeps the tempo even when nothing is being played into it.
    //
    // Not decoration. The taps are static geometry — they say *where* the
    // repeats land — and the sweep is what makes that a rhythm you can read
    // at a glance rather than a row of sticks.
    let beat = f64::from(view.beat).max(1e-3);
    let cycle = beat * ((window / beat).ceil()).max(1.0);
    let head = if view.on && cycle > 0.0 {
        (f64::from(view.time) % cycle) / cycle
    } else {
        -1.0
    };

    // Each tap: an impulse whose height is its level and whose offset from the
    // centre is its pan, with a head bright enough to count at a glance. A tap
    // blooms as the sweep reaches it and falls back over the next beat.
    for tap in &view.taps {
        let x = f64::from(tap.at) / window * w;
        if x > w {
            continue;
        }
        let level = f64::from(tap.level).clamp(0.0, 1.0);

        // How recently the playhead passed this tap, 0..=1.
        let hit = if head < 0.0 {
            0.0
        } else {
            let at = f64::from(tap.at) / window;
            let since = (head - at + 1.0) % 1.0;
            // A bloom that dies within a beat, so two taps a beat apart never
            // glow at once and the eye follows one moving highlight.
            let over = beat / cycle;
            if since < over {
                (1.0 - since / over).powi(3)
            } else {
                0.0
            }
        };

        let reach = level * h * 0.42 * (1.0 + 0.22 * hit);
        let pan = f64::from(tap.pan).clamp(-1.0, 1.0);
        let y = mid - pan * h * 0.16;

        // The bloom, behind: a soft halo that only exists while lit.
        if hit > 0.01 {
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                with_alpha(accent, (0.30 * hit) as f32),
                None,
                &Circle::new(Point::new(x, y), 4.0 + 16.0 * hit * (0.4 + level)),
            );
        }

        scene.stroke(
            &Stroke::new(2.0 + 1.5 * hit),
            Affine::IDENTITY,
            with_alpha(signal, (0.35 + 0.65 * level + 0.6 * hit) as f32),
            None,
            &Line::new(Point::new(x, y - reach), Point::new(x, y + reach)),
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            with_alpha(signal, (0.5 + 0.5 * level + 0.5 * hit) as f32),
            None,
            &Circle::new(Point::new(x, y), 1.5 + 2.0 * level + 2.5 * hit),
        );
    }

    // The sweep itself — a thin bright edge with a trail behind it.
    if head >= 0.0 {
        let hx = head * w;
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &Gradient::new_linear(Point::new(hx - w * 0.08, 0.0), Point::new(hx, 0.0)).with_stops([
                ColorStop::from((0.0, with_alpha(signal, 0.0))),
                ColorStop::from((1.0, with_alpha(signal, 0.16))),
            ]),
            None,
            &Rect::new((hx - w * 0.08).max(0.0), 0.0, hx.max(0.0), h),
        );
        scene.stroke(
            &Stroke::new(1.0),
            Affine::IDENTITY,
            with_alpha(signal, 0.5),
            None,
            &Line::new(Point::new(hx, 0.0), Point::new(hx, h)),
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

    paint_beats(scene, w, h, window, f64::from(view.beat), view.on);

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

    // A shimmer riding the tail: a bright band travelling from the onset out
    // to where the decay dies, once per bar.
    //
    // What it shows is the reverb's own time — how far the tail actually
    // reaches before it is gone — which a static envelope states and a moving
    // one makes you feel.
    let beat = f64::from(view.beat).max(1e-3);
    if view.on {
        let bar = beat * 4.0;
        let phase = (f64::from(view.time) % bar) / bar;
        let head_t = phase * window;
        let hx = pre + (head_t / window) * (w - pre);
        let amp = (-6.908 * head_t / decay).exp();
        let top = h - amp * h * 0.88;
        // Fades out as the tail does, so the shimmer dies where the reverb
        // does rather than sweeping on through silence.
        let lit = (amp * (1.0 - phase * 0.35)).clamp(0.0, 1.0) as f32;
        if lit > 0.01 && hx <= w {
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                &Gradient::new_linear(Point::new(hx - w * 0.10, 0.0), Point::new(hx, 0.0))
                    .with_stops([
                        ColorStop::from((0.0, with_alpha(signal, 0.0))),
                        ColorStop::from((1.0, with_alpha(signal, 0.26 * lit))),
                    ]),
                None,
                &Rect::new((hx - w * 0.10).max(0.0), top, hx, h),
            );
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                with_alpha(accent, 0.55 * lit),
                None,
                &Circle::new(Point::new(hx, top), 2.0 + 5.0 * f64::from(lit)),
            );
        }
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
            beat: 0.4,
            division: "1/4".to_string(),
            time: 0.0,
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
            beat: 0.4,
            time: 0.0,
        };
        for (w, h) in [(2.0, 2.0), (120.0, 40.0), (1280.0, 300.0)] {
            let mut scene = Scene::new();
            paint_reverb(&mut scene, &rev, w, h);
        }
    }

    /// A delay locked to a quarter note puts a tap on every beat line — the
    /// whole reason the grid is drawn. Checked as geometry rather than
    /// pixels: tap `n` sits at `n` beats, so it shares an x with gridline `n`.
    #[test]
    fn a_quarter_note_delay_lands_on_the_grid() {
        let beat = 0.4_f32;
        let taps: Vec<Tap> = (1..=4)
            .map(|n| Tap {
                at: beat * n as f32,
                level: 0.8_f32.powi(n),
                pan: 0.0,
            })
            .collect();
        let window = 8.0_f64 * f64::from(beat);
        for (i, tap) in taps.iter().enumerate() {
            let tap_x = f64::from(tap.at) / window;
            let line_x = f64::from(beat) * (i + 1) as f64 / window;
            // A fraction of the window, so the tolerance means something on
            // screen: 1e-6 of a 2560px panel is three thousandths of a pixel.
            // Tighter than that is measuring f32's rounding, not alignment.
            assert!(
                (tap_x - line_x).abs() < 1e-6,
                "tap {i} at {tap_x} should sit on gridline at {line_x}"
            );
        }
    }

    /// No tempo, no grid — and no division by zero either.
    #[test]
    fn a_free_running_delay_draws_no_grid() {
        let mut scene = Scene::new();
        paint_beats(&mut scene, 400.0, 60.0, 2.0, 0.0, true);
    }

    /// The animation stays inside the panel and repeats: a sweep that runs
    /// off the end, or never comes back, is a sweep nobody can read a tempo
    /// from. Painting at a spread of times must not panic or diverge.
    #[test]
    fn the_sweep_wraps_and_stays_in_frame() {
        let mut view = DelayView {
            taps: taps(4),
            window: 3.2,
            mix: 1.0,
            on: true,
            beat: 0.4,
            division: "1/4".to_string(),
            time: 0.0,
        };
        for step in 0..200 {
            view.time = step as f32 * 0.05;
            let mut scene = Scene::new();
            paint_delay(&mut scene, &view, 640.0, 56.0);
        }
        let mut rev = ReverbView {
            decay: 2.4,
            density: 0.6,
            predelay: 0.02,
            mix: 0.3,
            on: true,
            beat: 0.4,
            time: 0.0,
        };
        for step in 0..200 {
            rev.time = step as f32 * 0.05;
            let mut scene = Scene::new();
            paint_reverb(&mut scene, &rev, 640.0, 56.0);
        }
    }

    /// A bypassed block does not animate — a panel that is not in the signal
    /// path must not look like one that is.
    #[test]
    fn bypassed_does_not_sweep() {
        let view = DelayView {
            taps: taps(4),
            window: 3.2,
            mix: 1.0,
            on: false,
            beat: 0.4,
            division: String::new(),
            time: 1.7,
        };
        let mut scene = Scene::new();
        paint_delay(&mut scene, &view, 640.0, 56.0);
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

/// Mark this scope dirty ~40 times a second, for as long as it lives.
///
/// Blitz repaints when the document changes, and an animation changes
/// nothing in the DOM — the movement is inside a widget's scene. So the
/// clock has to come from outside: a thread that pokes the runtime, which
/// is the same driver `eq_graph` uses for its analyser.
///
/// `schedule_update` is documented as safe to call from off the runtime,
/// which is exactly what this is.
fn use_repaint_clock() {
    use_hook(|| {
        let updater = dioxus_core::schedule_update();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(25));
                updater();
            }
        });
    });
}

/// A delay lane, painted by [`DelayWidget`].
///
/// A component per lane because `CustomWidgetAttr` is write-once: the widget
/// is built in a hook that runs once, and every later render pushes numbers
/// through the shared cell instead of rebuilding it. Rebuilding would hand
/// Blitz a second widget for the same node and lose the first one's state.
#[component]
pub fn DelayViz(
    taps: Vec<(f32, f32, bool)>,
    win_ms: f32,
    on: bool,
    beat_ms: f32,
    division: String,
) -> Element {
    use_repaint_clock();
    let view: Shared<DelayView> = use_hook(|| Rc::new(RefCell::new(DelayView::default())));
    let attr = use_hook(|| {
        dioxus_native_dom::CustomWidgetAttr::new(DelayWidget::new(Rc::clone(&view)))
    });

    // Milliseconds in, seconds out: the widget speaks in the units a tail is
    // measured in, and the panel happens to hold the other.
    // Levels normalised to the loudest tap.
    //
    // The panel's amplitudes start at the wet mix, so a delay at 8% sits in
    // the bottom twentieth of the lane and its decay is invisible. What the
    // picture is for is the PATTERN — where the taps land and how fast they
    // give up — and both survive normalising; the absolute level is on the
    // MIX knob two inches away.
    let peak = taps.iter().map(|(_, a, _)| *a).fold(0.0f32, f32::max);
    let scale = if peak > f32::EPSILON { 1.0 / peak } else { 1.0 };
    *view.borrow_mut() = DelayView {
        taps: taps
            .iter()
            .map(|(t, amp, up)| Tap {
                at: t / 1000.0,
                level: (amp * scale).clamp(0.0, 1.0),
                pan: if *up { -0.8 } else { 0.8 },
            })
            .collect(),
        window: win_ms / 1000.0,
        mix: 1.0,
        on,
        beat: beat_ms / 1000.0,
        division,
        // The widget keeps its own clock; this is only a starting value.
        time: 0.0,
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
pub fn ReverbViz(
    decay: f32,
    density: f32,
    predelay: f32,
    mix: f32,
    on: bool,
    beat_ms: f32,
) -> Element {
    use_repaint_clock();
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
        beat: beat_ms / 1000.0,
        time: 0.0,
    };

    rsx! {
        object {
            "data": attr,
            style: "position:absolute; top:0; left:0; right:0; bottom:0; \
                    width:100%; height:100%; display:block; pointer-events:none;",
        }
    }
}
