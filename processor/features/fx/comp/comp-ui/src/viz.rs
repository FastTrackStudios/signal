//! What the compressor is actually doing, painted — data in, nothing else.
//!
//! # Why this is a separate component from [`CompGraph`]
//!
//! [`CompGraph`](crate::comp_graph::CompGraph) is the plugin's editor: it
//! reads `SharedState` and the editor's `ParamContext` out of context, edits
//! parameters through `ParamPtr`, and owns the drag gestures. All of that is
//! correct inside the plugin and impossible outside it — a rig remote has no
//! parameter tree, and a channel strip embedding one compressor among twenty
//! has no business inheriting the plugin's context.
//!
//! So the *picture* is split from the *editing*, the way `eq-ui` already
//! splits `EqGraph` (bands in, band events out) from the plugin that binds it
//! to parameters. [`CompView`] is the whole of what the picture needs, and
//! [`CompViz`] takes it as props. The plugin keeps its gestures and hands the
//! same numbers down; anything else — the rig, a strip, a detached surface —
//! mounts this directly.
//!
//! The transfer maths is NOT re-derived here: it is
//! [`compress_transfer`](crate::comp_graph_svg::compress_transfer), the same
//! function the SVG path and the plugin's curve use. A second copy that drew
//! a slightly different knee would be worse than no picture, because it would
//! be believed.

use std::cell::RefCell;
use std::rc::Rc;

use anyrender::{PaintScene, RenderContext, Scene};
#[cfg(not(target_arch = "wasm32"))]
use blitz_dom::node::{ComputedStyles, Widget};
use dioxus::prelude::*;
use fts_audio_ui::paint::lane;
use fts_audio_ui::shader::ShaderSurface;

use vello::kurbo::{Affine, BezPath, Circle, Line, Point, Rect, Stroke};
use vello::peniko::{Color, Fill};

use crate::comp_graph_svg::{RANGE_DB, compress_transfer, db_to_y};

/// How many trace samples the shader carries, per trace.
///
/// A uniform array is fixed-length. 128 is a little over four seconds at the
/// rate the rig reports, which is as far back as a player reads a trace.
pub const TRACE_LEN: usize = 128;

/// What the compressor panel draws.
///
/// Levels are the display's, not the DSP's: `input`/`gr` are 0..=1 across the
/// panel's own range, already scaled by whoever owns the meter. The curve is
/// in dB, because the curve is the parameter.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompView {
    /// Threshold, dBFS (negative).
    pub threshold: f32,
    /// Ratio, ≥ 1.
    pub ratio: f32,
    /// Knee width, dB.
    pub knee: f32,
    /// Live input level, dBFS — where the ball sits on the curve.
    pub in_db: f32,
    /// Detector gain reduction, dB positive.
    pub gr_db: f32,
    /// Rolling input trace, 0..=1, oldest → newest.
    pub input: Vec<f32>,
    /// Rolling gain-reduction trace, 0..=1, oldest → newest.
    pub gr: Vec<f32>,
    pub on: bool,
    /// The pointer is within reach of the threshold line.
    ///
    /// A control you can grab should look like one before you try to. The
    /// threshold is the only thing on this panel that is draggable and it
    /// looks exactly like the rules that are not, so without this the only
    /// way to find it is to press and see whether anything moved.
    pub grabbable: bool,
    /// The lane's colour. The compressor's input is drawn white-grey — it is
    /// the signal ITSELF rather than an effect's contribution, and giving it
    /// a hue would make it look like one more coloured block in the rack.
    pub color: [u8; 3],
    /// Seconds since the panel appeared — the animation's clock, kept by the
    /// widget rather than pushed in, so it advances on every repaint.
    pub time: f32,
}

/// The numbers a widget reads, written by the component that owns it.
pub type Shared<T> = Rc<RefCell<T>>;

/// The widget's own box, in the units a pointer arrives in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CompMetrics {
    /// Physical pixels.
    pub width: f32,
    pub height: f32,
    /// Device pixel ratio, so a host can get back to CSS pixels.
    pub scale: f32,
}

impl CompMetrics {
    /// The panel's height in CSS pixels — the units `element_coordinates()`
    /// reports in. Zero before the first paint, which a caller must treat as
    /// "not measured yet" rather than as a real height.
    #[must_use]
    pub fn css_height(self) -> f64 {
        if self.scale <= 0.0 {
            return 0.0;
        }
        f64::from(self.height) / f64::from(self.scale)
    }
}

/// A handle the widget writes its own box into, so a host can map a pointer
/// into graph space WITHOUT measuring the element.
///
/// This exists because measuring is asynchronous and therefore wrong. A host
/// that calls `get_client_rect().await` on pointer-down does not know whether
/// the press hit anything until the await resolves — so the gesture starts a
/// frame or more late, the first movement is dropped, and the cached rect is
/// stale for the rest of the drag. `eq_graph` hit exactly this and says so in
/// its own source: the offsets "were often stale (`get_client_rect` is async)
/// which made hit-tests miss entirely".
///
/// The widget already knows its box — Blitz hands it one every paint — so it
/// publishes it here and the host reads it synchronously.
///
/// A `Cell`, not a `RefCell`: this is read from event handlers and written
/// from Blitz's paint traversal, and a `Copy` payload behind a `Cell` cannot
/// be caught mid-borrow by either.
#[derive(Clone, Default)]
pub struct MetricsHandle(Rc<std::cell::Cell<CompMetrics>>);

impl MetricsHandle {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// What the widget last painted into. All zeroes before the first paint.
    #[must_use]
    pub fn get(&self) -> CompMetrics {
        self.0.get()
    }

    fn set(&self, m: CompMetrics) {
        self.0.set(m);
    }

    /// Publish a box measured by a host that paints this picture itself —
    /// the browser's canvas surface, which has no `Widget::paint` to do it.
    pub fn publish(&self, width: f32, height: f32, scale: f32) {
        self.set(CompMetrics {
            width,
            height,
            scale,
        });
    }
}

impl From<CompMetrics> for MetricsHandle {
    /// A handle that already holds a box, for a host's tests: the pointer
    /// maths is worth testing without standing a renderer up to paint one.
    fn from(m: CompMetrics) -> Self {
        Self(Rc::new(std::cell::Cell::new(m)))
    }
}

impl PartialEq for MetricsHandle {
    /// By identity. Comparing the contents would make a prop that changes
    /// every frame — the box is written on every paint — and re-render the
    /// panel for a number nothing in the DOM draws.
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for MetricsHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MetricsHandle").field(&self.get()).finish()
    }
}

/// The WGSL the shader path runs.
///
/// Public so the panel sheet can render the real thing rather than a copy of
/// it — see `examples/comp_sheet.rs`.
pub const SHADER: &str = include_str!("viz.wgsl");

/// The uniform block, laid out to match `Comp` in `viz.wgsl`.
///
/// Every row is a `vec4`, so there is no padding for the two languages to
/// disagree about. The two traces are packed four samples to a row.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CompUniforms {
    /// `[width, height, seconds, on]`.
    pub frame: [f32; 4],
    /// `[threshold_db, ratio, knee_db, range_db]`.
    pub curve: [f32; 4],
    /// `[in_db, gr_db, trace_len, grabbable]`.
    pub meter: [f32; 4],
    /// The lane's colour; `w` unused.
    pub color: [f32; 4],
    /// Input trace, four samples per row.
    pub input: [[f32; 4]; TRACE_LEN / 4],
    /// Gain-reduction trace, four samples per row.
    pub gr: [[f32; 4]; TRACE_LEN / 4],
}

impl Default for CompUniforms {
    fn default() -> Self {
        Self {
            frame: [0.0; 4],
            curve: [0.0; 4],
            meter: [0.0; 4],
            color: [0.0; 4],
            input: [[0.0; 4]; TRACE_LEN / 4],
            gr: [[0.0; 4]; TRACE_LEN / 4],
        }
    }
}

/// Resample a trace of any length into the fixed array, newest at the right.
///
/// Resampling here rather than in WGSL keeps the shader's indexing trivial and
/// means a change of telemetry rate costs nothing on the GPU.
fn pack_trace(out: &mut [[f32; 4]; TRACE_LEN / 4], src: &[f32]) {
    if src.is_empty() {
        *out = [[0.0; 4]; TRACE_LEN / 4];
        return;
    }
    let last = (src.len() - 1) as f32;
    for i in 0..TRACE_LEN {
        let pos = i as f32 / (TRACE_LEN - 1) as f32 * last;
        let lo = pos.floor() as usize;
        let hi = (lo + 1).min(src.len() - 1);
        let f = pos - lo as f32;
        out[i / 4][i % 4] = src[lo].mul_add(1.0 - f, src[hi] * f);
    }
}

impl CompUniforms {
    /// Fill from a view.
    #[must_use]
    pub fn of(view: &CompView, w: f32, h: f32) -> Self {
        let mut u = Self::default();
        let [r, g, b] = view.color;
        u.frame = [w, h, view.time, if view.on { 1.0 } else { 0.0 }];
        u.curve = [view.threshold, view.ratio.max(1.0), view.knee, RANGE_DB];
        u.meter = [
            view.in_db,
            view.gr_db,
            if view.input.is_empty() {
                0.0
            } else {
                TRACE_LEN as f32
            },
            if view.grabbable { 1.0 } else { 0.0 },
        ];
        u.color = [
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            1.0,
        ];
        pack_trace(&mut u.input, &view.input);
        pack_trace(&mut u.gr, &view.gr);
        u
    }
}

/// The compressor's curve and traces, painted.
pub struct CompWidget {
    view: Shared<CompView>,
    /// Where this widget publishes its own box. See [`MetricsHandle`].
    metrics: MetricsHandle,
    /// Built once, from whatever `can_create_surfaces` hands over. `None` on a
    /// renderer with no device to give, which is not an error: the vector
    /// painter below draws the same picture.
    gpu: Option<ShaderSurface>,
    uniforms: CompUniforms,
    born: web_time::Instant,
}

impl CompWidget {
    #[must_use]
    pub fn new(view: Shared<CompView>, metrics: MetricsHandle) -> Self {
        Self {
            view,
            metrics,
            gpu: None,
            uniforms: CompUniforms::default(),
            born: web_time::Instant::now(),
        }
    }
}

impl CompWidget {
    /// Take a GPU device and build the shader surface — the host-agnostic
    /// half of `Widget::can_create_surfaces`, so a browser canvas can make
    /// the same call (`fts_audio_ui::scene_canvas`).
    pub fn create_surfaces(&mut self, render_ctx: &mut dyn RenderContext) {
        self.gpu = render_ctx.renderer_specific_context().and_then(|ctx| {
            ShaderSurface::with_uniform_size(
                ctx,
                SHADER,
                std::mem::size_of::<CompUniforms>() as u64,
            )
        });
    }

    /// Record a frame — the host-agnostic half of `Widget::paint`.
    pub fn paint_frame(
        &mut self,
        render_ctx: &mut dyn RenderContext,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Scene {
        // Published before the early-out: a host mapping a pointer needs the
        // box even on a frame this widget declines to draw.
        self.metrics.set(CompMetrics {
            width: width as f32,
            height: height as f32,
            scale: scale.max(1.0) as f32,
        });
        let mut scene = Scene::new();
        let (w, h) = (f64::from(width), f64::from(height));
        if w < 2.0 || h < 2.0 {
            return scene;
        }
        let mut view = self.view.borrow().clone();
        view.time = self.born.elapsed().as_secs_f32();

        if self.gpu.is_some() {
            self.uniforms = CompUniforms::of(&view, w as f32, h as f32);
            let bytes = bytemuck::bytes_of(&self.uniforms);
            let drew = self
                .gpu
                .as_mut()
                .is_some_and(|gpu| gpu.draw_raw(render_ctx, &mut scene, width, height, bytes));
            if drew {
                // The threshold line stays geometry: it is the thing being
                // dragged, so it has to sit exactly where the pointer maths
                // says it does, and a 1 px rule is what the vector pass is
                // best at.
                paint_threshold(&mut scene, &view, w, h);
                return scene;
            }
        }

        paint_comp(&mut scene, &view, w, h);
        scene
    }
}

/// Blitz's custom widget, natively.
#[cfg(not(target_arch = "wasm32"))]
impl Widget for CompWidget {
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
impl fts_audio_ui::scene_canvas::CanvasPanel for CompWidget {
    fn can_create_surfaces(&mut self, ctx: &mut dyn RenderContext) {
        self.create_surfaces(ctx);
    }

    fn paint(&mut self, ctx: &mut dyn RenderContext, width: u32, height: u32, scale: f64) -> Scene {
        self.paint_frame(ctx, width, height, scale)
    }
}

// ── The fallback painter ────────────────────────────────────────────────────

/// The threshold, as a dashed rule across the panel.
///
/// Drawn in both paths: under the shader it is the only geometry, and in the
/// fallback it is one layer of several.
fn paint_threshold(scene: &mut Scene, view: &CompView, w: f64, h: f64) {
    let (signal, _) = lane::palette(view.on, view.color);
    let y = db_to_y(f64::from(view.threshold), h);
    let (width, alpha) = if view.grabbable {
        (2.0, 0.95)
    } else {
        (1.0, 0.55)
    };
    scene.stroke(
        &Stroke::new(width),
        Affine::IDENTITY,
        lane::faded(signal, alpha),
        None,
        &Line::new(Point::new(0.0, y), Point::new(w, y)),
    );
}

/// The transfer curve, the traces and the ball — everything, in vectors.
pub fn paint_comp(scene: &mut Scene, view: &CompView, w: f64, h: f64) {
    let (signal, accent) = lane::palette(view.on, view.color);

    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(8, 8, 8),
        None,
        &Rect::new(0.0, 0.0, w, h),
    );

    // The input trace, filled from the floor: the signal arriving.
    if view.input.len() > 1 {
        let mut path = BezPath::new();
        path.move_to((0.0, h));
        for (i, v) in view.input.iter().enumerate() {
            let x = i as f64 / (view.input.len() - 1) as f64 * w;
            path.line_to((x, h - f64::from(*v).clamp(0.0, 1.0) * h));
        }
        path.line_to((w, h));
        path.close_path();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            lane::faded(signal, 0.22),
            None,
            &path,
        );
    }

    // The gain reduction, hanging from the ceiling: what was taken away.
    if view.gr.len() > 1 {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        for (i, v) in view.gr.iter().enumerate() {
            let x = i as f64 / (view.gr.len() - 1) as f64 * w;
            path.line_to((x, f64::from(*v).clamp(0.0, 1.0) * h));
        }
        path.line_to((w, 0.0));
        path.close_path();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            lane::faded(Color::from_rgb8(239, 68, 68), 0.30),
            None,
            &path,
        );
    }

    paint_threshold(scene, view, w, h);

    // The transfer curve itself, sampled through the shared function.
    let mut curve = BezPath::new();
    for i in 0..=96 {
        let t = f64::from(i) / 96.0;
        let in_db = (t - 1.0) * f64::from(RANGE_DB);
        let out_db = f64::from(compress_transfer(
            in_db as f32,
            view.threshold,
            view.ratio.max(1.0),
            view.knee,
        ));
        let x = t * w;
        let y = db_to_y(out_db, h);
        if i == 0 {
            curve.move_to((x, y));
        } else {
            curve.line_to((x, y));
        }
    }
    scene.stroke(
        &Stroke::new(1.5),
        Affine::IDENTITY,
        lane::faded(accent, 0.9),
        None,
        &curve,
    );

    // The ball: where the signal is sitting on its own curve right now.
    if view.on {
        let t = (f64::from(view.in_db) / f64::from(RANGE_DB) + 1.0).clamp(0.0, 1.0);
        let out_db = f64::from(compress_transfer(
            view.in_db,
            view.threshold,
            view.ratio.max(1.0),
            view.knee,
        ));
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            lane::faded(accent, 0.95),
            None,
            &Circle::new(Point::new(t * w, db_to_y(out_db, h)), 3.0),
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

/// The compressor's picture, as props. No context, no parameter tree.
///
/// A component per panel because `CustomWidgetAttr` is write-once: the widget
/// is built in a hook that runs once, and every later render pushes numbers
/// through the shared cell instead of rebuilding it. Rebuilding would hand
/// Blitz a second widget for the same node and lose the first one's state.
#[component]
pub fn CompViz(
    threshold: f32,
    ratio: f32,
    knee: f32,
    in_db: f32,
    gr_db: f32,
    /// Rolling traces `(input, gain reduction)`, each 0..=1, oldest → newest.
    wave: (Vec<f32>, Vec<f32>),
    on: bool,
    /// The pointer is within reach of the threshold line. See
    /// [`CompView::grabbable`].
    #[props(default = false)]
    grabbable: bool,
    /// Where the widget publishes its own box, for a host that maps pointer
    /// events into graph space. See [`MetricsHandle`].
    #[props(default)]
    metrics: MetricsHandle,
    color: [u8; 3],
) -> Element {
    use_repaint_clock(on);
    let view: Shared<CompView> = use_hook(|| Rc::new(RefCell::new(CompView::default())));
    // Marked each render: the view below is new, and only the widget
    // knows to draw it (the DOM does not change).
    #[cfg(not(target_arch = "wasm32"))]
    let fresh = use_hook(fts_audio_ui::widget::Fresh::default);
    #[cfg(not(target_arch = "wasm32"))]
    let attr = use_hook({
        let metrics = metrics.clone();
        let view = Rc::clone(&view);
        let fresh = fresh.clone();
        move || dioxus_native_dom::CustomWidgetAttr::new(fts_audio_ui::widget::Rasterized::new(fts_audio_ui::widget::Freshened::new(CompWidget::new(view, metrics), fresh)))
    });

    let (input, gr) = wave;
    *view.borrow_mut() = CompView {
        threshold,
        ratio,
        knee,
        in_db,
        gr_db,
        input,
        gr,
        on,
        grabbable,
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
        // picture — not the vector fallback. It publishes its own box as it
        // paints, so the pointer maths is the same in both hosts.
        let panel = use_hook(|| {
            fts_audio_ui::scene_canvas::Panel::new(CompWidget::new(
                Rc::clone(&view),
                metrics.clone(),
            ))
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

    fn view() -> CompView {
        CompView {
            threshold: -18.0,
            ratio: 4.0,
            knee: 6.0,
            in_db: -12.0,
            gr_db: 3.0,
            input: (0..40).map(|i| i as f32 / 40.0).collect(),
            gr: (0..40).map(|i| i as f32 / 80.0).collect(),
            on: true,
            grabbable: false,
            color: [228, 228, 231],
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
            paint_comp(&mut scene, &view(), w, h);
        }
    }

    /// An empty panel has no telemetry yet. It must draw the curve anyway —
    /// the curve is the parameter, and it is legible before a note is played.
    #[test]
    fn it_paints_with_no_telemetry() {
        let mut v = view();
        v.input.clear();
        v.gr.clear();
        let mut scene = Scene::new();
        paint_comp(&mut scene, &v, 320.0, 200.0);
    }

    /// The picture uses the editor's own transfer function rather than its own
    /// idea of a knee. A second copy that disagreed would be believed.
    #[test]
    fn the_curve_is_the_editors_curve() {
        let v = view();
        // Well below the knee, the compressor is not doing anything.
        let below = compress_transfer(-50.0, v.threshold, v.ratio, v.knee);
        assert!((below - -50.0).abs() < 1e-3);
        // Well above it, the slope is the ratio.
        let a = compress_transfer(-6.0, v.threshold, v.ratio, v.knee);
        let b = compress_transfer(-2.0, v.threshold, v.ratio, v.knee);
        assert!(((b - a) - 4.0 / v.ratio).abs() < 1e-2, "slope is 1/ratio");
    }

    /// A trace of any length lands in the fixed array, ends preserved, so the
    /// panel spans the whole window whatever the telemetry rate is.
    #[test]
    fn a_trace_resamples_to_the_array() {
        let mut out = [[0.0f32; 4]; TRACE_LEN / 4];
        let src: Vec<f32> = (0..500).map(|i| i as f32 / 499.0).collect();
        pack_trace(&mut out, &src);
        assert!((out[0][0] - 0.0).abs() < 1e-3);
        let last = TRACE_LEN - 1;
        assert!((out[last / 4][last % 4] - 1.0).abs() < 1e-3);
    }

    /// An empty trace is zeros, not a panic or a read past the end.
    #[test]
    fn an_empty_trace_is_silence() {
        let mut out = [[1.0f32; 4]; TRACE_LEN / 4];
        pack_trace(&mut out, &[]);
        assert_eq!(out[0][0], 0.0);
    }

    /// The uniform block is four-float rows all the way down, so what Rust
    /// writes and what WGSL reads cannot drift apart over padding.
    #[test]
    fn the_uniform_block_is_vec4_rows() {
        assert_eq!(std::mem::size_of::<CompUniforms>() % 16, 0);
        // frame + curve + meter + color + two traces.
        assert_eq!(
            std::mem::size_of::<CompUniforms>(),
            (4 + 2 * (TRACE_LEN / 4)) * 16
        );
    }

    /// The shader compiles, as `compose` assembles it — validating the
    /// fragment alone passes happily while the composed module does not.
    #[test]
    fn the_shader_compiles_and_validates() {
        let source = fts_audio_ui::shader::compose(SHADER);
        let module = naga::front::wgsl::parse_str(&source).unwrap_or_else(|e| {
            panic!(
                "the comp shader does not parse: {}",
                e.emit_to_string(&source)
            )
        });
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        );
        if let Err(e) = validator.validate(&module) {
            panic!("the comp shader does not validate: {e:?}");
        }
    }
}
