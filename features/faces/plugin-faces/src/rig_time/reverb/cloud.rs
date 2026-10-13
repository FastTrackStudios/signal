//! Cloud: the ambient reverb as the sky it sounds like — a dusk sky, soft
//! clouds drifting across it and grains of light glinting in them. The
//! clouds grow with the size and thicken with the decay; the modulation
//! sets how fast they drift. Beside the sky, a frosted panel.
//!
//! Two finishes: light, a dusk sky on a frosted lavender panel; dark, a
//! moonlit night on deep indigo.

use document_proto::motion::{Dynamics, Motion, Target};
use document_proto::NodeId;
use surface_kit::{draw, motion};
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, VERB};

/// One finish's colours.
struct Look {
    ink: &'static str,
    dim: &'static str,
    panel: [(f64, &'static str); 3],
    sky: [(f64, &'static str); 4],
    star: &'static str,
    /// (far, mid, near) clouds and how opaque each gets.
    clouds: [(&'static str, f64); 3],
    grain: (&'static str, &'static str),
    knob: [&'static str; 6],
}

const LIGHT: Look = Look {
    ink: "#2e2a4a",
    dim: "#6d6790",
    panel: [(0.0, "#f4f0fb"), (0.6, "#e3dcf2"), (1.0, "#d2c8ea")],
    sky: [(0.0, "#1b1840"), (0.45, "#4a3a78"), (0.8, "#a8739a"), (1.0, "#f0a88a")],
    star: "#fff6e8",
    clouds: [("#b9a8d8", 0.55), ("#e2d2f0", 0.75), ("#fff1f4", 0.9)],
    grain: ("#fffbe0", "#ffe9b0"),
    knob: ["color=#fbf9ff", "pointer=#7a68c8", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#cfc4ea"],
};

const DARK: Look = Look {
    ink: "#ece6ff",
    dim: "#8f86b8",
    panel: [(0.0, "#1c1733"), (0.6, "#141027"), (1.0, "#0c0a19")],
    sky: [(0.0, "#07061a"), (0.5, "#15113a"), (0.85, "#2a1f5c"), (1.0, "#3b2a72")],
    star: "#f4f0ff",
    clouds: [("#5b4f8f", 0.6), ("#7d70b3", 0.7), ("#a99cd8", 0.75)],
    grain: ("#efe9ff", "#c4b5fd"),
    knob: ["color=#221c3d", "pointer=#c4b5fd", "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#0c0a19"],
};

fn a(id: &str) -> String {
    format!("{VERB}/{id}")
}


/// A cloud: a cluster of soft puffs, `w × h` (drawn at its full size),
/// in `tint`.
fn cloud(b: &mut Builder<'_>, parent: NodeId, name: &str, (x, y, w, h): (f64, f64, f64, f64), tint: &str, seed: f64) -> NodeId {
    let g = b.add(parent, draw::frame(name, x, y, w, h));
    let puffs = 7;
    for i in 0..puffs {
        let t = f64::from(i) / f64::from(puffs - 1);
        let j = crate::reverb::jitter(seed + f64::from(i));
        let r = h * (0.28 + 0.2 * (std::f64::consts::PI * t).sin() + 0.06 * j);
        let (cx, cy) = (w * (0.12 + 0.76 * t), h * (0.62 - 0.22 * (std::f64::consts::PI * t).sin() + 0.05 * j));
        let puff = draw::circle("puff", cx, cy, r, draw::radial_at(0.45, 0.4, 0.75, &[(0.0, &ui::alpha(tint, 0.6)), (0.35, &ui::alpha(tint, 0.38)), (0.7, &ui::alpha(tint, 0.12)), (1.0, &ui::alpha(tint, 0.0))]));
        // Lit from within: the big middle puff gives off the cloud's light
        // (one emitter a cloud keeps the bloom cheap).
        b.add(g, if i == puffs / 2 { ui::glowing(puff, tint, 0.3, kit::BLOOM) } else { puff });
    }
    g
}

/// The sky in `sky` (`w × h`).
fn sky(b: &mut Builder<'_>, sky: NodeId, w: f64, h: f64, l: &Look, dark: bool) {
    // Stars, faint, fading in and out slowly.
    for i in 0..26 {
        let f = f64::from(i);
        let (x, y) = ((f * 83.0 + 17.0) % w, 6.0 + (f * 41.0) % (h * 0.55));
        let s = b.add(sky, draw::circle("star", x, y, 0.8 + 0.5 * crate::reverb::jitter(f).abs(), draw::solid(l.star)));
        motion::add(b, s, Motion::new(Target::Opacity).input(format!("0.5 + 0.5 * sin(time * {:.2} + {f})", 0.4 + 0.3 * crate::reverb::jitter(f * 3.0).abs())).range(0.15, 0.9));
    }
    if dark {
    // The moon, high on the right, and its halo.
    b.add(sky, draw::circle("halo", w * 0.78, h * 0.26, h * 0.62, draw::radial_at(0.5, 0.5, 0.5, &[(0.0, "#c4b5fd40"), (1.0, "#c4b5fd00")])));
    b.add(sky, draw::emissive(draw::circle("moon", w * 0.78, h * 0.26, 11.0, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#fbf8ff"), (1.0, "#d6ccf5")])), draw::hex("#e9e2ff"), 0.8, kit::BLOOM));
    } else {
        // The glow low on the horizon.
        b.add(sky, draw::circle("afterglow", w * 0.3, h * 1.25, h * 0.9, draw::radial_at(0.5, 0.5, 0.5, &[(0.0, "#ffb38a99"), (1.0, "#ffb38a00")])));
    }

    // The clouds, back to front: each drifts across and wraps round, the
    // near ones faster; all of them faster with the modulation.
    let layers = [
        ("far", 0.7, 0.012, 22.0, l.clouds[0].0, l.clouds[0].1),
        ("mid", 0.85, 0.02, 48.0, l.clouds[1].0, l.clouds[1].1),
        ("near", 1.0, 0.032, 70.0, l.clouds[2].0, l.clouds[2].1),
    ];
    for (li, (name, scale, speed, cy, tint, alpha)) in layers.iter().enumerate() {
        for k in 0..3 {
            let cw = 220.0 * scale;
            let ch = 70.0 * scale;
            let c = cloud(b, sky, name, (0.0, cy - ch / 2.0 + 6.0 * f64::from(k % 2), cw, ch), tint, f64::from(li as u32 * 7 + k));
            let offset = f64::from(k) / 3.0 + 0.13 * li as f64;
            let drift = format!("fract(time * {speed} * (0.4 + 1.6 * norm({VERB}/modulation)) + {offset})");
            motion::add(b, c, Motion::new(Target::X).input(drift).range(-cw, w));
            // Bigger with the size, thicker with the decay.
            motion::add(b, c, Motion::new(Target::Scale).input(format!("norm({VERB}/size)")).range(0.7, 1.15).dynamics(Dynamics::spring(160.0, 22.0)));
            motion::add(b, c, Motion::new(Target::Opacity).input(format!("(0.35 + 0.65 * norm({VERB}/decay)) * (0.35 + 0.65 * param({VERB}/on))")).range(0.0, *alpha));
        }
    }
    // Grains of light in the cloud: the reverb's grains, glinting.
    for i in 0..30 {
        let f = f64::from(i);
        let (x, y) = ((f * 131.0 + 40.0) % (w - 20.0) + 10.0, 30.0 + (f * 59.0) % (h - 50.0));
        let g = b.add(sky, draw::emissive(draw::circle("grain", x, y, 1.3, draw::solid(l.grain.0)), draw::hex(l.grain.1), 1.6, kit::BLOOM));
        let glint = format!("pow(0.5 + 0.5 * sin(time * {:.2} + {:.2}), 12) * param({VERB}/on)", 1.1 + 0.9 * crate::reverb::jitter(f).abs(), f * 1.7);
        motion::add(b, g, Motion::new(Target::Opacity).input(glint).range(0.0, 1.0));
    }
    kit::glass(b, sky);
}

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

/// The face at `tier`: one design — the sky and the frosted panel in the
/// same inks and lettering — laid out for the room. The smaller tiers
/// give the sky less width and the panel larger knobs and type (Decay,
/// Size, Level, On); the top right stays clear for the lane's preset bar.
#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let (ink, dim) = (l.ink, l.dim);
    let small_room = tier.small();
    let w = tier.lane_w();
    let scene = kit::tier_lane("Cloud", tier, |b, root| {
        // The frosted panel.
        let mut panel = draw::frame("Panel", 0.0, 0.0, w, kit::H);
        panel.fills = vec![draw::linear(160.0, &l.panel)];
        panel.corner_radii = document_proto::CornerRadii::uniform(10.0);
        b.add(root, draw::with_effects(panel, vec![draw::inner_shadow("#ffffff1a", 0.0, 1.0, 0.0)]));
        let panel = root;

        // ── The sky ────────────────────────────────────────────────────
        // On a phone the controls come first: the sky shrinks for them.
        let sw = if small_room { kit::picture_r(tier) - 10.0 } else { 600.0 };
        let (sx, sy, sh) = (10.0, 10.0, kit::H - 20.0);
        let mut s = draw::frame("Sky", sx, sy, sw, sh);
        s.fills = vec![draw::linear(180.0, &l.sky)];
        s.clips_content = true;
        s.corner_radii = document_proto::CornerRadii::uniform(8.0);
        s.effects = vec![draw::inner_shadow("#00000066", 0.0, 2.0, 8.0)];
        let s = b.add(panel, s);
        sky(b, s, sw, sh, l, dark);

        // ── The panel ──────────────────────────────────────────────────
        let x0 = sx + sw + 18.0;
        let serif = draw::Lettering::family(draw::family::PLAYFAIR_DISPLAY).weight(400.0);
        let pearl = l.knob;
        if small_room {
            kit::nameplate_top_right(b, panel, VERB, tier, (6.0, 30.0), 86.0, if tier == kit::Tier::Ipad { 150.0 } else { 220.0 }, 12.0, (if dark { kit::PURPLE_LAMP } else { "#6a3fc4" }, dim), |b, t| {
                crate::suite_studio_blues::parts::set(b, t, "Cloud", 34.0, 15.0, 22.0, ink, &serif, 0.08);
            });
            // Spread across the panel: Decay, Size, Level, On — each with
            // its value and name under it, inside the lane.
            let (tx, ty, td) = kit::time_spot(tier);
            kit::time_knob(b, panel, VERB, (tx, ty, td), if tier == kit::Tier::Ipad { 10.0 } else { 11.0 }, (ink, dim, &ui::alpha(kit::PURPLE_LAMP, 0.55)), |b, p, addr| {
                kit::knob3d_bare(b, p, addr, td, (tx, ty), &pearl);
            });
            let (xs, ky, kd) = kit::knob_row(tier);
            for (i, (id, legend)) in [("size", "Size"), ("level", "Level")].iter().enumerate() {
                kit::knob3d(b, panel, &a(id), legend, kd, (xs[i], ky), &pearl, ink, dim);
            }
        } else {
            kit::nameplate_top_right(b, panel, VERB, tier, (10.0, 28.0), 82.0, 200.0, 10.0, (if dark { kit::PURPLE_LAMP } else { "#6a3fc4" }, dim), |b, t| {
                crate::suite_studio_blues::parts::set(b, t, "Cloud", 34.0, 14.0, 22.0, ink, &serif, 0.08);
            });
            kit::time_knob(b, panel, VERB, (x0 + 34.0, 80.0, 48.0), 8.5, (ink, dim, &ui::alpha(kit::PURPLE_LAMP, 0.55)), |b, p, addr| {
                kit::knob3d_bare(b, p, addr, 48.0, (x0 + 34.0, 80.0), &pearl);
            });
            let small = [("size", "Size"), ("modulation", "Drift"), ("predelay", "Pre-Delay"), ("level", "Level")];
            for (i, (id, legend)) in small.iter().enumerate() {
                kit::knob3d(b, panel, &a(id), legend, 30.0, (x0 + 96.0 + 50.0 * i as f64, 92.0), &pearl, ink, dim);
            }
        }
        kit::face_bypass(b, panel, VERB, tier, "purple", "On");
        kit::flip_arrow(b, panel, VERB, tier, false, (ink, &ui::alpha(dim, 0.3)));
        // Responsive: the sky takes the width.
        kit::responsive(b, root, &[s]);
    });
    let mut f = kit::reverb_face(kit::tier_file("84-verb-cloud", tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::time_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its back at `tier`: the frosted panel, no sky — the advanced controls
/// across it, the nameplate, the bypass and the arrow where they are on
/// the front.
#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let (ink, dim) = (l.ink, l.dim);
    let small_room = tier.small();
    let w = tier.lane_w();
    let scene = kit::tier_lane("Cloud", tier, |b, root| {
        let mut panel = draw::frame("Panel", 0.0, 0.0, w, kit::H);
        panel.fills = vec![draw::linear(160.0, &l.panel)];
        panel.corner_radii = document_proto::CornerRadii::uniform(10.0);
        b.add(root, draw::with_effects(panel, vec![draw::inner_shadow("#ffffff1a", 0.0, 1.0, 0.0)]));
        let pearl = l.knob;
        let knobs = [("predelay", "Pre-Delay"), ("modulation", "Drift"), ("damping", "HF Damp"), ("low_cut", "Low Cut"), ("high_cut", "High Cut"), ("duck", "Duck"), ("tone", "Tone"), ("duck_threshold", "Duck Thr"), ("duck_release", "Duck Rel"), ("pan_a", "Pan")];
        kit::back_knobs(b, root, tier, &knobs, ink, |b, p, id, legend, d, at| {
            kit::knob3d(b, p, &a(id), legend, d, at, &pearl, ink, dim);
        });
        let (np_h, title_w, preset_w, size) = if small_room { (30.0, 86.0, if tier == kit::Tier::Ipad { 150.0 } else { 220.0 }, 12.0) } else { (28.0, 82.0, 200.0, 10.0) };
        let serif = draw::Lettering::family(draw::family::PLAYFAIR_DISPLAY).weight(400.0);
        kit::head(b, root, VERB, true, |b, head| {
            kit::nameplate_top_right(b, head, VERB, tier, (if small_room { 6.0 } else { 10.0 }, np_h), title_w, preset_w, size, (if dark { kit::PURPLE_LAMP } else { "#6a3fc4" }, dim), |b, t| {
                crate::suite_studio_blues::parts::set(b, t, "Cloud", 34.0, np_h / 2.0, 22.0, ink, &serif, 0.08);
            });
            kit::face_bypass(b, head, VERB, tier, "purple", "On");
        });
        kit::flip_arrow(b, root, VERB, tier, true, (ink, &ui::alpha(dim, 0.3)));
    });
    let mut f = kit::reverb_face(kit::tier_file("84-verb-cloud-back", tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its tall face: the sky the whole top row, every control on the frosted
/// panel below it.
#[must_use]
pub fn tall(dark: bool) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let (ink, dim) = (l.ink, l.dim);
    let scene = kit::tall_lane("Cloud", |b, root| {
        let mut panel = draw::frame("Panel", 0.0, 0.0, kit::TALL_W, kit::TALL_H);
        panel.fills = vec![draw::linear(160.0, &l.panel)];
        panel.corner_radii = document_proto::CornerRadii::uniform(10.0);
        let panel = b.add(root, draw::with_effects(panel, vec![draw::inner_shadow("#ffffff1a", 0.0, 1.0, 0.0)]));
        let (sx, sy, sw, sh) = kit::TALL_PICTURE;
        let mut s = draw::frame("Sky", sx, sy, sw, sh);
        s.fills = vec![draw::linear(180.0, &l.sky)];
        s.clips_content = true;
        s.corner_radii = document_proto::CornerRadii::uniform(8.0);
        s.effects = vec![draw::inner_shadow("#00000066", 0.0, 2.0, 8.0)];
        let s = b.add(panel, s);
        sky(b, s, sw, sh, l, dark);
        let pearl = l.knob;
        let serif = draw::Lettering::family(draw::family::PLAYFAIR_DISPLAY).weight(400.0);
        let t = kit::Tall { ns: VERB, main: &[("size", "Size"), ("modulation", "Drift"), ("predelay", "Pre-Delay"), ("level", "Level")], inks: (ink, dim, &ui::alpha(kit::PURPLE_LAMP, 0.55), if dark { kit::PURPLE_LAMP } else { "#6a3fc4" }), scrim: if dark { "#141027e0" } else { "#f4f0fbe0" }, bypass: ("purple", "On"), title_w: 86.0 };
        kit::tall_chrome(b, panel, &t, |b, p, addr, legend, d, at| {
            kit::knob3d(b, p, addr, legend, d, at, &pearl, ink, dim);
        }, |b, p, addr, (cx, cy, d)| {
            kit::knob3d_bare(b, p, addr, d, (cx, cy), &pearl);
        }, |b, tn| {
            crate::suite_studio_blues::parts::set(b, tn, "Cloud", 34.0, 15.0, 22.0, ink, &serif, 0.08);
        });
    });
    let mut f = kit::reverb_face(kit::tall_file("84-verb-cloud", dark), scene);
    f.params.extend(kit::tall_params(VERB));
    f
}

/// The night sky laid out for a block (`s` square): the moon high, the
/// clouds — each a fraction of the block — drifting through its middle,
/// where the block's labels leave it clear; stars and grains as on the
/// face, the same motions.
fn block_sky(b: &mut Builder<'_>, v: NodeId, s: f64, l: &Look) {
    for i in 0..18 {
        let f = f64::from(i);
        let (x, y) = ((f * 67.0 + 11.0) % s, s * 0.22 + (f * 37.0) % (s * 0.56));
        let st = b.add(v, draw::circle("star", x, y, 0.9 + 0.4 * crate::reverb::jitter(f).abs(), draw::solid(l.star)));
        motion::add(b, st, Motion::new(Target::Opacity).input(format!("0.5 + 0.5 * sin(time * {:.2} + {f})", 0.4 + 0.3 * crate::reverb::jitter(f * 3.0).abs())).range(0.15, 0.9));
    }
    b.add(v, draw::circle("halo", s * 0.74, s * 0.34, s * 0.22, draw::radial_at(0.5, 0.5, 0.5, &[(0.0, "#c4b5fd40"), (1.0, "#c4b5fd00")])));
    b.add(v, draw::emissive(draw::circle("moon", s * 0.74, s * 0.34, 8.0, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, "#fbf8ff"), (1.0, "#d6ccf5")])), draw::hex("#e9e2ff"), 0.8, 10.0));
    let layers = [("far", 0.42, 0.012), ("mid", 0.55, 0.02), ("near", 0.68, 0.032)];
    for (li, (name, cy, speed)) in layers.iter().enumerate() {
        let (tint, alpha) = l.clouds[li];
        for k in 0..2 {
            let cw = s * (0.5 + 0.12 * li as f64);
            let ch = cw * 0.34;
            let c = cloud(b, v, name, (0.0, s * cy - ch / 2.0, cw, ch), tint, f64::from(li as u32 * 7 + k));
            let offset = f64::from(k) / 2.0 + 0.13 * li as f64;
            let drift = format!("fract(time * {speed} * 2.4 * (0.4 + 1.6 * norm({VERB}/modulation)) + {offset})");
            motion::add(b, c, Motion::new(Target::X).input(drift).range(-cw, s));
            motion::add(b, c, Motion::new(Target::Scale).input(format!("norm({VERB}/size)")).range(0.75, 1.1).dynamics(Dynamics::spring(160.0, 22.0)));
            motion::add(b, c, Motion::new(Target::Opacity).input(format!("(0.35 + 0.65 * norm({VERB}/decay)) * (0.35 + 0.65 * param({VERB}/on))")).range(0.0, alpha));
        }
    }
    for i in 0..16 {
        let f = f64::from(i);
        let (x, y) = ((f * 53.0 + 17.0) % (s - 20.0) + 10.0, s * 0.3 + (f * 29.0) % (s * 0.45));
        let g = b.add(v, draw::emissive(draw::circle("grain", x, y, 1.2, draw::solid(l.grain.0)), draw::hex(l.grain.1), 1.0, 5.0));
        let glint = format!("pow(0.5 + 0.5 * sin(time * {:.2} + {:.2}), 12) * param({VERB}/on)", 1.1 + 0.9 * crate::reverb::jitter(f).abs(), f * 1.7);
        motion::add(b, g, Motion::new(Target::Opacity).input(glint).range(0.0, 1.0));
    }
}

/// The routing grid's block: the night sky, its clouds drifting.
#[must_use]
pub fn block() -> Face {
    use crate::rig_cells::S;
    let l = &DARK;
    let sky_fill: Vec<(f64, &str)> = l.sky.to_vec();
    let scene = crate::rig_cells::block("Cloud", &sky_fill, |b, v, _| block_sky(b, v, S, l));
    kit::reverb_face("84-verb-cloud-block", scene)
}
