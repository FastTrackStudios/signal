//! The four pre-FX stompboxes (156 × 286, the Standard class) as rendered
//! enclosures (`panel.pedal_3d`: soft satin lids in cream, peach and
//! khaki running down to the board, the jacks out of the sides): teal
//! scalloped `knob.3d` knobs inside fine tick
//! rings with `min` / `max` (all turned down near `min`, as the reference
//! shows them), the brush-lettered mark at the top, the block-caps name
//! between chevrons, and a carved walnut plate round a brass stomp with a
//! teal cap and a glowing teal LED.
//!
//! Positions are fractions of the lid, measured off the reference.
//!
//! - `01-comp-standard-156x286`: COMP (Comp, EQ, Volume, Mix) — grooves.
//! - `02-od1-standard-156x286`: OD1 (Volume, Gain, Tone, Mid Boost) — a
//!   hexagon lattice.
//! - `03-od2-standard-156x286`: OD2 (Volume, Gain, Cut) — tumbling blocks
//!   with droplets.
//! - `04-mod-standard-156x286`: MOD (Speed, Intensity; tremolo / chorus /
//!   vibrato on a three-way toggle) — concentric rings; its LED breathes
//!   at the speed.

use frame::document::proto::NodeId;
use frame::surface::proto::{Range, Unit};
use frame::surface::widgets::Builder;

use super::look::KnobLook;
use super::look::{self, INK, TEAL};
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::suite_funk_rhythm::parts::{self, Lid, PEDAL_H, STANDARD_W, Type};

const DIM: &str = "#4a4744";
const KNOB_D: f64 = 40.0;
/// The pedals' knob: a teal cylinder of rounded scallops, a flat top whose
/// edge follows them, a painted white line for the index.
pub const KNOB: KnobLook<'static> = KnobLook {
    preset: "scalloped",
    opts: &[
        "color=#5a807f",
        "pointer=#f3f0e7",
        "ribs=16",
        "rib_depth=0.045",
        "dome=0.08",
        "edge=0.14",
        "ticks=none",
    ],
};
/// Where the reference leaves every pedal knob: turned down near `min`.
const DOWN: f64 = 0.07;

/// Plate geometry, as fractions of the lid.
const PLATE: (f64, f64, f64, f64) = (0.068, 0.615, 0.864, 0.315);

/// A carving: the plate, its size, and the stomp's centre in it.
type Carve = fn(&mut Builder<'_>, NodeId, (f64, f64), (f64, f64));

struct Pedal<'a> {
    title: &'a str,
    ns: &'a str,
    name: &'a str,
    color: &'a str,
    /// Knob: param id, legend, centre and legend y (lid fractions).
    knobs: &'a [(&'a str, &'a str, (f64, f64), f64)],
    carve: Carve,
    pulse: Option<&'a str>,
    extra: Option<fn(&mut Builder<'_>, &Lid)>,
}

fn build(p: &Pedal<'_>) -> frame::document::proto::Scene {
    canvas(p.title, |b, root| {
        let lid = look::stompbox(b, root, (STANDARD_W, PEDAL_H), p.color, "satin", 12.0);
        look::brush_mark(b, lid.id, lid.at(0.5, 0.042), 11.0, INK);
        for (id, legend, (fx, fy), ly) in p.knobs {
            look::knob(
                b,
                lid.id,
                &KNOB,
                &format!("{}/{id}", p.ns),
                KNOB_D,
                lid.at(*fx, *fy),
                29,
                DIM,
                true,
            );
            parts::centred(
                b,
                lid.id,
                legend,
                lid.at(*fx, *ly),
                Type::new(6.8, INK, 700.0).track(0.04),
            );
        }
        if let Some(extra) = p.extra {
            extra(b, &lid);
        }
        look::name_row(b, lid.id, p.name, lid.w, lid.h * 0.533, INK);
        let (px, py) = lid.at(PLATE.0, PLATE.1);
        let (pw, ph) = (lid.w * PLATE.2, lid.h * PLATE.3);
        let plate = look::walnut(b, lid.id, (px, py, pw, ph));
        let stomp_at = lid.at(0.5, 0.78);
        (p.carve)(b, plate, (pw, ph), (stomp_at.0 - px, stomp_at.1 - py));
        look::plate_edge(b, plate);
        let on = format!("{}/on", p.ns);
        parts::status_led(
            b,
            lid.id,
            &on,
            5.5,
            lid.at(0.5, 0.668),
            TEAL,
            "dark",
            p.pulse,
        );
        look::stomp(b, lid.id, &on, 20.0, stomp_at, "teal");
    })
}

// ── carvings, in the plate's own space (w × h), `c` the stomp ─────────────

fn grooves(b: &mut Builder<'_>, plate: NodeId, (w, h): (f64, f64), _: (f64, f64)) {
    let n = (w / 4.0) as i32 + 2;
    let lines: Vec<Vec<(f64, f64)>> = (0..n)
        .map(|i| {
            let x = 2.0 + 4.0 * f64::from(i);
            vec![(x, -2.0), (x, h + 2.0)]
        })
        .collect();
    look::carve(b, plate, &lines, 1.5);
}

fn hexes(b: &mut Builder<'_>, plate: NodeId, _: (f64, f64), _: (f64, f64)) {
    let r = 7.5;
    let (dx, dy) = (r * 3.0_f64.sqrt(), r * 1.5);
    let mut lines = Vec::new();
    for row in -1..9 {
        for col in -1..11 {
            let cx = f64::from(col) * dx + if row % 2 == 0 { 0.0 } else { dx / 2.0 };
            let cy = f64::from(row) * dy;
            let hex: Vec<(f64, f64)> = (0..=6)
                .map(|i| frame::surface::kit::draw::polar(cx, cy, r, 60.0 * f64::from(i)))
                .collect();
            let inner: Vec<(f64, f64)> = (0..=6)
                .map(|i| frame::surface::kit::draw::polar(cx, cy, r * 0.5, 60.0 * f64::from(i)))
                .collect();
            lines.push(hex);
            lines.push(inner[..4].to_vec());
        }
    }
    look::carve(b, plate, &lines, 0.9);
}

fn blocks(b: &mut Builder<'_>, plate: NodeId, (w, h): (f64, f64), _: (f64, f64)) {
    let r = 11.0;
    let (dx, dy) = (r * 3.0_f64.sqrt(), r * 1.5);
    let mut lines = Vec::new();
    let mut drops = String::new();
    for row in -1..8 {
        for col in -1..9 {
            let cx = f64::from(col) * dx + if row % 2 == 0 { 0.0 } else { dx / 2.0 };
            let cy = f64::from(row) * dy;
            let hex: Vec<(f64, f64)> = (0..=6)
                .map(|i| frame::surface::kit::draw::polar(cx, cy, r, 60.0 * f64::from(i)))
                .collect();
            lines.push(hex);
            // The cube's three edges from the centre.
            for a in [0.0, 120.0, 240.0] {
                lines.push(vec![
                    (cx, cy),
                    frame::surface::kit::draw::polar(cx, cy, r, a),
                ]);
            }
            // A droplet on each face.
            for a in [60.0, 180.0, 300.0] {
                let (x, y) = frame::surface::kit::draw::polar(cx, cy, r * 0.5, a);
                let (tx, ty) = frame::surface::kit::draw::polar(x, y, 3.2, a + 180.0);
                drops.push_str(&format!("M {tx:.2} {ty:.2} Q {:.2} {:.2} {x:.2} {y:.2} Q {:.2} {:.2} {tx:.2} {ty:.2} Z ", x + 2.4, y - 0.5, x - 2.4, y + 0.5));
            }
        }
    }
    look::carve(b, plate, &lines, 0.9);
    b.add(
        plate,
        frame::surface::kit::draw::path(
            "droplets",
            0.0,
            0.0,
            w,
            h,
            drops,
            frame::surface::kit::draw::solid("#3e1b0fb8"),
        ),
    );
}

fn rings(b: &mut Builder<'_>, plate: NodeId, _: (f64, f64), (cx, cy): (f64, f64)) {
    let lines: Vec<Vec<(f64, f64)>> = (1..22)
        .map(|i| parts::ring(cx, cy, 6.0 + 5.0 * f64::from(i)))
        .collect();
    look::carve(b, plate, &lines, 1.6);
}

// ── the pedals ───────────────────────────────────────────────────────────

/// Four knobs two by two: centre and legend y.
const TWO_BY_TWO: [((f64, f64), f64); 4] = [
    ((0.252, 0.13), 0.228),
    ((0.748, 0.13), 0.228),
    ((0.252, 0.34), 0.44),
    ((0.748, 0.34), 0.44),
];

/// A 0..100 % control left where the reference leaves it.
fn down_pct(addr: &str, name: &str) -> frame::surface::proto::ParamInfo {
    parts::pct(addr, name, DOWN * 100.0)
}

/// A ±12 dB control left where the reference leaves it.
fn down_db(addr: &str, name: &str) -> frame::surface::proto::ParamInfo {
    parts::db(addr, name, -12.0, 12.0, -12.0 + 24.0 * DOWN)
}

pub(super) fn comp() -> Face {
    let k = [
        ("comp", "Comp"),
        ("eq", "EQ"),
        ("volume", "Volume"),
        ("mix", "Mix"),
    ];
    let knobs: Vec<(&str, &str, (f64, f64), f64)> = k
        .iter()
        .zip(TWO_BY_TWO)
        .map(|((id, l), (at, y))| (*id, *l, at, y))
        .collect();
    let scene = build(&Pedal {
        title: "Comp",
        ns: "comp",
        name: "COMP",
        color: "#ece6dc",
        knobs: &knobs,
        carve: grooves,
        pulse: None,
        extra: None,
    });
    let params = vec![
        down_pct("comp/comp", "Comp"),
        down_pct("comp/eq", "EQ"),
        down_db("comp/volume", "Volume"),
        down_pct("comp/mix", "Mix"),
        parts::switch("comp/on", "Bypass", true),
    ];
    look::face("01-comp-standard-156x286", scene, params)
}

pub(super) fn od1() -> Face {
    let k = [
        ("volume", "Volume"),
        ("gain", "Gain"),
        ("tone", "Tone"),
        ("mid", "Mid Boost"),
    ];
    let knobs: Vec<(&str, &str, (f64, f64), f64)> = k
        .iter()
        .zip(TWO_BY_TWO)
        .map(|((id, l), (at, y))| (*id, *l, at, y))
        .collect();
    let scene = build(&Pedal {
        title: "OD1",
        ns: "od1",
        name: "OD1",
        color: "#e7e1d7",
        knobs: &knobs,
        carve: hexes,
        pulse: None,
        extra: None,
    });
    let params = vec![
        down_db("od1/volume", "Volume"),
        down_pct("od1/gain", "Gain"),
        down_pct("od1/tone", "Tone"),
        down_pct("od1/mid", "Mid Boost"),
        parts::switch("od1/on", "Bypass", true),
    ];
    look::face("02-od1-standard-156x286", scene, params)
}

pub(super) fn od2() -> Face {
    let knobs = [
        ("volume", "Volume", (0.26, 0.135), 0.24),
        ("gain", "Gain", (0.74, 0.135), 0.24),
        ("cut", "Cut", (0.5, 0.33), 0.443),
    ];
    let scene = build(&Pedal {
        title: "OD2",
        ns: "od2",
        name: "OD2",
        color: "#edc8ae",
        knobs: &knobs,
        carve: blocks,
        pulse: None,
        extra: None,
    });
    let params = vec![
        down_db("od2/volume", "Volume"),
        down_pct("od2/gain", "Gain"),
        down_pct("od2/cut", "Cut"),
        parts::switch("od2/on", "Bypass", true),
    ];
    look::face("03-od2-standard-156x286", scene, params)
}

fn mode_switch(b: &mut Builder<'_>, lid: &Lid) {
    look::brass_toggle3(b, lid.id, "mod/mode", "Mode", 18.0, lid.at(0.5, 0.354));
    let t = Type::new(6.8, INK, 700.0).track(0.04);
    parts::centred(b, lid.id, "Tremolo", lid.at(0.24, 0.34), t);
    parts::centred(b, lid.id, "Vibrato", lid.at(0.76, 0.34), t);
    parts::centred(b, lid.id, "Chorus", lid.at(0.5, 0.43), t);
}

pub(super) fn modulation() -> Face {
    let knobs = [
        ("speed", "Speed", (0.26, 0.13), 0.228),
        ("intensity", "Intensity", (0.74, 0.13), 0.228),
    ];
    let scene = build(&Pedal {
        title: "MOD",
        ns: "mod",
        name: "MOD",
        color: "#ddd3b5",
        knobs: &knobs,
        carve: rings,
        pulse: Some("param(mod/speed)"),
        extra: Some(mode_switch),
    });
    let params = vec![
        ui::param(
            "mod/speed",
            "Speed",
            Range::log(0.1, 12.0, Unit::Hz),
            parts::log_at(0.1, 12.0, DOWN),
        )
        .featured(),
        down_pct("mod/intensity", "Intensity"),
        // In the toggle's order: right, down, left. The reference's lever
        // leans right.
        parts::pick("mod/mode", "Mode", &["Vibrato", "Chorus", "Tremolo"], 0),
        parts::switch("mod/on", "Bypass", true),
    ];
    look::face("04-mod-standard-156x286", scene, params)
}
