//! The painters' mixer strip, written out as a theme.
//!
//! `fts-dark.frame` is not drawn by hand. It is generated from the same
//! sources the painters draw from — `daw_theme_art`'s controls, every
//! state of each, and `strip::Strip`'s geometry written as bindings — so
//! the themed mixer is the painters' mixer to the pixel, and from then on
//! a file anyone can edit. `cargo run -p session-daw --bin theme_export`
//! writes it; `just theme-parity` measures how far it has drifted.
//!
//! # How a drawing becomes nodes
//!
//! A `Drawing` is shapes and text in its own box. Rectangles become
//! `<Rectangle>` (so a width binding can resize one), everything else a
//! `<Vector>` holding the drawing's own path in the drawing's box, text a
//! `<Text>` whose box puts the baseline where the drawing put it (see
//! [`super::canvas::BASELINE`]). Gradients are laid across the node's box
//! the way the drawing laid them across its own.
//!
//! A control drawn several ways — lit or not, hovered, pressed — is every
//! one of those drawings, each shown by a binding on the state it is. A
//! colour the drawing takes from the track (the arm's housing is the
//! strip's ground) is drawn with a sentinel colour and becomes a binding.
//!
//! # How the geometry becomes bindings
//!
//! `strip::Strip` places each control by rules over the strip's width,
//! its height and the mixer's height (which every strip resolves its
//! sections against, so the buttons line up across a nested folder). The
//! rules are written here once more as expressions over `w`, `h` and the
//! layout field `mixer.h`; `Collapse`'s height tables are sampled into
//! piecewise ones. The `parity` test holds the two to the same rects.
//!
//! # The console theme
//!
//! `themes/console/` began as this generator's output and is its own files
//! now, edited as they are; its layout tests are here, run against those
//! files.

use std::fmt::Write as _;

use daw_theme::Color as Tc;
use daw_theme_art::geometry::mcp as g;
use daw_theme_art::mixer_controls::Interaction;
use daw_theme_art::paint::tcp as art;
use daw_theme_art::paint::{Align, Brush, Drawing, Op, Shape};
use vello::kurbo::Shape as _;

use crate::arrangement::Palette;
use crate::tcp::to_theme;

use super::canvas::BASELINE;

pub mod tcp;

/// A colour no control is drawn in, standing for one the track supplies.
const SENTINEL: Tc = Tc {
    r: 1,
    g: 2,
    b: 3,
    a: 255,
};

/// The strip's own width, as `Strip::laid_out` uses it: a strip twice the
/// REAPER width lays its controls out at REAPER's.
const OWN_W: &str = "(w >= 172 ? 86 : w)";

/// The theme, as markup: the painters' strip, REAPER's layout.
#[must_use]
pub fn fts_dark(palette: &Palette) -> String {
    generate(palette)
}

fn generate(palette: &Palette) -> String {
    let mut x = Gen {
        out: String::new(),
        palette,
    };
    x.document();
    x.out
}

struct Gen<'a> {
    out: String,
    palette: &'a Palette,
}

/// Every control's place on the strip, as expressions over the box: the
/// one table the drawing and the hit targets both read, so a layout is a
/// table and a control is hit where it is drawn.
struct Spots {
    band: (String, String),
    input: Spot,
    pan: Spot,
    fx: Spot,
    routing: Spot,
    /// The housing (on a full strip) and the ring.
    arm_mixer: Spot,
    arm_panel: Spot,
    monitor: Spot,
    mute: Spot,
    solo: Spot,
    fader_x: String,
    fader_top: String,
    travel: String,
    scale_x: String,
    scale_visible: String,
}

/// A control's left, top, width and height, and when it shows (empty:
/// always).
struct Spot {
    x: String,
    y: String,
    w: String,
    h: String,
    visible: String,
}

impl Spot {
    fn new(
        x: impl Into<String>,
        y: impl Into<String>,
        w: impl Into<String>,
        h: impl Into<String>,
        visible: impl Into<String>,
    ) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
            w: w.into(),
            h: h.into(),
            visible: visible.into(),
        }
    }
}

/// Formats a number the way markup wants it: exactly, and short.
fn n(v: f64) -> String {
    let v = if v.abs() < 1e-9 { 0.0 } else { v };
    format!("{v}")
}

fn hex(c: Tc) -> String {
    if c.a == 255 {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a)
    }
}

/// An expression in an attribute value.
fn q(expr: &str) -> String {
    serde_json_string(expr)
}

fn serde_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('{', "&lbrace;")
}

/// The state expression for a control named `name` being `at`.
fn state(name: &str, at: Interaction) -> String {
    match at {
        Interaction::Pressed => format!("ui.pressed == \"{name}\""),
        Interaction::Hover => format!("(ui.hover == \"{name}\" && ui.pressed == \"\")"),
        Interaction::Normal => {
            format!("!(ui.pressed == \"{name}\" || (ui.hover == \"{name}\" && ui.pressed == \"\"))")
        }
    }
}

const STATES: [Interaction; 3] = [
    Interaction::Normal,
    Interaction::Hover,
    Interaction::Pressed,
];

/// `values` (sampled at every half pixel of `var` up to `top`) as one
/// expression: the value it has from each threshold up.
fn piecewise(var: &str, top: f64, value: impl Fn(f64) -> String) -> String {
    let mut runs: Vec<(f64, String)> = Vec::new();
    let mut at = 0.0;
    while at <= top {
        let v = value(at);
        if runs.last().is_none_or(|(_, last)| *last != v) {
            runs.push((at, v));
        }
        at += 0.5;
    }
    let mut expr = runs
        .first()
        .map_or_else(|| "0".to_owned(), |(_, v)| v.clone());
    for (from, v) in runs.iter().skip(1) {
        expr = format!("({var} >= {} ? {v} : {expr})", n(*from));
    }
    expr
}

/// How a width-dependent drawing's nodes follow its box: the same drawing
/// at a second width, so each node's place and size can be fitted as a
/// line in the width.
#[derive(Clone, Copy)]
enum Follow<'d> {
    Width { w0: f64, twin: &'d Drawing, w1: f64 },
    Fixed,
}

/// Where a drawing goes and when it shows.
#[derive(Clone, Copy)]
struct Place<'a> {
    name: &'a str,
    /// Its left and top, as expressions.
    x: &'a str,
    y: &'a str,
    /// When it shows (empty: always).
    visible: &'a str,
    follow: Follow<'a>,
    /// The field the drawing's sentinel colour stands for.
    track: &'a str,
    /// Any further attributes for the drawing's frame.
    extra: &'a str,
}

/// `v` at width `w0` and `v1` at `w1`, as an expression in `pw`, when it
/// moves with the width at all.
fn line_in_width(v0: f64, v1: f64, w0: f64, w1: f64) -> Option<String> {
    let a = (v1 - v0) / (w1 - w0);
    (a.abs() > 1e-9).then(|| format!("{} * pw + {}", n(a), n(v0 - a * w0)))
}

impl Gen<'_> {
    fn line(&mut self, depth: usize, text: &str) {
        for _ in 0..depth {
            self.out.push_str("  ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn document(&mut self) {
        self.line(0, "<Document>");
        self.tokens();
        self.line(1, "<Page name=\"mcp.strip\">");
        self.line(2, "<Frame name=\"Strip\" w=86 h=400>");
        self.recorded();
        self.live();
        self.hits();
        self.line(2, "</Frame>");
        self.line(1, "</Page>");
        self.tcp();
        self.editor();
        self.line(0, "</Document>");
    }

    /// The palette as tokens: what a hand edit reaches for. The controls
    /// below are drawn in the colours their drawings were, which are these
    /// and shades of these.
    fn tokens(&mut self) {
        let p = self.palette;
        self.line(1, "<Variables modes=\"dark\">");
        let c = p.chrome;
        let named: [(&str, Tc); 18] = [
            ("text", to_theme(p.text)),
            ("text.dim", to_theme(p.text_dim)),
            ("text.faint", to_theme(p.text_faint)),
            ("accent", to_theme(p.accent)),
            ("mute", to_theme(p.mute)),
            ("solo", to_theme(p.solo)),
            ("rec", to_theme(p.rec)),
            ("meter.safe", to_theme(p.meter_safe)),
            ("meter.warn", to_theme(p.meter_warn)),
            ("meter.danger", to_theme(p.meter_danger)),
            ("field", to_theme(p.tcp_field)),
            ("combo", to_theme(p.tcp_combo)),
            ("hardware", c.hardware),
            ("hardware.edge", c.hardware_edge),
            ("hardware.mark", c.hardware_mark),
            ("surface", c.surface),
            ("surface.sunken", c.surface_sunken),
            ("border", c.border),
        ];
        for (name, color) in named {
            self.line(
                2,
                &format!("<Var name=\"{name}\" value=\"{}\" />", hex(color)),
            );
        }
        self.editor_tokens();
        self.line(1, "</Variables>");
    }

    /// The editor's colours as tokens (`editor.<name>`), as it draws
    /// itself from this palette, and its notes' palettes as they are —
    /// so this theme draws the editor's chrome exactly as the editor
    /// would without it.
    fn editor_tokens(&mut self) {
        let mut look = crate::expression::base_look(self.palette);
        let mut vars: Vec<(String, String)> = crate::expression::look_fields(&mut look)
            .into_iter()
            .map(|(name, c)| (format!("editor.{name}"), hex(to_theme(*c))))
            .collect();
        vars.push(("sheen".to_owned(), "#ffffff12".to_owned()));
        for (i, c) in expression_editor_paint::theme::PITCH_CLASS
            .iter()
            .enumerate()
        {
            vars.push((format!("editor.pitch_{i}"), (*c).to_owned()));
        }
        use expression_editor_core::rows as kit;
        for (name, c) in [
            ("kick", kit::DRUM_KICK_COLOR),
            ("snare", kit::DRUM_SNARE_COLOR),
            ("tom", kit::DRUM_TOM_COLOR),
            ("hat", kit::DRUM_HAT_COLOR),
            ("ride", kit::DRUM_RIDE_COLOR),
            ("cymbal", kit::DRUM_CYMBAL_COLOR),
            ("other", kit::DRUM_OTHER_COLOR),
        ] {
            vars.push((format!("editor.drum_{name}"), c.to_owned()));
        }
        for (name, value) in vars {
            self.line(2, &format!("<Var name=\"{name}\" value=\"{value}\" />"));
        }
    }

    /// The expression editor's components — its bars, keyboard, ruler,
    /// strip and menu — the console theme's, one definition: drawn in
    /// this theme's `editor.*` tokens.
    fn editor(&mut self) {
        const EDITOR: &str = include_str!("../../themes/console/editor.frame");
        for line in EDITOR.lines() {
            let t = line.trim();
            if t == "<Document>" || t == "</Document>" || t.is_empty() {
                continue;
            }
            self.out.push_str(line);
            self.out.push('\n');
        }
    }

    // ── geometry, as `Strip` lays it out ──────────────────────────────

    fn full(&self) -> String {
        format!("({OWN_W} >= 78)")
    }

    fn head(&self) -> String {
        format!("({OWN_W} >= 60)")
    }

    /// The shared sections, against the mixer's height.
    fn pan_band(&self) -> String {
        piecewise("mixer.h", 2000.0, |h| {
            n(f64::from(crate::strip::shape(h, false).pan_band))
        })
    }

    fn input_band(&self) -> String {
        piecewise("mixer.h", 2000.0, |h| {
            n(f64::from(crate::strip::shape(h, false).input_band))
        })
    }

    fn band_bottom(&self) -> String {
        format!("(33 + {} + {})", self.pan_band(), self.input_band())
    }

    fn arm_top(&self) -> String {
        let bb = self.band_bottom();
        format!(
            "({} ? {bb} + {} - {} : {bb} + 1)",
            self.full(),
            n(f64::from(g::ARM_OVERHANG)),
            n(f64::from(g::ARM_CELL_H))
        )
    }

    fn step(&self, control: crate::mcp::Control) -> String {
        let spread = format!("(({} || !{}) ? 4 : 0)", self.full(), self.head());
        let mute = format!("({} + {spread})", n(f64::from(g::RECMON_FROM_ARM)));
        let solo = format!("({mute} + {} + {spread})", n(f64::from(g::SOLO_FROM_MUTE)));
        match control {
            crate::mcp::Control::Mute => mute,
            crate::mcp::Control::Solo => solo,
            _ => format!("({solo} + {} + {spread})", n(f64::from(g::IO_FROM_SOLO))),
        }
    }

    fn column_x(&self) -> String {
        format!(
            "({} ? {OWN_W} - {} : max({OWN_W} - {}, 0) / 2)",
            self.full(),
            n(f64::from(g::STRIP_W - g::COLUMN)),
            n(f64::from(g::BUTTON_W))
        )
    }

    fn column_axis(&self) -> String {
        format!(
            "({} + {})",
            self.column_x(),
            n(f64::from(g::BUTTON_W) / 2.0)
        )
    }

    fn fader_x(&self) -> String {
        format!("(({OWN_W} - 30) / 2)")
    }

    fn fader_top(&self) -> String {
        format!(
            "({} ? {} : max({} + {} + {}, {}) + 3)",
            self.full(),
            self.band_bottom(),
            self.arm_top(),
            self.step(crate::mcp::Control::Solo),
            n(f64::from(g::BUTTON_H)),
            self.band_bottom()
        )
    }

    fn name_top(&self) -> String {
        format!(
            "(h - {} - {})",
            n(crate::mcp::INDENT_STEP),
            n(f64::from(g::NAME_PLATE))
        )
    }

    fn travel(&self) -> String {
        let pan = piecewise("h", 2000.0, |h| {
            n(f64::from(crate::strip::shape(h, false).pan_band))
        });
        let input = piecewise("h", 2000.0, |h| {
            n(f64::from(crate::strip::shape(h, false).input_band))
        });
        let stretch = format!(
            "max(h - {} - {pan} - {input} - {}, 0)",
            n(f64::from(daw_theme_art::collapse::FX_SECTION)),
            n(f64::from(daw_theme_art::collapse::BOTTOM_SECTION))
        );
        format!(
            "clamp({} - 3 - {}, 0, {stretch})",
            self.name_top(),
            self.fader_top()
        )
    }

    fn has_fader(&self) -> String {
        piecewise("h", 2000.0, |h| {
            let fader = matches!(
                crate::strip::shape(h, false).volume,
                daw_ui::controls::VolumeWidget::Fader
            );
            fader.to_string()
        })
    }

    /// Every control's place, for this layout.
    fn spots(&self) -> Spots {
        let (full, head) = (self.full(), self.head());
        let arm_top = self.arm_top();
        let col = self.column_x();
        let axis = self.column_axis();
        let step = |c| format!("{arm_top} + {}", self.step(c));
        let show_input = piecewise("mixer.h", 2000.0, |h| {
            crate::strip::shape(h, false).show_record_input.to_string()
        });
        let travel = self.travel();
        let shown = format!("{} && {travel} > 0", self.has_fader());
        Spots {
            band: (
                "33".into(),
                format!("{} + {}", self.pan_band(), self.input_band()),
            ),
            input: Spot::new(
                "6",
                format!("(33 + {} + 2)", self.pan_band()),
                "min(w - 12, 75)",
                "16",
                format!("{head} && {show_input} && track.armed"),
            ),
            pan: Spot::new(
                "5",
                "35",
                n(f64::from(g::PAN_KNOB_W)),
                format!("{} - 35", self.band_bottom()),
                format!("{head} && {} + {} > 26", self.pan_band(), self.input_band()),
            ),
            fx: Spot::new("7", "9", "max(w - 14, 36)", "24", head.clone()),
            routing: Spot::new(
                format!("{col} - {}", n(art::ROUTING_INSET_V)),
                step(crate::mcp::Control::Routing),
                n(art::ROUTING_CELL_V.0),
                n(art::ROUTING_CELL_V.1),
                full.clone(),
            ),
            arm_mixer: Spot::new(
                format!("{axis} - {}", n(f64::from(g::ARM_CELL_W) * 0.486)),
                arm_top.clone(),
                "36",
                "24",
                full.clone(),
            ),
            arm_panel: Spot::new(
                format!("{axis} - 10"),
                arm_top.clone(),
                "20",
                "20",
                format!("!{full} && w >= 22"),
            ),
            monitor: Spot::new(
                col.clone(),
                format!("{arm_top} + 3 - {}", n(f64::from(g::BUTTON_H))),
                "21",
                "20",
                format!("{full} && track.armed"),
            ),
            mute: Spot::new(col.clone(), step(crate::mcp::Control::Mute), "21", "20", ""),
            solo: Spot::new(col, step(crate::mcp::Control::Solo), "21", "20", ""),
            fader_x: self.fader_x(),
            fader_top: self.fader_top(),
            scale_x: format!("max({} - 24, 1)", self.fader_x()),
            scale_visible: format!("{head} && {shown}"),
            travel,
        }
    }

    // ── drawings as nodes ─────────────────────────────────────────────

    /// A frame holding `drawing`, at `x`/`y` (expressions), shown when
    /// `visible`.
    fn drawing(&mut self, depth: usize, d: &Drawing, at: &Place<'_>) {
        let Place {
            name,
            x,
            y,
            visible,
            follow,
            track,
            extra,
        } = *at;
        let mut attrs = format!("name={} w={} h={}", q(name), n(d.w), n(d.h));
        let _ = write!(attrs, " bind:x={} bind:y={}", q(x), q(y));
        if let Follow::Width { .. } = follow {
            let _ = write!(attrs, " bind:w={}", q("pw"));
        }
        if !visible.is_empty() {
            let _ = write!(attrs, " bind:visible={}", q(visible));
        }
        if !extra.is_empty() {
            let _ = write!(attrs, " {extra}");
        }
        self.line(depth, &format!("<Frame {attrs}>"));
        for (i, op) in d.ops.iter().enumerate() {
            let twin = match follow {
                Follow::Width { w0, twin, w1 } => twin.ops.get(i).map(|t| (t, w0, w1)),
                Follow::Fixed => None,
            };
            self.op(depth + 1, d, op, twin, track);
        }
        self.line(depth, "</Frame>");
    }

    fn op(
        &mut self,
        depth: usize,
        d: &Drawing,
        op: &Op,
        twin: Option<(&Op, f64, f64)>,
        track: &str,
    ) {
        // Where this op is in the twin drawing, for a node that follows the
        // width: its left edge and width (a rect's), or its left edge.
        let fit = |v0: f64, pick: fn(&Op) -> Option<f64>| {
            twin.and_then(|(t, w0, w1)| pick(t).and_then(|v1| line_in_width(v0, v1, w0, w1)))
        };
        match op {
            Op::Fill(Shape::Rect { x, y, w, h, r }, brush) => {
                let rect = (*x, *y, *w, *h);
                let mut a = format!("x={} y={} w={} h={}", n(*x), n(*y), n(*w), n(*h));
                if *r > 0.0 {
                    let _ = write!(a, " radius={}", n(*r));
                }
                a.push_str(&self.fill(brush, rect, track));
                if let Some(e) = fit(*x, |t| match t {
                    Op::Fill(Shape::Rect { x, .. }, _) => Some(*x),
                    _ => None,
                }) {
                    let _ = write!(a, " bind:x={}", q(&e));
                }
                if let Some(e) = fit(*w, |t| match t {
                    Op::Fill(Shape::Rect { w, .. }, _) => Some(*w),
                    _ => None,
                }) {
                    let _ = write!(a, " bind:w={}", q(&e));
                }
                self.line(depth, &format!("<Rectangle {a} />"));
            }
            Op::Fill(shape, brush) => {
                let mut a = format!(
                    "w={} h={} geometry={}",
                    n(d.w),
                    n(d.h),
                    q(&crate::art::path_of(shape).to_svg())
                );
                a.push_str(&self.fill(brush, (0.0, 0.0, d.w, d.h), track));
                a.push_str(&shift(&fit, shape));
                self.line(depth, &format!("<Vector {a} />"));
            }
            Op::Stroke(Shape::Rect { x, y, w, h, r }, brush, stroke) => {
                // A rectangle's outline stays a rectangle, so it can follow
                // the width like its fill does.
                let color = match brush {
                    Brush::Solid(c) => *c,
                    Brush::Linear { stops, .. } | Brush::Radial { stops, .. } => {
                        stops.first().map_or(SENTINEL, |s| s.1)
                    }
                };
                let mut a = format!(
                    "x={} y={} w={} h={} stroke-weight={} stroke-align=\"center\" stroke-join=\"round\" {}",
                    n(*x),
                    n(*y),
                    n(*w),
                    n(*h),
                    n(stroke.width),
                    self.color(color, track).replace("fill", "stroke")
                );
                if *r > 0.0 {
                    let _ = write!(a, " radius={}", n(*r));
                }
                if let Some(e) = fit(*x, |t| match t {
                    Op::Stroke(Shape::Rect { x, .. }, _, _) => Some(*x),
                    _ => None,
                }) {
                    let _ = write!(a, " bind:x={}", q(&e));
                }
                if let Some(e) = fit(*w, |t| match t {
                    Op::Stroke(Shape::Rect { w, .. }, _, _) => Some(*w),
                    _ => None,
                }) {
                    let _ = write!(a, " bind:w={}", q(&e));
                }
                self.line(depth, &format!("<Rectangle {a} />"));
            }
            Op::Stroke(shape, brush, stroke) => {
                let color = match brush {
                    Brush::Solid(c) => *c,
                    Brush::Linear { stops, .. } | Brush::Radial { stops, .. } => {
                        stops.first().map_or(SENTINEL, |s| s.1)
                    }
                };
                let ink = self.color(color, track);
                let mut a = format!(
                    "w={} h={} geometry={} stroke-weight={} stroke-join=\"round\"",
                    n(d.w),
                    n(d.h),
                    q(&crate::art::path_of(shape).to_svg()),
                    n(stroke.width)
                );
                if stroke.round_cap {
                    a.push_str(" stroke-cap=\"round\"");
                }
                let _ = write!(a, " {}", ink.replace("fill", "stroke"));
                a.push_str(&shift(&fit, shape));
                self.line(depth, &format!("<Vector {a} />"));
            }
            Op::Text {
                body,
                x,
                baseline,
                size,
                color,
                align,
            } => {
                let size = f64::from(*size);
                let top = BASELINE.mul_add(-size, *baseline) - 1.0;
                let (left, width, how) = match align {
                    Align::Left => (x - 1.0, 400.0, "left"),
                    Align::Centre => (x - 201.0, 400.0, "center"),
                };
                let mut a = format!(
                    "x={} y={} w={} h={} size={} text-align=\"{how}\" {}",
                    n(left),
                    n(top),
                    n(width),
                    n(size + 4.0),
                    n(size),
                    self.color(*color, track)
                );
                if let Some(e) = fit(*x, |t| match t {
                    Op::Text { x, .. } => Some(*x),
                    _ => None,
                }) {
                    let shift = left - x;
                    let _ = write!(a, " bind:x={}", q(&format!("{e} + {}", n(shift))));
                }
                self.line(depth, &format!("<Text {a}>{}</Text>", xml(body)));
            }
        }
    }

    /// `fill=…` (or `bind:fill=…`) for a colour, a token where it is one.
    fn color(&self, c: Tc, track: &str) -> String {
        if c == SENTINEL && !track.is_empty() {
            return format!("bind:fill={}", q(track));
        }
        format!("fill=\"{}\"", hex(c))
    }

    /// A brush laid across a node whose box is `rect` (in drawing space).
    fn fill(&self, brush: &Brush, rect: (f64, f64, f64, f64), track: &str) -> String {
        let (rx, ry, rw, rh) = rect;
        let (rw, rh) = (rw.max(1e-6), rh.max(1e-6));
        let stops = |stops: &[(f32, Tc)]| {
            stops
                .iter()
                .map(|(at, c)| {
                    format!(
                        "{{\"color\":\"{}\",\"position\":{}}}",
                        hex(*c),
                        n(f64::from(*at))
                    )
                })
                .collect::<Vec<_>>()
                .join(",")
        };
        match brush {
            Brush::Solid(c) => format!(" {}", self.color(*c, track)),
            Brush::Linear { from, to, stops: s } => {
                let (sx, sy) = ((from.0 - rx) / rw, (from.1 - ry) / rh);
                let (ex, ey) = ((to.0 - rx) / rw, (to.1 - ry) / rh);
                let (a, c) = (ex - sx, ey - sy);
                let (b, d) = (-c, a);
                format!(
                    " fill={{[{{\"kind\":{{\"LinearGradient\":{{\"stops\":[{}],\"transform\":{{\"a\":{},\"b\":{},\"c\":{},\"d\":{},\"tx\":{},\"ty\":{}}}}}}}}}]}}",
                    stops(s),
                    n(a),
                    n(b),
                    n(c),
                    n(d),
                    n(b.mul_add(-0.5, sx)),
                    n(d.mul_add(-0.5, sy))
                )
            }
            Brush::Radial {
                centre,
                radius,
                stops: s,
            } => {
                let (a, d) = (2.0 * radius / rw, 2.0 * radius / rh);
                format!(
                    " fill={{[{{\"kind\":{{\"RadialGradient\":{{\"stops\":[{}],\"transform\":{{\"a\":{},\"b\":0,\"c\":0,\"d\":{},\"tx\":{},\"ty\":{}}}}}}}}}]}}",
                    stops(s),
                    n(a),
                    n(d),
                    n((centre.0 - rx - radius) / rw),
                    n((centre.1 - ry - radius) / rh)
                )
            }
        }
    }

    // ── the strip ─────────────────────────────────────────────────────

    /// What the painters record: the ground, the edge, the bands, the
    /// record input, the number.
    fn recorded(&mut self) {
        let p = self.palette;
        self.line(3, "<Rectangle name=\"Ground\" w=86 h=400 bind:w=\"w\" bind:h=\"h\" bind:fill=\"track.ground\" />");
        self.line(3, "<Rectangle name=\"Edge\" w=1 h=400 bind:h=\"h\" bind:fill=\"track.edge\" bind:visible=\"track.has_edge\" />");
        self.line(3, "<Rectangle name=\"Selected\" w=86 h=2 bind:fill=\"$accent\" bind:w=\"w\" bind:visible=\"track.selected && !ui.rack\" />");
        let spots = self.spots();
        self.line(
            3,
            &format!(
                "<Rectangle name=\"Band\" y=33 w=86 h=87 bind:w=\"w\" bind:y={} bind:h={} bind:fill=\"track.tint\" />",
                q(&spots.band.0),
                q(&spots.band.1)
            ),
        );

        // The record input, on armed tracks.
        self.line(
            3,
            &format!(
                "<Frame name=\"Input\" x=6 w=75 h=16 bind:x={} bind:y={} bind:w={} bind:visible={} fill=\"{}\">",
                q(&spots.input.x),
                q(&spots.input.y),
                q(&spots.input.w),
                q(&spots.input.visible),
                hex(to_theme(p.tcp_combo))
            ),
        );
        let size = 10.0;
        self.line(
            4,
            &format!(
                "<Text name=\"Label\" x=3 y={} w=400 h=14 size={} fill=\"{}\" bind:text=\"track.input\">Input</Text>",
                n(8.0 + 3.5 - BASELINE * size - 1.0),
                n(size),
                hex(to_theme(p.text_dim))
            ),
        );
        self.line(
            4,
            &format!(
                "<Vector name=\"Caret\" y=6 w=7 h=4 geometry=\"M 0 0 L 7 0 L 3.5 4 Z\" fill=\"{}\" bind:x=\"pw - 9\" />",
                hex(to_theme(p.text_faint))
            ),
        );
        self.line(3, "</Frame>");

        // The number, on the track's colour, one indent step tall.
        let number = 9.0;
        self.line(3, "<Rectangle name=\"Number band\" w=86 h=12 bind:w=\"w\" bind:y=\"h - 12\" bind:fill=\"track.band\" />");
        self.line(
            3,
            &format!(
                "<Text name=\"Number\" x=-1 w=86 h=13 size={} text-align=\"center\" fill=\"{}\" bind:w=\"w\" bind:y={} bind:text=\"fmt(track.number)\">1</Text>",
                n(number),
                hex(to_theme(p.text_dim)),
                q(&format!("h - 12 + {} - 1", n((12.0 + number * 0.72) / 2.0 - BASELINE * number)))
            ),
        );
        let ink = hex(to_theme(p.text_dim));
        let (cx, cy, arm) = (f64::from(crate::mcp::FOLD_W) / 2.0, 6.0, 3.0);
        let open = format!(
            "M {} {} L {} {} L {} {} Z",
            n(cx - arm),
            n(cy - arm * 0.6),
            n(cx),
            n(cy + arm * 0.6),
            n(cx + arm),
            n(cy - arm * 0.6)
        );
        let shut = format!(
            "M {} {} L {} {} L {} {} Z",
            n(cx - arm * 0.6),
            n(cy - arm),
            n(cx + arm * 0.6),
            n(cy),
            n(cx - arm * 0.6),
            n(cy + arm)
        );
        self.line(3, &format!("<Vector name=\"Fold open\" w=14 h=12 geometry={} fill=\"{ink}\" bind:y=\"h - 12\" bind:visible=\"track.folder && !track.collapsed\" />", q(&open)));
        self.line(3, &format!("<Vector name=\"Fold shut\" w=14 h=12 geometry={} fill=\"{ink}\" bind:y=\"h - 12\" bind:visible=\"track.folder && track.collapsed\" />", q(&shut)));
    }

    /// What the painters draw every frame.
    fn live(&mut self) {
        let spots = self.spots();
        self.pan(&spots.pan);
        self.fx(&spots.fx);
        self.routing(&spots.routing);
        self.arm(&spots.arm_mixer, art::Arm::Mixer);
        self.arm(&spots.arm_panel, art::Arm::Panel);
        self.monitor(&spots.monitor);
        self.button(crate::mcp::Control::Mute, &spots.mute);
        self.button(crate::mcp::Control::Solo, &spots.solo);
        self.name_plate();
        self.fader();
    }

    // ── the controls, each where its spot puts it ──────────────────────

    /// The pan knob.
    fn pan(&mut self, spot: &Spot) {
        self.line(
            3,
            &format!(
                "<Frame name=\"Pan\" x=5 y=35 w=24 h=25 bind:x={} bind:y={} bind:visible={}>",
                q(&spot.x),
                q(&spot.y),
                q(&spot.visible)
            ),
        );
        self.pan_knob(4);
        self.line(3, "</Frame>");
    }

    /// The FX button, across the strip.
    fn fx(&mut self, spot: &Spot) {
        let p = self.palette;
        let chrome = &p.chrome;
        for chain in [art::Chain::Empty, art::Chain::Active] {
            for at in STATES {
                let (w0, w1) = (72.0, 172.0);
                let d = art::fx_pill_wide(chrome, crate::tcp::lit(p), chain, at, w0);
                let twin = art::fx_pill_wide(chrome, crate::tcp::lit(p), chain, at, w1);
                let has = if chain == art::Chain::Empty {
                    "track.fx == 0"
                } else {
                    "track.fx > 0"
                };
                let (name, visible) = (
                    format!("FX {chain:?} {at:?}"),
                    format!("{} && {has} && {}", spot.visible, state("fx", at)),
                );
                self.drawing(
                    3,
                    &d,
                    &Place {
                        name: &name,
                        x: &spot.x,
                        y: &spot.y,
                        visible: &visible,
                        follow: Follow::Width {
                            w0,
                            twin: &twin,
                            w1,
                        },
                        track: "",
                        extra: "",
                    },
                );
                // The pill is as wide as the strip lets it be.
                self.patch_last_frame_width(&spot.w);
            }
        }
    }

    /// Routing: which of parent send, sends and receives the track has.
    fn routing(&mut self, spot: &Spot) {
        let p = self.palette;
        let chrome = &p.chrome;
        for bits in 0..8_u8 {
            let routes = art::Routing {
                parent_send: bits & 1 != 0,
                sends: bits & 2 != 0,
                receives: bits & 4 != 0,
            };
            let when = format!(
                "{} && {} && {}",
                if routes.parent_send {
                    "track.parent_send"
                } else {
                    "!track.parent_send"
                },
                if routes.sends {
                    "track.sends > 0"
                } else {
                    "track.sends == 0"
                },
                if routes.receives {
                    "track.receives > 0"
                } else {
                    "track.receives == 0"
                }
            );
            for at in STATES {
                let d = art::routing(
                    chrome,
                    art::Axis::Vertical,
                    routes,
                    crate::overlay::route_ink(p),
                    at,
                );
                let (name, visible) = (
                    format!("Routing {bits} {at:?}"),
                    format!("{} && {when} && {}", spot.visible, state("routing", at)),
                );
                self.drawing(
                    3,
                    &d,
                    &Place {
                        name: &name,
                        x: &spot.x,
                        y: &spot.y,
                        visible: &visible,
                        follow: Follow::Fixed,
                        track: "",
                        extra: "",
                    },
                );
            }
        }
    }

    /// The record arm: the housing (`Mixer`) or the ring (`Panel`).
    fn arm(&mut self, spot: &Spot, style: art::Arm) {
        let p = self.palette;
        for armed in [false, true] {
            for at in STATES {
                let d = art::record_arm(
                    &p.chrome,
                    crate::tcp::lit(p).rec,
                    armed,
                    at,
                    style,
                    SENTINEL,
                );
                let name = format!("Arm {style:?} {armed} {at:?}");
                let visible = format!(
                    "{} && {}track.armed && {}",
                    spot.visible,
                    if armed { "" } else { "!" },
                    state("arm", at)
                );
                self.drawing(
                    3,
                    &d,
                    &Place {
                        name: &name,
                        x: &spot.x,
                        y: &spot.y,
                        visible: &visible,
                        follow: Follow::Fixed,
                        track: "track.ground",
                        extra: "",
                    },
                );
            }
        }
    }

    /// Input monitoring: off, on, or not while playing.
    fn monitor(&mut self, spot: &Spot) {
        let p = self.palette;
        for (mode, value) in [
            (art::Monitoring::Off, 0),
            (art::Monitoring::Normal, 1),
            (art::Monitoring::NotWhenPlaying, 2),
        ] {
            for at in STATES {
                let lit = match mode {
                    art::Monitoring::NotWhenPlaying => Tc {
                        r: 0xf0,
                        g: 0x8c,
                        b: 0x2e,
                        a: 0xff,
                    },
                    _ => to_theme(p.text),
                };
                let d = art::monitor(&p.chrome, mode, lit, at, art::Facing::Up);
                let name = format!("Monitor {mode:?} {at:?}");
                let visible = format!(
                    "{} && track.monitor == {value} && {}",
                    spot.visible,
                    state("monitor", at)
                );
                self.drawing(
                    3,
                    &d,
                    &Place {
                        name: &name,
                        x: &spot.x,
                        y: &spot.y,
                        visible: &visible,
                        follow: Follow::Fixed,
                        track: "",
                        extra: "",
                    },
                );
            }
        }
    }

    /// Mute or solo.
    fn button(&mut self, control: crate::mcp::Control, spot: &Spot) {
        let p = self.palette;
        let (label, field, lit) = if control == crate::mcp::Control::Mute {
            ("M", "track.mute", crate::tcp::mute_lit(p))
        } else {
            ("S", "track.solo", crate::tcp::solo_lit(p))
        };
        for on in [false, true] {
            for at in STATES {
                let d = art::gutter_button_sized(
                    &p.chrome,
                    label,
                    on,
                    lit,
                    at,
                    (f64::from(g::BUTTON_W), f64::from(g::BUTTON_H)),
                );
                let title = format!("{label} {on} {at:?}");
                let visible = format!(
                    "{}{field} && {}",
                    if on { "" } else { "!" },
                    state(super::name(control), at)
                );
                self.drawing(
                    3,
                    &d,
                    &Place {
                        name: &title,
                        x: &spot.x,
                        y: &spot.y,
                        visible: &visible,
                        follow: Follow::Fixed,
                        track: "",
                        extra: "",
                    },
                );
            }
        }
    }

    /// The last `<Frame …>` line written gets a width binding in place of
    /// `pw` (a drawing that follows the width of its slot).
    fn patch_last_frame_width(&mut self, width: &str) {
        let Some(at) = self.out.rfind("<Frame name=\"FX") else {
            return;
        };
        let end = self.out[at..].find('\n').map_or(self.out.len(), |e| at + e);
        let line = self.out[at..end].replace("bind:w=\"pw\"", &format!("bind:w={}", q(width)));
        self.out.replace_range(at..end, &line);
    }

    fn pan_knob(&mut self, depth: usize) {
        let p = self.palette;
        let ink = to_theme(p.pan);
        let rest = art::pan_knob(&p.chrome, 0.0, ink, Interaction::Normal);
        let (cx, cy, r) = (12.0_f64, 12.08_f64, 9.37_f64);
        let ring = 2.4_f64;
        let ring_r = r - ring / 2.0 - 0.35;
        let d = Drawing {
            w: rest.w,
            h: rest.h,
            ops: Vec::new(),
        };
        // Rim and the empty ring.
        for op in rest.ops.iter().take(2) {
            self.op(depth, &d, op, None, "");
        }
        // The value: the ring stroked out from twelve o'clock as far as the
        // pan goes — the arc the painters draw, built at that sweep.
        let sweep = "clamp(track.pan, -1, 1) * 135";
        self.line(
            depth,
            &format!(
                "<Vector name=\"Value\" w=24 h=25 geometry=\"M 0 0\" stroke=\"{}\" stroke-weight={} stroke-join=\"round\" bind:geometry={} bind:visible={} />",
                hex(ink),
                n(ring),
                q(&format!("arc({}, {}, {}, 0, {sweep})", n(cx), n(cy), n(ring_r))),
                q(&format!("abs({sweep}) > 0.5"))
            ),
        );
        // The face, by state.
        for at in STATES {
            let drawn = art::pan_knob(&p.chrome, 0.0, ink, at);
            if let Some(face) = drawn.ops.get(2) {
                self.line(
                    depth,
                    &format!(
                        "<Frame name=\"Face {at:?}\" w=24 h=25 bind:visible={}>",
                        q(&state("pan", at))
                    ),
                );
                self.op(depth + 1, &d, face, None, "");
                self.line(depth, "</Frame>");
            }
        }
        // The pointer, turned about the knob's centre.
        if let Some(Op::Fill(Shape::Poly(points), _)) = rest.ops.get(3) {
            let mut path = String::new();
            for (i, (x, y)) in points.iter().enumerate() {
                let _ = write!(
                    path,
                    "{} {} {} ",
                    if i == 0 { "M" } else { "L" },
                    n(x - cx),
                    n(y - cy)
                );
            }
            path.push('Z');
            self.line(
                depth,
                &format!(
                    "<Vector name=\"Pointer\" x={} y={} w=1 h=1 geometry={} fill=\"{}\" bind:rotation=\"clamp(track.pan, -1, 1) * 135\" />",
                    n(cx),
                    n(cy),
                    q(&path),
                    hex(ink)
                ),
            );
        }
        // The cap.
        for op in rest.ops.iter().skip(4) {
            self.op(depth, &d, op, None, "");
        }
    }

    fn name_plate(&mut self) {
        let p = self.palette;
        let top = self.name_top();
        self.line(
            3,
            &format!(
                "<Rectangle name=\"Name selected\" x=2 w=82 h=2 fill=\"{}\" bind:w=\"w - 4\" bind:y={} bind:visible=\"track.selected\" />",
                hex(to_theme(p.accent)),
                q(&top)
            ),
        );
        let ink = format!(
            "track.solo ? {} : track.mute ? {} : track.selected ? {} : {}",
            hex(to_theme(p.solo)),
            hex(to_theme(p.text_faint)),
            hex(to_theme(p.text)),
            hex(to_theme(p.text_dim))
        );
        let plate = f64::from(g::NAME_PLATE);
        // Baseline at the plate's middle plus a third of the size.
        let y = format!(
            "{top} + {} + track.label_size / 3 - {} * track.label_size - 1",
            n(plate / 2.0),
            n(BASELINE)
        );
        self.line(
            3,
            &format!(
                "<Text name=\"Name\" x=-1 w=86 h=16 size=11 text-align=\"center\" bind:w=\"w\" bind:y={} bind:size=\"track.label_size\" bind:text=\"track.label\" bind:fill={}>Track</Text>",
                q(&y),
                q(&ink)
            ),
        );
    }

    fn fader(&mut self) {
        let p = self.palette;
        let chrome = &p.chrome;
        let zones = [
            to_theme(p.meter_safe),
            to_theme(p.meter_warn),
            to_theme(p.meter_danger),
        ];
        let spots = self.spots();
        let top = spots.fader_top.clone();
        let travel = spots.travel.clone();
        let shown = format!("{} && {travel} > 0", self.has_fader());

        // The scale, against the meter.
        let scale_x = spots.scale_x.clone();
        let scale_shown = spots.scale_visible.clone();
        self.line(
            3,
            &format!(
                "<Frame name=\"Scale\" w=24 h=200 bind:x={} bind:y={} bind:h={} bind:visible={}>",
                q(&scale_x),
                q(&top),
                q(&travel),
                q(&scale_shown)
            ),
        );
        let faint = to_theme(p.text_faint);
        let size = 8.0;
        for (i, db) in art::FADER_MARKS.iter().enumerate() {
            let at = art::meter_norm(*db);
            // Shown when the marks above it leave it room, at this height.
            let visible = piecewise("ph", 2000.0, |h| {
                let mut last = f64::NEG_INFINITY;
                let mut shown = false;
                for (j, mark) in art::FADER_MARKS.iter().enumerate() {
                    let y = h * (1.0 - art::meter_norm(*mark));
                    if y - last < size + 2.0 {
                        continue;
                    }
                    last = y;
                    if j == i {
                        shown = true;
                    }
                }
                shown.to_string()
            });
            let lit = zone_at(zones, at);
            let label = if *db > 0.0 {
                format!("+{db:.0}")
            } else {
                format!("{:.0}", db.abs())
            };
            let passed = format!("meter.peak >= {}", n(at));
            self.line(
                4,
                &format!(
                    "<Rectangle name=\"Tick {label}\" x=19 w=5 h=1 bind:y={} bind:visible={} bind:fill={} />",
                    q(&format!("ph * (1 - {}) - 0.5", n(at))),
                    q(&visible),
                    q(&format!("{passed} ? {} : {}", hex(lit.with_alpha(200)), hex(faint.with_alpha(90))))
                ),
            );
            self.line(
                4,
                &format!(
                    "<Text name=\"Mark {label}\" x={} w=400 h=12 size={} text-align=\"center\" bind:y={} bind:visible={} bind:fill={}>{label}</Text>",
                    n((24.0 - 5.0 - 2.0) / 2.0 - 201.0),
                    n(size),
                    q(&format!("ph * (1 - {}) + {} - {} - 1", n(at), n(size / 3.0), n(BASELINE * size))),
                    q(&visible),
                    q(&format!("{passed} ? {} : {}", hex(lit), hex(faint)))
                ),
            );
        }
        self.line(3, "</Frame>");

        // The fader's column: the meter in the groove, the cap over it.
        let fader_x = spots.fader_x.clone();
        self.line(
            3,
            &format!(
                "<Frame name=\"Fader\" w=30 h=200 bind:x={} bind:y={} bind:h={} bind:visible={}>",
                q(&fader_x),
                q(&top),
                q(&travel),
                q(&shown)
            ),
        );
        let groove = art::groove_w(30.0);
        let groove_x = (30.0 - groove) / 2.0;
        let each = ((groove - 1.0) / 2.0).max(1.0);
        let divider = groove_x + each;
        let sunk = chrome.surface_sunken.shade(-0.5);
        self.line(
            4,
            &format!(
                "<Rectangle name=\"Groove\" x={} y=0.5 w=1 h=100 fill=\"{}\" bind:h=\"ph - 1\" />",
                n(divider),
                hex(sunk)
            ),
        );
        let brush = level_brush(chrome, zones);
        for (ch, at) in [("left", groove_x), ("right", divider + 1.0)] {
            let level = format!("clamp(meter.{ch}, 0, 1)");
            // The bar is the column's ramp shown from the floor up to the
            // level: the colours stay where they are on the travel.
            self.line(
                4,
                &format!(
                    "<Rectangle name=\"Lit {ch}\" x={} w={} h=200 bind:h=\"ph\" bind:inset-top={} bind:inset-bottom=\"0.5\" bind:visible={}{} />",
                    n(at),
                    n(each),
                    q(&format!("ph - 0.5 - (ph - 1) * {level}")),
                    q(&format!("(ph - 1) * {level} > 0.5")),
                    gradient_up(&brush)
                ),
            );
            let hold = format!("clamp(meter.hold_{ch}, 0, 1)");
            let zone = format!(
                "{hold} <= 0.75 ? mix({}, {}, {hold} / 0.75) : mix({}, {}, ({hold} - 0.75) / 0.25)",
                hex(zones[0]),
                hex(zones[1]),
                hex(zones[1]),
                hex(zones[2])
            );
            self.line(
                4,
                &format!(
                    "<Rectangle name=\"Hold {ch}\" x={} w={} h=2 bind:y={} bind:fill={} bind:visible={} />",
                    n(at),
                    n(each),
                    q(&format!("max((ph - 1) * -{hold} + ph - 0.5 - 1, 0.5)")),
                    q(&zone),
                    q(&format!("meter.hold_{ch} > 0.001"))
                ),
            );
        }
        self.line(4, &format!("<Rectangle name=\"Groove over\" x={} y=0.5 w=1 h=100 fill=\"{}\" bind:h=\"ph - 1\" />", n(divider), hex(sunk.with_alpha(210))));
        let clip_h = art::CLIP_H;
        self.line(
            4,
            &format!(
                "<Rectangle name=\"Clip latch\" x={} y=0.5 w={} h={} radius={} fill=\"{}\" bind:visible=\"meter.clip\" />",
                n(groove_x),
                n(groove),
                n(clip_h),
                n(clip_h / 3.0),
                hex(zones[2].shade(0.35))
            ),
        );

        // The cap, glass over the meter.
        let cap_w = art::cap_w(30.0);
        let cap_h = format!("min({}, ph * 0.5)", n(cap_w * 53.0 / 27.0));
        let cap_y = format!(
            "clamp(ph - ph * clamp(track.fader, 0, 1) - {cap_h} / 2, 0, max(ph - {cap_h}, 0))"
        );
        // Drawn at its own 27 by 53 and scaled to the cap's height, which
        // is less than its full size on a short fader.
        let cap = art::fader_cap_through(chrome, chrome.hardware_mark, 0.88);
        let cap_x = n((30.0 - cap_w) / 2.0);
        let cap_scale = format!("bind:scale={}", q(&format!("{cap_h} / 53")));
        self.drawing(
            4,
            &cap,
            &Place {
                name: "Cap",
                x: &cap_x,
                y: &cap_y,
                visible: "",
                follow: Follow::Fixed,
                track: "",
                extra: &cap_scale,
            },
        );

        // The value, while it is being set.
        let readout_top = format!("max({cap_y} + {cap_h} / 2 - 6.5, 0)");
        self.line(
            4,
            &format!(
                "<Rectangle name=\"Readout\" x=33 w=40 h=13 fill=\"{}\" bind:y={} bind:w=\"track.volume_label_w\" bind:visible={} />",
                hex(to_theme(p.tcp_field)),
                q(&readout_top),
                q("ui.pressed == \"fader\"")
            ),
        );
        self.line(
            4,
            &format!(
                "<Text name=\"Readout value\" x=35 w=400 h=13 size=9 fill=\"{}\" bind:y={} bind:text=\"track.volume_label\" bind:visible={}>0.0</Text>",
                hex(to_theme(p.text)),
                q(&format!("{readout_top} + 9 - {} - 1", n(BASELINE * 9.0))),
                q("ui.pressed == \"fader\"")
            ),
        );

        // The level again where the cap's pane crosses it.
        let pane_x0 = (30.0 - cap_w) / 2.0 + cap_w * art::CAP_PANE_X;
        let pane_x1 = pane_x0 + cap_w * art::CAP_PANE_W;
        let through = [
            (0.0_f32, zones[0].with_alpha(130)),
            (0.75, zones[1].with_alpha(130)),
            (1.0, zones[2].with_alpha(130)),
        ];
        for (ch, at) in [("left", groove_x), ("right", divider + 1.0)] {
            let (left, right) = (at.max(pane_x0), (at + each).min(pane_x1));
            if right - left < 0.5 {
                continue;
            }
            let level = format!("clamp(meter.{ch}, 0, 1)");
            let from = format!("({cap_y} + {cap_h} * {})", n(art::CAP_PANE_Y0));
            let to = format!("min({cap_y} + {cap_h} * {}, ph - 0.5)", n(art::CAP_PANE_Y1));
            let lit_top = format!("((ph - 1) * -{level} + ph - 0.5)");
            let top = format!("max({lit_top}, {from})");
            self.line(
                4,
                &format!(
                    "<Rectangle name=\"Through {ch}\" x={} w={} h=200 bind:h=\"ph\" bind:inset-top={} bind:inset-bottom={} bind:visible={}{} />",
                    n(left),
                    n(right - left),
                    q(&top),
                    q(&format!("ph - {to}")),
                    q(&format!("{to} - {top} >= 0.5")),
                    gradient_up(&through)
                ),
            );
        }
        self.line(3, "</Frame>");
    }

    /// The controls a click lands on, last on top — the order
    /// `Strip::control_at` asks them in, reversed.
    fn hits(&mut self) {
        let spots = self.spots();
        let arm_x = format!(
            "{} ? {} : {}",
            spots.arm_mixer.visible, spots.arm_mixer.x, spots.arm_panel.x
        );
        let arm_y = format!(
            "{} ? {} : {}",
            spots.arm_mixer.visible, spots.arm_mixer.y, spots.arm_panel.y
        );
        let arm_w = format!(
            "{} ? {} : {}",
            spots.arm_mixer.visible, spots.arm_mixer.w, spots.arm_panel.w
        );
        let arm_h = format!(
            "{} ? {} : {}",
            spots.arm_mixer.visible, spots.arm_mixer.h, spots.arm_panel.h
        );
        let arm_visible = format!("{} || {}", spots.arm_mixer.visible, spots.arm_panel.visible);
        let hit = |s: &Spot| {
            (
                s.x.clone(),
                s.y.clone(),
                s.w.clone(),
                s.h.clone(),
                s.visible.clone(),
            )
        };
        let mut rows: Vec<(&str, String, String, String, String, String)> = vec![
            (
                "name",
                "0".into(),
                self.name_top(),
                "w".into(),
                n(f64::from(g::NAME_PLATE)),
                String::new(),
            ),
            (
                "folder",
                "0".into(),
                "h - 12".into(),
                n(f64::from(crate::mcp::FOLD_W)),
                "12".into(),
                String::new(),
            ),
            (
                "fader",
                spots.fader_x.clone(),
                spots.fader_top.clone(),
                "30".into(),
                spots.travel.clone(),
                String::new(),
            ),
            (
                "clip",
                spots.fader_x.clone(),
                spots.fader_top.clone(),
                "30".into(),
                n(art::CLIP_H),
                format!("{} > 0", spots.travel),
            ),
        ];
        // Last on top: `Strip::control_at`'s order, reversed.
        for (name, spot) in [
            ("pan", &spots.pan),
            ("fx", &spots.fx),
            ("routing", &spots.routing),
            ("solo", &spots.solo),
            ("mute", &spots.mute),
            ("monitor", &spots.monitor),
        ] {
            let (x, y, w, h, visible) = hit(spot);
            rows.push((name, x, y, w, h, visible));
        }
        rows.push(("arm", arm_x, arm_y, arm_w, arm_h, arm_visible));
        for (name, x, y, w, h, visible) in rows {
            let mut a = format!(
                "name=\"@{name}\" w=10 h=10 bind:x={} bind:y={} bind:w={} bind:h={}",
                q(&x),
                q(&y),
                q(&w),
                q(&h)
            );
            if !visible.is_empty() {
                let _ = write!(a, " bind:visible={}", q(&visible));
            }
            self.line(3, &format!("<Frame {a} />"));
        }
    }
}

/// `bind:x` moving a path drawn at the drawing's origin as far as its
/// left edge moves with the width.
fn shift(fit: &dyn Fn(f64, fn(&Op) -> Option<f64>) -> Option<String>, shape: &Shape) -> String {
    let x0 = crate::art::path_of(shape).bounding_box().x0;
    fit(x0, |t| match t {
        Op::Fill(sh, _) | Op::Stroke(sh, _, _) => Some(crate::art::path_of(sh).bounding_box().x0),
        Op::Text { .. } => None,
    })
    .map_or_else(String::new, |e| {
        format!(" bind:x={}", q(&format!("{e} - {}", n(x0))))
    })
}

/// A gradient from the bottom of a node's box to its top.
fn gradient_up(stops: &[(f32, Tc)]) -> String {
    let s = stops
        .iter()
        .map(|(at, c)| {
            format!(
                "{{\"color\":\"{}\",\"position\":{}}}",
                hex(*c),
                n(f64::from(*at))
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    // (0, 0.5) → the bottom middle, (1, 0.5) → the top middle.
    format!(
        " fill={{[{{\"kind\":{{\"LinearGradient\":{{\"stops\":[{s}],\"transform\":{{\"a\":0,\"b\":1,\"c\":-1,\"d\":0,\"tx\":0,\"ty\":1}}}}}}}}]}}"
    )
}

/// The meter's colours up its travel, as `paint::tcp::level_brush` lays
/// them.
fn level_brush(chrome: &daw_theme::Chrome, zones: [Tc; 3]) -> Vec<(f32, Tc)> {
    match art::fader_track(chrome, (1.0, 1.0), (0.0, 0.0), false, zones, 30.0, 100.0)
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Fill(_, Brush::Linear { stops, .. }) => Some(stops.clone()),
            _ => None,
        }) {
        Some(stops) => stops,
        None => vec![(0.0, zones[0]), (1.0, zones[2])],
    }
}

/// `paint::tcp::zone_at`.
fn zone_at(zones: [Tc; 3], at: f64) -> Tc {
    let at = at.clamp(0.0, 1.0);
    if at <= 0.75 {
        zones[0].mix(zones[1], crate::mcp::f64_to_f32(at / 0.75))
    } else {
        zones[1].mix(zones[2], crate::mcp::f64_to_f32((at - 0.75) / 0.25))
    }
}

#[cfg(test)]
mod tests {
    use theme_runtime::{Schema, Theme};

    use crate::mcp::Control;

    fn theme() -> Theme {
        let palette = Palette::from_theme(&daw_ui::theming::Theme::dark());
        Theme::from_markup(&super::fts_dark(&palette), super::super::schema(), None).unwrap_or_else(
            |e| {
                panic!(
                    "{}",
                    e.iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            },
        )
    }

    use crate::arrangement::Palette;

    /// The theme's controls are where `Strip` puts them, at every width
    /// tier and section height, nested or not.
    #[test]
    fn the_theme_lays_the_strip_out_as_strip_does() {
        let theme = theme();
        let palette = Palette::from_theme(&daw_ui::theming::Theme::dark());
        let _ = Schema::new();
        let mut checked = 0;
        for width in [
            30.0, 54.0, 56.0, 59.9, 60.0, 60.5, 77.9, 78.0, 86.0, 100.3, 133.37, 171.9, 172.0,
            260.5,
        ] {
            for mixer_h in [120.0, 160.0, 220.0, 260.0, 300.0, 400.0, 600.0, 870.0] {
                for depth in [0_usize, 1, 3] {
                    let height = mixer_h - crate::num::coord(depth) * crate::mcp::INDENT_STEP;
                    let shared = crate::strip::shape(mixer_h, false);
                    let top = crate::strip::fx_section(false, false)
                        + f64::from(shared.pan_band)
                        + f64::from(shared.input_band)
                        + 4.0;
                    let strip = crate::strip::Strip::laid_out(
                        width, height, mixer_h, 0.0, top, false, false,
                    );
                    let track = daw_proto::Track {
                        armed: true,
                        ..daw_proto::Track::default()
                    };
                    let fields = super::super::fields(
                        &palette,
                        &track,
                        super::super::Shown {
                            width,
                            mixer_h,
                            depth,
                            ..Default::default()
                        },
                    );
                    for control in [
                        Control::Fx,
                        Control::Pan,
                        Control::RecArm,
                        Control::Monitor,
                        Control::Mute,
                        Control::Solo,
                        Control::Routing,
                        Control::Volume,
                        Control::Name,
                        Control::Folder,
                    ] {
                        let native = strip
                            .rect(control)
                            .filter(|r| r.width() > 0.0 && r.height() > 0.0);
                        let themed = theme
                            .target_rect(
                                super::super::STRIP,
                                (width, height),
                                &fields,
                                super::super::name(control),
                            )
                            .filter(|r| r.width() > 0.0 && r.height() > 0.0);
                        let same = match (native, themed) {
                            (Some(a), Some(b)) => {
                                (a.x0 - b.x0).abs() < 1e-6
                                    && (a.y0 - b.y0).abs() < 1e-6
                                    && (a.x1 - b.x1).abs() < 1e-6
                                    && (a.y1 - b.y1).abs() < 1e-6
                            }
                            (None, None) => true,
                            _ => false,
                        };
                        assert!(
                            same,
                            "{control:?} at {width}x{height} (mixer {mixer_h}, depth {depth}): painters {native:?}, theme {themed:?}"
                        );
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 3000);
    }
    /// A console theme's controls for a strip in a mode.
    fn console_targets(
        theme: &Theme,
        palette: &Palette,
        (width, height): (f64, f64),
        (mixing, recording, armed): (bool, bool, bool),
    ) -> Vec<(&'static str, vello::kurbo::Rect)> {
        let track = daw_proto::Track {
            armed,
            ..daw_proto::Track::default()
        };
        let fields = super::super::fields(
            palette,
            &track,
            super::super::Shown {
                width,
                mixer_h: height,
                mixing,
                recording,
                ..Default::default()
            },
        );
        [
            "fx", "pan", "arm", "monitor", "mute", "solo", "routing", "fader", "name",
        ]
        .into_iter()
        .filter_map(|name| {
            theme
                .target_rect(super::super::STRIP, (width, height), &fields, name)
                .filter(|r| r.width() > 0.0 && r.height() > 0.0)
                .map(|r| (name, r))
        })
        .collect()
    }

    /// The console theme as it ships: its files, layered.
    fn console() -> (Theme, Palette) {
        let palette = Palette::from_theme(&daw_ui::theming::Theme::dark());
        let files = [
            (
                "tokens.frame",
                include_str!("../../themes/console/tokens.frame"),
            ),
            (
                "controls.frame",
                include_str!("../../themes/console/controls.frame"),
            ),
            (
                "strip.frame",
                include_str!("../../themes/console/strip.frame"),
            ),
        ];
        let theme = Theme::from_sources(&files, super::super::schema(), None).unwrap_or_else(|e| {
            panic!(
                "{}",
                e.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        });
        (theme, palette)
    }

    const MODES: [(bool, bool); 3] = [(false, false), (true, false), (false, true)];

    /// The console layout: in every mode, at every width, its controls stay
    /// inside the strip and off each other, armed or not.
    #[test]
    fn the_console_controls_do_not_collide() {
        let (theme, palette) = console();
        for (mixing, recording) in MODES {
            for armed in [false, true] {
                for width in [30.0, 46.0, 56.0, 77.9, 86.0, 133.37, 260.5] {
                    for height in [300.0, 600.0, 900.0] {
                        let rects = console_targets(
                            &theme,
                            &palette,
                            (width, height),
                            (mixing, recording, armed),
                        );
                        let mode = format!(
                            "{width}x{height} armed {armed} mixing {mixing} recording {recording}"
                        );
                        for (name, r) in &rects {
                            assert!(
                                r.x0 >= -0.5
                                    && r.x1 <= width + 0.5
                                    && r.y0 >= -0.5
                                    && r.y1 <= height + 0.5,
                                "{name} outside the strip at {mode}: {r:?}"
                            );
                        }
                        for (i, (a, ra)) in rects.iter().enumerate() {
                            for (b, rb) in rects.iter().skip(i + 1) {
                                let overlap = ra.intersect(*rb);
                                // The recording row's boxes reach four rows
                                // into the name plate's empty top; their
                                // drawings do not.
                                let slack = if *b == "name" && matches!(*a, "arm" | "monitor") {
                                    4.0
                                } else {
                                    0.5
                                };
                                assert!(
                                    overlap.width() <= 0.5 || overlap.height() <= slack,
                                    "{a} and {b} collide at {mode}: {ra:?} {rb:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Mute and solo are in one place on every console strip of a mode:
    /// the same heights, on the strip's centre line with everything else.
    /// Mixing, no arm at all; recording, the arm on the name.
    #[test]
    fn the_console_puts_mute_and_solo_in_one_place_at_every_width() {
        let (theme, palette) = console();
        // (mute, solo) heights: recording and neither, the full stack;
        // mixing, one row shorter.
        let heights = [(74.0, 52.0); 3];
        for ((mixing, recording), heights) in MODES.into_iter().zip(heights) {
            let mut seen: Vec<(f64, f64, f64, f64)> = Vec::new();
            for width in [30.0, 46.0, 56.0, 77.9, 86.0, 133.37] {
                let rects =
                    console_targets(&theme, &palette, (width, 600.0), (mixing, recording, true));
                let at = |name| rects.iter().find(|(n, _)| *n == name).map(|(_, r)| *r);
                let (mute, solo) = (at("mute").expect("mute"), at("solo").expect("solo"));
                // Measured from the strip's centre: on it, at fixed heights.
                let off = |r: vello::kurbo::Rect| r.center().x - width / 2.0;
                seen.push((off(mute), mute.y0, off(solo), solo.y0));
                for name in ["pan", "arm", "routing", "fx"] {
                    if let Some(r) = at(name) {
                        assert!(off(r).abs() < 0.01, "{name} off centre at {width}: {r:?}");
                    }
                }
                // From the fader up: the pan, mute, solo, the arm, the routing.
                let (pan, route) = (at("pan").expect("pan"), at("routing").expect("routing"));
                let fader = at("fader").expect("fader");
                assert!(
                    pan.y1 <= fader.y0 && mute.y1 <= pan.y0 && solo.y1 <= mute.y0,
                    "pan, mute, solo from the fader up at {width}"
                );
                // The pan centred on the fader, not on the mute/solo column.
                assert!(
                    (pan.center().x - fader.center().x).abs() < 1.0,
                    "pan over the fader at {width}: {pan:?} {fader:?}"
                );
                assert!(route.y1 <= solo.y0, "the routing over solo at {width}");
                assert_eq!(
                    at("arm").is_some(),
                    !mixing,
                    "arm at {width} (mixing {mixing})"
                );
                // The monitor right of the arm, once armed and where it fits.
                if let (Some(arm), Some(monitor)) = (at("arm"), at("monitor")) {
                    assert!(monitor.x0 >= arm.x1, "monitor right of the arm at {width}");
                }
                if recording && width >= 66.0 {
                    assert!(at("monitor").is_some(), "monitor at {width}");
                    let unarmed = console_targets(
                        &theme,
                        &palette,
                        (width, 600.0),
                        (mixing, recording, false),
                    );
                    assert!(
                        unarmed.iter().all(|(n, _)| *n != "monitor"),
                        "no monitor unarmed at {width}"
                    );
                }
                // The arm under the fader's travel, on the name.
                if let Some(arm) = at("arm") {
                    let name = at("name").expect("name");
                    assert!(
                        arm.y0 >= fader.y1 && arm.y1 <= name.y0 + 4.0,
                        "the arm on the name at {width}: {arm:?} {fader:?} {name:?}"
                    );
                }
                // The chain on every strip, at the head.
                let fx = at("fx").expect("fx");
                assert!(fx.y1 <= route.y0, "the chain at the head at {width}");
            }
            assert!(seen.windows(2).all(|w| w[0] == w[1]), "{seen:?}");
            assert_eq!(seen[0], (0.0, heights.0, 0.0, heights.1));
        }
    }
}
