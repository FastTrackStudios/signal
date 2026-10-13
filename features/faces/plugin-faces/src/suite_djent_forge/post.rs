//! Djent Forge post-FX and special FX.
//!
//! - **Treadle Modulator** — Pedal, Wide (222 × 286): a rose-gold box with
//!   a rubber-treaded rocker on the left (a vertical `Fader`, heel to toe)
//!   and a small black screen on the right: CHORUS / FLANGER, the MIX,
//!   DEPTH and RATE ranges the rocker sweeps (two handles each, H and T),
//!   the auto speed; PEDAL / AUTO toggle, AUTO RANDOM and AUTO SPEED knobs,
//!   the LED and ENGAGE stomp.
//! - **Stereo Echo** — Pedal, BigBox (404 × 286): black, two outlined
//!   panels — MIX and FEEDBACK round SYNC / MODE keys over a red
//!   sixteen-segment display, then low cut, TIME, MOD, DUCKER, high cut —
//!   and ENGAGE / TAP TEMPO stomps with red LEDs beside the name.
//! - **Shimmer Verb** — Pedal, Standard (156 × 286): midnight blue, MIX,
//!   PRE DELAY, DECAY, low and high cut round a SHIMMER / NORMAL toggle.
//! - **Beam Treadle** — Pedal, Wide (222 × 286): a violet pitch treadle
//!   with a patch list on its screen, PEDAL / ENVELOPE, SPEED and
//!   SENSITIVITY, a lit TRIGGER button.
//! - **Fracture** — Tabletop, 800 × 479: a big grain-glitch unit with a
//!   screen (waveform, loop length, read point, mode, grain / mix / sync
//!   sections of green-arc dials) above ENGAGE and TEMPO stomps.

use document_proto::{NodeId, TextAlign};
use surface_kit::materials::{self, Material, light};
use surface_kit::{Place, draw};
use surface_proto::{
    Axis, Binding, ButtonMode, Control, ControlKind, HandleAxis, Orientation, SlotRole, ValueFormat,
};
use surface_widgets::Builder;

use super::type_::{fine, legend, mark, name};
use super::{LED_BLUE, PLUGIN, pedal_knob};
use crate::delay::parts::{self, Ink};
use crate::face::{Face, canvas};
use crate::rigkit::{self as rk, Enclosure, KnobStyle, Letter, lcd};

pub fn faces() -> Vec<Face> {
    vec![
        modulator(),
        stereo_echo(),
        shimmer_verb(),
        beam(),
        fracture(),
    ]
}

const KNOB: KnobStyle<'static> = pedal_knob(40.0);
const WHITE: &str = "#f2f2ef";

// ── shared parts ─────────────────────────────────────────────────────────

/// The rubber tread on a rocker: a dark pad, two straight centre grooves
/// and the curved side grooves of a tyre.
fn tread(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64)) {
    // The pad: moulded rubber standing a little off the plate.
    let under = draw::rounded(
        draw::rect("tread shadow", x, y, w, h, draw::solid("#141416")),
        3.0,
    );
    b.add(parent, draw::with_effects(under, light::stand(2.5, 1.0)));
    let pad = materials::fill(
        b,
        parent,
        (x, y, w, h),
        3.0,
        &Material::Rubber {
            color: "#2b2c2f".into(),
        },
    );
    let cx = w / 2.0;
    let mut grooves: Vec<Vec<(f64, f64)>> = Vec::new();
    // Straight channels down the middle.
    for dx in [-0.1, -0.045, 0.0, 0.045, 0.1] {
        grooves.push(vec![(cx + dx * w, -2.0), (cx + dx * w, h + 2.0)]);
    }
    let pitch = h / 5.2;
    for side in [-1.0_f64, 1.0] {
        // The shoulder groove, and short blades across the shoulder.
        grooves.push(
            (0..=12)
                .map(|k| {
                    let v = f64::from(k) / 12.0;
                    (
                        cx + side * w * (0.39 + 0.012 * (v * 12.0).sin()),
                        -2.0 + (h + 4.0) * v,
                    )
                })
                .collect(),
        );
        let mut sy = -pitch * 0.2;
        while sy < h + 2.0 {
            grooves.push(vec![
                (cx + side * w * 0.395, sy),
                (cx + side * w * 0.52, sy + 1.2),
            ]);
            sy += pitch * 0.5;
        }
        // The long blades: steep off the channels, flattening as they
        // sweep out to the shoulder.
        let mut ys = -pitch * 2.0;
        while ys < h + 2.0 {
            let len = pitch * 1.9;
            grooves.push(
                (0..=14)
                    .map(|k| {
                        let u = f64::from(k) / 14.0;
                        (
                            cx + side * w * (0.14 + 0.25 * u),
                            ys + len * (1.0 - (1.0 - u) * (1.0 - u)),
                        )
                    })
                    .collect(),
            );
            ys += pitch;
        }
    }
    let lit: Vec<Vec<(f64, f64)>> = grooves
        .iter()
        .map(|g| g.iter().map(|(px, py)| (px + 0.4, py + 1.2)).collect())
        .collect();
    b.add(
        pad,
        parts::stroke_path(
            "groove light",
            (0.0, 0.0, w, h),
            &lit,
            0.9,
            "#ffffff1f",
            false,
        ),
    );
    b.add(
        pad,
        parts::stroke_path("grooves", (0.0, 0.0, w, h), &grooves, 1.7, "#050506", false),
    );
}

/// The rocker a treadle unit is played with: a raised plate with two tread
/// pads and the name between; the plate is a vertical `Fader` (heel at the
/// bottom, toe at the top), a small lit marker on its edge showing where.
fn rocker(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    addr: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    plate: &str,
    marker: &str,
) -> NodeId {
    let mut f = draw::frame(name, x, y, w, h);
    f.fills = vec![draw::linear(
        180.0,
        &[
            (0.0, &parts::mixed(plate, "#ffffff", 0.18)),
            (1.0, &parts::mixed(plate, "#000000", 0.12)),
        ],
    )];
    let root = b.add(
        parent,
        draw::with_effects(
            draw::stroked(draw::rounded(f, 4.0), "#00000066", 0.8),
            vec![
                draw::drop_shadow("#00000080", 0.0, 4.0, 6.0),
                draw::inner_shadow("#ffffff40", 0.0, 1.0, 1.0),
            ],
        ),
    );
    tread(b, root, (5.0, 5.0, w - 10.0, h * 0.37));
    tread(b, root, (5.0, h * 0.6, w - 10.0, h * 0.37 + 1.0));
    // The travel: a lane down the plate's right edge.
    let lane = b.add(root, draw::frame("travel", w - 4.0, 6.0, 3.0, h - 12.0));
    let thumb = b.add(
        lane,
        parts::emit(
            draw::rounded(
                draw::rect("position", 0.0, 0.0, 3.0, 10.0, draw::solid(marker)),
                1.5,
            ),
            marker,
            0.6,
            6.0,
        ),
    );
    let mut c = Control::new(ControlKind::Fader {
        orientation: Orientation::Vertical,
    })
    .slot(SlotRole::Thumb, thumb.0);
    c.binding = Some(rk::bind(addr));
    b.controls.insert(root, c);
    root
}

/// A black screen with a thin light bezel, returning its frame.
fn screen(b: &mut Builder<'_>, parent: NodeId, name: &str, r: (f64, f64, f64, f64)) -> NodeId {
    let s = parts::screen(
        b,
        parent,
        name,
        r,
        parts::Screen {
            glass: ("#050506", "#0d0e10"),
            ink: "#e6e6e6",
            mesh: false,
        },
    );
    if let Some(n) = b.scene.node_mut(s) {
        n.stroke = document_proto::Stroke {
            paints: vec![draw::solid("#8c8f96")],
            weight: 0.8,
            ..document_proto::Stroke::default()
        };
    }
    s
}

/// A mini toggle with a tight box `touch` wide, centred on (`cx`, `cy`).
fn toggle(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    addr: &str,
    (cx, cy): (f64, f64),
    size: f64,
    touch: f64,
) -> NodeId {
    let p = Place::new(name)
        .size(size)
        .variant("black")
        .bind(rk::bind(addr))
        .legend("")
        .touch(touch);
    let t = parts::put(b, parent, "switch.toggle_nut", &p, cx, cy);
    parts::strip_legend(b, t);
    t
}

/// A lever lying in a dark oval slot, thrown left or right (the treadles'
/// mode switch): the chrome lever sits right while on.
fn slide_toggle(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    addr: &str,
    (cx, cy): (f64, f64),
) -> NodeId {
    let (w, h) = (24.0, 12.0);
    let root = b.add(parent, draw::frame(name, cx - w / 2.0, cy - h / 2.0, w, h));
    let slot = draw::rounded(
        draw::rect(
            "slot",
            1.0,
            2.0,
            w - 2.0,
            h - 4.0,
            draw::linear(180.0, &[(0.0, "#050506"), (1.0, "#25262a")]),
        ),
        (h - 4.0) / 2.0,
    );
    b.add(
        root,
        draw::with_effects(
            draw::stroked(slot, "#8a8c92", 0.6),
            vec![draw::inner_shadow("#000000", 0.0, 1.0, 2.0)],
        ),
    );
    let lever = |x: f64| {
        draw::rounded(
            draw::rect(
                "lever",
                x,
                3.2,
                9.0,
                h - 6.4,
                draw::linear(
                    180.0,
                    &[(0.0, "#f0f0ee"), (0.5, "#8d8e90"), (1.0, "#d0d0ce")],
                ),
            ),
            2.5,
        )
    };
    b.add(root, draw::with_opacity(lever(3.0), 0.35));
    let on = b.add(root, lever(w - 12.0));
    let mut c = Control::new(ControlKind::Button {
        mode: ButtonMode::Toggle,
    })
    .slot(SlotRole::Led, on.0);
    c.binding = Some(rk::bind(addr));
    b.controls.insert(root, c);
    root
}

/// A screen's pill switch lying on its side: the thumb to the right when on.
fn screen_switch(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    addr: &str,
    (cx, cy): (f64, f64),
    (w, h): (f64, f64),
) -> NodeId {
    let root = b.add(parent, draw::frame(name, cx - w / 2.0, cy - h / 2.0, w, h));
    let (tw, th) = (w * 0.8, h * 0.5);
    b.add(
        root,
        draw::rounded(
            draw::rect(
                "track",
                (w - tw) / 2.0,
                (h - th) / 2.0,
                tw,
                th,
                draw::solid("#d7d8dc"),
            ),
            th / 2.0,
        ),
    );
    let off = b.add(
        root,
        draw::circle(
            "thumb off",
            (w - tw) / 2.0 + th / 2.0,
            h / 2.0,
            th * 0.36,
            draw::solid("#4a4b50"),
        ),
    );
    let _ = off;
    let on = b.add(
        root,
        draw::circle(
            "thumb",
            (w + tw) / 2.0 - th / 2.0,
            h / 2.0,
            th * 0.36,
            draw::solid("#2a2b2e"),
        ),
    );
    let mut c = Control::new(ControlKind::Button {
        mode: ButtonMode::Toggle,
    })
    .slot(SlotRole::Led, on.0);
    c.binding = Some(rk::bind(addr));
    b.controls.insert(root, c);
    root
}

/// A range the rocker sweeps: a hairline lane with the swept span lit
/// between two handles, H (heel) and T (toe), each a `Handle` on its own
/// param; the row prints its name and the span.
#[allow(clippy::too_many_arguments)]
fn range_row(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (heel, toe): (&str, &str),
    (lo, hi): (f64, f64),
    (x, y0, w): (f64, f64, f64),
    accent: &str,
    (heel_text, toe_text): (&str, &str),
) {
    let box_ = draw::stroked(
        draw::rounded(
            draw::rect("row", x - 3.0, y0, w + 6.0, 19.0, draw::solid("#0d0e10")),
            2.0,
        ),
        "#2e3035",
        0.6,
    );
    b.add(parent, box_);
    // The span, as the screen prints it: NAME heel-toe, set as one line —
    // the heel's readout starts after the name, the dash and the toe's
    // after the heel as it prints at rest.
    let ty = y0 + 6.6;
    let label = Letter::new(6.0, 500.0, "#c3c5ca");
    let value = Letter::new(6.0, 600.0, "#ffffff");
    rk::letter_aligned(b, parent, name, (x + 1.0, ty), label, TextAlign::Left);
    let hx = x + 1.0 + rk::lettering::set_width(name, &label) + 2.0;
    let hz = lo > 0.0;
    // (A unit prints after a space; the screen sets % tight, so the %
    // is printed after the toe as it reads at rest.)
    let (fmt_h, fmt_t) = if hz {
        (
            ValueFormat::decimals(1).unit(""),
            ValueFormat::decimals(1).unit("Hz"),
        )
    } else {
        (
            ValueFormat::decimals(0).unit(""),
            ValueFormat::decimals(0).unit(""),
        )
    };
    let hw = rk::lettering::set_width(heel_text, &value);
    lcd::readout(
        b,
        parent,
        &format!("{name} heel value"),
        rk::bind(heel),
        (hx - 1.0, ty, hw + 8.0),
        value,
        TextAlign::Left,
        None,
        fmt_h,
    );
    let dx = hx + hw + 0.4;
    rk::letter_aligned(b, parent, "-", (dx, ty), value, TextAlign::Left);
    let tx = dx + rk::lettering::set_width("-", &value) + 0.4;
    lcd::readout(
        b,
        parent,
        &format!("{name} toe value"),
        rk::bind(toe),
        (tx - 1.0, ty, x + w - tx + 1.0),
        value,
        TextAlign::Left,
        None,
        fmt_t,
    );
    if !hz {
        let px = tx + rk::lettering::set_width(toe_text, &value) + 0.4;
        rk::letter_aligned(b, parent, "%", (px, ty), value, TextAlign::Left);
    }
    let area = b.add(
        parent,
        draw::frame(&format!("{name} range"), x, y0 + 11.5, w, 6.0),
    );
    b.add(
        area,
        draw::rounded(
            draw::rect("lane", 0.0, 2.5, w, 1.0, draw::solid("#5a5c62")),
            0.5,
        ),
    );
    // The swept span between the handles as they rest.
    let at = |t: &str| ((t.parse::<f64>().unwrap_or(lo) - lo) / (hi - lo)).clamp(0.0, 1.0);
    let (u0, u1) = (at(heel_text), at(toe_text));
    b.add(
        area,
        draw::rect("span", w * u0, 2.3, w * (u1 - u0), 1.4, draw::solid(accent)),
    );
    for (addr, letter) in [(heel, "H"), (toe, "T")] {
        let hbox = 16.0;
        let h = b.add(
            area,
            draw::frame(
                &format!("{name} {letter}"),
                0.0,
                3.0 - hbox / 2.0,
                hbox,
                hbox,
            ),
        );
        b.add(
            h,
            draw::stroked(
                draw::circle("dot", hbox / 2.0, hbox / 2.0, 3.6, draw::solid("#f6f6f7")),
                "#1a1b1d",
                0.7,
            ),
        );
        rk::letter(
            b,
            h,
            letter,
            (hbox / 2.0, hbox / 2.0),
            Letter::new(4.4, 800.0, "#111214"),
        );
        let kind = ControlKind::Handle {
            x: Some(Binding::param(addr)),
            y: None,
            wheel: None,
            x_map: HandleAxis::new(Axis::Linear { min: lo, max: hi }),
            y_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
        };
        b.controls.insert(h, Control::new(kind));
    }
}

/// A bank of radio keys over a choice param: one `Select` button per
/// option in (`x, y, w, h`) split evenly, the selected one filled.
#[allow(clippy::too_many_arguments)]
fn segments(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    addr: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    options: &[&str],
    vertical: bool,
    ink: Ink<'_>,
    fill: &str,
) {
    let n = options.len();
    b.add(
        parent,
        draw::stroked(
            draw::rounded(
                draw::rect(&format!("{name} frame"), x, y, w, h, draw::solid("#0b0c0d")),
                2.5,
            ),
            "#3a3c40",
            0.8,
        ),
    );
    for (i, o) in options.iter().enumerate() {
        let (sx, sy, sw, sh) = if vertical {
            let sh = h / n as f64;
            (x, y + sh * i as f64, w, sh)
        } else {
            let sw = w / n as f64;
            (x + sw * i as f64, y, sw, h)
        };
        let root = b.add(parent, draw::frame(&format!("{name} {o}"), sx, sy, sw, sh));
        let lit = b.add(
            root,
            draw::rounded(
                draw::rect("selected", 1.5, 1.5, sw - 3.0, sh - 3.0, draw::solid(fill)),
                1.5,
            ),
        );
        rk::print(b, root, o, (sw / 2.0, sh / 2.0), ink);
        let mut c = Control::new(ControlKind::Button {
            mode: ButtonMode::Select {
                index: i as u32,
                count: n as u32,
            },
        })
        .slot(SlotRole::Led, lit.0);
        c.binding = Some(rk::bind(addr));
        b.controls.insert(root, c);
    }
}

/// An outlined section with its title set into the top rule.
fn section(
    b: &mut Builder<'_>,
    parent: NodeId,
    title: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    ink: &str,
    size: f64,
) {
    let tw = if title.is_empty() {
        0.0
    } else {
        title.chars().count() as f64 * size * 0.78 + size * 1.2
    };
    let (tx0, tx1) = (x + size * 2.2, x + size * 2.2 + tw);
    let r = size * 0.9;
    let d = if title.is_empty() {
        vec![vec![
            (x + r, y),
            (x + w - r, y),
            (x + w, y + r),
            (x + w, y + h - r),
            (x + w - r, y + h),
            (x + r, y + h),
            (x, y + h - r),
            (x, y + r),
            (x + r, y),
        ]]
    } else {
        vec![vec![
            (tx1, y),
            (x + w - r, y),
            (x + w, y + r),
            (x + w, y + h - r),
            (x + w - r, y + h),
            (x + r, y + h),
            (x, y + h - r),
            (x, y + r),
            (x + r, y),
            (tx0, y),
        ]]
    };
    b.add(parent, parts::lines_node("section", &d, 0.9, ink));
    if !title.is_empty() {
        rk::letter_aligned(
            b,
            parent,
            title,
            (tx0 + size * 0.6, y),
            Letter::new(size, 700.0, ink).track(0.14),
            TextAlign::Left,
        );
    }
}

// ── 11 · Treadle Modulator ───────────────────────────────────────────────

/// Treadle Modulator — Pedal, Wide (222 × 286). See the module docs.
#[allow(clippy::too_many_lines)]
fn modulator() -> Face {
    let ns = "dfmod";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("Treadle Modulator", |b, root| {
        let e = Enclosure::new("wide", "#a88577", "brushed")
            .base("#6e544b")
            .jacks("none");
        let p = rk::pedal_body(b, root, &e);
        let r = rocker(
            b,
            p,
            "Rocker",
            &a("pedal"),
            (10.0, 9.0, 108.0, 232.0),
            "#b39487",
            LED_BLUE,
        );
        rk::letter_fit(b, r, "MODULATOR", (54.0, 110.0), 76.0, name(9.0, "#f3eee9"));
        let wave: Vec<(f64, f64)> = (0..=40)
            .map(|i| {
                let u = f64::from(i) / 40.0;
                let bump = if (0.42..0.58).contains(&u) {
                    -((u - 0.42) / 0.16 * std::f64::consts::PI).sin() * 4.0
                } else {
                    0.0
                };
                (22.0 + 64.0 * u, 122.0 + bump)
            })
            .collect();
        rk::line(b, r, &wave, 0.7, "#efe6e0");
        // The screen.
        let s = screen(b, p, "Screen", (126.0, 10.0, 85.0, 133.0));
        let head = Letter::new(5.8, 500.0, "#b9bbc0");
        rk::letter(b, s, "CHORUS", (17.5, 9.0), head);
        rk::letter(
            b,
            s,
            "FLANGER",
            (68.0, 9.0),
            head.color("#ffffff").weight(600.0),
        );
        screen_switch(
            b,
            s,
            "Chorus / Flanger",
            &a("flanger"),
            (41.0, 9.0),
            (17.0, 11.0),
        );
        range_row(
            b,
            s,
            "MIX",
            (&a("mix_heel"), &a("mix_toe")),
            (0.0, 100.0),
            (6.0, 20.0, 73.0),
            "#f2b36b",
            ("30", "80"),
        );
        range_row(
            b,
            s,
            "DEPTH",
            (&a("depth_heel"), &a("depth_toe")),
            (0.0, 100.0),
            (6.0, 45.5, 73.0),
            "#f2b36b",
            ("30", "80"),
        );
        range_row(
            b,
            s,
            "RATE",
            (&a("rate_heel"), &a("rate_toe")),
            (0.05, 5.2),
            (6.0, 71.0, 73.0),
            "#f2b36b",
            ("1.5", "5.0"),
        );
        rk::letter(
            b,
            s,
            "AUTO SPEED",
            (42.5, 106.0),
            Letter::new(5.6, 500.0, "#c3c5ca"),
        );
        parts::note_picker(
            b,
            s,
            "Auto speed",
            &a("auto_div"),
            (21.0, 117.0, 43.0),
            6.2,
            "#f0f1f3",
            "#26282c",
        );
        // Mode, the auto knobs, the stomp.
        let lg = legend(6.2, WHITE);
        slide_toggle(b, p, "Pedal / Auto", &a("auto"), (169.5, 155.5));
        rk::letter(b, p, "PEDAL", (145.5, 155.5), lg);
        rk::letter(b, p, "AUTO", (193.0, 155.5), lg);
        let small = KNOB.size(28.0).touch(34.0);
        rk::knob_l(
            b,
            p,
            &small,
            &a("auto_random"),
            "Auto",
            (146.0, 178.5),
            200.5,
            lg,
        );
        rk::knob_l(
            b,
            p,
            &small,
            &a("auto_speed"),
            "Auto",
            (192.0, 178.5),
            200.5,
            lg,
        );
        rk::letter(b, p, "RANDOM", (146.0, 206.5), lg);
        rk::letter(b, p, "SPEED", (192.0, 206.5), lg);
        rk::stomp(
            b,
            p,
            "Engage",
            &a("on"),
            (187.0, 238.0),
            32.0,
            LED_BLUE,
            (-40.0, -2.0),
            14.0,
            "bezel",
            false,
        );
        rk::letter(b, p, "ENGAGE", (187.0, 258.5), lg);
        rk::letter_fit(
            b,
            p,
            "DESIGNED IN THE FORGE",
            (64.0, 254.0),
            80.0,
            fine(4.0, "#eaded7"),
        );
    });
    let params = vec![
        parts::param(
            &a("pedal"),
            "Pedal",
            surface_proto::Range {
                steps: None,
                ..surface_proto::Range::linear(0.0, 100.0, surface_proto::Unit::Percent)
            },
            0.0,
        )
        .featured(),
        rk::toggle(&a("flanger"), "Flanger", true),
        rk::percent(&a("mix_heel"), "Mix Heel", 30.0),
        rk::percent(&a("mix_toe"), "Mix Toe", 80.0),
        rk::percent(&a("depth_heel"), "Depth Heel", 30.0),
        rk::percent(&a("depth_toe"), "Depth Toe", 80.0),
        parts::param(
            &a("rate_heel"),
            "Rate Heel",
            surface_proto::Range::linear(0.05, 5.2, surface_proto::Unit::Hz),
            1.5,
        ),
        parts::param(
            &a("rate_toe"),
            "Rate Toe",
            surface_proto::Range::linear(0.05, 5.2, surface_proto::Unit::Hz),
            5.0,
        ),
        rk::choice(&a("auto_div"), "Auto Speed", rk::DIVISIONS, 15),
        rk::toggle(&a("auto"), "Auto", false),
        rk::percent(&a("auto_random"), "Auto Random", 50.0),
        rk::percent(&a("auto_speed"), "Auto Speed Amount", 50.0),
        rk::toggle(&a("on"), "Engage", true).featured(),
    ];
    Face {
        file: "11-post-wide-treadle-modulator",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

// ── 12 · Stereo Echo ─────────────────────────────────────────────────────

/// A red fourteen-segment readout of `cells` characters `w` wide, right
/// aligned, over its unlit ghosts.
fn seg_readout(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    addr: &str,
    (x, cy, w): (f64, f64, f64),
    cells: usize,
    f: ValueFormat,
) -> NodeId {
    let probe = lcd::seg14(100.0, RED);
    let ghost: String = std::iter::repeat_n('~', cells).collect();
    let size = 100.0 * w / rk::lettering::set_width(&ghost, &probe);
    lcd::readout(
        b,
        parent,
        name,
        rk::bind(addr),
        (x, cy, w + 4.0),
        lcd::seg14(size, RED),
        TextAlign::Right,
        Some(("#ff4a3d17", cells)),
        f,
    )
}

/// The display's red.
const RED: &str = "#ff4a3d";

/// One annunciator: a lamp and its word, lit while `addr` is on option
/// `index` of `count` (a `Select` key: touching it picks that mode).
#[allow(clippy::too_many_arguments)]
fn annunciator(
    b: &mut Builder<'_>,
    parent: NodeId,
    word: &str,
    addr: &str,
    (index, count): (u32, u32),
    (lamp_x, cy): (f64, f64),
    lamp_left: bool,
) {
    let ink = Letter::new(6.0, 500.0, "#b3b6bc");
    let tw = rk::lettering::set_width(word, &ink);
    let (x0, w) = if lamp_left {
        (lamp_x - 3.0, tw + 13.0)
    } else {
        (lamp_x - tw - 10.0, tw + 13.0)
    };
    let root = b.add(parent, draw::frame(word, x0, cy - 4.5, w, 9.0));
    let (lx, tx, align) = if lamp_left {
        (3.0, 10.0, TextAlign::Left)
    } else {
        (w - 3.0, w - 10.0, TextAlign::Right)
    };
    rk::letter_aligned(b, root, word, (tx, 4.5), ink, align);
    b.add(root, parts::dark_lens(lx, 4.5, 1.6, RED));
    let lit = b.add(root, parts::lens(lx, 4.5, 1.6, RED));
    let mut c = Control::new(ControlKind::Button {
        mode: ButtonMode::Select { index, count },
    })
    .slot(SlotRole::Led, lit.0);
    c.binding = Some(rk::bind(addr));
    b.controls.insert(root, c);
}

/// Stereo Echo — Pedal, BigBox (404 × 286). See the module docs.
#[allow(clippy::too_many_lines)]
fn stereo_echo() -> Face {
    let ns = "dfecho";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("Stereo Echo", |b, root| {
        let e = Enclosure::new("bigbox", "#2a2b2e", "powder")
            .base("#18191b")
            .jacks("none");
        let p = rk::pedal_body(b, root, &e);
        let line = "#8d9096";
        section(b, p, "", (17.0, 12.5, 368.0, 105.0), line, 6.0);
        section(b, p, "", (17.0, 120.0, 368.0, 57.5), line, 6.0);
        let lg = legend(7.4, WHITE);
        let big = KNOB.size(42.0);
        rk::knob_l(b, p, &big, &a("mix"), "Mix", (56.0, 31.0), 64.5, lg);
        rk::knob_l(
            b,
            p,
            &big,
            &a("feedback"),
            "Feedback",
            (345.0, 31.0),
            64.5,
            lg,
        );
        // SYNC and MODE: black keys that step their modes.
        for (i, (addr, label)) in [("sync", "Sync"), ("mode", "Mode")].iter().enumerate() {
            let x = 164.0 + 38.0 * i as f64;
            let key = b.add(p, draw::frame(label, x, 20.5, 38.0, 31.0));
            let cap = draw::rounded(
                draw::rect(
                    "cap",
                    1.0,
                    1.0,
                    36.0,
                    29.0,
                    draw::linear(180.0, &[(0.0, "#2c2d30"), (1.0, "#121314")]),
                ),
                2.5,
            );
            b.add(key, draw::with_effects(cap, light::stand(2.5, 1.0)));
            for n in light::edge("cap edge", (1.0, 1.0, 36.0, 29.0), 2.5, 1.2, 0.8, true) {
                b.add(key, n);
            }
            rk::letter(
                b,
                key,
                &label.to_uppercase(),
                (19.0, 13.0 + 2.0 * i as f64),
                Letter::new(5.8, 500.0, "#d8d9dc"),
            );
            let mut c = Control::new(ControlKind::Button {
                mode: ButtonMode::Cycle,
            });
            c.binding = Some(rk::bind(&a(addr)));
            b.controls.insert(key, c);
            rk::line(b, p, &[(x + 19.0, 56.0), (x + 19.0, 71.0)], 0.8, line);
        }
        // The display: division left, time right, the annunciators between
        // — each lamp lit by its param's position.
        let d = parts::screen(
            b,
            p,
            "Display",
            (29.0, 75.0, 345.0, 36.0),
            parts::Screen {
                glass: ("#060303", "#100404"),
                ink: RED,
                mesh: false,
            },
        );
        seg_readout(
            b,
            d,
            "Division",
            &a("div"),
            (8.0, 18.0, 102.0),
            5,
            ValueFormat::labels(rk::DIVISIONS),
        );
        seg_readout(
            b,
            d,
            "Time",
            &a("time"),
            (233.0, 18.0, 102.0),
            5,
            ValueFormat::decimals(1).unit(""),
        );
        for (i, (l, r)) in [
            ("FREE MS", "STEREO"),
            ("DAW BPM", "PHASE INV"),
            ("TAP BPM", "PING PONG"),
        ]
        .iter()
        .enumerate()
        {
            let y = 10.0 + 8.8 * i as f64;
            annunciator(b, d, l, &a("sync"), (i as u32, 3), (165.0, y), false);
            annunciator(b, d, r, &a("mode"), (i as u32, 3), (180.5, y), true);
        }
        // The lower row.
        let knobs = [
            ("low_cut", ""),
            ("time", "Time"),
            ("mod", "Mod"),
            ("ducker", "Ducker"),
            ("high_cut", ""),
        ];
        for (i, (addr, label)) in knobs.iter().enumerate() {
            let x = 56.0 + 72.25 * i as f64;
            rk::knob_l(b, p, &big, &a(addr), label, (x, 146.0), 177.5, lg);
        }
        // The low- and high-cut glyphs and the rule the legends sit in.
        rk::line(
            b,
            p,
            &[(46.0, 182.0), (51.0, 177.5), (66.0, 177.5)],
            0.9,
            WHITE,
        );
        rk::line(
            b,
            p,
            &[(337.0, 177.5), (352.0, 177.5), (357.0, 182.0)],
            0.9,
            WHITE,
        );
        for (x0, x1) in [
            (76.0, 113.0),
            (145.0, 187.0),
            (214.0, 255.0),
            (290.0, 328.0),
        ] {
            rk::line(b, p, &[(x0, 177.5), (x1, 177.5)], 0.6, "#9da0a6");
        }
        // Foot: engage and tap, their LEDs, the name.
        rk::stomp(
            b,
            p,
            "Engage",
            &a("on"),
            (48.0, 224.0),
            40.0,
            RED,
            (52.0, 0.0),
            15.0,
            "bezel",
            false,
        );
        rk::stomp(
            b,
            p,
            "Tap Tempo",
            &a("tap"),
            (356.0, 224.0),
            40.0,
            RED,
            (-54.0, 0.0),
            15.0,
            "bezel",
            true,
        );
        rk::letter(b, p, "ENGAGE", (48.0, 252.0), lg);
        rk::letter(b, p, "TAP TEMPO", (356.0, 252.0), lg);
        rk::letter_fit(
            b,
            p,
            "STEREO ECHO",
            (202.0, 217.5),
            108.0,
            name(12.0, "#dcdddf").track(0.08),
        );
        let y = 232.0;
        rk::line(
            b,
            p,
            &[
                (146.0, y),
                (196.0, y),
                (199.0, y + 3.0),
                (205.0, y - 3.0),
                (208.0, y),
                (258.0, y),
            ],
            0.8,
            "#c9cbcf",
        );
        rk::letter_fit(
            b,
            p,
            "DESIGNED IN THE FORGE",
            (202.0, 250.0),
            114.0,
            fine(6.0, "#dcdddf"),
        );
    });
    let params = vec![
        rk::percent(&a("mix"), "Mix", 50.0).featured(),
        rk::percent(&a("feedback"), "Feedback", 50.0).featured(),
        rk::choice(&a("sync"), "Sync", &["Free", "DAW", "Tap"], 0),
        rk::choice(&a("mode"), "Mode", &["Stereo", "Phase Inv", "Ping Pong"], 0),
        rk::choice(&a("div"), "Division", rk::DIVISIONS, 20),
        rk::ms(&a("time"), "Time", 20.0, 2400.0, 240.0).featured(),
        rk::hz(&a("low_cut"), "Low Cut", 20.0, 1000.0, 30.0),
        rk::percent(&a("mod"), "Mod", 50.0),
        rk::percent(&a("ducker"), "Ducker", 50.0),
        rk::hz(&a("high_cut"), "High Cut", 1000.0, 20000.0, 14000.0),
        rk::toggle(&a("on"), "Engage", true).featured(),
        rk::toggle(&a("tap"), "Tap Tempo", false),
    ];
    Face {
        file: "12-post-bigbox-stereo-echo",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

// ── 13 · Shimmer Verb ────────────────────────────────────────────────────

/// Shimmer Verb — Pedal, Standard (156 × 286). See the module docs.
fn shimmer_verb() -> Face {
    let ns = "dfverb";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("Shimmer Verb", |b, root| {
        let e = Enclosure::new("standard", "#44577a", "powder").base("#2e3b53");
        let p = rk::pedal_body(b, root, &e);
        let (w, _) = e.size();
        let lg = legend(7.0, WHITE);
        let k = KNOB.size(38.0).touch(40.0);
        rk::knob_l(b, p, &k, &a("mix"), "Mix", (31.0, 31.0), 57.5, lg);
        rk::knob_l(
            b,
            p,
            &k,
            &a("pre_delay"),
            "Pre Delay",
            (78.0, 31.0),
            57.5,
            lg,
        );
        rk::knob_l(b, p, &k, &a("decay"), "Decay", (125.0, 31.0), 57.5, lg);
        rk::knob_l(b, p, &k, &a("low_cut"), "", (31.0, 84.0), 0.0, lg);
        rk::knob_l(b, p, &k, &a("high_cut"), "", (125.0, 84.0), 0.0, lg);
        rk::line(
            b,
            p,
            &[(22.0, 119.0), (28.0, 113.0), (42.0, 113.0)],
            0.8,
            WHITE,
        );
        rk::line(
            b,
            p,
            &[(114.0, 113.0), (128.0, 113.0), (134.0, 119.0)],
            0.8,
            WHITE,
        );
        toggle(
            b,
            p,
            "Shimmer / Normal",
            &a("shimmer"),
            (78.0, 90.0),
            8.0,
            26.0,
        );
        let sm = legend(5.6, WHITE);
        rk::letter(b, p, "SHIMMER", (78.0, 75.0), sm);
        rk::letter(b, p, "NORMAL", (78.0, 111.0), sm);
        rk::letter_fit(
            b,
            p,
            "REVERB",
            (w / 2.0, 134.5),
            78.0,
            name(11.0, WHITE).track(0.18),
        );
        rk::stomp(
            b,
            p,
            "Bypass",
            &a("on"),
            (w / 2.0, 190.0),
            40.0,
            LED_BLUE,
            (0.0, -33.0),
            17.0,
            "bezel",
            false,
        );
        // The mark: rings in brackets on a rule.
        let y = 230.0;
        rk::line(b, p, &[(34.0, y), (64.0, y)], 0.8, "#dfe4ee");
        rk::line(b, p, &[(92.0, y), (122.0, y)], 0.8, "#dfe4ee");
        for r in [1.6, 3.2, 4.8] {
            b.add(
                p,
                draw::stroked(
                    draw::circle("ring", w / 2.0, y, r, draw::solid("#00000000")),
                    "#dfe4ee",
                    0.7,
                ),
            );
        }
        for (i, r) in [7.0, 9.5, 12.0].iter().enumerate() {
            let _ = i;
            let arc_l: Vec<(f64, f64)> = (0..=10)
                .map(|j| draw::polar(w / 2.0, y, *r, 230.0 + 80.0 * f64::from(j) / 10.0))
                .collect();
            let arc_r: Vec<(f64, f64)> = (0..=10)
                .map(|j| draw::polar(w / 2.0, y, *r, 50.0 + 80.0 * f64::from(j) / 10.0))
                .collect();
            rk::line(b, p, &arc_l, 0.7, "#dfe4ee");
            rk::line(b, p, &arc_r, 0.7, "#dfe4ee");
        }
        rk::letter_fit(
            b,
            p,
            "DESIGNED IN THE FORGE",
            (w / 2.0, 250.0),
            w * 0.74,
            fine(4.6, "#dfe4ee"),
        );
    });
    let params = vec![
        rk::percent(&a("mix"), "Mix", 50.0).featured(),
        rk::ms(&a("pre_delay"), "Pre Delay", 1.0, 500.0, 22.4),
        parts::param(
            &a("decay"),
            "Decay",
            surface_proto::Range::log(0.2, 20.0, surface_proto::Unit::Seconds),
            2.0,
        )
        .featured(),
        rk::hz(&a("low_cut"), "Low Cut", 20.0, 1000.0, 141.0),
        rk::hz(&a("high_cut"), "High Cut", 1000.0, 20000.0, 4470.0),
        rk::toggle(&a("shimmer"), "Shimmer", false),
        rk::toggle(&a("on"), "Bypass", true).featured(),
    ];
    Face {
        file: "13-post-standard-shimmer-verb",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

// ── 14 · Beam Treadle ────────────────────────────────────────────────────

/// The pitch patches the treadle's screen lists.
const PATCHES: &[&str] = &[
    "Dissonant Dive",
    "Dual Dive",
    "Inv 5th Chord",
    "Inverted Atonal",
    "Inverted Dive",
    "Inverted Dive 2",
    "Lead Octave",
];

/// Beam Treadle — Pedal, Wide (222 × 286). See the module docs.
#[allow(clippy::too_many_lines)]
fn beam() -> Face {
    let ns = "dfbeam";
    let a = |s: &str| format!("{ns}/{s}");
    let scene = canvas("Beam Treadle", |b, root| {
        let e = Enclosure::new("wide", "#5d5487", "brushed")
            .base("#3d3760")
            .jacks("none");
        let p = rk::pedal_body(b, root, &e);
        let r = rocker(
            b,
            p,
            "Rocker",
            &a("pedal"),
            (10.0, 10.0, 105.0, 231.0),
            "#6d64a0",
            "#a6c8ff",
        );
        rk::letter_fit(
            b,
            r,
            "BEAM",
            (52.5, 110.0),
            46.0,
            name(11.0, "#ece9f6").track(0.22),
        );
        for dy in [0.0, 2.2, 4.4] {
            rk::line(
                b,
                r,
                &[(42.0, 121.0 + dy), (63.0, 121.0 + dy)],
                0.6,
                "#ece9f6",
            );
        }
        rk::line(b, r, &[(20.0, 123.2), (38.0, 123.2)], 0.6, "#ece9f6");
        rk::line(b, r, &[(67.0, 123.2), (85.0, 123.2)], 0.6, "#ece9f6");
        // The patch list: one radio row per patch, the chosen one lit, set
        // as large as the screen's own UI type.
        let s = screen(b, p, "Screen", (126.0, 11.0, 85.0, 102.0));
        let n = PATCHES.len();
        let row_ink = Letter::new(6.0, 500.0, "#ececf0");
        for (i, patch) in PATCHES.iter().enumerate() {
            let y = 1.5 + 12.1 * i as f64;
            let row = b.add(
                s,
                draw::frame(&format!("Patch {patch}"), 1.5, y, 79.0, 11.6),
            );
            let lit = b.add(
                row,
                draw::rounded(
                    draw::rect("selected", 0.0, 0.4, 79.0, 10.8, draw::solid("#30344f")),
                    1.5,
                ),
            );
            rk::letter_aligned(
                b,
                row,
                &format!("PITCH - {patch}"),
                (2.5, 5.8),
                row_ink,
                TextAlign::Left,
            );
            let mut c = Control::new(ControlKind::Button {
                mode: ButtonMode::Select {
                    index: i as u32,
                    count: n as u32,
                },
            })
            .slot(SlotRole::Led, lit.0);
            c.binding = Some(rk::bind(&a("patch")));
            b.controls.insert(row, c);
        }
        b.add(
            s,
            draw::rounded(
                draw::rect("scroll", 81.8, 2.0, 1.4, 26.0, draw::solid("#9a9da6")),
                0.7,
            ),
        );
        b.add(
            s,
            draw::rect("footer rule", 0.0, 87.0, 85.0, 0.6, draw::solid("#3a3c42")),
        );
        let foot = Letter::new(6.4, 500.0, "#a9acb4");
        rk::letter_aligned(b, s, "MIX", (40.0, 94.5), foot, TextAlign::Right);
        lcd::readout(
            b,
            s,
            "Mix",
            rk::bind(&a("mix")),
            (42.0, 94.5, 30.0),
            foot.color("#ffffff").weight(600.0),
            TextAlign::Left,
            None,
            ValueFormat::decimals(0).unit("%"),
        );
        let lg = legend(6.2, WHITE);
        slide_toggle(b, p, "Pedal / Envelope", &a("envelope"), (168.0, 126.5));
        rk::letter(b, p, "PEDAL", (144.5, 126.5), lg);
        rk::letter(b, p, "ENVELOPE", (198.0, 126.5), lg);
        let small = KNOB.size(28.0).touch(34.0);
        rk::knob_l(
            b,
            p,
            &small,
            &a("speed"),
            "Speed",
            (144.5, 153.0),
            173.0,
            lg,
        );
        rk::knob_l(
            b,
            p,
            &small,
            &a("sensitivity"),
            "Sensitivity",
            (190.5, 153.0),
            173.0,
            lg,
        );
        rk::line(
            b,
            p,
            &[
                (208.0, 141.0),
                (212.0, 141.0),
                (212.0, 172.0),
                (208.0, 172.0),
            ],
            0.7,
            WHITE,
        );
        rk::line(b, p, &[(157.0, 173.0), (171.0, 173.0)], 0.7, WHITE);
        // TRIGGER: a lit round key.
        let trig = b.add(p, draw::frame("Trigger", 155.5, 180.0, 24.0, 24.0));
        b.add(
            trig,
            draw::with_effects(
                draw::circle(
                    "bezel",
                    12.0,
                    12.0,
                    10.0,
                    draw::conic(
                        20.0,
                        &[
                            (0.0, "#f2f2f0"),
                            (0.25, "#6c6d6f"),
                            (0.5, "#dcdcda"),
                            (0.75, "#4c4d50"),
                            (1.0, "#f2f2f0"),
                        ],
                    ),
                ),
                light::stand(1.5, 0.9),
            ),
        );
        let lens = draw::circle(
            "lens",
            12.0,
            12.0,
            8.0,
            draw::radial(&[(0.0, "#ffffff"), (0.5, "#c4d8ff"), (1.0, "#6f8fe0")]),
        );
        let lit = b.add(trig, parts::emit(lens, "#9ab8ff", 0.8, 12.0));
        let mut c = Control::new(ControlKind::Button {
            mode: ButtonMode::Toggle,
        })
        .slot(SlotRole::Led, lit.0);
        c.binding = Some(rk::bind(&a("trigger")));
        b.controls.insert(trig, c);
        rk::letter(b, p, "TRIGGER", (167.5, 208.0), lg);
        rk::stomp(
            b,
            p,
            "Engage",
            &a("on"),
            (186.0, 237.0),
            34.0,
            LED_BLUE,
            (-40.0, -4.0),
            14.0,
            "bezel",
            false,
        );
        rk::letter(b, p, "ENGAGE", (186.0, 258.5), lg);
        rk::letter_fit(
            b,
            p,
            "DESIGNED IN THE FORGE",
            (64.0, 254.0),
            80.0,
            fine(4.0, "#dcd8ec"),
        );
    });
    let params = vec![
        parts::param(
            &a("pedal"),
            "Pedal",
            surface_proto::Range::linear(0.0, 100.0, surface_proto::Unit::Percent),
            0.0,
        )
        .featured(),
        rk::choice(&a("patch"), "Patch", PATCHES, 0).featured(),
        rk::percent(&a("mix"), "Mix", 100.0),
        rk::toggle(&a("envelope"), "Envelope", false),
        rk::percent(&a("speed"), "Speed", 50.0),
        rk::percent(&a("sensitivity"), "Sensitivity", 50.0),
        rk::toggle(&a("trigger"), "Trigger", true),
        rk::toggle(&a("on"), "Engage", true).featured(),
    ];
    Face {
        file: "14-fx-wide-treadle-beam",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}

// ── 15 · Fracture ────────────────────────────────────────────────────────

/// A static grain waveform: a mirrored green envelope across the lane, the
/// part past the read head dimmed, and the grains as dots.
fn waveform(b: &mut Builder<'_>, parent: NodeId, (x0, x1, cy): (f64, f64, f64), head: f64) {
    let mut seed: u32 = 0x9a1f_00d1;
    let mut rnd = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        f64::from(seed >> 8) / f64::from(1_u32 << 24)
    };
    let n = 260;
    let mut top = Vec::new();
    for i in 0..=n {
        let u = f64::from(i) / f64::from(n);
        let x = x0 + (x1 - x0) * u;
        let block = if (u * 9.0).fract() < 0.04 { 0.25 } else { 1.0 };
        let env = (u * std::f64::consts::PI).sin().powf(0.15) * (0.75 + 0.25 * rnd()) * 8.0 * block;
        top.push((x, cy - env));
    }
    let mut d = format!("M {x0:.1} {cy:.1} ");
    for (x, y) in &top {
        d.push_str(&format!("L {x:.1} {y:.1} "));
    }
    for (x, y) in top.iter().rev() {
        d.push_str(&format!("L {x:.1} {:.1} ", 2.0 * cy - y));
    }
    d.push('Z');
    let full = draw::path(
        "waveform",
        0.0,
        0.0,
        x1 + 10.0,
        cy + 20.0,
        d.clone(),
        draw::solid("#89c893"),
    );
    b.add(parent, parts::emit(full, "#89c893", 0.3, 6.0));
    // The unplayed part, dimmed: a veil over the waveform past the head.
    b.add(
        parent,
        draw::rect(
            "unplayed",
            head + 1.0,
            cy - 8.0,
            x1 - head + 4.0,
            16.0,
            draw::solid("#0a0b0cb0"),
        ),
    );
    for (gx, gy) in [
        (0.07, -14.0),
        (0.15, -12.0),
        (0.2, 11.0),
        (0.23, -12.0),
        (0.26, -13.0),
        (0.36, -15.0),
        (0.38, 11.0),
        (0.42, -22.0),
        (0.47, -11.0),
        (0.59, -12.0),
    ] {
        let x = x0 + (x1 - x0) * gx;
        let dim = gx > 0.25 && gx < 0.28;
        b.add(
            parent,
            parts::emit(
                draw::circle(
                    "grain",
                    x,
                    cy + gy,
                    1.7,
                    draw::solid(if dim { "#8a7a4a" } else { "#ffd98a" }),
                ),
                "#ffd98a",
                0.5,
                4.0,
            ),
        );
    }
    b.add(
        parent,
        draw::rect(
            "read head",
            head,
            cy - 38.0,
            1.0,
            78.0,
            draw::solid("#f4f4f4"),
        ),
    );
}

/// Fracture — Tabletop, 800 × 479. See the module docs.
#[allow(clippy::too_many_lines)]
fn fracture() -> Face {
    let ns = "dffrac";
    let a = |s: &str| format!("{ns}/{s}");
    let (w, h) = (800.0, 479.0);
    let scene = canvas("Fracture", |b, root| {
        parts::fit_root(b, root, w, h);
        let case = draw::rounded(
            draw::rect(
                "case",
                2.0,
                2.0,
                w - 4.0,
                h - 6.0,
                draw::linear(
                    180.0,
                    &[(0.0, "#3a3b3e"), (0.8, "#27282b"), (1.0, "#1c1d1f")],
                ),
            ),
            10.0,
        );
        b.add(
            root,
            draw::stroked(
                draw::with_effects(
                    case,
                    vec![
                        draw::drop_shadow("#000000a0", 0.0, 8.0, 16.0),
                        draw::inner_shadow("#ffffff26", 0.0, 1.5, 1.0),
                    ],
                ),
                "#0c0c0d",
                1.2,
            ),
        );
        b.add(
            root,
            draw::rect(
                "lip",
                6.0,
                h - 30.0,
                w - 12.0,
                26.0,
                draw::linear(180.0, &[(0.0, "#1a1b1d"), (1.0, "#2e2f32")]),
            ),
        );
        // The screen.
        let sc = b.add(
            root,
            draw::stroked(
                draw::rounded(
                    draw::rect("screen", 23.0, 19.0, 758.0, 324.0, draw::solid("#0c0d0e")),
                    3.0,
                ),
                "#b9bcc2",
                1.4,
            ),
        );
        let _ = sc;
        let scr = b.add(root, draw::frame("screen area", 23.0, 19.0, 758.0, 324.0));
        waveform(b, scr, (25.0, 740.0, 55.0), 496.0);
        let ink = Ink::new(9.5, "#c6c8cc").weight(500.0).track(0.02).plain();
        let key = Ink::new(9.0, "#e6e7ea").weight(500.0).track(0.02).plain();
        let green = "#3e6b45";
        rk::print(b, scr, "LOOP LENGTH", (151.0, 113.0), ink);
        segments(
            b,
            scr,
            "Loop length",
            &a("loop"),
            (198.0, 104.0, 235.0, 20.0),
            &["1/2 BAR", "1 BAR", "2 BARS", "4 BARS"],
            false,
            key,
            green,
        );
        rk::print(b, scr, "READ POINT", (509.0, 113.0), ink);
        // Read point: a field that fills as it moves, its value printed.
        let rp = b.add(
            scr,
            draw::stroked(
                draw::rounded(draw::frame("Read point", 552.0, 104.0, 93.0, 20.0), 2.5),
                "#3a3c40",
                0.8,
            ),
        );
        let fill = b.add(
            rp,
            draw::rect("fill", 1.5, 1.5, 90.0, 17.0, draw::solid(green)),
        );
        let mut t = parts::text_node("", 6.0, 10.0, 70.0, &key, TextAlign::Left);
        t.name = "value".into();
        let v = b.add(rp, t);
        let mut c = Control::new(ControlKind::Fader {
            orientation: Orientation::Horizontal,
        })
        .slot(SlotRole::Fill, fill.0)
        .slot(SlotRole::Value, v.0);
        c.binding = Some(rk::bind(&a("read")));
        c.format = Some(ValueFormat::labels(rk::DIVISIONS));
        b.controls.insert(rp, c);
        // Mode.
        let head = "#d8d9dc";
        section(b, scr, "MODE", (17.0, 134.0, 722.0, 54.0), head, 9.5);
        segments(
            b,
            scr,
            "Mode",
            &a("mode"),
            (42.0, 152.0, 673.0, 19.0),
            &["SEQUENCE", "PITCH DELAY", "INTERFERE", "TEXTURIZER"],
            false,
            key,
            green,
        );
        // Grain, mix, sync.
        section(b, scr, "GRAIN", (17.0, 207.0, 436.0, 106.0), head, 9.5);
        section(b, scr, "MIX", (471.0, 207.0, 187.0, 106.0), head, 9.5);
        section(b, scr, "SYNC", (676.0, 207.0, 63.0, 106.0), head, 9.5);
        let dial = |b: &mut Builder<'_>, addr: &str, label: &str, x: f64, f: ValueFormat| {
            let pl = Place {
                options: Some(vec!["legend_first".into()]),
                ..Place::new(label)
                    .size(36.0)
                    .variant("green")
                    .legend(label)
                    .bind(rk::bind(addr))
                    .touch(40.0)
                    .format(f)
            };
            parts::put(b, scr, "knob.plugin_ring", &pl, x, 245.0)
        };
        dial(
            b,
            &a("length"),
            "Length",
            74.0,
            ValueFormat::labels(rk::DIVISIONS),
        );
        dial(
            b,
            &a("occurrence"),
            "Occurrence",
            160.0,
            ValueFormat::labels(rk::DIVISIONS),
        );
        dial(
            b,
            &a("pitch"),
            "Pitch Amount",
            262.0,
            ValueFormat::decimals(0).unit("%"),
        );
        dial(
            b,
            &a("chaos"),
            "Chaos",
            396.0,
            ValueFormat::decimals(0).unit("%"),
        );
        dial(
            b,
            &a("filter"),
            "Filter",
            529.0,
            ValueFormat::decimals(0).unit("%"),
        );
        dial(
            b,
            &a("mix"),
            "Mix",
            602.0,
            ValueFormat::decimals(0).unit("%"),
        );
        segments(
            b,
            scr,
            "Direction",
            &a("direction"),
            (311.0, 229.0, 33.0, 62.0),
            &["↑", "↑↓", "↓"],
            true,
            key,
            green,
        );
        segments(
            b,
            scr,
            "Sync",
            &a("sync"),
            (688.0, 225.0, 40.0, 42.0),
            &["DAW", "TAP"],
            true,
            key,
            green,
        );
        let bpm = parts::readout(
            b,
            scr,
            "Tempo",
            rk::bind(&a("bpm")),
            (682.0, 288.0, 52.0),
            9.5,
            "#8f9297",
            TextAlign::Center,
        );
        parts::format(b, bpm, ValueFormat::decimals(1).unit(""));
        // The foot: stomps, LEDs, the name.
        let red = "#ff4a3d";
        rk::stomp(
            b,
            root,
            "Engage",
            &a("on"),
            (63.0, 395.0),
            50.0,
            red,
            (64.0, -4.0),
            14.0,
            "bezel",
            false,
        );
        rk::stomp(
            b,
            root,
            "Tempo",
            &a("tap"),
            (735.0, 395.0),
            50.0,
            red,
            (-63.0, -4.0),
            14.0,
            "bezel",
            true,
        );
        let lg = legend(9.5, WHITE);
        rk::letter(b, root, "ENGAGE", (63.0, 433.0), lg);
        rk::letter(b, root, "TEMPO", (735.0, 433.0), lg);
        // FRACTURE, its strokes broken as the unit's lettering is.
        let t = rk::letter(
            b,
            root,
            "FRACTURE",
            (400.0, 397.0),
            mark(27.0, "#dfe0e2").weight(500.0).track(0.14),
        );
        let _ = t;
        for (x, y, cw) in [
            (292.0, 393.0, 6.0),
            (345.0, 401.0, 5.0),
            (417.0, 390.0, 6.0),
            (470.0, 404.0, 5.0),
            (505.0, 392.0, 4.0),
        ] {
            b.add(
                root,
                draw::rect("break", x, y, cw, 2.0, draw::solid("#2b2c2f")),
            );
        }
        rk::letter(
            b,
            root,
            "DESIGNED IN THE FORGE",
            (400.0, 432.0),
            fine(6.4, "#b9bcc2"),
        );
    });
    let params = vec![
        rk::choice(
            &a("loop"),
            "Loop Length",
            &["1/2 Bar", "1 Bar", "2 Bars", "4 Bars"],
            0,
        ),
        rk::choice(&a("read"), "Read Point", rk::DIVISIONS, 19),
        rk::choice(
            &a("mode"),
            "Mode",
            &["Sequence", "Pitch Delay", "Interfere", "Texturizer"],
            0,
        )
        .featured(),
        rk::choice(&a("length"), "Grain Length", rk::DIVISIONS, 19),
        rk::choice(&a("occurrence"), "Occurrence", rk::DIVISIONS, 19),
        rk::percent(&a("pitch"), "Pitch Amount", 50.0),
        rk::choice(
            &a("direction"),
            "Pitch Direction",
            &["Up", "Both", "Down"],
            0,
        ),
        rk::percent(&a("chaos"), "Chaos", 50.0).featured(),
        rk::percent(&a("filter"), "Filter", 50.0),
        rk::percent(&a("mix"), "Mix", 50.0).featured(),
        rk::choice(&a("sync"), "Sync", &["DAW", "Tap"], 0),
        parts::param(
            &a("bpm"),
            "Tempo",
            surface_proto::Range::linear(40.0, 240.0, surface_proto::Unit::Bpm),
            120.0,
        ),
        rk::toggle(&a("on"), "Engage", true).featured(),
        rk::toggle(&a("tap"), "Tempo Tap", false),
    ];
    Face {
        file: "15-fx-tabletop-fracture",
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    }
}
