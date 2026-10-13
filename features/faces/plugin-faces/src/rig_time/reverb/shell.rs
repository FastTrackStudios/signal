//! The layouts Swell, Reflections, Velvet, FreeVerb and Convolution share:
//! each draws its own ground, picture and lettering; this lays them out the
//! way Room lays out its own — the lane at its tiers (the picture at the
//! left, the brass-free Time knob and three knobs beside it, the nameplate,
//! bypass and More), the back (the advanced knobs), and the tall face (the
//! picture across a shorter top row, big knobs under it).
//!
//! Every picture carries a handle on `verb/decay`, drawn as the device's
//! own (a tail tab, a range gate, a slide pot, a trim flag), so the decay
//! is in the hand on the picture as well as on the Time knob.

use document_proto::NodeId;
use surface_kit::draw;
use surface_proto::{Axis, Control, ControlKind, HandleAxis};
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, VERB};

/// The room a picture is drawn for: a phone's narrow well, a lane's, or a
/// tall face's wide, short one (whose top right the nameplate covers).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Room {
    Phone,
    Lane,
    Tall,
}

/// A finish's inks and knobs.
pub struct Look {
    pub ink: &'static str,
    pub dim: &'static str,
    /// The nameplate's accent.
    pub accent: &'static str,
    /// The tall face's nameplate scrim.
    pub scrim: &'static str,
    /// The small knobs.
    pub knob: [&'static str; 6],
    /// The Time knob.
    pub wheel: [&'static str; 6],
}

/// A device: its name and file, its knobs, and how it draws itself.
pub struct Unit {
    pub name: &'static str,
    /// Its file's base (`90-verb-swell`).
    pub base: &'static str,
    pub title_w: f64,
    /// Its bypass key's legend.
    pub legend: &'static str,
    /// The lane's three knobs beside the Time knob (the first and Level on
    /// a phone).
    pub lane: [(&'static str, &'static str); 3],
    /// The tall face's first line.
    pub tall: [(&'static str, &'static str); 4],
    pub look: fn(bool) -> &'static Look,
    /// The panel, `w × h` on the root: returns it.
    pub ground: fn(&mut Builder<'_>, NodeId, (f64, f64), bool) -> NodeId,
    /// The picture in (x, y, w, h) on the panel: returns what takes a
    /// change of width.
    pub picture: fn(&mut Builder<'_>, NodeId, (f64, f64, f64, f64), Room, bool) -> Vec<NodeId>,
    /// The nameplate's title in its box, `h` tall, at `size`.
    pub title: fn(&mut Builder<'_>, NodeId, f64, f64, bool),
}

/// The tall faces' rows: a shorter picture, bigger knobs under it.
pub const ROWS: kit::TallRows = kit::TallRows {
    split: 134.0,
    main: (30.0, 50.0),
    more: (126.0, 40.0),
};

fn a(id: &str) -> String {
    format!("{VERB}/{id}")
}

/// A well for a picture in (x, y, w, h) on `panel`: clipped, rounded,
/// shaded in, filled top to bottom with `fill`. The caller draws into it
/// and puts the glass over it last.
pub fn well(
    b: &mut Builder<'_>,
    panel: NodeId,
    name: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    fill: &[(f64, &str)],
    r: f64,
) -> NodeId {
    let mut s = draw::frame(name, x, y, w, h);
    s.fills = vec![draw::linear(180.0, fill)];
    s.clips_content = true;
    s.corner_radii = document_proto::CornerRadii::uniform(r);
    s.effects = vec![draw::inner_shadow("#000000b0", 0.0, 2.0, 7.0)];
    b.add(panel, s)
}

/// A handle on `verb/decay` riding along x in `parent`: its centre travels
/// from `x0` to `x1` at `y` (its top), `hw × hh`; `build` draws it in its
/// own box. Returns the travel (the frame to stretch with the picture).
pub fn decay_handle(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (x0, x1, y): (f64, f64, f64),
    (hw, hh): (f64, f64),
    build: impl FnOnce(&mut Builder<'_>, NodeId),
) -> NodeId {
    let travel = b.add(
        parent,
        draw::frame(&format!("{name} travel"), x0, y, x1 - x0, hh),
    );
    let h = b.add(travel, draw::frame(name, 0.0, 0.0, hw, hh));
    build(b, h);
    let kind = ControlKind::Handle {
        x: Some(ui::bind(&a("decay"))),
        y: None,
        wheel: None,
        x_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
        y_map: HandleAxis::new(Axis::Linear { min: 0.0, max: 1.0 }),
    };
    b.controls
        .insert(h, Control::new(kind).bound(ui::bind(&a("decay"))));
    if let Some(n) = b.scene.node_mut(travel) {
        n.layout_child.resize_x = Some(document_proto::ResizeX::Stretch);
    }
    travel
}

/// The lane face at `tier`.
#[must_use]
pub fn face_at(u: &Unit, dark: bool, tier: kit::Tier) -> Face {
    let l = (u.look)(dark);
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane(u.name, tier, |b, root| {
        let panel = (u.ground)(b, root, (w, kit::H), dark);
        let pw = if small_room {
            kit::PHONE_PICTURE_R - 10.0
        } else {
            560.0
        };
        let stretch = (u.picture)(
            b,
            panel,
            (10.0, 10.0, pw, kit::H - 20.0),
            if small_room { Room::Phone } else { Room::Lane },
            dark,
        );
        let lit = ui::alpha(kit::PURPLE_LAMP, 0.55);
        if small_room {
            kit::nameplate_top_right(
                b,
                panel,
                VERB,
                tier,
                (6.0, 30.0),
                u.title_w + 4.0,
                220.0,
                12.0,
                (l.accent, l.dim),
                |b, t| (u.title)(b, t, 30.0, 22.0, dark),
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
            for (i, (id, legend)) in [u.lane[0], ("level", "Level")].iter().enumerate() {
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
                u.title_w,
                200.0,
                10.0,
                (l.accent, l.dim),
                |b, t| (u.title)(b, t, 28.0, 20.0, dark),
            );
            let x0 = 10.0 + pw + 20.0;
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
            for (i, (id, legend)) in u.lane.iter().enumerate() {
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
        kit::face_bypass(b, panel, VERB, tier, "purple", u.legend);
        kit::flip_arrow(b, panel, VERB, tier, false, (l.ink, &ui::alpha(l.dim, 0.3)));
        kit::responsive(b, panel, &stretch);
    });
    let mut f = kit::reverb_face(kit::tier_file(u.base, tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::time_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its back at `tier`: the ground, no picture — the advanced controls
/// across it, and (standing alone) the nameplate, the bypass and the arrow.
#[must_use]
pub fn back_at(u: &Unit, dark: bool, tier: kit::Tier) -> Face {
    let l = (u.look)(dark);
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane(u.name, tier, |b, root| {
        let panel = (u.ground)(b, root, (w, kit::H), dark);
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
        let (np, title_w, preset_w, size, ts) = if small_room {
            ((6.0, 30.0), u.title_w + 4.0, 220.0, 12.0, 22.0)
        } else {
            ((10.0, 28.0), u.title_w, 200.0, 10.0, 20.0)
        };
        kit::head(b, panel, VERB, true, |b, head| {
            kit::nameplate_top_right(
                b,
                head,
                VERB,
                tier,
                np,
                title_w,
                preset_w,
                size,
                (l.accent, l.dim),
                |b, t| (u.title)(b, t, np.1, ts, dark),
            );
            kit::face_bypass(b, head, VERB, tier, "purple", u.legend);
        });
        kit::flip_arrow(b, panel, VERB, tier, true, (l.ink, &ui::alpha(l.dim, 0.3)));
    });
    let mut f = kit::reverb_face(
        kit::tier_file(
            Box::leak(format!("{}-back", u.base).into_boxed_str()),
            tier,
            dark,
        ),
        scene,
    );
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its tall face: the picture across a top row shorter than half, so the
/// knobs under it are big.
#[must_use]
pub fn tall(u: &Unit, dark: bool) -> Face {
    let l = (u.look)(dark);
    let scene = kit::tall_lane(u.name, |b, root| {
        let panel = (u.ground)(b, root, (kit::TALL_W, kit::TALL_H), dark);
        (u.picture)(
            b,
            panel,
            (10.0, 10.0, kit::TALL_W - 20.0, ROWS.split - 16.0),
            Room::Tall,
            dark,
        );
        let t = kit::Tall {
            ns: VERB,
            main: &u.tall,
            inks: (l.ink, l.dim, &ui::alpha(kit::PURPLE_LAMP, 0.55), l.accent),
            scrim: l.scrim,
            bypass: ("purple", u.legend),
            title_w: u.title_w + 4.0,
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
            |b, tn| (u.title)(b, tn, 30.0, 22.0, dark),
        );
    });
    let mut f = kit::reverb_face(kit::tall_file(u.base, dark), scene);
    f.params.extend(kit::tall_params(VERB));
    f
}

/// A panel `w × h` on `root` filled with `fill`, lit along its top edge and
/// shaded along its bottom. Returns it.
pub fn panel(
    b: &mut Builder<'_>,
    root: NodeId,
    (w, h): (f64, f64),
    fill: &[(f64, &str)],
    r: f64,
) -> NodeId {
    let mut p = draw::frame("Panel", 0.0, 0.0, w, h);
    p.fills = vec![draw::linear(180.0, fill)];
    p.corner_radii = document_proto::CornerRadii::uniform(r);
    b.add(
        root,
        draw::with_effects(
            p,
            vec![
                draw::inner_shadow("#ffffff2a", 0.0, 1.0, 0.0),
                draw::inner_shadow("#00000050", 0.0, -2.0, 3.0),
            ],
        ),
    )
}
