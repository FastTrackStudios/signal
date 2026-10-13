//! The cab page: an open-back 1×12 on the studio floor under its room
//! light, with two mics on its grille — a ribbon on the left, a dynamic on
//! the right — that you drag across the speaker (x = position, y =
//! distance). Each mic's panel sits beside the cab in the plugin's flat
//! chrome: a dropdown naming the mic and its power button, then POSITION,
//! DISTANCE, MIC LEVEL and PAN on flat arc knobs with their readouts. The
//! room size is over the cab, the room send, its level and PHASE under it.
//! Cab, 900 × 900.

use document_proto::{CornerRadii, NodeId, TextAlign};
use surface_kit::draw::{self, Lettering, family};
use surface_kit::materials::light;
use surface_kit::materials::paint::{self, Cloth, Grain, Material};
use surface_kit::Place;
use surface_proto::{Axis, Binding, ButtonMode, Control, ControlKind, HandleAxis, Range, SlotRole, Unit, ValueFormat};
use surface_widgets::Builder;

use super::parts::{Cap, PLUGIN, knob, set};
use crate::face::{Face, canvas};
use crate::reverb::ui;

const W: f64 = 900.0;
const H: f64 = 900.0;
const INK: &str = "#eceae6";
const DIM: &str = "#9c9894";

const MICS: &[&str] = &[
    "Dynamic 57",
    "Dynamic 57 Off-Axis",
    "Dynamic 409",
    "Dynamic 421",
    "Condenser 184",
    "Condenser 414",
    "Condenser 4006",
    "Condenser U67",
    "Ribbon 121",
    "Ribbon 121 Horizontal",
    "Ribbon 160",
];

fn ui_font(weight: f64) -> Lettering {
    Lettering::family(family::INTER).weight(weight)
}

/// The mic dropdown: a black box printing the chosen mic, a chevron.
fn mic_picker(b: &mut Builder<'_>, parent: NodeId, addr: &str, (x, cy, w): (f64, f64, f64)) -> NodeId {
    let mut f = draw::frame("Mic model", x, cy - 22.0, w, 44.0);
    f.fills = vec![draw::solid("#0b0b0c")];
    f.corner_radii = CornerRadii::uniform(5.0);
    let root = b.add(parent, f);
    // The name runs under a fade before the chevron, as the plugin's does.
    let mut clip = draw::frame("name", 10.0, 0.0, w - 40.0, 44.0);
    clip.clips_content = true;
    let clip = b.add(root, clip);
    let size = 13.0;
    let mut t = draw::styled_text("value", "", 0.0, 22.0 - size * 0.68, w + 60.0, size, INK, TextAlign::Left, &ui_font(450.0));
    t.name = "value".into();
    let value = b.add(clip, t);
    b.add(root, draw::rect("fade", w - 44.0, 4.0, 14.0, 36.0, draw::linear(90.0, &[(0.0, "#0b0b0c00"), (1.0, "#0b0b0c")])));
    b.add(root, ui::strokes("chevron", &[vec![(w - 25.0, 19.5), (w - 19.0, 25.5), (w - 13.0, 19.5)]], 1.8, draw::solid(INK)));
    let mut c = Control::new(ControlKind::Selector).slot(SlotRole::Value, value.0);
    c.binding = Some(Binding::param(addr));
    c.format = Some(ValueFormat::labels(MICS));
    b.controls.insert(root, c);
    root
}

/// A dark square button with the power glyph, lit while on.
fn power(b: &mut Builder<'_>, parent: NodeId, addr: &str, (cx, cy): (f64, f64)) -> NodeId {
    let root = b.add(parent, draw::frame("Mic on", cx - 22.0, cy - 22.0, 44.0, 44.0));
    b.add(root, draw::rounded(draw::rect("bg", 0.0, 0.0, 44.0, 44.0, draw::solid("#1f1f22")), 5.0));
    let glyph = |c: &str| {
        let arc = draw::path("glyph", 0.0, 0.0, 44.0, 44.0, surface_widgets::sector_path(22.0, 23.0, 8.0, 9.8, 35f64.to_radians(), 325f64.to_radians()), draw::solid(c));
        let stem = draw::rect("stem", 21.1, 11.5, 1.8, 10.5, draw::solid(c));
        (arc, stem)
    };
    let (arc, stem) = glyph("#5a5a5e");
    b.add(root, arc);
    b.add(root, stem);
    let lit = b.add(root, draw::frame("lit", 0.0, 0.0, 44.0, 44.0));
    let (arc, stem) = glyph(INK);
    b.add(lit, draw::emissive(arc, draw::hex(INK), 0.25, 4.0));
    b.add(lit, stem);
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Led, lit.0);
    c.binding = Some(Binding::param(addr));
    b.controls.insert(root, c);
    root
}

/// A flat arc knob with its reading and its legend under it.
#[allow(clippy::too_many_arguments)]
fn arc_knob(b: &mut Builder<'_>, parent: NodeId, addr: &str, name: &str, legend: &str, (cx, cy): (f64, f64), d: f64, format: ValueFormat) -> NodeId {
    let k = knob(b, parent, Cap::ArcDisc, addr, name, d, cx, cy);
    if let Some(c) = b.controls.get_mut(&k) {
        c.format = Some(format);
    }
    ui::add_value(b, k, 13.0, INK);
    set(b, parent, legend, cx, cy + d * 0.5 + 44.0, 12.5, INK, &ui_font(650.0), 0.02);
    k
}

/// A pill switch (the plugin's toggle): a track that lights, a thumb that
/// slides right when on.
fn pill(b: &mut Builder<'_>, parent: NodeId, name: &str, addr: &str, (cx, cy): (f64, f64)) -> NodeId {
    let root = b.add(parent, draw::frame(name, cx - 22.0, cy - 22.0, 44.0, 44.0));
    b.add(root, draw::stroked(draw::rounded(draw::rect("track", 6.0, 14.0, 32.0, 16.0, draw::solid("#00000000")), 8.0), INK, 1.4));
    let lit = b.add(root, draw::rounded(draw::rect("lit", 6.0, 14.0, 32.0, 16.0, draw::solid(INK)), 8.0));
    let lane = b.add(root, draw::frame("lane", 9.0, 17.0, 26.0, 10.0));
    let thumb = b.add(lane, draw::circle("thumb", 5.0, 5.0, 5.0, draw::solid("#8a8a8e")));
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Thumb, thumb.0).slot(SlotRole::Led, lit.0);
    c.binding = Some(Binding::param(addr));
    b.controls.insert(root, c);
    root
}

/// A mic's panel, a column `w` wide from `x`.
fn mic_panel(b: &mut Builder<'_>, parent: NodeId, n: u8, x: f64, w: f64) {
    let ns = format!("cab/m{n}");
    let p = b.add(parent, draw::frame(&format!("Mic {n}"), x, 160.0, w, 480.0));
    let c = w / 2.0;
    set(b, p, &format!("MIC {n}"), c, 12.0, 12.0, DIM, &ui_font(650.0), 0.1);
    mic_picker(b, p, &format!("{ns}_model"), (0.0, 52.0, w - 50.0));
    power(b, p, &format!("{ns}_on"), (w - 22.0, 52.0));
    let (l, r) = (w * 0.26, w * 0.74);
    let three = ValueFormat::decimals(3);
    arc_knob(b, p, &format!("{ns}_position"), &format!("Mic {n} position"), "POSITION", (l, 146.0), 62.0, three.clone());
    arc_knob(b, p, &format!("{ns}_distance"), &format!("Mic {n} distance"), "DISTANCE", (r, 146.0), 62.0, three);
    arc_knob(b, p, &format!("{ns}_level"), &format!("Mic {n} level"), "MIC LEVEL", (l, 318.0), 62.0, ValueFormat::decimals(1).unit("dB"));
    arc_knob(b, p, &format!("{ns}_pan"), &format!("Mic {n} pan"), "PAN", (r, 318.0), 62.0, ValueFormat::decimals(0));
}

/// The cab: a tolex box, grille-cloth side panels behind its uprights, the
/// big dark grille sat back with the speaker's shadow behind it. Returns
/// the grille frame (the mics' area).
fn cabinet(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64)) -> NodeId {
    b.add(parent, draw::circle("floor light", x + w / 2.0, y + h + 10.0, w * 0.72, draw::radial(&[(0.0, "#ff9a5a38"), (1.0, "#00000000")])));
    b.add(parent, draw::rect("floor shadow", x - 8.0, y + h - 6.0, w + 16.0, 18.0, draw::radial(&[(0.0, "#000000c0"), (1.0, "#00000000")])));
    let mut f = draw::frame("Cab", x, y, w, h);
    f.corner_radii = CornerRadii::uniform(12.0);
    f.effects = light::stand(10.0, 1.1);
    let cab = b.add(parent, f);
    paint::fill(b, cab, (0.0, 0.0, w, h), 12.0, &Material::Tolex { grain: Grain::Levant, color: "#1a1919".into(), scale: 0.7 });
    for e in light::edge("cab edge", (0.0, 0.0, w, h), 12.0, 7.0, 1.2, true) {
        b.add(cab, e);
    }
    // Side panels: cloth between the box's end and an upright.
    let side = w * 0.1;
    for sx in [16.0, w - 16.0 - side] {
        let panel = b.add(cab, draw::frame("side cloth", sx, 22.0, side, h - 44.0));
        paint::fill(b, panel, (0.0, 0.0, side, h - 44.0), 2.0, &Material::GrilleCloth { cloth: Cloth::Black });
        for e in light::edge("side recess", (0.0, 0.0, side, h - 44.0), 2.0, 5.0, 1.0, false) {
            b.add(panel, e);
        }
    }
    for ux in [16.0 + side, w - 16.0 - side - 14.0] {
        let up = b.add(cab, draw::frame("upright", ux, 8.0, 14.0, h - 16.0));
        paint::fill(b, up, (0.0, 0.0, 14.0, h - 16.0), 3.0, &Material::Tolex { grain: Grain::Levant, color: "#1d1c1c".into(), scale: 0.7 });
        for e in light::edge("upright edge", (0.0, 0.0, 14.0, h - 16.0), 3.0, 3.0, 1.0, true) {
            b.add(up, e);
        }
    }
    let (gx, gy, gw, gh) = (16.0 + side + 22.0, 26.0, w - 2.0 * (16.0 + side + 22.0), h - 52.0);
    let mut g = draw::frame("grille", gx, gy, gw, gh);
    g.clips_content = true;
    let grille = b.add(cab, g);
    paint::fill(b, grille, (0.0, 0.0, gw, gh), 2.0, &Material::GrilleCloth { cloth: Cloth::Black });
    // The speaker behind the cloth: a soft disc and its rim.
    let (sx, sy, r) = (gw / 2.0, gh / 2.0, gh * 0.4);
    b.add(grille, draw::circle("speaker", sx, sy, r, draw::radial(&[(0.0, "#ffffff10"), (0.35, "#ffffff08"), (0.62, "#00000030"), (0.66, "#ffffff0e"), (0.7, "#00000000"), (1.0, "#00000000")])));
    b.add(grille, draw::with_opacity(draw::stroked(draw::circle("speaker rim", sx, sy, r, draw::solid("#00000000")), "#ffffff14", 2.0), 1.0));
    b.add(grille, draw::stroked(draw::rounded(draw::rect("frame", 1.0, 1.0, gw - 2.0, gh - 2.0, draw::solid("#00000000")), 2.0), "#6a686c", 2.0));
    for e in light::edge("grille recess", (0.0, 0.0, gw, gh), 2.0, 8.0, 1.2, false) {
        b.add(grille, e);
    }
    grille
}

/// A ribbon mic seen side on, pointing right, drawn in a handle's box
/// with its grille end at the box's centre.
fn ribbon(b: &mut Builder<'_>, h: NodeId) {
    let (c, cy) = (22.0, 22.0);
    let body = draw::rounded(draw::rect("ribbon body", c - 120.0, cy - 9.0, 100.0, 18.0, draw::linear(180.0, &[(0.0, "#eceef0"), (0.45, "#9aa0a6"), (1.0, "#4a4e54")])), 8.0);
    b.add(h, draw::with_effects(body, vec![draw::drop_shadow("#000000a0", 2.0, 6.0, 8.0)]));
    b.add(h, draw::rounded(draw::rect("band", c - 70.0, cy - 3.5, 30.0, 7.0, draw::linear(180.0, &[(0.0, "#3ab8b0"), (1.0, "#12605c")])), 3.0));
    b.add(h, draw::with_effects(draw::rounded(draw::rect("head", c - 24.0, cy - 14.0, 58.0, 28.0, draw::linear(180.0, &[(0.0, "#e2e4e6"), (0.5, "#8a9096"), (1.0, "#4a4e54")])), 6.0), vec![draw::drop_shadow("#000000a0", 2.0, 6.0, 8.0)]));
    let lines: Vec<Vec<(f64, f64)>> = (0..10).map(|i| {
        let lx = c - 18.0 + f64::from(i) * 5.0;
        vec![(lx, cy - 12.0), (lx, cy + 12.0)]
    }).collect();
    b.add(h, ui::strokes("slots", &lines, 1.3, draw::solid("#3a3e44")));
}

/// A dynamic mic angled in from the right, its grille at the box's centre.
fn dynamic(b: &mut Builder<'_>, h: NodeId) {
    let c = 22.0;
    let body = draw::rounded(draw::rect("dynamic body", c + 4.0, c - 13.0, 80.0, 26.0, draw::linear(180.0, &[(0.0, "#4a4a4e"), (0.5, "#1c1c1e"), (1.0, "#08080a")])), 10.0);
    b.add(h, draw::with_effects(draw::rotated(body, -30.0), vec![draw::drop_shadow("#000000a0", 2.0, 6.0, 8.0)]));
    b.add(h, draw::with_effects(draw::circle("dynamic grille", c, c, 17.0, draw::radial_at(0.4, 0.35, 0.9, &[(0.0, "#3e3e42"), (1.0, "#0a0a0c")])), vec![draw::drop_shadow("#00000090", 0.0, 3.0, 6.0)]));
    b.add(h, draw::with_opacity(draw::stroked(draw::circle("dynamic ring", c, c, 16.0, draw::solid("#00000000")), "#6a6a70", 1.2), 0.8));
}

/// A mic's drag handle in `grille`: its area runs from the speaker's
/// centre out to its edge (x = position, 0 at the centre) and down
/// (y = distance); `left` mirrors the position axis.
pub fn mic_handle(b: &mut Builder<'_>, grille: NodeId, n: u8, left: bool, (gw, gh): (f64, f64)) {
    let ns = format!("cab/m{n}");
    let r = gh * 0.4;
    let (ax, aw) = if left { (gw / 2.0 - r, r) } else { (gw / 2.0, r) };
    let area = b.add(grille, draw::frame(&format!("Mic {n} area"), ax, gh / 2.0 - 30.0, aw, gh * 0.35));
    let h = b.add(area, draw::frame(&format!("Mic {n}"), 0.0, 0.0, 44.0, 44.0));
    if left { ribbon(b, h) } else { dynamic(b, h) };
    let lin = |a: f64, z: f64| HandleAxis::new(Axis::Linear { min: a, max: z });
    let kind = ControlKind::Handle {
        x: Some(Binding::param(format!("{ns}_position"))),
        y: Some(Binding::param(format!("{ns}_distance"))),
        wheel: None,
        x_map: if left { lin(1.0, 0.0) } else { lin(0.0, 1.0) },
        y_map: lin(1.0, 0.0),
    };
    b.controls.insert(h, Control::new(kind).bound(Binding::param(format!("{ns}_position"))));
}

/// An outlined latching button printing `text` (PHASE Ø).
fn outline_button(b: &mut Builder<'_>, parent: NodeId, name: &str, addr: &str, text: &str, (cx, cy, w): (f64, f64, f64)) -> NodeId {
    let root = b.add(parent, draw::frame(name, cx - w / 2.0, cy - 22.0, w, 44.0));
    b.add(root, draw::stroked(draw::rounded(draw::rect("outline", 0.0, 4.0, w, 36.0, draw::solid("#0b0b0c")), 5.0), "#8e8f94", 1.4));
    let lit = b.add(root, draw::rounded(draw::rect("latched", 1.0, 5.0, w - 2.0, 34.0, draw::solid("#ffffff26")), 4.0));
    set(b, root, text, w / 2.0, 22.0, 14.0, INK, &ui_font(650.0), 0.04);
    let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Toggle }).slot(SlotRole::Led, lit.0);
    c.binding = Some(Binding::param(addr));
    b.controls.insert(root, c);
    root
}

pub fn cab() -> Face {
    let scene = canvas("Cab Room", |b, root| {
        ui::fit_root(b, root, W, H);
        let mut room = draw::frame("Room", 0.0, 0.0, W, H);
        room.fills = vec![draw::linear(180.0, &[(0.0, "#1a1210"), (0.45, "#2a1a16"), (0.62, "#3a2420"), (0.63, "#5a3624"), (1.0, "#1c100a")])];
        room.clips_content = true;
        let room = b.add(root, room);
        // The wall's slats and the floorboards.
        // Wooden slats on the wall, each a rounded board lit down its face.
        for i in 0..31 {
            let x = f64::from(i) * 30.0 - 6.0;
            let tone = if i % 3 == 1 { ("#4a2c1c", "#7a4a2e") } else { ("#3a2216", "#63391f") };
            let slat = draw::rounded(draw::rect("slat", x, -4.0, 22.0, H * 0.62 + 4.0, draw::linear(180.0, &[(0.0, "#1a0e08"), (0.35, tone.0), (0.8, tone.1), (1.0, "#2a160c")])), 3.0);
            b.add(room, slat);
            b.add(room, draw::rect("slat light", x + 2.0, 0.0, 3.0, H * 0.62, draw::linear(180.0, &[(0.0, "#ffffff00"), (0.7, "#ffd0a018"), (1.0, "#ffffff00")])));
        }
        let boards: Vec<Vec<(f64, f64)>> = (0..10).map(|i| {
            let y = H * 0.63 + f64::from(i).powf(1.4) * 12.0;
            vec![(0.0, y), (W, y)]
        }).collect();
        b.add(room, ui::strokes("boards", &boards, 1.0, draw::solid("#00000050")));
        b.add(room, draw::rect("light", 0.0, 0.0, W, H, draw::radial_at(0.5, 0.62, 0.6, &[(0.0, "#ff9a6030"), (1.0, "#00000000")])));

        // Room size over the cab.
        let mut pill_bg = draw::frame("room size pill", 290.0, 66.0, 320.0, 52.0);
        pill_bg.fills = vec![draw::solid("#0b0b0cb0")];
        pill_bg.corner_radii = CornerRadii::uniform(26.0);
        b.add(room, pill_bg);
        set(b, room, "ROOM SIZE", 372.0, 92.0, 14.0, INK, &ui_font(650.0), 0.04);
        let size = Place::new("Room size").size(34.0).variant("white").bind(Binding::param("cab/room_size")).options(&["S", "M", "L"]).legend("");
        ui::put(b, room, "button.square_bank", &size, 520.0, 92.0);

        let (cx, cy, cw, ch) = (226.0, 196.0, 448.0, 300.0);
        let grille = cabinet(b, room, (cx, cy, cw, ch));
        let (gw, gh) = b.scene.node(grille).map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
        mic_handle(b, grille, 1, true, (gw, gh));
        mic_handle(b, grille, 2, false, (gw, gh));

        mic_panel(b, room, 1, 12.0, 196.0);
        mic_panel(b, room, 2, W - 208.0, 196.0);

        // The room send, its level and phase under the cab.
        set(b, room, "ROOM SEND", 432.0, 600.0, 14.0, INK, &ui_font(650.0), 0.02);
        pill(b, room, "Room send", "cab/room_on", (516.0, 600.0));
        arc_knob(b, room, "cab/room_level", "Room level", "LEVEL", (450.0, 672.0), 64.0, ValueFormat::decimals(1).unit("dB"));
        outline_button(b, room, "Phase", "cab/phase", "PHASE Ø", (450.0, 836.0, 120.0));
    });
    let mut params = vec![ui::choice("cab/room_size", "Room size", &["S", "M", "L"], 1)];
    for (n, model, position) in [(1, 9, 0.3), (2, 0, 0.3)] {
        let ns = format!("cab/m{n}");
        params.push(ui::choice(&format!("{ns}_model"), &format!("Mic {n} model"), MICS, model).featured());
        params.push(ui::toggle(&format!("{ns}_on"), &format!("Mic {n}"), true));
        params.push(ui::param(&format!("{ns}_position"), &format!("Mic {n} position"), Range::linear(0.0, 1.0, Unit::None), position));
        params.push(ui::param(&format!("{ns}_distance"), &format!("Mic {n} distance"), Range::linear(0.0, 1.0, Unit::None), 0.0));
        params.push(ui::param(&format!("{ns}_level"), &format!("Mic {n} level"), Range::linear(-24.0, 12.0, Unit::Db), 0.0).featured());
        params.push(ui::param(&format!("{ns}_pan"), &format!("Mic {n} pan"), Range::linear(-100.0, 100.0, Unit::Percent), 0.0).bipolar());
    }
    params.push(ui::toggle("cab/room_on", "Room send", false));
    params.push(ui::param("cab/room_level", "Room level", Range::linear(-60.0, 0.0, Unit::Db), -40.0));
    params.push(ui::toggle("cab/phase", "Phase", false));
    Face { file: "14-cab-room", plugin: PLUGIN, scene, params, streams: Vec::new() }
}
