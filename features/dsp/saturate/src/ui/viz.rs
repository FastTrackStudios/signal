//! A picture per saturation circuit, and a multiband overview.
//!
//! The panels already draw a transfer curve, and a transfer curve alone says
//! less than it looks like it does: a valve and a transistor at the same
//! drive can draw nearly the same S, and what separates them — which
//! harmonics come out, which frequencies reach the knee first, how the stage
//! moves under a loud passage — is not on it. So each circuit gets the curve
//! *and* the one other picture that says what kind of machine it is:
//!
//! | circuit | what it is | what it draws |
//! |---|---|---|
//! | Valve | asymmetric, so even | the curve leaning, its asymmetry shaded, and the harmonic ladder with the evens lit |
//! | Tape | a soft knee into compression, top pre-emphasised | the curve with the loop a real note traces round it, the level knee, and the emphasis shelf |
//! | Core | iron that saturates on lows | the curve, and distortion measured across frequency — the bloom at the bottom |
//! | Solid | a hard corner, odd harmonics | the curve with its corners and crossover deadband marked, a clipped sine, and the odd comb |
//! | Steps | quantisation and rate reduction | the staircase, and a sine through the converter: held, stepped, ceilinged |
//!
//! **Nothing here is a drawing of a circuit.** Every line is measured off the
//! engine: [`crate::profiles::apply`] voices a stage exactly as the audio
//! thread does, the curves are [`ClassAPreamp::transfer`], and the ladders,
//! loops and knees run sines through [`ClassAPreamp::process`] and
//! [`DigitalStage::process`] on a state clone. Turn a knob and the picture
//! changes because the machine did.
//!
//! The painters record into an [`anyrender::Scene`] — a command list, so
//! whatever can replay it can draw this: vello on a GPU, `vello_cpu` in a
//! test, a canvas in a browser. No widget and no shader: the consumer owns
//! its scene and calls [`paint_sat`] / [`paint_multiband_sat`] into it. Plain
//! arithmetic and allocation only — no clock, no threads — so it is as
//! wasm-portable as the DSP it reads.

use crate::digital::DigitalStage;
use crate::preamp::{ClassAPreamp, analysis};
use crate::profiles::{Controls, Profile};
use anyrender::{PaintScene, Scene};
use dsp_core::num;
use kurbo::{Affine, BezPath, Circle, Line, Point, Rect, RoundedRect, Stroke};
use peniko::{Color, ColorStop, Fill, Gradient};

/// What kind of machine a stage reads as — the engine's own classification,
/// decided from its shapers rather than its preset name, so a stage edited
/// away from its profile is drawn as what it has become.
pub use crate::preamp::Circuit;

/// The rate the probes run the engine at. The pictures are about shape, and
/// shape does not move between 44.1 and 96 kHz in any way a panel can show.
const SAMPLE_RATE: f32 = 48_000.0;

/// Where the probes play when there is no live level: a healthy −6 dBFS, so a
/// stage at rest is drawn doing what it does to a real signal.
const DEFAULT_PROBE_DBFS: f32 = -6.0;

/// Points along a curve. Enough that a hard corner reads as a corner.
const CURVE_POINTS: u16 = 240;

// ── The view ────────────────────────────────────────────────────────────────

/// What a saturation panel draws.
///
/// Plain data: the profile (which carries the voicing the curve is computed
/// from) and the panel's knobs, normalised as the parameters are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SatView {
    /// Which saturator. Its [`crate::profiles::Voicing`] is the circuit.
    pub profile: Profile,
    /// Drive, bias, sag, tilt, the two circuit knobs and mix — the same
    /// struct the plugin hands to [`crate::profiles::apply`].
    pub controls: Controls,
    /// Output trim, dB.
    pub output_db: f32,
    /// A recent input peak, linear full-scale (1.0 = 0 dBFS), for the live
    /// dot on the curve. `None` draws the stage at rest and probes it at
    /// −6 dBFS.
    pub level: Option<f32>,
    /// Bypassed blocks draw in grey.
    pub on: bool,
}

impl SatView {
    /// A view of the profile `id` with every knob at noon.
    #[must_use]
    pub fn new(id: &str) -> Option<Self> {
        crate::profiles::profile_by_id(id).map(|profile| Self::of(*profile, Controls::default()))
    }

    /// A view of `profile` at `controls`.
    #[must_use]
    pub const fn of(profile: Profile, controls: Controls) -> Self {
        Self {
            profile,
            controls,
            output_db: 0.0,
            level: None,
            on: true,
        }
    }

    /// The engine, voiced exactly as the audio thread voices it.
    #[must_use]
    pub fn stage(&self) -> Stage {
        let mut pre = ClassAPreamp::new(SAMPLE_RATE);
        let mut digital = DigitalStage::new();
        crate::profiles::apply(&self.profile, &self.controls, &mut pre, &mut digital);
        Stage {
            pre,
            digital,
            mix: self.controls.mix.clamp(0.0, 1.0),
            output_gain: 10.0_f32.powf(self.output_db.clamp(-48.0, 24.0) / 20.0),
        }
    }

    /// Which machine this is. The quantiser is invisible to the preamp's own
    /// classification, so a digital voicing is asked about first.
    #[must_use]
    pub fn circuit(&self) -> Circuit {
        if self.profile.voicing.digital {
            Circuit::Steps
        } else {
            self.stage().pre.circuit()
        }
    }

    /// The level the probes play at, dBFS.
    fn probe_dbfs(&self) -> f32 {
        self.level.map_or(DEFAULT_PROBE_DBFS, |peak| {
            (20.0 * peak.max(1.0e-4).log10()).clamp(-48.0, 0.0)
        })
    }
}

/// A saturator, voiced: the two engine stages plus the mix and trim the
/// plugin applies after them.
#[derive(Clone, Debug)]
pub struct Stage {
    pub pre: ClassAPreamp,
    pub digital: DigitalStage,
    /// Dry/wet, 0..=1.
    pub mix: f32,
    /// Output trim, linear.
    pub output_gain: f32,
}

impl Stage {
    /// The static curve the listener hears, sampled over x ∈ [−1, 1]:
    /// shaped, made up, quantised, mixed with the dry and trimmed.
    ///
    /// Rate reduction and dither are left out — a held sample is a fact
    /// about time and dither is noise, so neither has a static curve — but
    /// word length does: a crushed signal really is a staircase.
    #[must_use]
    pub fn heard_curve(&self, points: u16) -> Vec<(f32, f32)> {
        let mut quantiser = self.digital.clone();
        quantiser.rate = 1.0;
        quantiser.dither = 0.0;
        quantiser.reset();
        let makeup = self.pre.makeup_gain();
        let last = f32::from(points.max(2).saturating_sub(1));
        (0..points.max(2))
            .map(|i| {
                let x = 2.0f32.mul_add(f32::from(i) / last, -1.0);
                let wet = quantiser.process(0, self.pre.transfer(x) * makeup);
                (x, self.heard(x, wet))
            })
            .collect()
    }

    /// Dry and wet, mixed and trimmed — the plugin's last two steps.
    fn heard(&self, dry: f32, wet: f32) -> f32 {
        (wet - dry).mul_add(self.mix, dry) * self.output_gain
    }

    /// The harmonic ladder H1..Hn at `level_dbfs`, normalised to H1.
    fn harmonics(&self, level_dbfs: f32, out: &mut [f32]) {
        analysis::harmonic_spectrum_at(&self.pre, level_dbfs, out);
    }
}

// ── Colour ──────────────────────────────────────────────────────────────────

/// One circuit's colours: its own hue, a lifted version for the lit parts,
/// and the ink the axes are drawn in.
#[derive(Clone, Copy, Debug)]
struct Palette {
    base: Color,
    bright: Color,
    ink: Color,
}

/// Each circuit's hue. Five machines that must be told apart at a glance,
/// in the multiband view especially, where they sit side by side.
#[must_use]
pub const fn circuit_rgb(circuit: Circuit) -> [u8; 3] {
    match circuit {
        // A lit heater.
        Circuit::Valve => [240, 150, 50],
        // Oxide.
        Circuit::Tape => [214, 112, 92],
        // Iron, cold.
        Circuit::Core => [104, 160, 226],
        // Solid state, unromantic.
        Circuit::Solid => [79, 209, 165],
        // Not saturation at all.
        Circuit::Steps => [224, 80, 122],
    }
}

impl Palette {
    fn of(circuit: Circuit, on: bool) -> Self {
        let ink = Color::from_rgba8(206, 212, 222, 255);
        if !on {
            let grey = Color::from_rgba8(96, 98, 108, 255);
            return Self {
                base: grey,
                bright: Color::from_rgba8(140, 142, 150, 255),
                ink,
            };
        }
        let [r, g, b] = circuit_rgb(circuit);
        let lift = |c: u8| -> u8 {
            // Toward white by ~45%: 255 − (255 − c)·0.55, in integers so no
            // float has to be narrowed back into a byte.
            let up = u16::from(255_u8.saturating_sub(c)).saturating_mul(55) / 100;
            255_u8.saturating_sub(u8::try_from(up).unwrap_or(u8::MAX))
        };
        Self {
            base: Color::from_rgba8(r, g, b, 255),
            bright: Color::from_rgba8(lift(r), lift(g), lift(b), 255),
            ink,
        }
    }
}

const fn fade(c: Color, a: f32) -> Color {
    c.multiply_alpha(a.clamp(0.0, 1.0))
}

// ── Primitives ──────────────────────────────────────────────────────────────

fn fill(scene: &mut Scene, color: Color, shape: &impl kurbo::Shape) {
    scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, shape);
}

fn stroke(scene: &mut Scene, width: f64, color: Color, shape: &impl kurbo::Shape) {
    scene.stroke(&Stroke::new(width), Affine::IDENTITY, color, None, shape);
}

fn dashed(scene: &mut Scene, width: f64, color: Color, shape: &impl kurbo::Shape) {
    let style = Stroke::new(width).with_dashes(0.0, [3.0, 4.0]);
    scene.stroke(&style, Affine::IDENTITY, color, None, shape);
}

/// A polyline through `points`.
fn polyline(points: impl IntoIterator<Item = Point>) -> BezPath {
    let mut path = BezPath::new();
    for p in points {
        if path.elements().is_empty() {
            path.move_to(p);
        } else {
            path.line_to(p);
        }
    }
    path
}

/// The ground: a wash in the circuit's hue, so even a picture with nothing
/// driven says which machine it is.
fn ground(scene: &mut Scene, rect: Rect, pal: Palette) {
    let wash = Gradient::new_linear(Point::new(rect.x0, rect.y0), Point::new(rect.x0, rect.y1))
        .with_stops([
            ColorStop::from((0.0, fade(pal.base, 0.12))),
            ColorStop::from((1.0, fade(pal.base, 0.02))),
        ]);
    scene.fill(Fill::NonZero, Affine::IDENTITY, &wash, None, &rect);
}

/// A thin frame, so a pane reads as an instrument rather than a smear.
fn frame(scene: &mut Scene, rect: Rect, pal: Palette) {
    stroke(
        scene,
        1.0,
        fade(pal.ink, 0.16),
        &RoundedRect::from_rect(rect, 3.0),
    );
}

/// Linear position of `v` across `lo..=hi`, clamped.
fn unit(v: f64, lo: f64, hi: f64) -> f64 {
    if hi <= lo {
        return 0.0;
    }
    ((v - lo) / (hi - lo)).clamp(0.0, 1.0)
}

/// dB of a linear magnitude, floored.
fn db(x: f32) -> f32 {
    20.0 * x.max(1.0e-6).log10()
}

/// A rectangle inset by `d` on every side, never inverted.
fn inset(r: Rect, d: f64) -> Rect {
    let dx = d.min(r.width() * 0.5);
    let dy = d.min(r.height() * 0.5);
    Rect::new(r.x0 + dx, r.y0 + dy, r.x1 - dx, r.y1 - dy)
}

/// A curve plot: x ∈ [−1, 1] across, y ∈ [−1, 1] up.
#[derive(Clone, Copy, Debug)]
struct Plot {
    rect: Rect,
}

impl Plot {
    fn point(self, x: f32, y: f32) -> Point {
        let r = self.rect;
        let ux = f64::from(x.clamp(-1.25, 1.25)).mul_add(0.5, 0.5);
        let uy = f64::from(y.clamp(-1.25, 1.25)).mul_add(0.5, 0.5);
        Point::new(r.width().mul_add(ux, r.x0), r.height().mul_add(-uy, r.y1))
    }

    /// Axes, and the unity line the curve is bending away from.
    fn axes(self, scene: &mut Scene, pal: Palette) {
        stroke(
            scene,
            0.8,
            fade(pal.ink, 0.28),
            &Line::new(self.point(-1.0, 0.0), self.point(1.0, 0.0)),
        );
        stroke(
            scene,
            0.8,
            fade(pal.ink, 0.28),
            &Line::new(self.point(0.0, -1.0), self.point(0.0, 1.0)),
        );
        dashed(
            scene,
            0.7,
            fade(pal.ink, 0.22),
            &Line::new(self.point(-1.0, -1.0), self.point(1.0, 1.0)),
        );
    }

    fn curve(self, scene: &mut Scene, width: f64, color: Color, points: &[(f32, f32)]) {
        let path = polyline(points.iter().map(|&(x, y)| self.point(x, y)));
        stroke(scene, width, color, &path);
    }
}

/// The square curve pane on the left and the rest on the right.
fn split(w: f64, h: f64) -> (Rect, Rect) {
    let pad = (h * 0.07).clamp(1.0, 14.0);
    let side = 2.0f64.mul_add(-pad, h).min(w * 0.42).max(0.0);
    let curve = Rect::new(pad, pad, pad + side, pad + side);
    let right = Rect::new(
        (pad * 2.0 + side).min(w),
        pad,
        (w - pad).max(0.0),
        (h - pad).max(pad),
    );
    (curve, right)
}

// ── The single-stage painter ────────────────────────────────────────────────

/// Draw whichever circuit `view` is, `w` × `h`.
pub fn paint_sat(scene: &mut Scene, view: &SatView, w: f64, h: f64) {
    let circuit = view.circuit();
    let pal = Palette::of(circuit, view.on);
    ground(scene, Rect::new(0.0, 0.0, w.max(0.0), h.max(0.0)), pal);
    if w < 8.0 || h < 8.0 {
        return;
    }
    let stage = view.stage();
    let curve = stage.heard_curve(CURVE_POINTS);
    let (left, right) = split(w, h);
    let plot = Plot {
        rect: inset(left, 4.0),
    };
    frame(scene, left, pal);
    frame(scene, right, pal);
    plot.axes(scene, pal);

    match circuit {
        Circuit::Valve => valve(scene, view, &stage, &curve, plot, right, pal),
        Circuit::Tape => tape(scene, view, &stage, &curve, plot, right, pal),
        Circuit::Core => core(scene, view, &stage, &curve, plot, right, pal),
        Circuit::Solid => solid(scene, view, &stage, &curve, plot, right, pal),
        Circuit::Steps => steps(scene, view, &stage, &curve, plot, right, pal),
    }

    live_dot(scene, view, &stage, plot, pal);
    mix_bar(scene, view, left, pal);
}

/// The transfer curve itself, with the wet-only curve ghosted behind it when
/// the mix is holding some of it back.
fn transfer(scene: &mut Scene, stage: &Stage, curve: &[(f32, f32)], plot: Plot, pal: Palette) {
    if stage.mix < 0.999 {
        let wet_only = Stage {
            mix: 1.0,
            ..stage.clone()
        };
        plot.curve(
            scene,
            1.0,
            fade(pal.base, 0.35),
            &wet_only.heard_curve(CURVE_POINTS),
        );
    }
    plot.curve(scene, 2.2, fade(pal.bright, 0.95), curve);
}

/// Where the signal is on the curve right now, both polarities.
fn live_dot(scene: &mut Scene, view: &SatView, stage: &Stage, plot: Plot, pal: Palette) {
    let Some(peak) = view.level else { return };
    let x = peak.clamp(0.0, 1.0);
    let makeup = stage.pre.makeup_gain();
    let mut quantiser = stage.digital.clone();
    quantiser.rate = 1.0;
    quantiser.dither = 0.0;
    for side in [x, -x] {
        let wet = quantiser.process(0, stage.pre.transfer(side) * makeup);
        let p = plot.point(side, stage.heard(side, wet));
        fill(scene, fade(pal.bright, 0.25), &Circle::new(p, 7.0));
        fill(scene, pal.bright, &Circle::new(p, 3.2));
    }
}

/// How much of it you are actually hearing.
fn mix_bar(scene: &mut Scene, view: &SatView, left: Rect, pal: Palette) {
    let y = left.y1 - 3.0;
    let track = Rect::new(left.x0 + 4.0, y - 1.5, left.x1 - 4.0, y + 1.5);
    fill(scene, fade(pal.ink, 0.10), &track);
    let mix = f64::from(view.controls.mix.clamp(0.0, 1.0));
    if mix > 0.0 {
        let lit = Rect::new(
            track.x0,
            track.y0,
            track.width().mul_add(mix, track.x0),
            track.y1,
        );
        fill(scene, fade(pal.base, 0.8), &lit);
    }
}

/// Bars for H2..H(n+1), on a dB scale; `lit` says which ones this circuit is
/// about.
fn harmonic_bars(
    scene: &mut Scene,
    rect: Rect,
    pal: Palette,
    ladder: &[f32],
    lit: impl Fn(usize) -> bool,
) {
    // H1 is the note; the picture is what was added to it.
    let added = ladder.get(1..).unwrap_or(&[]);
    let n = num::count_to_f64(added.len().max(1));
    let slot = rect.width() / n;
    for (k, &mag) in added.iter().enumerate() {
        // `k` is H(k+2): even when k is even.
        let order = k.saturating_add(2);
        let height = unit(f64::from(db(mag)), -90.0, 0.0) * rect.height();
        let x = slot.mul_add(num::count_to_f64(k), rect.x0);
        let bar = Rect::new(x + slot * 0.18, rect.y1 - height, x + slot * 0.82, rect.y1);
        let on = lit(order);
        fill(
            scene,
            if on {
                fade(pal.bright, 0.9)
            } else {
                fade(pal.base, 0.35)
            },
            &bar,
        );
        // A tick at the floor, so a buried harmonic is visibly *there and
        // buried* rather than missing.
        stroke(
            scene,
            1.0,
            fade(pal.ink, 0.25),
            &Line::new(Point::new(bar.x0, rect.y1), Point::new(bar.x1, rect.y1)),
        );
    }
}

/// Valve: the curve leaning, the lean shaded, and the ladder with its evens
/// lit — plus the balance between them, which is the warmth in one number.
fn valve(
    scene: &mut Scene,
    view: &SatView,
    stage: &Stage,
    curve: &[(f32, f32)],
    plot: Plot,
    right: Rect,
    pal: Palette,
) {
    // The negative half turned over onto the positive one: where they part
    // is the asymmetry, and the asymmetry is the even harmonics.
    let mirrored: Vec<(f32, f32)> = curve
        .iter()
        .rev()
        .filter(|&&(x, _)| x <= 0.0)
        .map(|&(x, y)| (-x, -y))
        .collect();
    let positive: Vec<(f32, f32)> = curve.iter().copied().filter(|&(x, _)| x >= 0.0).collect();
    let mut lean = polyline(positive.iter().map(|&(x, y)| plot.point(x, y)));
    for &(x, y) in mirrored.iter().rev() {
        lean.line_to(plot.point(x, y));
    }
    lean.close_path();
    fill(scene, fade(pal.base, 0.28), &lean);
    dashed(
        scene,
        1.0,
        fade(pal.base, 0.7),
        &polyline(mirrored.iter().map(|&(x, y)| plot.point(x, y))),
    );
    transfer(scene, stage, curve, plot, pal);

    let mut ladder = [0.0f32; 9];
    stage.harmonics(view.probe_dbfs(), &mut ladder);
    let (even, odd) =
        ladder
            .iter()
            .enumerate()
            .skip(1)
            .fold((0.0f32, 0.0f32), |(e, o), (k, &m)| {
                if k % 2 == 1 {
                    (m.mul_add(m, e), o)
                } else {
                    (e, m.mul_add(m, o))
                }
            });

    let pad = 6.0;
    let balance_h = 6.0;
    let bars = Rect::new(
        right.x0 + pad,
        right.y0 + pad * 2.0 + balance_h,
        right.x1 - pad,
        right.y1 - pad,
    );
    harmonic_bars(scene, bars, pal, &ladder, |order| order % 2 == 0);

    // Even against odd, as a bar: the warmer the stage, the further right.
    let share = if even + odd > 1.0e-12 {
        f64::from(even / (even + odd))
    } else {
        0.0
    };
    let track = Rect::new(
        right.x0 + pad,
        right.y0 + pad,
        right.x1 - pad,
        right.y0 + pad + balance_h,
    );
    fill(
        scene,
        fade(pal.ink, 0.10),
        &RoundedRect::from_rect(track, 3.0),
    );
    let lit = Rect::new(
        track.x0,
        track.y0,
        track.width().mul_add(share, track.x0),
        track.y1,
    );
    fill(
        scene,
        fade(pal.bright, 0.85),
        &RoundedRect::from_rect(lit, 3.0),
    );
}

/// Tape: the curve with the loop a real low note traces round it, the knee
/// in the level domain, and the shelf that decides which frequencies are
/// printed hotter.
fn tape(
    scene: &mut Scene,
    view: &SatView,
    stage: &Stage,
    curve: &[(f32, f32)],
    plot: Plot,
    right: Rect,
    pal: Palette,
) {
    // The loop: one cycle of a 60 Hz note at the probe level, run through
    // the whole stage after it has settled. Where it opens away from the
    // static curve is everything the curve cannot hold — the sag, the
    // emphasis pair, the coupling.
    let amplitude = 10.0_f32.powf(view.probe_dbfs().max(-24.0) / 20.0).max(0.25);
    let trace = trajectory(stage, 60.0, amplitude);
    let loop_path = polyline(trace.iter().map(|&(x, y)| plot.point(x, y)));
    fill(scene, fade(pal.base, 0.14), &loop_path);
    stroke(scene, 1.2, fade(pal.base, 0.75), &loop_path);
    transfer(scene, stage, curve, plot, pal);

    let pad = 6.0;
    let gap = 6.0;
    let knee_h = 2.0f64.mul_add(-pad, right.height() - gap) * 0.62;
    let knee = Rect::new(
        right.x0 + pad,
        right.y0 + pad,
        right.x1 - pad,
        right.y0 + pad + knee_h,
    );
    let shelf = Rect::new(
        right.x0 + pad,
        knee.y1 + gap,
        right.x1 - pad,
        right.y1 - pad,
    );
    level_knee(scene, stage, knee, pal);
    emphasis_strip(scene, stage, shelf, pal);
}

/// One cycle of a sine at `freq_hz` through the real stage, after a quarter
/// second of settling, as (input, heard output) pairs.
fn trajectory(stage: &Stage, freq_hz: f32, amplitude: f32) -> Vec<(f32, f32)> {
    let mut pre = stage.pre.clone();
    pre.mix = 1.0;
    pre.output_gain = 1.0;
    pre.reset();
    let period = num::f32_to_index(SAMPLE_RATE / freq_hz.max(1.0)).max(8);
    let settle = num::f32_to_index(SAMPLE_RATE * 0.25);
    let step = core::f32::consts::TAU * freq_hz / SAMPLE_RATE;
    let mut phase = 0.0f32;
    let mut out = Vec::with_capacity(period.saturating_add(1));
    for i in 0..settle.saturating_add(period).saturating_add(1) {
        let x = amplitude * phase.sin();
        let wet = pre.process(0, x);
        if i >= settle {
            out.push((x, stage.heard(x, wet)));
        }
        phase = (phase + step) % core::f32::consts::TAU;
    }
    out
}

/// Output level against input level, measured through the real stage: the
/// soft knee where a tape stops getting louder and starts getting denser.
fn level_knee(scene: &mut Scene, stage: &Stage, rect: Rect, pal: Palette) {
    const LO: f32 = -36.0;
    const HI: f32 = 0.0;
    const STEPS: u16 = 16;
    let to_point = |input_db: f32, output_db: f32| {
        Point::new(
            rect.width().mul_add(
                unit(f64::from(input_db), f64::from(LO), f64::from(HI)),
                rect.x0,
            ),
            rect.height().mul_add(
                -unit(f64::from(output_db), f64::from(LO), f64::from(HI)),
                rect.y1,
            ),
        )
    };
    // Unity, for something to bend away from.
    dashed(
        scene,
        0.8,
        fade(pal.ink, 0.25),
        &Line::new(to_point(LO, LO), to_point(HI, HI)),
    );
    let points: Vec<Point> = (0..STEPS)
        .map(|i| {
            let input_db = (HI - LO).mul_add(f32::from(i) / f32::from(STEPS - 1), LO);
            to_point(input_db, rms_db_through(stage, input_db))
        })
        .collect();
    stroke(
        scene,
        1.8,
        fade(pal.bright, 0.9),
        &polyline(points.iter().copied()),
    );
    for p in points {
        fill(scene, fade(pal.base, 0.8), &Circle::new(p, 1.6));
    }
}

/// The heard RMS of a 200 Hz sine at `input_dbfs` peak, in dB relative to
/// the same reference — sag and all, after the stage has settled.
fn rms_db_through(stage: &Stage, input_dbfs: f32) -> f32 {
    let mut pre = stage.pre.clone();
    pre.mix = 1.0;
    pre.output_gain = 1.0;
    pre.reset();
    let amplitude = 10.0_f32.powf(input_dbfs / 20.0);
    // 200 Hz: a whole number of cycles in 4800 samples.
    let step = core::f32::consts::TAU * 200.0 / SAMPLE_RATE;
    let (mut sum_in, mut sum_out) = (0.0f32, 0.0f32);
    let mut phase = 0.0f32;
    for i in 0..9600_u16 {
        let x = amplitude * phase.sin();
        let y = stage.heard(x, pre.process(0, x));
        if i >= 4800 {
            sum_in = x.mul_add(x, sum_in);
            sum_out = y.mul_add(y, sum_out);
        }
        phase = (phase + step) % core::f32::consts::TAU;
    }
    // Reported against the input's peak scale, so unity is the diagonal.
    10.0f32.mul_add(
        (sum_out.max(1.0e-12) / sum_in.max(1.0e-12)).log10(),
        input_dbfs,
    )
}

/// Log-frequency position, 20 Hz–20 kHz, as 0..=1.
fn freq_unit(freq_hz: f64) -> f64 {
    unit(
        freq_hz.max(1.0).log10(),
        20.0_f64.log10(),
        20_000.0_f64.log10(),
    )
}

/// The frequency at log position `u`.
fn freq_at(u: f64) -> f64 {
    10.0_f64.powf(3.0_f64.mul_add(u, 20.0_f64.log10()))
}

/// Faint gridlines at the decades a mix engineer navigates by.
fn freq_grid(scene: &mut Scene, rect: Rect, pal: Palette) {
    for f in [
        50.0, 100.0, 200.0, 500.0, 1_000.0, 2_000.0, 5_000.0, 10_000.0,
    ] {
        let x = rect.width().mul_add(freq_unit(f), rect.x0);
        let strong = (f - 100.0_f64).abs() < 1.0
            || (f - 1_000.0_f64).abs() < 1.0
            || (f - 10_000.0_f64).abs() < 1.0;
        stroke(
            scene,
            1.0,
            fade(pal.ink, if strong { 0.14 } else { 0.07 }),
            &Line::new(Point::new(x, rect.y0), Point::new(x, rect.y1)),
        );
    }
}

/// The emphasis shelf across the band: above the line, that frequency hits
/// the knee harder than it arrived.
fn emphasis_strip(scene: &mut Scene, stage: &Stage, rect: Rect, pal: Palette) {
    let span = f64::from(crate::preamp::TILT_MAX_DB);
    let mid = rect.height().mul_add(0.5, rect.y0);
    freq_grid(scene, rect, pal);
    stroke(
        scene,
        0.8,
        fade(pal.ink, 0.25),
        &Line::new(Point::new(rect.x0, mid), Point::new(rect.x1, mid)),
    );
    let points: Vec<Point> = (0..=96_u16)
        .map(|i| {
            let u = f64::from(i) / 96.0;
            let gain = f64::from(analysis::emphasis_db(
                &stage.pre,
                num::f64_to_f32(freq_at(u)),
            ));
            Point::new(
                rect.width().mul_add(u, rect.x0),
                (rect.height() * 0.5).mul_add(-(gain / span).clamp(-1.0, 1.0), mid),
            )
        })
        .collect();
    let mut area = polyline(points.iter().copied());
    area.line_to(Point::new(rect.x1, mid));
    area.line_to(Point::new(rect.x0, mid));
    area.close_path();
    fill(scene, fade(pal.base, 0.25), &area);
    stroke(scene, 1.6, fade(pal.bright, 0.9), &polyline(points));
}

/// Probe frequencies for the bloom: low strings to presence. Capped at
/// 4 kHz so the fifth harmonic the measure reads stays under Nyquist.
const BLOOM_FREQS: [f32; 11] = [
    30.0, 50.0, 80.0, 130.0, 200.0, 320.0, 500.0, 800.0, 1_300.0, 2_200.0, 4_000.0,
];

/// How far under the probe level the bloom is measured. A core driven flat
/// out saturates every frequency alike and the bloom disappears into the
/// ceiling; program sits ~12 dB under its peaks, near the knee, and that is
/// where which band reaches it first is audible.
const BLOOM_UNDER_PEAK_DB: f32 = 12.0;

/// Transformer: the curve, and distortion measured across frequency — the
/// core saturating on the lows, which is the bloom.
fn core(
    scene: &mut Scene,
    view: &SatView,
    stage: &Stage,
    curve: &[(f32, f32)],
    plot: Plot,
    right: Rect,
    pal: Palette,
) {
    transfer(scene, stage, curve, plot, pal);

    let pad = 6.0;
    let rect = inset(right, pad);
    freq_grid(scene, rect, pal);
    let level = (view.probe_dbfs() - BLOOM_UNDER_PEAK_DB).max(-48.0);
    // THD, H2..H5 against H1, at each probe frequency.
    let thd: Vec<(f64, f64)> = BLOOM_FREQS
        .iter()
        .map(|&f| {
            let mut ladder = [0.0f32; 5];
            analysis::harmonic_spectrum_at_freq(&stage.pre, f, level, &mut ladder);
            let added: f32 = ladder.iter().skip(1).map(|m| m * m).sum();
            let height = unit(f64::from(db(added.sqrt())), -80.0, 0.0);
            (freq_unit(f64::from(f)), height)
        })
        .collect();
    let to_point = |(u, v): (f64, f64)| {
        Point::new(
            rect.width().mul_add(u, rect.x0),
            rect.height().mul_add(-v, rect.y1),
        )
    };
    // The bloom: filled, so its weight sits where the core is working.
    let mut area = BezPath::new();
    if let Some(&(u0, _)) = thd.first() {
        area.move_to(to_point((u0, 0.0)));
    }
    for &p in &thd {
        area.line_to(to_point(p));
    }
    if let Some(&(u1, _)) = thd.last() {
        area.line_to(to_point((u1, 0.0)));
    }
    area.close_path();
    let bloom = Gradient::new_linear(Point::new(rect.x0, rect.y0), Point::new(rect.x0, rect.y1))
        .with_stops([
            ColorStop::from((0.0, fade(pal.bright, 0.55))),
            ColorStop::from((1.0, fade(pal.base, 0.08))),
        ]);
    scene.fill(Fill::NonZero, Affine::IDENTITY, &bloom, None, &area);
    stroke(
        scene,
        1.8,
        fade(pal.bright, 0.95),
        &polyline(thd.iter().map(|&p| to_point(p))),
    );
    for &p in &thd {
        fill(scene, pal.bright, &Circle::new(to_point(p), 2.0));
    }

    // The emphasis that causes it, as a line over the top: lows pushed in.
    let span = f64::from(crate::preamp::TILT_MAX_DB);
    let mid = rect.height().mul_add(0.5, rect.y0);
    let shelf = polyline((0..=64_u16).map(|i| {
        let u = f64::from(i) / 64.0;
        let gain = f64::from(analysis::emphasis_db(
            &stage.pre,
            num::f64_to_f32(freq_at(u)),
        ));
        Point::new(
            rect.width().mul_add(u, rect.x0),
            (rect.height() * 0.5).mul_add(-(gain / span).clamp(-1.0, 1.0), mid),
        )
    }));
    dashed(scene, 1.0, fade(pal.ink, 0.5), &shelf);
}

/// Transistor: the curve with its corners and its crossover deadband marked,
/// a sine squared off by it, and the odd comb that squaring makes.
fn solid(
    scene: &mut Scene,
    view: &SatView,
    stage: &Stage,
    curve: &[(f32, f32)],
    plot: Plot,
    right: Rect,
    pal: Palette,
) {
    // The deadband: the run around zero where nothing comes out.
    let dead = curve
        .iter()
        .filter(|&&(x, y)| x >= 0.0 && y.abs() < 1.0e-4)
        .map(|&(x, _)| x)
        .fold(0.0f32, f32::max);
    if dead > 0.0 {
        let band = Rect::from_points(plot.point(-dead, -1.0), plot.point(dead, 1.0));
        fill(scene, fade(pal.base, 0.22), &band);
    }
    transfer(scene, stage, curve, plot, pal);

    // The corners: where the slope has fallen to a third of its steepest.
    // Read off the curve, not off the knobs, so the marker sits wherever
    // the stage actually gives up.
    let slopes: Vec<(f32, f32, f32)> = curve
        .windows(2)
        .filter_map(|pair| match pair {
            [(x0, y0), (x1, y1)] if x1 > x0 => Some((*x0, *y0, (y1 - y0) / (x1 - x0))),
            _ => None,
        })
        .collect();
    let steepest = slopes
        .iter()
        .map(|&(_, _, s)| s.abs())
        .fold(0.0f32, f32::max);
    if steepest > 1.0e-6 {
        let flat = steepest / 3.0;
        let pos = slopes.iter().find(|&&(x, _, s)| x > dead && s.abs() < flat);
        let neg = slopes
            .iter()
            .rev()
            .find(|&&(x, _, s)| x < -dead && s.abs() < flat);
        for &(x, y, _) in pos.into_iter().chain(neg) {
            let p = plot.point(x, y);
            stroke(scene, 1.4, fade(pal.bright, 0.9), &Circle::new(p, 5.0));
        }
    }

    let pad = 6.0;
    let gap = 6.0;
    let wave_h = 2.0f64.mul_add(-pad, right.height() - gap) * 0.55;
    let wave = Rect::new(
        right.x0 + pad,
        right.y0 + pad,
        right.x1 - pad,
        right.y0 + pad + wave_h,
    );
    let comb = Rect::new(
        right.x0 + pad,
        wave.y1 + gap,
        right.x1 - pad,
        right.y1 - pad,
    );

    // Two cycles of a sine at the probe level through the heard curve.
    let amplitude = 10.0_f32.powf(view.probe_dbfs() / 20.0);
    let mid = wave.height().mul_add(0.5, wave.y0);
    let half = wave.height() * 0.45;
    let mut quantiser = stage.digital.clone();
    quantiser.rate = 1.0;
    quantiser.dither = 0.0;
    let makeup = stage.pre.makeup_gain();
    let mut ghost = Vec::new();
    let mut clipped = Vec::new();
    for i in 0..=200_u16 {
        let u = f64::from(i) / 200.0;
        let x = amplitude * num::f64_to_f32(u * core::f64::consts::TAU * 2.0).sin();
        let y = stage.heard(x, quantiser.process(0, stage.pre.transfer(x) * makeup));
        let px = wave.width().mul_add(u, wave.x0);
        ghost.push(Point::new(px, half.mul_add(-f64::from(x), mid)));
        clipped.push(Point::new(
            px,
            half.mul_add(-f64::from(y.clamp(-1.2, 1.2)), mid),
        ));
    }
    dashed(scene, 0.8, fade(pal.ink, 0.3), &polyline(ghost));
    stroke(scene, 1.8, fade(pal.bright, 0.95), &polyline(clipped));

    let mut ladder = [0.0f32; 9];
    stage.harmonics(view.probe_dbfs(), &mut ladder);
    harmonic_bars(scene, comb, pal, &ladder, |order| order % 2 == 1);
}

/// Digital: the staircase, and a sine through the converter — held, stepped
/// and stopped at the ceiling.
fn steps(
    scene: &mut Scene,
    view: &SatView,
    stage: &Stage,
    curve: &[(f32, f32)],
    plot: Plot,
    right: Rect,
    pal: Palette,
) {
    // The ceiling the clipper stops at, as a pair of rails.
    let peak = curve.iter().map(|&(_, y)| y.abs()).fold(0.0f32, f32::max);
    for rail in [peak, -peak] {
        dashed(
            scene,
            0.9,
            fade(pal.base, 0.6),
            &Line::new(plot.point(-1.0, rail), plot.point(1.0, rail)),
        );
    }
    transfer(scene, stage, curve, plot, pal);

    let rect = inset(right, 6.0);
    let mid = rect.height().mul_add(0.5, rect.y0);
    let half = rect.height() * 0.42;
    let amplitude = 10.0_f32.powf(view.probe_dbfs().max(-12.0) / 20.0);
    // One and a half cycles across the pane, slow enough that the holds read
    // as steps: 1.5 cycles in `span` samples.
    let span: u16 = 192;
    let cycles = 1.5_f32;
    let mut converter = stage.digital.clone();
    converter.reset();
    let makeup = stage.pre.makeup_gain();
    let mut ghost = Vec::new();
    let mut held = BezPath::new();
    let mut last_y: Option<f64> = None;
    let mut holds = Vec::new();
    for i in 0..=span {
        let u = f32::from(i) / f32::from(span);
        let x = amplitude * (core::f32::consts::TAU * cycles * u).sin();
        let y = stage.heard(x, converter.process(0, stage.pre.transfer(x) * makeup));
        let px = rect.width().mul_add(f64::from(u), rect.x0);
        let py = half.mul_add(-f64::from(y.clamp(-1.2, 1.2)), mid);
        ghost.push(Point::new(px, half.mul_add(-f64::from(x), mid)));
        match last_y {
            None => held.move_to(Point::new(px, py)),
            Some(prev) => {
                // Zero-order hold: across at the old value, then up.
                held.line_to(Point::new(px, prev));
                if (prev - py).abs() > 1.0e-9 {
                    holds.push(px);
                }
                held.line_to(Point::new(px, py));
            }
        }
        last_y = Some(py);
    }
    // Where a new sample was latched, ticked along the floor.
    for px in holds {
        stroke(
            scene,
            1.0,
            fade(pal.base, 0.45),
            &Line::new(Point::new(px, rect.y1 - 5.0), Point::new(px, rect.y1)),
        );
    }
    for rail in [f64::from(peak), -f64::from(peak)] {
        let y = half.mul_add(-rail.clamp(-1.2, 1.2), mid);
        dashed(
            scene,
            0.9,
            fade(pal.base, 0.6),
            &Line::new(Point::new(rect.x0, y), Point::new(rect.x1, y)),
        );
    }
    dashed(scene, 0.8, fade(pal.ink, 0.3), &polyline(ghost));
    stroke(scene, 1.8, fade(pal.bright, 0.95), &held);
}

// ── Glyphs ──────────────────────────────────────────────────────────────────

/// A circuit in one glyph, `size` across, centred on `c`: a lit valve, tape
/// over a head, an iron core, a transistor's corner, a staircase. What the
/// multiband view labels its bands with — a band too narrow for a word still
/// has room for one of these.
fn glyph(scene: &mut Scene, circuit: Circuit, c: Point, size: f64, color: Color) {
    let s = size * 0.5;
    let at = |dx: f64, dy: f64| Point::new(s.mul_add(dx, c.x), s.mul_add(dy, c.y));
    let line_w = (size * 0.09).max(1.0);
    match circuit {
        Circuit::Valve => {
            let envelope = RoundedRect::from_points(at(-0.55, -0.95), at(0.55, 0.75), s * 0.5);
            stroke(scene, line_w, color, &envelope);
            stroke(
                scene,
                line_w,
                color,
                &polyline([at(-0.3, 0.5), at(0.0, -0.4), at(0.3, 0.5)]),
            );
            fill(
                scene,
                color,
                &Rect::from_points(at(-0.6, 0.78), at(0.6, 0.98)),
            );
        }
        Circuit::Tape => {
            stroke(
                scene,
                line_w,
                color,
                &Circle::new(at(-0.5, -0.25), s * 0.38),
            );
            stroke(scene, line_w, color, &Circle::new(at(0.5, -0.25), s * 0.38));
            stroke(
                scene,
                line_w,
                color,
                &polyline([
                    at(-0.88, -0.2),
                    at(-0.6, 0.75),
                    at(0.6, 0.75),
                    at(0.88, -0.2),
                ]),
            );
            fill(
                scene,
                color,
                &Rect::from_points(at(-0.14, 0.6), at(0.14, 0.95)),
            );
        }
        Circuit::Core => {
            stroke(
                scene,
                line_w,
                color,
                &Rect::from_points(at(-0.8, -0.8), at(0.8, 0.8)),
            );
            for k in [-0.4, 0.0, 0.4] {
                stroke(
                    scene,
                    line_w,
                    color,
                    &polyline([at(-0.95, k - 0.15), at(0.95, k + 0.15)]),
                );
            }
        }
        Circuit::Solid => {
            stroke(
                scene,
                line_w * 1.3,
                color,
                &polyline([
                    at(-0.95, 0.75),
                    at(-0.4, 0.75),
                    at(0.4, -0.75),
                    at(0.95, -0.75),
                ]),
            );
        }
        Circuit::Steps => {
            let mut stair = BezPath::new();
            stair.move_to(at(-0.9, 0.85));
            for k in 0..4_u8 {
                let x = f64::from(k).mul_add(0.45, -0.9);
                let y = f64::from(k).mul_add(-0.45, 0.85);
                stair.line_to(at(x + 0.45, y));
                stair.line_to(at(x + 0.45, y - 0.45));
            }
            stroke(scene, line_w * 1.2, color, &stair);
        }
    }
}

// ── Multiband ───────────────────────────────────────────────────────────────

/// One band of a multiband saturator.
#[derive(Clone, Debug, PartialEq)]
pub struct BandView {
    /// The band's circuit and its knobs — the same pair a single stage takes.
    pub profile: Profile,
    pub controls: Controls,
    /// Band output level, dB.
    pub level_db: f32,
    pub mute: bool,
    pub solo: bool,
    /// The band's recent spectrum, dB (0 = full scale), on log-spaced bins
    /// from 20 Hz to 20 kHz. Empty draws none.
    pub spectrum: Vec<f32>,
}

impl BandView {
    /// A band running `profile_id` at noon, or `None` for an unknown id.
    #[must_use]
    pub fn new(profile_id: &str) -> Option<Self> {
        crate::profiles::profile_by_id(profile_id).map(|profile| Self {
            profile: *profile,
            controls: Controls::default(),
            level_db: 0.0,
            mute: false,
            solo: false,
            spectrum: Vec::new(),
        })
    }

    const fn sat(&self) -> SatView {
        SatView::of(self.profile, self.controls)
    }
}

/// A multiband saturator, Saturn-style: the spectrum split at crossovers,
/// a circuit per band.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MultibandSatView {
    /// Crossover frequencies, Hz, low to high. `bands.len() − 1` of them.
    pub crossovers: Vec<f32>,
    /// Low band first. At most [`multiband_dsp::MAX_BANDS`] are drawn — the
    /// splitter carries no more.
    pub bands: Vec<BandView>,
    /// The band being edited, outlined.
    pub selected: Option<usize>,
    pub on: bool,
}

/// One band's LR4 magnitude at `freq_hz`: the high-pass of the crossover
/// below it times the low-pass of the one above.
///
/// `multiband-dsp` splits with Linkwitz–Riley 4th-order crossovers (two
/// cascaded Butterworth-2 sections each side) and its allpass compensation
/// is magnitude-flat, so a band's magnitude is exactly this product. The
/// analogue prototype is used — |LP| = 1/(1 + (f/fc)⁴), |HP| = (f/fc)⁴/(1 +
/// (f/fc)⁴) — which the bilinear design matches everywhere below the top
/// octave the axis ends in.
#[must_use]
pub fn lr4_band_gain(freq_hz: f64, below: Option<f64>, above: Option<f64>) -> f64 {
    let hp = below.map_or(1.0, |fc| {
        let r = (freq_hz / fc.max(1.0)).powi(4);
        r / (1.0 + r)
    });
    let lp = above.map_or(1.0, |fc| 1.0 / (1.0 + (freq_hz / fc.max(1.0)).powi(4)));
    hp * lp
}

/// The bands a view actually draws, with their edges: `(band, below, above)`.
fn band_edges(view: &MultibandSatView) -> Vec<(&BandView, Option<f64>, Option<f64>)> {
    let mut cuts: Vec<f64> = view
        .crossovers
        .iter()
        .map(|&f| f64::from(f).clamp(20.0, 20_000.0))
        .collect();
    cuts.sort_by(f64::total_cmp);
    let count = view
        .bands
        .len()
        .min(cuts.len().saturating_add(1))
        .min(multiband_dsp::MAX_BANDS);
    view.bands
        .iter()
        .take(count)
        .enumerate()
        .map(|(i, band)| {
            let below = i.checked_sub(1).and_then(|j| cuts.get(j)).copied();
            let above = if i.saturating_add(1) < count {
                cuts.get(i).copied()
            } else {
                None
            };
            (band, below, above)
        })
        .collect()
}

/// Draw a multiband saturator, `w` × `h`.
pub fn paint_multiband_sat(scene: &mut Scene, view: &MultibandSatView, w: f64, h: f64) {
    let ink = Color::from_rgba8(206, 212, 222, 255);
    let rect = Rect::new(0.0, 0.0, w.max(0.0), h.max(0.0));
    fill(scene, Color::from_rgba8(14, 16, 21, 255), &rect);
    if w < 8.0 || h < 8.0 {
        return;
    }
    let field = inset(rect, 4.0);
    let grid = Palette {
        base: ink,
        bright: ink,
        ink,
    };
    freq_grid(scene, field, grid);

    let bands = band_edges(view);
    let any_solo = bands.iter().any(|(b, _, _)| b.solo);

    for (index, &(band, below, above)) in bands.iter().enumerate() {
        let slot = BandSlot {
            band,
            below,
            above,
            audible: view.on && !band.mute && (!any_solo || band.solo),
            selected: view.selected == Some(index),
        };
        paint_band(scene, field, slot);
    }

    // The crossovers, over everything: a line and a handle each.
    for &(_, below, _) in bands.iter().skip(1) {
        let Some(f) = below else { continue };
        let x = field_x(field, f);
        stroke(
            scene,
            1.2,
            fade(ink, 0.55),
            &Line::new(Point::new(x, field.y0), Point::new(x, field.y1)),
        );
        let handle = Point::new(x, field.height().mul_add(-0.12, field.y1));
        fill(
            scene,
            Color::from_rgba8(14, 16, 21, 255),
            &Circle::new(handle, 6.0),
        );
        stroke(scene, 1.6, fade(ink, 0.9), &Circle::new(handle, 6.0));
        fill(scene, fade(ink, 0.9), &Circle::new(handle, 2.0));
    }
}

/// The x of frequency `f` across `field`.
fn field_x(field: Rect, f: f64) -> f64 {
    field.width().mul_add(freq_unit(f), field.x0)
}

/// One band and how it is to be drawn.
#[derive(Clone, Copy)]
struct BandSlot<'a> {
    band: &'a BandView,
    below: Option<f64>,
    above: Option<f64>,
    /// Not muted, not overruled by another band's solo, and the block on.
    audible: bool,
    selected: bool,
}

/// One band: its region, its drive, its spectrum and response, its label
/// and its curve.
fn paint_band(scene: &mut Scene, field: Rect, slot: BandSlot<'_>) {
    let BandSlot {
        band,
        below,
        above,
        audible,
        selected,
    } = slot;
    let sat = band.sat();
    let pal = Palette::of(sat.circuit(), audible);
    let x0 = below.map_or(field.x0, |f| field_x(field, f));
    let x1 = above.map_or(field.x1, |f| field_x(field, f));
    let region = Rect::new(x0, field.y0, x1, field.y1);

    // The region, washed in its circuit's hue.
    fill(
        scene,
        fade(pal.base, if audible { 0.10 } else { 0.04 }),
        &region,
    );
    if band.mute {
        hatch(scene, region, fade(pal.ink, 0.10));
    }

    // Drive, as a fill rising from the floor.
    let drive = f64::from(band.controls.drive.clamp(0.0, 1.0));
    let fill_top = field.height().mul_add(-drive * 0.85, field.y1);
    let drive_fill = Gradient::new_linear(Point::new(x0, fill_top), Point::new(x0, field.y1))
        .with_stops([
            ColorStop::from((0.0, fade(pal.base, if audible { 0.42 } else { 0.14 }))),
            ColorStop::from((1.0, fade(pal.base, 0.04))),
        ]);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &drive_fill,
        None,
        &Rect::new(x0, fill_top, x1, field.y1),
    );
    stroke(
        scene,
        1.4,
        fade(pal.bright, if audible { 0.8 } else { 0.3 }),
        &Line::new(Point::new(x0, fill_top), Point::new(x1, fill_top)),
    );

    // The band's recent spectrum, behind its response.
    if !band.spectrum.is_empty() {
        spectrum_line(
            scene,
            field,
            &band.spectrum,
            fade(pal.bright, if audible { 0.5 } else { 0.18 }),
        );
    }

    // Its LR4 response across the whole axis, lifted by its level.
    let level = f64::from(band.level_db);
    let response = polyline((0..=160_u16).map(|i| {
        let u = f64::from(i) / 160.0;
        let gain = lr4_band_gain(freq_at(u), below, above).max(1.0e-6);
        let gain_db = 20.0f64.mul_add(gain.log10(), level);
        Point::new(
            field.width().mul_add(u, field.x0),
            // +12 dB at the top of the field, −36 at the floor.
            field
                .height()
                .mul_add(-unit(gain_db, -36.0, 12.0), field.y1),
        )
    }));
    if band.mute {
        dashed(scene, 1.2, fade(pal.base, 0.5), &response);
    } else {
        stroke(
            scene,
            1.6,
            fade(pal.bright, if audible { 0.9 } else { 0.35 }),
            &response,
        );
    }

    // Label and thumbnail, where there is room for them.
    let region_w = x1 - x0;
    let centre = region_w.mul_add(0.5, x0);
    let glyph_size = (region_w * 0.5).min(field.height() * 0.13).min(22.0);
    if glyph_size >= 7.0 {
        glyph(
            scene,
            sat.circuit(),
            Point::new(centre, field.y0 + glyph_size * 0.5 + 6.0),
            glyph_size,
            fade(pal.bright, if audible { 0.95 } else { 0.4 }),
        );
    }
    let thumb = (region_w * 0.62).min(field.height() * 0.34).min(90.0);
    if thumb >= 18.0 {
        let top = field.y0 + glyph_size.max(0.0) + 14.0;
        let box_rect = Rect::new(centre - thumb * 0.5, top, centre + thumb * 0.5, top + thumb);
        thumbnail(scene, &sat, box_rect, pal, audible);
    }

    if selected {
        stroke(
            scene,
            1.6,
            fade(pal.bright, 0.9),
            &RoundedRect::from_rect(inset(region, 1.0), 3.0),
        );
    }
}

/// Diagonal lines across a muted band.
fn hatch(scene: &mut Scene, region: Rect, color: Color) {
    const STEP: f64 = 9.0;
    // One line per step along the floor, starting a full height to the left
    // so the region's left edge is covered too.
    let count = num::f64_to_u32(((region.width() + region.height()) / STEP).ceil()).min(4_000);
    for k in 0..count {
        let x = STEP.mul_add(f64::from(k), region.x0 - region.height());
        let start = x.max(region.x0);
        let end = (x + region.height()).min(region.x1);
        if end > start {
            let a = Point::new(start, region.y1 - (start - x));
            let b = Point::new(end, region.y1 - (end - x));
            stroke(scene, 1.0, color, &Line::new(a, b));
        }
    }
}

/// A spectrum on log-spaced bins across the field, 0 dB at the top and
/// −90 at the floor.
fn spectrum_line(scene: &mut Scene, field: Rect, bins: &[f32], color: Color) {
    let last = num::count_to_f64(bins.len().saturating_sub(1).max(1));
    let points: Vec<Point> = bins
        .iter()
        .enumerate()
        .map(|(i, &mag)| {
            let u = num::count_to_f64(i) / last;
            Point::new(
                field.width().mul_add(u, field.x0),
                field
                    .height()
                    .mul_add(-unit(f64::from(mag), -90.0, 0.0), field.y1),
            )
        })
        .collect();
    let mut area = polyline(points.iter().copied());
    area.line_to(Point::new(field.x1, field.y1));
    area.line_to(Point::new(field.x0, field.y1));
    area.close_path();
    fill(scene, color.multiply_alpha(0.3), &area);
    stroke(scene, 1.0, color, &polyline(points));
}

/// A band's transfer curve, small.
fn thumbnail(scene: &mut Scene, sat: &SatView, rect: Rect, pal: Palette, audible: bool) {
    fill(
        scene,
        Color::from_rgba8(10, 12, 16, 200),
        &RoundedRect::from_rect(rect, 3.0),
    );
    frame(scene, rect, pal);
    let plot = Plot {
        rect: inset(rect, 3.0),
    };
    plot.axes(scene, pal);
    let curve = sat.stage().heard_curve(96);
    plot.curve(
        scene,
        1.5,
        fade(pal.bright, if audible { 0.95 } else { 0.45 }),
        &curve,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(id: &str, drive: f32) -> SatView {
        let mut v = SatView::new(id).expect("profile");
        v.controls.drive = drive;
        v
    }

    fn painted(v: &SatView, w: f64, h: f64) -> Scene {
        let mut scene = Scene::new();
        paint_sat(&mut scene, v, w, h);
        scene
    }

    /// The circuit a profile draws is the circuit the engine says it is.
    #[test]
    fn profiles_land_on_their_circuits() {
        for (id, circuit) in [
            ("triode", Circuit::Valve),
            ("pentode", Circuit::Valve),
            ("tape", Circuit::Tape),
            ("tape_hot", Circuit::Tape),
            ("transformer", Circuit::Core),
            ("transistor", Circuit::Solid),
            ("fuzz", Circuit::Solid),
            ("clip", Circuit::Steps),
            ("crush", Circuit::Steps),
        ] {
            assert_eq!(view(id, 0.5).circuit(), circuit, "{id}");
        }
    }

    /// Every profile paints, at every size a panel takes — including the
    /// degenerate ones a flex row hands out mid-layout.
    #[test]
    fn every_profile_paints() {
        for p in crate::profiles::PROFILES {
            let v = view(p.id, 0.6);
            for (w, h) in [(0.0, 0.0), (3.0, 3.0), (140.0, 48.0), (760.0, 190.0)] {
                let scene = painted(&v, w, h);
                assert!(!scene.commands.is_empty(), "{} at {w}x{h}", p.id);
            }
            assert!(
                painted(&v, 760.0, 190.0).commands.len() > 20,
                "{} drew almost nothing",
                p.id
            );
        }
    }

    /// Turning drive changes the picture, on every circuit.
    #[test]
    fn drive_changes_every_picture() {
        for p in crate::profiles::PROFILES {
            let low = painted(&view(p.id, 0.1), 760.0, 190.0);
            let high = painted(&view(p.id, 0.9), 760.0, 190.0);
            assert_ne!(low, high, "{}: drive moved nothing", p.id);
        }
    }

    /// Bias, mix, output and the live level each move the picture too.
    #[test]
    fn the_other_controls_move_it() {
        let base = view("triode", 0.5);
        let reference = painted(&base, 760.0, 190.0);
        let mut biased = base;
        biased.controls.bias = 0.9;
        let mut dry = base;
        dry.controls.mix = 0.4;
        let mut trimmed = base;
        trimmed.output_db = -6.0;
        let mut live = base;
        live.level = Some(0.7);
        for (what, v) in [
            ("bias", biased),
            ("mix", dry),
            ("output", trimmed),
            ("level", live),
        ] {
            assert_ne!(painted(&v, 760.0, 190.0), reference, "{what} moved nothing");
        }
    }

    /// The bits knob reaches the staircase.
    #[test]
    fn word_length_changes_the_staircase() {
        let mut coarse = view("crush", 0.5);
        coarse.controls.character_a = 0.2;
        let mut fine = coarse;
        fine.controls.character_a = 0.8;
        assert_ne!(painted(&coarse, 760.0, 190.0), painted(&fine, 760.0, 190.0));
    }

    /// The valve's ladder lights its evens because the engine makes them:
    /// a biased triode's H2 is well above the floor.
    #[test]
    fn a_triode_is_measurably_even() {
        let stage = view("triode", 0.5).stage();
        let mut ladder = [0.0f32; 4];
        stage.harmonics(DEFAULT_PROBE_DBFS, &mut ladder);
        assert!(db(ladder[1]) > -50.0, "H2 = {} dB", db(ladder[1]));
    }

    /// The transformer saturates its lows harder than its mids — the bloom
    /// the panel draws is in the engine.
    #[test]
    fn a_transformer_blooms_low() {
        let stage = view("transformer", 0.7).stage();
        let thd = |f: f32| {
            let mut ladder = [0.0f32; 6];
            analysis::harmonic_spectrum_at_freq(&stage.pre, f, DEFAULT_PROBE_DBFS, &mut ladder);
            ladder.iter().skip(1).map(|m| m * m).sum::<f32>().sqrt()
        };
        assert!(
            thd(40.0) > thd(2_000.0),
            "40 Hz {} vs 2 kHz {}",
            thd(40.0),
            thd(2_000.0)
        );
    }

    fn multiband() -> MultibandSatView {
        MultibandSatView {
            crossovers: vec![180.0, 1_500.0, 6_000.0],
            bands: ["transformer", "triode", "tape", "crush"]
                .iter()
                .map(|id| BandView::new(id).expect("profile"))
                .collect(),
            selected: Some(1),
            on: true,
        }
    }

    fn painted_mb(v: &MultibandSatView, w: f64, h: f64) -> Scene {
        let mut scene = Scene::new();
        paint_multiband_sat(&mut scene, v, w, h);
        scene
    }

    #[test]
    fn multiband_paints_at_every_size() {
        let v = multiband();
        for (w, h) in [(0.0, 0.0), (5.0, 5.0), (160.0, 60.0), (900.0, 260.0)] {
            assert!(!painted_mb(&v, w, h).commands.is_empty());
        }
        assert!(
            painted_mb(&MultibandSatView::default(), 400.0, 120.0)
                .commands
                .len()
                > 1
        );
    }

    #[test]
    fn multiband_moves_with_its_controls() {
        let base = multiband();
        let reference = painted_mb(&base, 900.0, 260.0);
        let mut driven = base.clone();
        driven.bands[2].controls.drive = 0.95;
        let mut moved = base.clone();
        moved.crossovers[1] = 2_400.0;
        let mut muted = base.clone();
        muted.bands[0].mute = true;
        let mut soloed = base.clone();
        soloed.bands[3].solo = true;
        let mut louder = base.clone();
        louder.bands[1].level_db = 6.0;
        let mut heard = base;
        heard.bands[1].spectrum = (0..64_u8)
            .map(|i| f32::from(i).mul_add(-1.0, -20.0))
            .collect();
        for (what, v) in [
            ("drive", driven),
            ("crossover", moved),
            ("mute", muted),
            ("solo", soloed),
            ("level", louder),
            ("spectrum", heard),
        ] {
            assert_ne!(
                painted_mb(&v, 900.0, 260.0),
                reference,
                "{what} moved nothing"
            );
        }
    }

    /// A band's response is the LR4 product: −6 dB at its crossovers, flat
    /// in its middle, and the bands sum to unity amplitude.
    #[test]
    fn lr4_bands_meet_at_minus_six() {
        let at_cut = lr4_band_gain(1_000.0, None, Some(1_000.0));
        assert!(20.0f64.mul_add(at_cut.log10(), 6.02).abs() < 0.05);
        let mid = lr4_band_gain(1_000.0, Some(100.0), Some(10_000.0));
        assert!(mid > 0.99);
        for f in [50.0, 300.0, 1_000.0, 3_000.0] {
            let low = lr4_band_gain(f, None, Some(1_000.0));
            let high = lr4_band_gain(f, Some(1_000.0), None);
            assert!((low + high - 1.0).abs() < 1.0e-9, "{f}: {low} + {high}");
        }
    }

    /// More bands than the splitter carries are not drawn.
    #[test]
    fn bands_cap_at_the_splitter() {
        let mut v = multiband();
        v.crossovers = vec![50.0, 100.0, 200.0, 400.0, 800.0, 1_600.0, 3_200.0];
        v.bands = (0..8)
            .map(|_| BandView::new("tape").expect("profile"))
            .collect();
        assert_eq!(band_edges(&v).len(), multiband_dsp::MAX_BANDS);
    }
}
