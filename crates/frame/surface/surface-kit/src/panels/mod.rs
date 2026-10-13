//! Kit family: panels — the faceplate everything else is mounted on.
//! Ported from `fts-audio-ui::hardware::{panel, rack}` and the paints of the
//! rack faces built on it (`comp-ui`/`eq-ui` `faces/units.rs`).
//!
//! A panel is a plain frame with the paint as its fill, the finish (brushed
//! grain) and edges (rack ears or walnut cheeks) as fixed children, and
//! **no control**. `place` returns the panel frame: put other components in
//! it at absolute positions (or give it an auto layout — the decoration is
//! taken out of flow).
//!
//! Sizing: `Place::size` is the width; the height defaults to a third of it
//! (the rack faces' 900 × 300). `Place::options` may override either and
//! pick the edges: `["w=900", "h=300", "ends=ears|wood|none"]`.

pub mod pedal;

use document_proto::{NodeId, TextAlign};
use surface_widgets::Builder;

use crate::controls::parts::{alpha, fixed, ink, lin};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

/// A panel paint: top-to-bottom sheen, silkscreen ink, chrome (ears and
/// screws), and brushed-grain strength (0 = painted steel).
struct Finish {
    stops: [&'static str; 3],
    angle: f64,
    ink: &'static str,
    chrome: &'static str,
    grain: f64,
}

const fn finish(
    stops: [&'static str; 3],
    ink: &'static str,
    chrome: &'static str,
    grain: f64,
) -> Finish {
    Finish {
        stops,
        angle: 178.0,
        ink,
        chrome,
        grain,
    }
}

fn brushed_finish(v: Option<&str>) -> Finish {
    match v {
        // Black anodising: the same grain, a third as visible on it.
        Some("black") => finish(
            ["#2b2b2f", "#1a1a1e", "#101013"],
            "#ded7c9",
            "#8d8a84",
            0.30,
        ),
        // The 1176 LN's plum: mid-toned, between natural and black.
        Some("plum") => finish(
            ["#3b2c3d", "#2b202d", "#1c141e"],
            "#ece2ec",
            "#9b8c9d",
            0.65,
        ),
        Some("gold") => finish(["#d8c49a", "#c4ad7e", "#a8915f"], "#2a2010", "#c4ad7e", 0.8),
        _ => finish(["#c9cac8", "#b6b7b5", "#9d9e9c"], "#1e1f21", "#b6b7b5", 1.0),
    }
}

fn painted_finish(v: Option<&str>) -> Finish {
    match v {
        Some("blue") => finish(["#2e5f86", "#1f4665", "#16334a"], "#eef4fa", "#9fb4c6", 0.0),
        Some("pultec") => Finish {
            angle: 176.0,
            ..finish(["#5b7e94", "#4e7188", "#3f5f74"], "#e8eef2", "#b9c6cf", 0.0)
        },
        Some("black") => finish(["#232426", "#191a1c", "#0f1012"], "#eceef0", "#a9adb2", 0.0),
        Some("charcoal") => finish(["#4a4d52", "#34373b", "#26282c"], "#e6e8ea", "#a9adb2", 0.0),
        Some("api") => finish(["#b9bcc0", "#a0a4a9", "#86898e"], "#23262a", "#7e8288", 0.0),
        Some("neve") => finish(["#2c3139", "#232830", "#191d24"], "#eef1f4", "#aeb4ba", 0.0),
        Some("cream") => finish(["#e7dcc4", "#d9cdb2", "#c3b697"], "#2b2620", "#b9b4a8", 0.0),
        Some("green") => finish(["#1f3b30", "#172d25", "#0e1e18"], "#e8eee8", "#a9b2ac", 0.0),
        _ => finish(["#cfd0cd", "#bcbdba", "#a7a8a5"], "#2e2f2d", "#b9bab6", 0.0),
    }
}

/// `key=value` from `Place::options`.
fn opt<'a>(p: &'a Place, key: &str) -> Option<&'a str> {
    p.options
        .as_ref()?
        .iter()
        .find_map(|o| o.strip_prefix(key)?.strip_prefix('='))
}

fn dims(p: &Place, default_w: f64, aspect: f64) -> (f64, f64) {
    let w = opt(p, "w")
        .and_then(|v| v.parse().ok())
        .or(p.size)
        .unwrap_or(default_w);
    let h = opt(p, "h")
        .and_then(|v| v.parse().ok())
        .unwrap_or(w * aspect);
    (w, h)
}

/// A small deterministic generator, so the grain is the same every build.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as f64 / f64::from(u32::MAX >> 1)
    }
}

/// Horizontal lines `[on, on + t)` every `period` px down `h`, as one path.
fn lines(w: f64, h: f64, period: f64, offset: f64, t: f64) -> String {
    let mut d = String::new();
    let mut y = offset;
    while y < h {
        d.push_str(&format!(
            "M 0 {y:.2} H {w:.2} V {:.2} H 0 Z ",
            (y + t).min(h)
        ));
        y += period;
    }
    d
}

/// Brushed aluminium: fine horizontal scratches at non-commensurate periods
/// (2 / 3 / 5 / 9 / 23 px) so they never line up the same way twice, some
/// longer smears, and the broad off-axis sheen that makes it read as metal
/// rather than grey card. All horizontal — brushing has one direction.
fn grain(b: &mut Builder<'_>, panel: NodeId, w: f64, h: f64, strength: f64) {
    let a = |v: f64| v * strength;
    let layers: [(&str, f64, f64, f64, f64); 6] = [
        ("#ffffff", 0.055, 2.0, 0.0, 1.0),
        ("#000000", 0.045, 2.0, 1.0, 1.0),
        ("#ffffff", 0.032, 3.0, 0.0, 1.0),
        ("#000000", 0.040, 5.0, 0.0, 1.0),
        ("#ffffff", 0.045, 9.0, 0.0, 2.0),
        ("#000000", 0.055, 23.0, 0.0, 3.0),
    ];
    for (c, v, period, off, t) in layers {
        b.add(
            panel,
            fixed(draw::path(
                "grain",
                0.0,
                0.0,
                w,
                h,
                lines(w, h, period, off, t),
                draw::solid(&alpha(c, a(v))),
            )),
        );
    }
    b.add(
        panel,
        fixed(draw::path(
            "grain",
            0.0,
            0.0,
            w,
            h,
            lines(w, h, 23.0, 3.0, 3.0),
            draw::solid(&alpha("#ffffff", a(0.03))),
        )),
    );
    // Smears: longer streaks of slightly lighter and darker metal.
    let mut rng = Lcg(0x5eed_1176);
    for (c, v) in [("#ffffff", 0.07), ("#000000", 0.06)] {
        let mut d = String::new();
        for _ in 0..((h / 6.0) as usize).max(4) {
            let y = rng.next() * h;
            let x = rng.next() * w * 0.9 - w * 0.1;
            let len = w * (0.15 + rng.next() * 0.5);
            let t = 0.6 + rng.next() * 1.2;
            d.push_str(&format!(
                "M {x:.2} {y:.2} h {len:.2} v {t:.2} h {:.2} Z ",
                -len
            ));
        }
        b.add(
            panel,
            fixed(draw::path(
                "smear",
                0.0,
                0.0,
                w,
                h,
                d,
                draw::solid(&alpha(c, a(v))),
            )),
        );
    }
    let sheen = lin(
        102.0,
        &[
            (0.0, "#ffffff00".into()),
            (0.22, alpha("#ffffff", a(0.10))),
            (0.41, "#ffffff00".into()),
            (0.63, alpha("#000000", a(0.06))),
            (0.84, alpha("#ffffff", a(0.07))),
            (1.0, "#ffffff00".into()),
        ],
    );
    b.add(panel, fixed(draw::rect("sheen", 0.0, 0.0, w, h, sheen)));
}

/// A panel screw — a slotted head, small enough to be texture.
fn screw(b: &mut Builder<'_>, parent: NodeId, cx: f64, cy: f64, d: f64, chrome: &str, k: f64) {
    let head = draw::circle(
        "screw",
        cx,
        cy,
        d / 2.0,
        draw::radial_at(0.35, 0.30, 0.8, &[(0.0, chrome), (1.0, "#000000b3")]),
    );
    b.add(
        parent,
        fixed(draw::with_effects(
            head,
            vec![draw::inner_shadow("#00000099", 0.0, 0.0, 2.0 * k)],
        )),
    );
    b.add(
        parent,
        fixed(draw::rotated(
            draw::rect(
                "slot",
                cx - d * 0.31,
                cy - 0.6 * k,
                d * 0.62,
                1.2 * k,
                draw::solid("#0000008c"),
            ),
            25.0,
        )),
    );
}

/// One screwed rack ear. Tall panels get a third screw, like the 2U units.
fn rack_ear(b: &mut Builder<'_>, panel: NodeId, left: bool, w: f64, h: f64, k: f64, chrome: &str) {
    let ew = 26.0 * k;
    let x = if left { 0.0 } else { w - ew };
    let ear = draw::rect(
        "ear",
        x,
        0.0,
        ew,
        h,
        draw::linear(90.0, &[(0.0, "#ffffff1a"), (1.0, "#00000029")]),
    );
    b.add(panel, fixed(ear));
    let seam = if left { x + ew - k } else { x };
    b.add(
        panel,
        fixed(draw::rect(
            "seam",
            seam,
            0.0,
            k.max(1.0),
            h,
            draw::solid("#00000047"),
        )),
    );
    let d = 9.0 * k;
    let pad = 10.0 * k + d / 2.0;
    let mut ys = vec![pad, h - pad];
    if h / w > 0.3 {
        ys.insert(1, h / 2.0);
    }
    for y in ys {
        screw(b, panel, x + ew / 2.0, y, d, chrome, k);
    }
}

/// A walnut side cheek — a desktop unit of the dbx 160's generation.
fn wood_cheek(b: &mut Builder<'_>, panel: NodeId, left: bool, w: f64, h: f64, k: f64) {
    let cw = 34.0 * k;
    let x = if left { 0.0 } else { w - cw };
    let wood = draw::rect(
        "cheek",
        x,
        0.0,
        cw,
        h,
        draw::linear(
            90.0,
            &[
                (0.0, "#6b3f22"),
                (0.26, "#8a5530"),
                (0.58, "#7a4a28"),
                (1.0, "#5e3419"),
            ],
        ),
    );
    b.add(
        panel,
        fixed(draw::with_effects(
            wood,
            vec![draw::inner_shadow(
                "#00000059",
                if left { 2.0 * k } else { -2.0 * k },
                0.0,
                4.0 * k,
            )],
        )),
    );
    // Grain: darker streaks down the cheek, not quite regular.
    let mut rng = Lcg(0x0a1c_160d);
    let mut d = String::new();
    let mut gx = x + 1.0;
    while gx < x + cw - 1.0 {
        let t = 0.6 + rng.next() * 0.9;
        d.push_str(&format!("M {gx:.2} 0 h {t:.2} V {h:.2} h {:.2} Z ", -t));
        gx += 3.0 + rng.next() * 6.0;
    }
    b.add(
        panel,
        fixed(draw::path(
            "grain",
            0.0,
            0.0,
            w,
            h,
            d,
            draw::solid("#00000029"),
        )),
    );
    let seam = if left { x + cw - 1.0 } else { x };
    b.add(
        panel,
        fixed(draw::rect(
            "seam",
            seam,
            0.0,
            1.0,
            h,
            draw::solid("#00000080"),
        )),
    );
}

fn panel(b: &mut Builder<'_>, parent: NodeId, p: &Place, f: &Finish, default_ends: &str) -> NodeId {
    let (w, h) = dims(p, 450.0, 1.0 / 3.0);
    let k = w / 900.0;
    let mut n = draw::frame(&p.name, 0.0, 0.0, w, h);
    n.fills = vec![draw::linear(
        f.angle,
        &[(0.0, f.stops[0]), (0.48, f.stops[1]), (1.0, f.stops[2])],
    )];
    n.clips_content = true;
    let n = draw::stroked(draw::rounded(n, 3.0 * k), "#0000008c", k.max(1.0));
    let n = draw::with_effects(
        n,
        vec![draw::drop_shadow("#00000080", 0.0, 4.0 * k, 18.0 * k)],
    );
    let root = b.add(parent, n);
    if f.grain > 0.0 {
        grain(b, root, w, h, f.grain);
    }
    let ends = opt(p, "ends").unwrap_or(default_ends);
    let edge = match ends {
        "wood" => {
            wood_cheek(b, root, true, w, h, k);
            wood_cheek(b, root, false, w, h, k);
            34.0 * k
        }
        "none" => 0.0,
        _ => {
            rack_ear(b, root, true, w, h, k, f.chrome);
            rack_ear(b, root, false, w, h, k, f.chrome);
            26.0 * k
        }
    };
    if let Some(legend) = p.legend.as_deref().filter(|l| !l.is_empty()) {
        let size = (26.0 * k).max(9.0);
        b.add(
            root,
            fixed(ink(
                "model",
                legend,
                edge + 18.0 * k,
                34.0 * k + size * 0.3,
                w - 2.0 * edge - 36.0 * k,
                size,
                f.ink,
                TextAlign::Left,
                800.0,
            )),
        );
    }
    root
}

fn brushed(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    panel(b, parent, p, &brushed_finish(p.variant.as_deref()), "ears")
}

fn painted(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    panel(b, parent, p, &painted_finish(p.variant.as_deref()), "ears")
}

fn desktop(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    panel(
        b,
        parent,
        p,
        &painted_finish(p.variant.as_deref().or(Some("black"))),
        "wood",
    )
}

/// A section of a panel: an outlined box (the Distressor's two frames), a
/// region in another paint (the 1176's blue meter band), or a recessed
/// plate. Also a parent for components.
fn section(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let (w, h) = dims(p, 200.0, 0.75);
    let mut n = draw::frame(&p.name, 0.0, 0.0, w, h);
    let r = 6.0;
    let n = match p.variant.as_deref() {
        Some("fet_blue") => {
            n.fills = vec![draw::linear(
                90.0,
                &[(0.0, "#164f65"), (0.5, "#1b5f79"), (1.0, "#164f65")],
            )];
            n
        }
        Some("inset") => {
            n.fills = vec![draw::linear(180.0, &[(0.0, "#0e0f11"), (1.0, "#1c1d20")])];
            draw::with_effects(
                draw::rounded(n, r),
                vec![
                    draw::inner_shadow("#000000b3", 0.0, 2.0, 6.0),
                    draw::inner_shadow("#ffffff14", 0.0, -1.0, 0.0),
                ],
            )
        }
        Some("outline_dark") => draw::stroked(draw::rounded(n, r), "#2b2620b0", 1.2),
        _ => draw::stroked(draw::rounded(n, r), "#e8eaec8c", 1.2),
    };
    let root = b.add(parent, n);
    if let Some(legend) = p.legend.as_deref().filter(|l| !l.is_empty()) {
        let color = if p.variant.as_deref() == Some("outline_dark") {
            "#2b2620"
        } else {
            "#e8eaec"
        };
        b.add(
            root,
            fixed(ink(
                "title",
                legend,
                0.0,
                12.0,
                w,
                9.0,
                color,
                TextAlign::Center,
                700.0,
            )),
        );
    }
    root
}

/// This family's components.
pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "panel.brushed",
            family: Family::Panel,
            title: "Brushed faceplate",
            description: "A brushed, anodised aluminium rack faceplate with screwed rack ears — the 1176's finishes (natural silver, blackface, the LN's plum). No control: place other components inside the returned frame. Size = width; height = width/3 unless `options` has \"h=…\" (also \"w=…\", \"ends=ears|wood|none\").",
            variants: &["natural", "black", "plum", "gold"],
            default_size: 450.0,
            binds: "none",
            build: brushed,
        },
        ComponentDef {
            id: "panel.painted",
            family: Family::Panel,
            title: "Painted faceplate",
            description: "A painted steel rack faceplate with screwed rack ears, in the paints of real units: LA-2A grey, CL 1B blue, Pultec blue-grey, Fairchild black, SSL charcoal, API grey, Neve 1073, cream, green. No control; a parent for other components. Size = width; height = width/3 unless `options` has \"h=…\" (also \"w=…\", \"ends=ears|wood|none\"). `legend` prints a model line top-left.",
            variants: &[
                "grey", "blue", "pultec", "black", "charcoal", "api", "neve", "cream", "green",
            ],
            default_size: 450.0,
            binds: "none",
            build: painted,
        },
        ComponentDef {
            id: "panel.desktop",
            family: Family::Panel,
            title: "Desktop unit (wood cheeks)",
            description: "A painted faceplate with walnut side cheeks instead of rack ears — a desktop unit, as the dbx 160 was sold. Same paints and sizing as `panel.painted` (default black). No control; a parent for other components.",
            variants: &[
                "black", "grey", "blue", "pultec", "charcoal", "api", "neve", "cream", "green",
            ],
            default_size: 450.0,
            binds: "none",
            build: desktop,
        },
        ComponentDef {
            id: "panel.section",
            family: Family::Panel,
            title: "Panel section",
            description: "A section on a faceplate: a hairline rounded outline (the Distressor's boxes; `outline` for dark panels, `outline_dark` for light), the 1176's blue meter band (`fet_blue`), or a recessed dark plate (`inset`). `legend` prints a small title. No control; a parent for components. Size = width; height = 3/4 width unless `options` has \"h=…\".",
            variants: &["outline", "outline_dark", "fet_blue", "inset"],
            default_size: 200.0,
            binds: "none",
            build: section,
        },
    ]
}
