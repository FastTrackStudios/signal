//! The unit the drawn delays (Lo-Fi, Shimmer, Reverse, Ice, Rhythm) share:
//! one panel in the machine's finish with its picture well at the left and
//! its controls at the right, laid out the way Analog and Room are — the
//! face at its tiers, its back, its tall face. A machine brings its finish
//! ([`Look`]), what is printed on its panel (`dress`) and its picture.
//!
//! The picture's echoes move the way every delay's do: the n-th repeat
//! sits n delay times after the note and falls by the feedback each time
//! round ([`slide`], [`fall`], [`fade`]).

use frame::document::proto::NodeId;
use frame::document::proto::motion::{Dynamics, Motion, Target};
use frame::surface::kit::{draw, motion};
use frame::surface::widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, DLY};

/// A finish: the panel, its picture well and its knobs, in one light.
pub struct Look {
    pub ink: &'static str,
    pub dim: &'static str,
    pub panel: [(f64, &'static str); 3],
    pub edge: &'static str,
    pub well: [(f64, &'static str); 2],
    pub knob: [&'static str; 6],
    pub wheel: [&'static str; 5],
    /// The nameplate's accent.
    pub accent: &'static str,
    /// The tall face's nameplate scrim, over the picture.
    pub scrim: &'static str,
    /// The light the picture gives off.
    pub light: &'static str,
    /// The picture's printing (its grid, its scale).
    pub faint: &'static str,
    /// Whether the panel is metal (brushed steel catching the light).
    pub metal: bool,
}

/// A drawn machine.
pub struct Device {
    pub name: &'static str,
    pub base: &'static str,
    /// Printed at the picture's top left, before the delay time.
    pub caption: &'static str,
    /// The nameplate's lettering: family, weight, size, tracking.
    pub title: (&'static str, f64, f64, f64),
    pub title_w: f64,
    /// The front's knobs (the tall face's first line).
    pub front: [(&'static str, &'static str); 4],
    /// The phone's three.
    pub phone: [(&'static str, &'static str); 3],
    pub look: fn(bool) -> Look,
    /// Draws on the panel (`w × h`) under everything else.
    pub dress: fn(&mut Builder<'_>, NodeId, (f64, f64), &Look),
    /// Draws the picture into its well (`w × h`).
    pub picture: fn(&mut Builder<'_>, NodeId, (f64, f64), &Look),
}

/// The tall face's rows: a shorter picture, big knobs under it.
const ROWS: kit::TallRows = kit::TallRows {
    split: 134.0,
    main: (30.0, 50.0),
    more: (126.0, 40.0),
};

fn a(id: &str) -> String {
    format!("{DLY}/{id}")
}

/// A spring for what follows a param.
#[must_use]
pub fn spring() -> Dynamics {
    Dynamics::spring(260.0, 28.0)
}

/// Slide `node` (drawn at its start) to the `i`-th repeat: `i` delay times
/// along a window of `window_ms`, `span` px.
pub fn slide(b: &mut Builder<'_>, node: NodeId, i: u32, window_ms: f64, span: f64) {
    let at = format!("clamp({i} * param({DLY}/time) / {window_ms}, 0, 1.2)");
    motion::add(
        b,
        node,
        Motion::new(Target::X)
            .input(at)
            .range(0.0, span)
            .dynamics(spring()),
    );
}

/// How loud the `i`-th repeat is drawn: the feedback `i - 1` times round,
/// on a display's log scale (half the exponent: a repeat 20 dB down
/// stands a tenth tall, not a hundredth).
#[must_use]
pub fn loudness(i: u32) -> String {
    format!(
        "pow(param({DLY}/feedback), {:.1}) * (0.3 + 0.7 * param({DLY}/on))",
        0.5 * f64::from(i.saturating_sub(1))
    )
}

/// The window a picture `span` px wide shows, ms: 2.5 s on a lane's
/// picture, less on a phone's, more on a tall face's.
#[must_use]
pub fn window(span: f64) -> f64 {
    (2500.0 * (span / 480.0).clamp(0.4, 1.5) / 500.0).round() * 500.0
}

/// `ms` as a scale's end ("2.5 s", "750 ms").
#[must_use]
pub fn ms_label(ms: f64) -> String {
    if ms >= 1000.0 {
        format!("{:.1} s", ms / 1000.0)
    } else {
        format!("{ms:.0} ms")
    }
}

/// Shrink `node` (about `pivot_y`, 1 its foot) by the feedback, `i`
/// times round.
pub fn fall(b: &mut Builder<'_>, node: NodeId, i: u32, pivot_y: f64) {
    let f = loudness(i);
    motion::add(
        b,
        node,
        Motion::new(Target::StretchY)
            .input(f)
            .range(0.02, 1.0)
            .pivot(0.5, pivot_y)
            .dynamics(spring()),
    );
}

/// Dim `node` by the feedback, `i` times round.
pub fn fade(b: &mut Builder<'_>, node: NodeId, i: u32) {
    let f = loudness(i);
    motion::add(
        b,
        node,
        Motion::new(Target::Opacity)
            .input(f)
            .range(0.08, 1.0)
            .dynamics(spring()),
    );
}

/// A path node `w × h` at (`x`, `y`) stroked `weight` in `color`.
#[must_use]
pub fn trace(
    name: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    d: String,
    color: &str,
    weight: f64,
) -> frame::document::proto::Node {
    draw::stroked(
        draw::path(name, x, y, w, h, d, draw::solid("#00000000")),
        color,
        weight,
    )
}

/// `pts` as path data.
#[must_use]
pub fn poly(pts: &[(f64, f64)]) -> String {
    let mut d = String::new();
    for (i, (x, y)) in pts.iter().enumerate() {
        d.push_str(&format!(
            "{} {x:.2} {y:.2} ",
            if i == 0 { "M" } else { "L" }
        ));
    }
    d
}

/// `pts` closed, as path data.
#[must_use]
pub fn closed(pts: &[(f64, f64)]) -> String {
    let mut d = poly(pts);
    d.push('Z');
    d
}

fn ground(b: &mut Builder<'_>, root: NodeId, d: &Device, (w, h): (f64, f64), l: &Look) -> NodeId {
    let mut panel = draw::frame("Panel", 0.0, 0.0, w, h);
    panel.fills = vec![draw::linear(180.0, &l.panel)];
    panel.corner_radii = frame::document::proto::CornerRadii::uniform(10.0);
    let panel = b.add(
        root,
        draw::with_effects(
            draw::stroked(panel, l.edge, 1.0),
            vec![
                draw::inner_shadow("#ffffff22", 0.0, 1.0, 0.0),
                draw::inner_shadow("#00000050", 0.0, -2.0, 3.0),
            ],
        ),
    );
    if l.metal {
        // The metal is a sheet of its own under everything on it: the
        // effect lights the sheet, not the picture and knobs.
        let mut sheet = draw::frame("Metal", 0.0, 0.0, w, h);
        sheet.fills = vec![draw::linear(180.0, &l.panel)];
        sheet.corner_radii = frame::document::proto::CornerRadii::uniform(10.0);
        let sheet = b.add(panel, sheet);
        kit::steel(b, sheet);
    }
    (d.dress)(b, panel, (w, h), l);
    panel
}

/// The picture's well at (`x`, `y`, `w`, `h`): sunk in the panel, the
/// machine's picture in it, its caption and the delay time at its top
/// left, glass over it.
fn well(
    b: &mut Builder<'_>,
    panel: NodeId,
    d: &Device,
    (x, y, w, h): (f64, f64, f64, f64),
    l: &Look,
) -> NodeId {
    let mut v = draw::frame("Picture", x, y, w, h);
    v.fills = vec![draw::linear(180.0, &l.well)];
    v.clips_content = true;
    v.corner_radii = frame::document::proto::CornerRadii::uniform(7.0);
    v.effects = vec![draw::inner_shadow("#000000b0", 0.0, 2.0, 6.0)];
    let v = b.add(panel, v);
    (d.picture)(b, v, (w, h), l);
    let size = 6.0;
    ui::silk_left(
        b,
        v,
        d.caption,
        10.0,
        11.0,
        size,
        &ui::alpha(l.light, 0.75),
        800.0,
    );
    let cw = draw::text_width(d.caption, size) * 1.15 + 14.0;
    ui::lit_readout(
        b,
        v,
        "Delay time",
        ui::bind(&a("time")),
        10.0 + cw,
        11.0,
        70.0,
        7.0,
        l.light,
        frame::document::proto::TextAlign::Left,
    );
    kit::glass(b, v);
    v
}

fn title(b: &mut Builder<'_>, t: NodeId, d: &Device, h: f64, scale: f64, ink: &str) {
    let (family, weight, size, track) = d.title;
    let l = draw::Lettering::family(family).weight(weight);
    crate::suite_studio_blues::parts::set(
        b,
        t,
        d.name,
        d.title_w / 2.0 - 7.0,
        h / 2.0,
        size * scale,
        ink,
        &l,
        track,
    );
}

/// The face at `tier`.
#[must_use]
pub fn face_at(d: &Device, dark: bool, tier: kit::Tier) -> Face {
    let l = (d.look)(dark);
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane(d.name, tier, |b, root| {
        let panel = ground(b, root, d, (w, kit::H), &l);
        let vw = if small_room {
            kit::PHONE_PICTURE_R - 14.0
        } else {
            540.0
        };
        let view = well(b, panel, d, (14.0, 14.0, vw, kit::H - 28.0), &l);
        let lit = ui::alpha(kit::BLUE, 0.5);
        if small_room {
            let (tx, ty, td) = kit::PHONE_TIME;
            kit::time_knob(
                b,
                panel,
                DLY,
                (tx, ty, td),
                11.0,
                (l.ink, l.dim, &lit),
                |b, p, addr| {
                    kit::knob3d_bare(b, p, addr, td, (tx, ty), &l.wheel);
                },
            );
            let (xs, ky, kd) = kit::PHONE_KNOBS;
            for (i, (id, legend)) in d.phone.iter().enumerate() {
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
            let x0 = 14.0 + vw + 24.0;
            let (tx, ty, td) = (x0 + 30.0, 76.0, 46.0);
            kit::time_knob(
                b,
                panel,
                DLY,
                (tx, ty, td),
                8.5,
                (l.ink, l.dim, &lit),
                |b, p, addr| {
                    kit::knob3d_bare(b, p, addr, td, (tx, ty), &l.wheel);
                },
            );
            for (i, (id, legend)) in d.front.iter().enumerate() {
                kit::knob3d(
                    b,
                    panel,
                    &a(id),
                    legend,
                    34.0,
                    (x0 + 100.0 + 60.0 * i as f64, 84.0),
                    &l.knob,
                    l.ink,
                    l.dim,
                );
            }
        }
        let (np_h, preset_w, size, scale) = if small_room {
            (30.0, 200.0, 12.0, 1.1)
        } else {
            (28.0, 190.0, 10.0, 1.0)
        };
        kit::nameplate_top_right(
            b,
            panel,
            DLY,
            tier,
            (8.0, np_h),
            d.title_w,
            preset_w,
            size,
            (l.accent, l.dim),
            |b, t| title(b, t, d, np_h, scale, l.ink),
        );
        kit::face_bypass(b, panel, DLY, tier, "blue", "On");
        kit::flip_arrow(b, panel, DLY, tier, false, (l.ink, &ui::alpha(l.dim, 0.3)));
        kit::responsive(b, panel, &[view]);
    });
    let mut f = kit::delay_face(kit::tier_file(d.base, tier, dark), scene);
    f.params.extend(kit::nameplate_params(DLY));
    f.params.extend(kit::time_params(DLY));
    f.params.extend(kit::flip_params(DLY));
    f
}

/// Its back at `tier`: the panel, no picture — the advanced controls the
/// front has not got across it, the nameplate, the bypass and the arrow
/// where they are on the front.
#[must_use]
pub fn back_at(d: &Device, dark: bool, tier: kit::Tier) -> Face {
    let l = (d.look)(dark);
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let base: &'static str = Box::leak(format!("{}-back", d.base).into_boxed_str());
    let scene = kit::tier_lane(d.name, tier, |b, root| {
        let panel = ground(b, root, d, (w, kit::H), &l);
        let knobs = kit::tall_more(DLY, &d.front);
        kit::back_knobs(b, panel, tier, &knobs, l.ink, |b, p, id, legend, dd, at| {
            kit::knob3d(b, p, &a(id), legend, dd, at, &l.knob, l.ink, l.dim);
        });
        let (np_h, preset_w, size, scale) = if small_room {
            (30.0, 200.0, 12.0, 1.1)
        } else {
            (28.0, 190.0, 10.0, 1.0)
        };
        kit::head(b, panel, DLY, true, |b, head| {
            kit::nameplate_top_right(
                b,
                head,
                DLY,
                tier,
                (8.0, np_h),
                d.title_w,
                preset_w,
                size,
                (l.accent, l.dim),
                |b, t| title(b, t, d, np_h, scale, l.ink),
            );
            kit::face_bypass(b, head, DLY, tier, "blue", "On");
        });
        kit::flip_arrow(b, panel, DLY, tier, true, (l.ink, &ui::alpha(l.dim, 0.3)));
    });
    let mut f = kit::delay_face(kit::tier_file(base, tier, dark), scene);
    f.params.extend(kit::nameplate_params(DLY));
    f.params.extend(kit::flip_params(DLY));
    f
}

/// Its tall face: the picture across the top, shorter than half so the
/// knobs under it are big; every control below.
#[must_use]
pub fn tall(d: &Device, dark: bool) -> Face {
    let l = (d.look)(dark);
    let scene = kit::tall_lane(d.name, |b, root| {
        let panel = ground(b, root, d, (kit::TALL_W, kit::TALL_H), &l);
        well(
            b,
            panel,
            d,
            (10.0, 10.0, kit::TALL_W - 20.0, ROWS.split - 16.0),
            &l,
        );
        let t = kit::Tall {
            ns: DLY,
            main: &d.front,
            inks: (l.ink, l.dim, &ui::alpha(kit::BLUE, 0.5), l.accent),
            scrim: l.scrim,
            bypass: ("blue", "On"),
            title_w: d.title_w,
        };
        kit::tall_chrome_rows(
            b,
            panel,
            &t,
            ROWS,
            |b, p, addr, legend, dd, at| {
                kit::knob3d(b, p, addr, legend, dd, at, &l.knob, l.ink, l.dim);
            },
            |b, p, addr, (cx, cy, dd)| {
                kit::knob3d_bare(b, p, addr, dd, (cx, cy), &l.wheel);
            },
            |b, tn| title(b, tn, d, 30.0, 1.1, l.ink),
        );
    });
    let mut f = kit::delay_face(kit::tall_file(d.base, dark), scene);
    f.params.extend(kit::tall_params(DLY));
    f
}
