//! What the reverb is actually doing, painted.
//!
//! A reverb's character is its decay: how long before anything arrives, how
//! densely the early reflections pack together, and how long the tail takes to
//! fall away. `TIME 1.6s · DAMP 0.30` says none of that, and the difference
//! between a room and a hall is exactly the part a number leaves out.
//!
//! The tail is drawn against the beat grid, so "two bars of reverb" is a thing
//! the picture can say rather than a sum the player has to do.
//!
//! See [`delay_ui::viz`](../../delay/delay-ui/src/viz.rs) for the two-painter
//! arrangement these share: a vector fallback that any anyrender backend can
//! replay, and a WGSL path taken when the renderer hands over a device.

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

/// Which machine is making the space.
///
/// Re-exported rather than redefined: [`reverb_dsp::algorithm::Family`] already says a
/// reverb's family "is not a preset — it is a different machine, and anything
/// that draws one should say which before it says anything else". This is the
/// drawing half of that sentence.
pub use reverb_dsp::algorithm::Family;

/// The family of the algorithm at `index` in the editor's algorithm list.
///
/// The host has an algorithm index and no business knowing the algorithm
/// table; the effect that owns the table owns the mapping.
#[must_use]
pub fn family_of_algorithm(index: u32) -> Family {
    reverb_dsp::algorithm::AlgorithmType::ALL
        .get(index as usize)
        .map_or(Family::Hall, |a| a.family())
}

/// The family, as the shader's `u.style.x`. The order is the shader's
/// constants; the two must agree, so they are written next to each other.
#[must_use]
fn family_index(family: Family) -> f32 {
    match family {
        Family::Room => 0.0,
        Family::Hall => 1.0,
        Family::Plate => 2.0,
        Family::Spring => 3.0,
        Family::Ambient => 4.0,
        Family::Random => 5.0,
        Family::Special => 6.0,
        Family::Convolution => 7.0,
    }
}

/// What the reverb panel draws.
#[derive(Clone, Debug, PartialEq)]
pub struct ReverbView {
    /// RT60 in seconds — the tail's length.
    pub decay: f32,
    /// 0..=1: how quickly early reflections thicken into a wash.
    pub density: f32,
    /// 0..=1: how much faster the top of the spectrum dies than the bottom.
    ///
    /// The single thing that separates one room from another at the same
    /// RT60, and the one a bare envelope cannot say: a damped hall and a
    /// bright one draw the identical wedge. See the shader's two-band tail.
    pub damp: f32,
    /// Pre-delay in seconds, before anything arrives.
    pub predelay: f32,
    pub mix: f32,
    pub on: bool,
    /// Seconds per beat — the tail is measured against the tempo, so "two bars
    /// of reverb" is a thing the picture can say.
    pub beat: f32,
    /// Which machine is making the space. A plate and a spring at identical
    /// decay draw the same wedge and sound nothing alike; this is what lets
    /// the picture say which it is.
    pub family: Family,
    /// The lane's own colour. Reverb is purple-led; delay is blue.
    pub color: [u8; 3],
    /// Seconds since the panel appeared — the animation's clock, kept by the
    /// widget rather than pushed in, so it advances on every repaint.
    pub time: f32,
}

impl Default for ReverbView {
    /// An empty hall. `Family` has no `Default` of its own — and should not:
    /// there is no neutral machine in the DSP, only a neutral PICTURE, and a
    /// hall is it because it is the family that adds least to a plain decay.
    fn default() -> Self {
        Self {
            decay: 0.0,
            density: 0.0,
            predelay: 0.0,
            mix: 0.0,
            damp: 0.0,
            on: false,
            beat: 0.0,
            family: Family::Hall,
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

/// The uniform block, laid out to match `Reverb` in `viz.wgsl`.
///
/// Every row is a `vec4`, so there is no padding for the two languages to
/// disagree about.
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ReverbUniforms {
    /// `[width, height, seconds, on]`.
    pub frame: [f32; 4],
    /// `[decay_s, density, predelay_s, mix]`.
    pub params: [f32; 4],
    /// `[beat_s, window_s, damp, _]`.
    pub time: [f32; 4],
    /// The lane's colour; `w` unused.
    pub color: [f32; 4],
    /// `[family, _, _, _]`.
    pub style: [f32; 4],
}

impl ReverbUniforms {
    /// Fill from a view.
    #[must_use]
    pub fn of(view: &ReverbView, w: f32, h: f32) -> Self {
        let [r, g, b] = view.color;
        Self {
            frame: [w, h, view.time, if view.on { 1.0 } else { 0.0 }],
            params: [view.decay, view.density, view.predelay, view.mix],
            time: [
                view.beat.max(1e-3),
                window_of(view),
                view.damp.clamp(0.0, 1.0),
                0.0,
            ],
            color: [
                f32::from(r) / 255.0,
                f32::from(g) / 255.0,
                f32::from(b) / 255.0,
                1.0,
            ],
            style: [family_index(view.family), 0.0, 0.0, 0.0],
        }
    }
}

/// The seconds the panel shows.
///
/// The tail plus its pre-delay, with a floor: a reverb switched to nothing
/// still needs an axis, or the grid divides by zero and the picture is a
/// blank panel that looks broken rather than empty.
#[must_use]
pub fn window_of(view: &ReverbView) -> f32 {
    (view.predelay + view.decay).max(0.05)
}

/// The reverb's decay, painted.
pub struct ReverbWidget {
    view: Shared<ReverbView>,
    /// Built once, from whatever `can_create_surfaces` hands over. `None` on a
    /// renderer with no device to give, which is not an error: the vector
    /// painter below draws the same decay.
    gpu: Option<ShaderSurface>,
    uniforms: ReverbUniforms,
    born: web_time::Instant,
}

impl ReverbWidget {
    #[must_use]
    pub fn new(view: Shared<ReverbView>) -> Self {
        Self {
            view,
            gpu: None,
            uniforms: ReverbUniforms::default(),
            born: web_time::Instant::now(),
        }
    }
}

impl ReverbWidget {
    /// Take a GPU device and build the shader surface — the
    /// host-agnostic half of `Widget::can_create_surfaces`.
    pub fn create_surfaces(&mut self, render_ctx: &mut dyn RenderContext) {
        self.gpu = render_ctx.renderer_specific_context().and_then(|ctx| {
            ShaderSurface::with_uniform_size(
                ctx,
                SHADER,
                std::mem::size_of::<ReverbUniforms>() as u64,
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

        if self.gpu.is_some() {
            self.uniforms = ReverbUniforms::of(&view, w as f32, h as f32);
            let bytes = bytemuck::bytes_of(&self.uniforms);
            let drew = self
                .gpu
                .as_mut()
                .is_some_and(|gpu| gpu.draw_raw(render_ctx, &mut scene, width, height, bytes));
            if drew {
                // Geometry on purpose — see the delay's note: a 1 px ruled
                // line is the one thing the vector pass does better, and the
                // grid is what the tail's length is read against.
                lane::beats(
                    &mut scene,
                    w,
                    h,
                    f64::from(window_of(&view)),
                    f64::from(view.beat),
                    view.on,
                );
                return scene;
            }
        }

        paint_reverb(&mut scene, &view, w, h);
        scene
    }
}

/// Blitz's custom widget, natively.
#[cfg(not(target_arch = "wasm32"))]
impl Widget for ReverbWidget {
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
impl fts_audio_ui::scene_canvas::CanvasPanel for ReverbWidget {
    fn can_create_surfaces(&mut self, ctx: &mut dyn RenderContext) {
        self.create_surfaces(ctx);
    }

    fn paint(&mut self, ctx: &mut dyn RenderContext, width: u32, height: u32, scale: f64) -> Scene {
        self.paint_frame(ctx, width, height, scale)
    }
}

// ── The fallback painter ────────────────────────────────────────────────────

/// The reverb's decay: pre-delay, early reflections, then the tail.
pub fn paint_reverb(scene: &mut Scene, view: &ReverbView, w: f64, h: f64) {
    let (signal, accent) = lane::palette(view.on, view.color);
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
            ColorStop::from((0.0, faded(signal, 0.12))),
            ColorStop::from((1.0, Color::from_rgba8(0, 0, 0, 0))),
        ]),
        None,
        &Rect::new(0.0, 0.0, w, h),
    );

    lane::beats(scene, w, h, window, f64::from(view.beat), view.on);

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
            ColorStop::from((0.0, faded(signal, 0.06))),
            ColorStop::from((1.0, faded(signal, 0.46))),
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
            faded(accent, (0.5 * (1.0 - t as f32).max(0.06))),
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
                        ColorStop::from((0.0, faded(signal, 0.0))),
                        ColorStop::from((1.0, faded(signal, 0.26 * lit))),
                    ]),
                None,
                &Rect::new((hx - w * 0.10).max(0.0), top, hx, h),
            );
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                faded(accent, 0.55 * lit),
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
            faded(signal, 0.07),
            None,
            &Rect::new(0.0, 0.0, pre, h),
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
/// thread here is not a slow path in wasm32, it is a panic.)
#[cfg(target_arch = "wasm32")]
pub fn use_repaint_clock(_active: bool) {}

/// A reverb, painted by [`ReverbWidget`].
///
/// A component per lane because `CustomWidgetAttr` is write-once — see the
/// delay's note.
#[component]
pub fn ReverbViz(
    decay: f32,
    density: f32,
    predelay: f32,
    mix: f32,
    /// 0..=1 — how much faster the highs die than the lows.
    #[props(default = 0.0)]
    damp: f32,
    /// Which machine is making the space. See [`family_of_algorithm`].
    #[props(default = Family::Hall)]
    family: Family,
    on: bool,
    beat_ms: f32,
    color: [u8; 3],
) -> Element {
    use_repaint_clock(on);
    let view: Shared<ReverbView> = use_hook(|| Rc::new(RefCell::new(ReverbView::default())));
    // Marked each render: the view below is new, and only the widget
    // knows to draw it (the DOM does not change).
    #[cfg(not(target_arch = "wasm32"))]
    let fresh = use_hook(fts_audio_ui::widget::Fresh::default);
    #[cfg(not(target_arch = "wasm32"))]
    let attr =
        use_hook(|| dioxus_native_dom::CustomWidgetAttr::new(fts_audio_ui::widget::Rasterized::new(fts_audio_ui::widget::Freshened::new(ReverbWidget::new(Rc::clone(&view)), fresh.clone()))));

    *view.borrow_mut() = ReverbView {
        decay,
        density,
        predelay,
        mix,
        damp,
        family,
        on,
        beat: beat_ms / 1000.0,
        color,
        // The widget keeps its own clock; this is only a starting value.
        time: 0.0,
    };
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
        let panel = use_hook(|| {
            fts_audio_ui::scene_canvas::Panel::new(ReverbWidget::new(Rc::clone(&view)))
        });
        rsx! {
            fts_audio_ui::scene_canvas::SceneCanvas {
                panel,
                class: "absolute inset-0 w-full h-full",
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> ReverbView {
        ReverbView {
            decay: 2.4,
            density: 0.6,
            predelay: 0.02,
            mix: 0.3,
            damp: 0.4,
            family: Family::Hall,
            on: true,
            beat: 0.4,
            color: [167, 139, 250],
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
            paint_reverb(&mut scene, &view(), w, h);
        }
    }

    /// The animation repeats and stays inside the panel.
    #[test]
    fn the_tail_stays_in_frame() {
        let mut v = view();
        for step in 0..200 {
            v.time = step as f32 * 0.05;
            let mut scene = Scene::new();
            paint_reverb(&mut scene, &v, 640.0, 56.0);
        }
    }

    /// A reverb with nothing set still has a window, so the envelope maths
    /// cannot divide by zero or run off the panel.
    #[test]
    fn a_zero_reverb_still_has_a_window() {
        let v = ReverbView::default();
        assert!(window_of(&v) > 0.0);
        let mut scene = Scene::new();
        paint_reverb(&mut scene, &v, 200.0, 80.0);
    }

    /// The window is the tail plus what comes before it. A pre-delay drawn
    /// outside the window would put the first reflection off the panel, which
    /// is the one arrival that has to be visible.
    #[test]
    fn the_window_holds_the_predelay_and_the_tail() {
        let mut v = view();
        v.predelay = 0.25;
        v.decay = 3.0;
        assert!((window_of(&v) - 3.25).abs() < 1e-6);
    }

    /// The uniform block is four-float rows all the way down, so what Rust
    /// writes and what WGSL reads cannot drift apart over padding.
    #[test]
    fn the_uniform_block_is_vec4_rows() {
        assert_eq!(std::mem::size_of::<ReverbUniforms>() % 16, 0);
        // frame + params + time + color + style.
        assert_eq!(std::mem::size_of::<ReverbUniforms>(), 5 * 16);
    }

    /// Bypassed reaches the shader as a flag rather than as absent data: the
    /// shape still has to draw, unlit.
    #[test]
    fn bypassed_keeps_its_numbers() {
        let mut v = view();
        v.on = false;
        let u = ReverbUniforms::of(&v, 640.0, 56.0);
        assert_eq!(u.frame[3], 0.0);
        assert!((u.params[0] - 2.4).abs() < 1e-6, "the decay is still there");
    }

    /// Every family gets its own number, and the shader's constants are the
    /// same numbers. One switch, two languages — a collision would silently
    /// draw a spring as a plate.
    #[test]
    fn the_shader_and_rust_agree_on_family_order() {
        let wgsl = include_str!("viz.wgsl");
        for (family, name) in [
            (Family::Room, "ROOM"),
            (Family::Hall, "HALL"),
            (Family::Plate, "PLATE"),
            (Family::Spring, "SPRING"),
            (Family::Ambient, "AMBIENT"),
            (Family::Random, "RANDOM"),
            (Family::Special, "SPECIAL"),
            (Family::Convolution, "CONVOLUTION"),
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

    /// The rig hands over an index into the editor's algorithm list; it has
    /// to land on the machine that list names.
    #[test]
    fn an_algorithm_index_names_its_machine() {
        assert_eq!(family_of_algorithm(0), Family::Room);
        assert_eq!(family_of_algorithm(1), Family::Hall);
        assert_eq!(family_of_algorithm(2), Family::Plate);
        assert_eq!(family_of_algorithm(3), Family::Spring);
        // Past the end is a hall rather than a panic: an index from the wire
        // is not something the picture should die on.
        assert_eq!(family_of_algorithm(9999), Family::Hall);
    }

    /// The shader compiles, as `compose` assembles it — see the delay's note
    /// for why validating the fragment alone is not enough.
    #[test]
    fn the_shader_compiles_and_validates() {
        let source = fts_audio_ui::shader::compose(SHADER);
        let module = naga::front::wgsl::parse_str(&source).unwrap_or_else(|e| {
            panic!(
                "the reverb shader does not parse: {}",
                e.emit_to_string(&source)
            )
        });
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        );
        if let Err(e) = validator.validate(&module) {
            panic!("the reverb shader does not validate: {e:?}");
        }
    }
}
