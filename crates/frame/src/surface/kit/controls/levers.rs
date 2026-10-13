//! Levers. `lever.pultec` is ported from `fts-audio-ui::hardware::lever`:
//! the paddle a Pultec selects its frequencies with — a bar through a
//! pivot, read by where it points, legends printed in an arc above it, and
//! a sweep far shorter than a knob's (±62°) so the positions stay readable
//! from across a room. The long-throw fader and the slide lever are the
//! linear levers (`Fader`), drawn in the same hardware idiom.

use crate::document::proto::{NodeId, TextAlign};
use crate::surface::proto::{ControlKind, Orientation, SlotRole};
use crate::surface::widgets::Builder;

use super::parts::{PANEL_INK, cased, ink as parts_ink, lin, shade, text_w, tint};
use crate::surface::kit::draw;
use crate::surface::kit::registry::{ComponentDef, Family, Place};

/// Half the Pultec lever's travel, degrees either side of vertical.
const LEVER_SWEEP_DEG: f64 = 62.0;

/// The paddle outline in the source's `-55 -55 110 110` box: 11.6 wide at
/// the index end, 20.4 at the base, over 52 — about 1:2.6, the unit's own
/// proportion, with radiused corners like a moulding.
const PADDLE: &[(char, &[f64])] = &[
    ('M', &[-4.4, -23.0]),
    ('Q', &[-5.8, -26.0, -3.2, -26.0]),
    ('L', &[3.2, -26.0]),
    ('Q', &[5.8, -26.0, 4.4, -23.0]),
    ('L', &[9.0, 22.0]),
    ('Q', &[9.6, 26.0, 6.0, 26.0]),
    ('L', &[-6.0, 26.0]),
    ('Q', &[-9.6, 26.0, -9.0, 22.0]),
];

/// `PADDLE` in box px: origin at the box centre, `k` px per unit, offset
/// by (`dx`, `dy`) units.
fn paddle_path(c: f64, k: f64, dx: f64, dy: f64) -> String {
    let mut d = String::new();
    for (cmd, xs) in PADDLE {
        d.push(*cmd);
        for pair in xs.chunks(2) {
            d.push_str(&format!(
                " {:.3} {:.3}",
                c + (pair[0] + dx) * k,
                c + (pair[1] + dy) * k
            ));
        }
        d.push(' ');
    }
    d.push('Z');
    d
}

/// The Pultec frequency lever: a `Knob` whose sweep is the lever's ±62°,
/// the paddle its `Needle`, the legends printed in an arc above.
fn pultec(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let case = p.case();
    #[allow(clippy::too_many_arguments)]
    let ink = |name: &str,
               s: &str,
               x: f64,
               cy: f64,
               w: f64,
               size: f64,
               color: &str,
               align: TextAlign,
               weight: f64| {
        cased(
            parts_ink(name, s, x, cy, w, size, color, align, weight),
            case,
        )
    };
    let s = p.size.unwrap_or(110.0);
    let k = s / 110.0;
    let ink_c = p.ink_color(if p.variant.as_deref() == Some("dark") {
        "#1e2328"
    } else {
        "#e6ecf0"
    });
    let ink_c = ink_c.as_str();
    let opts: Vec<String> = match &p.options {
        Some(o) if !o.is_empty() => o.clone(),
        _ => (1..=3).map(|i| i.to_string()).collect(),
    };
    let n = opts.len();
    let sweep = p
        .sweep
        .unwrap_or(if n < 2 { 0.0 } else { 2.0 * LEVER_SWEEP_DEG });
    // Headroom above the box for the unit caption.
    let head = 12.0 * k;
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, s, s + head));
    let c = s / 2.0;
    // Small caption above the arc ("CPS", "KCS") — the `Label`.
    let label = b.add(
        root,
        ink(
            "unit",
            p.legend.as_deref().unwrap_or(""),
            0.0,
            head + c - 55.0 * k,
            s,
            8.5 * k,
            ink_c,
            TextAlign::Center,
            700.0,
        ),
    );
    for (i, o) in opts.iter().enumerate() {
        let a = if n < 2 {
            0.0
        } else {
            (i as f64 / (n - 1) as f64 - 0.5) * sweep
        };
        let (lx, ly) = draw::polar(c, head + c, 42.0 * k, a);
        let w = text_w(o, 10.0 * k) + 6.0;
        b.add(
            root,
            ink(
                "position",
                o,
                lx - w / 2.0,
                ly,
                w,
                10.0 * k,
                ink_c,
                TextAlign::Center,
                700.0,
            ),
        );
    }
    // The paddle turns about the pivot: it fills a square frame centred on
    // it (a `Needle` turns about its parent's centre, and the root is
    // taller than it is wide).
    let pivot = b.add(root, draw::frame("pivot", 0.0, head, s, s));
    let paddle = b.add(pivot, draw::frame("paddle", 0.0, 0.0, s, s));
    b.add(
        paddle,
        draw::path(
            "cast",
            0.0,
            0.0,
            s,
            s,
            paddle_path(c, k, 1.2, 2.0),
            draw::solid("#0000006b"),
        ),
    );
    let body = draw::path(
        "bar",
        0.0,
        0.0,
        s,
        s,
        paddle_path(c, k, 0.0, 0.0),
        lin(
            90.0,
            &[
                (0.0, "#3a3a41".into()),
                (0.35, "#232328".into()),
                (1.0, "#101013".into()),
            ],
        ),
    );
    b.add(paddle, draw::stroked(body, "#00000099", (0.6 * k).max(0.5)));
    // The moulded edge highlight down the paddle's left side.
    let hl = draw::polygon(&[
        (c - 5.9 * k, c - 25.0 * k),
        (c - 5.1 * k, c - 25.0 * k),
        (c - 9.6 * k, c + 24.0 * k),
        (c - 10.6 * k, c + 24.0 * k),
    ]);
    b.add(
        paddle,
        draw::path("edge", 0.0, 0.0, s, s, hl, draw::solid("#ffffff24")),
    );
    // The white stripe you actually read.
    b.add(
        paddle,
        draw::rounded(
            draw::rect(
                "stripe",
                c - 1.7 * k,
                c - 24.0 * k,
                3.4 * k,
                20.0 * k,
                draw::solid("#f1f1ef"),
            ),
            1.6 * k,
        ),
    );
    // Pivot boss.
    b.add(
        root,
        draw::circle(
            "boss",
            c,
            head + c,
            7.0 * k,
            draw::radial_at(0.4, 0.35, 0.7, &[(0.0, "#2e2e33"), (1.0, "#141416")]),
        ),
    );
    b.add(
        root,
        draw::circle("screw", c, head + c, 2.4 * k, draw::solid("#ffffff24")),
    );
    let ctl = p
        .control(ControlKind::Knob {
            sweep_degrees: sweep,
        })
        .slot(SlotRole::Needle, paddle.0)
        .slot(SlotRole::Label, label.0);
    b.controls.insert(root, ctl);
    if let Some(touch) = p.touch {
        // The paddle's reach: 26 of the box's 55 half-units.
        crate::surface::kit::registry::tighten(b, root, (c, head + c), 60.0 * k, touch);
    }
    root
}

/// Cap colours: `(body, index line)`.
fn cap_colors(variant: Option<&str>) -> (&'static str, &'static str) {
    match variant {
        Some("grey") => ("#8e9298", "#16181b"),
        Some("red") => ("#b3281f", "#f4ece8"),
        Some("blue") => ("#2f5f8e", "#eef3f8"),
        Some("white") => ("#e2ded2", "#1d1f22"),
        _ => ("#1d1e21", "#f1f1ef"),
    }
}

/// A long-throw console fader: a dark slot with a printed scale either
/// side, and a grooved cap with a white index line as the `Thumb`.
fn fader(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let case = p.case();
    #[allow(clippy::too_many_arguments)]
    let ink = |name: &str,
               s: &str,
               x: f64,
               cy: f64,
               w: f64,
               size: f64,
               color: &str,
               align: TextAlign,
               weight: f64| {
        cased(
            parts_ink(name, s, x, cy, w, size, color, align, weight),
            case,
        )
    };
    let inkc = p.ink_color(PANEL_INK);
    let travel = p.size.unwrap_or(160.0);
    let k = travel / 160.0;
    let (body_c, line_c) = cap_colors(p.variant.as_deref());
    let (cap_w, cap_h) = (26.0 * k, 38.0 * k);
    let w = 56.0 * k;
    let lab_h = (9.0 * k * 1.4).ceil();
    let root = b.add(
        parent,
        draw::frame(&p.name, 0.0, 0.0, w, travel + cap_h + 6.0 * k + lab_h),
    );
    let x0 = w / 2.0;
    let top = cap_h / 2.0;
    // Scale: ticks either side, majors every quarter.
    for i in 0..=20 {
        let y = top + travel * f64::from(i) / 20.0;
        let (len, a) = if i % 5 == 0 {
            (8.0 * k, inkc.clone())
        } else {
            (4.0 * k, super::parts::alpha(&inkc, 0.63))
        };
        b.add(
            root,
            draw::rect(
                "tick",
                x0 - 7.0 * k - len,
                y - 0.5 * k,
                len,
                k.max(1.0),
                draw::solid(&a),
            ),
        );
        b.add(
            root,
            draw::rect(
                "tick",
                x0 + 7.0 * k,
                y - 0.5 * k,
                len,
                k.max(1.0),
                draw::solid(&a),
            ),
        );
    }
    let slot = draw::rect(
        "slot",
        x0 - 2.5 * k,
        top - 2.0 * k,
        5.0 * k,
        travel + 4.0 * k,
        draw::solid("#060607"),
    );
    b.add(
        root,
        draw::with_effects(
            draw::rounded(slot, 2.5 * k),
            vec![draw::inner_shadow("#ffffff1f", 0.0, -k, 0.0)],
        ),
    );
    // The lane is the cap's whole travel.
    let lane = b.add(
        root,
        draw::frame("lane", x0 - cap_w / 2.0, 0.0, cap_w, travel + cap_h),
    );
    let cap = b.add(lane, draw::frame("cap", 0.0, 0.0, cap_w, cap_h));
    let shell = draw::rect(
        "shell",
        0.0,
        0.0,
        cap_w,
        cap_h,
        lin(
            180.0,
            &[
                (0.0, tint(body_c, 0.22)),
                (0.48, body_c.to_string()),
                (0.52, shade(body_c, 0.25)),
                (1.0, tint(body_c, 0.08)),
            ],
        ),
    );
    let shell = draw::stroked(draw::rounded(shell, 3.0 * k), "#000000b3", k.max(1.0));
    b.add(
        cap,
        draw::with_effects(
            shell,
            vec![
                draw::drop_shadow("#000000a0", 0.0, 3.0 * k, 5.0 * k),
                draw::inner_shadow("#ffffff40", 0.0, k, 0.0),
            ],
        ),
    );
    for i in [-2.0_f64, -1.0, 1.0, 2.0] {
        let y = cap_h / 2.0 + i * 5.0 * k;
        b.add(
            cap,
            draw::rect(
                "groove",
                2.0 * k,
                y - 0.6 * k,
                cap_w - 4.0 * k,
                1.2 * k,
                draw::solid("#00000073"),
            ),
        );
    }
    b.add(
        cap,
        draw::rect(
            "index",
            1.5 * k,
            cap_h / 2.0 - 1.0 * k,
            cap_w - 3.0 * k,
            2.0 * k,
            draw::solid(line_c),
        ),
    );
    let label = b.add(
        root,
        ink(
            "label",
            p.legend.as_deref().unwrap_or(""),
            0.0,
            travel + cap_h + 6.0 * k + lab_h / 2.0,
            w,
            9.0 * k,
            &inkc,
            TextAlign::Center,
            700.0,
        ),
    );
    let c = p
        .control(ControlKind::Fader {
            orientation: Orientation::Vertical,
        })
        .slot(SlotRole::Thumb, cap.0)
        .slot(SlotRole::Label, label.0);
    b.controls.insert(root, c);
    root
}

/// A horizontal slide lever: a slot in a recessed plate, a ridged grip as
/// the `Thumb`.
fn slide(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let case = p.case();
    #[allow(clippy::too_many_arguments)]
    let ink = |name: &str,
               s: &str,
               x: f64,
               cy: f64,
               w: f64,
               size: f64,
               color: &str,
               align: TextAlign,
               weight: f64| {
        cased(
            parts_ink(name, s, x, cy, w, size, color, align, weight),
            case,
        )
    };
    let inkc = p.ink_color(PANEL_INK);
    let w = p.size.unwrap_or(120.0);
    let k = w / 120.0;
    let (body_c, line_c) = cap_colors(p.variant.as_deref());
    let (grip_w, grip_h) = (14.0 * k, 26.0 * k);
    let lab_h = (9.0 * k * 1.4).ceil();
    let root = b.add(
        parent,
        draw::frame(&p.name, 0.0, 0.0, w, lab_h + 4.0 * k + grip_h),
    );
    let label = b.add(
        root,
        ink(
            "label",
            p.legend.as_deref().unwrap_or(""),
            0.0,
            lab_h / 2.0,
            w,
            9.0 * k,
            &inkc,
            TextAlign::Center,
            700.0,
        ),
    );
    let top = lab_h + 4.0 * k;
    let plate = draw::rect(
        "plate",
        0.0,
        top + grip_h * 0.3,
        w,
        grip_h * 0.4,
        lin(180.0, &[(0.0, "#141416".into()), (1.0, "#2a2a2e".into())]),
    );
    b.add(
        root,
        draw::with_effects(
            draw::rounded(plate, grip_h * 0.2),
            vec![draw::inner_shadow("#000000b3", 0.0, k, 2.0 * k)],
        ),
    );
    b.add(
        root,
        draw::rounded(
            draw::rect(
                "slot",
                grip_w / 2.0,
                top + grip_h / 2.0 - 1.5 * k,
                w - grip_w,
                3.0 * k,
                draw::solid("#040405"),
            ),
            1.5 * k,
        ),
    );
    let lane = b.add(root, draw::frame("lane", 0.0, top, w, grip_h));
    let grip = b.add(lane, draw::frame("grip", 0.0, 0.0, grip_w, grip_h));
    let shell = draw::rect(
        "shell",
        0.0,
        0.0,
        grip_w,
        grip_h,
        lin(
            90.0,
            &[
                (0.0, tint(body_c, 0.2)),
                (0.5, body_c.to_string()),
                (1.0, shade(body_c, 0.3)),
            ],
        ),
    );
    let shell = draw::stroked(draw::rounded(shell, 2.5 * k), "#000000b3", k.max(1.0));
    b.add(
        grip,
        draw::with_effects(
            shell,
            vec![draw::drop_shadow("#000000a0", 0.0, 2.0 * k, 4.0 * k)],
        ),
    );
    for i in [-1.0_f64, 1.0] {
        b.add(
            grip,
            draw::rect(
                "ridge",
                grip_w / 2.0 + i * 3.5 * k - 0.6 * k,
                3.0 * k,
                1.2 * k,
                grip_h - 6.0 * k,
                draw::solid("#00000066"),
            ),
        );
    }
    b.add(
        grip,
        draw::rect(
            "index",
            grip_w / 2.0 - k,
            2.0 * k,
            2.0 * k,
            grip_h - 4.0 * k,
            draw::solid(line_c),
        ),
    );
    let c = p
        .control(ControlKind::Fader {
            orientation: Orientation::Horizontal,
        })
        .slot(SlotRole::Thumb, grip.0)
        .slot(SlotRole::Label, label.0);
    b.controls.insert(root, c);
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "lever.pultec",
            family: Family::Lever,
            title: "Pultec lever switch",
            description: "The paddle a Pultec EQP-1A selects its frequencies with: a black bar with a white stripe through a pivot boss, swinging ±62° between detents, legends printed in an arc above. A stepped `Knob` over a choice param — pass its option names as `Place::options` (e.g. [\"20\",\"30\",\"60\",\"100\"]) and the unit (\"CPS\") as `legend`. Size = box width. `variant` sets the legend ink for a light or dark panel.",
            variants: &["light", "dark"],
            default_size: 110.0,
            binds: "choice param",
            build: pultec,
        },
        ComponentDef {
            id: "lever.fader",
            family: Family::Lever,
            title: "Console fader",
            description: "A long-throw console channel fader: dark slot with a printed tick scale either side, a grooved cap with a white index line that slides with the value. Vertical `Fader`. Size = travel.",
            variants: &["black", "grey", "red", "blue", "white"],
            default_size: 160.0,
            binds: "param",
            build: fader,
        },
        ComponentDef {
            id: "lever.slide",
            family: Family::Lever,
            title: "Slide lever",
            description: "A short horizontal slide lever: a ridged grip in a recessed slot (pan, blend, a guitar-pedal slider). Horizontal `Fader`. Size = width.",
            variants: &["black", "grey", "red", "blue", "white"],
            default_size: 120.0,
            binds: "param",
            build: slide,
        },
    ]
}
