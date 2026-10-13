//! The FET limiter in a host's FX column: the same three plates redrawn
//! for a lane rather than a rack.
//!
//! - **Lane** (760 × 164): one of two units stacked in a lane — INPUT and
//!   OUTPUT on their dial discs, ATTACK and RELEASE beside them, the ratio
//!   keys in their column and the meter at the right with the plate's
//!   lettering under it. The meter keys stay on the rack face; the needle
//!   reads gain reduction.
//! - **Tall** (760 × 328): the unit alone in the column — the meter between
//!   the ratio and meter key banks across the top half, the four knobs
//!   across the bottom.
//!
//! The plate, the knobs, the keys, the meter card and the lettering are
//! the rack face's own; the addresses are the same `fet/…`, so a host
//! drives every shape of the unit with one set of params.

use frame::document::proto::{NodeId, NodeKind, Size};
use frame::surface::kit::{Ink, Place, TOUCH_MIN, draw};
use frame::surface::proto::{Binding, MeterTap, ParamInfo, SlotRole, SourceSelect};
use frame::surface::widgets::Builder;

use super::{
    ATTACK, ATTEN, BLACK, INPUT, LATE, Layout, METER, METER_SRC, OUTPUT, POWER, Plate, RATIO,
    RATIOS, RELEASE, SILVER, TIMES, centre_knob, ink, kit, move_to, params, screw, size_of,
    streams,
};
use crate::face::{Face, canvas};
use crate::lane::Shape;

/// Every plate at both shapes: `<file>-lane`, `<file>-tall`.
pub fn faces() -> Vec<Face> {
    let mut v = Vec::new();
    for f in [BLACK, SILVER, LATE] {
        for shape in Shape::ALL {
            let file = shape.file(f.file);
            let title = format!("{} ({})", f.title, shape.suffix());
            v.push(Face {
                file,
                plugin: "comp",
                scene: canvas(&title, |b, root| unit(b, root, &f, shape)),
                params: lane_params(&f, shape),
                streams: streams(),
            });
        }
    }
    v
}

/// The params a shape binds: the lane leaves the meter keys to the rack.
fn lane_params(f: &Layout, shape: Shape) -> Vec<ParamInfo> {
    params(f.plate == Plate::Silver)
        .into_iter()
        .filter(|p| shape == Shape::Tall || p.addr.0 != METER_SRC)
        .collect()
}

/// Where a shape puts things, panel px.
struct Spots {
    /// INPUT and OUTPUT shafts, ATTACK and RELEASE shafts.
    big: [(f64, f64); 2],
    time: [(f64, f64); 2],
    /// The line every knob legend sits on.
    legend_y: f64,
    /// The knob sizes (collar diameters).
    big_d: f64,
    time_d: f64,
    /// Ratio keys: centre x, top, key width, key height.
    ratio: (f64, f64, f64, f64),
    /// Meter keys likewise, where the shape has them.
    meter_keys: Option<(f64, f64, f64, f64)>,
    /// Meter: centre x, top, width.
    meter: (f64, f64, f64),
    /// Where the plate's lettering is centred (its first line); `None`
    /// under the meter.
    name: Option<(f64, f64)>,
}

const fn spots(shape: Shape) -> Spots {
    match shape {
        Shape::Lane => Spots {
            big: [(72.0, 70.0), (200.0, 70.0)],
            time: [(310.0, 70.0), (388.0, 70.0)],
            legend_y: 147.0,
            big_d: 62.0,
            time_d: 50.0,
            ratio: (482.0, 6.0, 36.0, 25.0),
            meter_keys: None,
            meter: (636.0, 12.0, 220.0),
            name: None,
        },
        Shape::Tall => Spots {
            big: [(110.0, 246.0), (290.0, 246.0)],
            time: [(470.0, 246.0), (636.0, 246.0)],
            legend_y: 314.0,
            big_d: 72.0,
            time_d: 64.0,
            ratio: (300.0, 10.0, 40.0, 27.0),
            meter_keys: Some((656.0, 10.0, 40.0, 34.0)),
            meter: (476.0, 14.0, 262.0),
            name: Some((132.0, 68.0)),
        },
    }
}

fn unit(b: &mut Builder<'_>, root: NodeId, f: &Layout, shape: Shape) {
    let (w, h) = shape.size();
    let s = spots(shape);
    if let Some(n) = b.scene.node_mut(root) {
        n.size = Size {
            width: w,
            height: h,
        };
    }
    let panel = kit(
        b,
        root,
        "panel.brushed",
        &Place::new("Faceplate").variant(f.panel).options(&[
            &format!("w={w}"),
            &format!("h={h}"),
            "ends=none",
        ]),
    );
    if let Some(n) = b.scene.node_mut(panel) {
        if f.plate != Plate::Black {
            n.fills = vec![draw::linear(
                178.0,
                &[(0.0, "#dddedc"), (0.48, "#d1d2d0"), (1.0, "#c2c3c1")],
            )];
        }
        n.corner_radii = frame::document::proto::CornerRadii::uniform(8.0);
    }
    for (x, y) in [
        (10.0, 10.0),
        (w - 10.0, 10.0),
        (10.0, h - 10.0),
        (w - 10.0, h - 10.0),
    ] {
        screw(b, panel, (x, y), 4.2, f.plate == Plate::Black);
    }
    match f.plate {
        Plate::Late => fields(b, panel, shape, &s),
        Plate::Silver => blue_band(b, panel, shape, &s),
        Plate::Black => {}
    }

    big_knob(b, panel, f, &s, ("Input", INPUT), s.big[0]);
    big_knob(b, panel, f, &s, ("Output", OUTPUT), s.big[1]);
    time_knob(b, panel, f, &s, ("Attack", ATTACK), s.time[0]);
    time_knob(b, panel, f, &s, ("Release", RELEASE), s.time[1]);

    let ratio = Bank {
        title: "Ratio",
        addr: RATIO,
        options: RATIOS,
        reverse: true,
        labels: "left",
        colors: None,
    };
    bank(b, panel, f, &ratio, s.ratio);
    if let Some(at) = s.meter_keys {
        let colors = (f.plate == Plate::Late).then_some("colors=#1c1c1e,#1c1c1e,#1c1c1e,#c3281c");
        let keys = Bank {
            title: "Meter",
            addr: METER_SRC,
            options: METER,
            reverse: false,
            labels: "right",
            colors,
        };
        bank(b, panel, f, &keys, at);
    }

    let (mx, my, mw) = s.meter;
    let vu = meter(b, panel, f, mw, shape == Shape::Tall);
    let (vw, vh) = size_of(b, vu);
    move_to(b, vu, mx - vw / 2.0, my);
    lettering(b, panel, f, s.name.unwrap_or((mx, my + vh + 12.0)), shape);
}

/// A knob legend: bold, a size a lane reads, on the shape's legend line.
fn legend(b: &mut Builder<'_>, k: NodeId, y: f64, size: f64) {
    let ty = b.scene.node(k).map_or(0.0, |n| n.transform.ty);
    let label = b
        .controls
        .get(&k)
        .and_then(|c| c.slots_of(SlotRole::Label).next());
    if let Some(n) = label.and_then(|l| b.scene.node_mut(NodeId(l))) {
        let old = if let NodeKind::Text(t) = &n.kind {
            t.font_size
        } else {
            size
        };
        if let NodeKind::Text(t) = &mut n.kind {
            t.font_size = size;
            t.font_weight = Some(800.0);
            t.letter_spacing = size * 0.08;
        }
        let grow = size / old.max(1.0);
        let (cx, w) = (n.transform.tx + n.size.width / 2.0, n.size.width * grow);
        n.size.width = w;
        n.size.height = (n.size.height * grow).ceil();
        n.transform.tx = cx - w / 2.0;
        n.transform.ty = y - ty - size * 0.68;
    }
}

/// INPUT / OUTPUT on their dial discs.
fn big_knob(
    b: &mut Builder<'_>,
    panel: NodeId,
    f: &Layout,
    s: &Spots,
    (name, addr): (&str, &str),
    c: (f64, f64),
) {
    let p = Place::new(name)
        .size(s.big_d)
        .legend(name)
        .scale(ATTEN)
        .variant(f.disc)
        .ink(Ink::Color(f.ink.into()))
        .touch(TOUCH_MIN)
        .bind(Binding::param(addr));
    let k = kit(b, panel, "knob.knurled_cap", &p);
    centre_knob(b, k, c);
    legend(b, k, s.legend_y, if s.big_d > 64.0 { 12.5 } else { 11.0 });
}

/// ATTACK / RELEASE: bare caps, 1…7 over 270°.
fn time_knob(
    b: &mut Builder<'_>,
    panel: NodeId,
    f: &Layout,
    s: &Spots,
    (name, addr): (&str, &str),
    c: (f64, f64),
) {
    let p = Place::new(name)
        .size(s.time_d)
        .legend(name)
        .scale(TIMES)
        .sweep(270.0)
        .variant("bare")
        .options(&["dots=0"])
        .ink(Ink::Color(f.ink.into()))
        .touch(TOUCH_MIN)
        .bind(Binding::param(addr));
    let k = kit(b, panel, "knob.knurled_cap", &p);
    centre_knob(b, k, c);
    legend(b, k, s.legend_y, if s.big_d > 64.0 { 12.5 } else { 11.0 });
}

/// A key column: its param, options top to bottom, where its labels go.
struct Bank<'a> {
    title: &'a str,
    addr: &'a str,
    options: &'a [&'a str],
    reverse: bool,
    labels: &'a str,
    colors: Option<&'a str>,
}

/// A key column whose keys are `kw × kh` (finger-sized), centred on `cx`
/// from `top`, its title over it and its position legends beside it at a
/// size the lane reads.
fn bank(
    b: &mut Builder<'_>,
    panel: NodeId,
    f: &Layout,
    k: &Bank<'_>,
    (cx, top, kw, kh): (f64, f64, f64, f64),
) {
    let label_opt = format!("labels={}", k.labels);
    let aspect = format!("aspect={:.3}", kh / kw);
    let mut opts: Vec<&str> = k.options.to_vec();
    opts.push(&label_opt);
    opts.push(&aspect);
    if f.plate != Plate::Black {
        opts.push("weight=500");
    }
    if let Some(c) = k.colors {
        opts.push(c);
    }
    let mut p = Place::new(k.title)
        .size(kw)
        .options(&opts)
        .variant("black")
        .ink(Ink::Color(f.ink.into()))
        .bind(Binding::param(k.addr));
    p.reverse = k.reverse;
    let id = kit(b, panel, "button.push_in_key_bank", &p);
    // The kit sets the legends to the key's scale; a lane wants them at
    // the size of its other print, tight to the column.
    let size = 11.5;
    let pad = 1.5 * kw / 20.0;
    let gap = 6.0;
    let (w, h) = size_of(b, id);
    let slot_w = kw + 2.0 * pad;
    let left = k.labels == "left";
    for row in b
        .scene
        .node(id)
        .map(|n| n.children.clone())
        .unwrap_or_default()
    {
        for t in b
            .scene
            .node(row)
            .map(|n| n.children.clone())
            .unwrap_or_default()
        {
            let Some(n) = b.scene.node_mut(t) else {
                continue;
            };
            if n.name != "legend" {
                continue;
            }
            if let NodeKind::Text(td) = &mut n.kind {
                td.font_size = size;
                td.font_weight = Some(800.0);
            }
            let lw = 34.0;
            n.size.width = lw;
            n.size.height = size * 1.4;
            n.transform.tx = if left {
                w - slot_w - gap - lw
            } else {
                slot_w + gap
            };
            n.transform.ty = kh / 2.0 - size * 0.7;
        }
    }
    let x = if left {
        cx + slot_w / 2.0 - w
    } else {
        cx - slot_w / 2.0
    };
    move_to(b, id, x, top + 14.0);
    let _ = h;
    b.add(
        panel,
        ink(
            "title",
            k.title,
            (cx, top + 4.0),
            10.5,
            f.ink,
            (800.0, 0.06),
        ),
    );
}

/// The movement on the plate's own card. The tall shape's meter keys switch
/// it as the rack face's do; the lane's reads gain reduction alone.
fn meter(b: &mut Builder<'_>, panel: NodeId, f: &Layout, w: f64, switched: bool) -> NodeId {
    let mut p = Place::new("GR meter")
        .size(w)
        .variant(f.card)
        .options(&["glass"])
        .bind(Binding::stream(super::GR));
    if switched {
        p = p.select(SourceSelect::new(
            METER_SRC,
            vec![
                Some(MeterTap::new(super::GR)),
                Some(MeterTap::new(super::OUT_LEVEL).offset(-8.0)),
                Some(MeterTap::new(super::OUT_LEVEL).offset(-4.0)),
                None,
            ],
        ));
    }
    kit(b, panel, "meter.vu_limiter", &p)
}

/// The plate's lettering: its two lines centred on `(cx, y)` (the
/// first), in the plate's ink — white on the early unit's band, where it
/// is on the band. The late unit prints its name with its fields.
fn lettering(b: &mut Builder<'_>, panel: NodeId, f: &Layout, (cx, y): (f64, f64), shape: Shape) {
    let tall = shape == Shape::Tall;
    let (lines, colors) = match f.plate {
        Plate::Black => (["Peak Limiter", "Limiting Amplifier"], [f.ink, f.ink]),
        Plate::Silver if tall => (["Limiting Amplifier", "Peak Limiter"], [f.ink, f.ink]),
        Plate::Silver => (
            ["Limiting Amplifier", "Peak Limiter"],
            ["#e6f0f5", "#ffffff"],
        ),
        Plate::Late => return,
    };
    if tall {
        b.add(
            panel,
            ink("model", lines[0], (cx, y), 10.5, colors[0], (700.0, 0.32)),
        );
        b.add(
            panel,
            ink(
                "maker",
                lines[1],
                (cx, y + 22.0),
                16.0,
                colors[1],
                (800.0, 0.06),
            ),
        );
    } else {
        b.add(
            panel,
            ink("maker", lines[1], (cx, y), 12.0, colors[1], (800.0, 0.06)),
        );
    }
}

/// The late plate's lighter printed fields, its name, and the blue badge.
fn fields(b: &mut Builder<'_>, panel: NodeId, shape: Shape, s: &Spots) {
    let (w, h) = shape.size();
    let split = s.time[1].0 + 38.0;
    let rects: Vec<(&str, f64, f64, f64, f64)> = match shape {
        Shape::Lane => vec![
            ("left field", 22.0, split, 6.0, h - 6.0),
            ("right field", split + 6.0, w - 22.0, 6.0, h - 6.0),
        ],
        Shape::Tall => vec![
            ("top field", 22.0, w - 22.0, 6.0, 172.0),
            ("bottom field", 22.0, w - 22.0, 178.0, h - 6.0),
        ],
    };
    for (name, x0, x1, y0, y1) in rects {
        let r = draw::rect(
            name,
            x0,
            y0,
            x1 - x0,
            y1 - y0,
            draw::linear(180.0, &[(0.0, "#dcdedd8c"), (1.0, "#cfd1d08c")]),
        );
        b.add(panel, draw::rounded(r, 7.0));
    }
    let (mx, _, mw) = s.meter;
    let (name_at, size, badge_at) = match s.name {
        Some((cx, cy)) => ((cx, cy + 10.0), 15.0, (cx, cy + 44.0)),
        None => ((mx, h - 13.0), 11.0, (mx + mw / 2.0 - 20.0, h - 13.0)),
    };
    b.add(
        panel,
        ink(
            "model",
            "Peak Limiter",
            name_at,
            size,
            "#26272a",
            (700.0, 0.10),
        ),
    );
    let (bx, by, bw, bh) = (badge_at.0, badge_at.1, 34.0, 15.0);
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

/// The early plate's painted blue band behind the meter (and, on the
/// lane, its lettering), with the pilot jewel.
fn blue_band(b: &mut Builder<'_>, panel: NodeId, shape: Shape, s: &Spots) {
    let (_, h) = shape.size();
    let (mx, _, mw) = s.meter;
    let (x0, x1, bh) = match shape {
        Shape::Lane => (mx - mw / 2.0 - 14.0, mx + mw / 2.0 + 14.0, h),
        Shape::Tall => (mx - mw / 2.0 - 16.0, mx + mw / 2.0 + 16.0, 174.0),
    };
    let band = kit(
        b,
        panel,
        "panel.section",
        &Place::new("Meter band")
            .variant("fet_blue")
            .options(&[&format!("w={}", x1 - x0), &format!("h={bh}")]),
    );
    if let Some(n) = b.scene.node_mut(band) {
        n.fills = vec![draw::linear(
            90.0,
            &[(0.0, "#1f6a92"), (0.5, "#2675a0"), (1.0, "#1f6a92")],
        )];
        n.corner_radii = frame::document::proto::CornerRadii::uniform(if shape == Shape::Tall {
            4.0
        } else {
            0.0
        });
    }
    move_to(b, band, x0, if shape == Shape::Tall { 4.0 } else { 0.0 });
    if shape == Shape::Lane {
        for x in [0.0, x1 - x0 - 1.0] {
            b.add(
                band,
                draw::rect("seam", x, 0.0, 1.0, bh, draw::solid("#0000004d")),
            );
        }
    }
    // The pilot jewel: low in the band's corner on the lane; under the
    // lettering when the unit stands alone (the band is all meter there).
    let d = 16.0;
    match s.name {
        Some((cx, cy)) => {
            let jewel = kit(
                b,
                panel,
                "button.jewel",
                &Place::new("Power")
                    .size(d + 4.0)
                    .variant("red")
                    .bind(Binding::param(POWER)),
            );
            move_to(b, jewel, cx - (d + 4.0) / 2.0, cy + 46.0);
        }
        None => {
            let jewel = kit(
                b,
                band,
                "button.jewel",
                &Place::new("Power")
                    .size(d)
                    .variant("red")
                    .bind(Binding::param(POWER)),
            );
            move_to(b, jewel, x1 - x0 - 10.0 - d, bh - 10.0 - d);
        }
    }
}
