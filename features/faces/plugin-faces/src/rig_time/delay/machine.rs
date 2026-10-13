//! What the drawn delay machines here share (drum, oil can, multitap,
//! spectral, filter): one design laid out for each room it is shown in —
//! the lane at its tiers (the picture in its well beside the Time knob and
//! the front's knobs), its back (the advanced controls), and its tall face
//! (the picture across the top, every control under it) — so each machine
//! writes only its finish and its picture.

use frame::document::proto::NodeId;
use frame::surface::kit::draw;
use frame::surface::widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, DLY};

/// Where a picture is drawn: what room it has.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Room {
    /// A phone's lane: the picture left of the controls, narrow.
    Phone,
    /// The lane: the picture beside the controls.
    Lane,
    /// The tall face's top row, its top right under the nameplate.
    Tall,
}

/// One finish's colours.
pub struct Look {
    pub ink: &'static str,
    pub dim: &'static str,
    /// The panel, top to bottom, and its edge.
    pub panel: [(f64, &'static str); 3],
    pub edge: &'static str,
    /// A fine texture over the panel (hammertone, brushing, grain…).
    pub texture: &'static str,
    /// The picture's well, top to bottom.
    pub well: [(f64, &'static str); 2],
    /// The front's knobs, and the Time knob.
    pub knob: [&'static str; 6],
    pub wheel: [&'static str; 6],
    /// The nameplate's accent, and its scrim on the tall face.
    pub accent: &'static str,
    pub scrim: &'static str,
}

/// A machine: its name, its file, its finishes, its title's lettering,
/// what it puts on its front, and its picture.
pub struct Machine {
    pub name: &'static str,
    pub file: &'static str,
    pub light: Look,
    pub dark: Look,
    /// The title: family, weight, size, tracking, and its letters' width
    /// (an em fraction: how wide the face sets).
    pub title: (&'static str, f64, f64, f64, f64),
    /// The front's knobs beside the Time knob on a lane, and on a phone.
    pub front: [(&'static str, &'static str); 4],
    pub phone: [(&'static str, &'static str); 3],
    /// The tall face's first line.
    pub main: [(&'static str, &'static str); 4],
    /// The picture into its well (`w × h`), for `room`.
    pub picture: fn(&mut Builder<'_>, NodeId, (f64, f64), &Look, Room, bool),
    /// Whether the well is behind glass (a picture that is a window); a
    /// machine whose metal catches the light leaves it open.
    pub glass: bool,
}

/// The tall face's rows: a shorter picture, bigger knobs (as Room's).
pub const ROWS: kit::TallRows = kit::TallRows {
    split: 134.0,
    main: (30.0, 50.0),
    more: (126.0, 40.0),
};

fn a(id: &str) -> String {
    format!("{DLY}/{id}")
}

impl Machine {
    fn look(&self, dark: bool) -> &Look {
        if dark { &self.dark } else { &self.light }
    }

    fn title_w(&self) -> f64 {
        let (_, _, size, track, em) = self.title;
        (self.name.chars().count() as f64 * size * (em + track) + 28.0).ceil()
    }

    fn set_title(&self, b: &mut Builder<'_>, t: NodeId, h: f64, ink: &str) {
        let (family, weight, size, track, _) = self.title;
        let l = draw::Lettering::family(family).weight(weight);
        crate::suite_studio_blues::parts::set(
            b,
            t,
            self.name,
            self.title_w() / 2.0 - 8.0,
            h / 2.0,
            size,
            ink,
            &l,
            track,
        );
    }

    /// The panel `w × h` in its finish, with its texture.
    fn ground(&self, b: &mut Builder<'_>, root: NodeId, (w, h): (f64, f64), l: &Look) -> NodeId {
        let mut panel = draw::frame("Panel", 0.0, 0.0, w, h);
        panel.fills = vec![draw::linear(180.0, &l.panel)];
        panel.corner_radii = frame::document::proto::CornerRadii::uniform(9.0);
        let panel = b.add(
            root,
            draw::with_effects(
                draw::stroked(panel, l.edge, 1.0),
                vec![
                    draw::inner_shadow("#ffffff26", 0.0, 1.0, 0.0),
                    draw::inner_shadow("#00000050", 0.0, -2.0, 3.0),
                ],
            ),
        );
        // The texture: short flecks, a hammertone's or a grain's.
        let mut flecks = Vec::new();
        let mut i = 0.0;
        while i < (w * h / 1400.0).floor() {
            let x = crate::reverb::jitter(i * 1.31 + 0.4) * w;
            let y = crate::reverb::jitter(i * 2.17 + 5.0) * h;
            let len = 2.0 + crate::reverb::jitter(i * 0.77 + 2.0) * 3.0;
            flecks.push(vec![(x, y), (x + len, y + 0.6)]);
            i += 1.0;
        }
        b.add(
            panel,
            ui::strokes("texture", &flecks, 1.0, draw::solid(l.texture)),
        );
        panel
    }

    /// The picture's well at (`x, y, w, h`), with the picture in it.
    fn well(
        &self,
        b: &mut Builder<'_>,
        panel: NodeId,
        (x, y, w, h): (f64, f64, f64, f64),
        l: &Look,
        room: Room,
        dark: bool,
    ) -> NodeId {
        let rim = draw::rounded(
            draw::rect(
                "well rim",
                x - 2.0,
                y - 2.0,
                w + 4.0,
                h + 4.0,
                draw::linear(180.0, &[(0.0, "#00000060"), (1.0, "#ffffff30")]),
            ),
            9.0,
        );
        b.add(panel, rim);
        let mut well = draw::frame("Picture", x, y, w, h);
        well.fills = vec![draw::linear(180.0, &l.well)];
        well.clips_content = true;
        well.corner_radii = frame::document::proto::CornerRadii::uniform(7.0);
        well.effects = vec![draw::inner_shadow("#000000b0", 0.0, 2.0, 7.0)];
        let well = b.add(panel, well);
        (self.picture)(b, well, (w, h), l, room, dark);
        if self.glass {
            kit::glass(b, well);
        }
        well
    }

    /// The lane at `tier`.
    #[must_use]
    pub fn face_at(&self, dark: bool, tier: kit::Tier) -> Face {
        let l = self.look(dark);
        let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
        let w = tier.lane_w();
        let tw = self.title_w();
        let scene = kit::tier_lane(self.name, tier, |b, root| {
            let panel = self.ground(b, root, (w, kit::H), l);
            let (ink, dim) = (l.ink, l.dim);
            let lit = ui::alpha(kit::BLUE, 0.5);
            let vw = if small_room {
                kit::PHONE_PICTURE_R - 14.0
            } else {
                540.0
            };
            let well = self.well(
                b,
                panel,
                (14.0, 14.0, vw, kit::H - 28.0),
                l,
                if small_room { Room::Phone } else { Room::Lane },
                dark,
            );
            if small_room {
                let (tx, ty, td) = kit::PHONE_TIME;
                kit::time_knob(
                    b,
                    panel,
                    DLY,
                    (tx, ty, td),
                    11.0,
                    (ink, dim, &lit),
                    |b, p, addr| {
                        kit::knob3d_bare(b, p, addr, td, (tx, ty), &l.wheel);
                    },
                );
                let (xs, ky, kd) = kit::PHONE_KNOBS;
                for (i, (id, legend)) in self.phone.iter().enumerate() {
                    kit::knob3d(b, panel, &a(id), legend, kd, (xs[i], ky), &l.knob, ink, dim);
                }
                kit::nameplate_top_right(
                    b,
                    panel,
                    DLY,
                    tier,
                    (6.0, 30.0),
                    tw,
                    200.0,
                    12.0,
                    (l.accent, dim),
                    |b, t| self.set_title(b, t, 30.0, ink),
                );
            } else {
                let x0 = 14.0 + vw + 24.0;
                let (tx, ty, td) = (x0 + 30.0, 78.0, 46.0);
                kit::time_knob(
                    b,
                    panel,
                    DLY,
                    (tx, ty, td),
                    8.5,
                    (ink, dim, &lit),
                    |b, p, addr| {
                        kit::knob3d_bare(b, p, addr, td, (tx, ty), &l.wheel);
                    },
                );
                for (i, (id, legend)) in self.front.iter().enumerate() {
                    kit::knob3d(
                        b,
                        panel,
                        &a(id),
                        legend,
                        34.0,
                        (x0 + 100.0 + 58.0 * i as f64, 86.0),
                        &l.knob,
                        ink,
                        dim,
                    );
                }
                kit::nameplate_top_right(
                    b,
                    panel,
                    DLY,
                    tier,
                    (8.0, 28.0),
                    tw,
                    186.0,
                    10.0,
                    (l.accent, dim),
                    |b, t| self.set_title(b, t, 28.0, ink),
                );
            }
            kit::face_bypass(b, panel, DLY, tier, "blue", "On");
            kit::flip_arrow(b, panel, DLY, tier, false, (ink, &ui::alpha(dim, 0.3)));
            let stretch = kit::with_sibling(b, panel, well, -1);
            kit::responsive(b, panel, &stretch);
        });
        let mut f = kit::delay_face(kit::tier_file(self.file, tier, dark), scene);
        f.params.extend(kit::nameplate_params(DLY));
        f.params.extend(kit::time_params(DLY));
        f.params.extend(kit::flip_params(DLY));
        f
    }

    /// Its back at `tier`: the panel, no picture — the advanced controls
    /// across it, and (standing alone) the nameplate, the bypass and the
    /// arrow where they are on the front.
    #[must_use]
    pub fn back_at(&self, dark: bool, tier: kit::Tier) -> Face {
        let l = self.look(dark);
        let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
        let w = tier.lane_w();
        let tw = self.title_w();
        let scene = kit::tier_lane(self.name, tier, |b, root| {
            let panel = self.ground(b, root, (w, kit::H), l);
            kit::back_knobs(
                b,
                panel,
                tier,
                &kit::DLY_MORE,
                l.ink,
                |b, p, id, legend, d, at| {
                    kit::knob3d(b, p, &a(id), legend, d, at, &l.knob, l.ink, l.dim);
                },
            );
            let (np, preset_w, size) = if small_room {
                ((6.0, 30.0), 200.0, 12.0)
            } else {
                ((8.0, 28.0), 186.0, 10.0)
            };
            kit::head(b, panel, DLY, true, |b, head| {
                kit::nameplate_top_right(
                    b,
                    head,
                    DLY,
                    tier,
                    np,
                    tw,
                    preset_w,
                    size,
                    (l.accent, l.dim),
                    |b, t| self.set_title(b, t, np.1, l.ink),
                );
                kit::face_bypass(b, head, DLY, tier, "blue", "On");
            });
            kit::flip_arrow(b, panel, DLY, tier, true, (l.ink, &ui::alpha(l.dim, 0.3)));
        });
        let mut f = kit::delay_face(
            kit::tier_file(
                Box::leak(format!("{}-back", self.file).into_boxed_str()),
                tier,
                dark,
            ),
            scene,
        );
        f.params.extend(kit::nameplate_params(DLY));
        f.params.extend(kit::flip_params(DLY));
        f
    }

    /// Its tall face: the picture across the top, shorter than half so the
    /// controls under it get big knobs.
    #[must_use]
    pub fn tall(&self, dark: bool) -> Face {
        let l = self.look(dark);
        let tw = self.title_w();
        let scene = kit::tall_lane(self.name, |b, root| {
            let panel = self.ground(b, root, (kit::TALL_W, kit::TALL_H), l);
            self.well(
                b,
                panel,
                (10.0, 10.0, kit::TALL_W - 20.0, ROWS.split - 16.0),
                l,
                Room::Tall,
                dark,
            );
            let t = kit::Tall {
                ns: DLY,
                main: &self.main,
                inks: (l.ink, l.dim, &ui::alpha(kit::BLUE, 0.5), l.accent),
                scrim: l.scrim,
                bypass: ("blue", "On"),
                title_w: tw,
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
                |b, tn| self.set_title(b, tn, 30.0, l.ink),
            );
        });
        let mut f = kit::delay_face(kit::tall_file(self.file, dark), scene);
        f.params.extend(kit::tall_params(DLY));
        f
    }
}

/// While the block plays: 1, else 0.
pub const ON: &str = "param(dly/on)";

/// The repeat period, ms (never below 20).
pub const PERIOD: &str = "max(param(dly/time), 20)";

/// The light of the `n`-th repeat now, 0…1: a flash every `n` times the
/// delay time, falling away, dimmer by the feedback for each repeat
/// before it, dark while bypassed.
#[must_use]
pub fn flash(n: u32) -> String {
    format!(
        "pow(1 - fract(time * 1000 / ({n} * {PERIOD})), 4) * pow(param(dly/feedback), {}) * {ON}",
        n.saturating_sub(1)
    )
}

/// The pictures' spring: what follows a knob follows it softly.
#[must_use]
pub fn spring() -> frame::document::proto::motion::Dynamics {
    frame::document::proto::motion::Dynamics::spring(260.0, 28.0)
}

/// An open stroke outline (no fill) of `n` in `color`, `weight` wide.
#[must_use]
pub fn outline(
    mut n: frame::document::proto::Node,
    color: &str,
    weight: f64,
) -> frame::document::proto::Node {
    n = draw::stroked(n, color, weight);
    n.fills.clear();
    n
}
