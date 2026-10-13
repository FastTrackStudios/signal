//! The three heads, in the Head form at the plugin's aspect: 1200 × 500
//! design px — the box (1200 × 446, its tolex border included), its handle
//! on top and its feet under it.
//!
//! - `04-amp-flame-lead-head` — pink tolex round violet hammered glass lit
//!   by its tubes; a pale marble faceplate with seven pink-capped knobs; a
//!   flat strap handle between pink brackets, black rubber feet.
//! - `05-amp-cherub-rhythm-head` — all white: a plaster relief (shield,
//!   scrolls, a cherub at each end), a channel lever among six white knobs;
//!   a chrome bar handle on posts, white flared feet.
//! - `06-amp-bloom-acoustic-head` — all black: a floral relief (roses,
//!   hearts, blossoms, wheat), seven black knurled knobs with a blend
//!   control; a black strap handle, black feet.
//!
//! Each faceplate reads left to right: the input jack with its cable, the
//! knobs under tracked capitals and inside printed tick rings, the power
//! button and its white pilot bar, the maker's scribbled mark.

use document_proto::{CornerRadii, NodeId};
use surface_kit::draw;
use surface_kit::materials::light;
use surface_kit::materials::texture::Relief;
use surface_proto::{ButtonMode, Control, ControlKind, ParamInfo, SlotRole};
use surface_widgets::Builder;

use super::parts::{self, Cap, knob, switch};
use super::{PLUGIN, art};
use crate::face::{Face, canvas};
use crate::reverb::ui;

pub const W: f64 = 1200.0;
pub const H: f64 = 500.0;
/// The box: under the handle, over the feet.
const BOX_Y: f64 = 24.0;
const BOX_H: f64 = 446.0;
/// The opening inside the tolex border (box coordinates).
const IX: f64 = 33.0;
const IY: f64 = 34.0;
const IW: f64 = W - 2.0 * IX;
/// The art's height above the faceplate, and the faceplate's.
const ART_H: f64 = 259.0;
const PLATE_H: f64 = 116.0;

/// What's on top.
enum Handle {
    /// A flat strap lying between two brackets in the tolex's colour.
    Strap {
        bracket: &'static str,
        strap: &'static str,
    },
    /// A chrome bar on two posts.
    Bar,
}

struct Look {
    tolex: &'static str,
    plate: [&'static str; 2],
    ink: &'static str,
    cap: Cap,
    knob_d: f64,
    cable: &'static str,
    handle: Handle,
    /// The feet's colour, and whether they flare (the white head's legs).
    feet: (&'static str, bool),
}

enum Art {
    Glass,
    Relief,
    Floral,
}

/// A faceplate control: a knob, or the channel lever.
enum Ctl {
    Knob(&'static str, &'static str, f64),
    Channel(&'static str, f64),
}

fn handle(b: &mut Builder<'_>, root: NodeId, h: &Handle) {
    let (x0, x1) = (W * 0.3, W * 0.7);
    match h {
        Handle::Strap { bracket, strap } => {
            // The strap: a flat band sagging a hair between its ends.
            let d = format!(
                "M {:.1} 16 Q {:.1} 6 {:.1} 16 L {:.1} 24 Q {:.1} 14 {:.1} 24 Z",
                x0 + 30.0,
                W / 2.0,
                x1 - 30.0,
                x1 - 30.0,
                W / 2.0,
                x0 + 30.0
            );
            let band = draw::path(
                "handle strap",
                0.0,
                0.0,
                W,
                H,
                d,
                draw::linear(
                    180.0,
                    &[
                        (0.0, &ui::blend(strap, "#ffffff", 0.25)),
                        (0.5, strap),
                        (1.0, &ui::blend(strap, "#000000", 0.5)),
                    ],
                ),
            );
            b.add(root, draw::with_effects(band, light::stand(3.0, 0.9)));
            for x in [x0, x1 - 64.0] {
                let br = draw::rounded(
                    draw::rect(
                        "handle bracket",
                        x,
                        12.0,
                        64.0,
                        BOX_Y - 6.0,
                        draw::linear(
                            180.0,
                            &[
                                (0.0, &ui::blend(bracket, "#ffffff", 0.35)),
                                (0.6, bracket),
                                (1.0, &ui::blend(bracket, "#000000", 0.3)),
                            ],
                        ),
                    ),
                    5.0,
                );
                b.add(root, draw::with_effects(br, light::stand(2.5, 0.9)));
                b.add(
                    root,
                    draw::rect(
                        "bracket seam",
                        x + 6.0,
                        13.5,
                        52.0,
                        1.0,
                        draw::solid("#ffffff55"),
                    ),
                );
            }
        }
        Handle::Bar => {
            let chrome = draw::linear(
                180.0,
                &[
                    (0.0, "#ffffff"),
                    (0.35, "#c9ccd0"),
                    (0.55, "#7d8187"),
                    (1.0, "#e4e6e9"),
                ],
            );
            for x in [x0 + 20.0, x1 - 20.0] {
                let post = draw::rounded(
                    draw::rect(
                        "handle post",
                        x - 7.0,
                        2.0,
                        14.0,
                        BOX_Y + 2.0,
                        draw::linear(
                            90.0,
                            &[(0.0, "#8c9096"), (0.4, "#ffffff"), (1.0, "#6c7076")],
                        ),
                    ),
                    3.0,
                );
                b.add(root, draw::with_effects(post, light::stand(2.0, 0.8)));
                b.add(
                    root,
                    draw::rounded(
                        draw::rect(
                            "post base",
                            x - 12.0,
                            BOX_Y - 4.0,
                            24.0,
                            6.0,
                            draw::solid("#b8bbc0"),
                        ),
                        2.0,
                    ),
                );
            }
            let d = format!(
                "M {:.1} 6 Q {:.1} -4 {:.1} 6 L {:.1} 12 Q {:.1} 2 {:.1} 12 Z",
                x0 + 16.0,
                W / 2.0,
                x1 - 16.0,
                x1 - 16.0,
                W / 2.0,
                x0 + 16.0
            );
            b.add(
                root,
                draw::with_effects(
                    draw::path("handle bar", 0.0, 0.0, W, H, d, chrome),
                    light::stand(2.5, 0.8),
                ),
            );
        }
    }
}

fn feet(b: &mut Builder<'_>, root: NodeId, (color, flare): (&str, bool)) {
    let top = BOX_Y + BOX_H - 14.0;
    for cx in [W * 0.115, W * 0.885] {
        let (wt, wb) = if flare { (104.0, 66.0) } else { (92.0, 80.0) };
        let pts = [
            (cx - wt / 2.0, top),
            (cx + wt / 2.0, top),
            (cx + wb / 2.0, H - 2.0),
            (cx - wb / 2.0, H - 2.0),
        ];
        let paint = draw::linear(
            90.0,
            &[
                (0.0, &ui::blend(color, "#000000", 0.45)),
                (0.35, &ui::blend(color, "#ffffff", 0.12)),
                (0.7, color),
                (1.0, &ui::blend(color, "#000000", 0.55)),
            ],
        );
        let foot = draw::path("foot", 0.0, 0.0, W, H, draw::polygon(&pts), paint);
        b.add(
            root,
            draw::with_effects(foot, vec![draw::drop_shadow("#00000080", 0.0, 3.0, 4.0)]),
        );
    }
}

fn head(b: &mut Builder<'_>, root: NodeId, ns: &str, look: &Look, art_kind: &Art, ctls: &[Ctl]) {
    ui::fit_root(b, root, W, H);
    // Feet and handle behind; then the box.
    feet(b, root, look.feet);
    handle(b, root, &look.handle);
    let r = 26.0;
    let shadow = draw::rounded(
        draw::rect("box shadow", 0.0, BOX_Y, W, BOX_H, draw::solid(look.tolex)),
        r,
    );
    b.add(root, draw::with_effects(shadow, light::stand(14.0, 1.0)));
    let mut bx = draw::frame("Head", 0.0, BOX_Y, W, BOX_H);
    bx.corner_radii = CornerRadii::uniform(r);
    let tolex = b.add(root, bx);
    parts::covering(
        b,
        tolex,
        (0.0, 0.0, W, BOX_H),
        r,
        look.tolex,
        Relief::Leather,
        0.8,
        1.0,
    );
    // The covering rolling round the box's edges: lit top-left, shaded bottom-right.
    for n in light::edge("box edge", (0.0, 0.0, W, BOX_H), r, 16.0, 1.1, true) {
        b.add(tolex, n);
    }
    // The opening's lip: the tolex folding in round the art and faceplate.
    for n in light::edge(
        "opening lip",
        (IX - 4.0, IY - 4.0, IW + 8.0, ART_H + PLATE_H + 8.0),
        8.0,
        5.0,
        1.2,
        false,
    ) {
        b.add(tolex, n);
    }

    let mut art_frame = draw::frame("Front art", IX, IY, IW, ART_H);
    art_frame.clips_content = true;
    art_frame.corner_radii = CornerRadii {
        top_left: 5.0,
        top_right: 5.0,
        bottom_left: 0.0,
        bottom_right: 0.0,
    };
    let art_id = b.add(tolex, art_frame);
    match art_kind {
        Art::Glass => art::hammered_glass(b, art_id, IW, ART_H),
        Art::Relief => art::relief_white(b, art_id, IW, ART_H),
        Art::Floral => art::floral_black(b, art_id, IW, ART_H),
    }

    let mut plate = draw::frame("Faceplate", IX, IY + ART_H, IW, PLATE_H);
    plate.fills = vec![
        draw::linear(180.0, &[(0.0, look.plate[0]), (1.0, look.plate[1])]),
        draw::linear(
            100.0,
            &[
                (0.0, "#ffffff00"),
                (0.35, "#ffffff14"),
                (0.5, "#ffffff00"),
                (0.8, "#0000000a"),
                (1.0, "#ffffff00"),
            ],
        ),
    ];
    plate.corner_radii = CornerRadii {
        top_left: 0.0,
        top_right: 0.0,
        bottom_left: 5.0,
        bottom_right: 5.0,
    };
    let plate = b.add(tolex, plate);
    // The plate's top edge catching the light, its lower edge in shadow.
    b.add(
        plate,
        draw::rect(
            "plate lit edge",
            0.0,
            0.0,
            IW,
            1.5,
            draw::solid("#ffffff66"),
        ),
    );
    b.add(
        plate,
        draw::rect(
            "plate shade",
            0.0,
            1.5,
            IW,
            5.0,
            draw::linear(180.0, &[(0.0, "#00000030"), (1.0, "#00000000")]),
        ),
    );
    b.add(
        plate,
        draw::rect(
            "plate foot",
            0.0,
            PLATE_H - 3.0,
            IW,
            3.0,
            draw::linear(180.0, &[(0.0, "#00000000"), (1.0, "#00000030")]),
        ),
    );

    // The input jack and its cable, curling out under the head.
    let (jx, jy) = (152.0, 71.0);
    let cable = [
        (jx, jy),
        (jx - 4.0, jy + 30.0),
        (jx - 22.0, PLATE_H + 40.0),
        (jx - 70.0, PLATE_H + 64.0),
        (jx - 150.0, PLATE_H + 76.0),
    ];
    b.add(
        plate,
        draw::with_effects(
            ui::line_node("cable", &cable, 8.0, look.cable),
            vec![draw::drop_shadow("#00000080", 1.0, 3.0, 3.0)],
        ),
    );
    b.add(
        plate,
        ui::line_node(
            "cable sheen",
            &cable
                .iter()
                .map(|(x, y)| (x - 1.5, y - 1.0))
                .collect::<Vec<_>>(),
            1.6,
            &ui::blend(look.cable, "#ffffff", 0.35),
        ),
    );
    b.add(
        plate,
        draw::path(
            "jack nut",
            0.0,
            0.0,
            IW,
            PLATE_H,
            draw::regular(jx, jy, 15.0, 6, 0.0),
            draw::linear(
                150.0,
                &[(0.0, "#f4f4f2"), (0.5, "#9a9b99"), (1.0, "#e0e0de")],
            ),
        ),
    );
    b.add(
        plate,
        draw::circle(
            "plug",
            jx,
            jy,
            9.5,
            draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#ffffff"), (1.0, "#6e6f71")]),
        ),
    );
    b.add(
        plate,
        draw::circle("plug tip", jx, jy, 4.0, draw::solid(look.cable)),
    );

    for c in ctls {
        match c {
            Ctl::Knob(id, legend, x) => {
                parts::print(b, plate, legend, *x, 25.0, 7.5, look.ink, 700.0, 0.3);
                knob(
                    b,
                    plate,
                    &look.cap,
                    &format!("{ns}/{id}"),
                    legend,
                    look.knob_d,
                    *x,
                    71.0,
                    look.ink,
                );
            }
            Ctl::Channel(id, x) => {
                parts::print(b, plate, "CH1", *x, 50.0, 6.0, look.ink, 700.0, 0.12);
                switch(
                    b,
                    plate,
                    "switch.mini_lever",
                    "chrome",
                    &format!("{ns}/{id}"),
                    "Channel",
                    14.0,
                    *x,
                    71.0,
                    &["vertical"],
                    &["bushing"],
                );
                parts::print(b, plate, "CH2", *x, 93.0, 6.0, look.ink, 700.0, 0.12);
            }
        }
    }
    power(b, plate, &format!("{ns}/power"), 887.0, 71.0, look.cable);
    art::flourish(b, plate, 995.0, 71.0, 100.0, look.ink);
}

/// The power button and its pilot: a small dark push-button, and a white
/// bar that lights while the amp is on.
fn power(b: &mut Builder<'_>, parent: NodeId, addr: &str, cx: f64, cy: f64, dark: &str) {
    let root = b.add(
        parent,
        draw::frame("Power", cx - 22.0, cy - 22.0, 100.0, 44.0),
    );
    b.add(
        root,
        draw::with_effects(
            draw::circle("button ring", 22.0, 22.0, 9.5, draw::solid("#00000055")),
            vec![draw::drop_shadow("#ffffff30", 0.0, 1.0, 0.5)],
        ),
    );
    let btn = draw::circle(
        "button",
        22.0,
        22.0,
        8.0,
        draw::radial_at(
            0.38,
            0.3,
            0.9,
            &[(0.0, "#6a6a6e"), (0.6, dark), (1.0, "#000000")],
        ),
    );
    b.add(
        root,
        draw::with_effects(btn, vec![draw::drop_shadow("#00000099", 0.0, 1.5, 2.0)]),
    );
    b.add(
        root,
        draw::rounded(
            draw::rect(
                "pilot slot",
                70.0,
                19.0,
                24.0,
                6.0,
                draw::solid("#00000040"),
            ),
            3.0,
        ),
    );
    let bar = draw::rounded(
        draw::rect(
            "pilot",
            70.0,
            19.5,
            24.0,
            5.0,
            draw::linear(180.0, &[(0.0, "#ffffff"), (1.0, "#e4e8ff")]),
        ),
        2.5,
    );
    let bar = draw::with_effects(bar, vec![draw::drop_shadow("#ffffffcc", 0.0, 0.0, 5.0)]);
    let bar = b.add(root, draw::emissive(bar, draw::hex("#f4f6ff"), 0.5, 6.0));
    let mut c = Control::new(ControlKind::Button {
        mode: ButtonMode::Toggle,
    })
    .slot(SlotRole::Led, bar.0);
    c.binding = Some(ui::bind(addr));
    b.controls.insert(root, c);
}

fn dials(ns: &str, ids: &[(&str, &str, f64)]) -> Vec<ParamInfo> {
    let mut v: Vec<ParamInfo> = ids
        .iter()
        .map(|(id, name, d)| parts::dial10(ns, id, name, *d))
        .collect();
    if let Some(first) = v.first_mut() {
        *first = first.clone().featured();
    }
    v
}

const XS: [f64; 7] = [266.0, 354.0, 442.0, 530.0, 617.0, 706.0, 793.0];

pub fn flame_lead() -> Face {
    const NS: &str = "lead";
    let look = Look {
        tolex: "#e98aa6",
        plate: ["#ebe7e9", "#d9d3d6"],
        ink: "#3a2a31",
        cap: parts::LEAD,
        knob_d: 40.0,
        cable: "#1e1e20",
        handle: Handle::Strap {
            bracket: "#e27f9d",
            strap: "#2a2426",
        },
        feet: ("#38383b", false),
    };
    let scene = canvas("Flame Lead (Head, 1200×500)", |b, root| {
        let names = [
            ("gain", "GAIN"),
            ("bass", "BASS"),
            ("middle", "MIDDLE"),
            ("treble", "TREBLE"),
            ("presence", "PRESENCE"),
            ("master", "MASTER"),
            ("output", "OUTPUT"),
        ];
        let ctls: Vec<Ctl> = names
            .iter()
            .zip(XS)
            .map(|((id, l), x)| Ctl::Knob(id, l, x))
            .collect();
        head(b, root, NS, &look, &Art::Glass, &ctls);
    });
    let mut params = dials(
        NS,
        &[
            ("gain", "Gain", 4.3),
            ("bass", "Bass", 5.0),
            ("middle", "Middle", 5.0),
            ("treble", "Treble", 5.0),
            ("presence", "Presence", 5.0),
            ("master", "Master", 5.0),
            ("output", "Output", 5.0),
        ],
    );
    params.push(parts::on(NS, "power", "Power", true).featured());
    Face {
        file: "04-amp-flame-lead-head",
        plugin: PLUGIN,
        scene,
        params,
        streams: vec![],
    }
}

pub fn cherub_rhythm() -> Face {
    const NS: &str = "rhythm";
    let look = Look {
        tolex: "#ececea",
        plate: ["#eeeeec", "#e0e0de"],
        ink: "#454545",
        cap: parts::RHYTHM,
        knob_d: 42.0,
        cable: "#e8e8e6",
        handle: Handle::Bar,
        feet: ("#e6e6e4", true),
    };
    let scene = canvas("Cherub Rhythm (Head, 1200×500)", |b, root| {
        let ctls = [
            Ctl::Knob("gain", "GAIN", 267.0),
            Ctl::Channel("channel", 338.0),
            Ctl::Knob("bass", "BASS", 411.0),
            Ctl::Knob("middle", "MIDDLE", 507.0),
            Ctl::Knob("treble", "TREBLE", 602.0),
            Ctl::Knob("presence", "PRESENCE", 699.0),
            Ctl::Knob("output", "OUTPUT", 794.0),
        ];
        head(b, root, NS, &look, &Art::Relief, &ctls);
    });
    let mut params = dials(
        NS,
        &[
            ("gain", "Gain", 5.0),
            ("bass", "Bass", 5.0),
            ("middle", "Middle", 5.0),
            ("treble", "Treble", 5.0),
            ("presence", "Presence", 5.0),
            ("output", "Output", 5.0),
        ],
    );
    params.push(parts::on(NS, "channel", "Channel 1", true));
    params.push(parts::on(NS, "power", "Power", true).featured());
    Face {
        file: "05-amp-cherub-rhythm-head",
        plugin: PLUGIN,
        scene,
        params,
        streams: vec![],
    }
}

pub fn bloom_acoustic() -> Face {
    const NS: &str = "acoustic";
    let look = Look {
        tolex: "#1f1f21",
        plate: ["#232325", "#19191a"],
        ink: "#e9e9e9",
        cap: parts::ACOUSTIC,
        knob_d: 40.0,
        cable: "#0c0c0d",
        handle: Handle::Strap {
            bracket: "#2a2a2c",
            strap: "#111112",
        },
        feet: ("#303033", false),
    };
    let scene = canvas("Bloom Acoustic (Head, 1200×500)", |b, root| {
        let names = [
            ("gain", "GAIN"),
            ("bass", "BASS"),
            ("middle", "MIDDLE"),
            ("treble", "TREBLE"),
            ("presence", "PRESENCE"),
            ("blend", "BLEND"),
            ("output", "OUTPUT"),
        ];
        let ctls: Vec<Ctl> = names
            .iter()
            .zip(XS)
            .map(|((id, l), x)| Ctl::Knob(id, l, x))
            .collect();
        head(b, root, NS, &look, &Art::Floral, &ctls);
    });
    let mut params = dials(
        NS,
        &[
            ("gain", "Gain", 5.0),
            ("bass", "Bass", 5.0),
            ("middle", "Middle", 5.0),
            ("treble", "Treble", 5.0),
            ("presence", "Presence", 5.0),
            ("blend", "Blend", 5.0),
            ("output", "Output", 5.0),
        ],
    );
    params.push(parts::on(NS, "power", "Power", true).featured());
    Face {
        file: "06-amp-bloom-acoustic-head",
        plugin: PLUGIN,
        scene,
        params,
        streams: vec![],
    }
}
