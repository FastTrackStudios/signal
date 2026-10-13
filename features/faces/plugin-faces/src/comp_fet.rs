//! Faces: the FET limiter — one circuit in the three faceplates it was
//! built with, each laid out from front-on photographs of the real unit
//! (the 1176, 1967 onward):
//!
//! - **Black** — the blackface 1176LN (revisions D–F, early 1970s): black
//!   anodised plate, smoked dial discs under INPUT / OUTPUT, a cream
//!   Modutec card lit from above, lettering under the meter.
//! - **Silver** — the first "bluestripe" units (revisions A/B, 1967–69):
//!   brushed plate with the meter, a red pilot jewel and the lettering on a
//!   painted blue band; clear dial discs; a Weston card.
//! - **Late** — the late silverface (revision H, 1980s): brushed plate with
//!   two lighter printed fields, clear discs with black lines, a backlit
//!   amber card, the meter bank's OFF key red, a blue badge at the right.
//!
//! Every position below is a fraction of the plate measured off a photo
//! (panel edge to panel edge, rack ears included — the plate *is* the
//! ears on this unit). Left to right: INPUT and OUTPUT (the ∞…0
//! attenuation scales — there is no threshold, INPUT drives a fixed one);
//! ATTACK over RELEASE, both 1…7 with 7 fastest; the ratio keys 20 / 12 /
//! 8 / 4 (plus the plugin's "All", the all-buttons-in trick, on top); the
//! meter reading gain reduction (0 GR rests on 0 VU and swings left); the
//! meter keys GR / +8 / +4 / OFF. On the unit OFF also cuts the power; here
//! it rests the needle. Only the early unit has a power jewel.
//!
//! No brand, model or serial is printed: the lettering where the unit
//! printed its maker says what it is instead.
//!
//! Every part is a `surface-kit` component (`panel.brushed`,
//! `panel.section`, `knob.knurled_cap`, `button.push_in_key_bank`,
//! `meter.vu_limiter`, `button.jewel`); this module lays them out, prints
//! the silkscreen, and draws the plate's hardware (rack slots, screws,
//! holes).

use document_proto::{Node, NodeId, NodeKind, TextAlign, TextCase, Transform};
use surface_kit::{Ink, Place, TOUCH_MIN, draw, place};
use surface_proto::{
    Binding, EntryId, MeterTap, ParamInfo, Range, SlotRole, SourceSelect, StreamInfo, StreamKind,
    Unit,
};
use surface_widgets::Builder;

use crate::face::{Face, canvas};

pub mod lane;

// ── the unit's controls ──────────────────────────────────────────────────

/// The attenuation scale printed round INPUT and OUTPUT, ∞ at the stop.
const ATTEN: &[&str] = &["∞", "48", "36", "30", "24", "18", "12", "6", "0"];
/// The time scales, 1 (slowest) to 7 (fastest): the even steps are dots.
const TIMES: &[&str] = &["1", "", "3", "", "5", "", "7"];
/// The ratio param's options, in param order.
const RATIOS: &[&str] = &["4", "8", "12", "20", "All"];
/// The meter-source param's options, in param order (= top to bottom).
const METER: &[&str] = &["GR", "+8", "+4", "Off"];

/// Addresses — one namespace for all three finishes: they're the same unit.
pub const INPUT: &str = "fet/input";
pub const OUTPUT: &str = "fet/output";
pub const ATTACK: &str = "fet/attack";
pub const RELEASE: &str = "fet/release";
pub const RATIO: &str = "fet/ratio";
pub const METER_SRC: &str = "fet/meter";
/// The early unit's pilot jewel (the later units have none).
pub const POWER: &str = "fet/on";
pub const GR: &str = "fet/gr";
pub const OUT_LEVEL: &str = "fet/out";

fn params(power: bool) -> Vec<ParamInfo> {
    let grp = "FET";
    let mut v = vec![
        ParamInfo::new(INPUT, "Input", Range::linear(-48.0, 0.0, Unit::Db))
            .with_default_real(-24.0)
            .in_group(grp)
            .featured(),
        ParamInfo::new(OUTPUT, "Output", Range::linear(-48.0, 0.0, Unit::Db))
            .with_default_real(-18.0)
            .in_group(grp)
            .featured(),
        ParamInfo::new(ATTACK, "Attack", Range::linear(1.0, 7.0, Unit::None))
            .with_default_real(3.0)
            .in_group(grp)
            .featured(),
        ParamInfo::new(RELEASE, "Release", Range::linear(1.0, 7.0, Unit::None))
            .with_default_real(5.0)
            .in_group(grp)
            .featured(),
        ParamInfo::new(RATIO, "Ratio", Range::default())
            .choice(RATIOS)
            .in_group(grp)
            .featured(),
        ParamInfo::new(METER_SRC, "Meter", Range::default())
            .choice(METER)
            .in_group(grp)
            .featured(),
    ];
    if power {
        v.push(
            ParamInfo::new(POWER, "Power", Range::default())
                .toggle()
                .with_default_real(1.0)
                .in_group(grp)
                .featured(),
        );
    }
    v
}

fn streams() -> Vec<StreamInfo> {
    vec![
        StreamInfo {
            id: EntryId::new(GR),
            name: "Gain Reduction".into(),
            kind: StreamKind::GainReduction,
            rate_hz: 30.0,
        },
        StreamInfo {
            id: EntryId::new(OUT_LEVEL),
            name: "Output Level".into(),
            kind: StreamKind::Level { channels: 2 },
            rate_hz: 30.0,
        },
    ]
}

// ── the three plates ─────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum Plate {
    Black,
    Silver,
    Late,
}

/// Where a legend over or under a time knob sits: `(y fraction)`.
#[derive(Clone, Copy)]
struct Layout {
    file: &'static str,
    title: &'static str,
    plate: Plate,
    /// `panel.brushed` variant.
    panel: &'static str,
    /// Silkscreen.
    ink: &'static str,
    /// `knob.knurled_cap` variant of the big knobs.
    disc: &'static str,
    /// `meter.vu_limiter` variant.
    card: &'static str,
    input: (f64, f64),
    output: (f64, f64),
    attack: (f64, f64),
    release: (f64, f64),
    /// Legend centres (y) of ATTACK and RELEASE.
    time_legends: (f64, f64),
    /// Legend centre (y) of INPUT / OUTPUT.
    big_legend: f64,
    /// The ratio keys' centre x, and both columns' top and bottom (y).
    ratio_x: f64,
    meter_keys_x: f64,
    keys: (f64, f64),
    /// Meter centre and width (fraction of the plate's width).
    meter: (f64, f64, f64),
    /// Holes in the plate: `(x, y, radius as a fraction of the width)`.
    holes: &'static [(f64, f64, f64)],
    screws: [(f64, f64); 4],
}

const BLACK: Layout = Layout {
    file: "01-fet-black",
    title: "FET Limiter — Black",
    plate: Plate::Black,
    panel: "black",
    ink: "#efede6",
    disc: "smoke",
    card: "cream",
    input: (0.134, 0.54),
    output: (0.328, 0.545),
    attack: (0.491, 0.30),
    release: (0.491, 0.765),
    time_legends: (0.055, 0.94),
    big_legend: 0.905,
    ratio_x: 0.622,
    meter_keys_x: 0.884,
    keys: (0.17, 0.885),
    meter: (0.757, 0.47, 0.168),
    holes: &[(0.231, 0.75, 0.0045), (0.423, 0.52, 0.0085)],
    screws: [(0.062, 0.2), (0.064, 0.8), (0.938, 0.2), (0.936, 0.8)],
};

const SILVER: Layout = Layout {
    file: "02-fet-silver",
    title: "FET Limiter — Silver",
    plate: Plate::Silver,
    panel: "natural",
    ink: "#26272a",
    disc: "clear",
    card: "ivory",
    input: (0.146, 0.55),
    output: (0.34, 0.55),
    attack: (0.499, 0.285),
    release: (0.499, 0.715),
    time_legends: (0.07, 0.94),
    big_legend: 0.9,
    ratio_x: 0.628,
    meter_keys_x: 0.872,
    keys: (0.18, 0.86),
    meter: (0.751, 0.45, 0.166),
    holes: &[(0.246, 0.73, 0.0045)],
    screws: [(0.07, 0.21), (0.07, 0.79), (0.93, 0.21), (0.93, 0.79)],
};

const LATE: Layout = Layout {
    file: "03-fet-late",
    title: "FET Limiter — Late",
    plate: Plate::Late,
    panel: "natural",
    ink: "#26272a",
    disc: "clear_dark",
    card: "amber",
    input: (0.143, 0.53),
    output: (0.322, 0.53),
    attack: (0.483, 0.35),
    release: (0.483, 0.775),
    time_legends: (0.125, 0.555),
    big_legend: 0.88,
    ratio_x: 0.585,
    meter_keys_x: 0.83,
    keys: (0.2, 0.87),
    meter: (0.708, 0.45, 0.166),
    holes: &[
        (0.233, 0.5, 0.0042),
        (0.073, 0.5, 0.0017),
        (0.869, 0.5, 0.0017),
    ],
    screws: [(0.056, 0.2), (0.056, 0.78), (0.93, 0.2), (0.93, 0.78)],
};

pub fn faces() -> Vec<Face> {
    [BLACK, SILVER, LATE]
        .iter()
        .map(|f| Face {
            file: f.file,
            plugin: "comp",
            scene: canvas(f.title, |b, root| unit(b, root, f)),
            params: params(f.plate == Plate::Silver),
            streams: streams(),
        })
        .collect()
}

// ── layout ───────────────────────────────────────────────────────────────

/// The plate: 19" × 2U, rack ears included (482.6 × 88.1 mm).
const W: f64 = 1120.0;
const H: f64 = 204.0;
/// Big knob (collar) and time knob diameters, and a key's width.
const BIG: f64 = 68.0;
const SMALL: f64 = 44.0;
const KEY: f64 = 20.0;

fn at((fx, fy): (f64, f64)) -> (f64, f64) {
    (fx * W, fy * H)
}

fn unit(b: &mut Builder<'_>, root: NodeId, f: &Layout) {
    if let Some(n) = b.scene.node_mut(root) {
        n.size = document_proto::Size {
            width: W,
            height: H,
        };
    }
    // The plate is its own rack ears: no separate ear, slots in the corners.
    let panel = kit(
        b,
        root,
        "panel.brushed",
        &Place::new("Faceplate").variant(f.panel).options(&[
            &format!("w={W}"),
            &format!("h={H}"),
            "ends=none",
        ]),
    );
    if f.plate != Plate::Black {
        // Clear-anodised aluminium reads lighter than the kit's natural plate.
        if let Some(n) = b.scene.node_mut(panel) {
            n.fills = vec![draw::linear(
                178.0,
                &[(0.0, "#dddedc"), (0.48, "#d1d2d0"), (1.0, "#c2c3c1")],
            )];
        }
    }
    hardware(b, panel, f);

    match f.plate {
        Plate::Late => fields(b, panel),
        Plate::Silver => blue_band(b, panel, f),
        Plate::Black => {}
    }

    big_knob(b, panel, f, "Input", INPUT, at(f.input));
    big_knob(b, panel, f, "Output", OUTPUT, at(f.output));
    time_knob(
        b,
        panel,
        f,
        "Attack",
        ATTACK,
        at(f.attack),
        f.time_legends.0,
    );
    time_knob(
        b,
        panel,
        f,
        "Release",
        RELEASE,
        at(f.release),
        f.time_legends.1,
    );

    // The ratio keys, numbers to their left, 20 at the top under "All".
    let ratio = keys(b, panel, f, "Ratio", RATIO, RATIOS, true, "left", None);
    place_keys(b, panel, ratio, (f, "Ratio"), f.ratio_x * W, true);
    // The meter keys, labels to their right; the late unit's OFF is red.
    let colors = (f.plate == Plate::Late).then_some("colors=#1c1c1e,#1c1c1e,#1c1c1e,#c3281c");
    let meter = keys(
        b, panel, f, "Meter", METER_SRC, METER, false, "right", colors,
    );
    place_keys(b, panel, meter, (f, "Meter"), f.meter_keys_x * W, false);

    let (mx, my, mw) = f.meter;
    let vu = gr_meter(b, panel, f, mw * W);
    let (w, h) = size_of(b, vu);
    move_to(b, vu, mx * W - w / 2.0, my * H - h / 2.0);
    lettering(b, panel, f, (mx * W, my * H + h / 2.0));
}

/// Place a kit component (the ids here are the kit's own; a typo is a bug).
fn kit(b: &mut Builder<'_>, parent: NodeId, id: &str, p: &Place) -> NodeId {
    place(b, parent, id, p).unwrap_or_else(|e| panic!("{e}"))
}

fn size_of(b: &Builder<'_>, id: NodeId) -> (f64, f64) {
    b.scene
        .node(id)
        .map_or((0.0, 0.0), |n| (n.size.width, n.size.height))
}

fn move_to(b: &mut Builder<'_>, id: NodeId, x: f64, y: f64) {
    if let Some(n) = b.scene.node_mut(id) {
        n.transform = Transform::translate(x, y);
    }
}

/// Silkscreen: uppercase, tracked, a stated weight, centred on (`cx`, `cy`).
fn ink(
    name: &str,
    s: &str,
    (cx, cy): (f64, f64),
    size: f64,
    color: &str,
    (weight, track): (f64, f64),
) -> Node {
    let w = s.chars().count() as f64 * size * (0.78 + track) + 8.0;
    let mut n = draw::text(
        name,
        s,
        cx - w / 2.0,
        cy - size * 0.68,
        w,
        size,
        color,
        TextAlign::Center,
    );
    if let NodeKind::Text(t) = &mut n.kind {
        t.font_weight = Some(weight);
        t.text_case = TextCase::Upper;
        t.letter_spacing = size * track;
    }
    n
}

/// Put a placed knob's shaft on panel (`cx`, `cy`).
fn centre_knob(b: &mut Builder<'_>, k: NodeId, (cx, cy): (f64, f64)) {
    let (ax, ay) = b
        .scene
        .node(k)
        .and_then(|n| {
            n.children
                .iter()
                .filter_map(|c| b.scene.node(*c))
                .find(|c| c.name == "knob")
        })
        .map_or((0.0, 0.0), |n| {
            (
                n.transform.tx + n.size.width / 2.0,
                n.transform.ty + n.size.height / 2.0,
            )
        });
    move_to(b, k, cx - ax, cy - ay);
}

/// Move a placed knob's legend so it is centred on panel `y`.
fn legend_to(b: &mut Builder<'_>, k: NodeId, y: f64) {
    let ty = b.scene.node(k).map_or(0.0, |n| n.transform.ty);
    let label = b
        .controls
        .get(&k)
        .and_then(|c| c.slots_of(SlotRole::Label).next());
    if let Some(n) = label.and_then(|l| b.scene.node_mut(NodeId(l))) {
        let size = if let NodeKind::Text(t) = &n.kind {
            t.font_size
        } else {
            9.0
        };
        n.transform = Transform::translate(n.transform.tx, y - ty - size * 0.68);
    }
}

/// INPUT / OUTPUT: the big knob on its dial disc, legend under it.
fn big_knob(
    b: &mut Builder<'_>,
    panel: NodeId,
    f: &Layout,
    legend: &str,
    addr: &str,
    c: (f64, f64),
) {
    let k = kit(
        b,
        panel,
        "knob.knurled_cap",
        &Place::new(legend)
            .size(BIG)
            .legend(legend)
            .scale(ATTEN)
            .variant(f.disc)
            .ink(Ink::Color(f.ink.into()))
            .touch(TOUCH_MIN)
            .bind(Binding::param(addr)),
    );
    centre_knob(b, k, c);
    legend_to(b, k, f.big_legend * H);
}

/// ATTACK / RELEASE: no disc, a ring on the cap, 270° of travel; the legend
/// where the unit prints it (over ATTACK; over or under RELEASE).
fn time_knob(
    b: &mut Builder<'_>,
    panel: NodeId,
    f: &Layout,
    legend: &str,
    addr: &str,
    c: (f64, f64),
    legend_y: f64,
) {
    let k = kit(
        b,
        panel,
        "knob.knurled_cap",
        &Place::new(legend)
            .size(SMALL)
            .legend(legend)
            .scale(TIMES)
            .sweep(270.0)
            .variant("bare")
            .options(&["dots=0"])
            .ink(Ink::Color(f.ink.into()))
            .touch(TOUCH_MIN)
            .bind(Binding::param(addr)),
    );
    centre_knob(b, k, c);
    legend_to(b, k, legend_y * H);
}

/// A key column over a choice param, its title printed over it.
#[allow(clippy::too_many_arguments)]
fn keys(
    b: &mut Builder<'_>,
    panel: NodeId,
    f: &Layout,
    title: &str,
    addr: &str,
    options: &[&str],
    reverse: bool,
    labels: &str,
    colors: Option<&str>,
) -> NodeId {
    let label_opt = format!("labels={labels}");
    let aspect = format!("aspect={:.3}", 1.85 * 4.0 / options.len() as f64);
    let mut opts: Vec<&str> = options.to_vec();
    opts.push(&label_opt);
    opts.push(&aspect);
    if f.plate != Plate::Black {
        opts.push("weight=500");
    }
    if let Some(c) = colors {
        opts.push(c);
    }
    let mut p = Place::new(title)
        .size(KEY)
        .options(&opts)
        .variant("black")
        .ink(Ink::Color(f.ink.into()))
        .bind(Binding::param(addr));
    p.reverse = reverse;
    let id = kit(b, panel, "button.push_in_key_bank", &p);
    let _ = title;
    id
}

/// Put a key column's keys on `cx`, spanning `top..bottom`, its title over it.
fn place_keys(
    b: &mut Builder<'_>,
    panel: NodeId,
    id: NodeId,
    (f, title): (&Layout, &str),
    cx: f64,
    labels_left: bool,
) {
    let (top, bottom) = f.keys;
    let (w, h) = size_of(b, id);
    let slot = KEY + 3.0;
    let x = if labels_left {
        cx + slot / 2.0 - w
    } else {
        cx - slot / 2.0
    };
    let y = (top + bottom) / 2.0 * H - h / 2.0;
    move_to(b, id, x, y);
    b.add(
        panel,
        ink("title", title, (cx, y - 11.0), 9.5, f.ink, (700.0, 0.04)),
    );
}

/// The plate's hardware: rack slots in the corners, the four panel screws,
/// the holes the unit has (a screwdriver adjustment, a lamp that isn't).
fn hardware(b: &mut Builder<'_>, panel: NodeId, f: &Layout) {
    let behind = if f.plate == Plate::Black {
        "#050506"
    } else {
        "#2a2b2d"
    };
    for (x, y) in [(0.017, 0.1), (0.017, 0.9), (0.983, 0.1), (0.983, 0.9)] {
        let (w, h) = (0.016 * W, 0.058 * H);
        let slot = draw::rect(
            "rack slot",
            x * W - w / 2.0,
            y * H - h / 2.0,
            w,
            h,
            draw::solid(behind),
        );
        b.add(
            panel,
            draw::with_effects(
                draw::rounded(slot, h / 2.0),
                vec![
                    draw::inner_shadow("#000000", 0.0, 1.5, 2.5),
                    draw::drop_shadow("#ffffff2e", 0.0, 1.0, 0.0),
                ],
            ),
        );
    }
    for (x, y) in f.screws {
        screw(b, panel, at((x, y)), 0.0072 * W, f.plate == Plate::Black);
    }
    for (x, y, r) in f.holes {
        let hole = draw::circle("hole", x * W, y * H, r * W, draw::solid("#060607"));
        b.add(
            panel,
            draw::with_effects(
                hole,
                vec![
                    draw::inner_shadow("#000000", 0.0, 1.0, 2.0),
                    draw::drop_shadow("#ffffff26", 0.0, 0.8, 0.0),
                ],
            ),
        );
    }
}

/// A pan-head Phillips screw, black on the black plate.
fn screw(b: &mut Builder<'_>, panel: NodeId, (cx, cy): (f64, f64), r: f64, black: bool) {
    let (hi, lo) = if black {
        ("#3a3a3d", "#050506")
    } else {
        ("#f4f4f2", "#8a8b8c")
    };
    let head = draw::circle(
        "screw",
        cx,
        cy,
        r,
        draw::radial_at(0.36, 0.3, 0.85, &[(0.0, hi), (1.0, lo)]),
    );
    b.add(
        panel,
        draw::with_effects(head, vec![draw::drop_shadow("#00000080", 0.0, 1.0, 1.6)]),
    );
    let cross = draw::polygon(&[
        (cx - r * 0.52, cy - r * 0.085),
        (cx - r * 0.085, cy - r * 0.085),
        (cx - r * 0.085, cy - r * 0.52),
        (cx + r * 0.085, cy - r * 0.52),
        (cx + r * 0.085, cy - r * 0.085),
        (cx + r * 0.52, cy - r * 0.085),
        (cx + r * 0.52, cy + r * 0.085),
        (cx + r * 0.085, cy + r * 0.085),
        (cx + r * 0.085, cy + r * 0.52),
        (cx - r * 0.085, cy + r * 0.52),
        (cx - r * 0.085, cy + r * 0.085),
        (cx - r * 0.52, cy + r * 0.085),
    ]);
    let cross = draw::path("recess", 0.0, 0.0, W, H, cross, draw::solid("#000000b3"));
    b.add(panel, cross);
}

/// The late plate's two printed fields: a lighter, finer-grained grey.
fn fields(b: &mut Builder<'_>, panel: NodeId) {
    for (name, x0, x1, y0, y1) in [
        ("left field", 0.072, 0.399, 0.15, 0.94),
        ("right field", 0.406, 0.876, 0.07, 0.94),
    ] {
        let r = draw::rect(
            name,
            x0 * W,
            y0 * H,
            (x1 - x0) * W,
            (y1 - y0) * H,
            draw::linear(180.0, &[(0.0, "#dcdedd8c"), (1.0, "#cfd1d08c")]),
        );
        b.add(panel, draw::rounded(r, 7.0));
    }
    b.add(
        panel,
        ink(
            "model",
            "Peak Limiter",
            (0.30 * W, 0.085 * H),
            10.5,
            "#26272a",
            (700.0, 0.10),
        ),
    );
    // The blue enamel badge at the right, blank of any maker's name.
    let (bx, by, bw, bh) = (0.944 * W, 0.5 * H, 0.03 * W, 0.075 * H);
    let badge = draw::rect(
        "badge",
        bx - bw / 2.0,
        by - bh / 2.0,
        bw,
        bh,
        draw::linear(180.0, &[(0.0, "#4f8fcf"), (1.0, "#245f9e")]),
    );
    b.add(
        panel,
        draw::with_effects(
            draw::stroked(draw::rounded(badge, 2.0), "#ffffffb3", 1.2),
            vec![draw::drop_shadow("#00000059", 0.0, 1.0, 1.5)],
        ),
    );
    b.add(
        panel,
        ink("badge", "FET", (bx, by), 8.5, "#ffffff", (800.0, 0.12)),
    );
}

/// The early plate's blue band behind the meter, jewel and lettering.
fn blue_band(b: &mut Builder<'_>, panel: NodeId, f: &Layout) {
    let (x0, x1) = (0.659 * W, 0.843 * W);
    let band = kit(
        b,
        panel,
        "panel.section",
        &Place::new("Meter band")
            .variant("fet_blue")
            .options(&[&format!("w={}", x1 - x0), &format!("h={H}")]),
    );
    if let Some(n) = b.scene.node_mut(band) {
        n.fills = vec![draw::linear(
            90.0,
            &[(0.0, "#1f6a92"), (0.5, "#2675a0"), (1.0, "#1f6a92")],
        )];
    }
    move_to(b, band, x0, 0.0);
    for x in [0.0, x1 - x0 - 1.0] {
        b.add(
            band,
            draw::rect("seam", x, 0.0, 1.0, H, draw::solid("#0000004d")),
        );
    }
    // The pilot jewel: the power switch, lit when on.
    let d = 0.018 * W;
    let jewel = kit(
        b,
        band,
        "button.jewel",
        &Place::new("Power")
            .size(d)
            .variant("red")
            .bind(Binding::param(POWER)),
    );
    move_to(b, jewel, 0.75 * W - x0 - d / 2.0, 0.15 * H - d / 2.0);
    let _ = f;
}

/// The movement: VU card, GR stream, switched by the METER keys.
fn gr_meter(b: &mut Builder<'_>, panel: NodeId, f: &Layout, w: f64) -> NodeId {
    kit(
        b,
        panel,
        "meter.vu_limiter",
        &Place::new("GR meter")
            .size(w)
            .variant(f.card)
            .options(&["glass"])
            .bind(Binding::stream(GR))
            // The METER keys switch the movement: gain reduction, the output
            // on its +8 or +4 range, or off (at rest).
            .select(SourceSelect::new(
                METER_SRC,
                vec![
                    Some(MeterTap::new(GR)),
                    Some(MeterTap::new(OUT_LEVEL).offset(-8.0)),
                    Some(MeterTap::new(OUT_LEVEL).offset(-4.0)),
                    None,
                ],
            )),
    )
}

/// What the unit printed under its meter — here what it is, not who made
/// it.
fn lettering(b: &mut Builder<'_>, panel: NodeId, f: &Layout, (cx, bottom): (f64, f64)) {
    match f.plate {
        Plate::Black => {
            b.add(
                panel,
                ink(
                    "model",
                    "Peak Limiter",
                    (cx, 0.835 * H),
                    9.5,
                    f.ink,
                    (700.0, 0.32),
                ),
            );
            b.add(
                panel,
                ink(
                    "maker",
                    "Limiting Amplifier",
                    (cx, 0.925 * H),
                    13.5,
                    f.ink,
                    (800.0, 0.06),
                ),
            );
        }
        Plate::Silver => {
            b.add(
                panel,
                ink(
                    "model",
                    "Limiting Amplifier",
                    (cx, 0.815 * H),
                    8.5,
                    "#e6f0f5",
                    (600.0, 0.30),
                ),
            );
            b.add(
                panel,
                ink(
                    "maker",
                    "Peak Limiter",
                    (cx, 0.9 * H),
                    13.0,
                    "#ffffff",
                    (800.0, 0.05),
                ),
            );
        }
        Plate::Late => {}
    }
    let _ = bottom;
}
