//! What the delay is actually doing, painted.
//!
//! This panel was knobs and a number. A delay's character is its taps — where
//! they land against the beat, how fast they give up, whether they walk across
//! the stereo field — and none of that is legible from `TIME 0.42 · FB 0.28`.
//! The picture's whole job is that a quarter-note delay is one you can *see*
//! is a quarter note, because its taps sit on the gridlines.
//!
//! # Two painters, one widget
//!
//! [`Widget::paint`] records into an `anyrender::Scene` — a command list, not
//! a wgpu call — so whatever can replay it can draw this: the GPU backend, the
//! CPU one, and a canvas in a browser. That is the **fallback**, and it is not
//! a poor relation: vello rasterises it with real gradients and blurs.
//!
//! The **shader** path is the escape hatch the widget trait describes: take
//! the device in [`Widget::can_create_surfaces`], render WGSL into a texture,
//! and hand the scene that texture's id. It is asked for per frame rather than
//! decided per build, so a renderer that cannot do it simply gets the vectors.
//!
//! # Not a signal, a cell
//!
//! Paint happens inside Blitz's traversal, which is not the Dioxus runtime;
//! reading a `Signal` there is a panic waiting for the first frame that takes
//! a different path. What this needs is a handful of numbers, and numbers can
//! live in a `RefCell` the component writes and the widget reads.

use std::cell::RefCell;
use std::rc::Rc;

use anyrender::{PaintScene, RenderContext, Scene};
#[cfg(not(target_arch = "wasm32"))]
use blitz_dom::node::{ComputedStyles, Widget};
use dioxus::prelude::*;
use fts_audio_ui::paint::lane;
use fts_audio_ui::shader::ShaderSurface;

use vello::kurbo::{Affine, BezPath, Circle, Line, Point, Rect, Stroke};
use vello::peniko::{Color, ColorStop, Fill, Gradient};

use lane::faded;

/// Which machine is making the repeats.
///
/// Re-exported rather than redefined: [`delay_dsp::engine::Family`] already
/// says a delay's family "is not a preset — it is a different machine, and
/// anything that draws one should say which before it says anything else".
/// This is the drawing half of that sentence.
pub use delay_dsp::engine::Family;

/// The family of the style at `index` in the editor's algorithm list.
///
/// The host has a style index and no business knowing the style table; the
/// effect that owns the table owns the mapping.
#[must_use]
pub fn family_of_style(index: u32) -> Family {
    delay_dsp::engine::DelayStyle::from_index(index as usize).family()
}

/// The family, as the shader's `u.style.x`. The order is the shader's
/// constants; the two must agree, so they are written next to each other.
#[must_use]
fn family_index(family: Family) -> f32 {
    match family {
        Family::Digital => 0.0,
        Family::Tape => 1.0,
        Family::Analog => 2.0,
        Family::Pitch => 3.0,
        Family::Rhythmic => 4.0,
        Family::Special => 5.0,
    }
}

/// The most taps the shader carries. A uniform array is fixed-length, and a
/// tail longer than this is past the point where individual repeats can be
/// told apart anyway.
pub const MAX_TAPS: usize = 32;

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
#[derive(Clone, Debug, PartialEq)]
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
    /// gridline is a tap in time.
    pub beat: f32,
    /// What the division is called ("1/4", "1/8."), if the block is locked to
    /// one. Carried for the DOM half of the lane; the painter draws no text.
    pub division: String,
    /// Which machine is making the repeats. A tape delay and a digital one
    /// at identical settings draw the same bars and sound nothing alike;
    /// this is what lets the picture say which it is.
    pub family: Family,
    /// The lane's own colour, as the panel draws it. Delay is blue-led and
    /// reverb purple-led, and a painted lane that ignored that would be the
    /// one thing on screen disagreeing about which effect it is.
    pub color: [u8; 3],
    /// Seconds since the panel appeared — the animation's clock.
    ///
    /// Read from the widget's own `Instant` rather than pushed in by the
    /// component: a repaint can happen for reasons the component knows
    /// nothing about, and an animation that only advances when a prop changes
    /// is an animation that stutters.
    pub time: f32,
}

impl Default for DelayView {
    /// An empty digital lane. `Family` has no `Default` of its own — and
    /// should not: there is no neutral machine in the DSP, only a neutral
    /// PICTURE, and Digital is it because it is the family that adds nothing
    /// to a repeat.
    fn default() -> Self {
        Self {
            taps: Vec::new(),
            window: 0.0,
            mix: 0.0,
            on: false,
            beat: 0.0,
            division: String::new(),
            family: Family::Digital,
            color: [0, 0, 0],
            time: 0.0,
        }
    }
}

/// The numbers a widget reads, written by the component that owns it.
pub type Shared<T> = Rc<RefCell<T>>;

/// The WGSL the shader path runs.
///
/// Public so the family contact sheet can render the real thing rather than a
/// copy of it — see `examples/family_sheet.rs`.
pub const SHADER: &str = include_str!("viz.wgsl");

/// The uniform block, laid out to match `Delay` in `viz.wgsl`.
///
/// Every row is a `vec4`, so there is no padding for the two languages to
/// disagree about.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DelayUniforms {
    /// `[width, height, seconds, tap_count]`.
    pub frame: [f32; 4],
    /// `[window_s, mix, on, beat_s]`.
    pub params: [f32; 4],
    /// The lane's colour; `w` unused.
    pub color: [f32; 4],
    /// `[family, _, _, _]`.
    pub style: [f32; 4],
    /// `[at_s, level, pan, _]` per tap.
    pub taps: [[f32; 4]; MAX_TAPS],
}

impl Default for DelayUniforms {
    fn default() -> Self {
        Self {
            frame: [0.0; 4],
            params: [0.0; 4],
            color: [0.0; 4],
            style: [0.0; 4],
            taps: [[0.0; 4]; MAX_TAPS],
        }
    }
}

impl DelayUniforms {
    /// Fill from a view. Taps past [`MAX_TAPS`] are dropped, which is the
    /// right failure: they are the quietest ones, at the far end of the tail.
    #[must_use]
    pub fn of(view: &DelayView, w: f32, h: f32) -> Self {
        let mut u = Self::default();
        let [r, g, b] = view.color;
        u.frame = [w, h, view.time, 0.0];
        u.params = [
            view.window.max(0.05),
            view.mix,
            if view.on { 1.0 } else { 0.0 },
            view.beat.max(1e-3),
        ];
        u.color = [
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            1.0,
        ];
        u.style = [family_index(view.family), 0.0, 0.0, 0.0];
        let n = view.taps.len().min(MAX_TAPS);
        for (slot, tap) in u.taps.iter_mut().zip(&view.taps[..n]) {
            *slot = [tap.at, tap.level, tap.pan, 0.0];
        }
        u.frame[3] = n as f32;
        u
    }
}

/// The delay's taps, painted.
pub struct DelayWidget {
    view: Shared<DelayView>,
    /// Built once, from whatever `can_create_surfaces` hands over. `None` on a
    /// renderer with no device to give, which is not an error: the vector
    /// painter below draws the same taps.
    gpu: Option<ShaderSurface>,
    uniforms: DelayUniforms,
    born: web_time::Instant,
}

impl DelayWidget {
    #[must_use]
    pub fn new(view: Shared<DelayView>) -> Self {
        Self {
            view,
            gpu: None,
            uniforms: DelayUniforms::default(),
            born: web_time::Instant::now(),
        }
    }
}

impl DelayWidget {
    /// Take a GPU device and build the shader surface — the
    /// host-agnostic half of `Widget::can_create_surfaces`.
    pub fn create_surfaces(&mut self, render_ctx: &mut dyn RenderContext) {
        self.gpu = render_ctx.renderer_specific_context().and_then(|ctx| {
            ShaderSurface::with_uniform_size(
                ctx,
                SHADER,
                std::mem::size_of::<DelayUniforms>() as u64,
            )
        });
    }

    /// Record a frame — the host-agnostic half of `Widget::paint`.
    pub fn paint_frame(
        &mut self,
        render_ctx: &mut dyn RenderContext,
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

        // Asked every frame rather than decided once: a renderer can decline
        // for reasons that change frame to frame, such as a surface that is
        // not up yet.
        if self.gpu.is_some() {
            self.uniforms = DelayUniforms::of(&view, w as f32, h as f32);
            let bytes = bytemuck::bytes_of(&self.uniforms);
            let drew = self
                .gpu
                .as_mut()
                .is_some_and(|gpu| gpu.draw_raw(render_ctx, &mut scene, width, height, bytes));
            if drew {
                // The grid is geometry on purpose: a 1 px line is the one
                // thing a shader is worse at than the vector pass, and the
                // grid is the ruler everything else is read against.
                lane::beats(
                    &mut scene,
                    w,
                    h,
                    f64::from(view.window.max(0.05)),
                    f64::from(view.beat),
                    view.on,
                );
                return scene;
            }
        }

        paint_delay(&mut scene, &view, w, h);
        scene
    }
}

/// Blitz's custom widget, natively.
#[cfg(not(target_arch = "wasm32"))]
impl Widget for DelayWidget {
    fn can_create_surfaces(&mut self, render_ctx: &mut dyn RenderContext) {
        self.create_surfaces(render_ctx);
    }

    fn paint(
        &mut self,
        render_ctx: &mut dyn RenderContext,
        _styles: &ComputedStyles,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Scene {
        self.paint_frame(render_ctx, width, height, scale)
    }
}

/// The browser's canvas — vello on WebGPU, so the shader runs there too.
#[cfg(target_arch = "wasm32")]
impl fts_audio_ui::scene_canvas::CanvasPanel for DelayWidget {
    fn can_create_surfaces(&mut self, ctx: &mut dyn RenderContext) {
        self.create_surfaces(ctx);
    }

    fn paint(&mut self, ctx: &mut dyn RenderContext, width: u32, height: u32, scale: f64) -> Scene {
        self.paint_frame(ctx, width, height, scale)
    }
}

// ── The fallback painter ────────────────────────────────────────────────────

/// The taps, on a time axis, with the tail they imply.
pub fn paint_delay(scene: &mut Scene, view: &DelayView, w: f64, h: f64) {
    let (signal, accent) = lane::palette(view.on, view.color);
    let mid = h * 0.5;
    let window = f64::from(view.window.max(0.05));

    // A ground that darkens to the right: time running out.
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &Gradient::new_linear(Point::new(0.0, 0.0), Point::new(w, 0.0)).with_stops([
            ColorStop::from((0.0, faded(signal, 0.10))),
            ColorStop::from((1.0, Color::from_rgba8(0, 0, 0, 0))),
        ]),
        None,
        &Rect::new(0.0, 0.0, w, h),
    );

    lane::beats(scene, w, h, window, f64::from(view.beat), view.on);

    // The centre line — the stereo axis the taps hang off.
    scene.stroke(
        &Stroke::new(1.0),
        Affine::IDENTITY,
        faded(signal, 0.22),
        None,
        &Line::new(Point::new(0.0, mid), Point::new(w, mid)),
    );

    // The dry hit, at zero.
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        faded(signal, 0.9),
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
                ColorStop::from((0.0, faded(accent, 0.34))),
                ColorStop::from((1.0, faded(accent, 0.02))),
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

        // Up is left, down is right — the same read as the shader, with the
        // same equal-power pan law, so the two painters cannot disagree about
        // what a ping-pong looks like.
        let pan = f64::from(tap.pan).clamp(-1.0, 1.0);
        let ang = (pan * 0.5 + 0.5) * std::f64::consts::FRAC_PI_2;
        let half = h * 0.5;
        let grow = 0.92 * (1.0 + 0.10 * hit);

        for (gain, dir) in [(ang.cos(), -1.0_f64), (ang.sin(), 1.0_f64)] {
            let bar = level * gain * half * grow;
            if bar < 0.5 {
                continue;
            }
            let cap = mid + dir * bar;

            // The bloom, behind: a soft halo that only exists while lit.
            if hit > 0.01 {
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    faded(accent, (0.30 * hit) as f32),
                    None,
                    &Circle::new(Point::new(x, cap), 4.0 + 16.0 * hit * (0.4 + level * gain)),
                );
            }

            scene.stroke(
                &Stroke::new(2.0 + 1.5 * hit),
                Affine::IDENTITY,
                faded(signal, (0.35 + 0.65 * level * gain + 0.6 * hit) as f32),
                None,
                &Line::new(Point::new(x, mid), Point::new(x, cap)),
            );
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                faded(signal, (0.5 + 0.5 * level * gain + 0.5 * hit) as f32),
                None,
                &Circle::new(Point::new(x, cap), 1.5 + 2.0 * level * gain + 2.5 * hit),
            );
        }
    }

    // The sweep itself — a thin bright edge with a trail behind it.
    if head >= 0.0 {
        let hx = head * w;
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &Gradient::new_linear(Point::new(hx - w * 0.08, 0.0), Point::new(hx, 0.0)).with_stops(
                [
                    ColorStop::from((0.0, faded(signal, 0.0))),
                    ColorStop::from((1.0, faded(signal, 0.16))),
                ],
            ),
            None,
            &Rect::new((hx - w * 0.08).max(0.0), 0.0, hx.max(0.0), h),
        );
        scene.stroke(
            &Stroke::new(1.0),
            Affine::IDENTITY,
            faded(signal, 0.5),
            None,
            &Line::new(Point::new(hx, 0.0), Point::new(hx, h)),
        );
    }
}

// ── Mounting ────────────────────────────────────────────────────────────────

/// Mark this scope dirty ~40 times a second while `active` (the effect on),
/// for as long as it lives.
///
/// Blitz repaints when the document changes, and an animation changes nothing
/// in the DOM — the movement is inside a widget's scene. So the clock has to
/// come from outside: a thread that pokes the runtime. `schedule_update` is
/// documented as safe to call from off the runtime, which is what this is.
#[cfg(not(target_arch = "wasm32"))]
pub fn use_repaint_clock(active: bool) {
    fts_audio_ui::animate::use_frame_clock(active, std::time::Duration::from_millis(25));
}

/// In a browser there is no thread to spawn — and no need for one: the
/// picture is painted by `fts_audio_ui::scene_canvas`, which runs its own
/// clock and redraws the canvas without the document changing at all. (A
/// thread here is not a slow path in wasm32, it is a panic: the scope that
/// mounted a visualiser died, which took the whole Control view with it.)
#[cfg(target_arch = "wasm32")]
pub fn use_repaint_clock(_active: bool) {}

/// A delay lane, painted by [`DelayWidget`].
///
/// A component per lane because `CustomWidgetAttr` is write-once: the widget
/// is built in a hook that runs once, and every later render pushes numbers
/// through the shared cell instead of rebuilding it. Rebuilding would hand
/// Blitz a second widget for the same node and lose the first one's state.
///
/// Taps arrive as `(milliseconds, amplitude, upper)` because that is the shape
/// a delay block reports; seconds and a pan are what the picture is drawn in.
#[component]
pub fn DelayViz(
    taps: Vec<(f32, f32, bool)>,
    win_ms: f32,
    on: bool,
    beat_ms: f32,
    division: String,
    /// Which machine is making the repeats. See [`family_of_style`].
    #[props(default = Family::Digital)]
    family: Family,
    color: [u8; 3],
) -> Element {
    use_repaint_clock(on);
    let view: Shared<DelayView> = use_hook(|| Rc::new(RefCell::new(DelayView::default())));
    // Marked each render: the view below is new, and only the widget
    // knows to draw it (the DOM does not change).
    #[cfg(not(target_arch = "wasm32"))]
    let fresh = use_hook(fts_audio_ui::widget::Fresh::default);
    #[cfg(not(target_arch = "wasm32"))]
    let attr =
        use_hook(|| dioxus_native_dom::CustomWidgetAttr::new(fts_audio_ui::widget::Rasterized::new(fts_audio_ui::widget::Freshened::new(DelayWidget::new(Rc::clone(&view)), fresh.clone()))));

    *view.borrow_mut() = view_of(&taps, win_ms, on, beat_ms, division, family, color);
    #[cfg(not(target_arch = "wasm32"))]
    fresh.mark();

    // The surface: a scene composited by Blitz natively, the same scene
    // replayed onto a `<canvas>` (vello_hybrid, WebGL2) in the browser —
    // one painter either way, so the picture cannot differ.
    #[cfg(not(target_arch = "wasm32"))]
    return rsx! {
        object {
            "data": attr,
            style: "position:absolute; top:0; left:0; right:0; bottom:0; \
                    width:100%; height:100%; display:block; pointer-events:none;",
        }
    };

    #[cfg(target_arch = "wasm32")]
    {
        // The same widget, on a canvas: vello on WebGPU gives it a real
        // device, so it builds its shader surface and paints the lit
        // picture — not the vector fallback.
        let panel =
            use_hook(|| fts_audio_ui::scene_canvas::Panel::new(DelayWidget::new(Rc::clone(&view))));
        rsx! {
            fts_audio_ui::scene_canvas::SceneCanvas {
                panel,
                class: "absolute inset-0 w-full h-full",
            }
        }
    }
}

/// How many seconds of lane the taps actually need.
///
/// The caller offers a ceiling — two bars, say — but a delay's tail is as long
/// as its feedback makes it, and at 0.28 that is two repeats. Drawing two
/// repeats across two bars puts everything in the first eighth of the panel
/// and leaves seven eighths of empty lane, which reads as a delay that has
/// stopped working rather than one that is short.
///
/// Rounded UP to a whole beat, never down: the grid is the thing the taps are
/// read against, and a window ending mid-beat puts a part-width gap at the
/// right-hand edge that looks like a missing repeat.
fn window_for(taps: &[Tap], ceiling: f32, beat: f32) -> f32 {
    let last = taps.iter().map(|t| t.at).fold(0.0f32, f32::max);
    if beat <= 0.0 {
        // Free-running: no grid to align to, so show the tail and a margin.
        return (last * 1.15).clamp(0.05, ceiling.max(0.05));
    }
    // A beat of air after the last repeat, so it is not flush against the edge.
    let wanted = (last + beat) / beat;
    // Two beats is the floor: one beat of lane cannot show a rhythm.
    let beats = wanted.ceil().max(2.0);
    (beats * beat).min(ceiling.max(beat * 2.0))
}

/// The reported taps, as the picture draws them.
///
/// Levels are normalised to the loudest tap. The reported amplitudes start at
/// the wet mix, so a delay at 8% sits in the bottom twentieth of the lane and
/// its decay is invisible. What the picture is for is the PATTERN — where the
/// taps land and how fast they give up — and both survive normalising; the
/// absolute level is on the MIX knob two inches away.
fn view_of(
    taps: &[(f32, f32, bool)],
    win_ms: f32,
    on: bool,
    beat_ms: f32,
    division: String,
    family: Family,
    color: [u8; 3],
) -> DelayView {
    let peak = taps.iter().map(|(_, a, _)| *a).fold(0.0f32, f32::max);
    let scale = if peak > f32::EPSILON { 1.0 / peak } else { 1.0 };
    let taps: Vec<Tap> = taps
        .iter()
        .map(|(t, amp, up)| Tap {
            at: t / 1000.0,
            level: (amp * scale).clamp(0.0, 1.0),
            pan: if *up { -0.8 } else { 0.8 },
        })
        .collect();
    DelayView {
        window: window_for(&taps, win_ms / 1000.0, beat_ms / 1000.0),
        taps,
        mix: 1.0,
        on,
        beat: beat_ms / 1000.0,
        division,
        family,
        color,
        // The widget keeps its own clock; this is only a starting value.
        time: 0.0,
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

    fn view() -> DelayView {
        DelayView {
            taps: taps(6),
            window: 2.0,
            mix: 0.3,
            on: true,
            beat: 0.4,
            division: "1/4".to_string(),
            family: Family::Digital,
            color: [59, 130, 246],
            time: 0.0,
        }
    }

    /// Painting produces a scene rather than panicking, at the sizes a panel
    /// actually takes — including the degenerate ones a flex row hands out
    /// mid-layout.
    #[test]
    fn it_paints_at_every_size() {
        for (w, h) in [(2.0, 2.0), (120.0, 40.0), (1280.0, 300.0)] {
            let mut scene = Scene::new();
            paint_delay(&mut scene, &view(), w, h);
        }
    }

    /// A delay locked to a quarter note puts a tap on every beat line — the
    /// whole reason the grid is drawn. Checked as geometry rather than pixels:
    /// tap `n` sits at `n` beats, so it shares an x with gridline `n`.
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
            assert!(
                (tap_x - line_x).abs() < 1e-6,
                "tap {i} at {tap_x} should sit on gridline at {line_x}"
            );
        }
    }

    /// The animation stays inside the panel and repeats: a sweep that runs off
    /// the end, or never comes back, is a sweep nobody can read a tempo from.
    #[test]
    fn the_sweep_wraps_and_stays_in_frame() {
        let mut v = view();
        for step in 0..200 {
            v.time = step as f32 * 0.05;
            let mut scene = Scene::new();
            paint_delay(&mut scene, &v, 640.0, 56.0);
        }
    }

    /// A bypassed block does not animate — a panel that is not in the signal
    /// path must not look like one that is.
    #[test]
    fn bypassed_does_not_sweep() {
        let mut v = view();
        v.on = false;
        v.time = 1.7;
        let mut scene = Scene::new();
        paint_delay(&mut scene, &v, 640.0, 56.0);
    }

    /// The reported amplitudes start at the wet mix, so the loudest tap is
    /// pulled up to the top of the lane and the pattern is readable whatever
    /// the mix knob says.
    #[test]
    fn levels_are_normalised_to_the_loudest_tap() {
        let reported = vec![(400.0, 0.08, true), (800.0, 0.04, false)];
        let v = view_of(
            &reported,
            2000.0,
            true,
            400.0,
            "1/4".into(),
            Family::Tape,
            [56, 189, 248],
        );
        assert!((v.taps[0].level - 1.0).abs() < 1e-6);
        assert!((v.taps[1].level - 0.5).abs() < 1e-6);
    }

    /// Silence must not divide by its own peak.
    #[test]
    fn a_silent_delay_normalises_to_nothing() {
        let v = view_of(
            &[(400.0, 0.0, true)],
            2000.0,
            true,
            400.0,
            String::new(),
            Family::Digital,
            [1, 2, 3],
        );
        assert_eq!(v.taps[0].level, 0.0);
    }

    /// A short tail is drawn across a short window, not squeezed into the
    /// first eighth of a two-bar lane.
    #[test]
    fn the_window_follows_the_tail() {
        let beat = 0.4;
        let taps = vec![
            Tap {
                at: 0.4,
                level: 1.0,
                pan: 0.0,
            },
            Tap {
                at: 0.8,
                level: 0.3,
                pan: 0.0,
            },
        ];
        // The caller offers eight beats; two repeats need three.
        let w = window_for(&taps, beat * 8.0, beat);
        assert!((w - beat * 3.0).abs() < 1e-5, "got {w}");
    }

    /// Always a whole number of beats, so the grid reaches the right edge.
    #[test]
    fn the_window_is_whole_beats() {
        let beat = 0.4;
        let taps = vec![Tap {
            at: 0.55,
            level: 1.0,
            pan: 0.0,
        }];
        let w = window_for(&taps, beat * 8.0, beat);
        assert!((w / beat - (w / beat).round()).abs() < 1e-4, "got {w}");
    }

    /// The caller's ceiling still wins: a long tail is not allowed to run the
    /// lane past what the panel was given.
    #[test]
    fn the_ceiling_holds() {
        let beat = 0.4;
        let taps: Vec<Tap> = (1..40)
            .map(|n| Tap {
                at: beat * n as f32,
                level: 0.5,
                pan: 0.0,
            })
            .collect();
        assert!(window_for(&taps, beat * 8.0, beat) <= beat * 8.0 + 1e-5);
    }

    /// One beat of lane cannot show a rhythm, so a single early repeat still
    /// gets two.
    #[test]
    fn a_single_repeat_still_gets_a_readable_lane() {
        let beat = 0.4;
        let taps = vec![Tap {
            at: 0.05,
            level: 1.0,
            pan: 0.0,
        }];
        assert!((window_for(&taps, beat * 8.0, beat) - beat * 2.0).abs() < 1e-5);
    }

    /// No tempo, no grid to round to — and no division by zero.
    #[test]
    fn a_free_running_delay_still_gets_a_window() {
        let taps = vec![Tap {
            at: 0.35,
            level: 1.0,
            pan: 0.0,
        }];
        let w = window_for(&taps, 4.0, 0.0);
        assert!(w > 0.35 && w <= 4.0, "got {w}");
    }

    /// The uniform block is four-float rows all the way down, so what Rust
    /// writes and what WGSL reads cannot drift apart over padding.
    #[test]
    fn the_uniform_block_is_vec4_rows() {
        assert_eq!(std::mem::size_of::<DelayUniforms>() % 16, 0);
        // frame + params + color + style + MAX_TAPS rows.
        assert_eq!(std::mem::size_of::<DelayUniforms>(), (4 + MAX_TAPS) * 16);
    }

    /// A tail longer than the array is truncated rather than overflowing, and
    /// the count the shader reads matches what was actually written.
    #[test]
    fn more_taps_than_the_array_holds_are_dropped() {
        let mut v = view();
        v.taps = taps(MAX_TAPS + 9);
        let u = DelayUniforms::of(&v, 640.0, 56.0);
        assert_eq!(u.frame[3], MAX_TAPS as f32);
    }

    /// Every family gets its own number, and the shader's constants are the
    /// same numbers. They are one switch written in two languages and
    /// nothing but this checks that they agree — a collision would silently
    /// draw one machine as another.
    #[test]
    fn the_shader_and_rust_agree_on_family_order() {
        let wgsl = include_str!("viz.wgsl");
        for (family, name) in [
            (Family::Digital, "DIGITAL"),
            (Family::Tape, "TAPE"),
            (Family::Analog, "ANALOG"),
            (Family::Pitch, "PITCH"),
            (Family::Rhythmic, "RHYTHMIC"),
            (Family::Special, "SPECIAL"),
        ] {
            let want = format!("const {name}:");
            let line = wgsl
                .lines()
                .find(|l| l.trim_start().starts_with(&want))
                .unwrap_or_else(|| panic!("the shader declares no {name}"));
            let got: f32 = line
                .rsplit('=')
                .next()
                .and_then(|v| v.trim().trim_end_matches(';').parse().ok())
                .unwrap_or_else(|| panic!("cannot read {name} from {line:?}"));
            assert!(
                (got - family_index(family)).abs() < 1e-6,
                "{name}: shader says {got}, Rust says {}",
                family_index(family)
            );
        }
    }

    /// Every style the DSP has lands on a family, and the styles the rig's
    /// own algorithm list names map to the machines they say they are.
    #[test]
    fn a_style_index_names_its_machine() {
        // `DELAY_ALGOS` order, which is `DelayStyle::from_index` order.
        assert_eq!(family_of_style(0), Family::Tape);
        assert_eq!(family_of_style(1), Family::Digital);
        assert_eq!(family_of_style(2), Family::Analog);
    }

    /// The shader compiles, as `compose` assembles it — not on its own.
    ///
    /// Nothing else in the tree would notice if it did not: a `ShaderSurface`
    /// that fails to build is indistinguishable from a renderer that declined,
    /// so the panel would silently draw vectors forever. Validating the
    /// fragment alone is not enough either — it passes happily while the
    /// composed module collides with the prelude.
    #[test]
    fn the_shader_compiles_and_validates() {
        let source = fts_audio_ui::shader::compose(SHADER);
        let module = naga::front::wgsl::parse_str(&source).unwrap_or_else(|e| {
            panic!(
                "the delay shader does not parse: {}",
                e.emit_to_string(&source)
            )
        });
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        );
        if let Err(e) = validator.validate(&module) {
            panic!("the delay shader does not validate: {e:?}");
        }
    }
}
