//! Bloom: a reverb that swells instead of striking — its diffusers feed
//! one another, so the tail is quiet at first, thickens, and opens out to
//! its peak well after the note before it dies away. Drawn as what it
//! sounds like: a night flower opening. Each ring of petals unfurls a
//! beat after the one inside it, the whole flower open at the swell's
//! peak and closing as the tail fades; it opens slower and stays open
//! longer the longer the decay, and a bigger space is a bigger flower.
//!
//! Beside it, where there is room, the bloom's envelope — the slow rise,
//! the late peak, the long fall — with a playhead in step with the
//! flower. Its tail end is the decay: drag it out and the bloom lasts.
//!
//! Two finishes: light, a conservatory's sage and ivory; dark, the night
//! garden.

use document_proto::NodeId;
use document_proto::motion::{Dynamics, Motion, Target};
use surface_kit::{draw, motion};
use surface_proto::{Axis, Control, ControlKind, HandleAxis};
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, VERB};

const NAME: &str = "Bloom";
const FILE: &str = "85-verb-bloom";
const BACK: &str = "85-verb-bloom-back";
/// The tall face's rows: the picture a little under half, big knobs below.
const ROWS: kit::TallRows = kit::TallRows {
    split: 134.0,
    main: (30.0, 50.0),
    more: (126.0, 40.0),
};
/// The bloom's light: the section's purple, and its hot heart.
const PETAL: &str = kit::PURPLE_LAMP;
const HEART: &str = "#fbeaff";

/// One finish's colours.
struct Look {
    ink: &'static str,
    dim: &'static str,
    panel: [(f64, &'static str); 3],
    vine: &'static str,
    well: [(f64, &'static str); 2],
    knob: [&'static str; 6],
    wheel: [&'static str; 6],
    accent: &'static str,
    scrim: &'static str,
}

const LIGHT: Look = Look {
    ink: "#22342a",
    dim: "#5f7464",
    panel: [(0.0, "#e9efe4"), (0.55, "#dbe5d5"), (1.0, "#cad8c4")],
    vine: "#3f6a4c30",
    well: [(0.0, "#22172f"), (1.0, "#120c1a")],
    knob: [
        "color=#f7f4ec",
        "pointer=#3b1d6e",
        "index=line",
        "index_w=0.12",
        "skirt=plain",
        "skirt_color=#6f8a6a",
    ],
    wheel: [
        "color=#f7f4ec",
        "pointer=#3b1d6e",
        "index=line",
        "index_w=0.1",
        "skirt=plain",
        "skirt_color=#58715a",
    ],
    accent: "#6a3fc4",
    scrim: "#dfe8d9e6",
};

const DARK: Look = Look {
    ink: "#efe6f3",
    dim: "#9d90a8",
    panel: [(0.0, "#211a2b"), (0.55, "#18121f"), (1.0, "#100b16")],
    vine: "#b58cff1c",
    well: [(0.0, "#160d20"), (1.0, "#0a0610")],
    knob: [
        "color=#2b2335",
        "pointer=#d9c8ff",
        "index=line",
        "index_w=0.12",
        "skirt=plain",
        "skirt_color=#120d17",
    ],
    wheel: [
        "color=#261e30",
        "pointer=#d9c8ff",
        "index=line",
        "index_w=0.1",
        "skirt=plain",
        "skirt_color=#0e0a12",
    ],
    accent: kit::PURPLE_LAMP,
    scrim: "#18121fe6",
};

fn a(id: &str) -> String {
    format!("{VERB}/{id}")
}

/// Where the bloom is in its swell, 0..1, round and round: slower the
/// longer the decay.
fn cycle() -> String {
    format!("fract(time * lerp(0.42, 0.12, norm({VERB}/decay)))")
}

/// How open ring `k` (0 the innermost) is now: it opens a beat after the
/// one inside it and closes as the tail fades.
fn open(k: usize) -> String {
    let k = k as f64;
    let c = cycle();
    format!(
        "smoothstep({:.2}, {:.2}, {c}) * (1 - smoothstep({:.2}, 1, {c}))",
        0.02 + 0.06 * k,
        0.24 + 0.07 * k,
        0.5 + 0.06 * k
    )
}

/// A ring of `n` petals `len` long round (`r`, `r`) in a `2r` box, turned
/// `turn` degrees: each a pointed leaf from the heart to its tip.
fn petals(r: f64, n: u32, len: f64, width: f64, turn: f64) -> String {
    let mut d = String::new();
    for i in 0..n {
        let phi = turn + 360.0 * f64::from(i) / f64::from(n);
        let tip = draw::polar(r, r, len, phi);
        let l = draw::polar(r, r, len * 0.62, phi - width);
        let rr = draw::polar(r, r, len * 0.62, phi + width);
        let base = draw::polar(r, r, len * 0.12, phi);
        d.push_str(&format!(
            "M {:.2} {:.2} Q {:.2} {:.2} {:.2} {:.2} Q {:.2} {:.2} {:.2} {:.2} Z ",
            base.0, base.1, l.0, l.1, tip.0, tip.1, rr.0, rr.1, base.0, base.1
        ));
    }
    d
}

/// The flower round (`cx`, `cy`), `big` its outer petals' reach.
fn flower(b: &mut Builder<'_>, parent: NodeId, (cx, cy): (f64, f64), big: f64) {
    // Its rings, outermost first: more petals, paler, opening last.
    let rings: [(u32, f64, f64, f64); 4] = [
        (12, 1.0, 0.0, 0.3),
        (9, 0.8, 0.0, 0.48),
        (7, 0.6, 0.0, 0.7),
        (5, 0.4, 0.0, 0.9),
    ];
    for (i, (n, reach, width, strength)) in rings.iter().enumerate() {
        let k = rings.len() - 1 - i;
        let len = big * reach;
        let r = len + 2.0;
        let turn = 360.0 / f64::from(*n) * 0.5 * (k % 2) as f64;
        let _ = width;
        let width = 360.0 / f64::from(*n) * 0.42;
        let fill = draw::radial_at(
            0.5,
            0.5,
            0.5,
            &[
                (0.0, &ui::alpha("#e9dcff", 0.5 * strength)),
                (0.3, &ui::alpha(PETAL, 0.7 * strength)),
                (0.85, &ui::alpha(PETAL, 0.22 * strength)),
                (1.0, &ui::alpha(PETAL, 0.04)),
            ],
        );
        // In a frame its own size: the motions turn and grow it about the
        // heart, not about its petals' extent.
        let holder = b.add(
            parent,
            draw::frame(&format!("ring {k}"), cx - r, cy - r, 2.0 * r, 2.0 * r),
        );
        let mut ring = draw::path(
            "petals",
            0.0,
            0.0,
            2.0 * r,
            2.0 * r,
            petals(r, *n, len, width, turn),
            fill,
        );
        ring.stroke = document_proto::Stroke {
            paints: vec![draw::solid(&ui::alpha("#e9dcff", 0.35 + 0.45 * strength))],
            weight: 0.9,
            ..document_proto::Stroke::default()
        };
        b.add(
            holder,
            ui::glowing(ring, PETAL, 0.25 + 0.4 * strength, kit::GLOW),
        );
        let ring = holder;
        let o = open(k);
        let grow = format!("(0.16 + 0.84 * {o}) * (0.7 + 0.3 * norm({VERB}/size))");
        motion::add(
            b,
            ring,
            Motion::new(Target::Scale)
                .input(grow)
                .range(0.0, 1.0)
                .pivot(0.5, 0.5),
        );
        // Each ring unfurls with a turn, alternate rings the other way.
        let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
        motion::add(
            b,
            ring,
            Motion::new(Target::Rotate)
                .input(format!("1 - {o}"))
                .range(0.0, 28.0 * sign)
                .pivot(0.5, 0.5),
        );
        motion::add(
            b,
            ring,
            Motion::new(Target::Opacity)
                .input(format!(
                    "(0.3 + 0.7 * {o}) * (0.45 + 0.55 * param({VERB}/on))"
                ))
                .range(0.0, 1.0),
        );
    }
    // The stamens and the heart, still.
    let sr = big * 0.2;
    let stamens: Vec<Vec<(f64, f64)>> = (0..10)
        .map(|i| {
            vec![
                (cx, cy),
                draw::polar(
                    cx,
                    cy,
                    sr * (0.8 + 0.25 * crate::reverb::jitter(f64::from(i)).abs()),
                    18.0 + 36.0 * f64::from(i),
                ),
            ]
        })
        .collect();
    b.add(
        parent,
        ui::strokes("stamens", &stamens, 0.7, draw::solid("#f3e6ffcc")),
    );
    let mut tips = String::new();
    for p in &stamens {
        tips.push_str(&draw::regular(p[1].0, p[1].1, 1.3, 6, 0.0));
        tips.push(' ');
    }
    b.add(
        parent,
        ui::glowing(
            ui::area("anthers", tips, cx + big, cy + big, draw::solid("#fff4c2")),
            "#ffe9a8",
            0.6,
            kit::GLOW,
        ),
    );
    let heart = draw::emissive(
        draw::circle(
            "heart",
            cx,
            cy,
            big * 0.07,
            draw::radial_at(0.5, 0.45, 0.6, &[(0.0, "#ffffff"), (1.0, HEART)]),
        ),
        draw::hex(PETAL),
        1.4,
        kit::BLOOM,
    );
    let heart = b.add(parent, heart);
    motion::add(
        b,
        heart,
        Motion::new(Target::Opacity)
            .input(format!(
                "(0.5 + 0.5 * {}) * (0.4 + 0.6 * param({VERB}/on))",
                open(0)
            ))
            .range(0.0, 1.0),
    );
}

/// The envelope at (`x`, `y`) `w` × `h` on `parent` (whose height is
/// `ph`): the bloom's energy over time, stretched by the decay, its
/// playhead in step with the flower and its tail end a handle on the
/// decay.
fn envelope(
    b: &mut Builder<'_>,
    parent: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    ph: f64,
    l: &Look,
) {
    let line = "#cdbde6";
    ui::silk_left(
        b,
        parent,
        "The bloom · energy over time",
        x,
        y - 8.0,
        5.5,
        line,
        800.0,
    );
    let base = y + h;
    ui::line(
        b,
        parent,
        "axis",
        &[(x, base), (x + w, base)],
        0.8,
        &ui::alpha(line, 0.6),
    );
    for k in 0..=6 {
        let tx = x + w * f64::from(k) / 6.0;
        ui::line(
            b,
            parent,
            "tick",
            &[(tx, base), (tx, base + 3.0)],
            0.8,
            &ui::alpha(line, 0.6),
        );
    }
    // The hit, then the swell: quiet at first, a late peak, a long fall.
    let peak = 0.32;
    let pts: Vec<(f64, f64)> = (0..=80)
        .map(|i| {
            let u = f64::from(i) / 80.0;
            let e = if u < peak {
                (std::f64::consts::FRAC_PI_2 * u / peak).sin().powi(2)
            } else {
                (-(u - peak) * 3.4).exp()
            };
            let ripple = 1.0 + 0.05 * (u * 70.0).sin();
            (u * w, h - (h - 4.0) * e * ripple)
        })
        .collect();
    let g = b.add(parent, draw::frame("Envelope", x, y, w, h));
    let mut d = format!("M 0 {h:.1} ");
    for (px, py) in &pts {
        d += &format!("L {px:.1} {py:.1} ");
    }
    d += &format!("L {w:.1} {h:.1} Z");
    b.add(
        g,
        draw::path(
            "swell",
            0.0,
            0.0,
            w,
            h,
            d,
            draw::linear(
                180.0,
                &[
                    (0.0, &ui::alpha(PETAL, 0.5)),
                    (1.0, &ui::alpha(PETAL, 0.04)),
                ],
            ),
        ),
    );
    b.add(
        g,
        ui::glowing(
            ui::strokes("edge", &[pts], 1.5, draw::solid(PETAL)),
            PETAL,
            1.0,
            kit::GLOW,
        ),
    );
    let stretch = |b: &mut Builder<'_>, id: NodeId| {
        motion::add(
            b,
            id,
            Motion::new(Target::StretchX)
                .input(format!("norm({VERB}/decay)"))
                .range(0.45, 1.0)
                .pivot(0.0, 0.5)
                .dynamics(Dynamics::spring(220.0, 26.0)),
        );
    };
    stretch(b, g);
    ui::line(
        b,
        parent,
        "hit",
        &[(x + 1.0, base), (x + 1.0, y + 6.0)],
        1.6,
        HEART,
    );
    // The playhead, where the flower is in its swell.
    let head = b.add(
        parent,
        ui::glowing(
            draw::rect("playhead", x, y, 1.6, h, draw::solid(HEART)),
            PETAL,
            1.1,
            kit::GLOW,
        ),
    );
    let at = format!("{w:.1} * {} * (0.45 + 0.55 * norm({VERB}/decay))", cycle());
    motion::add(b, head, Motion::new(Target::X).input(at).range(0.0, 1.0));
    motion::add(
        b,
        head,
        Motion::new(Target::Opacity)
            .input(format!("0.25 + 0.75 * param({VERB}/on)"))
            .range(0.0, 1.0),
    );

    // The tail end: a handle on the decay, travelling as the envelope
    // stretches.
    let (x0, x1) = (x + w * 0.45, x + w);
    let area = b.add(parent, draw::frame("tail travel", x0, 0.0, x1 - x0, ph));
    let hw = 30.0;
    let grip = b.add(
        area,
        draw::frame("Tail", 0.4 * (x1 - x0) - hw / 2.0, 0.0, hw, ph),
    );
    let c = hw / 2.0;
    b.add(
        grip,
        ui::strokes(
            "drop",
            &[vec![(c, y + 4.0), (c, base - 6.0)]],
            0.8,
            draw::solid(&ui::alpha(line, 0.45)),
        ),
    );
    let tab = draw::rounded(
        draw::rect("tab", c - 13.0, base - 6.0, 26.0, 12.0, draw::solid(PETAL)),
        3.0,
    );
    b.add(grip, ui::glowing(tab, PETAL, 0.6, kit::GLOW));
    ui::plain(b, grip, "Decay", c, base, 5.2, "#140a24", 800.0);
    let kind = ControlKind::Handle {
        x: Some(ui::bind(&a("decay"))),
        y: None,
        wheel: None,
        x_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
        y_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
    };
    b.controls
        .insert(grip, Control::new(kind).bound(ui::bind(&a("decay"))));
    let _ = l;
}

/// The picture in (`x`, `y`, `w`, `h`) on `panel`: the flower and, where
/// it is wide, its envelope from `top` down (below a nameplate over the
/// picture). Returns its well.
fn picture(
    b: &mut Builder<'_>,
    panel: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    top: f64,
    l: &Look,
) -> NodeId {
    let mut well = draw::frame("Night", x, y, w, h);
    well.fills = vec![draw::linear(180.0, &l.well)];
    well.clips_content = true;
    well.corner_radii = document_proto::CornerRadii::uniform(6.0);
    well.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 7.0)];
    let well = b.add(panel, well);
    let wide = w > 360.0;
    let fw = if wide { (h * 1.25).min(w * 0.4) } else { w };
    // The flower's own dusk: a pool of its light behind it.
    let (cx, cy) = (fw / 2.0 + if wide { 4.0 } else { 0.0 }, h / 2.0);
    let big = fw.min(h) * 0.46;
    b.add(
        well,
        draw::circle(
            "dusk",
            cx,
            cy,
            big * 1.25,
            draw::radial_at(
                0.5,
                0.5,
                0.5,
                &[
                    (0.0, &ui::alpha(PETAL, 0.16)),
                    (1.0, &ui::alpha(PETAL, 0.0)),
                ],
            ),
        ),
    );
    flower(b, well, (cx, cy), big);
    if wide {
        let ex = fw + 22.0;
        let ey = top.max(24.0);
        envelope(b, well, (ex, ey, w - ex - 18.0, h - ey - 16.0), h, l);
    }
    kit::glass(b, well);
    well
}

/// The panel, `w` × `h`, a vine engraved along its foot.
fn ground(b: &mut Builder<'_>, root: NodeId, (w, h): (f64, f64), l: &Look, foot: bool) -> NodeId {
    let mut panel = draw::frame("Panel", 0.0, 0.0, w, h);
    panel.fills = vec![draw::linear(180.0, &l.panel)];
    panel.corner_radii = document_proto::CornerRadii::uniform(10.0);
    let panel = b.add(
        root,
        draw::with_effects(
            panel,
            vec![
                draw::inner_shadow("#ffffff22", 0.0, 1.0, 0.0),
                draw::inner_shadow("#00000040", 0.0, -2.0, 3.0),
            ],
        ),
    );
    // (Not on a phone: its knobs' values sit along the foot.)
    if !foot {
        return panel;
    }
    // The vine: a stem wandering along the foot, a leaf at each turn.
    let y0 = h - 7.0;
    let stem: Vec<(f64, f64)> = (0..=(w / 4.0) as i32)
        .map(|i| {
            let x = f64::from(i) * 4.0;
            (x, y0 + 2.2 * (x / 23.0).sin())
        })
        .collect();
    b.add(
        panel,
        ui::strokes("vine", &[stem], 0.8, draw::solid(l.vine)),
    );
    let mut leaves = String::new();
    let mut x = 12.0;
    let mut up = true;
    while x < w - 12.0 {
        let y = y0 + 2.2 * (x / 23.0).sin();
        let tip = (x + 5.0, if up { y - 5.0 } else { y + 4.0 });
        leaves.push_str(&format!(
            "M {x:.1} {y:.1} Q {:.1} {:.1} {:.1} {:.1} Q {:.1} {:.1} {x:.1} {y:.1} Z ",
            x + 1.0,
            tip.1,
            tip.0,
            tip.1,
            tip.0,
            y
        ));
        x += 36.0;
        up = !up;
    }
    b.add(panel, ui::area("leaves", leaves, w, h, draw::solid(l.vine)));
    panel
}

/// The nameplate's title: a botanist's hand.
fn title(b: &mut Builder<'_>, t: NodeId, h: f64, size: f64, ink: &str) {
    let script = draw::Lettering::family(draw::family::DANCING_SCRIPT).weight(700.0);
    crate::suite_studio_blues::parts::set(b, t, NAME, 34.0, h / 2.0, size, ink, &script, 0.0);
}

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

/// The face at `tier`: the flower and its envelope beside the controls.
/// On a phone the picture is the flower alone and the knobs are Size and
/// Level.
#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane(NAME, tier, |b, root| {
        let panel = ground(b, root, (w, kit::H), l, !small_room);
        let sw = if small_room {
            kit::PHONE_PICTURE_R - 10.0
        } else {
            560.0
        };
        let well = picture(b, panel, (10.0, 10.0, sw, kit::H - 20.0), 0.0, l);
        let lit = ui::alpha(kit::PURPLE_LAMP, 0.55);
        if small_room {
            kit::nameplate_top_right(
                b,
                panel,
                VERB,
                tier,
                (6.0, 30.0),
                84.0,
                220.0,
                12.0,
                (l.accent, l.dim),
                |b, t| title(b, t, 30.0, 24.0, l.ink),
            );
            let (tx, ty, td) = kit::PHONE_TIME;
            kit::time_knob(
                b,
                panel,
                VERB,
                (tx, ty, td),
                11.0,
                (l.ink, l.dim, &lit),
                |b, p, addr| {
                    kit::knob3d_bare(b, p, addr, td, (tx, ty), &l.wheel);
                },
            );
            let (xs, ky, kd) = kit::PHONE_KNOBS;
            for (i, (id, legend)) in [("size", "Size"), ("level", "Level")].iter().enumerate() {
                kit::knob3d(
                    b,
                    panel,
                    &a(id),
                    legend,
                    kd,
                    (xs[i], ky),
                    &l.knob,
                    l.ink,
                    l.dim,
                );
            }
        } else {
            kit::nameplate_top_right(
                b,
                panel,
                VERB,
                tier,
                (10.0, 28.0),
                84.0,
                200.0,
                10.0,
                (l.accent, l.dim),
                |b, t| title(b, t, 28.0, 22.0, l.ink),
            );
            let x0 = 10.0 + sw + 20.0;
            let (tx, ty, td) = (x0 + 40.0, 78.0, 52.0);
            kit::time_knob(
                b,
                panel,
                VERB,
                (tx, ty, td),
                8.5,
                (l.ink, l.dim, &lit),
                |b, p, addr| {
                    kit::knob3d_bare(b, p, addr, td, (tx, ty), &l.wheel);
                },
            );
            for (i, (id, legend)) in [
                ("size", "Size"),
                ("predelay", "Pre-Delay"),
                ("level", "Level"),
            ]
            .iter()
            .enumerate()
            {
                kit::knob3d(
                    b,
                    panel,
                    &a(id),
                    legend,
                    34.0,
                    (x0 + 116.0 + 60.0 * i as f64, 90.0),
                    &l.knob,
                    l.ink,
                    l.dim,
                );
            }
        }
        kit::face_bypass(b, panel, VERB, tier, "purple", "On");
        kit::flip_arrow(b, panel, VERB, tier, false, (l.ink, &ui::alpha(l.dim, 0.3)));
        kit::responsive(b, panel, &[well]);
    });
    let mut f = kit::reverb_face(kit::tier_file(FILE, tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::time_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its back at `tier`: the panel, no picture — the advanced controls
/// across it, and (standing alone) the nameplate, the bypass and the
/// arrow.
#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane(NAME, tier, |b, root| {
        let panel = ground(b, root, (w, kit::H), l, !small_room);
        kit::back_knobs(
            b,
            panel,
            tier,
            &kit::VERB_MORE,
            l.ink,
            |b, p, id, legend, d, at| {
                kit::knob3d(b, p, &a(id), legend, d, at, &l.knob, l.ink, l.dim);
            },
        );
        let (np, preset_w, size, ts) = if small_room {
            ((6.0, 30.0), 220.0, 12.0, 24.0)
        } else {
            ((10.0, 28.0), 200.0, 10.0, 22.0)
        };
        kit::head(b, panel, VERB, true, |b, head| {
            kit::nameplate_top_right(
                b,
                head,
                VERB,
                tier,
                np,
                84.0,
                preset_w,
                size,
                (l.accent, l.dim),
                |b, t| title(b, t, np.1, ts, l.ink),
            );
            kit::face_bypass(b, head, VERB, tier, "purple", "On");
        });
        kit::flip_arrow(b, panel, VERB, tier, true, (l.ink, &ui::alpha(l.dim, 0.3)));
    });
    let mut f = kit::reverb_face(kit::tier_file(BACK, tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its tall face: the flower and its envelope across the top, every
/// control under them.
#[must_use]
pub fn tall(dark: bool) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let scene = kit::tall_lane(NAME, |b, root| {
        let panel = ground(b, root, (kit::TALL_W, kit::TALL_H), l, true);
        picture(
            b,
            panel,
            (10.0, 10.0, kit::TALL_W - 20.0, ROWS.split - 16.0),
            48.0,
            l,
        );
        let t = kit::Tall {
            ns: VERB,
            main: &[
                ("size", "Size"),
                ("predelay", "Pre-Delay"),
                ("modulation", "Mod"),
                ("level", "Level"),
            ],
            inks: (l.ink, l.dim, &ui::alpha(kit::PURPLE_LAMP, 0.55), l.accent),
            scrim: l.scrim,
            bypass: ("purple", "On"),
            title_w: 84.0,
        };
        kit::tall_chrome_rows(
            b,
            panel,
            &t,
            ROWS,
            |b, p, addr, legend, d, at| {
                kit::knob3d(b, p, addr, legend, d, at, &l.knob, l.ink, l.dim);
            },
            |b, p, addr, (cx, cy, d)| {
                kit::knob3d_bare(b, p, addr, d, (cx, cy), &l.wheel);
            },
            |b, tn| title(b, tn, 30.0, 24.0, l.ink),
        );
    });
    let mut f = kit::reverb_face(kit::tall_file(FILE, dark), scene);
    f.params.extend(kit::tall_params(VERB));
    f
}
