//! The boutique suite's own look: its brush-lettered maker's mark and
//! script signature, the carved walnut plates, the chevron name row, the
//! suite's knobs and printed tick rings, brass hardware placed by centre,
//! and the soft teal lamp.

use document_proto::{Node, NodeId};
use surface_kit::materials::{Dir, Material, fill};
use surface_kit::{Ink, Place, TOUCH_MIN, draw};
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::suite_funk_rhythm::parts::{self, Lid, Type};

pub const PLUGIN: &str = "suite-boutique-clean";
/// The suite's lamp colour.
pub const TEAL: &str = "#3ff0e6";
/// Printed ink on the cream enclosures.
pub const INK: &str = "#161514";
/// The maker's mark, invented, set in a brush hand where the rig signs its
/// gear.
pub const MARK: &str = "hana";
/// The invented signature the rig prints in script.
pub const SIGNATURE: &str = "Hana K.";

/// A face in this suite, its readouts formatted.
#[must_use]
pub fn face(
    file: &'static str,
    scene: document_proto::Scene,
    params: Vec<surface_proto::ParamInfo>,
) -> Face {
    ui::formatted(Face {
        file,
        plugin: PLUGIN,
        scene,
        params,
        streams: Vec::new(),
    })
}

/// A face with streams.
#[must_use]
pub fn face_with(
    file: &'static str,
    scene: document_proto::Scene,
    params: Vec<surface_proto::ParamInfo>,
    streams: Vec<surface_proto::StreamInfo>,
) -> Face {
    ui::formatted(Face {
        file,
        plugin: PLUGIN,
        scene,
        params,
        streams,
    })
}

/// The maker's mark: the invented name brush-lettered (a marker brush
/// hand), `s` px tall, centred at (`cx`, `cy`).
pub fn brush_mark(
    b: &mut Builder<'_>,
    parent: NodeId,
    (cx, cy): (f64, f64),
    s: f64,
    color: &str,
) -> NodeId {
    parts::centred(
        b,
        parent,
        MARK,
        (cx, cy),
        Type::new(s * 1.25, color, 400.0).family(draw::family::CAVEAT_BRUSH),
    )
}

/// The invented signature in a flowing script, `s` px tall, centred at
/// (`cx`, `cy`), leaning a little uphill.
pub fn signature(
    b: &mut Builder<'_>,
    parent: NodeId,
    (cx, cy): (f64, f64),
    s: f64,
    color: &str,
) -> NodeId {
    let t = Type::new(s, color, 500.0).family(draw::family::DANCING_SCRIPT);
    let w = s * 5.0;
    b.add(
        parent,
        draw::rotated(
            parts::set(
                SIGNATURE,
                cx - w / 2.0,
                cy,
                w,
                document_proto::TextAlign::Center,
                t,
            ),
            -8.0,
        ),
    )
}

/// The rig's emblem, invented: a small lotus in a circle.
pub fn emblem(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), r: f64, color: &str) {
    let petal = |a: f64| {
        let tip = draw::polar(cx, cy + r * 0.25, r * 0.85, a);
        let l = draw::polar(cx, cy + r * 0.25, r * 0.3, a - 40.0);
        let rr = draw::polar(cx, cy + r * 0.25, r * 0.3, a + 40.0);
        vec![(cx, cy + r * 0.25), l, tip, rr, (cx, cy + r * 0.25)]
    };
    let lines = vec![
        petal(0.0),
        petal(-45.0),
        petal(45.0),
        vec![(cx - r * 0.7, cy + r * 0.45), (cx + r * 0.7, cy + r * 0.45)],
    ];
    parts::art(b, parent, &lines, (r * 0.09).max(0.9), color);
}

/// A carved walnut plate `(x, y, w, h)`: real walnut (grain running
/// down), a dark rebate round it. Returns the clipped plate frame to carve
/// into.
pub fn walnut(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64)) -> NodeId {
    let mut plate = draw::frame("Walnut", x, y, w, h);
    plate.corner_radii = document_proto::CornerRadii::uniform(3.5);
    plate.clips_content = true;
    plate.effects = vec![draw::drop_shadow("#ffffff66", 0.0, 0.8, 0.0)];
    let plate = b.add(parent, plate);
    fill(
        b,
        plate,
        (0.0, 0.0, w, h),
        3.5,
        &Material::Wood {
            color: "#7a3d27".into(),
            dir: Dir::Vertical,
        },
    );
    plate
}

/// Finish a carved plate: its rebate (shadow in along the top and left,
/// a lit lip at the bottom) over the carving.
pub fn plate_edge(b: &mut Builder<'_>, plate: NodeId) {
    let (w, h) = b
        .scene
        .node(plate)
        .map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
    for n in surface_kit::materials::light::edge("rebate", (0.0, 0.0, w, h), 3.5, 3.0, 1.0, false) {
        b.add(plate, n);
    }
    b.add(
        plate,
        draw::stroked(
            draw::rounded(
                draw::rect("rim", 0.0, 0.0, w, h, draw::solid("#00000000")),
                3.5,
            ),
            "#2c150c",
            1.2,
        ),
    );
}

/// Carve `lines` into a plate: a dark groove with a lit lower lip.
pub fn carve(b: &mut Builder<'_>, plate: NodeId, lines: &[Vec<(f64, f64)>], weight: f64) {
    let lit: Vec<Vec<(f64, f64)>> = lines
        .iter()
        .map(|l| {
            l.iter()
                .map(|(x, y)| (*x + weight * 0.35, *y + weight * 0.45))
                .collect()
        })
        .collect();
    b.add(
        plate,
        ui::strokes("carve light", &lit, weight * 0.8, draw::solid("#d08a6a60")),
    );
    b.add(
        plate,
        ui::strokes("carve", lines, weight, draw::solid("#2e140bd8")),
    );
}

/// The `> >>>> NAME >>>> >` row across a pedal at `cy`.
pub fn name_row(b: &mut Builder<'_>, parent: NodeId, name: &str, w: f64, cy: f64, ink: &str) {
    let chevron = |x: f64, h: f64| {
        vec![
            (x - h * 0.45, cy - h),
            (x + h * 0.45, cy),
            (x - h * 0.45, cy + h),
        ]
    };
    let big = vec![chevron(12.0, 5.0), chevron(w - 12.0, 5.0)];
    let mut small = Vec::new();
    for i in 0..4 {
        small.push(chevron(27.0 + 4.0 * f64::from(i), 1.8));
        small.push(chevron(w - 40.0 + 4.0 * f64::from(i), 1.8));
    }
    parts::art(b, parent, &big, 1.9, ink);
    parts::art(b, parent, &small, 0.7, ink);
    parts::centred(
        b,
        parent,
        name,
        (w / 2.0, cy),
        Type::new(12.5, ink, 700.0).track(0.06),
    );
}

/// A tick ring printed round a knob at (`cx`, `cy`): `n` ticks over a
/// 300° travel from `r` out `len`, every fifth (and the ends) longer, and
/// `min` / `max` under its ends when `minmax`.
#[allow(clippy::too_many_arguments)]
pub fn tick_ring(
    b: &mut Builder<'_>,
    parent: NodeId,
    (cx, cy): (f64, f64),
    r: f64,
    len: f64,
    n: usize,
    ink: &str,
    minmax: bool,
) {
    let lines: Vec<Vec<(f64, f64)>> = (0..n)
        .map(|i| {
            let a = -150.0 + 300.0 * i as f64 / (n - 1) as f64;
            let l = if i == 0 || i == n - 1 || i % 5 == 0 {
                len * 1.5
            } else {
                len
            };
            vec![draw::polar(cx, cy, r, a), draw::polar(cx, cy, r + l, a)]
        })
        .collect();
    parts::art(b, parent, &lines, (r * 0.03).clamp(0.5, 1.2), ink);
    if minmax {
        // Beside the ends of the travel, as the reference prints them.
        let t = Type::new((r * 0.2).max(3.5), ink, 500.0);
        let (dx, dy) = (r + len * 1.2, r * 0.95);
        parts::centred(b, parent, "min", (cx - dx, cy + dy), t);
        parts::centred(b, parent, "max", (cx + dx, cy + dy), t);
    }
}

/// How one of the suite's knobs looks: a `knob.3d` preset and its options
/// (the GPU knob; `knob.render` draws underneath where the shader doesn't
/// run).
#[derive(Clone, Copy)]
pub struct KnobLook<'a> {
    pub preset: &'a str,
    pub opts: &'a [&'a str],
}

/// Every knob of the suite is placed here: a `knob.3d` of diameter `d` on
/// `addr`, its shaft at (`cx`, `cy`), its hit box tight round the body, its
/// printed ring (if any) in `ink`. The face prints the legend.
#[allow(clippy::too_many_arguments)]
pub fn place_knob(
    b: &mut Builder<'_>,
    parent: NodeId,
    look: &KnobLook<'_>,
    addr: &str,
    name: &str,
    d: f64,
    (cx, cy): (f64, f64),
    ink: &str,
) -> NodeId {
    let mut p = Place::new(name)
        .size(d)
        .variant(look.preset)
        .bind(ui::bind(addr))
        .ink(Ink::Color(ink.into()))
        .keep_case()
        .touch(TOUCH_MIN);
    if !look.opts.is_empty() {
        p = p.options(look.opts);
    }
    let k = ui::put(b, parent, "knob.3d", &p, cx, cy);
    ui::strip_legend(b, k);
    k
}

/// The suite's knob: `look` of diameter `d` on `addr` at `at`, with its
/// tick ring printed round it in `ink` (`n` ticks; none when 0).
#[allow(clippy::too_many_arguments)]
pub fn knob(
    b: &mut Builder<'_>,
    parent: NodeId,
    look: &KnobLook<'_>,
    addr: &str,
    d: f64,
    at: (f64, f64),
    n: usize,
    ink: &str,
    minmax: bool,
) -> NodeId {
    if n > 0 {
        tick_ring(b, parent, at, d * 0.56, d * 0.09, n, ink, minmax);
    }
    place_knob(b, parent, look, addr, "", d, at, ink)
}

/// Give a control's readout its own format (`OFF` at a filter's stop,
/// `+` on a gain).
pub fn set_format(b: &mut Builder<'_>, control: NodeId, f: surface_proto::ValueFormat) {
    if let Some(c) = b.controls.get_mut(&control) {
        c.format = Some(f);
    }
}

/// A rendered stompbox (`panel.pedal_3d`) filling the page (`w × h`): the
/// lid in `finish` and `color` running down to the board (no front-face
/// band: the reference's boxes show only their lid), the jacks out of its
/// sides. Returns the lid; the pedal's parts go on it.
///
/// # Panics
/// If the kit's enclosure has no `lid` frame.
pub fn stompbox(
    b: &mut Builder<'_>,
    root: NodeId,
    (w, h): (f64, f64),
    color: &str,
    finish: &str,
    radius: f64,
) -> Lid {
    parts::page(b, root, w, h);
    let opts = vec![
        format!("w={w}"),
        format!("h={h}"),
        format!("color={color}"),
        format!("finish={finish}"),
        format!("radius={radius}"),
        "jacks=side".into(),
        "front=0".into(),
    ];
    let p = Place {
        options: Some(opts),
        ..Place::new("Enclosure").variant("standard")
    };
    let body = ui::put_at(b, root, "panel.pedal_3d", &p, 0.0, 0.0);
    let lid = b
        .scene
        .node(body)
        .and_then(|n| {
            n.children
                .iter()
                .copied()
                .find(|c| b.scene.node(*c).is_some_and(|k| k.name == "lid"))
        })
        .expect("panel.pedal_3d has a lid");
    let (lw, lh) = b
        .scene
        .node(lid)
        .map_or((w, h), |n| (n.size.width, n.size.height));
    Lid {
        id: lid,
        w: lw,
        h: lh,
    }
}

/// A brass stomp on `addr`, its cap `d` across, centred at `at`.
pub fn stomp(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    d: f64,
    at: (f64, f64),
    variant: &str,
) -> NodeId {
    let p = Place::new("Bypass")
        .size(d)
        .variant(variant)
        .bind(ui::bind(addr))
        .touch(TOUCH_MIN);
    parts::part(b, parent, "switch.brass_stomp", p, &["cap"], at)
}

/// A brass bat toggle on toggle `addr`, nut `d`, centred at `at`.
pub fn brass_toggle(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    name: &str,
    d: f64,
    at: (f64, f64),
) -> NodeId {
    let p = Place::new(name)
        .size(d)
        .bind(ui::bind(addr))
        .touch(TOUCH_MIN);
    parts::part(b, parent, "switch.brass_toggle", p, &["bushing"], at)
}

/// A three-way brass toggle on choice `addr` (options right, down, left).
pub fn brass_toggle3(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    name: &str,
    d: f64,
    at: (f64, f64),
) -> NodeId {
    let p = Place::new(name)
        .size(d)
        .bind(ui::bind(addr))
        .touch(TOUCH_MIN);
    parts::part(b, parent, "switch.brass_toggle3", p, &["bushing"], at)
}

/// A rounded-rect frame with fills — the building block of cabinets.
#[must_use]
pub fn slab(
    name: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    r: f64,
    fills: Vec<document_proto::Paint>,
) -> Node {
    let mut n = draw::frame(name, x, y, w, h);
    n.fills = fills;
    n.corner_radii = document_proto::CornerRadii::uniform(r);
    n
}
