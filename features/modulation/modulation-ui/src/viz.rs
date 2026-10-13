//! A picture per modulation engine, rather than one sine for all six.
//!
//! The panel drew the same LFO whatever was loaded, which says only "something
//! is wobbling at this rate". But a chorus and a phaser at identical rate and
//! depth do entirely different things to a signal, and the point of a
//! visualiser is to tell them apart — so each engine draws what it actually
//! does:
//!
//! | engine | what it is | what it draws |
//! |---|---|---|
//! | Chorus | detuned copies around the dry | voices weaving apart and back together |
//! | Flanger | comb filter, swept | the comb, its teeth sliding |
//! | Phaser | allpass notches, swept | notches travelling through a band |
//! | Tremolo | amplitude modulation | a waveform pumping in level |
//! | Vibrato | pitch modulation | a waveform stretching and compressing |
//! | Rotary | a speaker going round | the orbit, and the doppler it makes |
//!
//! Modulation is cyan-led and motion pink-led, so which of the two groups you
//! are reading is answerable without going to the label.

use std::cell::RefCell;
use std::rc::Rc;

use anyrender::{PaintScene, RenderContext, Scene};
#[cfg(not(target_arch = "wasm32"))]
use blitz_dom::node::{ComputedStyles, Widget};
use dioxus::prelude::*;
use fts_audio_ui::shader::{ShaderSurface, Uniforms};

use vello::kurbo::{Affine, BezPath, Circle, Line, Point, Rect, Stroke};
use vello::peniko::{Color, ColorStop, Fill, Gradient};

/// Which engine is loaded. Ordered as the rig groups them: the three that
/// colour a signal, then the three that move it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Engine {
    #[default]
    Chorus,
    Flanger,
    Phaser,
    Tremolo,
    Vibrato,
    Rotary,
}

/// Which of the two slots an engine belongs in.
///
/// Not a preset and not a colour: the rig has two modulation slots and they
/// hold different machines. One colours a signal where it stands, the other
/// moves it — and a panel that offered a tremolo in the slot meant for a
/// chorus would be offering the wrong thing, whatever hue it was drawn in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    /// Chorus, flanger, phaser — they change the signal's colour in place.
    Colouring,
    /// Tremolo, vibrato, rotary — they move it: in level, in pitch, in space.
    Moving,
}

impl Engine {
    /// The three that colour a signal. The rig's MOD slot.
    pub const COLOURING: [Self; 3] = [Self::Chorus, Self::Flanger, Self::Phaser];

    /// The three that move it. The rig's MOTION slot.
    pub const MOVING: [Self; 3] = [Self::Tremolo, Self::Vibrato, Self::Rotary];

    /// Which slot this engine belongs in.
    #[must_use]
    pub const fn group(self) -> Group {
        match self {
            Self::Chorus | Self::Flanger | Self::Phaser => Group::Colouring,
            Self::Tremolo | Self::Vibrato | Self::Rotary => Group::Moving,
        }
    }

    /// The engines in `group`.
    #[must_use]
    pub const fn of(group: Group) -> [Self; 3] {
        match group {
            Group::Colouring => Self::COLOURING,
            Group::Moving => Self::MOVING,
        }
    }

    /// Every engine, in the order the shader's constants declare them.
    ///
    /// Which is also [`COLOURING`](Self::COLOURING) followed by
    /// [`MOVING`](Self::MOVING) — the rig's two slots, in order.
    pub const ALL: [Self; 6] = [
        Self::Chorus,
        Self::Flanger,
        Self::Phaser,
        Self::Tremolo,
        Self::Vibrato,
        Self::Rotary,
    ];
}

/// What a modulation panel draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModView {
    pub engine: Engine,
    /// LFO rate, Hz.
    pub rate: f32,
    /// 0..=1.
    pub depth: f32,
    /// 0..=1 — how much of the effect is in the signal.
    pub mix: f32,
    pub on: bool,
    /// The group's colour: cyan for modulation, pink for motion.
    pub color: [u8; 3],
    /// Seconds since the panel appeared.
    pub time: f32,
}

pub type Shared<T> = Rc<RefCell<T>>;

/// The WGSL the shader path runs. See `mod_shader.wgsl`.
///
/// Public so the engine contact sheet can render the real thing rather than a
/// copy of it — see `examples/engine_sheet.rs`.
pub const SHADER: &str = include_str!("viz.wgsl");

/// The engine, painted — on the GPU where there is one, in vectors where
/// there is not.
pub struct ModWidget {
    view: Shared<ModView>,
    born: web_time::Instant,
    /// Built once, from whatever `can_create_surfaces` hands over. `None` on a
    /// renderer with no device to give, which is not an error: the vector
    /// painter below draws the same engines.
    gpu: Option<ShaderSurface>,
}

impl ModWidget {
    #[must_use]
    pub fn new(view: Shared<ModView>) -> Self {
        Self {
            view,
            born: web_time::Instant::now(),
            gpu: None,
        }
    }
}

impl ModWidget {
    /// Take a GPU device and build the shader surface — the
    /// host-agnostic half of `Widget::can_create_surfaces`.
    pub fn create_surfaces(&mut self, render_ctx: &mut dyn RenderContext) {
        self.gpu = render_ctx
            .renderer_specific_context()
            .and_then(|ctx| ShaderSurface::new(ctx, SHADER));
    }

    /// Record a frame — the host-agnostic half of `Widget::paint`.
    pub fn paint_frame(
        &mut self,
        _render_ctx: &mut dyn RenderContext,
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

        // The shader, if the renderer will take its texture this frame. It
        // may decline for reasons that change frame to frame — a surface that
        // is not up yet — so this is asked every time rather than decided
        // once.
        if let Some(gpu) = self.gpu.as_mut() {
            let [r, g, b] = view.color;
            let uniforms = Uniforms {
                frame: [w as f32, h as f32, view.time, engine_index(view.engine)],
                params: [
                    view.rate,
                    view.depth,
                    view.mix,
                    if view.on { 1.0 } else { 0.0 },
                ],
                color: [
                    f32::from(r) / 255.0,
                    f32::from(g) / 255.0,
                    f32::from(b) / 255.0,
                    1.0,
                ],
            };
            if gpu.draw(_render_ctx, &mut scene, width, height, uniforms) {
                return scene;
            }
        }

        paint_mod(&mut scene, &view, w, h);
        scene
    }
}

/// Blitz's custom widget, natively.
#[cfg(not(target_arch = "wasm32"))]
impl Widget for ModWidget {
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
impl fts_audio_ui::scene_canvas::CanvasPanel for ModWidget {
    fn can_create_surfaces(&mut self, ctx: &mut dyn RenderContext) {
        self.create_surfaces(ctx);
    }

    fn paint(&mut self, ctx: &mut dyn RenderContext, width: u32, height: u32, scale: f64) -> Scene {
        self.paint_frame(ctx, width, height, scale)
    }
}

/// The engine, as the shader's `u.frame.w`. The order is the shader's
/// constants; the two must agree, so they are written next to each other.
pub fn engine_index(engine: Engine) -> f32 {
    match engine {
        Engine::Chorus => 0.0,
        Engine::Flanger => 1.0,
        Engine::Phaser => 2.0,
        Engine::Tremolo => 3.0,
        Engine::Vibrato => 4.0,
        Engine::Rotary => 5.0,
    }
}

fn colors(view: &ModView) -> (Color, Color) {
    if !view.on {
        let grey = Color::from_rgba8(63, 63, 70, 255);
        return (grey, grey);
    }
    let [r, g, b] = view.color;
    let lift = |c: u8| f32::from(c).mul_add(0.42, 255.0 * 0.58) as u8;
    (
        Color::from_rgba8(r, g, b, 255),
        Color::from_rgba8(lift(r), lift(g), lift(b), 255),
    )
}

fn fade(c: Color, a: f64) -> Color {
    c.multiply_alpha(a.clamp(0.0, 1.0) as f32)
}

/// Draw whichever engine this is.
pub fn paint_mod(scene: &mut Scene, view: &ModView, w: f64, h: f64) {
    let (base, bright) = colors(view);
    // A frozen picture for a bypassed block: the shape, without the motion.
    let t = if view.on { f64::from(view.time) } else { 0.0 };
    let rate = f64::from(view.rate).clamp(0.01, 20.0);
    let depth = f64::from(view.depth).clamp(0.0, 1.0);
    let phase = t * rate;

    // A ground tinted by the group, so the two panels differ even at a glance
    // and even when nothing is loaded.
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &Gradient::new_linear(Point::new(0.0, 0.0), Point::new(0.0, h)).with_stops([
            ColorStop::from((0.0, fade(base, 0.10))),
            ColorStop::from((1.0, fade(base, 0.01))),
        ]),
        None,
        &Rect::new(0.0, 0.0, w, h),
    );

    match view.engine {
        Engine::Chorus => chorus(scene, base, bright, w, h, phase, depth),
        Engine::Flanger => flanger(scene, base, bright, w, h, phase, depth),
        Engine::Phaser => phaser(scene, base, bright, w, h, phase, depth),
        Engine::Tremolo => tremolo(scene, base, bright, w, h, phase, depth),
        Engine::Vibrato => vibrato(scene, base, bright, w, h, phase, depth),
        Engine::Rotary => rotary(scene, base, bright, w, h, phase, depth),
    }
}

/// Detuned copies drifting around the dry signal and back.
///
/// Three voices, each delayed a little differently, so they separate and
/// converge — which is the sound: one guitar becoming several and returning.
fn chorus(scene: &mut Scene, base: Color, bright: Color, w: f64, h: f64, phase: f64, depth: f64) {
    let mid = h * 0.5;
    let voices = 3;
    for v in 0..voices {
        let offset = v as f64 / voices as f64;
        let mut path = BezPath::new();
        for px in 0..=120 {
            let x = f64::from(px) / 120.0;
            // Each voice's own slow drift, a third of a cycle apart.
            let drift = ((phase + offset) * std::f64::consts::TAU).sin();
            let wobble = ((x * 6.0 + drift * 2.0) * std::f64::consts::TAU).sin();
            let spread = depth * h * 0.30 * drift;
            let y = mid + wobble * h * 0.10 + spread * (offset - 0.5) * 2.0;
            let p = (x * w, y);
            if px == 0 {
                path.move_to(p);
            } else {
                path.line_to(p);
            }
        }
        let lit = if v == 1 { bright } else { base };
        scene.stroke(
            &Stroke::new(if v == 1 { 1.8 } else { 1.2 }),
            Affine::IDENTITY,
            fade(lit, if v == 1 { 0.85 } else { 0.42 }),
            None,
            &path,
        );
    }
}

/// The comb, with its teeth sliding.
///
/// A flanger is a very short delay swept against the dry — the notches march
/// up and down the spectrum, which is what the moving teeth are.
fn flanger(scene: &mut Scene, base: Color, bright: Color, w: f64, h: f64, phase: f64, depth: f64) {
    let sweep = (phase * std::f64::consts::TAU).sin() * 0.5 + 0.5;
    let teeth = 48;
    for i in 0..teeth {
        let x = (f64::from(i) + 0.5) / f64::from(teeth) * w;
        let f = f64::from(i) / f64::from(teeth);
        // Comb response: notch spacing set by the swept delay.
        let comb = ((f * (4.0 + 12.0 * sweep) * std::f64::consts::TAU).cos() * 0.5 + 0.5)
            .powf(1.0 + 2.0 * depth);
        let height = comb * h * 0.82;
        let lit = comb > 0.82;
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            fade(if lit { bright } else { base }, 0.22 + 0.62 * comb),
            None,
            &Rect::new(
                x - w / f64::from(teeth) * 0.35,
                h - height,
                x + w / f64::from(teeth) * 0.35,
                h,
            ),
        );
    }
}

/// Notches travelling through a band.
///
/// A phaser's allpass stages put a handful of moving notches in the response;
/// the curve is the response and the dips are the stages.
fn phaser(scene: &mut Scene, base: Color, bright: Color, w: f64, h: f64, phase: f64, depth: f64) {
    let stages = 4;
    let sweep = (phase * std::f64::consts::TAU).sin() * 0.5 + 0.5;
    let mut path = BezPath::new();
    for px in 0..=160 {
        let x = f64::from(px) / 160.0;
        let mut y = 0.0;
        for s in 0..stages {
            let centre = (0.12 + 0.20 * f64::from(s) + sweep * 0.30).fract();
            let d = (x - centre) / 0.045;
            y -= depth * (-d * d).exp();
        }
        let yy = h * 0.5 - y * h * 0.40;
        let p = (x * w, yy);
        if px == 0 {
            path.move_to(p);
        } else {
            path.line_to(p);
        }
    }
    scene.stroke(
        &Stroke::new(1.8),
        Affine::IDENTITY,
        fade(bright, 0.9),
        None,
        &path,
    );
    // The notch positions, marked — so the travel is countable.
    for s in 0..stages {
        let centre = (0.12 + 0.20 * f64::from(s) + sweep * 0.30).fract();
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            fade(base, 0.5),
            None,
            &Circle::new(Point::new(centre * w, h * 0.5 + depth * h * 0.40), 2.0),
        );
    }
}

/// A waveform pumping in level.
///
/// Tremolo is amplitude modulation and nothing else, so the picture is a
/// signal whose envelope breathes at the rate — the shape of the sound.
fn tremolo(scene: &mut Scene, base: Color, bright: Color, w: f64, h: f64, phase: f64, depth: f64) {
    let mid = h * 0.5;
    let mut top = BezPath::new();
    let mut bottom = BezPath::new();
    for px in 0..=160 {
        let x = f64::from(px) / 160.0;
        // The LFO, scrolling right to left so the panel reads as time passing.
        let lfo =
            (((x - phase) * std::f64::consts::TAU).sin() * 0.5 + 0.5).mul_add(depth, 1.0 - depth);
        let carrier = ((x * 34.0) * std::f64::consts::TAU).sin();
        let amp = lfo * carrier * h * 0.40;
        let p_top = (x * w, mid - amp.abs());
        let p_bot = (x * w, mid + amp.abs());
        if px == 0 {
            top.move_to(p_top);
            bottom.move_to(p_bot);
        } else {
            top.line_to(p_top);
            bottom.line_to(p_bot);
        }
    }
    for path in [&top, &bottom] {
        scene.stroke(
            &Stroke::new(1.4),
            Affine::IDENTITY,
            fade(bright, 0.85),
            None,
            path,
        );
    }
    // The envelope itself, behind — the thing actually being modulated.
    let mut env = BezPath::new();
    env.move_to((0.0, mid));
    for px in 0..=160 {
        let x = f64::from(px) / 160.0;
        let lfo =
            (((x - phase) * std::f64::consts::TAU).sin() * 0.5 + 0.5).mul_add(depth, 1.0 - depth);
        env.line_to((x * w, mid - lfo * h * 0.40));
    }
    for px in (0..=160).rev() {
        let x = f64::from(px) / 160.0;
        let lfo =
            (((x - phase) * std::f64::consts::TAU).sin() * 0.5 + 0.5).mul_add(depth, 1.0 - depth);
        env.line_to((x * w, mid + lfo * h * 0.40));
    }
    env.close_path();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        fade(base, 0.16),
        None,
        &env,
    );
}

/// A waveform stretching and compressing.
///
/// Vibrato moves pitch, not level — so the wave keeps its height and changes
/// its spacing, which is the one picture that cannot be confused with tremolo.
fn vibrato(scene: &mut Scene, base: Color, bright: Color, w: f64, h: f64, phase: f64, depth: f64) {
    let mid = h * 0.5;
    let mut path = BezPath::new();
    for px in 0..=200 {
        let x = f64::from(px) / 200.0;
        // Phase modulation: the argument is bent, so cycles bunch and spread.
        let bend = depth * 0.5 * (((x * 2.0 - phase) * std::f64::consts::TAU).sin());
        let y = mid + ((x * 26.0 + bend * 6.0) * std::f64::consts::TAU).sin() * h * 0.36;
        let p = (x * w, y);
        if px == 0 {
            path.move_to(p);
        } else {
            path.line_to(p);
        }
    }
    scene.stroke(
        &Stroke::new(1.6),
        Affine::IDENTITY,
        fade(bright, 0.9),
        None,
        &path,
    );

    // Gridlines that bend with it, so the stretch is visible against something
    // straight-ish rather than having to be inferred from the wave alone.
    for i in 1..8 {
        let x0 = f64::from(i) / 8.0;
        let bend = depth * 0.5 * (((x0 * 2.0 - phase) * std::f64::consts::TAU).sin());
        let x = (x0 + bend * 0.03).clamp(0.0, 1.0) * w;
        scene.stroke(
            &Stroke::new(1.0),
            Affine::IDENTITY,
            fade(base, 0.22),
            None,
            &Line::new(Point::new(x, h * 0.12), Point::new(x, h * 0.88)),
        );
    }
}

/// The orbit, and the doppler it makes.
///
/// A rotary is a speaker going round: the horn approaches and recedes, which
/// is heard as level and pitch moving together. The dot is the horn, the
/// trail is where it has been, the width is how far it throws.
fn rotary(scene: &mut Scene, base: Color, bright: Color, w: f64, h: f64, phase: f64, depth: f64) {
    let (cx, cy) = (w * 0.5, h * 0.5);
    let (rx, ry) = (w * 0.34 * (0.5 + 0.5 * depth), h * 0.33);
    // The orbit.
    let mut ring = BezPath::new();
    for i in 0..=72 {
        let a = f64::from(i) / 72.0 * std::f64::consts::TAU;
        let p = (cx + a.cos() * rx, cy + a.sin() * ry);
        if i == 0 {
            ring.move_to(p);
        } else {
            ring.line_to(p);
        }
    }
    scene.stroke(
        &Stroke::new(1.0),
        Affine::IDENTITY,
        fade(base, 0.35),
        None,
        &ring,
    );

    // The trail: the last half turn, fading.
    let head = phase * std::f64::consts::TAU;
    for i in 0..14 {
        let back = f64::from(i) * 0.075;
        let a = head - back;
        let p = Point::new(cx + a.cos() * rx, cy + a.sin() * ry);
        // Nearer the front of the orbit = louder, which is the doppler.
        let front = (a.sin() * 0.5 + 0.5).mul_add(0.6, 0.4);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            fade(base, (1.0 - back / 1.05) * 0.5 * front),
            None,
            &Circle::new(p, 1.5 + 2.5 * front * (1.0 - back)),
        );
    }
    // The horn itself.
    let p = Point::new(cx + head.cos() * rx, cy + head.sin() * ry);
    let front = (head.sin() * 0.5 + 0.5).mul_add(0.6, 0.4);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        fade(bright, 0.95),
        None,
        &Circle::new(p, 2.5 + 3.0 * front),
    );
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

/// One modulation panel, painted.
#[component]
pub fn ModViz(
    engine: Engine,
    rate: f32,
    depth: f32,
    mix: f32,
    on: bool,
    color: [u8; 3],
) -> Element {
    use_repaint_clock(on);
    let view: Shared<ModView> = use_hook(|| Rc::new(RefCell::new(ModView::default())));
    // Marked each render: the view below is new, and only the widget
    // knows to draw it (the DOM does not change).
    #[cfg(not(target_arch = "wasm32"))]
    let fresh = use_hook(fts_audio_ui::widget::Fresh::default);
    #[cfg(not(target_arch = "wasm32"))]
    let attr =
        use_hook(|| dioxus_native_dom::CustomWidgetAttr::new(fts_audio_ui::widget::Rasterized::new(fts_audio_ui::widget::Freshened::new(ModWidget::new(Rc::clone(&view)), fresh.clone()))));

    *view.borrow_mut() = ModView {
        engine,
        rate,
        depth,
        mix,
        on,
        color,
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
        let panel =
            use_hook(|| fts_audio_ui::scene_canvas::Panel::new(ModWidget::new(Rc::clone(&view))));
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

    fn view(engine: Engine) -> ModView {
        ModView {
            engine,
            rate: 1.6,
            depth: 0.7,
            mix: 0.4,
            on: true,
            color: [34, 211, 238],
            time: 0.0,
        }
    }

    /// The two slots partition the engines: every engine is in exactly one,
    /// and `ALL` is the two of them in order. A host that lists the wrong
    /// three offers a tremolo where a chorus belongs.
    #[test]
    fn the_groups_partition_the_engines() {
        for engine in Engine::ALL {
            let group = engine.group();
            assert!(
                Engine::of(group).contains(&engine),
                "{engine:?} says {group:?} but is not listed there"
            );
            let other = match group {
                Group::Colouring => Group::Moving,
                Group::Moving => Group::Colouring,
            };
            assert!(
                !Engine::of(other).contains(&engine),
                "{engine:?} is in both groups"
            );
        }
        let mut joined = Engine::COLOURING.to_vec();
        joined.extend(Engine::MOVING);
        assert_eq!(
            joined,
            Engine::ALL.to_vec(),
            "ALL is the two slots in order"
        );
    }

    /// Every engine paints, at every size a panel takes, at every point in its
    /// cycle — including the degenerate sizes a flex row hands out mid-layout.
    #[test]
    fn every_engine_paints() {
        for engine in Engine::ALL {
            let mut v = view(engine);
            for (w, h) in [(2.0, 2.0), (140.0, 48.0), (900.0, 200.0)] {
                for step in 0..40 {
                    v.time = step as f32 * 0.05;
                    let mut scene = Scene::new();
                    paint_mod(&mut scene, &v, w, h);
                }
            }
        }
    }

    /// A rate of zero does not divide by anything, and a depth of zero draws
    /// the engine at rest rather than nothing at all.
    #[test]
    fn the_extremes_are_safe() {
        for engine in Engine::ALL {
            let mut v = view(engine);
            v.rate = 0.0;
            v.depth = 0.0;
            let mut scene = Scene::new();
            paint_mod(&mut scene, &v, 200.0, 60.0);
        }
    }

    /// The shader compiles.
    ///
    /// Nothing else in the tree would notice if it did not: the screenshot
    /// tool cannot run the shader path at all — `VelloImageRenderer` hands out
    /// no device, so the vector fallback is what it always draws — and the
    /// window would simply show the fallback too, silently, because a
    /// `ShaderSurface` that fails to build is indistinguishable from a
    /// renderer that declined. This is the only thing standing between a typo
    /// and a feature that quietly never runs.
    #[test]
    fn the_shader_compiles_and_validates() {
        let source = fts_audio_ui::shader::compose(SHADER);
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("the shader does not parse: {}", e.emit_to_string(&source)));
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        );
        if let Err(e) = validator.validate(&module) {
            panic!("the shader does not validate: {e:?}");
        }
    }

    /// The engine indices the shader branches on match the order its
    /// constants declare. They are two halves of one switch written in two
    /// languages, and nothing but this checks that they agree.
    #[test]
    fn the_shader_and_rust_agree_on_engine_order() {
        let wgsl = include_str!("viz.wgsl");
        for (engine, name) in [
            (Engine::Chorus, "CHORUS"),
            (Engine::Flanger, "FLANGER"),
            (Engine::Phaser, "PHASER"),
            (Engine::Tremolo, "TREMOLO"),
            (Engine::Vibrato, "VIBRATO"),
            (Engine::Rotary, "ROTARY"),
        ] {
            let want = format!("const {name}:");
            let line = wgsl
                .lines()
                .find(|l| l.trim_start().starts_with(&want))
                .unwrap_or_else(|| panic!("{name} is not declared in the shader"));
            let value: f32 = line
                .rsplit('=')
                .next()
                .and_then(|v| v.trim().trim_end_matches(';').parse().ok())
                .unwrap_or_else(|| panic!("{name} has no numeric value: {line}"));
            assert!(
                (value - engine_index(engine)).abs() < f32::EPSILON,
                "{name} is {value} in the shader and {} in rust",
                engine_index(engine)
            );
        }
    }
}
