//! Boutique pedal parts: the hand-painted enclosure, a stomp with its LED
//! beside it, a segment display, and the label-maker tape strip.

use crate::document::proto::{CornerRadii, NodeId, Stroke, StrokeCap, StrokeJoin, TextAlign};
use crate::surface::proto::{ControlKind, SlotRole};
use crate::surface::widgets::Builder;

use crate::surface::kit::draw;
use crate::surface::kit::registry::{ComponentDef, Family, Place};

/// A deterministic noise source for brushwork.
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: &str) -> Self {
        Self(seed.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |a, c| {
            (a ^ u64::from(c)).wrapping_mul(0x0100_0000_01b3)
        }))
    }
    /// Uniform in 0..1.
    pub(crate) fn f(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1_u64 << 53) as f64
    }
    pub(crate) fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.f()
    }
}

/// Polylines as one path string.
pub(crate) fn polylines(lines: &[Vec<(f64, f64)>]) -> String {
    let mut d = String::new();
    for l in lines {
        for (i, (x, y)) in l.iter().enumerate() {
            d.push_str(&format!(
                "{} {x:.2} {y:.2} ",
                if i == 0 { "M" } else { "L" }
            ));
        }
    }
    d
}

/// A stroked-only vector of `lines` in a `w × h` box at the origin.
pub(crate) fn strokes(
    name: &str,
    w: f64,
    h: f64,
    lines: &[Vec<(f64, f64)>],
    color: &str,
    weight: f64,
) -> crate::document::proto::Node {
    let mut n = draw::path(
        name,
        0.0,
        0.0,
        w,
        h,
        polylines(lines),
        draw::solid("#00000000"),
    );
    n.fills.clear();
    n.stroke = Stroke {
        paints: vec![draw::solid(color)],
        weight,
        cap: StrokeCap::Round,
        join: StrokeJoin::Round,
        ..Stroke::default()
    };
    n
}

/// The paint an enclosure variant takes: (top, middle, bottom, light streak, dark streak).
fn paint(v: Option<&str>) -> [&'static str; 5] {
    match v {
        Some("black") => ["#2a2b31", "#1e1f24", "#15161a", "#6a6d7a", "#050506"],
        Some("cream") => ["#e9dfcc", "#d9ccb2", "#c4b596", "#fffaf0", "#8f8266"],
        Some("red") => ["#8e2a2a", "#761f22", "#5a171a", "#d06a60", "#2e0a0c"],
        _ => ["#31365a", "#292d4b", "#20233a", "#9aa2d8", "#07080f"],
    }
}

/// `panel.painted_pedal`: a stompbox under thick, hand-brushed paint.
fn painted_pedal(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let w = p
        .opt("w")
        .and_then(|v| v.parse().ok())
        .or(p.size)
        .unwrap_or(156.0);
    let h = p.opt("h").and_then(|v| v.parse().ok()).unwrap_or(286.0);
    let trim = p
        .opt("trim")
        .filter(|c| c.starts_with('#'))
        .map(ToString::to_string);
    let [top, mid, bottom, light, dark] = paint(p.variant.as_deref());
    let mut rng = Rng::new(&format!("{}{w}{h}", p.name));
    let r = (w.min(h) * 0.07).clamp(6.0, 16.0);
    let lip = (h * 0.035).max(6.0);

    let mut root = draw::frame(&p.name, 0.0, 0.0, w, h);
    root.fills = vec![draw::linear(180.0, &[(0.0, "#11131f"), (1.0, "#07080d")])];
    root.corner_radii = CornerRadii::uniform(r);
    root.effects = vec![draw::drop_shadow("#000000b0", 0.0, 6.0, 14.0)];
    let root = b.add(parent, root);

    // The lid, brushed thick: its paint, then the streaks the brush left.
    let mut lid = draw::frame("lid", 0.0, 0.0, w, h - lip);
    lid.fills = vec![
        draw::linear(172.0, &[(0.0, top), (0.55, mid), (1.0, bottom)]),
        draw::radial_at(0.3, 0.2, 0.9, &[(0.0, "#ffffff14"), (1.0, "#ffffff00")]),
    ];
    lid.corner_radii = CornerRadii::uniform(r);
    lid.clips_content = true;
    lid.effects = vec![
        draw::inner_shadow("#ffffff30", 0.0, 1.0, 1.5),
        draw::inner_shadow("#00000060", 0.0, -2.0, 5.0),
    ];
    let lid_id = b.add(root, lid);
    let lh = h - lip;
    let n = (lh / 1.3) as usize;
    let mut bands: [Vec<Vec<(f64, f64)>>; 4] = Default::default();
    for i in 0..n {
        let y0 = lh * (i as f64 + rng.f()) / n as f64;
        let x0 = rng.range(-10.0, w * 0.4);
        let len = rng.range(w * 0.4, w * 1.1);
        let amp = rng.range(0.4, 2.4);
        let ph = rng.range(0.0, std::f64::consts::TAU);
        let freq = rng.range(0.03, 0.11);
        let tilt = rng.range(-0.03, 0.03);
        let pts: Vec<(f64, f64)> = (0..=24)
            .map(|k| {
                let x = x0 + len * f64::from(k) / 24.0;
                (x, y0 + amp * (x * freq + ph).sin() + tilt * (x - x0))
            })
            .collect();
        bands[i % 4].push(pts);
    }
    let styles = [
        (light, 0.3, 0.8),
        (dark, 0.6, 2.0),
        (light, 0.16, 2.8),
        (dark, 0.45, 0.8),
    ];
    for (k, (lines, (c, a, wgt))) in bands.iter().zip(styles).enumerate() {
        let col = crate::surface::kit::controls::parts::alpha(c, a);
        b.add(
            lid_id,
            strokes(&format!("brush {k}"), w, lh, lines, &col, wgt),
        );
    }
    // Where the paint wore through to the colour under it.
    if let Some(t) = trim {
        let mut chips = Vec::new();
        for _ in 0..10 {
            // Worn where hands and cables catch it: the corners and edges.
            let corner = (rng.f() * 4.0) as usize;
            let (cx, cy) = [
                (r * 0.6, r * 0.6),
                (w - r * 0.6, r * 0.6),
                (w - r * 0.6, lh - r * 0.6),
                (r * 0.6, lh - r * 0.6),
            ][corner.min(3)];
            let along = rng.range(0.0, r * 2.5);
            let (x, y) = if rng.f() < 0.5 {
                (
                    cx + if cx < w / 2.0 { along } else { -along },
                    if cy < lh / 2.0 { 0.8 } else { lh - 0.8 },
                )
            } else {
                (
                    if cx < w / 2.0 { 0.8 } else { w - 0.8 },
                    cy + if cy < lh / 2.0 { along } else { -along },
                )
            };
            let len = rng.range(2.0, 7.0);
            let horiz = y < 2.0 || y > lh - 2.0;
            chips.push(if horiz {
                vec![(x, y), (x + len, y)]
            } else {
                vec![(x, y), (x, y + len)]
            });
        }
        let mut wear = strokes(
            "wear",
            w,
            lh,
            &chips,
            &crate::surface::kit::controls::parts::alpha(&t, 0.9),
            1.8,
        );
        // A clear fill keeps the path for the light (see `emissive`).
        wear.fills = vec![draw::solid("#00000000")];
        b.add(
            lid_id,
            draw::with_opacity(draw::emissive(wear, draw::hex(&t), 0.25, 2.0), 0.8),
        );
    }
    // The enclosure's front edge, seen under the lid.
    b.add(
        root,
        draw::rounded(
            draw::rect(
                "edge",
                1.0,
                lh - 1.0,
                w - 2.0,
                lip,
                draw::linear(180.0, &[(0.0, "#1b1f35"), (1.0, "#0a0b12")]),
            ),
            r * 0.6,
        ),
    );
    root
}

/// `switch.stomp_side_led`: a footswitch whose status LED sits beside it —
/// seen from above and a little in front: a turned chrome collar, the
/// actuator's barrel, its flat cap.
fn stomp_side_led(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    use crate::surface::kit::controls::parts::{shade, tint};
    let d = p.size.unwrap_or(30.0).max(12.0);
    let left = p.opt("side") != Some("right");
    let color = p.lamp_color(0).unwrap_or_else(|| "#ff3b2f".to_string());
    let led_r = d * 0.13;
    let gap = d * 1.25;
    let fw = d * 1.7;
    let fh = d * 1.8;
    let w = fw + gap;
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, w, fh));
    let (x0, y0) = (if left { gap + fw / 2.0 } else { fw / 2.0 }, fh / 2.0);
    // Shadow, the collar (a turned ring with a hex nut's flats), the barrel.
    b.add(
        root,
        draw::circle(
            "shadow",
            x0,
            y0 + d * 0.12,
            d * 0.84,
            draw::radial(&[(0.0, "#000000a0"), (0.75, "#00000060"), (1.0, "#00000000")]),
        ),
    );
    b.add(
        root,
        draw::path(
            "nut",
            0.0,
            0.0,
            w,
            fh,
            draw::regular(x0, y0 + d * 0.08, d * 0.72, 8, 22.5),
            draw::linear(
                160.0,
                &[
                    (0.0, "#b8b8b4"),
                    (0.45, "#6e6f6c"),
                    (0.6, "#454644"),
                    (1.0, "#9a9a96"),
                ],
            ),
        ),
    );
    b.add(
        root,
        draw::circle(
            "collar",
            x0,
            y0 + d * 0.08,
            d * 0.64,
            draw::conic(
                0.0,
                &[
                    (0.0, "#f2f2ee"),
                    (0.2, "#8c8d8a"),
                    (0.45, "#e6e6e2"),
                    (0.7, "#6e6f6c"),
                    (1.0, "#f2f2ee"),
                ],
            ),
        ),
    );
    // The barrel: a cylinder whose front face shows below the cap.
    let (bw, top, bot) = (d * 0.9, y0 - d * 0.3, y0 + d * 0.34);
    b.add(
        root,
        draw::rounded(
            draw::rect(
                "barrel",
                x0 - bw / 2.0,
                top,
                bw,
                bot - top,
                draw::linear(
                    90.0,
                    &[
                        (0.0, "#5a5a5c"),
                        (0.25, "#c8c8c4"),
                        (0.5, "#f0f0ec"),
                        (0.75, "#b4b4b0"),
                        (1.0, "#4a4a4c"),
                    ],
                ),
            ),
            bw / 2.0,
        ),
    );
    let cap = draw::circle(
        "cap",
        x0,
        y0 - d * 0.18,
        d * 0.44,
        draw::radial_at(
            0.42,
            0.36,
            0.8,
            &[(0.0, "#fbfaf7"), (0.6, "#e2e0db"), (1.0, "#a9a7a2")],
        ),
    );
    b.add(
        root,
        draw::with_effects(
            cap,
            vec![
                draw::inner_shadow("#00000040", 0.0, -1.0, 2.0),
                draw::drop_shadow("#00000060", 0.0, 1.0, 1.0),
            ],
        ),
    );
    // The LED in its bezel.
    let lcx = if left { gap * 0.42 } else { fw + gap * 0.58 };
    b.add(
        root,
        draw::with_effects(
            draw::circle(
                "led bezel",
                lcx,
                y0,
                led_r * 1.5,
                draw::linear(180.0, &[(0.0, "#1c1c20"), (1.0, "#050506")]),
            ),
            vec![draw::drop_shadow("#ffffff22", 0.0, 0.6, 0.0)],
        ),
    );
    b.add(
        root,
        draw::circle(
            "led off",
            lcx,
            y0,
            led_r,
            draw::radial_at(
                0.4,
                0.35,
                0.9,
                &[
                    (0.0, "#ffffff33"),
                    (0.5, &shade(&color, 0.65)),
                    (1.0, "#000000"),
                ],
            ),
        ),
    );
    let lit = draw::circle(
        "led",
        lcx,
        y0,
        led_r,
        draw::radial_at(
            0.4,
            0.35,
            0.9,
            &[
                (0.0, "#ffffff"),
                (0.3, &tint(&color, 0.4)),
                (0.7, &color),
                (1.0, &shade(&color, 0.4)),
            ],
        ),
    );
    let lit = b.add(
        root,
        draw::emissive(lit, draw::hex(&color), 0.9, led_r * 5.0),
    );
    let kind = if p.flag("momentary") {
        ControlKind::Button {
            mode: crate::surface::proto::ButtonMode::Momentary,
        }
    } else {
        ControlKind::footswitch()
    };
    let c = p.control(kind).slot(SlotRole::Led, lit.0);
    b.controls.insert(root, c);
    root
}

/// The segments of a seven-segment digit in a `cw × ch` cell at (x, y),
/// leaning `lean` (px across its height).
fn seven(x: f64, y: f64, cw: f64, ch: f64, lean: f64, t: f64) -> Vec<Vec<(f64, f64)>> {
    let sk = |px: f64, py: f64| (x + px + lean * (1.0 - py / ch), y + py);
    let g = t * 0.9;
    let (w, h2) = (cw, ch / 2.0);
    vec![
        vec![sk(g, 0.0), sk(w - g, 0.0)],
        vec![sk(w, g), sk(w, h2 - g)],
        vec![sk(w, h2 + g), sk(w, ch - g)],
        vec![sk(g, ch), sk(w - g, ch)],
        vec![sk(0.0, h2 + g), sk(0.0, ch - g)],
        vec![sk(0.0, g), sk(0.0, h2 - g)],
        vec![sk(g, h2), sk(w - g, h2)],
    ]
}

/// `meter.segment_display`: a recessed LED readout — unlit segments behind
/// the value, lit in the display's colour.
fn segment_display(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let w = p.size.unwrap_or(90.0).max(30.0);
    let h = p.opt("h").and_then(|v| v.parse().ok()).unwrap_or(w * 0.34);
    let digits: usize = p.opt("digits").and_then(|v| v.parse().ok()).unwrap_or(4);
    let color = p.lamp_color(0).unwrap_or_else(|| "#ff4a3a".to_string());
    let mut root = draw::frame(&p.name, 0.0, 0.0, w, h);
    root.fills = vec![draw::linear(180.0, &[(0.0, "#050505"), (1.0, "#16171a")])];
    root.corner_radii = CornerRadii::uniform(h * 0.16);
    root.stroke = Stroke {
        paints: vec![draw::solid("#3a3b40")],
        weight: (h * 0.04).max(1.0),
        ..Stroke::default()
    };
    root.effects = vec![
        draw::inner_shadow("#000000e0", 0.0, 1.5, 4.0),
        draw::drop_shadow("#ffffff26", 0.0, 1.0, 0.0),
    ];
    let root = b.add(parent, root);
    let pad = h * 0.2;
    let cell = (w - 2.0 * pad) / digits as f64;
    let (cw, ch) = (cell * 0.62, h - 2.0 * pad);
    let t = (ch * 0.09).max(1.0);
    let mut ghost = Vec::new();
    for i in 0..digits {
        ghost.extend(seven(
            pad + cell * i as f64 + cell * 0.12,
            pad,
            cw,
            ch,
            ch * 0.12,
            t,
        ));
    }
    b.add(
        root,
        strokes(
            "ghost",
            w,
            h,
            &ghost,
            &crate::surface::kit::controls::parts::alpha(&color, 0.1),
            t,
        ),
    );
    let size = (ch * 0.92).round();
    let mut v = draw::text(
        "value",
        "",
        pad * 0.5,
        h / 2.0 - size * 0.68,
        w - pad,
        size,
        &color,
        TextAlign::Right,
    );
    if let crate::document::proto::NodeKind::Text(d) = &mut v.kind {
        d.font_weight = Some(500.0);
        d.letter_spacing = size * 0.12;
    }
    let v = b.add(root, draw::emissive(v, draw::hex(&color), 0.7, size * 0.4));
    let sheen = draw::linear(
        165.0,
        &[
            (0.0, "#ffffff14"),
            (0.4, "#ffffff05"),
            (0.41, "#ffffff00"),
            (1.0, "#ffffff00"),
        ],
    );
    b.add(
        root,
        draw::rounded(draw::rect("glass", 0.0, 0.0, w, h, sheen), h * 0.16),
    );
    let c = p.control(ControlKind::Readout).slot(SlotRole::Value, v.0);
    b.controls.insert(root, c);
    root
}

/// A label-maker tape strip `w × h` centred on (`cx`, `cy`), turned `tilt`
/// degrees — the frame only; [`add_tape_label`] adds it with its text.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn tape_label(
    name: &str,
    cx: f64,
    cy: f64,
    w: f64,
    h: f64,
    tape: &str,
    tilt: f64,
) -> crate::document::proto::Node {
    let mut f = draw::frame(name, cx - w / 2.0, cy - h / 2.0, w, h);
    f.fills = vec![
        draw::solid(tape),
        draw::linear(
            180.0,
            &[(0.0, "#ffffff40"), (0.5, "#ffffff10"), (1.0, "#00000018")],
        ),
    ];
    f.effects = vec![draw::drop_shadow("#00000070", 0.0, 1.0, 1.5)];
    let mut f = draw::rotated(f, tilt);
    f.name = name.to_string();
    f
}

/// Add a [`tape_label`] under `parent`, its text inside it.
#[allow(clippy::too_many_arguments)]
pub fn add_tape_label(
    b: &mut Builder<'_>,
    parent: NodeId,
    text: &str,
    cx: f64,
    cy: f64,
    w: f64,
    h: f64,
    tape: &str,
    tilt: f64,
) -> NodeId {
    let f = b.add(parent, tape_label("tape", cx, cy, w, h, tape, tilt));
    let size = h * 0.74;
    let mut t = draw::text(
        "text",
        text,
        0.0,
        h / 2.0 - size * 0.64,
        w,
        size,
        "#141414",
        TextAlign::Center,
    );
    if let crate::document::proto::NodeKind::Text(d) = &mut t.kind {
        d.font_weight = Some(430.0);
        d.letter_spacing = size * 0.02;
    }
    b.add(f, t);
    f
}

/// `switch.mini_slide`: a small slide switch in a black slot, its coloured
/// thumb up when the toggle is on (a boutique amp's voicing switches).
fn mini_slide(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let h = p.size.unwrap_or(24.0).max(10.0);
    let w = h * 0.5;
    let thumb = match p.variant.as_deref() {
        Some("black") => ["#3a3a3e", "#1a1a1c"],
        Some("cream") => ["#f4ecd8", "#bfb49a"],
        _ => ["#e0343a", "#8a1418"],
    };
    let side = p.touch.unwrap_or(0.0).max(w).max(h);
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, side, side.max(h)));
    let (x0, y0) = ((side - w) / 2.0, (side.max(h) - h) / 2.0);
    let slot = draw::rounded(
        draw::rect(
            "slot",
            x0,
            y0,
            w,
            h,
            draw::linear(180.0, &[(0.0, "#050506"), (1.0, "#1a1a1d")]),
        ),
        w * 0.18,
    );
    b.add(
        root,
        draw::with_effects(
            slot,
            vec![
                draw::inner_shadow("#000000", 0.0, 1.0, 2.0),
                draw::drop_shadow("#ffffff40", 0.0, 0.8, 0.0),
            ],
        ),
    );
    let pad = w * 0.12;
    let lane = b.add(
        root,
        draw::frame("lane", x0 + pad, y0 + pad, w - 2.0 * pad, h - 2.0 * pad),
    );
    let (tw, th) = (w - 2.0 * pad, (h - 2.0 * pad) * 0.52);
    let t = draw::rounded(
        draw::rect(
            "thumb",
            0.0,
            0.0,
            tw,
            th,
            draw::linear(180.0, &[(0.0, thumb[0]), (1.0, thumb[1])]),
        ),
        tw * 0.15,
    );
    let mut t = draw::with_effects(
        t,
        vec![
            draw::inner_shadow("#ffffff55", 0.0, 1.0, 0.5),
            draw::drop_shadow("#00000090", 0.0, 1.0, 1.5),
        ],
    );
    t.name = "thumb".into();
    let t = b.add(lane, t);
    for i in 0..3 {
        let y = th * (0.3 + 0.2 * f64::from(i));
        b.add(
            t,
            draw::rect(
                "rib",
                tw * 0.18,
                y,
                tw * 0.64,
                (th * 0.06).max(0.6),
                draw::solid("#00000055"),
            ),
        );
    }
    let c = p
        .control(ControlKind::Button {
            mode: crate::surface::proto::ButtonMode::Toggle,
        })
        .slot(SlotRole::Thumb, t.0);
    b.controls.insert(root, c);
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "panel.painted_pedal",
            family: Family::Panel,
            title: "Painted pedal",
            description: "A boutique stompbox enclosure under thick hand-brushed paint — visible brush streaks, the enclosure's dark front edge below the lid, paint worn through at the edges to the colour under it (`trim=#hex`). Variant = paint (navy, black, cream, red). `w=`/`h=` in options (default 156 × 286, the standard pedal).",
            variants: &["navy", "black", "cream", "red"],
            default_size: 156.0,
            binds: "none",
            build: painted_pedal,
        },
        ComponentDef {
            id: "switch.stomp_side_led",
            family: Family::Switch,
            title: "Stomp, LED beside",
            description: "A chrome footswitch with its status LED beside it rather than above (`side=left|right`, `color=#hex`) — how hand-built pedals mount them. Latching; `momentary` makes it a tap switch. Size = actuator diameter.",
            variants: &[],
            default_size: 30.0,
            binds: "toggle param",
            build: stomp_side_led,
        },
        ComponentDef {
            id: "meter.segment_display",
            family: Family::Meter,
            title: "Segment display",
            description: "A small recessed LED readout — the unlit seven-segment digits faintly behind a lit value (`color=#hex`, `digits=4`, `h=`) — a digital delay's time or a tremolo's subdivision. Binds the param it shows. Size = width.",
            variants: &[],
            default_size: 90.0,
            binds: "param",
            build: segment_display,
        },
        ComponentDef {
            id: "switch.mini_slide",
            family: Family::Switch,
            title: "Mini slide switch",
            description: "A small slide switch in a black slot whose coloured thumb (red, black, cream) rides up when the toggle is on — a boutique amp's BRIGHT/DEEP/MID voicing switches or a combo's bright switch. Print its legends round it. Size = slot height.",
            variants: &["red", "black", "cream"],
            default_size: 24.0,
            binds: "toggle param",
            build: mini_slide,
        },
    ]
}
