//! Panel buttons — the console's switching. Ported from
//! `fts-audio-ui::hardware::{button, button_kit, button_parts}`.
//!
//! What tells an 1176's ratio bank from an SSL's illuminated square is the
//! *shape*, not the colour: whether the cap lights from within or has a
//! jewel under it, whether it sits in a bezel, how far it travels. So each
//! style is a [`Spec`] (the source's `ButtonSpec`, as data) and one builder
//! walks it. Sizes in a spec are design px at the style's default cap width;
//! everything scales from `Place::size`.
//!
//! Wiring: a single button is `Button { Toggle }` over a toggle param. Its
//! `Led` slots are the lit part (the jewel, or the backlit cap) **and** a
//! "pressed" overlay — the cap's shadow moving inside it — because a
//! latching switch whose position you cannot see is read as broken. A
//! `_bank` places one `Button { Select }` per `Place::options` entry.
//! Any single button takes the `momentary` flag (`Place::options`) and is
//! then `Button { Momentary }`: on while held — a TAP key.

use document_proto::{NodeId, TextAlign, TextCase};
use surface_proto::{ButtonMode, Control, ControlKind, SlotRole};
use surface_widgets::Builder;

use super::parts::{alpha, cased, fixed, ink, lamp, lin, mixs, shade, text_w, tint};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

/// How the cap takes the light, which is what makes it read as a material.
#[derive(Clone, Copy)]
enum Finish {
    /// Moulded plastic: lit across the top, matte below.
    Matte,
    /// A glossy cap, with a hard sheen in the upper third.
    Gloss,
    /// Brushed metal — a sweep rather than a wash.
    Metal,
}

impl Finish {
    fn paint(self, c: &str) -> document_proto::Paint {
        match self {
            Self::Matte => lin(180.0, &[(0.0, tint(c, 0.14)), (0.5, tint(c, 0.04)), (1.0, shade(c, 0.06))]),
            Self::Gloss => lin(
                180.0,
                &[(0.0, tint(c, 0.34)), (0.34, tint(c, 0.08)), (0.52, c.to_string()), (1.0, shade(c, 0.18))],
            ),
            Self::Metal => lin(
                152.0,
                &[(0.0, tint(c, 0.38)), (0.34, c.to_string()), (0.66, shade(c, 0.24)), (1.0, tint(c, 0.30))],
            ),
        }
    }
}

/// Where the button's light comes from.
#[derive(Clone, Copy)]
enum Lit {
    /// A separate jewel below the cap — the console idiom.
    Jewel { d: f64, gap: f64 },
    /// The cap itself lights and the legend glows through it.
    Backlit { bloom: f64 },
    /// No light: read by which one is *down* (the 1176's ratio bank).
    Unlit,
}

/// A bezel the cap sits inside.
#[derive(Clone, Copy)]
struct Surround {
    top: &'static str,
    bottom: &'static str,
    pad: f64,
    radius: f64,
}

/// A button, as data (the source's `ButtonSpec`).
#[derive(Clone, Copy)]
struct Spec {
    /// Cap width × height, design px.
    w: f64,
    h: f64,
    radius: f64,
    finish: Finish,
    border: &'static str,
    lit: Lit,
    surround: Option<Surround>,
    /// How far the cap sinks when pressed, design px.
    travel: f64,
    /// Legend type size, design px.
    legend: f64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Style {
    Console,
    Illuminated,
    PushIn,
    Square,
    Metal,
}

impl Style {
    const fn spec(self) -> Spec {
        match self {
            // A latching rectangular cap with a jewel under it — the desk idiom.
            Self::Console => Spec {
                w: 34.0,
                h: 34.0,
                radius: 3.0,
                finish: Finish::Matte,
                border: "#0000009e",
                lit: Lit::Jewel { d: 9.0, gap: 5.0 },
                surround: None,
                travel: 1.5,
                legend: 9.0,
            },
            // The SSL bus compressor's IN: the cap lights, in a dark surround
            // that stops the glow bleeding into the panel.
            Self::Illuminated => Spec {
                w: 34.0,
                h: 34.0,
                radius: 3.5,
                finish: Finish::Gloss,
                border: "#0000008c",
                lit: Lit::Backlit { bloom: 11.0 },
                surround: Some(Surround { top: "#4a3524", bottom: "#2e2116", pad: 5.0, radius: 5.0 }),
                travel: 1.4,
                legend: 9.5,
            },
            // The 1176's ratio buttons: no lamp, the deepest travel in the kit.
            Self::PushIn => Spec {
                w: 26.0,
                h: 26.0,
                radius: 1.6,
                finish: Finish::Matte,
                border: "#000000b3",
                lit: Lit::Unlit,
                surround: Some(Surround { top: "#23252a", bottom: "#15171a", pad: 3.2, radius: 2.2 }),
                travel: 2.6,
                legend: 8.5,
            },
            // The SSL 4000 channel's small backlit switch, for rows of them.
            Self::Square => Spec {
                w: 28.0,
                h: 28.0,
                radius: 2.0,
                finish: Finish::Matte,
                border: "#00000099",
                lit: Lit::Backlit { bloom: 6.0 },
                surround: None,
                travel: 1.3,
                legend: 9.0,
            },
            // A machined metal cap on outboard gear, with a jewel under it.
            Self::Metal => Spec {
                w: 30.0,
                h: 30.0,
                radius: 2.6,
                finish: Finish::Metal,
                border: "#000000a8",
                lit: Lit::Jewel { d: 8.0, gap: 5.0 },
                surround: None,
                travel: 1.7,
                legend: 8.5,
            },
        }
    }

    const fn variants(self) -> &'static [&'static str] {
        match self {
            Self::Console => &["cream", "black", "red", "grey", "blue"],
            Self::Illuminated => &["amber", "green", "red", "white", "blue", "purple"],
            Self::PushIn => &["black", "grey", "cream"],
            Self::Square => &["white", "yellow", "red", "green", "blue", "purple"],
            Self::Metal => &["silver", "black", "gold"],
        }
    }

    /// `(cap, ink, lamp)` for a variant; the first variant is the default.
    fn colors(self, variant: Option<&str>) -> (String, &'static str, &'static str) {
        let v = variant.unwrap_or(self.variants()[0]);
        match self {
            Self::Console => match v {
                "black" => ("#1c1e21".into(), "#e8eaec", "#43d17a"),
                "red" => ("#c2382c".into(), "#f4e9e7", "#e0483a"),
                "grey" => ("#a4a8ae".into(), "#1a1c1f", "#43d17a"),
                "blue" => ("#3d6f9e".into(), "#eef3f8", "#43d17a"),
                _ => ("#e6e2d4".into(), "#23252a", "#43d17a"),
            },
            Self::Illuminated | Self::Square => {
                let lampc = match v {
                    "green" => "#5ad24a",
                    "red" => "#ec4a3a",
                    "white" => "#fff4d6",
                    "blue" => "#4aa0f4",
                    "purple" => "#b58cff",
                    "yellow" => "#f6d43c",
                    _ if self == Self::Square => "#fff4d6",
                    _ => "#f4a62c",
                };
                // Unlit, a backlit cap is still a coloured lens, not a hole.
                let dim = if self == Self::Square { "#5a5a5c" } else { "#3a2e22" };
                (mixs(lampc, dim, 0.55), "#1d1f22", lampc)
            }
            Self::PushIn => match v {
                "grey" => ("#8d9096".into(), "#15171a", ""),
                "cream" => ("#ddd6c4".into(), "#23252a", ""),
                _ => ("#2c2d31".into(), "#e8e2d8", ""),
            },
            Self::Metal => match v {
                "black" => ("#34353a".into(), "#e8e8e6", "#e0483a"),
                "gold" => ("#c9a85e".into(), "#2a2010", "#e0483a"),
                _ => ("#bfc2c6".into(), "#1f2124", "#e0483a"),
            },
        }
    }
}

/// What drawing one cap produced.
struct CapParts {
    /// Lit overlay (backlit caps) and the pressed overlay — `Led` slots.
    leds: Vec<NodeId>,
    /// The legend — a `Label` slot.
    label: NodeId,
}

/// Draw one cap (with its surround, if the part has one) at (`x`, `y`).
#[allow(clippy::too_many_arguments)]
fn cap(b: &mut Builder<'_>, parent: NodeId, x: f64, y: f64, k: f64, spec: &Spec, (cap_c, ink_c, lamp_c): (&str, &str, &str), (legend, size): (&str, f64), case: TextCase) -> CapParts {
    let pad = spec.surround.map_or(0.0, |s| s.pad * k);
    let (cw, ch) = (spec.w * k, spec.h * k);
    let lip = k.clamp(0.6, 1.6);
    if let Some(s) = spec.surround {
        let well = draw::rect("surround", x, y, cw + 2.0 * pad, ch + 2.0 * pad, lin(180.0, &[(0.0, s.top.into()), (1.0, s.bottom.into())]));
        let well = draw::with_effects(draw::rounded(well, s.radius * k), vec![draw::inner_shadow("#0000008c", 0.0, k, 3.0 * k)]);
        b.add(parent, well);
    }
    let (cx, cy) = (x + pad, y + pad);
    let drop = (spec.travel * k).max(1.0);
    // Standing proud: a lit chamfer along the top, a dark one under it, and
    // the cast shadow on the panel — a flat fill with a shadow is a sticker.
    let body = draw::rect("cap", cx, cy, cw, ch, spec.finish.paint(cap_c));
    let body = draw::stroked(draw::rounded(body, spec.radius * k), spec.border, k.max(1.0));
    let body = draw::with_effects(
        body,
        vec![
            draw::drop_shadow("#00000073", 0.0, drop, (drop * 2.0).max(2.0)),
            draw::inner_shadow("#ffffff57", 0.0, lip, 0.0),
            draw::inner_shadow("#00000057", 0.0, -lip, 0.0),
        ],
    );
    b.add(parent, body);
    let mut leds = Vec::new();
    if let Lit::Backlit { bloom } = spec.lit {
        // A backlit cap *is* the lamp: on, it takes the lamp's colour and
        // throws light past its own edge.
        let lit = draw::rect("lit", cx, cy, cw, ch, spec.finish.paint(lamp_c));
        let lit = draw::stroked(draw::rounded(lit, spec.radius * k), spec.border, k.max(1.0));
        let lit = draw::with_effects(
            lit,
            vec![draw::drop_shadow(&alpha(lamp_c, 0.9), 0.0, 0.0, bloom * k), draw::inner_shadow("#ffffff80", 0.0, lip, 0.0)],
        );
        // Its light rides on the `Led` node, so it dims when the cap goes out.
        let lit = draw::emissive(lit, draw::hex(lamp_c), 0.6, bloom * k);
        leds.push(b.add(parent, lit));
    }
    // Down in its well: the light on the top lip is now on the bottom one,
    // and the shadow falls *into* the button.
    let pressed = draw::rect("pressed", cx, cy, cw, ch, lin(180.0, &[(0.0, "#000000a6".into()), (0.28, "#00000033".into()), (0.8, "#00000000".into()), (1.0, "#ffffff1f".into())]));
    let pressed = draw::with_effects(
        draw::rounded(pressed, spec.radius * k),
        vec![draw::inner_shadow("#0000008c", 0.0, lip * 1.5, drop * 2.0), draw::inner_shadow("#ffffff1a", 0.0, -lip, 0.0)],
    );
    leds.push(b.add(parent, pressed));
    // Caps are narrow: no tracking, and the size the caller fitted.
    let mut text = cased(ink("legend", legend, cx - 2.0 * k, cy + ch / 2.0, cw + 4.0 * k, size, ink_c, TextAlign::Center, 800.0), case);
    if let document_proto::NodeKind::Text(t) = &mut text.kind {
        t.letter_spacing = 0.0;
    }
    let label = b.add(parent, text);
    CapParts { leds, label }
}

/// The type size a cap's legend is set at: the style's size, or smaller
/// until the longest word fits inside the cap with a margin either side.
/// A bank fits all its caps at once — every cap takes the smallest size
/// any of them needs, so a row reads as one set, not CLEAN big and
/// FILTERED small.
fn cap_legend_size<'a>(spec: &Spec, k: f64, legends: impl IntoIterator<Item = &'a str>) -> f64 {
    let (size, width) = (spec.legend * k * 0.9, spec.w * k - 6.0 * k);
    legends.into_iter().map(|l| fit_legend(l, size, width)).fold(size, f64::min)
}

/// The type size a legend is set at: `size`, or smaller (down to a
/// legible floor) until `legend`'s longest line fits `width`.
fn fit_legend(legend: &str, size: f64, width: f64) -> f64 {
    let longest = legend.lines().map(|l| l.chars().count()).max().unwrap_or(0) as f64;
    if longest == 0.0 {
        return size;
    }
    // Bold capitals, untracked, set about 0.62 em a character (measured: a
    // word of eight runs 4.8 em).
    let fits = width / (longest * 0.62);
    size.min(fits).max((size * 0.45).min(5.5))
}

/// The unit a cap occupies: surround included, plus the jewel below it.
fn unit(spec: &Spec, k: f64) -> (f64, f64) {
    let pad = spec.surround.map_or(0.0, |s| s.pad);
    let w = spec.w + 2.0 * pad;
    let h = spec.h + 2.0 * pad;
    match spec.lit {
        Lit::Jewel { d, gap } => (w.max(d) * k, (h + gap + d) * k),
        _ => (w * k, h * k),
    }
}

/// One button (with its jewel) under `parent` at (`x`, `y`), registered as a
/// control of `mode`.
#[allow(clippy::too_many_arguments)]
fn button_at(b: &mut Builder<'_>, parent: NodeId, x: f64, y: f64, k: f64, style: Style, p: &Place, legend: (&str, f64), mode: ButtonMode, name: &str) -> NodeId {
    let spec = style.spec();
    let (cap_c, ink_c, lamp_c) = style.colors(p.variant.as_deref());
    let (w, h) = unit(&spec, k);
    let root = b.add(parent, draw::frame(name, x, y, w, h));
    let pad = spec.surround.map_or(0.0, |s| s.pad);
    let unit_w = (spec.w + 2.0 * pad) * k;
    let parts = cap(b, root, (w - unit_w) / 2.0, 0.0, k, &spec, (&cap_c, ink_c, lamp_c), legend, p.case());
    let mut c = Control::new(ControlKind::Button { mode });
    for led in parts.leds {
        c = c.slot(SlotRole::Led, led.0);
    }
    if let Lit::Jewel { d, gap } = spec.lit {
        let lens = lamp(b, root, w / 2.0, (spec.h + 2.0 * pad + gap + d / 2.0) * k, d * k, lamp_c);
        c = c.slot(SlotRole::Led, lens.0);
    }
    c = c.slot(SlotRole::Label, parts.label.0);
    c.binding.clone_from(&p.binding);
    b.controls.insert(root, c);
    root
}

fn single(b: &mut Builder<'_>, parent: NodeId, p: &Place, style: Style) -> NodeId {
    let spec = style.spec();
    let k = p.size.unwrap_or(spec.w) / spec.w;
    let legend = p.legend.clone().unwrap_or_default();
    let size = cap_legend_size(&spec, k, [legend.as_str()]);
    // A TAP key: on only while held.
    let mode = if p.flag("momentary") { ButtonMode::Momentary } else { ButtonMode::Toggle };
    button_at(b, parent, 0.0, 0.0, k, style, p, (&legend, size), mode, &p.name)
}

/// Bank options: `Place::options`, else numbered positions.
fn options(p: &Place) -> Vec<String> {
    match &p.options {
        Some(o) if !o.is_empty() => o.clone(),
        _ => (1..=3).map(|i| i.to_string()).collect(),
    }
}

/// A row of `Select` buttons, one per option, left to right.
fn bank(b: &mut Builder<'_>, parent: NodeId, p: &Place, style: Style) -> NodeId {
    let spec = style.spec();
    let k = p.size.unwrap_or(spec.w) / spec.w;
    let opts = options(p);
    let n = opts.len();
    let (w, h) = unit(&spec, k);
    let gap = 6.0 * k;
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, n as f64 * w + (n as f64 - 1.0) * gap, h));
    let size = cap_legend_size(&spec, k, opts.iter().map(String::as_str));
    for (i, label) in opts.iter().enumerate() {
        let mode = ButtonMode::Select { index: i as u32, count: n as u32 };
        let at = if p.reverse { n - 1 - i } else { i };
        button_at(b, root, at as f64 * (w + gap), 0.0, k, style, p, (label, size), mode, &format!("{} {label}", p.name));
    }
    root
}

/// The 1176's bank: tall caps butted into one column in a housing, the
/// numbers printed on the *panel beside them*; the engaged one sits lower.
fn push_in_bank(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let spec = Style::PushIn.spec();
    let k = p.size.unwrap_or(spec.w) / spec.w;
    let (cap_c, _, _) = Style::PushIn.colors(p.variant.as_deref());
    // The scale is silkscreened in the panel's ink, not the cap's.
    let panel_ink = p.ink_color(super::parts::PANEL_INK);
    let panel_ink = panel_ink.as_str();
    let opts = options(p);
    let n = opts.len() as f64;
    let (cw, ch) = (spec.w * k, spec.h * k);
    let size = 9.5 * k;
    let label_w = opts.iter().map(|o| text_w(o, size)).fold(size * 1.6, f64::max);
    let gap = 7.0 * k;
    let pad = 2.0 * k;
    let (hx, hw, hh) = (label_w + gap, cw + 2.0 * pad, n * ch + 2.0 * pad);
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, hx + hw, hh));
    let housing = draw::rect("housing", hx, 0.0, hw, hh, lin(180.0, &[(0.0, "#17181a".into()), (1.0, "#0b0c0d".into())]));
    let housing = draw::stroked(draw::rounded(housing, 2.5 * k), "#000000b3", k.max(1.0));
    b.add(root, fixed(draw::with_effects(housing, vec![draw::inner_shadow("#00000099", 0.0, k, 3.0 * k)])));
    for (i, label) in opts.iter().enumerate() {
        // Top to bottom in param order — or, reversed, the last option on top.
        let at = if p.reverse { opts.len() - 1 - i } else { i };
        let row = b.add(root, draw::frame(&format!("{} {label}", p.name), 0.0, pad + at as f64 * ch, hx + hw, ch));
        let text = b.add(row, cased(ink("legend", label, 0.0, ch / 2.0, label_w, size, panel_ink, TextAlign::Right, 700.0), p.case()));
        let body = draw::rect("cap", hx + pad, 0.0, cw, ch, lin(180.0, &[(0.0, tint(&cap_c, 0.10)), (1.0, shade(&cap_c, 0.35))]));
        let body = draw::with_effects(
            draw::rounded(body, 1.5 * k),
            vec![draw::inner_shadow("#ffffff29", 0.0, k.max(0.8), 0.0), draw::inner_shadow("#0000008c", 0.0, -k.max(0.8), 0.0)],
        );
        b.add(row, body);
        let down = draw::rect("pressed", hx + pad, 0.0, cw, ch, lin(180.0, &[(0.0, alpha(&shade(&cap_c, 0.7), 0.95)), (1.0, alpha(&shade(&cap_c, 0.45), 0.9))]));
        let down = draw::with_effects(draw::rounded(down, 1.5 * k), vec![draw::inner_shadow("#000000c0", 0.0, 2.0 * k, 4.0 * k)]);
        let down = b.add(row, down);
        let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Select { index: i as u32, count: n as u32 } })
            .slot(SlotRole::Led, down.0)
            .slot(SlotRole::Label, text.0);
        c.binding.clone_from(&p.binding);
        b.controls.insert(row, c);
    }
    root
}

/// A panel jewel lamp in a chrome bezel that is itself the switch — the
/// 1176's power jewel.
fn jewel(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let s = p.size.unwrap_or(26.0);
    let color = match p.variant.as_deref() {
        Some("green") => "#43d17a",
        Some("amber") => "#f4a62c",
        Some("white") => "#fff2d0",
        Some("blue") => "#4aa0f4",
        Some("purple") => "#b58cff",
        _ => "#e33a2a",
    };
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, s, s));
    let chrome = draw::circle("bezel", s / 2.0, s / 2.0, s / 2.0, draw::conic(20.0, &[(0.0, "#f2f2ef"), (0.25, "#8d8e8b"), (0.5, "#e6e6e2"), (0.75, "#7c7d7a"), (1.0, "#f2f2ef")]));
    b.add(root, draw::with_effects(chrome, vec![draw::drop_shadow("#00000080", 0.0, s * 0.05, s * 0.1)]));
    b.add(root, draw::circle("seat", s / 2.0, s / 2.0, s * 0.38, draw::solid("#1a1a1c")));
    let lens = lamp(b, root, s / 2.0, s / 2.0, s * 0.62, color);
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Led, lens.0);
    c.binding.clone_from(&p.binding);
    b.controls.insert(root, c);
    root
}

/// The Distressor's ratio row: labelled lamps that *are* the readout, each
/// one a `Select` button over the stepped param.
fn led_bank(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let pitch = p.size.unwrap_or(44.0);
    let k = pitch / 44.0;
    let color = match p.variant.as_deref() {
        Some("amber") => "#f4a62c",
        Some("red") => "#e8483a",
        _ => "#5ad24a",
    };
    let opts = options(p);
    let size = 7.5 * k;
    // One size for the row: the smallest any of its legends needs.
    let fitted = opts.iter().map(|l| fit_legend(l, size, pitch - 4.0 * k)).fold(size, f64::min);
    let d = 9.0 * k;
    let h = size * 1.4 + 3.0 * k + d + 2.0 * k;
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, pitch * opts.len() as f64, h));
    let legend_ink = p.ink_color("#cfd4d8");
    for (i, label) in opts.iter().enumerate() {
        let at = if p.reverse { opts.len() - 1 - i } else { i };
        let cell = b.add(root, draw::frame(&format!("{} {label}", p.name), at as f64 * pitch, 0.0, pitch, h));
        let text = b.add(cell, cased(ink("legend", label, 0.0, 7.5 * k * 0.7, pitch, fitted, &legend_ink, TextAlign::Center, 700.0), p.case()));
        let lens = lamp(b, cell, pitch / 2.0, size * 1.4 + 3.0 * k + d / 2.0, d, color);
        let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Select { index: i as u32, count: opts.len() as u32 } })
            .slot(SlotRole::Led, lens.0)
            .slot(SlotRole::Label, text.0);
        c.binding.clone_from(&p.binding);
        b.controls.insert(cell, c);
    }
    root
}

macro_rules! style_fns {
    ($($name:ident => $f:ident($style:expr);)*) => {$(
        fn $name(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId { $f(b, parent, p, $style) }
    )*};
}

style_fns! {
    console => single(Style::Console);
    console_bank => bank(Style::Console);
    illuminated => single(Style::Illuminated);
    illuminated_bank => bank(Style::Illuminated);
    push_in => single(Style::PushIn);
    square => single(Style::Square);
    square_bank => bank(Style::Square);
    metal => single(Style::Metal);
    metal_bank => bank(Style::Metal);
}

#[allow(clippy::too_many_lines)]
pub(crate) fn defs() -> Vec<ComponentDef> {
    let def = |id, title, description, style: Style, binds, build| ComponentDef {
        id,
        family: Family::Button,
        title,
        description,
        variants: style.variants(),
        default_size: style.spec().w,
        binds,
        build,
    };
    vec![
        def(
            "button.console",
            "Console button",
            "A latching rectangular desk button with a green jewel lamp under the cap (SSL/Neve channel switching: FLTR IN, EQ IN, phase). The cap sinks and the jewel lights when on; legend = `Place::legend` or the param name. Size = cap width. For a radio row over a choice param use `button.console_bank`.",
            Style::Console,
            "toggle param",
            console,
        ),
        def(
            "button.console_bank",
            "Console button bank",
            "A row of console buttons with jewels, one per `Place::options` entry, acting as radio buttons over a choice param (each is a `Select` button; only the selected one's jewel lights). Without options it falls back to three numbered positions — pass the param's option names.",
            Style::Console,
            "choice param",
            console_bank,
        ),
        def(
            "button.illuminated",
            "Illuminated button",
            "A glossy cap that lights from within, legend glowing through it, set in a dark bezel — the SSL bus compressor's amber IN button. Use for a main in/bypass switch — or, with the `momentary` option, a TAP key (on only while held). Size = cap width. `button.illuminated_bank` makes a radio row.",
            Style::Illuminated,
            "toggle param",
            illuminated,
        ),
        def(
            "button.illuminated_bank",
            "Illuminated button bank",
            "A row of backlit bezel buttons, one per `Place::options` entry, over a choice param; the selected one lights. Falls back to three numbered positions without options.",
            Style::Illuminated,
            "choice param",
            illuminated_bank,
        ),
        def(
            "button.push_in",
            "Push-in button",
            "An unlit, nearly square-cornered push-in cap in a black housing, read by whether it is *down* (the 1176's buttons); legend on the cap. For the 1176 ratio column use `button.push_in_bank`.",
            Style::PushIn,
            "toggle param",
            push_in,
        ),
        def(
            "button.push_in_bank",
            "Push-in bank (1176 ratio)",
            "The 1176's ratio/meter selector: black push-in caps butted into one column in a housing, `Place::options` printed on the panel beside them top to bottom (option order = param order); the engaged one sits in. Each cap is a `Select` button over a choice param. Falls back to three numbered positions.",
            Style::PushIn,
            "choice param",
            push_in_bank,
        ),
        def(
            "button.square",
            "Square backlit switch",
            "The SSL 4000 channel's small hard-cornered backlit switch: a pale cap that lights in its colour with a tight glow, for rows of them. Size = cap edge. `button.square_bank` makes a radio row.",
            Style::Square,
            "toggle param",
            square,
        ),
        def(
            "button.square_bank",
            "Square switch bank",
            "A row of small backlit square switches, one per `Place::options` entry, over a choice param; the selected one lights. Falls back to three numbered positions.",
            Style::Square,
            "choice param",
            square_bank,
        ),
        def(
            "button.metal",
            "Metal button",
            "A machined, brushed metal cap with a red jewel under it — outboard gear rather than a desk. Size = cap width. `button.metal_bank` makes a radio row.",
            Style::Metal,
            "toggle param",
            metal,
        ),
        def(
            "button.metal_bank",
            "Metal button bank",
            "A row of machined metal caps with jewels, one per `Place::options` entry, over a choice param; the selected one's jewel lights. Falls back to three numbered positions.",
            Style::Metal,
            "choice param",
            metal_bank,
        ),
        ComponentDef {
            id: "button.jewel",
            family: Family::Button,
            title: "Jewel lamp switch",
            description: "A domed jewel lamp in a chrome bezel that is itself the switch — the 1176's red power jewel. Lit when the toggle is on. Size = bezel diameter.",
            variants: &["red", "green", "amber", "white", "blue", "purple"],
            default_size: 26.0,
            binds: "toggle param",
            build: jewel,
        },
        ComponentDef {
            id: "button.led_bank",
            family: Family::Button,
            title: "LED select row",
            description: "The Distressor's ratio row: a row of small labelled LEDs, one per `Place::options` entry, each clickable to select it; the lamp is the readout. Size = pitch between lamps. For dark panels.",
            variants: &["green", "amber", "red"],
            default_size: 44.0,
            binds: "choice param",
            build: led_bank,
        },
    ]
}
