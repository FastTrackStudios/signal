//! `01-wah-treadle-241x600`: the *Funk Wah*, seen from above (the Treadle
//! class, 241 × 600) — a royal-blue metallic casting with a raised rim
//! round a black rubber tread moulded in rows of chevrons.
//!
//! The tread is the control: a vertical `Fader` over `wah/position` whose
//! lane is the whole pad — drag toward the toe for treble. Its `Thumb` is
//! the light catching the tilted treadle, a soft band that rides toe-ward
//! with the position. Pressing the toe lip engages it (`wah/on`, a
//! footswitch over the lip), its LED set in the base above.

use document_proto::{Node, NodeId};
use surface_kit::draw;
use surface_proto::{ControlKind, Orientation, SlotRole};
use surface_widgets::Builder;

use super::parts::{self, TREADLE};
use super::pedals::PLUGIN;
use crate::face::{Face, canvas};
use crate::reverb::ui;

const BLUE: &str = "#3f5fb3";

fn rounded_rect(
    name: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    r: f64,
    fill: document_proto::Paint,
) -> Node {
    draw::rounded(draw::rect(name, x, y, w, h, fill), r)
}

/// Rows of embossed chevrons across the pad (`x, y, w, h`).
fn chevrons(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    let peaks = 3.0;
    let pitch = w / peaks;
    let amp = pitch * 0.55;
    let rows = ((h + amp) / 24.0).ceil() as usize;
    let mut dark = Vec::new();
    let mut light = Vec::new();
    for r in 0..rows {
        let base = y - amp * 0.3 + r as f64 * 24.0;
        let line: Vec<(f64, f64)> = (0..=(peaks as usize * 2))
            .map(|i| {
                (
                    x + pitch / 2.0 * i as f64,
                    base + if i % 2 == 0 { 0.0 } else { amp },
                )
            })
            .map(|(px, py)| (px, py.clamp(y + 1.0, y + h - 1.0)))
            .collect();
        light.push(
            line.iter()
                .map(|(px, py)| (*px, (py + 1.6).min(y + h - 1.0)))
                .collect::<Vec<_>>(),
        );
        dark.push(line);
    }
    b.add(
        parent,
        ui::strokes("ridge light", &light, 1.0, draw::solid("#8d8d8a80")),
    );
    b.add(
        parent,
        ui::strokes("ridges", &dark, 3.2, draw::solid("#1b1b1b")),
    );
}

pub(super) fn face() -> Face {
    let scene = canvas("Funk Wah", |b, root| {
        let (w, h) = TREADLE;
        parts::page(b, root, w, h);
        // The base casting, its top lip showing above the treadle.
        let base = rounded_rect(
            "base",
            18.0,
            0.0,
            w - 36.0,
            h - 6.0,
            40.0,
            draw::linear(170.0, &[(0.0, "#5a79c9"), (0.5, BLUE), (1.0, "#2c4488")]),
        );
        b.add(
            root,
            draw::with_effects(
                base,
                vec![
                    draw::drop_shadow("#00000080", 0.0, 4.0, 10.0),
                    draw::inner_shadow("#ffffff40", 0.0, 1.5, 1.0),
                ],
            ),
        );
        // Side hinge studs.
        for x in [10.0, w - 14.0] {
            b.add(
                root,
                rounded_rect(
                    "hinge",
                    x,
                    330.0,
                    4.0,
                    34.0,
                    1.5,
                    draw::linear(
                        90.0,
                        &[(0.0, "#6d6e70"), (0.5, "#e9e9e6"), (1.0, "#6d6e70")],
                    ),
                ),
            );
            parts::screw(b, root, x + 2.0, 470.0, 3.0, "#8a8b88");
        }
        // The treadle: an outer rim, a raised inner rim, the tread.
        let (tx, ty, tw, th) = (8.0, 44.0, w - 16.0, h - 56.0);
        let rim = rounded_rect(
            "treadle",
            tx,
            ty,
            tw,
            th,
            30.0,
            draw::linear(
                165.0,
                &[(0.0, "#6b89d6"), (0.4, "#4565b8"), (1.0, "#2a4185")],
            ),
        );
        b.add(
            root,
            draw::with_effects(
                rim,
                vec![
                    draw::drop_shadow("#00000099", 0.0, 3.0, 8.0),
                    draw::inner_shadow("#ffffff4d", 0.0, 1.5, 1.0),
                    draw::inner_shadow("#00000059", 0.0, -2.0, 4.0),
                ],
            ),
        );
        let inner = rounded_rect(
            "rim",
            tx + 7.0,
            ty + 7.0,
            tw - 14.0,
            th - 14.0,
            25.0,
            draw::linear(170.0, &[(0.0, "#5877c8"), (1.0, "#324d9a")]),
        );
        b.add(
            root,
            draw::with_effects(
                inner,
                vec![
                    draw::drop_shadow("#00000066", 0.0, 1.0, 3.0),
                    draw::inner_shadow("#ffffff33", 0.0, 1.0, 0.5),
                ],
            ),
        );
        let (px, py, pw, ph) = (tx + 19.0, ty + 19.0, tw - 38.0, th - 38.0);
        let mut pad = draw::frame("Tread", px, py, pw, ph);
        pad.fills = vec![draw::linear(180.0, &[(0.0, "#4a4a48"), (1.0, "#3a3a38")])];
        pad.corner_radii = document_proto::CornerRadii::uniform(10.0);
        pad.clips_content = true;
        pad.effects = vec![draw::inner_shadow("#000000b3", 0.0, 2.0, 5.0)];
        let pad = b.add(root, pad);
        chevrons(b, pad, (0.0, 0.0, pw, ph));
        // The treadle's position: the light on the tilted tread rides a
        // lane the pad's height.
        let lane = b.add(pad, draw::frame("lane", 0.0, 0.0, pw, ph));
        let band_h = 120.0;
        let band = b.add(lane, draw::frame("tilt", 0.0, 0.0, pw, band_h));
        b.add(
            band,
            draw::rect(
                "sheen",
                0.0,
                0.0,
                pw,
                band_h,
                draw::linear(
                    180.0,
                    &[(0.0, "#ffffff00"), (0.5, "#ffffff1f"), (1.0, "#ffffff00")],
                ),
            ),
        );
        b.add(
            band,
            draw::rect(
                "toe mark",
                pw * 0.3,
                band_h / 2.0 - 1.0,
                pw * 0.4,
                2.0,
                draw::solid("#ffffff3d"),
            ),
        );
        let mut c = surface_proto::Control::new(ControlKind::Fader {
            orientation: Orientation::Vertical,
        })
        .slot(SlotRole::Thumb, band.0);
        c.binding = Some(ui::bind("wah/position"));
        b.controls.insert(lane, c);
        // The toe lip engages it; its LED sits in the base above.
        let lip = b.add(
            root,
            draw::frame("Toe switch", tx + 50.0, 0.0, tw - 100.0, ty),
        );
        let mut fc = surface_proto::Control::new(ControlKind::footswitch());
        fc.binding = Some(ui::bind("wah/on"));
        b.controls.insert(lip, fc);
        parts::status_led(
            b,
            root,
            "wah/on",
            6.0,
            (w / 2.0, 22.0),
            "#ff4630",
            "dark",
            None,
        );
    });
    let params = vec![
        parts::pct("wah/position", "Position", 50.0),
        parts::switch("wah/on", "Engage", true),
    ];
    ui::formatted(Face {
        file: "01-wah-treadle-241x600",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    })
}
