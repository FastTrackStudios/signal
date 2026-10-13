//! The Blackout head — Head form, 1200 × 460: the plugin's black
//! ("blackout") faceplate at its 2.9 : 1 aspect, plus its handle above.
//!
//! Black leather tolex with a welt round the front and a seam a third of
//! the way down; the invented maker's plate top right in squared extended
//! capitals; a recessed black control panel low across the front: a green
//! power jewel, OFF/ON (power) and STANDBY toggles, then MVC (pull for
//! more gain), PRESENCE, BASS, MIDDLE, TREBLE, GAIN II and GAIN I, each a
//! black knob in a printed 0–10 scale, and the four inputs (HIGH / LOW ×
//! channel I / II) at the right — the cable's plug shows which is in use.

use frame::document::proto::{NodeId, TextAlign};
use frame::surface::kit::{Place, TOUCH_MIN, draw, motion};
use frame::surface::proto::{ButtonMode, Control, ControlKind, SlotRole};
use frame::surface::widgets::Builder;

use super::PLUGIN;
use super::art;
use crate::delay::parts::{self, Ink};
use crate::face::{Face, canvas};
use crate::rigkit::{self as rk, Letter};

const W: f64 = 1200.0;
const H: f64 = 460.0;
/// Where the head's box starts (the handle is above it).
const TOP: f64 = 44.0;
const PANEL: &str = "#1d1d1d";
const PRINT: &str = "#d9d9d6";
const KNOB_Y: f64 = 381.0;
const LEGEND_Y: f64 = 345.0;
const UNDER_Y: f64 = 419.0;

fn a(s: &str) -> String {
    format!("bgamp/{s}")
}

pub fn face() -> Face {
    let scene = canvas("Blackout Head", |b, root| {
        parts::fit_root(b, root, W, H);
        handle(b, root);
        // The box: tolex, the welt, the seam.
        let shadow = draw::rounded(
            draw::rect(
                "box shadow",
                4.0,
                TOP + 8.0,
                W - 8.0,
                H - TOP - 8.0,
                draw::solid("#000000"),
            ),
            24.0,
        );
        b.add(
            root,
            draw::with_effects(shadow, vec![draw::drop_shadow("#000000c0", 0.0, 6.0, 14.0)]),
        );
        let head = art::tolex(b, root, "Head", (0.0, TOP, W, H - TOP), 22.0, 0xb1ac_0417);
        let hh = H - TOP;
        // The box's rounded edge: the covering turns away from the light.
        let edge = draw::rounded(draw::frame("edge", 0.0, 0.0, W, hh), 22.0);
        let edge = draw::stroked(edge, "#050505", 3.0);
        b.add(
            head,
            draw::with_effects(
                edge,
                vec![
                    draw::inner_shadow("#000000e0", 0.0, 0.0, 14.0),
                    draw::inner_shadow("#ffffff26", 0.0, 2.0, 1.0),
                ],
            ),
        );
        art::piping(b, head, (20.0, 20.0, W - 40.0, hh - 40.0), 12.0);
        parts::rule(b, head, 22.0, W - 22.0, 148.0, 2.0, "#080808");
        parts::rule(b, head, 22.0, W - 22.0, 150.0, 1.0, "#ffffff18");
        maker_plate(b, head);
        // The recess the panel sits in: a deep shadow over it.
        let recess = draw::rounded(
            draw::rect(
                "recess",
                150.0,
                262.0,
                900.0,
                60.0,
                draw::linear(180.0, &[(0.0, "#00000000"), (1.0, "#000000d0")]),
            ),
            30.0,
        );
        b.add(
            head,
            draw::with_effects(recess, vec![draw::drop_shadow("#000000", 0.0, 0.0, 18.0)]),
        );
        let panel = control_panel(b, root);
        let _ = panel;
    });
    let mut params = vec![
        rk::toggle(&a("power"), "Power", true).featured(),
        rk::toggle(&a("standby"), "Standby", false),
        rk::dial10(&a("mvc"), "MVC", 5.0).featured(),
        rk::toggle(&a("pull_gain"), "Pull Gain", false),
        rk::dial10(&a("presence"), "Presence", 5.0),
        rk::dial10(&a("bass"), "Bass", 5.0),
        rk::dial10(&a("middle"), "Middle", 5.0),
        rk::dial10(&a("treble"), "Treble", 5.0),
        rk::dial10(&a("gain2"), "Gain II", 7.0).featured(),
        rk::dial10(&a("gain1"), "Gain I", 6.0).featured(),
    ];
    params.push(rk::choice(&a("input"), "Input", INPUTS, 0));
    Face {
        file: "04-amp-head-blackout",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

const INPUTS: &[&str] = &["High I", "High II", "Low I", "Low II"];

/// The carry handle: two chrome end caps and a leather strap arching
/// between them.
fn handle(b: &mut Builder<'_>, root: NodeId) {
    let (x0, x1) = (362.0, 838.0);
    for (x, w) in [(x0, 90.0), (x1 - 90.0, 90.0)] {
        let cap = draw::rounded(
            draw::rect(
                "handle cap",
                x,
                30.0,
                w,
                16.0,
                draw::linear(180.0, &[(0.0, "#3a3a3a"), (1.0, "#101010")]),
            ),
            3.0,
        );
        b.add(root, draw::stroked(cap, "#000000", 1.0));
    }
    rk::curve(
        b,
        root,
        "strap",
        (W, H),
        &[
            (x0 + 80.0, 34.0),
            (W / 2.0 - 150.0, 8.0),
            (W / 2.0 + 150.0, 8.0),
            (x1 - 80.0, 34.0),
        ],
        9.0,
        "#0c0c0c",
    );
    rk::curve(
        b,
        root,
        "strap light",
        (W, H),
        &[
            (x0 + 80.0, 31.0),
            (W / 2.0 - 150.0, 5.5),
            (W / 2.0 + 150.0, 5.5),
            (x1 - 80.0, 31.0),
        ],
        1.5,
        "#4a4a4a",
    );
    for x in [x0 + 80.0, x1 - 80.0] {
        b.add(
            root,
            draw::rounded(
                draw::rect(
                    "strap loop",
                    x - 7.0,
                    28.0,
                    14.0,
                    12.0,
                    draw::solid("#262626"),
                ),
                3.0,
            ),
        );
    }
}

/// The maker's plate, top right: a raised dark slab, the invented name in
/// squared extended capitals, bevel-lit.
fn maker_plate(b: &mut Builder<'_>, head: NodeId) {
    let (x, y, w, h) = (962.0, 39.0, 202.0, 38.0);
    let slab = draw::rounded(
        draw::rect(
            "maker plate",
            x,
            y,
            w,
            h,
            draw::linear(180.0, &[(0.0, "#2e2e2e"), (1.0, "#151515")]),
        ),
        4.0,
    );
    b.add(
        head,
        draw::with_effects(
            slab,
            vec![
                draw::drop_shadow("#000000d0", 0.0, 2.0, 4.0),
                draw::inner_shadow("#ffffff22", 0.0, 1.0, 0.0),
            ],
        ),
    );
    let l = Letter::new(26.0, 640.0, "#cfcfcf")
        .stretch(1.32)
        .track(0.08);
    let (_, size) = rk::letter_fit(b, head, "VANTOM", (x + w / 2.0, y + h / 2.0), w - 22.0, l);
    let _ = size;
}

/// The recessed black control panel with everything on it; returns it.
#[allow(clippy::too_many_lines)]
fn control_panel(b: &mut Builder<'_>, root: NodeId) -> NodeId {
    let (px, py, pw, ph) = (171.0, 332.0, 856.0, 106.0);
    let mut plate = draw::frame("Control panel", px, py, pw, ph);
    plate.fills = vec![draw::linear(
        180.0,
        &[(0.0, "#262626"), (0.5, PANEL), (1.0, "#161616")],
    )];
    let plate = draw::with_effects(
        draw::stroked(draw::rounded(plate, 8.0), "#000000", 1.0),
        vec![
            draw::inner_shadow("#ffffff1c", 0.0, 1.0, 0.0),
            draw::drop_shadow("#000000", 0.0, -2.0, 6.0),
        ],
    );
    let panel = b.add(root, plate);
    // The panel's print is in the root's coordinates (everything below is
    // placed on `root`, over the panel).
    let legend = Ink::new(8.2, PRINT).weight(500.0).track(0.02);
    let scale = Ink::new(7.4, PRINT).weight(500.0).track(0.0).plain();

    // Power: the green jewel is the power switch's lamp, OFF/ON its toggle.
    let jewel = Place::new("Power lamp")
        .size(34.0)
        .variant("green")
        .bind(rk::bind(&a("power")))
        .touch(TOUCH_MIN);
    let jewel = parts::put(b, root, "button.jewel", &jewel, 212.0, KNOB_Y);
    if let Some(lamp) = b
        .controls
        .get(&jewel)
        .and_then(|c| c.slots_of(SlotRole::Led).next())
        .map(NodeId)
    {
        motion::add(b, lamp, motion::flicker(0.04));
    }
    for (addr, name, x) in [("power", "Off", 264.0), ("standby", "Standby", 315.0)] {
        let t = rk::mini_toggle(b, root, name, &a(addr), (x, KNOB_Y), 20.0, "chrome");
        parts::label_at(b, t, name, (x, LEGEND_Y), legend);
        rk::silk(b, root, "On", x, UNDER_Y, legend);
    }

    // The knobs, each in a printed 0–10.
    let knobs = [
        ("mvc", "MVC", 374.5),
        ("presence", "Presence", 451.0),
        ("bass", "Bass", 527.0),
        ("middle", "Middle", 606.0),
        ("treble", "Treble", 682.0),
        ("gain2", "", 759.0),
        ("gain1", "", 836.0),
    ];
    let marks: &[(f64, &str)] = &[
        (0.0, "0"),
        (0.2, "2"),
        (0.4, "4"),
        (0.6, "6"),
        (0.8, "8"),
        (1.0, "10"),
    ];
    for (addr, name, x) in knobs {
        let mut p = Place::new(if name.is_empty() { addr } else { name })
            .size(40.0)
            .variant("flat")
            .bind(rk::bind(&a(addr)))
            .touch(TOUCH_MIN);
        p.options = Some(super::AMP_KNOB.iter().map(ToString::to_string).collect());
        let k = parts::put(b, root, super::knob_id("knob.bakelite"), &p, x, KNOB_Y);
        parts::strip_legend(b, k);
        parts::strip_scale(b, k);
        let sweep = parts::sweep_of(b, k);
        parts::marks(b, root, (x, KNOB_Y), 23.0, sweep, marks, 0, scale);
        if name.is_empty() {
            let under = if addr == "gain2" { "Gain II" } else { "Gain I" };
            parts::label_at(b, k, under, (x, UNDER_Y), legend);
        } else {
            parts::label_at(b, k, name, (x, LEGEND_Y), legend);
        }
    }
    pull_gain(b, root, legend);
    inputs(b, root, legend);
    panel
}

/// MVC's pull switch: the PULL GAIN legend under the knob is the button;
/// it lights while the pot is pulled.
fn pull_gain(b: &mut Builder<'_>, root: NodeId, legend: Ink<'_>) {
    let (cx, cy) = (374.5, UNDER_Y);
    let f = b.add(
        root,
        draw::frame("Pull Gain", cx - 32.0, 404.0, 64.0, TOUCH_MIN),
    );
    let dim = parts::text_node(
        "Pull Gain",
        0.0,
        cy - 404.0,
        64.0,
        &legend.color("#6a6a68"),
        TextAlign::Center,
    );
    b.add(f, dim);
    let lit = parts::text_node(
        "Pull Gain",
        0.0,
        cy - 404.0,
        64.0,
        &legend,
        TextAlign::Center,
    );
    let lit = b.add(f, parts::emit(lit, "#ffffff", 0.3, 4.0));
    let mut c = Control::new(ControlKind::Button {
        mode: ButtonMode::Toggle,
    })
    .slot(SlotRole::Led, lit.0)
    .slot(SlotRole::Label, lit.0);
    c.binding = Some(rk::bind(&a("pull_gain")));
    b.controls.insert(f, c);
}

/// The four input jacks: a Select bank over the input param, the plug in
/// the chosen jack.
fn inputs(b: &mut Builder<'_>, root: NodeId, legend: Ink<'_>) {
    let jacks = [
        (914.0, 355.0),
        (991.0, 355.0),
        (914.0, 408.0),
        (991.0, 408.0),
    ];
    for (i, (cx, cy)) in jacks.iter().enumerate() {
        let f = b.add(
            root,
            draw::frame(INPUTS[i], cx - 22.0, cy - 22.0, 44.0, 44.0),
        );
        let (c0, c1) = (22.0, 22.0);
        let nut = draw::path(
            "jack nut",
            0.0,
            0.0,
            44.0,
            44.0,
            draw::regular(c0, c1, 12.5, 6, 0.0),
            draw::linear(
                160.0,
                &[(0.0, "#f2f2f0"), (0.5, "#7d7e7c"), (1.0, "#c9cac7")],
            ),
        );
        b.add(f, draw::stroked(nut, "#202020", 0.8));
        b.add(
            f,
            draw::circle(
                "jack ring",
                c0,
                c1,
                8.5,
                draw::radial(&[(0.0, "#6a6a6a"), (1.0, "#d8d8d6")]),
            ),
        );
        b.add(
            f,
            draw::circle("jack hole", c0, c1, 5.0, draw::solid("#050505")),
        );
        // The plug: a black barrel with a chrome ferrule, its cable
        // dropping away below the panel.
        let plug = b.add(f, draw::frame("plug", 0.0, 0.0, 44.0, 44.0));
        rk::curve(
            b,
            plug,
            "cable",
            (44.0, 44.0),
            &[
                (c0, c1),
                (c0 + 4.0, c1 + 20.0),
                (c0 + 30.0, c1 + 60.0),
                (c0 + 80.0, c1 + 110.0),
            ],
            5.0,
            "#0a0a0a",
        );
        b.add(
            plug,
            draw::circle(
                "plug ferrule",
                c0,
                c1,
                7.0,
                draw::conic(0.0, &[(0.0, "#e8e8e6"), (0.5, "#6b6b69"), (1.0, "#e8e8e6")]),
            ),
        );
        b.add(
            plug,
            draw::circle(
                "plug barrel",
                c0,
                c1,
                5.2,
                draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#4a4a4a"), (1.0, "#080808")]),
            ),
        );
        let mut c = Control::new(ControlKind::Button {
            mode: ButtonMode::Select {
                index: i as u32,
                count: 4,
            },
        })
        .slot(SlotRole::Led, plug.0);
        c.binding = Some(rk::bind(&a("input")));
        b.controls.insert(f, c);
    }
    let serif = Letter::new(11.0, 500.0, PRINT).stretch(0.8);
    for (y, word) in [(355.0, "HIGH"), (408.0, "LOW")] {
        rk::letter(b, root, "I", (894.0, y), serif);
        rk::letter(b, root, "II", (1016.0, y), serif);
        rk::silk(
            b,
            root,
            word,
            953.0,
            y,
            Ink::new(10.0, PRINT).weight(500.0).track(0.02),
        );
    }
    let _ = legend;
}
