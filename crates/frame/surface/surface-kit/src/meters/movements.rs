//! Two more needle movements, matched from photographs:
//!
//! - `meter.vu_decibel_white` — the desktop VCA compressor's big meter: a
//!   pale ivory card printed in blue with an evenly spaced −40…+20 decibel
//!   scale, behind glass in a deep, bevelled black frame.
//! - `meter.gr_compression` — the rack bus compressor's gain-reduction
//!   meter: a small near-black card lit amber from behind, printed 0 4 8 12
//!   16 20 in cream, its needle resting at the *left* stop and swinging
//!   right as the reduction deepens, set in a tall black housing with a
//!   zero-adjust screw under the window.

use document_proto::{Color, NodeId};
use surface_proto::{ControlKind, SlotRole};
use surface_widgets::Builder;

use super::faces::{Bezel, CardPaint, Face, Lamp, Lin, Needle, Shade};
use super::scale::Card;
use super::{css, drop, inner, lin, print, quad};
use crate::draw;
use crate::registry::{BuildFn, ComponentDef, Family, Place};

/// A deep bevelled black frame: the chamfer is the frame's whole face.
pub const BEVEL: Bezel = Bezel {
    name: "bevel",
    frame: Lin {
        angle: 180.0,
        stops: &[(0.0, "#1a1b1e"), (1.0, "#0b0b0d")],
    },
    radius: 2.0,
    glow: None,
    top: "#08080a",
    left: "#141518",
    right: "#2c2e32",
    bottom: "#3a3c41",
    depth: 16.0,
    vent: None,
    sheen: Some(Lin {
        angle: 158.0,
        stops: &[
            (0.0, "rgba(255,255,255,0.16)"),
            (0.22, "rgba(255,255,255,0.05)"),
            (0.5, "rgba(255,255,255,0.0)"),
            (1.0, "rgba(0,0,0,0.12)"),
        ],
    }),
    shade: Some(Shade {
        color: "rgba(0,0,0,0.40)",
        top: 7.0,
        side: 4.0,
    }),
};

/// Pale ivory printed in blue — a decibel readout, not a VU.
pub static WHITE_BLUE: Face = Face {
    card_paint: CardPaint::Linear(Lin {
        angle: 180.0,
        stops: &[(0.0, "#f1f1e8"), (0.6, "#e6e6da"), (1.0, "#d6d6c8")],
    }),
    ink: "#2a5aa8",
    hot: "#2a5aa8",
    needle: Needle {
        color: "#15171b",
        width: 1.1,
        hub_r: 4.0,
        hub_opacity: 0.9,
        halo: None,
    },
    lamp: Some(Lamp {
        color: "rgba(255,250,232,0.30)",
        x: 50.0,
        y: 8.0,
        reach: 70.0,
        core: None,
        light: (0.0, 0.0),
    }),
    glass: Some(Lin {
        angle: 160.0,
        stops: &[
            (0.0, "rgba(255,255,255,0.24)"),
            (0.38, "rgba(255,255,255,0.04)"),
            (1.0, "rgba(0,0,0,0.10)"),
        ],
    }),
    bezel: &BEVEL,
    card: Card::Decibels,
    tintable: false,
};

fn build_white_blue(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    super::build(&WHITE_BLUE, b, parent, p)
}

/// The printed compression marks, dB of reduction, left to right.
const MARKS: &[f64] = &[0.0, 4.0, 8.0, 12.0, 16.0, 20.0];
/// Half the needle's travel, degrees either side of straight up.
const HALF_SWEEP: f64 = 38.0;

/// The bus compressor's gain-reduction movement.
fn build_compression(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let w = p.size.unwrap_or(120.0).max(30.0);
    let h = w * 0.86;
    let s = w / 120.0;

    // ── the housing: a tall black block ──
    let mut housing = draw::frame(&p.name, 0.0, 0.0, w, h);
    housing.fills = vec![draw::linear(
        180.0,
        &[(0.0, "#1b1c1f"), (0.5, "#121315"), (1.0, "#0c0d0e")],
    )];
    let housing = draw::with_effects(
        draw::stroked(draw::rounded(housing, 2.0 * s), "#000000", s.max(1.0)),
        vec![
            drop(css("rgba(0,0,0,0.5)"), 0.0, 2.0 * s, 5.0 * s),
            inner(css("rgba(255,255,255,0.08)"), 0.0, 1.0 * s, 0.0),
        ],
    );
    let root = b.add(parent, housing);

    // ── the window and its card ──
    let (cx0, cy0, cw, ch) = (0.065 * w, 0.14 * h, 0.87 * w, 0.54 * h);
    let mut card = draw::frame("card", cx0, cy0, cw, ch);
    card.fills = vec![draw::linear(180.0, &[(0.0, "#1a140c"), (1.0, "#0a0806")])];
    card.clips_content = true;
    card.effects = vec![inner(css("rgba(0,0,0,0.85)"), 0.0, 2.0 * s, 5.0 * s)];
    if p.flag("glass") {
        card.shader = Some(document_proto::ShaderFx {
            shader: "glass".into(),
            params: std::iter::once(("strength".to_string(), 0.4)).collect(),
            inputs: Vec::new(),
        });
    }
    let card = b.add(root, draw::rounded(card, 1.5 * s));
    // The amber lamp behind it, pooled in the upper middle.
    let amber = css("rgba(255,176,72,0.55)");
    let wash = draw::rect(
        "lamp",
        0.0,
        0.0,
        cw,
        ch,
        draw::radial_at(
            0.5,
            0.2,
            0.75,
            &[(0.0, "#ffb04c8c"), (0.55, "#c0782a30"), (1.0, "#00000000")],
        ),
    );
    b.add(card, draw::emissive(wash, amber, 0.35, 14.0 * s));

    // The scale: an arc of marks round a pivot well below the card.
    let (px, py) = (cw / 2.0, ch * 1.55);
    let reach = ch * 1.38;
    let angle = |v: f64| -HALF_SWEEP + 2.0 * HALF_SWEEP * v / 20.0;
    let ink = css("#f2e6c8");
    let mut ticks = String::new();
    for v in MARKS {
        let a = draw::polar(px, py, reach - 2.0 * s, angle(*v));
        let e = draw::polar(px, py, reach - 9.0 * s, angle(*v));
        ticks.push_str(&quad(a, e, 1.3 * s));
        ticks.push(' ');
        let (lx, ly) = draw::polar(px, py, reach - 17.0 * s, angle(*v));
        let label = format!("{v:.0}");
        b.add(
            card,
            print("numeral", &label, (lx, ly), 8.0 * s, ink, (600.0, 0.0)),
        );
    }
    b.add(
        card,
        draw::path("ticks", 0.0, 0.0, cw, ch, ticks, draw::solid("#f2e6c8")),
    );
    let legend = p
        .legend
        .clone()
        .unwrap_or_else(|| "Compression".to_string());
    if !legend.trim().is_empty() {
        b.add(
            card,
            print(
                "legend",
                &legend.to_uppercase(),
                (cw / 2.0, ch * 0.90),
                4.2 * s,
                ink,
                (500.0, 0.3 * s),
            ),
        );
    }

    // The needle: a pale line in a square frame centred on the pivot,
    // drawn at the centre of travel (straight up).
    let len = reach + 1.0 * s;
    let nw = 1.2 * s;
    let r = len + nw;
    let pivot = b.add(
        card,
        draw::frame("needle pivot", px - r, py - r, 2.0 * r, 2.0 * r),
    );
    let line = draw::rounded(
        draw::rect(
            "needle",
            r - nw / 2.0,
            r - len,
            nw,
            len,
            draw::solid("#fff4dc"),
        ),
        nw / 2.0,
    );
    let needle = b.add(
        pivot,
        draw::emissive(line, Color { a: 1.0, ..amber }, 0.3, 3.0 * s),
    );

    // The frame's shadow on the card, then the glass over it.
    let mut lip = draw::rect("lip shadow", 0.0, 0.0, cw, ch, draw::solid("#00000000"));
    lip.effects = vec![
        inner(css("rgba(0,0,0,0.55)"), 0.0, 5.0 * s, 8.0 * s),
        inner(css("rgba(0,0,0,0.4)"), 3.0 * s, 0.0, 5.0 * s),
    ];
    b.add(card, lip);
    let glass = Lin {
        angle: 158.0,
        stops: &[
            (0.0, "rgba(255,255,255,0.20)"),
            (0.3, "rgba(255,255,255,0.04)"),
            (0.6, "rgba(255,255,255,0.0)"),
            (1.0, "rgba(0,0,0,0.12)"),
        ],
    };
    b.add(card, draw::rect("glass", 0.0, 0.0, cw, ch, lin(&glass)));

    // The zero-adjust screw under the window.
    let (sx, sy, sr) = (w / 2.0, 0.84 * h, 4.2 * s);
    b.add(
        root,
        draw::with_effects(
            draw::circle(
                "zero screw",
                sx,
                sy,
                sr,
                draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#3a3b3f"), (1.0, "#0a0a0b")]),
            ),
            vec![inner(css("rgba(0,0,0,0.7)"), 0.0, 0.0, 1.5 * s)],
        ),
    );
    b.add(
        root,
        draw::rect(
            "zero slot",
            sx - 0.6 * s,
            sy - sr * 0.75,
            1.2 * s,
            sr * 1.5,
            draw::solid("#000000"),
        ),
    );

    // Reduction reads left to right: 0 dB at the left stop, 20 dB at the right.
    let c = p
        .control(ControlKind::Gauge {
            sweep_degrees: 2.0 * HALF_SWEEP,
            floor_db: 0.0,
            ceiling_db: -20.0,
        })
        .slot(SlotRole::Needle, needle.0);
    b.controls.insert(root, c);
    root
}

/// These movements' components.
pub(crate) fn defs() -> Vec<ComponentDef> {
    let def = |id, title, description, variants, default_size, build: BuildFn| ComponentDef {
        id,
        family: Family::Meter,
        title,
        description,
        variants,
        default_size,
        binds: "level stream",
        build,
    };
    vec![
        def(
            "meter.vu_decibel_white",
            "dB meter (ivory, blue print)",
            "A pale ivory card printed in blue with a plain, evenly spaced −40…+20 dB scale, behind glass in a deep bevelled black frame — the 1970s desktop VCA compressor's big meter. `legend` prints under the arc (\"Decibels\").",
            &["bevel", "open", "black_rack", "slim", "chrome", "recessed"],
            240.0,
            build_white_blue,
        ),
        def(
            "meter.gr_compression",
            "Compression meter (amber)",
            "A small near-black card lit amber from behind, printed 0 4 8 12 16 20 in cream, in a tall black housing with a zero-adjust screw — the rack bus compressor's gain-reduction meter. Bind a gain-reduction stream: the needle rests at the left stop and swings right as reduction deepens. Size = housing width.",
            &[],
            120.0,
            build_compression,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use document_proto::{Node, NodeKind, Scene};
    use surface_proto::Binding;
    use surface_widgets::Theme;

    use super::*;

    #[test]
    fn the_compression_meter_reads_reduction_left_to_right_on_one_needle() {
        let mut scene = Scene::new("m");
        let page = scene.pages()[0];
        let mut controls = BTreeMap::new();
        let theme = Theme::stage_dark();
        let mut b = Builder {
            scene: &mut scene,
            controls: &mut controls,
            theme: &theme,
        };
        let holder = b.add(page, Node::new(NodeKind::Frame, "holder"));
        let root = build_compression(
            &mut b,
            holder,
            &Place::new("GR").size(150.0).bind(Binding::stream("x/gr")),
        );
        let c = &controls[&root];
        assert!(
            matches!(c.kind, ControlKind::Gauge { floor_db, ceiling_db, .. } if floor_db == 0.0 && ceiling_db == -20.0)
        );
        assert_eq!(c.slots_of(SlotRole::Needle).count(), 1);
        let n = scene.node(root).expect("root");
        assert!((n.size.width - 150.0).abs() < 1e-9);
    }
}
