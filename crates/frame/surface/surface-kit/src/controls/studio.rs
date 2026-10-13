//! Switches drawn from photographs of particular classic studio units:
//!
//! - `switch.toggle_nut`: a bat toggle seen straight on — a chrome washer
//!   and hex nut on the panel, the bat leaning up (on) or down (off). No
//!   slot or box round it, as on valve-era equalizers and 500-series
//!   modules.
//! - `button.round_cap`: a round, cream latching push cap (a 500-series EQ
//!   module's shelf/peak and IN switches) with a small red LED above it.
//! - `button.key`: a console's rectangular grey key with its legend printed
//!   on the cap, a status LED under it.
//!
//! Wiring as the rest of the kit: toggles are `Button { Toggle }`; the bat
//! is a `Thumb` in a lane (up = on); a push cap's `Led` is its lamp — or,
//! with the `nolamp` option, its "latched" shading.

use document_proto::{NodeId, TextAlign};
use surface_proto::{ButtonMode, ControlKind, SlotRole};
use surface_widgets::Builder;

use super::parts::{PANEL_INK, cased, ink, lamp, lin, rad, text_w};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

fn toggle_nut(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(24.0).max(12.0);
    let k = d / 24.0;
    let inkc = p.ink_color(PANEL_INK);
    let legend = p.legend.clone().unwrap_or_default();
    let size = (9.0 * k).max(8.0);
    let right = p.flag("legend_right");
    let body_w = (d * 1.4).max(p.touch.unwrap_or(0.0));
    let cap_h = if right { 0.0 } else { (size * 1.5).ceil() };
    let text_wd = text_w(&legend, size) + size;
    let w = if right {
        body_w + 4.0 * k + text_wd
    } else {
        body_w.max(text_wd)
    };
    let h = cap_h + (d * 2.4).max(p.touch.unwrap_or(0.0));
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, w, h));
    let (cx, cy) = (
        if right { body_w / 2.0 } else { w / 2.0 },
        cap_h + (h - cap_h) / 2.0,
    );
    let label = if right {
        ink(
            "label",
            &legend,
            body_w + 4.0 * k,
            cy,
            text_wd,
            size,
            &inkc,
            TextAlign::Left,
            700.0,
        )
    } else {
        ink(
            "label",
            &legend,
            0.0,
            cap_h / 2.0,
            w,
            size,
            &inkc,
            TextAlign::Center,
            700.0,
        )
    };
    let label = b.add(root, cased(label, p.case()));
    // The washer and nut on the panel, the bushing the bat comes out of.
    b.add(
        root,
        draw::circle(
            "washer shadow",
            cx + 0.8 * k,
            cy + 1.6 * k,
            d * 0.62,
            draw::radial(&[(0.0, "#00000080"), (0.8, "#00000040"), (1.0, "#00000000")]),
        ),
    );
    let washer = draw::conic(
        20.0,
        &[
            (0.0, "#f2f2ef"),
            (0.2, "#8e8f8c"),
            (0.45, "#ecece8"),
            (0.7, "#7c7d7a"),
            (1.0, "#f2f2ef"),
        ],
    );
    b.add(
        root,
        draw::stroked(
            draw::circle("washer", cx, cy, d * 0.56, washer),
            "#00000066",
            0.8 * k,
        ),
    );
    let nut = lin(
        150.0,
        &[
            (0.0, "#fbfbf8".into()),
            (0.45, "#a7a8a5".into()),
            (0.6, "#6e6f6c".into()),
            (1.0, "#d2d3cf".into()),
        ],
    );
    b.add(
        root,
        draw::stroked(
            draw::path(
                "nut",
                0.0,
                0.0,
                w,
                h,
                draw::regular(cx, cy, d * 0.44, 6, 30.0),
                nut,
            ),
            "#00000059",
            0.6 * k,
        ),
    );
    b.add(
        root,
        draw::circle(
            "bushing",
            cx,
            cy,
            d * 0.2,
            rad(
                0.4,
                0.35,
                0.8,
                &[(0.0, "#d6d6d2".into()), (1.0, "#3a3a38".into())],
            ),
        ),
    );
    // The lane the bat leans along: its top half when on.
    let bw = d * 0.34;
    let lane = b.add(
        root,
        draw::frame("lane", cx - bw / 2.0, cy - d * 1.1, bw, d * 2.2),
    );
    let bat_paint = match p.variant.as_deref() {
        Some("black") => lin(
            90.0,
            &[
                (0.0, "#050506".into()),
                (0.35, "#3a3a3e".into()),
                (0.55, "#1c1c1f".into()),
                (1.0, "#050506".into()),
            ],
        ),
        _ => lin(
            90.0,
            &[
                (0.0, "#6c6b66".into()),
                (0.35, "#fbfaf6".into()),
                (0.6, "#bebdb7".into()),
                (1.0, "#5f5e5a".into()),
            ],
        ),
    };
    let bat = draw::with_effects(
        draw::rounded(
            draw::rect("bat", 0.0, 0.0, bw, d * 1.16, bat_paint),
            bw / 2.0,
        ),
        vec![draw::drop_shadow("#000000a6", 1.0 * k, 2.0 * k, 3.0 * k)],
    );
    let bat = b.add(lane, bat);
    let c = p
        .control(ControlKind::Button {
            mode: ButtonMode::Toggle,
        })
        .slot(SlotRole::Thumb, bat.0)
        .slot(SlotRole::Label, label.0);
    b.controls.insert(root, c);
    root
}

/// `(cap, lamp)` stops for a push cap's colour.
fn cap_colors(v: Option<&str>) -> [&'static str; 3] {
    match v {
        Some("grey") => ["#dedcd5", "#c7c5be", "#9d9b95"],
        Some("black") => ["#3a3b3e", "#222326", "#0d0d0f"],
        _ => ["#fbf9f1", "#ebe7da", "#bfbaa9"],
    }
}

/// The lamp or the latched shading an unlit cap shows (the `Led`).
fn latched(b: &mut Builder<'_>, root: NodeId, node: document_proto::Node) -> NodeId {
    b.add(
        root,
        draw::with_effects(node, vec![draw::inner_shadow("#00000080", 0.0, 1.5, 4.0)]),
    )
}

fn round_cap(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(36.0).max(16.0);
    let k = d / 36.0;
    let inkc = p.ink_color(PANEL_INK);
    let lit = !p.flag("nolamp");
    let size = (9.0 * k).max(8.0);
    let top = if lit { d * 0.95 } else { 0.0 };
    let w = (d * 1.2).max(p.touch.unwrap_or(0.0));
    let h = top + d * 1.1 + size * 1.8;
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, w, h));
    let (cx, cy) = (w / 2.0, top + d * 0.55);
    let [hi, mid, lo] = cap_colors(p.variant.as_deref());
    b.add(
        root,
        draw::circle("well", cx, cy, d * 0.53, draw::solid("#0b0b0c")),
    );
    let cap = draw::circle(
        "cap",
        cx,
        cy,
        d * 0.47,
        rad(
            0.4,
            0.34,
            0.8,
            &[(0.0, hi.into()), (0.6, mid.into()), (1.0, lo.into())],
        ),
    );
    b.add(
        root,
        draw::with_effects(
            cap,
            vec![draw::drop_shadow("#000000a6", 0.6 * k, 1.6 * k, 2.5 * k)],
        ),
    );
    let label = b.add(
        root,
        cased(
            ink(
                "label",
                p.legend.as_deref().unwrap_or(""),
                -w,
                top + d * 1.1 + size * 0.8,
                3.0 * w,
                size,
                &inkc,
                TextAlign::Center,
                800.0,
            ),
            p.case(),
        ),
    );
    let led = if lit {
        let color = p.lamp_color(0).unwrap_or_else(|| "#ff3322".into());
        lamp(b, root, cx, d * 0.35, d * 0.24, &color)
    } else {
        latched(
            b,
            root,
            draw::circle("latched", cx, cy, d * 0.47, draw::solid("#0000001f")),
        )
    };
    let mode = if p.flag("momentary") {
        ButtonMode::Momentary
    } else {
        ButtonMode::Toggle
    };
    let c = p
        .control(ControlKind::Button { mode })
        .slot(SlotRole::Led, led.0)
        .slot(SlotRole::Label, label.0);
    b.controls.insert(root, c);
    root
}

fn key(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let kw = p.size.unwrap_or(44.0).max(20.0);
    let k = kw / 44.0;
    let kh = p
        .opt("h")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(kw * 0.5);
    let lit = !p.flag("nolamp");
    let led_d = 7.0 * k;
    let w = kw.max(p.touch.unwrap_or(0.0));
    let h = (kh + if lit { 5.0 * k + led_d + 2.0 * k } else { 0.0 }).max(p.touch.unwrap_or(0.0));
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, w, h));
    let (x, y) = ((w - kw) / 2.0, 0.0);
    let [hi, mid, lo] = cap_colors(Some(p.variant.as_deref().unwrap_or("grey")));
    b.add(
        root,
        draw::rounded(
            draw::rect(
                "well",
                x - 1.0 * k,
                y + 1.5 * k,
                kw + 2.0 * k,
                kh + 1.5 * k,
                draw::solid("#0d0d0e"),
            ),
            2.0 * k,
        ),
    );
    let cap = draw::rounded(
        draw::rect(
            "cap",
            x,
            y,
            kw,
            kh,
            lin(
                180.0,
                &[(0.0, hi.into()), (0.55, mid.into()), (1.0, lo.into())],
            ),
        ),
        1.8 * k,
    );
    b.add(
        root,
        draw::with_effects(
            cap,
            vec![draw::drop_shadow("#000000b3", 1.0 * k, 2.2 * k, 3.0 * k)],
        ),
    );
    let size = (kh * 0.42).clamp(7.0, 12.0);
    let on_cap = if p.variant.as_deref() == Some("black") {
        "#e9e9e6"
    } else {
        "#232427"
    };
    let label = b.add(
        root,
        cased(
            ink(
                "label",
                p.legend.as_deref().unwrap_or(""),
                x,
                y + kh / 2.0,
                kw,
                size,
                on_cap,
                TextAlign::Center,
                700.0,
            ),
            p.case(),
        ),
    );
    let led = if lit {
        let color = p.lamp_color(0).unwrap_or_else(|| "#39d353".into());
        lamp(b, root, w / 2.0, kh + 5.0 * k + led_d / 2.0, led_d, &color)
    } else {
        latched(
            b,
            root,
            draw::rounded(
                draw::rect("latched", x, y, kw, kh, draw::solid("#00000024")),
                1.8 * k,
            ),
        )
    };
    let mode = if p.flag("momentary") {
        ButtonMode::Momentary
    } else {
        ButtonMode::Toggle
    };
    let c = p
        .control(ControlKind::Button { mode })
        .slot(SlotRole::Led, led.0)
        .slot(SlotRole::Label, label.0);
    b.controls.insert(root, c);
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "switch.toggle_nut",
            family: Family::Switch,
            title: "Bat toggle on a nut",
            description: "A bat toggle seen straight on: a chrome washer and hex nut on the panel, the bat leaning up (on) or down (off) — no slot round it, as on valve-era equalizers and 500-series modules. `chrome` or `black` bat; legend above, or beside with the `legend_right` option. Size = the nut.",
            variants: &["chrome", "black"],
            default_size: 24.0,
            binds: "toggle param",
            build: toggle_nut,
        },
        ComponentDef {
            id: "button.round_cap",
            family: Family::Button,
            title: "Round push cap",
            description: "A round latching push cap in a black well (a 500-series EQ module's shelf/peak and IN switches), `cream`, `grey` or `black`, legend under it, a small LED above (`color=#hex`), or no LED with `nolamp` (the cap then shades when latched).",
            variants: &["cream", "grey", "black"],
            default_size: 36.0,
            binds: "toggle param",
            build: round_cap,
        },
        ComponentDef {
            id: "button.key",
            family: Family::Button,
            title: "Console key",
            description: "A large-format console's rectangular key: a grey (or `cream`, `black`) cap in a dark well with its legend printed on it (BELL, IN, PRE), a status LED under it (`color=#hex`), or none with `nolamp`. Size = the cap's width; `h=…` its height (default half the width).",
            variants: &["grey", "cream", "black"],
            default_size: 44.0,
            binds: "toggle param",
            build: key,
        },
    ]
}
