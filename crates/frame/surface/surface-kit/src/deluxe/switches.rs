//! `switch.hex_stomp`, `switch.mini_lever`, `switch.slot_lever`,
//! `lever.glow_cap`.

use document_proto::NodeId;
use surface_proto::{ButtonMode, ControlKind, Orientation, SlotRole};
use surface_widgets::Builder;

use super::finish;
use super::knobs::ellipse_d;
use crate::controls::parts::{alpha, lamp};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

/// A stomp actuator on a hex nut, seen a little from the front, with its
/// status LED beside it (`led=left|right|none`, `color=#hex`).
fn hex_stomp(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(30.0).max(12.0);
    let k = d / 30.0;
    let f = finish(p.variant.as_deref());
    let side = p.opt("led").unwrap_or("right");
    let led = (side != "none").then(|| p.lamp_color(0).unwrap_or_else(|| "#eef4ff".into()));
    let nr = d * 0.86;
    let drop = d * 0.34;
    let led_off = nr + d * 0.62;
    let led_w = if led.is_some() { d * 0.9 } else { 0.0 };
    let w = (2.0 * nr + 6.0 * k + led_w).max(p.touch.unwrap_or(0.0));
    let h = (2.0 * nr + drop + 6.0 * k).max(p.touch.unwrap_or(0.0));
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, w, h));
    let x0 = if side == "left" { w - nr - 3.0 * k } else { nr + 3.0 * k };
    let y0 = nr + 3.0 * k;
    let ny = y0 + drop;
    let mut c = p.control(if p.flag("momentary") { ControlKind::Button { mode: ButtonMode::Momentary } } else { ControlKind::footswitch() });

    let sh = draw::path("shadow", 0.0, 0.0, w, h, ellipse_d(x0, ny + 3.0 * k, nr, nr * 0.9), draw::solid("#00000073"));
    b.add(root, draw::with_effects(sh, vec![draw::drop_shadow("#00000059", 0.0, 3.0 * k, 5.0 * k)]));
    let nut = draw::path("nut", 0.0, 0.0, w, h, draw::regular(x0, ny, nr, 6, 0.0), draw::linear(150.0, &[(0.0, f.hi), (0.4, f.mid), (0.62, f.lo), (1.0, f.mid)]));
    b.add(root, draw::with_effects(draw::stroked(nut, &alpha(f.lo, 0.9), 0.6 * k), vec![draw::inner_shadow("#ffffff55", 0.0, 1.0 * k, 1.0 * k)]));
    let washer = draw::circle("washer", x0, ny, nr * 0.74, draw::conic(0.0, &[(0.0, f.hi), (0.22, f.lo), (0.45, f.hi), (0.72, f.lo), (1.0, f.hi)]));
    b.add(root, draw::stroked(washer, &alpha(f.lo, 0.8), 0.5 * k));
    // The locking slot at the nut's front.
    b.add(root, draw::rounded(draw::rect("slot", x0 - d * 0.12, ny + nr * 0.6, d * 0.24, d * 0.13, draw::solid("#0d0b09")), 1.0 * k));
    // The barrel rising to the dome.
    let barrel = draw::rect("barrel", x0 - d * 0.47, y0, d * 0.94, drop, draw::linear(90.0, &[(0.0, f.lo), (0.3, f.hi), (0.55, f.mid), (1.0, f.lo)]));
    b.add(root, barrel);
    b.add(root, draw::path("barrel foot", 0.0, 0.0, w, h, ellipse_d(x0, ny, d * 0.47, d * 0.16), draw::solid(&alpha(f.lo, 0.9))));
    let cap = draw::circle("cap", x0, y0, d / 2.0, draw::radial_at(0.36, 0.3, 0.85, &[(0.0, "#ffffff"), (0.2, f.hi), (0.6, f.mid), (1.0, f.lo)]));
    b.add(root, draw::with_effects(draw::stroked(cap, &alpha(f.lo, 0.9), 0.7 * k), vec![draw::inner_shadow("#00000059", 0.0, -1.5 * k, 3.0 * k)]));
    b.add(root, draw::circle("spec", x0 - d * 0.15, y0 - d * 0.17, d * 0.13, draw::solid("#ffffffb0")));
    if let Some(color) = led {
        let lx = if side == "left" { x0 - led_off } else { x0 + led_off };
        let lens = lamp(b, root, lx, ny - d * 0.08, d * 0.26, &color);
        c = c.slot(SlotRole::Led, lens.0);
    }
    b.controls.insert(root, c);
    root
}

/// The points of a bat leaning `deg` from the pivot (`px`, `py`).
fn bat(px: f64, py: f64, len: f64, w0: f64, w1: f64, deg: f64) -> (String, (f64, f64)) {
    let a = deg.to_radians();
    let (ux, uy) = (a.sin(), -a.cos());
    let (nx, ny) = (-uy, ux);
    let (tx, ty) = (px + ux * len, py + uy * len);
    let pts = [
        (px + nx * w0 / 2.0, py + ny * w0 / 2.0),
        (tx + nx * w1 / 2.0, ty + ny * w1 / 2.0),
        (tx - nx * w1 / 2.0, ty - ny * w1 / 2.0),
        (px - nx * w0 / 2.0, py - ny * w0 / 2.0),
    ];
    (draw::polygon(&pts), (tx, ty))
}

/// A two-position mini toggle: hex nut, the bat leaning left (off) or
/// right (on) — or, `vertical`, down (off) or up (on). The ball tip is the
/// `Thumb`, riding a lane between its two positions; the shaft is drawn in
/// each pose.
fn mini_lever(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let d = p.size.unwrap_or(20.0).max(10.0);
    let k = d / 20.0;
    let f = finish(p.variant.as_deref());
    let w = (d * 2.3).max(p.touch.unwrap_or(0.0));
    let h = (d * 2.3).max(p.touch.unwrap_or(0.0));
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, w, h));
    let vertical = p.flag("vertical");
    let (px, py) = if vertical { (w / 2.0, h / 2.0) } else { (w / 2.0, h / 2.0 + d * 0.3) };
    let sh = draw::circle("shadow", px + 0.5 * k, py + 2.0 * k, d * 0.52, draw::solid("#00000066"));
    b.add(root, draw::with_effects(sh, vec![draw::drop_shadow("#00000059", 0.0, 1.5 * k, 3.0 * k)]));
    let nut = draw::path("nut", 0.0, 0.0, w, h, draw::regular(px, py, d * 0.5, 6, 0.0), draw::linear(150.0, &[(0.0, f.hi), (0.45, f.mid), (1.0, f.lo)]));
    b.add(root, draw::stroked(nut, &alpha(f.lo, 0.9), 0.5 * k));
    b.add(root, draw::circle("bushing", px, py, d * 0.3, draw::radial_at(0.4, 0.35, 0.8, &[(0.0, f.hi), (1.0, f.lo)])));
    b.add(root, draw::circle("hole", px, py, d * 0.16, draw::solid("#0c0a08")));
    let (angles, len) = if vertical { ([180.0, 0.0], d * 0.62) } else { ([-30.0, 30.0], d * 0.95) };
    let mut poses = Vec::new();
    let mut tips = Vec::new();
    for (name, deg) in [("off", angles[0]), ("on", angles[1])] {
        let pose = b.add(root, draw::frame(name, 0.0, 0.0, w, h));
        let (shape, tip) = bat(px, py, len, d * 0.2, d * 0.14, deg);
        b.add(pose, draw::path("bat shadow", 1.0 * k, 2.0 * k, w, h, shape.clone(), draw::solid("#00000055")));
        b.add(pose, draw::path("bat", 0.0, 0.0, w, h, shape, draw::linear(90.0 + deg, &[(0.0, "#6d6e70"), (0.4, "#f4f4f2"), (1.0, "#8a8b8d")])));
        poses.push(pose);
        tips.push(tip);
    }
    // The ball rides a lane from the off tip to the on tip.
    let br = d * 0.13;
    let (x0, x1) = (tips[0].0.min(tips[1].0) - br, tips[0].0.max(tips[1].0) + br);
    let (y0, y1) = (tips[0].1.min(tips[1].1) - br, tips[0].1.max(tips[1].1) + br);
    let lane = b.add(root, draw::frame("lane", x0, y0, x1 - x0, y1 - y0));
    let (bx, by) = (tips[0].0 - br - x0, tips[0].1 - br - y0);
    let thumb = b.add(lane, draw::frame("tip", bx, by, 2.0 * br, 2.0 * br));
    let ball = draw::circle("ball", br, br, br, draw::radial_at(0.35, 0.3, 0.8, &[(0.0, "#ffffff"), (1.0, "#77787a")]));
    b.add(thumb, draw::stroked(ball, "#00000055", 0.4 * k));
    let c = p
        .control(ControlKind::Button { mode: ButtonMode::Toggle })
        .slot(SlotRole::Off, poses[0].0)
        .slot(SlotRole::On, poses[1].0)
        .slot(SlotRole::Thumb, thumb.0);
    b.controls.insert(root, c);
    root
}

/// A three-position lever in a horizontal slot, seen from the front: a
/// chrome ball on a stem rocking left, upright or right about the slot's
/// pivot — a stepped `Knob` over a three-option choice (±60°).
fn slot_lever(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let w = p.size.unwrap_or(40.0).max(20.0);
    let k = w / 40.0;
    let h = w * 0.62;
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, w, h));
    let (cx, cy) = (w / 2.0, h * 0.62);
    let sh = w * 0.22;
    let slot = draw::rect("slot", 0.0, cy - sh / 2.0, w, sh, draw::linear(180.0, &[(0.0, "#050505"), (1.0, "#1b1b1c")]));
    let slot = draw::stroked(draw::rounded(slot, sh / 2.0), "#ffffff1f", 0.6 * k);
    b.add(root, draw::with_effects(slot, vec![draw::inner_shadow("#000000", 0.0, k, 3.0 * k)]));
    let len = w * 0.36;
    let side = 2.0 * (len + w * 0.1);
    let knob = b.add(root, draw::frame("knob", cx - side / 2.0, cy - side / 2.0, side, side));
    let c = side / 2.0;
    b.add(knob, draw::circle("pivot", c, c, sh * 0.3, draw::solid("#3a3a3c")));
    let body = b.add(knob, draw::frame("body", 0.0, 0.0, side, side));
    b.add(body, draw::rounded(draw::rect("stem", c - w * 0.035, c - len, w * 0.07, len, draw::linear(90.0, &[(0.0, "#555658"), (0.5, "#e7e7e5"), (1.0, "#555658")])), w * 0.03));
    let ball = draw::circle("ball", c, c - len, w * 0.085, draw::radial(&[(0.0, "#ffffff"), (0.5, "#bdbebf"), (1.0, "#4a4b4d")]));
    b.add(body, draw::stroked(ball, "#00000066", 0.4 * k));
    let sweep = p.sweep.unwrap_or(120.0);
    let ctl = p.control(ControlKind::Knob { sweep_degrees: sweep }).slot(SlotRole::Needle, body.0);
    b.controls.insert(root, ctl);
    root
}

/// A graphic-EQ fader: a deep dark slot and a cap whose white core glows.
fn glow_cap(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let travel = p.size.unwrap_or(150.0).max(40.0);
    let k = travel / 150.0;
    let (cw, ch) = (22.0 * k, 36.0 * k);
    let w = (cw + 10.0 * k).max(p.touch.unwrap_or(0.0));
    let h = travel + ch;
    let glow = p.lamp_color(0).unwrap_or_else(|| "#ffffff".into());
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, w, h));
    let sw = 17.0 * k;
    let slot = draw::rect("slot", (w - sw) / 2.0, 0.0, sw, h, draw::linear(90.0, &[(0.0, "#050304"), (0.5, "#1c1416"), (1.0, "#050304")]));
    let slot = draw::stroked(draw::rounded(slot, sw / 2.0), "#00000099", 0.8 * k);
    b.add(root, draw::with_effects(slot, vec![draw::inner_shadow("#000000", 0.0, 2.0 * k, 4.0 * k), draw::drop_shadow("#ffffff30", 0.0, 1.0 * k, 0.5 * k)]));
    b.add(root, draw::rect("slit", w / 2.0 - 1.2 * k, ch * 0.3, 2.4 * k, h - ch * 0.6, draw::solid("#000000")));
    let lane = b.add(root, draw::frame("lane", 0.0, 0.0, w, h));
    let thumb = b.add(lane, draw::frame("thumb", (w - cw) / 2.0, 0.0, cw, ch));
    let body = draw::rect("cap", 0.0, 0.0, cw, ch, draw::linear(90.0, &[(0.0, "#5c5c5e"), (0.3, "#d9d9d7"), (0.7, "#a9a9a7"), (1.0, "#4c4c4e")]));
    b.add(thumb, draw::with_effects(draw::rounded(body, 3.0 * k), vec![draw::drop_shadow("#000000aa", 0.0, 2.0 * k, 3.0 * k)]));
    let core = draw::rounded(draw::rect("core", 3.0 * k, 5.0 * k, cw - 6.0 * k, ch - 10.0 * k, draw::linear(90.0, &[(0.0, "#d8d8d8"), (0.5, &glow), (1.0, "#d8d8d8")])), 2.0 * k);
    let core = draw::with_effects(core, vec![draw::drop_shadow(&alpha(&glow, 0.8), 0.0, 0.0, 6.0 * k)]);
    b.add(thumb, draw::emissive(core, draw::hex(&glow), 0.7, 14.0 * k));
    let c = p.control(ControlKind::Fader { orientation: Orientation::Vertical }).slot(SlotRole::Thumb, thumb.0);
    b.controls.insert(root, c);
    root
}

pub(super) fn defs() -> Vec<ComponentDef> {
    vec![
        ComponentDef {
            id: "switch.hex_stomp",
            family: Family::Switch,
            title: "Hex-nut stomp",
            description: "A boutique pedal's footswitch seen a little from the front: a domed actuator on a barrel through a big hex nut with a locking slot, finished to match the knobs (gold, rose, chrome, black, magenta), and a white status LED beside it (`options` `led=left|right|none`, `color=#hex`). Latching (`momentary` for tap). Size = actuator diameter.",
            variants: &["gold", "rose", "chrome", "black", "magenta"],
            default_size: 30.0,
            binds: "toggle param",
            build: hex_stomp,
        },
        ComponentDef {
            id: "switch.mini_lever",
            family: Family::Switch,
            title: "Mini lever toggle",
            description: "A two-position mini toggle through a small hex nut: the chrome bat with a ball tip leans left when off and right when on (slow/fast, off/on); with the `vertical` option it points down when off and up when on (a head's CH1/CH2). The unit prints the legends. Size = nut width.",
            variants: &["gold", "chrome", "black", "magenta", "rose"],
            default_size: 20.0,
            binds: "toggle param",
            build: mini_lever,
        },
        ComponentDef {
            id: "switch.slot_lever",
            family: Family::Switch,
            title: "Slot lever (3-way)",
            description: "A three-way lever in a black horizontal slot seen from the front — a chrome ball on a stem rocking left, upright or right (±60°) — for a three-option choice param (sync, mode, type on a delay). A stepped `Knob` over the choice. Size = slot width.",
            variants: &[],
            default_size: 40.0,
            binds: "choice param",
            build: slot_lever,
        },
        ComponentDef {
            id: "lever.glow_cap",
            family: Family::Lever,
            title: "Lit-cap EQ fader",
            description: "A graphic-EQ band fader from a boutique tabletop EQ: a deep dark slot and a chrome-ended cap whose white core glows (`color=#hex` for another glow). Vertical `Fader`. Size = travel.",
            variants: &[],
            default_size: 150.0,
            binds: "param",
            build: glow_cap,
        },
    ]
}
