//! The three amp heads (Head form, 1200 wide, the height from each
//! faceplate's aspect, handle included), drawn from the plugin's own amp
//! pages, their knobs at the angles those screenshots show:
//!
//! - `05-blush-head-1200x700`: *Blush* — a glossy pink head with rounded
//!   ends, a recessed control shelf (input, VOLUME, BASS, CUT, MASTER,
//!   LEVEL, power toggle and teal pilot) over a woven grille embossed with
//!   a bold maze of raised keys — spirals, combs and meanders, one per
//!   tile — and a braided cord handle.
//! - `06-ranch-head-1200x680`: *Ranch* — a leather-covered box with leather
//!   corners, a cream faceplate with a swooping lower edge (input, a
//!   three-way MODE, GAIN, BASS, MID, TREBLE, PRESENCE, MASTER, OUTPUT,
//!   power, pilot, a script signature) over a brass circle-lattice grille
//!   with a centre plaque.
//! - `07-kintsugi-head-1200x650`: *Kintsugi* — a cream tolex head whose
//!   front is an art panel: a sleeping face between dark hair, broken like
//!   porcelain and mended in gold, gold leaves drifting over the hair —
//!   over a brushed-gold control strip (input, BRIGHT, GAIN, BASS, MIDDLE,
//!   TREBLE, SHIFT, MASTER, PRESENCE, OUTPUT, POWER).

use frame::document::proto::NodeId;
use frame::surface::kit::materials::{Dir, Grain, Material, fill};
use frame::surface::kit::{Place, TOUCH_MIN, draw};
use frame::surface::proto::ParamInfo;
use frame::surface::widgets::Builder;

use super::look::KnobLook;
use super::look::{self, TEAL};
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::suite_funk_rhythm::parts::{self, HEAD_W, Type};

// ── shared ───────────────────────────────────────────────────────────────

/// A carry handle: a strap (`strap` paint, `thick` px) arching between two
/// brass fittings at `x0`/`x1`, its feet on `y`, rising `rise`.
#[allow(clippy::too_many_arguments)]
fn handle(
    b: &mut Builder<'_>,
    root: NodeId,
    x0: f64,
    x1: f64,
    y: f64,
    rise: f64,
    thick: f64,
    strap: &str,
) {
    let pts: Vec<(f64, f64)> = (0..=40)
        .map(|i| {
            let u = f64::from(i) / 40.0;
            let lift = (u * std::f64::consts::PI).sin().powf(0.45);
            (x0 + (x1 - x0) * u, y - rise * lift)
        })
        .collect();
    b.add(
        root,
        draw::with_effects(
            ui::strokes(
                "strap",
                std::slice::from_ref(&pts),
                thick,
                draw::solid(strap),
            ),
            vec![draw::drop_shadow("#00000059", 0.0, 3.0, 5.0)],
        ),
    );
    // The braid: short diagonal lays along the strap.
    let lays: Vec<Vec<(f64, f64)>> = pts
        .windows(2)
        .map(|p| {
            let ((ax, ay), (bx, by)) = (p[0], p[1]);
            vec![(ax, ay - thick * 0.3), (bx, by + thick * 0.3)]
        })
        .collect();
    b.add(
        root,
        ui::strokes("braid", &lays, thick * 0.18, draw::solid("#ffffff1c")),
    );
    let hi: Vec<(f64, f64)> = pts.iter().map(|(x, y)| (*x, y - thick * 0.22)).collect();
    b.add(
        root,
        ui::strokes("strap light", &[hi], thick * 0.2, draw::solid("#ffffff22")),
    );
    for x in [x0, x1] {
        let loop_ = parts::ring(x, y - 6.0, 9.0);
        b.add(
            root,
            ui::strokes("fitting", &[loop_], 4.0, draw::solid("#c49a52")),
        );
        b.add(
            root,
            draw::rounded(
                draw::rect(
                    "foot",
                    x - 16.0,
                    y - 2.0,
                    32.0,
                    6.0,
                    draw::linear(180.0, &[(0.0, "#e6c886"), (1.0, "#8a6630")]),
                ),
                3.0,
            ),
        );
    }
}

/// A round push cap (bright, shift, power), its lamp over it in teal.
fn push(
    b: &mut Builder<'_>,
    parent: NodeId,
    addr: &str,
    d: f64,
    at: (f64, f64),
    lamp: bool,
) -> NodeId {
    let mut p = Place::new(addr)
        .size(d)
        .variant("grey")
        .bind(ui::bind(addr))
        .legend("")
        .touch(TOUCH_MIN);
    let color = format!("color={TEAL}");
    p = p.options(&[if lamp { color.as_str() } else { "nolamp" }]);
    parts::part(b, parent, "button.round_cap", p, &["cap"], at)
}

fn amp_face(
    file: &'static str,
    scene: frame::document::proto::Scene,
    params: Vec<ParamInfo>,
) -> Face {
    look::face(file, scene, params)
}

/// A covered box `(x, y, w, h)` in `m`, its edges lit: returns the box.
fn covered(
    b: &mut Builder<'_>,
    parent: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    r: f64,
    m: &Material,
) -> NodeId {
    let mut n = draw::rounded(draw::frame("Covering", x, y, w, h), r);
    n.effects = vec![draw::drop_shadow("#00000066", 0.0, 10.0, 22.0)];
    let n = b.add(parent, n);
    fill(b, n, (0.0, 0.0, w, h), r, m);
    for e in frame::surface::kit::materials::light::edge(
        "covering edge",
        (0.0, 0.0, w, h),
        r,
        10.0,
        1.0,
        true,
    ) {
        b.add(n, e);
    }
    n
}

// ── Blush ────────────────────────────────────────────────────────────────

/// One maze tile's raised key inside (`x, y, w, h`), motif `v`: a squared
/// spiral, a comb, a mirrored spiral or a meander — drawn as thick
/// polylines on a `step` grid.
fn maze(x: f64, y: f64, w: f64, h: f64, v: usize) -> Vec<Vec<(f64, f64)>> {
    let step = 21.0;
    let (l, r, t, bt) = (x + 16.0, x + w - 16.0, y + 15.0, y + h - 15.0);
    let flip = |pts: Vec<(f64, f64)>, fx: bool, fy: bool| -> Vec<(f64, f64)> {
        pts.into_iter()
            .map(|(px, py)| {
                (
                    if fx { l + r - px } else { px },
                    if fy { t + bt - py } else { py },
                )
            })
            .collect()
    };
    // A squared spiral winding in from the bottom-left: up, right, down,
    // left, each lap a step further in.
    let spiral = || {
        let mut pts = vec![(l, bt)];
        let (mut a, mut b, mut c, mut d) = (l, t, r, bt);
        let mut k = 0;
        while c - a > step * 0.9 && d - b > step * 0.9 && k < 14 {
            match k % 4 {
                0 => pts.push((a, b)),
                1 => pts.push((c, b)),
                2 => {
                    pts.push((c, d));
                    a += step;
                    b += step;
                }
                _ => {
                    pts.push((a, d));
                    c -= step;
                    d -= step;
                }
            }
            k += 1;
        }
        pts
    };
    match v % 4 {
        0 => vec![spiral()],
        1 => {
            // A comb: a spine down the left, teeth to the right, a hook at the end.
            let n = ((bt - t) / step).floor() as usize;
            let mut lines = vec![vec![(l, t), (l, bt)]];
            for i in 0..=n {
                let yy = t + step * i as f64;
                let reach = if i % 2 == 0 { r } else { r - step * 1.5 };
                lines.push(vec![(l, yy), (reach, yy)]);
            }
            lines.push(vec![(r, t), (r, t + step)]);
            lines
                .into_iter()
                .map(|p| flip(p, v % 8 == 5, false))
                .collect()
        }
        2 => vec![flip(spiral(), true, true)],
        _ => {
            // A meander: rows joined at alternate ends.
            let n = ((bt - t) / step).floor() as usize;
            let mut pts = Vec::new();
            for i in 0..=n {
                let yy = t + step * i as f64;
                let (a, c) = if i % 2 == 0 { (l, r) } else { (r, l + step) };
                pts.push((a, yy));
                pts.push((c, yy));
            }
            vec![flip(pts, v % 8 == 7, false)]
        }
    }
}

pub(super) fn blush() -> Face {
    const PINK: &str = "#efc9cf";
    const INK: &str = "#2a1d20";
    let (w, h) = (HEAD_W, 700.0);
    let top = 96.0; // the head's top edge (the handle stands above)
    let scene = canvas("Blush Head", |b, root| {
        parts::page(b, root, w, h);
        // Feet.
        for x in [300.0, 900.0] {
            b.add(
                root,
                draw::path(
                    "foot",
                    0.0,
                    0.0,
                    w,
                    h,
                    draw::polygon(&[
                        (x - 60.0, top + 560.0),
                        (x + 60.0, top + 560.0),
                        (x + 30.0, h),
                        (x - 30.0, h),
                    ]),
                    draw::linear(180.0, &[(0.0, "#2d2c3a"), (1.0, "#12121a")]),
                ),
            );
        }
        // the shell: smooth painted, fully rounded ends.
        let shell = covered(
            b,
            root,
            (0.0, top, w, 580.0),
            130.0,
            &Material::Powder {
                color: PINK.into(),
                coarse: false,
            },
        );
        // The recessed front: the control shelf over the grille.
        let front = look::slab(
            "Front",
            (34.0, 36.0, w - 68.0, 510.0),
            96.0,
            vec![draw::solid("#e8bac1")],
        );
        let front = b.add(
            shell,
            draw::with_effects(front, vec![draw::inner_shadow("#8a4a5577", 0.0, 6.0, 12.0)]),
        );
        let shelf = look::slab(
            "Shelf",
            (8.0, 8.0, w - 84.0, 196.0),
            88.0,
            vec![draw::linear(180.0, &[(0.0, "#f2c9cf"), (1.0, "#e6b3ba")])],
        );
        let shelf = b.add(
            front,
            draw::with_effects(
                shelf,
                vec![
                    draw::drop_shadow("#00000033", 0.0, 3.0, 5.0),
                    draw::inner_shadow("#ffffff80", 0.0, 1.5, 1.0),
                ],
            ),
        );
        // Grille panel: woven cloth with a bold maze of raised keys.
        let (gw, gh) = (w - 180.0, 250.0);
        let grille = look::slab(
            "Grille",
            (56.0, 226.0, gw, gh),
            22.0,
            vec![draw::solid("#dcb0b7")],
        );
        let grille = b.add(
            front,
            draw::with_effects(grille, vec![draw::inner_shadow("#7a3a4566", 0.0, 3.0, 6.0)]),
        );
        let mut weave = String::new();
        for i in 0..(gh as usize / 3) {
            weave.push_str(&format!("M 0 {y:.1} L {gw:.1} {y:.1} ", y = 3.0 * i as f64));
        }
        let mut wn = draw::path("weave", 0.0, 0.0, gw, gh, weave, draw::solid("#00000000"));
        wn.stroke = frame::document::proto::Stroke {
            paints: vec![draw::solid("#ffffff22")],
            weight: 1.0,
            ..frame::document::proto::Stroke::default()
        };
        b.add(grille, wn);
        let cols = 7;
        let (tw, th) = (gw / cols as f64, gh / 2.0);
        let mut keys = Vec::new();
        for row in 0..2 {
            for col in 0..cols {
                if row == 1 && col == cols - 1 {
                    continue;
                }
                let (x, y) = (tw * col as f64, th * f64::from(row));
                keys.extend(maze(x, y, tw, th, col * 3 + row as usize * 5 + 1));
                keys.push(parts::round_rect(x + 4.0, y + 4.0, tw - 8.0, th - 8.0, 8.0));
            }
        }
        // Raised keys: their shadow down-right, the light up-left, the top.
        let off = |dx: f64, dy: f64| {
            keys.iter()
                .map(|l| l.iter().map(|(x, y)| (x + dx, y + dy)).collect())
                .collect::<Vec<Vec<(f64, f64)>>>()
        };
        b.add(
            grille,
            ui::strokes("key shade", &off(2.6, 3.2), 11.0, draw::solid("#8a4a58a0")),
        );
        b.add(
            grille,
            ui::strokes("key light", &off(-1.3, -1.3), 10.0, draw::solid("#fff0f2")),
        );
        b.add(
            grille,
            ui::strokes("keys", &keys, 9.4, draw::solid("#ebbec5")),
        );
        look::brush_mark(b, front, (w - 150.0, 440.0), 34.0, INK);
        // The control shelf. Coordinates in the shelf's space.
        let plate = look::slab(
            "Emblem",
            (58.0, 38.0, 104.0, 104.0),
            12.0,
            vec![draw::linear(160.0, &[(0.0, "#f8dde1"), (1.0, "#e0b0b8")])],
        );
        let plate = b.add(
            shelf,
            draw::with_effects(
                plate,
                vec![
                    draw::drop_shadow("#00000040", 0.0, 3.0, 6.0),
                    draw::inner_shadow("#ffffffa0", 0.0, 1.5, 1.0),
                ],
            ),
        );
        for (x, y) in [(12.0, 12.0), (92.0, 12.0), (12.0, 92.0), (92.0, 92.0)] {
            b.add(
                plate,
                draw::circle(
                    "rivet",
                    x,
                    y,
                    3.0,
                    draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#fff1c4"), (1.0, "#8a6630")]),
                ),
            );
        }
        look::emblem(b, plate, (52.0, 48.0), 30.0, INK);
        let t = Type::new(10.5, INK, 600.0).track(0.28).upper();
        parts::centred(b, shelf, "Input", (245.0, 58.0), t);
        let knob = KnobLook {
            preset: "scalloped",
            opts: &[
                "color=#0e0b0c",
                "pointer=#d9a64e",
                "ribs=9",
                "rib_depth=0.12",
                "height=0.6",
                "coat=0.85",
                "rough=0.14",
                "index_in=0.08",
                "index_out=0.74",
                "index_w=0.07",
                "ticks=none",
                "fallback=fluted",
            ],
        };
        let knobs = [
            ("blush/volume", "Volume"),
            ("blush/bass", "Bass"),
            ("blush/cut", "Cut"),
            ("blush/master", "Master"),
            ("blush/level", "Level"),
        ];
        for (i, (addr, legend)) in knobs.iter().enumerate() {
            let x = 358.0 + 133.0 * i as f64;
            look::knob(
                b,
                shelf,
                &knob,
                addr,
                70.0,
                (x, 104.0),
                41,
                "#5a4046",
                false,
            );
            parts::centred(b, shelf, legend, (x, 166.0), t);
        }
        parts::centred(b, shelf, "Power", (1003.0, 50.0), t);
        look::brass_toggle(b, shelf, "blush/power", "Power", 22.0, (1003.0, 100.0));
        parts::status_led(
            b,
            shelf,
            "blush/power",
            13.0,
            (1003.0, 148.0),
            TEAL,
            "light",
            None,
        );
        // Input jack, and the cable over the grille and off the unit.
        let (jx, jy) = (34.0 + 8.0 + 245.0, top + 36.0 + 8.0 + 104.0);
        parts::cable(
            b,
            root,
            (jx, jy),
            14.0,
            &[
                (jx + 4.0, jy + 200.0),
                (jx - 6.0, jy + 400.0),
                (jx - 60.0, h - 30.0),
                (jx - 200.0, h - 8.0),
            ],
        );
        handle(b, root, 340.0, 860.0, top + 6.0, 84.0, 12.0, "#2b2340");
    });
    // The knobs as the reference screenshot turns them.
    let params = vec![
        parts::dial10("blush/volume", "Volume", 4.0),
        parts::dial10("blush/bass", "Bass", 2.3),
        parts::dial10("blush/cut", "Cut", 0.8),
        parts::dial10("blush/master", "Master", 5.7),
        parts::dial10("blush/level", "Level", 6.7),
        parts::switch("blush/power", "Power", true),
    ];
    amp_face("05-blush-head-1200x700", scene, params)
}

// ── Ranch ────────────────────────────────────────────────────────────────

pub(super) fn ranch() -> Face {
    const INK: &str = "#1d1a17";
    let (w, h) = (HEAD_W, 680.0);
    let top = 56.0;
    let bh = 606.0;
    let scene = canvas("Ranch Head", |b, root| {
        parts::page(b, root, w, h);
        for x in [210.0, 990.0] {
            b.add(
                root,
                draw::rounded(
                    draw::rect(
                        "foot",
                        x - 40.0,
                        top + bh - 10.0,
                        80.0,
                        h - top - bh + 10.0,
                        draw::linear(180.0, &[(0.0, "#2a2a2c"), (1.0, "#0e0e10")]),
                    ),
                    14.0,
                ),
            );
        }
        let body = covered(
            b,
            root,
            (0.0, top, w, bh),
            30.0,
            &Material::Leather {
                color: "#8e4524".into(),
            },
        );
        // The grille: brass circle lattice over black cloth.
        let (gx, gy, gw, gh) = (54.0, 170.0, w - 108.0, bh - 170.0 - 54.0);
        let mut g = look::slab(
            "Grille",
            (gx, gy, gw, gh),
            4.0,
            vec![draw::radial_at(
                0.5,
                0.5,
                0.8,
                &[(0.0, "#1a1714"), (1.0, "#090807")],
            )],
        );
        g.clips_content = true;
        let grille = b.add(body, g);
        let pitch = 32.0;
        let mut circles = Vec::new();
        for row in 0..((gh / pitch) as i32 + 2) {
            for col in 0..((gw / pitch) as i32 + 2) {
                circles.push(parts::ring(
                    f64::from(col) * pitch,
                    f64::from(row) * pitch,
                    pitch * 0.5,
                ));
                circles.push(parts::ring(
                    f64::from(col) * pitch + pitch / 2.0,
                    f64::from(row) * pitch + pitch / 2.0,
                    pitch * 0.36,
                ));
            }
        }
        b.add(
            grille,
            ui::strokes(
                "lattice shade",
                &circles
                    .iter()
                    .map(|l| l.iter().map(|(x, y)| (x + 1.5, y + 2.0)).collect())
                    .collect::<Vec<_>>(),
                5.0,
                draw::solid("#00000099"),
            ),
        );
        b.add(
            grille,
            ui::strokes("lattice", &circles, 3.4, draw::solid("#bf954c")),
        );
        for n in frame::surface::kit::materials::light::edge(
            "grille recess",
            (0.0, 0.0, gw, gh),
            4.0,
            6.0,
            1.0,
            false,
        ) {
            b.add(grille, n);
        }
        // The faceplate, its lower edge swooping up in the middle.
        let (fx, fy, fw) = (54.0, 54.0, w - 108.0);
        let mut edge = vec![(0.0, 0.0), (fw, 0.0), (fw, 248.0)];
        edge.extend((0..=40).map(|i| {
            let u = f64::from(i) / 40.0;
            (
                fw * (1.0 - u),
                248.0 - 56.0 * (u * std::f64::consts::PI).sin().powf(0.8),
            )
        }));
        let mut face = draw::path(
            "Faceplate",
            fx,
            fy,
            fw,
            250.0,
            draw::polygon(&edge),
            draw::linear(180.0, &[(0.0, "#f3ecdc"), (1.0, "#e2d6bd")]),
        );
        face.effects = vec![
            draw::drop_shadow("#00000080", 0.0, 4.0, 8.0),
            draw::inner_shadow("#ffffff99", 0.0, 1.5, 1.0),
        ];
        b.add(body, face);
        let plate = b.add(body, draw::frame("Controls", fx, fy, fw, 250.0));
        for (x, y) in [(20.0, 20.0), (fw - 20.0, 20.0)] {
            parts::screw(b, plate, x, y, 5.0, "#9a7236");
        }
        let t = Type::new(9.5, INK, 700.0).track(0.06).upper();
        let small = Type::new(8.0, INK, 700.0).upper();
        parts::centred(b, plate, "Input", (78.0, 64.0), t);
        let (jx, jy) = (fx + 78.0, top + fy + 98.0);
        parts::centred(b, plate, "Mode", (138.0, 38.0), t);
        parts::centred(b, plate, "Thicker", (138.0, 64.0), small);
        parts::centred(b, plate, "Thick", (182.0, 98.0), small);
        parts::centred(b, plate, "Normal", (138.0, 132.0), small);
        look::brass_toggle3(b, plate, "ranch/mode", "Mode", 20.0, (138.0, 98.0));
        let knob = KnobLook {
            preset: "knurled",
            opts: &[
                "color=#171616",
                "pointer=#e0b48a",
                "index=line",
                "cap_r=0.96",
                "height=0.62",
                "dome=0.06",
                "edge=0.12",
                "ribs=70",
                "knurl_band=0.55",
                "rib_depth=0.025",
                "tilt=18",
                "index_in=0.02",
                "index_out=0.95",
                "index_w=0.06",
                "coat=0.6",
                "rough=0.22",
                "ticks=none",
                "fallback=domed",
            ],
        };
        let knobs = [
            ("ranch/gain", "Gain"),
            ("ranch/bass", "Bass"),
            ("ranch/mid", "Mid"),
            ("ranch/treble", "Treble"),
            ("ranch/presence", "Presence"),
            ("ranch/master", "Master"),
            ("ranch/output", "Output"),
        ];
        for (i, (addr, legend)) in knobs.iter().enumerate() {
            let x = 253.0 + 100.0 * i as f64;
            look::knob(b, plate, &knob, addr, 54.0, (x, 98.0), 41, "#4a4238", false);
            parts::centred(b, plate, legend, (x, 150.0), t);
            if i + 1 < knobs.len() {
                ui::rule(b, plate, x + 30.0, 150.0, 40.0, 1.0, "#4a423880");
            }
        }
        ui::rule(b, plate, 897.0, 98.0, 40.0, 1.0, "#4a423880");
        parts::centred(b, plate, "Power", (955.0, 60.0), t);
        look::brass_toggle(b, plate, "ranch/power", "Power", 22.0, (955.0, 98.0));
        parts::status_led(
            b,
            plate,
            "ranch/power",
            14.0,
            (955.0, 148.0),
            TEAL,
            "dark",
            None,
        );
        look::emblem(b, plate, (44.0, 190.0), 22.0, INK);
        look::signature(b, plate, (fw - 72.0, 188.0), 22.0, "#4a4238");
        // The plaque on the grille.
        let (px, py, pw, ph) = (w / 2.0 - 100.0, gy + 150.0, 200.0, 100.0);
        let plq = look::slab(
            "Plaque",
            (px, py, pw, ph),
            12.0,
            vec![draw::linear(170.0, &[(0.0, "#f4efe4"), (1.0, "#d9d1c0")])],
        );
        let plq = b.add(
            body,
            draw::with_effects(
                draw::stroked(plq, "#a88a5a", 3.0),
                vec![draw::drop_shadow("#000000a0", 0.0, 4.0, 8.0)],
            ),
        );
        for (x, y) in [
            (12.0, 12.0),
            (pw - 12.0, 12.0),
            (12.0, ph - 12.0),
            (pw - 12.0, ph - 12.0),
        ] {
            b.add(
                plq,
                draw::circle(
                    "rivet",
                    x,
                    y,
                    3.5,
                    draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#fff1c4"), (1.0, "#8a6630")]),
                ),
            );
        }
        look::brush_mark(b, plq, (pw / 2.0, ph / 2.0), 26.0, "#2b2622");
        for (x0, dir) in [(18.0, 1.0), (pw - 18.0, -1.0)] {
            let wave: Vec<(f64, f64)> = (0..=20)
                .map(|i| {
                    let u = f64::from(i) / 20.0;
                    (
                        x0 + dir * 40.0 * u,
                        ph / 2.0 + 10.0 - 12.0 * (u * 3.2).sin(),
                    )
                })
                .collect();
            parts::art(b, plq, &[wave], 2.4, "#4d5a52");
        }
        // Leather corners.
        for (cx, cy) in [
            (0.0, top),
            (w - 64.0, top),
            (0.0, top + bh - 64.0),
            (w - 64.0, top + bh - 64.0),
        ] {
            let mut c = draw::rounded(draw::frame("Corner", cx, cy, 64.0, 64.0), 22.0);
            c.effects = vec![draw::drop_shadow("#00000080", 0.0, 2.0, 4.0)];
            let c = b.add(root, c);
            fill(
                b,
                c,
                (0.0, 0.0, 64.0, 64.0),
                22.0,
                &Material::Leather {
                    color: "#6a2f16".into(),
                },
            );
            for (rx, ry) in [(16.0, 16.0), (48.0, 16.0), (16.0, 48.0), (48.0, 48.0)] {
                b.add(
                    c,
                    draw::circle(
                        "rivet",
                        rx,
                        ry,
                        3.0,
                        draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#fff1c4"), (1.0, "#8a6630")]),
                    ),
                );
            }
        }
        parts::cable(
            b,
            root,
            (jx, jy),
            13.0,
            &[
                (jx + 2.0, jy + 180.0),
                (jx - 8.0, jy + 360.0),
                (jx - 50.0, h - 20.0),
                (jx - 150.0, h - 4.0),
            ],
        );
        handle(b, root, 430.0, 770.0, top + 4.0, 44.0, 16.0, "#7a3a1c");
    });
    let params = vec![
        parts::pick("ranch/mode", "Mode", &["Thick", "Normal", "Thicker"], 0),
        parts::dial10("ranch/gain", "Gain", 4.5),
        parts::dial10("ranch/bass", "Bass", 2.7),
        parts::dial10("ranch/mid", "Mid", 4.3),
        parts::dial10("ranch/treble", "Treble", 6.2),
        parts::dial10("ranch/presence", "Presence", 5.3),
        parts::dial10("ranch/master", "Master", 5.3),
        parts::dial10("ranch/output", "Output", 5.5),
        parts::switch("ranch/power", "Power", true),
    ];
    amp_face("06-ranch-head-1200x680", scene, params)
}

// ── Kintsugi ─────────────────────────────────────────────────────────────

/// A five-pointed maple leaf at (`cx`, `cy`), `r` across, turned `rot`.
fn leaf(cx: f64, cy: f64, r: f64, rot: f64) -> String {
    // Five broad lobes (the top three largest), each serrated, over a
    // notched base where the stem comes in.
    let pts: Vec<(f64, f64)> = (0..120)
        .map(|i| {
            let t = f64::from(i) / 120.0;
            let a = t * 360.0 - 180.0; // −180..180, 0 = up
            let lobe = (a.to_radians() * 2.5).cos().abs().powf(0.7);
            let size = if a.abs() > 150.0 {
                0.25
            } else if a.abs() > 100.0 {
                0.72
            } else {
                1.0
            };
            let serr = if i % 4 == 0 { 0.1 } else { 0.0 };
            draw::polar(cx, cy, r * size * (0.42 + 0.58 * lobe + serr), rot + a)
        })
        .collect();
    let mut d = draw::polygon(&pts);
    let stem = draw::polar(cx, cy, r * 0.7, rot + 180.0);
    d.push_str(&format!(
        " M {cx:.1} {cy:.1} L {:.1} {:.1} L {:.1} {:.1} Z",
        stem.0 + 1.0,
        stem.1,
        stem.0 - 1.0,
        stem.1
    ));
    d
}

/// A small deterministic noise source.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        f64::from(self.0) / f64::from(u32::MAX)
    }
}

/// A crack through `pts`, broken into short jagged runs.
fn jag(pts: &[(f64, f64)], n: &mut Noise, amp: f64) -> Vec<(f64, f64)> {
    let mut out = vec![pts[0]];
    for p in pts.windows(2) {
        let ((ax, ay), (bx, by)) = (p[0], p[1]);
        let len = (bx - ax).hypot(by - ay);
        let steps = (len / 14.0).ceil().max(1.0) as usize;
        let (nx, ny) = (-(by - ay) / len.max(1e-6), (bx - ax) / len.max(1e-6));
        for s in 1..=steps {
            let u = s as f64 / steps as f64;
            let off = if s == steps {
                0.0
            } else {
                (n.next() - 0.5) * 2.0 * amp
            };
            out.push((ax + (bx - ax) * u + nx * off, ay + (by - ay) * u + ny * off));
        }
    }
    out
}

/// A tapered stroke (a brow) along `spine`, `w0` wide at its start and
/// `w1` at its end, as a filled outline.
fn tapered(spine: &[(f64, f64)], w0: f64, w1: f64) -> String {
    let n = spine.len();
    let norm = |i: usize| {
        let (a, c) = (spine[i.saturating_sub(1)], spine[(i + 1).min(n - 1)]);
        let (dx, dy) = (c.0 - a.0, c.1 - a.1);
        let l = dx.hypot(dy).max(1e-6);
        (-dy / l, dx / l)
    };
    let half = |i: usize| (w0 + (w1 - w0) * i as f64 / (n - 1) as f64) / 2.0;
    let mut pts: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            (
                spine[i].0 + norm(i).0 * half(i),
                spine[i].1 + norm(i).1 * half(i),
            )
        })
        .collect();
    pts.extend((0..n).rev().map(|i| {
        (
            spine[i].0 - norm(i).0 * half(i),
            spine[i].1 - norm(i).1 * half(i),
        )
    }));
    draw::polygon(&pts)
}

/// An ellipse filling `(x, y, w, h)`.
fn oval(
    name: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    paint: frame::document::proto::Paint,
) -> frame::document::proto::Node {
    let mut n = draw::circle(name, x + w / 2.0, y + w / 2.0, w / 2.0, paint);
    n.size.height = h;
    n.transform.ty = y;
    n
}

/// A quadratic curve from `a` through control `c` to `z`, `n` points.
fn curve(a: (f64, f64), c: (f64, f64), z: (f64, f64), n: usize) -> Vec<(f64, f64)> {
    (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            (
                u * u * a.0 + 2.0 * u * t * c.0 + t * t * z.0,
                u * u * a.1 + 2.0 * u * t * c.1 + t * t * z.1,
            )
        })
        .collect()
}

/// One closed eye: the lid's line from `inner` to `outer` sagging to
/// `sag`, the lashes fanning down from it (longer toward the outer
/// corner), a blue wash of shadow over the lid, a warm glint at the inner
/// corner.
fn eye(
    b: &mut Builder<'_>,
    art: NodeId,
    (w, h): (f64, f64),
    inner: (f64, f64),
    outer: (f64, f64),
    sag: (f64, f64),
) {
    let (mx, my) = ((inner.0 + outer.0) / 2.0, (inner.1 + outer.1) / 2.0);
    let span = (outer.0 - inner.0).abs();
    b.add(
        art,
        oval(
            "eyeshadow",
            mx - span * 0.62,
            my - span * 0.42,
            span * 1.24,
            span * 0.62,
            draw::radial(&[(0.0, "#6d78b0a0"), (0.55, "#7d86b855"), (1.0, "#8a90c000")]),
        ),
    );
    // The lid's crease above it, faint.
    let crease = curve(
        (inner.0, inner.1 - span * 0.1),
        (sag.0, sag.1 - span * 0.32),
        (outer.0, outer.1 - span * 0.12),
        24,
    );
    b.add(
        art,
        ui::strokes("crease", &[crease], 1.6, draw::solid("#7a6f8060")),
    );
    let lid = curve(inner, sag, outer, 30);
    b.add(
        art,
        draw::path(
            "lid",
            0.0,
            0.0,
            w,
            h,
            tapered(&lid, 3.0, 6.5),
            draw::solid("#1f1c2a"),
        ),
    );
    let lashes: Vec<Vec<(f64, f64)>> = lid
        .iter()
        .enumerate()
        .skip(3)
        .map(|(i, (x, y))| {
            let u = i as f64 / 30.0;
            let len = 8.0 + 16.0 * u.powf(1.6);
            let lean = (outer.0 - inner.0).signum() * (4.0 + 10.0 * u);
            vec![
                (*x, *y),
                (x + lean * 0.5, y + len * 0.6),
                (x + lean, y + len),
            ]
        })
        .collect();
    b.add(
        art,
        ui::strokes("lashes", &lashes, 1.5, draw::solid("#1f1c2a")),
    );
    b.add(
        art,
        oval(
            "glint",
            inner.0 - 7.0,
            inner.1 - 1.0,
            14.0,
            6.0,
            draw::radial(&[(0.0, "#e0703acc"), (1.0, "#e0703a00")]),
        ),
    );
}

#[allow(clippy::too_many_lines)]
fn kintsugi_art(b: &mut Builder<'_>, art: NodeId, w: f64, h: f64) {
    let at = |u: f64, v: f64| (w * u, h * v);
    // Porcelain skin, lit from the upper left, cooler toward the hair.
    b.add(
        art,
        draw::rect(
            "skin",
            0.0,
            0.0,
            w,
            h,
            draw::radial_at(
                0.47,
                0.42,
                0.62,
                &[
                    (0.0, "#f4efec"),
                    (0.45, "#e6e0de"),
                    (0.75, "#b9b4b8"),
                    (1.0, "#6a676d"),
                ],
            ),
        ),
    );
    // A soft shade down the nose and under the brows.
    b.add(
        art,
        oval(
            "nose shade",
            w * 0.4,
            h * 0.35,
            w * 0.2,
            h * 0.9,
            draw::radial(&[(0.0, "#ffffff26"), (0.5, "#ffffff10"), (1.0, "#ffffff00")]),
        ),
    );
    // Dark hair falling either side of the face, in strands.
    let mut n = Noise(0x9e37_79b9);
    for (x0, x1, flip) in [(0.0, 0.2, false), (0.81, 1.0, true)] {
        let edge: Vec<(f64, f64)> = (0..=24)
            .map(|i| {
                let v = f64::from(i) / 24.0;
                let bulge = (v * std::f64::consts::PI).sin() * 0.05;
                let x = if flip {
                    x0 - bulge + 0.02 * v
                } else {
                    x1 + bulge - 0.03 * v
                };
                (w * x, h * v)
            })
            .collect();
        let mut poly = if flip {
            vec![(w, 0.0)]
        } else {
            vec![(0.0, 0.0)]
        };
        poly.extend(edge.iter().copied());
        poly.push(if flip { (w, h) } else { (0.0, h) });
        let hair = draw::path(
            "hair",
            0.0,
            0.0,
            w,
            h,
            draw::polygon(&poly),
            draw::linear(
                if flip { 270.0 } else { 90.0 },
                &[(0.0, "#0c0b0d"), (0.7, "#1e1b1f"), (1.0, "#39343a")],
            ),
        );
        b.add(
            art,
            draw::with_effects(
                hair,
                vec![draw::drop_shadow(
                    "#00000080",
                    if flip { -8.0 } else { 8.0 },
                    0.0,
                    18.0,
                )],
            ),
        );
        let strands: Vec<Vec<(f64, f64)>> = (0..40)
            .map(|i| {
                let u = x0 + (x1 - x0) * (f64::from(i) + n.next()) / 40.0;
                let drift = (n.next() - 0.5) * 0.08;
                curve(
                    (w * u, -10.0),
                    (w * (u + drift), h * 0.5),
                    (
                        w * (u + drift * 2.0 + if flip { 0.03 } else { -0.03 }),
                        h + 10.0,
                    ),
                    16,
                )
            })
            .collect();
        b.add(
            art,
            ui::strokes("strands", &strands, 1.1, draw::solid("#ffffff16")),
        );
    }
    // The brows: tapered strokes, thick at the inner end.
    b.add(
        art,
        draw::path(
            "brow",
            0.0,
            0.0,
            w,
            h,
            tapered(
                &curve(at(0.4, 0.34), at(0.31, 0.2), at(0.22, 0.34), 30),
                19.0,
                4.0,
            ),
            draw::linear(90.0, &[(0.0, "#232233"), (1.0, "#3c3b52")]),
        ),
    );
    b.add(
        art,
        draw::path(
            "brow",
            0.0,
            0.0,
            w,
            h,
            tapered(
                &curve(at(0.56, 0.17), at(0.67, 0.02), at(0.78, 0.13), 30),
                19.0,
                4.0,
            ),
            draw::linear(90.0, &[(0.0, "#232233"), (1.0, "#3c3b52")]),
        ),
    );
    // The closed eyes, the face turned a little: the right eye higher.
    eye(
        b,
        art,
        (w, h),
        at(0.42, 0.6),
        at(0.25, 0.57),
        at(0.33, 0.71),
    );
    eye(
        b,
        art,
        (w, h),
        at(0.575, 0.47),
        at(0.745, 0.41),
        at(0.665, 0.57),
    );
    // The gold seams: long breaks across the face, branching.
    let mut n = Noise(0x2545_f491);
    let mains: Vec<Vec<(f64, f64)>> = vec![
        vec![
            at(0.3, 0.0),
            at(0.34, 0.2),
            at(0.3, 0.4),
            at(0.36, 0.55),
            at(0.33, 0.8),
            at(0.4, 1.0),
        ],
        vec![at(0.34, 0.2), at(0.46, 0.18), at(0.52, 0.0)],
        vec![
            at(0.46, 0.18),
            at(0.5, 0.42),
            at(0.6, 0.5),
            at(0.64, 0.8),
            at(0.6, 1.0),
        ],
        vec![at(0.5, 0.42), at(0.41, 0.5), at(0.36, 0.55)],
        vec![at(0.6, 0.5), at(0.72, 0.3), at(0.86, 0.36), at(1.0, 0.33)],
        vec![at(0.0, 0.3), at(0.1, 0.27), at(0.19, 0.12), at(0.24, 0.0)],
        vec![at(0.1, 0.27), at(0.18, 0.46), at(0.3, 0.4)],
        vec![at(0.64, 0.8), at(0.78, 0.9), at(0.86, 1.0)],
        vec![at(0.57, 0.0), at(0.6, 0.13), at(0.7, 0.1), at(0.75, 0.0)],
        vec![at(0.72, 0.3), at(0.69, 0.18), at(0.6, 0.13)],
        vec![at(0.18, 0.46), at(0.12, 0.7), at(0.2, 1.0)],
        vec![at(0.86, 0.36), at(0.83, 0.6), at(0.9, 0.78), at(1.0, 0.8)],
        vec![at(0.33, 0.8), at(0.22, 0.86), at(0.14, 1.0)],
        vec![at(0.5, 0.42), at(0.54, 0.62), at(0.49, 0.83), at(0.52, 1.0)],
    ];
    let cracks: Vec<Vec<(f64, f64)>> = mains.iter().map(|l| jag(l, &mut n, 4.0)).collect();
    // Short hairline breaks off the main seams.
    let mut twigs = Vec::new();
    for c in &cracks {
        for p in c.iter().step_by(5).skip(1) {
            if n.next() < 0.45 {
                let a = n.next() * 360.0;
                let len = 10.0 + n.next() * 24.0;
                twigs.push(jag(&[*p, draw::polar(p.0, p.1, len, a)], &mut n, 2.0));
            }
        }
    }
    let shade = |ls: &[Vec<(f64, f64)>]| {
        ls.iter()
            .map(|l| l.iter().map(|(x, y)| (x + 1.2, y + 1.6)).collect())
            .collect::<Vec<Vec<(f64, f64)>>>()
    };
    b.add(
        art,
        ui::strokes("seam shade", &shade(&cracks), 4.2, draw::solid("#00000050")),
    );
    b.add(
        art,
        ui::strokes("twig shade", &shade(&twigs), 2.0, draw::solid("#00000040")),
    );
    let gold = draw::solid("#dcb257");
    let seams = ui::strokes("seams", &cracks, 3.2, gold.clone());
    b.add(art, seams);
    b.add(art, ui::strokes("twigs", &twigs, 1.6, gold));
    b.add(
        art,
        ui::strokes(
            "seam light",
            &cracks
                .iter()
                .map(|l| l.iter().map(|(x, y)| (x - 0.6, y - 0.6)).collect())
                .collect::<Vec<_>>(),
            0.9,
            draw::solid("#fff6d0c0"),
        ),
    );
    // Gold leaves drifting over the hair, flakes of leaf along the seams.
    let leaves = [
        (60.0, 60.0, 34.0, 20.0),
        (150.0, 150.0, 26.0, -30.0),
        (70.0, 250.0, 30.0, 50.0),
        (190.0, 300.0, 20.0, 10.0),
        (40.0, 330.0, 16.0, -25.0),
        (220.0, 40.0, 14.0, 40.0),
        (w - 150.0, 50.0, 36.0, -15.0),
        (w - 60.0, 150.0, 28.0, 35.0),
        (w - 200.0, 300.0, 30.0, -40.0),
        (w - 70.0, 320.0, 20.0, 5.0),
        (w - 230.0, 170.0, 16.0, 60.0),
        (w - 110.0, 240.0, 12.0, -60.0),
        (w * 0.58, h * 0.72, 16.0, 25.0),
        (w * 0.44, h * 0.9, 12.0, -20.0),
        (w * 0.27, h * 0.14, 10.0, 15.0),
    ];
    let mut d = String::new();
    for (x, y, r, rot) in leaves {
        d.push_str(&leaf(x, y, r, rot));
        d.push(' ');
    }
    let mut flakes = String::new();
    for c in &cracks {
        for p in c.iter().step_by(7) {
            if n.next() < 0.35 {
                let s = 2.0 + n.next() * 4.0;
                let pts: Vec<(f64, f64)> = (0..5)
                    .map(|k| {
                        draw::polar(
                            p.0 + (n.next() - 0.5) * 16.0,
                            p.1 + (n.next() - 0.5) * 16.0,
                            s * (0.6 + n.next() * 0.6),
                            72.0 * f64::from(k) + n.next() * 30.0,
                        )
                    })
                    .collect();
                flakes.push_str(&draw::polygon(&pts));
                flakes.push(' ');
            }
        }
    }
    let leaf_gold = draw::linear(
        140.0,
        &[(0.0, "#fbe7a8"), (0.5, "#cfa24a"), (1.0, "#f2d88a")],
    );
    b.add(
        art,
        draw::with_effects(
            draw::path("leaves", 0.0, 0.0, w, h, d, leaf_gold.clone()),
            vec![draw::drop_shadow("#00000066", 1.0, 2.0, 2.0)],
        ),
    );
    b.add(art, draw::path("flakes", 0.0, 0.0, w, h, flakes, leaf_gold));
    // The panel's glaze.
    b.add(
        art,
        draw::rect(
            "glaze",
            0.0,
            0.0,
            w,
            h,
            draw::linear(
                160.0,
                &[(0.0, "#ffffff1c"), (0.3, "#ffffff00"), (1.0, "#00000026")],
            ),
        ),
    );
}

pub(super) fn kintsugi() -> Face {
    const INK: &str = "#2a2418";
    let (w, h) = (HEAD_W, 650.0);
    let top = 58.0;
    let bh = 575.0;
    let scene = canvas("Kintsugi Head", |b, root| {
        parts::page(b, root, w, h);
        for x in [150.0, 1050.0] {
            b.add(
                root,
                draw::rounded(
                    draw::rect(
                        "foot",
                        x - 50.0,
                        top + bh - 10.0,
                        100.0,
                        h - top - bh + 10.0,
                        draw::linear(180.0, &[(0.0, "#2a2a2c"), (1.0, "#0e0e10")]),
                    ),
                    10.0,
                ),
            );
        }
        let body = covered(
            b,
            root,
            (0.0, top, w, bh),
            34.0,
            &Material::Tolex {
                grain: Grain::Fine,
                color: "#efe9de".into(),
                scale: 1.0,
            },
        );
        let (ax, ay, aw, ah) = (41.0, 38.0, w - 82.0, 368.0);
        let mut art = look::slab("Art", (ax, ay, aw, ah), 10.0, vec![draw::solid("#1c1b1d")]);
        art.clips_content = true;
        art.effects = vec![draw::drop_shadow("#ffffff80", 0.0, 1.5, 0.0)];
        let art = b.add(body, art);
        kintsugi_art(b, art, aw, ah);
        for n in frame::surface::kit::materials::light::edge(
            "art recess",
            (0.0, 0.0, aw, ah),
            10.0,
            6.0,
            1.0,
            false,
        ) {
            b.add(art, n);
        }
        // The gold control strip: brushed brass.
        let (sx, sy, sw, sh) = (25.0, 439.0, w - 50.0, 108.0);
        let mut strip = draw::rounded(draw::frame("Strip", sx, sy, sw, sh), 8.0);
        strip.effects = vec![draw::drop_shadow("#00000080", 0.0, 3.0, 6.0)];
        strip.stroke = frame::document::proto::Stroke {
            paints: vec![draw::solid("#7a5a26")],
            weight: 2.0,
            ..frame::document::proto::Stroke::default()
        };
        let strip = b.add(body, strip);
        fill(
            b,
            strip,
            (0.0, 0.0, sw, sh),
            8.0,
            &Material::Brushed {
                color: "#d2ae62".into(),
                dir: Dir::Horizontal,
            },
        );
        for n in frame::surface::kit::materials::light::edge(
            "strip edge",
            (0.0, 0.0, sw, sh),
            8.0,
            4.0,
            1.0,
            true,
        ) {
            b.add(strip, n);
        }
        for (x, y) in [
            (14.0, 14.0),
            (sw - 14.0, 14.0),
            (14.0, sh - 14.0),
            (sw - 14.0, sh - 14.0),
        ] {
            parts::screw(b, strip, x, y, 5.0, "#7a5a26");
        }
        let cy = 53.0;
        let t = Type::new(9.5, INK, 600.0).track(0.12).upper();
        look::brush_mark(b, strip, (80.0, cy - 2.0), 22.0, INK);
        look::emblem(b, strip, (140.0, cy), 18.0, INK);
        parts::centred(b, strip, "Input", (218.0, 86.0), t);
        push(b, strip, "kintsugi/bright", 26.0, (306.0, cy), true);
        parts::centred(b, strip, "Bright", (306.0, 86.0), t);
        let knob = KnobLook {
            preset: "fluted",
            opts: &[
                "color=#3b2427",
                "pointer=#e8d6bc",
                "ribs=18",
                "rib_depth=0.045",
                "cap_r=0.92",
                "height=0.6",
                "dome=0.04",
                "edge=0.1",
                "index=line",
                "index_in=0.3",
                "index_out=0.85",
                "index_w=0.05",
                "coat=0.5",
                "rough=0.25",
                "ticks=dots",
                "fallback=fluted",
            ],
        };
        let knobs: [(&str, &str, f64); 7] = [
            ("kintsugi/gain", "Gain", 388.0),
            ("kintsugi/bass", "Bass", 475.0),
            ("kintsugi/middle", "Middle", 561.0),
            ("kintsugi/treble", "Treble", 648.0),
            ("kintsugi/master", "Master", 808.0),
            ("kintsugi/presence", "Presence", 894.0),
            ("kintsugi/output", "Output", 979.0),
        ];
        for (addr, legend, x) in knobs {
            look::place_knob(b, strip, &knob, addr, legend, 48.0, (x, cy - 4.0), INK);
            parts::centred(b, strip, legend, (x, 90.0), t);
        }
        push(b, strip, "kintsugi/shift", 26.0, (729.0, cy), true);
        parts::centred(b, strip, "Shift", (729.0, 86.0), t);
        push(b, strip, "kintsugi/power", 26.0, (1057.0, cy), true);
        parts::centred(b, strip, "Power", (1057.0, 86.0), t);
        let (jx, jy) = (sx + 218.0, top + sy + cy - 4.0);
        parts::cable(
            b,
            root,
            (jx, jy),
            12.0,
            &[
                (jx - 60.0, jy + 26.0),
                (jx - 140.0, jy + 70.0),
                (jx - 200.0, h - 6.0),
            ],
        );
        handle(b, root, 390.0, 810.0, top + 4.0, 46.0, 14.0, "#2a2224");
    });
    // Every knob at noon, as the reference screenshot shows them.
    let params = vec![
        parts::switch("kintsugi/bright", "Bright", false),
        parts::dial10("kintsugi/gain", "Gain", 5.0),
        parts::dial10("kintsugi/bass", "Bass", 5.0),
        parts::dial10("kintsugi/middle", "Middle", 5.0),
        parts::dial10("kintsugi/treble", "Treble", 5.0),
        parts::switch("kintsugi/shift", "Shift", false),
        parts::dial10("kintsugi/master", "Master", 5.0),
        parts::dial10("kintsugi/presence", "Presence", 5.0),
        parts::dial10("kintsugi/output", "Output", 5.0),
        parts::switch("kintsugi/power", "Power", true),
    ];
    amp_face("07-kintsugi-head-1200x650", scene, params)
}
